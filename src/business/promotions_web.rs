//! Owner-composed local offers and consented visitor allocation.
use super::{engagement, promotions};
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
        .route("/admin/promotions", get(index).post(create))
        .route("/admin/promotions/{id}", get(editor).post(save))
        .route("/admin/promotions/{id}/rewards", post(reward))
        .route("/api/engagement/offers", post(visit))
        .route("/api/engagement/offers/{id}/claim", post(claim))
        .layer(axum::extract::DefaultBodyLimit::max(128 * 1024))
}
async fn owner(app: &App, h: &HeaderMap) -> Result<crate::model::Session> {
    let s = auth::session(app, h).await?;
    if !s.is_admin() {
        return Err(Error::forbidden());
    }
    Ok(s)
}
async fn index(State(app): State<App>, h: HeaderMap) -> Result<Html<String>> {
    let s = owner(&app, &h).await?;
    let rows=sqlx::query("SELECT p.id,p.title,p.active,COALESCE((SELECT CAST(SUM(count) AS BIGINT) FROM promotion_impressions WHERE promotion_id=p.id),0) AS views,(SELECT COUNT(*) FROM promotion_claims WHERE promotion_id=p.id) AS claims FROM business_promotions p ORDER BY created_at DESC,id LIMIT 100").fetch_all(&app.db.pool).await?;
    Ok(Html(view::layout(
        "Local offers",
        &app.db.settings().await?,
        Some(&s),
        html! {
        (view::heading("Engagement","Local offers and experiments","Compose accessible messages, compare variants and allocate bounded local rewards."))
        p {a href="/admin/engagement" {"Engagement reports"}}
        form method="post" {(view::csrf(&s))label {"Promotion title" input name="title" required maxlength="160";}button {"Create promotion"}}
        @for row in rows {section class="panel" {h2 {a href=(format!("/admin/promotions/{}",row.get::<String,_>("id"))){(row.get::<String,_>("title"))}}p {(if row.get::<i64,_>("active")==1 {"Active"} else {"Draft"}) " · " (row.get::<i64,_>("views")) " impressions · " (row.get::<i64,_>("claims")) " retained claims"}}}
        },
    )))
}
#[derive(Deserialize)]
struct Create {
    csrf: String,
    title: String,
}
async fn create(State(app): State<App>, h: HeaderMap, Form(i): Form<Create>) -> Result<Redirect> {
    let s = owner(&app, &h).await?;
    auth::csrf(&s, &i.csrf)?;
    let id = promotions::create(&app, &i.title).await?;
    Ok(Redirect::to(&format!("/admin/promotions/{id}")))
}
async fn editor(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let s = owner(&app, &h).await?;
    let row = sqlx::query("SELECT * FROM business_promotions WHERE id=$1")
        .bind(&id)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
    let target: promotions::Target = serde_json::from_str(&row.get::<String, _>("target"))
        .map_err(|_| Error::invalid("Stored target needs repair."))?;
    let rewards=sqlx::query("SELECT label,weight,remaining,issued FROM promotion_rewards WHERE promotion_id=$1 ORDER BY id LIMIT 16").bind(&id).fetch_all(&app.db.pool).await?;
    let variants=sqlx::query("SELECT i.variant,CAST(SUM(i.count) AS BIGINT) AS impressions,COUNT(*) AS sessions,COUNT(c.id) AS claims FROM promotion_impressions i LEFT JOIN promotion_claims c ON c.promotion_id=i.promotion_id AND c.session_hash=i.session_hash WHERE i.promotion_id=$1 GROUP BY i.variant ORDER BY i.variant").bind(&id).fetch_all(&app.db.pool).await?;
    Ok(Html(view::layout(
        "Compose local offer",
        &app.db.settings().await?,
        Some(&s),
        html! {
        (view::heading("Engagement","Compose local offer",&row.get::<String,_>("title")))p {a href="/admin/promotions" {"All offers"}}
        @for variant in ["a","b"] {section class="panel" {h2 {"Variant " (variant.to_uppercase())}form method="post" data-editor data-owner=(&s.user.id) {(view::csrf(&s))input type="hidden" name="version" value=(row.get::<i64,_>("version"));input type="hidden" name="variant" value=(variant);input type="hidden" name="locale" value="en";input type="hidden" name="document" value=(row.get::<String,_>(if variant=="a"{"document_a"}else{"document_b"}));div data-writing-canvas hidden {}label {"Message" textarea name="body" class="editor-body" {}}label data-markdown-replacement {input type="checkbox" name="import_markdown" value="true";"Replace using Markdown"}
        fieldset {legend {"Targeting and frequency"}label {"Public paths (one per line; empty means all)" textarea name="paths" maxlength="3216" {(target.paths.join("\n"))}}label {"Device" select name="device" {@for value in ["all","mobile","desktop"]{option value=(value) selected[target.device==value]{(value)}}}}label {"Referrer category" select name="referrer" {@for value in ["all","direct","same_site","external"]{option value=(value) selected[target.referrer==value]{(value)}}}}label {"Maximum impressions per consented visitor session" input name="max_impressions" type="number" min="1" max="10" value=(target.max_impressions) required;}
        input type="hidden" name="starts_at" value=(target.starts_at);label {"Starts (empty means now)" input type="datetime-local" data-epoch-for="starts_at";}
        input type="hidden" name="ends_at" value=(target.ends_at);label {"Ends (empty means no end date)" input type="datetime-local" data-epoch-for="ends_at";}}
        label {input name="active" type="checkbox" value="true" checked[row.get::<i64,_>("active")==1];"Activate"}label {input name="experiment" type="checkbox" value="true" checked[row.get::<i64,_>("experiment")==1];"Compare stable A/B variants"}label {input name="wheel" type="checkbox" value="true" checked[row.get::<i64,_>("wheel")==1];"Offer a weighted local reward draw"}p {"Save one variant at a time. Saving updates the privacy policy version; visitors must renew consent before targeting resumes."}button {"Save variant " (variant.to_uppercase())}}}}
        section class="panel" {h2 {"Local rewards"}p {"No payment or redemption engine. Each consented cookie session may draw once. Clearing cookies creates a new session; this is not proof of a unique person."}@for r in rewards {p {(r.get::<String,_>("label")) " · weight " (r.get::<i64,_>("weight")) " · " (r.get::<i64,_>("remaining")) " remaining · " (r.get::<i64,_>("issued")) " issued"}}form method="post" action=(format!("/admin/promotions/{id}/rewards")){(view::csrf(&s))label {"Reward label" input name="label" required maxlength="160";}label {"Relative weight" input name="weight" type="number" min="1" max="10000" value="1" required;}label {"Available stock" input name="stock" type="number" min="1" max="1000000" value="1" required;}button {"Add reward"}}}
        section class="panel" {h2 {"Variant report"}@for r in variants {p {"Variant " (r.get::<String,_>("variant")) " · " (r.get::<i64,_>("impressions")) " impressions · " (r.get::<i64,_>("sessions")) " retained visitor sessions · " (r.get::<i64, _>("claims")) " retained claims"}}p {"Withdrawal removes visitor-linked reports and receipts. Issued inventory counts remain. Small samples do not establish statistical significance."}}
        script defer src="/assets/editor.js" {}
        },
    )))
}
#[derive(Deserialize)]
struct Save {
    csrf: String,
    version: i64,
    variant: String,
    document: String,
    body: String,
    #[serde(default)]
    import_markdown: String,
    paths: String,
    device: String,
    referrer: String,
    max_impressions: i64,
    starts_at: i64,
    ends_at: i64,
    #[serde(default)]
    active: String,
    #[serde(default)]
    experiment: String,
    #[serde(default)]
    wheel: String,
}
async fn save(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(i): Form<Save>,
) -> Result<Redirect> {
    let s = owner(&app, &h).await?;
    auth::csrf(&s, &i.csrf)?;
    let document = if i.import_markdown == "true" {
        crate::document::import(&i.body, "[]")?.encode()
    } else {
        i.document
    };
    promotions::save(
        &app,
        &id,
        i.version,
        &i.variant,
        &document,
        promotions::Target {
            paths: i
                .paths
                .lines()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .collect(),
            device: i.device,
            referrer: i.referrer,
            starts_at: i.starts_at,
            ends_at: i.ends_at,
            max_impressions: i.max_impressions,
        },
        i.active == "true",
        i.experiment == "true",
        i.wheel == "true",
    )
    .await?;
    Ok(Redirect::to(&format!("/admin/promotions/{id}")))
}
#[derive(Deserialize)]
struct Reward {
    csrf: String,
    label: String,
    weight: i64,
    stock: i64,
}
async fn reward(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(i): Form<Reward>,
) -> Result<Redirect> {
    let s = owner(&app, &h).await?;
    auth::csrf(&s, &i.csrf)?;
    promotions::reward(&app, &id, &i.label, i.weight, i.stock).await?;
    Ok(Redirect::to(&format!("/admin/promotions/{id}")))
}
async fn visit(
    State(app): State<App>,
    h: HeaderMap,
    Json(i): Json<promotions::Visit>,
) -> Result<Json<Value>> {
    Ok(Json(json!({"offer":promotions::visit(&app,&h,i).await?})))
}
async fn claim(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let result = promotions::claim(&app, &h, &id).await?;
    let path: Option<String> = sqlx::query_scalar(
        "SELECT path FROM promotion_impressions WHERE promotion_id=$1 AND session_hash=$2",
    )
    .bind(&id)
    .bind(engagement::token(&h).unwrap_or_default())
    .fetch_optional(&app.db.pool)
    .await?;
    if let Some(path) = path {
        if let Err(error) = engagement::conversion(
            &app,
            &h,
            result["id"].as_str().unwrap_or_default(),
            &path,
            "offer_claim",
        )
        .await
        {
            tracing::warn!(
                status = error.0.as_u16(),
                "Offer conversion was not recorded"
            );
        }
    }
    Ok(Json(result))
}
