//! One bounded route plan, shared by preview, publication and 404 fallback.
use crate::{
    App,
    error::{Error, Result},
};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use sqlx::{Any, Row, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, atomic::Ordering};

pub const MAX_RULES: usize = 128;
pub const MAX_ALIASES: usize = 1000;
pub const MAX_POLICY_BYTES: usize = 512 * 1024;
pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS redirect_policy(id BIGINT PRIMARY KEY CHECK(id=1),definition TEXT NOT NULL,version BIGINT NOT NULL CHECK(version>0));
CREATE TABLE IF NOT EXISTS missing_paths(path TEXT PRIMARY KEY,hits BIGINT NOT NULL CHECK(hits>0),first_seen BIGINT NOT NULL,last_seen BIGINT NOT NULL,version BIGINT NOT NULL CHECK(version>0));
CREATE INDEX IF NOT EXISTS missing_paths_retention ON missing_paths(last_seen);
";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    pub label: String,
    pub pattern: String,
    pub target: String,
    pub code: i64,
    pub priority: i64,
    pub case_insensitive: bool,
    pub enabled: bool,
}
impl Default for Rule {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            pattern: String::new(),
            target: String::new(),
            code: 301,
            priority: 0,
            case_insensitive: false,
            enabled: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub automatic_slug_redirects: bool,
    pub report_404: bool,
    pub retention_days: i64,
    pub max_paths: usize,
    pub rules: Vec<Rule>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            automatic_slug_redirects: false,
            report_404: false,
            retention_days: 30,
            max_paths: 500,
            rules: vec![],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alias {
    pub post_id: String,
    pub target: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub settings: Settings,
    pub aliases: BTreeMap<String, Alias>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exact {
    pub target: String,
    pub code: i64,
}
#[derive(Clone)]
struct Compiled {
    rule: Rule,
    regex: Regex,
}
#[derive(Clone)]
pub struct Plan {
    pub policy: Policy,
    pub exact: BTreeMap<String, Exact>,
    rules: Vec<Compiled>,
}
pub type CachedPlan = Option<(u64, Arc<Plan>)>;
#[derive(Clone, Debug, Serialize)]
pub struct Hop {
    pub source: String,
    pub target: String,
    pub code: i64,
    pub rule: String,
    pub matching_patterns: Vec<String>,
    pub owner: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Resolution {
    pub source: String,
    pub destination: String,
    pub code: i64,
    pub hops: Vec<Hop>,
}

/// Existing publication path policy plus account/financial workflow protection.
pub fn safe_path(path: &str) -> bool {
    path.is_ascii()
        && super::safe_path(path)
        && ![
            "/r",
            "/registration",
            "/audience",
            "/passkeys",
            "/account",
            "/members",
            "/commerce",
            "/forms",
            "/shop/cart",
            "/shop/checkout",
            "/shop/orders",
            "/shop/subscriptions",
        ]
        .iter()
        .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
}
fn captures_in_target(target: &str, groups: usize) -> Result<Vec<(usize, usize, usize)>> {
    let bytes = target.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'$' {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        let braced = bytes.get(i) == Some(&b'{');
        if braced {
            i += 1;
        }
        let digits = i;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if digits == i || i - digits > 2 || (braced && bytes.get(i) != Some(&b'}')) {
            return Err(Error::invalid(
                "Use $1 or ${1} for a declared numbered capture.",
            ));
        }
        let group: usize = target[digits..i]
            .parse()
            .map_err(|_| Error::invalid("Invalid capture."))?;
        if group == 0 || group >= groups {
            return Err(Error::invalid("Replacement refers to a missing capture."));
        }
        if braced {
            i += 1;
        }
        out.push((start, i, group));
    }
    Ok(out)
}
fn compile(rule: &Rule) -> Result<Compiled> {
    if !crate::schema::identifier(&rule.id)
        || rule.id.len() > 48
        || rule.label.trim().is_empty()
        || rule.label.len() > 100
        || !super::plain_text(&rule.label)
        || rule.pattern.len() > 512
        || !rule.pattern.starts_with("^/")
        || !rule.pattern.ends_with('$')
        || rule.pattern.chars().any(char::is_control)
        || rule.target.len() > 300
        || ![301, 302].contains(&rule.code)
        || !(0..=10000).contains(&rule.priority)
    {
        return Err(Error::invalid(
            "Use a named rule, anchored ^/…$ path expression, local target and 301 or 302.",
        ));
    }
    let anchored = format!(r"\A(?:{})\z", rule.pattern);
    let regex = RegexBuilder::new(&anchored).unicode(true).case_insensitive(rule.case_insensitive)
        .multi_line(false).nest_limit(32).size_limit(64 * 1024).dfa_size_limit(64 * 1024)
        .build().map_err(|_| Error::invalid("Pattern is unsupported or exceeds bounded compilation. Lookaround/backreferences are unavailable."))?;
    if regex.captures_len() > 17 {
        return Err(Error::invalid("A rule allows at most 16 captures."));
    }
    let captures = captures_in_target(&rule.target, regex.captures_len())?;
    let mut skeleton = rule.target.clone();
    for (start, end, _) in captures.into_iter().rev() {
        skeleton.replace_range(start..end, "sample");
    }
    if !safe_path(&skeleton) {
        return Err(Error::invalid(
            "Replacement must remain a safe local public path.",
        ));
    }
    Ok(Compiled {
        rule: rule.clone(),
        regex,
    })
}
impl Policy {
    fn parse_shape(raw: &str) -> Result<Self> {
        if raw.len() > MAX_POLICY_BYTES {
            return Err(Error::invalid("Redirect policy exceeds its byte budget."));
        }
        let policy: Self =
            serde_json::from_str(raw).map_err(|_| Error::invalid("Invalid redirect policy."))?;
        policy.validate_shape()?;
        Ok(policy)
    }
    pub fn parse(raw: &str) -> Result<Self> {
        let policy = Self::parse_shape(raw)?;
        policy.validate_rules()?;
        Ok(policy)
    }
    pub fn validate(&self) -> Result<()> {
        self.validate_shape()?;
        self.validate_rules()
    }
    fn validate_rules(&self) -> Result<()> {
        for rule in &self.settings.rules {
            compile(rule)?;
        }
        Ok(())
    }
    fn validate_shape(&self) -> Result<()> {
        if self.settings.rules.len() > MAX_RULES
            || self.aliases.len() > MAX_ALIASES
            || !(1..=90).contains(&self.settings.retention_days)
            || !(1..=2000).contains(&self.settings.max_paths)
            || self
                .settings
                .rules
                .iter()
                .map(|r| r.pattern.len())
                .sum::<usize>()
                > 32 * 1024
        {
            return Err(Error::invalid(
                "Redirect rules, aliases, retention or report size exceed their documented limits.",
            ));
        }
        let mut ids = BTreeSet::new();
        for rule in &self.settings.rules {
            if !ids.insert(&rule.id) {
                return Err(Error::invalid("Redirect rule identifiers must be unique."));
            }
        }
        for (source, alias) in &self.aliases {
            if !safe_path(source)
                || !safe_path(&alias.target)
                || source == &alias.target
                || alias.post_id.is_empty()
                || alias.post_id.len() > 100
            {
                return Err(Error::invalid("Invalid automatic publication alias."));
            }
        }
        if serde_json::to_vec(self)
            .map_err(|_| Error::invalid("Cannot encode policy."))?
            .len()
            > MAX_POLICY_BYTES
        {
            return Err(Error::invalid("Redirect policy exceeds its byte budget."));
        }
        Ok(())
    }
}
impl Plan {
    pub fn new(policy: Policy, exact: BTreeMap<String, Exact>) -> Result<Self> {
        policy.validate_shape()?;
        let mut graph = BTreeMap::new();
        for (source, rule) in &exact {
            if !safe_path(source) || !safe_path(&rule.target) || ![301, 302].contains(&rule.code) {
                return Err(Error::invalid("Invalid stored exact redirect."));
            }
            graph.insert(source.clone(), rule.target.clone());
        }
        super::validate_redirect_graph(&graph)?;
        for (source, alias) in &policy.aliases {
            if exact.contains_key(source) {
                return Err(Error::invalid(
                    "Manual and automatic redirect sources conflict.",
                ));
            }
            graph.insert(source.clone(), alias.target.clone());
        }
        // The combined exact/alias budget is distinct from the legacy 1,000 rule budget.
        for source in graph.keys() {
            let mut path = source.as_str();
            let mut visited = BTreeSet::new();
            for hop in 0..16 {
                if !visited.insert(path) {
                    return Err(Error::invalid(
                        "The redirect plan contains an exact/automatic loop.",
                    ));
                }
                match graph.get(path) {
                    Some(_) if hop == 15 => {
                        return Err(Error::invalid("The redirect plan exceeds 15 hops."));
                    }
                    Some(next) => path = next,
                    None => break,
                }
            }
            if graph.contains_key(path) {
                return Err(Error::invalid("The redirect plan exceeds 15 hops."));
            }
        }
        let mut rules = policy
            .settings
            .rules
            .iter()
            .map(compile)
            .collect::<Result<Vec<_>>>()?;
        rules.sort_by(|a, b| (a.rule.priority, &a.rule.id).cmp(&(b.rule.priority, &b.rule.id)));
        Ok(Self {
            policy,
            exact,
            rules,
        })
    }
    pub fn next(&self, path: &str) -> Result<Option<Hop>> {
        self.next_with_diagnostics(path, true)
    }
    fn next_with_diagnostics(&self, path: &str, diagnostics: bool) -> Result<Option<Hop>> {
        if !safe_path(path) {
            return Ok(None);
        }
        let matching: Vec<_> = if diagnostics {
            self.rules
                .iter()
                .filter(|r| r.rule.enabled && r.regex.is_match(path))
                .map(|r| r.rule.id.clone())
                .collect()
        } else {
            Vec::new()
        };
        if let Some(rule) = self.exact.get(path) {
            return Ok(Some(Hop {
                source: path.into(),
                target: rule.target.clone(),
                code: rule.code,
                rule: "manual exact".into(),
                matching_patterns: matching,
                owner: None,
            }));
        }
        if let Some(alias) = self.policy.aliases.get(path) {
            return Ok(Some(Hop {
                source: path.into(),
                target: alias.target.clone(),
                code: 301,
                rule: "published route".into(),
                matching_patterns: matching,
                owner: Some(alias.post_id.clone()),
            }));
        }
        for compiled in &self.rules {
            if !compiled.rule.enabled {
                continue;
            }
            let Some(captures) = compiled.regex.captures(path) else {
                continue;
            };
            let whole = captures.get(0).unwrap();
            if whole.start() != 0 || whole.end() != path.len() {
                continue;
            }
            let mut target = compiled.rule.target.clone();
            for (start, end, group) in captures_in_target(&target, captures.len())?
                .into_iter()
                .rev()
            {
                let value = captures.get(group).map_or("", |c| c.as_str());
                target.replace_range(start..end, value);
            }
            if !safe_path(&target) || target == path {
                return Err(Error::invalid(
                    "Matched rule produces an unsafe destination or loop.",
                ));
            }
            return Ok(Some(Hop {
                source: path.into(),
                target,
                code: compiled.rule.code,
                rule: compiled.rule.id.clone(),
                matching_patterns: matching,
                owner: None,
            }));
        }
        Ok(None)
    }
    pub fn resolve(&self, source: &str, live: impl Fn(&str) -> bool) -> Result<Option<Resolution>> {
        if !safe_path(source) || live(source) {
            return Ok(None);
        }
        let mut at = source.to_owned();
        let mut visited = BTreeSet::new();
        let mut hops = vec![];
        let mut code = 301;
        for _ in 0..16 {
            if !visited.insert(at.clone()) {
                return Err(Error::invalid("Redirect preview found a loop."));
            }
            if live(&at) {
                break;
            }
            let Some(hop) = self.next(&at)? else {
                break;
            };
            if hops.len() == 15 {
                return Err(Error::invalid("Redirect resolution exceeds 15 hops."));
            }
            if hop.code == 302 {
                code = 302;
            }
            at = hop.target.clone();
            hops.push(hop);
        }
        Ok((!hops.is_empty()).then(|| Resolution {
            source: source.into(),
            destination: at,
            code,
            hops,
        }))
    }
}

pub async fn initialize(tx: &mut Transaction<'_, Any>) -> Result<()> {
    sqlx::query("INSERT INTO redirect_policy(id,definition,version) VALUES(1,$1,1) ON CONFLICT(id) DO NOTHING")
        .bind(serde_json::to_string(&Policy::default()).unwrap()).execute(&mut **tx).await?;
    Ok(())
}
/// All plan writers, including actual publication, acquire this before reading rules.
pub async fn lock(tx: &mut Transaction<'_, Any>) -> Result<(Policy, i64)> {
    let row = sqlx::query(
        "UPDATE redirect_policy SET version=version WHERE id=1 RETURNING definition,version",
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok((
        Policy::parse_shape(&row.get::<String, _>("definition"))?,
        row.get("version"),
    ))
}
pub async fn exact(tx: &mut Transaction<'_, Any>) -> Result<BTreeMap<String, Exact>> {
    let rows = sqlx::query("SELECT source,target,code FROM redirects ORDER BY source LIMIT 1001")
        .fetch_all(&mut **tx)
        .await?;
    if rows.len() > 1000 {
        return Err(Error::invalid("Exact redirect budget exceeded."));
    }
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.get("source"),
                Exact {
                    target: r.get("target"),
                    code: r.get("code"),
                },
            )
        })
        .collect())
}
pub async fn store(tx: &mut Transaction<'_, Any>, policy: &Policy, version: i64) -> Result<()> {
    policy.validate()?;
    let changed = sqlx::query(
        "UPDATE redirect_policy SET definition=$1,version=version+1 WHERE id=1 AND version=$2",
    )
    .bind(
        serde_json::to_string(policy)
            .map_err(|_| Error::invalid("Cannot encode redirect policy."))?,
    )
    .bind(version)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(Error::conflict());
    }
    Ok(())
}
pub async fn load(app: &App) -> Result<(Policy, i64)> {
    let row = sqlx::query("SELECT definition,version FROM redirect_policy WHERE id=1")
        .fetch_one(&app.db.pool)
        .await?;
    Ok((
        Policy::parse(&row.get::<String, _>("definition"))?,
        row.get("version"),
    ))
}
pub async fn discovery_in(tx: &mut Transaction<'_, Any>) -> Result<super::Definition> {
    let raw: String = sqlx::query_scalar("SELECT definition FROM discovery_settings WHERE id=1")
        .fetch_one(&mut **tx)
        .await?;
    let discovery: super::Definition = serde_json::from_str(&raw)
        .map_err(|_| Error::invalid("Invalid discovery configuration."))?;
    discovery.validate()?;
    Ok(discovery)
}

