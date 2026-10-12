//! Connected editorial observations and bounded durable public analysis.
pub mod epoch;
pub mod jobs;
pub mod jobs_web;
pub mod text;
pub mod web;

use crate::{
    App,
    error::{Error, Result},
    model::Session,
};
use serde::Serialize;
use sqlx::Row;

#[derive(Serialize)]
pub struct Analysis {
    pub copy: String,
    pub path: String,
    pub content_version: i64,
    pub source_epoch: i64,
    pub html_sha256: String,
    pub html_bytes: usize,
    pub text: text::PageText,
    pub observations: text::Observation,
}
async fn working_versions(app: &App, id: &str) -> Result<(i64, i64, i64)> {
    let row = sqlx::query("SELECT p.version AS content_version,t.version AS theme_version,d.version AS design_version FROM posts p CROSS JOIN settings s JOIN themes t ON t.id=s.theme CROSS JOIN site_design d WHERE p.id=$1 AND s.id=1 AND d.id=1").bind(id).fetch_optional(&app.db.pool).await?.ok_or_else(Error::not_found)?;
    Ok((
        row.get("content_version"),
        row.get("theme_version"),
        row.get("design_version"),
    ))
}
pub async fn analyze(
    app: &App,
    session: &Session,
    id: &str,
    copy: &str,
    focus: &str,
) -> Result<Analysis> {
    if !session.can_edit() || session.hash.starts_with("integration:") {
        return Err(Error::forbidden());
    }
    text::focus(focus)?;
    if !["working", "public"].contains(&copy) {
        return Err(Error::invalid(
            "Choose saved working copy or public publication.",
        ));
    }
    let _work = app.audit_work.lock().await;
    let _read = app.mutations.lock().await;
    crate::auth::current_editor(app, session).await?;
    let source_epoch = epoch::current(app).await?;
    let versions = if copy == "working" {
        Some(working_versions(app, id).await?)
    } else {
        None
    };
    // The public choice never loads private working body/fields/documents.
    let p =
        sqlx::query("SELECT version,status,published_locale,published_slug FROM posts WHERE id=$1")
            .bind(id)
            .fetch_optional(&app.db.pool)
            .await?
            .ok_or_else(Error::not_found)?;
    let (discovery, _) = crate::discovery::load(app).await?;
    let (path, html) = if copy == "working" {
        (
            format!("/admin/preview/{id}"),
            crate::web::render_working_source(app, id).await?,
        )
    } else {
        if p.get::<String, _>("status") != "published" {
            return Err(Error::not_found());
        }
        crate::membership::require(app, "post", id, None).await?;
        (
            discovery.path(
                &p.get::<String, _>("published_locale"),
                &p.get::<String, _>("published_slug"),
            ),
            crate::web::render_anonymous_source(
                app,
                p.get("published_locale"),
                Some(p.get("published_slug")),
            )
            .await?,
        )
    };
    let html_sha256 = crate::auth::digest(html.as_bytes());
    let html_bytes = html.len();
    let page = tokio::task::spawn_blocking(move || text::parse(&html))
        .await
        .map_err(|_| Error::invalid("Editorial text worker interrupted."))??;
    if source_epoch != epoch::current(app).await?
        || (versions.is_some() && versions != Some(working_versions(app, id).await?))
    {
        return Err(Error::conflict());
    }
    crate::auth::current_editor(app, session).await?;
    let observations = page.observe(focus)?;
    tracing::info!(event = "editorial_analysis", copy, html_bytes, source_epoch);
    Ok(Analysis {
        copy: copy.into(),
        path,
        content_version: p.get("version"),
        source_epoch,
        html_sha256,
        html_bytes,
        text: page,
        observations,
    })
}
