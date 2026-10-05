//! Bounded declarative composition shared by live pages and authenticated previews.
use crate::{
    App, content,
    error::{Error, Result},
    model::{Post, Settings},
    schema::Registry,
};
use maud::{DOCTYPE, Markup, html};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Any, QueryBuilder, Row};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub format: u32,
    pub name: String,
    pub tokens: BTreeMap<String, String>,
    #[serde(default)]
    pub components: BTreeMap<String, Component>,
    pub header: Node,
    pub footer: Node,
    pub templates: BTreeMap<String, Node>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    #[serde(default)]
    pub parameters: BTreeMap<String, String>,
    pub root: Node,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub kind: String,
    #[serde(default = "heading_level")]
    pub level: u8,
    #[serde(default)]
    pub text: Value,
    #[serde(default)]
    pub href: Value,
    #[serde(default)]
    pub image: Value,
    #[serde(default = "image_loading", skip_serializing_if = "lazy_loading")]
    pub loading: String,
    #[serde(default)]
    pub children: Vec<Node>,
    #[serde(default)]
    pub component: String,
    #[serde(default)]
    pub arguments: BTreeMap<String, Value>,
    #[serde(default)]
    pub source: String,
    #[serde(default = "limit")]
    pub limit: usize,
    #[serde(default)]
    pub condition: Value,
    #[serde(default)]
    pub style: Style,
}
fn lazy_loading(value: &str) -> bool {
    value == "lazy"
}
fn image_loading() -> String {
    "lazy".into()
}
fn heading_level() -> u8 {
    2
}
fn limit() -> usize {
    12
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Style {
    #[serde(default)]
    pub layout: String,
    #[serde(default)]
    pub columns: u8,
    #[serde(default)]
    pub mobile_columns: u8,
    #[serde(default)]
    pub gap: u8,
    #[serde(default)]
    pub padding: u8,
    #[serde(default)]
    pub width: u16,
    #[serde(default)]
    pub background: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub align: String,
}
fn color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}
impl Package {
    pub fn parse(raw: &str, registry: &Registry) -> Result<Self> {
        if raw.len() > 256 * 1024 {
            return Err(Error::invalid("Theme packages must be at most 256 KiB."));
        }
        let p: Self = serde_json::from_str(raw)
            .map_err(|_| Error::invalid("Invalid theme package structure."))?;
        p.validate(registry)?;
        Ok(p)
    }
    pub fn validate(&self, registry: &Registry) -> Result<()> {
        if self.format != 1
            || self.name.trim().is_empty()
            || self.name.len() > 100
            || self.components.len() > 32
            || self.templates.len() > 36
        {
            return Err(Error::invalid(
                "Unsupported theme format or package limits.",
            ));
        }
        for key in ["background", "panel", "text", "muted", "accent"] {
            if !self.tokens.get(key).is_some_and(|s| color(s)) {
                return Err(Error::invalid(
                    "Theme color tokens must be six-digit hexadecimal colors.",
                ));
            }
        }
        if self.tokens.len() != 6
            || !self
                .tokens
                .get("font")
                .is_some_and(|s| ["system", "serif", "mono"].contains(&s.as_str()))
        {
            return Err(Error::invalid(
                "Choose system, serif or mono font and the five color tokens.",
            ));
        }
        for key in ["home", "search", "content"] {
            if !self.templates.contains_key(key) {
                return Err(Error::invalid(
                    "Themes need home, search and content templates.",
                ));
            }
        }
        let mut ids = BTreeSet::new();
        let mut count = 0;
        for (name, c) in &self.components {
            if !crate::schema::identifier(name)
                || c.parameters.len() > 16
                || c.parameters.iter().any(|(n, t)| {
                    !crate::schema::identifier(n)
                        || !["string", "number", "boolean", "media", "relationship"]
                            .contains(&t.as_str())
                })
            {
                return Err(Error::invalid("Invalid component parameters."));
            }
            self.check_node(
                &c.root,
                registry,
                None,
                &c.parameters,
                &mut ids,
                &mut count,
                0,
            )?;
        }
        for node in [&self.header, &self.footer] {
            self.check_node(
                node,
                registry,
                None,
                &BTreeMap::new(),
                &mut ids,
                &mut count,
                0,
            )?
        }
        for (name, node) in &self.templates {
            if !["home", "search", "content"].contains(&name.as_str())
                && !registry.models.contains_key(name)
            {
                return Err(Error::invalid("Template targets an unknown content model."));
            }
            self.check_node(
                node,
                registry,
                registry.models.contains_key(name).then_some(name.as_str()),
                &BTreeMap::new(),
                &mut ids,
                &mut count,
                0,
            )?;
        }
        // Count expanded work, including component calls and collection multiplication.
        for node in self.templates.values().chain([&self.header, &self.footer]) {
            self.expansion(node, &mut Vec::new(), 0)?;
        }
        for c in self.components.values() {
            self.expansion(&c.root, &mut Vec::new(), 0)?;
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)] // Explicit validation context; no hidden global state.
    fn check_node(
        &self,
        n: &Node,
        r: &Registry,
        model: Option<&str>,
        params: &BTreeMap<String, String>,
        ids: &mut BTreeSet<String>,
        count: &mut usize,
        depth: usize,
    ) -> Result<()> {
        *count += 1;
        if n.level < 1
            || n.level > 6
            || depth > 12
            || *count > 512
            || !crate::schema::identifier(&n.id)
            || !ids.insert(n.id.clone())
        {
            return Err(Error::invalid(
                "Theme nodes need unique safe identifiers, at most 512 nodes and depth 12.",
            ));
        }
        if ![
            "section",
            "grid",
            "row",
            "heading",
            "text",
            "body",
            "image",
            "form",
            "link",
            "navigation",
            "component",
            "collection",
            "repeater",
            "condition",
            "accordion",
            "tabs",
            "gallery",
            "carousel",
        ]
        .contains(&n.kind.as_str())
        {
            return Err(Error::invalid("Unknown composition node."));
        }
        if n.kind == "form"
            && n.text
                .as_str()
                .is_none_or(|id| uuid::Uuid::parse_str(id).is_err())
        {
            return Err(Error::invalid(
                "A form block needs a literal published form ID.",
            ));
        }
        if !["lazy", "eager"].contains(&n.loading.as_str())
            || (n.kind != "image" && n.loading != "lazy")
        {
            return Err(Error::invalid("Image loading must be lazy or eager."));
        }
        let s = &n.style;
        if !["", "grid", "row", "stack"].contains(&s.layout.as_str())
            || s.columns > 6
            || s.mobile_columns > 3
            || s.gap > 64
            || s.padding > 96
            || s.width > 1600
            || !["", "left", "center", "right"].contains(&s.align.as_str())
            || (!s.background.is_empty() && !color(&s.background))
            || (!s.color.is_empty() && !color(&s.color))
        {
            return Err(Error::invalid("Invalid responsive style values."));
        }
        if n.limit == 0 || n.limit > 50 || n.arguments.len() > 16 || n.children.len() > 50 {
            return Err(Error::invalid("Composition item limit must be 1–50."));
        }
        for v in [&n.text, &n.href, &n.image] {
            binding(v, r, model, params)?;
        }
        check_condition(&n.condition, r, model, params, 0, &mut 64)?;
        if !n.image.is_null() && !n.image.is_string() && !n.image.is_object() {
            return Err(Error::invalid(
                "Media values must be a UUID or a typed binding.",
            ));
        }
        if !n.href.is_null() && !n.href.is_string() && !n.href.is_object() {
            return Err(Error::invalid(
                "Link destinations must be text URLs or bindings.",
            ));
        }
        if n.image.is_object()
            && let Some(kind) = binding_kind(&n.image, r, model, params)
        {
            let expected = if n.kind == "image" {
                "media"
            } else {
                "gallery"
            };
            if kind != expected {
                return Err(Error::invalid(
                    "Image/gallery binding has the wrong declared field type.",
                ));
            }
        }
        if n.kind == "repeater"
            && let Some(kind) = binding_kind(&json!({"bind":n.source}), r, model, params)
            && !["repeater", "flexible", "gallery"].contains(&kind.as_str())
        {
            return Err(Error::invalid("Repeaters must bind a declared list field."));
        }

        if let Some(id) = n.image.as_str()
            && !id.is_empty()
            && uuid::Uuid::parse_str(id).is_err()
        {
            return Err(Error::invalid(
                "Literal media must reference an uploaded media UUID.",
            ));
        }
        if let Some(url) = n.href.as_str()
            && !url.is_empty()
            && !content::safe_nav_url(url)
        {
            return Err(Error::invalid("Unsafe theme link URL."));
        }
        if n.kind == "component" {
            let c = self
                .components
                .get(&n.component)
                .ok_or(Error::invalid("Unknown reusable component."))?;
            if n.arguments.len() != c.parameters.len()
                || n.arguments.keys().any(|k| !c.parameters.contains_key(k))
            {
                return Err(Error::invalid("Supply each declared component parameter."));
            }
            for (key, v) in &n.arguments {
                binding(v, r, model, params)?;
                if v.get("bind").is_none() && !parameter_value(&c.parameters[key], v) {
                    return Err(Error::invalid(
                        "Component literal parameter has the wrong declared type.",
                    ));
                }
            }
        } else if !n.component.is_empty() || !n.arguments.is_empty() {
            return Err(Error::invalid(
                "Component arguments belong on component nodes.",
            ));
        }
        if n.kind == "collection" && n.source != "listing" && !r.models.contains_key(&n.source) {
            return Err(Error::invalid(
                "Collection source must be listing or a declared model.",
            ));
        }
        if n.kind == "repeater" {
            binding(&json!({"bind":n.source}), r, model, params)?;
        }
        for child in &n.children {
            self.check_node(
                child,
                r,
                if n.kind == "collection" && n.source != "listing" {
                    Some(&n.source)
                } else {
                    model
                },
                params,
                ids,
                count,
                depth + 1,
            )?
        }
        Ok(())
    }
    fn expansion(&self, n: &Node, stack: &mut Vec<String>, depth: usize) -> Result<usize> {
        if depth > 16 {
            return Err(Error::invalid("Expanded component nesting is too deep."));
        }
        let mut work = 1usize;
        if n.kind == "component" {
            if stack.contains(&n.component) {
                return Err(Error::invalid("Reusable components cannot form cycles."));
            }
            stack.push(n.component.clone());
            work += self.expansion(&self.components[&n.component].root, stack, depth + 1)?;
            stack.pop();
        }
        for c in &n.children {
            work = work.saturating_add(self.expansion(c, stack, depth + 1)?)
        }
        if ["collection", "repeater", "gallery"].contains(&n.kind.as_str()) {
            work = work.saturating_mul(n.limit)
        }
        if work > 5000 {
            return Err(Error::invalid(
                "Expanded theme exceeds the 5,000-node render budget.",
            ));
        }
        Ok(work)
    }
    /// Historical revisions preserve their original field dependencies. Validate
    /// execution/style structure without requiring obsolete fields to stay installed.
    pub fn parse_historical(raw: &str, registry: &Registry) -> Result<Self> {
        if raw.len() > 256 * 1024 {
            return Err(Error::invalid("Historical package exceeds 256 KiB."));
        }
        let original: Self =
            serde_json::from_str(raw).map_err(|_| Error::invalid("Invalid historical package."))?;
        let mut check = original.clone();
        let mut context = registry.clone();
        fn neutralize(n: &mut Node) {
            for value in [&mut n.text, &mut n.href, &mut n.image] {
                if value.is_object() {
                    *value = Value::Null;
                }
            }
            n.condition = Value::Null;
            if n.kind == "repeater" {
                n.source = "site.title".into();
            }
            if n.kind == "collection" {
                n.source = "listing".into();
            }
            for value in n.arguments.values_mut() {
                *value = Value::Null;
            }
            for child in &mut n.children {
                neutralize(child)
            }
        }
        for (name, n) in &mut check.templates {
            if !["home", "search", "content"].contains(&name.as_str()) {
                if !crate::schema::identifier(name) {
                    return Err(Error::invalid("Invalid historical model name."));
                }
                context
                    .models
                    .entry(name.clone())
                    .or_insert(crate::schema::Model::initial(name));
            }
            neutralize(n);
        }
        neutralize(&mut check.header);
        neutralize(&mut check.footer);
        for c in check.components.values_mut() {
            neutralize(&mut c.root);
        }
        check.validate(&context)?;
        Ok(original)
    }
    pub fn css(&self) -> String {
        self.css_scope(None)
    }
    /// A deterministic render-reachable stylesheet. Preserve conditional branches
    /// and responsive rules; discard unrelated templates/components, not guessed
    /// browser states. The stylesheet is served locally under strict CSP.
    pub fn css_for(&self, template: &str) -> Result<String> {
        if !self.templates.contains_key(template) {
            return Err(Error::not_found());
        }
        Ok(self.css_scope(Some(template)))
    }
    fn css_scope(&self, template: Option<&str>) -> String {
        let font = match self.tokens["font"].as_str() {
            "serif" => "Georgia,serif",
            "mono" => "ui-monospace,monospace",
            _ => "system-ui,sans-serif",
        };
        let mut out = format!(
            "body{{background:{};color:{};font-family:{font}}}a{{color:{}}}.theme-shell{{max-width:1200px;margin:auto;padding:24px}}.theme-node img{{max-width:100%;height:auto}}.theme-grid{{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:24px}}.theme-row{{display:flex;flex-wrap:wrap;gap:24px}}.theme-card{{padding:24px;background:{};border-radius:12px}}@media(max-width:700px){{.theme-grid{{grid-template-columns:1fr}}}}",
            self.tokens["background"],
            self.tokens["text"],
            self.tokens["accent"],
            self.tokens["panel"]
        );
        fn add(n: &Node, out: &mut String) {
            let s = &n.style;
            out.push_str(&format!(".n-{}{{", n.id));
            match s.layout.as_str() {
                "grid" => out.push_str("display:grid;"),
                "row" => out.push_str("display:flex;flex-wrap:wrap;"),
                _ => {}
            }
            if s.columns > 0 {
                out.push_str(&format!(
                    "grid-template-columns:repeat({},minmax(0,1fr));",
                    s.columns
                ))
            }
            if s.gap > 0 {
                out.push_str(&format!("gap:{}px;", s.gap))
            }
            if s.padding > 0 {
                out.push_str(&format!("padding:{}px;", s.padding))
            }
            if s.width > 0 {
                out.push_str(&format!("max-width:{}px;margin-inline:auto;", s.width))
            }
            if !s.background.is_empty() {
                out.push_str(&format!("background:{};", s.background))
            }
            if !s.color.is_empty() {
                out.push_str(&format!("color:{};", s.color))
            }
            if !s.align.is_empty() {
                out.push_str(&format!("text-align:{};", s.align))
            }
            out.push('}');
            if s.mobile_columns > 0 {
                out.push_str(&format!("@media(max-width:700px){{.n-{}{{grid-template-columns:repeat({},minmax(0,1fr))}}}}",n.id,s.mobile_columns))
            }
            for c in &n.children {
                add(c, out)
            }
        }
        fn reachable<'a>(
            p: &'a Package,
            n: &'a Node,
            seen: &mut BTreeSet<String>,
            roots: &mut Vec<&'a Node>,
        ) {
            if n.kind == "component" && seen.insert(n.component.clone()) {
                let root = &p.components[&n.component].root;
                roots.push(root);
                reachable(p, root, seen, roots);
            }
            for child in &n.children {
                reachable(p, child, seen, roots);
            }
        }
        let mut roots = vec![&self.header, &self.footer];
        if let Some(template) = template {
            roots.push(&self.templates[template]);
            let mut seen = BTreeSet::new();
            let initial = roots.clone();
            for root in initial {
                reachable(self, root, &mut seen, &mut roots);
            }
        } else {
            roots.extend(self.templates.values());
            roots.extend(self.components.values().map(|c| &c.root));
        }
        for n in roots {
            add(n, &mut out);
        }
        out
    }
}
fn field_kind(
    r: &Registry,
    fields: &BTreeMap<String, crate::schema::Field>,
    parts: &[&str],
) -> Option<String> {
    let (first, tail) = parts.split_first()?;
    let field = fields.get(*first)?;
    if tail.is_empty() {
        return Some(field.kind.clone());
    }
    match field.kind.as_str() {
        "object" | "group" | "repeater" => field_kind(r, r.child_fields(field).ok()?, tail),
        "relationship" => {
            if tail.len() == 1 && ["title", "body", "url", "kind", "id"].contains(&tail[0]) {
                Some("string".into())
            } else if tail.first() == Some(&"fields") {
                field_kind(r, &r.fields_for(&field.target).ok()?, &tail[1..])
            } else {
                None
            }
        }
        _ => None,
    }
}
fn binding_kind(
    v: &Value,
    r: &Registry,
    model: Option<&str>,
    params: &BTreeMap<String, String>,
) -> Option<String> {
    if let Some(path) = v.get("bind").and_then(Value::as_str) {
        let parts: Vec<_> = path.split('.').collect();
        match parts.as_slice() {
            ["site", ..]
            | ["post", "title" | "body" | "url" | "kind" | "id"]
            | ["item", "title" | "body" | "url" | "kind" | "id" | "type"] => Some("string".into()),
            ["params", key] => params.get(*key).cloned(),
            ["post", "fields", ..] => field_kind(
                r,
                &model
                    .and_then(|m| r.fields_for(m).ok())
                    .unwrap_or_else(|| r.common.fields.clone()),
                &parts[2..],
            ),
            ["options", ..] => field_kind(r, &r.common.options, &parts[1..]),
            ["item", "fields", ..] => model
                .and_then(|m| r.fields_for(m).ok())
                .and_then(|fields| field_kind(r, &fields, &parts[2..])),
            _ => None,
        }
    } else if v.is_string() {
        Some("string".into())
    } else if v.is_number() {
        Some("number".into())
    } else if v.is_boolean() {
        Some("boolean".into())
    } else {
        None
    }
}
fn nested_field_path(
    r: &Registry,
    fields: &BTreeMap<String, crate::schema::Field>,
    parts: &[&str],
) -> bool {
    fields.values().any(|f| match f.kind.as_str() {
        "object" | "group" | "repeater" => r.child_fields(f).is_ok_and(|fields| {
            field_path(r, fields, parts, 0) || nested_field_path(r, fields, parts)
        }),
        _ => false,
    })
}
fn field_path(
    r: &Registry,
    fields: &BTreeMap<String, crate::schema::Field>,
    parts: &[&str],
    depth: usize,
) -> bool {
    if depth > 8 {
        return false;
    }
    let Some((first, tail)) = parts.split_first() else {
        return false;
    };
    let Some(f) = fields.get(*first) else {
        return false;
    };
    if tail.is_empty() {
        return true;
    }
    match f.kind.as_str() {
        "object" | "group" | "repeater" => r
            .child_fields(f)
            .is_ok_and(|fields| field_path(r, fields, tail, depth + 1)),
        "relationship" => {
            if tail.len() == 1 && ["title", "body", "url", "kind", "id"].contains(&tail[0]) {
                true
            } else {
                tail.first() == Some(&"fields")
                    && r.fields_for(&f.target)
                        .is_ok_and(|fields| field_path(r, &fields, &tail[1..], depth + 1))
            }
        }
        _ => false,
    }
}
fn check_condition(
    v: &Value,
    r: &Registry,
    model: Option<&str>,
    params: &BTreeMap<String, String>,
    depth: usize,
    budget: &mut usize,
) -> Result<()> {
    if depth > 8 || *budget == 0 {
        return Err(Error::invalid("Condition nesting/work exceeds its budget."));
    }
    *budget -= 1;
    if let Some(op) = v.get("op").and_then(Value::as_str) {
        let o = v.as_object().ok_or(Error::invalid("Invalid condition."))?;
        match op {
            "eq" | "ne" | "gt" | "lt" => {
                if o.len() != 3 || !o.contains_key("left") || !o.contains_key("right") {
                    return Err(Error::invalid(
                        "Comparison conditions need left and right values.",
                    ));
                }
                binding(&v["left"], r, model, params)?;
                binding(&v["right"], r, model, params)?;
            }
            "all" | "any" => {
                if o.len() != 2 {
                    return Err(Error::invalid("Invalid condition list."));
                }
                let values = v["conditions"]
                    .as_array()
                    .ok_or(Error::invalid("Condition lists require an array."))?;
                if values.is_empty() || values.len() > 16 {
                    return Err(Error::invalid("Use 1–16 nested conditions."));
                }
                for child in values {
                    check_condition(child, r, model, params, depth + 1, budget)?;
                }
            }
            "not" => {
                if o.len() != 2 || !o.contains_key("condition") {
                    return Err(Error::invalid("Not requires one condition."));
                }
                check_condition(&v["condition"], r, model, params, depth + 1, budget)?;
            }
            _ => return Err(Error::invalid("Unsupported condition operator.")),
        }
        Ok(())
    } else {
        binding(v, r, model, params)
    }
}
fn parameter_value(kind: &str, v: &Value) -> bool {
    v.is_null()
        || match kind {
            "string" => v.is_string(),
            "number" => v.is_number(),
            "boolean" => v.is_boolean(),
            "media" | "relationship" => {
                v.as_str().is_some_and(|s| uuid::Uuid::parse_str(s).is_ok())
            }
            _ => false,
        }
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}
fn evaluate_condition(ctx: &Context, v: &Value, item: &Value, params: &Value) -> bool {
    match v.get("op").and_then(Value::as_str) {
        Some("eq") => {
            ctx.resolve(&v["left"], item, params) == ctx.resolve(&v["right"], item, params)
        }
        Some("ne") => {
            ctx.resolve(&v["left"], item, params) != ctx.resolve(&v["right"], item, params)
        }
        Some(op @ ("gt" | "lt")) => {
            let left = ctx.resolve(&v["left"], item, params).as_f64();
            let right = ctx.resolve(&v["right"], item, params).as_f64();
            match (left, right) {
                (Some(a), Some(b)) => {
                    if op == "gt" {
                        a > b
                    } else {
                        a < b
                    }
                }
                _ => false,
            }
        }
        Some("all") => v["conditions"].as_array().is_some_and(|values| {
            values
                .iter()
                .all(|c| evaluate_condition(ctx, c, item, params))
        }),
        Some("any") => v["conditions"].as_array().is_some_and(|values| {
            values
                .iter()
                .any(|c| evaluate_condition(ctx, c, item, params))
        }),
        Some("not") => !evaluate_condition(ctx, &v["condition"], item, params),
        _ => truthy(&ctx.resolve(v, item, params)),
    }
}
fn binding(
    v: &Value,
    r: &Registry,
    model: Option<&str>,
    params: &BTreeMap<String, String>,
) -> Result<()> {
    if v.is_null() || v.is_boolean() || v.is_number() {
        return Ok(());
    }
    if let Some(s) = v.as_str() {
        return if s.len() <= 8000 {
            Ok(())
        } else {
            Err(Error::invalid("Theme text is too long."))
        };
    }
    let o = v
        .as_object()
        .ok_or(Error::invalid("Use a scalar or a binding object."))?;
    if o.keys().any(|k| k != "bind" && k != "fallback")
        || o.get("fallback")
            .is_some_and(|v| !(v.is_null() || v.is_string() || v.is_boolean() || v.is_number()))
    {
        return Err(Error::invalid("Invalid binding shape."));
    }
    let path = o
        .get("bind")
        .and_then(Value::as_str)
        .ok_or(Error::invalid("Bindings require a path."))?;
    let parts: Vec<_> = path.split('.').collect();
    if parts.len() > 8 || parts.iter().any(|s| !crate::schema::identifier(s)) {
        return Err(Error::invalid("Invalid binding path."));
    }
    let valid = match parts.as_slice() {
        ["site", key] => ["title", "description", "role", "region"].contains(key),
        ["post", key] => ["title", "body", "url", "kind", "id"].contains(key),
        ["params", key, tail @ ..] => params.get(*key).is_some_and(|kind| {
            tail.is_empty()
                || (kind == "relationship"
                    && ((tail.len() == 1
                        && ["title", "body", "url", "kind", "id"].contains(&tail[0]))
                        || (tail.first() == Some(&"fields")
                            && r.models.keys().any(|model| {
                                r.fields_for(model)
                                    .is_ok_and(|fields| field_path(r, &fields, &tail[1..], 0))
                            }))))
        }),
        ["options", ..] => field_path(r, &r.common.options, &parts[1..], 0),
        ["post", "fields", ..] => model.map_or_else(
            || field_path(r, &r.common.fields, &parts[2..], 0),
            |m| {
                r.fields_for(m)
                    .is_ok_and(|fields| field_path(r, &fields, &parts[2..], 0))
            },
        ),
        ["item", key, tail @ ..] => {
            if ["title", "body", "url", "kind", "id", "type"].contains(key) {
                tail.is_empty()
            } else if *key == "values" {
                r.common
                    .groups
                    .values()
                    .any(|fields| field_path(r, fields, tail, 0))
            } else if *key == "fields" {
                r.models.keys().any(|m| {
                    r.fields_for(m)
                        .is_ok_and(|fields| field_path(r, &fields, tail, 0))
                })
            } else {
                r.common
                    .groups
                    .values()
                    .any(|fields| field_path(r, fields, &parts[1..], 0))
                    || r.models
                        .values()
                        .any(|m| nested_field_path(r, &m.fields, &parts[1..]))
                    || nested_field_path(r, &r.common.fields, &parts[1..])
            }
        }
        _ => false,
    };
    if !valid {
        return Err(Error::invalid(
            "Binding refers to an unknown field or context.",
        ));
    }
    Ok(())
}
#[derive(Clone)]
pub struct Stored {
    pub id: String,
    pub version: i64,
    pub published_version: i64,
    pub package: Package,
}
pub async fn load(app: &App, id: &str, draft: bool) -> Result<Stored> {
    let row = sqlx::query("SELECT draft,live,version,published_version FROM themes WHERE id=$1")
        .bind(id)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
    if !draft && row.get::<i64, _>("published_version") == 0 {
        return Err(Error::invalid(
            "Publish the theme before activating or exporting its live version.",
        ));
    }
    let raw: String = row.get(if draft { "draft" } else { "live" });
    Ok(Stored {
        id: id.into(),
        version: row.get("version"),
        published_version: row.get("published_version"),
        package: Package::parse(&raw, &Registry::load(app).await?)?,
    })
}
async fn validate_literal_references(app: &App, package: &Package) -> Result<()> {
    fn gather(
        p: &Package,
        n: &Node,
        media: &mut BTreeSet<String>,
        posts: &mut BTreeSet<String>,
        forms: &mut BTreeSet<String>,
    ) {
        if n.kind == "form" {
            forms.insert(n.text.as_str().unwrap_or("").to_owned());
        }
        if let Some(id) = n.image.as_str()
            && uuid::Uuid::parse_str(id).is_ok()
        {
            media.insert(id.into());
        }
        if n.kind == "component" {
            for (key, value) in &n.arguments {
                if let Some(id) = value.as_str() {
                    match p.components[&n.component].parameters[key].as_str() {
                        "media" => {
                            media.insert(id.into());
                        }
                        "relationship" => {
                            posts.insert(id.into());
                        }
                        _ => {}
                    }
                }
            }
        }
        for c in &n.children {
            gather(p, c, media, posts, forms)
        }
    }
    let mut media = BTreeSet::new();
    let mut posts = BTreeSet::new();
    let mut forms = BTreeSet::new();
    for n in package
        .templates
        .values()
        .chain([&package.header, &package.footer])
        .chain(package.components.values().map(|c| &c.root))
    {
        gather(package, n, &mut media, &mut posts, &mut forms)
    }
    if media.len() + posts.len() + forms.len() > 128 {
        return Err(Error::invalid(
            "Use at most 128 literal media/relationship references in a theme.",
        ));
    }
    for (table, ids) in [
        ("media", media),
        ("posts", posts),
        ("business_forms", forms),
    ] {
        if ids.is_empty() {
            continue;
        }
        let mut q = QueryBuilder::<Any>::new(format!("SELECT id FROM {table} WHERE id IN ("));
        let mut list = q.separated(",");
        for id in &ids {
            list.push_bind(id);
        }
        list.push_unseparated(")");
        if table == "business_forms" {
            q.push(" AND published_version>0");
            if !app.config.business_enabled {
                return Err(Error::invalid(
                    "Enable the business module before publishing an embedded form.",
                ));
            }
        }
        if app.db.fetch_builder(&mut q).await?.len() != ids.len() {
            return Err(Error::invalid(
                "Theme references missing media or content; install dependencies or replace their UUIDs.",
            ));
        }
    }
    Ok(())
}
pub async fn save(
    app: &App,
    id: &str,
    package: Package,
    version: i64,
    publish: bool,
) -> Result<i64> {
    let _guard = app.mutation().await;
    if !crate::schema::identifier(id) {
        return Err(Error::invalid("Invalid theme identifier."));
    }
    let registry = Registry::load(app).await?;
    package.validate(&registry)?;
    validate_literal_references(app, &package).await?;
    let raw = serde_json::to_string(&package).map_err(|_| Error::invalid("Invalid package."))?;
    if raw.len() > 256 * 1024 {
        return Err(Error::invalid("Theme package exceeds 256 KiB."));
    }
    let mut tx = app.db.pool.begin().await?;
    if version == 0 {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM themes")
            .fetch_one(&mut *tx)
            .await?;
        if count >= 32 {
            return Err(Error::invalid("Install at most 32 themes."));
        }
        // Imported themes must explicitly publish before activation; initial live is empty.
        sqlx::query("INSERT INTO themes(id,name,draft,live,version,published_version,updated_at) VALUES($1,$2,$3,$4,1,$5,$6)").bind(id).bind(&package.name).bind(&raw).bind(if publish{raw.as_str()}else{""}).bind(if publish{1i64}else{0}).bind(crate::now()).execute(&mut *tx).await?;
    } else {
        let sql = if publish {
            "UPDATE themes SET name=$1,draft=$2,live=$2,version=version+1,published_version=version+1,updated_at=$3 WHERE id=$4 AND version=$5"
        } else {
            "UPDATE themes SET name=$1,draft=$2,version=version+1,updated_at=$3 WHERE id=$4 AND version=$5"
        };
        let result = sqlx::query(sql)
            .bind(&package.name)
            .bind(&raw)
            .bind(crate::now())
            .bind(id)
            .bind(version)
            .execute(&mut *tx)
            .await?;
        if result.rows_affected() != 1 {
            return Err(Error::conflict());
        }
    }
    sqlx::query("INSERT INTO theme_revisions(id,theme_id,version,package,published,created_at) VALUES($1,$2,$3,$4,$5,$6)").bind(uuid::Uuid::new_v4().to_string()).bind(id).bind(version+1).bind(raw).bind(i64::from(publish)).bind(crate::now()).execute(&mut *tx).await?;
    // Keep 50 working revisions plus at least two published revisions for in-flight CSS.
    sqlx::query("DELETE FROM theme_revisions WHERE theme_id=$1 AND version NOT IN (SELECT version FROM theme_revisions WHERE theme_id=$1 ORDER BY version DESC LIMIT 50) AND version NOT IN (SELECT version FROM theme_revisions WHERE theme_id=$1 AND published=1 ORDER BY version DESC LIMIT 2)").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    app.themes.lock().await.retain(|(key, _), _| key != id);
    tracing::info!(event="theme_saved",theme_id=%id,version=version+1,published=publish);
    Ok(version + 1)
}
pub async fn activate(app: &App, id: &str) -> Result<()> {
    let _guard = app.mutation().await;
    let stored = load(app, id, false).await?;
    if stored.published_version < 1 {
        return Err(Error::invalid("Publish the theme before activating it."));
    }
    sqlx::query("UPDATE settings SET theme=$1 WHERE id=1")
        .bind(id)
        .execute(&app.db.pool)
        .await?;
    tracing::info!(event="theme_activated",theme_id=%id);
    Ok(())
}
// Immutable compiled packages only; never cache rendered data or unpublished drafts.
pub async fn published(app: &App, id: &str) -> Result<Stored> {
    for _ in 0..3 {
        let row = sqlx::query("SELECT published_version,version FROM themes WHERE id=$1")
            .bind(id)
            .fetch_optional(&app.db.pool)
            .await?
            .ok_or_else(Error::not_found)?;
        let published_version: i64 = row.get("published_version");
        let key = (id.to_string(), published_version);
        let cached = app.themes.lock().await.get(&key).cloned();
        let package = if let Some(package) = cached {
            package
        } else {
            let raw:Option<String>=sqlx::query_scalar("SELECT package FROM theme_revisions WHERE theme_id=$1 AND version=$2 AND published=1").bind(id).bind(published_version).fetch_optional(&app.db.pool).await?;
            let Some(raw) = raw else { continue };
            let package = match Package::parse(&raw, &Registry::load(app).await?) {
                Ok(package) => package,
                Err(error) => {
                    let current: i64 =
                        sqlx::query_scalar("SELECT published_version FROM themes WHERE id=$1")
                            .bind(id)
                            .fetch_one(&app.db.pool)
                            .await?;
                    if current != published_version {
                        continue;
                    }
                    return Err(error);
                }
            };
            let mut cache = app.themes.lock().await;
            if cache.len() >= 32 {
                cache.clear();
            }
            cache.insert(key, package.clone());
            package
        };
        return Ok(Stored {
            id: id.into(),
            version: row.get("version"),
            published_version,
            package,
        });
    }
    Err(Error::conflict())
}
pub struct Context {
    pub root: Value,
    pub collections: BTreeMap<String, Vec<Value>>,
    pub relations: BTreeMap<String, Value>,
    pub media: BTreeMap<String, String>,
    pub media_dimensions: BTreeMap<String, (u32, u32)>,
    pub queries: usize,
    reference_ids: BTreeSet<String>,
}
/// Stream selected published columns and stop before aggregating excessive data.
async fn public_values(
    app: &App,
    builder: &mut QueryBuilder<'_, Any>,
    remaining: &mut usize,
    discovery: &crate::discovery::Definition,
) -> Result<Vec<Value>> {
    use futures_util::TryStreamExt;
    use sqlx::Execute;
    let mut query = builder.build();
    let sql = crate::db::Db::numbered(query.sql());
    let args = query
        .take_arguments()
        .map_err(sqlx::Error::Encode)?
        .unwrap_or_default();
    let mut rows = sqlx::query_with(&sql, args).fetch(&app.db.pool);
    let mut values = Vec::new();
    while let Some(row) = rows.try_next().await? {
        let value = json!({"id":row.get::<String,_>("id"),"kind":row.get::<String,_>("kind"),"title":row.get::<String,_>("published_title"),"url":discovery.path(&row.get::<String,_>("published_locale"), &row.get::<String,_>("published_slug")),"body":row.get::<String,_>("published_body"),"document":row.get::<String,_>("published_document"),"fields":serde_json::from_str::<Value>(&row.get::<String,_>("published_fields")).map_err(|_|Error::invalid("Invalid published fields."))?});
        *remaining = remaining
            .checked_sub(value.to_string().len())
            .ok_or(Error::invalid("Render data exceeds the 2-MiB budget."))?;
        values.push(value);
    }
    Ok(values)
}
fn post_value(p: &Post, draft: bool, discovery: &crate::discovery::Definition) -> Value {
    json!({"id":p.id,"kind":p.kind,"title":if draft{&p.title}else{&p.published_title},"body":if draft{&p.body}else{&p.published_body},"document":if draft{&p.document}else{&p.published_document},"url":discovery.path(if draft{&p.locale}else{&p.published_locale},if draft{&p.slug}else{&p.published_slug}),"fields":serde_json::from_str::<Value>(if draft{&p.fields}else{&p.published_fields}).unwrap_or(json!({}))})
}
#[allow(clippy::too_many_arguments)] // Request template narrows dependency loading.
pub async fn context(
    app: &App,
    settings: &Settings,
    package: &Package,
    post: Option<&Post>,
    listing: Vec<Value>,
    draft: bool,
    template: &str,
    language: Option<&str>,
) -> Result<Context> {
    let (discovery, _) = crate::discovery::load(app).await?;
    context_with_discovery(
        app, settings, package, post, listing, draft, template, language, &discovery,
    )
    .await
}
#[allow(clippy::too_many_arguments)] // Request template and already loaded discovery definition.
pub async fn context_with_discovery(
    app: &App,
    settings: &Settings,
    package: &Package,
    post: Option<&Post>,
    listing: Vec<Value>,
    draft: bool,
    template: &str,
    language: Option<&str>,
    discovery: &crate::discovery::Definition,
) -> Result<Context> {
    let locale = language.unwrap_or_else(|| {
        post.map(|p| {
            if draft {
                p.locale.as_str()
            } else {
                p.published_locale.as_str()
            }
        })
        .unwrap_or(&discovery.default_language)
    });
    let language_config = discovery.language(locale)?;
    let options: String = sqlx::query_scalar(if draft {
        "SELECT draft_options FROM site_design WHERE id=1"
    } else {
        "SELECT live_options FROM site_design WHERE id=1"
    })
    .fetch_one(&app.db.pool)
    .await?;
    let mut ctx = Context {
        root: json!({"site":{"title":settings.title,"description":settings.description,"role":"anonymous","region":"unknown"},"navigation":serde_json::from_str::<Value>(&settings.navigation).unwrap_or(json!([])),"post":post.map(|p|post_value(p,draft,discovery)).unwrap_or(json!({})),"options":serde_json::from_str::<Value>(&options).map_err(|_|Error::invalid("Invalid shared options."))?}),
        collections: BTreeMap::from([("listing".into(), listing)]),
        relations: BTreeMap::new(),
        media: BTreeMap::new(),
        media_dimensions: BTreeMap::new(),
        queries: 2,
        reference_ids: BTreeSet::new(),
    };
    ctx.root["_asset_scope"] = app.config.assets.scoped_theme_css.into();
    ctx.root["_asset_preload"] = app.config.assets.preload_theme_css.into();
    ctx.root["language"] = locale.into();
    ctx.root["direction"] = language_config.direction.clone().into();
    if !language_config.navigation.is_empty() {
        ctx.root["navigation"] = serde_json::to_value(&language_config.navigation).unwrap();
    }
    fn dependencies<'a>(
        package: &'a Package,
        n: &'a Node,
        out: &mut Vec<&'a Node>,
        seen: &mut BTreeSet<String>,
    ) {
        out.push(n);
        for child in &n.children {
            dependencies(package, child, out, seen)
        }
        if n.kind == "component" && seen.insert(n.component.clone()) {
            dependencies(package, &package.components[&n.component].root, out, seen)
        }
    }
    let mut nodes = Vec::new();
    let mut seen = BTreeSet::new();
    for root in [
        &package.header,
        &package.footer,
        package
            .templates
            .get(template)
            .unwrap_or(&package.templates["content"]),
    ] {
        dependencies(package, root, &mut nodes, &mut seen)
    }
    let mut sets = BTreeMap::<String, usize>::new();
    for n in &nodes {
        if n.kind == "collection" && n.source != "listing" {
            let value = sets.entry(n.source.clone()).or_default();
            *value = (*value).max(n.limit)
        }
    }
    let initial = ctx.root.to_string().len()
        + serde_json::to_string(&ctx.collections)
            .map_err(|_| Error::invalid("Invalid listing data."))?
            .len();
    let mut remaining = (2 * 1024 * 1024usize)
        .checked_sub(initial)
        .ok_or(Error::invalid("Render data exceeds the 2-MiB budget."))?;
    // Select published columns only; stream under one request-wide memory budget.
    for (kind, limit) in sets {
        let mut query = QueryBuilder::<Any>::new(
            "SELECT id,kind,published_slug,published_locale,published_title,published_body,published_document,published_fields FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id) AND kind=",
        );
        query
            .push_bind(&kind)
            .push(" AND published_locale=")
            .push_bind(locale)
            .push(" ORDER BY published_at DESC,id DESC LIMIT ")
            .push_bind(limit as i64);
        let values = public_values(app, &mut query, &mut remaining, discovery).await?;
        ctx.queries += 1;
        ctx.collections.insert(kind, values);
    }
    // Typed references gathered in batches. Public resolution *always* uses published snapshots.
    let registry = Registry::load(app).await?;
    ctx.queries += 1;
    let mut refs = BTreeMap::new();
    let mut media = Vec::new();
    let mut literal_relations = Vec::new();
    for n in nodes {
        if n.kind == "component" {
            for (key, value) in &n.arguments {
                if let Some(id) = value.as_str()
                    && uuid::Uuid::parse_str(id).is_ok()
                {
                    match package.components[&n.component].parameters[key].as_str() {
                        "media" => media.push(id.into()),
                        "relationship" => literal_relations.push(id.to_owned()),
                        _ => {}
                    }
                }
            }
        }
        if let Some(id) = n.image.as_str()
            && uuid::Uuid::parse_str(id).is_ok()
        {
            media.push(id.to_owned())
        }
    }
    registry.references(
        &registry.common.options,
        &ctx.root["options"],
        &mut refs,
        &mut media,
    )?;
    if let Some(p) = post {
        registry.references(
            &registry.fields_for(&p.kind)?,
            &ctx.root["post"]["fields"],
            &mut refs,
            &mut media,
        )?;
    }
    for rows in ctx.collections.values() {
        for row in rows {
            registry.references(
                &registry.fields_for(row["kind"].as_str().unwrap_or("post"))?,
                &row["fields"],
                &mut refs,
                &mut media,
            )?
        }
    }
    for id in literal_relations {
        refs.entry(id).or_default();
    }
    let mut seen = BTreeSet::new();
    for _ in 0..4 {
        let ids: Vec<_> = refs
            .keys()
            .filter(|id| !seen.contains(*id))
            .cloned()
            .collect();
        if ids.is_empty() {
            break;
        }
        if seen.len() + ids.len() > 128 {
            return Err(Error::invalid(
                "Related content exceeds the 128-record resolution budget.",
            ));
        }
        seen.extend(ids.iter().cloned());
        let mut q = QueryBuilder::<Any>::new(
            "SELECT id,kind,published_slug,published_locale,published_title,published_body,published_document,published_fields FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id) AND id IN (",
        );
        let mut list = q.separated(",");
        for id in &ids {
            list.push_bind(id);
        }
        list.push_unseparated(")");
        let rows = public_values(app, &mut q, &mut remaining, discovery).await?;
        ctx.queries += 1;
        for value in rows {
            let kind = value["kind"]
                .as_str()
                .ok_or(Error::invalid("Invalid related model."))?;
            let id = value["id"]
                .as_str()
                .ok_or(Error::invalid("Invalid related identifier."))?
                .to_owned();
            if refs.get(&id).is_some_and(String::is_empty) {
                refs.insert(id.clone(), kind.into());
            }
            registry.references(
                &registry.fields_for(kind)?,
                &value["fields"],
                &mut refs,
                &mut media,
            )?;
            let id = value["id"]
                .as_str()
                .ok_or(Error::invalid("Invalid related identifier."))?
                .to_owned();
            ctx.relations.insert(id, value);
        }
    }

    // Layout metadata is optional, bounded independently of typed references.
    // Resolve only local image UUIDs; external imported URLs are never fetched.
    let mut doc_media = BTreeSet::new();
    for value in std::iter::once(&ctx.root["post"])
        .chain(ctx.collections.values().flatten())
        .chain(ctx.relations.values())
    {
        if let Some(raw) = value["document"].as_str()
            && raw.contains("/media/")
            && let Ok(doc) = crate::document::Document::parse(raw)
        {
            doc_media.extend(doc.image_ids().into_iter().take(128));
            if doc_media.len() >= 128 {
                break;
            }
        }
    }
    let known: BTreeSet<_> = media.iter().cloned().collect();
    media.extend(
        doc_media
            .into_iter()
            .filter(|id| !known.contains(id))
            .take(128usize.saturating_sub(known.len())),
    );
    ctx.reference_ids.extend(refs.keys().cloned());
    ctx.reference_ids.extend(media.iter().cloned());
    media.sort();
    media.dedup();
    if media.len() > 128 {
        return Err(Error::invalid("Media resolution exceeds 128 items."));
    }
    if !media.is_empty() {
        let mut q = QueryBuilder::<Any>::new(if draft {
            "SELECT id,alt,filename,sha256 FROM media WHERE mime<>'video/mp4' AND id IN ("
        } else {
            "SELECT id,alt,filename,sha256 FROM media WHERE mime<>'video/mp4' AND visibility='public' AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='media' AND mr.resource_id=media.id) AND id IN ("
        });
        let mut list = q.separated(",");
        for id in &media {
            list.push_bind(id);
        }
        list.push_unseparated(")");
        let mut sources = Vec::new();
        for row in app.db.fetch_builder(&mut q).await? {
            sources.push((row.get("id"), row.get("filename"), row.get("sha256")));
            ctx.media.insert(row.get("id"), row.get("alt"));
        }
        ctx.media_dimensions = crate::operations::media::dimensions(app, sources).await;
        ctx.queries += 1;
    }
    let bytes = serde_json::to_vec(&ctx.root)
        .map_err(|_| Error::invalid("Invalid render data."))?
        .len()
        + serde_json::to_vec(&ctx.collections)
            .map_err(|_| Error::invalid("Invalid collection data."))?
            .len()
        + serde_json::to_vec(&ctx.relations)
            .map_err(|_| Error::invalid("Invalid related data."))?
            .len();
    if bytes > 2 * 1024 * 1024 {
        return Err(Error::invalid("Render data exceeds the 2-MiB budget."));
    }
    Ok(ctx)
}
impl Context {
    fn resolve(&self, v: &Value, item: &Value, params: &Value) -> Value {
        let Some(path) = v.get("bind").and_then(Value::as_str) else {
            return v.clone();
        };
        let mut parts = path.split('.');
        let root = parts.next().unwrap_or("");
        let mut value = match root {
            "item" => item,
            "params" => params,
            _ => &self.root[root],
        };
        for key in parts {
            if let Some(id) = value.as_str()
                && let Some(related) = self.relations.get(id)
            {
                value = related
            }
            value = value.get(key).unwrap_or(&Value::Null);
        }
        // Missing/private relationship IDs never become rendered content or conditions.
        if value
            .as_str()
            .is_some_and(|s| self.reference_ids.contains(s))
            && !self.relations.contains_key(value.as_str().unwrap())
            && !self.media.contains_key(value.as_str().unwrap())
        {
            return v.get("fallback").cloned().unwrap_or(Value::Null);
        }
        if value.is_null() {
            v.get("fallback").cloned().unwrap_or(Value::Null)
        } else {
            fn public_projection(ctx: &Context, value: &Value) -> Value {
                match value {
                    Value::String(id)
                        if ctx.reference_ids.contains(id)
                            && !ctx.relations.contains_key(id)
                            && !ctx.media.contains_key(id) =>
                    {
                        Value::Null
                    }
                    Value::Array(values) => {
                        Value::Array(values.iter().map(|v| public_projection(ctx, v)).collect())
                    }
                    Value::Object(values) => Value::Object(
                        values
                            .iter()
                            .map(|(key, v)| (key.clone(), public_projection(ctx, v)))
                            .collect(),
                    ),
                    _ => value.clone(),
                }
            }
            public_projection(self, value)
        }
    }
}
fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        _ => v.to_string(),
    }
}
fn append_markup(target: &mut Markup, addition: Markup) -> Result<()> {
    if target.0.len() + addition.0.len() > 1024 * 1024 {
        return Err(Error::invalid(
            "A rendered section exceeds the 1-MiB output budget.",
        ));
    }
    target.0.push_str(&addition.0);
    Ok(())
}
#[allow(clippy::too_many_arguments)] // One recursive render context, including a shared work counter.
fn render_node(
    p: &Package,
    n: &Node,
    ctx: &Context,
    item: &Value,
    params: &Value,
    budget: &mut usize,
    post: Option<&Post>,
    draft: bool,
) -> Result<Markup> {
    *budget += 1;
    if *budget > 5000 {
        return Err(Error::invalid(
            "Render work exceeded the 5,000-node budget.",
        ));
    }
    let class = format!(
        "theme-node n-{} {}",
        n.id,
        match n.kind.as_str() {
            "grid" => "theme-grid",
            "row" => "theme-row",
            _ => "",
        }
    );
    let value = ctx.resolve(&n.text, item, params);
    let label = text(&value);
    let mut children = Markup::default();
    if !["component", "collection", "repeater", "condition", "tabs"].contains(&n.kind.as_str()) {
        for c in &n.children {
            append_markup(
                &mut children,
                render_node(p, c, ctx, item, params, budget, post, draft)?,
            )?
        }
    }
    Ok(match n.kind.as_str() {
        "form" => {
            if draft {
                html! {p {a href=(format!("/forms/{label}")) {"Open published form"}}}
            } else {
                html! {(maud::PreEscaped(crate::document::form_embed(&label,"Embedded form")))}
            }
        }
        "heading" => match n.level {
            1 => html! {h1 class=(class){(label)}},
            3 => html! {h3 class=(class){(label)}},
            4 => html! {h4 class=(class){(label)}},
            5 => html! {h5 class=(class){(label)}},
            6 => html! {h6 class=(class){(label)}},
            _ => html! {h2 class=(class){(label)}},
        },
        "text" => html! {p class=(class){(label)}},
        "link" => {
            let href = text(&ctx.resolve(&n.href, item, params));
            if content::safe_nav_url(&href) {
                html! {a class=(class) href=(href){(label)(children)}}
            } else {
                html! {span class=(class){(label)(children)}}
            }
        }
        "image" => {
            let id = text(&ctx.resolve(&n.image, item, params));
            if let Some(alt) = ctx.media.get(&id) {
                html! {img class=(class) src=(format!("/media/{id}")) alt=(if label.is_empty(){alt.as_str()}else{&label}) width=[ctx.media_dimensions.get(&id).map(|d|d.0)] height=[ctx.media_dimensions.get(&id).map(|d|d.1)] loading=(&n.loading) fetchpriority=(if n.loading=="eager" {"high"} else {"auto"}) decoding="async";}
            } else {
                Markup::default()
            }
        }
        "body" => {
            let canonical = n
                .text
                .get("bind")
                .and_then(Value::as_str)
                .and_then(|path| path.strip_suffix(".body"))
                .map(|prefix| {
                    ctx.resolve(&json!({"bind":format!("{prefix}.document")}), item, params)
                });
            if let Some(doc) = canonical.as_ref().and_then(Value::as_str) {
                html! {div class=(class){(maud::PreEscaped(crate::document::Document::parse(doc).map(|mut d| {d.apply_dimensions(&ctx.media_dimensions);if draft {d.preview_html()} else {d.html()}}).unwrap_or_default()))}}
            } else if !n.text.is_null() {
                html! {div class=(class){(maud::PreEscaped(content::markdown(&label)))}}
            } else if let Some(doc) = item.get("document").and_then(Value::as_str) {
                html! {div class=(class){(maud::PreEscaped(crate::document::Document::parse(doc).map(|mut d| {d.apply_dimensions(&ctx.media_dimensions);if draft {d.preview_html()} else {d.html()}}).unwrap_or_default()))}}
            } else {
                post.map(|p| {
                    crate::view::public_body_with_dimensions(p, draft, &ctx.media_dimensions)
                })
                .unwrap_or_default()
            }
        }
        "navigation" => {
            let nav: Vec<crate::model::NavItem> =
                serde_json::from_value(ctx.root["navigation"].clone()).unwrap_or_default();
            html! {header class="site-header"{a class="site-brand" href="/"{(text(&ctx.root["site"]["title"]))}nav aria-label="Website"{@for link in nav{a href=(link.url){(link.label)}}a href="/search"{"Search"}}}}
        }
        "component" => {
            let c = &p.components[&n.component];
            let args: serde_json::Map<_, _> = n
                .arguments
                .iter()
                .map(|(k, v)| (k.clone(), ctx.resolve(v, item, params)))
                .collect();
            if args
                .iter()
                .any(|(key, v)| !parameter_value(&c.parameters[key], v))
            {
                return Err(Error::invalid(
                    "Resolved component parameter has the wrong type.",
                ));
            }
            render_node(
                p,
                &c.root,
                ctx,
                item,
                &Value::Object(args),
                budget,
                post,
                draft,
            )?
        }
        "collection" => {
            let rows = ctx.collections.get(&n.source);
            let mut body = Markup::default();
            if let Some(rows) = rows {
                for row in rows.iter().take(n.limit) {
                    if n.children.is_empty() {
                        append_markup(
                            &mut body,
                            html! {article class="theme-card"{h2{a href=(text(&row["url"])){(text(&row["title"]))}}p{(crate::view::excerpt(&text(&row["body"])))}a href=(text(&row["url"])){"Read more →"}}},
                        )?
                    } else {
                        for child in &n.children {
                            append_markup(
                                &mut body,
                                render_node(p, child, ctx, row, params, budget, post, draft)?,
                            )?
                        }
                    }
                }
            }
            html! {div class=(format!("{class} theme-grid")){(body)}}
        }
        "repeater" => {
            let rows = ctx.resolve(&json!({"bind":n.source}), item, params);
            let mut body = Markup::default();
            if let Some(rows) = rows.as_array() {
                for row in rows.iter().take(n.limit) {
                    for child in &n.children {
                        append_markup(
                            &mut body,
                            render_node(p, child, ctx, row, params, budget, post, draft)?,
                        )?
                    }
                }
            }
            html! {div class=(class){(body)}}
        }
        "condition" => {
            if evaluate_condition(ctx, &n.condition, item, params) {
                let mut body = Markup::default();
                for c in &n.children {
                    append_markup(
                        &mut body,
                        render_node(p, c, ctx, item, params, budget, post, draft)?,
                    )?
                }
                body
            } else {
                Markup::default()
            }
        }
        "accordion" => html! {details class=(class){summary{(label)}(children)}},
        "tabs" => {
            let instance = *budget;
            let mut panels = Vec::new();
            for (i, c) in n.children.iter().enumerate() {
                let title = text(&ctx.resolve(&c.text, item, params));
                let body = render_node(p, c, ctx, item, params, budget, post, draft)?;
                panels.push((i, title, body));
            }
            html! {section class=(class) data-tabs aria-label=(label){div data-tab-list aria-label=(label){@for (i,title,_) in &panels{button type="button" id=(format!("tab-{instance}-{i}")) data-tab aria-controls=(format!("panel-{instance}-{i}")){(if title.is_empty(){format!("Section {}",i+1)}else{title.clone()})}}}@for(i,_,body)in panels{section id=(format!("panel-{instance}-{i}")) data-tab-panel aria-labelledby=(format!("tab-{instance}-{i}")) tabindex="0"{(body)}}}}
        }
        "gallery" | "carousel" => {
            let items = ctx.resolve(&n.image, item, params);
            html! {div class=(format!("{class} {}",if n.kind=="carousel"{"theme-carousel"}else{"theme-grid"})){@if let Some(ids)=items.as_array(){@for id in ids.iter().take(n.limit){@if let Some(alt)=id.as_str().and_then(|id|ctx.media.get(id)){img src=(format!("/media/{}",id.as_str().unwrap())) alt=(alt) width=[id.as_str().and_then(|id|ctx.media_dimensions.get(id)).map(|d|d.0)] height=[id.as_str().and_then(|id|ctx.media_dimensions.get(id)).map(|d|d.1)] loading="lazy" decoding="async";}}}}}
        }
        _ => html! {section class=(class){(children)}},
    })
}
pub fn document(
    stored: &Stored,
    settings: &Settings,
    ctx: &Context,
    post: Option<&Post>,
    draft: bool,
    template: &str,
    extra: Markup,
) -> Result<String> {
    let p = &stored.package;
    let root = p.templates.get(template).unwrap_or(&p.templates["content"]);
    let mut budget = 0;
    let item = Value::Null;
    let params = json!({});
    let header = render_node(p, &p.header, ctx, &item, &params, &mut budget, post, draft)?;
    let body = render_node(p, root, ctx, &item, &params, &mut budget, post, draft)?;
    let footer = render_node(p, &p.footer, ctx, &item, &params, &mut budget, post, draft)?;
    let style = if draft {
        format!("/admin/design/{}/style.css?v={}", stored.id, stored.version)
    } else {
        format!(
            "/themes/{}/{}/style.css",
            stored.id, stored.published_version
        )
    };
    let style = if ctx.root["_asset_scope"].as_bool().unwrap_or(true) {
        let selected = if p.templates.contains_key(template) {
            template
        } else {
            "content"
        };
        format!(
            "{style}{}template={selected}",
            if draft { "&" } else { "?" }
        )
    } else {
        style
    };
    let preload = ctx.root["_asset_preload"].as_bool().unwrap_or(true);
    let priority_image = post
        .and_then(|post| {
            crate::document::Document::parse(if draft {
                &post.document
            } else {
                &post.published_document
            })
            .ok()
        })
        .and_then(|doc| doc.priority_image())
        .filter(|src| body.0.contains(&format!("src=\"{src}\"")))
        .or_else(|| {
            body.0.split("<img ").skip(1).find_map(|part| {
                let tag = part.split_once('>')?.0;
                if !tag.contains("loading=\"eager\"") {
                    return None;
                }
                let id = tag.split_once("src=\"/media/")?.1.split_once('"')?.0;
                uuid::Uuid::parse_str(id)
                    .ok()
                    .map(|_| format!("/media/{id}"))
            })
        });

    tracing::debug!(event="theme_render",theme_id=%stored.id,nodes=budget,resolution_queries=ctx.queries,preview=draft);
    fn has_tabs(p: &Package, n: &Node) -> bool {
        n.kind == "tabs"
            || n.children.iter().any(|c| has_tabs(p, c))
            || (n.kind == "component" && has_tabs(p, &p.components[&n.component].root))
    }
    let scripts = [&p.header, &p.footer, root]
        .into_iter()
        .any(|n| has_tabs(p, n));
    let output=html!{(DOCTYPE)html lang=(ctx.root["language"].as_str().unwrap_or("en")) dir=(ctx.root["direction"].as_str().unwrap_or("ltr")){head{meta charset="utf-8";meta name="viewport" content="width=device-width,initial-scale=1";@if ctx.root["_discovery"].is_object(){(crate::discovery::head(&ctx.root["_discovery"],draft))}@else{title{(post.map(|p|if draft{p.title.as_str()}else{p.published_title.as_str()}).unwrap_or(&settings.title))}meta name="description" content=(settings.description);@if draft{meta name="robots" content="noindex,nofollow";}}link rel="stylesheet" href="/assets/app.css";@if let Some(src)=priority_image{link rel="preload" href=(src) as="image";}@if preload{link rel="preload" href=(&style) as="style";}link rel="stylesheet" href=(style);@if !draft&&scripts{script defer src="/assets/widgets.js"{}}}body class=(format!("theme-site {}",settings.theme)){a class="skip" href="#main"{"Skip to content"}(header)main id="main" class="theme-shell"{(body)(extra)}(footer)(crate::business::engagement::markup(settings,draft))(crate::discovery::business_footer(&ctx.root["_discovery"]))footer class="site-footer"{a href="/login"{"Manage site"}}}}}.into_string();
    if output.len() > 2 * 1024 * 1024 {
        return Err(Error::invalid("Rendered document exceeds 2 MiB."));
    }
    Ok(output)
}
pub async fn preflight(app: &App, registry: &Registry) -> Result<()> {
    let rows = sqlx::query("SELECT draft,live FROM themes")
        .fetch_all(&app.db.pool)
        .await?;
    for row in rows {
        for column in ["draft", "live"] {
            let raw: String = row.get(column);
            if !raw.is_empty() {
                Package::parse(&raw, registry)?;
            }
        }
    }
    Ok(())
}
