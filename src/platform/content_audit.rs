//! Bounded offline editorial diagnostics, never a ranking score or web crawler.
use crate::{
    App,
    error::{Error, Result},
};
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::{BTreeMap, BTreeSet};

pub async fn report(app: &App, keyword: &str) -> Result<Value> {
    if keyword.len() > 100 || keyword.contains(['\n', '\r']) {
        return Err(Error::invalid("Use a keyword up to 100 bytes on one line."));
    }
    // Refuse excess cardinality before decompressing/summing large bodies.
    let items: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM (SELECT id FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id) LIMIT 1001) AS bounded")
        .fetch_one(&app.db.pool).await?;
    if items > 1000 {
        return Err(Error::invalid(
            "Complete local audit supports 1,000 public items; no partial orphan result was produced.",
        ));
    }
    let characters: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(LENGTH(published_body)+LENGTH(published_document)),0) FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id)")
        .fetch_one(&app.db.pool).await?;
    if characters > 8 * 1024 * 1024 {
        return Err(Error::invalid(
            "Complete local audit supports 1,000 public items and eight million body characters; split the site analysis externally. No partial orphan result was produced.",
        ));
    }
    let (discovery, _) = crate::discovery::load(app).await?;
    let rows = sqlx::query("SELECT id,published_title,published_document,published_slug,published_locale FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id) ORDER BY id LIMIT 1001")
        .fetch_all(&app.db.pool).await?;
    let origin = app.config.origin();
    let mut paths = BTreeMap::new();
    for row in &rows {
        let id: String = row.get("id");
        let slug: String = row.get("published_slug");
        paths.insert(
            discovery.path(&row.get::<String, _>("published_locale"), &slug),
            id.clone(),
        );
        paths.insert(format!("/{slug}"), id);
    }
    let redirects = sqlx::query("SELECT source,target FROM redirects ORDER BY source LIMIT 1001")
        .fetch_all(&app.db.pool)
        .await?;
    if redirects.len() > 1000 {
        return Err(Error::invalid("Complete audit supports 1,000 redirects."));
    }
    let redirects: BTreeMap<String, String> = redirects
        .into_iter()
        .map(|r| (r.get("source"), r.get("target")))
        .collect();
    let mut edges = BTreeSet::new();
    let mut items = Vec::new();
    let mut link_count = 0;
    for row in &rows {
        let id: String = row.get("id");
        let title: String = row.get("published_title");
        let body = crate::document::Document::parse(&row.get::<String, _>("published_document"))?
            .markdown();
        let path = discovery.path(
            &row.get::<String, _>("published_locale"),
            &row.get::<String, _>("published_slug"),
        );
        let base = url::Url::parse(&format!("{origin}{path}"))
            .map_err(|_| Error::invalid("Invalid public origin."))?;
        let mut plain = String::new();
        let mut paragraphs = 0;
        for event in pulldown_cmark::Parser::new(&body) {
            match event {
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, .. }) => {
                    link_count += 1;
                    if link_count > 20000 {
                        return Err(Error::invalid(
                            "Complete audit supports 20,000 content links.",
                        ));
                    }
                    if dest_url.starts_with('#') {
                        continue;
                    }
                    let Ok(url) = base.join(&dest_url) else {
                        continue;
                    };
                    if url.origin().ascii_serialization() != origin {
                        continue;
                    }
                    let mut destination = url.path().to_owned();
                    let mut visited = BTreeSet::new();
                    for _ in 0..16 {
                        if !visited.insert(destination.clone()) {
                            break;
                        }
                        let Some(next) = redirects.get(&destination) else {
                            break;
                        };
                        destination = next.clone();
                    }
                    if let Some(target) = paths.get(&destination)
                        && target != &id
                    {
                        edges.insert((id.clone(), target.clone()));
                    }
                }
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Paragraph) => paragraphs += 1,
                pulldown_cmark::Event::Text(text) | pulldown_cmark::Event::Code(text) => {
                    plain.push_str(&text);
                    plain.push(' ');
                }
                _ => {}
            }
        }
        let words = plain.split_whitespace().count();
        let sentences = plain
            .split(['.', '!', '?', '。', '！', '？'])
            .filter(|s| !s.trim().is_empty())
            .count();
        let keyword_occurrences = if keyword.trim().is_empty() {
            0
        } else {
            plain
                .to_lowercase()
                .matches(&keyword.trim().to_lowercase())
                .count()
        };
        items.push(json!({"id":id,"title":title,"path":path,"locale":row.get::<String,_>("published_locale"),
            "whitespace_tokens":words,"sentences_approximate":sentences,"paragraphs":paragraphs,
            "tokens_per_sentence_approximate":if sentences>0 {words as f64/sentences as f64}else{0.0},
            "keyword_occurrences":keyword_occurrences,"keyword_in_title":!keyword.trim().is_empty()&&title.to_lowercase().contains(&keyword.trim().to_lowercase())}));
    }
    let inbound: BTreeSet<&str> = edges.iter().map(|(_, target)| target.as_str()).collect();
    let orphans: Vec<_> = items
        .iter()
        .filter(|item| !inbound.contains(item["id"].as_str().unwrap_or_default()))
        .map(|item| item["id"].clone())
        .collect();
    Ok(
        json!({"format":"wpalt-content-audit-v1","items":items,"edges":edges,"without_inbound_content_links":orphans,
        "keyword":keyword,"boundary":"Public publication snapshots only. Markdown content-link graph excludes navigation, templates, private/draft content, images and external fetching. No inbound content links is an editorial candidate, not proof of unreachable content. Whitespace tokens, punctuation sentences and keyword substring counts are language-sensitive editorial heuristics, never a ranking score, preferred length or keyword-density target."}),
    )
}
