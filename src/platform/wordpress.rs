//! Offline WXR assessment. XML namespaces are resolved by URI, never by a trusted prefix.
use crate::{
    auth::digest,
    error::{Error, Result},
};
use quick_xml::{NsReader, events::Event, name::ResolveResult};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_NODES: usize = 100_000;
const MAX_ITEMS: usize = 10_000;
const MAX_TEXT: usize = 512 * 1024;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Element {
    pub name: String,
    pub text: String,
    pub attributes: BTreeMap<String, String>,
    pub children: Vec<Element>,
}
impl Element {
    pub fn value(&self, name: &str) -> &str {
        self.children
            .iter()
            .find(|n| n.name == name)
            .map_or("", |n| n.text.as_str())
    }
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter(move |n| n.name == name)
    }
}
pub struct Assessment {
    pub source_sha256: String,
    pub origin: String,
    pub items: Vec<Element>,
    pub report: Value,
}
fn invalid() -> Error {
    Error::invalid("Invalid or unsupported WordPress WXR 1.2 XML export.")
}
fn name(namespace: ResolveResult<'_>, local: &[u8]) -> Result<String> {
    let local = std::str::from_utf8(local).map_err(|_| invalid())?;
    let prefix = match namespace {
        ResolveResult::Unbound => "",
        ResolveResult::Unknown(_) => return Err(invalid()),
        ResolveResult::Bound(ns) => match ns.as_ref() {
            b"http://wordpress.org/export/1.2/" => "wp:",
            b"http://purl.org/rss/1.0/modules/content/" => "content:",
            b"http://purl.org/dc/elements/1.1/" => "dc:",
            b"http://wordpress.org/export/1.2/excerpt/" => "excerpt:",
            _ => "unsupported:",
        },
    };
    Ok(format!("{prefix}{local}"))
}
/// Fixed input/tree/depth/item/text budgets; no DTD, external entity, network or archive extraction.
pub fn assess(bytes: &[u8]) -> Result<Assessment> {
    let started = std::time::Instant::now();
    if bytes.is_empty() || bytes.len() > MAX_BYTES || std::str::from_utf8(bytes).is_err() {
        return Err(invalid());
    }
    let mut reader = NsReader::from_reader(bytes);
    let mut stack = Vec::<Element>::new();
    let mut root = None;
    let mut count = 0;
    loop {
        let (namespace, event) = reader.read_resolved_event().map_err(|_| invalid())?;
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                count += 1;
                if count > MAX_NODES || stack.len() >= 64 || root.is_some() {
                    return Err(invalid());
                }
                let mut n = Element {
                    name: name(namespace, e.local_name().as_ref())?,
                    ..Element::default()
                };
                for a in e.attributes() {
                    let a = a.map_err(|_| invalid())?;
                    let key = std::str::from_utf8(a.key.as_ref()).map_err(|_| invalid())?;
                    if key.len() > 200 || a.value.len() > 2000 || n.attributes.len() >= 64 {
                        return Err(invalid());
                    }
                    let value = a
                        .decode_and_unescape_value(reader.decoder())
                        .map_err(|_| invalid())?
                        .into_owned();
                    n.attributes.insert(key.into(), value);
                }
                stack.push(n);
                if empty {
                    close(&mut stack, &mut root)?;
                }
            }
            Event::End(_) => close(&mut stack, &mut root)?,
            Event::Text(e) => {
                let text = e.xml_content().map_err(|_| invalid())?;
                append(&mut stack, &text)?;
            }
            Event::CData(e) => append(&mut stack, &e.decode().map_err(|_| invalid())?)?,
            Event::GeneralRef(e) => {
                let encoded = e.decode().map_err(|_| invalid())?;
                let text = quick_xml::escape::unescape(&format!("&{encoded};"))
                    .map_err(|_| invalid())?
                    .into_owned();
                append(&mut stack, &text)?;
            }
            Event::Decl(e) => {
                if root.is_some()
                    || !stack.is_empty()
                    || e.version().map_err(|_| invalid())?.as_ref() != b"1.0"
                {
                    return Err(invalid());
                }
                if let Some(enc) = e.encoding()
                    && !enc.map_err(|_| invalid())?.eq_ignore_ascii_case(b"utf-8")
                {
                    return Err(invalid());
                }
            }
            Event::DocType(_) | Event::PI(_) => return Err(invalid()),
            Event::Comment(_) => (),
            Event::Eof => break,
        }
    }
    if !stack.is_empty() {
        return Err(invalid());
    }
    let mut root = root.ok_or_else(invalid)?;
    if root.name != "rss" || root.children.len() != 1 || root.children[0].name != "channel" {
        return Err(invalid());
    }
    let mut channel = root.children.remove(0);
    if channel.value("wp:wxr_version") != "1.2" {
        return Err(invalid());
    }
    let origin = channel.value("wp:base_site_url").to_owned();
    let url = url::Url::parse(&origin).map_err(|_| invalid())?;
    if !["http", "https"].contains(&url.scheme())
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    let items: Vec<_> = channel
        .children
        .drain(..)
        .filter(|n| n.name == "item")
        .collect();
    if items.len() > MAX_ITEMS {
        return Err(invalid());
    }
    let mut ids = BTreeSet::new();
    let mut types = BTreeMap::<String, usize>::new();
    let mut warnings = Vec::new();
    let mut supported = 0;
    let mut previews = Vec::new();
    for item in &items {
        let id = item.value("wp:post_id");
        if id.parse::<u64>().ok().filter(|v| *v > 0).is_none() || !ids.insert(id.to_owned()) {
            return Err(Error::invalid(
                "WordPress export contains missing or duplicate source post identities.",
            ));
        }
        for key in [
            "wp:post_id",
            "wp:post_type",
            "wp:status",
            "title",
            "content:encoded",
            "wp:post_name",
            "wp:post_password",
            "wp:post_parent",
            "wp:attachment_url",
            "link",
            "wp:post_date_gmt",
        ] {
            if item.all(key).count() > 1 || item.all(key).any(|n| !n.children.is_empty()) {
                return Err(invalid());
            }
        }
        for meta in item.all("wp:postmeta") {
            if meta.all("wp:meta_key").count() != 1
                || meta.all("wp:meta_value").count() != 1
                || meta.children.iter().any(|n| !n.children.is_empty())
            {
                return Err(invalid());
            }
        }
        let kind = item.value("wp:post_type");
        *types.entry(kind.into()).or_default() += 1;
        let state = item.value("wp:status");
        let eligible = ["post", "page"].contains(&kind)
            && ["publish", "draft", "pending", "private", "future"].contains(&state);
        if eligible {
            supported += 1;
        } else {
            warnings.push(
                json!({"source_id":id,"code":"unsupported_record","type":kind,"status":state}),
            );
        }
        if ["private", "future", "pending"].contains(&state) {
            warnings.push(json!({"source_id":id,"code":"retain_as_draft","reason":"No implicit private access, schedule or approval is inferred."}));
        }
        if eligible && state == "publish" && !publicly_importable(item) {
            warnings.push(json!({"source_id":id,"code":"access_mapping_required","reason":"Password, shortcode or unrecognized plugin access cannot establish safe public delivery; retain as draft."}));
        }
        let meta: Vec<_> = item
            .all("wp:postmeta")
            .map(|n| n.value("wp:meta_key"))
            .collect();
        if !meta.is_empty() {
            warnings.push(json!({"source_id":id,"code":"metadata_requires_adapter","keys":meta}));
        }
        if item.value("content:encoded").contains('[') {
            warnings.push(json!({"source_id":id,"code":"review_shortcodes","reason":"PHP shortcodes are not executed."}));
        }
        previews.push(json!({"source_id":id,"title":item.value("title"),"source_url":item.value("link"),"slug":item.value("wp:post_name"),"type":kind,"source_status":state,"supported_core_content":eligible,"comments":item.all("wp:comment").count(),"terms":item.all("category").count()}));
    }
    let source_sha256 = digest(bytes);
    let report = json!({"format":"wpalt-wordpress-assessment-v1","source_sha256":source_sha256,"source_site":origin,"source_items":items.len(),"supported_core_items":supported,"types":types,"items":previews,"warnings":warnings,"boundaries":["WXR is not a complete database/plugin/payment export.","No network/media fetch, source mutation, PHP execution or imported account authentication.","Raw source must be independently retained; unsupported records remain in the source, never silently claimed imported."],"budgets":{"input_bytes":MAX_BYTES,"nodes":MAX_NODES,"depth":64,"items":MAX_ITEMS,"text_per_element":MAX_TEXT}});
    tracing::info!(
        event = "wordpress_assessed",
        bytes = bytes.len(),
        items = items.len(),
        nodes = count,
        elapsed_us = started.elapsed().as_micros() as u64
    );
    Ok(Assessment {
        source_sha256,
        origin,
        items,
        report,
    })
}
fn append(stack: &mut [Element], value: &str) -> Result<()> {
    if let Some(n) = stack.last_mut() {
        if n.text.len() + value.len() > MAX_TEXT {
            return Err(invalid());
        }
        n.text.push_str(value);
    } else if !value.trim().is_empty() {
        return Err(invalid());
    }
    Ok(())
}
fn close(stack: &mut Vec<Element>, root: &mut Option<Element>) -> Result<()> {
    let n = stack.pop().ok_or_else(invalid)?;
    if let Some(parent) = stack.last_mut() {
        parent.children.push(n);
    } else if root.replace(n).is_some() {
        return Err(invalid());
    }
    Ok(())
}

