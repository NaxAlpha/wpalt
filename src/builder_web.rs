use crate::{
    App, auth, content,
    error::{Error, Result},
    schema, theme, view,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use maud::html;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
pub fn routes() -> Router<App> {
    Router::new()
        .route("/admin/builder", get(page))
        .route("/assets/builder.js", get(bundle))
        .route("/assets/widgets.js", get(widgets))
        .route("/api/admin/design", get(state))
        .route("/api/admin/design/{id}", post(save))
        .route("/api/admin/design/{id}/activate", post(activate))
        .route("/api/admin/design/{id}/restore/{version}", post(restore))
        .route("/api/admin/schema", post(common))
        .route("/api/admin/models/{id}", post(model))
        .route("/api/admin/options", post(options))
        .route("/admin/design/{id}/preview", get(preview))
        .route("/admin/design/{id}/style.css", get(draft_css))
        .route("/themes/{id}/{version}/style.css", get(live_css))
        .layer(DefaultBodyLimit::max(320 * 1024))
        .merge(crate::theme::asset_web::routes())
}
async fn owner(
    app: &App,
    headers: &HeaderMap,
    csrf: Option<&str>,
) -> Result<crate::model::Session> {
    let s = auth::session(app, headers).await?;
    if !s.is_admin() {
        return Err(Error::forbidden());
    }
    if let Some(token) = csrf {
        auth::csrf(&s, token)?
    }
    Ok(s)
}
async fn page(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let s = owner(&app, &headers, None).await?;
    let settings = app.db.settings().await?;
    Ok(Html(view::layout(
        "Design studio",
        &settings,
        Some(&s),
        html! {(view::heading("Design studio","Design studio","Compose templates and reusable components. Preview drafts before publishing."))div id="builder" data-csrf=(s.csrf){p{"Loading local studio…"}}script defer src="/assets/builder.js"{}},
    )))
}
async fn bundle() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../assets/generated/builder.js"),
    )
        .into_response()
}
#[derive(Default, Deserialize)]
struct DesignQuery {
    theme: Option<String>,
}
async fn state(
    State(app): State<App>,
    headers: HeaderMap,
    Query(query): Query<DesignQuery>,
) -> Result<Json<Value>> {
    let s = auth::session(&app, &headers).await?;
    if !s.can_edit() {
        return Err(Error::forbidden());
    }
    let registry = schema::Registry::load(&app).await?;
    let models = sqlx::query("SELECT id,version FROM content_models ORDER BY id")
        .fetch_all(&app.db.pool)
        .await?;
    let versions: std::collections::BTreeMap<String, i64> = models
        .iter()
        .map(|r| (r.get("id"), r.get("version")))
        .collect();
    let posts =
        sqlx::query("SELECT id,title,kind,status FROM posts ORDER BY updated_at DESC LIMIT 128")
            .fetch_all(&app.db.pool)
            .await?;
    let media = sqlx::query(
        "SELECT id,alt,original_name,visibility FROM media ORDER BY created_at DESC LIMIT 128",
    )
    .fetch_all(&app.db.pool)
    .await?;
    let mut out = json!({"registry":registry,"model_versions":versions,"posts":posts.iter().map(|r|json!({"id":r.get::<String,_>("id"),"title":r.get::<String,_>("title"),"kind":r.get::<String,_>("kind"),"status":r.get::<String,_>("status")})).collect::<Vec<_>>(),"media":media.iter().map(|r|json!({"id":r.get::<String,_>("id"),"label":r.get::<String,_>("original_name"),"alt":r.get::<String,_>("alt"),"visibility":r.get::<String,_>("visibility")})).collect::<Vec<_>>()});
    if app.config.business_enabled {
        let query = if app.db.postgres {
            "SELECT id,live::jsonb->'form'->>'title' AS title FROM business_forms WHERE published_version>0 ORDER BY updated_at DESC,id DESC LIMIT 100"
        } else {
            "SELECT id,json_extract(live,'$.form.title') AS title FROM business_forms WHERE published_version>0 ORDER BY updated_at DESC,id DESC LIMIT 100"
        };
        out["forms"] = json!(
            sqlx::query(query)
                .fetch_all(&app.db.pool)
                .await?
                .iter()
                .map(|r| json!({"id":r.get::<String,_>("id"),"title":r.get::<String,_>("title")}))
                .collect::<Vec<_>>()
        );
    }
    if s.is_admin() {
        let themes =
            sqlx::query("SELECT id,name,version,published_version FROM themes ORDER BY name")
                .fetch_all(&app.db.pool)
                .await?;
        let active = app.db.settings().await?.theme;
        let selected = query.theme.as_deref().unwrap_or(&active);
        let raw: String = sqlx::query_scalar("SELECT draft FROM themes WHERE id=$1")
            .bind(selected)
            .fetch_optional(&app.db.pool)
            .await?
            .ok_or_else(Error::not_found)?;
        let selected_package: Value =
            serde_json::from_str(&raw).map_err(|_| Error::invalid("Invalid stored package."))?;
        let revisions = sqlx::query(
            "SELECT theme_id,version,published FROM theme_revisions ORDER BY version DESC",
        )
        .fetch_all(&app.db.pool)
        .await?;
        let design=sqlx::query("SELECT version,published_version,draft_options,live_options FROM site_design WHERE id=1").fetch_one(&app.db.pool).await?;
        out["themes"]=json!(themes.iter().map(|r|json!({"id":r.get::<String,_>("id"),"name":r.get::<String,_>("name"),"version":r.get::<i64,_>("version"),"published_version":r.get::<i64,_>("published_version"),"package":if r.get::<String,_>("id")==selected{selected_package.clone()}else{Value::Null}})).collect::<Vec<_>>());
        out["revisions"]=json!(revisions.iter().map(|r|json!({"theme":r.get::<String,_>("theme_id"),"version":r.get::<i64,_>("version"),"published":r.get::<i64,_>("published")})).collect::<Vec<_>>());
        out["design"] = json!({"version":design.get::<i64,_>("version"),"published_version":design.get::<i64,_>("published_version"),"draft_options":serde_json::from_str::<Value>(&design.get::<String,_>("draft_options")).unwrap_or(Value::Null)});
        out["active"] = json!(active);
        let fonts = sqlx::query("SELECT id,definition FROM theme_assets ORDER BY id LIMIT 129")
            .fetch_all(&app.db.pool)
            .await?;
        if fonts.len() > 128 {
            return Err(Error::invalid(
                "Local font inventory exceeds its supported budget.",
            ));
        }
        let mut inventory = Vec::new();
        for row in fonts {
            let id: String = row.get("id");
            let metadata =
                crate::theme::assets::Definition::parse(&row.get::<String, _>("definition"))?;
            inventory
                .push(json!({"id":id,"label":metadata.label,"inspection":metadata.inspection}));
        }
        out["fonts"] = inventory.into();
        out["languages"] = serde_json::to_value(crate::discovery::load(&app).await?.0.languages)
            .map_err(|_| Error::invalid("Invalid configured languages."))?;
    }
    Ok(Json(out))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Edit {
    csrf: String,
    version: i64,
    package: theme::Package,
    #[serde(default)]
    publish: bool,
}
async fn save(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Edit>,
) -> Result<Json<Value>> {
    let actor = owner(&app, &headers, Some(&input.csrf)).await?;
    let v = theme::save_as(
        &app,
        &actor,
        &id,
        input.package,
        input.version,
        input.publish,
    )
    .await?;
    Ok(Json(json!({"version":v})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Token {
    csrf: String,
}
async fn activate(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Token>,
) -> Result<Json<Value>> {
    let actor = owner(&app, &headers, Some(&input.csrf)).await?;
    theme::activate_as(&app, &actor, &id).await?;
    Ok(Json(json!({"active":id})))
}
async fn restore(
    State(app): State<App>,
    headers: HeaderMap,
    Path((id, version)): Path<(String, i64)>,
    Json(input): Json<Versioned>,
) -> Result<Json<Value>> {
    let actor = owner(&app, &headers, Some(&input.csrf)).await?;
    let raw: String =
        sqlx::query_scalar("SELECT package FROM theme_revisions WHERE theme_id=$1 AND version=$2")
            .bind(&id)
            .bind(version)
            .fetch_optional(&app.db.pool)
            .await?
            .ok_or_else(Error::not_found)?;
    let package = theme::Package::parse(&raw, &schema::Registry::load(&app).await?)?;
    Ok(Json(
        json!({"version":theme::save_as(&app,&actor,&id,package,input.version,false).await?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Versioned {
    csrf: String,
    version: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SchemaEdit {
    csrf: String,
    version: i64,
    definition: schema::Definition,
}
async fn common(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<SchemaEdit>,
) -> Result<Json<Value>> {
    owner(&app, &headers, Some(&input.csrf)).await?;
    let v = schema::save_common(&app, input.definition, input.version).await?;
    Ok(Json(json!({"version":v})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelEdit {
    csrf: String,
    version: i64,
    definition: schema::Model,
}
async fn model(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<ModelEdit>,
) -> Result<Json<Value>> {
    owner(&app, &headers, Some(&input.csrf)).await?;
    let v = schema::save_model(&app, &id, input.definition, input.version).await?;
    Ok(Json(json!({"version":v})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OptionsEdit {
    csrf: String,
    version: i64,
    values: Value,
    #[serde(default)]
    publish: bool,
}
async fn options(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<OptionsEdit>,
) -> Result<Json<Value>> {
    owner(&app, &headers, Some(&input.csrf)).await?;
    let v = schema::save_options(&app, input.values, input.version, input.publish).await?;
    Ok(Json(json!({"version":v})))
}
#[derive(Default, Deserialize)]
struct PreviewQuery {
    #[serde(default)]
    post: String,
    #[serde(default = "home")]
    template: String,
}
fn home() -> String {
    "home".into()
}
async fn preview(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<PreviewQuery>,
) -> Result<Html<String>> {
    let s = auth::session(&app, &headers).await?;
    if !s.can_edit() {
        return Err(Error::forbidden());
    }
    let stored = theme::load(&app, &id, true).await?;
    let settings = app.db.settings().await?;
    let post = if q.post.is_empty() {
        None
    } else {
        Some(content::get(&app, &q.post).await?)
    };
    let listing=sqlx::query("SELECT * FROM posts WHERE status='published' ORDER BY published_at DESC,id DESC LIMIT 20").fetch_all(&app.db.pool).await?.into_iter().map(|r|{let p=crate::model::Post::from_row(r);json!({"id":p.id,"kind":p.kind,"title":p.published_title,"url":format!("/{}",p.published_slug),"body":crate::view::excerpt(&p.published_body.chars().take(220).collect::<String>()),"fields":serde_json::from_str::<Value>(&p.published_fields).unwrap_or(json!({}))})}).collect();
    let mut ctx = theme::context(
        &app,
        &settings,
        &stored.package,
        post.as_ref(),
        listing,
        true,
        &q.template,
        None,
    )
    .await?;
    ctx.root["navigation"] = serde_json::from_str(&settings.navigation).unwrap_or(json!([]));
    Ok(Html(theme::document(
        &stored,
        &settings,
        &ctx,
        post.as_ref(),
        true,
        &q.template,
        html! {},
    )?))
}
#[derive(Deserialize)]
struct StyleQuery {
    v: Option<i64>,
    template: Option<String>,
}
async fn draft_css(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<StyleQuery>,
) -> Result<Response> {
    let session = auth::session(&app, &headers).await?;
    if !session.can_edit() {
        return Err(Error::forbidden());
    }
    let package = if let Some(version) = query.v {
        let raw: String = sqlx::query_scalar(
            "SELECT package FROM theme_revisions WHERE theme_id=$1 AND version=$2",
        )
        .bind(&id)
        .bind(version)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
        theme::Package::parse_historical(&raw, &schema::Registry::load(&app).await?)?
    } else {
        theme::load(&app, &id, true).await?.package
    };
    Ok((
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        if let Some(template) = query.template {
            package.css_for(&template)?
        } else {
            package.css()
        },
    )
        .into_response())
}
async fn live_css(
    State(app): State<App>,
    Path((id, version)): Path<(String, i64)>,
    Query(query): Query<StyleQuery>,
) -> Result<Response> {
    let raw: String = sqlx::query_scalar(
        "SELECT package FROM theme_revisions WHERE theme_id=$1 AND version=$2 AND published=1",
    )
    .bind(id)
    .bind(version)
    .fetch_optional(&app.db.pool)
    .await?
    .ok_or_else(Error::not_found)?;
    let package = theme::Package::parse_historical(&raw, &schema::Registry::load(&app).await?)?;
    // CSS was validated before publication; do not expose working revisions.
    Ok((
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        if let Some(template) = query.template {
            package.css_for(&template)?
        } else {
            package.css()
        },
    )
        .into_response())
}

async fn widgets() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../assets/widgets.js"),
    )
        .into_response()
}
