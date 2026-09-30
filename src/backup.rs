use crate::{
    App,
    auth::digest,
    error::{Error, Result},
};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Any, Execute, QueryBuilder, Row};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

// Types and table names are an allowlist, never supplied by an archive.
const TABLES: &[(&str, &[(&str, bool)])] = &[
    (
        "settings",
        &[
            ("id", true),
            ("title", false),
            ("description", false),
            ("theme", false),
            ("navigation", false),
            ("field_schema", false),
        ],
    ),
    (
        "content_models",
        &[("id", false), ("definition", false), ("version", true)],
    ),
    (
        "site_design",
        &[
            ("id", true),
            ("draft_options", false),
            ("live_options", false),
            ("version", true),
            ("published_version", true),
        ],
    ),
    (
        "themes",
        &[
            ("id", false),
            ("name", false),
            ("draft", false),
            ("live", false),
            ("version", true),
            ("published_version", true),
            ("updated_at", true),
        ],
    ),
    (
        "theme_revisions",
        &[
            ("id", false),
            ("theme_id", false),
            ("version", true),
            ("package", false),
            ("published", true),
            ("created_at", true),
        ],
    ),
    (
        "users",
        &[
            ("id", false),
            ("email", false),
            ("name", false),
            ("role", false),
            ("password_hash", false),
            ("created_at", true),
        ],
    ),
    (
        "posts",
        &[
            ("id", false),
            ("slug", false),
            ("kind", false),
            ("title", false),
            ("body", false),
            ("fields", false),
            ("blocks", false),
            ("status", false),
            ("version", true),
            ("published_slug", false),
            ("published_title", false),
            ("published_body", false),
            ("published_fields", false),
            ("published_blocks", false),
            ("publish_at", true),
            ("published_at", true),
            ("updated_at", true),
            ("author_id", false),
        ],
    ),
    (
        "revisions",
        &[
            ("id", false),
            ("post_id", false),
            ("version", true),
            ("snapshot", false),
            ("created_at", true),
        ],
    ),
    (
        "terms",
        &[
            ("id", false),
            ("name", false),
            ("slug", false),
            ("kind", false),
        ],
    ),
    ("post_terms", &[("post_id", false), ("term_id", false)]),
    (
        "published_post_terms",
        &[("post_id", false), ("term_id", false)],
    ),
    (
        "media",
        &[
            ("id", false),
            ("filename", false),
            ("original_name", false),
            ("mime", false),
            ("alt", false),
            ("visibility", false),
            ("size", true),
            ("sha256", false),
            ("created_at", true),
        ],
    ),
    (
        "comments",
        &[
            ("id", false),
            ("post_id", false),
            ("name", false),
            ("body", false),
            ("status", false),
            ("created_at", true),
        ],
    ),
];
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema: i64,
    created_at: i64,
    tables: BTreeMap<String, Vec<BTreeMap<String, Value>>>,
    files: Vec<MediaFile>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MediaFile {
    filename: String,
    data: Vec<u8>,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    sha256: String,
    payload: String,
}

