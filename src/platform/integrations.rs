//! Revocable owner-issued external credentials. They are never browser sessions
//! or recovery credentials, and do not execute extension code in the server.
use crate::{
    App, auth,
    error::{Error, Result},
    model::{Session, User},
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::HeaderMap,
    routing::get,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

pub const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS integration_credentials(id TEXT PRIMARY KEY,token_hash TEXT UNIQUE NOT NULL,user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,password_fingerprint TEXT NOT NULL,origin TEXT NOT NULL,name TEXT NOT NULL,read_content BIGINT NOT NULL CHECK(read_content IN (0,1)),draft_content BIGINT NOT NULL CHECK(draft_content IN (0,1)),expires_at BIGINT NOT NULL,created_at BIGINT NOT NULL); CREATE INDEX IF NOT EXISTS integration_owner ON integration_credentials(user_id,id);";

pub struct Issued {
    pub id: String,
    pub token: String,
}
pub async fn issue(
    app: &App,
    owner: &Session,
    email: &str,
    name: &str,
    draft: bool,
    days: i64,
) -> Result<Issued> {
    if owner.hash.starts_with("integration:")
        || owner.user.role != "admin"
        || name.trim().is_empty()
        || name.len() > 100
        || name.chars().any(char::is_control)
        || !(1..=30).contains(&days)
    {
        return Err(Error::invalid(
            "Choose a name within 100 bytes and an expiry of 1–30 days.",
        ));
    }
    let _guard = app.mutation().await?;
    auth::current_editor(app, owner).await?;
    if app.clone_held.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(Error::forbidden());
    }
    let origin = url::Url::parse(&app.config.origin()).map_err(|_| Error::forbidden())?;
    if origin.scheme() != "https"
        && ![Some("127.0.0.1"), Some("localhost"), Some("[::1]")].contains(&origin.host_str())
    {
        return Err(Error::invalid(
            "External integration credentials require HTTPS; loopback development is supported.",
        ));
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM integration_credentials")
        .fetch_one(&app.db.pool)
        .await?;
    if count >= 128 {
        return Err(Error::invalid(
            "Remove unused credentials before creating more; at most 128 are retained.",
        ));
    }
    let user = sqlx::query(
        "SELECT id,password_hash FROM users WHERE email=$1 AND role IN ('admin','editor')",
    )
    .bind(email)
    .fetch_optional(&app.db.pool)
    .await?
    .ok_or_else(|| Error::invalid("Choose an existing editor or administrator account."))?;
    let id = uuid::Uuid::new_v4().to_string();
    let token = auth::random_token();
    sqlx::query("INSERT INTO integration_credentials(id,token_hash,user_id,password_fingerprint,origin,name,read_content,draft_content,expires_at,created_at) VALUES($1,$2,$3,$4,$5,$6,1,$7,$8,$9)")
        .bind(&id).bind(auth::digest(token.as_bytes())).bind(user.get::<String,_>("id"))
        .bind(auth::digest(user.get::<String,_>("password_hash").as_bytes())).bind(app.config.origin()).bind(name.trim()).bind(i64::from(draft)).bind(crate::now()+days*86400).bind(crate::now()).execute(&app.db.pool).await?;
    Ok(Issued { id, token })
}
pub async fn revoke(app: &App, owner: &Session, id: &str) -> Result<()> {
    if owner.hash.starts_with("integration:") || owner.user.role != "admin" {
        return Err(Error::forbidden());
    }
    uuid::Uuid::parse_str(id).map_err(|_| Error::invalid("Use the credential UUID."))?;
    let _guard = app.mutation().await?;
    auth::current_editor(app, owner).await?;
    sqlx::query("DELETE FROM integration_credentials WHERE id=$1")
        .bind(id)
        .execute(&app.db.pool)
        .await?;
    Ok(())
}
pub async fn inventory(app: &App, owner: &Session) -> Result<Value> {
    if owner.hash.starts_with("integration:") || owner.user.role != "admin" {
        return Err(Error::forbidden());
    }
    auth::current_editor(app, owner).await?;
    let rows=sqlx::query("SELECT id,user_id,name,draft_content,expires_at,created_at FROM integration_credentials ORDER BY id LIMIT 128").fetch_all(&app.db.pool).await?;
    Ok(
        json!({"credentials":rows.into_iter().map(|r|json!({"id":r.get::<String,_>("id"),"user_id":r.get::<String,_>("user_id"),"name":r.get::<String,_>("name"),"scopes":if r.get::<i64,_>("draft_content")==1 {vec!["content:read","content:draft"]}else{vec!["content:read"]},"expires_at":r.get::<i64,_>("expires_at"),"created_at":r.get::<i64,_>("created_at")})).collect::<Vec<_>>()}),
    )
}
async fn lookup(app: &App, hash: &str, draft: bool) -> Result<(Session, String)> {
    let r=sqlx::query("SELECT u.id,u.email,u.name,u.role,u.password_hash,c.password_fingerprint,c.id AS credential_id FROM integration_credentials c JOIN users u ON u.id=c.user_id WHERE c.token_hash=$1 AND c.expires_at>$2 AND c.read_content=1 AND ($3=0 OR c.draft_content=1) AND u.role IN ('admin','editor') AND c.origin=$4")
        .bind(hash).bind(crate::now()).bind(i64::from(draft)).bind(app.config.origin()).fetch_optional(&app.db.pool).await?.ok_or_else(Error::forbidden)?;
    if auth::digest(r.get::<String, _>("password_hash").as_bytes())
        != r.get::<String, _>("password_fingerprint")
    {
        return Err(Error::forbidden());
    }
    Ok((
        Session {
            interface_locale: "en".into(),
            user: User {
                id: r.get("id"),
                email: r.get("email"),
                name: r.get("name"),
                role: r.get("role"),
            },
            csrf: String::new(),
            hash: format!("integration:{hash}"),
        },
        r.get("credential_id"),
    ))
}
pub async fn authenticate(app: &App, headers: &HeaderMap, draft: bool) -> Result<Session> {
    Ok(authenticate_with_identity(app, headers, draft).await?.0)
}
pub async fn authenticate_with_identity(
    app: &App,
    headers: &HeaderMap,
    draft: bool,
) -> Result<(Session, String)> {
    if headers.get_all("authorization").iter().count() != 1
        || headers.contains_key("cookie")
        || headers
            .get("origin")
            .is_some_and(|o| o.to_str().ok() != Some(app.config.origin().as_str()))
    {
        return Err(Error::forbidden());
    }
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(Error::forbidden)?;
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::forbidden());
    }
    lookup(app, &auth::digest(token.as_bytes()), draft).await
}
/// Called only under content's mutation lock; revocation wins over a held actor.
pub async fn current_draft_editor(app: &App, session: &Session) -> Result<()> {
    let hash = session
        .hash
        .strip_prefix("integration:")
        .ok_or_else(Error::forbidden)?;
    let (current, _) = lookup(app, hash, true).await?;
    if current.user.id != session.user.id || current.user.role != session.user.role {
        return Err(Error::forbidden());
    }
    Ok(())
}
pub fn routes() -> Router<App> {
    Router::new()
        .route("/api/v1/content", get(list).post(create))
        .route("/api/v1/content/{id}", get(detail).put(update))
        .route(
            "/api/v1/content/{id}/translation",
            get(translation_manifest),
        )
        .route(
            "/api/v1/translations",
            axum::routing::post(translation_apply),
        )
        .layer(DefaultBodyLimit::max(1024 * 1024))
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    #[serde(default)]
    after: String,
}
async fn list(
    State(app): State<App>,
    headers: HeaderMap,
    Query(cursor): Query<Cursor>,
) -> Result<Json<Value>> {
    authenticate(&app, &headers, false).await?;
    if !cursor.after.is_empty() {
        uuid::Uuid::parse_str(&cursor.after)
            .map_err(|_| Error::invalid("Use the last content UUID as the cursor."))?;
    }
    let rows=sqlx::query("SELECT id,title,slug,kind,status,version,updated_at FROM posts WHERE id>$1 ORDER BY id LIMIT 26").bind(&cursor.after).fetch_all(&app.db.pool).await?;
    let more = rows.len() > 25;
    let records:Vec<_>=rows.into_iter().take(25).map(|r|json!({"id":r.get::<String,_>("id"),"title":r.get::<String,_>("title"),"slug":r.get::<String,_>("slug"),"kind":r.get::<String,_>("kind"),"status":r.get::<String,_>("status"),"version":r.get::<i64,_>("version"),"updated_at":r.get::<i64,_>("updated_at")})).collect();
    Ok(Json(
        json!({"api_version":1,"next":if more {records.last().map(|r|r["id"].clone())}else{None},"content":records}),
    ))
}
async fn detail(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    authenticate(&app, &headers, false).await?;
    uuid::Uuid::parse_str(&id).map_err(|_| Error::not_found())?;
    let post = crate::content::get(&app, &id).await?;
    Ok(Json(json!({"api_version":1,"content":post})))
}
async fn write(
    app: &App,
    headers: &HeaderMap,
    id: Option<&str>,
    input: crate::model::PostInput,
) -> Result<Json<Value>> {
    let actor = authenticate(app, headers, true).await?;
    if input.action != "save" || input.publish_at != 0 {
        return Err(Error::forbidden());
    }
    let post = crate::content::save(app, &actor, id, input).await?;
    Ok(Json(
        json!({"api_version":1,"id":post.id,"version":post.version,"status":post.status}),
    ))
}
async fn create(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<crate::model::PostInput>,
) -> Result<Json<Value>> {
    write(&app, &headers, None, input).await
}
async fn update(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<crate::model::PostInput>,
) -> Result<Json<Value>> {
    uuid::Uuid::parse_str(&id).map_err(|_| Error::not_found())?;
    write(&app, &headers, Some(&id), input).await
}

async fn translation_manifest(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    authenticate(&app, &headers, false).await?;
    uuid::Uuid::parse_str(&id).map_err(|_| Error::not_found())?;
    Ok(Json(super::document_translation::manifest(
        &crate::content::get(&app, &id).await?,
    )?))
}
async fn translation_apply(
    State(app): State<App>,
    headers: HeaderMap,
    Json(proposal): Json<super::document_translation::Proposal>,
) -> Result<Json<Value>> {
    let actor = authenticate(&app, &headers, true).await?;
    let post = super::document_translation::apply(&app, &actor, proposal).await?;
    Ok(Json(
        json!({"api_version":1,"id":post.id,"status":post.status,"version":post.version}),
    ))
}
