//! Versioned canonical authoring tree. Markdown is an explicit import/export projection.
use crate::error::{Error, Result};
use maud::{PreEscaped, html};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub version: u8,
    pub root: Node,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub attrs: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<Node>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<Mark>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mark {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub attrs: Value,
}
fn node(kind: &str, content: Vec<Node>) -> Node {
    Node {
        kind: kind.into(),
        attrs: Value::Null,
        content,
        text: None,
        marks: vec![],
    }
}
fn text(value: &str, marks: &[Mark]) -> Node {
    Node {
        text: Some(value.into()),
        marks: marks.to_vec(),
        ..node("text", vec![])
    }
}
pub fn empty() -> String {
    serde_json::to_string(&Document {
        version: 1,
        root: node("doc", vec![node("paragraph", vec![])]),
    })
    .unwrap()
}
impl Document {
    pub fn parse(source: &str) -> Result<Self> {
        let start = std::time::Instant::now();
        if source.len() > 2 * 1024 * 1024 {
            return Err(Error::invalid("Document exceeds two MiB."));
        }
        let doc: Self = serde_json::from_str(source)
            .map_err(|_| Error::invalid("Invalid structured document."))?;
        let mut count = 0;
        let mut bytes = 0;
        if doc.version != 1 || doc.root.kind != "doc" {
            return Err(Error::invalid("Unsupported document version or root."));
        }
        validate(&doc.root, 0, &mut count, &mut bytes)?;
        tracing::debug!(
            event = "document_validated",
            nodes = count,
            text_bytes = bytes,
            json_bytes = source.len(),
            elapsed_us = start.elapsed().as_micros() as u64
        );
        Ok(doc)
    }
    pub fn encode(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
    /// At most one explicit local priority image. Never preload imported external URLs.
    pub fn priority_image(&self) -> Option<String> {
        fn visit(n: &Node) -> Option<String> {
            if n.kind == "image" && n.attrs["loading"].as_str() == Some("eager") {
                let src = n.attrs["src"].as_str()?;
                if src
                    .strip_prefix("/media/")
                    .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
                {
                    return Some(src.into());
                }
            }
            n.content.iter().find_map(visit)
        }
        visit(&self.root)
    }
    pub fn html(&self) -> String {
        render(&self.root)
    }
    pub fn image_ids(&self) -> std::collections::BTreeSet<String> {
        fn visit(n: &Node, ids: &mut std::collections::BTreeSet<String>) {
            if n.kind == "image"
                && let Some(id) = n.attrs["src"]
                    .as_str()
                    .and_then(|v| v.strip_prefix("/media/"))
                && uuid::Uuid::parse_str(id).is_ok()
            {
                ids.insert(id.into());
            }
            for child in &n.content {
                visit(child, ids);
            }
        }
        let mut ids = std::collections::BTreeSet::new();
        visit(&self.root, &mut ids);
        ids
    }
    /// Enrich a transient render tree only. Historical publications remain unchanged.
    pub fn apply_dimensions(
        &mut self,
        dimensions: &std::collections::BTreeMap<String, (u32, u32)>,
    ) {
        fn visit(n: &mut Node, dimensions: &std::collections::BTreeMap<String, (u32, u32)>) {
            if n.kind == "image"
                && !n.attrs["width"].is_number()
                && let Some((width, height)) = n.attrs["src"]
                    .as_str()
                    .and_then(|v| v.strip_prefix("/media/"))
                    .and_then(|id| dimensions.get(id))
                && (1..=4096).contains(width)
                && (1..=4096).contains(height)
            {
                n.attrs["width"] = (*width).into();
                n.attrs["height"] = (*height).into();
            }
            for child in &mut n.content {
                visit(child, dimensions);
            }
        }
        visit(&mut self.root, dimensions);
    }
    /// Draft inspection must not create live response collectors.
    pub fn preview_html(&self) -> String {
        fn project(n: &mut Node) {
            if n.kind == "form" {
                *n = node(
                    "paragraph",
                    vec![text("Published form (inactive in draft preview)", &[])],
                );
            } else {
                for child in &mut n.content {
                    project(child);
                }
            }
        }
        let mut root = self.root.clone();
        project(&mut root);
        render(&root)
    }
    pub fn form_ids(&self) -> std::collections::BTreeSet<String> {
        fn visit(n: &Node, ids: &mut std::collections::BTreeSet<String>) {
            if n.kind == "form" {
                ids.insert(n.attrs["id"].as_str().unwrap_or("").to_owned());
            }
            for child in &n.content {
                visit(child, ids);
            }
        }
        let mut ids = std::collections::BTreeSet::new();
        visit(&self.root, &mut ids);
        ids
    }
    pub fn mail_html(&self, base_url: &str) -> String {
        fn project(n: &mut Node, base: &str) {
            if n.kind == "form" {
                let href = format!(
                    "{}/forms/{}",
                    base.trim_end_matches('/'),
                    n.attrs["id"].as_str().unwrap_or("")
                );
                let title = n.attrs["title"].as_str().unwrap_or("Form").to_owned();
                let mut t = text(&title, &[]);
                t.marks.push(Mark {
                    kind: "link".into(),
                    attrs: json!({"href":href}),
                });
                *n = node("paragraph", vec![t]);
            } else {
                for child in &mut n.content {
                    project(child, base);
                }
            }
        }
        let mut root = self.root.clone();
        project(&mut root, base_url);
        render(&root)
    }
    pub fn markdown(&self) -> String {
        projection(&self.root)
    }
}
fn invalid() -> Error {
    Error::invalid("Document contains unsupported structure, attributes, links or exceeds limits.")
}
fn block(k: &str) -> bool {
    [
        "paragraph",
        "heading",
        "blockquote",
        "bullet_list",
        "ordered_list",
        "code_block",
        "callout",
        "table",
        "horizontal_rule",
        "form",
    ]
    .contains(&k)
}
fn validate(n: &Node, depth: usize, count: &mut usize, bytes: &mut usize) -> Result<()> {
    *count += 1;
    *bytes += n.text.as_ref().map_or(0, String::len);
    if depth > 16 || *count > 10000 || *bytes > 512 * 1024 {
        return Err(invalid());
    }
    let allowed: &[&str] = match n.kind.as_str() {
        "heading" => &["level"],
        "form" => &["id", "title"],
        "ordered_list" => &["order"],
        "code_block" => &["params"],
        "image" => &["src", "alt", "title", "width", "height", "loading"],
        "table_cell" | "table_header" => &["colspan", "rowspan", "colwidth"],
        _ => &[],
    };
    if !n.attrs.is_null()
        && (!n.attrs.is_object()
            || n.attrs
                .as_object()
                .unwrap()
                .keys()
                .any(|k| !allowed.contains(&k.as_str())))
    {
        return Err(invalid());
    }
    if n.kind == "heading" && !matches!(n.attrs["level"].as_u64(), Some(1..=6)) {
        return Err(invalid());
    }
    if n.kind == "ordered_list" && !matches!(n.attrs["order"].as_u64(), Some(1..=1000000)) {
        return Err(invalid());
    }
    if n.kind == "code_block"
        && n.attrs
            .get("params")
            .is_some_and(|v| !v.is_null() && v.as_str().is_none_or(|s| s.len() > 100))
    {
        return Err(invalid());
    }
    if n.kind == "form"
        && (n.attrs["id"]
            .as_str()
            .is_none_or(|id| uuid::Uuid::parse_str(id).is_err())
            || n.attrs["title"]
                .as_str()
                .is_none_or(|title| title.is_empty() || title.len() > 160))
    {
        return Err(invalid());
    }
    if n.kind == "image" {
        let src = n.attrs["src"].as_str().ok_or_else(invalid)?;
        // Existing HTTP(S) images may be imported, but are never fetched by this server.
        if !crate::content::safe_nav_url(src) || src.len() > 2000 {
            return Err(invalid());
        }
        let width = n.attrs.get("width").filter(|v| !v.is_null());
        let height = n.attrs.get("height").filter(|v| !v.is_null());
        if width.is_some() != height.is_some()
            || width.is_some_and(|v| !matches!(v.as_u64(), Some(1..=4096)))
            || height.is_some_and(|v| !matches!(v.as_u64(), Some(1..=4096)))
            || n.attrs
                .get("loading")
                .is_some_and(|v| !v.is_null() && !matches!(v.as_str(), Some("lazy" | "eager")))
        {
            return Err(invalid());
        }
        for k in ["alt", "title"] {
            if n.attrs
                .get(k)
                .is_some_and(|v| !v.is_null() && v.as_str().is_none_or(|s| s.len() > 2000))
            {
                return Err(invalid());
            }
        }
    }
    if ["table_cell", "table_header"].contains(&n.kind.as_str())
        && (!matches!(n.attrs["colspan"].as_u64(), Some(1))
            || !matches!(n.attrs["rowspan"].as_u64(), Some(1))
            || !n.attrs["colwidth"].is_null())
    {
        return Err(invalid());
    }
    let children = &n.content;
    let valid = match n.kind.as_str() {
        "doc" => depth == 0 && !children.is_empty() && children.iter().all(|c| block(&c.kind)),
        "paragraph" | "heading" => children
            .iter()
            .all(|c| ["text", "image", "hard_break"].contains(&c.kind.as_str())),
        "code_block" => children
            .iter()
            .all(|c| c.kind == "text" && c.marks.is_empty()),
        "blockquote" | "callout" => !children.is_empty() && children.iter().all(|c| block(&c.kind)),
        "bullet_list" | "ordered_list" => {
            !children.is_empty() && children.iter().all(|c| c.kind == "list_item")
        }
        "list_item" => {
            children.first().is_some_and(|c| c.kind == "paragraph")
                && children.iter().all(|c| block(&c.kind))
        }
        "table" => {
            !children.is_empty()
                && children.len() <= 100
                && children.iter().all(|c| c.kind == "table_row")
                && children
                    .iter()
                    .all(|r| r.content.len() == children[0].content.len())
        }
        "table_row" => {
            !children.is_empty()
                && children.len() <= 20
                && children
                    .iter()
                    .all(|c| ["table_cell", "table_header"].contains(&c.kind.as_str()))
        }
        "table_cell" | "table_header" => {
            !children.is_empty() && children.iter().all(|c| block(&c.kind) && c.kind != "table")
        }
        "text" => children.is_empty() && n.text.as_ref().is_some_and(|t| !t.is_empty()),
        "form" | "image" | "hard_break" | "horizontal_rule" => children.is_empty(),
        _ => false,
    };
    if !valid
        || (n.kind != "text" && (n.text.is_some() || !n.marks.is_empty()))
        || n.marks.len() > 5
    {
        return Err(invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    for m in &n.marks {
        if !seen.insert(&m.kind) {
            return Err(invalid());
        }
        if m.kind == "link" {
            if !m.attrs.is_object()
                || m.attrs
                    .as_object()
                    .unwrap()
                    .keys()
                    .any(|k| !["href", "title"].contains(&k.as_str()))
                || m.attrs["href"]
                    .as_str()
                    .is_none_or(|s| s.len() > 2000 || !crate::content::safe_nav_url(s))
                || m.attrs
                    .get("title")
                    .is_some_and(|v| !v.is_null() && v.as_str().is_none_or(|s| s.len() > 1000))
            {
                return Err(invalid());
            }
        } else if !["strong", "em", "code", "strike"].contains(&m.kind.as_str())
            || (!m.attrs.is_null() && m.attrs != json!({}))
        {
            return Err(invalid());
        }
    }
    for child in children {
        validate(child, depth + 1, count, bytes)?;
    }
    Ok(())
}
fn render_image(n: &Node) -> String {
    let eager = n.attrs["loading"].as_str() == Some("eager");
    html! {img src=(n.attrs["src"].as_str().unwrap_or("")) alt=(n.attrs["alt"].as_str().unwrap_or(""))
        width=[n.attrs["width"].as_u64()] height=[n.attrs["height"].as_u64()]
        loading=(if eager {"eager"} else {"lazy"}) fetchpriority=(if eager {"high"} else {"auto"}) decoding="async";}.into_string()
}
fn render(n: &Node) -> String {
    let inner = n.content.iter().map(render).collect::<String>();
    let inner = PreEscaped(inner);
    match n.kind.as_str() {
        "form" => form_embed(
            n.attrs["id"].as_str().unwrap_or(""),
            n.attrs["title"].as_str().unwrap_or("Form"),
        ),
        "text" => {
            let mut v = html! {(n.text.as_deref().unwrap_or(""))}.into_string();
            for m in n.marks.iter().rev() {
                let x = PreEscaped(v);
                v=match m.kind.as_str(){"strong"=>html!{strong{(x)}},"em"=>html!{em{(x)}},"strike"=>html!{s{(x)}},"code"=>html!{code{(x)}},"link"=>html!{a href=(m.attrs["href"].as_str().unwrap_or("")) rel="noopener noreferrer"{(x)}},_=>html!{(x)}}.into_string();
            }
            v
        }
        "paragraph" => html! {p{(inner)}}.into_string(),
        "heading" => {
            let level = n.attrs["level"].as_u64().unwrap_or(2);
            format!("<h{level}>{}</h{level}>", inner.0)
        }
        "blockquote" => html! {blockquote{(inner)}}.into_string(),
        "callout" => html! {aside class="callout"{(inner)}}.into_string(),
        "bullet_list" => html! {ul{(inner)}}.into_string(),
        "ordered_list" => {
            html! {ol start=(n.attrs["order"].as_u64().unwrap_or(1)){(inner)}}.into_string()
        }
        "list_item" => html! {li{(inner)}}.into_string(),
        "code_block" => html! {pre{code{(inner)}}}.into_string(),
        "image" => render_image(n),
        "table" => html! {div class="document-table"{table{tbody{(inner)}}}}.into_string(),
        "table_row" => html! {tr{(inner)}}.into_string(),
        "table_cell" => html! {td{(inner)}}.into_string(),
        "table_header" => html! {th scope="col"{(inner)}}.into_string(),
        "hard_break" => "<br>".into(),
        "horizontal_rule" => "<hr>".into(),
        _ => inner.0,
    }
}
fn projection(n: &Node) -> String {
    let inner = n.content.iter().map(projection).collect::<String>();
    match n.kind.as_str() {
        "text" => {
            let mut v = n.text.clone().unwrap_or_default();
            for m in n.marks.iter().rev() {
                v = match m.kind.as_str() {
                    "strong" => format!("**{v}**"),
                    "em" => format!("*{v}*"),
                    "code" => format!("`{v}`"),
                    "strike" => format!("~~{v}~~"),
                    "link" => format!("[{v}]({})", m.attrs["href"].as_str().unwrap_or("")),
                    _ => v,
                };
            }
            v
        }
        "form" => format!(
            "[{}](/forms/{})\n\n",
            n.attrs["title"].as_str().unwrap_or("Form"),
            n.attrs["id"].as_str().unwrap_or("")
        ),
        "paragraph" => format!("{inner}\n\n"),
        "heading" => format!(
            "{} {inner}\n\n",
            "#".repeat(n.attrs["level"].as_u64().unwrap_or(2) as usize)
        ),
        "blockquote" | "callout" => format!("> {}\n\n", inner.trim().replace('\n', "\n> ")),
        "code_block" => format!(
            "````{}\n{inner}\n````\n\n",
            n.attrs["params"].as_str().unwrap_or("")
        ),
        "list_item" => format!("- {}\n", inner.trim()),
        "bullet_list" | "ordered_list" => format!("{inner}\n"),
        "image" => format!(
            "![{}]({})",
            n.attrs["alt"].as_str().unwrap_or(""),
            n.attrs["src"].as_str().unwrap_or("")
        ),
        "hard_break" => "  \n".into(),
        "horizontal_rule" => "---\n\n".into(),
        "table_cell" | "table_header" => format!("{} | ", inner.trim()),
        "table_row" => format!("| {inner}\n"),
        _ => inner,
    }
}
/// Preserve unsupported raw HTML and extensions as readable source, never execute it.
pub fn import(markdown: &str, blocks: &str) -> Result<Document> {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
    let mut stack = vec![node("doc", vec![])];
    let mut marks = vec![];
    let mut image: Option<Node> = None;
    let mut table_head = false;
    for event in Parser::new_ext(
        markdown,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH,
    ) {
        match event {
            Event::Start(tag) => {
                let next = match tag {
                    Tag::Paragraph => Some(node("paragraph", vec![])),
                    Tag::Heading { level, .. } => {
                        let mut n = node("heading", vec![]);
                        n.attrs = json!({"level":level as u8});
                        Some(n)
                    }
                    Tag::BlockQuote(_) => Some(node("blockquote", vec![])),
                    Tag::CodeBlock(kind) => {
                        let mut n = node("code_block", vec![]);
                        n.attrs = json!({"params":match kind{pulldown_cmark::CodeBlockKind::Fenced(s)=>s.to_string(),_=>String::new()}});
                        Some(n)
                    }
                    Tag::List(order) => {
                        let mut n = node(
                            if order.is_some() {
                                "ordered_list"
                            } else {
                                "bullet_list"
                            },
                            vec![],
                        );
                        if let Some(order) = order {
                            n.attrs = json!({"order":order});
                        }
                        Some(n)
                    }
                    Tag::Item => Some(node("list_item", vec![])),
                    Tag::Emphasis | Tag::Strong | Tag::Strikethrough => {
                        marks.push(Mark {
                            kind: match tag {
                                Tag::Emphasis => "em",
                                Tag::Strong => "strong",
                                _ => "strike",
                            }
                            .into(),
                            attrs: Value::Null,
                        });
                        None
                    }
                    Tag::Link {
                        dest_url, title, ..
                    } => {
                        if crate::content::safe_nav_url(&dest_url) {
                            marks.push(Mark{kind:"link".into(),attrs:json!({"href":dest_url.to_string(),"title":title.to_string()})});
                        } else {
                            marks.push(Mark {
                                kind: "em".into(),
                                attrs: Value::Null,
                            });
                        }
                        None
                    }
                    Tag::Image {
                        dest_url, title, ..
                    } => {
                        let mut n = node("image", vec![]);
                        n.attrs =
                            json!({"src":dest_url.to_string(),"title":title.to_string(),"alt":""});
                        image = Some(n);
                        None
                    }
                    Tag::Table(_) => Some(node("table", vec![])),
                    Tag::TableHead => {
                        table_head = true;
                        Some(node("table_row", vec![]))
                    }
                    Tag::TableRow => Some(node("table_row", vec![])),
                    Tag::TableCell => {
                        let mut n = node(
                            if table_head {
                                "table_header"
                            } else {
                                "table_cell"
                            },
                            vec![],
                        );
                        n.attrs = json!({"colspan":1,"rowspan":1,"colwidth":null});
                        Some(n)
                    }
                    _ => None,
                };
                if let Some(next) = next {
                    stack.push(next);
                }
            }
            Event::End(tag) => match tag {
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                    marks.pop();
                }
                TagEnd::Image => {
                    if let Some(n) = image.take() {
                        if crate::content::safe_nav_url(n.attrs["src"].as_str().unwrap_or("")) {
                            stack.last_mut().unwrap().content.push(n);
                        } else {
                            stack
                                .last_mut()
                                .unwrap()
                                .content
                                .push(text(&format!("Image source: {}", n.attrs["src"]), &[]));
                        }
                    }
                }
                TagEnd::TableHead => {
                    table_head = false;
                    if stack.len() > 1 {
                        let mut n = stack.pop().unwrap();
                        normalize(&mut n);
                        stack.last_mut().unwrap().content.push(n);
                    }
                }
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::BlockQuote(_)
                | TagEnd::CodeBlock
                | TagEnd::List(_)
                | TagEnd::Item
                | TagEnd::Table
                | TagEnd::TableRow
                | TagEnd::TableCell
                    if stack.len() > 1 =>
                {
                    let mut n = stack.pop().unwrap();
                    normalize(&mut n);
                    stack.last_mut().unwrap().content.push(n);
                }
                _ => {}
            },
            Event::Text(s) => {
                if let Some(n) = &mut image {
                    let alt = n.attrs["alt"].as_str().unwrap_or("").to_owned() + &s;
                    n.attrs["alt"] = alt.into();
                } else if !s.is_empty() {
                    stack.last_mut().unwrap().content.push(text(&s, &marks));
                }
            }
            Event::Code(s) => {
                let mut m = marks.clone();
                m.push(Mark {
                    kind: "code".into(),
                    attrs: Value::Null,
                });
                stack.last_mut().unwrap().content.push(text(&s, &m));
            }
            Event::Html(s) | Event::InlineHtml(s) => {
                if !s.is_empty() {
                    stack.last_mut().unwrap().content.push(text(&s, &[]));
                }
            }
            Event::SoftBreak => stack.last_mut().unwrap().content.push(text("\n", &marks)),
            Event::HardBreak => stack
                .last_mut()
                .unwrap()
                .content
                .push(node("hard_break", vec![])),
            Event::Rule => stack
                .last_mut()
                .unwrap()
                .content
                .push(node("horizontal_rule", vec![])),
            _ => {}
        }
    }
    let mut root = stack.remove(0);
    normalize(&mut root);
    for b in serde_json::from_str::<Vec<crate::model::Block>>(blocks).map_err(|_| invalid())? {
        let mut n = node(
            if b.kind == "heading" {
                "heading"
            } else if b.kind == "callout" {
                "callout"
            } else {
                "paragraph"
            },
            vec![],
        );
        if n.kind == "heading" {
            n.attrs = json!({"level":2});
        }
        n.content = if n.kind == "callout" {
            vec![node("paragraph", vec![text(&b.text, &[])])]
        } else {
            vec![text(&b.text, &[])]
        };
        if !b.text.is_empty() {
            root.content.push(n);
        }
    }
    let doc = Document { version: 1, root };
    Document::parse(&doc.encode())
}
fn normalize(n: &mut Node) {
    if [
        "doc",
        "list_item",
        "table_cell",
        "table_header",
        "blockquote",
        "callout",
    ]
    .contains(&n.kind.as_str())
    {
        let old = std::mem::take(&mut n.content);
        let mut inline = vec![];
        for c in old {
            if ["text", "image", "hard_break"].contains(&c.kind.as_str()) {
                inline.push(c);
            } else {
                if !inline.is_empty() {
                    n.content
                        .push(node("paragraph", std::mem::take(&mut inline)));
                }
                n.content.push(c);
            }
        }
        if !inline.is_empty() {
            n.content.push(node("paragraph", inline));
        }
        if n.content.is_empty() {
            n.content.push(node("paragraph", vec![]));
        }
        if n.kind == "list_item" && n.content[0].kind != "paragraph" {
            n.content.insert(0, node("paragraph", vec![]));
        }
    }
}

/// A typed, same-origin form block. No arbitrary embed URLs or executable markup.
pub fn form_embed(id: &str, title: &str) -> String {
    html!{section class="form-embed"{iframe src=(format!("/forms/{id}?embedded=true")) title=(title) loading="lazy" sandbox="allow-scripts allow-forms allow-same-origin" {}p{a href=(format!("/forms/{id}")){"Open " (title) " in a full page"}}}script defer src="/assets/form-embed.js"{}}.into_string()
}
