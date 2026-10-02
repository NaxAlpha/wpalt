//! Authenticated form administration and same-origin public collection.
use super::{forms::FormDefinition, store};
use crate::{
    App, auth,
    error::{Error, Result},
    view,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use maud::html;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

pub fn routes() -> Router<App> {
    Router::new()
        .route("/admin/forms", get(list).post(create))
        .route("/admin/forms/{id}", get(editor))
        .route("/admin/forms/{id}/entries", get(entries))
        .route("/admin/forms/{id}/entries/{entry}", get(entry))
        .route("/api/admin/forms/{id}", get(state).post(save))
        .route("/api/forms/{id}/entries", post(submit))
        .route("/forms/{id}", get(public))
        .route("/assets/forms.js", get(bundle))
        .layer(DefaultBodyLimit::max(128 * 1024))
}
async fn editor_session(app: &App, headers: &HeaderMap) -> Result<crate::model::Session> {
    let session = auth::session(app, headers).await?;
    if !session.can_edit() {
        return Err(Error::forbidden());
    }
    Ok(session)
}
async fn list(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let session = editor_session(&app, &headers).await?;
    let query = if app.db.postgres {
        "SELECT id,draft::jsonb->>'title' AS title,version,published_version FROM business_forms ORDER BY updated_at DESC,id DESC LIMIT 40"
    } else {
        "SELECT id,json_extract(draft,'$.title') AS title,version,published_version FROM business_forms ORDER BY updated_at DESC,id DESC LIMIT 40"
    };
    let rows = sqlx::query(query).fetch_all(&app.db.pool).await?;
    let settings = app.db.settings().await?;
    Ok(Html(view::layout(
        "Forms",
        &settings,
        Some(&session),
        html! {
            (view::heading("Business", "Forms", "Design a form, preview its working copy and publish when it is ready."))
            section class="panel" {h2 {"Create a form"} form method="post" action="/admin/forms" {
                (view::csrf(&session)) label {"Title" input name="title" required maxlength="160";} button {"Create form"}
            }}
            section class="panel" {h2 {"Recent forms"} @if rows.is_empty() {p {"Your forms will appear here."}}
                @for row in rows {
                    @let id: String = row.get("id");
                    @let form: FormDefinition = serde_json::from_str(&row.get::<String,_>("draft")).map_err(|_| Error::invalid("Stored form requires repair."))?;
                    div class="toolbar" {a href=(format!("/admin/forms/{id}")) {(form.title)} span class="status" {(if row.get::<i64,_>("published_version")>0 {"Published"}else{"Draft"})}}
                }
            }
        },
    )))
}
#[derive(Deserialize)]
struct Create {
    csrf: String,
    title: String,
}
async fn create(
    State(app): State<App>,
    headers: HeaderMap,
    axum::extract::Form(input): axum::extract::Form<Create>,
) -> Result<Redirect> {
    let session = editor_session(&app, &headers).await?;
    auth::csrf(&session, &input.csrf)?;
    let definition: FormDefinition = serde_json::from_value(json!({"title":input.title,"fields":[{"name":"message","schema":{"kind":"string","label":"Message","required":true}}]})).map_err(|_| Error::invalid("Check the form title."))?;
    let id = store::create(&app, &session.user.id, &definition).await?;
    Ok(Redirect::to(&format!("/admin/forms/{id}")))
}
async fn editor(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let session = editor_session(&app, &headers).await?;
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM business_forms WHERE id=$1")
        .bind(&id)
        .fetch_optional(&app.db.pool)
        .await?;
    exists.ok_or_else(Error::not_found)?;
    Ok(Html(view::layout(
        "Edit form",
        &app.db.settings().await?,
        Some(&session),
        html! {
            (view::heading("Business", "Edit form", "Save the working copy before publishing changes to visitors."))
            p {a href=(format!("/admin/forms/{id}/entries")) {"View responses"}}
        div id="forms-studio" data-id=(id) data-csrf=(session.csrf) {p {"Loading local form designer…"}}
            script defer src="/assets/forms.js" {}
        },
    )))
}
async fn state(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    editor_session(&app, &headers).await?;
    let row = sqlx::query("SELECT draft,version,published_version FROM business_forms WHERE id=$1")
        .bind(id)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
    Ok(Json(
        json!({"definition":serde_json::from_str::<Value>(&row.get::<String,_>("draft")).map_err(|_| Error::invalid("Stored form requires repair."))?,"version":row.get::<i64,_>("version"),"published_version":row.get::<i64,_>("published_version")}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    csrf: String,
    version: i64,
    definition: FormDefinition,
    publish: bool,
}
async fn save(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Save>,
) -> Result<Json<Value>> {
    let session = editor_session(&app, &headers).await?;
    auth::csrf(&session, &input.csrf)?;
    store::save(&app, &id, input.version, &input.definition, input.publish).await?;
    Ok(Json(
        json!({"version":input.version+1,"published":input.publish}),
    ))
}
async fn public(State(app): State<App>, Path(id): Path<String>) -> Result<Html<String>> {
    let row = sqlx::query(
        "SELECT live,published_version FROM business_forms WHERE id=$1 AND published_version>0",
    )
    .bind(&id)
    .fetch_optional(&app.db.pool)
    .await?
    .ok_or_else(Error::not_found)?;
    let raw: String = row.get("live");
    let definition: store::PublishedForm =
        serde_json::from_str(&raw).map_err(|_| Error::invalid("Stored form requires repair."))?;
    Ok(Html(view::layout(
        &definition.form.title,
        &app.db.settings().await?,
        None,
        html! {
            h1 {(definition.form.title)}
            div id="public-form" data-id=(id) data-version=(row.get::<i64,_>("published_version")) data-definition=(raw) {p {"Loading form…"}noscript {"Enable JavaScript to complete this form."}}
            script defer src="/assets/forms.js" {}
        },
    )))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Submission {
    version: i64,
    key: String,
    values: Value,
}
async fn submit(
    State(app): State<App>,
    Path(id): Path<String>,
    Json(input): Json<Submission>,
) -> Result<Json<Value>> {
    let entry = store::submit(&app, &id, input.version, &input.key, &input.values).await?;
    Ok(Json(json!({"accepted":true,"entry":entry})))
}
async fn bundle() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../../assets/generated/forms.js"),
    )
        .into_response()
}

async fn entries(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let session = editor_session(&app, &headers).await?;
    let rows = sqlx::query("SELECT id,created_at,form_version FROM form_entries WHERE form_id=$1 ORDER BY created_at DESC,id DESC LIMIT 40").bind(&id).fetch_all(&app.db.pool).await?;
    Ok(Html(view::layout(
        "Form responses",
        &app.db.settings().await?,
        Some(&session),
        html! {
            (view::heading("Business", "Form responses", "Accepted responses remain attached to the form version that collected them."))
            p {a href=(format!("/admin/forms/{id}")) {"Back to form"}}
            section class="panel" {h2 {"Recent responses"} @if rows.is_empty() {p {"No responses yet."}}
                @for row in rows {
                    @let entry: String = row.get("id");
                    p {a href=(format!("/admin/forms/{id}/entries/{entry}")) {"Response " (entry)} " · version " (row.get::<i64,_>("form_version"))}
                }
            }
        },
    )))
}
async fn entry(
    State(app): State<App>,
    headers: HeaderMap,
    Path((id, entry)): Path<(String, String)>,
) -> Result<Html<String>> {
    let session = editor_session(&app, &headers).await?;
    let row = sqlx::query("SELECT e.values_json,e.form_version,p.definition FROM form_entries e JOIN form_publications p ON p.form_id=e.form_id AND p.version=e.form_version WHERE e.form_id=$1 AND e.id=$2").bind(&id).bind(entry).fetch_optional(&app.db.pool).await?.ok_or_else(Error::not_found)?;
    let publication: store::PublishedForm =
        serde_json::from_str(&row.get::<String, _>("definition"))
            .map_err(|_| Error::invalid("Stored publication requires repair."))?;
    let values: Value = serde_json::from_str(&row.get::<String, _>("values_json"))
        .map_err(|_| Error::invalid("Stored response requires repair."))?;
    Ok(Html(view::layout(
        "Form response",
        &app.db.settings().await?,
        Some(&session),
        html! {
            (view::heading("Business", &publication.form.title, "A preserved response to the published form."))
            p {a href=(format!("/admin/forms/{id}/entries")) {"All responses"} " · version " (row.get::<i64,_>("form_version"))}
            section class="panel" {
                @for field in &publication.form.fields {
                    @if let Some(value) = values.get(&field.name) {
                        h2 {(if field.schema.label.is_empty() {field.name.as_str()}else{field.schema.label.as_str()})}
                        (entry_value(value))
                    }
                }
            }
        },
    )))
}
fn entry_value(value: &Value) -> maud::Markup {
    match value {
        Value::Object(values) => {
            html! {dl {@for (name,value) in values {dt {(name)}dd {(entry_value(value))}}}}
        }
        Value::Array(values) => html! {ol {@for value in values {li {(entry_value(value))}}}},
        Value::String(value) => html! {p class="entry-text" {(value)}},
        Value::Bool(value) => html! {p {(if *value {"Yes"}else{"No"})}},
        Value::Null => html! {p {"No response"}},
        Value::Number(value) => html! {p {(value)}},
    }
}
