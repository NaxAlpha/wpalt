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
        .route("/registration/{token}", get(review).post(verify))
        .route("/admin/registrations", get(index))
        .route("/admin/registrations/{id}", axum::routing::post(decide))
}
async fn review(State(app): State<App>, Path(token): Path<String>) -> Result<Html<String>> {
    let name = super::registration::review(&app, &token).await?;
    Ok(Html(view::layout(
        "Verify account request",
        &app.db.settings().await?,
        None,
        html! {(view::heading("Account request","Verify and choose a password",&format!("Hello {name}. The owner must approve before you can sign in.")))form method="post" {label {"New password" input type="password" name="password" required minlength="12" maxlength="256" autocomplete="new-password";}button {"Verify account request"}}},
    )))
}
#[derive(Deserialize)]
struct Verify {
    password: String,
}
async fn verify(
    State(app): State<App>,
    Path(token): Path<String>,
    Form(i): Form<Verify>,
) -> Result<Html<String>> {
    super::registration::verify(&app, &token, &i.password).await?;
    Ok(Html(view::layout(
        "Awaiting approval",
        &app.db.settings().await?,
        None,
        html! {h1 {"Mailbox verified"}p {"Your request is awaiting the site owner’s approval. You can sign in after approval."}a href="/login" {"Sign in"}},
    )))
}
async fn index(State(app): State<App>, h: HeaderMap) -> Result<Html<String>> {
    let s = auth::session(&app, &h).await?;
    if !s.is_admin() {
        return Err(Error::forbidden());
    }
    let rows=sqlx::query("SELECT id,email,name,state,version FROM registration_requests ORDER BY created_at DESC,id LIMIT 100").fetch_all(&app.db.pool).await?;
    Ok(Html(view::layout(
        "Account requests",
        &app.db.settings().await?,
        Some(&s),
        html! {(view::heading("Forms","Account requests","Verify the mailbox first, then deliberately approve a subscriber account with no editorial permissions."))@for row in rows {section class="panel" {h2 {(row.get::<String,_>("name"))}p {(row.get::<String,_>("email")) " · " (row.get::<String,_>("state"))}@if row.get::<String,_>("state")=="verified"{form method="post" action=(format!("/admin/registrations/{}",row.get::<String,_>("id"))){(view::csrf(&s))input type="hidden" name="version" value=(row.get::<i64,_>("version"));button name="action" value="approve" {"Approve subscriber"}button name="action" value="reject" {"Reject"}}}}}},
    )))
}
#[derive(Deserialize)]
struct Decide {
    csrf: String,
    version: i64,
    action: String,
}
async fn decide(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(i): Form<Decide>,
) -> Result<Redirect> {
    let s = auth::session(&app, &h).await?;
    if !s.is_admin() {
        return Err(Error::forbidden());
    }
    auth::csrf(&s, &i.csrf)?;
    if !["approve", "reject"].contains(&i.action.as_str()) {
        return Err(Error::invalid("Choose approve or reject."));
    }
    super::registration::decide(&app, &id, i.version, i.action == "approve").await?;
    Ok(Redirect::to("/admin/registrations"))
}
