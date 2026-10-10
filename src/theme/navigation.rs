//! Bounded theme-owned navigation; labels and links never carry executable markup.
use crate::error::{Error, Result};
use maud::{Markup, html};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Navigation {
    pub language: String,
    pub direction: String,
    pub label: String,
    #[serde(default)]
    pub layout: String,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub languages: BTreeMap<String, Variant>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variant {
    pub direction: String,
    pub label: String,
    pub items: Vec<Item>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub label: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub children: Vec<Item>,
}
fn label(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn locale_code(value: &str) -> bool {
    let parts: Vec<_> = value.split('-').collect();
    parts.len() <= 2
        && (2..=3).contains(&parts[0].len())
        && parts[0].bytes().all(|b| b.is_ascii_lowercase())
        && (parts.len() == 1
            || ((2..=3).contains(&parts[1].len())
                && parts[1]
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())))
}
impl Navigation {
    pub fn validate(&self) -> Result<()> {
        if !["", "list", "columns"].contains(&self.layout.as_str())
            || !label(&self.label, 100)
            || self.languages.len() > 16
            || !["ltr", "rtl"].contains(&self.direction.as_str())
            || !locale_code(&self.language)
        {
            return Err(Error::invalid(
                "Navigation needs a label and at most 16 language variants.",
            ));
        }
        fn items(values: &[Item], depth: usize, total: &mut usize) -> Result<()> {
            *total += values.len();
            if depth > 4 || values.len() > 32 || *total > 128 {
                return Err(Error::invalid(
                    "Navigation allows 128 items, 32 siblings and four levels.",
                ));
            }
            for item in values {
                if !label(&item.label, 100)
                    || item.description.len() > 240
                    || item.description.chars().any(char::is_control)
                    || item.url.len() > 2000
                    || (!item.url.is_empty() && !crate::content::safe_nav_url(&item.url))
                    || (item.url.is_empty() && item.children.is_empty())
                {
                    return Err(Error::invalid(
                        "Navigation items need safe links or a labelled group of children.",
                    ));
                }
                if !item.children.is_empty() {
                    items(&item.children, depth + 1, total)?;
                }
            }
            Ok(())
        }
        let mut aggregate = 0;
        items(&self.items, 1, &mut aggregate)?;
        for (locale, variant) in &self.languages {
            if !locale_code(locale)
                || locale == &self.language
                || !["ltr", "rtl"].contains(&variant.direction.as_str())
                || !label(&variant.label, 100)
            {
                return Err(Error::invalid(
                    "Navigation language variants need configured language codes and labels.",
                ));
            }
            let mut count = 0;
            items(&variant.items, 1, &mut count)?;
            aggregate += count;
        }
        if aggregate > 512 {
            return Err(Error::invalid(
                "A navigation family allows 512 items across language variants.",
            ));
        }
        Ok(())
    }
    pub fn render(&self, locale: &str, budget: &mut usize) -> Result<Markup> {
        let (language, direction, name, entries) = self
            .languages
            .get(locale)
            .map(|v| {
                (
                    locale,
                    v.direction.as_str(),
                    v.label.as_str(),
                    v.items.as_slice(),
                )
            })
            .unwrap_or((&self.language, &self.direction, &self.label, &self.items));
        fn list(values: &[Item], budget: &mut usize) -> Result<Markup> {
            let mut body = Markup::default();
            for item in values {
                *budget += 1;
                if *budget > 5000 {
                    return Err(Error::invalid(
                        "Navigation exceeds the shared render budget.",
                    ));
                }
                let children = if item.children.is_empty() {
                    None
                } else {
                    Some(list(&item.children, budget)?)
                };
                body.0.push_str(&html! {li {
                    @if let Some(children) = children {
                        details class="theme-nav-group" {
                            summary {span {(item.label)}}
                            @if !item.url.is_empty() {a class="theme-nav-overview" href=(item.url) {(item.label)}}
                            @if !item.description.is_empty() {p class="theme-nav-description" {(item.description)}}
                            (children)
                        }
                    } @else {
                        a href=(item.url) {(item.label)}
                        @if !item.description.is_empty() {span class="theme-nav-description" {(item.description)}}
                    }
                }}.0);
            }
            Ok(html! {ul {(body)}})
        }
        Ok(
            html! {nav class=(if self.layout=="columns" {"theme-navigation theme-nav-columns"} else {"theme-navigation"}) aria-label=(name) lang=(language) dir=(direction) {(list(entries,budget)?)}},
        )
    }
}
