use crate::{
    App,
    error::{Error, Result},
    model::{Session, Settings, User},
    now,
};
use argon2::{
    Argon2, PasswordHasher, PasswordVerifier,
    password_hash::{PasswordHash, SaltString, rand_core::OsRng},
};
use axum::http::{HeaderMap, StatusCode};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::{HashMap, VecDeque};

pub fn random_token() -> String {
    let mut b = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut b);
    hex::encode(b)
}
pub fn digest(value: &[u8]) -> String {
    hex::encode(Sha256::digest(value))
}
pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|_| anyhow::anyhow!("password hashing failed"))
}
pub fn valid_user(email: &str, name: &str, role: &str) -> Result<()> {
    if email.len() > 254
        || !email.is_ascii()
        || email.matches('@').count() != 1
        || email.split('@').any(str::is_empty)
        || email.chars().any(char::is_whitespace)
        || name.trim().is_empty()
        || name.len() > 100
        || !["admin", "editor", "moderator", "disabled"].contains(&role)
    {
        return Err(Error::invalid(
            "Use a valid email, a name up to 100 characters and a supported role.",
        ));
    }
    Ok(())
}
pub async fn initialize(app: &App, email: &str, name: &str, password: &str) -> anyhow::Result<()> {
    valid_user(email, name, "admin").map_err(|e| anyhow::anyhow!(e.1))?;
    anyhow::ensure!(
        (12..=256).contains(&password.len()),
        "password must be 12..256 bytes"
    );
    let password = password.to_string();
    let hash = tokio::task::spawn_blocking(move || hash_password(&password)).await??;
    let _guard = app.mutations.lock().await;
    let mut tx = app.db.pool.begin().await?;
    let s = Settings::default();
    sqlx::query("INSERT INTO settings(id,title,description,theme,navigation,field_schema) VALUES(1,$1,$2,$3,$4,$5)").bind(s.title).bind(s.description).bind(s.theme).bind(s.navigation).bind(s.field_schema).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO users(id,email,name,role,password_hash,created_at) VALUES($1,$2,$3,'admin',$4,$5)").bind(uuid::Uuid::new_v4().to_string()).bind(email.to_ascii_lowercase()).bind(name.trim()).bind(hash).bind(now()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn add_user(
    app: &App,
    email: &str,
    name: &str,
    role: &str,
    password: &str,
) -> anyhow::Result<()> {
    valid_user(email, name, role).map_err(|e| anyhow::anyhow!(e.1))?;
    anyhow::ensure!(
        (12..=256).contains(&password.len()),
        "password must be 12..256 bytes"
    );
    let password = password.to_string();
    let hash = tokio::task::spawn_blocking(move || hash_password(&password)).await??;
    let _guard = app.mutations.lock().await;
    sqlx::query(
        "INSERT INTO users(id,email,name,role,password_hash,created_at) VALUES($1,$2,$3,$4,$5,$6)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(email.to_ascii_lowercase())
    .bind(name.trim())
    .bind(role)
    .bind(hash)
    .bind(now())
    .execute(&app.db.pool)
    .await?;
    Ok(())
}
#[derive(Default)]
pub struct LoginLimits {
    global: VecDeque<i64>,
    failures: HashMap<String, (i64, u32)>,
}
impl LoginLimits {
    fn check(&mut self, email: &str, time: i64) -> Result<()> {
        self.global.retain(|t| *t > time - 60);
        self.failures.retain(|_, (t, _)| *t > time - 900);
        if self.global.len() >= 30
            || self
                .failures
                .get(email)
                .is_some_and(|(_, count)| *count >= 8)
        {
            return Err(Error(
                StatusCode::TOO_MANY_REQUESTS,
                "Login is temporarily rate limited. Try again later.",
            ));
        }
        self.global.push_back(time);
        Ok(())
    }
    fn failed(&mut self, email: String, time: i64) {
        if self.failures.len() < 1024 || self.failures.contains_key(&email) {
            let entry = self.failures.entry(email).or_insert((time, 0));
            entry.1 += 1;
        }
    }
}
pub async fn login(app: &App, email: &str, password: &str) -> Result<(String, Session)> {
    let email = email.to_ascii_lowercase();
    if email.len() > 254 || password.len() > 256 {
        return Err(Error(
            StatusCode::UNAUTHORIZED,
            "Email or password is incorrect.",
        ));
    }
    let key = digest(email.as_bytes());
    app.login_limits.lock().await.check(&key, now())?;
    let row = sqlx::query("SELECT id,email,name,role,password_hash FROM users WHERE email=$1")
        .bind(&email)
        .fetch_optional(&app.db.pool)
        .await?;
    let hash: String = row
        .as_ref()
        .map(|r| r.get("password_hash"))
        .unwrap_or_else(|| app.dummy_hash.as_ref().clone());
    let password = password.to_string();
    let permit = app
        .password_work
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| {
            Error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Password worker unavailable.",
            )
        })?;
    let verified = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        PasswordHash::new(&hash).is_ok_and(|h| {
            Argon2::default()
                .verify_password(password.as_bytes(), &h)
                .is_ok()
        })
    })
    .await
    .unwrap_or(false);
    if !verified
        || row.is_none()
        || row
            .as_ref()
            .is_some_and(|r| r.get::<String, _>("role") == "disabled")
    {
        app.login_limits.lock().await.failed(key, now());
        tracing::warn!(event = "login_failed");
        return Err(Error(
            StatusCode::UNAUTHORIZED,
            "Email or password is incorrect.",
        ));
    }
    app.login_limits.lock().await.failures.remove(&key);
    let r = row.unwrap();
    let token = random_token();
    let session = Session {
        user: User {
            id: r.get("id"),
            email: r.get("email"),
            name: r.get("name"),
            role: r.get("role"),
        },
        csrf: random_token(),
        hash: digest(token.as_bytes()),
    };
    let _guard = app.mutations.lock().await;
    sqlx::query("DELETE FROM sessions WHERE expires_at<$1")
        .bind(now())
        .execute(&app.db.pool)
        .await?;
    sqlx::query("INSERT INTO sessions(token_hash,user_id,csrf,expires_at) VALUES($1,$2,$3,$4)")
        .bind(&session.hash)
        .bind(&session.user.id)
        .bind(&session.csrf)
        .bind(now() + app.config.session_seconds)
        .execute(&app.db.pool)
        .await?;
    tracing::info!(event = "login_succeeded");
    Ok((token, session))
}
pub async fn session(app: &App, headers: &HeaderMap) -> Result<Session> {
    let token = headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.split(';')
                .map(str::trim)
                .find_map(|part| part.strip_prefix("wpalt_session="))
        })
        .filter(|v| v.len() == 64 && v.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or(Error(StatusCode::UNAUTHORIZED, "Sign in to continue."))?;
    let hash = digest(token.as_bytes());
    let r=sqlx::query("SELECT u.id,u.email,u.name,u.role,s.csrf FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=$1 AND s.expires_at>$2 AND u.role<>'disabled'").bind(&hash).bind(now()).fetch_optional(&app.db.pool).await?.ok_or(Error(StatusCode::UNAUTHORIZED,"Your session expired. Sign in again."))?;
    Ok(Session {
        user: User {
            id: r.get("id"),
            email: r.get("email"),
            name: r.get("name"),
            role: r.get("role"),
        },
        csrf: r.get("csrf"),
        hash,
    })
}
pub fn csrf(session: &Session, value: &str) -> Result<()> {
    let a = session.csrf.as_bytes();
    let b = value.as_bytes();
    if a.len() != b.len() || a.iter().zip(b).fold(0_u8, |acc, (a, b)| acc | (a ^ b)) != 0 {
        return Err(Error::forbidden());
    }
    Ok(())
}
pub fn same_origin(app: &App, headers: &HeaderMap) -> Result<()> {
    if headers.get("origin").and_then(|v| v.to_str().ok()) != Some(app.config.origin().as_str()) {
        return Err(Error::forbidden());
    }
    Ok(())
}
pub fn cookie(app: &App, token: &str) -> String {
    format!(
        "wpalt_session={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        app.config.session_seconds,
        if app.config.secure_cookie() {
            "; Secure"
        } else {
            ""
        }
    )
}

