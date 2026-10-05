//! Optional OIDC authorization-code/PKCE connector. No email-based account linking.
use crate::{
    App, auth,
    error::{Error, Result},
    model::{Session, User},
    now,
};
use axum::{
    Router,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub issuer: String,
    pub authorization_url: String,
    pub token_url: String,
    pub jwks_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub ca_cert_file: String,
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        anyhow::ensure!(
            !self.client_id.is_empty() && self.client_id.len() <= 256,
            "OIDC needs a bounded client ID"
        );
        anyhow::ensure!(
            self.client_secret.len() <= 4096 && self.ca_cert_file.len() <= 4096,
            "OIDC secret and CA path must be bounded"
        );
        if !self.ca_cert_file.is_empty() {
            ca_certificate(&self.ca_cert_file)?;
        }
        for raw in [
            &self.issuer,
            &self.authorization_url,
            &self.token_url,
            &self.jwks_url,
        ] {
            anyhow::ensure!(raw.len() <= 4096, "OIDC endpoint must be bounded");
            let u = url::Url::parse(raw)?;
            anyhow::ensure!(
                u.scheme() == "https"
                    && u.host_str().is_some()
                    && u.username().is_empty()
                    && u.password().is_none()
                    && u.fragment().is_none(),
                "OIDC endpoints must be owner-configured HTTPS URLs without credentials/fragments"
            );
        }
        Ok(())
    }
}
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS member_identities(issuer TEXT NOT NULL,subject TEXT NOT NULL,user_id TEXT NOT NULL REFERENCES users(id),PRIMARY KEY(issuer,subject));
CREATE TABLE IF NOT EXISTS identity_flows(state_hash TEXT PRIMARY KEY,browser_hash TEXT NOT NULL,nonce TEXT NOT NULL,verifier TEXT NOT NULL,expires_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS identity_flow_expiry ON identity_flows(expires_at);
"#;
pub fn routes(app: &App) -> Router<App> {
    if !app.config.membership_enabled || !app.config.identity.enabled {
        return Router::new();
    }
    Router::new()
        .route("/members/identity/start", get(start))
        .route("/members/identity/callback", get(callback))
}
fn unavailable() -> Error {
    tracing::warn!(event = "identity_provider_unavailable");
    Error(
        StatusCode::SERVICE_UNAVAILABLE,
        "Identity provider unavailable. Use local sign-in or retry later.",
    )
}
fn ca_certificate(path: &str) -> anyhow::Result<reqwest::Certificate> {
    use std::io::Read;
    let metadata = std::fs::metadata(path)?;
    anyhow::ensure!(
        metadata.is_file() && metadata.len() <= 32 * 1024,
        "OIDC CA must be a regular PEM certificate file up to 32 KiB"
    );
    let file = std::fs::File::open(path)?;
    anyhow::ensure!(
        file.metadata()?.is_file(),
        "OIDC CA must be a regular certificate file"
    );
    let mut pem = Vec::new();
    file.take(32 * 1024 + 1).read_to_end(&mut pem)?;
    anyhow::ensure!(pem.len() <= 32 * 1024, "OIDC CA exceeds 32 KiB");
    Ok(reqwest::Certificate::from_pem(&pem)?)
}
fn client(config: &Config) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder();
    if !config.ca_cert_file.is_empty() {
        builder = builder
            .add_root_certificate(ca_certificate(&config.ca_cert_file).map_err(|_| unavailable())?);
    }
    builder
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(5))
        .https_only(true)
        .build()
        .map_err(|_| unavailable())
}
async fn bounded(mut response: reqwest::Response) -> Result<Vec<u8>> {
    if !response.status().is_success() {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if bytes.len() + chunk.len() > 256 * 1024 {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
async fn start(State(app): State<App>) -> Result<Response> {
    let state = auth::random_token();
    let browser = auth::random_token();
    let nonce = auth::random_token();
    let verifier = auth::random_token();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let _guard = app.mutation().await?;
    sqlx::query("DELETE FROM identity_flows WHERE expires_at<=$1")
        .bind(now())
        .execute(&app.db.pool)
        .await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM identity_flows")
        .fetch_one(&app.db.pool)
        .await?;
    if count >= 1000 {
        return Err(Error(
            StatusCode::TOO_MANY_REQUESTS,
            "Sign-in is busy. Retry shortly.",
        ));
    }
    sqlx::query("INSERT INTO identity_flows(state_hash,browser_hash,nonce,verifier,expires_at) VALUES($1,$2,$3,$4,$5)").bind(auth::digest(state.as_bytes())).bind(auth::digest(browser.as_bytes())).bind(&nonce).bind(verifier).bind(now()+300).execute(&app.db.pool).await?;
    let cfg = &app.config.identity;
    let mut url = url::Url::parse(&cfg.authorization_url).map_err(|_| unavailable())?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &cfg.client_id)
        .append_pair(
            "redirect_uri",
            &format!("{}/members/identity/callback", app.config.origin()),
        )
        .append_pair("scope", "openid")
        .append_pair("state", &state)
        .append_pair("nonce", &nonce)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256");
    let mut response = Redirect::to(url.as_str()).into_response();
    response.headers_mut().insert("set-cookie",HeaderValue::from_str(&format!("wpalt_identity={browser}; Path=/members/identity; Max-Age=300; HttpOnly; Secure; SameSite=Lax")).unwrap());
    Ok(response)
}
#[derive(Deserialize)]
struct Callback {
    #[serde(default)]
    code: String,
    #[serde(default)]
    state: String,
}
#[derive(Deserialize)]
struct Tokens {
    id_token: String,
    #[serde(default)]
    access_token: String,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct Claims {
    pub iss: String,
    pub sub: String,
    pub aud: serde_json::Value,
    pub exp: u64,
    pub iat: u64,
    pub nonce: String,
    #[serde(default)]
    pub at_hash: String,
    #[serde(default)]
    pub azp: String,
}
pub fn verify_token(
    cfg: &Config,
    token: &str,
    keys: &jsonwebtoken::jwk::JwkSet,
    nonce: &str,
    access: &str,
) -> Result<Claims> {
    if token.len() > 16384 || keys.keys.len() > 16 {
        return Err(Error::forbidden());
    }
    let header = jsonwebtoken::decode_header(token).map_err(|_| Error::forbidden())?;
    if header.alg != jsonwebtoken::Algorithm::RS256 {
        return Err(Error::forbidden());
    }
    let kid = header.kid.ok_or_else(Error::forbidden)?;
    if kid.len() > 200 {
        return Err(Error::forbidden());
    }
    let jwk = keys.find(&kid).ok_or_else(Error::forbidden)?;
    let key = jsonwebtoken::DecodingKey::from_jwk(jwk).map_err(|_| Error::forbidden())?;
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.set_issuer(&[&cfg.issuer]);
    validation.set_audience(&[&cfg.client_id]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    let claims = jsonwebtoken::decode::<Claims>(token, &key, &validation)
        .map_err(|_| Error::forbidden())?
        .claims;
    if claims.nonce != nonce
        || claims.sub.is_empty()
        || claims.sub.len() > 255
        || claims.iat > now() as u64 + 60
        || (!claims.azp.is_empty() && claims.azp != cfg.client_id)
        || (claims.aud.as_array().is_some_and(|a| a.len() > 1) && claims.azp != cfg.client_id)
    {
        return Err(Error::forbidden());
    }
    if !claims.at_hash.is_empty() {
        let hash = Sha256::digest(access.as_bytes());
        if access.is_empty() || claims.at_hash != URL_SAFE_NO_PAD.encode(&hash[..16]) {
            return Err(Error::forbidden());
        }
    }
    Ok(claims)
}
async fn callback(
    State(app): State<App>,
    h: HeaderMap,
    Query(i): Query<Callback>,
) -> Result<Response> {
    if i.state.len() != 64
        || !i.state.bytes().all(|b| b.is_ascii_hexdigit())
        || i.code.is_empty()
        || i.code.len() > 2048
    {
        return Err(Error::forbidden());
    }
    let browser = h
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.split(';')
                .map(str::trim)
                .find_map(|v| v.strip_prefix("wpalt_identity="))
        })
        .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(Error::forbidden)?;
    let flow=sqlx::query("DELETE FROM identity_flows WHERE state_hash=$1 AND browser_hash=$2 AND expires_at>$3 RETURNING nonce,verifier").bind(auth::digest(i.state.as_bytes())).bind(auth::digest(browser.as_bytes())).bind(now()).fetch_optional(&app.db.pool).await?.ok_or_else(Error::forbidden)?;
    let cfg = &app.config.identity;
    let http = client(cfg)?;
    let request = http.post(&cfg.token_url).form(&[
        ("grant_type", "authorization_code"),
        ("code", i.code.as_str()),
        (
            "redirect_uri",
            &format!("{}/members/identity/callback", app.config.origin()),
        ),
        ("client_id", cfg.client_id.as_str()),
        ("code_verifier", &flow.get::<String, _>("verifier")),
    ]);
    let request = if cfg.client_secret.is_empty() {
        request
    } else {
        // RFC 6749 client_secret_basic encodes each credential as a form value
        // before composing the Basic header, including ':' and '+' characters.
        let encode = |value: &str| {
            url::form_urlencoded::Serializer::new(String::new())
                .append_pair("", value)
                .finish()[1..]
                .to_owned()
        };
        request.basic_auth(encode(&cfg.client_id), Some(encode(&cfg.client_secret)))
    };
    let bytes = bounded(request.send().await.map_err(|_| unavailable())?).await?;
    let tokens: Tokens = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    let keys: jsonwebtoken::jwk::JwkSet = serde_json::from_slice(
        &bounded(
            http.get(&cfg.jwks_url)
                .send()
                .await
                .map_err(|_| unavailable())?,
        )
        .await?,
    )
    .map_err(|_| unavailable())?;
    let claims = verify_token(
        cfg,
        &tokens.id_token,
        &keys,
        &flow.get::<String, _>("nonce"),
        &tokens.access_token,
    )?;
    let (token, session) = identity_session(&app, &cfg.issuer, &claims.sub).await?;
    let mut response = Redirect::to(if session.user.role == "subscriber" {
        "/members"
    } else {
        "/admin"
    })
    .into_response();
    response.headers_mut().append(
        "set-cookie",
        HeaderValue::from_str(&auth::cookie(&app, &token)).unwrap(),
    );
    response.headers_mut().append(
        "set-cookie",
        HeaderValue::from_static(
            "wpalt_identity=; Path=/members/identity; Max-Age=0; HttpOnly; Secure; SameSite=Lax",
        ),
    );
    Ok(response)
}
pub async fn identity_session(app: &App, issuer: &str, subject: &str) -> Result<(String, Session)> {
    let _guard = app.mutation().await?;
    let r=sqlx::query("SELECT u.id,u.email,u.name,u.role FROM member_identities i JOIN users u ON u.id=i.user_id WHERE i.issuer=$1 AND i.subject=$2 AND u.role<>'disabled'").bind(issuer).bind(subject).fetch_optional(&app.db.pool).await?.ok_or_else(Error::forbidden)?;
    let token = auth::random_token();
    let session = Session {
        user: User {
            id: r.get("id"),
            email: r.get("email"),
            name: r.get("name"),
            role: r.get("role"),
        },
        csrf: auth::random_token(),
        hash: auth::digest(token.as_bytes()),
    };
    let factor: Option<String> =
        sqlx::query_scalar("SELECT secret FROM user_factors WHERE user_id=$1")
            .bind(&session.user.id)
            .fetch_optional(&app.db.pool)
            .await?;
    if factor.is_some_and(|s| !s.is_empty()) {
        return Err(Error::invalid(
            "This account requires local authenticator sign-in; use password and code.",
        ));
    }
    sqlx::query("INSERT INTO sessions(token_hash,user_id,csrf,expires_at) VALUES($1,$2,$3,$4)")
        .bind(&session.hash)
        .bind(&session.user.id)
        .bind(&session.csrf)
        .bind(now() + app.config.session_seconds)
        .execute(&app.db.pool)
        .await?;
    tracing::info!(event = "identity_login_succeeded");
    Ok((token, session))
}
