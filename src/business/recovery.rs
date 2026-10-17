//! Explicit publication-bound consent. Private drafts never imply owner-review authority.
use super::{
    forms::{FormDefinition, Widget},
    mail,
    store::PublishedForm,
};
use crate::{
    App, auth,
    error::{Error, Result},
    now,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Any, Row, Transaction};

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS form_recovery_policies(form_id TEXT PRIMARY KEY REFERENCES business_forms(id),form_version BIGINT NOT NULL,policy_id TEXT NOT NULL,definition TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS form_recoveries(id TEXT PRIMARY KEY,form_id TEXT NOT NULL REFERENCES business_forms(id),form_version BIGINT NOT NULL,policy_id TEXT NOT NULL,token_hash TEXT NOT NULL UNIQUE,confirmation_hash TEXT NOT NULL UNIQUE,request_key TEXT NOT NULL,email TEXT NOT NULL,policy_definition TEXT NOT NULL,values_json TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN ('pending','confirmed')),created_at BIGINT NOT NULL,expires_at BIGINT NOT NULL,UNIQUE(form_id,request_key),FOREIGN KEY(form_id,form_version) REFERENCES form_publications(form_id,version));
CREATE INDEX IF NOT EXISTS form_recovery_email ON form_recoveries(email,state,id);
CREATE INDEX IF NOT EXISTS form_recovery_expiry ON form_recoveries(expires_at,id);
CREATE INDEX IF NOT EXISTS form_recovery_review ON form_recoveries(form_id,state,created_at DESC,id DESC);
"#;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub purpose: String,
    pub fields: Vec<String>,
}
impl Policy {
    pub fn validate(&self, form: &FormDefinition) -> Result<()> {
        if self.purpose.trim().is_empty()
            || self.purpose.len() > 1000
            || self.fields.is_empty()
            || self.fields.len() > 16
        {
            return Err(Error::invalid(
                "Recovery needs a specific purpose and 1..16 safe fields.",
            ));
        }
        let mut names = std::collections::HashSet::new();
        for name in &self.fields {
            let valid = form.fields.iter().any(|f| {
                &f.name == name
                    && ["string", "number", "boolean"].contains(&f.schema.kind.as_str())
                    && f.visible_when.is_none()
                    && f.calculation.is_none()
                    && !matches!(
                        f.widget,
                        Some(
                            Widget::Upload
                                | Widget::Signature { .. }
                                | Widget::Acknowledgment { .. }
                        )
                    )
            });
            if !names.insert(name) || !valid {
                return Err(Error::invalid(
                    "Choose unique unconditional scalar fields; attachments, signatures, acknowledgments and calculated values cannot be retained for recovery.",
                ));
            }
        }
        if form.registration.is_some() || form.subscription.is_some() {
            return Err(Error::invalid(
                "Account and subscription forms require their separate consent workflows.",
            ));
        }
        Ok(())
    }
}
#[derive(Serialize)]
pub struct PublicPolicy {
    pub id: String,
    pub version: i64,
    pub policy: Policy,
}
pub async fn policy(app: &App, id: &str) -> Result<Option<PublicPolicy>> {
    let r=sqlx::query("SELECT p.policy_id,p.form_version,p.definition FROM form_recovery_policies p JOIN business_forms f ON f.id=p.form_id AND f.published_version=p.form_version WHERE p.form_id=$1").bind(id).fetch_optional(&app.db.pool).await?;
    r.map(|r| {
        Ok(PublicPolicy {
            id: r.get("policy_id"),
            version: r.get("form_version"),
            policy: serde_json::from_str(&r.get::<String, _>("definition"))
                .map_err(|_| Error::invalid("Recovery policy needs repair."))?,
        })
    })
    .transpose()
}
pub async fn publish_policy(
    app: &App,
    id: &str,
    version: i64,
    policy: Option<Policy>,
) -> Result<()> {
    let mut tx = app.db.pool.begin().await?;
    let raw:String=sqlx::query_scalar("UPDATE business_forms SET updated_at=updated_at WHERE id=$1 AND published_version=$2 AND published_version>0 RETURNING live").bind(id).bind(version).fetch_optional(&mut *tx).await?.ok_or_else(Error::conflict)?;
    let form: PublishedForm =
        serde_json::from_str(&raw).map_err(|_| Error::invalid("Stored form needs repair."))?;
    if let Some(policy) = policy {
        policy.validate(&form.form)?;
        sqlx::query("INSERT INTO form_recovery_policies(form_id,form_version,policy_id,definition) VALUES($1,$2,$3,$4) ON CONFLICT(form_id) DO UPDATE SET form_version=$2,policy_id=$3,definition=$4").bind(id).bind(version).bind(uuid::Uuid::new_v4().to_string()).bind(serde_json::to_string(&policy).unwrap()).execute(&mut *tx).await?;
    } else {
        sqlx::query("DELETE FROM form_recovery_policies WHERE form_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub token: String,
    pub version: i64,
    pub policy_id: String,
    pub request_key: String,
    pub email: String,
    pub values: Value,
    pub consent: bool,
    #[serde(default)]
    pub spam: crate::operations::spam::Proof,
}
#[derive(Serialize)]
pub struct Saved {
    pub id: String,
    pub state: String,
    pub version: i64,
    pub values: Value,
    pub email: String,
    pub policy: Policy,
    pub request_key: String,
    pub expires_at: i64,
}
fn capability(token: &str) -> Result<String> {
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::invalid("Invalid private recovery capability."));
    }
    Ok(auth::digest(token.as_bytes()))
}
pub async fn request(app: &App, id: &str, input: Request) -> Result<Saved> {
    if !app.config.mail.enabled {
        return Err(Error::invalid(
            "Response recovery mail is disabled; use a private draft instead.",
        ));
    }
    if !input.consent || input.values.to_string().len() > 16 * 1024 {
        return Err(Error::invalid(
            "Explicit recovery consent and bounded selected values are required.",
        ));
    }
    crate::membership::uuid(&input.request_key)?;
    let email = mail::email(&input.email)?;
    let token = capability(&input.token)?;
    let mut tx = app.db.pool.begin().await?;
    let raw:String=sqlx::query_scalar("UPDATE business_forms SET updated_at=updated_at WHERE id=$1 AND published_version=$2 AND published_version>0 RETURNING live").bind(id).bind(input.version).fetch_optional(&mut *tx).await?.ok_or_else(Error::conflict)?;
    let row=sqlx::query("SELECT definition,policy_id FROM form_recovery_policies WHERE form_id=$1 AND form_version=$2").bind(id).bind(input.version).fetch_optional(&mut *tx).await?.ok_or_else(Error::conflict)?;
    if row.get::<String, _>("policy_id") != input.policy_id {
        return Err(Error::conflict());
    }
    let policy: Policy = serde_json::from_str(&row.get::<String, _>("definition"))
        .map_err(|_| Error::invalid("Recovery policy needs repair."))?;
    let publication: PublishedForm =
        serde_json::from_str(&raw).map_err(|_| Error::invalid("Stored form needs repair."))?;
    policy.validate(&publication.form)?;
    let supplied = input
        .values
        .as_object()
        .ok_or_else(|| Error::invalid("Use selected recovery values."))?;
    if supplied.keys().any(|k| !policy.fields.contains(k)) {
        return Err(Error::invalid(
            "Only explicitly selected safe fields may be shared for recovery.",
        ));
    }
    let values = publication
        .form
        .evaluate(&publication.common(), &input.values, true)?;
    // No derived value or unrelated hidden field becomes owner-visible implicitly.
    let values: Value = json!(
        values
            .as_object()
            .ok_or_else(|| Error::invalid("Invalid partial values."))?
            .iter()
            .filter(|(k, _)| policy.fields.contains(k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<serde_json::Map<_, _>>()
    );
    let encoded = values.to_string();
    super::quotas::release(&mut tx, "drafts", 0, 0).await?;
    super::quotas::release(&mut tx, "mail", 0, 0).await?;
    if let Some(row)=sqlx::query("SELECT id,form_version,policy_id,email,request_key,values_json FROM form_recoveries WHERE form_id=$1 AND token_hash=$2 AND expires_at>$3").bind(id).bind(&token).bind(now()).fetch_optional(&mut *tx).await?{
        if row.get::<i64,_>("form_version")!=input.version || row.get::<String,_>("policy_id")!=input.policy_id || row.get::<String,_>("email")!=email || row.get::<String,_>("request_key")!=input.request_key || row.get::<String,_>("values_json")!=encoded {return Err(Error::conflict());}
        tx.commit().await?;return load(app,id,&input.token).await;
    }
    let completed: Option<String> =
        sqlx::query_scalar("SELECT id FROM form_entries WHERE form_id=$1 AND request_key=$2")
            .bind(id)
            .bind(&input.request_key)
            .fetch_optional(&mut *tx)
            .await?;
    if completed.is_some() {
        return Err(Error::invalid(
            "This response was already received; recovery is unnecessary.",
        ));
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM form_recoveries WHERE form_id=$1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if count >= 1000 {
        return Err(Error::invalid("This form's recovery storage is full."));
    }
    super::quotas::reserve(app, &mut tx, "drafts", 1, encoded.len() as i64).await?;
    let record = uuid::Uuid::new_v4().to_string();
    let confirmation = auth::random_token();
    let confirmation_hash = auth::digest(confirmation.as_bytes());
    let created = now();
    sqlx::query("INSERT INTO form_recoveries(id,form_id,form_version,policy_id,token_hash,confirmation_hash,request_key,email,policy_definition,values_json,state,created_at,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'pending',$11,$12)").bind(&record).bind(id).bind(input.version).bind(&input.policy_id).bind(token).bind(confirmation_hash).bind(&input.request_key).bind(&email).bind(serde_json::to_string(&policy).unwrap()).bind(encoded).bind(created).bind(created+7*86400).execute(&mut *tx).await?;
    let link = format!(
        "{}/forms/{id}#recover={}&confirm={confirmation}",
        app.config.origin(),
        input.token
    );
    let plain = format!(
        "You explicitly requested partial-response recovery for this purpose: {}\nConfirm the address and permission on the live form: {link}\nNo response has been submitted. Ignore this message if you did not request it. This request expires in seven days. No marketing subscription is created.",
        policy.purpose
    );
    mail::enqueue(
        app,
        &mut tx,
        mail::MessageInput {
            dedupe: &format!("recovery-confirm:{record}"),
            contact: "",
            list: "",
            kind: "notification",
            recipient: &email,
            subject: "Confirm your response recovery request",
            html: "",
            plain: &plain,
        },
    )
    .await?;
    tx.commit().await?;
    load(app, id, &input.token).await
}
pub async fn load(app: &App, id: &str, token: &str) -> Result<Saved> {
    let r=sqlx::query("SELECT id,state,form_version,values_json,email,policy_definition,request_key,expires_at FROM form_recoveries WHERE form_id=$1 AND token_hash=$2 AND expires_at>$3").bind(id).bind(capability(token)?).bind(now()).fetch_optional(&app.db.pool).await?.ok_or_else(Error::not_found)?;
    Ok(Saved {
        id: r.get("id"),
        state: r.get("state"),
        version: r.get("form_version"),
        values: serde_json::from_str(&r.get::<String, _>("values_json"))
            .map_err(|_| Error::invalid("Stored recovery needs repair."))?,
        email: r.get("email"),
        policy: serde_json::from_str(&r.get::<String, _>("policy_definition"))
            .map_err(|_| Error::invalid("Recovery policy needs repair."))?,
        request_key: r.get("request_key"),
        expires_at: r.get("expires_at"),
    })
}
pub async fn confirm(app: &App, id: &str, token: &str, confirmation: &str) -> Result<()> {
    let token_hash = capability(token)?;
    let confirmation_hash = capability(confirmation)?;
    let mut tx = app.db.pool.begin().await?;
    sqlx::query("UPDATE business_forms SET updated_at=updated_at WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    super::quotas::release(&mut tx, "mail", 0, 0).await?;
    let row=sqlx::query("SELECT r.id,r.email,r.state,p.definition FROM form_recoveries r JOIN business_forms f ON f.id=r.form_id AND f.published_version=r.form_version JOIN form_recovery_policies p ON p.form_id=r.form_id AND p.form_version=r.form_version AND p.policy_id=r.policy_id WHERE r.form_id=$1 AND r.token_hash=$2 AND r.confirmation_hash=$3 AND r.expires_at>$4").bind(id).bind(token_hash).bind(confirmation_hash).bind(now()).fetch_optional(&mut *tx).await?.ok_or_else(Error::not_found)?;
    let record: String = row.get("id");
    let policy: Policy = serde_json::from_str(&row.get::<String, _>("definition"))
        .map_err(|_| Error::invalid("Recovery policy needs repair."))?;
    sqlx::query("UPDATE form_recoveries SET state='confirmed' WHERE id=$1")
        .bind(&record)
        .execute(&mut *tx)
        .await?;
    let plain = format!(
        "One response recovery reminder, requested for: {}\nYour partial response has not been submitted. Review, continue or withdraw/delete it at {}/forms/{id}#recover={token}\nNo marketing subscription is created. The original request expires seven days after creation.",
        policy.purpose,
        app.config.origin()
    );
    let job = mail::enqueue(
        app,
        &mut tx,
        mail::MessageInput {
            dedupe: &format!("recovery-reminder:{record}"),
            contact: "",
            list: "",
            kind: "notification",
            recipient: &row.get::<String, _>("email"),
            subject: "Your requested response recovery reminder",
            html: "",
            plain: &plain,
        },
    )
    .await?;
    if row.get::<String, _>("state") == "pending" {
        sqlx::query("UPDATE mail_jobs SET next_at=$1 WHERE id=$2 AND state='pending'")
            .bind(now() + 86400)
            .bind(job)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}
pub async fn remove(app: &App, id: &str, token: &str) -> Result<()> {
    let mut tx = app.db.pool.begin().await?;
    sqlx::query("UPDATE business_forms SET updated_at=updated_at WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    super::quotas::release(&mut tx, "drafts", 0, 0).await?;
    let rows = sqlx::query(
        "DELETE FROM form_recoveries WHERE form_id=$1 AND token_hash=$2 RETURNING id,values_json",
    )
    .bind(id)
    .bind(capability(token)?)
    .fetch_all(&mut *tx)
    .await?;
    release_removed(&mut tx, &rows).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn complete(tx: &mut Transaction<'_, Any>, id: &str, key: &str) -> Result<()> {
    super::quotas::release(tx, "drafts", 0, 0).await?;
    let rows = sqlx::query(
        "DELETE FROM form_recoveries WHERE form_id=$1 AND request_key=$2 RETURNING id,values_json",
    )
    .bind(id)
    .bind(key)
    .fetch_all(&mut **tx)
    .await?;
    release_removed(tx, &rows).await
}
async fn release_removed(tx: &mut Transaction<'_, Any>, rows: &[sqlx::any::AnyRow]) -> Result<()> {
    super::quotas::release(
        tx,
        "drafts",
        rows.len() as i64,
        rows.iter()
            .map(|r| r.get::<String, _>("values_json").len() as i64)
            .sum(),
    )
    .await?;
    for row in rows {
        for prefix in ["recovery-confirm:", "recovery-reminder:"] {
            sqlx::query("UPDATE mail_jobs SET state='cancelled' WHERE dedupe=$1 AND state IN ('pending','retry')").bind(format!("{prefix}{}",row.get::<String,_>("id"))).execute(&mut **tx).await?;
        }
    }
    Ok(())
}
/// Re-check after lease and immediately before delivery through the existing mail worker.
pub async fn mail_permitted(app: &App, dedupe: &str) -> Result<Option<bool>> {
    let (id, confirmation) = if let Some(id) = dedupe.strip_prefix("recovery-confirm:") {
        (id, true)
    } else if let Some(id) = dedupe.strip_prefix("recovery-reminder:") {
        (id, false)
    } else {
        return Ok(None);
    };
    let state = if confirmation { "pending" } else { "confirmed" };
    let permitted:Option<i64>=sqlx::query_scalar("SELECT 1 FROM form_recoveries r JOIN business_forms f ON f.id=r.form_id AND f.published_version=r.form_version JOIN form_recovery_policies p ON p.form_id=r.form_id AND p.form_version=r.form_version AND p.policy_id=r.policy_id WHERE r.id=$1 AND r.state=$2 AND r.expires_at>$3 AND EXISTS(SELECT 1 FROM mail_jobs m WHERE m.dedupe=$4 AND m.recipient=r.email AND m.kind='notification') AND NOT EXISTS(SELECT 1 FROM form_entries e WHERE e.form_id=r.form_id AND e.request_key=r.request_key)").bind(id).bind(state).bind(now()).bind(dedupe).fetch_optional(&app.db.pool).await?;
    Ok(Some(permitted.is_some()))
}
pub async fn cleanup(app: &App) -> Result<()> {
    let mut tx = app.db.pool.begin().await?;
    super::quotas::release(&mut tx, "drafts", 0, 0).await?;
    let rows=sqlx::query("DELETE FROM form_recoveries WHERE id IN (SELECT id FROM form_recoveries WHERE expires_at<=$1 ORDER BY expires_at,id LIMIT 50) RETURNING id,values_json").bind(now()).fetch_all(&mut *tx).await?;
    release_removed(&mut tx, &rows).await?;
    tx.commit().await?;
    Ok(())
}
