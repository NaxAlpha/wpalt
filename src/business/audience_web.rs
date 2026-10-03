//! Local audience administration and capability-scoped confirmation screens.
use super::audience;
use crate::{
    App, auth,
    error::{Error, Result},
    view,
};
use axum::{
    Router,
    extract::{Form, Path, State},
    http::HeaderMap,
    response::{Html, Redirect},
    routing::get,
};
use maud::html;
use serde::Deserialize;
use sqlx::Row;
pub fn routes() -> Router<App> {
    Router::new()
        .route(
            "/admin/audience/contacts/{id}",
            get(contact).post(contact_save),
        )
        .route("/admin/audience/contacts/{id}/export", get(contact_export))
        .route("/admin/mail/{id}", get(mail_record).post(retry_mail))
        .route("/admin/mail/{id}/download", get(mail_download))
        .route("/admin/campaigns", get(campaigns).post(campaign_create))
        .route(
            "/admin/campaigns/{id}",
            get(campaign_editor).post(campaign_save),
        )
        .route("/admin/audience", get(index).post(create))
        .route("/audience/confirm/{token}", get(confirm_page).post(confirm))
        .route(
            "/audience/withdraw/{token}",
            get(withdraw_page).post(withdraw),
        )
        .route("/admin/mail", get(outbox))
}
async fn session(app: &App, headers: &HeaderMap) -> Result<crate::model::Session> {
    let s = auth::session(app, headers).await?;
    if !s.can_edit() {
        return Err(Error::forbidden());
    }
    Ok(s)
}
async fn index(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let s = session(&app, &headers).await?;
    let lists=sqlx::query("SELECT id,title,purpose,policy FROM audience_lists ORDER BY created_at DESC,id DESC LIMIT 40").fetch_all(&app.db.pool).await?;
    let contacts=sqlx::query("SELECT c.id,c.email,m.state,l.title FROM audience_memberships m JOIN audience_contacts c ON c.id=m.contact_id JOIN audience_lists l ON l.id=m.list_id ORDER BY m.created_at DESC,m.contact_id DESC LIMIT 40").fetch_all(&app.db.pool).await?;
    Ok(Html(view::layout(
        "Audience",
        &app.db.settings().await?,
        Some(&s),
        html! {
        (view::heading("Business","Audience","Purpose-specific lists. Subscription requests remain pending until the recipient confirms."))
        p {a href="/admin/forms" {"Forms"} " · " a href="/admin/mail" {"Mail outbox"} " · " a href="/admin/campaigns" {"Campaigns"}}
        section class="panel" {h2 {"Create a list"} form method="post" {(view::csrf(&s)) label {"Title" input name="title" required maxlength="160";} label {"Subscription purpose" textarea name="purpose" required maxlength="1000" {}} label {"Policy version" input name="policy" required maxlength="64" value="1";} button {"Create list"}}}
        section class="panel" {h2 {"Lists"} @for row in lists {div class="field-row" {h3 {(row.get::<String,_>("title"))} p {(row.get::<String,_>("purpose"))} p {"Policy " (row.get::<String,_>("policy")) " · Identifier " code {(row.get::<String,_>("id"))}}}}
        section class="panel" {h2 {"Recent subscriptions"} @for row in contacts {div class="toolbar" {a href=(format!("/admin/audience/contacts/{}",row.get::<String,_>("id"))) {(row.get::<String,_>("email"))} span {(row.get::<String,_>("title"))} span class="status" {(row.get::<String,_>("state"))}}}}
        }},
    )))
}
#[derive(Deserialize)]
struct Create {
    csrf: String,
    title: String,
    purpose: String,
    policy: String,
}
async fn create(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<Create>,
) -> Result<Redirect> {
    let s = session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    audience::create_list(&app, &input.title, &input.purpose, &input.policy).await?;
    Ok(Redirect::to("/admin/audience"))
}
async fn page(app: &App, token: &str, withdraw: bool) -> Result<Html<String>> {
    let (title, purpose) = audience::review(app, token, withdraw).await?;
    let action = if withdraw {
        "Withdraw subscription"
    } else {
        "Confirm subscription"
    };
    Ok(Html(view::layout(
        action,
        &app.db.settings().await?,
        None,
        html! {(view::heading("Your choices",action,&title)) p {(purpose)} p {"Opening this page does not change your subscription. Use the button to record your decision."} form method="post" {button {(action)}}},
    )))
}
async fn confirm_page(State(app): State<App>, Path(token): Path<String>) -> Result<Html<String>> {
    page(&app, &token, false).await
}
async fn withdraw_page(State(app): State<App>, Path(token): Path<String>) -> Result<Html<String>> {
    page(&app, &token, true).await
}
async fn decision(app: &App, token: &str, withdraw: bool) -> Result<Html<String>> {
    audience::decide(app, token, withdraw).await?;
    Ok(Html(view::layout(
        "Subscription updated",
        &app.db.settings().await?,
        None,
        html! {h1 {"Subscription updated"} p {(if withdraw{"Your subscription has been withdrawn. Queued campaign messages have been cancelled."}else{"Your subscription is confirmed."})}},
    )))
}
async fn confirm(State(app): State<App>, Path(token): Path<String>) -> Result<Html<String>> {
    decision(&app, &token, false).await
}
async fn withdraw(State(app): State<App>, Path(token): Path<String>) -> Result<Html<String>> {
    decision(&app, &token, true).await
}
async fn outbox(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let s = session(&app, &headers).await?;
    let rows=sqlx::query("SELECT id,recipient,subject,state,attempts FROM mail_jobs ORDER BY created_at DESC,id DESC LIMIT 40").fetch_all(&app.db.pool).await?;
    Ok(Html(view::layout(
        "Mail outbox",
        &app.db.settings().await?,
        Some(&s),
        html! {(view::heading("Business","Mail outbox","Durable delivery status. Local .eml files are the default; a spool receipt does not mean inbox delivery.")) p {a href="/admin/audience" {"Audience"}} @for row in rows {div class="field-row" {h2 {a href=(format!("/admin/mail/{}",row.get::<String,_>("id"))) {(row.get::<String,_>("subject"))}} p {(row.get::<String,_>("recipient"))} p {(row.get::<String,_>("state")) " · Attempts " (row.get::<i64,_>("attempts"))}}}},
    )))
}

