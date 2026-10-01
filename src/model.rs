use serde::{Deserialize, Serialize};
use sqlx::{Row, any::AnyRow};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Post {
    pub id: String,
    pub slug: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub fields: String,
    pub blocks: String,
    pub status: String,
    pub version: i64,
    pub published_slug: String,
    pub published_title: String,
    pub published_body: String,
    pub published_fields: String,
    pub published_blocks: String,
    pub publish_at: i64,
    pub published_at: i64,
    pub updated_at: i64,
    pub author_id: String,
    pub locale: String,
    pub translation_group: String,
    pub seo: String,
    pub published_locale: String,
    pub published_translation_group: String,
    pub published_seo: String,
}
impl Post {
    pub fn from_row(r: AnyRow) -> Self {
        Self {
            id: r.get("id"),
            slug: r.get("slug"),
            kind: r.get("kind"),
            title: r.get("title"),
            body: r.get("body"),
            fields: r.get("fields"),
            blocks: r.get("blocks"),
            status: r.get("status"),
            version: r.get("version"),
            published_slug: r.get("published_slug"),
            published_title: r.get("published_title"),
            published_body: r.get("published_body"),
            published_fields: r.get("published_fields"),
            published_blocks: r.get("published_blocks"),
            publish_at: r.get("publish_at"),
            published_at: r.get("published_at"),
            updated_at: r.get("updated_at"),
            author_id: r.get("author_id"),
            locale: r.get("locale"),
            translation_group: r.get("translation_group"),
            seo: r.get("seo"),
            published_locale: r.get("published_locale"),
            published_translation_group: r.get("published_translation_group"),
            published_seo: r.get("published_seo"),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub title: String,
    pub description: String,
    pub theme: String,
    pub navigation: String,
    pub field_schema: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            title: "My wpalt site".into(),
            description: "A place for ideas, built on your own server.".into(),
            theme: "paper".into(),
            navigation: "[]".into(),
            field_schema: serde_json::to_string(&crate::schema::Definition::initial()).unwrap(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: String,
    pub role: String,
}
#[derive(Clone, Debug)]
pub struct Session {
    pub user: User,
    pub csrf: String,
    pub hash: String,
}
impl Session {
    pub fn can_edit(&self) -> bool {
        self.user.role == "admin" || self.user.role == "editor"
    }
    pub fn can_moderate(&self) -> bool {
        self.user.role == "admin" || self.user.role == "moderator"
    }
    pub fn is_admin(&self) -> bool {
        self.user.role == "admin"
    }
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct NavItem {
    pub label: String,
    pub url: String,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub kind: String,
    pub text: String,
}
#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct PostInput {
    #[serde(default = "default_locale")]
    pub locale: String,
    #[serde(default)]
    pub translation_group: String,
    #[serde(default = "empty_object")]
    pub seo: String,
    pub title: String,
    pub slug: String,
    pub kind: String,
    pub body: String,
    #[serde(default = "empty_object")]
    pub fields: String,
    #[serde(default = "empty_array")]
    pub blocks: String,
    #[serde(default)]
    pub categories: String,
    #[serde(default)]
    pub tags: String,
    #[serde(default = "empty_object")]
    pub taxonomies: String,
    #[serde(default)]
    pub version: i64,
    #[serde(default = "save_action")]
    pub action: String,
    #[serde(default)]
    pub publish_at: i64,
    #[serde(default)]
    pub csrf: String,
}
fn empty_object() -> String {
    "{}".into()
}
fn empty_array() -> String {
    "[]".into()
}
fn save_action() -> String {
    "save".into()
}

fn default_locale() -> String {
    "en".into()
}
