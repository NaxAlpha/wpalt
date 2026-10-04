//! Private bounded HTTP privileged-action journal. Intent precedes dispatch;
//! outcome follows response creation. An intent without outcome may be interrupted.
//! This is operational evidence, not a tamper-proof accounting ledger.
use crate::{
    App, backup,
    error::{Error, Result},
};
use serde::{Deserialize, Serialize};
use std::io::Write;

const LIMIT: u64 = 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub at: i64,
    pub request_id: String,
    pub actor: String,
    pub route: String,
    pub phase: String,
    pub status: u16,
}
pub async fn append(app: &App, event: Event) -> Result<()> {
    let _guard = app.audit_work.lock().await;
    let directory = app.config.data_dir.clone();
    let bytes = serde_json::to_vec(&event).map_err(|_| Error::invalid("Invalid audit event."))?;
    if !valid(&event) || bytes.len() > 4096 {
        return Err(Error::invalid("Audit event exceeds its budget."));
    }
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let path = directory.join("privileged-audit.jsonl");
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            if !meta.is_file() {
                return Err(std::io::Error::other("unsafe audit path"));
            }
            if meta.len() + bytes.len() as u64 + 1 > LIMIT {
                let old = directory.join("privileged-audit.previous.jsonl");
                if old.exists() {
                    std::fs::remove_file(&old)?;
                }
                std::fs::rename(&path, &old)?;
            }
        }
        let mut options = std::fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(())
    })
    .await
    .map_err(|_| Error::invalid("Audit worker interrupted."))?
    .map_err(|_| Error::invalid("Cannot persist audit history; check site storage."))?;
    Ok(())
}
pub async fn read(app: &App) -> Result<Vec<Event>> {
    let _guard = app.audit_work.lock().await;
    let mut records = Vec::new();
    for name in ["privileged-audit.jsonl", "privileged-audit.previous.jsonl"] {
        let path = app.config.data_dir.join(name);
        match tokio::fs::symlink_metadata(&path).await {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => return Err(Error::invalid("Unsafe audit history path.")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
        let bytes = backup::read_bounded(&path, LIMIT as usize).await?;
        let text =
            std::str::from_utf8(&bytes).map_err(|_| Error::invalid("Audit history is damaged."))?;
        for line in text.lines().rev() {
            let event: Event = serde_json::from_str(line).map_err(|_| {
                Error::invalid(
                    "Audit history contains an interrupted record; inspect the private file.",
                )
            })?;
            if !valid(&event) {
                return Err(Error::invalid("Audit history contains an invalid record."));
            }
            records.push(event);
        }
    }
    records.sort_by_key(|event: &Event| std::cmp::Reverse(event.at));
    let mut seen = std::collections::HashSet::new();
    records
        .retain(|event| seen.insert((event.request_id.clone(), event.phase.clone(), event.status)));
    records.truncate(200);
    Ok(records)
}
pub fn valid(event: &Event) -> bool {
    event.at >= 0
        && !event.request_id.is_empty()
        && !event.route.is_empty()
        && event.request_id.len() <= 128
        && event.actor.len() <= 256
        && event.route.len() <= 3072
        && ["intent", "response", "outcome"].contains(&event.phase.as_str())
        && event.status <= 599
        && serde_json::to_vec(event).is_ok_and(|bytes| bytes.len() <= 4096)
}
