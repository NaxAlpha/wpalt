//! Bundled, bounded interface messages. Content language is a separate authority.
use crate::error::{Error, Result};
use maud::{Markup, html};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    version: u8,
    scope: String,
    languages: BTreeMap<String, Language>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Language {
    direction: String,
    messages: BTreeMap<String, String>,
}
static BUNDLE: OnceLock<Bundle> = OnceLock::new();
fn bundle() -> &'static Bundle {
    BUNDLE.get_or_init(|| {
        let b: Bundle = serde_json::from_str(include_str!("../../assets/locales/admin-v1.json"))
            .expect("validated bundled interface catalog");
        assert_eq!(b.version, 1);
        assert!(b.languages.len() <= 16 && !b.scope.is_empty());
        assert!(b.languages.contains_key("en"));
        for l in b.languages.values() {
            assert!(matches!(l.direction.as_str(), "ltr" | "rtl"));
            assert!(l.messages.len() <= 512);
            for (id, text) in &l.messages {
                assert!(id.len() <= 80 && text.len() <= 4096);
                assert!(text.matches("{value}").count() <= 1);
                let literal = text.replace("{value}", "");
                assert!(!literal.contains(['{', '}']));
            }
        }
        b
    })
}
#[derive(Clone, Copy)]
pub struct Catalog(&'static str);
impl Catalog {
    pub fn select(locale: &str) -> Result<Self> {
        match locale {
            "en" => Ok(Self("en")),
            "fr" => Ok(Self("fr")),
            "ja" => Ok(Self("ja")),
            "ar" => Ok(Self("ar")),
            _ => Err(Error::invalid("Choose a supported interface language.")),
        }
    }
    pub fn english() -> Self {
        Self("en")
    }
    pub fn locale(self) -> &'static str {
        self.0
    }
    pub fn direction(self) -> &'static str {
        &bundle().languages[self.0].direction
    }
    pub fn scope(self) -> &'static str {
        &bundle().scope
    }
    pub fn text(self, id: &str) -> &'static str {
        let b = bundle();
        b.languages[self.0]
            .messages
            .get(id)
            .or_else(|| b.languages["en"].messages.get(id))
            .map(String::as_str)
            .unwrap_or("Translation unavailable")
    }
    /// User values remain escaped text, even if they contain catalog-looking tokens or HTML.
    pub fn value(self, id: &str, value: &str) -> Markup {
        let text = self.text(id);
        if let Some((before, after)) = text.split_once("{value}") {
            html! { (before) (value) (after) }
        } else {
            html! { (text) }
        }
    }
    pub fn missing(self) -> Vec<&'static str> {
        let b = bundle();
        b.languages["en"]
            .messages
            .keys()
            .filter(|id| !b.languages[self.0].messages.contains_key(*id))
            .map(String::as_str)
            .collect()
    }
    /// Counts choose language grammar; arbitrary user strings never choose catalog keys.
    pub fn content_count(self, count: u64) -> String {
        let n = self.number(count);
        match self.0 {
            "fr" if count <= 1 => format!("{n} contenu"),
            "fr" => format!("{n} contenus"),
            "ja" => format!("{n} 件のコンテンツ"),
            "ar" if count == 0 => "لا توجد عناصر".into(),
            "ar" if count == 1 => "عنصر واحد".into(),
            "ar" if count == 2 => "عنصران".into(),
            "ar" if (3..=10).contains(&(count % 100)) => format!("{n} عناصر"),
            "ar" if (11..=99).contains(&(count % 100)) => format!("{n} عنصرًا"),
            "ar" => format!("{n} عنصر"),
            _ if count == 1 => format!("{n} content item"),
            _ => format!("{n} content items"),
        }
    }
    pub fn number(self, number: u64) -> String {
        let s = number.to_string();
        let separator = match self.0 {
            "fr" => '\u{202f}',
            "ar" => '\u{066c}',
            _ => ',',
        };
        let mut out = String::new();
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i).is_multiple_of(3) {
                out.push(separator);
            }
            out.push(if self.0 == "ar" {
                char::from_u32('\u{0660}' as u32 + c as u32 - '0' as u32).unwrap()
            } else {
                c
            });
        }
        out
    }
}
