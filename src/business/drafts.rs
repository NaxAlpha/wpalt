//! Expiring, form-scoped recovery capabilities; partial drafts cannot trigger actions.
use crate::{
    App, auth,
    error::{Error, Result},
    now,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS form_drafts(form_id TEXT NOT NULL REFERENCES business_forms(id),token_hash TEXT NOT NULL,form_version BIGINT NOT NULL,revision BIGINT NOT NULL,values_json TEXT NOT NULL,expires_at BIGINT NOT NULL,PRIMARY KEY(form_id,token_hash));
CREATE INDEX IF NOT EXISTS form_draft_expiry ON form_drafts(expires_at);
"#;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Save {
    pub token: String,
    pub version: i64,
    pub revision: i64,
    pub values: Value,
}
#[derive(Serialize)]
pub struct Draft {
    pub version: i64,
    pub revision: i64,
    pub values: Value,
    pub expires_at: i64,
}
fn hash(token: &str) -> Result<String> {
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::invalid("Invalid recovery capability."));
    }
    Ok(auth::digest(token.as_bytes()))
}
pub async fn save(app: &App, id: &str, input: Save) -> Result<Draft> {
    let token = hash(&input.token)?;
    if input.values.to_string().len() > 128 * 1024 {
        return Err(Error::invalid("Draft exceeds 128 KiB."));
    }
    let mut tx = app.db.pool.begin().await?;
    let row=sqlx::query("UPDATE business_forms SET updated_at=updated_at WHERE id=$1 AND published_version=$2 RETURNING live").bind(id).bind(input.version).fetch_optional(&mut *tx).await?.ok_or_else(Error::conflict)?;
    let form: super::store::PublishedForm = serde_json::from_str(&row.get::<String, _>("live"))
        .map_err(|_| Error::invalid("Stored publication needs repair."))?;
    let values = form.form.evaluate(&form.common(), &input.values, true)?;
    let encoded = values.to_string();
    super::quotas::release(&mut tx, "drafts", 0, 0).await?;
    let expired = sqlx::query(
        "DELETE FROM form_drafts WHERE form_id=$1 AND expires_at<=$2 RETURNING values_json",
    )
    .bind(id)
    .bind(now())
    .fetch_all(&mut *tx)
    .await?;
    let bytes = expired
        .iter()
        .map(|r| r.get::<String, _>("values_json").len() as i64)
        .sum();
    super::quotas::release(&mut tx, "drafts", expired.len() as i64, bytes).await?;
    if input.revision == 0 {
        if let Some(existing)=sqlx::query("SELECT form_version,revision,values_json FROM form_drafts WHERE form_id=$1 AND token_hash=$2").bind(id).bind(&token).fetch_optional(&mut *tx).await? {
   if existing.get::<i64,_>("form_version")!=input.version || existing.get::<i64,_>("revision")!=1 || existing.get::<String,_>("values_json")!=encoded{return Err(Error::conflict());}tx.commit().await?;return load(app,id,&input.token).await;
 }

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM form_drafts WHERE form_id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        if count >= 1000 {
            return Err(Error::invalid("This form's recovery storage is full."));
        }
        super::quotas::reserve(app, &mut tx, "drafts", 1, encoded.clone().len() as i64).await?;
        sqlx::query("INSERT INTO form_drafts(form_id,token_hash,form_version,revision,values_json,expires_at) VALUES($1,$2,$3,1,$4,$5)").bind(id).bind(&token).bind(input.version).bind(encoded.clone()).bind(now()+7*86400).execute(&mut *tx).await?;
    } else {
        let previous:String=sqlx::query_scalar("SELECT values_json FROM form_drafts WHERE form_id=$1 AND token_hash=$2 AND revision=$3 AND expires_at>$4").bind(id).bind(&token).bind(input.revision).bind(now()).fetch_optional(&mut *tx).await?.ok_or_else(Error::conflict)?;
        super::quotas::reserve(
            app,
            &mut tx,
            "drafts",
            0,
            encoded.clone().len() as i64 - previous.len() as i64,
        )
        .await?;
        if sqlx::query("UPDATE form_drafts SET values_json=$1,revision=revision+1 WHERE form_id=$2 AND token_hash=$3 AND revision=$4 AND form_version=$5 AND expires_at>$6").bind(encoded.clone()).bind(id).bind(&token).bind(input.revision).bind(input.version).bind(now()).execute(&mut *tx).await?.rows_affected()!=1{return Err(Error::conflict());}
    }
    tx.commit().await?;
    load(app, id, &input.token).await
}
pub async fn load(app: &App, id: &str, token: &str) -> Result<Draft> {
    let row=sqlx::query("SELECT form_version,revision,values_json,expires_at FROM form_drafts WHERE form_id=$1 AND token_hash=$2 AND expires_at>$3").bind(id).bind(hash(token)?).bind(now()).fetch_optional(&app.db.pool).await?.ok_or_else(Error::not_found)?;
    Ok(Draft {
        version: row.get("form_version"),
        revision: row.get("revision"),
        values: serde_json::from_str(&row.get::<String, _>("values_json"))
            .map_err(|_| Error::invalid("Stored draft needs repair."))?,
        expires_at: row.get("expires_at"),
    })
}
