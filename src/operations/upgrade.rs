//! Explicit pre-upgrade recovery point. No automatic executable replacement.
use crate::{
    App, backup,
    error::{Error, Result},
};
use serde::Serialize;
use std::path::Path;
#[derive(Serialize)]
pub struct Receipt {
    pub format: &'static str,
    pub created_at: i64,
    pub executable_sha256: String,
    pub application_version: &'static str,
    pub schema: serde_json::Value,
    pub archive_sha256: String,
    pub archive_bytes: usize,
    pub recovery: &'static str,
}
pub async fn prepare(app: &App, output: &Path, key_file: &Path) -> Result<Receipt> {
    let executable = std::env::current_exe()
        .map_err(|_| Error::invalid("Cannot locate the pre-upgrade executable."))?;
    let executable_bytes = backup::read_bounded(&executable, 256 * 1024 * 1024).await?;
    let executable_sha256 = crate::auth::digest(&executable_bytes);
    drop(executable_bytes);
    let key = super::encryption::read_key(key_file).await?;
    let plaintext = backup::capture(app).await?;
    let graph = backup::inspect(&app.config, &plaintext)?;
    let encoded = super::encryption::seal(&key, &plaintext)?;
    backup::write_private(output, &encoded)
        .map_err(|_| Error::invalid("Cannot create a new private upgrade recovery point."))?;
    let stored = backup::read_bounded(
        output,
        app.config.max_backup_bytes + super::encryption::OVERHEAD,
    )
    .await?;
    let recovered = super::encryption::open(&key, &stored, app.config.max_backup_bytes)?;
    backup::inspect(&app.config, &recovered)?;
    if recovered != plaintext {
        return Err(Error::invalid(
            "Upgrade recovery point verification failed.",
        ));
    }
    Ok(Receipt {
        format: "wpalt-upgrade-receipt-v1",
        executable_sha256,
        created_at: crate::now(),
        application_version: env!("CARGO_PKG_VERSION"),
        schema: serde_json::to_value(graph)
            .map_err(|_| Error::invalid("Cannot describe recovery graph."))?["schema"]
            .clone(),
        archive_sha256: crate::auth::digest(&stored),
        archive_bytes: stored.len(),
        recovery: "Keep this archive, its independent key, the old executable and private config. Restore with the old executable into an empty database and data directory, verify, then switch the site. Do not start an older executable against upgraded data.",
    })
}

pub struct Request<'a> {
    pub execute: Option<&'a str>,
    pub output: Option<&'a Path>,
    pub key_file: Option<&'a Path>,
    pub rebind_origin: Option<&'a str>,
    pub acknowledge_source_stopped: bool,
}