/// Prepare an independently recoverable fresh-target package. The source/template stay unchanged.
/// This core slice deliberately requires an empty initialized template and one explicit owner mapping.
pub async fn prepare(
    app: &crate::App,
    bytes: &[u8],
    owner_email: &str,
) -> Result<crate::backup::selection::Prepared> {
    prepare_with_media(app, bytes, owner_email, None).await
}

pub async fn prepare_with_media(
    app: &crate::App,
    bytes: &[u8],
    owner_email: &str,
    media_dir: Option<&std::path::Path>,
) -> Result<crate::backup::selection::Prepared> {
    let assessment = assess(bytes)?;
    let captured = crate::backup::capture(app).await?;
    let envelope: Value = serde_json::from_slice(&captured).map_err(|_| invalid())?;
    let mut snapshot: Value =
        serde_json::from_str(envelope["payload"].as_str().ok_or_else(invalid)?)
            .map_err(|_| invalid())?;
    let tables = snapshot["tables"].as_object_mut().ok_or_else(invalid)?;
    let users = tables["users"].as_array().ok_or_else(invalid)?;
    if users.len() != 1 || users[0]["email"] != owner_email || users[0]["role"] != "admin" {
        return Err(Error::invalid(
            "Use an empty initialized migration template with one explicitly mapped administrator.",
        ));
    }
    let owner = users[0]["id"].as_str().ok_or_else(invalid)?.to_owned();
    for (name, rows) in tables.iter() {
        if ![
            "users",
            "settings",
            "content_models",
            "options",
            "themes",
            "theme_revisions",
            "discovery_settings",
            "site_design",
            "shop_settings",
            "business_usage",
            "engagement_settings",
            "engagement_usage",
            "engagement_dimension_values",
            "engagement_event_names",
            "recovery_mode",
        ]
        .contains(&name.as_str())
            && !rows.as_array().ok_or_else(invalid)?.is_empty()
        {
            return Err(Error::invalid(
                "Migration template contains existing domain records; use a fresh initialized site.",
            ));
        }
    }
    if tables["recovery_mode"][0]["held"] != 0 {
        return Err(Error::invalid(
            "Use an ordinary empty template rather than a held recovery clone.",
        ));
    }
    let mut posts = Vec::new();
    let mut terms = BTreeMap::new();
    let mut assignments = Vec::new();
    let mut published_assignments = Vec::new();
    let mut comments = Vec::new();
    let mut redirects = BTreeMap::new();
    let mut slugs = BTreeSet::new();
    let mut related = BTreeSet::new();
    let links: BTreeMap<String, String> = assessment
        .items
        .iter()
        .filter(|item| {
            ["post", "page"].contains(&item.value("wp:post_type"))
                && crate::content::valid_slug(item.value("wp:post_name"))
        })
        .map(|item| {
            (
                item.value("link").to_owned(),
                format!("/{}", item.value("wp:post_name")),
            )
        })
        .collect();
    let mut warnings = assessment.report["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    warnings.push(json!({"code":"explicit_author_mapping","reason":"All imported authors map to the selected owner; no WordPress credentials or roles are imported."}));
    let (media, files, media_urls) =
        local_media(app, &assessment, media_dir, &mut warnings).await?;
    let source_origin = url::Url::parse(&assessment.origin)
        .map_err(|_| invalid())?
        .origin();
    for item in &assessment.items {
        let kind = item.value("wp:post_type");
        let state = item.value("wp:status");
        if !["post", "page"].contains(&kind)
            || !["publish", "draft", "pending", "private", "future"].contains(&state)
        {
            continue;
        }
        let source_id = item.value("wp:post_id");
        let slug = item.value("wp:post_name");
        if !crate::content::valid_slug(slug) || !slugs.insert(slug.to_owned()) {
            return Err(Error::invalid(
                "Resolve invalid, reserved or duplicate WordPress slugs before package creation.",
            ));
        }
        let title = item.value("title");
        if title.trim().is_empty() || title.len() > 300 {
            return Err(Error::invalid(
                "Imported content needs a nonempty title within 300 bytes.",
            ));
        }
        let html = item.value("content:encoded");
        // Guard recursive HTML conversion independently from the XML envelope.
        // Conservative markup admission can reject malformed/unclosed legacy HTML.
        if html.len() > 128 * 1024 {
            return Err(Error::invalid(
                "Imported HTML exceeds the bounded conversion budget.",
            ));
        }
        let mut depth = 0usize;
        let mut tags = 0usize;
        for chunk in html.split('<').skip(1) {
            let tag = chunk.split('>').next().unwrap_or("").trim();
            if tag.starts_with('!') || tag.starts_with('?') {
                continue;
            }
            tags += 1;
            if tag.starts_with('/') {
                depth = depth.saturating_sub(1);
            } else if !tag.ends_with('/')
                && ![
                    "img", "br", "hr", "input", "meta", "link", "source", "area", "base", "col",
                    "embed", "param", "track", "wbr",
                ]
                .contains(
                    &tag.split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .as_str(),
                )
            {
                depth += 1;
            }
            if depth > 64 || tags > 8192 || html.len() > 128 * 1024 {
                return Err(Error::invalid(
                    "Imported HTML exceeds the bounded conversion budget.",
                ));
            }
        }
        let mut document = super::html::import(html)?;
        map_document(&mut document.root, &media_urls, &links, &mut warnings);
        let document = crate::document::Document::parse(&document.encode())?;
        let body = document.markdown();
        let published = publicly_importable(item);
        let id = stable_id(&assessment.source_sha256, "post", source_id);
        let date = chrono::NaiveDateTime::parse_from_str(
            item.value("wp:post_date_gmt"),
            "%Y-%m-%d %H:%M:%S",
        )
        .map_or(0, |d| d.and_utc().timestamp().max(0));
        let encoded = document.encode();
        let empty = crate::document::empty();
        let mut seo = crate::discovery::Seo {
            schema_type: if kind == "post" { "Article" } else { "WebPage" }.into(),
            ..Default::default()
        };
        for meta in item.all("wp:postmeta") {
            match meta.value("wp:meta_key") {
                "_yoast_wpseo_metadesc" => seo.description = meta.value("wp:meta_value").into(),
                "_yoast_wpseo_title" => {
                    let value = meta.value("wp:meta_value");
                    if value.contains("%%") {
                        warnings
                            .push(json!({"source_id":source_id,"code":"unresolved_seo_variable"}));
                    } else {
                        seo.title = value.into();
                    }
                }
                "_yoast_wpseo_meta-robots-noindex" => {
                    seo.noindex = meta.value("wp:meta_value") == "1"
                }
                _ => (),
            }
        }
        let seo = serde_json::to_string(&seo).map_err(|_| invalid())?;
        crate::discovery::Seo::parse(&seo)?;
        posts.push(json!({"id":id,"slug":slug,"kind":kind,"title":title,"body":body,"document":encoded,"fields":"{}","blocks":"[]","status":if published {"published"}else{"draft"},"version":1,"published_slug":if published{slug}else{""},"published_title":if published{title}else{""},"published_body":if published{body.as_str()}else{""},"published_document":if published{encoded.as_str()}else{empty.as_str()},"published_fields":"{}","published_blocks":"[]","publish_at":0,"published_at":if published{date}else{0},"updated_at":date,"author_id":owner,"locale":"en","translation_group":"","seo":seo,"published_locale":"en","published_translation_group":"","published_seo":if published{seo.as_str()}else{"{}"}}));
        for category in item.all("category") {
            let domain = category
                .attributes
                .get("domain")
                .map(String::as_str)
                .unwrap_or("");
            let taxonomy = match domain {
                "category" => "category",
                "post_tag" => "tag",
                _ => {
                    warnings.push(json!({"source_id":source_id,"code":"unsupported_taxonomy","taxonomy":domain}));
                    continue;
                }
            };
            let term_slug = category
                .attributes
                .get("nicename")
                .map(String::as_str)
                .unwrap_or("");
            if !crate::schema::identifier(term_slug)
                || category.text.trim().is_empty()
                || category.text.len() > 100
            {
                return Err(Error::invalid(
                    "Review invalid source taxonomy names or slugs.",
                ));
            }
            let term_id = stable_id(&assessment.source_sha256, taxonomy, term_slug);
            terms.entry(term_id.clone()).or_insert(
                json!({"id":term_id,"name":category.text,"slug":term_slug,"kind":taxonomy}),
            );
            let relation = json!({"post_id":id,"term_id":term_id});
            if related.insert((id.clone(), term_id.clone())) {
                assignments.push(relation.clone());
                if published {
                    published_assignments.push(relation);
                }
            }
        }
        for comment in item.all("wp:comment") {
            let comment_id = comment.value("wp:comment_id");
            if comment_id.parse::<u64>().is_err() {
                return Err(invalid());
            }
            let name = comment.value("wp:comment_author");
            let body = comment.value("wp:comment_content");
            if name.trim().is_empty()
                || name.len() > 100
                || body.trim().is_empty()
                || body.len() > 4000
            {
                warnings.push(json!({"source_id":source_id,"code":"unsupported_comment","comment_id":comment_id}));
                continue;
            }
            comments.push(json!({"id":stable_id(&assessment.source_sha256,"comment",comment_id),"post_id":id,"name":name,"body":body,"status":if comment.value("wp:comment_approved")=="1"{"approved"}else{"pending"},"created_at":date}));
        }
        if let Ok(link) = url::Url::parse(item.value("link")) {
            let source = link.path();
            let target = format!("/{slug}");
            if link.origin() == source_origin
                && link.query().is_none()
                && link.fragment().is_none()
                && crate::discovery::safe_path(source)
                && source != target
                && source != "/"
                && published
            {
                redirects.insert(
                    source.to_owned(),
                    json!({"source":source,"target":target,"code":301,"version":1}),
                );
            } else if link.query().is_some() {
                warnings
                    .push(json!({"source_id":source_id,"code":"query_permalink_needs_mapping"}));
            }
        }
    }
    if posts.is_empty() {
        return Err(Error::invalid(
            "Export has no supported core content to import.",
        ));
    }
    let counts = json!({"posts":posts.len(),"terms":terms.len(),"comments":comments.len(),"redirects":redirects.len()});
    tables.insert("posts".into(), posts.into());
    tables.insert(
        "terms".into(),
        terms.into_values().collect::<Vec<_>>().into(),
    );
    tables.insert("post_terms".into(), assignments.into());
    tables.insert("published_post_terms".into(), published_assignments.into());
    tables.insert("comments".into(), comments.into());
    tables.insert(
        "redirects".into(),
        redirects.into_values().collect::<Vec<_>>().into(),
    );
    tables.insert("media".into(), media.into());
    snapshot["files"] = files.into();
    snapshot["created_at"] = 0.into();
    snapshot["audit_history"] = json!([]);
    let payload = serde_json::to_string(&snapshot).map_err(|_| invalid())?;
    let bytes = serde_json::to_vec(
        &json!({"format":"wpalt-backup-v12","sha256":digest(payload.as_bytes()),"payload":payload}),
    )
    .map_err(|_| invalid())?;
    crate::backup::inspect(&app.config, &bytes)?;
    let output_hash = digest(&bytes);
    let report = json!({"format":"wpalt-wordpress-package-preview-v1","source_sha256":assessment.source_sha256,"source_site":assessment.origin,"target_origin":app.config.origin(),"owner_email":owner_email,"output_sha256":output_hash,"counts":counts,"media_mapped":media_urls.len(),"warnings":warnings,"boundary":"Fresh-target package only; source/template unchanged, safely mapped public content stays published, ambiguous access stays draft, private/future/pending become drafts, no imported credentials/network/plugin execution. Only explicitly supplied local media is embedded. Retain raw WXR and review unsupported source independently."});
    let plan = digest(
        serde_json::to_string(&report)
            .map_err(|_| invalid())?
            .as_bytes(),
    );
    let mut report = report;
    report["plan"] = plan.clone().into();
    Ok(crate::backup::selection::Prepared {
        report,
        bytes,
        plan,
    })
}
fn stable_id(source: &str, kind: &str, id: &str) -> String {
    let hash = digest(format!("{source}\0{kind}\0{id}").as_bytes());
    let bytes: [u8; 16] = hex::decode(&hash[..32]).unwrap().try_into().unwrap();
    uuid::Uuid::from_bytes(bytes).to_string()
}

async fn local_media(
    app: &crate::App,
    assessment: &Assessment,
    directory: Option<&std::path::Path>,
    warnings: &mut Vec<Value>,
) -> Result<(Vec<Value>, Vec<Value>, BTreeMap<String, String>)> {
    let mut rows = Vec::new();
    let mut files = Vec::new();
    let mut urls = BTreeMap::new();
    let mut used = 0usize;
    let published: BTreeSet<_> = assessment
        .items
        .iter()
        .filter(|i| publicly_importable(i))
        .map(|i| i.value("wp:post_id"))
        .collect();
    let root = if let Some(directory) = directory {
        Some(tokio::fs::canonicalize(directory).await?)
    } else {
        None
    };
    for item in assessment
        .items
        .iter()
        .filter(|i| i.value("wp:post_type") == "attachment")
    {
        let source_id = item.value("wp:post_id");
        let Some(root) = &root else {
            warnings.push(json!({"source_id":source_id,"code":"local_media_directory_required","reason":"Source URLs are not fetched; supply independently copied uploads."}));
            continue;
        };
        let names: Vec<_> = item
            .all("wp:postmeta")
            .filter(|m| m.value("wp:meta_key") == "_wp_attached_file")
            .collect();
        if names.len() != 1 {
            return Err(Error::invalid(
                "Each mapped attachment needs exactly one relative _wp_attached_file value.",
            ));
        }
        let name = names[0].value("wp:meta_value");
        if name.is_empty()
            || name.len() > 500
            || name.contains('\\')
            || name.chars().any(char::is_control)
            || std::path::Path::new(name)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(Error::invalid(
                "Unsafe WordPress media path; use ordinary relative upload paths without traversal.",
            ));
        }
        let root = root.clone();
        let relative = name.to_owned();
        let data = tokio::task::spawn_blocking(move || read_relative(&root, &relative))
            .await
            .map_err(|_| invalid())??;
        used = used.checked_add(data.len()).ok_or_else(invalid)?;
        if used > app.config.max_backup_bytes / 6 {
            return Err(Error::invalid(
                "Mapped media exceeds the configured recovery package budget.",
            ));
        }
        let format = image::guess_format(&data)
            .map_err(|_| Error::invalid("Mapped source is not a supported local image."))?;
        let (extension, mime) = match format {
            image::ImageFormat::Png => ("png", "image/png"),
            image::ImageFormat::Jpeg => ("jpg", "image/jpeg"),
            image::ImageFormat::WebP => ("webp", "image/webp"),
            image::ImageFormat::Gif => ("gif", "image/gif"),
            _ => {
                return Err(Error::invalid(
                    "Only PNG/JPEG/WebP/GIF originals are mapped by this core adapter.",
                ));
            }
        };
        let (width, height) = image::ImageReader::with_format(std::io::Cursor::new(&data), format)
            .into_dimensions()
            .map_err(|_| invalid())?;
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(Error::invalid(
                "Mapped image dimensions exceed the supported 4096-pixel boundary.",
            ));
        }
        let source_url = item.value("wp:attachment_url");
        let parsed = url::Url::parse(source_url).map_err(|_| invalid())?;
        if !["http", "https"].contains(&parsed.scheme())
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || urls.contains_key(source_url)
        {
            return Err(Error::invalid(
                "Source attachment URL is invalid or ambiguous.",
            ));
        }
        let id = stable_id(&assessment.source_sha256, "media", source_id);
        let filename = format!("{id}.{extension}");
        let hash = digest(&data);
        // An orphan/private attachment is never made public merely because it had a URL.
        let public = published.contains(item.value("wp:post_parent"));
        if !public {
            warnings.push(json!({"source_id":source_id,"code":"media_retained_private","reason":"No supported published parent establishes public delivery."}));
        }
        rows.push(json!({"id":id,"filename":filename,"original_name":std::path::Path::new(name).file_name().unwrap().to_string_lossy(),"mime":mime,"alt":item.all("wp:postmeta").find(|m|m.value("wp:meta_key")=="_wp_attachment_image_alt").map_or("",|m|m.value("wp:meta_value")),"visibility":if public{"public"}else{"private"},"size":data.len(),"sha256":hash,"created_at":0}));
        files.push(json!({"filename":filename,"data":data,"sha256":hash}));
        urls.insert(source_url.into(), format!("/media/{id}"));
    }
    Ok((rows, files, urls))
}
fn map_document(
    node: &mut crate::document::Node,
    media: &BTreeMap<String, String>,
    links: &BTreeMap<String, String>,
    warnings: &mut Vec<Value>,
) {
    if node.kind == "image"
        && let Some(src) = node.attrs["src"].as_str().map(String::from)
    {
        if let Some(target) = media.get(&src) {
            node.attrs["src"] = target.clone().into();
        } else {
            warnings.push(json!({"code":"unmapped_image","source_url":src,"reason":"Original URL remains; this server does not fetch or retain its bytes."}));
        }
    }
    for mark in &mut node.marks {
        if mark.kind == "link"
            && let Some(href) = mark.attrs["href"].as_str().map(String::from)
        {
            if let Some(target) = media.get(&href) {
                mark.attrs["href"] = target.clone().into();
                continue;
            }
            if let Some(target) = links.get(&href) {
                mark.attrs["href"] = target.clone().into();
            }
        }
    }
    for child in &mut node.content {
        map_document(child, media, links, warnings);
    }
}

