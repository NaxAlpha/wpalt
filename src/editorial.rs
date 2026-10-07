//! Private editorial decisions are separate from immutable content revisions.
//! Public projection never reads these tables; approval binds material and policy.
use crate::{
    App, auth,
    error::{Error, Result},
    model::{Post, PostInput, Session},
    now,
    schema::Registry,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Any, Row, Transaction, any::AnyRow};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS editorial_work(post_id TEXT PRIMARY KEY REFERENCES posts(id) ON DELETE CASCADE,content_version BIGINT NOT NULL CHECK(content_version>0),version BIGINT NOT NULL CHECK(version>0),state TEXT NOT NULL CHECK(state IN ('draft','pending','approved','changes','scheduled','published')),edited_by TEXT NOT NULL,assigned_to TEXT NOT NULL DEFAULT '',requested_by TEXT NOT NULL DEFAULT '',approved_by TEXT NOT NULL DEFAULT '',fingerprint TEXT NOT NULL,policy TEXT NOT NULL,notes TEXT NOT NULL DEFAULT '',requested_at BIGINT NOT NULL DEFAULT 0,decided_at BIGINT NOT NULL DEFAULT 0,updated_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS editorial_assignment ON editorial_work(assigned_to,state,requested_at,post_id);
CREATE INDEX IF NOT EXISTS editorial_requested ON editorial_work(requested_by,state,requested_at,post_id);
CREATE INDEX IF NOT EXISTS editorial_reviewers ON users(role,name,id);
CREATE INDEX IF NOT EXISTS editorial_queue ON editorial_work(state,requested_at,post_id);
CREATE TABLE IF NOT EXISTS editorial_decisions(id TEXT PRIMARY KEY,post_id TEXT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,workflow_version BIGINT NOT NULL CHECK(workflow_version>0),content_version BIGINT NOT NULL CHECK(content_version>0),action TEXT NOT NULL,actor_id TEXT NOT NULL,reviewer_id TEXT NOT NULL DEFAULT '',notes TEXT NOT NULL DEFAULT '',created_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS editorial_actor ON editorial_decisions(actor_id,id);
CREATE UNIQUE INDEX IF NOT EXISTS editorial_decision_version ON editorial_decisions(post_id,workflow_version);
CREATE INDEX IF NOT EXISTS editorial_history ON editorial_decisions(post_id,workflow_version DESC,id);
"#;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Work {
    pub post_id: String,
    pub content_version: i64,
    pub version: i64,
    pub state: String,
    pub edited_by: String,
    pub assigned_to: String,
    pub requested_by: String,
    pub approved_by: String,
    pub fingerprint: String,
    pub policy: String,
    pub notes: String,
    pub requested_at: i64,
    pub decided_at: i64,
    pub updated_at: i64,
}
impl Work {
    pub fn from_row(r: AnyRow) -> Self {
        Self {
            post_id: r.get("post_id"),
            content_version: r.get("content_version"),
            version: r.get("version"),
            state: r.get("state"),
            edited_by: r.get("edited_by"),
            assigned_to: r.get("assigned_to"),
            requested_by: r.get("requested_by"),
            approved_by: r.get("approved_by"),
            fingerprint: r.get("fingerprint"),
            policy: r.get("policy"),
            notes: r.get("notes"),
            requested_at: r.get("requested_at"),
            decided_at: r.get("decided_at"),
            updated_at: r.get("updated_at"),
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub reviewer_id: String,
    #[serde(default)]
    pub notes: String,
}
fn text(notes: &str) -> Result<()> {
    if notes.len() > 2000
        || notes
            .chars()
            .any(|c| c.is_control() && !['\n', '\r', '\t'].contains(&c))
    {
        return Err(Error::invalid(
            "Review notes are limited to 2,000 bytes of plain text.",
        ));
    }
    Ok(())
}
pub fn policy(registry: &Registry, kind: &str) -> Result<String> {
    let model = registry.models.get(kind).ok_or_else(Error::not_found)?;
    Ok(auth::digest(
        json!({"common":registry.common,"model":model})
            .to_string()
            .as_bytes(),
    ))
}
fn classification(name: &str) -> String {
    name.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
fn terms(names: &str) -> BTreeSet<String> {
    names
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(classification)
        .collect()
}
pub fn fingerprint(input: &PostInput) -> Result<String> {
    let fields: Value = serde_json::from_str(&input.fields)
        .map_err(|_| Error::invalid("Invalid reviewed fields."))?;
    let document: Value = serde_json::from_str(&input.document)
        .map_err(|_| Error::invalid("Save the current document before requesting review."))?;
    let seo: Value = serde_json::from_str(&input.seo)
        .map_err(|_| Error::invalid("Invalid reviewed discovery values."))?;
    let custom: BTreeMap<String, Vec<String>> = serde_json::from_str(&input.taxonomies)
        .map_err(|_| Error::invalid("Invalid reviewed classifications."))?;
    let custom: BTreeMap<_, BTreeSet<_>> = custom
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().map(|s| classification(&s)).collect()))
        .collect();
    Ok(auth::digest(json!({"kind":input.kind,"title":input.title.trim(),"slug":input.slug,"document":document,"fields":fields,"locale":input.locale,"translation_group":input.translation_group,"seo":seo,"categories":terms(&input.categories),"tags":terms(&input.tags),"taxonomies":custom}).to_string().as_bytes()))
}
pub async fn get(app: &App, id: &str) -> Result<Option<Work>> {
    Ok(sqlx::query("SELECT * FROM editorial_work WHERE post_id=$1")
        .bind(id)
        .fetch_optional(&app.db.pool)
        .await?
        .map(Work::from_row))
}
async fn get_tx(tx: &mut Transaction<'_, Any>, id: &str) -> Result<Option<Work>> {
    Ok(sqlx::query("SELECT * FROM editorial_work WHERE post_id=$1")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .map(Work::from_row))
}
async fn active(tx: &mut Transaction<'_, Any>, id: &str) -> Result<bool> {
    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id=$1 AND role IN ('admin','editor')")
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
    Ok(n == 1)
}
async fn participants_active(tx: &mut Transaction<'_, Any>, w: &Work) -> Result<bool> {
    let reviewer = if w.approved_by.is_empty() {
        &w.assigned_to
    } else {
        &w.approved_by
    };
    let ids = BTreeSet::from([
        w.edited_by.as_str(),
        w.requested_by.as_str(),
        reviewer.as_str(),
    ]);
    if ids.contains("") {
        return Ok(false);
    }
    let mut q = sqlx::QueryBuilder::<Any>::new(
        "SELECT COUNT(*) FROM users WHERE role IN ('admin','editor') AND id IN (",
    );
    let mut bind = q.separated(",");
    for id in &ids {
        bind.push_bind(id);
    }
    bind.push_unseparated(")");
    let rows = crate::db::Db::fetch_builder_in(tx, &mut q).await?;
    let n: i64 = rows[0].get(0);
    Ok(n as usize == ids.len())
}
async fn history(
    tx: &mut Transaction<'_, Any>,
    w: &Work,
    action: &str,
    actor: &str,
    notes: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO editorial_decisions(id,post_id,workflow_version,content_version,action,actor_id,reviewer_id,notes,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&w.post_id).bind(w.version).bind(w.content_version).bind(action).bind(actor).bind(&w.assigned_to).bind(notes).bind(now()).execute(&mut **tx).await?;
    // Twenty recent decisions per content record; no unbounded note/event log.
    sqlx::query("DELETE FROM editorial_decisions WHERE post_id=$1 AND id IN (SELECT id FROM editorial_decisions WHERE post_id=$1 ORDER BY workflow_version DESC,id DESC LIMIT 20 OFFSET 20)")
        .bind(&w.post_id).execute(&mut **tx).await?;
    Ok(())
}
/// Called inside the content write transaction after canonical normalization.
/// No query is added for models which have never required review.
pub(crate) async fn saved(
    tx: &mut Transaction<'_, Any>,
    post: &Post,
    input: &PostInput,
    registry: &Registry,
    session: &Session,
    approved: Option<&Work>,
) -> Result<Option<Work>> {
    if !registry.models[&post.kind].review_required {
        return Ok(None);
    }
    let material = fingerprint(input)?;
    let policy = policy(registry, &post.kind)?;
    let state = match input.action.as_str() {
        "publish" => "published",
        "schedule" => "scheduled",
        _ => "draft",
    };
    let prior = approved;
    let previous = if prior.is_none() {
        get_tx(tx, &post.id).await?
    } else {
        None
    };
    let row=sqlx::query("INSERT INTO editorial_work(post_id,content_version,version,state,edited_by,assigned_to,requested_by,approved_by,fingerprint,policy,requested_at,decided_at,updated_at) VALUES($1,$2,1,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(post_id) DO UPDATE SET content_version=excluded.content_version,version=editorial_work.version+1,state=excluded.state,edited_by=excluded.edited_by,assigned_to=excluded.assigned_to,requested_by=excluded.requested_by,approved_by=excluded.approved_by,fingerprint=excluded.fingerprint,policy=excluded.policy,requested_at=excluded.requested_at,decided_at=excluded.decided_at,updated_at=excluded.updated_at RETURNING *")
        .bind(&post.id).bind(post.version).bind(state).bind(prior.map(|w|w.edited_by.as_str()).unwrap_or(&session.user.id))
        .bind(prior.map(|w|w.assigned_to.as_str()).unwrap_or(""))
        .bind(prior.map(|w|w.requested_by.as_str()).unwrap_or(""))
        .bind(prior.map(|w|w.approved_by.as_str()).unwrap_or(""))
        .bind(material).bind(policy).bind(prior.map(|w|w.requested_at).unwrap_or(0)).bind(prior.map(|w|w.decided_at).unwrap_or(0)).bind(now()).fetch_one(&mut **tx).await?;
    let w = Work::from_row(row);
    if prior.is_some() {
        history(tx, &w, state, &session.user.id, "").await?;
    } else if previous
        .is_some_and(|old| ["pending", "approved", "scheduled"].contains(&old.state.as_str()))
    {
        history(
            tx,
            &w,
            "invalidate",
            &session.user.id,
            "Working material changed; request review again.",
        )
        .await?;
    }
    Ok(Some(w))
}
pub(crate) async fn require_approval(
    tx: &mut Transaction<'_, Any>,
    post: &Post,
    input: &PostInput,
    registry: &Registry,
) -> Result<Option<Work>> {
    if !registry.models[&post.kind].review_required
        || !["publish", "schedule"].contains(&input.action.as_str())
    {
        return Ok(None);
    }
    let w = get_tx(tx, &post.id)
        .await?
        .ok_or_else(|| Error::invalid("Save this draft and request review before publication."))?;
    if w.state != "approved"
        || w.content_version != post.version
        || w.fingerprint != fingerprint(input)?
        || w.policy != policy(registry, &post.kind)?
        || w.approved_by.is_empty()
        || w.approved_by != w.assigned_to
        || w.approved_by == w.edited_by
        || w.approved_by == w.requested_by
        || !participants_active(tx, &w).await?
    {
        return Err(Error::invalid(
            "This exact material needs a current assigned review before publication.",
        ));
    }
    Ok(Some(w))
}
pub(crate) async fn request_tx(
    tx: &mut Transaction<'_, Any>,
    post: &Post,
    registry: &Registry,
    s: &Session,
    request: &Request,
) -> Result<Work> {
    text(&request.notes)?;
    if s.hash.starts_with("integration:") || !registry.models[&post.kind].review_required {
        return Err(Error::forbidden());
    }
    let w = get_tx(tx, &post.id)
        .await?
        .ok_or_else(|| Error::invalid("Save the current draft before requesting review."))?;
    if w.content_version != post.version || w.policy != policy(registry, &post.kind)? {
        return Err(Error::conflict());
    }
    if request.reviewer_id == s.user.id
        || request.reviewer_id == w.edited_by
        || !active(tx, &request.reviewer_id).await?
    {
        return Err(Error::invalid(
            "Choose another active editor or administrator to review this material.",
        ));
    }
    let row=sqlx::query("UPDATE editorial_work SET state='pending',assigned_to=$1,requested_by=$2,approved_by='',notes=$3,requested_at=$4,decided_at=0,updated_at=$4,version=version+1 WHERE post_id=$5 AND version=$6 RETURNING *")
        .bind(&request.reviewer_id).bind(&s.user.id).bind(&request.notes).bind(now()).bind(&post.id).bind(w.version).fetch_optional(&mut **tx).await?.ok_or_else(Error::conflict)?;
    let w = Work::from_row(row);
    history(tx, &w, "request", &s.user.id, &request.notes).await?;
    Ok(w)
}
/// Explicit API request on a previously saved draft; the native authoring path
/// saves and requests within one content transaction instead.
pub async fn request(
    app: &App,
    s: &Session,
    id: &str,
    content_version: i64,
    workflow_version: i64,
    input: &Request,
) -> Result<Work> {
    let _guard = app.mutation().await?;
    auth::current_editor(app, s).await?;
    if s.hash.starts_with("integration:") {
        return Err(Error::forbidden());
    }
    let registry = Registry::load(app).await?;
    let mut tx = app.db.pool.begin().await?;
    let post = sqlx::query("UPDATE posts SET version=version WHERE id=$1 RETURNING *")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .map(Post::from_row)
        .ok_or_else(Error::not_found)?;
    let current = get_tx(&mut tx, id)
        .await?
        .ok_or_else(|| Error::invalid("Save this draft before requesting review."))?;
    if post.version != content_version || current.version != workflow_version {
        return Err(Error::conflict());
    }
    let work = request_tx(&mut tx, &post, &registry, s, input).await?;
    tx.commit().await?;
    tracing::info!(event="editorial_review_requested",content_id=%id,workflow_version=work.version);
    Ok(work)
}
pub async fn decide(
    app: &App,
    s: &Session,
    id: &str,
    content_version: i64,
    workflow_version: i64,
    action: &str,
    notes: &str,
) -> Result<Work> {
    text(notes)?;
    if !["approve", "changes", "withdraw"].contains(&action) {
        return Err(Error::invalid("Choose approval, changes or withdrawal."));
    }
    let _guard = app.mutation().await?;
    auth::current_editor(app, s).await?;
    if s.hash.starts_with("integration:") {
        return Err(Error::forbidden());
    }
    let registry = Registry::load(app).await?;
    let mut tx = app.db.pool.begin().await?;
    let post = sqlx::query("UPDATE posts SET version=version WHERE id=$1 RETURNING *")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .map(Post::from_row)
        .ok_or_else(Error::not_found)?;
    let w = get_tx(&mut tx, id).await?.ok_or_else(Error::not_found)?;
    if post.version != content_version
        || w.content_version != content_version
        || w.version != workflow_version
        || w.policy != policy(&registry, &post.kind)?
    {
        return Err(Error::conflict());
    }
    if !registry.models[&post.kind].review_required {
        return Err(Error::forbidden());
    }
    if action == "withdraw" {
        if !["pending", "approved"].contains(&w.state.as_str()) || s.user.id != w.requested_by {
            return Err(Error::forbidden());
        }
    } else if w.state != "pending"
        || s.user.id != w.assigned_to
        || s.user.id == w.requested_by
        || s.user.id == w.edited_by
    {
        return Err(Error::forbidden());
    }
    if action == "approve" && !participants_active(&mut tx, &w).await? {
        return Err(Error::forbidden());
    }
    let state = match action {
        "approve" => "approved",
        "changes" => "changes",
        _ => "draft",
    };
    let row=sqlx::query("UPDATE editorial_work SET state=$1,approved_by=$2,notes=$3,decided_at=$4,updated_at=$4,version=version+1 WHERE post_id=$5 AND version=$6 RETURNING *")
        .bind(state).bind(if action=="approve"{s.user.id.as_str()}else{""}).bind(notes).bind(now()).bind(id).bind(workflow_version).fetch_optional(&mut *tx).await?.ok_or_else(Error::conflict)?;
    let work = Work::from_row(row);
    history(&mut tx, &work, action, &s.user.id, notes).await?;
    tx.commit().await?;
    tracing::info!(event="editorial_review_decided",content_id=%id,action=%action,workflow_version=work.version);
    Ok(work)
}

pub(crate) async fn scheduled_allowed(
    tx: &mut Transaction<'_, Any>,
    post: &Post,
    registry: &Registry,
    categories: &str,
    tags: &str,
    taxonomies: &BTreeMap<String, Vec<String>>,
) -> Result<bool> {
    if !registry
        .models
        .get(&post.kind)
        .is_some_and(|m| m.review_required)
    {
        return Ok(true);
    }
    let Some(w) = get_tx(tx, &post.id).await? else {
        return Ok(false);
    };
    if w.state != "scheduled"
        || w.content_version != post.version
        || w.policy != policy(registry, &post.kind)?
        || w.approved_by.is_empty()
        || w.approved_by != w.assigned_to
        || w.approved_by == w.edited_by
        || w.approved_by == w.requested_by
        || !participants_active(tx, &w).await?
    {
        return Ok(false);
    }
    let input = PostInput {
        import_markdown: false,
        locale: post.locale.clone(),
        translation_group: post.translation_group.clone(),
        seo: post.seo.clone(),
        title: post.title.clone(),
        slug: post.slug.clone(),
        kind: post.kind.clone(),
        body: post.body.clone(),
        document: post.document.clone(),
        fields: post.fields.clone(),
        blocks: post.blocks.clone(),
        categories: categories.into(),
        tags: tags.into(),
        taxonomies: serde_json::to_string(taxonomies)
            .map_err(|_| Error::invalid("Invalid scheduled classification."))?,
        version: post.version,
        action: "publish".into(),
        publish_at: 0,
        csrf: String::new(),
    };
    Ok(w.fingerprint == fingerprint(&input)?)
}
pub(crate) async fn scheduled_result(
    tx: &mut Transaction<'_, Any>,
    post: &Post,
    published: bool,
) -> Result<()> {
    let note = if published {
        ""
    } else {
        "Scheduled publication stopped: current material, policy or reviewer authority needs review."
    };
    let row=sqlx::query("UPDATE editorial_work SET state=$1,content_version=$2,version=version+1,notes=CASE WHEN $3='' THEN notes ELSE $3 END,approved_by=CASE WHEN $4=1 THEN approved_by ELSE '' END,updated_at=$5 WHERE post_id=$6 RETURNING *")
        .bind(if published{"published"}else{"changes"}).bind(post.version).bind(note).bind(i64::from(published)).bind(now()).bind(&post.id).fetch_optional(&mut **tx).await?;
    if let Some(row) = row {
        history(
            tx,
            &Work::from_row(row),
            if published {
                "scheduled_publish"
            } else {
                "scheduled_cancel"
            },
            "",
            note,
        )
        .await?;
    }
    Ok(())
}

/// Archive validation is independent of the live database and runs before writes.
pub(crate) fn validate_archive(
    tables: &BTreeMap<String, Vec<BTreeMap<String, Value>>>,
) -> Result<()> {
    let invalid = || Error::invalid("Invalid archived editorial graph.");
    let users: BTreeSet<_> = tables["users"]
        .iter()
        .filter_map(|r| r["id"].as_str())
        .collect();
    let posts: BTreeMap<_, _> = tables["posts"]
        .iter()
        .map(|r| (r["id"].as_str().unwrap(), r["version"].as_i64().unwrap()))
        .collect();
    let mut work = BTreeMap::new();
    let actor = |id: &str, empty: bool| {
        (empty && id.is_empty()) || (uuid::Uuid::parse_str(id).is_ok() && users.contains(id))
    };
    for row in &tables["editorial_work"] {
        let w: Work = serde_json::from_value(serde_json::to_value(row).map_err(|_| invalid())?)
            .map_err(|_| invalid())?;
        if w.version <= 0
            || w.content_version <= 0
            || posts
                .get(w.post_id.as_str())
                .is_none_or(|v| *v < w.content_version)
            || work.contains_key(&w.post_id)
            || ![
                "draft",
                "pending",
                "approved",
                "changes",
                "scheduled",
                "published",
            ]
            .contains(&w.state.as_str())
            || !actor(&w.edited_by, false)
            || !actor(&w.assigned_to, true)
            || !actor(&w.requested_by, true)
            || !actor(&w.approved_by, true)
            || [&w.fingerprint, &w.policy]
                .iter()
                .any(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
            || [w.requested_at, w.decided_at, w.updated_at]
                .iter()
                .any(|n| *n < 0)
        {
            return Err(invalid());
        }
        text(&w.notes)?;
        if ["pending", "approved", "scheduled", "published"].contains(&w.state.as_str())
            && (w.requested_by.is_empty()
                || w.assigned_to.is_empty()
                || w.assigned_to == w.edited_by
                || w.assigned_to == w.requested_by)
        {
            return Err(invalid());
        }
        if ["approved", "scheduled", "published"].contains(&w.state.as_str())
            && w.approved_by != w.assigned_to
        {
            return Err(invalid());
        }
        if ["draft", "pending", "changes"].contains(&w.state.as_str()) && !w.approved_by.is_empty()
        {
            return Err(invalid());
        }
        work.insert(w.post_id.clone(), w);
    }
    let mut ids = BTreeSet::new();
    let mut versions = BTreeSet::new();
    let mut counts = BTreeMap::new();
    for row in &tables["editorial_decisions"] {
        let string = |key: &str| row[key].as_str().unwrap();
        let id = string("id");
        let post = string("post_id");
        let action = string("action");
        let actor_id = string("actor_id");
        let reviewer = string("reviewer_id");
        let w = work.get(post).ok_or_else(invalid)?;
        let version = row["workflow_version"].as_i64().unwrap();
        let content = row["content_version"].as_i64().unwrap();
        let count = counts.entry(post).or_insert(0);
        *count += 1;
        if uuid::Uuid::parse_str(id).is_err()
            || !ids.insert(id)
            || !versions.insert((post, version))
            || *count > 20
            || version <= 0
            || version > w.version
            || content <= 0
            || content > posts[post]
            || row["created_at"].as_i64().unwrap() < 0
            || ![
                "request",
                "approve",
                "changes",
                "withdraw",
                "invalidate",
                "published",
                "scheduled",
                "scheduled_publish",
                "scheduled_cancel",
            ]
            .contains(&action)
            || !actor(
                actor_id,
                ["scheduled_publish", "scheduled_cancel"].contains(&action),
            )
            || !actor(reviewer, true)
        {
            return Err(invalid());
        }
        text(string("notes"))?;
    }
    Ok(())
}
