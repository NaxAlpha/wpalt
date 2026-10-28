//! Capability visitor controls and current-editor partial recovery workspace.
use super::recovery::{self, Policy};
use crate::{
    App, auth,
    error::{Error, Result},
    view,
};
use axum::{
    Json, Router,
    extract::{Form, Path, State},
    http::HeaderMap,
    response::{Html, Redirect},
    routing::{get, post},
};
use maud::html;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
pub fn routes() -> Router<App> {
    Router::new()
        .route(
            "/admin/forms/{id}/recovery",
            get(workspace).post(policy_save),
        )
        .route("/api/forms/{id}/recovery-policy", get(policy))
        .route("/api/forms/{id}/recovery", post(save))
        .route("/api/forms/{id}/recovery/read", post(read))
        .route("/api/forms/{id}/recovery/confirm", post(confirm))
        .route("/api/forms/{id}/recovery/delete", post(remove))
}
async fn policy(State(app): State<App>, Path(id): Path<String>) -> Result<Json<Value>> {
    Ok(Json(json!({"recovery":recovery::policy(&app,&id).await?})))
}
async fn save(
    State(app): State<App>,
    Path(id): Path<String>,
    Json(input): Json<recovery::Request>,
) -> Result<Json<Value>> {
    crate::operations::spam::verify(
        &app,
        &format!("form:{id}"),
        &input.spam,
        &input.values.to_string(),
    )
    .await?;
    let _guard = app.mutation().await?;
    Ok(Json(
        serde_json::to_value(recovery::request(&app, &id, input).await?).unwrap(),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Token {
    token: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Confirmation {
    token: String,
    confirmation: String,
}
async fn read(
    State(app): State<App>,
    Path(id): Path<String>,
    Json(input): Json<Token>,
) -> Result<Json<Value>> {
    Ok(Json(
        serde_json::to_value(recovery::load(&app, &id, &input.token).await?).unwrap(),
    ))
}
async fn confirm(
    State(app): State<App>,
    Path(id): Path<String>,
    Json(input): Json<Confirmation>,
) -> Result<Json<Value>> {
    let _guard = app.mutation().await?;
    recovery::confirm(&app, &id, &input.token, &input.confirmation).await?;
    Ok(Json(json!({"confirmed":true})))
}
async fn remove(
    State(app): State<App>,
    Path(id): Path<String>,
    Json(input): Json<Token>,
) -> Result<Json<Value>> {
    let _guard = app.mutation().await?;
    recovery::remove(&app, &id, &input.token).await?;
    Ok(Json(json!({"deleted":true})))
}
async fn workspace(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    super::web::editor_session(&app, &headers).await?;
    let row = sqlx::query(
        "SELECT published_version,live FROM business_forms WHERE id=$1 AND published_version>0",
    )
    .bind(&id)
    .fetch_optional(&app.db.pool)
    .await?
    .ok_or_else(Error::not_found)?;
    let version: i64 = row.get("published_version");
    let publication: super::store::PublishedForm =
        serde_json::from_str(&row.get::<String, _>("live"))
            .map_err(|_| Error::invalid("Stored form needs repair."))?;
    let current = recovery::policy(&app, &id).await?;
    let rows=sqlx::query("SELECT r.email,r.values_json,r.created_at,r.expires_at FROM form_recoveries r JOIN form_recovery_policies p ON p.form_id=r.form_id AND p.form_version=r.form_version AND p.policy_id=r.policy_id WHERE r.form_id=$1 AND r.form_version=$2 AND r.state='confirmed' AND r.expires_at>$3 ORDER BY r.created_at DESC,r.id DESC LIMIT 51").bind(&id).bind(version).bind(crate::now()).fetch_all(&app.db.pool).await?;
    // Re-check current permission before private disclosure after awaited source reads.
    let s = super::web::editor_session(&app, &headers).await?;
    // No unconfirmed values, private capabilities or unrelated draft-store rows.
    Ok(Html(view::layout(
        "Response recovery",
        &app.db.settings().await?,
        Some(&s),
        html! {
         (view::heading("Forms","Response recovery","Publish a specific purpose. Only explicitly shared, address-confirmed partial responses appear here. A single reminder is scheduled one day after confirmation; leaving a page is not consent."))
         p {a href=(format!("/admin/forms/{id}")) {"Back to form"}}
         form method="post" {(view::csrf(&s))input type="hidden" name="version" value=(version);
          label {input type="checkbox" name="enabled" value="true" checked[current.is_some()]; " Enable recovery for this publication"}
          label {"Specific recovery purpose" textarea name="purpose" maxlength="1000" {(current.as_ref().map(|p|p.policy.purpose.as_str()).unwrap_or(""))}}
          label {"Selected safe fields · comma separated" input name="fields" maxlength="1024" value=(current.as_ref().map(|p|p.policy.fields.join(",")).unwrap_or_default());}
          p class="muted" {"Available names: " (publication.form.fields.iter().map(|f|f.name.as_str()).collect::<Vec<_>>().join(", ")) ". Only unconditional scalar fields are eligible. Replacing policy cancels earlier permission; republishing requires an explicit new policy."}
          button {"Publish recovery policy"}}
         h2 {"Confirmed partial responses"}
         @if rows.is_empty(){p {"No confirmed partial responses for the current policy."}}
         @for r in rows.iter().take(50){section class="record-row" {h3 {(r.get::<String,_>("email"))}p {"Created " (view::timestamp(r.get("created_at"))) " · expires " (view::timestamp(r.get("expires_at")))}pre class="offline-original-values" {(r.get::<String,_>("values_json"))}}}
         @if rows.len()>50{p {"Showing the latest 50 confirmed partial responses. Older records remain private and expire after seven days."}}
        },
    )))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configure {
    csrf: String,
    version: i64,
    #[serde(default)]
    enabled: String,
    #[serde(default)]
    purpose: String,
    #[serde(default)]
    fields: String,
}
async fn policy_save(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(input): Form<Configure>,
) -> Result<Redirect> {
    let _guard = app.mutation().await?;
    let s = super::web::editor_session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    let policy = (input.enabled == "true").then(|| Policy {
        purpose: input.purpose.trim().into(),
        fields: input
            .fields
            .split(',')
            .map(|s| s.trim().to_owned())
            .collect(),
    });
    recovery::publish_policy(&app, &id, input.version, policy).await?;
    Ok(Redirect::to(&format!("/admin/forms/{id}/recovery")))
}
