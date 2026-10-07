//! Explicit stopped-site translation draft duplication and selected field synchronization.
use crate::{
    App, auth, content,
    error::{Error, Result},
    model::{PostInput, Session, User},
};
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::BTreeMap;

pub struct Request<'a> {
    pub source: &'a str,
    pub locale: &'a str,
    pub slug: &'a str,
    pub target: Option<&'a str>,
    pub fields: &'a [String],
    pub execute: Option<&'a str>,
}

/// CLI owns the exclusive stopped-site lifecycle lock. Existing native save owns validation,
/// version conflicts, revisions and events. HTTP uses prepare_as with its actual session.
pub async fn prepare(app: &App, request: Request<'_>) -> Result<Value> {
    let owner = sqlx::query(
        "SELECT id,email,name,role FROM users WHERE role='admin' ORDER BY created_at,id LIMIT 1",
    )
    .fetch_optional(&app.db.pool)
    .await?
    .ok_or_else(Error::forbidden)?;
    let session = Session {
        user: User {
            id: owner.get("id"),
            email: owner.get("email"),
            name: owner.get("name"),
            role: owner.get("role"),
        },
        csrf: String::new(),
        hash: String::new(),
    };
    prepare_as(app, &session, request).await
}
/// Browser workflows use the actual current session, never the stopped CLI owner.
pub async fn prepare_as(app: &App, session: &Session, request: Request<'_>) -> Result<Value> {
    let guard = app.mutation().await?;
    auth::current_editor(app, session).await?;
    if session.hash.starts_with("integration:") {
        return Err(Error::forbidden());
    }
    let Request {
        source,
        locale,
        slug,
        target,
        fields,
        execute,
    } = request;
    if uuid::Uuid::parse_str(source).is_err()
        || fields.len() > 32
        || fields.iter().any(|f| !crate::schema::identifier(f))
    {
        return Err(Error::invalid(
            "Use a source UUID and up to 32 explicit field identifiers.",
        ));
    }
    let source = content::get(app, source).await?;
    crate::discovery::load(app).await?.0.language(locale)?;
    if source.locale == locale || source.translation_group.is_empty() {
        return Err(Error::invalid(
            "Configure a different language and assign the source translation group in the editor first.",
        ));
    }
    let existing = if let Some(id) = target {
        Some(content::get(app, id).await?)
    } else {
        None
    };
    if existing.is_none() {
        let restricted: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM member_resources WHERE kind='post' AND resource_id=$1",
        )
        .bind(&source.id)
        .fetch_one(&app.db.pool)
        .await?;
        if restricted != 0 {
            return Err(Error::invalid(
                "Restricted-source duplication requires an explicitly provisioned target access policy; create the protected target first and synchronize selected fields.",
            ));
        }
    }
    // Synchronization must not transfer protected fields into a weaker target policy.
    if let Some(existing) = &existing {
        let missing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM member_resources source WHERE source.kind='post' AND source.resource_id=$1 AND NOT EXISTS(SELECT 1 FROM member_resources target WHERE target.kind='post' AND target.resource_id=$2 AND target.policy_id=source.policy_id)")
            .bind(&source.id).bind(&existing.id).fetch_one(&app.db.pool).await?;
        if missing != 0 {
            return Err(Error::invalid(
                "Provision every source access policy on the target before synchronizing protected values.",
            ));
        }
    }
    if let Some(existing) = &existing {
        if existing.translation_group != source.translation_group
            || existing.locale != locale
            || existing.kind != source.kind
            || existing.slug != slug
            || fields.is_empty()
        {
            return Err(Error::invalid(
                "Sync requires a matching translation group, language, kind and slug, plus explicitly selected fields.",
            ));
        }
        if existing.status == "scheduled" {
            return Err(Error::invalid(
                "Cancel the target schedule before synchronizing unpublished translated fields.",
            ));
        }
    } else if !fields.is_empty() {
        return Err(Error::invalid(
            "Selected fields apply to an existing translation; new drafts duplicate the current source.",
        ));
    }
    let base = existing.as_ref().unwrap_or(&source);
    let terms = sqlx::query("SELECT t.name,t.kind FROM terms t JOIN post_terms pt ON pt.term_id=t.id WHERE pt.post_id=$1 ORDER BY t.name,t.kind")
        .bind(&base.id).fetch_all(&app.db.pool).await?;
    let names = |kind: &str| {
        terms
            .iter()
            .filter(|r| r.get::<String, _>("kind") == kind)
            .map(|r| r.get::<String, _>("name"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut custom = BTreeMap::<String, Vec<String>>::new();
    for row in &terms {
        let kind: String = row.get("kind");
        if !["category", "tag"].contains(&kind.as_str()) {
            custom.entry(kind).or_default().push(row.get("name"));
        }
    }
    let mut translated_fields: Value =
        serde_json::from_str(&base.fields).map_err(|_| Error::invalid("Invalid native fields."))?;
    let source_fields: Value = serde_json::from_str(&source.fields)
        .map_err(|_| Error::invalid("Invalid native source fields."))?;
    let translated_map = translated_fields
        .as_object_mut()
        .ok_or_else(|| Error::invalid("Native translated fields must be an object."))?;
    for name in fields {
        let value = source_fields.get(name).ok_or_else(|| {
            Error::invalid("Selected field is absent from source; deletion is not implicit.")
        })?;
        translated_map.insert(name.clone(), value.clone());
    }
    let input = PostInput {
        import_markdown: false,
        locale: locale.into(),
        translation_group: source.translation_group.clone(),
        seo: if existing.is_some() {
            base.seo.clone()
        } else {
            "{}".into()
        },
        title: base.title.clone(),
        slug: slug.into(),
        kind: base.kind.clone(),
        body: base.body.clone(),
        document: base.document.clone(),
        fields: translated_fields.to_string(),
        blocks: base.blocks.clone(),
        categories: names("category"),
        tags: names("tag"),
        taxonomies: serde_json::to_string(&custom)
            .map_err(|_| Error::invalid("Invalid native taxonomy."))?,
        version: existing.as_ref().map_or(0, |p| p.version),
        action: "save".into(),
        publish_at: 0,
        csrf: String::new(),
    };
    content::validate_input(&input, &app.db.settings().await?)?;
    let plan = auth::digest(
        &serde_json::to_vec(&json!({"source":source,"target":existing,"input":input}))
            .map_err(|_| Error::invalid("Cannot review translation plan."))?,
    );
    let mut report = json!({"format":"wpalt-translation-plan-v1","plan":plan,"source_id":source.id,"source_version":source.version,"target_id":target,"locale":locale,"slug":slug,"selected_fields":fields,"executed":false,
        "boundary":"Exact source/target review under one mutation boundary. Duplication creates an unscheduled draft with untranslated source text; translated SEO must be authored separately. Synchronization copies only explicitly named fields, preserving translated text, slug, SEO, taxonomy, publication and access rules. No background overwrite or automatic publication."});
    if let Some(reviewed) = execute {
        if reviewed != plan {
            return Err(Error::invalid(
                "Source, target or selection changed; review a fresh translation plan.",
            ));
        }
        let result = content::save_guarded(app, session, target, input, None, &guard)
            .await?
            .post;
        report["executed"] = true.into();
        report["result"] = json!({"id":result.id,"version":result.version,"status":result.status});
    }
    Ok(report)
}
