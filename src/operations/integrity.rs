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
    pub next_image: String,
    pub next_attachment: String,
    pub pattern_warnings: Vec<Finding>,
}
#[derive(Serialize)]
pub struct Finding {
    pub kind: &'static str,
    pub id: String,
    pub reason: &'static str,
}

pub async fn scan(app: &App) -> Result<Report> {
    scan_page(app, "", "").await
}

pub async fn scan_page(app: &App, after_image: &str, after_attachment: &str) -> Result<Report> {
    for cursor in [after_image, after_attachment] {
        if !cursor.is_empty() && cursor != "done" {
            uuid::Uuid::parse_str(cursor).map_err(|_| Error::invalid("Invalid scan cursor."))?;
        }
    }
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
        next_image: String::new(),
        next_attachment: String::new(),
        pattern_warnings: vec![],
    };
    let mut total = 0usize;
    for (kind, table, dir, limit, after) in [
        (
            "image",
            "media",
            "media",
            32 * 1024 * 1024usize,
            after_image,
        ),
        (
            "attachment",
            "form_attachments",
            "attachments",
            2 * 1024 * 1024usize,
            after_attachment,
        ),
    ] {
        if after == "done" {
            if kind == "image" {
                report.next_image = "done".into();
            } else {
                report.next_attachment = "done".into();
            }
            continue;
        }
        let rows = sqlx::query(&format!(
            "SELECT id,filename,sha256,size FROM {table} WHERE id>$1 ORDER BY id LIMIT 1001"
        ))
        .bind(after)
        .fetch_all(&app.db.pool)
        .await?;
        if rows.len() > 1000 {
            report.limited = true;
        }
        let mut cursor = after.to_owned();
        let mut incomplete = rows.len() > 1000;
        for row in rows.iter().take(1000) {
            let id: String = row.get("id");
            let filename: String = row.get("filename");
            let expected: i64 = row.get("size");
            if total >= 64 * 1024 * 1024
                || (expected > 0
                    && expected as usize > 64 * 1024 * 1024 - total
                    && expected as usize <= limit)
            {
                report.limited = true;
                incomplete = true;
                break;
            }
            cursor = id.clone();
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
            let checked: Result<()> = async {
                let metadata = tokio::fs::symlink_metadata(&path).await?;
                if !metadata.is_file() || expected < 0 || metadata.len() != expected as u64 {
                    return Err(Error::invalid("Invalid file metadata."));
                }
                let bytes =
                    backup::read_bounded(&path, limit.min(64 * 1024 * 1024 - total)).await?;
                total += bytes.len();
                let (hash, warning) = tokio::task::spawn_blocking(move || {
                    let warning = [
                        b"<?php".as_slice(),
                        b"eval(base64_decode(".as_slice(),
                        b"/JavaScript".as_slice(),
                    ]
                    .iter()
                    .any(|pattern| bytes.windows(pattern.len()).any(|w| w == *pattern));
                    (digest(&bytes), warning)
                })
                .await
                .map_err(|_| Error::invalid("Integrity worker interrupted."))?;
                if warning {
                    report.pattern_warnings.push(Finding {
                        kind,
                        id: id.clone(),
                        reason: "embedded_executable_pattern_review_required",
                    });
                }
                if hash != row.get::<String, _>("sha256") {
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
        let next = if incomplete { cursor } else { "done".into() };
        if kind == "image" {
            report.next_image = next;
        } else {
            report.next_attachment = next;
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
