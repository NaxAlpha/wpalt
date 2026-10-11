//! Publication-owned discovery assets. No remote fetch or plugin execution.
use super::{Seo, xml};
use crate::{
    App,
    error::{Error, Result},
};
use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
};
use maud::{Markup, html};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Any, QueryBuilder, Row, Transaction};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS published_image_refs(post_id TEXT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,media_id TEXT NOT NULL,PRIMARY KEY(post_id,media_id));";
pub const MAX_IMAGES: usize = 1000;
const PAGE: usize = 100;
const MAX_PAGE_IMAGES: usize = 32_000;
const MAX_XML_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Video {
    pub media: String,
    pub poster: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub sitemap: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct News {
    pub publication: String,
    pub language: String,
}
pub(crate) fn indexes(postgres: bool) -> String {
    let indexable = if postgres {
        "NOT COALESCE((published_seo::jsonb->>'noindex')::boolean,false)"
    } else {
        "COALESCE(json_extract(published_seo,'$.noindex'),0)=0"
    };
    let video = if postgres {
        "COALESCE((published_seo::jsonb->'video'->>'sitemap')::boolean,false)"
    } else {
        "COALESCE(json_extract(published_seo,'$.video.sitemap'),0)=1"
    };
    let news = if postgres {
        "jsonb_typeof(published_seo::jsonb->'news')='object'"
    } else {
        "json_type(published_seo,'$.news')='object'"
    };
    format!(
        "CREATE INDEX IF NOT EXISTS discovery_video_identity ON posts(status,id) WHERE status='published' AND {indexable} AND {video}; CREATE INDEX IF NOT EXISTS discovery_news_identity ON posts(status,published_at,id) WHERE status='published' AND {indexable} AND {news};"
    )
}
/// Static validated predicates; integer time and fixed kind are never archive SQL.
pub(crate) fn predicate(postgres: bool, kind: &str, at: i64) -> Result<String> {
    let indexable = if postgres {
        "NOT COALESCE((published_seo::jsonb->>'noindex')::boolean,false)"
    } else {
        "COALESCE(json_extract(published_seo,'$.noindex'),0)=0"
    };
    let extra = match kind {
        "image" => {
            "EXISTS(SELECT 1 FROM published_image_refs r WHERE r.post_id=posts.id)".to_string()
        }
        "video" => {
            if postgres {
                "COALESCE((published_seo::jsonb->'video'->>'sitemap')::boolean,false)".into()
            } else {
                "COALESCE(json_extract(published_seo,'$.video.sitemap'),0)=1".into()
            }
        }
        "news" => {
            let object = if postgres {
                "jsonb_typeof(published_seo::jsonb->'news')='object'"
            } else {
                "json_type(published_seo,'$.news')='object'"
            };
            format!(
                "{object} AND published_at>0 AND published_at>={} AND published_at<={at}",
                at.saturating_sub(2 * 86400)
            )
        }
        _ => return Err(Error::invalid("Invalid discovery sitemap kind.")),
    };
    Ok(format!(
        "status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id) AND {indexable} AND {extra}"
    ))
}
pub(crate) async fn index_entries(app: &App) -> Result<String> {
    let mut output = String::new();
    let at = crate::now();
    for kind in ["image", "video", "news"] {
        let predicate = predicate(app.db.postgres, kind, at)?;
        // Fetch only page boundaries plus a last-row sentinel: one ordered bounded scan.
        let query = format!(
            "SELECT id,n FROM (SELECT id,ROW_NUMBER() OVER(ORDER BY id) AS n,COUNT(*) OVER() AS total FROM (SELECT id FROM posts WHERE {predicate} ORDER BY id LIMIT 500001) bounded) numbered WHERE n % 100=0 OR n=total ORDER BY n"
        );
        let rows = sqlx::query(&query).fetch_all(&app.db.pool).await?;
        if rows.iter().any(|r| r.get::<i64, _>("n") > 500000) {
            return Err(Error::invalid(
                "Discovery sitemap index exceeds the 500,000-item work budget.",
            ));
        }
        let count = rows.last().map(|r| r.get::<i64, _>("n")).unwrap_or(0);
        // Stable first URL remains listed for an intentionally empty period.
        let mut cursors = vec![String::new()];
        for r in rows {
            let n: i64 = r.get("n");
            if n < count && n % 100 == 0 {
                cursors.push(r.get("id"));
            }
        }
        for after in cursors {
            let suffix = if after.is_empty() {
                String::new()
            } else {
                format!("?after={after}")
            };
            output.push_str(&format!(
                "<sitemap><loc>{}</loc></sitemap>",
                xml(&format!(
                    "{}/discovery-sitemap/{kind}{suffix}",
                    app.config.origin()
                ))
            ));
        }
    }
    Ok(output)
}
fn identifier(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok()
}
fn text(s: &str, limit: usize) -> bool {
    !s.trim().is_empty() && s.len() <= limit && !s.chars().any(char::is_control)
}
pub fn validate(seo: &Seo) -> Result<()> {
    if (!seo.social_image.is_empty() && !identifier(&seo.social_image))
        || seo.social_alt.len() > 1000
        || seo.social_alt.chars().any(char::is_control)
    {
        return Err(Error::invalid(
            "Choose a local social image identity and bounded plain alt text.",
        ));
    }
    if let Some(v) = &seo.video
        && (!identifier(&v.media)
            || !identifier(&v.poster)
            || !text(&v.title, 300)
            || !text(&v.description, 2048))
    {
        return Err(Error::invalid(
            "Video requires local MP4/poster identities, a title and description.",
        ));
    }
    if let Some(n) = &seo.news
        && (seo.schema_type != "Article"
            || !text(&n.publication, 300)
            || !news_language(&n.language))
    {
        return Err(Error::invalid(
            "News requires Article output, a publication name and an registered ISO news language code.",
        ));
    }
    Ok(())
}
pub fn news_language(code: &str) -> bool {
    if ["zh-cn", "zh-tw"].contains(&code) {
        return true;
    }
    if ["zh", "zho", "chi", "und", "mul", "zxx", "mis"].contains(&code) {
        return false;
    }
    include_str!("../../assets/news-language-codes.txt")
        .lines()
        .any(|c| c == code)
}
pub fn image_ids(raw: &str) -> Result<BTreeSet<String>> {
    let ids = crate::document::Document::parse(raw)?.image_ids();
    if ids.len() > MAX_IMAGES {
        return Err(Error::invalid(
            "Publication exceeds 1,000 distinct local editorial images.",
        ));
    }
    Ok(ids)
}
pub async fn index(tx: &mut Transaction<'_, Any>, id: &str, raw: &str) -> Result<()> {
    let ids = image_ids(raw)?;
    sqlx::query("DELETE FROM published_image_refs WHERE post_id=$1")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    // One bounded batch, with driver-correct numbered bind parameters.
    for chunk in ids.into_iter().collect::<Vec<_>>().chunks(200) {
        let mut q = QueryBuilder::<Any>::new("INSERT INTO published_image_refs(post_id,media_id) ");
        q.push_values(chunk, |mut row, image| {
            row.push_bind(id).push_bind(image);
        });
        execute(tx, &mut q).await?;
    }
    Ok(())
}
async fn execute(tx: &mut Transaction<'_, Any>, q: &mut QueryBuilder<'_, Any>) -> Result<()> {
    use sqlx::Execute;
    let mut query = q.build();
    let sql = crate::db::Db::numbered(query.sql());
    let args = query
        .take_arguments()
        .map_err(sqlx::Error::Encode)?
        .unwrap_or_default();
    sqlx::query_with(&sql, args).execute(&mut **tx).await?;
    Ok(())
}
/// Rebuild only in stopped migration/fresh recovery; runtime never scans documents.
pub async fn rebuild(tx: &mut Transaction<'_, Any>) -> Result<()> {
    sqlx::query("DELETE FROM published_image_refs")
        .execute(&mut **tx)
        .await?;
    let mut after = String::new();
    loop {
        let rows =
            sqlx::query("SELECT id,published_document FROM posts WHERE id>$1 ORDER BY id LIMIT 16")
                .bind(&after)
                .fetch_all(&mut **tx)
                .await?;
        if rows.is_empty() {
            break;
        }
        for r in rows {
            let id: String = r.get("id");
            index(tx, &id, &r.get::<String, _>("published_document")).await?;
            after = id;
        }
    }
    Ok(())
}
#[derive(Clone)]
struct Asset {
    mime: String,
    filename: String,
    alt: String,
    sha256: String,
}
async fn assets_in(
    tx: &mut Transaction<'_, Any>,
    ids: &BTreeSet<String>,
    draft: bool,
) -> Result<BTreeMap<String, Asset>> {
    if ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    if ids.len() > PAGE * 3 {
        return Err(Error::invalid("Discovery asset work budget exceeded."));
    }
    let mut q = asset_query(ids, draft);
    let rows = crate::db::Db::fetch_builder_in(tx, &mut q).await?;
    Ok(asset_records(rows))
}
fn asset_query(ids: &BTreeSet<String>, draft: bool) -> QueryBuilder<'_, Any> {
    let mut q = QueryBuilder::<Any>::new("SELECT id,mime,filename,alt,sha256 FROM media WHERE ");
    if !draft {
        q.push("visibility='public' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='media' AND mr.resource_id=media.id) AND ");
    }
    q.push("id IN (");
    let mut list = q.separated(",");
    for id in ids {
        list.push_bind(id);
    }
    list.push_unseparated(")");
    q
}
fn asset_records(rows: Vec<sqlx::any::AnyRow>) -> BTreeMap<String, Asset> {
    rows.into_iter()
        .map(|r| {
            (
                r.get("id"),
                Asset {
                    mime: r.get("mime"),
                    filename: r.get("filename"),
                    alt: r.get("alt"),
                    sha256: r.get("sha256"),
                },
            )
        })
        .collect()
}
fn dependencies(seo: &Seo, ids: &mut BTreeSet<String>) {
    if !seo.social_image.is_empty() {
        ids.insert(seo.social_image.clone());
    }
    if let Some(v) = &seo.video {
        ids.insert(v.media.clone());
        ids.insert(v.poster.clone());
    }
}
fn image(asset: &Asset) -> bool {
    ["image/png", "image/jpeg", "image/webp", "image/gif"].contains(&asset.mime.as_str())
}
fn video_value(app: &App, seo: &Seo, assets: &BTreeMap<String, Asset>) -> Option<Value> {
    let v = seo.video.as_ref()?;
    if assets.get(&v.media)?.mime != "video/mp4" || !image(assets.get(&v.poster)?) {
        return None;
    }
    Some(
        json!({"url":format!("{}/media/{}",app.config.origin(),v.media),"poster":format!("{}/media/{}",app.config.origin(),v.poster),"title":v.title,"description":v.description}),
    )
}
/// `draft` is called only after editor authentication, never from public routes.
pub async fn enrich(app: &App, seo: &Seo, meta: &mut Value, draft: bool) -> Result<()> {
    let mut ids = BTreeSet::new();
    dependencies(seo, &mut ids);
    meta["video"] = Value::Null;
    meta["social_image"] = Value::Null;
    if ids.is_empty() {
        return Ok(());
    }
    let mut query = asset_query(&ids, draft);
    let assets = asset_records(app.db.fetch_builder(&mut query).await?);
    meta["video"] = video_value(app, seo, &assets).unwrap_or(Value::Null);
    meta["social_image"] = Value::Null;
    if let Some(a) = assets.get(&seo.social_image).filter(|a| image(a)) {
        let dimensions = crate::operations::media::dimensions(
            app,
            vec![(
                seo.social_image.clone(),
                a.filename.clone(),
                a.sha256.clone(),
            )],
        )
        .await
        .get(&seo.social_image)
        .copied();
        meta["social_image"] = json!({"url":format!("{}/media/{}",app.config.origin(),seo.social_image),"type":a.mime,"alt":if seo.social_alt.is_empty(){&a.alt}else{&seo.social_alt},"width":dimensions.map(|d|d.0),"height":dimensions.map(|d|d.1)});
    }
    Ok(())
}
pub fn video(meta: &Value) -> Markup {
    let v = &meta["video"];
    html! {@if let Some(url)=v["url"].as_str(){section class="publication-video" aria-label="Attached video" {h2 {(v["title"].as_str().unwrap_or(""))}video controls preload="metadata" src=(url) poster=(v["poster"].as_str().unwrap_or("")) {"Your browser does not support this video."}p {(v["description"].as_str().unwrap_or(""))}}}}
}
pub fn editor_controls(seo: &Seo, p: &crate::model::PostInput, d: &super::Definition) -> Markup {
    let v = seo.video.as_ref();
    let n = seo.news.as_ref();
    html! {
        div data-discovery-preview data-default-language=(d.default_language) {
            h3 {"Discovery preview"}
            p class="muted" {"Illustrative working-copy preview. Search and social platforms choose their own final appearance; saving a draft does not update live metadata."}
            p class="discovery-preview-url" data-preview-url {(d.path(&p.locale,&p.slug))}
            h4 data-preview-title {(if seo.title.is_empty(){&p.title}else{&seo.title})}
            p data-preview-description {(seo.description)}
            p data-preview-indexing {(if seo.noindex{"Excluded from search indexing when published"}else{"Available for search indexing when published"})}
            img data-preview-image hidden alt="";
        }
        details {summary {"Social image"}
            (media_control("seo_social_image","Social image","image",&seo.social_image))
            label {"Social image description" input name="seo_social_alt" value=(seo.social_alt) maxlength="1000";small {"Describe the selected image. Blank uses its library alt text. Public output requires current public, unrestricted image access."}}
        }
        details {summary {"Attached local video"}
            p class="muted" {"A declared video appears visibly below the content. Its sitemap entry requires both the MP4 and poster to remain public and unrestricted."}
            (media_control("seo_video_media","Video file","video",v.map(|v|v.media.as_str()).unwrap_or("")))
            (media_control("seo_video_poster","Video poster","image",v.map(|v|v.poster.as_str()).unwrap_or("")))
            label {"Video title" input name="seo_video_title" value=(v.map(|v|v.title.as_str()).unwrap_or("")) maxlength="300";}
            label {"Video description" textarea name="seo_video_description" maxlength="2048" {(v.map(|v|v.description.as_str()).unwrap_or(""))}}
            label {input type="checkbox" name="seo_video_sitemap" checked[v.is_some_and(|v|v.sitemap)];"Include in video discovery: this video is central and relevant to the article"}
            button type="button" class="secondary" data-discovery-remove-video hidden {"Remove video attachment"}
            small {"Clear every video field and its discovery selection to remove the attachment. Incomplete declarations cannot be saved."}
        }
        details {summary {"News publication"}
            p class="muted" {"Declare only actual news articles. Article output is required. News sitemaps include the original publication date for two days; editing does not reset it."}
            label {"Publication name" input name="seo_news_publication" value=(n.map(|n|n.publication.as_str()).unwrap_or("")) maxlength="300";}
            label {"News language code" input name="seo_news_language" value=(n.map(|n|n.language.as_str()).unwrap_or("")) maxlength="5";small {"ISO 639 two- or three-letter code, such as en, fr or deu; use zh-cn or zh-tw for Chinese. Clear both news fields to remove the declaration."}}
        }
    }
}
fn media_control(name: &str, label: &str, kind: &str, value: &str) -> Markup {
    html! {div data-discovery-media=(name) data-media-kind=(kind) {
        label {(label) input aria-label=(label) name=(name) value=(value) maxlength="36" autocomplete="off";small {"Select from the local library, or enter its media UUID. No remote files are fetched."}}
        div class="toolbar" {button type="button" class="secondary" data-media-choose {"Choose from library"}button type="button" class="secondary" data-media-clear {"Clear selection"}}
        div data-media-options hidden {}p role="status" data-media-status {}
    }}
}
#[derive(Default, Deserialize)]
pub struct Cursor {
    #[serde(default)]
    pub after: String,
}
pub async fn sitemap(
    State(app): State<App>,
    Path(kind): Path<String>,
    Query(q): Query<Cursor>,
) -> Result<Response> {
    if !["image", "video", "news"].contains(&kind.as_str())
        || (!q.after.is_empty() && !identifier(&q.after))
    {
        return Err(Error::invalid("Invalid discovery sitemap or cursor."));
    }
    let started = std::time::Instant::now();
    let mut tx = app.db.pool.begin().await?;
    if app.db.postgres {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
    }
    let predicate = predicate(app.db.postgres, &kind, crate::now())?;
    let query = format!(
        "SELECT id,published_slug,published_locale,published_seo,published_title,published_at FROM posts WHERE {predicate} AND id>$1 ORDER BY id LIMIT 101"
    );
    let rows = sqlx::query(&query)
        .bind(&q.after)
        .fetch_all(&mut *tx)
        .await?;
    let raw: String = sqlx::query_scalar("SELECT definition FROM discovery_settings WHERE id=1")
        .fetch_one(&mut *tx)
        .await?;
    let d: super::Definition =
        serde_json::from_str(&raw).map_err(|_| Error::invalid("Invalid discovery settings."))?;
    d.validate()?;
    let entries = rows
        .iter()
        .take(PAGE)
        .map(|r| {
            let s = Seo::parse(&r.get::<String, _>("published_seo"))?;
            Ok((r, s))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut asset_ids = BTreeSet::new();
    if kind == "video" {
        for (_, s) in &entries {
            if !s.noindex {
                dependencies(s, &mut asset_ids);
            }
        }
    }
    let assets = assets_in(&mut tx, &asset_ids, false).await?;
    let mut images = BTreeMap::<String, Vec<String>>::new();
    if kind == "image" && !entries.is_empty() {
        let mut q = QueryBuilder::<Any>::new(
            "SELECT r.post_id,r.media_id FROM published_image_refs r JOIN media m ON m.id=r.media_id WHERE m.visibility='public' AND m.mime IN ('image/png','image/jpeg','image/webp','image/gif') AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='media' AND mr.resource_id=m.id) AND r.post_id IN (",
        );
        let mut ids = q.separated(",");
        for (r, s) in &entries {
            if !s.noindex {
                ids.push_bind(r.get::<String, _>("id"));
            }
        }
        // Noindex pages cannot be used to force an invalid empty IN list.
        ids.push_bind("");
        ids.push_unseparated(") ORDER BY r.post_id,r.media_id LIMIT 32001");
        let rows = crate::db::Db::fetch_builder_in(&mut tx, &mut q).await?;
        if rows.len() > MAX_PAGE_IMAGES {
            return Err(Error::invalid(
                "Image sitemap exceeds the 32,000-image page work budget.",
            ));
        }
        for r in rows {
            images
                .entry(r.get("post_id"))
                .or_default()
                .push(r.get("media_id"));
        }
    }
    let namespace = match kind.as_str() {
        "image" => "http://www.google.com/schemas/sitemap-image/1.1",
        "video" => "http://www.google.com/schemas/sitemap-video/1.1",
        _ => "http://www.google.com/schemas/sitemap-news/0.9",
    };
    let mut output = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:{kind}=\"{namespace}\">"
    );
    let now = crate::now();
    let mut included = 0usize;
    for (r, s) in entries {
        if s.noindex {
            continue;
        }
        let loc = format!(
            "{}{}",
            app.config.origin(),
            d.path(
                &r.get::<String, _>("published_locale"),
                &r.get::<String, _>("published_slug")
            )
        );
        let mut extension = String::new();
        match kind.as_str() {
            "image" => {
                for id in images.get(&r.get::<String, _>("id")).into_iter().flatten() {
                    extension.push_str(&format!(
                        "<image:image><image:loc>{}</image:loc></image:image>",
                        xml(&format!("{}/media/{id}", app.config.origin()))
                    ));
                }
            }
            "video" => {
                if s.video.as_ref().is_some_and(|v| v.sitemap)
                    && let Some(v) = video_value(&app, &s, &assets)
                {
                    extension = format!(
                        "<video:video><video:thumbnail_loc>{}</video:thumbnail_loc><video:title>{}</video:title><video:description>{}</video:description><video:content_loc>{}</video:content_loc></video:video>",
                        xml(v["poster"].as_str().unwrap()),
                        xml(v["title"].as_str().unwrap()),
                        xml(v["description"].as_str().unwrap()),
                        xml(v["url"].as_str().unwrap())
                    );
                }
            }
            _ => {
                let date: i64 = r.get("published_at");
                if date > 0
                    && date <= now
                    && date >= now - 2 * 86400
                    && let Some(n) = s.news.as_ref()
                {
                    let date = chrono::DateTime::from_timestamp(date, 0)
                        .ok_or_else(|| Error::invalid("Invalid original news date."))?
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                    extension = format!(
                        "<news:news><news:publication><news:name>{}</news:name><news:language>{}</news:language></news:publication><news:publication_date>{}</news:publication_date><news:title>{}</news:title></news:news>",
                        xml(&n.publication),
                        xml(&n.language),
                        xml(&date),
                        xml(&r.get::<String, _>("published_title"))
                    );
                }
            }
        }
        if !extension.is_empty() {
            included += 1;
            output.push_str(&format!("<url><loc>{}</loc>{extension}</url>", xml(&loc)));
        }
        if output.len() + "</urlset>".len() > MAX_XML_BYTES {
            return Err(Error::invalid(
                "Discovery sitemap exceeds the four MiB output budget.",
            ));
        }
    }
    tx.commit().await?;
    output.push_str("</urlset>");
    let bytes = output.len();
    let mut response = (
        [
            ("content-type", "application/xml; charset=utf-8"),
            ("cache-control", "no-store"),
        ],
        output,
    )
        .into_response();
    if rows.len() > PAGE {
        let id: String = rows[PAGE - 1].get("id");
        response.headers_mut().insert(
            "link",
            format!(
                "<{}/discovery-sitemap/{kind}?after={id}>; rel=\"next\"",
                app.config.origin()
            )
            .parse()
            .map_err(|_| Error::invalid("Invalid sitemap link."))?,
        );
    }
    tracing::debug!(event="discovery_sitemap",kind=%kind,scanned=rows.len().min(PAGE),included,bytes,elapsed_us=started.elapsed().as_micros() as u64);
    Ok(response)
}
