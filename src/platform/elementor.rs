//! Explicit Elementor 0.4 content projection with per-element loss reporting.
use crate::{
    document::Document,
    error::{Error, Result},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub const MAX_BYTES: usize = 2 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Export {
    title: String,
    #[serde(rename = "type")]
    kind: String,
    version: String,
    page_settings: Value,
    content: Vec<Element>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Element {
    id: String,
    #[serde(rename = "elType")]
    kind: String,
    #[serde(rename = "widgetType", default)]
    widget: String,
    #[serde(rename = "isInner", default)]
    inner: bool,
    settings: Value,
    elements: Vec<Element>,
}
pub struct Projection {
    pub document: Document,
    pub report: Value,
}
fn invalid() -> Error {
    Error::invalid("Review the bounded Elementor 0.4 export structure.")
}
fn settings(value: &Value) -> Result<Vec<&str>> {
    match value {
        Value::Object(o) => Ok(o.keys().map(String::as_str).collect()),
        Value::Array(a) if a.is_empty() => Ok(vec![]),
        _ => Err(invalid()),
    }
}
pub fn project(bytes: &[u8]) -> Result<Projection> {
    if bytes.len() > MAX_BYTES {
        return Err(invalid());
    }
    let source: Export = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if source.version != "0.4"
        || source.title.len() > 300
        || !["page", "post", "header", "footer", "popup", "error-404"]
            .contains(&source.kind.as_str())
    {
        return Err(invalid());
    }
    let page_settings = settings(&source.page_settings)?;
    let mut ids = BTreeSet::new();
    let mut warnings = vec![];
    let mut mappings = vec![];
    let mut content = vec![];
    fn walk(
        items: &[Element],
        depth: usize,
        ids: &mut BTreeSet<String>,
        warnings: &mut Vec<Value>,
        mappings: &mut Vec<Value>,
        content: &mut Vec<crate::document::Node>,
    ) -> Result<()> {
        if depth > 32 {
            return Err(invalid());
        }
        for item in items {
            if item.id.is_empty()
                || item.id.len() > 64
                || !item
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_')
                || !ids.insert(item.id.clone())
                || ids.len() > 2000
            {
                return Err(invalid());
            }
            let keys = settings(&item.settings)?;
            if ["container", "section", "column"].contains(&item.kind.as_str()) {
                mappings.push(json!({"element":item.id,"source":item.kind,"target":"ordered content","inner":item.inner}));
                if !keys.is_empty() {
                    warnings.push(
                        json!({"element":item.id,"code":"layout_settings_not_mapped","keys":keys}),
                    );
                }
                walk(&item.elements, depth + 1, ids, warnings, mappings, content)?;
                continue;
            }
            let mut allowed = Vec::new();
            let document = if item.kind == "widget" && item.widget == "heading" {
                allowed.extend(["title", "header_size"]);
                let title = item
                    .settings
                    .get("title")
                    .and_then(Value::as_str)
                    .ok_or_else(invalid)?;
                let level = match item.settings.get("header_size") {
                    None => "h2",
                    Some(Value::String(level)) => level.as_str(),
                    _ => return Err(invalid()),
                };
                if !["h1", "h2", "h3", "h4", "h5", "h6"].contains(&level) {
                    return Err(invalid());
                }
                // Escape through the HTML adapter; source title cannot inject markup.
                let safe = title
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                Some(super::html::import(&format!("<{level}>{safe}</{level}>"))?)
            } else if item.kind == "widget" && item.widget == "text-editor" {
                allowed.push("editor");
                let html = item
                    .settings
                    .get("editor")
                    .and_then(Value::as_str)
                    .ok_or_else(invalid)?;
                if html.len() > 128 * 1024 {
                    return Err(invalid());
                }
                warnings.push(json!({"element":item.id,"code":"rich_text_projected", "reason":"Sanitized canonical document blocks; inline styles, HTML attributes and executable markup are not reproduced. Review source against native text."}));
                Some(super::html::import(html)?)
            } else {
                None
            };
            if let Some(document) = document {
                let omitted: Vec<_> = keys
                    .into_iter()
                    .filter(|key| !allowed.contains(key))
                    .collect();
                if !omitted.is_empty() {
                    warnings.push(json!({"element":item.id,"code":"widget_settings_not_mapped","keys":omitted}));
                }
                mappings.push(json!({"element":item.id,"source":item.widget,"target":"native document blocks"}));
                content.extend(document.root.content);
            } else {
                warnings.push(json!({"element":item.id,"code":"unsupported_element","type":item.kind,"widget":item.widget,"setting_keys":keys}));
            }
            // Nested children remain independently accounted for, even below unknown widgets.
            walk(&item.elements, depth + 1, ids, warnings, mappings, content)?;
        }
        Ok(())
    }
    walk(
        &source.content,
        0,
        &mut ids,
        &mut warnings,
        &mut mappings,
        &mut content,
    )?;
    let mut document = Document::parse(&crate::document::empty())?;
    if !content.is_empty() {
        document.root.content = content;
    }
    let document = Document::parse(&document.encode())?;
    let report = json!({"format":"wpalt-elementor-projection-v1","source_sha256":crate::auth::digest(bytes),"source_version":source.version,"title":source.title,"type":source.kind,"elements":ids.len(),"page_settings_not_mapped":page_settings,"mappings":mappings,"warnings":warnings,"boundary":"Draft content projection only. Container ordering, heading and text-editor content supported; arbitrary styles, assets, widgets, dynamic tags, popup behavior and template conditions are not reproduced. Retain the source and review every loss. F024 full exact mapping remains incomplete."});
    Ok(Projection { document, report })
}
