//! Conservative same-host coordination. A durable unresolved intent fences the
//! entire site after crash/cancellation; this is not a cross-host lease protocol.
use crate::{
    App, auth,
    config::Config,
    error::{Error, Result},
    operations,
};
use axum::http::StatusCode;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS process_authority(id BIGINT PRIMARY KEY CHECK(id=1),directory_digest TEXT NOT NULL);";

/// Bind native database authority to one canonical host directory. This private
/// deployment identity is deliberately absent from portable recovery graphs.
pub async fn verify_directory(db: &crate::db::Db, config: &Config, initialize: bool) -> Result<()> {
    let present: i64 = if db.postgres {
        sqlx::query_scalar(
            "SELECT CASE WHEN to_regclass('process_authority') IS NULL THEN 0 ELSE 1 END",
        )
        .fetch_one(&db.pool)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='process_authority'",
        )
        .fetch_one(&db.pool)
        .await?
    };
    if present == 0 {
        return if initialize {
            Err(unavailable())
        } else {
            Ok(())
        };
    }
    let directory = std::fs::canonicalize(&config.data_dir).map_err(|_| unavailable())?;
    let digest = auth::digest(directory.as_os_str().as_encoded_bytes());
    if initialize {
        sqlx::query("INSERT INTO process_authority(id,directory_digest) VALUES(1,$1) ON CONFLICT(id) DO NOTHING")
            .bind(&digest).execute(&db.pool).await?;
    }
    let stored: Option<String> =
        sqlx::query_scalar("SELECT directory_digest FROM process_authority WHERE id=1")
            .fetch_optional(&db.pool)
            .await?;
    if stored.as_ref().is_some_and(|s| s != &digest) || (initialize && stored.is_none()) {
        return Err(Error::invalid(
            "Database is bound to another site directory. Stop all nodes and use fresh-target recovery; do not create a second coordination root.",
        ));
    }
    Ok(())
}

const MAX_STATE: usize = 4 * 1024 * 1024;
fn unavailable() -> Error {
    unavailable_reason("private-storage-or-state-validation")
}
fn unavailable_reason(reason: &'static str) -> Error {
    tracing::error!(event = "local_process_coordination_refused", reason);
    Error(
        StatusCode::SERVICE_UNAVAILABLE,
        "Local process coordination is unavailable or paused; stop all nodes and inspect the site before resuming.",
    )
}

fn executable_identity() -> Result<String> {
    use sha2::{Digest, Sha256};
    static IDENTITY: std::sync::OnceLock<std::result::Result<String, ()>> =
        std::sync::OnceLock::new();
    IDENTITY
        .get_or_init(|| {
            let path = std::env::current_exe().map_err(|_| ())?;
            let mut file = File::open(path).map_err(|_| ())?;
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 16384];
            let mut total = 0usize;
            loop {
                let read = file.read(&mut buffer).map_err(|_| ())?;
                if read == 0 {
                    break;
                }
                total = total.checked_add(read).ok_or(())?;
                if total > 512 * 1024 * 1024 {
                    return Err(());
                }
                hash.update(&buffer[..read]);
            }
            Ok(format!("{:x}", hash.finalize()))
        })
        .as_ref()
        .cloned()
        .map_err(|_| unavailable_reason("executable-identity-unavailable"))
}

