//! Stable experiments, consented targeting and atomic local offer allocation.
use super::engagement;
use crate::{
    App, auth,
    document::Document,
    error::{Error, Result},
    now,
};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use sqlx::Row;
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Target {
    pub paths: Vec<String>,
    pub device: String,
    pub referrer: String,
    pub starts_at: i64,
    pub ends_at: i64,
    pub max_impressions: i64,
}
impl Default for Target {
    fn default() -> Self {
        Self {
            paths: vec![],
            device: "all".into(),
            referrer: "all".into(),
            starts_at: 0,
            ends_at: 0,
            max_impressions: 1,
        }
    }
}
impl Target {
    pub fn validate(&self) -> Result<()> {
        if self.paths.len() > 16
            || self
                .paths
                .iter()
                .any(|p| p.len() > 200 || !p.starts_with('/') || p.contains(['?', '#', '\\']))
            || !["all", "mobile", "desktop"].contains(&self.device.as_str())
            || !["all", "direct", "same_site", "external"].contains(&self.referrer.as_str())
            || self.starts_at < 0
            || self.ends_at < 0
            || (self.ends_at != 0 && self.ends_at <= self.starts_at)
            || !(1..=10).contains(&self.max_impressions)
        {
            return Err(Error::invalid(
                "Use bounded public paths, supported device/referrer categories, valid schedule and 1..10 impressions.",
            ));
        }
        Ok(())
    }
    fn matches(&self, path: &str, device: &str, referrer: &str) -> bool {
        (self.paths.is_empty() || self.paths.iter().any(|p| p == path))
            && (self.device == "all" || self.device == device)
            && (self.referrer == "all" || self.referrer == referrer)
            && now() >= self.starts_at
            && (self.ends_at == 0 || now() < self.ends_at)
    }
}
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS business_promotions(id TEXT PRIMARY KEY,title TEXT NOT NULL,document_a TEXT NOT NULL,document_b TEXT NOT NULL,experiment BIGINT NOT NULL DEFAULT 0,wheel BIGINT NOT NULL DEFAULT 0,target TEXT NOT NULL,active BIGINT NOT NULL DEFAULT 0,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS promotions_active ON business_promotions(active,created_at,id);
CREATE TABLE IF NOT EXISTS promotion_rewards(id TEXT PRIMARY KEY,promotion_id TEXT NOT NULL REFERENCES business_promotions(id),label TEXT NOT NULL,weight BIGINT NOT NULL,remaining BIGINT NOT NULL CHECK(remaining>=0),issued BIGINT NOT NULL DEFAULT 0 CHECK(issued>=0));
CREATE INDEX IF NOT EXISTS promotion_reward_available ON promotion_rewards(promotion_id,remaining,id);
CREATE TABLE IF NOT EXISTS promotion_impressions(promotion_id TEXT NOT NULL REFERENCES business_promotions(id),session_hash TEXT NOT NULL REFERENCES engagement_sessions(hash) ON DELETE CASCADE,variant TEXT NOT NULL,path TEXT NOT NULL,count BIGINT NOT NULL DEFAULT 1,last_at BIGINT NOT NULL,PRIMARY KEY(promotion_id,session_hash));
CREATE TABLE IF NOT EXISTS promotion_claims(id TEXT PRIMARY KEY,promotion_id TEXT NOT NULL REFERENCES business_promotions(id),session_hash TEXT NOT NULL REFERENCES engagement_sessions(hash) ON DELETE CASCADE,reward_id TEXT NOT NULL REFERENCES promotion_rewards(id),label TEXT NOT NULL,code TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(promotion_id,session_hash));
"#;
pub async fn create(app: &App, title: &str) -> Result<String> {
    if title.trim().is_empty() || title.len() > 160 {
        return Err(Error::invalid("Use a promotion title up to 160 bytes."));
    }
    let mut tx = app.db.pool.begin().await?;
    // Serializes admission and purpose updates without a process-only mutex.
    sqlx::query("UPDATE engagement_settings SET version=version+1 WHERE id=1")
        .execute(&mut *tx)
        .await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM business_promotions")
        .fetch_one(&mut *tx)
        .await?;
    if count >= 100 {
        return Err(Error::invalid(
            "At most 100 local promotions are supported.",
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO business_promotions(id,title,document_a,document_b,target,created_at) VALUES($1,$2,$3,$3,$4,$5)").bind(&id).bind(title).bind(crate::document::empty()).bind(serde_json::to_string(&Target::default()).map_err(|_|Error::invalid("Invalid target."))?).bind(now()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(id)
}
#[allow(clippy::too_many_arguments)] // One compare-and-swap saves variant content and its targeting controls.
pub async fn save(
    app: &App,
    id: &str,
    version: i64,
    variant: &str,
    document: &str,
    target: Target,
    active: bool,
    experiment: bool,
    wheel: bool,
) -> Result<()> {
    target.validate()?;
    let document = Document::parse(document)?.encode();
    let target = serde_json::to_string(&target).map_err(|_| Error::invalid("Invalid target."))?;
    let column = match variant {
        "a" => "document_a",
        "b" => "document_b",
        _ => return Err(Error::invalid("Choose experiment variant A or B.")),
    };
    let mut tx = app.db.pool.begin().await?;
    sqlx::query("UPDATE engagement_settings SET version=version+1 WHERE id=1")
        .execute(&mut *tx)
        .await?;
    if sqlx::query(&format!("UPDATE business_promotions SET {column}=$1,target=$2,active=$3,experiment=$4,wheel=$5,version=version+1 WHERE id=$6 AND version=$7")).bind(document).bind(target).bind(i64::from(active)).bind(i64::from(experiment)).bind(i64::from(wheel)).bind(id).bind(version).execute(&mut *tx).await?.rows_affected()!=1{return Err(Error::conflict());}
    tx.commit().await?;
    Ok(())
}
pub async fn reward(
    app: &App,
    promotion: &str,
    label: &str,
    weight: i64,
    stock: i64,
) -> Result<()> {
    if label.trim().is_empty()
        || label.len() > 160
        || !(1..=10000).contains(&weight)
        || !(1..=1000000).contains(&stock)
    {
        return Err(Error::invalid(
            "Use a reward label, weight 1..10,000 and stock 1..1,000,000.",
        ));
    }
    let mut tx = app.db.pool.begin().await?;
    let found: Option<String> = sqlx::query_scalar(
        "UPDATE business_promotions SET version=version+1 WHERE id=$1 RETURNING id",
    )
    .bind(promotion)
    .fetch_optional(&mut *tx)
    .await?;
    found.ok_or_else(Error::not_found)?;
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM promotion_rewards WHERE promotion_id=$1")
            .bind(promotion)
            .fetch_one(&mut *tx)
            .await?;
    if count >= 16 {
        return Err(Error::invalid(
            "A local wheel supports at most sixteen rewards.",
        ));
    }
    sqlx::query("INSERT INTO promotion_rewards(id,promotion_id,label,weight,remaining) VALUES($1,$2,$3,$4,$5)").bind(uuid::Uuid::new_v4().to_string()).bind(promotion).bind(label.trim()).bind(weight).bind(stock).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
async fn eligible_session(app: &App, headers: &HeaderMap) -> Result<Option<String>> {
    if !app.config.business_enabled
        || !app.config.engagement.enabled
        || engagement::privacy_signal(app, headers)
    {
        return Ok(None);
    }
    let Some(hash) = engagement::token(headers) else {
        return Ok(None);
    };
    let found:Option<String>=sqlx::query_scalar("SELECT s.hash FROM engagement_sessions s JOIN engagement_settings e ON e.id=1 WHERE s.hash=$1 AND s.policy=e.version AND s.expires_at>$2 AND e.enabled=1").bind(&hash).bind(now()).fetch_optional(&app.db.pool).await?;
    Ok(found)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Visit {
    pub path: String,
    pub device: String,
    pub referrer: String,
}
pub async fn visit(
    app: &App,
    headers: &HeaderMap,
    input: Visit,
) -> Result<Option<serde_json::Value>> {
    let Some(hash) = eligible_session(app, headers).await? else {
        return Ok(None);
    };
    if !engagement::known_path(app, &input.path).await?
        || !["mobile", "desktop"].contains(&input.device.as_str())
        || !["direct", "same_site", "external"].contains(&input.referrer.as_str())
    {
        return Err(Error::invalid(
            "Use declared public-page targeting categories.",
        ));
    }
    let mut tx = app.db.pool.begin().await?;
    let live: Option<i64> = sqlx::query_scalar(
        "UPDATE engagement_settings SET enabled=enabled WHERE id=1 AND enabled=1 RETURNING version",
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(policy) = live else {
        return Ok(None);
    };
    let valid: Option<String> = sqlx::query_scalar(
        "SELECT hash FROM engagement_sessions WHERE hash=$1 AND policy=$2 AND expires_at>$3",
    )
    .bind(&hash)
    .bind(policy)
    .bind(now())
    .fetch_optional(&mut *tx)
    .await?;
    if valid.is_none() {
        return Ok(None);
    }
    let rows=sqlx::query("SELECT id,title,target,experiment,wheel,document_a,document_b FROM business_promotions WHERE active=1 ORDER BY created_at,id LIMIT 100").fetch_all(&mut *tx).await?;
    for row in rows {
        let target: Target = serde_json::from_str(&row.get::<String, _>("target"))
            .map_err(|_| Error::invalid("Stored targeting needs repair."))?;
        if !target.matches(&input.path, &input.device, &input.referrer) {
            continue;
        }
        let id: String = row.get("id");
        let existing=sqlx::query("SELECT variant,count FROM promotion_impressions WHERE promotion_id=$1 AND session_hash=$2").bind(&id).bind(&hash).fetch_optional(&mut *tx).await?;
        if existing
            .as_ref()
            .is_some_and(|r| r.get::<i64, _>("count") >= target.max_impressions)
        {
            continue;
        }
        let variant = existing
            .as_ref()
            .map(|r| r.get::<String, _>("variant"))
            .unwrap_or_else(|| {
                if row.get::<i64, _>("experiment") == 1
                    && auth::digest(format!("{id}:{hash}").as_bytes()).as_bytes()[0] & 1 == 1
                {
                    "b".into()
                } else {
                    "a".into()
                }
            });
        sqlx::query("INSERT INTO promotion_impressions(promotion_id,session_hash,variant,last_at,path) VALUES($1,$2,$3,$4,$5) ON CONFLICT(promotion_id,session_hash) DO UPDATE SET count=promotion_impressions.count+1,last_at=$4,path=$5").bind(&id).bind(&hash).bind(&variant).bind(now()).bind(&input.path).execute(&mut *tx).await?;
        let rewards=sqlx::query("SELECT id,label,weight FROM promotion_rewards WHERE promotion_id=$1 AND remaining>0 ORDER BY id LIMIT 16").bind(&id).fetch_all(&mut *tx).await?.into_iter().map(|r|serde_json::json!({"id":r.get::<String,_>("id"),"label":r.get::<String,_>("label"),"weight":r.get::<i64,_>("weight")})).collect::<Vec<_>>();
        let doc = Document::parse(&row.get::<String, _>(if variant == "b" {
            "document_b"
        } else {
            "document_a"
        }))?;
        let output = serde_json::json!({"id":id,"title":row.get::<String,_>("title"),"variant":variant,"html":doc.html(),"wheel":row.get::<i64,_>("wheel")==1,"rewards":rewards});
        tx.commit().await?;
        return Ok(Some(output));
    }
    Ok(None)
}
pub async fn claim(app: &App, headers: &HeaderMap, promotion: &str) -> Result<serde_json::Value> {
    let hash = eligible_session(app, headers)
        .await?
        .ok_or_else(Error::forbidden)?;
    let mut tx = app.db.pool.begin().await?;
    let policy: Option<i64> = sqlx::query_scalar(
        "UPDATE engagement_settings SET enabled=enabled WHERE id=1 AND enabled=1 RETURNING version",
    )
    .fetch_optional(&mut *tx)
    .await?;
    let policy = policy.ok_or_else(Error::forbidden)?;
    let valid: Option<String> = sqlx::query_scalar(
        "SELECT hash FROM engagement_sessions WHERE hash=$1 AND policy=$2 AND expires_at>$3",
    )
    .bind(&hash)
    .bind(policy)
    .bind(now())
    .fetch_optional(&mut *tx)
    .await?;
    valid.ok_or_else(Error::forbidden)?;

    let valid: Option<String> = sqlx::query_scalar(
        "UPDATE business_promotions SET version=version WHERE id=$1 AND active=1 RETURNING target",
    )
    .bind(promotion)
    .fetch_optional(&mut *tx)
    .await?;
    let target: Target = serde_json::from_str(&valid.ok_or_else(Error::not_found)?)
        .map_err(|_| Error::invalid("Stored targeting needs repair."))?;
    if now() < target.starts_at || (target.ends_at != 0 && now() >= target.ends_at) {
        return Err(Error::forbidden());
    }
    if let Some(row) = sqlx::query(
        "SELECT id,label,code FROM promotion_claims WHERE promotion_id=$1 AND session_hash=$2",
    )
    .bind(promotion)
    .bind(&hash)
    .fetch_optional(&mut *tx)
    .await?
    {
        return Ok(
            serde_json::json!({"id":row.get::<String,_>("id"),"label":row.get::<String,_>("label"),"code":row.get::<String,_>("code")}),
        );
    }
    let viewed: Option<String> = sqlx::query_scalar(
        "SELECT variant FROM promotion_impressions WHERE promotion_id=$1 AND session_hash=$2",
    )
    .bind(promotion)
    .bind(&hash)
    .fetch_optional(&mut *tx)
    .await?;
    viewed.ok_or_else(Error::forbidden)?;
    let rows=sqlx::query("SELECT id,label,weight FROM promotion_rewards WHERE promotion_id=$1 AND remaining>0 ORDER BY id LIMIT 16").bind(promotion).fetch_all(&mut *tx).await?;
    let total: i64 = rows.iter().map(|r| r.get::<i64, _>("weight")).sum();
    if total == 0 {
        return Err(Error::invalid("All local offers have been claimed."));
    }
    use rand::Rng;
    let mut draw = rand::rngs::OsRng.gen_range(0..total);
    let selected = rows
        .iter()
        .find(|r| {
            draw -= r.get::<i64, _>("weight");
            draw < 0
        })
        .ok_or_else(|| Error::invalid("Invalid local offer weights."))?;
    let reward: String = selected.get("id");
    let label: String = selected.get("label");
    if sqlx::query("UPDATE promotion_rewards SET remaining=remaining-1,issued=issued+1 WHERE id=$1 AND remaining>0").bind(&reward).execute(&mut *tx).await?.rows_affected()!=1{return Err(Error::conflict());}
    let id = uuid::Uuid::new_v4().to_string();
    let code = format!("LOCAL-{}", uuid::Uuid::new_v4().simple());
    sqlx::query("INSERT INTO promotion_claims(id,promotion_id,session_hash,reward_id,label,code,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(&id).bind(promotion).bind(hash).bind(reward).bind(&label).bind(&code).bind(now()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(serde_json::json!({"id":id,"label":label,"code":code}))
}
