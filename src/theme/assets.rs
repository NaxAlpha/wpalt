//! Shared immutable local font bytes, separate from theme revision payloads.
use super::font::{self, Inspection};
use crate::{
    App,
    error::{Error, Result},
    model::Session,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

pub const MAX_ASSETS: i64 = 128;
pub const MAX_TOTAL_BYTES: i64 = 16 * 1024 * 1024;
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS theme_assets(
 id TEXT PRIMARY KEY, definition TEXT NOT NULL, size BIGINT NOT NULL CHECK(size>0 AND size<=2097152), data BYTEA NOT NULL CHECK(length(data)=size)
);
CREATE TABLE IF NOT EXISTS theme_asset_references(
 theme_id TEXT NOT NULL, version BIGINT NOT NULL CHECK(version>0), asset_id TEXT NOT NULL,
 PRIMARY KEY(theme_id,version,asset_id), FOREIGN KEY(asset_id) REFERENCES theme_assets(id)
);
CREATE INDEX IF NOT EXISTS theme_asset_usage ON theme_asset_references(asset_id,theme_id,version);
"#;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub format: u32,
    pub label: String,
    pub source: String,
    pub license: String,
    pub inspection: Inspection,
}
impl Definition {
    pub fn validate(&self) -> Result<()> {
        if self.format != 1
            || self.label.trim().is_empty()
            || self.label.len() > 100
            || self.label.chars().any(char::is_control)
            || self.source.trim().is_empty()
            || self.source.len() > 2000
            || self.source.chars().any(char::is_control)
            || self.license.trim().is_empty()
            || self.license.len() > 16384
            || self.license.contains('\0')
            || !valid_id(&self.inspection.sha256)
            || self.inspection.bytes == 0
            || self.inspection.bytes > font::MAX_FONT_BYTES
        {
            return Err(Error::invalid(
                "Local fonts need a bounded label, source and license/provenance statement.",
            ));
        }
        Ok(())
    }
    pub fn parse(raw: &str) -> Result<Self> {
        if raw.len() > 24 * 1024 {
            return Err(Error::invalid("Font metadata exceeds its review budget."));
        }
        let value: Self =
            serde_json::from_str(raw).map_err(|_| Error::invalid("Invalid font metadata."))?;
        value.validate()?;
        Ok(value)
    }
}
pub fn valid_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub async fn admit(
    app: &App,
    actor: &Session,
    label: String,
    source: String,
    license: String,
    data: Vec<u8>,
) -> Result<(String, bool)> {
    admit_checked(app, Some(actor), label, source, license, data).await
}
pub async fn admit_stopped(
    app: &App,
    label: String,
    source: String,
    license: String,
    data: Vec<u8>,
) -> Result<(String, bool)> {
    admit_checked(app, None, label, source, license, data).await
}
async fn admit_checked(
    app: &App,
    actor: Option<&Session>,
    label: String,
    source: String,
    license: String,
    data: Vec<u8>,
) -> Result<(String, bool)> {
    super::current_owner(app, actor).await?;
    if data.len() > font::MAX_FONT_BYTES {
        return Err(Error::invalid("Fonts must be at most 2 MiB."));
    }
    // Finite validation work runs away from asynchronous request executors.
    let permit = app
        .media_work
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| Error::invalid("Font work unavailable."))?;
    let (data, inspection) = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let inspected = font::inspect(&data)?;
        Ok::<_, Error>((data, inspected))
    })
    .await
    .map_err(|_| Error::invalid("Font inspection failed."))??;
    let definition = Definition {
        format: 1,
        label,
        source,
        license,
        inspection,
    };
    definition.validate()?;
    let id = definition.inspection.sha256.clone();
    let raw =
        serde_json::to_string(&definition).map_err(|_| Error::invalid("Invalid font metadata."))?;
    let _guard = app.mutation().await?;
    super::current_owner(app, actor).await?;
    let existing: Option<String> = sqlx::query_scalar("SELECT id FROM theme_assets WHERE id=$1")
        .bind(&id)
        .fetch_optional(&app.db.pool)
        .await?;
    if existing.is_some() {
        return Ok((id, false));
    }
    let quota=sqlx::query("SELECT COUNT(*) AS records,CAST(COALESCE(SUM(size),0) AS BIGINT) AS bytes FROM theme_assets").fetch_one(&app.db.pool).await?;
    if quota.get::<i64, _>("records") >= MAX_ASSETS
        || quota.get::<i64, _>("bytes") + data.len() as i64 > MAX_TOTAL_BYTES
    {
        return Err(Error::invalid(
            "Local font storage allows 128 assets and 16 MiB of shared bytes; remove unused assets first.",
        ));
    }
    sqlx::query("INSERT INTO theme_assets(id,definition,size,data) VALUES($1,$2,$3,$4)")
        .bind(&id)
        .bind(raw)
        .bind(data.len() as i64)
        .bind(data)
        .execute(&app.db.pool)
        .await?;
    tracing::info!(event="theme_asset_admitted",asset_id=%id,bytes=definition.inspection.bytes);
    Ok((id, true))
}
pub async fn metadata(app: &App, id: &str) -> Result<Definition> {
    if !valid_id(id) {
        return Err(Error::not_found());
    }
    let raw: Option<String> = sqlx::query_scalar("SELECT definition FROM theme_assets WHERE id=$1")
        .bind(id)
        .fetch_optional(&app.db.pool)
        .await?;
    Definition::parse(&raw.ok_or_else(Error::not_found)?)
}
pub async fn remove(app: &App, actor: &Session, id: &str) -> Result<()> {
    if !valid_id(id) {
        return Err(Error::not_found());
    }
    let _guard = app.mutation().await?;
    super::current_owner(app, Some(actor)).await?;
    let used: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM theme_asset_references WHERE asset_id=$1")
            .bind(id)
            .fetch_one(&app.db.pool)
            .await?;
    if used > 0 {
        return Err(Error::invalid(
            "This font is retained by a theme revision. Remove its references and retained revisions before deleting it.",
        ));
    }
    let result = sqlx::query("DELETE FROM theme_assets WHERE id=$1")
        .bind(id)
        .execute(&app.db.pool)
        .await?;
    if result.rows_affected() != 1 {
        return Err(Error::not_found());
    }
    Ok(())
}
