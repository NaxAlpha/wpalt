//! Bounded graph of actual anonymous server output, with explicit scope/refusals.
use crate::{
    App,
    error::{Error, Result},
};
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use serde::Serialize;
use sqlx::Row;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::Ordering,
};

#[derive(Debug, Serialize)]
pub struct Source {
    pub path: String,
    pub publication: Option<String>,
    pub html_bytes: usize,
    pub html_sha256: String,
}
#[derive(Debug, Serialize)]
pub struct Edge {
    pub source: String,
    pub target: String,
    pub element: String,
    pub placement: String,
    pub state: String,
    pub destination: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub complete: bool,
    pub scope: &'static str,
    pub generation: u64,
    pub sources: Vec<Source>,
    pub edges: Vec<Edge>,
    pub without_inbound: Vec<String>,
    pub html_bytes: usize,
    pub elapsed_ms: f64,
}
#[derive(Default)]
pub struct Parsed {
    pub ids: BTreeSet<String>,
    pub links: Vec<(String, String, String)>,
}
/// A streaming allocation preflight before building RcDom. This is deliberately
/// conservative for malformed/self-closing non-void HTML; refusal is explicit.
fn allocation_preflight(html: &str) -> Result<()> {
    use html5ever::tokenizer::states::RawKind;
    use html5ever::tokenizer::{
        BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
    };
    use std::cell::{Cell, RefCell};
    #[derive(Default)]
    struct Budget {
        tokens: Cell<usize>,
        refused: Cell<bool>,
        open: RefCell<Vec<String>>,
    }
    impl TokenSink for Budget {
        type Handle = ();
        fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
            if self.refused.get() {
                return TokenSinkResult::Continue;
            }
            if !matches!(token, Token::ParseError(_) | Token::EOFToken) {
                self.tokens.set(self.tokens.get() + 1);
                if self.tokens.get() > 50000 {
                    self.refused.set(true);
                    return TokenSinkResult::Continue;
                }
            }
            if let Token::TagToken(tag) = token {
                let name = tag.name.as_ref();
                let mut open = self.open.borrow_mut();
                if tag.kind == TagKind::EndTag {
                    if let Some(index) = open.iter().rposition(|s| s == name) {
                        open.truncate(index);
                    }
                } else if ![
                    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta",
                    "param", "source", "track", "wbr",
                ]
                .contains(&name)
                {
                    if open.len() == 128 {
                        self.refused.set(true);
                        return TokenSinkResult::Continue;
                    }
                    open.push(name.to_string());
                    let raw = match name {
                        "script" => Some(RawKind::ScriptData),
                        "style" | "xmp" | "iframe" | "noembed" | "noframes" => {
                            Some(RawKind::Rawtext)
                        }
                        "title" | "textarea" => Some(RawKind::Rcdata),
                        _ => None,
                    };
                    if let Some(raw) = raw {
                        return TokenSinkResult::RawData(raw);
                    }
                    if name == "plaintext" {
                        return TokenSinkResult::Plaintext;
                    }
                }
            }
            TokenSinkResult::Continue
        }
    }
    let tokenizer = Tokenizer::new(Budget::default(), TokenizerOpts::default());
    let input = BufferQueue::default();
    // Bounded chunks allow refusal before allocating a full DOM or deep stack.
    let mut start = 0;
    while start < html.len() {
        let mut end = (start + 4096).min(html.len());
        while !html.is_char_boundary(end) {
            end -= 1;
        }
        input.push_back(html[start..end].into());
        let _ = tokenizer.feed(&input);
        if tokenizer.sink.refused.get() {
            return Err(Error::invalid(
                "Rendered source exceeds its allocation preflight; no complete graph was produced.",
            ));
        }
        start = end;
    }
    tokenizer.end();
    if tokenizer.sink.refused.get() {
        return Err(Error::invalid(
            "Rendered source exceeds its allocation preflight; no complete graph was produced.",
        ));
    }
    Ok(())
}
/// Parse only bounded, already-rendered HTML. Head/scripts/styles are not graph edges.
pub fn parse(html: &str) -> Result<Parsed> {
    if html.len() > 2 * 1024 * 1024 {
        return Err(Error::invalid(
            "Rendered source exceeds 2 MiB; no complete graph was produced.",
        ));
    }
    allocation_preflight(html)?;
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let mut out = Parsed::default();
    let mut count = 0;
    fn walk(
        node: &Handle,
        depth: usize,
        count: &mut usize,
        placement: &str,
        blocked: bool,
        out: &mut Parsed,
    ) -> Result<()> {
        *count += 1;
        if depth > 64 || *count > 50000 {
            return Err(Error::invalid(
                "Rendered source exceeds its tree work limit; no complete graph was produced.",
            ));
        }
        let mut placement = placement;
        let mut blocked = blocked;
        if let NodeData::Element { name, attrs, .. } = &node.data {
            let tag = name.local.as_ref();
            blocked |= ["head", "script", "style", "template"].contains(&tag);
            placement = match tag {
                "nav" => "navigation",
                "header" => "header",
                "footer" => "footer",
                "article" => "publication",
                _ => placement,
            };
            for attr in attrs.borrow().iter() {
                let key = attr.name.local.as_ref();
                let value = attr.value.to_string();
                if key == "id" || (tag == "a" && key == "name") {
                    out.ids.insert(value.clone());
                }
                if !blocked && ((tag == "a" && key == "href") || (tag == "img" && key == "src")) {
                    if value.len() > 2000 || out.links.len() == 20000 {
                        return Err(Error::invalid(
                            "Rendered edges exceed their work limit; no complete graph was produced.",
                        ));
                    }
                    out.links.push((value, tag.into(), placement.into()));
                }
            }
        }
        for child in node.children.borrow().iter() {
            walk(child, depth + 1, count, placement, blocked, out)?;
        }
        Ok(())
    }
    if let Err(error) = walk(&dom.document, 0, &mut count, "template", false, &mut out) {
        // RcDom children drop recursively. Detach a refused tree iteratively so
        // repaired/malformed markup cannot turn a safe refusal into stack overflow.
        let mut pending = vec![dom.document.clone()];
        while let Some(node) = pending.pop() {
            pending.extend(node.children.borrow_mut().drain(..));
            if let NodeData::Element {
                template_contents, ..
            } = &node.data
                && let Some(content) = template_contents.borrow_mut().take()
            {
                pending.push(content);
            }
        }
        return Err(error);
    }
    Ok(out)
}
fn fragment(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let pair = std::str::from_utf8(bytes.get(at + 1..at + 3)?).ok()?;
            out.push(u8::from_str_radix(pair, 16).ok()?);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}
