//! Native owner font library and visibility-aware immutable local byte delivery.
use super::{assets, font};
use crate::{
    App, auth,
    error::{Error, Result},
    view,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Form, Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use maud::html;
use serde::Deserialize;
use sqlx::Row;
use std::collections::BTreeMap;
pub fn routes() -> Router<App> {
    Router::new()
        .route("/admin/design-assets", get(page).post(upload))
        .route("/admin/design-assets/{id}/remove", post(remove))
        .route("/theme-assets/{id}", get(bytes))
        .layer(DefaultBodyLimit::max(font::MAX_FONT_BYTES + 32 * 1024))
        .merge(
            Router::new()
                .route(
                    "/api/admin/design/{id}/bundle",
                    get(export_bundle).post(import_bundle),
                )
                .layer(DefaultBodyLimit::max(super::bundle::MAX_BYTES + 32768)),
        )
}
async fn owner(app: &App, headers: &HeaderMap) -> Result<crate::model::Session> {
    let actor = auth::session(app, headers).await?;
    if !actor.is_admin() || actor.hash.starts_with("integration:") {
        return Err(Error::forbidden());
    }
    Ok(actor)
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    #[serde(default)]
    after: String,
}
async fn page(
    State(app): State<App>,
    headers: HeaderMap,
    Query(query): Query<Page>,
) -> Result<Html<String>> {
    let actor = owner(&app, &headers).await?;
    if !query.after.is_empty() && !assets::valid_id(&query.after) {
        return Err(Error::invalid("Invalid font cursor."));
    }
    let rows =
        sqlx::query("SELECT id,definition,size FROM theme_assets WHERE id>$1 ORDER BY id LIMIT 41")
            .bind(query.after)
            .fetch_all(&app.db.pool)
            .await?;
    let mut entries = Vec::new();
    for row in rows.iter().take(40) {
        entries.push((
            row.get::<String, _>("id"),
            assets::Definition::parse(&row.get::<String, _>("definition"))?,
            row.get::<i64, _>("size"),
        ));
    }
    Ok(Html(view::layout(
        "Local fonts",
        &app.db.settings().await?,
        Some(&actor),
        html! {
            (view::heading("Design assets","Local fonts","Upload a font you have permission to distribute, then select it in a theme draft. Publication controls visitor access."))
            p {a href="/admin/builder" {"Return to Design studio"}}
            form method="post" action="/admin/design-assets" enctype="multipart/form-data" {
                (view::csrf(&actor))
                label {"Font label" input name="label" maxlength="100" required;}
                label {"Static TrueType font" input type="file" name="font" accept=".ttf,font/ttf" required;}
                p class="muted" {"Static TrueType only, up to 2 MiB. Compressed, variable, color and other font containers are not admitted yet."}
                label {"Source / provenance" input name="source" maxlength="2000" required;}
                label {"License / distribution permission" textarea name="license" maxlength="16384" required {}}
                label {input type="checkbox" name="rights" value="yes" required; " I have permission to use and distribute this font and its stated license."}
                button {"Upload local font"}
            }
            @for (id,metadata,size) in &entries {
                section class="field-row" {div {h2 {(metadata.label)}p {(format!("{} bytes · weight {} · {}",size,metadata.inspection.weight,if metadata.inspection.italic {"italic"} else {"normal"}))}
                    details {summary {"Provenance and license"}p {(metadata.source)}pre class="font-license" tabindex="0" aria-label="Font license" {(metadata.license)}code class="font-digest" {(id)}}}
                    form method="post" action=(format!("/admin/design-assets/{id}/remove")) {(view::csrf(&actor))button class="secondary" {"Remove unused font"}}
                }
            }
            @if rows.len()>40 {a href=(format!("/admin/design-assets?after={}",entries.last().unwrap().0)) {"Next fonts"}}
        },
    )))
}
async fn upload(
    State(app): State<App>,
    headers: HeaderMap,
    mut parts: Multipart,
) -> Result<Redirect> {
    let actor = owner(&app, &headers).await?;
    let mut fields = BTreeMap::new();
    let mut data = None;
    while let Some(field) = parts
        .next_field()
        .await
        .map_err(|_| Error::invalid("Invalid font upload."))?
    {
        let name = field
            .name()
            .ok_or_else(|| Error::invalid("Unnamed font field."))?
            .to_owned();
        if name == "font" {
            if data.is_some() {
                return Err(Error::invalid("Upload one font at a time."));
            }
            let value = field
                .bytes()
                .await
                .map_err(|_| Error::invalid("Invalid font bytes."))?;
            if value.len() > font::MAX_FONT_BYTES {
                return Err(Error::invalid("Font exceeds 2 MiB."));
            }
            data = Some(value.to_vec());
        } else {
            if !["csrf", "label", "source", "license", "rights"].contains(&name.as_str())
                || fields.contains_key(&name)
            {
                return Err(Error::invalid("Unknown or repeated font metadata field."));
            }
            let value = field
                .text()
                .await
                .map_err(|_| Error::invalid("Invalid font metadata text."))?;
            if value.len() > 16384 {
                return Err(Error::invalid("Font metadata exceeds its budget."));
            }
            fields.insert(name, value);
        }
    }
    auth::csrf(&actor, fields.get("csrf").map(String::as_str).unwrap_or(""))?;
    if fields.get("rights").map(String::as_str) != Some("yes") {
        return Err(Error::invalid(
            "Confirm font distribution permission before admission.",
        ));
    }
    let take = |name: &str| {
        fields
            .get(name)
            .cloned()
            .ok_or_else(|| Error::invalid("Complete font metadata before admission."))
    };
    assets::admit(
        &app,
        &actor,
        take("label")?,
        take("source")?,
        take("license")?,
        data.ok_or_else(|| Error::invalid("Choose a font file."))?,
    )
    .await?;
    Ok(Redirect::to("/admin/design-assets"))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Remove {
    csrf: String,
}
async fn remove(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(input): Form<Remove>,
) -> Result<Redirect> {
    let actor = owner(&app, &headers).await?;
    auth::csrf(&actor, &input.csrf)?;
    assets::remove(&app, &actor, &id).await?;
    Ok(Redirect::to("/admin/design-assets"))
}
async fn bytes(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response> {
    if !assets::valid_id(&id) {
        return Err(Error::not_found());
    }
    // Public fonts do not require a session lookup. Conditional requests check
    // live visibility and bounded metadata without loading the shared blob.
    let published: Option<i64> = sqlx::query_scalar("SELECT a.size FROM theme_assets a WHERE a.id=$1 AND EXISTS (SELECT 1 FROM theme_asset_references r JOIN themes t ON t.id=r.theme_id AND t.published_version=r.version WHERE r.asset_id=a.id)")
        .bind(&id).fetch_optional(&app.db.pool).await?;
    let preview = published.is_none();
    let size = if let Some(size) = published {
        size
    } else {
        let actor = auth::session(&app, &headers)
            .await
            .map_err(|_| Error::not_found())?;
        if !actor.is_admin() || actor.hash.starts_with("integration:") {
            return Err(Error::not_found());
        }
        sqlx::query_scalar::<_, i64>("SELECT size FROM theme_assets WHERE id=$1")
            .bind(&id)
            .fetch_optional(&app.db.pool)
            .await?
            .ok_or_else(Error::not_found)?
    };
    if size <= 0 || size > font::MAX_FONT_BYTES as i64 {
        return Err(Error::invalid("Stored local font size exceeds its budget."));
    }
    let etag = format!("\"{id}\"");
    let unchanged = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        == Some(&etag);
    let data = if unchanged {
        Vec::new()
    } else {
        let _permit = app
            .media_reads
            .acquire()
            .await
            .map_err(|_| Error::invalid("Font reads unavailable."))?;
        let data: Vec<u8> = sqlx::query_scalar("SELECT data FROM theme_assets WHERE id=$1")
            .bind(&id)
            .fetch_optional(&app.db.pool)
            .await?
            .ok_or_else(Error::not_found)?;
        if data.len() as i64 != size || auth::digest(&data) != id {
            return Err(Error::invalid("Stored local font integrity failed."));
        }
        data
    };
    let mut response = if unchanged {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        data.into_response()
    };
    let values = response.headers_mut();
    values.insert(header::CONTENT_TYPE, "font/ttf".parse().unwrap());
    values.insert(header::ETAG, etag.parse().unwrap());
    values.insert(header::VARY, "Cookie".parse().unwrap());
    values.insert(
        header::CACHE_CONTROL,
        if preview {
            "private, no-store"
        } else {
            "public, max-age=31536000, immutable"
        }
        .parse()
        .unwrap(),
    );
    values.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    Ok(response)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleQuery {
    #[serde(default)]
    draft: bool,
}
async fn export_bundle(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<BundleQuery>,
) -> Result<Response> {
    owner(&app, &headers).await?;
    if !crate::schema::identifier(&id) {
        return Err(Error::not_found());
    }
    let bundle = super::bundle::export(&app, &id, query.draft).await?;
    let mut response = Json(bundle).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "private, no-store".parse().unwrap());
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{id}.wpalt-theme.json\"")
            .parse()
            .map_err(|_| Error::invalid("Invalid export filename."))?,
    );
    Ok(response)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleImport {
    csrf: String,
    version: i64,
    #[serde(default)]
    publish: bool,
    rights: bool,
    bundle: super::bundle::Bundle,
}
async fn import_bundle(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<BundleImport>,
) -> Result<Json<serde_json::Value>> {
    let actor = owner(&app, &headers).await?;
    auth::csrf(&actor, &input.csrf)?;
    if !input.rights {
        return Err(Error::invalid(
            "Review the bundled font licenses and confirm distribution permission.",
        ));
    }
    let bytes = serde_json::to_vec(&input.bundle).map_err(|_| Error::invalid("Invalid bundle."))?;
    let version =
        super::bundle::import(&app, Some(&actor), &id, bytes, input.version, input.publish).await?;
    Ok(Json(serde_json::json!({"version":version})))
}
