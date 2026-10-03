//! Versioned publication and idempotent entry collection across real engines.
use super::forms::FormDefinition;
use crate::{
    App,
    error::{Error, Result},
    now,
    schema::{Definition, Field, Registry},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedForm {
    pub form: FormDefinition,
    #[serde(default)]
    pub subscription_purpose: Option<String>,
    pub groups: BTreeMap<String, BTreeMap<String, Field>>,
}
impl PublishedForm {
    pub fn common(&self) -> Definition {
        Definition {
            groups: self.groups.clone(),
            ..Definition::default()
        }
    }
}
use sqlx::Row;

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS business_forms(id TEXT PRIMARY KEY,owner_id TEXT NOT NULL REFERENCES users(id),draft TEXT NOT NULL,live TEXT NOT NULL,version BIGINT NOT NULL CHECK(version>0),published_version BIGINT NOT NULL CHECK(published_version>=0),updated_at BIGINT NOT NULL,entry_count BIGINT NOT NULL DEFAULT 0 CHECK(entry_count>=0));
CREATE INDEX IF NOT EXISTS business_forms_updated ON business_forms(updated_at DESC,id DESC);
CREATE TABLE IF NOT EXISTS form_publications(form_id TEXT NOT NULL REFERENCES business_forms(id),version BIGINT NOT NULL,definition TEXT NOT NULL,created_at BIGINT NOT NULL,PRIMARY KEY(form_id,version));
CREATE TABLE IF NOT EXISTS form_entries(id TEXT PRIMARY KEY,form_id TEXT NOT NULL REFERENCES business_forms(id),request_key TEXT NOT NULL,request_hash TEXT NOT NULL,form_version BIGINT NOT NULL,values_json TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(form_id,request_key),FOREIGN KEY(form_id,form_version) REFERENCES form_publications(form_id,version));
CREATE INDEX IF NOT EXISTS form_entries_recent ON form_entries(form_id,created_at DESC,id DESC);
"#;

pub async fn create(app: &App, owner: &str, definition: &FormDefinition) -> Result<String> {
    let _guard = app.mutations.lock().await;
    definition.validate(&Registry::load(app).await?.common)?;
    let id = uuid::Uuid::new_v4().to_string();
    let raw = serde_json::to_string(definition)
        .map_err(|_| Error::invalid("Invalid form definition."))?;
    sqlx::query("INSERT INTO business_forms(id,owner_id,draft,live,version,published_version,updated_at) VALUES($1,$2,$3,'',1,0,$4)")
        .bind(&id).bind(owner).bind(raw).bind(now()).execute(&app.db.pool).await?;
    Ok(id)
}
pub async fn save(
    app: &App,
    id: &str,
    version: i64,
    definition: &FormDefinition,
    publish: bool,
) -> Result<()> {
    let _guard = app.mutations.lock().await;
    let common = Registry::load(app).await?.common;
    definition.validate(&common)?;
    let raw = serde_json::to_string(definition)
        .map_err(|_| Error::invalid("Invalid form definition."))?;
    let subscription_purpose = if let Some(action) = &definition.subscription {
        let row = sqlx::query("SELECT purpose,policy FROM audience_lists WHERE id=$1")
            .bind(&action.list)
            .fetch_optional(&app.db.pool)
            .await?
            .ok_or_else(|| Error::invalid("Select an existing audience list."))?;
        if row.get::<String, _>("policy") != action.policy {
            return Err(Error::conflict());
        }
        Some(row.get::<String, _>("purpose"))
    } else {
        None
    };
    let live = serde_json::to_string(&PublishedForm {
        subscription_purpose,
        form: definition.clone(),
        groups: snapshot_groups(definition, &common)?,
    })
    .map_err(|_| Error::invalid("Invalid published form."))?;
    // One atomic compare-and-swap publishes the same validated snapshot that is saved.
    let sql = if publish {
        "UPDATE business_forms SET draft=$1,live=$5,published_version=version+1,version=version+1,updated_at=$2 WHERE id=$3 AND version=$4"
    } else {
        "UPDATE business_forms SET draft=$1,version=version+1,updated_at=$2 WHERE id=$3 AND version=$4"
    };
    let mut tx = app.db.pool.begin().await?;
    let query = sqlx::query(sql)
        .bind(raw)
        .bind(now())
        .bind(id)
        .bind(version);
    let query = if publish { query.bind(&live) } else { query };
    if query.execute(&mut *tx).await?.rows_affected() != 1 {
        return Err(Error::conflict());
    }
    if publish {
        sqlx::query("INSERT INTO form_publications(form_id,version,definition,created_at) VALUES($1,$2,$3,$4)").bind(id).bind(version+1).bind(live).bind(now()).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn submit(app: &App, id: &str, version: i64, key: &str, input: &Value) -> Result<String> {
    if uuid::Uuid::parse_str(key).is_err() || input.to_string().len() > 128 * 1024 {
        return Err(Error::invalid(
            "Use a submission identifier and bounded form values.",
        ));
    }
    let mut tx = app.db.pool.begin().await?;
    // Obtain the per-form write lock before reads, avoiding SQLite deferred-read
    // upgrade failures and serializing publication with submission validation.
    let row = sqlx::query("UPDATE business_forms SET updated_at=updated_at WHERE id=$1 AND published_version>0 RETURNING live,published_version,entry_count")
        .bind(id).fetch_optional(&mut *tx).await?.ok_or_else(Error::not_found)?;
    let raw: String = row.get("live");
    let hash = crate::auth::digest(format!("{version}:{}", input).as_bytes());
    if let Some(existing) =
        sqlx::query("SELECT id,request_hash FROM form_entries WHERE form_id=$1 AND request_key=$2")
            .bind(id)
            .bind(key)
            .fetch_optional(&mut *tx)
            .await?
    {
        if existing.get::<String, _>("request_hash") != hash {
            return Err(Error::conflict());
        }
        let entry = existing.get("id");
        tx.commit().await?;
        return Ok(entry);
    }
    if row.get::<i64, _>("published_version") != version {
        return Err(Error::conflict());
    }
    let definition: PublishedForm = serde_json::from_str(&raw)
        .map_err(|_| Error::invalid("The stored form requires repair."))?;
    if row.get::<i64, _>("entry_count") >= definition.form.max_entries {
        return Err(Error(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "This form has reached its response limit. Contact the site owner.",
        ));
    }
    let mut values = definition
        .form
        .evaluate(&definition.common(), input, false)?;
    let entry = uuid::Uuid::new_v4().to_string();
    for field in &definition.form.fields {
        if matches!(field.widget, Some(super::forms::Widget::Upload)) {
            if let Some(capability) = values.get(&field.name).and_then(Value::as_str) {
                let attachment = super::attachments::attach(
                    &mut tx,
                    id,
                    version,
                    &field.name,
                    capability,
                    &entry,
                )
                .await?;
                values[&field.name] = Value::String(attachment);
            }
        }
    }
    sqlx::query("INSERT INTO form_entries(id,form_id,request_key,request_hash,form_version,values_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(&entry).bind(id).bind(key).bind(hash).bind(version).bind(values.to_string()).bind(now()).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO form_entry_search(entry_id,search_text) VALUES($1,$2)")
        .bind(&entry)
        .bind(values.to_string())
        .execute(&mut *tx)
        .await?;
    if let Some(action) = &definition.form.subscription {
        super::audience::subscribe(app, &mut tx, action, &values).await?;
    }
    sqlx::query("UPDATE business_forms SET entry_count=entry_count+1 WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!(event="form_entry_recorded", form_id=id, entry_id=%entry, form_version=version);
    Ok(entry)
}

fn snapshot_groups(
    form: &FormDefinition,
    common: &Definition,
) -> Result<BTreeMap<String, BTreeMap<String, Field>>> {
    fn collect(
        field: &Field,
        common: &Definition,
        groups: &mut BTreeMap<String, BTreeMap<String, Field>>,
    ) -> Result<()> {
        for name in std::iter::once(&field.group)
            .chain(field.variants.values())
            .filter(|name| !name.is_empty())
        {
            if groups.contains_key(name) {
                continue;
            }
            let source = common
                .groups
                .get(name)
                .ok_or(Error::invalid("A reusable form group is missing."))?;
            groups.insert(name.clone(), source.clone());
            for child in source.values() {
                collect(child, common, groups)?;
            }
        }
        for child in field.fields.values() {
            collect(child, common, groups)?;
        }
        Ok(())
    }
    let mut groups = BTreeMap::new();
    for field in &form.fields {
        collect(&field.schema, common, &mut groups)?;
    }
    Ok(groups)
}
