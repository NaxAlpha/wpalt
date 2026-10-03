//! Consent-gated, bounded first-party events and geometry-only interaction capture.
use crate::{
    App, auth,
    error::{Error, Result},
    now,
};
use axum::http::{HeaderMap, header};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::BTreeMap;
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub max_events: i64,
    pub max_sessions: i64,
    pub retention_days: i64,
    pub respect_dnt: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            max_events: 100000,
            max_sessions: 10000,
            retention_days: 30,
            respect_dnt: true,
        }
    }
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (100..=1000000).contains(&self.max_events)
                && (100..=100000).contains(&self.max_sessions)
                && (1..=90).contains(&self.retention_days),
            "Engagement limits: 100..1,000,000 events; 1..90 retention days."
        );
        Ok(())
    }
}
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS engagement_settings(id BIGINT PRIMARY KEY CHECK(id=1),enabled BIGINT NOT NULL DEFAULT 0,recording BIGINT NOT NULL DEFAULT 0,purpose TEXT NOT NULL DEFAULT 'Understand and improve this site using local interaction data.',version BIGINT NOT NULL DEFAULT 1);
INSERT INTO engagement_settings(id) VALUES(1) ON CONFLICT(id) DO NOTHING;
CREATE TABLE IF NOT EXISTS engagement_sessions(hash TEXT PRIMARY KEY,policy BIGINT NOT NULL,purpose TEXT NOT NULL,recording BIGINT NOT NULL DEFAULT 0,events BIGINT NOT NULL DEFAULT 0,frames BIGINT NOT NULL DEFAULT 0,expires_at BIGINT NOT NULL,created_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS engagement_session_age ON engagement_sessions(created_at,hash);
CREATE INDEX IF NOT EXISTS engagement_session_expiry ON engagement_sessions(expires_at);
CREATE TABLE IF NOT EXISTS engagement_event_names(name TEXT PRIMARY KEY,label TEXT NOT NULL);
INSERT INTO engagement_event_names(name,label) VALUES('pageview','Page view'),('form_submit','Accepted form response'),('offer_claim','Offer claimed'),('interaction','Masked interaction') ON CONFLICT(name) DO NOTHING;
CREATE TABLE IF NOT EXISTS engagement_events(id TEXT PRIMARY KEY,session_hash TEXT NOT NULL REFERENCES engagement_sessions(hash) ON DELETE CASCADE,path TEXT NOT NULL,name TEXT NOT NULL REFERENCES engagement_event_names(name),dimensions TEXT NOT NULL,frame TEXT NOT NULL,created_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS engagement_event_report ON engagement_events(created_at,name,path);
CREATE INDEX IF NOT EXISTS engagement_event_path ON engagement_events(path,created_at);
CREATE INDEX IF NOT EXISTS engagement_event_session ON engagement_events(session_hash,created_at,id);
CREATE TABLE IF NOT EXISTS engagement_usage(id BIGINT PRIMARY KEY CHECK(id=1),events BIGINT NOT NULL DEFAULT 0,sessions BIGINT NOT NULL DEFAULT 0);
INSERT INTO engagement_usage(id) VALUES(1) ON CONFLICT(id) DO NOTHING;
"#;
pub fn privacy_signal(app: &App, headers: &HeaderMap) -> bool {
    headers.get("sec-gpc").is_some_and(|v| v == "1")
        || (app.config.engagement.respect_dnt && headers.get("dnt").is_some_and(|v| v == "1"))
}
pub fn token(headers: &HeaderMap) -> Option<String> {
    let value = headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|pair| pair.trim().strip_prefix("wpalt_visitor="))?;
    if value.len() != 64 || !value.bytes().all(|v| v.is_ascii_hexdigit()) {
        return None;
    }
    Some(auth::digest(value.as_bytes()))
}
pub fn cookie(app: &App, value: &str, age: i64) -> String {
    format!(
        "wpalt_visitor={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={age}{}",
        if app.config.secure_cookie() {
            "; Secure"
        } else {
            ""
        }
    )
}
pub async fn consent(
    app: &App,
    headers: &HeaderMap,
    allow: bool,
    recording: bool,
    policy: i64,
) -> Result<Option<String>> {
    if !app.config.business_enabled || !app.config.engagement.enabled {
        return Err(Error::not_found());
    }
    if !allow || privacy_signal(app, headers) {
        if let Some(hash) = token(headers) {
            erase(app, &hash).await?;
        }
        return Ok(None);
    }
    let mut tx = app.db.pool.begin().await?;
    let row=sqlx::query("UPDATE engagement_settings SET enabled=enabled WHERE id=1 AND enabled=1 AND version=$1 RETURNING purpose,recording").bind(policy).fetch_optional(&mut *tx).await?.ok_or_else(Error::conflict)?;
    if let Some(old) = token(headers) {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM engagement_events WHERE session_hash=$1")
                .bind(&old)
                .fetch_one(&mut *tx)
                .await?;
        let removed = sqlx::query("DELETE FROM engagement_sessions WHERE hash=$1")
            .bind(old)
            .execute(&mut *tx)
            .await?;
        if removed.rows_affected() == 1 {
            sqlx::query(
                "UPDATE engagement_usage SET events=events-$1,sessions=sessions-1 WHERE id=1",
            )
            .bind(count)
            .execute(&mut *tx)
            .await?;
        }
    }
    if sqlx::query("UPDATE engagement_usage SET sessions=sessions+1 WHERE id=1 AND sessions<$1")
        .bind(app.config.engagement.max_sessions)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        != 1
    {
        return Err(Error::invalid(
            "This site's analytics consent storage has reached its configured limit.",
        ));
    }
    let value = auth::random_token();
    sqlx::query("INSERT INTO engagement_sessions(hash,policy,purpose,recording,expires_at,created_at) VALUES($1,$2,$3,$4,$5,$6)").bind(auth::digest(value.as_bytes())).bind(policy).bind(row.get::<String,_>("purpose")).bind(i64::from(recording && row.get::<i64,_>("recording")!=0)).bind(now()+app.config.engagement.retention_days*86400).bind(now()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Some(value))
}
pub async fn status(app: &App, headers: &HeaderMap) -> Result<Value> {
    let row =
        sqlx::query("SELECT enabled,recording,purpose,version FROM engagement_settings WHERE id=1")
            .fetch_one(&app.db.pool)
            .await?;
    let active = app.config.business_enabled
        && app.config.engagement.enabled
        && row.get::<i64, _>("enabled") == 1
        && !privacy_signal(app, headers);
    let session = if active {
        if let Some(hash) = token(headers) {
            sqlx::query("SELECT recording FROM engagement_sessions WHERE hash=$1 AND expires_at>$2 AND policy=$3").bind(hash).bind(now()).bind(row.get::<i64,_>("version")).fetch_optional(&app.db.pool).await?
        } else {
            None
        }
    } else {
        None
    };
    Ok(
        json!({"enabled":active,"purpose":row.get::<String,_>("purpose"),"policy":row.get::<i64,_>("version"),"recording_available":active && row.get::<i64,_>("recording")==1,"consented":session.is_some(),"recording":session.is_some_and(|r|r.get::<i64,_>("recording")==1)}),
    )
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rectangle {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub kind: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub scroll_y: u32,
    pub elapsed: u32,
    pub rectangles: Vec<Rectangle>,
    #[serde(default)]
    pub click: Option<[u32; 2]>,
}
impl Frame {
    pub fn validate(&self) -> Result<()> {
        if !(16..=4096).contains(&self.width)
            || !(16..=4096).contains(&self.height)
            || self.scroll_y > 100000
            || self.elapsed > 900
            || self.rectangles.len() > 200
            || self.click.is_some_and(|c| c[0] > 4096 || c[1] > 100000)
            || self.rectangles.iter().any(|r| {
                r.x > 4096
                    || r.y > 100000
                    || r.width > 4096
                    || r.height > 100000
                    || !["block", "image", "control"].contains(&r.kind.as_str())
            })
        {
            return Err(Error::invalid(
                "Interaction capture accepts bounded, masked geometry only.",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub id: String,
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub dimensions: BTreeMap<String, String>,
    #[serde(default)]
    pub frame: Option<Frame>,
}
pub async fn known_path(app: &App, path: &str) -> Result<bool> {
    if path.len() > 200
        || path.contains(['?', '#', '\\'])
        || !path.starts_with('/')
        || path.starts_with("/admin")
        || path.starts_with("/api")
        || path.starts_with("/audience")
        || path.starts_with("/login")
    {
        return Ok(false);
    }
    if ["/", "/search"].contains(&path) {
        return Ok(true);
    }
    if let Some(id) = path.strip_prefix("/forms/") {
        let found: Option<String> =
            sqlx::query_scalar("SELECT id FROM business_forms WHERE id=$1 AND published_version>0")
                .bind(id)
                .fetch_optional(&app.db.pool)
                .await?;
        return Ok(found.is_some());
    }
    let plain = path.trim_start_matches('/');
    let (locale, slug) = plain
        .split_once('/')
        .map_or((None, plain), |(l, s)| (Some(l), s));
    if slug.contains('/') {
        return Ok(false);
    }
    if slug.is_empty() || (locale.is_some() && slug == "search") {
        let (d, _) = crate::discovery::load(app).await?;
        return Ok(locale.is_some_and(|code| d.languages.iter().any(|l| l.code == code)));
    }
    let found: Option<String> = if let Some(locale) = locale {
        sqlx::query_scalar("SELECT id FROM posts WHERE status='published' AND published_slug=$1 AND published_locale=$2 LIMIT 1").bind(slug).bind(locale).fetch_optional(&app.db.pool).await?
    } else {
        sqlx::query_scalar(
            "SELECT id FROM posts WHERE status='published' AND published_slug=$1 LIMIT 1",
        )
        .bind(slug)
        .fetch_optional(&app.db.pool)
        .await?
    };
    Ok(found.is_some())
}
pub async fn capture(app: &App, headers: &HeaderMap, input: Capture) -> Result<bool> {
    capture_inner(app, headers, input, false).await
}
pub async fn conversion(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    path: &str,
    name: &str,
) -> Result<bool> {
    capture_inner(
        app,
        headers,
        Capture {
            id: id.into(),
            path: path.into(),
            name: name.into(),
            dimensions: BTreeMap::new(),
            frame: None,
        },
        true,
    )
    .await
}
async fn capture_inner(
    app: &App,
    headers: &HeaderMap,
    input: Capture,
    trusted: bool,
) -> Result<bool> {
    if !trusted && ["form_submit", "offer_claim"].contains(&input.name.as_str()) {
        return Err(Error::forbidden());
    }
    if !app.config.business_enabled
        || !app.config.engagement.enabled
        || privacy_signal(app, headers)
    {
        return Ok(false);
    }
    let Some(hash) = token(headers) else {
        return Ok(false);
    };
    if uuid::Uuid::parse_str(&input.id).is_err() || !known_path(app, &input.path).await? {
        return Err(Error::invalid(
            "Use a public page, a registered event and declared dimensions only.",
        ));
    }
    validate_dimensions(app, &input.dimensions).await?;
    if let Some(frame) = &input.frame {
        frame.validate()?;
    }
    let mut tx = app.db.pool.begin().await?;
    let policy: Option<i64> = sqlx::query_scalar(
        "UPDATE engagement_settings SET enabled=enabled WHERE id=1 AND enabled=1 RETURNING version",
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(policy) = policy else {
        return Ok(false);
    };
    let event: Option<String> =
        sqlx::query_scalar("SELECT name FROM engagement_event_names WHERE name=$1")
            .bind(&input.name)
            .fetch_optional(&mut *tx)
            .await?;
    if event.is_none() {
        return Err(Error::invalid("This event name is not registered."));
    }
    let allowed: Option<String> = sqlx::query_scalar(
        "SELECT hash FROM engagement_sessions WHERE hash=$1 AND policy=$2 AND expires_at>$3",
    )
    .bind(&hash)
    .bind(policy)
    .bind(now())
    .fetch_optional(&mut *tx)
    .await?;
    if allowed.is_none() {
        return Ok(false);
    }
    let duplicate: Option<String> =
        sqlx::query_scalar("SELECT session_hash FROM engagement_events WHERE id=$1")
            .bind(&input.id)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(owner) = duplicate {
        return if owner == hash {
            Ok(true)
        } else {
            Err(Error::conflict())
        };
    }
    let valid=sqlx::query("UPDATE engagement_sessions SET events=events+1,frames=frames+$1 WHERE hash=$2 AND policy=$3 AND expires_at>$4 AND events<500 AND ($1=0 OR (recording=1 AND frames<60))").bind(i64::from(input.frame.is_some())).bind(&hash).bind(policy).bind(now()).execute(&mut *tx).await?;
    if valid.rows_affected() != 1 {
        return Ok(false);
    }
    if sqlx::query("UPDATE engagement_usage SET events=events+1 WHERE id=1 AND events<$1")
        .bind(app.config.engagement.max_events)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        != 1
    {
        return Ok(false);
    }
    sqlx::query("INSERT INTO engagement_events(id,session_hash,path,name,dimensions,frame,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(&input.id).bind(hash).bind(input.path).bind(input.name).bind(serde_json::to_string(&input.dimensions).map_err(|_|Error::invalid("Invalid dimensions."))?).bind(input.frame.map(|f|serde_json::to_string(&f)).transpose().map_err(|_|Error::invalid("Invalid frame."))?.unwrap_or_default()).bind(now()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}
pub async fn erase(app: &App, hash: &str) -> Result<()> {
    let mut tx = app.db.pool.begin().await?;
    // Acquire write ownership before reads, including SQLite's first statement.
    let exists = sqlx::query("UPDATE engagement_sessions SET events=events WHERE hash=$1")
        .bind(hash)
        .execute(&mut *tx)
        .await?;
    if exists.rows_affected() == 0 {
        return Ok(());
    }
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM engagement_events WHERE session_hash=$1")
            .bind(hash)
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query("DELETE FROM engagement_sessions WHERE hash=$1")
        .bind(hash)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE engagement_usage SET events=events-$1,sessions=sessions-1 WHERE id=1")
        .bind(count)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
pub async fn cleanup(app: &App) -> Result<()> {
    if !app.config.business_enabled || !app.config.engagement.enabled {
        return Ok(());
    }
    let expired=sqlx::query("SELECT hash FROM engagement_sessions WHERE expires_at<=$1 OR created_at<$2 ORDER BY expires_at,hash LIMIT 50").bind(now()).bind(now()-app.config.engagement.retention_days*86400).fetch_all(&app.db.pool).await?;
    for row in expired {
        erase(app, &row.get::<String, _>("hash")).await?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct PublicState {
    pub purpose: String,
    pub policy: i64,
    pub recording: bool,
    pub respect_dnt: bool,
}
pub fn markup(settings: &crate::model::Settings, preview: bool) -> maud::Markup {
    if preview {
        return maud::html! {};
    }
    if let Some(state) = &settings.analytics {
        maud::html! {aside id="engagement-controls" class="privacy-controls" data-policy=(state.policy) data-recording=(state.recording) data-purpose=(&state.purpose) data-respect-dnt=(state.respect_dnt) {} script defer src="/assets/engagement.js" {}}
    } else {
        maud::html! {}
    }
}

pub const DIMENSION_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS engagement_dimension_values(name TEXT NOT NULL,value TEXT NOT NULL,PRIMARY KEY(name,value));
INSERT INTO engagement_dimension_values(name,value) VALUES('device','mobile'),('device','desktop'),('referrer','direct'),('referrer','same_site'),('referrer','external') ON CONFLICT(name,value) DO NOTHING;
"#;
pub async fn validate_dimensions(app: &App, dimensions: &BTreeMap<String, String>) -> Result<()> {
    if dimensions.len() > 4
        || dimensions.iter().any(|(key, value)| {
            !crate::schema::identifier(key) || !crate::schema::identifier(value)
        })
    {
        return Err(Error::invalid(
            "Use at most four declared category dimensions; free text is not accepted.",
        ));
    }
    for (name, value) in dimensions {
        let built_in = match name.as_str() {
            "device" => Some(["mobile", "desktop"].contains(&value.as_str())),
            "referrer" => Some(["direct", "same_site", "external"].contains(&value.as_str())),
            _ => None,
        };
        if built_in == Some(false) {
            return Err(Error::invalid("This category value is not declared."));
        }
        if built_in.is_none() {
            let exists: Option<String> = sqlx::query_scalar(
                "SELECT value FROM engagement_dimension_values WHERE name=$1 AND value=$2",
            )
            .bind(name)
            .bind(value)
            .fetch_optional(&app.db.pool)
            .await?;
            if exists.is_none() {
                return Err(Error::invalid("This category value is not declared."));
            }
        }
    }
    Ok(())
}
pub async fn catalog(app: &App, kind: &str, name: &str, label: &str, values: &str) -> Result<()> {
    if !crate::schema::identifier(name)
        || [
            "device",
            "referrer",
            "pageview",
            "form_submit",
            "offer_claim",
            "interaction",
        ]
        .contains(&name)
    {
        return Err(Error::invalid(
            "Choose a custom safe identifier; built-in categories/events are reserved.",
        ));
    }
    let mut tx = app.db.pool.begin().await?;
    sqlx::query("UPDATE engagement_settings SET version=version+1 WHERE id=1")
        .execute(&mut *tx)
        .await?;
    match kind {
        "event" => {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM engagement_event_names")
                .fetch_one(&mut *tx)
                .await?;
            if count >= 64 || label.trim().is_empty() || label.len() > 160 {
                return Err(Error::invalid(
                    "Event catalog limit reached or label invalid.",
                ));
            }
            sqlx::query("INSERT INTO engagement_event_names(name,label) VALUES($1,$2)")
                .bind(name)
                .bind(label.trim())
                .execute(&mut *tx)
                .await?;
        }
        "dimension" => {
            let values = values.split(',').map(str::trim).collect::<Vec<_>>();
            let distinct: std::collections::HashSet<_> = values.iter().collect();
            if values.is_empty()
                || values.len() > 32
                || distinct.len() != values.len()
                || values.iter().any(|v| !crate::schema::identifier(v))
            {
                return Err(Error::invalid(
                    "Use 1..32 unique category identifiers, separated by commas.",
                ));
            }
            let count: i64 =
                sqlx::query_scalar("SELECT COUNT(DISTINCT name) FROM engagement_dimension_values")
                    .fetch_one(&mut *tx)
                    .await?;
            if count >= 16 {
                return Err(Error::invalid(
                    "At most sixteen dimension categories may be declared.",
                ));
            }
            for value in values {
                sqlx::query("INSERT INTO engagement_dimension_values(name,value) VALUES($1,$2)")
                    .bind(name)
                    .bind(value)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        _ => return Err(Error::invalid("Choose an event or category declaration.")),
    }
    tx.commit().await?;
    Ok(())
}