/// Open each component through the already-owned directory descriptor. No path-based
/// metadata/read gap can redirect a raced ancestor or final symlink outside the root.
#[cfg(unix)]
fn read_relative(root: &std::path::Path, name: &str) -> Result<Vec<u8>> {
    use rustix::fs::{Mode, OFlags, openat};
    use std::io::Read;
    let mut directory = std::fs::File::open("/")?;
    for component in root.components() {
        match component {
            std::path::Component::RootDir => (),
            std::path::Component::Normal(component) => {
                let fd = openat(
                    &directory,
                    component,
                    OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::DIRECTORY,
                    Mode::empty(),
                )
                .map_err(|_| invalid())?;
                directory = std::fs::File::from(fd);
            }
            _ => return Err(invalid()),
        }
    }
    let parts: Vec<_> = std::path::Path::new(name).components().collect();
    for (index, component) in parts.iter().enumerate() {
        let last = index + 1 == parts.len();
        let flags = OFlags::RDONLY
            | OFlags::CLOEXEC
            | OFlags::NOFOLLOW
            | OFlags::NONBLOCK
            | if last {
                OFlags::empty()
            } else {
                OFlags::DIRECTORY
            };
        let fd = openat(&directory, component.as_os_str(), flags, Mode::empty()).map_err(|_| {
            Error::invalid(
                "Local media cannot be opened safely; symlinks and invalid paths are refused.",
            )
        })?;
        let file = std::fs::File::from(fd);
        if last {
            if !file.metadata()?.is_file() {
                return Err(Error::invalid("Mapped media must be a regular file."));
            }
            let mut bytes = Vec::new();
            file.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > 8 * 1024 * 1024 {
                return Err(Error::invalid("Mapped media exceeds eight MiB."));
            }
            return Ok(bytes);
        }
        directory = file;
    }
    Err(invalid())
}
#[cfg(not(unix))]
fn read_relative(_root: &std::path::Path, _name: &str) -> Result<Vec<u8>> {
    Err(Error::invalid(
        "Safe local media mapping currently requires a Unix host.",
    ))
}

fn publicly_importable(item: &Element) -> bool {
    ["post", "page"].contains(&item.value("wp:post_type"))
        && item.value("wp:status") == "publish"
        && item.value("wp:post_password").is_empty()
        && !item.value("content:encoded").contains('[')
        && item.all("wp:postmeta").all(|m| {
            let key = m.value("wp:meta_key");
            key.starts_with("_yoast_wpseo_")
                || [
                    "_edit_lock",
                    "_edit_last",
                    "_thumbnail_id",
                    "_wp_page_template",
                    "_wp_old_slug",
                ]
                .contains(&key)
        })
}
