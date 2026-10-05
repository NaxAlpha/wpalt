//! Native owner management: explicit grants, private one-time download and live revocation.
use super::integrations;
use crate::{
    App, auth,
    error::{Error, Result},
    model::Session,
    view,
};
use axum::{
    Form, Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use maud::html;
use serde::Deserialize;
use serde_json::Value;

pub fn routes() -> Router<App> {
    Router::new()
        .route("/admin/integrations", get(page).post(create))
        .route("/admin/integrations/revoke", post(revoke))
        .layer(DefaultBodyLimit::max(4096))
}
async fn owner(app: &App, headers: &HeaderMap) -> Result<Session> {
    let s = auth::session(app, headers).await?;
    if !s.is_admin() {
        return Err(Error::forbidden());
    }
    Ok(s)
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    csrf: String,
    user_email: String,
    name: String,
    scope: String,
    days: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revoke {
    csrf: String,
    id: String,
}
async fn render(
    app: &App,
    s: &Session,
    form: Option<&Grant>,
    error: Option<&str>,
) -> Result<String> {
    let inventory = integrations::inventory(app, s).await?;
    Ok(view::layout(
        "Integrations",
        &app.db.settings().await?,
        Some(s),
        html! {
            (view::heading("Operations","Integrations","Give independent tools explicit content access and revoke it here whenever needed."))
            a href="/admin/operations" {"Back to operations"}
            @if let Some(error)=error {p class="notice error" role="alert" {(error)}}
            section class="panel" {h2 {"Create a credential"}
                p {"Content read includes private and draft editorial content across this site. Grant it only to a trusted process. Draft access can create or edit drafts; it cannot publish or change scheduled or published records."}
                form method="post" action="/admin/integrations" {
                    (view::csrf(s))
                    label for="integration-name" {"Integration name"}
                    input id="integration-name" name="name" required maxlength="100" value=(form.map_or("",|f|f.name.as_str()));
                    label for="integration-user" {"Delegate account email"}
                    input id="integration-user" name="user_email" type="email" required maxlength="254" value=(form.map_or(s.user.email.as_str(),|f|f.user_email.as_str())) aria-describedby="integration-authority";
                    p id="integration-authority" class="muted" {"An existing editor or administrator. Changing its password or disabling the account invalidates the credential."}
                    label for="integration-scope" {"Allowed access"}
                    select id="integration-scope" name="scope" {option value="read" selected[form.is_none_or(|f|f.scope!="draft")] {"Read editorial content"}option value="draft" selected[form.is_some_and(|f|f.scope=="draft")] {"Read content and write drafts"}}
                    label for="integration-days" {"Expires after"}
                    select id="integration-days" name="days" {@for days in [1,7,30] {option value=(days) selected[form.map_or(days==7,|f|f.days==days)] {(days) " days"}}}
                    p class="muted" {"The secret downloads once and is not displayed in the list. Keep the file private. A recovered or moved site requires a new credential."}
                    button type="submit" {"Create and download credential"}
                }
            }
            section class="panel migration-review" {h2 {"Granted credentials"}
                p {a href="/admin/integrations" {"Refresh credential list"}}
                @if inventory["credentials"].as_array().is_none_or(|a|a.is_empty()) {p class="muted" {"No integration credentials. Your site works without external tools."}}
                @for credential in inventory["credentials"].as_array().into_iter().flatten() {
                    div class="list-row" {h3 {(credential["name"].as_str().unwrap_or("Integration"))}
                        p {(credential["scopes"].as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>().join(" · "))}
                        p class="muted" {"Expires: " (crate::view::timestamp(credential["expires_at"].as_i64().unwrap_or(0))) " · " (credential["id"].as_str().unwrap_or(""))}
                        form method="post" action="/admin/integrations/revoke" {(view::csrf(s))input type="hidden" name="id" value=(credential["id"].as_str().unwrap_or(""));button type="submit" class="danger" aria-label=(format!("Revoke {}",credential["name"].as_str().unwrap_or("integration"))) {"Revoke credential"}}
                    }
                }
            }
            section class="panel" {h2 {"Connect an independent process"}p {"Use the versioned content API with the downloaded credential. Integrations run separately from the CMS and retain their own resource limits. Proposed drafts use normal content validation and owner review."}p class="muted" {"Keep integration credentials private and narrowly scoped. Review proposed drafts before publication and revoke credentials when a tool no longer needs access."}}
        },
    ))
}
async fn page(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let s = owner(&app, &headers).await?;
    Ok(Html(render(&app, &s, None, None).await?))
}
async fn create(
    State(app): State<App>,
    headers: HeaderMap,
    Form(form): Form<Grant>,
) -> Result<Response> {
    let s = owner(&app, &headers).await?;
    auth::csrf(&s, &form.csrf)?;
    let result = match form.scope.as_str() {
        "read" | "draft" => {
            integrations::issue(
                &app,
                &s,
                &form.user_email,
                &form.name,
                form.scope == "draft",
                form.days,
            )
            .await
        }
        _ => Err(Error::invalid("Choose a declared content scope.")),
    };
    match result {
        Ok(issued) => Ok((
            [
                (header::CONTENT_TYPE, "text/plain; charset=utf-8".to_owned()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=wpalt-{}-token.txt", issued.id),
                ),
                (header::CACHE_CONTROL, "no-store".into()),
            ],
            issued.token,
        )
            .into_response()),
        Err(error) if error.0 == axum::http::StatusCode::UNPROCESSABLE_ENTITY => Ok((
            error.0,
            Html(render(&app, &s, Some(&form), Some(error.1)).await?),
        )
            .into_response()),
        Err(error) => Err(error),
    }
}
async fn revoke(
    State(app): State<App>,
    headers: HeaderMap,
    Form(form): Form<Revoke>,
) -> Result<Redirect> {
    let s = owner(&app, &headers).await?;
    auth::csrf(&s, &form.csrf)?;
    integrations::revoke(&app, &s, &form.id).await?;
    Ok(Redirect::to("/admin/integrations"))
}
