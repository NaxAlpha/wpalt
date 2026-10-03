//! Authenticated form administration and same-origin public collection.
use super::{forms::FormDefinition, store};
use crate::{
    App, auth,
    error::{Error, Result},
    view,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use maud::html;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

pub fn routes(app: &App) -> Router<App> {
    let engagement = if app.config.engagement.enabled {
        super::engagement_web::routes()
    } else {
        Router::new()
    };
    Router::new()
        .merge(super::audience_web::routes())
        .merge(super::workflows_web::routes())
        .merge(super::registration_web::routes())
        .merge(engagement)
        .route("/api/admin/forms/catalog", get(catalog))
        .route("/admin/forms", get(list).post(create))
        .route("/admin/forms/{id}", get(editor))
        .route("/admin/forms/{id}/entries", get(entries))
        .route("/admin/forms/{id}/export", get(export_entries))
        .route(
            "/admin/forms/{id}/entries/{entry}",
            get(entry).post(follow_up),
        )
        .route(
            "/admin/forms/{id}/entries/{entry}/attachments/{attachment}",
            get(attachment_download),
        )
        .route("/api/admin/forms/{id}", get(state).post(save))
        .route("/api/forms/{id}/entries", post(submit))
        .route(
            "/api/forms/{id}/attachments/{field}",
            post(super::attachments::upload).layer(DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        .route("/api/forms/{id}/drafts", post(draft_save))
        .route("/api/forms/{id}/drafts/{token}", get(draft_load))
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
#[derive(Deserialize, Default)]
struct FormsPage {
    after: Option<String>,
}
async fn list(
    State(app): State<App>,
    headers: HeaderMap,
    Query(page): Query<FormsPage>,
) -> Result<Html<String>> {
    let session = editor_session(&app, &headers).await?;
    let (time, id) = if let Some(after) = page.after {
        let (time, id) = after
            .split_once(':')
            .ok_or_else(|| Error::invalid("Invalid form cursor."))?;
        let time = time
            .parse::<i64>()
            .map_err(|_| Error::invalid("Invalid form cursor."))?;
        if time < 0 || uuid::Uuid::parse_str(id).is_err() {
            return Err(Error::invalid("Invalid form cursor."));
        }
        (time, id.to_owned())
    } else {
        (i64::MAX, String::new())
    };
    let query = if app.db.postgres {
        "SELECT id,draft::jsonb->>'title' AS title,published_version,updated_at FROM business_forms WHERE updated_at<$1 OR (updated_at=$1 AND id<$2) ORDER BY updated_at DESC,id DESC LIMIT 41"
    } else {
        "SELECT id,json_extract(draft,'$.title') AS title,published_version,updated_at FROM business_forms WHERE updated_at<$1 OR (updated_at=$1 AND id<$2) ORDER BY updated_at DESC,id DESC LIMIT 41"
    };
    let mut rows = sqlx::query(query)
        .bind(time)
        .bind(id)
        .fetch_all(&app.db.pool)
        .await?;
    let more = rows.len() > 40;
    if more {
        rows.pop();
    }
    let next = if more {
        rows.last().map(|r| {
            format!(
                "/admin/forms?after={}:{}",
                r.get::<i64, _>("updated_at"),
                r.get::<String, _>("id")
            )
        })
    } else {
        None
    };
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
            @if let Some(next)=next{p {a href=(next){"Older forms"}}}
            section class="panel" {h2 {"Recent forms"} @if rows.is_empty() {p {"Your forms will appear here."}}
                @for row in rows {
                    @let id: String = row.get("id");
                    div class="toolbar" {a href=(format!("/admin/forms/{id}")) {(row.get::<String,_>("title"))} span class="status" {(if row.get::<i64,_>("published_version")>0 {"Published"}else{"Draft"})}}
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
    let lists = sqlx::query("SELECT id,title,purpose,policy FROM audience_lists ORDER BY created_at DESC,id DESC LIMIT 100").fetch_all(&app.db.pool).await?.into_iter().map(|r| json!({"id":r.get::<String,_>("id"),"title":r.get::<String,_>("title"),"purpose":r.get::<String,_>("purpose"),"policy":r.get::<String,_>("policy")})).collect::<Vec<_>>();
    Ok(Json(
        json!({"lists":lists,"definition":serde_json::from_str::<Value>(&row.get::<String,_>("draft")).map_err(|_| Error::invalid("Stored form requires repair."))?,"version":row.get::<i64,_>("version"),"published_version":row.get::<i64,_>("published_version")}),
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
    if session.user.role != "admin" {
        let previous: String = sqlx::query_scalar("SELECT draft FROM business_forms WHERE id=$1")
            .bind(&id)
            .fetch_optional(&app.db.pool)
            .await?
            .ok_or_else(Error::not_found)?;
        let previous: FormDefinition = serde_json::from_str(&previous)
            .map_err(|_| Error::invalid("Stored form requires repair."))?;
        let actions =
            |form: &FormDefinition| json!([form.notifications, form.draft_post, form.registration]);
        if actions(&previous) != actions(&input.definition) {
            return Err(Error::forbidden());
        }
    }
    store::save(&app, &id, input.version, &input.definition, input.publish).await?;
    Ok(Json(
        json!({"version":input.version+1,"published":input.publish}),
    ))
}
#[derive(Deserialize, Default)]
struct Embed {
    #[serde(default)]
    embedded: bool,
}
async fn public(
    State(app): State<App>,
    Path(id): Path<String>,
    Query(embed): Query<Embed>,
) -> Result<Html<String>> {
    let row = sqlx::query(
        "SELECT live,published_version FROM business_forms WHERE id=$1 AND published_version>0",
    )
    .bind(&id)
    .fetch_optional(&app.db.pool)
    .await?
    .ok_or_else(Error::not_found)?;
    let raw: String = row.get("live");
    let mut definition: store::PublishedForm =
        serde_json::from_str(&raw).map_err(|_| Error::invalid("Stored form requires repair."))?;
    // Public rendering needs field grammar and consent purpose, never private
    // recipients, message templates or owner-configured contribution actions.
    definition.form.notifications.clear();
    definition.form.draft_post = None;
    definition.form.registration = None;
    let raw = serde_json::to_string(&definition)
        .map_err(|_| Error::invalid("Stored form requires repair."))?;
    let settings = app.db.settings().await?;
    if embed.embedded {
        return Ok(Html(html!{(maud::DOCTYPE)html lang="en"{head{meta charset="utf-8";meta name="viewport" content="width=device-width, initial-scale=1";title {(definition.form.title)}link rel="stylesheet" href="/assets/app.css";}body class=(format!("{} embedded-form",settings.theme)){main{h1 {(definition.form.title)}div id="public-form" data-id=(id) data-version=(row.get::<i64,_>("published_version")) data-definition=(raw){p{"Loading form…"}noscript{"Enable JavaScript to complete this form."}}}script defer src="/assets/forms.js"{}script defer src="/assets/form-embed.js"{}}}}.into_string()));
    }
    Ok(Html(view::layout(
        &definition.form.title,
        &settings,
        None,
        html! {
            h1 {(definition.form.title)}
            (super::engagement::markup(&settings,false))
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
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Submission>,
) -> Result<Json<Value>> {
    let entry = store::submit(&app, &id, input.version, &input.key, &input.values).await?;
    if super::engagement::conversion(
        &app,
        &headers,
        &entry,
        &format!("/forms/{id}"),
        "form_submit",
    )
    .await
    .is_err()
    {
        tracing::warn!(event = "form_conversion_not_recorded");
    }
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
    axum::extract::Query(query): axum::extract::Query<EntryQuery>,
) -> Result<Html<String>> {
    let session = editor_session(&app, &headers).await?;
    let rows = super::entries::search(&app, &id, &query.q, query.before, &query.after).await?;
    let next = if rows.len() > 40 {
        let last = &rows[39];
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("q", &query.q)
            .append_pair("before", &last.get::<i64, _>("created_at").to_string())
            .append_pair("after", &last.get::<String, _>("id"))
            .finish();
        Some(format!("/admin/forms/{id}/entries?{query}"))
    } else {
        None
    };
    Ok(Html(view::layout(
        "Form responses",
        &app.db.settings().await?,
        Some(&session),
        html! {
            (view::heading("Business", "Form responses", "Accepted responses remain attached to the form version that collected them."))
            p {a href=(format!("/admin/forms/{id}")) {"Back to form"} " · " a href=(format!("/admin/forms/{id}/export")){"Export responses (500 per page)"}}
            form method="get" class="toolbar" {label {"Search responses" input name="q" maxlength="100" value=(&query.q);} button {"Search"}}
            section class="panel" {h2 {"Responses"} @if rows.is_empty() {p {"No responses yet."}}
                @for row in rows.iter().take(40) {
                    @let entry: String = row.get("id");
                    p {a href=(format!("/admin/forms/{id}/entries/{entry}")) {"Response " (entry)} " · version " (row.get::<i64,_>("form_version"))}
                }
            }
            @if let Some(next)=next {a class="button secondary" href=(next){"Older responses"}}
        },
    )))
}
async fn entry(
    State(app): State<App>,
    headers: HeaderMap,
    Path((id, entry)): Path<(String, String)>,
) -> Result<Html<String>> {
    let session = editor_session(&app, &headers).await?;
    let row = sqlx::query("SELECT e.values_json,e.form_version,p.definition FROM form_entries e JOIN form_publications p ON p.form_id=e.form_id AND p.version=e.form_version WHERE e.form_id=$1 AND e.id=$2").bind(&id).bind(&entry).fetch_optional(&app.db.pool).await?.ok_or_else(Error::not_found)?;
    let publication: store::PublishedForm =
        serde_json::from_str(&row.get::<String, _>("definition"))
            .map_err(|_| Error::invalid("Stored publication requires repair."))?;
    let values: Value = serde_json::from_str(&row.get::<String, _>("values_json"))
        .map_err(|_| Error::invalid("Stored response requires repair."))?;
    let attachments=sqlx::query("SELECT id,original_name FROM form_attachments WHERE form_id=$1 AND entry_id=$2 ORDER BY id LIMIT 32").bind(&id).bind(&entry).fetch_all(&app.db.pool).await?;
    let workflow =
        sqlx::query("SELECT notes,assignee,version FROM form_entry_workflows WHERE entry_id=$1")
            .bind(&entry)
            .fetch_optional(&app.db.pool)
            .await?;
    let notes = workflow
        .as_ref()
        .map(|r| r.get::<String, _>("notes"))
        .unwrap_or_default();
    let assignee = workflow
        .as_ref()
        .map(|r| r.get::<String, _>("assignee"))
        .unwrap_or_default();
    let version = workflow
        .as_ref()
        .map(|r| r.get::<i64, _>("version"))
        .unwrap_or(1);
    let editors = sqlx::query(
        "SELECT id,name FROM users WHERE role IN ('admin','editor') ORDER BY name,id LIMIT 100",
    )
    .fetch_all(&app.db.pool)
    .await?;
    Ok(Html(view::layout(
        "Form response",
        &app.db.settings().await?,
        Some(&session),
        html! {
            (view::heading("Business", &publication.form.title, "A preserved response to the published form."))
            p {a href=(format!("/admin/forms/{id}/entries")) {"All responses"} " · version " (row.get::<i64,_>("form_version"))}
            section class="panel" {h2 {"Follow up"} form method="post" {(view::csrf(&session)) input type="hidden" name="version" value=(version); label {"Private notes" textarea name="notes" maxlength="8000" {(notes)}} label {"Assigned to" select name="assignee" {option value="" {"Unassigned"} @for person in editors {option value=(person.get::<String,_>("id")) selected[person.get::<String,_>("id")==assignee] {(person.get::<String,_>("name"))}}}} button {"Save follow-up"}}}
            section class="panel" {
                @for field in &publication.form.fields {
                    @if let Some(value) = values.get(&field.name) {
                        h2 {(if field.schema.label.is_empty() {field.name.as_str()}else{field.schema.label.as_str()})}
                        @if matches!(field.widget,Some(super::forms::Widget::Upload)) {
                            @if let Some(file)=attachments.iter().find(|r|Some(r.get::<String,_>("id").as_str())==value.as_str()) {a href=(format!("/admin/forms/{id}/entries/{entry}/attachments/{}",file.get::<String,_>("id"))) {"Download " (file.get::<String,_>("original_name"))}}
                        } @else {(entry_value(value))}
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

async fn draft_save(
    State(app): State<App>,
    Path(id): Path<String>,
    Json(input): Json<super::drafts::Save>,
) -> Result<Json<super::drafts::Draft>> {
    Ok(Json(super::drafts::save(&app, &id, input).await?))
}
async fn draft_load(
    State(app): State<App>,
    Path((id, token)): Path<(String, String)>,
) -> Result<Json<super::drafts::Draft>> {
    Ok(Json(super::drafts::load(&app, &id, &token).await?))
}

async fn attachment_download(
    State(app): State<App>,
    headers: HeaderMap,
    Path((form, entry, id)): Path<(String, String, String)>,
) -> Result<Response> {
    editor_session(&app, &headers).await?;
    let row=sqlx::query("SELECT filename,sha256,size FROM form_attachments WHERE id=$1 AND form_id=$2 AND entry_id=$3 AND entry_id!=''").bind(id).bind(form).bind(entry).fetch_optional(&app.db.pool).await?.ok_or_else(Error::not_found)?;
    let filename: String = row.get("filename");
    if !super::attachments::safe_filename(&filename) {
        return Err(Error::invalid("Unsafe attachment path."));
    }
    let data = tokio::fs::read(app.config.data_dir.join("attachments").join(filename)).await?;
    if data.len() as i64 != row.get::<i64, _>("size")
        || crate::auth::digest(&data) != row.get::<String, _>("sha256")
    {
        return Err(Error::invalid("Attachment integrity check failed."));
    }
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=attachment.bin",
            ),
        ],
        data,
    )
        .into_response())
}

#[derive(Deserialize)]
struct FollowUp {
    csrf: String,
    version: i64,
    notes: String,
    assignee: String,
}
async fn follow_up(
    State(app): State<App>,
    headers: HeaderMap,
    Path((form, entry)): Path<(String, String)>,
    axum::extract::Form(input): axum::extract::Form<FollowUp>,
) -> Result<Redirect> {
    let s = editor_session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    super::entries::follow_up(
        &app,
        &form,
        &entry,
        input.version,
        &input.notes,
        &input.assignee,
    )
    .await?;
    Ok(Redirect::to(&format!(
        "/admin/forms/{form}/entries/{entry}"
    )))
}
#[derive(Deserialize, Default)]
struct ExportQuery {
    #[serde(default)]
    after: String,
}
async fn export_entries(
    State(app): State<App>,
    headers: HeaderMap,
    Path(form): Path<String>,
    axum::extract::Query(query): axum::extract::Query<ExportQuery>,
) -> Result<Response> {
    editor_session(&app, &headers).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=responses.csv",
            ),
        ],
        super::entries::export(&app, &form, &query.after).await?,
    )
        .into_response())
}

#[derive(Deserialize, Default)]
struct EntryQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    before: i64,
    #[serde(default)]
    after: String,
}

async fn catalog(State(app): State<App>, headers: HeaderMap) -> Result<Json<Value>> {
    let session = auth::session(&app, &headers).await?;
    if !session.can_edit() {
        return Err(Error::forbidden());
    }
    let rows=sqlx::query("SELECT id,live FROM business_forms WHERE published_version>0 ORDER BY updated_at DESC,id LIMIT 100").fetch_all(&app.db.pool).await?;
    let forms = rows
        .into_iter()
        .map(|row| {
            let definition: store::PublishedForm =
                serde_json::from_str(&row.get::<String, _>("live"))
                    .map_err(|_| Error::invalid("Stored form needs repair."))?;
            Ok(json!({"id":row.get::<String,_>("id"),"title":definition.form.title}))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Json(json!({"forms":forms})))
}
