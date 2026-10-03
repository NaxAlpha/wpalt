//! Private attachment storage, bounded admission and single-entry capabilities.
use crate::{
    App, auth,
    error::{Error, Result},
    now,
};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, header},
};
use serde_json::{Value, json};
use sqlx::{Any, Row, Transaction};
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS form_upload_usage(form_id TEXT PRIMARY KEY REFERENCES business_forms(id),bytes BIGINT NOT NULL DEFAULT 0,files BIGINT NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS form_attachments(id TEXT PRIMARY KEY,form_id TEXT NOT NULL REFERENCES business_forms(id),field_name TEXT NOT NULL,form_version BIGINT NOT NULL,token_hash TEXT NOT NULL,filename TEXT NOT NULL UNIQUE,original_name TEXT NOT NULL,mime TEXT NOT NULL,size BIGINT NOT NULL,sha256 TEXT NOT NULL,entry_id TEXT NOT NULL DEFAULT '',expires_at BIGINT NOT NULL,created_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS attachment_entry ON form_attachments(entry_id,id);
CREATE INDEX IF NOT EXISTS attachment_expiry ON form_attachments(expires_at) WHERE entry_id='';
"#;
pub fn safe_filename(value: &str) -> bool {
    value
        .strip_suffix(".dat")
        .is_some_and(|v| uuid::Uuid::parse_str(v).is_ok())
}
pub async fn upload(
    State(app): State<App>,
    Path((form, field)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>> {
    let version = headers
        .get("x-form-version")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .ok_or_else(|| Error::invalid("Choose the published form version."))?;
    let name = headers
        .get("x-file-name")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("attachment");
    if name.len() > 200
        || name.contains(['\r', '\n', '/', '\\'])
        || body.is_empty()
        || body.len() > app.config.max_upload_bytes.min(2 * 1024 * 1024)
    {
        return Err(Error::invalid(
            "Choose a file up to the configured limit (at most 2 MiB).",
        ));
    }
    let row = sqlx::query("SELECT live FROM business_forms WHERE id=$1 AND published_version=$2")
        .bind(&form)
        .bind(version)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::conflict)?;
    let definition: super::store::PublishedForm =
        serde_json::from_str(&row.get::<String, _>("live"))
            .map_err(|_| Error::invalid("Stored publication needs repair."))?;
    if !definition
        .form
        .fields
        .iter()
        .any(|f| f.name == field && matches!(f.widget, Some(super::forms::Widget::Upload)))
    {
        return Err(Error::forbidden());
    }
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let (bytes, mime) = match mime {
        "text/plain" if std::str::from_utf8(&body).is_ok() && !body.contains(&0) => {
            (body.to_vec(), "text/plain")
        }
        "application/pdf" if body.starts_with(b"%PDF-") => (body.to_vec(), "application/pdf"),
        "image/png" | "image/jpeg" | "image/webp" => {
            let format =
                image::guess_format(&body).map_err(|_| Error::invalid("Invalid image."))?;
            if ![
                image::ImageFormat::Png,
                image::ImageFormat::Jpeg,
                image::ImageFormat::WebP,
            ]
            .contains(&format)
            {
                return Err(Error::invalid("Unsupported attachment image."));
            }
            let permit = app
                .media_work
                .clone()
                .try_acquire_owned()
                .map_err(|_| Error::invalid("Image processing is busy; retry shortly."))?;
            let bytes = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let mut reader =
                    image::ImageReader::with_format(std::io::Cursor::new(body), format);
                let mut limits = image::Limits::default();
                limits.max_image_width = Some(4096);
                limits.max_image_height = Some(4096);
                limits.max_alloc = Some(64 * 1024 * 1024);
                reader.limits(limits);
                let image = reader
                    .decode()
                    .map_err(|_| Error::invalid("Invalid or oversized image."))?;
                let mut output = std::io::Cursor::new(Vec::new());
                image
                    .write_to(&mut output, image::ImageFormat::Png)
                    .map_err(|_| Error::invalid("Cannot process image."))?;
                Ok::<_, Error>(output.into_inner())
            })
            .await
            .map_err(|_| Error::invalid("Image worker failed."))??;
            (bytes, "image/png")
        }
        _ => {
            return Err(Error::invalid(
                "Accepts UTF-8 text, PDF, PNG, JPEG and WebP only. Files are downloaded as attachments; no malware-scan guarantee.",
            ));
        }
    };
    if bytes.len() > app.config.max_upload_bytes.min(2 * 1024 * 1024) {
        return Err(Error::invalid(
            "The processed attachment exceeds the file limit.",
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let token = auth::random_token();
    let filename = format!("{id}.dat");
    let hash = auth::digest(&bytes);
    let mut tx = app.db.pool.begin().await?;
    sqlx::query(
        "INSERT INTO form_upload_usage(form_id) VALUES($1) ON CONFLICT(form_id) DO NOTHING",
    )
    .bind(&form)
    .execute(&mut *tx)
    .await?;
    if sqlx::query("UPDATE form_upload_usage SET bytes=bytes+$1,files=files+1 WHERE form_id=$2 AND bytes+$1<=67108864 AND files<1000").bind(bytes.len() as i64).bind(&form).execute(&mut *tx).await?.rows_affected()!=1 {return Err(Error::invalid("This form's private attachment storage is full (64 MiB / 1,000 files)."));}
    sqlx::query("INSERT INTO form_attachments(id,form_id,field_name,form_version,token_hash,filename,original_name,mime,size,sha256,expires_at,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(&id).bind(form).bind(field).bind(version).bind(auth::digest(token.as_bytes())).bind(&filename).bind(name).bind(mime).bind(bytes.len() as i64).bind(hash).bind(now()+86400).bind(now()).execute(&mut *tx).await?;
    let path = app.config.data_dir.join("attachments").join(filename);
    crate::backup::write_private(&path, &bytes)
        .map_err(|_| Error::invalid("Private attachment storage failed."))?;
    if let Err(error) = tx.commit().await {
        let _ = tokio::fs::remove_file(path).await;
        return Err(error.into());
    }
    Ok(Json(
        json!({"capability":format!("{id}:{token}"),"name":name}),
    ))
}
pub async fn attach(
    tx: &mut Transaction<'_, Any>,
    form: &str,
    version: i64,
    field: &str,
    capability: &str,
    entry: &str,
) -> Result<String> {
    let (id, token) = capability
        .split_once(':')
        .ok_or_else(|| Error::invalid("Invalid attachment capability."))?;
    if uuid::Uuid::parse_str(id).is_err()
        || token.len() != 64
        || !token.bytes().all(|v| v.is_ascii_hexdigit())
    {
        return Err(Error::invalid("Invalid attachment capability."));
    }
    if sqlx::query("UPDATE form_attachments SET entry_id=$1 WHERE id=$2 AND form_id=$3 AND form_version=$4 AND field_name=$5 AND token_hash=$6 AND entry_id='' AND expires_at>$7").bind(entry).bind(id).bind(form).bind(version).bind(field).bind(auth::digest(token.as_bytes())).bind(now()).execute(&mut **tx).await?.rows_affected()!=1{return Err(Error::invalid("Attachment is expired, already used or belongs to another form or field."));}
    Ok(id.into())
}
