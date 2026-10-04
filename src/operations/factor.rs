//! Local TOTP with transactional replay prevention and one-use recovery codes.
use crate::{
    App, auth,
    error::{Error, Result},
    model::Session,
};
use rand::RngCore;
use ring::hmac;
use sqlx::{Any, Row, Transaction};

pub const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS user_factors(user_id TEXT PRIMARY KEY REFERENCES users(id),secret TEXT NOT NULL DEFAULT '',pending TEXT NOT NULL DEFAULT '',pending_until BIGINT NOT NULL DEFAULT 0,last_step BIGINT NOT NULL DEFAULT -1,recovery TEXT NOT NULL DEFAULT '[]');";

pub fn code(secret: &[u8], step: i64) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, secret);
    let tag = hmac::sign(&key, &(step as u64).to_be_bytes());
    let bytes = tag.as_ref();
    let offset = (bytes[19] & 15) as usize;
    let number = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) & 0x7fffffff;
    format!("{:06}", number % 1_000_000)
}
fn equal(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.as_bytes()
            .iter()
            .zip(b.as_bytes())
            .fold(0u8, |x, (a, b)| x | (a ^ b))
            == 0
}
fn step(secret: &str, submitted: &str, timestamp: i64, last: i64) -> Result<i64> {
    let secret = hex::decode(secret).map_err(|_| Error::invalid("Invalid authenticator state."))?;
    if secret.len() != 20 || submitted.len() != 6 || !submitted.bytes().all(|c| c.is_ascii_digit())
    {
        return Err(Error::forbidden());
    }
    let current = timestamp / 30;
    for candidate in [current, current - 1, current + 1] {
        if candidate > last && equal(&code(&secret, candidate), submitted) {
            return Ok(candidate);
        }
    }
    Err(Error::forbidden())
}
fn base32(bytes: &[u8]) -> String {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut output = String::new();
    let mut bits = 0u32;
    let mut count = 0u32;
    for byte in bytes {
        bits = (bits << 8) | *byte as u32;
        count += 8;
        while count >= 5 {
            count -= 5;
            output.push(alphabet[((bits >> count) & 31) as usize] as char);
        }
    }
    if count > 0 {
        output.push(alphabet[((bits << (5 - count)) & 31) as usize] as char);
    }
    output
}
pub async fn begin(app: &App, session: &Session, password: &str) -> Result<String> {
    let verified_hash = reauthenticate(app, session, password).await?;
    let _guard = app.mutation().await;
    current_credential(app, session, &verified_hash).await?;
    let existing: Option<String> =
        sqlx::query_scalar("SELECT secret FROM user_factors WHERE user_id=$1")
            .bind(&session.user.id)
            .fetch_optional(&app.db.pool)
            .await?;
    if existing.is_some_and(|s| !s.is_empty()) {
        return Err(Error::invalid(
            "Authenticator already enabled; disable with a current code first.",
        ));
    }
    let mut key = [0u8; 20];
    rand::rngs::OsRng.fill_bytes(&mut key);
    sqlx::query("INSERT INTO user_factors(user_id,pending,pending_until) VALUES($1,$2,$3) ON CONFLICT(user_id) DO UPDATE SET pending=excluded.pending,pending_until=excluded.pending_until").bind(&session.user.id).bind(hex::encode(key)).bind(crate::now()+600).execute(&app.db.pool).await?;
    Ok(format!(
        "otpauth://totp/wpalt:{}?secret={}&issuer=wpalt&algorithm=SHA1&digits=6&period=30",
        url::form_urlencoded::byte_serialize(session.user.email.as_bytes()).collect::<String>(),
        base32(&key)
    ))
}
pub async fn confirm(app: &App, session: &Session, submitted: &str) -> Result<Vec<String>> {
    let _guard = app.mutation().await;
    live_session(app, session).await?;
    let mut tx = app.db.pool.begin().await?;
    let row = sqlx::query("SELECT pending,pending_until,secret FROM user_factors WHERE user_id=$1")
        .bind(&session.user.id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(Error::forbidden)?;
    if row.get::<i64, _>("pending_until") < crate::now()
        || !row.get::<String, _>("secret").is_empty()
    {
        return Err(Error::forbidden());
    }
    let pending: String = row.get("pending");
    let used = step(&pending, submitted, crate::now(), -1)?;
    let codes: Vec<String> = (0..8)
        .map(|_| auth::random_token()[..24].to_owned())
        .collect();
    let hashes: Vec<String> = codes.iter().map(|s| auth::digest(s.as_bytes())).collect();
    sqlx::query("UPDATE user_factors SET secret=pending,pending='',pending_until=0,last_step=$2,recovery=$3 WHERE user_id=$1").bind(&session.user.id).bind(used).bind(serde_json::to_string(&hashes).unwrap()).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM sessions WHERE user_id=$1 AND token_hash<>$2")
        .bind(&session.user.id)
        .bind(&session.hash)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(codes)
}
pub async fn verify(tx: &mut Transaction<'_, Any>, user: &str, submitted: &str) -> Result<()> {
    let row = sqlx::query("SELECT secret,last_step,recovery FROM user_factors WHERE user_id=$1")
        .bind(user)
        .fetch_optional(&mut **tx)
        .await?;
    let Some(row) = row else {
        return Ok(());
    };
    let secret: String = row.get("secret");
    if secret.is_empty() {
        return Ok(());
    }
    if let Ok(used) = step(&secret, submitted, crate::now(), row.get("last_step")) {
        let updated = sqlx::query(
            "UPDATE user_factors SET last_step=$2 WHERE user_id=$1 AND secret=$3 AND last_step<$2",
        )
        .bind(user)
        .bind(used)
        .bind(secret)
        .execute(&mut **tx)
        .await?;
        if updated.rows_affected() == 1 {
            return Ok(());
        }
    } else if submitted.len() == 24 {
        let raw: String = row.get("recovery");
        let mut recovery: Vec<String> =
            serde_json::from_str(&raw).map_err(|_| Error::forbidden())?;
        let digest = auth::digest(submitted.as_bytes());
        if let Some(index) = recovery.iter().position(|s| equal(s, &digest)) {
            recovery.remove(index);
            let updated =
                sqlx::query("UPDATE user_factors SET recovery=$2 WHERE user_id=$1 AND recovery=$3")
                    .bind(user)
                    .bind(serde_json::to_string(&recovery).unwrap())
                    .bind(raw)
                    .execute(&mut **tx)
                    .await?;
            if updated.rows_affected() == 1 {
                return Ok(());
            }
        }
    }
    Err(Error(
        axum::http::StatusCode::UNAUTHORIZED,
        "Sign-in failed. Check credentials and authenticator or recovery code.",
    ))
}
pub async fn disable(app: &App, session: &Session, password: &str, submitted: &str) -> Result<()> {
    let verified_hash = reauthenticate(app, session, password).await?;
    let _guard = app.mutation().await;
    current_credential(app, session, &verified_hash).await?;
    let mut tx = app.db.pool.begin().await?;
    verify(&mut tx, &session.user.id, submitted).await?;
    sqlx::query("DELETE FROM user_factors WHERE user_id=$1")
        .bind(&session.user.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id=$1 AND token_hash<>$2")
        .bind(&session.user.id)
        .bind(&session.hash)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
pub(crate) async fn reauthenticate(app: &App, session: &Session, password: &str) -> Result<String> {
    use argon2::{Argon2, PasswordVerifier, password_hash::PasswordHash};
    let limit = auth::digest(format!("reauth:{}", session.user.id).as_bytes());
    app.login_limits.lock().await.check(&limit, crate::now())?;
    if password.len() > 256 {
        return Err(Error::forbidden());
    }
    let hash: String =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id=$1 AND role<>'disabled'")
            .bind(&session.user.id)
            .fetch_optional(&app.db.pool)
            .await?
            .ok_or_else(Error::forbidden)?;
    let verified_hash = hash.clone();
    let password = password.to_owned();
    let permit = app
        .password_work
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::invalid("Password workers busy."))?;
    let valid = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        auth::supported_password_hash(&hash)
            && PasswordHash::new(&hash).is_ok_and(|h| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &h)
                    .is_ok()
            })
    })
    .await
    .unwrap_or(false);
    if valid {
        app.login_limits.lock().await.failures.remove(&limit);
        Ok(verified_hash)
    } else {
        app.login_limits.lock().await.failed(limit, crate::now());
        Err(Error::forbidden())
    }
}

