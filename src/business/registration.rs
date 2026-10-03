//! Ownership proof followed by deliberate owner approval; no public role selection.
use crate::{
    App, auth,
    error::{Error, Result},
    now,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Any, Row, Transaction};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub email_field: String,
    pub name_field: String,
}
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS registration_requests(id TEXT PRIMARY KEY,entry_id TEXT NOT NULL UNIQUE REFERENCES form_entries(id),email TEXT NOT NULL UNIQUE,name TEXT NOT NULL,token_hash TEXT NOT NULL UNIQUE,password_hash TEXT NOT NULL DEFAULT '',state TEXT NOT NULL CHECK(state IN ('pending','verified','approved','rejected')),expires_at BIGINT NOT NULL,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS registration_pending ON registration_requests(state,created_at,id);
CREATE INDEX IF NOT EXISTS registration_expiry ON registration_requests(expires_at,id) WHERE state='pending';
"#;
pub async fn request(
    app: &App,
    tx: &mut Transaction<'_, Any>,
    entry: &str,
    action: &Action,
    values: &Value,
) -> Result<()> {
    let email = super::mail::email(
        values
            .get(&action.email_field)
            .and_then(Value::as_str)
            .unwrap_or(""),
    )?;
    let name = values
        .get(&action.name_field)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if name.is_empty() || name.len() > 100 {
        return Err(Error::invalid(
            "Registration requires a name up to 100 bytes.",
        ));
    }
    // A mailbox cannot be flooded with repeated proofs; existing account/request is a generic no-op.
    let existing:Option<String>=sqlx::query_scalar("SELECT email FROM users WHERE email=$1 UNION SELECT email FROM registration_requests WHERE email=$1").bind(&email).fetch_optional(&mut **tx).await?;
    if existing.is_some() {
        return Ok(());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let token = auth::random_token();
    let inserted=sqlx::query("INSERT INTO registration_requests(id,entry_id,email,name,token_hash,state,expires_at,created_at) VALUES($1,$2,$3,$4,$5,'pending',$6,$7) ON CONFLICT(email) DO NOTHING").bind(&id).bind(entry).bind(&email).bind(name).bind(auth::digest(token.as_bytes())).bind(now()+86400).bind(now()).execute(&mut **tx).await?;
    if inserted.rows_affected() == 0 {
        return Ok(());
    }
    super::quotas::reserve(app, tx, "registrations", 1, 0).await?;
    let url = format!(
        "{}/registration/{token}",
        app.config.base_url.trim_end_matches('/')
    );
    let html=maud::html!{p {"You requested a local account. Verify your mailbox and choose a password. The owner must approve before you can sign in."}p {a href=(&url){"Review account request"}}}.into_string();
    super::mail::enqueue(
        app,
        tx,
        super::mail::MessageInput {
            dedupe: &format!("registration:{id}"),
            contact: "",
            list: "",
            kind: "notification",
            recipient: &email,
            subject: "Verify your account request",
            html: &html,
            plain: &format!("Verify account request: {url}\nOwner approval is required."),
        },
    )
    .await?;
    Ok(())
}
pub async fn review(app: &App, token: &str) -> Result<String> {
    if token.len() != 64 || !token.bytes().all(|v| v.is_ascii_hexdigit()) {
        return Err(Error::not_found());
    }
    sqlx::query_scalar("SELECT name FROM registration_requests WHERE token_hash=$1 AND expires_at>$2 AND state='pending'").bind(auth::digest(token.as_bytes())).bind(now()).fetch_optional(&app.db.pool).await?.ok_or_else(Error::not_found)
}
pub async fn verify(app: &App, token: &str, password: &str) -> Result<()> {
    review(app, token).await?;
    if !(12..=256).contains(&password.len()) {
        return Err(Error::invalid("Use a password between 12 and 256 bytes."));
    }
    let permit = app.password_work.clone().try_acquire_owned().map_err(|_| {
        Error(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "Password workers are busy. Retry shortly.",
        )
    })?;
    let password = password.to_owned();
    let hash = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        auth::hash_password(&password)
    })
    .await
    .map_err(|_| Error::invalid("Password could not be processed."))?
    .map_err(|_| Error::invalid("Password could not be processed."))?;
    if sqlx::query("UPDATE registration_requests SET password_hash=$1,state='verified',token_hash=$2,version=version+1 WHERE token_hash=$3 AND expires_at>$4 AND state='pending'").bind(hash).bind(auth::digest(auth::random_token().as_bytes())).bind(auth::digest(token.as_bytes())).bind(now()).execute(&app.db.pool).await?.rows_affected()!=1{return Err(Error::conflict());}
    Ok(())
}
pub async fn decide(app: &App, id: &str, version: i64, approve: bool) -> Result<()> {
    let mut tx = app.db.pool.begin().await?;
    let row=sqlx::query("UPDATE registration_requests SET state=$1,version=version+1 WHERE id=$2 AND version=$3 AND state='verified' RETURNING email,name,password_hash").bind(if approve{"approved"}else{"rejected"}).bind(id).bind(version).fetch_optional(&mut *tx).await?.ok_or_else(Error::conflict)?;
    if approve {
        sqlx::query("INSERT INTO users(id,email,name,role,password_hash,created_at) VALUES($1,$2,$3,'subscriber',$4,$5)").bind(uuid::Uuid::new_v4().to_string()).bind(row.get::<String,_>("email")).bind(row.get::<String,_>("name")).bind(row.get::<String,_>("password_hash")).bind(now()).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE registration_requests SET password_hash='' WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
