use super::encryption;
use crate::{
    App, backup,
    error::{Error, Result},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub key_file: PathBuf,
    /// Existing owner-managed directories. A mounted NAS/secondary disk can be used;
    /// configuration alone cannot prove that it is physically independent.
    pub destinations: Vec<PathBuf>,
    pub interval_seconds: u64,
    pub retain: usize,
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.enabled {
            anyhow::ensure!(
                !self.key_file.as_os_str().is_empty(),
                "recovery requires key_file"
            );
            anyhow::ensure!(
                (60..=2_592_000).contains(&self.interval_seconds),
                "recovery interval must be 60 seconds to 30 days"
            );
            anyhow::ensure!(
                (1..=365).contains(&self.retain),
                "recovery retain must be 1..365"
            );
            anyhow::ensure!(
                (1..=8).contains(&self.destinations.len()),
                "recovery needs 1..8 existing destinations"
            );
            for p in &self.destinations {
                anyhow::ensure!(p.is_absolute(), "recovery destinations must be absolute");
            }
        }
        Ok(())
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub last_attempt: i64,
    pub last_complete: i64,
    pub package: String,
    pub copies: Vec<CopyStatus>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyStatus {
    pub destination: PathBuf,
    pub state: String,
}

pub async fn status(app: &App) -> Result<Status> {
    let path = app.config.data_dir.join("recovery-status.json");
    match backup::read_bounded(&path, 32 * 1024).await {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| Error::invalid("Recovery status is damaged; inspect it before retrying.")),
        Err(Error(_, _)) if !path.exists() => Ok(Status::default()),
        Err(e) => Err(e),
    }
}

// Private write, fsync and atomic rename. Temp files never look like completed
// packages. A crash after rename leaves a complete package, not half ciphertext.
fn publish(path: &Path, bytes: &[u8], replace: bool) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("missing destination"))?;
    let temp = parent.join(format!(".wpalt-partial-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        backup::write_private(&temp, bytes)?;
        if replace {
            std::fs::rename(&temp, path)?;
        } else {
            // Unlike rename, hard_link cannot replace an existing package.
            std::fs::hard_link(&temp, path)?;
            std::fs::remove_file(&temp)?;
        }
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}
fn package_name(name: &str) -> bool {
    name.strip_prefix("wpalt-")
        .and_then(|s| s.strip_suffix(".wpbackup"))
        .and_then(|s| s.split_once('-'))
        .is_some_and(|(time, id)| {
            time.len() == 10
                && time.bytes().all(|b| b.is_ascii_digit())
                && uuid::Uuid::parse_str(id).is_ok()
        })
}
async fn retain(
    destination: &Path,
    keep: usize,
    current: &str,
    key: &[u8; 32],
    limit: usize,
) -> Result<()> {
    let mut entries = tokio::fs::read_dir(destination).await?;
    let mut packages = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if package_name(&name) && entry.file_type().await?.is_file() {
            packages.push((name, entry.path()));
            if packages.len() > 10_000 {
                return Err(Error::invalid(
                    "Recovery destination has too many packages; inspect retention manually.",
                ));
            }
        }
    }
    packages.sort_by(|a, b| b.0.cmp(&a.0));
    // Retention only follows successful write/authentication. Current package is
    // always retained, including same-second filenames with arbitrary UUID order.
    let mut kept = 1;
    for (name, path) in packages {
        if name == current {
            continue;
        }
        if kept < keep {
            kept += 1;
            continue;
        }
        let bytes = backup::read_bounded(&path, limit + encryption::OVERHEAD).await?;
        encryption::open(key, &bytes, limit)?;
        tokio::fs::remove_file(path).await?;
    }
    Ok(())
}

pub async fn run(app: &App) -> Result<Status> {
    let _guard = app.recovery_work.lock().await;
    let config = &app.config.recovery;
    if !config.enabled {
        return Err(Error::invalid(
            "Configure and enable recovery before creating managed backups.",
        ));
    }
    let key = encryption::read_key(&config.key_file).await?;
    let previous = status(app).await?;
    let mut state = Status {
        last_attempt: crate::now(),
        last_complete: previous.last_complete,
        package: format!("wpalt-{}-{}.wpbackup", crate::now(), uuid::Uuid::new_v4()),
        copies: config
            .destinations
            .iter()
            .map(|p| CopyStatus {
                destination: p.clone(),
                state: "pending".into(),
            })
            .collect(),
    };
    let state_path = app.config.data_dir.join("recovery-status.json");
    // Persist before capture/copy: restart can show an interrupted attempt.
    publish(&state_path, &serde_json::to_vec(&state).unwrap(), true)
        .map_err(|_| Error::invalid("Cannot persist recovery status; check site storage."))?;
    let plaintext = backup::capture(app).await?;
    let encoded = encryption::seal(&key, &plaintext)?;
    for copy in &mut state.copies {
        let result: Result<()> = async {
            let metadata = tokio::fs::symlink_metadata(&copy.destination).await?;
            if !metadata.is_dir() {
                return Err(Error::invalid(
                    "Backup destination must be an existing directory.",
                ));
            }
            let destination = tokio::fs::canonicalize(&copy.destination).await?;
            let key_path = tokio::fs::canonicalize(&config.key_file).await?;
            if key_path.starts_with(&destination) {
                return Err(Error::invalid(
                    "Recovery key must be stored outside the backup destination.",
                ));
            }
            let path = destination.join(&state.package);
            let bytes = encoded.clone();
            let output = path.clone();
            tokio::task::spawn_blocking(move || publish(&output, &bytes, false))
                .await
                .map_err(|_| Error::invalid("Backup worker interrupted."))?
                .map_err(|_| Error::invalid("Backup destination write failed."))?;
            let stored =
                backup::read_bounded(&path, app.config.max_backup_bytes + encryption::OVERHEAD)
                    .await?;
            if encryption::open(&key, &stored, app.config.max_backup_bytes)? != plaintext {
                return Err(Error::invalid("Stored backup verification failed."));
            }
            retain(
                &destination,
                config.retain,
                &state.package,
                &key,
                app.config.max_backup_bytes,
            )
            .await?;
            Ok(())
        }
        .await;
        copy.state = if result.is_ok() { "verified" } else { "failed" }.into();
        tracing::info!(event="recovery_copy", state=%copy.state);
    }
    if state.copies.iter().all(|c| c.state == "verified") {
        state.last_complete = state.last_attempt;
    }
    publish(&state_path, &serde_json::to_vec(&state).unwrap(), true)
        .map_err(|_| Error::invalid("Cannot persist completed recovery status."))?;
    Ok(state)
}

pub async fn tick(app: &App) -> Result<()> {
    if !app.config.recovery.enabled {
        return Ok(());
    }
    let previous = status(app).await?;
    if crate::now().saturating_sub(previous.last_attempt)
        < app.config.recovery.interval_seconds as i64
    {
        return Ok(());
    }
    let result = run(app).await?;
    if result.copies.iter().any(|c| c.state != "verified") {
        return Err(Error::invalid(
            "Recovery copy incomplete; inspect Operations and destination storage.",
        ));
    }
    Ok(())
}