pub async fn update_user(
    app: &App,
    id: &str,
    name: &str,
    role: &str,
    password: &str,
) -> Result<()> {
    if name.trim().is_empty()
        || name.len() > 100
        || !["admin", "editor", "moderator", "disabled"].contains(&role)
        || (!password.is_empty() && !(12..=256).contains(&password.len()))
    {
        return Err(Error::invalid("Check name, role and password length."));
    }
    let hash = if password.is_empty() {
        String::new()
    } else {
        let password = password.to_owned();
        let permit = app
            .password_work
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| Error::invalid("Password worker unavailable."))?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            hash_password(&password)
        })
        .await
        .map_err(|_| Error::invalid("Password worker failed."))?
        .map_err(|_| Error::invalid("Password hash failed."))?
    };
    let _guard = app.mutations.lock().await;
    let mut tx = app.db.pool.begin().await?;
    let current: String = sqlx::query_scalar("SELECT role FROM users WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(Error::not_found)?;
    if current == "admin" && role != "admin" {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role='admin'")
            .fetch_one(&mut *tx)
            .await?;
        if n <= 1 {
            return Err(Error::invalid(
                "The last administrator cannot be disabled or demoted.",
            ));
        }
    }
    sqlx::query("UPDATE users SET name=$1,role=$2,password_hash=CASE WHEN $3='' THEN password_hash ELSE $3 END WHERE id=$4").bind(name.trim()).bind(role).bind(hash).bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM sessions WHERE user_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!(event = "account_access_updated", sessions_revoked = true);
    Ok(())
}
