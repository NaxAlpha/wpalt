//! Purpose-specific, confirmed subscriptions. Public claims never overwrite a contact.
use super::mail::{self, MessageInput};
use crate::{
    App, auth,
    error::{Error, Result},
    now,
};
use serde::{Deserialize, Serialize};
use sqlx::{Any, Row, Transaction};
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS business_secrets(id TEXT PRIMARY KEY,value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS audience_suppressions(hash TEXT PRIMARY KEY,suppressed BIGINT NOT NULL DEFAULT 0,created_at BIGINT NOT NULL);
CREATE TABLE IF NOT EXISTS audience_contacts(id TEXT PRIMARY KEY,email TEXT NOT NULL UNIQUE,name TEXT NOT NULL,attributes TEXT NOT NULL DEFAULT '{}',version BIGINT NOT NULL DEFAULT 1,suppressed BIGINT NOT NULL DEFAULT 0 CHECK(suppressed IN (0,1)),created_at BIGINT NOT NULL);
CREATE TABLE IF NOT EXISTS audience_lists(id TEXT PRIMARY KEY,title TEXT NOT NULL,purpose TEXT NOT NULL,policy TEXT NOT NULL,created_at BIGINT NOT NULL);
CREATE TABLE IF NOT EXISTS audience_memberships(contact_id TEXT NOT NULL REFERENCES audience_contacts(id) ON DELETE CASCADE,list_id TEXT NOT NULL REFERENCES audience_lists(id),state TEXT NOT NULL CHECK(state IN ('pending','confirmed','withdrawn')),policy TEXT NOT NULL,nonce_hash TEXT NOT NULL,withdraw_hash TEXT NOT NULL,expires_at BIGINT NOT NULL,confirmed_at BIGINT NOT NULL DEFAULT 0,created_at BIGINT NOT NULL,PRIMARY KEY(contact_id,list_id));
CREATE INDEX IF NOT EXISTS audience_confirmation ON audience_memberships(nonce_hash);
CREATE INDEX IF NOT EXISTS audience_withdrawal ON audience_memberships(withdraw_hash);
CREATE INDEX IF NOT EXISTS audience_recipients ON audience_memberships(list_id,state,confirmed_at,contact_id);
CREATE TABLE IF NOT EXISTS audience_consent_events(id TEXT PRIMARY KEY,contact_id TEXT NOT NULL REFERENCES audience_contacts(id) ON DELETE CASCADE,list_id TEXT NOT NULL REFERENCES audience_lists(id),action TEXT NOT NULL,policy TEXT NOT NULL,purpose TEXT NOT NULL,created_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS audience_consent_history ON audience_consent_events(contact_id,created_at,id);
"#;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionAction {
    pub list: String,
    pub policy: String,
    pub email_field: String,
    pub consent_field: String,
}
impl SubscriptionAction {
    pub fn validate(&self, fields: &[super::forms::FormField]) -> Result<()> {
        if uuid::Uuid::parse_str(&self.list).is_err()
            || self.policy.is_empty()
            || self.policy.len() > 64
            || !fields
                .iter()
                .any(|f| f.name == self.email_field && f.schema.kind == "string")
            || !fields
                .iter()
                .any(|f| f.name == self.consent_field && f.schema.kind == "boolean")
        {
            return Err(Error::invalid(
                "Choose an email field, a consent checkbox and a versioned audience list.",
            ));
        }
        Ok(())
    }
}
pub async fn create_list(app: &App, title: &str, purpose: &str, policy: &str) -> Result<String> {
    if title.trim().is_empty()
        || title.len() > 160
        || purpose.trim().is_empty()
        || purpose.len() > 1000
        || policy.trim().is_empty()
        || policy.len() > 64
    {
        return Err(Error::invalid(
            "A list needs a title, an explicit purpose and a policy version.",
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO audience_lists(id,title,purpose,policy,created_at) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(&id)
    .bind(title.trim())
    .bind(purpose.trim())
    .bind(policy.trim())
    .bind(now())
    .execute(&app.db.pool)
    .await?;
    Ok(id)
}
pub async fn subscribe(
    app: &App,
    tx: &mut Transaction<'_, Any>,
    action: &SubscriptionAction,
    values: &serde_json::Value,
) -> Result<()> {
    if values.get(&action.consent_field).and_then(|v| v.as_bool()) != Some(true) {
        return Ok(());
    }
    let email = mail::email(
        values
            .get(&action.email_field)
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::invalid("Provide an email address to subscribe."))?,
    )?;
    let suppression = suppression_hash(tx, &email).await?;
    sqlx::query("INSERT INTO audience_suppressions(hash,created_at) VALUES($1,$2) ON CONFLICT(hash) DO NOTHING").bind(&suppression).bind(now()).execute(&mut **tx).await?;
    let suppressed: i64 = sqlx::query_scalar(
        "UPDATE audience_suppressions SET suppressed=suppressed WHERE hash=$1 RETURNING suppressed",
    )
    .bind(&suppression)
    .fetch_one(&mut **tx)
    .await?;
    if suppressed != 0 {
        return Ok(());
    }
    let list = sqlx::query("SELECT purpose,policy FROM audience_lists WHERE id=$1")
        .bind(&action.list)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| Error::invalid("The audience list is unavailable."))?;
    if list.get::<String, _>("policy") != action.policy {
        return Err(Error::conflict());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let inserted=sqlx::query("INSERT INTO audience_contacts(id,email,name,created_at) VALUES($1,$2,'',$3) ON CONFLICT(email) DO NOTHING").bind(id).bind(&email).bind(now()).execute(&mut **tx).await?;
    if inserted.rows_affected() == 1 {
        super::quotas::reserve(app, tx, "contacts", 1, 0).await?;
    }
    // Lock the shared contact before membership reads on both engines.
    let contact = sqlx::query(
        "UPDATE audience_contacts SET suppressed=suppressed WHERE email=$1 RETURNING id,suppressed",
    )
    .bind(&email)
    .fetch_one(&mut **tx)
    .await?;
    if contact.get::<i64, _>("suppressed") != 0 {
        return Ok(());
    }
    let id: String = contact.get("id");
    if let Some(existing)=sqlx::query("SELECT state,policy,expires_at FROM audience_memberships WHERE contact_id=$1 AND list_id=$2").bind(&id).bind(&action.list).fetch_optional(&mut **tx).await? {
  if existing.get::<String,_>("state")=="withdrawn" || (existing.get::<String,_>("policy")==action.policy && (existing.get::<String,_>("state")=="confirmed" || existing.get::<i64,_>("expires_at")>now())) {return Ok(());}
 }
    let nonce = auth::random_token();
    let withdraw = auth::random_token();
    sqlx::query("INSERT INTO audience_memberships(contact_id,list_id,state,policy,nonce_hash,withdraw_hash,expires_at,created_at) VALUES($1,$2,'pending',$3,$4,$5,$6,$7) ON CONFLICT(contact_id,list_id) DO UPDATE SET state='pending',policy=$3,nonce_hash=$4,withdraw_hash=$5,expires_at=$6,confirmed_at=0")
 .bind(&id).bind(&action.list).bind(&action.policy).bind(auth::digest(nonce.as_bytes())).bind(auth::digest(withdraw.as_bytes())).bind(now()+86400).bind(now()).execute(&mut **tx).await?;
    let purpose: String = list.get("purpose");
    record(tx, &id, &action.list, "requested", &action.policy, &purpose).await?;
    let url = format!(
        "{}/audience/confirm/{nonce}",
        app.config.base_url.trim_end_matches('/')
    );
    let withdraw_url = format!(
        "{}/audience/withdraw/{withdraw}",
        app.config.base_url.trim_end_matches('/')
    );
    let body=maud::html!{p {"Confirm subscription for: " (&purpose)} p {a href=(&url){"Review and confirm"}} p {a href=(&withdraw_url){"Withdraw this request"}}}.into_string();
    let plain = format!("Confirm subscription for: {purpose}\n{url}\nWithdraw: {withdraw_url}");
    mail::enqueue(
        app,
        tx,
        MessageInput {
            dedupe: &format!("confirmation:{id}:{}:{nonce}", action.list),
            contact: &id,
            list: &action.list,
            kind: "confirmation",
            recipient: &email,
            subject: "Confirm your subscription",
            html: &body,
            plain: &plain,
        },
    )
    .await?;
    Ok(())
}
async fn record(
    tx: &mut Transaction<'_, Any>,
    contact: &str,
    list: &str,
    action: &str,
    policy: &str,
    purpose: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO audience_consent_events(id,contact_id,list_id,action,policy,purpose,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(uuid::Uuid::new_v4().to_string()).bind(contact).bind(list).bind(action).bind(policy).bind(purpose).bind(now()).execute(&mut **tx).await?;
    Ok(())
}
pub async fn review(app: &App, token: &str, withdraw: bool) -> Result<(String, String)> {
    if token.len() != 64 || !token.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::not_found());
    }
    let query = if withdraw {
        "SELECT l.title,l.purpose FROM audience_memberships m JOIN audience_lists l ON l.id=m.list_id WHERE m.withdraw_hash=$1 OR EXISTS(SELECT 1 FROM audience_withdrawal_tokens t WHERE t.hash=$1 AND t.contact_id=m.contact_id AND t.list_id=m.list_id)"
    } else {
        "SELECT l.title,l.purpose FROM audience_memberships m JOIN audience_lists l ON l.id=m.list_id WHERE m.nonce_hash=$1 AND m.expires_at>$2 AND m.policy=l.policy"
    };
    let q = sqlx::query(query).bind(auth::digest(token.as_bytes()));
    let q = if withdraw { q } else { q.bind(now()) };
    let row = q
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
    Ok((row.get("title"), row.get("purpose")))
}
pub async fn decide(app: &App, token: &str, withdraw: bool) -> Result<()> {
    review(app, token, withdraw).await?;
    let mut tx = app.db.pool.begin().await?;
    super::quotas::release(&mut tx, "mail", 0, 0).await?;
    let query = if withdraw {
        "UPDATE audience_memberships SET state='withdrawn' WHERE (withdraw_hash=$1 OR EXISTS(SELECT 1 FROM audience_withdrawal_tokens t WHERE t.hash=$1 AND t.contact_id=audience_memberships.contact_id AND t.list_id=audience_memberships.list_id)) AND state!='withdrawn' RETURNING contact_id,list_id,policy"
    } else {
        "UPDATE audience_memberships SET state='confirmed',confirmed_at=$2 WHERE nonce_hash=$1 AND expires_at>$2 AND state='pending' AND policy=(SELECT policy FROM audience_lists WHERE id=list_id) RETURNING contact_id,list_id,policy"
    };
    let q = sqlx::query(query).bind(auth::digest(token.as_bytes()));
    let q = if withdraw { q } else { q.bind(now()) };
    if let Some(row) = q.fetch_optional(&mut *tx).await? {
        let contact: String = row.get("contact_id");
        let list: String = row.get("list_id");
        let purpose: String = sqlx::query_scalar("SELECT purpose FROM audience_lists WHERE id=$1")
            .bind(&list)
            .fetch_one(&mut *tx)
            .await?;
        record(
            &mut tx,
            &contact,
            &list,
            if withdraw { "withdrawn" } else { "confirmed" },
            &row.get::<String, _>("policy"),
            &purpose,
        )
        .await?;
        if !withdraw {
            super::workflows::confirmed(app, &mut tx, &contact, &list).await?;
        }
        if withdraw {
            sqlx::query("UPDATE mail_jobs SET state='cancelled',lease_owner='',lease_until=0 WHERE contact_id=$1 AND list_id=$2 AND kind='campaign' AND state IN ('pending','retry','leased')").bind(contact).bind(list).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Attributes {
    pub company: String,
    pub source: String,
    pub score: f64,
}
impl Attributes {
    pub fn validate(&self) -> Result<()> {
        if self.company.len() > 160
            || self.source.len() > 160
            || !self.score.is_finite()
            || self.score.abs() > 1000000.0
        {
            return Err(Error::invalid(
                "Contact attributes exceed their text or numeric limits.",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Segment {
    pub company: Option<String>,
    pub source: Option<String>,
    pub minimum_score: Option<f64>,
}
impl Segment {
    pub fn validate(&self) -> Result<()> {
        if self.company.as_ref().is_some_and(|v| v.len() > 160)
            || self.source.as_ref().is_some_and(|v| v.len() > 160)
            || self
                .minimum_score
                .is_some_and(|v| !v.is_finite() || v.abs() > 1000000.0)
        {
            return Err(Error::invalid("Invalid bounded contact segment."));
        }
        Ok(())
    }
    pub fn matches(&self, value: &Attributes) -> bool {
        self.company.as_ref().is_none_or(|v| v == &value.company)
            && self.source.as_ref().is_none_or(|v| v == &value.source)
            && self.minimum_score.is_none_or(|v| value.score >= v)
    }
}
async fn suppression_hash(tx: &mut Transaction<'_, Any>, email: &str) -> Result<String> {
    sqlx::query("INSERT INTO business_secrets(id,value) VALUES('audience_suppression',$1) ON CONFLICT(id) DO NOTHING").bind(auth::random_token()).execute(&mut **tx).await?;
    let secret: String =
        sqlx::query_scalar("SELECT value FROM business_secrets WHERE id='audience_suppression'")
            .fetch_one(&mut **tx)
            .await?;
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|_| Error::invalid("Invalid owner suppression key."))?;
    mac.update(email.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}
pub async fn update_contact(
    app: &App,
    id: &str,
    version: i64,
    name: &str,
    attributes: Attributes,
    suppress: bool,
) -> Result<()> {
    attributes.validate()?;
    if name.len() > 100 {
        return Err(Error::invalid("Contact names are limited to 100 bytes."));
    }
    let mut tx = app.db.pool.begin().await?;
    if sqlx::query("UPDATE audience_contacts SET name=$1,attributes=$2,suppressed=$3,version=version+1 WHERE id=$4 AND version=$5")
        .bind(name)
        .bind(
            serde_json::to_string(&attributes)
                .map_err(|_| Error::invalid("Invalid contact attributes."))?,
        )
        .bind(i64::from(suppress))
        .bind(id)
        .bind(version)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        != 1
    {
        return Err(Error::conflict());
    }
    if suppress {
        sqlx::query("UPDATE mail_jobs SET state='cancelled',lease_owner='',lease_until=0 WHERE contact_id=$1 AND kind='campaign' AND state IN ('pending','retry','leased')").bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}
pub async fn delete_contact(app: &App, id: &str) -> Result<()> {
    let _guard = app.mutations.lock().await;
    let mut tx = app.db.pool.begin().await?;
    super::quotas::release(&mut tx, "contacts", 0, 0).await?;
    super::quotas::release(&mut tx, "mail", 0, 0).await?;
    if !app.db.postgres {
        sqlx::query("UPDATE audience_contacts SET suppressed=suppressed WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    let email: String = sqlx::query_scalar("SELECT email FROM audience_contacts WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(Error::not_found)?;
    let hash = suppression_hash(&mut tx, &email).await?;
    sqlx::query("INSERT INTO audience_suppressions(hash,suppressed,created_at) VALUES($1,1,$2) ON CONFLICT(hash) DO UPDATE SET suppressed=1").bind(hash).bind(now()).execute(&mut *tx).await?;
    // Address admission locks the same keyed guard before touching the contact,
    // so a concurrent anonymous claim cannot race past deletion and recreate it.
    sqlx::query("UPDATE audience_contacts SET suppressed=1 WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    super::quotas::release(&mut tx, "contacts", 1, 0).await?;
    super::quotas::release(&mut tx, "mail", 0, 0).await?;
    let jobs =
        sqlx::query("DELETE FROM mail_jobs WHERE contact_id=$1 RETURNING id,html,plain,subject")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
    let mail_bytes: i64 = jobs
        .iter()
        .map(|r| {
            r.get::<String, _>("html").len() as i64
                + r.get::<String, _>("plain").len() as i64
                + r.get::<String, _>("subject").len() as i64
        })
        .sum();
    super::quotas::release(&mut tx, "mail", jobs.len() as i64, mail_bytes).await?;

    sqlx::query("DELETE FROM audience_contacts WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    for job in jobs {
        let id: String = job.get("id");
        if uuid::Uuid::parse_str(&id).is_ok() {
            let _ = tokio::fs::remove_file(
                app.config.data_dir.join("outbox").join(format!("{id}.eml")),
            )
            .await;
        }
    }
    Ok(())
}
