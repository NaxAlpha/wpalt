//! Independently implemented HTML-to-canonical-document adapter; no executable HTML.
use crate::{
    document::{Document, Mark, Node},
    error::{Error, Result},
};
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use serde_json::{Value, json};
fn node(kind: &str, content: Vec<Node>) -> Node {
    Node {
        kind: kind.into(),
        content,
        attrs: Value::Null,
        text: None,
        marks: vec![],
    }
}
fn text(value: String, marks: Vec<Mark>) -> Node {
    Node {
        text: Some(value),
        marks,
        ..node("text", vec![])
    }
}
fn inline(n: &Node) -> bool {
    ["text", "image", "hard_break"].contains(&n.kind.as_str())
}
fn blocks(children: Vec<Node>) -> Vec<Node> {
    let mut out = vec![];
    let mut pending = vec![];
    for child in children {
        if inline(&child) {
            if pending.is_empty() && child.text.as_ref().is_some_and(|t| t.trim().is_empty()) {
                continue;
            }
            pending.push(child);
        } else {
            if !pending.is_empty() {
                out.push(node("paragraph", std::mem::take(&mut pending)));
            }
            out.push(child);
        }
    }
    if !pending.is_empty() {
        out.push(node("paragraph", pending));
    }
    if out.is_empty() {
        out.push(node("paragraph", vec![]));
    }
    out
}
pub fn import(source: &str) -> Result<Document> {
    if source.len() > 128 * 1024 {
        return Err(Error::invalid(
            "Imported HTML exceeds the bounded conversion budget.",
        ));
    }
    // Conservative balanced markup admission: mismatched closing tags cannot
    // cancel unrelated nesting, and HTML ignores self-closing non-void tags.
    let mut stack = Vec::new();
    let mut tags = 0usize;
    for chunk in source.split('<').skip(1) {
        let (tag, _) = chunk
            .split_once('>')
            .ok_or_else(|| Error::invalid("Imported markup needs balanced tags."))?;
        let tag = tag.trim();
        if tag.starts_with('!') || tag.starts_with('?') {
            continue;
        }
        tags += 1;
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches('/')
            .to_ascii_lowercase();
        if name.is_empty() || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
            return Err(Error::invalid("Imported markup needs balanced tags."));
        }
        let void = [
            "img", "br", "hr", "input", "meta", "link", "source", "area", "base", "col", "embed",
            "param", "track", "wbr",
        ]
        .contains(&name.as_str());
        if closing {
            if void || stack.pop().as_deref() != Some(name.as_str()) {
                return Err(Error::invalid("Imported markup needs balanced tags."));
            }
        } else if !void {
            if tag.ends_with('/') {
                return Err(Error::invalid(
                    "Imported non-void HTML tags require explicit closing tags.",
                ));
            }
            stack.push(name);
        }
        if stack.len() > 48 || tags > 8192 {
            return Err(Error::invalid(
                "Imported HTML exceeds the bounded conversion budget.",
            ));
        }
    }
    if !stack.is_empty() {
        return Err(Error::invalid("Imported markup needs balanced tags."));
    }
    let clean = ammonia::clean(source);
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(clean);
    let mut count = 0;
    let children = walk(&dom.document, &[], 0, &mut count)?;
    let document = Document {
        version: 1,
        root: node("doc", blocks(children)),
    };
    Document::parse(&document.encode())
}
fn walk(handle: &Handle, marks: &[Mark], depth: usize, count: &mut usize) -> Result<Vec<Node>> {
    *count += 1;
    if depth > 64 || *count > 20000 {
        return Err(Error::invalid("HTML tree exceeds its conversion boundary."));
    }
    match &handle.data {
        NodeData::Text { contents } => {
            Ok(vec![text(contents.borrow().to_string(), marks.to_vec())])
        }
        NodeData::Document => children(handle, marks, depth, count),
        NodeData::Element { name, attrs, .. } => {
            let tag = name.local.as_ref();
            let attrs = attrs.borrow();
            let value = |key: &str| {
                attrs
                    .iter()
                    .find(|a| a.name.local.as_ref() == key)
                    .map_or("", |a| a.value.as_ref())
            };
            let mut marks = marks.to_vec();
            match tag {
                "head" | "script" | "style" => return Ok(vec![]),
                "strong" | "b" => marks.push(Mark {
                    kind: "strong".into(),
                    attrs: Value::Null,
                }),
                "em" | "i" => marks.push(Mark {
                    kind: "em".into(),
                    attrs: Value::Null,
                }),
                "s" | "del" => marks.push(Mark {
                    kind: "strike".into(),
                    attrs: Value::Null,
                }),
                "code" => marks.push(Mark {
                    kind: "code".into(),
                    attrs: Value::Null,
                }),
                "a" if !value("href").is_empty() => marks.push(Mark {
                    kind: "link".into(),
                    attrs: json!({"href":value("href")}),
                }),
                "img" => {
                    let mut image = node("image", vec![]);
                    image.attrs = json!({"src":value("src"),"alt":value("alt"),"title":value("title"),"loading":"lazy"});
                    return Ok(vec![image]);
                }
                "br" => return Ok(vec![node("hard_break", vec![])]),
                "hr" => return Ok(vec![node("horizontal_rule", vec![])]),
                _ => (),
            }
            // Nested duplicate semantic marks have one canonical representation.
            let mut unique = std::collections::BTreeSet::new();
            marks.retain(|m| unique.insert(m.kind.clone()));
            let converted = children(handle, &marks, depth, count)?;
            let mut wrapped = match tag {
                "p" => node("paragraph", converted),
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    let mut n = node("heading", converted);
                    n.attrs = json!({"level":tag[1..].parse::<u8>().unwrap()});
                    n
                }
                "blockquote" => node("blockquote", blocks(converted)),
                "ul" | "ol" => {
                    let items: Vec<_> = converted
                        .into_iter()
                        .filter(|n| n.kind == "list_item")
                        .collect();
                    if items.is_empty() {
                        return Ok(vec![]);
                    }
                    let mut n = node(
                        if tag == "ul" {
                            "bullet_list"
                        } else {
                            "ordered_list"
                        },
                        items,
                    );
                    if tag == "ol" {
                        n.attrs = json!({"order":value("start").parse::<u64>().ok().filter(|n|(1..=1000000).contains(n)).unwrap_or(1)});
                    }
                    n
                }
                "li" => {
                    let mut c = blocks(converted);
                    if c.first().is_none_or(|n| n.kind != "paragraph") {
                        c.insert(0, node("paragraph", vec![]));
                    }
                    node("list_item", c)
                }
                "pre" => {
                    fn collect(n: &Node, out: &mut String) {
                        if let Some(text) = &n.text {
                            out.push_str(text);
                        }
                        for child in &n.content {
                            collect(child, out);
                        }
                    }
                    let mut content = String::new();
                    for n in converted {
                        collect(&n, &mut content);
                    }
                    node("code_block", vec![text(content, vec![])])
                }
                "table" => {
                    // Native tables have no caption slot: preserve caption prose beside the table.
                    let (rows, prose): (Vec<_>, Vec<_>) =
                        converted.into_iter().partition(|n| n.kind == "table_row");
                    let mut result = if prose.is_empty() {
                        vec![]
                    } else {
                        blocks(prose)
                    };
                    if !rows.is_empty() {
                        result.push(node("table", rows));
                    }
                    return Ok(result);
                }
                "tr" => node(
                    "table_row",
                    converted
                        .into_iter()
                        .filter(|n| ["table_cell", "table_header"].contains(&n.kind.as_str()))
                        .collect(),
                ),
                "td" | "th" => {
                    if !["", "1"].contains(&value("colspan"))
                        || !["", "1"].contains(&value("rowspan"))
                    {
                        return Err(Error::invalid(
                            "Review merged HTML table cells before migration.",
                        ));
                    }
                    let mut n = node(
                        if tag == "th" {
                            "table_header"
                        } else {
                            "table_cell"
                        },
                        blocks(converted),
                    );
                    n.attrs = json!({"colspan":1,"rowspan":1,"colwidth":null});
                    n
                }
                "div" | "section" | "article" | "body" | "html" | "figure" | "figcaption" => {
                    return Ok(blocks(converted));
                }
                _ => return Ok(converted),
            };
            if wrapped.kind == "code_block" {
                wrapped.attrs = json!({"params":""});
            }
            Ok(vec![wrapped])
        }
        _ => Ok(vec![]),
    }
}
fn children(handle: &Handle, marks: &[Mark], depth: usize, count: &mut usize) -> Result<Vec<Node>> {
    let mut result = vec![];
    for child in handle.children.borrow().iter() {
        result.extend(walk(child, marks, depth + 1, count)?);
    }
    Ok(result)
}
