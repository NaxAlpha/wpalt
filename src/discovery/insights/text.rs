//! Declared editorial observations, never a ranking grade or language model.
use crate::error::{Error, Result};
use markup5ever_rcdom::{Handle, NodeData};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageText {
    pub title: String,
    pub description: String,
    pub body: String,
    pub headings: Vec<String>,
    pub image_alternatives: Vec<String>,
    pub link_labels: Vec<String>,
    pub blocks: usize,
    pub main_element: bool,
}
#[derive(Serialize)]
pub struct Observation {
    pub algorithm: &'static str,
    pub unicode_version: String,
    pub normalization_unicode_version: String,
    pub lowercase_unicode_version: String,
    pub scope: &'static str,
    pub graphemes: usize,
    pub word_segments: usize,
    pub sentence_segments: usize,
    pub blocks: usize,
    pub main_element: bool,
    pub focus: String,
    pub locations: Vec<Location>,
}
#[derive(Serialize)]
pub struct Location {
    pub label: &'static str,
    pub occurrences: usize,
}
fn normalized(text: &str) -> String {
    text.nfc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .nfc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn focus(value: &str) -> Result<String> {
    if value.len() > 100 || value.chars().any(char::is_control) {
        return Err(Error::invalid(
            "Use a focus phrase up to 100 bytes on one line.",
        ));
    }
    let value = normalized(value);
    if value.len() > 100 {
        return Err(Error::invalid("Normalized focus phrase exceeds 100 bytes."));
    }
    let words = value.unicode_words().count();
    if !value.is_empty() && !(1..=8).contains(&words) {
        return Err(Error::invalid(
            "Use one to eight Unicode word segments in the focus phrase.",
        ));
    }
    Ok(value)
}
fn occurrences(text: &str, phrase: &str) -> usize {
    if phrase.is_empty() {
        return 0;
    }
    let text = normalized(text);
    let mut boundaries = BTreeSet::from([text.len()]);
    boundaries.extend(text.split_word_bound_indices().map(|(offset, _)| offset));
    text.match_indices(phrase)
        .filter(|(offset, _)| {
            boundaries.contains(offset) && boundaries.contains(&(offset + phrase.len()))
        })
        .count()
}
impl PageText {
    pub fn observe(&self, keyword: &str) -> Result<Observation> {
        let keyword = focus(keyword)?;
        let version = unicode_segmentation::UNICODE_VERSION;
        Ok(Observation {
            algorithm: "wpalt-editorial-text-v1",
            unicode_version: format!("{}.{}.{}", version.0, version.1, version.2),
            normalization_unicode_version: format!("{:?}", unicode_normalization::UNICODE_VERSION),
            lowercase_unicode_version: format!("{:?}", std::char::UNICODE_VERSION),
            scope: "Server HTML main text, or body fallback, excluding navigation/footer/inert/explicitly hidden markup. Default Unicode boundaries are not dictionary word recognition. NFC, Unicode lowercase and normalized whitespace matching; no stemming, full case folding or locale-specific casing. CSS/layout/client-generated text is not evaluated. No score, preferred length or density target.",
            graphemes: self.body.graphemes(true).count(),
            word_segments: self.body.unicode_words().count(),
            sentence_segments: self
                .body
                .unicode_sentences()
                .filter(|s| s.unicode_words().next().is_some())
                .count(),
            blocks: self.blocks,
            main_element: self.main_element,
            locations: vec![
                Location {
                    label: "title",
                    occurrences: occurrences(&self.title, &keyword),
                },
                Location {
                    label: "description",
                    occurrences: occurrences(&self.description, &keyword),
                },
                Location {
                    label: "main text",
                    occurrences: occurrences(&self.body, &keyword),
                },
                Location {
                    label: "headings",
                    occurrences: self.headings.iter().map(|s| occurrences(s, &keyword)).sum(),
                },
                Location {
                    label: "image alternatives",
                    occurrences: self
                        .image_alternatives
                        .iter()
                        .map(|s| occurrences(s, &keyword))
                        .sum(),
                },
                Location {
                    label: "link labels",
                    occurrences: self
                        .link_labels
                        .iter()
                        .map(|s| occurrences(s, &keyword))
                        .sum(),
                },
            ],
            focus: keyword,
        })
    }
}
fn excluded(node: &Handle) -> bool {
    if let NodeData::Element { name, attrs, .. } = &node.data {
        let tag = name.local.as_ref();
        return ["script", "style", "template", "noscript", "nav", "footer"].contains(&tag)
            || attrs.borrow().iter().any(|a| {
                a.name.local.as_ref() == "hidden"
                    || (a.name.local.as_ref() == "aria-hidden"
                        && a.value.as_ref().eq_ignore_ascii_case("true"))
            });
    }
    false
}
fn text(node: &Handle, out: &mut String) {
    if excluded(node) {
        return;
    }
    if let NodeData::Text { contents } = &node.data {
        out.push_str(&contents.borrow());
    }
    for child in node.children.borrow().iter() {
        text(child, out);
    }
}
pub fn parse(html: &str) -> Result<PageText> {
    let (dom, _) = crate::discovery::rendered_links::document(html)?;
    from_document(&dom.document)
}
pub(crate) fn from_document(document: &Handle) -> Result<PageText> {
    let mut page = PageText::default();
    let mut pending = vec![document.clone()];
    let mut main = None;
    let mut body = None;
    while let Some(node) = pending.pop() {
        if excluded(&node) {
            continue;
        }
        if let NodeData::Element { name, attrs, .. } = &node.data {
            let tag = name.local.as_ref();
            if tag == "main" && main.is_none() {
                main = Some(node.clone());
            }
            if tag == "body" {
                body = Some(node.clone());
            }
            if tag == "title" && page.title.is_empty() {
                text(&node, &mut page.title);
            }
            if tag == "meta" {
                let attrs = attrs.borrow();
                if attrs.iter().any(|a| {
                    a.name.local.as_ref() == "name"
                        && a.value.as_ref().eq_ignore_ascii_case("description")
                }) && let Some(content) =
                    attrs.iter().find(|a| a.name.local.as_ref() == "content")
                {
                    page.description = content.value.to_string();
                }
            }
        }
        pending.extend(node.children.borrow().iter().rev().cloned());
    }
    page.main_element = main.is_some();
    let root = main
        .or(body)
        .ok_or(Error::invalid("Rendered text has no document body."))?;
    fn walk(node: &Handle, page: &mut PageText) -> Result<()> {
        if excluded(node) {
            return Ok(());
        }
        if let NodeData::Element { name, attrs, .. } = &node.data {
            let tag = name.local.as_ref();
            if ["div", "section", "article", "tr", "dt", "dd"].contains(&tag) {
                page.body.push_str("\n\n");
            }
            if [
                "p",
                "li",
                "blockquote",
                "pre",
                "td",
                "figcaption",
                "summary",
                "h1",
                "h2",
                "h3",
                "h4",
                "h5",
                "h6",
            ]
            .contains(&tag)
            {
                page.blocks += 1;
                page.body.push_str("\n\n");
            }
            if ["h1", "h2", "h3", "h4", "h5", "h6", "a"].contains(&tag) {
                let mut label = String::new();
                text(node, &mut label);
                if tag == "a" {
                    page.link_labels.push(label);
                } else {
                    page.headings.push(label);
                }
            }
            if tag == "img" {
                let alternative = attrs
                    .borrow()
                    .iter()
                    .find(|a| a.name.local.as_ref() == "alt")
                    .map(|a| a.value.to_string())
                    .unwrap_or_default();
                page.image_alternatives.push(alternative);
            }
            if tag == "br" {
                page.body.push('\n');
            }
        }
        if let NodeData::Text { contents } = &node.data {
            page.body.push_str(&contents.borrow());
        }
        if page.body.len() > 512 * 1024
            || page.headings.len() + page.link_labels.len() + page.image_alternatives.len() > 20000
        {
            return Err(Error::invalid(
                "Rendered editorial text exceeds its declared work budget.",
            ));
        }
        for child in node.children.borrow().iter() {
            walk(child, page)?;
        }
        if let NodeData::Element { name, .. } = &node.data
            && [
                "div",
                "section",
                "article",
                "tr",
                "dt",
                "dd",
                "p",
                "li",
                "blockquote",
                "pre",
                "td",
                "figcaption",
                "summary",
                "h1",
                "h2",
                "h3",
                "h4",
                "h5",
                "h6",
            ]
            .contains(&name.local.as_ref())
        {
            page.body.push_str("\n\n");
        }
        Ok(())
    }
    walk(&root, &mut page)?;
    let labels = page.title.len()
        + page.description.len()
        + page
            .headings
            .iter()
            .chain(&page.link_labels)
            .chain(&page.image_alternatives)
            .map(String::len)
            .sum::<usize>();
    if labels > 2 * 1024 * 1024 {
        return Err(Error::invalid(
            "Rendered editorial labels exceed their work budget.",
        ));
    }
    if page.body.len() > 512 * 1024 {
        return Err(Error::invalid(
            "Rendered editorial text exceeds its declared work budget.",
        ));
    }
    Ok(page)
}
