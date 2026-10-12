//! Finite derived public analysis. Claims commit before rendering; cancellation
//! and source changes fence every batch, including independent App instances.
use crate::{
    App,
    error::{Error, Result},
};
use serde::{Deserialize, Serialize};
use sqlx::{Any, Execute, QueryBuilder, Row, Transaction};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub max_publications: usize,
    pub max_edges: usize,
    pub max_html_bytes: usize,
    pub max_facts_bytes: usize,
    pub batch_sources: usize,
    pub batch_seconds: u64,
    pub lease_seconds: i64,
    pub retention_seconds: i64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            max_publications: 5000,
            max_edges: 100000,
            max_html_bytes: 128 * 1024 * 1024,
            max_facts_bytes: 256 * 1024 * 1024,
            batch_sources: 8,
            batch_seconds: 5,
            lease_seconds: 120,
            retention_seconds: 7 * 86400,
        }
    }
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (1..=5000).contains(&self.max_publications),
            "analysis max_publications must be 1..5000"
        );
        anyhow::ensure!(
            (1..=100000).contains(&self.max_edges),
            "analysis max_edges must be 1..100000"
        );
        anyhow::ensure!(
            (1024..=128 * 1024 * 1024).contains(&self.max_html_bytes),
            "analysis max_html_bytes must be 1 KiB..128 MiB"
        );
        anyhow::ensure!(
            (1024..=256 * 1024 * 1024).contains(&self.max_facts_bytes),
            "analysis max_facts_bytes must be 1 KiB..256 MiB"
        );
        anyhow::ensure!(
            (1..=32).contains(&self.batch_sources) && (1..=10).contains(&self.batch_seconds),
            "analysis batches must be 1..32 sources and 1..10 seconds"
        );
        anyhow::ensure!(
            (30..=600).contains(&self.lease_seconds)
                && self.lease_seconds > self.batch_seconds as i64,
            "analysis lease must be 30..600 seconds and exceed its batch deadline"
        );
        anyhow::ensure!(
            (60..=30 * 86400).contains(&self.retention_seconds),
            "analysis retention must be 60 seconds..30 days"
        );
        Ok(())
    }
}
pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS analysis_jobs(id TEXT PRIMARY KEY,sequence BIGINT NOT NULL UNIQUE CHECK(sequence>0),state TEXT NOT NULL CHECK(state IN ('rendering','classifying','complete','failed','cancelled','stale')),epoch BIGINT NOT NULL,identity TEXT NOT NULL,focus TEXT NOT NULL,version BIGINT NOT NULL DEFAULT 1,cursor BIGINT NOT NULL DEFAULT 0,total BIGINT NOT NULL CHECK(total BETWEEN 1 AND 5032),html_bytes BIGINT NOT NULL DEFAULT 0,facts_bytes BIGINT NOT NULL DEFAULT 0,edge_count BIGINT NOT NULL DEFAULT 0,reused BIGINT NOT NULL DEFAULT 0,token TEXT NOT NULL DEFAULT '',lease_until BIGINT NOT NULL DEFAULT 0,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,error TEXT NOT NULL DEFAULT '');
CREATE UNIQUE INDEX IF NOT EXISTS analysis_one_active ON analysis_jobs((1)) WHERE state IN ('rendering','classifying');
CREATE TABLE IF NOT EXISTS analysis_pages(job_id TEXT NOT NULL REFERENCES analysis_jobs(id) ON DELETE CASCADE,ordinal BIGINT NOT NULL,path TEXT NOT NULL,locale TEXT NOT NULL,slug TEXT NOT NULL,publication TEXT NOT NULL,html_sha256 TEXT NOT NULL DEFAULT '',html_bytes BIGINT NOT NULL DEFAULT 0,text TEXT NOT NULL DEFAULT '',ids TEXT NOT NULL DEFAULT '',links TEXT NOT NULL DEFAULT '',edges TEXT NOT NULL DEFAULT '',PRIMARY KEY(job_id,ordinal),UNIQUE(job_id,path));
CREATE INDEX IF NOT EXISTS analysis_reuse ON analysis_pages(path,html_sha256);
CREATE TABLE IF NOT EXISTS analysis_inbound(job_id TEXT NOT NULL REFERENCES analysis_jobs(id) ON DELETE CASCADE,source TEXT NOT NULL,destination TEXT NOT NULL,PRIMARY KEY(job_id,destination,source));
";
#[derive(Serialize)]
pub struct Job {
    pub id: String,
    pub state: String,
    pub current: bool,
    pub version: i64,
    pub cursor: i64,
    pub total: i64,
    pub html_bytes: i64,
    pub facts_bytes: i64,
    pub edge_count: i64,
    pub reused: i64,
    pub focus: Option<String>,
    pub error: String,
    pub created_at: i64,
}
fn identity(app: &App) -> Result<String> {
    let value = serde_json::json!({"algorithm":"wpalt-editorial-text-v1","executable":crate::platform::local_processes::executable_identity()?,"segmentation":unicode_segmentation::UNICODE_VERSION,"normalization":unicode_normalization::UNICODE_VERSION,"lowercase":std::char::UNICODE_VERSION,"origin":app.config.origin(),"assets":app.config.assets,"spam":app.config.spam.enabled,"business":app.config.business_enabled,"membership":app.config.membership_enabled,"consent":app.consent_scripts.manifest,"engagement":app.config.engagement,"analysis":app.config.analysis});
    Ok(crate::auth::digest(&serde_json::to_vec(&value).map_err(
        |_| Error::invalid("Invalid analysis identity."),
    )?))
}
fn writable(app: &App) -> Result<()> {
    if !app.config.analysis.enabled {
        return Err(Error::invalid(
            "Public background analysis is disabled in configuration.",
        ));
    }
    if app.clone_held.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(Error::invalid(
            "Analysis writes are paused for this recovered clone.",
        ));
    }
    Ok(())
}
async fn lock_epoch(tx: &mut Transaction<'_, Any>, epoch: i64) -> Result<bool> {
    Ok(
        sqlx::query("UPDATE analysis_epoch SET version=version WHERE id=1 AND version=$1")
            .bind(epoch)
            .execute(&mut **tx)
            .await?
            .rows_affected()
            == 1,
    )
}
pub async fn current_owner(app: &App, session: &crate::model::Session) -> Result<()> {
    if !session.is_admin() || session.hash.starts_with("integration:") {
        return Err(Error::forbidden());
    }
    crate::auth::current_editor(app, session).await
}
pub async fn start(app: &App, focus: &str) -> Result<String> {
    start_as(app, focus, None).await
}
pub async fn start_for_owner(
    app: &App,
    focus: &str,
    session: &crate::model::Session,
) -> Result<String> {
    start_as(app, focus, Some(session)).await
}
async fn start_as(
    app: &App,
    focus: &str,
    session: Option<&crate::model::Session>,
) -> Result<String> {
    writable(app)?;
    let focus = super::text::focus(focus)?;
    let _read = app.mutations.lock().await;
    if let Some(session) = session {
        current_owner(app, session).await?;
    }
    let epoch = super::epoch::current(app).await?;
    let identity = identity(app)?;
    let (discovery, _) = crate::discovery::load(app).await?;
    if discovery.languages.len() > 32 {
        return Err(Error::invalid(
            "Analysis supports at most 32 language homes.",
        ));
    }
    let mut tx = app.db.begin_write().await?;
    if !lock_epoch(&mut tx, epoch).await? {
        return Err(Error::conflict());
    }
    // A changed source permits a fresh run without waiting for the scheduler.
    // This transaction fences any unfinished old worker before admitting work.
    sqlx::query("UPDATE analysis_jobs SET state='stale',token='',lease_until=0,version=version+1 WHERE state IN ('rendering','classifying') AND (epoch<>$1 OR identity<>$2)")
        .bind(epoch).bind(&identity).execute(&mut *tx).await?;
    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM analysis_jobs WHERE state IN ('rendering','classifying')",
    )
    .fetch_one(&mut *tx)
    .await?;
    if active != 0 {
        return Err(Error::invalid(
            "An analysis is already active; cancel or finish it before starting another.",
        ));
    }
    let rows=sqlx::query("SELECT id,published_slug,published_locale FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id) ORDER BY id LIMIT $1").bind((app.config.analysis.max_publications+1) as i64).fetch_all(&mut *tx).await?;
    if rows.len() > app.config.analysis.max_publications {
        return Err(Error::invalid(
            "Public source count exceeds the configured analysis budget; no partial run was created.",
        ));
    }
    let mut declarations = Vec::new();
    for language in &discovery.languages {
        declarations.push((
            discovery.path(&language.code, ""),
            language.code.clone(),
            String::new(),
            String::new(),
        ));
    }
    for row in rows {
        let locale: String = row.get("published_locale");
        let slug: String = row.get("published_slug");
        declarations.push((
            discovery.path(&locale, &slug),
            locale,
            slug,
            row.get::<String, _>("id"),
        ));
    }
    // Keep the latest previous run for exact newly-rendered HTML reuse. Never
    // retain an unbounded history or expose old content through a stale view.
    let prior = sqlx::query("SELECT id,sequence FROM analysis_jobs ORDER BY sequence DESC LIMIT 1")
        .fetch_optional(&mut *tx)
        .await?;
    let sequence = prior
        .as_ref()
        .map(|r| r.get::<i64, _>("sequence"))
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(Error::conflict)?;
    let previous = prior.map(|r| r.get::<String, _>("id"));
    let declaration_bytes = encode(&declarations)?.len();
    if declaration_bytes > app.config.analysis.max_facts_bytes {
        return Err(Error::invalid(
            "Source declarations exceed the derived data budget.",
        ));
    }
    sqlx::query("DELETE FROM analysis_jobs WHERE id<>$1")
        .bind(previous.unwrap_or_default())
        .execute(&mut *tx)
        .await?;
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO analysis_jobs(id,state,epoch,identity,focus,total,created_at,updated_at,sequence,facts_bytes) VALUES($1,'rendering',$2,$3,$4,$5,$6,$6,$7,$8)").bind(&id).bind(epoch).bind(identity).bind(focus).bind(declarations.len() as i64).bind(crate::now()).bind(sequence).bind(declaration_bytes as i64).execute(&mut *tx).await?;
    let declarations: Vec<_> = declarations.into_iter().enumerate().collect();
    for chunk in declarations.chunks(100) {
        let mut q = QueryBuilder::<Any>::new(
            "INSERT INTO analysis_pages(job_id,ordinal,path,locale,slug,publication) ",
        );
        q.push_values(
            chunk,
            |mut row, (ordinal, (path, locale, slug, publication))| {
                row.push_bind(&id)
                    .push_bind(*ordinal as i64)
                    .push_bind(path)
                    .push_bind(locale)
                    .push_bind(slug)
                    .push_bind(publication);
            },
        );
        let mut query = q.build();
        let sql = crate::db::Db::numbered(query.sql());
        let args = query
            .take_arguments()
            .map_err(sqlx::Error::Encode)?
            .unwrap_or_default();
        sqlx::query_with(&sql, args).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    tracing::info!(event="analysis_started", job_id=%id, source_epoch=epoch);
    Ok(id)
}
pub async fn list(app: &App) -> Result<Vec<Job>> {
    let _read = app.mutations.lock().await;
    let epoch = super::epoch::current(app).await?;
    let identity = identity(app)?;
    let rows=sqlx::query("SELECT id,state,epoch,identity,focus,version,cursor,total,html_bytes,facts_bytes,edge_count,reused,error,created_at FROM analysis_jobs ORDER BY sequence DESC LIMIT 2").fetch_all(&app.db.pool).await?;
    if epoch != super::epoch::current(app).await? {
        return Err(Error::conflict());
    }
    Ok(rows
        .into_iter()
        .map(|row| {
            let current =
                row.get::<i64, _>("epoch") == epoch && row.get::<String, _>("identity") == identity;
            Job {
                id: row.get("id"),
                state: if current {
                    row.get("state")
                } else {
                    "stale".into()
                },
                current,
                version: row.get("version"),
                cursor: row.get("cursor"),
                total: row.get("total"),
                html_bytes: row.get("html_bytes"),
                facts_bytes: row.get("facts_bytes"),
                edge_count: row.get("edge_count"),
                reused: row.get("reused"),
                focus: current.then(|| row.get("focus")),
                error: row.get("error"),
                created_at: row.get("created_at"),
            }
        })
        .collect())
}
pub async fn change_for_owner(
    app: &App,
    id: &str,
    version: i64,
    remove: bool,
    session: &crate::model::Session,
) -> Result<()> {
    let _write = app.mutations.lock().await;
    current_owner(app, session).await?;
    change(app, id, version, remove).await
}
pub async fn change(app: &App, id: &str, version: i64, remove: bool) -> Result<()> {
    if app.clone_held.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(Error::invalid(
            "Analysis writes are paused for this recovered clone.",
        ));
    }
    let sql = if remove {
        "DELETE FROM analysis_jobs WHERE id=$1 AND version=$2"
    } else {
        "UPDATE analysis_jobs SET state='cancelled',token='',lease_until=0,version=version+1,updated_at=$3 WHERE id=$1 AND version=$2 AND state IN ('rendering','classifying','failed','stale')"
    };
    let mut q = sqlx::query(sql).bind(id).bind(version);
    if !remove {
        q = q.bind(crate::now());
    }
    if q.execute(&app.db.pool).await?.rows_affected() != 1 {
        return Err(Error::conflict());
    }
    Ok(())
}
struct Claim {
    id: String,
    state: String,
    epoch: i64,
    version: i64,
    cursor: i64,
    total: i64,
    token: String,
    html_bytes: i64,
    facts_bytes: i64,
    edge_count: i64,
    reused: i64,
}