/// Supported stopped-site native maintenance. No mixed-version serving.
pub async fn apply(
    app: &App,
    owner_config: &crate::config::Config,
    request: Request<'_>,
) -> Result<serde_json::Value> {
    let Request {
        execute,
        output,
        key_file,
        rebind_origin,
        acknowledge_source_stopped,
    } = request;
    if owner_config.data_dir.join(".process-intent").exists() {
        return Err(Error::invalid(
            "Reconcile interrupted operations before changing the schema.",
        ));
    }
    let installed: i64 = sqlx::query_scalar("SELECT version FROM schema_version WHERE id=1")
        .fetch_one(&app.db.pool)
        .await?;
    if ![14, 15, crate::db::SCHEMA_VERSION].contains(&installed) {
        return Err(Error::invalid(
            "Unsupported maintenance source; use fresh-target recovery.",
        ));
    }
    let old_directory: Option<String> = if rebind_origin.is_some() {
        if !app.db.postgres || installed != crate::db::SCHEMA_VERSION {
            return Err(Error::invalid(
                "Directory rebind supports current-schema PostgreSQL engine recovery only; use portable fresh-target recovery otherwise.",
            ));
        }
        if owner_config.data_dir.join(".process-state.json").exists() {
            return Err(Error::invalid(
                "Use a fresh recovery directory without transferred temporary runtime authority.",
            ));
        }
        let source = rebind_origin.unwrap_or_default();
        let parsed = url::Url::parse(source)
            .map_err(|_| Error::invalid("Use a complete reviewed source HTTP origin."))?;
        if !["http", "https"].contains(&parsed.scheme())
            || parsed.origin().ascii_serialization() != source
        {
            return Err(Error::invalid(
                "Use a complete reviewed source HTTP origin.",
            ));
        }
        let old: String =
            sqlx::query_scalar("SELECT directory_digest FROM process_authority WHERE id=1")
                .fetch_one(&app.db.pool)
                .await?;
        if old.len() != 64
            || !old.bytes().all(|v| v.is_ascii_hexdigit())
            || old == app.db.directory_digest
        {
            return Err(Error::invalid(
                "Recovery directory must differ from the valid stored authority.",
            ));
        }
        let live: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM pg_stat_activity WHERE application_name=$1")
                .bind(format!("wpalt:{}", &old[..56]))
                .fetch_one(&app.db.pool)
                .await?;
        if live != 0 {
            return Err(Error::invalid(
                "Source application connections still exist. Stop all source nodes/workers before rebind.",
            ));
        }
        Some(old)
    } else {
        None
    };
    let bytes = backup::capture(app).await?;
    let graph = backup::inspect(&app.config, &bytes)?;
    let mut envelope: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| Error::invalid("Invalid maintenance recovery graph."))?;
    let mut payload: serde_json::Value = serde_json::from_str(
        envelope["payload"]
            .as_str()
            .ok_or_else(|| Error::invalid("Invalid maintenance recovery graph."))?,
    )
    .map_err(|_| Error::invalid("Invalid maintenance recovery graph."))?;
    payload["created_at"] = 0.into();
    for rows in payload["tables"]
        .as_object_mut()
        .ok_or_else(|| Error::invalid("Invalid maintenance recovery graph."))?
        .values_mut()
    {
        rows.as_array_mut()
            .ok_or_else(|| Error::invalid("Invalid maintenance recovery graph."))?
            .sort_by_cached_key(|row| row.to_string());
    }
    for name in ["files", "private_files"] {
        payload[name]
            .as_array_mut()
            .ok_or_else(|| Error::invalid("Invalid maintenance recovery graph."))?
            .sort_by_cached_key(|row| row["filename"].to_string());
    }
    let executable = std::env::current_exe()
        .map_err(|_| Error::invalid("Cannot identify upgrade executable."))?;
    let binary = backup::read_bounded(&executable, 256 * 1024 * 1024).await?;
    envelope = serde_json::json!({"domain":payload,"source_schema":installed,
        "target_schema":crate::db::SCHEMA_VERSION,"executable":crate::auth::digest(&binary),
        "old_directory":old_directory,"rebind_origin":rebind_origin,
        "configuration":crate::auth::digest(&serde_json::to_vec(owner_config)
            .map_err(|_| Error::invalid("Cannot identify maintenance configuration."))?)});
    let plan = crate::auth::digest(
        &serde_json::to_vec(&envelope)
            .map_err(|_| Error::invalid("Cannot identify maintenance graph."))?,
    );
    let mut report = serde_json::json!({"format":"wpalt-maintenance-upgrade-v1","plan":plan,
        "source_schema":installed,"target_schema":crate::db::SCHEMA_VERSION,"graph":graph,"executed":false,"directory_rebind":old_directory.is_some(),
        "boundary":"All nodes stopped. Exact domain/configuration/executable review, verified encrypted pre-change recovery point, then transactional native migration. Restore rollback into a fresh database/directory using the retained old executable and private configuration. No mixed-version serving."});
    if let Some(reviewed) = execute {
        if old_directory.is_some() && !acknowledge_source_stopped {
            return Err(Error::invalid(
                "Explicitly acknowledge source shutdown and verified fresh engine/media recovery.",
            ));
        }
        if reviewed != plan {
            return Err(Error::invalid("Review a fresh exact maintenance plan."));
        }
        let output =
            output.ok_or_else(|| Error::invalid("A new encrypted recovery output is required."))?;
        let key = super::encryption::read_key(
            key_file.ok_or_else(|| Error::invalid("An independent recovery key is required."))?,
        )
        .await?;
        let sealed = super::encryption::seal(&key, &bytes)?;
        backup::write_private(output, &sealed).map_err(|_| {
            Error::invalid("Cannot create a NEW private maintenance recovery point.")
        })?;
        let stored = backup::read_bounded(
            output,
            app.config.max_backup_bytes + super::encryption::OVERHEAD,
        )
        .await?;
        let opened = super::encryption::open(&key, &stored, app.config.max_backup_bytes)?;
        backup::inspect(&app.config, &opened)?;
        if opened != bytes {
            return Err(Error::invalid("Maintenance recovery verification failed."));
        }
        app.db.migrate().await.map_err(|_| Error::invalid("Maintenance migration failed; preserve the recovery point and inspect before retrying."))?;
        if let Some(old) = old_directory {
            let live: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM pg_stat_activity WHERE application_name=$1",
            )
            .bind(format!("wpalt:{}", &old[..56]))
            .fetch_one(&app.db.pool)
            .await?;
            if live != 0 {
                return Err(Error::invalid(
                    "Source reconnected; preserve the recovery point and repeat stopped-source review.",
                ));
            }
            let mut tx = app.db.pool.begin().await?;
            let changed = sqlx::query("UPDATE process_authority SET directory_digest=$1 WHERE id=1 AND directory_digest=$2")
                .bind(&app.db.directory_digest).bind(old).execute(&mut *tx).await?.rows_affected();
            if changed != 1 {
                return Err(Error::conflict());
            }
            sqlx::query("DELETE FROM sessions")
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM integration_credentials")
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE recovery_mode SET held=1,source_origin=$1,target_origin=$2,review='' WHERE id=1")
                .bind(rebind_origin.unwrap_or_default()).bind(owner_config.origin()).execute(&mut *tx).await?;
            tx.commit().await?;
            report["held"] = true.into();
            report["activation"] = "Review restored accounts/identities, credentials, queues and external effects; then use stopped-host clone-activate. Sessions/integration grants were invalidated.".into();
        }
        crate::platform::local_processes::verify_directory(&app.db, owner_config, true).await?;
        report["executed"] = true.into();
        report["recovery_sha256"] = crate::auth::digest(&stored).into();
        report["temporary_state_review"] = owner_config
            .data_dir
            .join(".process-state.json")
            .exists()
            .into();
    }
    Ok(report)
}