pub(crate) async fn live_session(app: &App, session: &Session) -> Result<()> {
    let live: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=$1 AND s.user_id=$2 AND s.expires_at>$3 AND u.role=$4 AND u.role<>'disabled'").bind(&session.hash).bind(&session.user.id).bind(crate::now()).bind(&session.user.role).fetch_one(&app.db.pool).await?;
    if live == 1 {
        Ok(())
    } else {
        Err(Error::forbidden())
    }
}

pub(crate) async fn current_credential(app: &App, session: &Session, hash: &str) -> Result<()> {
    live_session(app, session).await?;
    let current: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id=$1 AND role=$2")
            .bind(&session.user.id)
            .bind(&session.user.role)
            .fetch_optional(&app.db.pool)
            .await?;
    if current.as_deref() == Some(hash) {
        Ok(())
    } else {
        Err(Error::forbidden())
    }
}

pub async fn authorize_change(
    app: &App,
    s: &Session,
    password: &str,
    code: &str,
) -> Result<String> {
    let hash = reauthenticate(app, s, password).await?;
    let _guard = app.mutation().await;
    current_credential(app, s, &hash).await?;
    let mut tx = app.db.pool.begin().await?;
    verify(&mut tx, &s.user.id, code).await?;
    tx.commit().await?;
    Ok(hash)
}
