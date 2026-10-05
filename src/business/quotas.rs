//! Persistent admission counters: constant-size conditional updates across all pools.
use crate::{
    App,
    error::{Error, Result},
};
use serde::{Deserialize, Serialize};
use sqlx::{Any, Transaction};
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub forms: i64,
    pub entries: i64,
    pub entry_bytes: i64,
    pub contacts: i64,
    pub drafts: i64,
    pub draft_bytes: i64,
    pub uploads: i64,
    pub upload_bytes: i64,
    pub registrations: i64,
    pub mail_jobs: i64,
    pub mail_bytes: i64,
    pub mail_retention_days: i64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            forms: 1000,
            entries: 100000,
            entry_bytes: 256 * 1024 * 1024,
            contacts: 100000,
            drafts: 10000,
            draft_bytes: 64 * 1024 * 1024,
            uploads: 10000,
            upload_bytes: 256 * 1024 * 1024,
            registrations: 10000,
            mail_jobs: 100000,
            mail_bytes: 256 * 1024 * 1024,
            mail_retention_days: 30,
        }
    }
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            [
                self.forms,
                self.entries,
                self.contacts,
                self.drafts,
                self.uploads,
                self.registrations,
                self.mail_jobs
            ]
            .iter()
            .all(|v| (1..=1000000).contains(v))
                && [
                    self.entry_bytes,
                    self.draft_bytes,
                    self.upload_bytes,
                    self.mail_bytes
                ]
                .iter()
                .all(|v| (1024..=16 * 1024 * 1024 * 1024).contains(v))
                && (1..=365).contains(&self.mail_retention_days),
            "Business limits require 1..1,000,000 records, 1 KiB..16 GiB byte limits and 1..365 mail retention days."
        );
        Ok(())
    }
}
pub const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS business_usage(kind TEXT PRIMARY KEY,items BIGINT NOT NULL DEFAULT 0 CHECK(items>=0),bytes BIGINT NOT NULL DEFAULT 0 CHECK(bytes>=0));";
pub async fn initialize(tx: &mut Transaction<'_, Any>, postgres: bool) -> Result<()> {
    let existing: Vec<String> = sqlx::query_scalar("SELECT kind FROM business_usage")
        .fetch_all(&mut **tx)
        .await?;
    for (kind, table, bytes) in [
        ("forms", "business_forms", "0"),
        (
            "entries",
            "form_entries",
            "COALESCE(SUM(length(values_json)),0)",
        ),
        ("contacts", "audience_contacts", "0"),
        (
            "drafts",
            "form_drafts",
            "COALESCE(SUM(length(values_json)),0)",
        ),
        ("uploads", "form_attachments", "COALESCE(SUM(size),0)"),
        ("registrations", "registration_requests", "0"),
        (
            "mail",
            "mail_jobs",
            "COALESCE(SUM(length(html)+length(plain)+length(subject)),0)",
        ),
    ] {
        if existing.iter().any(|value| value == kind) {
            continue;
        }
        let expression = if postgres {
            bytes.replace("length(", "octet_length(")
        } else {
            bytes
                .replace("length(values_json)", "length(CAST(values_json AS BLOB))")
                .replace("length(html)", "length(CAST(html AS BLOB))")
                .replace("length(plain)", "length(CAST(plain AS BLOB))")
                .replace("length(subject)", "length(CAST(subject AS BLOB))")
        };
        // Only runs once per new counter, preserving pre-M4 rows without runtime legacy branches.
        sqlx::query(&format!("INSERT INTO business_usage(kind,items,bytes) SELECT $1,COUNT(*),{expression} FROM {table} WHERE NOT EXISTS(SELECT 1 FROM business_usage WHERE kind=$1) ON CONFLICT(kind) DO NOTHING")).bind(kind).execute(&mut **tx).await?;
    }
    Ok(())
}
pub async fn reserve(
    app: &App,
    tx: &mut Transaction<'_, Any>,
    kind: &str,
    items: i64,
    bytes: i64,
) -> Result<()> {
    let c = &app.config.business_limits;
    let (max_items, max_bytes) = match kind {
        "forms" => (c.forms, 0),
        "entries" => (c.entries, c.entry_bytes),
        "contacts" => (c.contacts, 0),
        "drafts" => (c.drafts, c.draft_bytes),
        "uploads" => (c.uploads, c.upload_bytes),
        "registrations" => (c.registrations, 0),
        "mail" => (c.mail_jobs, c.mail_bytes),
        _ => return Err(Error::invalid("Unknown business quota.")),
    };
    if sqlx::query("UPDATE business_usage SET items=items+$1,bytes=bytes+$2 WHERE kind=$3 AND items+$1<=$4 AND bytes+$2<=$5 AND items+$1>=0 AND bytes+$2>=0").bind(items).bind(bytes).bind(kind).bind(max_items).bind(max_bytes).execute(&mut **tx).await?.rows_affected()!=1{return Err(Error(axum::http::StatusCode::TOO_MANY_REQUESTS,"This site's configured business storage limit has been reached. Contact the site owner."));}
    Ok(())
}
pub async fn release(
    tx: &mut Transaction<'_, Any>,
    kind: &str,
    items: i64,
    bytes: i64,
) -> Result<()> {
    sqlx::query("UPDATE business_usage SET items=items-$1,bytes=bytes-$2 WHERE kind=$3")
        .bind(items)
        .bind(bytes)
        .bind(kind)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Small maintenance batches free staged/expired data and old terminal mail.
/// Uncertain deliveries are retained for deliberate owner recovery.
pub async fn cleanup(app: &App) -> Result<()> {
    if !app.config.business_enabled {
        return Ok(());
    }
    // Capture holds this same single-server coordinator through database and file reads.
    // An expired staged file must not be unlinked from an in-flight recovery point.
    let _guard = app.mutation().await?;
    use sqlx::Row;
    let mut tx = app.db.pool.begin().await?;
    release(&mut tx, "registrations", 0, 0).await?;
    let removed=sqlx::query("DELETE FROM registration_requests WHERE id IN (SELECT id FROM registration_requests WHERE state='pending' AND expires_at<=$1 ORDER BY expires_at,id LIMIT 50)").bind(crate::now()).execute(&mut *tx).await?;
    release(&mut tx, "registrations", removed.rows_affected() as i64, 0).await?;
    tx.commit().await?;
    let mut tx = app.db.pool.begin().await?;
    release(&mut tx, "drafts", 0, 0).await?;
    let rows=sqlx::query("DELETE FROM form_drafts WHERE (form_id,token_hash) IN (SELECT form_id,token_hash FROM form_drafts WHERE expires_at<=$1 ORDER BY expires_at LIMIT 50) RETURNING values_json").bind(crate::now()).fetch_all(&mut *tx).await?;
    release(
        &mut tx,
        "drafts",
        rows.len() as i64,
        rows.iter()
            .map(|r| r.get::<String, _>("values_json").len() as i64)
            .sum(),
    )
    .await?;
    tx.commit().await?;
    let mut tx = app.db.pool.begin().await?;
    release(&mut tx, "uploads", 0, 0).await?;
    let rows=sqlx::query("DELETE FROM form_attachments WHERE id IN (SELECT id FROM form_attachments WHERE entry_id='' AND expires_at<=$1 ORDER BY expires_at,id LIMIT 50) RETURNING form_id,filename,size").bind(crate::now()).fetch_all(&mut *tx).await?;
    for row in &rows {
        sqlx::query("UPDATE form_upload_usage SET bytes=bytes-$1,files=files-1 WHERE form_id=$2")
            .bind(row.get::<i64, _>("size"))
            .bind(row.get::<String, _>("form_id"))
            .execute(&mut *tx)
            .await?;
    }
    release(
        &mut tx,
        "uploads",
        rows.len() as i64,
        rows.iter().map(|r| r.get::<i64, _>("size")).sum(),
    )
    .await?;
    tx.commit().await?;
    for row in rows {
        let filename: String = row.get("filename");
        if filename.ends_with(".dat")
            && uuid::Uuid::parse_str(filename.trim_end_matches(".dat")).is_ok()
        {
            let _ = tokio::fs::remove_file(app.config.data_dir.join("attachments").join(filename))
                .await;
        }
    }
    let mut tx = app.db.pool.begin().await?;
    release(&mut tx, "mail", 0, 0).await?;
    let rows=sqlx::query("DELETE FROM mail_jobs WHERE id IN (SELECT id FROM mail_jobs WHERE state IN ('sent','spooled','dead','cancelled') AND created_at<$1 ORDER BY created_at,id LIMIT 50) RETURNING id,html,plain,subject").bind(crate::now()-app.config.business_limits.mail_retention_days*86400).fetch_all(&mut *tx).await?;
    release(
        &mut tx,
        "mail",
        rows.len() as i64,
        rows.iter()
            .map(|r| {
                r.get::<String, _>("html").len() as i64
                    + r.get::<String, _>("plain").len() as i64
                    + r.get::<String, _>("subject").len() as i64
            })
            .sum(),
    )
    .await?;
    tx.commit().await?;
    for row in rows {
        let id: String = row.get("id");
        if uuid::Uuid::parse_str(&id).is_ok() {
            let _ = tokio::fs::remove_file(
                app.config.data_dir.join("outbox").join(format!("{id}.eml")),
            )
            .await;
        }
    }
    Ok(())
}