pub fn safe_filename(name: &str) -> bool {
    let Some((id, ext)) = name.rsplit_once('.') else {
        return false;
    };
    uuid::Uuid::parse_str(id).is_ok()
        && ["png", "jpg", "webp", "gif"].contains(&ext)
        && !name.contains('/')
        && !name.contains('\\')
}
pub async fn capture(app: &App) -> Result<Vec<u8>> {
    let _guard = app.mutations.lock().await;
    let mut tx = app.db.pool.begin().await?;
    if app.db.postgres {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
    }
    let mut tables = BTreeMap::new();
    let mut budget = 0_usize;
    for (name, columns) in TABLES {
        let sql = format!(
            "SELECT {} FROM {}",
            columns
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>()
                .join(","),
            name
        );
        let mut rows = sqlx::query(&sql).fetch(&mut *tx);
        let mut records = Vec::new();
        while let Some(row) = rows.try_next().await? {
            let mut record = BTreeMap::new();
            for (column, number) in *columns {
                let value = if *number {
                    Value::from(row.get::<i64, _>(*column))
                } else {
                    Value::from(row.get::<String, _>(*column))
                };
                budget += value.to_string().len();
                if budget > app.config.max_backup_bytes / 2 {
                    return Err(Error::invalid("Backup exceeds the configured size limit."));
                }
                record.insert((*column).into(), value);
            }
            records.push(record);
        }
        tables.insert((*name).into(), records);
    }
    let mut files = Vec::new();
    for row in tables.get("media").expect("media table collected") {
        let filename = row["filename"]
            .as_str()
            .ok_or(Error::invalid("Invalid media metadata."))?;
        if !safe_filename(filename) {
            return Err(Error::invalid("Unsafe stored media filename."));
        }
        let data = tokio::fs::read(app.config.data_dir.join("media").join(filename)).await?;
        budget += data.len() * 5;
        if budget > app.config.max_backup_bytes / 2 {
            return Err(Error::invalid("Backup exceeds the configured size limit."));
        }
        let hash = digest(&data);
        if row["sha256"].as_str() != Some(hash.as_str()) {
            return Err(Error::invalid("Stored media failed its integrity check."));
        }
        files.push(MediaFile {
            filename: filename.into(),
            data,
            sha256: hash,
        });
    }
    tx.commit().await?;
    let snapshot = Snapshot {
        schema: 2,
        created_at: crate::now(),
        tables,
        files,
    };
    let payload = serde_json::to_string(&snapshot)
        .map_err(|_| Error::invalid("Backup serialization failed."))?;
    let encoded = serde_json::to_vec(&Envelope {
        format: "wpalt-backup-v2".into(),
        sha256: digest(payload.as_bytes()),
        payload,
    })
    .map_err(|_| Error::invalid("Backup serialization failed."))?;
    if encoded.len() > app.config.max_backup_bytes {
        return Err(Error::invalid("Backup exceeds the configured size limit."));
    }
    tracing::info!(event = "backup_created", bytes = encoded.len());
    Ok(encoded)
}
pub async fn restore(app: &App, encoded: &[u8]) -> Result<()> {
    if encoded.len() > app.config.max_backup_bytes {
        return Err(Error::invalid("Backup exceeds the configured size limit."));
    }
    let envelope: Envelope =
        serde_json::from_slice(encoded).map_err(|_| Error::invalid("Invalid backup envelope."))?;
    if envelope.format != "wpalt-backup-v2"
        || digest(envelope.payload.as_bytes()) != envelope.sha256
    {
        return Err(Error::invalid("Backup checksum or format is invalid."));
    }
    let snapshot: Snapshot = serde_json::from_str(&envelope.payload)
        .map_err(|_| Error::invalid("Invalid backup payload."))?;
    if snapshot.schema != 2
        || snapshot.tables.len() != TABLES.len()
        || TABLES
            .iter()
            .any(|(name, _)| !snapshot.tables.contains_key(*name))
    {
        return Err(Error::invalid("Unsupported backup schema or table set."));
    }
    let settings = snapshot.tables["settings"]
        .first()
        .ok_or(Error::invalid("Backup has no site settings."))?;
    if snapshot.tables["settings"].len() != 1 {
        return Err(Error::invalid("Backup must contain one site."));
    }
    let setting_string = |key: &str| {
        settings
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or(Error::invalid("Invalid backup settings."))
    };
    crate::content::validate_settings(&crate::model::Settings {
        title: setting_string("title")?,
        description: setting_string("description")?,
        theme: setting_string("theme")?,
        navigation: setting_string("navigation")?,
        field_schema: setting_string("field_schema")?,
    })?;
    let mut expected = BTreeMap::new();
    for row in &snapshot.tables["media"] {
        let name = row
            .get("filename")
            .and_then(Value::as_str)
            .ok_or(Error::invalid("Invalid media metadata."))?;
        let hash = row
            .get("sha256")
            .and_then(Value::as_str)
            .ok_or(Error::invalid("Invalid media metadata."))?;
        let size = row
            .get("size")
            .and_then(Value::as_i64)
            .ok_or(Error::invalid("Invalid media metadata."))?;
        if !safe_filename(name) || expected.insert(name, (hash, size)).is_some() {
            return Err(Error::invalid("Unsafe or duplicate media filename."));
        }
    }
    if expected.len() != snapshot.files.len() {
        return Err(Error::invalid("Backup media is incomplete."));
    }
    let mut seen = HashSet::new();
    for file in &snapshot.files {
        let Some((hash, size)) = expected.get(file.filename.as_str()) else {
            return Err(Error::invalid("Unexpected media file."));
        };
        if !seen.insert(&file.filename)
            || !safe_filename(&file.filename)
            || file.data.len() > app.config.max_upload_bytes
            || digest(&file.data) != file.sha256
            || file.sha256 != *hash
            || file.data.len() as i64 != *size
        {
            return Err(Error::invalid("Backup media failed validation."));
        }
    }
    // Validate every row before any writes; no archive SQL or paths are executed.
    for (name, columns) in TABLES {
        for row in &snapshot.tables[*name] {
            if row.len() != columns.len()
                || columns.iter().any(|(column, number)| {
                    !row.get(*column).is_some_and(|v| {
                        if *number {
                            v.as_i64().is_some()
                        } else {
                            v.is_string()
                        }
                    })
                })
            {
                return Err(Error::invalid("Backup row has an invalid shape."));
            }
        }
    }
    let registry = crate::schema::Registry {
        common: serde_json::from_str(&setting_string("field_schema")?)
            .map_err(|_| Error::invalid("Invalid backup field definitions."))?,
        models: snapshot.tables["content_models"]
            .iter()
            .map(|row| {
                let id = row["id"]
                    .as_str()
                    .ok_or(Error::invalid("Invalid model identifier."))?;
                let definition = serde_json::from_str(
                    row["definition"]
                        .as_str()
                        .ok_or(Error::invalid("Invalid model definition."))?,
                )
                .map_err(|_| Error::invalid("Invalid model definition."))?;
                Ok((id.to_owned(), definition))
            })
            .collect::<Result<_>>()?,
    };
    registry.validate()?;
    let mut relationships = BTreeMap::new();
    let mut references = Vec::new();
    for row in &snapshot.tables["posts"] {
        let fields = registry.fields_for(row["kind"].as_str().unwrap())?;
        for column in ["fields", "published_fields"] {
            if column == "published_fields" && row["published_slug"].as_str().unwrap().is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(row[column].as_str().unwrap())
                .map_err(|_| Error::invalid("Invalid backup structured values."))?;
            registry.validate_values(&fields, &value)?;
            registry.references(&fields, &value, &mut relationships, &mut references)?;
        }
    }
    if snapshot.tables["site_design"].len() != 1 {
        return Err(Error::invalid("Backup needs one design state."));
    }
    for row in &snapshot.tables["site_design"] {
        for column in ["draft_options", "live_options"] {
            let value: Value = serde_json::from_str(row[column].as_str().unwrap())
                .map_err(|_| Error::invalid("Invalid option values."))?;
            registry.validate_values(&registry.common.options, &value)?;
            registry.references(
                &registry.common.options,
                &value,
                &mut relationships,
                &mut references,
            )?;
        }
    }
    let post_kinds: BTreeMap<_, _> = snapshot.tables["posts"]
        .iter()
        .map(|row| (row["id"].as_str().unwrap(), row["kind"].as_str().unwrap()))
        .collect();
    let media_ids: HashSet<_> = snapshot.tables["media"]
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect();
    if relationships
        .iter()
        .any(|(id, kind)| post_kinds.get(id.as_str()) != Some(&kind.as_str()))
        || references.iter().any(|id| !media_ids.contains(id.as_str()))
    {
        return Err(Error::invalid(
            "Backup has missing or mistyped structured references.",
        ));
    }
    let active = setting_string("theme")?;
    let mut active_found = false;
    if snapshot.tables["themes"].len() > 32 {
        return Err(Error::invalid("Backup has too many themes."));
    }
    for row in &snapshot.tables["themes"] {
        if !crate::schema::identifier(row["id"].as_str().unwrap()) {
            return Err(Error::invalid("Invalid backup theme identifier."));
        }
        for column in ["draft", "live"] {
            let raw = row[column].as_str().unwrap();
            if !raw.is_empty() {
                crate::theme::Package::parse(raw, &registry)?;
            }
        }
        let version = row["version"].as_i64().unwrap();
        let published = row["published_version"].as_i64().unwrap();
        if version < 1 || published < 0 || published > version {
            return Err(Error::invalid("Backup has invalid theme versions."));
        }
        if published > 0
            && !snapshot.tables["theme_revisions"].iter().any(|history| {
                history["theme_id"] == row["id"]
                    && history["version"].as_i64() == Some(published)
                    && history["published"].as_i64() == Some(1)
                    && history["package"] == row["live"]
            })
        {
            return Err(Error::invalid(
                "Backup publication history does not match its live theme.",
            ));
        }
        if row["id"].as_str() == Some(&active)
            && row["published_version"].as_i64().unwrap() > 0
            && !row["live"].as_str().unwrap().is_empty()
        {
            active_found = true;
        }
    }
    if !active_found {
        return Err(Error::invalid("Backup lacks its active published theme."));
    }
    // Old revisions can retain removed fields; validate executable/style grammar using
    // the current registry before exposing any historical publication stylesheet.
    for row in &snapshot.tables["theme_revisions"] {
        crate::theme::Package::parse_historical(row["package"].as_str().unwrap(), &registry)?;
    }
    let _guard = app.mutations.lock().await;
    let mut tx = app.db.pool.begin().await?;
    for (name, _) in TABLES {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {name}"))
            .fetch_one(&mut *tx)
            .await?;
        if count != 0 {
            return Err(Error::invalid(
                "Restore requires an empty target. Back up and use a fresh database/data directory.",
            ));
        }
    }
    for (name, columns) in TABLES {
        for row in &snapshot.tables[*name] {
            let mut q = QueryBuilder::<Any>::new(format!(
                "INSERT INTO {name}({}) VALUES(",
                columns
                    .iter()
                    .map(|(n, _)| *n)
                    .collect::<Vec<_>>()
                    .join(",")
            ));
            let mut values = q.separated(",");
            for (column, number) in *columns {
                if *number {
                    values.push_bind(row[*column].as_i64().unwrap());
                } else {
                    values.push_bind(row[*column].as_str().unwrap());
                }
            }
            values.push_unseparated(")");
            let mut query = q.build();
            let sql = crate::db::Db::numbered(query.sql());
            let args = query
                .take_arguments()
                .map_err(sqlx::Error::Encode)?
                .unwrap_or_default();
            sqlx::query_with(&sql, args).execute(&mut *tx).await?;
        }
    }
    // Files precede commit: interruption cannot commit a site whose files are missing.
    // A failed fresh restore may leave orphan files; a retry overwrites only validated UUID paths.
    for file in snapshot.files {
        tokio::fs::write(
            app.config.data_dir.join("media").join(file.filename),
            file.data,
        )
        .await?;
    }
    tx.commit().await?;
    tracing::info!(event = "backup_restored");
    Ok(())
}
pub fn write_private(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