async fn campaigns(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let s = session(&app, &headers).await?;
    let lists = sqlx::query(
        "SELECT id,title FROM audience_lists ORDER BY created_at DESC,id DESC LIMIT 100",
    )
    .fetch_all(&app.db.pool)
    .await?;
    let rows = sqlx::query(
        "SELECT id,title,state FROM business_campaigns ORDER BY created_at DESC,id DESC LIMIT 40",
    )
    .fetch_all(&app.db.pool)
    .await?;
    Ok(Html(view::layout(
        "Campaigns",
        &app.db.settings().await?,
        Some(&s),
        html! {
        (view::heading("Business","Campaigns","Compose once, then deliver in bounded batches to confirmed subscribers."))
        section class="panel" {h2 {"Create campaign"} form method="post" {(view::csrf(&s)) label {"Title" input name="title" required maxlength="160";} label {"Audience list" select name="list" required {@for row in lists {option value=(row.get::<String,_>("id")) {(row.get::<String,_>("title"))}}}} button {"Create campaign"}}}
        section class="panel" {h2 {"Recent campaigns"} @for row in rows {div class="toolbar" {a href=(format!("/admin/campaigns/{}",row.get::<String,_>("id"))) {(row.get::<String,_>("title"))} span class="status" {(row.get::<String,_>("state"))}}}}
        },
    )))
}
#[derive(Deserialize)]
struct CampaignCreate {
    csrf: String,
    title: String,
    list: String,
}
async fn campaign_create(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<CampaignCreate>,
) -> Result<Redirect> {
    let s = session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    let id = super::campaigns::create(&app, &input.title, &input.list).await?;
    Ok(Redirect::to(&format!("/admin/campaigns/{id}")))
}
async fn campaign_editor(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let s = session(&app, &headers).await?;
    let row = sqlx::query("SELECT * FROM business_campaigns WHERE id=$1")
        .bind(&id)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
    let segment: audience::Segment = serde_json::from_str(&row.get::<String, _>("segment"))
        .map_err(|_| Error::invalid("Stored campaign segment needs repair."))?;
    let editable = ["draft", "scheduled"].contains(&row.get::<String, _>("state").as_str());
    Ok(Html(view::layout(
        "Compose campaign",
        &app.db.settings().await?,
        Some(&s),
        html! {
        (view::heading("Business","Compose campaign",&row.get::<String,_>("title")))
        p {a href="/admin/campaigns" {"Campaigns"} " · " a href="/admin/mail" {"Delivery status"}}
        @if editable {form method="post" action=(format!("/admin/campaigns/{id}")) data-editor data-owner=(&s.user.id) {
        (view::csrf(&s)) input type="hidden" name="version" value=(row.get::<i64,_>("version")); input type="hidden" name="locale" value="en";
        section class="panel" {label {"Subject" input name="subject" value=(row.get::<String,_>("subject")) maxlength="200" required;}
        input type="hidden" name="document" value=(row.get::<String,_>("document")); div data-writing-canvas hidden {}
        label {input type="checkbox" name="triggered" value="true" checked[row.get::<String,_>("trigger_kind")=="confirmation"];"Send automatically when a subscriber confirms this list"}p {"Triggered messages use the declared segment and current consent. Keep the schedule empty for a reusable confirmation template."}
        label {"Content" textarea name="body" class="editor-body" {}}
        label data-markdown-replacement {input type="checkbox" name="import_markdown" value="true"; "Replace content using Markdown"}
        input type="hidden" name="send_at" value=(row.get::<i64,_>("send_at")); label {"Schedule delivery" input type="datetime-local" data-epoch-for="send_at";small {"Uses your browser’s local time. Leave empty to keep a draft, or choose Send now."}}
        fieldset {legend {"Recipient segment (all conditions must match)"} label {"Company equals" input name="company" maxlength="160" value=(segment.company.unwrap_or_default());} label {"Source equals" input name="source" maxlength="160" value=(segment.source.unwrap_or_default());} label {"Minimum contact score" input name="minimum_score" type="number" step="any" value=(segment.minimum_score.map(|v|v.to_string()).unwrap_or_default());}}
        div class="toolbar" {button name="action" value="save" {"Save campaign"} button name="action" value="send" {"Send now"}}}
        } script defer src="/assets/editor.js" {}}
        @else {p class="status" {(row.get::<String,_>("state"))} (maud::PreEscaped(crate::document::Document::parse(&row.get::<String,_>("document"))?.html()))}
        },
    )))
}
#[derive(Deserialize)]
struct CampaignSave {
    #[serde(default)]
    body: String,
    #[serde(default)]
    import_markdown: String,
    #[serde(default)]
    triggered: String,
    csrf: String,
    version: i64,
    subject: String,
    document: String,
    send_at: i64,
    action: String,
    #[serde(default)]
    company: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    minimum_score: String,
}
async fn campaign_save(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(input): Form<CampaignSave>,
) -> Result<Redirect> {
    let s = session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    let document = if input.import_markdown == "true" {
        crate::document::import(&input.body, "[]")?.encode()
    } else {
        input.document
    };
    super::campaigns::save_with_trigger(
        &app,
        &id,
        input.version,
        &input.subject,
        &document,
        if input.action == "send" {
            crate::now()
        } else {
            input.send_at
        },
        audience::Segment {
            company: (!input.company.is_empty()).then_some(input.company),
            source: (!input.source.is_empty()).then_some(input.source),
            minimum_score: if input.minimum_score.is_empty() {
                None
            } else {
                Some(
                    input
                        .minimum_score
                        .parse()
                        .map_err(|_| Error::invalid("Use a numeric minimum score."))?,
                )
            },
        },
        Some(input.triggered == "true"),
    )
    .await?;
    Ok(Redirect::to(&format!("/admin/campaigns/{id}")))
}

