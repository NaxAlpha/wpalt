//! Verified local-account data requests. Erasure decisions preserve financial/recovery records.
use crate::{
    App, auth, backup,
    error::{Error, Result},
    model::Session,
    view,
};
use axum::{
    Router,
    extract::{Form, Path, Query, State},
    http::HeaderMap,
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use futures_util::TryStreamExt;
use maud::html;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::BTreeMap;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub max_requests: usize,
    pub requests_per_account: usize,
    pub export_group_records: usize,
    pub export_bytes: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            max_requests: 10000,
            requests_per_account: 50,
            export_group_records: 1000,
            export_bytes: 4 * 1024 * 1024,
        }
    }
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (100..=100000).contains(&self.max_requests)
                && (2..=500).contains(&self.requests_per_account)
                && (1..=10000).contains(&self.export_group_records)
                && (64 * 1024..=32 * 1024 * 1024).contains(&self.export_bytes),
            "Privacy limits: 100..100000 site requests, 2..500/account, 1..10000 records/group, 64 KiB..32 MiB export"
        );
        Ok(())
    }
}

pub const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS privacy_requests(id TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id),kind TEXT NOT NULL CHECK(kind IN ('access','erase')),state TEXT NOT NULL DEFAULT 'requested' CHECK(state IN ('requested','fulfilled','partial','refused')),response TEXT NOT NULL DEFAULT '',version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,resolved_at BIGINT NOT NULL DEFAULT 0); CREATE INDEX IF NOT EXISTS privacy_owner_queue ON privacy_requests(state DESC,created_at,id); CREATE INDEX IF NOT EXISTS privacy_subject_history ON privacy_requests(user_id,created_at,id); CREATE UNIQUE INDEX IF NOT EXISTS privacy_open_kind ON privacy_requests(user_id,kind) WHERE state='requested';";