pub struct Coordinator {
    directory: PathBuf,
    fingerprint: String,
    pub admission: tokio::sync::Semaphore,
    owned: Arc<AtomicBool>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    format: String,
    fingerprint: String,
    generation: u64,
    spam_secret: [u8; 32],
    login: auth::LoginLimits,
    protection: operations::protection::Limits,
    spam: operations::spam::Used,
    ceremonies: operations::passkeys::Ceremonies,
}
#[cfg(unix)]
fn private_open(path: &Path, create: bool) -> Result<File> {
    use rustix::fs::{Mode, OFlags, open};
    use std::os::unix::fs::MetadataExt;
    let flags = OFlags::RDWR | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let file: File = open(
        path,
        if create {
            flags | OFlags::CREATE
        } else {
            flags
        },
        Mode::from_raw_mode(0o600),
    )
    .map_err(|_| unavailable())?
    .into();
    let meta = file.metadata().map_err(|_| unavailable())?;
    if !meta.is_file() || meta.mode() & 0o077 != 0 || meta.nlink() != 1 {
        return Err(unavailable());
    }
    Ok(file)
}
#[cfg(not(unix))]
fn private_open(_path: &Path, _create: bool) -> Result<File> {
    Err(unavailable())
}
fn replace(path: &Path, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_STATE {
        return Err(unavailable());
    }
    if path.exists() {
        private_open(path, false)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    crate::backup::write_private(&temporary, bytes).map_err(|_| unavailable())?;
    let result = std::fs::rename(&temporary, path)
        .and_then(|_| File::open(path.parent().unwrap()).and_then(|f| f.sync_all()));
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Err(unavailable());
    }
    Ok(())
}
impl Coordinator {
    pub fn new(c: &Config) -> Result<Self> {
        let mut value = serde_json::to_value(c).map_err(|_| unavailable())?;
        value["listen"] = serde_json::Value::Null;
        value["debug"] = serde_json::Value::Null;
        value["runtime_version"] = env!("CARGO_PKG_VERSION").into();
        value["executable_sha256"] = executable_identity()?.into();
        let scripts = if c.business_enabled && c.engagement.enabled {
            operations::consent_scripts::Scripts::compile(&c.consent_scripts)
                .map_err(|_| unavailable_reason("consent-manifest-unavailable"))?
        } else {
            operations::consent_scripts::Scripts::default()
        };
        value["consent_script_manifest"] = scripts.manifest.into();
        value["database_schema"] = crate::db::SCHEMA_VERSION.into();
        value["data_dir"] =
            serde_json::to_value(std::fs::canonicalize(&c.data_dir).map_err(|_| unavailable())?)
                .map_err(|_| unavailable())?;
        Ok(Self {
            directory: c.data_dir.clone(),
            fingerprint: auth::digest(&serde_json::to_vec(&value).map_err(|_| unavailable())?),
            admission: tokio::sync::Semaphore::new(c.request_concurrency),
            owned: Arc::new(AtomicBool::new(false)),
        })
    }
    pub async fn lock(&self) -> Result<File> {
        let file = private_open(&self.directory.join(".process-coordinator.lock"), true)?;
        let started = std::time::Instant::now();
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(file),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => return Err(unavailable()),
            }
            if started.elapsed() >= std::time::Duration::from_secs(10) {
                return Err(unavailable_reason("admission-timeout"));
            }
            // A bounded asynchronous retry avoids spinning while reducing the
            // tail penalty of successive short cross-process ownership holds.
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    }
    fn read(&self) -> Result<State> {
        let mut file = private_open(&self.directory.join(".process-state.json"), false)?;
        let mut bytes = Vec::new();
        file.by_ref()
            .take(MAX_STATE as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| unavailable())?;
        if bytes.len() > MAX_STATE {
            return Err(unavailable());
        }
        let state: State = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
        if state.format != "wpalt-local-process-state-v1" || state.fingerprint != self.fingerprint {
            return Err(unavailable_reason("format-or-configuration-mismatch"));
        }
        Ok(state)
    }
    pub fn initialize(&self) -> Result<[u8; 32]> {
        if self.directory.join(".process-intent").exists() {
            return Err(unavailable_reason("unreconciled-operation"));
        }
        if self.directory.join(".process-state.json").exists() {
            return Ok(self.read()?.spam_secret);
        }
        let state = State {
            format: "wpalt-local-process-state-v1".into(),
            fingerprint: self.fingerprint.clone(),
            generation: 0,
            spam_secret: operations::encryption::generate_key(),
            login: Default::default(),
            protection: Default::default(),
            spam: Default::default(),
            ceremonies: Default::default(),
        };
        replace(
            &self.directory.join(".process-state.json"),
            &serde_json::to_vec(&state).map_err(|_| unavailable())?,
        )?;
        Ok(state.spam_secret)
    }
    /// Mark before entering native mutation authority, after request parsing.
    /// Offline CLI callers are protected by the exclusive lifecycle lock instead.
    pub fn mark_intent(&self) -> Result<()> {
        if !self.owned.load(Ordering::SeqCst) {
            return Ok(());
        }
        let path = self.directory.join(".process-intent");
        if path.exists() {
            private_open(&path, false)?;
            return Ok(());
        }
        crate::backup::write_private(&path, b"Unresolved domain operation. Stop all nodes; inspect state and external effects before resuming.\n").map_err(|_| unavailable())?;
        File::open(&self.directory)
            .and_then(|f| f.sync_all())
            .map_err(|_| unavailable())?;
        Ok(())
    }
    pub async fn check(&self) -> Result<()> {
        let _file = self.lock().await?;
        if self.directory.join(".process-intent").exists() {
            return Err(unavailable_reason("unreconciled-operation"));
        }
        self.read()?;
        Ok(())
    }
    pub async fn begin(&self, app: &App) -> Result<Guard> {
        let file = self.lock().await?;
        if self.directory.join(".process-intent").exists() {
            return Err(unavailable_reason("unreconciled-operation"));
        }
        let state = self.read()?;
        if state.spam_secret != *app.spam_secret {
            return Err(unavailable_reason("runtime-security-key-mismatch"));
        }
        let previous = auth::digest(
            &serde_json::to_vec(&serde_json::to_value(&state).map_err(|_| unavailable())?)
                .map_err(|_| unavailable())?,
        );
        app.cache_generation
            .store(state.generation, Ordering::SeqCst);
        *app.login_limits.lock().await = state.login;
        *app.protection_limits.lock().await = state.protection;
        *app.spam_used.lock().await = state.spam;
        *app.passkey_ceremonies.lock().await = state.ceremonies;
        let (schema, held, directory): (i64, i64, String) = sqlx::query_as("SELECT s.version,r.held,a.directory_digest FROM schema_version s CROSS JOIN recovery_mode r CROSS JOIN process_authority a WHERE s.id=1 AND r.id=1 AND a.id=1").fetch_one(&app.db.pool).await?;
        if schema != crate::db::SCHEMA_VERSION || directory != app.db.directory_digest {
            return Err(unavailable());
        }
        app.clone_held.store(held == 1, Ordering::SeqCst);
        self.owned.store(true, Ordering::SeqCst);
        Ok(Guard {
            _file: file,
            owned: self.owned.clone(),
            directory: self.directory.clone(),
            fingerprint: self.fingerprint.clone(),
            previous,
        })
    }
}
pub struct Guard {
    owned: Arc<AtomicBool>,
    _file: File,
    directory: PathBuf,
    fingerprint: String,
    previous: String,
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.owned.store(false, Ordering::SeqCst);
    }
}
impl Guard {
    pub fn unresolved(&self) -> bool {
        self.directory.join(".process-intent").exists()
    }
    /// Explicit completion only. Drop after abort/error leaves durable pause intent.
    pub async fn complete(self, app: &App) -> Result<()> {
        let state = State {
            format: "wpalt-local-process-state-v1".into(),
            fingerprint: self.fingerprint.clone(),
            generation: app.cache_generation.load(Ordering::SeqCst),
            spam_secret: *app.spam_secret,
            login: serde_json::from_value(
                serde_json::to_value(&*app.login_limits.lock().await).map_err(|_| unavailable())?,
            )
            .map_err(|_| unavailable())?,
            protection: serde_json::from_value(
                serde_json::to_value(&*app.protection_limits.lock().await)
                    .map_err(|_| unavailable())?,
            )
            .map_err(|_| unavailable())?,
            spam: serde_json::from_value(
                serde_json::to_value(&*app.spam_used.lock().await).map_err(|_| unavailable())?,
            )
            .map_err(|_| unavailable())?,
            ceremonies: serde_json::from_value(
                serde_json::to_value(&*app.passkey_ceremonies.lock().await)
                    .map_err(|_| unavailable())?,
            )
            .map_err(|_| unavailable())?,
        };
        let bytes = serde_json::to_vec(&serde_json::to_value(&state).map_err(|_| unavailable())?)
            .map_err(|_| unavailable())?;
        if auth::digest(&bytes) == self.previous && !self.unresolved() {
            // Read-only requests keep the same durable authority; no redundant fsync.
            // Do not bypass admission, replay/abuse state or changed-generation writes.
            return Ok(());
        }
        // Keep ownership in the blocking task through sync, even if caller is aborted.
        tokio::task::spawn_blocking(move || {
            replace(&self.directory.join(".process-state.json"), &bytes)?;
            if self.directory.join(".process-intent").exists() {
                std::fs::remove_file(self.directory.join(".process-intent"))
                    .map_err(|_| unavailable())?;
            }
            File::open(&self.directory)
                .and_then(|f| f.sync_all())
                .map_err(|_| unavailable())?;
            drop(self);
            Ok(())
        })
        .await
        .map_err(|_| unavailable())?
    }
}

