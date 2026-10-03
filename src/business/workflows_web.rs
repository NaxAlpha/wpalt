//! Manage frozen form routes with the shared message editor.
use super::{
    forms::{Condition, FormDefinition},
    store,
    workflows::{DraftPost, Notification},
};
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
    Router::new().route("/admin/forms/{id}/workflows", get(editor).post(save))
}
async fn owner(app: &App, h: &HeaderMap) -> Result<crate::model::Session> {
    let s = auth::session(app, h).await?;
    if !s.is_admin() {
        return Err(Error::forbidden());
    }
    Ok(s)
}
async fn editor(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let s = owner(&app, &h).await?;
    let row = sqlx::query("SELECT draft,version FROM business_forms WHERE id=$1")
        .bind(&id)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
    let f: FormDefinition = serde_json::from_str(&row.get::<String, _>("draft"))
        .map_err(|_| Error::invalid("Stored form needs repair."))?;
    Ok(Html(view::layout(
        "Form workflows",
        &app.db.settings().await?,
        Some(&s),
        html! {
        (view::heading("Forms","Workflows",&f.title))p {a href=(format!("/admin/forms/{id}")){"Form designer"}}p {"Changes save to the working copy. Publish the form to activate the frozen rules. Recipient addresses are owner-configured; visitors cannot choose notification destinations."}
        section class="panel" {h2 {"Current routes"}@for rule in &f.notifications {p {(rule.recipient) " · " (rule.subject)}}
        form method="post" data-editor data-owner=(&s.user.id){(view::csrf(&s))input type="hidden" name="version" value=(row.get::<i64,_>("version"));input type="hidden" name="locale" value="en";input type="hidden" name="action" value="add";
        label {"Recipient" input name="recipient" type="email" required maxlength="254";}label {"Subject" input name="subject" required maxlength="200";}
        label {"Condition field (empty means every accepted response)" select name="condition_field" {option value="" {"Every response"}@for field in &f.fields{option value=(&field.name){(&field.name)}}}}label {"Condition" select name="operation" {option value="present" {"Has a value"}option value="equal" {"Equals"}}}label {"Comparison value (JSON scalar)" input name="comparison" value="true" maxlength="8000";}
        input type="hidden" name="document" value=(crate::document::empty());div data-writing-canvas hidden {}label {"Notification message" textarea name="body" class="editor-body" {}}label data-markdown-replacement {input type="checkbox" name="import_markdown" value="true";"Replace using Markdown"}button {"Add conditional route"}}
        form method="post" {(view::csrf(&s))input type="hidden" name="version" value=(row.get::<i64,_>("version"));input type="hidden" name="action" value="clear";button {"Remove all notification routes"}}}
        section class="panel" {h2 {"Moderated contributions"}p {"Accepted values create a private draft owned by the form owner. No visitor can publish, choose a role or supply a post identifier."}form method="post" {(view::csrf(&s))input type="hidden" name="version" value=(row.get::<i64,_>("version"));input type="hidden" name="action" value="post";
        @for(name,label)in [("title_field","Title field"),("body_field","Body field")]{label {(label)select name=(name){option value="" {"Disable draft creation"}@for field in f.fields.iter().filter(|f|f.schema.kind=="string"){option value=(&field.name) selected[f.draft_post.as_ref().is_some_and(|a|if name=="title_field"{a.title_field==field.name}else{a.body_field==field.name})]{(&field.name)}}}}}button {"Save contribution mapping"}}}
        section class="panel" {h2 {"Account requests"}p {a href="/admin/registrations" {"Review verified requests"}}p {"The applicant proves mailbox ownership and chooses a password using a private email link. You must approve a subscriber account; it has no administration privileges."}form method="post" {(view::csrf(&s))input type="hidden" name="version" value=(row.get::<i64,_>("version"));input type="hidden" name="action" value="registration";@for(name,label)in [("email_field","Email field"),("name_field","Name field")]{label {(label)select name=(name){option value="" {"Disable account requests"}@for field in f.fields.iter().filter(|f|f.schema.kind=="string"){option value=(&field.name) selected[f.registration.as_ref().is_some_and(|a|if name=="email_field"{a.email_field==field.name}else{a.name_field==field.name})]{(&field.name)}}}}}button {"Save account request mapping"}}}
        script defer src="/assets/editor.js" {}
        },
    )))
}
#[derive(Deserialize)]
struct Save {
    csrf: String,
    version: i64,
    action: String,
    #[serde(default)]
    recipient: String,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    condition_field: String,
    #[serde(default)]
    operation: String,
    #[serde(default)]
    comparison: String,
    #[serde(default)]
    document: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    import_markdown: String,
    #[serde(default)]
    title_field: String,
    #[serde(default)]
    body_field: String,
    #[serde(default)]
    email_field: String,
    #[serde(default)]
    name_field: String,
}
async fn save(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(i): Form<Save>,
) -> Result<Redirect> {
    let s = owner(&app, &h).await?;
    auth::csrf(&s, &i.csrf)?;
    let raw: String = sqlx::query_scalar("SELECT draft FROM business_forms WHERE id=$1")
        .bind(&id)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
    let mut f: FormDefinition =
        serde_json::from_str(&raw).map_err(|_| Error::invalid("Stored form needs repair."))?;
    match i.action.as_str() {
        "registration" => {
            f.registration = if i.email_field.is_empty() || i.name_field.is_empty() {
                None
            } else {
                Some(super::registration::Action {
                    email_field: i.email_field,
                    name_field: i.name_field,
                })
            }
        }
        "clear" => f.notifications.clear(),
        "post" => {
            f.draft_post = if i.title_field.is_empty() || i.body_field.is_empty() {
                None
            } else {
                Some(DraftPost {
                    title_field: i.title_field,
                    body_field: i.body_field,
                })
            }
        }
        "add" => {
            let when = if i.condition_field.is_empty() {
                None
            } else {
                Some(match i.operation.as_str() {
                    "present" => Condition::Present {
                        field: i.condition_field,
                    },
                    "equal" => Condition::Equal {
                        field: i.condition_field,
                        value: serde_json::from_str(&i.comparison).map_err(|_| {
                            Error::invalid(
                                "Use a JSON scalar comparison such as true or a quoted string.",
                            )
                        })?,
                    },
                    _ => return Err(Error::invalid("Unknown condition.")),
                })
            };
            let document = if i.import_markdown == "true" {
                crate::document::import(&i.body, "[]")?.encode()
            } else {
                i.document
            };
            f.notifications.push(Notification {
                recipient: i.recipient,
                subject: i.subject,
                document,
                when,
            });
        }
        _ => return Err(Error::invalid("Unknown workflow action.")),
    };
    store::save(&app, &id, i.version, &f, false).await?;
    Ok(Redirect::to(&format!("/admin/forms/{id}/workflows")))
}