async fn mail_record(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let s = session(&app, &headers).await?;
    let row =
        sqlx::query("SELECT recipient,subject,state,attempts,last_code FROM mail_jobs WHERE id=$1")
            .bind(&id)
            .fetch_optional(&app.db.pool)
            .await?
            .ok_or_else(Error::not_found)?;
    let attempts=sqlx::query("SELECT outcome,created_at FROM mail_attempts WHERE job_id=$1 ORDER BY created_at DESC,id DESC LIMIT 40").bind(&id).fetch_all(&app.db.pool).await?;
    Ok(Html(view::layout(
        "Message delivery",
        &app.db.settings().await?,
        Some(&s),
        html! {
        (view::heading("Business","Message delivery",&row.get::<String,_>("subject"))) p {(row.get::<String,_>("recipient"))} p class="status" {(row.get::<String,_>("state")) " · " (row.get::<String,_>("last_code"))}
        p {a href=(format!("/admin/mail/{id}/download")){"Download private .eml message"}}
        section class="panel" {h2 {"Manual recovery"} p {"An uncertain or previously completed delivery may already have reached the recipient. A deliberate resend can produce a duplicate. Campaign consent is checked again before delivery."} form method="post" {(view::csrf(&s)) label {input type="checkbox" name="duplicates" value="true"; "I accept that this retry may deliver a duplicate"} button {"Queue deliberate retry"}}}
        section class="panel" {h2 {"Recent delivery attempts"} @for attempt in attempts {p {(attempt.get::<String,_>("outcome")) " · " (attempt.get::<i64,_>("created_at"))}}}
        },
    )))
}
#[derive(Deserialize)]
struct RetryMail {
    csrf: String,
    #[serde(default)]
    duplicates: String,
}
async fn retry_mail(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(input): Form<RetryMail>,
) -> Result<Redirect> {
    let s = session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    let query = if input.duplicates == "true" {
        "UPDATE mail_jobs SET state='pending',next_at=$1,attempts=0,last_code='manual_retry' WHERE id=$2 AND state IN ('dead','retry','uncertain','sent','spooled')"
    } else {
        "UPDATE mail_jobs SET state='pending',next_at=$1,attempts=0,last_code='manual_retry' WHERE id=$2 AND state IN ('dead','retry')"
    };
    if sqlx::query(query)
        .bind(crate::now())
        .bind(&id)
        .execute(&app.db.pool)
        .await?
        .rows_affected()
        != 1
    {
        return Err(Error::invalid(
            "A completed or uncertain message requires explicit duplicate-delivery acknowledgment. Cancelled or active messages cannot be retried here.",
        ));
    }
    Ok(Redirect::to(&format!("/admin/mail/{id}")))
}
async fn mail_download(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<axum::response::Response> {
    use axum::response::IntoResponse;
    session(&app, &headers).await?;
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "message/rfc822"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=message.eml",
            ),
        ],
        super::mail::download(&app, &id).await?,
    )
        .into_response())
}

