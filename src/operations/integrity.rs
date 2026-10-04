//! Bounded inventory-based integrity scan; not a malware intelligence service.
use crate::{
    App,
    auth::digest,
    backup,
    error::{Error, Result},
};
use serde::Serialize;
use sqlx::Row;

#[derive(Serialize)]
pub struct Report {
    pub checked_at: i64,
    pub checked: usize,
    pub failed: Vec<Finding>,
    pub limited: bool,
}
#[derive(Serialize)]
pub struct Finding {
    pub kind: &'static str,
    pub id: String,
    pub reason: &'static str,
}

pub async fn scan(app: &App) -> Result<Report> {
    let _permit = app
        .media_work
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::invalid("Maintenance workers are busy; try again shortly."))?;
    let _guard = app.mutations.lock().await;
    let mut report = Report {
        checked_at: crate::now(),
        checked: 0,
        failed: vec![],
        limited: false,
    };
    let mut total = 0usize;
    for (kind, table, dir, limit) in [
        ("image", "media", "media", 32 * 1024 * 1024usize),
        (
            "attachment",
            "form_attachments",
            "attachments",
            2 * 1024 * 1024usize,
        ),
    ] {
        let rows = sqlx::query(&format!(
            "SELECT id,filename,sha256,size FROM {table} ORDER BY id LIMIT 1001"
        ))
        .fetch_all(&app.db.pool)
        .await?;
        if rows.len() > 1000 {
            report.limited = true;
        }
        for row in rows.iter().take(1000) {
            let id: String = row.get("id");
            let filename: String = row.get("filename");
            let safe = if kind == "image" {
                backup::safe_filename(&filename)
            } else {
                crate::business::attachments::safe_filename(&filename)
            };
            if !safe {
                report.failed.push(Finding {
                    kind,
                    id,
                    reason: "unsafe_metadata_path",
                });
                continue;
            }
            if total >= 64 * 1024 * 1024 {
                report.limited = true;
                break;
            }
            let path = app.config.data_dir.join(dir).join(filename);
            let expected: i64 = row.get("size");
            let checked: Result<()> = async {
                let metadata = tokio::fs::symlink_metadata(&path).await?;
                if !metadata.is_file() || expected < 0 || metadata.len() != expected as u64 {
                    return Err(Error::invalid("Invalid file metadata."));
                }
                let bytes =
                    backup::read_bounded(&path, limit.min(64 * 1024 * 1024 - total)).await?;
                total += bytes.len();
                if digest(&bytes) != row.get::<String, _>("sha256") {
                    return Err(Error::invalid("Integrity mismatch."));
                }
                Ok(())
            }
            .await;
            report.checked += 1;
            if checked.is_err() {
                report.failed.push(Finding {
                    kind,
                    id,
                    reason: "missing_damaged_or_unsafe_file",
                });
            }
        }
    }
    tracing::info!(
        event = "integrity_scan",
        checked = report.checked,
        failures = report.failed.len(),
        limited = report.limited
    );
    Ok(report)
}