pub fn validate_archive(
    tables: &BTreeMap<String, Vec<BTreeMap<String, serde_json::Value>>>,
    discovery: &super::Definition,
    created_at: i64,
) -> Result<()> {
    let rows = &tables["redirect_policy"];
    if rows.len() != 1 || rows[0]["id"] != 1 || rows[0]["version"].as_i64().is_none_or(|v| v < 1) {
        return Err(Error::invalid(
            "Archive requires one versioned redirect policy.",
        ));
    }
    let policy = Policy::parse(
        rows[0]["definition"]
            .as_str()
            .ok_or_else(|| Error::invalid("Invalid archived redirect policy."))?,
    )?;
    let exact = tables["redirects"]
        .iter()
        .map(|r| {
            (
                r["source"].as_str().unwrap().into(),
                Exact {
                    target: r["target"].as_str().unwrap().into(),
                    code: r["code"].as_i64().unwrap(),
                },
            )
        })
        .collect();
    Plan::new(policy.clone(), exact)?;
    let posts: BTreeMap<_, _> = tables["posts"]
        .iter()
        .map(|p| (p["id"].as_str().unwrap(), p))
        .collect();
    let published_paths: BTreeSet<_> = tables["posts"]
        .iter()
        .filter(|p| p["status"] == "published")
        .map(|p| {
            discovery.path(
                p["published_locale"].as_str().unwrap(),
                p["published_slug"].as_str().unwrap(),
            )
        })
        .collect();
    for (source, alias) in &policy.aliases {
        let Some(post) = posts.get(alias.post_id.as_str()) else {
            return Err(Error::invalid(
                "Automatic alias references missing content.",
            ));
        };
        if !["published", "scheduled"].contains(&post["status"].as_str().unwrap_or(""))
            || post["published_slug"].as_str().is_none_or(str::is_empty)
            || discovery.path(
                post["published_locale"].as_str().unwrap(),
                post["published_slug"].as_str().unwrap(),
            ) != alias.target
        {
            return Err(Error::invalid(
                "Automatic alias must identify its actual retained publication route.",
            ));
        }
        if published_paths.contains(source) {
            return Err(Error::invalid(
                "Automatic alias conflicts with a current publication.",
            ));
        }
    }
    let missing = &tables["missing_paths"];
    if missing.len() > policy.settings.max_paths
        || (!policy.settings.report_404 && !missing.is_empty())
    {
        return Err(Error::invalid(
            "Archived missing-path report exceeds its enabled policy.",
        ));
    }
    let mut paths = BTreeSet::new();
    for row in missing {
        let path = row["path"].as_str().unwrap();
        let first = row["first_seen"].as_i64().unwrap();
        let last = row["last_seen"].as_i64().unwrap();
        if !reportable_path(path)
            || !paths.insert(path)
            || row["hits"].as_i64().unwrap() <= 0
            || row["version"].as_i64().unwrap() <= 0
            || first <= 0
            || last < first
            || last > created_at.saturating_add(300)
        {
            return Err(Error::invalid("Invalid archived missing-path aggregate."));
        }
    }
    Ok(())
}
/// Reporting has a narrower deliberate surface than redirect matching.
pub fn reportable_path(path: &str) -> bool {
    safe_path(path)
        && path != "/"
        && path != "/search"
        && path.len() <= 200
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-_".contains(&b))
        && path
            .split('/')
            .all(|segment| segment.len() <= 64 && !segment.contains('@'))
}

