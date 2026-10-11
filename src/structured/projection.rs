//! One bounded graph supplies both JSON-LD and visible facts. No database reads.
use super::{
    declaration::{self, Attribute, Binding, Definition},
    vocabulary,
};
use crate::error::{Error, Result};
use maud::{Markup, html};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub struct Output {
    pub graph: Value,
    pub article: bool,
    pub organization: bool,
    pub warnings: Vec<String>,
}
fn bad() -> Error {
    Error::invalid(
        "Structured output exceeds its supported graph budget or contains invalid facts.",
    )
}
type Key = (String, String, usize);
struct Builder<'a> {
    definitions: &'a BTreeMap<String, Definition>,
    posts: BTreeMap<String, &'a Value>,
    media: &'a BTreeMap<String, String>,
    dimensions: &'a BTreeMap<String, (u32, u32)>,
    origin: &'a str,
    seen: BTreeSet<Key>,
    pending: VecDeque<Key>,
}
impl Builder<'_> {
    fn targets(&mut self, post_id: &str, entity: &str) -> Result<Vec<Value>> {
        let Some(post) = self.posts.get(post_id) else {
            return Ok(Vec::new());
        };
        let Some(d) = post["kind"].as_str().and_then(|k| self.definitions.get(k)) else {
            return Ok(Vec::new());
        };
        let Some(e) = d.entities.get(entity) else {
            return Ok(Vec::new());
        };
        let mut refs = Vec::new();
        for (index, _) in declaration::scopes(&post["fields"], &e.scope)?
            .iter()
            .enumerate()
        {
            let key = (post_id.to_owned(), entity.to_owned(), index);
            if self.seen.insert(key.clone()) {
                if self.seen.len() > 64 {
                    return Err(bad());
                }
                self.pending.push_back(key);
            }
            refs.push(json!({"@id":self.identity(post,entity,index)}));
        }
        Ok(refs)
    }
    fn identity(&self, post: &Value, entity: &str, index: usize) -> String {
        format!(
            "{}{}#wpalt-{}-{}",
            self.origin,
            post["url"].as_str().unwrap_or(""),
            entity,
            index
        )
    }
    fn node(&mut self, key: Key) -> Result<Value> {
        let (post_id, name, index) = key;
        let post = *self.posts.get(&post_id).ok_or_else(bad)?;
        let definition = self
            .definitions
            .get(post["kind"].as_str().ok_or_else(bad)?)
            .ok_or_else(bad)?;
        let entity = definition.entities.get(&name).ok_or_else(bad)?;
        let values = declaration::scopes(&post["fields"], &entity.scope)?;
        let scope = *values.get(index).ok_or_else(bad)?;
        let mut node = json!({"@id":self.identity(post,&name,index),"@type":if entity.types.len()==1 {json!(entity.types[0])} else {json!(entity.types)}});
        for (property, binding) in &entity.properties {
            let mut values = match binding {
                Binding::Field { path, datatype } => {
                    declaration::items(declaration::at(scope, path))
                        .into_iter()
                        .map(|v| declaration::scalar(v, datatype))
                        .collect::<Result<Vec<_>>>()?
                }
                Binding::Enumeration { path, choices } => {
                    declaration::items(declaration::at(scope, path))
                        .into_iter()
                        .map(|v| {
                            let term = v.as_str().and_then(|k| choices.get(k)).ok_or_else(bad)?;
                            Ok(json!(format!("https://schema.org/{term}")))
                        })
                        .collect::<Result<Vec<_>>>()?
                }
                Binding::ConstantEnumeration { value } => {
                    vec![json!(format!("https://schema.org/{value}"))]
                }
                Binding::Entity { entity } => self.targets(&post_id, entity)?,
                Binding::Relationship { path, entity } => {
                    let mut refs = Vec::new();
                    for id in declaration::items(declaration::at(scope, path)) {
                        if let Some(id) = id.as_str() {
                            refs.extend(self.targets(id, entity)?);
                        }
                    }
                    refs
                }
                Binding::Image { path } => {
                    let mut images = Vec::new();
                    for id in declaration::items(declaration::at(scope, path)) {
                        if let Some(id) = id.as_str()
                            && let Some(alt) = self.media.get(id)
                            && let Some((width, height)) = self.dimensions.get(id)
                        {
                            images.push(json!({"@type":"ImageObject","contentUrl":format!("{}/media/{}",self.origin,id),"caption":declaration::scalar(&json!(alt),"Text")?,"width":width,"height":height}));
                        }
                    }
                    images
                }
                Binding::Publication { attribute } => match attribute {
                    Attribute::Title => declaration::items(&post["title"])
                        .into_iter()
                        .map(|v| declaration::scalar(v, "Text"))
                        .collect::<Result<Vec<_>>>()?,
                    Attribute::Language => declaration::items(&post["language"])
                        .into_iter()
                        .map(|v| declaration::scalar(v, "Text"))
                        .collect::<Result<Vec<_>>>()?,
                    Attribute::Url => vec![json!(format!(
                        "{}{}",
                        self.origin,
                        post["url"].as_str().ok_or_else(bad)?
                    ))],
                    Attribute::PublishedAt => {
                        if let Some(at) = post["published_at"].as_i64().filter(|v| *v > 0) {
                            vec![json!(
                                chrono::DateTime::from_timestamp(at, 0)
                                    .ok_or_else(bad)?
                                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                            )]
                        } else {
                            Vec::new()
                        }
                    }
                },
            };
            values.dedup();
            if !values.is_empty() {
                node[property] = if values.len() == 1 {
                    values.remove(0)
                } else {
                    Value::Array(values)
                };
            }
        }
        Ok(node)
    }
}
/// Inputs must already be the authorized published projection from the shared theme
/// resolver. It deliberately cannot obtain accounts, draft relations or private media.
pub fn project(
    definitions: &BTreeMap<String, Definition>,
    root: &Value,
    relations: &BTreeMap<String, Value>,
    media: &BTreeMap<String, String>,
    dimensions: &BTreeMap<String, (u32, u32)>,
    origin: &str,
) -> Result<Option<Output>> {
    let Some(d) = root["kind"].as_str().and_then(|k| definitions.get(k)) else {
        return Ok(None);
    };
    let id = root["id"].as_str().ok_or_else(bad)?;
    let mut posts: BTreeMap<_, _> = relations.iter().map(|(id, v)| (id.clone(), v)).collect();
    posts.insert(id.into(), root);
    let mut builder = Builder {
        definitions,
        posts,
        media,
        dimensions,
        origin,
        seen: BTreeSet::new(),
        pending: VecDeque::new(),
    };
    builder.targets(id, &d.primary)?;
    let mut nodes = Vec::new();
    let mut bytes = 0;
    while let Some(key) = builder.pending.pop_front() {
        let node = builder.node(key)?;
        bytes += serde_json::to_vec(&node).map_err(|_| bad())?.len();
        if bytes > 64 * 1024 {
            return Err(bad());
        }
        nodes.push(node);
    }
    let v = vocabulary::get()?;
    let article = d.entities[&d.primary]
        .types
        .iter()
        .any(|t| v.is_a(t, "Article"));
    let mut organization = false;
    for node in &nodes {
        organization |= declaration::items(&node["@type"])
            .iter()
            .any(|t| t.as_str().is_some_and(|t| v.is_a(t, "Organization")));
    }
    let warnings = super::guidance::warnings(&nodes, v);
    let graph = json!({"@context":"https://schema.org","@graph":nodes});
    if serde_json::to_vec(&graph).map_err(|_| bad())?.len() > 64 * 1024 {
        return Err(bad());
    }
    Ok(Some(Output {
        graph,
        article,
        organization,
        warnings,
    }))
}
fn caption(name: &str) -> String {
    let known = match name {
        "PostalAddress" => Some("Postal address"),
        "OpeningHoursSpecification" | "openingHoursSpecification" => Some("Opening hours"),
        "GeoCoordinates" | "geo" => Some("Coordinates"),
        "streetAddress" => Some("Street"),
        "addressLocality" => Some("City"),
        "addressRegion" => Some("Region"),
        "addressCountry" => Some("Country"),
        "mainEntityOfPage" => Some("Page"),
        "url" => Some("Website"),
        "datePublished" => Some("First published"),
        "inLanguage" => Some("Language"),
        "dayOfWeek" => Some("Days"),
        "itemListElement" => Some("Breadcrumbs"),
        _ => None,
    };
    if let Some(label) = known {
        return label.into();
    }
    let mut label = String::new();
    for (i, c) in name.chars().enumerate() {
        if i > 0 && c.is_ascii_uppercase() {
            label.push(' ');
        }
        label.push(if i == 0 {
            c.to_ascii_uppercase()
        } else {
            c.to_ascii_lowercase()
        });
    }
    label
}
fn node_caption(node: &Value) -> String {
    node["name"]
        .as_str()
        .or_else(|| node["headline"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            let types = declaration::items(&node["@type"]);
            types
                .first()
                .and_then(|t| t.as_str())
                .map(caption)
                .unwrap_or_else(|| "Published entity".into())
        })
}
fn anchor(identity: &str) -> String {
    format!("wpalt-fact-{}", crate::auth::digest(identity.as_bytes()))
}
fn value_markup(value: &Value, nodes: &[Value]) -> Markup {
    if let Some(id) = value["@id"].as_str() {
        let label = nodes
            .iter()
            .find(|n| n["@id"] == id)
            .map(node_caption)
            .unwrap_or_else(|| "Related published entity".into());
        return html! {a href=(format!("#{}",anchor(id))){(label)}};
    }
    if value["@type"] == "ImageObject" {
        return html! {figure {img src=(value["contentUrl"].as_str().unwrap_or("")) alt=(value["caption"].as_str().unwrap_or("")) width=(value["width"].as_u64().unwrap_or(0)) height=(value["height"].as_u64().unwrap_or(0)) loading="lazy";figcaption{(value["caption"].as_str().unwrap_or(""))}}};
    }
    if let Some(s) = value.as_str()
        && let Some(term) = s.strip_prefix("https://schema.org/")
        && vocabulary::get().is_ok_and(|v| v.enumerations.contains_key(term))
    {
        return html! {a href=(s){(caption(term))}};
    }
    html! {(value.as_str().map(str::to_owned).unwrap_or_else(||value.to_string()))}
}
pub fn visible(graph: &Value) -> Markup {
    html! {
        @if let Some(nodes)=graph["@graph"].as_array() {
            section class="structured-facts" aria-label="Published facts" {
                h2 {"Published facts"}
                @for node in nodes {
                    section data-structured-id=(node["@id"].as_str().unwrap_or("")) id=(anchor(node["@id"].as_str().unwrap_or(""))) {
                        h3 {(node_caption(node))}
                        dl {
                            @for (property,value) in node.as_object().into_iter().flatten().filter(|(k,v)|!(k.starts_with('@') || ["name","headline"].contains(&k.as_str())&&v.is_string())) {
                                dt {(caption(property))}
                                dd {@for item in declaration::items(value) {div {(value_markup(item,nodes))}}}
                            }
                        }
                    }
                }
            }
        }
    }
}
pub fn inspection(graph: &Value, warnings: &Value, draft: bool) -> Markup {
    html! {@if draft&&graph.is_object(){section class="structured-facts" aria-label="Structured output inspection" {
        h2 {"Structured output inspection"}
        p {"Working values on this preview; related entities still use currently public publications. This report is private and is not emitted as public JSON-LD."}
        p {"Schema.org 30.1. Selected Google guidance reviewed 11 October 2026. Warnings are recommendations to review, not a promise of indexing or rich results."}
        @for warning in warnings.as_array().into_iter().flatten(){p {(warning.as_str().unwrap_or(""))}}
        details {summary {"Inspect graph JSON"}pre tabindex="0" role="region" aria-label="Structured graph JSON" {(serde_json::to_string_pretty(graph).unwrap_or_default())}}
    }}}
}