async fn contact(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let s = session(&app, &headers).await?;
    let row = sqlx::query(
        "SELECT email,name,attributes,suppressed,version FROM audience_contacts WHERE id=$1",
    )
    .bind(&id)
    .fetch_optional(&app.db.pool)
    .await?
    .ok_or_else(Error::not_found)?;
    let attrs: audience::Attributes = serde_json::from_str(&row.get::<String, _>("attributes"))
        .map_err(|_| Error::invalid("Stored contact attributes need repair."))?;
    let history=sqlx::query("SELECT action,policy,purpose,created_at FROM audience_consent_events WHERE contact_id=$1 ORDER BY created_at DESC,id DESC LIMIT 100").bind(&id).fetch_all(&app.db.pool).await?;
    Ok(Html(view::layout(
        "Contact",
        &app.db.settings().await?,
        Some(&s),
        html! {
        (view::heading("Business","Contact",&row.get::<String,_>("email"))) p {a href="/admin/audience" {"Audience"} " · " a href=(format!("/admin/audience/contacts/{id}/export")){"Export this contact"}}
        section class="panel" {h2 {"Contact details"} form method="post" {(view::csrf(&s)) input type="hidden" name="version" value=(row.get::<i64,_>("version")); label {"Name" input name="name" maxlength="100" value=(row.get::<String,_>("name"));} label {"Company" input name="company" maxlength="160" value=(attrs.company);} label {"Source" input name="source" maxlength="160" value=(attrs.source);} label {"Contact score" input name="score" type="number" step="any" min="-1000000" max="1000000" value=(attrs.score);} label {input type="checkbox" name="suppressed" value="true" checked[row.get::<i64,_>("suppressed")!=0]; "Suppress all campaign delivery"} button name="action" value="save" {"Save contact"}}}
        section class="panel" {h2 {"Recorded consent"} @for event in history {div class="field-row" {p {(event.get::<String,_>("action")) " · " (event.get::<String,_>("policy"))} p {(event.get::<String,_>("purpose"))} p {(event.get::<i64,_>("created_at"))}}}}
        section class="panel" {h2 {"Delete audience data"} p {"Deletes this contact, list memberships, consent records and associated mail/outbox data. A keyed address digest prevents anonymous re-subscription. Form responses and independent historical backups are separate records; manage their retention separately."} form method="post" {(view::csrf(&s)) input type="hidden" name="version" value=(row.get::<i64,_>("version")); input type="hidden" name="name" value="";input type="hidden" name="company" value="";input type="hidden" name="source" value="";input type="hidden" name="score" value="0";label {input type="checkbox" name="delete_confirm" value="true" required; "Delete this contact and its audience history"} button name="action" value="delete" class="danger" {"Delete audience data"}}}
        },
    )))
}
#[derive(Deserialize)]
struct ContactSave {
    csrf: String,
    version: i64,
    name: String,
    company: String,
    source: String,
    score: f64,
    #[serde(default)]
    suppressed: String,
    action: String,
    #[serde(default)]
    delete_confirm: String,
}
async fn contact_save(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(input): Form<ContactSave>,
) -> Result<Redirect> {
    let s = session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    if input.action == "delete" {
        if input.delete_confirm != "true" {
            return Err(Error::invalid("Confirm deletion of audience data."));
        }
        audience::delete_contact(&app, &id).await?;
        return Ok(Redirect::to("/admin/audience"));
    }
    audience::update_contact(
        &app,
        &id,
        input.version,
        &input.name,
        audience::Attributes {
            company: input.company,
            source: input.source,
            score: input.score,
        },
        input.suppressed == "true",
    )
    .await?;
    Ok(Redirect::to(&format!("/admin/audience/contacts/{id}")))
}
async fn contact_export(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<axum::response::Response> {
    use axum::response::IntoResponse;
    session(&app, &headers).await?;
    let row = sqlx::query(
        "SELECT email,name,attributes,suppressed,created_at FROM audience_contacts WHERE id=$1",
    )
    .bind(&id)
    .fetch_optional(&app.db.pool)
    .await?
    .ok_or_else(Error::not_found)?;
    let events=sqlx::query("SELECT action,policy,purpose,created_at FROM audience_consent_events WHERE contact_id=$1 ORDER BY created_at DESC,id DESC LIMIT 500").bind(&id).fetch_all(&app.db.pool).await?;
    let value = serde_json::json!({"email":row.get::<String,_>("email"),"name":row.get::<String,_>("name"),"attributes":serde_json::from_str::<serde_json::Value>(&row.get::<String,_>("attributes")).map_err(|_|Error::invalid("Stored attributes need repair."))?,"suppressed":row.get::<i64,_>("suppressed")!=0,"created_at":row.get::<i64,_>("created_at"),"consent_events":events.into_iter().map(|r|serde_json::json!({"action":r.get::<String,_>("action"),"policy":r.get::<String,_>("policy"),"purpose":r.get::<String,_>("purpose"),"created_at":r.get::<i64,_>("created_at")})).collect::<Vec<_>>()});
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "application/json"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=contact.json",
            ),
        ],
        axum::Json(value),
    )
        .into_response())
}