pub async fn plan(app: &App) -> Result<Arc<Plan>> {
    let generation = app.cache_generation.load(Ordering::SeqCst);
    let mut cache = app.redirect_cache.lock().await;
    if let Some((old, plan)) = &*cache
        && *old == generation
    {
        return Ok(plan.clone());
    }
    let mut tx = app.db.pool.begin().await?;
    if app.db.postgres {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
    }
    let raw: String = sqlx::query_scalar("SELECT definition FROM redirect_policy WHERE id=1")
        .fetch_one(&mut *tx)
        .await?;
    let policy = Policy::parse_shape(&raw)?;
    let exact = exact(&mut tx).await?;
    tx.commit().await?;
    // A publication/theme epoch can change while the route grammar stays equal.
    // Reuse compiled patterns only after reading the current complete policy.
    if let Some((_, previous)) = &*cache
        && previous.policy == policy
        && previous.exact == exact
    {
        let plan = previous.clone();
        *cache = Some((generation, plan.clone()));
        return Ok(plan);
    }
    let started = std::time::Instant::now();
    let plan = Arc::new(Plan::new(policy, exact)?);
    tracing::debug!(
        event = "redirect_plan_compiled",
        rules = plan.rules.len(),
        aliases = plan.policy.aliases.len(),
        exact = plan.exact.len(),
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0
    );
    *cache = Some((generation, plan.clone()));
    Ok(plan)
}
#[derive(Clone, Debug)]
pub enum RouteState {
    Missing,
    Public(Option<String>),
    Protected,
}
pub async fn route(app: &App, discovery: &super::Definition, path: &str) -> Result<RouteState> {
    let mut connection = app.db.pool.acquire().await?;
    route_on(app, discovery, path, &mut connection).await
}
async fn route_on(
    app: &App,
    discovery: &super::Definition,
    path: &str,
    connection: &mut sqlx::AnyConnection,
) -> Result<RouteState> {
    if path == "/"
        || path == "/search"
        || discovery.languages.iter().any(|l| {
            path == format!("/{}", l.code)
                || path == discovery.path(&l.code, "")
                || path == discovery.path(&l.code, "search")
        })
    {
        return Ok(RouteState::Public(None));
    }
    if app.config.commerce.enabled {
        if path == "/shop" {
            return Ok(RouteState::Public(None));
        }
        if let Some(id) = path.strip_prefix("/shop/products/")
            && !id.contains('/')
        {
            let exists: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM shop_products WHERE id=$1 AND published=1",
            )
            .bind(id)
            .fetch_one(&mut *connection)
            .await?;
            return Ok(if exists > 0 {
                RouteState::Public(None)
            } else {
                RouteState::Missing
            });
        }
    }
    let parts: Vec<_> = path.trim_start_matches('/').split('/').collect();
    if parts.is_empty() || parts.len() > 2 {
        return Ok(RouteState::Missing);
    }
    let row=sqlx::query("SELECT p.id,p.published_locale,CASE WHEN EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=p.id) THEN 1 ELSE 0 END AS protected FROM posts p WHERE p.published_slug=$1 AND p.published_slug<>'' AND p.status='published'")
        .bind(parts[parts.len()-1]).fetch_optional(&mut *connection).await?;
    let Some(row) = row else {
        return Ok(RouteState::Missing);
    };
    let locale: String = row.get("published_locale");
    if parts.len() == 2 && parts[0] != locale {
        return Ok(RouteState::Missing);
    }
    if row.get::<i64, _>("protected") == 1 {
        return Ok(RouteState::Protected);
    }
    Ok(RouteState::Public(Some(row.get("id"))))
}
/// Shared by owner preview and fallback: real public/private routes retain authority.
pub async fn resolve_checked(app: &App, plan: &Plan, path: &str) -> Result<Option<Resolution>> {
    resolve_with_diagnostics(app, plan, path, true).await
}
pub async fn resolve_runtime(app: &App, plan: &Plan, path: &str) -> Result<Option<Resolution>> {
    resolve_with_diagnostics(app, plan, path, false).await
}
async fn resolve_with_diagnostics(
    app: &App,
    plan: &Plan,
    path: &str,
    diagnostics: bool,
) -> Result<Option<Resolution>> {
    if !safe_path(path) {
        return Err(Error::invalid(
            "Preview requires a safe ASCII local path without query or fragment.",
        ));
    }
    // An empty/default plan needs no route-authority query on ordinary missing paths.
    if plan.exact.is_empty()
        && plan.policy.aliases.is_empty()
        && !plan.rules.iter().any(|r| r.rule.enabled)
    {
        return Ok(None);
    }
    let (discovery, _) = super::load(app).await?;
    if !matches!(route(app, &discovery, path).await?, RouteState::Missing) {
        return Ok(None);
    }
    let mut at = path.to_owned();
    let mut visited = BTreeSet::new();
    let mut hops = Vec::new();
    let mut code = 301;
    loop {
        if !visited.insert(at.clone()) {
            return Err(Error::invalid("Redirect rules form a loop for this path."));
        }
        if !hops.is_empty() {
            match route(app, &discovery, &at).await? {
                RouteState::Protected => {
                    return Err(Error::invalid(
                        "This destination is outside the public route authority.",
                    ));
                }
                RouteState::Public(_) => break,
                RouteState::Missing => {}
            }
        }
        let Some(next) = plan.next_with_diagnostics(&at, diagnostics)? else {
            break;
        };
        if hops.len() == 15 {
            return Err(Error::invalid("Redirect resolution exceeds 15 hops."));
        }
        if let Some(owner) = &next.owner
            && !matches!(route(app,&discovery,&next.target).await?,RouteState::Public(Some(ref id)) if id==owner)
        {
            return Err(Error::invalid(
                "Automatic destination no longer identifies its public publication.",
            ));
        }
        if next.code == 302 {
            code = 302;
        }
        at = next.target.clone();
        hops.push(next);
    }
    Ok((!hops.is_empty()).then(|| Resolution {
        source: path.into(),
        destination: at,
        code,
        hops,
    }))
}
pub async fn response(app: &App, path: &str) -> Result<Option<Response>> {
    if !safe_path(path) {
        return Ok(None);
    }
    let _read = app.mutations.lock().await;
    let plan = plan(app).await?;
    let resolved = match resolve_runtime(app, &plan, path).await {
        Ok(value) => value,
        Err(e) if e.0 == StatusCode::UNPROCESSABLE_ENTITY => {
            tracing::warn!(
                event = "redirect_resolution_refused",
                reason = "invalid-plan-result"
            );
            return Ok(None);
        }
        Err(e) => return Err(e),
    };
    let Some(resolved) = resolved else {
        return Ok(None);
    };
    tracing::debug!(
        event = "redirect_resolved",
        hops = resolved.hops.len(),
        status = resolved.code
    );
    Ok(Some(
        (
            StatusCode::from_u16(resolved.code as u16).unwrap(),
            [
                ("location", resolved.destination),
                ("cache-control", "no-store".into()),
            ],
        )
            .into_response(),
    ))
}

