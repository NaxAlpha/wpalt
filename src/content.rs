use crate::{
    App,
    error::{Error, Result},
    model::{Block, NavItem, Post, PostInput, Session, Settings},
    now,
};
use serde_json::Value;
use sqlx::Row;

pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 120
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && ![
            "admin",
            "login",
            "logout",
            "account",
            "assets",
            "media",
            "api",
            "health",
            "search",
            "preview",
            "forms",
            "audience",
            "registration",
            "themes",
            "members",
            "shop",
            "commerce",
            "r",
        ]
        .contains(&slug)
}
pub fn markdown(text: &str) -> String {
    let mut rendered = String::new();
    pulldown_cmark::html::push_html(
        &mut rendered,
        pulldown_cmark::Parser::new_ext(
            text,
            pulldown_cmark::Options::ENABLE_TABLES | pulldown_cmark::Options::ENABLE_STRIKETHROUGH,
        ),
    );
    ammonia::Builder::default().clean(&rendered).to_string()
}
pub(crate) fn safe_nav_url(value: &str) -> bool {
    if value.contains('\\') || value.chars().any(char::is_control) {
        return false;
    }
    if value.starts_with('/') && !value.starts_with("//") {
        return true;
    }
    url::Url::parse(value).is_ok_and(|u| {
        ["http", "https"].contains(&u.scheme())
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
    })
}
pub fn validate_settings(s: &Settings) -> Result<()> {
    if s.title.trim().is_empty()
        || s.title.len() > 200
        || s.description.len() > 1000
        || !crate::schema::identifier(&s.theme)
    {
        return Err(Error::invalid(
            "Provide a site title, a short description and a supported theme.",
        ));
    }
    let nav: Vec<NavItem> = serde_json::from_str(&s.navigation)
        .map_err(|_| Error::invalid("Navigation must be a JSON array of label and url objects."))?;
    if nav.len() > 30
        || nav.iter().any(|n| {
            n.label.trim().is_empty()
                || n.label.len() > 100
                || n.url.len() > 2000
                || !safe_nav_url(&n.url)
        })
    {
        return Err(Error::invalid(
            "Navigation requires safe local paths or HTTP(S) URLs and short labels.",
        ));
    }
    let _: crate::schema::Definition = serde_json::from_str(&s.field_schema)
        .map_err(|_| Error::invalid("Field definitions must follow the current typed schema."))?;
    Ok(())
}
pub fn validate_input(p: &PostInput, s: &Settings) -> Result<()> {
    if p.title.trim().is_empty()
        || p.title.len() > 300
        || !valid_slug(&p.slug)
        || !crate::schema::identifier(&p.kind)
        || p.body.len() > 512 * 1024
        || p.fields.len() > 32 * 1024
        || p.blocks.len() > 64 * 1024
        || p.taxonomies.len() > 16 * 1024
        || p.tags.len() > 2000
        || p.categories.len() > 2000
        || !["save", "autosave", "publish", "schedule", "unpublish"].contains(&p.action.as_str())
    {
        return Err(Error::invalid(
            "Check title, slug, content type, action and content size.",
        ));
    }
    if p.action == "schedule" && (p.publish_at <= now() || p.publish_at > now() + 10 * 365 * 86400)
    {
        return Err(Error::invalid(
            "Choose a future publication time within ten years.",
        ));
    }
    let fields: Value = serde_json::from_str(&p.fields)
        .map_err(|_| Error::invalid("Fields must be valid JSON."))?;
    if !fields.is_object() {
        return Err(Error::invalid("Fields must be an object."));
    }
    let _ = s;
    let blocks: Vec<Block> = serde_json::from_str(&p.blocks)
        .map_err(|_| Error::invalid("Blocks must be a JSON array of kind and text objects."))?;
    if blocks.len() > 100
        || blocks.iter().any(|b| {
            !["heading", "text", "callout"].contains(&b.kind.as_str()) || b.text.len() > 8000
        })
    {
        return Err(Error::invalid(
            "Use heading, text or callout blocks within the size limits.",
        ));
    }
    for terms in [&p.categories, &p.tags] {
        if terms.split(',').filter(|s| !s.trim().is_empty()).count() > 30
            || terms.split(',').any(|s| s.trim().len() > 100)
        {
            return Err(Error::invalid("Use up to 30 short categories and tags."));
        }
    }
    Ok(())
}
pub async fn get(app: &App, id: &str) -> Result<Post> {
    sqlx::query("SELECT * FROM posts WHERE id=$1")
        .bind(id)
        .fetch_optional(&app.db.pool)
        .await?
        .map(Post::from_row)
        .ok_or_else(Error::not_found)
}
pub async fn save(
    app: &App,
    session: &Session,
    id: Option<&str>,
    mut input: PostInput,
) -> Result<Post> {
    if !session.can_edit() {
        return Err(Error::forbidden());
    }
    let _guard = app.mutation().await;
    let settings = app.db.settings().await?;
    validate_input(&input, &settings)?;
    let structured = !input.document.is_empty() && !input.import_markdown;
    let doc = if !structured {
        crate::document::import(&input.body, &input.blocks)?
    } else {
        crate::document::Document::parse(&input.document)?
    };
    let form_ids = doc.form_ids();
    if !form_ids.is_empty() {
        if !app.config.business_enabled || form_ids.len() > 128 {
            return Err(Error::invalid(
                "Enable forms and use at most 128 published form references.",
            ));
        }
        let mut query = sqlx::QueryBuilder::<sqlx::Any>::new(
            "SELECT id FROM business_forms WHERE published_version>0 AND id IN (",
        );
        let mut list = query.separated(",");
        for id in &form_ids {
            list.push_bind(id);
        }
        list.push_unseparated(")");
        if app.db.fetch_builder(&mut query).await?.len() != form_ids.len() {
            return Err(Error::invalid(
                "Publish each embedded form before saving the content.",
            ));
        }
    }
    input.document = doc.encode();
    // Imported source stays in the revision; canonical editing derives the search/export projection.
    if structured {
        input.body = doc.markdown();
        input.blocks = "[]".into();
    }
    let registry = crate::schema::Registry::load(app).await?;
    let values: Value = serde_json::from_str(&input.fields)
        .map_err(|_| Error::invalid("Fields must be valid JSON."))?;
    let fields = registry.fields_for(&input.kind)?;
    registry.validate_values(&fields, &values)?;
    registry.validate_references(app, &fields, &values).await?;
    let custom: std::collections::BTreeMap<String, Vec<String>> =
        serde_json::from_str(&input.taxonomies)
            .map_err(|_| Error::invalid("Taxonomies must be an object of term-name arrays."))?;
    let model = &registry.models[&input.kind];
    if custom.len() > 16
        || custom.iter().any(|(kind, names)| {
            !model.taxonomies.contains_key(kind)
                || ["category", "tag"].contains(&kind.as_str())
                || names.len() > 30
                || names.iter().any(|n| n.trim().is_empty() || n.len() > 100)
        })
    {
        return Err(Error::invalid(
            "Use declared custom taxonomies with up to 30 short term names.",
        ));
    }

    crate::discovery::validate_content(app, &input).await?;
    let mut tx = app.db.pool.begin().await?;
    let old = if let Some(id) = id {
        Some(
            sqlx::query("SELECT * FROM posts WHERE id=$1")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .map(Post::from_row)
                .ok_or_else(Error::not_found)?,
        )
    } else {
        None
    };
    if let Some(old) = &old {
        if old.version != input.version {
            return Err(Error::conflict());
        }
        if old.kind != input.kind {
            return Err(Error::invalid(
                "Content type cannot be changed after creation in M1.",
            ));
        }
    }
    let time = now();
    let mut post = old.clone().unwrap_or_else(|| Post {
        id: uuid::Uuid::new_v4().to_string(),
        slug: String::new(),
        kind: input.kind.clone(),
        title: String::new(),
        body: String::new(),
        document: String::new(),
        fields: "{}".into(),
        blocks: "[]".into(),
        status: "draft".into(),
        version: 0,
        published_slug: String::new(),
        published_title: String::new(),
        published_body: String::new(),
        published_document: crate::document::empty(),
        published_fields: "{}".into(),
        published_blocks: "[]".into(),
        publish_at: 0,
        published_at: 0,
        updated_at: time,
        author_id: session.user.id.clone(),
        locale: input.locale.clone(),
        translation_group: input.translation_group.clone(),
        seo: input.seo.clone(),
        published_locale: input.locale.clone(),
        published_translation_group: String::new(),
        published_seo: "{}".into(),
    });
    post.locale = input.locale.clone();
    post.translation_group = input.translation_group.clone();
    post.seo = input.seo.clone();
    post.title = input.title.trim().into();
    post.slug = input.slug.clone();
    post.body = input.body.clone();
    post.document = input.document.clone();
    post.fields = input.fields.clone();
    post.blocks = input.blocks.clone();
    post.version += 1;
    post.updated_at = time;
    match input.action.as_str() {
        "publish" => {
            post.status = "published".into();
            promote(&mut post, time);
        }
        "schedule" => {
            post.status = "scheduled".into();
            post.publish_at = input.publish_at;
        }
        "unpublish" => {
            post.status = "draft".into();
            post.publish_at = 0;
        }
        _ => {}
    }
    if old.is_some() {
        let result=sqlx::query("UPDATE posts SET slug=$1,title=$2,body=$3,fields=$4,blocks=$5,status=$6,version=$7,published_slug=$8,published_title=$9,published_body=$10,published_fields=$11,published_blocks=$12,publish_at=$13,published_at=$14,updated_at=$15,locale=$18,translation_group=$19,seo=$20,published_locale=$21,published_translation_group=$22,published_seo=$23,document=$24,published_document=$25 WHERE id=$16 AND version=$17")
            .bind(&post.slug).bind(&post.title).bind(&post.body).bind(&post.fields).bind(&post.blocks).bind(&post.status).bind(post.version).bind(&post.published_slug).bind(&post.published_title).bind(&post.published_body).bind(&post.published_fields).bind(&post.published_blocks).bind(post.publish_at).bind(post.published_at).bind(post.updated_at).bind(&post.id).bind(input.version).bind(&post.locale).bind(&post.translation_group).bind(&post.seo).bind(&post.published_locale).bind(&post.published_translation_group).bind(&post.published_seo).bind(&post.document).bind(&post.published_document).execute(&mut *tx).await?;
        if result.rows_affected() != 1 {
            return Err(Error::conflict());
        }
    } else {
        sqlx::query("INSERT INTO posts(id,slug,kind,title,body,fields,blocks,status,version,published_slug,published_title,published_body,published_fields,published_blocks,publish_at,published_at,updated_at,author_id,locale,translation_group,seo,published_locale,published_translation_group,published_seo,document,published_document) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26)")
            .bind(&post.id).bind(&post.slug).bind(&post.kind).bind(&post.title).bind(&post.body).bind(&post.fields).bind(&post.blocks).bind(&post.status).bind(post.version).bind(&post.published_slug).bind(&post.published_title).bind(&post.published_body).bind(&post.published_fields).bind(&post.published_blocks).bind(post.publish_at).bind(post.published_at).bind(post.updated_at).bind(&post.author_id).bind(&post.locale).bind(&post.translation_group).bind(&post.seo).bind(&post.published_locale).bind(&post.published_translation_group).bind(&post.published_seo).bind(&post.document).bind(&post.published_document).execute(&mut *tx).await?;
    }
    sqlx::query("DELETE FROM post_terms WHERE post_id=$1")
        .bind(&post.id)
        .execute(&mut *tx)
        .await?;
    let mut all_terms = custom.clone();
    for (kind, names) in [("category", &input.categories), ("tag", &input.tags)] {
        if !names.trim().is_empty() && !model.taxonomies.contains_key(kind) {
            return Err(Error::invalid("This model does not define that taxonomy."));
        }
        all_terms.insert(
            kind.into(),
            names
                .split(',')
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(String::from)
                .collect(),
        );
    }
    for (kind, names) in &all_terms {
        for name in names {
            let slug = term_slug(name);
            if slug.is_empty() {
                return Err(Error::invalid(
                    "Term names need at least one ASCII letter or digit.",
                ));
            }
            sqlx::query("INSERT INTO terms(id,name,slug,kind) VALUES($1,$2,$3,$4) ON CONFLICT(kind,slug) DO NOTHING").bind(uuid::Uuid::new_v4().to_string()).bind(name.trim()).bind(&slug).bind(kind).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO post_terms(post_id,term_id) SELECT $1,id FROM terms WHERE kind=$2 AND slug=$3 ON CONFLICT DO NOTHING").bind(&post.id).bind(kind).bind(slug).execute(&mut *tx).await?;
        }
    }
    if input.action == "publish" {
        copy_terms(&mut tx, &post.id).await?;
    }
    let snapshot = serde_json::json!({"post":post,"categories":input.categories,"tags":input.tags,"taxonomies":custom});
    sqlx::query(
        "INSERT INTO revisions(id,post_id,version,snapshot,created_at) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&post.id)
    .bind(post.version)
    .bind(snapshot.to_string())
    .bind(time)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM revisions WHERE post_id=$1 AND version<=$2")
        .bind(&post.id)
        .bind(post.version - app.config.revision_retention)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!(event="content_saved", content_id=%post.id, version=post.version, action=%input.action);
    Ok(post)
}
fn promote(post: &mut Post, time: i64) {
    post.published_locale = post.locale.clone();
    post.published_translation_group = post.translation_group.clone();
    post.published_seo = post.seo.clone();
    post.published_slug = post.slug.clone();
    post.published_title = post.title.clone();
    post.published_body = post.body.clone();
    post.published_document = post.document.clone();
    post.published_fields = post.fields.clone();
    post.published_blocks = post.blocks.clone();
    if post.published_at == 0 {
        post.published_at = time;
    }
    post.publish_at = 0;
}
fn term_slug(name: &str) -> String {
    name.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
async fn copy_terms(tx: &mut sqlx::Transaction<'_, sqlx::Any>, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM published_post_terms WHERE post_id=$1")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO published_post_terms(post_id,term_id) SELECT post_id,term_id FROM post_terms WHERE post_id=$1").bind(id).execute(&mut **tx).await?;
    Ok(())
}
pub async fn publish_due(app: &App) -> Result<usize> {
    let _guard = app.mutation().await;
    let mut tx = app.db.pool.begin().await?;
    let rows=sqlx::query("SELECT * FROM posts WHERE status='scheduled' AND publish_at<=$1 ORDER BY publish_at LIMIT 50").bind(now()).fetch_all(&mut *tx).await?;
    let mut n = 0;
    for row in rows {
        let mut p = Post::from_row(row);
        promote(&mut p, now());
        p.version += 1;
        p.status = "published".into();
        let result=sqlx::query("UPDATE posts SET status='published',published_slug=slug,published_title=title,published_body=body,published_document=document,published_fields=fields,published_blocks=blocks,published_locale=locale,published_translation_group=translation_group,published_seo=seo,publish_at=0,published_at=$1,version=$2 WHERE id=$3 AND status='scheduled' AND version=$4").bind(p.published_at).bind(p.version).bind(&p.id).bind(p.version-1).execute(&mut *tx).await?;
        if result.rows_affected() == 1 {
            copy_terms(&mut tx, &p.id).await?;
            let terms=sqlx::query("SELECT t.name,t.kind FROM terms t JOIN post_terms pt ON pt.term_id=t.id WHERE pt.post_id=$1 ORDER BY t.name").bind(&p.id).fetch_all(&mut *tx).await?;
            let names = |kind: &str| {
                terms
                    .iter()
                    .filter(|r| r.get::<String, _>("kind") == kind)
                    .map(|r| r.get::<String, _>("name"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let mut custom = std::collections::BTreeMap::<String, Vec<String>>::new();
            for row in &terms {
                let kind: String = row.get("kind");
                if !["category", "tag"].contains(&kind.as_str()) {
                    custom.entry(kind).or_default().push(row.get("name"));
                }
            }
            let snapshot = serde_json::json!({"post":p,"categories":names("category"),"tags":names("tag"),"taxonomies":custom});
            sqlx::query("INSERT INTO revisions(id,post_id,version,snapshot,created_at) VALUES($1,$2,$3,$4,$5)").bind(uuid::Uuid::new_v4().to_string()).bind(&p.id).bind(p.version).bind(snapshot.to_string()).bind(now()).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM revisions WHERE post_id=$1 AND version<=$2")
                .bind(&p.id)
                .bind(p.version - app.config.revision_retention)
                .execute(&mut *tx)
                .await?;
            n += 1;
        }
    }
    tx.commit().await?;
    if n > 0 {
        tracing::info!(event = "scheduled_publication_completed", count = n);
    }
    Ok(n)
}
pub async fn restore_revision(
    app: &App,
    session: &Session,
    id: &str,
    revision: &str,
    version: i64,
) -> Result<Post> {
    if !session.can_edit() {
        return Err(Error::forbidden());
    }
    let snapshot: String =
        sqlx::query_scalar("SELECT snapshot FROM revisions WHERE id=$1 AND post_id=$2")
            .bind(revision)
            .bind(id)
            .fetch_optional(&app.db.pool)
            .await?
            .ok_or_else(Error::not_found)?;
    let v: Value =
        serde_json::from_str(&snapshot).map_err(|_| Error::invalid("Revision is invalid."))?;
    let p: Post = serde_json::from_value(v["post"].clone())
        .map_err(|_| Error::invalid("Revision is invalid."))?;
    save(
        app,
        session,
        Some(id),
        PostInput {
            import_markdown: false,
            locale: p.locale,
            translation_group: p.translation_group,
            seo: p.seo,
            title: p.title,
            slug: p.slug,
            kind: p.kind,
            body: p.body,
            document: p.document,
            fields: p.fields,
            blocks: p.blocks,
            categories: v["categories"].as_str().unwrap_or("").into(),
            tags: v["tags"].as_str().unwrap_or("").into(),
            taxonomies: v
                .get("taxonomies")
                .ok_or(Error::invalid("Revision lacks current taxonomy data."))?
                .to_string(),
            version,
            action: "save".into(),
            publish_at: 0,
            csrf: String::new(),
        },
    )
    .await
}
