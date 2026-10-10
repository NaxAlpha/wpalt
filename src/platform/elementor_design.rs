//! Reviewable source-data compilation into native composition; no source execution.
use crate::{
    auth, content,
    error::{Error, Result},
    schema::Registry,
    theme::{Component, Node, Package},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_BYTES: usize = super::elementor::MAX_BYTES;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub source: Value,
    pub component: String,
    pub target: String,
    #[serde(default)]
    pub content_id: String,
    #[serde(default)]
    pub content_kind: String,
    #[serde(default)]
    pub media: BTreeMap<String, String>,
    #[serde(default)]
    pub fonts: BTreeMap<String, String>,
    #[serde(default)]
    pub global_styles: BTreeMap<String, String>,
}
#[derive(Serialize)]
pub struct Review {
    pub package: Package,
    pub report: Value,
    pub fingerprint: String,
}
fn invalid() -> Error {
    Error::invalid("Review the bounded Elementor design export, mappings and explicit destination.")
}
fn object(value: &Value) -> Result<BTreeMap<String, Value>> {
    if value
        .as_object()
        .is_some_and(|o| o.len() > 256 || o.keys().any(|k| k.len() > 256))
    {
        return Err(invalid());
    }
    if value.as_array().is_some_and(Vec::is_empty) {
        return Ok(BTreeMap::new());
    }
    value
        .as_object()
        .map(|v| v.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .ok_or_else(invalid)
}
struct Compiler<'a> {
    request: &'a Request,
    ids: BTreeSet<String>,
    counter: usize,
    prefix: String,
    native_styles: BTreeMap<String, crate::theme::Style>,
    mappings: Vec<Value>,
    losses: Vec<Value>,
}
impl Compiler<'_> {
    fn node(&mut self, kind: &str) -> Result<Node> {
        self.counter += 1;
        if self.counter > 512 {
            return Err(Error::invalid(
                "Imported composition exceeds 512 nodes; split the source design.",
            ));
        }
        serde_json::from_value(json!({"id":format!("{}-{}",self.prefix,self.counter),"kind":kind}))
            .map_err(|_| invalid())
    }
    fn loss(&mut self, id: &str, code: &str, detail: Value) {
        self.losses
            .push(json!({"element":id,"code":code,"detail":detail}));
    }
    fn literal(
        &mut self,
        id: &str,
        settings: &mut BTreeMap<String, Value>,
        key: &str,
    ) -> Result<String> {
        let Some(value) = settings.remove(key) else {
            return Ok(String::new());
        };
        let value = if let Some(kind) = value.get("$$type").and_then(Value::as_str) {
            if !["string", "html", "escaped-html", "number", "boolean"].contains(&kind) {
                self.loss(
                    id,
                    "typed_property_not_mapped",
                    json!({"key":key,"type":kind}),
                );
                return Ok(String::new());
            }
            if value
                .as_object()
                .is_none_or(|o| o.keys().any(|k| !["$$type", "value"].contains(&k.as_str())))
            {
                self.loss(id, "typed_property_metadata_not_mapped", json!(key));
            }
            value.get("value").cloned().ok_or_else(invalid)?
        } else {
            value
        };
        match value {
            Value::String(s) if s.len() <= 128 * 1024 => Ok(s),
            Value::Null => Ok(String::new()),
            _ => Err(invalid()),
        }
    }
    fn link(
        &mut self,
        id: &str,
        settings: &mut BTreeMap<String, Value>,
        key: &str,
    ) -> Result<String> {
        let Some(value) = settings.remove(key) else {
            return Ok(String::new());
        };
        let value = if value.get("$$type").and_then(Value::as_str) == Some("link") {
            value.get("value").cloned().ok_or_else(invalid)?
        } else {
            value
        };
        let href = if value.get("destination").is_some() {
            let destination = &value["destination"];
            self.loss(id, "atomic_link_options_normalized", json!(key));
            if destination.get("$$type").and_then(Value::as_str) == Some("url") {
                destination
                    .get("value")
                    .and_then(Value::as_str)
                    .ok_or_else(invalid)?
                    .to_owned()
            } else {
                self.loss(id, "dynamic_link_not_mapped", json!(key));
                return Ok(String::new());
            }
        } else if let Some(s) = value.as_str() {
            s.to_owned()
        } else if let Some(s) = value.get("url").and_then(Value::as_str) {
            if value
                .as_object()
                .is_some_and(|o| o.keys().any(|k| k != "url"))
            {
                self.loss(id, "link_options_not_mapped", json!(key));
            }
            s.to_owned()
        } else {
            self.loss(id, "link_not_mapped", json!(key));
            return Ok(String::new());
        };
        if href.len() > 2000 || (!href.is_empty() && !content::safe_nav_url(&href)) {
            return Err(Error::invalid("Unsafe imported link."));
        }
        Ok(href)
    }
    fn media(&mut self, id: &str, value: Value) -> Result<Option<String>> {
        let value = if value.get("$$type").and_then(Value::as_str) == Some("image") {
            self.loss(id,"atomic_image_metadata_normalized",json!("Native admitted asset alt text and dimensions; source sizes and external alt metadata are not copied."));
            let src = &value["value"]["src"];
            if src.get("$$type").and_then(Value::as_str) != Some("image-src") {
                return Err(invalid());
            }
            let src = &src["value"];
            json!({"id":src["id"].get("value").cloned().unwrap_or(Value::Null),"url":src["url"].get("value").cloned().unwrap_or(Value::Null)})
        } else {
            value
        };
        let key = value
            .get("id")
            .and_then(Value::as_i64)
            .map(|v| v.to_string())
            .or_else(|| value.get("url").and_then(Value::as_str).map(str::to_owned));
        if let Some(target) = key.as_ref().and_then(|k| self.request.media.get(k)) {
            return Ok(Some(target.clone()));
        }
        self.loss(id, "local_media_mapping_required", json!({"source":key}));
        Ok(None)
    }
    fn rich_text(&mut self, id: &str, html: &str) -> Result<String> {
        let document = super::html::import(html)?;
        fn escaped(value: &str) -> String {
            let mut out = String::with_capacity(value.len());
            for c in value.chars() {
                if c.is_ascii_punctuation() {
                    out.push('\\');
                }
                out.push(c);
            }
            out
        }
        fn raw(n: &crate::document::Node) -> String {
            n.text
                .clone()
                .unwrap_or_else(|| n.content.iter().map(raw).collect())
        }
        fn fence(value: &str, minimum: usize) -> String {
            let mut longest = 0;
            let mut run = 0;
            for c in value.chars() {
                if c == '`' {
                    run += 1;
                    longest = longest.max(run);
                } else {
                    run = 0;
                }
            }
            "`".repeat((longest + 1).max(minimum))
        }
        fn href(value: &str) -> String {
            let mut out = String::new();
            for c in value.chars() {
                if c.is_ascii() && (c.is_ascii_whitespace() || "<>\\`\"'()[]".contains(c)) {
                    out.push_str(&format!("%{:02X}", c as u32));
                } else {
                    out.push(c);
                }
            }
            out
        }
        fn walk(c: &mut Compiler<'_>, id: &str, n: &crate::document::Node) -> String {
            if n.kind == "image" {
                c.loss(id,"inline_image_not_imported",json!({"source":n.attrs["src"],"reason":"Place an explicitly mapped native image widget; literal rich-text images do not bypass native asset visibility."}));
                return escaped(n.attrs["alt"].as_str().unwrap_or(""));
            }
            if n.kind == "code_block" {
                let value = raw(n);
                let f = fence(&value, 3);
                return format!("{f}\n{value}\n{f}\n\n");
            }
            if n.kind == "text" {
                let value = n.text.as_deref().unwrap_or("");
                let mut text = if n.marks.iter().any(|m| m.kind == "code") {
                    let f = fence(value, 1);
                    format!("{f} {value} {f}")
                } else {
                    escaped(value)
                };
                for mark in n.marks.iter().rev() {
                    text = match mark.kind.as_str() {
                        "code" => text,
                        "strong" => format!("**{text}**"),
                        "em" => format!("*{text}*"),
                        "strike" => format!("~~{text}~~"),
                        "link" => format!(
                            "[{text}](<{}>)",
                            href(mark.attrs["href"].as_str().unwrap_or(""))
                        ),
                        _ => text,
                    };
                }
                return text;
            }
            let inner = n.content.iter().map(|n| walk(c, id, n)).collect::<String>();
            match n.kind.as_str() {
                "paragraph" => format!("{inner}\n\n"),
                "heading" => format!(
                    "{} {inner}\n\n",
                    "#".repeat(n.attrs["level"].as_u64().unwrap_or(2) as usize)
                ),
                "blockquote" | "callout" => format!("> {}\n\n", inner.trim().replace('\n', "\n> ")),
                "list_item" => format!("- {}\n", inner.trim()),
                "bullet_list" | "ordered_list" => format!("{inner}\n"),
                "hard_break" => "  \n".into(),
                "horizontal_rule" => "---\n\n".into(),
                "table_cell" | "table_header" => format!("{} | ", inner.trim()),
                "table_row" => format!("| {inner}\n"),
                _ => inner,
            }
        }
        Ok(walk(self, id, &document.root))
    }
    fn dimension(
        &mut self,
        id: &str,
        s: &mut BTreeMap<String, Value>,
        key: &str,
        max: u64,
    ) -> Result<Option<u64>> {
        let Some(v) = s.remove(key) else {
            return Ok(None);
        };
        let number = if v.is_number() {
            v.as_u64()
        } else if v.get("unit").and_then(Value::as_str) == Some("px") {
            v.get("size").and_then(Value::as_u64)
        } else {
            None
        };
        match number {
            Some(n) if n <= max => Ok(Some(n)),
            _ => {
                self.loss(id, "dimension_not_mapped", json!(key));
                Ok(None)
            }
        }
    }
    fn style(&mut self, id: &str, s: &mut BTreeMap<String, Value>, n: &mut Node) -> Result<()> {
        for (key, is_background) in [
            ("background_color", true),
            ("title_color", false),
            ("text_color", false),
        ] {
            if let Some(v) = s.remove(key) {
                let color = v.as_str().unwrap_or("");
                if color.len() == 7
                    && color.starts_with('#')
                    && color[1..].bytes().all(|b| b.is_ascii_hexdigit())
                {
                    if is_background {
                        n.style.background = color.into()
                    } else {
                        n.style.color = color.into()
                    }
                } else {
                    self.loss(id, "color_not_mapped", json!(key))
                }
            }
        }
        if let Some(v) = self.dimension(id, s, "typography_font_size", 96)? {
            if v >= 12 {
                n.style.font_size = v as u8
            } else {
                self.loss(id, "font_size_not_mapped", json!(v))
            }
        }
        if let Some(v) = s.remove("typography_font_weight") {
            let weight = v
                .as_str()
                .and_then(|s| s.parse::<u16>().ok())
                .or_else(|| v.as_u64().and_then(|n| u16::try_from(n).ok()));
            if let Some(w) = weight.filter(|w| (100..=900).contains(w)) {
                n.style.font_weight = w
            } else {
                self.loss(
                    id,
                    "font_weight_not_mapped",
                    json!("typography_font_weight"),
                )
            }
        }
        if let Some(v) = s.remove("typography_font_family") {
            if v.as_str().is_none_or(|v| v.len() > 300) {
                self.loss(
                    id,
                    "font_family_not_mapped",
                    json!("typography_font_family"),
                );
            } else if let Some(alias) = v.as_str().and_then(|v| self.request.fonts.get(v)) {
                n.style.font = alias.clone();
            } else {
                self.loss(id, "local_font_mapping_required", v)
            }
        }
        if let Some(v) = s.remove("align") {
            match v.as_str() {
                Some("left") => n.style.align = "start".into(),
                Some("right") => n.style.align = "end".into(),
                Some("center") => n.style.align = "center".into(),
                _ => self.loss(id, "alignment_not_mapped", json!("align")),
            }
        }
        if let Some(v) = self.dimension(id, s, "width", 1600)? {
            n.style.width = v as u16;
        }
        if let Some(v) = s.remove("flex_direction") {
            match v.as_str() {
                Some("row") => n.style.layout = "row".into(),
                Some("column") => n.style.layout = "stack".into(),
                _ => self.loss(id, "layout_direction_not_mapped", json!("flex_direction")),
            }
        }
        if let Some(v) = s.remove("gap") {
            let gap = v.get("size").and_then(Value::as_u64).filter(|v| *v <= 64);
            if v.get("unit").and_then(Value::as_str) == Some("px")
                && let Some(gap) = gap
            {
                n.style.gap = gap as u8
            } else {
                self.loss(id, "gap_not_mapped", json!("gap"))
            }
        }
        if let Some(globals) = s.remove("__globals__") {
            for (control, reference) in object(&globals)? {
                let reference = reference
                    .as_str()
                    .filter(|v| v.len() <= 512)
                    .ok_or_else(invalid)?;
                let style = self
                    .request
                    .global_styles
                    .get(reference)
                    .and_then(|alias| self.native_styles.get(alias))
                    .cloned();
                match (control.as_str(), style) {
                    ("title_color" | "text_color", Some(style)) => n.style.color = style.color,
                    ("background_color", Some(style)) => n.style.background = style.background,
                    ("typography_typography", Some(style)) => {
                        n.style.font = style.font;
                        n.style.font_size = style.font_size;
                        n.style.font_weight = style.font_weight;
                        n.style.line_height = style.line_height;
                    }
                    (_, None) => {
                        match control.as_str() {
                            "title_color" | "text_color" => n.style.color.clear(),
                            "background_color" => n.style.background.clear(),
                            "typography_typography" => {
                                n.style.font.clear();
                                n.style.font_size = 0;
                                n.style.font_weight = 0;
                                n.style.line_height = 0;
                            }
                            _ => {}
                        }
                        self.loss(
                            id,
                            "global_style_mapping_required",
                            json!({"control":control,"reference":reference}),
                        );
                        continue;
                    }
                    _ => {
                        self.loss(
                            id,
                            "global_control_not_mapped",
                            json!({"control":control,"reference":reference}),
                        );
                        continue;
                    }
                }
                self.mappings.push(json!({"element":id,"source_global":reference,"control":control,"native_style":self.request.global_styles[reference]}));
            }
            if n.style != crate::theme::Style::default()
                && let Some((name, _)) = self
                    .native_styles
                    .iter()
                    .find(|(_, style)| **style == n.style)
            {
                n.style_ref = name.clone();
                n.style = crate::theme::Style::default();
            }
        }
        Ok(())
    }
    fn walk(&mut self, items: &[Value], depth: usize) -> Result<Vec<Node>> {
        if items.len() > 50 {
            return Err(Error::invalid(
                "A source container has more than 50 children. Split it into native nested groups before import.",
            ));
        }
        if depth > 10 {
            return Err(Error::invalid(
                "Imported design nesting exceeds the native placement depth budget.",
            ));
        }
        let mut nodes = Vec::new();
        for value in items {
            let mut fields = object(value)?;
            let id = fields
                .remove("id")
                .and_then(|v| v.as_str().map(str::to_owned))
                .ok_or_else(invalid)?;
            if id.is_empty()
                || id.len() > 64
                || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                || !self.ids.insert(id.clone())
                || self.ids.len() > 2000
            {
                return Err(invalid());
            }
            let kind = fields
                .remove("elType")
                .and_then(|v| v.as_str().map(str::to_owned))
                .ok_or_else(invalid)?;
            if kind.len() > 64 {
                return Err(invalid());
            }
            let widget = match fields.remove("widgetType") {
                None => String::new(),
                Some(Value::String(s)) if s.len() <= 128 => s,
                _ => return Err(invalid()),
            };
            let mut settings = object(&fields.remove("settings").ok_or_else(invalid)?)?;
            let children = fields
                .remove("elements")
                .and_then(|v| v.as_array().cloned())
                .ok_or_else(invalid)?;
            if let Some(inner) = fields.remove("isInner") {
                if !inner.is_boolean() {
                    return Err(invalid());
                }
                if inner == json!(true) {
                    self.loss(
                        &id,
                        "inner_layout_normalized",
                        json!("Native nested composition"),
                    );
                }
            }
            let extra: Vec<_> = fields
                .into_iter()
                .filter(|(_, v)| !v.is_null() && *v != json!({}) && *v != json!([]))
                .map(|(key, _)| key)
                .collect();
            if !extra.is_empty() {
                self.loss(&id, "element_fields_not_mapped", json!(extra));
            }
            let target = match (kind.as_str(), widget.as_str()) {
                ("container" | "section" | "column" | "e-div-block" | "e-flexbox", _)
                | (_, "e-div-block") => "section",
                ("e-grid", _) => "grid",
                ("widget", "heading" | "e-heading") => "heading",
                ("widget", "text-editor" | "e-paragraph") => "body",
                ("widget", "button" | "e-button") => "link",
                ("widget", "image" | "e-image") => "image",
                ("widget", "image-gallery" | "image-carousel") => "section",
                ("widget", "accordion" | "toggle") => "accordion",
                ("widget", "tabs") => "tabs",
                ("widget", "divider" | "spacer") => "section",
                _ => {
                    self.loss(
                        &id,
                        "widget_not_mapped",
                        json!({"type":kind,"widget":widget}),
                    );
                    "section"
                }
            };
            let mut n = self.node(target)?;
            if kind == "e-grid" {
                n.style.layout = "grid".into();
                self.loss(&id,"atomic_grid_normalized",json!("Native grid uses its own breakpoint/default tracks until explicit source styles are mapped."));
            }
            if kind == "e-flexbox" {
                n.style.layout = "row".into();
                self.loss(
                    &id,
                    "atomic_flexbox_normalized",
                    json!("Native wrapping row; source styles are reported separately."),
                );
            }
            match target {
                "heading" => {
                    let title = self.literal(&id, &mut settings, "title")?;
                    if title.contains('<') || title.contains('&') {
                        self.loss(
                            &id,
                            "heading_markup_rendered_as_literal",
                            json!("Native headings escape source markup."),
                        );
                    }
                    n.text = json!(title);
                    let key = if widget == "e-heading" {
                        "tag"
                    } else {
                        "header_size"
                    };
                    let level = self.literal(&id, &mut settings, key)?;
                    if !level.is_empty() {
                        if let Some(d) = level
                            .strip_prefix('h')
                            .and_then(|v| v.parse::<u8>().ok())
                            .filter(|v| (1..=6).contains(v))
                        {
                            n.level = d
                        } else if level.len() <= 64 {
                            n.kind = "text".into();
                            self.loss(
                                &id,
                                "heading_tag_normalized",
                                json!({"source":level,"target":"native paragraph"}),
                            );
                        } else {
                            return Err(invalid());
                        }
                    }
                    let href = self.link(&id, &mut settings, "link")?;
                    if !href.is_empty() {
                        let mut link = self.node("link")?;
                        link.href = json!(href);
                        link.children.push(n);
                        n = link;
                    }
                }
                "body" => {
                    let key = if widget == "e-paragraph" {
                        "paragraph"
                    } else {
                        "editor"
                    };
                    let html = self.literal(&id, &mut settings, key)?;
                    n.text = json!(self.rich_text(&id, &html)?);
                    self.loss(&id,"rich_text_normalized",json!("Sanitized canonical text rendered as native Markdown; source HTML attributes/styles are not retained."));
                }
                "link" => {
                    n.text = json!(self.literal(&id, &mut settings, "text")?);
                    n.href = json!(self.link(&id, &mut settings, "link")?);
                }
                "image" => {
                    if let Some(image) = settings.remove("image") {
                        n.image = json!(self.media(&id, image)?);
                    }
                    if n.image.is_null() {
                        n.kind = "section".into();
                    }
                    let href = self.link(&id, &mut settings, "link")?;
                    if !href.is_empty() {
                        let mut link = self.node("link")?;
                        link.href = json!(href);
                        link.children.push(n);
                        n = link;
                    }
                }
                "accordion" | "tabs" => {
                    if target == "accordion" {
                        n.kind = "section".into();
                    }
                    if target == "tabs" {
                        n.text = json!("Imported sections");
                    }
                    if let Some(tabs) = settings.remove("tabs") {
                        for item in tabs.as_array().ok_or_else(invalid)? {
                            let mut tab = object(item)?;
                            tab.remove("_id");
                            let title = self.literal(&id, &mut tab, "tab_title")?;
                            let body = self.literal(&id, &mut tab, "tab_content")?;
                            let mut panel = self.node(if target == "tabs" {
                                "section"
                            } else {
                                "accordion"
                            })?;
                            panel.text = json!(title);
                            let mut body_node = self.node("body")?;
                            body_node.text = json!(self.rich_text(&id, &body)?);
                            panel.children.push(body_node);
                            n.children.push(panel);
                            if !tab.is_empty() {
                                self.loss(
                                    &id,
                                    "tab_settings_not_mapped",
                                    json!(tab.keys().collect::<Vec<_>>()),
                                );
                            }
                        }
                        self.loss(&id,"interactive_structure_normalized",json!("Native accessible disclosure/tab behavior; source animation and selected-state settings are not copied."));
                    }
                }
                _ => {
                    if ["image-gallery", "image-carousel"].contains(&widget.as_str()) {
                        let key = if widget == "image-gallery" {
                            "wp_gallery"
                        } else {
                            "carousel"
                        };
                        if let Some(images) = settings.remove(key) {
                            for image in images.as_array().ok_or_else(invalid)? {
                                if let Some(local) = self.media(&id, image.clone())? {
                                    let mut image = self.node("image")?;
                                    image.image = json!(local);
                                    n.children.push(image)
                                }
                            }
                        }
                        n.style.layout = "grid".into();
                        n.style.columns = 3;
                        n.style.mobile_columns = 1;
                        self.loss(&id,"gallery_behavior_normalized",json!("Native local image grid; source carousel/lightbox behavior is not reproduced."));
                    }
                    if widget == "spacer"
                        && let Some(space) = self.dimension(&id, &mut settings, "space", 96)?
                    {
                        n.style.margin_block = space as u8;
                        self.loss(
                            &id,
                            "spacer_normalized",
                            json!("Native block margin replaces the source fixed spacer height."),
                        );
                    }
                    if widget == "divider" {
                        n.style.border_width = 1;
                        n.style.border_color = "#cccccc".into();
                        self.loss(&id,"divider_normalized",json!("Native border section; source decorative shapes are not reproduced."));
                    }
                }
            }
            self.style(&id, &mut settings, &mut n)?;
            if !settings.is_empty() {
                self.loss(
                    &id,
                    "settings_not_mapped",
                    json!(settings.keys().collect::<Vec<_>>()),
                );
            }
            let descendants = self.walk(&children, depth + 1)?;
            if !descendants.is_empty() && ["heading", "body", "image", "link"].contains(&target) {
                let mut group = self.node("section")?;
                group.children.push(n);
                group.children.extend(descendants);
                n = group;
                self.loss(&id, "leaf_children_lifted", json!("Source widget children follow the mapped widget in a native section; they cannot disappear inside a leaf or become nested links."));
            } else {
                n.children.extend(descendants);
            }
            self.mappings
                .push(json!({"element":id,"widget":widget,"target":n.kind,"native_id":n.id}));
            nodes.push(n);
        }
        Ok(nodes)
    }
}
pub fn review(
    theme_id: &str,
    base: Package,
    version: i64,
    request: Request,
    registry: &Registry,
) -> Result<Review> {
    if !crate::schema::identifier(&request.component)
        || request.media.len() > 128
        || request.fonts.len() > 32
        || request.global_styles.len() > 128
        || !["component", "home", "header", "footer", "content"].contains(&request.target.as_str())
    {
        return Err(invalid());
    }
    for (key, id) in &request.media {
        if key.len() > 2000 || uuid::Uuid::parse_str(id).is_err() {
            return Err(invalid());
        }
    }
    for (key, alias) in &request.fonts {
        if key.len() > 300
            || !(["system", "serif", "mono"].contains(&alias.as_str())
                || alias
                    .strip_prefix("local:")
                    .is_some_and(|name| base.fonts.contains_key(name)))
        {
            return Err(invalid());
        }
    }
    for (reference, alias) in &request.global_styles {
        if reference.len() > 512 || !base.styles.contains_key(alias) {
            return Err(invalid());
        }
    }
    let source_bytes = serde_json::to_vec(&request.source).map_err(|_| invalid())?;
    if source_bytes.len() > MAX_BYTES {
        return Err(invalid());
    }
    let mut source = object(&request.source)?;
    if source.remove("version") != Some(json!("0.4")) {
        return Err(invalid());
    }
    let title = source
        .remove("title")
        .and_then(|v| v.as_str().map(str::to_owned))
        .ok_or_else(invalid)?;
    if title.len() > 300 {
        return Err(invalid());
    }
    let kind = source
        .remove("type")
        .and_then(|v| v.as_str().map(str::to_owned))
        .ok_or_else(invalid)?;
    if ![
        "page",
        "post",
        "section",
        "container",
        "header",
        "footer",
        "popup",
        "error-404",
    ]
    .contains(&kind.as_str())
    {
        return Err(invalid());
    }
    let page = object(&source.remove("page_settings").ok_or_else(invalid)?)?;
    let content = source
        .remove("content")
        .and_then(|v| v.as_array().cloned())
        .ok_or_else(invalid)?;
    let request_hash = auth::digest(&serde_json::to_vec(&request).map_err(|_| invalid())?);
    let mut c = Compiler {
        request: &request,
        ids: BTreeSet::new(),
        counter: 0,
        prefix: format!("import-{}", &request_hash[..12]),
        native_styles: base.styles.clone(),
        mappings: vec![],
        losses: vec![],
    };
    if ["header", "footer", "popup", "error-404"].contains(&kind.as_str()) {
        c.loss("export", "document_behavior_not_imported", json!("Source display conditions, popup triggers and template assignment are not imported; placement is chosen explicitly in the native draft."));
    }
    if !page.is_empty() {
        c.loss(
            "export",
            "page_settings_not_mapped",
            json!(page.keys().collect::<Vec<_>>()),
        );
    }
    if !source.is_empty() {
        c.loss(
            "export",
            "export_fields_not_mapped",
            json!(source.keys().collect::<Vec<_>>()),
        );
    }
    let mut root = c.node("section")?;
    root.children = c.walk(&content, 0)?;
    let mut package = base;
    if package.components.contains_key(&request.component) {
        return Err(Error::invalid(
            "Choose a new component name; imported designs do not overwrite reusable components.",
        ));
    }
    package.components.insert(
        request.component.clone(),
        Component {
            parameters: BTreeMap::new(),
            root,
        },
    );
    let mut placement = c.node("component")?;
    placement.component = request.component.clone();
    match request.target.as_str() {
        "component" => {}
        "home" => {
            package.templates.insert("home".into(), placement);
        }
        "header" => package.header = placement,
        "footer" => package.footer = placement,
        "content" => {
            if uuid::Uuid::parse_str(&request.content_id).is_err()
                || !registry.models.contains_key(&request.content_kind)
            {
                return Err(invalid());
            }
            let mut fallback = package
                .templates
                .get(&request.content_kind)
                .or_else(|| package.templates.get("content"))
                .ok_or_else(invalid)?
                .clone();
            fn reid(n: &mut Node, c: &mut Compiler<'_>) -> Result<()> {
                n.id = c.node("section")?.id;
                for child in &mut n.children {
                    reid(child, c)?;
                }
                Ok(())
            }
            reid(&mut fallback, &mut c)?;
            let condition = json!({"op":"eq","left":{"bind":"post.id"},"right":request.content_id});
            let mut imported = c.node("condition")?;
            imported.condition = condition.clone();
            imported.children.push(placement);
            let mut original = c.node("condition")?;
            original.condition = json!({"op":"not","condition":condition});
            original.children.push(fallback);
            let mut container = c.node("section")?;
            container.children = vec![imported, original];
            package
                .templates
                .insert(request.content_kind.clone(), container);
        }
        _ => return Err(invalid()),
    }
    package.validate(registry)?;
    if serde_json::to_vec(&package).map_err(|_| invalid())?.len() > 256 * 1024 {
        return Err(Error::invalid(
            "Imported theme exceeds the native 256-KiB package budget.",
        ));
    }
    let report = json!({"format":"wpalt-elementor-design-review-v1","source_sha256":auth::digest(&source_bytes),"source_version":"0.4","title":title,"type":kind,"base_version":version,"target":request.target,"component":request.component,"content_id":request.content_id,"content_kind":request.content_kind,"elements":c.ids.len(),"mappings":c.mappings,"losses":c.losses,"boundary":"Editable native private draft only; no source PHP/CSS/scripts/remote fetch. Review every loss and preview before explicit publication."});
    let fingerprint = auth::digest(
        &serde_json::to_vec(
            &json!({"theme":theme_id,"request":request,"package":package,"report":report}),
        )
        .map_err(|_| invalid())?,
    );
    Ok(Review {
        package,
        report,
        fingerprint,
    })
}

pub async fn validate_destination(app: &crate::App, review: &Review) -> Result<()> {
    if review.report["target"] == "content" {
        let actual: Option<String> = sqlx::query_scalar("SELECT kind FROM posts WHERE id=$1")
            .bind(review.report["content_id"].as_str().ok_or_else(invalid)?)
            .fetch_optional(&app.db.pool)
            .await?;
        if actual.as_deref() != review.report["content_kind"].as_str() {
            return Err(Error::invalid(
                "Choose an existing content destination with its current model.",
            ));
        }
    }
    Ok(())
}