/// Removal is a versioned policy edit; automatic ownership cannot overwrite a manual rule.
pub async fn remove_alias(app: &App, source: &str, version: i64) -> Result<()> {
    let _guard = app.mutation().await?;
    let mut tx = app.db.begin_write().await?;
    let (mut policy, current) = lock(&mut tx).await?;
    if current != version || policy.aliases.remove(source).is_none() {
        return Err(Error::conflict());
    }
    store(&mut tx, &policy, current).await?;
    tx.commit().await?;
    Ok(())
}
/// A changed count cannot be erased by a stale report page.
pub async fn delete_missing(app: &App, path: &str, version: i64) -> Result<()> {
    let _guard = app.mutation().await?;
    let mut tx = app.db.begin_write().await?;
    lock(&mut tx).await?;
    let affected = sqlx::query("DELETE FROM missing_paths WHERE path=$1 AND version=$2")
        .bind(path)
        .bind(version)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if affected != 1 {
        return Err(Error::conflict());
    }
    tx.commit().await?;
    Ok(())
}
pub async fn purge_missing(app: &App, version: i64) -> Result<()> {
    let _guard = app.mutation().await?;
    let mut tx = app.db.begin_write().await?;
    let (_, current) = lock(&mut tx).await?;
    if current != version {
        return Err(Error::conflict());
    }
    sqlx::query("DELETE FROM missing_paths")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn configure(app: &App, settings: Settings, version: i64) -> Result<i64> {
    // Compile owner input before acquiring transaction ownership.
    Policy {
        settings: settings.clone(),
        aliases: BTreeMap::new(),
    }
    .validate()?;
    let _guard = app.mutation().await?;
    let mut tx = app.db.begin_write().await?;
    let (mut policy, current) = lock(&mut tx).await?;
    if version != current {
        return Err(Error::conflict());
    }
    policy.settings = settings;
    Plan::new(policy.clone(), exact(&mut tx).await?)?;
    if !policy.settings.report_404 {
        sqlx::query("DELETE FROM missing_paths")
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query("DELETE FROM missing_paths WHERE last_seen<$1")
            .bind(crate::now() - policy.settings.retention_days * 86400)
            .execute(&mut *tx)
            .await?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM missing_paths")
            .fetch_one(&mut *tx)
            .await?;
        if count > policy.settings.max_paths as i64 {
            return Err(Error::invalid(
                "Purge the report before reducing its path budget below retained data.",
            ));
        }
    }
    store(&mut tx, &policy, current).await?;
    tx.commit().await?;
    Ok(current + 1)
}

/// Optional operational aggregates are atomic but do not invalidate public pages.
pub async fn record_missing(app: &App, path: &str) -> Result<()> {
    if !reportable_path(path) {
        return Ok(());
    }
    let _read = app.mutations.lock().await;
    let cached = plan(app).await?;
    if !cached.policy.settings.report_404 {
        return Ok(());
    }
    if let Some(coordinator) = &app.local_coordinator {
        coordinator.mark_intent()?;
    }
    let mut tx = app.db.begin_write().await?;
    let (policy, _) = lock(&mut tx).await?;
    if !policy.settings.report_404 {
        return Ok(());
    }
    // Recheck after plan ownership; publication cannot race an aggregate write.
    let discovery = discovery_in(&mut tx).await?;
    if !matches!(
        route_on(app, &discovery, path, &mut tx).await?,
        RouteState::Missing
    ) {
        return Ok(());
    }
    let time = crate::now();
    sqlx::query("DELETE FROM missing_paths WHERE last_seen<$1")
        .bind(time - policy.settings.retention_days * 86400)
        .execute(&mut *tx)
        .await?;
    let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM missing_paths WHERE path=$1")
        .bind(path)
        .fetch_one(&mut *tx)
        .await?;
    if existing == 0 {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM missing_paths")
            .fetch_one(&mut *tx)
            .await?;
        if count >= policy.settings.max_paths as i64 {
            tx.commit().await?;
            return Ok(());
        }
    }
    sqlx::query("INSERT INTO missing_paths(path,hits,first_seen,last_seen,version) VALUES($1,1,$2,$2,1) ON CONFLICT(path) DO UPDATE SET hits=CASE WHEN missing_paths.hits<9223372036854775807 THEN missing_paths.hits+1 ELSE missing_paths.hits END,last_seen=$2,version=CASE WHEN missing_paths.version<9223372036854775807 THEN missing_paths.version+1 ELSE missing_paths.version END")
        .bind(path).bind(time).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn retention(app: &App) -> Result<usize> {
    // Retained aggregates are operational data, not a public-page mutation.
    let _read = app.mutations.lock().await;
    if !plan(app).await?.policy.settings.report_404 {
        return Ok(0); // Disabling reporting atomically clears its rows.
    }
    if let Some(coordinator) = &app.local_coordinator {
        coordinator.mark_intent()?;
    }
    let mut tx = app.db.begin_write().await?;
    let (policy, _) = lock(&mut tx).await?;
    let count = if policy.settings.report_404 {
        sqlx::query("DELETE FROM missing_paths WHERE last_seen<$1")
            .bind(crate::now() - policy.settings.retention_days * 86400)
            .execute(&mut *tx)
            .await?
            .rows_affected()
    } else {
        sqlx::query("DELETE FROM missing_paths")
            .execute(&mut *tx)
            .await?
            .rows_affected()
    };
    tx.commit().await?;
    Ok(count as usize)
}

/// Called within the content transaction after acquiring plan ownership.
pub async fn publication(
    tx: &mut Transaction<'_, Any>,
    policy: &mut Policy,
    post: &crate::model::Post,
    old: Option<&crate::model::Post>,
    action: &str,
) -> Result<bool> {
    if !["publish", "schedule", "unpublish"].contains(&action) {
        return Ok(false);
    }
    let before = serde_json::to_string(policy).unwrap();
    if action == "unpublish" {
        policy.aliases.retain(|_, alias| alias.post_id != post.id);
        return Ok(before != serde_json::to_string(policy).unwrap());
    }
    let discovery = discovery_in(tx).await?;
    discovery.language(&post.locale)?;
    let destination = discovery.path(&post.locale, &post.slug);
    let plain = format!("/{}", post.slug);
    let manual = exact(tx).await?;
    if manual.contains_key(&destination) || manual.contains_key(&plain) {
        return Err(Error::invalid(
            "Remove the conflicting manual redirect before publication.",
        ));
    }
    for path in [&destination, &plain] {
        if policy
            .aliases
            .get(path)
            .is_some_and(|alias| alias.post_id != post.id)
        {
            return Err(Error::invalid(
                "Publication route belongs to another automatic alias; review redirects first.",
            ));
        }
    }
    if action == "publish" {
        policy
            .aliases
            .retain(|source, _| source != &destination && source != &plain);
        for alias in policy
            .aliases
            .values_mut()
            .filter(|alias| alias.post_id == post.id)
        {
            alias.target = destination.clone();
        }
        let protected: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM member_resources WHERE kind='post' AND resource_id=$1",
        )
        .bind(&post.id)
        .fetch_one(&mut **tx)
        .await?;
        if protected == 0
            && policy.settings.automatic_slug_redirects
            && let Some(old) = old
            && !old.published_slug.is_empty()
        {
            let mut sources =
                BTreeSet::from([discovery.path(&old.published_locale, &old.published_slug)]);
            if old.published_slug != post.slug {
                sources.insert(format!("/{}", old.published_slug));
            }
            for source in sources {
                if source == destination || source == plain {
                    continue;
                }
                if !safe_path(&source) || !safe_path(&destination) {
                    return Err(Error::invalid(
                        "Automatic publication route is outside the safe local path policy.",
                    ));
                }
                if manual.contains_key(&source)
                    || policy
                        .aliases
                        .get(&source)
                        .is_some_and(|a| a.post_id != post.id)
                {
                    return Err(Error::invalid(
                        "Previous publication route has a conflicting manual/owned rule.",
                    ));
                }
                policy.aliases.insert(
                    source,
                    Alias {
                        post_id: post.id.clone(),
                        target: destination.clone(),
                    },
                );
            }
        }
        if protected > 0 {
            policy.aliases.retain(|_, alias| alias.post_id != post.id);
        }
    }
    Plan::new(policy.clone(), manual)?;
    Ok(before != serde_json::to_string(policy).unwrap())
}
