//! Owner reports and explicit visitor privacy choices.
use super::engagement;
use crate::{
    App, auth,
    error::{Error, Result},
    view,
};
use axum::{
    Json, Router,
    extract::{Form, Path, State},
    http::{HeaderMap, HeaderValue, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use maud::html;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
pub fn routes() -> Router<App> {
    Router::new()
        .merge(super::promotions_web::routes())
        .route(
            "/admin/engagement/catalog",
            get(catalog_page).post(catalog_save),
        )
        .route("/admin/engagement", get(reports).post(settings))
        .route("/api/engagement/status", get(status))
        .route("/api/engagement/consent", post(consent))
        .route("/api/engagement/events", post(capture))
        .route("/assets/engagement.js", get(bundle))
        .route("/admin/engagement/sessions/{hash}", get(playback))
        .route("/api/admin/engagement/sessions/{hash}", get(frames))
        .route("/assets/engagement-review.js", get(review_bundle))
        .layer(axum::extract::DefaultBodyLimit::max(32 * 1024))
}
async fn owner(app: &App, headers: &HeaderMap) -> Result<crate::model::Session> {
    let s = auth::session(app, headers).await?;
    if !s.is_admin() {
        return Err(Error::forbidden());
    }
    Ok(s)
}
async fn status(State(app): State<App>, headers: HeaderMap) -> Result<Json<Value>> {
    Ok(Json(engagement::status(&app, &headers).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Consent {
    allow: bool,
    #[serde(default)]
    recording: bool,
    policy: i64,
}
async fn consent(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<Consent>,
) -> Result<Response> {
    let value =
        engagement::consent(&app, &headers, input.allow, input.recording, input.policy).await?;
    let mut response = Json(json!({"consented":value.is_some()})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&engagement::cookie(
            &app,
            value.as_deref().unwrap_or(""),
            if value.is_some() {
                app.config.engagement.retention_days * 86400
            } else {
                0
            },
        ))
        .map_err(|_| Error::invalid("Invalid privacy cookie."))?,
    );
    Ok(response)
}
async fn capture(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<engagement::Capture>,
) -> Result<Json<Value>> {
    Ok(Json(
        json!({"recorded":engagement::capture(&app,&headers,input).await?}),
    ))
}
async fn bundle(State(app): State<App>) -> Result<Response> {
    if app.db.settings().await?.analytics.is_none() {
        return Err(Error::not_found());
    }
    Ok((
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../../assets/engagement.js"),
    )
        .into_response())
}
async fn review_bundle(State(app): State<App>, headers: HeaderMap) -> Result<Response> {
    owner(&app, &headers).await?;
    Ok((
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../../assets/engagement-review.js"),
    )
        .into_response())
}
async fn reports(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let s = owner(&app, &headers).await?;
    let settings =
        sqlx::query("SELECT enabled,recording,purpose,version FROM engagement_settings WHERE id=1")
            .fetch_one(&app.db.pool)
            .await?;
    let cutoff = crate::now() - app.config.engagement.retention_days * 86400;
    let totals=sqlx::query("SELECT name,path,COUNT(*) AS total FROM engagement_events WHERE created_at>=$1 GROUP BY name,path ORDER BY total DESC,name,path LIMIT 50").bind(cutoff).fetch_all(&app.db.pool).await?;
    let funnels=sqlx::query("SELECT COUNT(*) AS visitors,COALESCE(SUM(CASE WHEN submitted IS NOT NULL AND viewed<=submitted THEN 1 ELSE 0 END),0) AS submissions,COALESCE(SUM(CASE WHEN claimed IS NOT NULL AND submitted<=claimed AND viewed<=submitted THEN 1 ELSE 0 END),0) AS offers FROM (SELECT session_hash,MIN(CASE WHEN name='pageview' THEN created_at END) AS viewed,MIN(CASE WHEN name='form_submit' THEN created_at END) AS submitted,MIN(CASE WHEN name='offer_claim' THEN created_at END) AS claimed FROM engagement_events WHERE created_at>=$1 GROUP BY session_hash) funnel WHERE viewed IS NOT NULL").bind(cutoff).fetch_one(&app.db.pool).await?;
    let category_sql = if app.db.postgres {
        "SELECT d.key AS category,d.value AS value,COUNT(*) AS total FROM engagement_events e CROSS JOIN LATERAL json_each_text(e.dimensions::json) d WHERE e.created_at>=$1 GROUP BY d.key,d.value ORDER BY total DESC,d.key,d.value LIMIT 64"
    } else {
        "SELECT d.key AS category,CAST(d.value AS TEXT) AS value,COUNT(*) AS total FROM engagement_events e,json_each(e.dimensions) d WHERE e.created_at>=$1 GROUP BY d.key,d.value ORDER BY total DESC,d.key,d.value LIMIT 64"
    };
    let categories = sqlx::query(category_sql)
        .bind(cutoff)
        .fetch_all(&app.db.pool)
        .await?;
    let sessions=sqlx::query("SELECT session_hash,COUNT(*) AS frames,MAX(created_at) AS recent FROM engagement_events WHERE frame<>'' AND created_at>=$1 GROUP BY session_hash ORDER BY recent DESC,session_hash DESC LIMIT 40").bind(cutoff).fetch_all(&app.db.pool).await?;
    Ok(Html(view::layout(
        "Engagement",
        &app.db.settings().await?,
        Some(&s),
        html! {
        (view::heading("Business","Engagement","Local events, consented interactions and transparent privacy choices.")) p {a href="/admin/engagement/catalog" {"Event and category catalog"} " · " a href="/admin/promotions" {"Local offers and experiments"}}
        section class="panel" {h2 {"Collection controls"} form method="post" {(view::csrf(&s)) input type="hidden" name="version" value=(settings.get::<i64,_>("version"));label {input type="checkbox" name="enabled" value="true" checked[settings.get::<i64,_>("enabled")==1];"Enable consented analytics"}label {input type="checkbox" name="recording" value="true" checked[settings.get::<i64,_>("recording")==1];"Offer optional masked interaction recording"}label {"Analytics purpose" textarea name="purpose" required maxlength="1000" {(settings.get::<String,_>("purpose"))}}button {"Save privacy settings"}p {"Changing the purpose/settings version requires renewed consent. Browser Global Privacy Control is honored; configured Do Not Track is also honored."}}}
        section class="panel" {h2 {"Conversion funnel"} p {"Page view → accepted form → offer claim"} div class="field-row" {p {strong {(funnels.get::<i64,_>("visitors"))}" visitor sessions"}p {strong {(funnels.get::<i64,_>("submissions"))}" submitted"}p {strong {(funnels.get::<i64,_>("offers"))}" claimed"}}p {"Counts describe consented cookie sessions within the configured retention window, not identified people. Sequence has one-second resolution."}}
        section class="panel" {h2 {"Recent page and event totals"} @for row in totals {div class="field-row" {p {(row.get::<String,_>("name"))}p {(row.get::<String,_>("path"))}p {(row.get::<i64,_>("total"))}}}}
        section class="panel" {h2 {"Declared category totals"}@for row in categories {div class="field-row"{p {(row.get::<String,_>("category")) " · " (row.get::<String,_>("value"))}p {(row.get::<i64,_>("total"))}}}}
        section class="panel" {h2 {"Masked interaction playback"} p {"Recorded geometry contains no page text, field values, links or credentials. It is a wireframe, not a DOM replay."} @for row in sessions {p {a href=(format!("/admin/engagement/sessions/{}",row.get::<String,_>("session_hash"))){"View " (row.get::<i64,_>("frames")) " recorded frames"} " · " (row.get::<i64,_>("recent"))}}}
        },
    )))
}
#[derive(Deserialize)]
struct Settings {
    csrf: String,
    version: i64,
    purpose: String,
    #[serde(default)]
    enabled: String,
    #[serde(default)]
    recording: String,
}
async fn settings(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<Settings>,
) -> Result<Redirect> {
    let s = owner(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    if input.purpose.trim().is_empty() || input.purpose.len() > 1000 {
        return Err(Error::invalid(
            "Use a clear analytics purpose up to 1,000 bytes.",
        ));
    }
    if sqlx::query("UPDATE engagement_settings SET purpose=$1,enabled=$2,recording=$3,version=version+1 WHERE id=1 AND version=$4").bind(input.purpose.trim()).bind(i64::from(input.enabled=="true")).bind(i64::from(input.recording=="true")).bind(input.version).execute(&app.db.pool).await?.rows_affected()!=1{return Err(Error::conflict());}
    Ok(Redirect::to("/admin/engagement"))
}
async fn playback(
    State(app): State<App>,
    headers: HeaderMap,
    Path(hash): Path<String>,
) -> Result<Html<String>> {
    let s = owner(&app, &headers).await?;
    Ok(Html(view::layout(
        "Masked playback",
        &app.db.settings().await?,
        Some(&s),
        html! {(view::heading("Engagement","Masked playback","Geometry-only frames; values and text are never recorded."))p {a href="/admin/engagement" {"Engagement reports"}}div id="engagement-playback" data-session=(hash) {p {"Loading masked frames…"}}script defer src="/assets/engagement-review.js" {}},
    )))
}
async fn frames(
    State(app): State<App>,
    headers: HeaderMap,
    Path(hash): Path<String>,
) -> Result<Json<Value>> {
    owner(&app, &headers).await?;
    if hash.len() != 64 || !hash.bytes().all(|v| v.is_ascii_hexdigit()) {
        return Err(Error::not_found());
    }
    let rows=sqlx::query("SELECT path,frame,created_at FROM engagement_events WHERE session_hash=$1 AND frame<>'' ORDER BY created_at,id LIMIT 60").bind(hash).fetch_all(&app.db.pool).await?;
    let frames=rows.into_iter().map(|row|Ok(json!({"path":row.get::<String,_>("path"),"created_at":row.get::<i64,_>("created_at"),"geometry":serde_json::from_str::<Value>(&row.get::<String,_>("frame")).map_err(|_|Error::invalid("Stored frame needs repair."))?}))).collect::<Result<Vec<_>>>()?;
    Ok(Json(json!({"frames":frames})))
}

async fn catalog_page(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let s = owner(&app, &headers).await?;
    let events =
        sqlx::query("SELECT name,label FROM engagement_event_names ORDER BY name LIMIT 64")
            .fetch_all(&app.db.pool)
            .await?;
    let dimensions = sqlx::query(
        "SELECT name,value FROM engagement_dimension_values ORDER BY name,value LIMIT 512",
    )
    .fetch_all(&app.db.pool)
    .await?;
    Ok(Html(view::layout(
        "Analytics declarations",
        &app.db.settings().await?,
        Some(&s),
        html! {
        (view::heading("Engagement","Events and categories","Declare names and bounded categories. No free text or private inputs enter capture."))
        p {a href="/admin/engagement" {"Reports"}} section class="panel" {h2 {"Declare an event"} form method="post" {(view::csrf(&s)) input type="hidden" name="kind" value="event";input type="hidden" name="values" value="";label {"Event identifier" input name="name" required maxlength="64" pattern="[a-z][a-z0-9_]*";}label {"Event label" input name="label" required maxlength="160";}button {"Declare event"}}}
        section class="panel" {h2 {"Declare a category"} form method="post" {(view::csrf(&s))input type="hidden" name="kind" value="dimension";input type="hidden" name="label" value="";label {"Category identifier" input name="name" required maxlength="64" pattern="[a-z][a-z0-9_]*";}label {"Allowed values (comma separated)" input name="values" required maxlength="2100";}p {"Use category identifiers such as plan: free, pro. Avoid personal identifiers. New declarations require renewed analytics consent."}button {"Declare category"}}}
        section class="panel" {h2 {"Registered events"}@for event in events {p {code {(event.get::<String,_>("name"))} " · " (event.get::<String,_>("label"))}}}
        section class="panel" {h2 {"Declared category values"}@for row in dimensions {p {code {(row.get::<String,_>("name"))} " · " (row.get::<String,_>("value"))}}}
        },
    )))
}
#[derive(Deserialize)]
struct Catalog {
    csrf: String,
    kind: String,
    name: String,
    label: String,
    values: String,
}
async fn catalog_save(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<Catalog>,
) -> Result<Redirect> {
    let s = owner(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    engagement::catalog(&app, &input.kind, &input.name, &input.label, &input.values).await?;
    Ok(Redirect::to("/admin/engagement/catalog"))
}