pub fn routes() -> Router<App> {
    Router::new()
        .route("/account/privacy", get(account).post(create))
        .route("/account/privacy/export", axum::routing::post(download))
        .route("/admin/privacy", get(queue))
        .route("/admin/privacy/{id}", get(case).post(resolve))
        .route(
            "/admin/privacy/{id}/erase-account",
            axum::routing::post(erase_account),
        )
        .layer(axum::extract::DefaultBodyLimit::max(8192))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    csrf: String,
    password: String,
    #[serde(default)]
    kind: String,
}
async fn proof(app: &App, headers: &HeaderMap, input: &Proof) -> Result<(Session, String)> {
    let session = auth::session(app, headers).await?;
    auth::csrf(&session, &input.csrf)?;
    let verified = super::factor::reauthenticate(app, &session, &input.password).await?;
    Ok((session, verified))
}
async fn current(
    app: &App,
    headers: &HeaderMap,
    previous: &Session,
    verified: &str,
) -> Result<Session> {
    let session = auth::session(app, headers).await?;
    let valid: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE id=$1 AND password_hash=$2 AND role<>'disabled'",
    )
    .bind(&previous.user.id)
    .bind(verified)
    .fetch_one(&app.db.pool)
    .await?;
    if session.user.id != previous.user.id || valid != 1 {
        return Err(Error::forbidden());
    }
    Ok(session)
}
#[derive(Default, Deserialize)]
struct Page {
    #[serde(default)]
    page: usize,
}
async fn account(
    State(app): State<App>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Result<Html<String>> {
    let session = auth::session(&app, &headers).await?;
    let offset = page.page.saturating_mul(50);
    if offset > app.config.privacy.requests_per_account {
        return Err(Error::invalid("Privacy history page is out of range."));
    }
    let mut rows=sqlx::query("SELECT id,kind,state,response,created_at,resolved_at FROM privacy_requests WHERE user_id=$1 ORDER BY created_at DESC,id DESC LIMIT 51 OFFSET $2").bind(&session.user.id).bind(offset as i64).fetch_all(&app.db.pool).await?;
    let more = rows.len() > 50;
    rows.truncate(50);
    Ok(Html(view::layout(
        "Data and privacy",
        &app.db.settings().await?,
        Some(&session),
        html! {
            (view::heading("Account","Data and privacy","Download account-linked records or ask the site owner to review an access or erasure request."))
            p {a href="/account" {"Your account"}}
            section class="panel" {h2 {"Your account-linked export"}p {"Includes identity, authored content, learning and purchase records. Excludes credentials, payment-provider secrets and other accounts. Anonymous form responses, visitor analytics and external systems cannot safely be matched to this account automatically."}form method="post" action="/account/privacy/export" {(view::csrf(&session))label {"Current password" input type="password" name="password" autocomplete="current-password" maxlength="256" required;}button {"Download my data"}}}
            section class="panel" {h2 {"Ask the site owner"}p {"Requests are recorded locally. Review the response here; no outside email service is required. Erasure may retain financial records, published work or independent recovery copies where the owner explains their retention. This does not erase data automatically."}form data-privacy-submit="true" method="post" {(view::csrf(&session))label {"Request type" select name="kind" aria-label="Request type" {option value="access" {"Data access review"}option value="erase" {"Erasure review"}}}label {"Current password" input type="password" name="password" autocomplete="current-password" maxlength="256" required;}button {"Send data request"}}}
            nav aria-label="Request history pages" {@if page.page>0{a class="button secondary" href=(format!("/account/privacy?page={}",page.page-1)){"Newer requests"}}@if more{a class="button secondary" href=(format!("/account/privacy?page={}",page.page+1)){"Older requests"}}}
            section class="panel" {h2 {"Request history"}@if rows.is_empty(){p {"No data requests yet."}}@for row in rows {article {h3 {(row.get::<String,_>("kind")) " · " (row.get::<String,_>("state"))}p {"Requested " (view::timestamp(row.get("created_at"))) " · resolved " (view::timestamp(row.get("resolved_at")))}@if !row.get::<String,_>("response").is_empty(){p {(row.get::<String,_>("response"))}}}}}
        },
    )))
}
async fn create(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<Proof>,
) -> Result<Redirect> {
    if !["access", "erase"].contains(&input.kind.as_str()) {
        return Err(Error::invalid("Choose access or erasure review."));
    }
    let (session, verified) = proof(&app, &headers, &input).await?;
    let _guard = app.mutation().await?;
    current(&app, &headers, &session, &verified).await?;
    let mut tx = app.db.pool.begin().await?;
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM privacy_requests WHERE user_id=$1 AND kind=$2 AND state='requested'",
    )
    .bind(&session.user.id)
    .bind(&input.kind)
    .fetch_optional(&mut *tx)
    .await?;
    if existing.is_none() {
        let global: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM privacy_requests")
            .fetch_one(&mut *tx)
            .await?;
        let personal: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM privacy_requests WHERE user_id=$1")
                .bind(&session.user.id)
                .fetch_one(&mut *tx)
                .await?;
        if global as usize >= app.config.privacy.max_requests
            || personal as usize >= app.config.privacy.requests_per_account
        {
            return Err(Error::invalid(
                "Data request storage is full; contact the site owner for manual handling.",
            ));
        }
        sqlx::query("INSERT INTO privacy_requests(id,user_id,kind,created_at) VALUES($1,$2,$3,$4)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&session.user.id)
            .bind(&input.kind)
            .bind(crate::now())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Redirect::to("/account/privacy"))
}
async fn owner(app: &App, headers: &HeaderMap) -> Result<Session> {
    let s = auth::session(app, headers).await?;
    if !s.is_admin() {
        return Err(Error::forbidden());
    }
    Ok(s)
}
async fn queue(
    State(app): State<App>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Result<Html<String>> {
    let s = owner(&app, &headers).await?;
    let offset = page.page.saturating_mul(100);
    if offset > app.config.privacy.max_requests {
        return Err(Error::invalid("Privacy queue page is out of range."));
    }
    // Select the bounded page through the queue index before looking up names.
    // Otherwise SQLite can drive the join from users and sort the whole queue.
    let mut rows=sqlx::query("SELECT p.id,p.kind,p.state,p.created_at,u.name FROM (SELECT id,user_id,kind,state,created_at FROM privacy_requests ORDER BY state DESC,created_at,id LIMIT 101 OFFSET $1) p JOIN users u ON u.id=p.user_id ORDER BY p.state DESC,p.created_at,p.id").bind(offset as i64).fetch_all(&app.db.pool).await?;
    let more = rows.len() > 100;
    rows.truncate(100);
    Ok(Html(view::layout(
        "Data requests",
        &app.db.settings().await?,
        Some(&s),
        html! {(view::heading("Privacy","Data requests","Verified local-account requests; record what was handled and what must be retained."))p {"Requests appear pending first. All pages remain available until the configured admission budget is reached."}nav aria-label="Data request pages" {@if page.page>0{a class="button secondary" href=(format!("/admin/privacy?page={}",page.page-1)){"Previous requests"}}@if more{a class="button secondary" href=(format!("/admin/privacy?page={}",page.page+1)){"More requests"}}}@if rows.is_empty(){p {"No requests yet."}}@for row in rows {section class="panel"{h2 {a href=(format!("/admin/privacy/{}",row.get::<String,_>("id"))) {(row.get::<String,_>("name")) " · " (row.get::<String,_>("kind"))}}p {(row.get::<String,_>("state")) " · " (view::timestamp(row.get("created_at")))}}}},
    )))
}
async fn case(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let s = owner(&app, &headers).await?;
    let row=sqlx::query("SELECT p.*,u.name,u.email FROM privacy_requests p JOIN users u ON u.id=p.user_id WHERE p.id=$1").bind(&id).fetch_optional(&app.db.pool).await?.ok_or_else(Error::not_found)?;
    Ok(Html(view::layout(
        "Review data request",
        &app.db.settings().await?,
        Some(&s),
        html! {(view::heading("Privacy","Review data request","Make a deliberate decision; retain financial identity and never replay delivery during erasure."))p {a class="button secondary" href="/admin/privacy" {"All data requests"}}section class="panel" {h2 {(row.get::<String,_>("name"))}p {(row.get::<String,_>("email")) " · " (row.get::<String,_>("kind")) " · " (row.get::<String,_>("state"))}p {"Requested " (view::timestamp(row.get("created_at")))}p {"Review account, profile, memberships, audience and form records using their existing controls. Anonymous records require independent identity verification; never infer ownership from a name alone. Record deletions, exports and any retained data, reasons and follow-up. Independent backups and external services need separate handling."} @if row.get::<String,_>("state")=="requested"{form data-privacy-submit="true" method="post" {(view::csrf(&s))input type="hidden" name="version" value=(row.get::<i64,_>("version"));label {"Decision" select name="state" aria-label="Decision" {option value="partial" {"Partially handled"}option value="fulfilled" {"Fulfilled"}option value="refused" {"Refused"}}}label {"Response for the requester" textarea name="response" maxlength="2000" required {}}p {"This response is shown to the requester. Record completed handling and explain any retained records."}button {"Record reviewed decision"}} @if row.get::<String,_>("kind")=="erase" {form data-privacy-submit="true" class="panel" method="post" action=(format!("/admin/privacy/{id}/erase-account")) {(view::csrf(&s))input type="hidden" name="version" value=(row.get::<i64,_>("version"));h3 {"Remove account identity and profile"}p {"Disables this account, removes its login credentials, linked identities and profile biography, and replaces its name/email with an anonymous local identity. Financial records, published content, audience/form data and independent backups remain for separate review. The request is recorded as partially handled. Give the requester the response through an independently verified channel because sign-in will stop."}label {"Removal and retention explanation" textarea name="response" maxlength="1800" required {}}label {input type="checkbox" name="confirm" value="true" required; "Disable account and remove its identity/profile; retained records require separate handling"}button class="danger" {"Remove account identity and profile"}}}}@else{p {(row.get::<String,_>("response"))}}}},
    )))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    csrf: String,
    version: i64,
    state: String,
    response: String,
}
async fn resolve(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(input): Form<Decision>,
) -> Result<Redirect> {
    if !["fulfilled", "partial", "refused"].contains(&input.state.as_str())
        || input.response.trim().is_empty()
        || input.response.len() > 2000
    {
        return Err(Error::invalid(
            "Record a clear handling outcome up to 2,000 bytes.",
        ));
    }
    let _guard = app.mutation().await?;
    let s = owner(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    if sqlx::query("UPDATE privacy_requests SET state=$1,response=$2,resolved_at=$3,version=version+1 WHERE id=$4 AND state='requested' AND version=$5").bind(&input.state).bind(input.response.trim()).bind(crate::now()).bind(&id).bind(input.version).execute(&app.db.pool).await?.rows_affected()!=1{return Err(Error::conflict());}
    Ok(Redirect::to(&format!("/admin/privacy/{id}")))
}
// Explicit field allowlist excludes tokens, credential material and provider payloads.
const EXPORTS: &[(&str, &str, &str)] = &[
    ("users", "id", "id,email,name,role,created_at"),
    (
        "posts",
        "author_id",
        "id,slug,title,body,document,fields,status,updated_at",
    ),
    // Only the subject's own authored decisions, never other reviewers' notes.
    (
        "editorial_decisions",
        "actor_id",
        "id,post_id,action,notes,created_at",
    ),
    ("member_profiles", "user_id", "user_id,biography,version"),
    (
        "member_grants",
        "user_id",
        "id,entitlement,starts_at,expires_at,revoked,created_at",
    ),
    (
        "member_group_users",
        "user_id",
        "group_id,user_id,created_at",
    ),
    (
        "member_progress",
        "user_id",
        "course_id,course_version,lesson_id,user_id,attempts,best_score,completed_at",
    ),
    (
        "member_assignments",
        "user_id",
        "id,course_id,lesson_id,lesson_title,body,state,feedback,created_at",
    ),
    (
        "member_certificates",
        "user_id",
        "id,course_id,course_version,issued_at,revoked",
    ),
    (
        "member_discussions",
        "user_id",
        "id,group_id,body,state,created_at",
    ),
    (
        "shop_orders",
        "user_id",
        "id,customer_name,customer_email,shipping_address,currency,total_minor,payment_state,fulfillment,paid_minor,refunded_minor,created_at",
    ),
    (
        "shop_subscriptions",
        "user_id",
        "id,variant_id,entitlement,state,price_minor,period_start,period_end",
    ),
    (
        "member_attempts",
        "user_id",
        "id,course_id,course_version,lesson_id,score,passed,created_at",
    ),
    ("member_referrals", "user_id", "id,title,visits,created_at"),
    (
        "member_commissions",
        "@referral",
        "id,referral_id,amount_minor,currency,state,created_at",
    ),
    (
        "member_gifts",
        "claimed_by",
        "id,entitlement,expires_at,duration_seconds,created_at",
    ),
    ("shop_carts", "user_id", "user_id,version,updated_at"),
    (
        "shop_cart_lines",
        "user_id",
        "user_id,variant_id,slot_id,quantity",
    ),
    (
        "shop_order_lines",
        "@order",
        "id,order_id,title,sku,kind,quantity,unit_minor,line_minor,slot_id,allocation",
    ),
    (
        "shop_payments",
        "@order",
        "id,order_id,provider,amount_minor,currency,created_at",
    ),
    (
        "shop_refunds",
        "@order",
        "id,order_id,amount_minor,restock,state,created_at",
    ),
    (
        "shop_payouts",
        "user_id",
        "id,amount_minor,currency,created_at",
    ),
    (
        "privacy_requests",
        "user_id",
        "id,kind,state,response,created_at,resolved_at",
    ),
];
pub async fn export(app: &App, subject: &str) -> Result<Vec<u8>> {
    let started = std::time::Instant::now();
    crate::membership::uuid(subject)?;
    let mut tx = app.db.pool.begin().await?;
    if app.db.postgres {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
    }
    let mut result = BTreeMap::new();
    let mut used = 0usize;
    for (table, key, fields) in EXPORTS {
        let columns = backup::TABLES
            .iter()
            .find(|(name, _)| name == table)
            .expect("export table allowlist")
            .1;
        let fields: Vec<_> = fields.split(',').collect();
        let selected = fields.join(",");
        let order = if *key == "@order" {
            "order_id,id"
        } else if *key == "@referral" {
            "referral_id,id"
        } else if *table == "shop_cart_lines" {
            "variant_id,slot_id"
        } else if *table == "member_progress" {
            "course_id,course_version,lesson_id"
        } else {
            fields[0]
        };
        let predicate = match *key {
            "@order" => "order_id IN (SELECT id FROM shop_orders WHERE user_id=$1)".to_owned(),
            "@referral" => {
                "referral_id IN (SELECT id FROM member_referrals WHERE user_id=$1)".to_owned()
            }
            _ => format!("{key}=$1"),
        };
        let sql =
            format!("SELECT {selected} FROM {table} WHERE {predicate} ORDER BY {order} LIMIT $2");
        let mut stream = sqlx::query(&sql)
            .bind(subject)
            .bind((app.config.privacy.export_group_records + 1) as i64)
            .fetch(&mut *tx);
        let mut records = Vec::new();
        while let Some(row) = stream.try_next().await? {
            if records.len() == app.config.privacy.export_group_records {
                return Err(Error::invalid(
                    "A data group exceeds its configured record budget; ask the owner for a scoped manual export.",
                ));
            }
            let mut record = BTreeMap::<&str, Value>::new();
            for field in &fields {
                let integer = columns
                    .iter()
                    .find(|(name, _)| name == field)
                    .ok_or_else(|| Error::invalid("Export field definition needs repair."))?
                    .1;
                record.insert(
                    field,
                    if integer {
                        json!(row.get::<i64, _>(*field))
                    } else {
                        json!(row.get::<String, _>(*field))
                    },
                );
            }
            used += serde_json::to_vec(&record)
                .map_err(|_| Error::invalid("Export serialization failed."))?
                .len();
            if used > app.config.privacy.export_bytes {
                return Err(Error::invalid(
                    "Account data exceeds its configured automatic export budget; ask the owner for a scoped export.",
                ));
            }
            records.push(record);
        }
        if *table == "users" && records.is_empty() {
            return Err(Error::not_found());
        }
        result.insert(*table, records);
    }
    tx.commit().await?;
    let bytes=serde_json::to_vec(&json!({"format":"wpalt-personal-data-v1","created_at":crate::now(),"account_linked_records":result,"additional_review":"Anonymous submissions, visitor cookies, script storage, independent backups and external systems require separately verified owner handling."})).map_err(|_|Error::invalid("Export serialization failed."))?;
    if bytes.len() > app.config.privacy.export_bytes {
        return Err(Error::invalid(
            "Account export exceeds its configured byte budget; ask the owner for a scoped export.",
        ));
    }
    tracing::debug!(
        event = "privacy_export_completed",
        groups = EXPORTS.len(),
        bytes = bytes.len(),
        elapsed_us = started.elapsed().as_micros() as u64
    );
    Ok(bytes)
}
async fn download(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<Proof>,
) -> Result<Response> {
    let (session, verified) = proof(&app, &headers, &input).await?;
    let _guard = app.mutations.lock().await;
    current(&app, &headers, &session, &verified).await?;
    let bytes = export(&app, &session.user.id).await?;
    Ok((
        [
            ("content-type", "application/json"),
            (
                "content-disposition",
                "attachment; filename=wpalt-personal-data.json",
            ),
            ("cache-control", "no-store"),
        ],
        bytes,
    )
        .into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Erasure {
    csrf: String,
    version: i64,
    response: String,
    #[serde(default)]
    confirm: String,
}
async fn erase_account(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(input): Form<Erasure>,
) -> Result<Redirect> {
    if input.confirm != "true" || input.response.trim().is_empty() || input.response.len() > 1800 {
        return Err(Error::invalid(
            "Confirm account identity removal and explain remaining retention up to 1,800 bytes.",
        ));
    }
    owner(&app, &headers).await?;
    let permit = app
        .password_work
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::invalid("Password workers busy; retry account removal."))?;
    let erased_hash = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        auth::hash_password(&auth::random_token())
    })
    .await
    .map_err(|_| Error::invalid("Account credential removal worker failed."))?
    .map_err(|_| Error::invalid("Account credential removal failed."))?;
    let _guard = app.mutation().await?;
    let actor = owner(&app, &headers).await?;
    auth::csrf(&actor, &input.csrf)?;
    let mut tx = app.db.pool.begin().await?;
    let row=sqlx::query("SELECT p.user_id,u.role FROM privacy_requests p JOIN users u ON u.id=p.user_id WHERE p.id=$1 AND p.kind='erase' AND p.state='requested' AND p.version=$2").bind(&id).bind(input.version).fetch_optional(&mut *tx).await?.ok_or_else(Error::conflict)?;
    let subject = row.get::<String, _>("user_id");
    if row.get::<String, _>("role") == "admin" {
        let others: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role='admin' AND id<>$1")
                .bind(&subject)
                .fetch_one(&mut *tx)
                .await?;
        if others == 0 {
            return Err(Error::invalid(
                "The last site owner cannot be removed; establish another owner first.",
            ));
        }
    }
    // Keep stable subject IDs: orders, course/payment histories and published authors remain referentially intact.
    sqlx::query("UPDATE users SET email=$1,name='Former account',role='disabled',password_hash=$2 WHERE id=$3").bind(format!("erased-{}@invalid.example",uuid::Uuid::new_v4())).bind(erased_hash).bind(&subject).execute(&mut *tx).await?;
    for table in [
        "sessions",
        "user_factors",
        "user_passkeys",
        "member_identities",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE user_id=$1"))
            .bind(&subject)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("DELETE FROM member_profiles WHERE user_id=$1")
        .bind(&subject)
        .execute(&mut *tx)
        .await?;
    let response = format!(
        "Account identity and profile removed; sign-in disabled. Other financial, content, audience/form and recovery records require separate handling. {}",
        input.response.trim()
    );
    sqlx::query("UPDATE privacy_requests SET state='partial',response=$1,resolved_at=$2,version=version+1 WHERE id=$3 AND version=$4 AND state='requested'").bind(response).bind(crate::now()).bind(&id).bind(input.version).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Redirect::to(&format!("/admin/privacy/{id}")))
}