/// Offline host-owner reconciliation. Caller holds the exclusive site lifecycle
/// lock and uses an App with automatic local coordination disabled.
pub async fn resume(
    config: &Config,
    app: &App,
    execute: Option<&str>,
    acknowledged: bool,
) -> Result<serde_json::Value> {
    let coordinator = Coordinator::new(config)?;
    let _ownership = coordinator.lock().await?;
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pg_stat_activity WHERE application_name=$1 AND pid<>pg_backend_pid() AND state<>'idle'")
        .bind(&app.db.application_name).fetch_one(&app.db.pool).await?;
    if pending != 0 {
        return Err(Error::invalid(
            "Previous database operations have not drained; inspect PostgreSQL before retrying.",
        ));
    }
    let state_path = config.data_dir.join(".process-state.json");
    let mut state_file = private_open(&state_path, false)?;
    let mut state_bytes = Vec::new();
    state_file
        .by_ref()
        .take(MAX_STATE as u64 + 1)
        .read_to_end(&mut state_bytes)
        .map_err(|_| unavailable())?;
    if state_bytes.len() > MAX_STATE {
        return Err(unavailable());
    }
    let state: State = serde_json::from_slice(&state_bytes).map_err(|_| unavailable())?;
    if state.format != "wpalt-local-process-state-v1" {
        return Err(unavailable());
    }
    let encoded = crate::backup::capture(app).await?;
    let report = crate::backup::inspect(&app.config, &encoded)?;
    let envelope: serde_json::Value =
        serde_json::from_slice(&encoded).map_err(|_| unavailable())?;
    let mut payload: serde_json::Value =
        serde_json::from_str(envelope["payload"].as_str().ok_or_else(unavailable)?)
            .map_err(|_| unavailable())?;
    payload["created_at"] = 0.into();
    for records in payload["tables"]
        .as_object_mut()
        .ok_or_else(unavailable)?
        .values_mut()
    {
        records
            .as_array_mut()
            .ok_or_else(unavailable)?
            .sort_by_cached_key(|row| row.to_string());
    }
    for name in ["files", "private_files"] {
        payload[name]
            .as_array_mut()
            .ok_or_else(unavailable)?
            .sort_by_cached_key(|row| row["filename"].to_string());
    }
    let intent_path = config.data_dir.join(".process-intent");
    let intent = if intent_path.exists() {
        private_open(&intent_path, false)?;
        crate::backup::read_bounded(&intent_path, 1024).await?
    } else {
        Vec::new()
    };
    let plan = auth::digest(&serde_json::to_vec(&serde_json::json!({"state": auth::digest(&state_bytes), "intent": auth::digest(&intent), "domain": payload, "configuration": coordinator.fingerprint})).map_err(|_| unavailable())?);
    let mut output = serde_json::json!({"format":"wpalt-local-resume-v1", "executed":false, "plan":plan, "graph":report, "paused":!intent.is_empty(), "configuration_changed":state.fingerprint != coordinator.fingerprint, "boundary":"All nodes stopped; validated native graph. Review external mail/payment/identity outcomes separately. Resume invalidates temporary challenges and rotates the spam key; it does not replay or settle external work."});
    if let Some(expected) = execute {
        if expected != plan || !acknowledged {
            return Err(Error::invalid(
                "Review a fresh exact resume plan and acknowledge external side-effect reconciliation.",
            ));
        }
        let replacement = State {
            format: state.format,
            fingerprint: coordinator.fingerprint,
            generation: state.generation.checked_add(2).ok_or_else(unavailable)?,
            spam_secret: operations::encryption::generate_key(),
            login: state.login,
            protection: state.protection,
            spam: Default::default(),
            ceremonies: Default::default(),
        };
        replace(
            &state_path,
            &serde_json::to_vec(&replacement).map_err(|_| unavailable())?,
        )?;
        if !intent.is_empty() {
            std::fs::remove_file(intent_path).map_err(|_| unavailable())?;
        }
        File::open(&config.data_dir)
            .and_then(|f| f.sync_all())
            .map_err(|_| unavailable())?;
        output["executed"] = true.into();
        output["paused"] = false.into();
    }
    Ok(output)
}
