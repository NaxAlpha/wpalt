//! Frozen, owner-declared routing and moderated content actions.
use super::{
    forms::{Condition, FormField},
    mail::{self, MessageInput},
};
use crate::{
    App,
    document::Document,
    error::{Error, Result},
    now,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Any, Row, Transaction};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notification {
    pub recipient: String,
    pub subject: String,
    pub document: String,
    #[serde(default)]
    pub when: Option<Condition>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftPost {
    pub title_field: String,
    pub body_field: String,
}
impl Notification {
    pub fn validate(&self, fields: &[FormField]) -> Result<()> {
        mail::email(&self.recipient)?;
        if self.subject.is_empty()
            || self.subject.len() > 200
            || self.subject.contains(['\r', '\n'])
        {
            return Err(Error::invalid("Use a bounded notification subject."));
        }
        Document::parse(&self.document)?;
        if let Some(condition) = &self.when {
            let field = match condition {
                Condition::Equal { field, .. } | Condition::Present { field } => field,
            };
            if !fields.iter().any(|f| &f.name == field) {
                return Err(Error::invalid(
                    "Route conditions must refer to a declared field.",
                ));
            }
            if let Condition::Equal { value, .. } = condition
                && !value.is_boolean()
                && !value.is_number()
                && !value.is_string()
            {
                return Err(Error::invalid("Route comparison must be a scalar."));
            }
        }
        Ok(())
    }
    fn matches(&self, values: &Value) -> bool {
        match &self.when {
            None => true,
            Some(Condition::Equal { field, value }) => values.get(field) == Some(value),
            Some(Condition::Present { field }) => values
                .get(field)
                .is_some_and(|v| !v.is_null() && v.as_str() != Some("")),
        }
    }
}
pub async fn apply(
    app: &App,
    tx: &mut Transaction<'_, Any>,
    form: &super::forms::FormDefinition,
    entry: &str,
    form_id: &str,
    values: &Value,
) -> Result<()> {
    for (index, rule) in form.notifications.iter().enumerate() {
        if !rule.matches(values) {
            continue;
        }
        let doc = Document::parse(&rule.document)?;
        mail::enqueue(
            app,
            tx,
            MessageInput {
                dedupe: &format!("entry:{entry}:notification:{index}"),
                contact: "",
                list: "",
                kind: "notification",
                recipient: &rule.recipient,
                subject: &rule.subject,
                html: &doc.mail_html(&app.config.base_url),
                plain: &doc.markdown(),
            },
        )
        .await?;
    }
    if let Some(action) = &form.registration {
        super::registration::request(app, tx, entry, action, values).await?;
    }
    if let Some(action) = &form.draft_post {
        let title = values
            .get(&action.title_field)
            .and_then(Value::as_str)
            .unwrap_or("");
        let body = values
            .get(&action.body_field)
            .and_then(Value::as_str)
            .unwrap_or("");
        if title.trim().is_empty() || title.len() > 240 || body.len() > 8000 {
            return Err(Error::invalid(
                "Moderated contributions need a title up to 240 bytes and body up to 8,000 bytes.",
            ));
        }
        // Visitor text is literal, never imported as executable HTML or arbitrary document nodes.
        let doc = serde_json::json!({"version":1,"root":{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":body}]}]}});
        let doc = if body.is_empty() {
            crate::document::empty()
        } else {
            Document::parse(&doc.to_string())?.encode()
        };
        let id = uuid::Uuid::new_v4().to_string();
        let slug = format!("contribution-{}", uuid::Uuid::new_v4().simple());
        let owner: String = sqlx::query_scalar("SELECT owner_id FROM business_forms WHERE id=$1")
            .bind(form_id)
            .fetch_one(&mut **tx)
            .await?;
        sqlx::query("INSERT INTO posts(id,slug,kind,title,body,fields,blocks,status,version,published_slug,published_title,published_body,published_fields,published_blocks,publish_at,published_at,updated_at,author_id,document,published_document) VALUES($1,$2,'post',$3,$4,'{}','[]','draft',1,'','','','{}','[]',0,0,$5,$6,$7,$8)").bind(&id).bind(slug).bind(title).bind(body).bind(now()).bind(owner).bind(doc).bind(crate::document::empty()).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO form_contributions(entry_id,post_id) VALUES($1,$2)")
            .bind(entry)
            .bind(id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
pub const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS form_contributions(entry_id TEXT PRIMARY KEY REFERENCES form_entries(id),post_id TEXT NOT NULL REFERENCES posts(id));";
/// Confirmed list membership triggers a bounded set of owner-composed campaign templates.
pub async fn confirmed(
    app: &App,
    tx: &mut Transaction<'_, Any>,
    contact: &str,
    list: &str,
) -> Result<()> {
    let contact_row=sqlx::query("SELECT c.email,c.attributes,l.purpose FROM audience_contacts c JOIN audience_lists l ON l.id=$2 WHERE c.id=$1 AND c.suppressed=0").bind(contact).bind(list).fetch_optional(&mut **tx).await?;
    let Some(contact_row) = contact_row else {
        return Ok(());
    };
    let attrs: super::audience::Attributes =
        serde_json::from_str(&contact_row.get::<String, _>("attributes"))
            .map_err(|_| Error::invalid("Stored attributes need repair."))?;
    let templates=sqlx::query("SELECT id,subject,document,segment FROM business_campaigns WHERE list_id=$1 AND trigger_kind='confirmation' AND state='draft' ORDER BY created_at,id LIMIT 16").bind(list).fetch_all(&mut **tx).await?;
    for row in templates {
        let segment: super::audience::Segment =
            serde_json::from_str(&row.get::<String, _>("segment"))
                .map_err(|_| Error::invalid("Stored segment needs repair."))?;
        if !segment.matches(&attrs) {
            continue;
        }
        let token = crate::auth::random_token();
        sqlx::query("INSERT INTO audience_withdrawal_tokens(hash,contact_id,list_id,created_at) VALUES($1,$2,$3,$4)").bind(crate::auth::digest(token.as_bytes())).bind(contact).bind(list).bind(now()).execute(&mut **tx).await?;
        let url = format!(
            "{}/audience/withdraw/{token}",
            app.config.base_url.trim_end_matches('/')
        );
        let doc = Document::parse(&row.get::<String, _>("document"))?;
        let html = format!(
            "{}{}",
            doc.mail_html(&app.config.base_url),
            maud::html! {hr;p{(contact_row.get::<String,_>("purpose"))}p{a href=(&url){"Withdraw subscription"}}}.into_string()
        );
        mail::enqueue(
            app,
            tx,
            MessageInput {
                dedupe: &format!("confirmed:{}:{contact}", row.get::<String, _>("id")),
                contact,
                list,
                kind: "campaign",
                recipient: &contact_row.get::<String, _>("email"),
                subject: &row.get::<String, _>("subject"),
                html: &html,
                plain: &format!("{}\nWithdraw subscription: {url}", doc.markdown()),
            },
        )
        .await?;
    }
    Ok(())
}
