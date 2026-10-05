//! Passkeys validated by webauthn-rs. Challenges never leave server state and are
//! single-use, time-bounded and tied to the account/session that created them.
use crate::{
    App, auth,
    error::{Error, Result},
    model::{Session, User},
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::BTreeMap;
use webauthn_rs::prelude::*;

pub const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS user_passkeys(credential_id TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id),definition TEXT NOT NULL,version BIGINT NOT NULL DEFAULT 1); CREATE INDEX IF NOT EXISTS user_passkeys_owner ON user_passkeys(user_id);";
#[derive(Serialize, Deserialize)]
pub enum Ceremony {
    Register {
        user: String,
        session: String,
        hash: String,
        expires: i64,
        state: PasskeyRegistration,
    },
    Authenticate {
        user: String,
        hash: String,
        expires: i64,
        state: PasskeyAuthentication,
        versions: BTreeMap<String, i64>,
    },
}
impl Ceremony {
    fn expires(&self) -> i64 {
        match self {
            Self::Register { expires, .. } | Self::Authenticate { expires, .. } => *expires,
        }
    }
}
#[derive(Default, Serialize, Deserialize)]
pub struct Ceremonies {
    entries: BTreeMap<String, Ceremony>,
}
impl Ceremonies {
    fn insert(&mut self, value: Ceremony) -> Result<String> {
        self.entries.retain(|_, v| v.expires() > crate::now());
        if self.entries.len() >= 256 {
            return Err(Error::invalid(
                "Passkey challenges are busy; retry after expiry.",
            ));
        }
        let id = auth::random_token();
        self.entries.insert(id.clone(), value);
        Ok(id)
    }
    fn take(&mut self, id: &str) -> Result<Ceremony> {
        self.entries
            .remove(id)
            .filter(|v| v.expires() > crate::now())
            .ok_or_else(Error::forbidden)
    }
}
fn engine(app: &App) -> Result<Webauthn> {
    let origin =
        Url::parse(&app.config.base_url).map_err(|_| Error::invalid("Invalid passkey origin."))?;
    let host = origin.host_str().ok_or_else(Error::forbidden)?;
    if origin.scheme() != "https" && !matches!(host, "localhost" | "127.0.0.1" | "[::1]") {
        return Err(Error::invalid("Passkeys require HTTPS outside localhost."));
    }
    WebauthnBuilder::new(host, &origin)
        .map_err(|_| Error::invalid("Unsupported passkey relying-party origin."))?
        .rp_name("wpalt")
        .build()
        .map_err(|_| Error::invalid("Cannot initialize passkeys."))
}
async fn credentials(app: &App, user: &str) -> Result<Vec<Passkey>> {
    let rows = sqlx::query(
        "SELECT definition FROM user_passkeys WHERE user_id=$1 ORDER BY credential_id LIMIT 9",
    )
    .bind(user)
    .fetch_all(&app.db.pool)
    .await?;
    if rows.len() > 8 {
        return Err(Error::invalid("Account passkey budget exceeded."));
    }
    rows.iter()
        .map(|r| {
            serde_json::from_str(&r.get::<String, _>("definition"))
                .map_err(|_| Error::invalid("Invalid stored passkey."))
        })
        .collect()
}
#[derive(Serialize)]
pub struct Challenge {
    pub id: String,
    pub options: serde_json::Value,
}
pub async fn register_start(
    app: &App,
    s: &Session,
    password: &str,
    code: &str,
) -> Result<Challenge> {
    let hash = super::factor::authorize_change(app, s, password, code).await?;
    let keys = credentials(app, &s.user.id).await?;
    if keys.len() >= 8 {
        return Err(Error::invalid("At most eight passkeys per account."));
    }
    let (options, state) = engine(app)?
        .start_passkey_registration(
            Uuid::parse_str(&s.user.id).map_err(|_| Error::forbidden())?,
            &s.user.email,
            &s.user.name,
            Some(keys.iter().map(|k| k.cred_id().clone()).collect()),
        )
        .map_err(|_| Error::invalid("Cannot begin passkey registration."))?;
    let id = app
        .passkey_ceremonies
        .lock()
        .await
        .insert(Ceremony::Register {
            user: s.user.id.clone(),
            session: s.hash.clone(),
            hash,
            expires: crate::now() + 300,
            state,
        })?;
    Ok(Challenge {
        id,
        options: serde_json::to_value(options)
            .map_err(|_| Error::invalid("Invalid passkey challenge."))?,
    })
}
#[derive(Deserialize)]
pub struct Registration {
    pub id: String,
    pub credential: RegisterPublicKeyCredential,
}
pub async fn register_finish(app: &App, s: &Session, input: Registration) -> Result<()> {
    let ceremony = app.passkey_ceremonies.lock().await.take(&input.id)?;
    let Ceremony::Register {
        user,
        session,
        hash,
        state,
        ..
    } = ceremony
    else {
        return Err(Error::forbidden());
    };
    if user != s.user.id || session != s.hash {
        return Err(Error::forbidden());
    }
    let passkey = engine(app)?
        .finish_passkey_registration(&input.credential, &state)
        .map_err(|_| Error::forbidden())?;
    let _guard = app.mutation().await?;
    super::factor::live_session(app, s).await?;
    let current: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id=$1 AND role<>'disabled'")
            .bind(&user)
            .fetch_optional(&app.db.pool)
            .await?;
    if current.as_deref() != Some(hash.as_str()) {
        return Err(Error::forbidden());
    }
    if credentials(app, &user).await?.len() >= 8 {
        return Err(Error::invalid("Account passkey budget exceeded."));
    }
    sqlx::query("INSERT INTO user_passkeys(credential_id,user_id,definition) VALUES($1,$2,$3)")
        .bind(hex::encode(passkey.cred_id().as_ref()))
        .bind(user)
        .bind(serde_json::to_string(&passkey).map_err(|_| Error::invalid("Invalid passkey."))?)
        .execute(&app.db.pool)
        .await?;
    Ok(())
}
pub async fn authenticate_start(app: &App, email: &str) -> Result<Challenge> {
    if email.len() > 254 {
        return Err(Error::forbidden());
    }
    let email = email.to_ascii_lowercase();
    let limit = auth::digest(email.as_bytes());
    app.login_limits.lock().await.check(&limit, crate::now())?;
    app.login_limits.lock().await.failed(limit, crate::now());
    let _read_guard = app.mutations.lock().await;
    let row = sqlx::query("SELECT id,password_hash FROM users WHERE email=$1 AND role<>'disabled'")
        .bind(email)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::forbidden)?;
    let user: String = row.get("id");
    let keys = credentials(app, &user).await?;
    if keys.is_empty() {
        return Err(Error::forbidden());
    }
    let versions:BTreeMap<String,i64>=sqlx::query("SELECT credential_id,version FROM user_passkeys WHERE user_id=$1 ORDER BY credential_id LIMIT 8").bind(&user).fetch_all(&app.db.pool).await?.iter().map(|r|(r.get("credential_id"),r.get("version"))).collect();
    let (options, state) = engine(app)?
        .start_passkey_authentication(&keys)
        .map_err(|_| Error::forbidden())?;
    let id = app
        .passkey_ceremonies
        .lock()
        .await
        .insert(Ceremony::Authenticate {
            user,
            hash: row.get("password_hash"),
            expires: crate::now() + 300,
            state,
            versions,
        })?;
    Ok(Challenge {
        id,
        options: serde_json::to_value(options)
            .map_err(|_| Error::invalid("Invalid passkey challenge."))?,
    })
}
#[derive(Deserialize)]
pub struct Authentication {
    pub id: String,
    pub credential: PublicKeyCredential,
}
pub async fn authenticate_finish(app: &App, input: Authentication) -> Result<(String, Session)> {
    let ceremony = app.passkey_ceremonies.lock().await.take(&input.id)?;
    let Ceremony::Authenticate {
        user,
        hash,
        state,
        versions,
        ..
    } = ceremony
    else {
        return Err(Error::forbidden());
    };
    let result = engine(app)?
        .finish_passkey_authentication(&input.credential, &state)
        .map_err(|_| Error::forbidden())?;
    let _guard = app.mutation().await?;
    let mut tx = app.db.pool.begin().await?;
    let account=sqlx::query("SELECT id,email,name,role FROM users WHERE id=$1 AND password_hash=$2 AND role<>'disabled'").bind(&user).bind(hash).fetch_optional(&mut *tx).await?.ok_or_else(Error::forbidden)?;
    let id = hex::encode(result.cred_id().as_ref());
    let version: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM user_passkeys WHERE credential_id=$1 AND user_id=$2",
    )
    .bind(&id)
    .bind(&user)
    .fetch_optional(&mut *tx)
    .await?;
    if version != versions.get(&id).copied() {
        return Err(Error::forbidden());
    }
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT definition FROM user_passkeys WHERE credential_id=$1 AND user_id=$2",
    )
    .bind(&id)
    .bind(&user)
    .fetch_optional(&mut *tx)
    .await?;
    let mut passkey: Passkey =
        serde_json::from_str(&raw.ok_or_else(Error::forbidden)?).map_err(|_| Error::forbidden())?;
    // webauthn-rs checks the authenticator counter and UV signature at finish.
    // Persist its returned credential updates inside the same session transaction.
    passkey
        .update_credential(&result)
        .ok_or_else(Error::forbidden)?;
    sqlx::query("UPDATE user_passkeys SET definition=$2,version=version+1 WHERE credential_id=$1")
        .bind(id)
        .bind(serde_json::to_string(&passkey).map_err(|_| Error::forbidden())?)
        .execute(&mut *tx)
        .await?;
    let token = auth::random_token();
    let s = Session {
        user: User {
            id: user,
            email: account.get("email"),
            name: account.get("name"),
            role: account.get("role"),
        },
        csrf: auth::random_token(),
        hash: auth::digest(token.as_bytes()),
    };
    sqlx::query("INSERT INTO sessions(token_hash,user_id,csrf,expires_at) VALUES($1,$2,$3,$4)")
        .bind(&s.hash)
        .bind(&s.user.id)
        .bind(&s.csrf)
        .bind(crate::now() + app.config.session_seconds)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    app.login_limits
        .lock()
        .await
        .failures
        .remove(&auth::digest(s.user.email.as_bytes()));
    Ok((token, s))
}