#[derive(Clone)]
struct Target {
    state: String,
    destination: Option<String>,
}
async fn target(
    app: &App,
    plan: &super::redirects::Plan,
    discovery: &super::Definition,
    path: &str,
    pages: &BTreeMap<String, Parsed>,
) -> Result<Target> {
    if pages.contains_key(path) {
        return Ok(Target {
            state: "resolved-source".into(),
            destination: Some(path.into()),
        });
    }
    if let Some(id) = path.strip_prefix("/media/") {
        let parts: Vec<_> = id.split('/').collect();
        if parts.len() != 1 {
            return Ok(Target {
                state: "media-variant-unscanned".into(),
                destination: None,
            });
        }
        if uuid::Uuid::parse_str(id).is_err() {
            return Ok(Target {
                state: "broken-media".into(),
                destination: None,
            });
        }
        let row=sqlx::query("SELECT filename,visibility,mime,CASE WHEN EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='media' AND mr.resource_id=media.id) THEN 1 ELSE 0 END AS protected FROM media WHERE id=$1").bind(id).fetch_optional(&app.db.pool).await?;
        let Some(row) = row else {
            return Ok(Target {
                state: "broken-media".into(),
                destination: None,
            });
        };
        if row.get::<String, _>("visibility") != "public" || row.get::<i64, _>("protected") != 0 {
            return Ok(Target {
                state: "private-skipped".into(),
                destination: None,
            });
        }
        let filename: String = row.get("filename");
        let mime: String = row.get("mime");
        if !crate::backup::safe_filename(&filename)
            || ![
                "image/png",
                "image/jpeg",
                "image/webp",
                "image/gif",
                "video/mp4",
            ]
            .contains(&mime.as_str())
        {
            return Ok(Target {
                state: "broken-media".into(),
                destination: None,
            });
        }
        let state = match tokio::fs::metadata(app.config.data_dir.join("media").join(filename))
            .await
        {
            Ok(m) if m.is_file() && m.len() > 0 && m.len() <= 32 * 1024 * 1024 => "resolved-media",
            _ => "broken-media",
        };
        return Ok(Target {
            state: state.into(),
            destination: None,
        });
    }
    if !super::redirects::safe_path(path) {
        return Ok(Target {
            state: "reserved-or-encoded-skipped".into(),
            destination: None,
        });
    }
    match super::redirects::route(app, discovery, path).await? {
        super::redirects::RouteState::Protected => {
            return Ok(Target {
                state: "private-skipped".into(),
                destination: None,
            });
        }
        super::redirects::RouteState::Public(Some(id)) => {
            // Noncanonical publication aliases are server-owned redirects, not missing pages.
            let row=sqlx::query("SELECT published_slug,published_locale FROM posts WHERE id=$1 AND status='published'").bind(id).fetch_one(&app.db.pool).await?;
            let canonical = discovery.path(
                &row.get::<String, _>("published_locale"),
                &row.get::<String, _>("published_slug"),
            );
            return Ok(Target {
                state: if pages.contains_key(&canonical) {
                    "resolved-publication-alias"
                } else {
                    "public-route-unscanned"
                }
                .into(),
                destination: pages.contains_key(&canonical).then_some(canonical),
            });
        }
        super::redirects::RouteState::Public(None) => {
            return Ok(Target {
                state: "public-route-unscanned".into(),
                destination: None,
            });
        }
        super::redirects::RouteState::Missing => {}
    }
    match super::redirects::resolve_runtime(app, plan, path).await {
        Ok(Some(resolved)) if pages.contains_key(&resolved.destination) => Ok(Target {
            state: "resolved-redirect".into(),
            destination: Some(resolved.destination),
        }),
        Ok(Some(resolved)) => {
            let next = super::redirects::route(app, discovery, &resolved.destination).await?;
            Ok(Target {
                state: if matches!(next, super::redirects::RouteState::Public(_)) {
                    "redirect-route-unscanned"
                } else {
                    "broken-redirect-target"
                }
                .into(),
                destination: None,
            })
        }
        Ok(None) => Ok(Target {
            state: "broken-local".into(),
            destination: None,
        }),
        Err(error) if error.0 == axum::http::StatusCode::UNPROCESSABLE_ENTITY => Ok(Target {
            state: "redirect-refused".into(),
            destination: None,
        }),
        Err(error) => Err(error),
    }
}
pub async fn report(app: &App) -> Result<Report> {
    tokio::time::timeout(std::time::Duration::from_secs(20),report_inner(app)).await
        .map_err(|_|Error::invalid("Rendered graph exceeded its 20-second work budget; no complete graph was produced."))?
}
async fn report_inner(app: &App) -> Result<Report> {
    let started = std::time::Instant::now();
    let _read = app.mutations.lock().await;
    let generation = app.cache_generation.load(Ordering::SeqCst);
    let (discovery, _) = super::load(app).await?;
    let rows=sqlx::query("SELECT id,published_slug,published_locale FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id) ORDER BY id LIMIT 1001").fetch_all(&app.db.pool).await?;
    if rows.len() > 1000 {
        return Err(Error::invalid(
            "Rendered graph supports 1,000 public publications; no partial orphan result was produced.",
        ));
    }
    let mut declarations: Vec<(String, String, Option<String>, Option<String>)> = discovery
        .languages
        .iter()
        .map(|l| (discovery.path(&l.code, ""), l.code.clone(), None, None))
        .collect();
    for row in rows {
        let locale: String = row.get("published_locale");
        let slug: String = row.get("published_slug");
        declarations.push((
            discovery.path(&locale, &slug),
            locale,
            Some(slug),
            Some(row.get("id")),
        ));
    }
    let mut sources = Vec::new();
    let mut pages = BTreeMap::new();
    let mut html_bytes = 0;
    let mut edges_count = 0;
    for (path, locale, slug, publication) in declarations {
        if started.elapsed().as_secs() > 20 {
            return Err(Error::invalid(
                "Rendered graph exceeded its 20-second work budget; no complete graph was produced.",
            ));
        }
        let html = crate::web::render_anonymous_source(app, locale, slug).await?;
        html_bytes += html.len();
        if html_bytes > 64 * 1024 * 1024 {
            return Err(Error::invalid(
                "Rendered graph exceeds 64 MiB aggregate HTML; no complete graph was produced.",
            ));
        }
        let digest = crate::auth::digest(html.as_bytes());
        let size = html.len();
        let parsed = tokio::task::spawn_blocking(move || parse(&html))
            .await
            .map_err(|_| Error::invalid("Rendered graph worker interrupted."))??;
        edges_count += parsed.links.len();
        if edges_count > 20000 {
            return Err(Error::invalid(
                "Rendered graph exceeds 20,000 edges; no partial orphan result was produced.",
            ));
        }
        sources.push(Source {
            path: path.clone(),
            publication,
            html_bytes: size,
            html_sha256: digest,
        });
        pages.insert(path, parsed);
    }
    let plan = super::redirects::plan(app).await?;
    let origin = url::Url::parse(&app.config.origin())
        .map_err(|_| Error::invalid("Invalid public origin."))?;
    let mut resolved = BTreeMap::new();
    let mut inbound = BTreeSet::new();
    let mut edges = Vec::new();
    for (source, parsed) in &pages {
        let base = origin
            .join(source)
            .map_err(|_| Error::invalid("Invalid rendered source path."))?;
        for (href, element, placement) in &parsed.links {
            let mut edge = Edge {
                source: source.clone(),
                target: href.clone(),
                element: element.clone(),
                placement: placement.clone(),
                state: String::new(),
                destination: None,
            };
            match base.join(href) {
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
                        resolved.insert(
                            path.to_string(),
                            target(app, &plan, &discovery, path, &pages).await?,
                        );
                    }
                    let outcome = &resolved[path];
                    edge.state = outcome.state.clone();
                    edge.destination = outcome.destination.clone();
                    if let Some(destination) = &edge.destination {
                        if let Some(raw) = url.fragment().filter(|s| !s.is_empty()) {
                            if raw.contains(":~:") {
                                // Browser text directives need client layout/text matching.
                                edge.state = "fragment-directive-unscanned".into();
                            } else {
                                let ids = &pages[destination].ids;
                                let found = ids.contains(raw)
                                    || fragment(raw).is_some_and(|id| {
                                        ids.contains(&id) || id.eq_ignore_ascii_case("top")
                                    });
                                if !found {
                                    edge.state = "broken-fragment".into();
                                }
                            }
                        }
                        if element == "a"
                            && source != destination
                            && !["broken-fragment", "fragment-directive-unscanned"]
                                .contains(&edge.state.as_str())
                        {
                            inbound.insert(destination.clone());
                        }
                    }
                }
            }
            edges.push(edge);
            if started.elapsed().as_secs() > 20 {
                return Err(Error::invalid(
                    "Rendered graph exceeded its 20-second work budget; no complete graph was produced.",
                ));
            }
        }
    }
    if generation != app.cache_generation.load(Ordering::SeqCst) {
        return Err(Error::conflict());
    }
    let without_inbound = sources
        .iter()
        .filter(|s| s.publication.is_some() && !inbound.contains(&s.path))
        .map(|s| s.path.clone())
        .collect();
    Ok(Report {
        complete: true,
        scope: "Canonical anonymous published content and enumerated language landing pages; no external fetch, query-state expansion or client JavaScript.",
        generation,
        sources,
        edges,
        without_inbound,
        html_bytes,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}
