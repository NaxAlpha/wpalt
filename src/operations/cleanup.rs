//! Conservative, bounded cleanup. Historical and draft references retain media.
//! Database removal commits before unlink: interruption leaves retryable orphans,
//! never a registered missing file. One owning application process is required.
use crate::{
    App,
    auth::digest,
    backup,
    error::{Error, Result},
};
use futures_util::TryStreamExt;
use serde::Serialize;
use sqlx::Row;
use std::collections::{BTreeMap, HashSet};

#[derive(Serialize)]
pub struct Candidate {
    pub id: String,
    pub filename: String,
    pub bytes: u64,
    pub registered: bool,
}
#[derive(Serialize)]
pub struct Plan {
    pub hash: String,
    pub candidates: Vec<Candidate>,
    pub retained: usize,
    pub expired_sessions: i64,
    pub cutoff: i64,
}
#[derive(Debug, Serialize)]
pub struct Outcome {
    pub removed_files: usize,
    pub removed_bytes: u64,
    pub removed_sessions: u64,
    pub pending_files: Vec<String>,
}
// Also recognize percent-encoded URLs and decoded JSON strings. No network fetches.
pub(crate) fn references(text: &str, ids: &mut HashSet<String>) {
    let mut decoded = Vec::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3])
            && let Ok(value) = u8::from_str_radix(hex, 16)
        {
            decoded.push(value);
            i += 3;
            continue;
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    for part in decoded.split(|b| !b.is_ascii_hexdigit() && *b != b'-') {
        // A UUID can occur inside a longer literal; conservatively keep it too.
        for chunk in part.windows(36) {
            if let Ok(s) = std::str::from_utf8(chunk)
                && let Ok(id) = uuid::Uuid::parse_str(s)
            {
                ids.insert(id.to_string());
            }
        }
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        fn strings(v: &serde_json::Value, ids: &mut HashSet<String>) {
            match v {
                serde_json::Value::String(s) => references(s, ids),
                serde_json::Value::Array(a) => a.iter().for_each(|v| strings(v, ids)),
                serde_json::Value::Object(o) => o.iter().for_each(|(k, v)| {
                    references(k, ids);
                    strings(v, ids)
                }),
                _ => (),
            }
        }
        // Only container values recurse; a JSON string containing itself cannot loop.
        if value.is_array() || value.is_object() {
            strings(&value, ids);
        }
    }
}
async fn scan_batch(texts: Vec<String>, referenced: &mut HashSet<String>) -> Result<()> {
    let found = tokio::task::spawn_blocking(move || {
        let mut found = HashSet::new();
        for text in texts {
            references(&text, &mut found);
            if found.len() > 100_000 {
                return Err(Error::invalid(
                    "Cleanup reference budget exceeded; no deletion performed.",
                ));
            }
        }
        Ok(found)
    })
    .await
    .map_err(|_| Error::invalid("Cleanup worker failed; no deletion performed."))??;
    referenced.extend(found);
    if referenced.len() > 100_000 {
        return Err(Error::invalid(
            "Cleanup reference budget exceeded; no deletion performed.",
        ));
    }
    Ok(())
}
async fn inventory(app: &App, cutoff: i64) -> Result<Plan> {
    let mut referenced = HashSet::new();
    let mut budget = 0usize;
    let mut batch = vec![];
    let mut batch_bytes = 0usize;
    let mut rows_count = 0usize;
    let mut tx = app.db.pool.begin().await?;
    if app.db.postgres {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
    }
    // The backup contract lists every persisted module, including revisions,
    // delivery payloads, protected learning and immutable commerce history.
    for (table, columns) in backup::TABLES {
        if *table == "media" {
            continue;
        }
        let text_columns: Vec<_> = columns
            .iter()
            .filter(|(_, number)| !number)
            .map(|(c, _)| *c)
            .collect();
        if text_columns.is_empty() {
            continue;
        }
        let query = format!("SELECT {} FROM {table}", text_columns.join(","));
        let mut rows = sqlx::query(&query).fetch(&mut *tx);
        while let Some(row) = rows.try_next().await? {
            rows_count += 1;
            if rows_count > 100_000 {
                return Err(Error::invalid(
                    "Cleanup scan exceeds 100,000 rows; no deletion performed.",
                ));
            }
            for column in &text_columns {
                let text: String = row.get(*column);
                budget = budget.saturating_add(text.len());
                if budget > 64 * 1024 * 1024 {
                    return Err(Error::invalid(
                        "Cleanup scan exceeds 64 MiB; no deletion performed.",
                    ));
                }
                batch_bytes += text.len();
                batch.push(text);
                if batch_bytes >= 1024 * 1024 {
                    scan_batch(std::mem::take(&mut batch), &mut referenced).await?;
                    batch_bytes = 0;
                }
            }
        }
    }
    scan_batch(batch, &mut referenced).await?;
    let rows = sqlx::query("SELECT id,filename FROM media ORDER BY id LIMIT 1001")
        .fetch_all(&mut *tx)
        .await?;
    if rows.len() > 1000 {
        return Err(Error::invalid(
            "Cleanup supports up to 1,000 media records; no deletion performed.",
        ));
    }
    let mut registered = BTreeMap::new();
    for row in rows {
        let id: String = row.get("id");
        let filename: String = row.get("filename");
        if !backup::safe_filename(&filename) || filename.split('.').next() != Some(id.as_str()) {
            return Err(Error::invalid("Unsafe media inventory blocks cleanup."));
        }
        registered.insert(filename, id);
    }
    let expired_sessions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE expires_at<$1")
            .bind(cutoff)
            .fetch_one(&mut *tx)
            .await?;
    tx.commit().await?;
    let directory = app.config.data_dir.join("media");
    if tokio::fs::symlink_metadata(&directory)
        .await?
        .file_type()
        .is_symlink()
    {
        return Err(Error::invalid("Unsafe media directory blocks cleanup."));
    }
    let mut entries = tokio::fs::read_dir(&directory).await?;
    let mut files = BTreeMap::new();
    while let Some(entry) = entries.next_entry().await? {
        if files.len() >= 1000 {
            return Err(Error::invalid(
                "Cleanup supports up to 1,000 media files; no deletion performed.",
            ));
        }
        let filename = entry
            .file_name()
            .into_string()
            .map_err(|_| Error::invalid("Unsafe file blocks cleanup."))?;
        let metadata = tokio::fs::symlink_metadata(entry.path()).await?;
        if !backup::safe_filename(&filename)
            || !metadata.is_file()
            || metadata.file_type().is_symlink()
        {
            return Err(Error::invalid(
                "Unknown or unsafe file blocks cleanup; inspect it manually.",
            ));
        }
        files.insert(filename, metadata.len());
    }
    // Registered files missing from storage require integrity repair, not cleanup.
    if registered.keys().any(|name| !files.contains_key(name)) {
        return Err(Error::invalid(
            "Missing registered media blocks cleanup; run integrity inspection.",
        ));
    }
    let mut candidates = vec![];
    let mut retained = 0;
    for (filename, bytes) in files {
        let id = filename.split('.').next().unwrap().to_owned();
        if referenced.contains(&id) {
            retained += 1;
            continue;
        }
        candidates.push(Candidate {
            registered: registered.contains_key(&filename),
            id,
            filename,
            bytes,
        });
    }
    let hash = digest(
        &serde_json::to_vec(&(cutoff, expired_sessions, &candidates))
            .map_err(|_| Error::invalid("Invalid cleanup inventory."))?,
    );
    Ok(Plan {
        hash,
        candidates,
        retained,
        expired_sessions,
        cutoff,
    })
}
pub async fn preview(app: &App) -> Result<Plan> {
    let _permit = app
        .media_work
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::invalid("Maintenance workers are busy; try again shortly."))?;
    let _guard = app.mutations.lock().await;
    inventory(app, crate::now()).await
}
pub async fn execute(app: &App, expected: &str, cutoff: i64) -> Result<Outcome> {
    if expected.len() != 64
        || !expected.bytes().all(|b| b.is_ascii_hexdigit())
        || cutoff > crate::now()
        || cutoff < crate::now() - 600
    {
        return Err(Error::invalid(
            "Cleanup preview expired or is invalid; inspect again.",
        ));
    }
    let _permit = app
        .media_work
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::invalid("Maintenance workers are busy; try again shortly."))?;
    let _guard = app.mutation().await?;
    let plan = inventory(app, cutoff).await?;
    if plan.hash != expected {
        return Err(Error::conflict());
    }
    let mut tx = app.db.pool.begin().await?;
    for item in &plan.candidates {
        if item.registered {
            sqlx::query("DELETE FROM media WHERE id=$1 AND filename=$2")
                .bind(&item.id)
                .bind(&item.filename)
                .execute(&mut *tx)
                .await?;
        }
    }
    let removed_sessions = sqlx::query("DELETE FROM sessions WHERE expires_at<$1")
        .bind(cutoff)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    let mut outcome = Outcome {
        removed_files: 0,
        removed_bytes: 0,
        removed_sessions,
        pending_files: vec![],
    };
    for item in plan.candidates {
        match tokio::fs::remove_file(app.config.data_dir.join("media").join(&item.filename)).await {
            Ok(()) => {
                outcome.removed_files += 1;
                outcome.removed_bytes += item.bytes;
            }
            Err(_) => outcome.pending_files.push(item.filename),
        }
    }
    // Failed/uninterrupted unlink retries appear as unregistered candidates in the
    // next preview. Their database IDs are absent and cannot be served meanwhile.
    app.media_cache.lock().await.clear();
    Ok(outcome)
}