#[derive(Serialize)]
pub struct SourcePage {
    pub ordinal: i64,
    pub path: String,
    pub publication: String,
    pub without_inbound: bool,
}
#[derive(Serialize)]
pub struct SourceDetails {
    pub path: String,
    pub publication: String,
    pub html_sha256: String,
    pub observations: super::text::Observation,
    pub text: super::text::PageText,
    pub issue_count: usize,
    pub issues: Vec<crate::discovery::rendered_links::Edge>,
}
async fn readable(app: &App, id: &str) -> Result<(i64, String)> {
    let row = sqlx::query("SELECT epoch,identity,focus,state FROM analysis_jobs WHERE id=$1")
        .bind(id)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
    let epoch: i64 = row.get("epoch");
    if row.get::<String, _>("state") != "complete"
        || epoch != super::epoch::current(app).await?
        || row.get::<String, _>("identity") != identity(app)?
    {
        return Err(Error::invalid(
            "Results are incomplete or stale; start a fresh run. Retained source details are hidden.",
        ));
    }
    Ok((epoch, row.get("focus")))
}
pub async fn sources(app: &App, id: &str, after: i64) -> Result<Vec<SourcePage>> {
    if after < -1 {
        return Err(Error::invalid("Invalid source cursor."));
    }
    let _read = app.mutations.lock().await;
    let (epoch, _) = readable(app, id).await?;
    let rows=sqlx::query("SELECT ordinal,path,publication,CASE WHEN publication<>'' AND NOT EXISTS(SELECT 1 FROM analysis_inbound i WHERE i.job_id=p.job_id AND i.destination=p.path) THEN 1 ELSE 0 END AS without_inbound FROM analysis_pages p WHERE job_id=$1 AND ordinal>$2 ORDER BY ordinal LIMIT 41").bind(id).bind(after).fetch_all(&app.db.pool).await?;
    if epoch != super::epoch::current(app).await? {
        return Err(Error::conflict());
    }
    Ok(rows
        .into_iter()
        .map(|r| SourcePage {
            ordinal: r.get("ordinal"),
            path: r.get("path"),
            publication: r.get("publication"),
            without_inbound: r.get::<i64, _>("without_inbound") == 1,
        })
        .collect())
}
pub async fn details(app: &App, id: &str, ordinal: i64) -> Result<SourceDetails> {
    let _read = app.mutations.lock().await;
    let (epoch, focus) = readable(app, id).await?;
    let row=sqlx::query("SELECT path,publication,html_sha256,text,edges FROM analysis_pages WHERE job_id=$1 AND ordinal=$2").bind(id).bind(ordinal).fetch_optional(&app.db.pool).await?.ok_or_else(Error::not_found)?;
    let text: super::text::PageText = decode(&row.get::<String, _>("text"))?;
    let edges: Vec<crate::discovery::rendered_links::Edge> =
        decode(&row.get::<String, _>("edges"))?;
    let issues: Vec<_> = edges
        .into_iter()
        .filter(|e| {
            e.state.starts_with("broken")
                || ["invalid-url", "redirect-refused"].contains(&e.state.as_str())
        })
        .collect();
    let issue_count = issues.len();
    if epoch != super::epoch::current(app).await? {
        return Err(Error::conflict());
    }
    Ok(SourceDetails {
        path: row.get("path"),
        publication: row.get("publication"),
        html_sha256: row.get("html_sha256"),
        observations: text.observe(&focus)?,
        text,
        issue_count,
        issues: issues.into_iter().take(100).collect(),
    })
}
async fn claim(app: &App) -> Result<Option<Claim>> {
    let epoch = super::epoch::current(app).await?;
    let identity = identity(app)?;
    sqlx::query("UPDATE analysis_jobs SET state='stale',token='',lease_until=0,version=version+1 WHERE state IN ('rendering','classifying') AND (epoch<>$1 OR identity<>$2)").bind(epoch).bind(identity).execute(&app.db.pool).await?;
    let row=sqlx::query("SELECT id,state,epoch,version,cursor,total,html_bytes,facts_bytes,edge_count,reused FROM analysis_jobs WHERE state IN ('rendering','classifying') AND lease_until<=$1 ORDER BY sequence LIMIT 1").bind(crate::now()).fetch_optional(&app.db.pool).await?;
    let Some(row) = row else { return Ok(None) };
    let id: String = row.get("id");
    let version: i64 = row.get("version");
    let token = uuid::Uuid::new_v4().to_string();
    if sqlx::query("UPDATE analysis_jobs SET token=$1,lease_until=$2,version=version+1 WHERE id=$3 AND version=$4 AND state IN ('rendering','classifying') AND lease_until<=$5").bind(&token).bind(crate::now()+app.config.analysis.lease_seconds).bind(&id).bind(version).bind(crate::now()).execute(&app.db.pool).await?.rows_affected()!=1 {return Ok(None)};
    Ok(Some(Claim {
        id,
        state: row.get("state"),
        epoch: row.get("epoch"),
        version: version + 1,
        cursor: row.get("cursor"),
        total: row.get("total"),
        token,
        html_bytes: row.get("html_bytes"),
        facts_bytes: row.get("facts_bytes"),
        edge_count: row.get("edge_count"),
        reused: row.get("reused"),
    }))
}
struct Rendered {
    ordinal: i64,
    digest: String,
    html_bytes: usize,
    text: String,
    ids: String,
    links: String,
    reused: bool,
}
async fn render_batch(app: &App, claim: &Claim) -> Result<Vec<Rendered>> {
    let rows=sqlx::query("SELECT ordinal,path,locale,slug FROM analysis_pages WHERE job_id=$1 AND ordinal>=$2 ORDER BY ordinal LIMIT $3").bind(&claim.id).bind(claim.cursor).bind(app.config.analysis.batch_sources as i64).fetch_all(&app.db.pool).await?;
    let mut batch = Vec::new();
    for (offset, row) in rows.into_iter().enumerate() {
        check_ordinal(claim, offset, row.get("ordinal"))?;
        let slug: String = row.get("slug");
        let html = crate::web::render_anonymous_source(
            app,
            row.get("locale"),
            (!slug.is_empty()).then_some(slug),
        )
        .await?;
        let html_bytes = html.len();
        let digest = crate::auth::digest(html.as_bytes());
        let prior=sqlx::query("SELECT p.text,p.ids,p.links FROM analysis_pages p JOIN analysis_jobs j ON j.id=p.job_id WHERE p.job_id<>$1 AND p.path=$2 AND p.html_sha256=$3 AND p.text<>'' AND j.identity=$4 LIMIT 1").bind(&claim.id).bind(row.get::<String,_>("path")).bind(&digest).bind(identity(app)?).fetch_optional(&app.db.pool).await?;
        let (text, ids, links, reused) = if let Some(old) = prior {
            (old.get("text"), old.get("ids"), old.get("links"), true)
        } else {
            tokio::task::spawn_blocking(move || {
                let (dom, parsed) = crate::discovery::rendered_links::document(&html)?;
                let page = super::text::from_document(&dom.document)?;
                Ok::<_, Error>((
                    encode(&page)?,
                    encode(&parsed.ids)?,
                    encode(&parsed.links)?,
                    false,
                ))
            })
            .await
            .map_err(|_| Error::invalid("Analysis parser interrupted."))??
        };
        batch.push(Rendered {
            ordinal: row.get("ordinal"),
            digest,
            html_bytes,
            text,
            ids,
            links,
            reused,
        });
    }
    Ok(batch)
}
fn encode<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|_| Error::invalid("Invalid derived analysis facts."))
}
fn decode<T: for<'a> Deserialize<'a>>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(|_| {
        Error::invalid("Derived analysis facts are damaged; remove and retry the run.")
    })
}
struct Classified {
    ordinal: i64,
    edges: String,
    inbound: BTreeSet<(String, String)>,
}
async fn classify_batch(app: &App, claim: &Claim) -> Result<Vec<Classified>> {
    let (discovery, _) = crate::discovery::load(app).await?;
    let plan = crate::discovery::redirects::plan(app).await?;
    let paths: BTreeSet<String> =
        sqlx::query_scalar("SELECT path FROM analysis_pages WHERE job_id=$1")
            .bind(&claim.id)
            .fetch_all(&app.db.pool)
            .await?
            .into_iter()
            .collect();
    let rows=sqlx::query("SELECT ordinal,path,links FROM analysis_pages WHERE job_id=$1 AND ordinal>=$2 ORDER BY ordinal LIMIT $3").bind(&claim.id).bind(claim.cursor).bind(app.config.analysis.batch_sources as i64).fetch_all(&app.db.pool).await?;
    let origin = url::Url::parse(&app.config.origin())
        .map_err(|_| Error::invalid("Invalid site origin."))?;
    let mut resolved = BTreeMap::new();
    let mut fragment_bytes = 0usize;
    let mut fragments = BTreeMap::<String, crate::discovery::rendered_links::Parsed>::new();
    let mut batch = Vec::new();
    for (offset, row) in rows.into_iter().enumerate() {
        check_ordinal(claim, offset, row.get("ordinal"))?;
        let source: String = row.get("path");
        let base = origin
            .join(&source)
            .map_err(|_| Error::invalid("Invalid source path."))?;
        let links: Vec<(String, String, String)> = decode(&row.get::<String, _>("links"))?;
        let mut edges = Vec::new();
        let mut inbound = BTreeSet::new();
        for (href, element, placement) in links {
            let mut edge = crate::discovery::rendered_links::Edge {
                source: source.clone(),
                target: href.clone(),
                element: element.clone(),
                placement,
                state: String::new(),
                destination: None,
            };
            match base.join(&href) {
                Err(_) => edge.state = "invalid-url".into(),
                Ok(url) if !["http", "https"].contains(&url.scheme()) => {
                    edge.state = "non-http-skipped".into()
                }
                Ok(url) if url.origin() != origin.origin() => {
                    edge.state = "external-skipped".into()
                }
                Ok(url) if !url.username().is_empty() || url.password().is_some() => {
                    edge.state = "credentials-skipped".into()
                }
                Ok(url) if url.query().is_some() => edge.state = "query-state-unscanned".into(),
                Ok(url) => {
                    let path = url.path();
                    if !resolved.contains_key(path) {
                        if resolved.len() >= 4096 {
                            resolved.clear();
                        }
                        resolved.insert(
                            path.to_string(),
                            crate::discovery::rendered_links::target(
                                app, &plan, &discovery, path, &paths,
                            )
                            .await?,
                        );
                    }
                    let outcome = &resolved[path];
                    edge.state = outcome.state.clone();
                    edge.destination = outcome.destination.clone();
                    if let Some(destination) = &edge.destination {
                        if let Some(raw) = url.fragment().filter(|s| !s.is_empty()) {
                            if raw.contains(":~:") {
                                edge.state = "fragment-directive-unscanned".into();
                            } else {
                                if !fragments.contains_key(destination) {
                                    let ids:String=sqlx::query_scalar("SELECT ids FROM analysis_pages WHERE job_id=$1 AND path=$2").bind(&claim.id).bind(destination).fetch_one(&app.db.pool).await?;
                                    if fragment_bytes + ids.len() > 4 * 1024 * 1024 {
                                        fragments.clear();
                                        fragment_bytes = 0;
                                    }
                                    fragment_bytes += ids.len();
                                    fragments.insert(
                                        destination.clone(),
                                        crate::discovery::rendered_links::Parsed {
                                            ids: decode(&ids)?,
                                            links: Vec::new(),
                                        },
                                    );
                                }
                                if !fragments[destination].has_fragment(raw) {
                                    edge.state = "broken-fragment".into();
                                }
                            }
                        }
                        if element == "a"
                            && source != *destination
                            && !["broken-fragment", "fragment-directive-unscanned"]
                                .contains(&edge.state.as_str())
                        {
                            inbound.insert((source.clone(), destination.clone()));
                        }
                    }
                }
            }
            edges.push(edge);
        }
        batch.push(Classified {
            ordinal: row.get("ordinal"),
            edges: encode(&edges)?,
            inbound,
        });
    }
    Ok(batch)
}
fn check_ordinal(claim: &Claim, offset: usize, ordinal: i64) -> Result<()> {
    if claim.cursor < 0 || ordinal != claim.cursor + offset as i64 || ordinal >= claim.total {
        return Err(Error::invalid(
            "Derived source declarations are damaged; remove and retry.",
        ));
    }
    Ok(())
}
pub async fn tick(app: &App) -> Result<usize> {
    tick_as(app, None).await
}
pub async fn tick_for_owner(
    app: &App,
    session: &crate::model::Session,
    id: &str,
    version: i64,
) -> Result<usize> {
    tick_as(app, Some((session, id, version))).await
}
async fn tick_as(app: &App, owner: Option<(&crate::model::Session, &str, i64)>) -> Result<usize> {
    if !app.config.analysis.enabled || app.clone_held.load(std::sync::atomic::Ordering::SeqCst) {
        return Ok(0);
    };
    let _work = app.audit_work.lock().await;
    let _read = app.mutations.lock().await;
    if let Some((session, id, version)) = owner {
        current_owner(app, session).await?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM analysis_jobs WHERE id=$1 AND version=$2 AND epoch=$3 AND identity=$4 AND state IN ('rendering','classifying')")
            .bind(id).bind(version).bind(super::epoch::current(app).await?).bind(identity(app)?).fetch_one(&app.db.pool).await?;
        if count != 1 {
            return Err(Error::conflict());
        }
    }
    sqlx::query("DELETE FROM analysis_jobs WHERE created_at<$1")
        .bind(crate::now() - app.config.analysis.retention_seconds)
        .execute(&app.db.pool)
        .await?;
    let Some(claim) = claim(app).await? else {
        return Ok(0);
    };
    let deadline = std::time::Duration::from_secs(app.config.analysis.batch_seconds);
    let result = tokio::time::timeout(deadline, advance(app, &claim)).await;
    match result {
        Ok(Ok(count)) => Ok(count),
        other => {
            let code = if other.is_err() {
                "batch-deadline"
            } else {
                "source-or-work-budget"
            };
            sqlx::query("UPDATE analysis_jobs SET state='failed',error=$1,token='',lease_until=0,version=version+1,updated_at=$2 WHERE id=$3 AND version=$4 AND token=$5 AND state=$6").bind(code).bind(crate::now()).bind(&claim.id).bind(claim.version).bind(&claim.token).bind(&claim.state).execute(&app.db.pool).await?;
            tracing::warn!(event="analysis_batch_failed",job_id=%claim.id,reason=code);
            Ok(0)
        }
    }
}
async fn advance(app: &App, claim: &Claim) -> Result<usize> {
    let mut html = claim.html_bytes;
    let mut facts = claim.facts_bytes;
    let mut edge_count = claim.edge_count;
    let mut reused = claim.reused;
    let rendered = if claim.state == "rendering" {
        render_batch(app, claim).await?
    } else {
        Vec::new()
    };
    let classified = if claim.state == "classifying" {
        classify_batch(app, claim).await?
    } else {
        Vec::new()
    };
    for page in &rendered {
        html += page.html_bytes as i64;
        facts += page.digest.len() as i64;
        facts += (page.text.len() + page.ids.len() + page.links.len()) as i64;
        edge_count += decode::<Vec<(String, String, String)>>(&page.links)?.len() as i64;
        reused += i64::from(page.reused);
    }
    for page in &classified {
        facts += (page.edges.len() + encode(&page.inbound)?.len()) as i64;
    }
    if html > app.config.analysis.max_html_bytes as i64
        || facts > app.config.analysis.max_facts_bytes as i64
        || edge_count > app.config.analysis.max_edges as i64
    {
        return Err(Error::invalid(
            "Analysis exceeds its aggregate work budget.",
        ));
    }
    let count = rendered.len() + classified.len();
    if count == 0 {
        return Err(Error::invalid("Analysis cursor has no declared source."));
    }
    let mut tx = app.db.begin_write().await?;
    if !lock_epoch(&mut tx, claim.epoch).await? {
        sqlx::query("UPDATE analysis_jobs SET state='stale',token='',lease_until=0,version=version+1 WHERE id=$1 AND version=$2 AND token=$3").bind(&claim.id).bind(claim.version).bind(&claim.token).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(0);
    }
    let next = claim.cursor + count as i64;
    let (state, cursor) = if next == claim.total {
        if claim.state == "rendering" {
            ("classifying", 0)
        } else {
            ("complete", next)
        }
    } else {
        (claim.state.as_str(), next)
    };
    let changed=sqlx::query("UPDATE analysis_jobs SET state=$1,cursor=$2,html_bytes=$3,facts_bytes=$4,edge_count=$5,reused=$6,token='',lease_until=0,version=version+1,updated_at=$7 WHERE id=$8 AND version=$9 AND token=$10 AND state=$11 AND lease_until>$7").bind(state).bind(cursor).bind(html).bind(facts).bind(edge_count).bind(reused).bind(crate::now()).bind(&claim.id).bind(claim.version).bind(&claim.token).bind(&claim.state).execute(&mut *tx).await?.rows_affected();
    if changed != 1 {
        return Ok(0);
    };
    for page in rendered {
        sqlx::query("UPDATE analysis_pages SET html_sha256=$1,html_bytes=$2,text=$3,ids=$4,links=$5 WHERE job_id=$6 AND ordinal=$7").bind(page.digest).bind(page.html_bytes as i64).bind(page.text).bind(page.ids).bind(page.links).bind(&claim.id).bind(page.ordinal).execute(&mut *tx).await?;
    }
    for page in classified {
        sqlx::query("UPDATE analysis_pages SET edges=$1 WHERE job_id=$2 AND ordinal=$3")
            .bind(page.edges)
            .bind(&claim.id)
            .bind(page.ordinal)
            .execute(&mut *tx)
            .await?;
        for (source, destination) in page.inbound {
            sqlx::query("INSERT INTO analysis_inbound(job_id,source,destination) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind(&claim.id).bind(source).bind(destination).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    tracing::info!(event="analysis_batch_committed",job_id=%claim.id,state,sources=count);
    Ok(count)
}
