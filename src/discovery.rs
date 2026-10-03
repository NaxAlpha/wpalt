//! Owner-hosted discovery. Public output always reads publication snapshots.
use crate::{
    App, content,
    error::{Error, Result},
    model::{NavItem, Post, PostInput},
    view,
};
use axum::{
    Router,
    extract::{Form, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use maud::{Markup, PreEscaped, html};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Language {
    pub code: String,
    pub label: String,
    pub direction: String,
    pub navigation: Vec<NavItem>,
    pub search_label: String,
}
impl Default for Language {
    fn default() -> Self {
        Self {
            code: "en".into(),
            label: "English".into(),
            direction: "ltr".into(),
            navigation: Vec::new(),
            search_label: "Search".into(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Business {
    pub name: String,
    pub street: String,
    pub city: String,
    pub postal_code: String,
    pub country: String,
    pub telephone: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Definition {
    pub default_language: String,
    pub languages: Vec<Language>,
    pub business: Business,
}
impl Default for Definition {
    fn default() -> Self {
        Self {
            default_language: "en".into(),
            languages: vec![Language::default()],
            business: Business::default(),
        }
    }
}
impl Definition {
    pub fn language(&self, code: &str) -> Result<&Language> {
        self.languages
            .iter()
            .find(|l| l.code == code)
            .ok_or(Error::not_found())
    }
    pub fn validate(&self) -> Result<()> {
        let valid_code = |v: &str| {
            let parts: Vec<_> = v.split('-').collect();
            parts.len() <= 2
                && (2..=3).contains(&parts[0].len())
                && parts[0].bytes().all(|b| b.is_ascii_lowercase())
                && (parts.len() == 1
                    || ((2..=3).contains(&parts[1].len())
                        && parts[1]
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())))
        };
        let mut codes = BTreeSet::new();
        if self.languages.is_empty()
            || self.languages.len() > 16
            || self.languages.iter().any(|l| {
                !valid_code(&l.code)
                    || ["api"].contains(&l.code.as_str())
                    || !codes.insert(&l.code)
                    || l.label.trim().is_empty()
                    || l.label.len() > 80
                    || l.label.chars().any(char::is_control)
                    || !["ltr", "rtl"].contains(&l.direction.as_str())
                    || l.navigation.len() > 30
                    || l.search_label.is_empty()
                    || l.search_label.len() > 80
                    || l.search_label.chars().any(char::is_control)
                    || l.navigation.iter().any(|n| {
                        n.label.is_empty()
                            || n.label.len() > 100
                            || n.url.len() > 2000
                            || !content::safe_nav_url(&n.url)
                    })
            })
            || self.language(&self.default_language).is_err()
        {
            return Err(Error::invalid(
                "Configure 1–16 unique language codes, labels, direction and safe navigation.",
            ));
        }
        let b = &self.business;
        if [
            &b.name,
            &b.street,
            &b.city,
            &b.postal_code,
            &b.country,
            &b.telephone,
        ]
        .iter()
        .any(|s| s.len() > 200 || s.chars().any(char::is_control))
            || (!b.name.is_empty()
                && (b.street.is_empty()
                    || b.city.is_empty()
                    || b.country.len() != 2
                    || !b.country.bytes().all(|b| b.is_ascii_uppercase())))
        {
            return Err(Error::invalid(
                "Business details require a name and visible address with a two-letter country code.",
            ));
        }
        Ok(())
    }
    pub fn path(&self, language: &str, slug: &str) -> String {
        if language == self.default_language {
            format!("/{slug}")
        } else {
            format!("/{language}/{slug}")
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Seo {
    pub title: String,
    pub description: String,
    pub noindex: bool,
    pub schema_type: String,
}
impl Default for Seo {
    fn default() -> Self {
        Self {
            title: String::new(),
            description: String::new(),
            noindex: false,
            schema_type: "WebPage".into(),
        }
    }
}
impl Seo {
    pub fn parse(raw: &str) -> Result<Self> {
        let s: Self = serde_json::from_str(raw).map_err(|_| {
            Error::invalid("SEO must contain title, description, noindex and schema_type only.")
        })?;
        if raw.len() > 4096
            || s.title.len() > 300
            || s.description.len() > 1000
            || !["WebPage", "Article"].contains(&s.schema_type.as_str())
            || [&s.title, &s.description]
                .iter()
                .any(|s| s.chars().any(char::is_control))
        {
            return Err(Error::invalid(
                "Use bounded SEO text and Article or WebPage structured output.",
            ));
        }
        Ok(s)
    }
}
pub async fn load(app: &App) -> Result<(Definition, i64)> {
    let row = sqlx::query("SELECT definition,version FROM discovery_settings WHERE id=1")
        .fetch_one(&app.db.pool)
        .await?;
    let d: Definition = serde_json::from_str(&row.get::<String, _>("definition"))
        .map_err(|_| Error::invalid("Invalid stored discovery settings."))?;
    d.validate()?;
    Ok((d, row.get("version")))
}
pub async fn validate_content(app: &App, p: &PostInput) -> Result<()> {
    let (d, _) = load(app).await?;
    d.language(&p.locale)
        .map_err(|_| Error::invalid("Choose an installed language."))?;
    Seo::parse(&p.seo)?;
    if p.translation_group.len() > 80
        || (!p.translation_group.is_empty() && !crate::schema::identifier(&p.translation_group))
    {
        return Err(Error::invalid(
            "Translation group must be a short identifier shared by related translations.",
        ));
    }
    if d.languages.iter().any(|l| l.code == p.slug) {
        return Err(Error::invalid(
            "Content slug conflicts with a language landing page.",
        ));
    }
    if !p.translation_group.is_empty() {
        let different: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM posts WHERE kind<>$1 AND (translation_group=$2 OR published_translation_group=$2)")
            .bind(&p.kind).bind(&p.translation_group).fetch_one(&app.db.pool).await?;
        if different > 0 {
            return Err(Error::invalid("Translations must share a content type."));
        }
    }
    if p.action == "publish" || p.action == "schedule" {
        let conflict: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM redirects WHERE source=$1 OR source=$2")
                .bind(d.path(&p.locale, &p.slug))
                .bind(format!("/{}", p.slug))
                .fetch_one(&app.db.pool)
                .await?;
        if conflict > 0 {
            return Err(Error::invalid(
                "Remove the conflicting redirect before publication.",
            ));
        }
    }
    Ok(())
}
pub async fn configure(app: &App, d: Definition, version: i64) -> Result<i64> {
    d.validate()?;
    let _guard = app.mutations.lock().await;
    let (old, _) = load(app).await?;
    let mut tx = app.db.pool.begin().await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM posts")
        .fetch_one(&mut *tx)
        .await?;
    if count > 0 && d.default_language != old.default_language {
        return Err(Error::invalid(
            "Default language can change only before content exists; plan a URL migration for an established site.",
        ));
    }
    let locales=sqlx::query("SELECT DISTINCT locale FROM posts UNION SELECT DISTINCT published_locale AS locale FROM posts WHERE published_slug<>''").fetch_all(&mut *tx).await?;
    if locales
        .iter()
        .any(|r| d.language(&r.get::<String, _>("locale")).is_err())
    {
        return Err(Error::invalid(
            "A language still used by content cannot be removed.",
        ));
    }
    for language in &d.languages {
        let collision: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM posts WHERE slug=$1 OR published_slug=$1")
                .bind(&language.code)
                .fetch_one(&mut *tx)
                .await?;
        if collision > 0 {
            return Err(Error::invalid(
                "Language landing URL conflicts with content.",
            ));
        }
    }
    let redirect_sources = sqlx::query("SELECT source FROM redirects LIMIT 1000")
        .fetch_all(&mut *tx)
        .await?;
    if redirect_sources.iter().any(|r| {
        let source: String = r.get("source");
        d.languages.iter().any(|l| {
            source == format!("/{}", l.code)
                || source == d.path(&l.code, "")
                || source == d.path(&l.code, "search")
        })
    }) {
        return Err(Error::invalid(
            "Remove redirects conflicting with language landing routes.",
        ));
    }
    let result = sqlx::query(
        "UPDATE discovery_settings SET definition=$1,version=version+1 WHERE id=1 AND version=$2",
    )
    .bind(serde_json::to_string(&d).unwrap())
    .bind(version)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(Error::conflict());
    }
    tx.commit().await?;
    tracing::info!(event = "discovery_settings_saved", version = version + 1);
    Ok(version + 1)
}
/// Local publication routes are the only canonical source. No HTTP requests.
pub async fn metadata(
    app: &App,
    p: Option<&Post>,
    language: &str,
    path: &str,
    search: bool,
) -> Result<Value> {
    let (d, _) = load(app).await?;
    let settings = app.db.settings().await?;
    metadata_with_settings(app, p, language, path, search, &d, &settings).await
}
#[allow(clippy::too_many_arguments)] // Reuse validated request settings; no duplicate reads.
pub async fn metadata_with_settings(
    app: &App,
    p: Option<&Post>,
    language: &str,
    path: &str,
    search: bool,
    d: &Definition,
    settings: &crate::model::Settings,
) -> Result<Value> {
    let l = d.language(language)?;
    let seo = if let Some(p) = p {
        Seo::parse(&p.published_seo)?
    } else {
        Seo::default()
    };
    let title = if seo.title.is_empty() {
        p.map(|p| p.published_title.clone())
            .unwrap_or(settings.title.clone())
    } else {
        seo.title.clone()
    };
    let description = if seo.description.is_empty() {
        p.map(|p| view::excerpt(&p.published_body.chars().take(220).collect::<String>()))
            .filter(|s| !s.is_empty())
            .unwrap_or(settings.description.clone())
    } else {
        seo.description.clone()
    };
    let canonical = format!("{}{path}", app.config.origin());
    let mut alternates = Vec::new();
    if let Some(p) = p {
        if !p.published_translation_group.is_empty() {
            let rows=sqlx::query("SELECT published_locale,published_slug,published_seo FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id) AND published_translation_group=$1 ORDER BY published_locale LIMIT 16").bind(&p.published_translation_group).fetch_all(&app.db.pool).await?;
            for r in rows {
                let code: String = r.get("published_locale");
                if !Seo::parse(&r.get::<String, _>("published_seo"))?.noindex {
                    alternates.push(json!({"language":code,"label":d.language(&code)?.label,"url":format!("{}{}",app.config.origin(),d.path(&code,&r.get::<String,_>("published_slug")))}));
                }
            }
        }
    } else if !search {
        for language in &d.languages {
            alternates.push(json!({"language":language.code,"label":language.label,"url":format!("{}{}",app.config.origin(),d.path(&language.code,""))}));
        }
    }
    if seo.noindex {
        alternates.clear();
    }
    let structured = if p.is_some() {
        json!({"@context":"https://schema.org","@type":seo.schema_type,"name":title,"headline":title,"description":description,"url":canonical,"inLanguage":language,"mainEntityOfPage":canonical})
    } else {
        json!({"@context":"https://schema.org","@type":"WebSite","name":title,"url":app.config.origin(),"inLanguage":language})
    };
    Ok(
        json!({"origin":app.config.origin(),"language":language,"direction":l.direction,"title":title,"description":description,"canonical":canonical,"noindex":seo.noindex||search,"alternates":alternates,"structured":structured,"business":d.business,"navigation":if l.navigation.is_empty(){serde_json::from_str::<Value>(&settings.navigation).unwrap_or(json!([]))}else{serde_json::to_value(&l.navigation).unwrap()},"search_label":l.search_label}),
    )
}
pub fn head(meta: &Value, draft: bool) -> Markup {
    let mut structures = vec![meta["structured"].clone()];
    let b = &meta["business"];
    if b["name"].as_str().is_some_and(|s| !s.is_empty()) {
        structures.push(json!({"@context":"https://schema.org","@type":"LocalBusiness","@id":format!("{}#business",meta["origin"].as_str().unwrap_or("")),"name":b["name"],"url":meta["origin"],"telephone":b["telephone"],"address":{"@type":"PostalAddress","streetAddress":b["street"],"addressLocality":b["city"],"postalCode":b["postal_code"],"addressCountry":b["country"]}}));
    }
    let script = serde_json::to_string(&structures)
        .unwrap()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026");
    html! {title{(meta["title"].as_str().unwrap_or(""))}meta name="description" content=(meta["description"].as_str().unwrap_or(""));meta name="robots" content=(if draft||meta["noindex"]==true{"noindex,follow"}else{"index,follow"});@if !draft{link rel="canonical" href=(meta["canonical"].as_str().unwrap_or(""));@for a in meta["alternates"].as_array().into_iter().flatten(){link rel="alternate" hreflang=(a["language"].as_str().unwrap_or("")) href=(a["url"].as_str().unwrap_or(""));}meta property="og:title" content=(meta["title"].as_str().unwrap_or(""));meta property="og:description" content=(meta["description"].as_str().unwrap_or(""));meta property="og:url" content=(meta["canonical"].as_str().unwrap_or(""));meta property="og:type" content=(if meta["structured"]["@type"]=="Article"{"article"}else{"website"});meta name="twitter:card" content="summary";script type="application/ld+json"{(PreEscaped(script))}}}
}
pub fn language_nav(meta: &Value) -> Markup {
    html! { @if meta["alternates"].as_array().is_some_and(|a|a.len()>1){nav class="toolbar" aria-label="Languages"{@for a in meta["alternates"].as_array().unwrap(){a href=(a["url"].as_str().unwrap_or("")) hreflang=(a["language"].as_str().unwrap_or("")) lang=(a["language"].as_str().unwrap_or("")) aria-current=[(a["language"]==meta["language"]).then_some("page")]{(a["label"].as_str().unwrap_or(""))}}}}}
}
fn xml(v: &str) -> String {
    v.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
#[derive(Default, Deserialize)]
struct SitemapQuery {
    #[serde(default)]
    after: String,
}
async fn sitemap(State(app): State<App>, Query(q): Query<SitemapQuery>) -> Result<Response> {
    if !q.after.is_empty() && uuid::Uuid::parse_str(&q.after).is_err() {
        return Err(Error::invalid("Invalid sitemap cursor."));
    }
    let rows=sqlx::query("SELECT id,published_slug,published_locale,published_seo FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id) AND id>$1 ORDER BY id LIMIT 1001").bind(&q.after).fetch_all(&app.db.pool).await?;
    let (d, _) = load(&app).await?;
    let mut output = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">",
    );
    for row in rows.iter().take(1000) {
        if Seo::parse(&row.get::<String, _>("published_seo"))?.noindex {
            continue;
        }
        let loc = format!(
            "{}{}",
            app.config.origin(),
            d.path(
                &row.get::<String, _>("published_locale"),
                &row.get::<String, _>("published_slug")
            )
        );
        output.push_str(&format!("<url><loc>{}</loc></url>", xml(&loc)));
    }
    output.push_str("</urlset>");
    let mut response =
        ([("content-type", "application/xml; charset=utf-8")], output).into_response();
    if rows.len() > 1000 {
        let id: String = rows[999].get("id");
        response.headers_mut().insert(
            "link",
            format!(
                "<{}/sitemap.xml?after={id}>; rel=\"next\"",
                app.config.origin()
            )
            .parse()
            .unwrap(),
        );
    }
    Ok(response)
}
async fn sitemap_index(State(app): State<App>) -> Result<Response> {
    let mut result = String::from(
        "<?xml version=\"1.0\"?><sitemapindex xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">",
    );
    // A single bounded ordered-index scan; return page boundaries, not content bodies.
    let rows = sqlx::query("SELECT id,n FROM (SELECT id,ROW_NUMBER() OVER (ORDER BY id) AS n FROM (SELECT id FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id) ORDER BY id LIMIT 500001) bounded) numbered WHERE n % 1000 = 0 OR n = 500001 ORDER BY n")
        .fetch_all(&app.db.pool).await?;
    if rows.iter().any(|r| r.get::<i64, _>("n") > 500000) {
        return Err(Error::invalid(
            "Sitemap exceeds the current 500,000-item work budget.",
        ));
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM (SELECT id FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id) LIMIT 500001) bounded",
    )
    .fetch_one(&app.db.pool)
    .await?;
    let mut cursors = vec![String::new()];
    for r in rows {
        if r.get::<i64, _>("n") < count {
            cursors.push(r.get("id"));
        }
    }
    for after in cursors {
        let suffix = if after.is_empty() {
            String::new()
        } else {
            format!("?after={after}")
        };
        result.push_str(&format!(
            "<sitemap><loc>{}</loc></sitemap>",
            xml(&format!("{}/sitemap.xml{suffix}", app.config.origin()))
        ));
    }
    result.push_str("</sitemapindex>");
    Ok(([("content-type", "application/xml; charset=utf-8")], result).into_response())
}
async fn robots(State(app): State<App>) -> Response {
    ([("content-type","text/plain; charset=utf-8")],format!("User-agent: *\nDisallow: /admin\nDisallow: /api/admin\nDisallow: /login\nSitemap: {}/sitemap-index.xml\n",app.config.origin())).into_response()
}
pub fn safe_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.starts_with("//")
        && path.len() <= 300
        && !path.contains(['?', '#', '%', '\\'])
        && !path.chars().any(|c| c.is_control() || c.is_whitespace())
        && !path.split('/').any(|p| p == "." || p == "..")
        && !path.starts_with("/admin")
        && !path.starts_with("/api")
        && !path.starts_with("/assets")
        && !path.starts_with("/media")
        && !path.starts_with("/themes")
        && !path.starts_with("/audience/")
        && !path.starts_with("/registration/")
        && !matches!(
            path,
            "/login"
                | "/account"
                | "/logout"
                | "/health"
                | "/feed.xml"
                | "/robots.txt"
                | "/sitemap.xml"
                | "/sitemap-index.xml"
        )
}
pub fn validate_redirect_graph(graph: &BTreeMap<String, String>) -> Result<()> {
    if graph.len() > 1000 {
        return Err(Error::invalid("Redirect budget is 1,000 exact rules."));
    }
    for (start, target) in graph {
        if !safe_path(start) || !safe_path(target) || start == target {
            return Err(Error::invalid("Invalid redirect paths."));
        }
        let mut at = start.as_str();
        let mut seen = BTreeSet::new();
        for hop in 0..=16 {
            if !seen.insert(at) || hop == 16 {
                return Err(Error::invalid(
                    "Redirect graph has a loop or exceeds 15 hops.",
                ));
            }
            match graph.get(at) {
                Some(next) => at = next,
                None => break,
            }
        }
    }
    Ok(())
}
pub async fn save_redirect(
    app: &App,
    source: &str,
    target: &str,
    code: i64,
    version: i64,
) -> Result<()> {
    if !safe_path(source) || !safe_path(target) || source == target || ![301, 302].contains(&code) {
        return Err(Error::invalid(
            "Use distinct exact local paths and 301 or 302.",
        ));
    }
    let _guard = app.mutations.lock().await;
    let (d, _) = load(app).await?;
    if source == "/"
        || source == "/search"
        || d.languages.iter().any(|l| {
            source == format!("/{}", l.code)
                || source == d.path(&l.code, "")
                || source == d.path(&l.code, "search")
        })
    {
        return Err(Error::invalid(
            "Redirect cannot replace a discovery landing route.",
        ));
    }
    let rows = sqlx::query("SELECT source,target FROM redirects LIMIT 1001")
        .fetch_all(&app.db.pool)
        .await?;
    if rows.len() >= 1000 && !rows.iter().any(|r| r.get::<String, _>("source") == source) {
        return Err(Error::invalid("Redirect budget is 1,000 exact rules."));
    }
    let mut graph: BTreeMap<String, String> = rows
        .into_iter()
        .map(|r| (r.get("source"), r.get("target")))
        .collect();
    graph.insert(source.into(), target.into());
    validate_redirect_graph(&graph)?;
    let posts=sqlx::query("SELECT published_slug,published_locale FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id) AND published_slug=$1").bind(source.rsplit('/').next().unwrap_or("")).fetch_all(&app.db.pool).await?;
    if posts.iter().any(|r| {
        d.path(
            &r.get::<String, _>("published_locale"),
            &r.get::<String, _>("published_slug"),
        ) == source
            || format!("/{}", r.get::<String, _>("published_slug")) == source
    }) {
        return Err(Error::invalid(
            "Redirect source conflicts with published content.",
        ));
    }
    let scheduled =
        sqlx::query("SELECT slug,locale FROM posts WHERE status='scheduled' AND slug=$1")
            .bind(source.rsplit('/').next().unwrap_or(""))
            .fetch_all(&app.db.pool)
            .await?;
    if scheduled.iter().any(|r| {
        d.path(&r.get::<String, _>("locale"), &r.get::<String, _>("slug")) == source
            || format!("/{}", r.get::<String, _>("slug")) == source
    }) {
        return Err(Error::invalid(
            "Redirect source conflicts with scheduled content.",
        ));
    }
    let result = if version == 0 {
        sqlx::query("INSERT INTO redirects(source,target,code,version) VALUES($1,$2,$3,1)")
            .bind(source)
            .bind(target)
            .bind(code)
            .execute(&app.db.pool)
            .await?
    } else {
        sqlx::query("UPDATE redirects SET target=$1,code=$2,version=version+1 WHERE source=$3 AND version=$4").bind(target).bind(code).bind(source).bind(version).execute(&app.db.pool).await?
    };
    if result.rows_affected() != 1 {
        return Err(Error::conflict());
    }
    Ok(())
}
pub async fn redirect_response(app: &App, path: &str) -> Result<Option<Response>> {
    if !safe_path(path) {
        return Ok(None);
    }
    let Some(r) = sqlx::query("SELECT target,code FROM redirects WHERE source=$1")
        .bind(path)
        .fetch_optional(&app.db.pool)
        .await?
    else {
        return Ok(None);
    };
    let target: String = r.get("target");
    let code: i64 = r.get("code");
    Ok(Some(
        (
            StatusCode::from_u16(code as u16).unwrap(),
            [("location", target)],
        )
            .into_response(),
    ))
}
async fn admin_session(app: &App, headers: &HeaderMap) -> Result<crate::model::Session> {
    let s = crate::auth::session(app, headers).await?;
    if !s.is_admin() {
        return Err(Error::forbidden());
    }
    Ok(s)
}
#[derive(Deserialize)]
struct ConfigForm {
    csrf: String,
    version: i64,
    definition: String,
}
async fn save_config(
    State(app): State<App>,
    headers: HeaderMap,
    Form(f): Form<ConfigForm>,
) -> Result<Response> {
    let s = admin_session(&app, &headers).await?;
    crate::auth::csrf(&s, &f.csrf)?;
    if f.definition.len() > 32 * 1024 {
        return Err(Error::invalid("Discovery settings exceed 32 KiB."));
    }
    let d = serde_json::from_str(&f.definition).map_err(|_| {
        Error::invalid(
            "Discovery configuration must follow the documented language/business schema.",
        )
    })?;
    configure(&app, d, f.version).await?;
    Ok(axum::response::Redirect::to("/admin/discovery").into_response())
}
#[derive(Deserialize)]
struct RedirectForm {
    csrf: String,
    source: String,
    target: String,
    code: i64,
    #[serde(default)]
    version: i64,
    #[serde(default)]
    action: String,
}
async fn edit_redirect(
    State(app): State<App>,
    headers: HeaderMap,
    Form(f): Form<RedirectForm>,
) -> Result<Response> {
    let s = admin_session(&app, &headers).await?;
    crate::auth::csrf(&s, &f.csrf)?;
    if f.action == "delete" {
        let _guard = app.mutations.lock().await;
        let r = sqlx::query("DELETE FROM redirects WHERE source=$1 AND version=$2")
            .bind(&f.source)
            .bind(f.version)
            .execute(&app.db.pool)
            .await?;
        if r.rows_affected() != 1 {
            return Err(Error::conflict());
        }
    } else {
        save_redirect(&app, &f.source, &f.target, f.code, f.version).await?;
    }
    Ok(axum::response::Redirect::to("/admin/discovery").into_response())
}
async fn admin_page(State(app): State<App>, headers: HeaderMap) -> Result<Response> {
    let s = admin_session(&app, &headers).await?;
    let (d, version) = load(&app).await?;
    let redirects =
        sqlx::query("SELECT source,target,code,version FROM redirects ORDER BY source LIMIT 1000")
            .fetch_all(&app.db.pool)
            .await?;
    let body = html! {
        (view::heading("Publishing", "Discovery", "Languages, search visibility and local redirects share your published content."))
        section class="panel" {
            h2 {"Languages"}p class="muted" {"Translations share a group but publish independently. Default language: " (d.default_language)}
            @for l in &d.languages {
                details open[l.code==d.default_language] {summary {(l.label) " · " (l.code)}form method="post" action="/admin/discovery/languages" {
                    (view::csrf(&s))input type="hidden" name="version" value=(version);input type="hidden" name="code" value=(l.code);
                    div class="field-row" {label {"Language label" input name="label" value=(l.label) required maxlength="80";}label {"Writing direction" select name="direction" aria-label="Writing direction" {option value="ltr" selected[l.direction=="ltr"] {"Left to right"}option value="rtl" selected[l.direction=="rtl"] {"Right to left"}}}}
                    label {"Search button text" input name="search_label" value=(l.search_label) required maxlength="80";}
                    details {summary {"Translated navigation"}label {"Navigation links (JSON)" textarea name="navigation" {(serde_json::to_string_pretty(&l.navigation).unwrap())}}}
                    div class="toolbar" {button {"Save language"}@if l.code!=d.default_language {button class="secondary" name="action" value="remove" {"Remove language"}}}
                }}
            }
            details {summary {"Add a language"}form method="post" action="/admin/discovery/languages" {(view::csrf(&s))input type="hidden" name="version" value=(version);div class="field-row" {label {"Language code" input name="code" required placeholder="fr" maxlength="7";}label {"Language label" input name="label" required placeholder="Français" maxlength="80";}}label {"Writing direction" select name="direction" aria-label="Writing direction" {option value="ltr" {"Left to right"}option value="rtl" {"Right to left"}}}label {"Search button text" input name="search_label" required value="Search" maxlength="80";}button {"Add language"}}}
        }
        section class="panel" {h2 {"Local business identity"}p class="muted" {"Business details appear visibly on public pages and in matching structured output. Leave the name blank to disable this information."}
            form method="post" action="/admin/discovery/business" {(view::csrf(&s))input type="hidden" name="version" value=(version);label {"Business name" input name="name" value=(d.business.name) maxlength="200";}label {"Street address" input name="street" value=(d.business.street) maxlength="200";}div class="field-row" {label {"City" input name="city" value=(d.business.city) maxlength="200";}label {"Postal code" input name="postal_code" value=(d.business.postal_code) maxlength="200";}}div class="field-row" {label {"Country code" input name="country" value=(d.business.country) placeholder="US" maxlength="2";}label {"Telephone" input name="telephone" value=(d.business.telephone) maxlength="200";}}button {"Save business details"}}
        }
        details class="panel" {summary {"Advanced discovery configuration"}form method="post" action="/admin/discovery" {(view::csrf(&s))input type="hidden" name="version" value=(version);label {"Language and business configuration" textarea name="definition" rows="18" maxlength="32768" {(serde_json::to_string_pretty(&d).unwrap())}}button {"Save discovery settings"}}}
        section class="panel" {
            h2 {"Published discovery"}
            p {a href="/sitemap-index.xml" {"Sitemap index"} " · " a href="/robots.txt" {"Robots policy"} " · " a href="/admin/discovery/links" {"Check published local links"}}
            p class="muted" {"Drafts and noindex pages are excluded from sitemap/search. Indexing and rich results remain outside the application's control."}
        }
        section class="panel" {
            h2 {"Exact redirects"}
            form class="field-row" method="post" action="/admin/discovery/redirects" {
                (view::csrf(&s))
                label {"Source path" input name="source" required placeholder="/old-page";}
                label {"Destination path" input name="target" required placeholder="/new-page";}
                label {"HTTP status" select name="code" aria-label="HTTP status" {option value="301" {"301 · permanent"} option value="302" {"302 · temporary"}}}
                button {"Add redirect"}
            }
            @for r in redirects {
                form class="toolbar" method="post" action="/admin/discovery/redirects" {
                    (view::csrf(&s)) input type="hidden" name="source" value=(r.get::<String,_>("source"));
                    input type="hidden" name="version" value=(r.get::<i64,_>("version"));
                    input type="hidden" name="target" value=(r.get::<String,_>("target"));
                    input type="hidden" name="code" value=(r.get::<i64,_>("code"));
                    span {(r.get::<String,_>("source")) " → " (r.get::<String,_>("target"))}
                    button class="secondary" name="action" value="delete" {"Remove redirect"}
                }
            }
        }
    };
    Ok(axum::response::Html(view::layout(
        "Discovery",
        &app.db.settings().await?,
        Some(&s),
        body,
    ))
    .into_response())
}
pub fn routes() -> Router<App> {
    Router::new()
        .route("/admin/discovery", get(admin_page).post(save_config))
        .route("/admin/discovery/redirects", post(edit_redirect))
        .route("/admin/discovery/languages", post(edit_language))
        .route("/admin/discovery/business", post(edit_business))
        .route("/admin/discovery/links", get(links_page))
        .route("/sitemap.xml", get(sitemap))
        .route("/sitemap-index.xml", get(sitemap_index))
        .route("/robots.txt", get(robots))
}

#[derive(Deserialize)]
struct LanguageForm {
    csrf: String,
    version: i64,
    code: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    direction: String,
    #[serde(default)]
    search_label: String,
    #[serde(default)]
    navigation: String,
    #[serde(default)]
    action: String,
}
async fn edit_language(
    State(app): State<App>,
    headers: HeaderMap,
    Form(f): Form<LanguageForm>,
) -> Result<Response> {
    let s = admin_session(&app, &headers).await?;
    crate::auth::csrf(&s, &f.csrf)?;
    let (mut d, _) = load(&app).await?;
    if f.action == "remove" {
        d.languages.retain(|l| l.code != f.code);
    } else {
        let navigation = if f.navigation.trim().is_empty() {
            Vec::new()
        } else {
            serde_json::from_str(&f.navigation).map_err(|_| {
                Error::invalid("Navigation must be a JSON array of label/url objects.")
            })?
        };
        let language = Language {
            code: f.code.clone(),
            label: f.label,
            direction: f.direction,
            search_label: f.search_label,
            navigation,
        };
        if let Some(old) = d.languages.iter_mut().find(|l| l.code == f.code) {
            *old = language;
        } else {
            d.languages.push(language);
        }
    }
    configure(&app, d, f.version).await?;
    Ok(axum::response::Redirect::to("/admin/discovery").into_response())
}
#[derive(Deserialize)]
struct BusinessForm {
    csrf: String,
    version: i64,
    name: String,
    street: String,
    city: String,
    postal_code: String,
    country: String,
    telephone: String,
}
async fn edit_business(
    State(app): State<App>,
    headers: HeaderMap,
    Form(f): Form<BusinessForm>,
) -> Result<Response> {
    let s = admin_session(&app, &headers).await?;
    crate::auth::csrf(&s, &f.csrf)?;
    let (mut d, _) = load(&app).await?;
    d.business = Business {
        name: f.name,
        street: f.street,
        city: f.city,
        postal_code: f.postal_code,
        country: f.country,
        telephone: f.telephone,
    };
    configure(&app, d, f.version).await?;
    Ok(axum::response::Redirect::to("/admin/discovery").into_response())
}
pub fn business_footer(meta: &Value) -> Markup {
    let b = &meta["business"];
    html! {@if b["name"].as_str().is_some_and(|s|!s.is_empty()){footer class="site-footer" aria-label="Business information"{address{strong{(b["name"].as_str().unwrap_or(""))}br;(b["street"].as_str().unwrap_or(""))br;(b["city"].as_str().unwrap_or(""))" "(b["postal_code"].as_str().unwrap_or(""))" · "(b["country"].as_str().unwrap_or(""))br;(b["telephone"].as_str().unwrap_or(""))}}}}
}
#[derive(Deserialize, Default)]
struct LinksQuery {
    #[serde(default)]
    after: String,
}
async fn links_page(
    State(app): State<App>,
    headers: HeaderMap,
    Query(q): Query<LinksQuery>,
) -> Result<Response> {
    let s = admin_session(&app, &headers).await?;
    if !q.after.is_empty() && uuid::Uuid::parse_str(&q.after).is_err() {
        return Err(Error::invalid("Invalid link-check cursor."));
    }
    let (d, _) = load(&app).await?;
    let rows=sqlx::query("SELECT id,published_title,published_body,published_locale,published_slug FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id) AND id>$1 ORDER BY id LIMIT 21").bind(&q.after).fetch_all(&app.db.pool).await?;
    let mut found = Vec::new();
    let mut candidates = BTreeSet::new();
    let mut media = BTreeSet::new();
    let origin = app.config.origin();
    for r in rows.iter().take(20) {
        for event in pulldown_cmark::Parser::new(&r.get::<String, _>("published_body")) {
            let dest = match event {
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, .. })
                | pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image { dest_url, .. }) => {
                    dest_url.to_string()
                }
                _ => continue,
            };
            if found.len() >= 200 {
                return Err(Error::invalid(
                    "This batch exceeds 200 links; split the source content before checking.",
                ));
            }
            if dest.starts_with('#') || dest.starts_with("//") {
                continue;
            }
            let path = if dest.starts_with('/') && !dest.starts_with("//") {
                dest.split(['?', '#']).next().unwrap_or("").to_owned()
            } else if let Ok(u) = url::Url::parse(&dest) {
                if u.origin().ascii_serialization() != origin {
                    continue;
                }
                u.path().to_owned()
            } else {
                let base = format!(
                    "{origin}{}",
                    d.path(
                        &r.get::<String, _>("published_locale"),
                        &r.get::<String, _>("published_slug")
                    )
                );
                url::Url::parse(&base)
                    .unwrap()
                    .join(&dest)
                    .map_err(|_| Error::invalid("Published link is not a valid URL."))?
                    .path()
                    .to_owned()
            };
            if let Some(id) = path.strip_prefix("/media/") {
                media.insert(id.to_owned());
            } else {
                candidates.insert(path.rsplit('/').next().unwrap_or("").to_owned());
            }
            found.push((r.get::<String, _>("published_title"), path));
        }
    }
    let mut known = BTreeSet::from(["/".to_owned(), "/search".into(), "/feed.xml".into()]);
    for l in &d.languages {
        known.insert(d.path(&l.code, ""));
        known.insert(d.path(&l.code, "search"));
        known.insert(format!("/{}", l.code));
    }
    if !candidates.is_empty() {
        let mut sql = sqlx::QueryBuilder::<sqlx::Any>::new(
            "SELECT published_locale,published_slug FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id) AND published_slug IN (",
        );
        let mut list = sql.separated(",");
        for slug in &candidates {
            list.push_bind(slug);
        }
        list.push_unseparated(")");
        for r in app.db.fetch_builder(&mut sql).await? {
            let slug: String = r.get("published_slug");
            known.insert(d.path(&r.get::<String, _>("published_locale"), &slug));
            known.insert(format!("/{slug}"));
        }
    }
    if !media.is_empty() {
        let mut sql = sqlx::QueryBuilder::<sqlx::Any>::new(
            "SELECT id FROM media WHERE visibility='public' AND id IN (",
        );
        let mut list = sql.separated(",");
        for id in &media {
            list.push_bind(id);
        }
        list.push_unseparated(")");
        for r in app.db.fetch_builder(&mut sql).await? {
            known.insert(format!("/media/{}", r.get::<String, _>("id")));
        }
    }
    let redirects = sqlx::query("SELECT source,target FROM redirects ORDER BY source LIMIT 1000")
        .fetch_all(&app.db.pool)
        .await?;
    let graph: BTreeMap<String, String> = redirects
        .into_iter()
        .map(|r| (r.get("source"), r.get("target")))
        .collect();
    let broken: Vec<_> = found
        .into_iter()
        .filter(|(_, path)| {
            let mut target = path;
            for _ in 0..16 {
                match graph.get(target) {
                    Some(next) => target = next,
                    None => break,
                }
            }
            !known.contains(target)
        })
        .collect();
    let body = html! {(view::heading("Discovery","Published link check","Checks up to 20 published items and 200 links per batch against local publication and public media. External URLs are skipped; no HTTP crawler runs."))section class="panel"{p{(broken.len())" unresolved local links in this batch."}@for(title,path)in broken{p{strong{(title)}" → "code{(path)}}}@if rows.len()>20{a class="button secondary" href=(format!("/admin/discovery/links?after={}",rows[19].get::<String,_>("id"))){"Check next batch"}}a class="button secondary" href="/admin/discovery"{"Back to discovery"}}};
    Ok(axum::response::Html(view::layout(
        "Published link check",
        &app.db.settings().await?,
        Some(&s),
        body,
    ))
    .into_response())
}
