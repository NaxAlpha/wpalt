//! One restricted field grammar shared by authoring, options and theme bindings.
use crate::{
    App,
    error::{Error, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::BTreeMap;

fn item_limit() -> usize {
    20
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub kind: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub fields: BTreeMap<String, Field>,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub variants: BTreeMap<String, String>,
    #[serde(default = "item_limit")]
    pub max_items: usize,
}
impl Field {
    pub fn primitive(kind: &str) -> Self {
        serde_json::from_value(json!({"kind":kind})).unwrap()
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    #[serde(default)]
    pub fields: BTreeMap<String, Field>,
    #[serde(default)]
    pub groups: BTreeMap<String, BTreeMap<String, Field>>,
    #[serde(default)]
    pub options: BTreeMap<String, Field>,
}
impl Definition {
    pub fn initial() -> Self {
        Self {
            fields: [
                ("subtitle", "string"),
                ("reading_minutes", "number"),
                ("featured", "boolean"),
            ]
            .into_iter()
            .map(|(name, kind)| (name.into(), Field::primitive(kind)))
            .collect(),
            ..Self::default()
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub label: String,
    #[serde(default)]
    pub fields: BTreeMap<String, Field>,
    #[serde(default)]
    pub taxonomies: BTreeMap<String, String>,
}
impl Model {
    pub fn initial(label: &str) -> Self {
        Self {
            label: label.into(),
            fields: BTreeMap::new(),
            taxonomies: [
                ("category".into(), "Categories".into()),
                ("tag".into(), "Tags".into()),
            ]
            .into(),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Registry {
    pub common: Definition,
    pub models: BTreeMap<String, Model>,
}
pub fn identifier(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !["__proto__", "constructor", "prototype"].contains(&name)
        && name.as_bytes()[0].is_ascii_lowercase()
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
}
impl Registry {
    pub async fn load(app: &App) -> Result<Self> {
        // One statement gives a coherent schema snapshot on both engines and
        // avoids loading unrelated site metadata or a second round trip.
        let rows=sqlx::query("SELECT '' AS id,field_schema AS definition FROM settings WHERE id=1 UNION ALL SELECT id,definition FROM content_models").fetch_all(&app.db.pool).await?;
        let mut common = None;
        let mut models = BTreeMap::new();
        for row in rows {
            let id: String = row.get("id");
            let raw: String = row.get("definition");
            if id.is_empty() {
                common = Some(
                    serde_json::from_str(&raw)
                        .map_err(|_| Error::invalid("Site field definitions are invalid."))?,
                );
            } else {
                models.insert(
                    id,
                    serde_json::from_str(&raw)
                        .map_err(|_| Error::invalid("A content model is invalid."))?,
                );
            }
        }
        let common = common.ok_or(Error::invalid("Site is not initialized."))?;
        Ok(Self { common, models })
    }
    pub fn fields_for(&self, kind: &str) -> Result<BTreeMap<String, Field>> {
        let model = self
            .models
            .get(kind)
            .ok_or(Error::invalid("Choose an installed content model."))?;
        let mut fields = self.common.fields.clone();
        for (name, field) in &model.fields {
            if fields.insert(name.clone(), field.clone()).is_some() {
                return Err(Error::invalid(
                    "Model fields must not override shared fields.",
                ));
            }
        }
        Ok(fields)
    }
    pub fn validate(&self) -> Result<()> {
        if self.models.len() > 32 || self.common.groups.len() > 32 {
            return Err(Error::invalid(
                "Use at most 32 content models and reusable field groups.",
            ));
        }
        let mut budget = 2048;
        self.check_fields(&self.common.fields, 0, &mut Vec::new(), &mut budget)?;
        self.check_fields(&self.common.options, 0, &mut Vec::new(), &mut budget)?;
        for (name, fields) in &self.common.groups {
            if !identifier(name) {
                return Err(Error::invalid("Use safe lowercase group identifiers."));
            }
            self.check_fields(fields, 0, &mut vec![name.clone()], &mut budget)?;
        }
        for (id, model) in &self.models {
            if ["home", "search", "content", "listing"].contains(&id.as_str()) {
                return Err(Error::invalid(
                    "Model names home, search, content and listing are reserved for templates.",
                ));
            }
            if !identifier(id)
                || model.label.trim().is_empty()
                || model.label.len() > 100
                || model.taxonomies.len() > 16
                || model
                    .taxonomies
                    .iter()
                    .any(|(k, v)| !identifier(k) || v.trim().is_empty() || v.len() > 100)
            {
                return Err(Error::invalid(
                    "Models require safe identifiers, labels and up to 16 named taxonomies.",
                ));
            }
            let fields = self.fields_for(id)?;
            self.check_fields(&fields, 0, &mut Vec::new(), &mut budget)?;
        }
        Ok(())
    }
    fn check_fields(
        &self,
        fields: &BTreeMap<String, Field>,
        depth: usize,
        ancestors: &mut Vec<String>,
        budget: &mut usize,
    ) -> Result<()> {
        if depth > 8 || fields.len() > 32 {
            return Err(Error::invalid(
                "Field definitions exceed the supported depth or width.",
            ));
        }
        for (name, field) in fields {
            if *budget == 0 {
                return Err(Error::invalid("Too many expanded field definitions."));
            }
            *budget -= 1;
            if !identifier(name)
                || field.label.len() > 100
                || !(1..=50).contains(&field.max_items)
                || ![
                    "string",
                    "number",
                    "boolean",
                    "media",
                    "relationship",
                    "object",
                    "group",
                    "repeater",
                    "gallery",
                    "flexible",
                ]
                .contains(&field.kind.as_str())
            {
                return Err(Error::invalid(
                    "A field identifier, kind, label or item limit is invalid.",
                ));
            }
            if field.kind == "relationship" && !self.models.contains_key(&field.target) {
                return Err(Error::invalid(
                    "A relationship must name an installed target model.",
                ));
            }
            if !field.group.is_empty() {
                if !["group", "object", "repeater"].contains(&field.kind.as_str())
                    || !field.fields.is_empty()
                {
                    return Err(Error::invalid(
                        "Only groups, objects and repeaters may reference a reusable group.",
                    ));
                }
                self.check_group(&field.group, depth, ancestors, budget)?;
            } else if ["group", "object", "repeater"].contains(&field.kind.as_str()) {
                self.check_fields(&field.fields, depth + 1, ancestors, budget)?;
            } else if !field.fields.is_empty() {
                return Err(Error::invalid(
                    "Nested fields require an object or repeater.",
                ));
            }
            if field.kind == "flexible" {
                if field.variants.is_empty() || field.variants.len() > 16 {
                    return Err(Error::invalid(
                        "Flexible content needs 1–16 named group variants.",
                    ));
                }
                for (variant, group) in &field.variants {
                    if !identifier(variant) {
                        return Err(Error::invalid("Flexible variants need safe identifiers."));
                    }
                    self.check_group(group, depth, ancestors, budget)?;
                }
            } else if !field.variants.is_empty() {
                return Err(Error::invalid(
                    "Variants are only supported by flexible content.",
                ));
            }
        }
        Ok(())
    }
    fn check_group(
        &self,
        name: &str,
        depth: usize,
        ancestors: &mut Vec<String>,
        budget: &mut usize,
    ) -> Result<()> {
        if ancestors.iter().any(|n| n == name) {
            return Err(Error::invalid(
                "Reusable field groups must not form cycles.",
            ));
        }
        let fields = self
            .common
            .groups
            .get(name)
            .ok_or(Error::invalid("A reusable field group is missing."))?;
        ancestors.push(name.into());
        let result = self.check_fields(fields, depth + 1, ancestors, budget);
        ancestors.pop();
        result
    }
    pub fn child_fields<'a>(&'a self, field: &'a Field) -> Result<&'a BTreeMap<String, Field>> {
        if field.group.is_empty() {
            Ok(&field.fields)
        } else {
            self.common
                .groups
                .get(&field.group)
                .ok_or(Error::invalid("A reusable group is missing."))
        }
    }
    pub fn validate_values(&self, fields: &BTreeMap<String, Field>, value: &Value) -> Result<()> {
        self.values(fields, value, 0, &mut 4096)
    }
    fn values(
        &self,
        fields: &BTreeMap<String, Field>,
        value: &Value,
        depth: usize,
        budget: &mut usize,
    ) -> Result<()> {
        if depth > 8 {
            return Err(Error::invalid("Structured values exceed the depth limit."));
        }
        let object = value
            .as_object()
            .ok_or(Error::invalid("Structured values must be an object."))?;
        if object.keys().any(|name| !fields.contains_key(name)) {
            return Err(Error::invalid(
                "A structured field is not defined by its model.",
            ));
        }
        for (name, field) in fields {
            let Some(value) = object.get(name).filter(|v| !v.is_null()) else {
                if field.required {
                    return Err(Error::invalid("A required structured field is missing."));
                }
                continue;
            };
            if *budget == 0 {
                return Err(Error::invalid("Too many structured values."));
            }
            *budget -= 1;
            match field.kind.as_str() {
                "string"
                    if value.as_str().is_some_and(|v| {
                        v.len() <= 8000 && (!field.required || !v.trim().is_empty())
                    }) => {}
                "number" if value.is_number() => {}
                "boolean" if value.is_boolean() => {}
                "media" | "relationship"
                    if value
                        .as_str()
                        .is_some_and(|v| uuid::Uuid::parse_str(v).is_ok()) => {}
                "object" | "group" => {
                    self.values(self.child_fields(field)?, value, depth + 1, budget)?
                }
                "repeater" | "gallery" | "flexible" => {
                    let rows = value
                        .as_array()
                        .ok_or(Error::invalid("Repeaters and galleries require an array."))?;
                    if rows.len() > field.max_items {
                        return Err(Error::invalid(
                            "A repeater or gallery exceeds its item limit.",
                        ));
                    }
                    for row in rows {
                        match field.kind.as_str() {
                            "gallery" => {
                                if row
                                    .as_str()
                                    .is_none_or(|v| uuid::Uuid::parse_str(v).is_err())
                                {
                                    return Err(Error::invalid(
                                        "A gallery must reference uploaded media.",
                                    ));
                                }
                            }
                            "flexible" => {
                                let variant = row.get("type").and_then(Value::as_str).ok_or(
                                    Error::invalid("A flexible section needs a declared type."),
                                )?;
                                let group = field.variants.get(variant).ok_or(Error::invalid(
                                    "A flexible section uses an unknown variant.",
                                ))?;
                                let fields = self.common.groups.get(group).ok_or(
                                    Error::invalid("A flexible section group is missing."),
                                )?;
                                if row.as_object().is_none_or(|o| o.len() != 2) {
                                    return Err(Error::invalid(
                                        "Flexible sections contain only type and values.",
                                    ));
                                }
                                self.values(fields, &row["values"], depth + 1, budget)?;
                            }
                            _ => self.values(self.child_fields(field)?, row, depth + 1, budget)?,
                        }
                    }
                }
                _ => {
                    return Err(Error::invalid(
                        "A structured value does not match its field type.",
                    ));
                }
            }
        }
        Ok(())
    }
    pub fn references(
        &self,
        fields: &BTreeMap<String, Field>,
        value: &Value,
        relations: &mut BTreeMap<String, String>,
        media: &mut Vec<String>,
    ) -> Result<()> {
        for (name, field) in fields {
            let Some(value) = value.get(name).filter(|v| !v.is_null()) else {
                continue;
            };
            match field.kind.as_str() {
                "relationship" => {
                    if let Some(id) = value.as_str()
                        && relations
                            .insert(id.into(), field.target.clone())
                            .is_some_and(|old| old != field.target)
                    {
                        return Err(Error::invalid(
                            "A record cannot represent two different relationship target models.",
                        ));
                    }
                }
                "media" => {
                    if let Some(id) = value.as_str() {
                        media.push(id.into());
                    }
                }
                "gallery" => {
                    if let Some(rows) = value.as_array() {
                        media.extend(rows.iter().filter_map(Value::as_str).map(String::from));
                    }
                }
                "object" | "group" => {
                    self.references(self.child_fields(field)?, value, relations, media)?
                }
                "repeater" => {
                    if let Some(rows) = value.as_array() {
                        for row in rows {
                            self.references(self.child_fields(field)?, row, relations, media)?;
                        }
                    }
                }
                "flexible" => {
                    if let Some(rows) = value.as_array() {
                        for row in rows {
                            if let Some(fields) = row
                                .get("type")
                                .and_then(Value::as_str)
                                .and_then(|v| field.variants.get(v))
                                .and_then(|g| self.common.groups.get(g))
                            {
                                self.references(fields, &row["values"], relations, media)?;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub async fn validate_references(
        &self,
        app: &App,
        fields: &BTreeMap<String, Field>,
        value: &Value,
    ) -> Result<()> {
        let mut relations = BTreeMap::new();
        let mut media = Vec::new();
        self.references(fields, value, &mut relations, &mut media)?;
        media.sort();
        media.dedup();
        if relations.len() + media.len() > 128 {
            return Err(Error::invalid(
                "Use at most 128 distinct relationships and media references per record.",
            ));
        }
        if !relations.is_empty() {
            let mut query =
                sqlx::QueryBuilder::<sqlx::Any>::new("SELECT id,kind FROM posts WHERE id IN (");
            let mut list = query.separated(",");
            for id in relations.keys() {
                list.push_bind(id);
            }
            list.push_unseparated(")");
            let rows = app.db.fetch_builder(&mut query).await?;
            if rows.len() != relations.len()
                || rows.iter().any(|r| {
                    relations.get(&r.get::<String, _>("id")) != Some(&r.get::<String, _>("kind"))
                })
            {
                return Err(Error::invalid(
                    "A relationship target is missing or has the wrong model.",
                ));
            }
        }
        if !media.is_empty() {
            let mut query =
                sqlx::QueryBuilder::<sqlx::Any>::new("SELECT id FROM media WHERE id IN (");
            let mut list = query.separated(",");
            for id in &media {
                list.push_bind(id);
            }
            list.push_unseparated(")");
            if app.db.fetch_builder(&mut query).await?.len() != media.len() {
                return Err(Error::invalid("An uploaded media reference is missing."));
            }
        }
        Ok(())
    }
}

/// Validate persisted values before committing a schema change. No silent data pruning.
async fn preflight(app: &App, registry: &Registry) -> Result<()> {
    use futures_util::TryStreamExt;
    registry.validate()?;
    let mut rows =
        sqlx::query("SELECT kind,fields,published_fields,published_slug FROM posts ORDER BY id")
            .fetch(&app.db.pool);
    let mut count = 0;
    let mut relations = BTreeMap::new();
    let mut media = Vec::new();
    while let Some(row) = rows.try_next().await? {
        count += 1;
        if count > 10000 {
            return Err(Error::invalid(
                "Schema changes above 10,000 records require an explicit offline data migration.",
            ));
        }
        let fields = registry.fields_for(&row.get::<String, _>("kind"))?;
        for column in ["fields", "published_fields"] {
            if column == "published_fields" && row.get::<String, _>("published_slug").is_empty() {
                continue;
            }
            let value: Value =
                serde_json::from_str(&row.get::<String, _>(column)).map_err(|_| {
                    Error::invalid(
                        "Existing structured content needs repair before changing definitions.",
                    )
                })?;
            registry.validate_values(&fields, &value)?;
            registry.references(&fields, &value, &mut relations, &mut media)?;
            if relations.len() > 4096 || media.len() > 8192 {
                return Err(Error::invalid(
                    "Large reference changes require an explicit offline migration.",
                ));
            }
        }
    }
    drop(rows);
    let mut forms = sqlx::query("SELECT draft FROM business_forms ORDER BY id").fetch(&app.db.pool);
    while let Some(row) = forms.try_next().await? {
        count += 1;
        if count > 10000 {
            return Err(Error::invalid(
                "Large schema changes require an explicit offline migration.",
            ));
        }
        let form: crate::business::forms::FormDefinition =
            serde_json::from_str(&row.get::<String, _>("draft"))
                .map_err(|_| Error::invalid("Existing form definitions require repair."))?;
        form.validate(&registry.common)?;
    }
    drop(forms);
    let design = sqlx::query("SELECT draft_options,live_options FROM site_design WHERE id=1")
        .fetch_one(&app.db.pool)
        .await?;
    for column in ["draft_options", "live_options"] {
        let value: Value = serde_json::from_str(&design.get::<String, _>(column))
            .map_err(|_| Error::invalid("Existing shared options need repair."))?;
        registry.validate_values(&registry.common.options, &value)?;
        registry.references(&registry.common.options, &value, &mut relations, &mut media)?;
    }
    let ids: Vec<_> = relations.keys().collect();
    for chunk in ids.chunks(128) {
        let mut q = sqlx::QueryBuilder::<sqlx::Any>::new("SELECT id,kind FROM posts WHERE id IN (");
        let mut list = q.separated(",");
        for id in chunk {
            list.push_bind(*id);
        }
        list.push_unseparated(")");
        let rows = app.db.fetch_builder(&mut q).await?;
        if rows.len() != chunk.len()
            || rows.iter().any(|row| {
                relations.get(&row.get::<String, _>("id")) != Some(&row.get::<String, _>("kind"))
            })
        {
            return Err(Error::invalid(
                "Existing relationship targets conflict with the proposed schema.",
            ));
        }
    }
    media.sort();
    media.dedup();
    for chunk in media.chunks(128) {
        let mut q = sqlx::QueryBuilder::<sqlx::Any>::new("SELECT id FROM media WHERE id IN (");
        let mut list = q.separated(",");
        for id in chunk {
            list.push_bind(id);
        }
        list.push_unseparated(")");
        if app.db.fetch_builder(&mut q).await?.len() != chunk.len() {
            return Err(Error::invalid("Existing media references need repair."));
        }
    }
    let taxonomies=sqlx::query("SELECT DISTINCT p.kind AS model,t.kind AS taxonomy FROM posts p JOIN post_terms pt ON pt.post_id=p.id JOIN terms t ON t.id=pt.term_id UNION SELECT DISTINCT p.kind AS model,t.kind AS taxonomy FROM posts p JOIN published_post_terms pt ON pt.post_id=p.id JOIN terms t ON t.id=pt.term_id").fetch_all(&app.db.pool).await?;
    for row in taxonomies {
        let model: String = row.get("model");
        let taxonomy: String = row.get("taxonomy");
        if !registry
            .models
            .get(&model)
            .is_some_and(|m| m.taxonomies.contains_key(&taxonomy))
        {
            return Err(Error::invalid(
                "A taxonomy with existing terms cannot be removed without a data migration.",
            ));
        }
    }
    crate::theme::preflight(app, registry).await?;
    Ok(())
}
pub async fn save_model(app: &App, id: &str, model: Model, version: i64) -> Result<i64> {
    let _guard = app.mutation().await;
    let actual: Option<i64> = sqlx::query_scalar("SELECT version FROM content_models WHERE id=$1")
        .bind(id)
        .fetch_optional(&app.db.pool)
        .await?;
    if actual.unwrap_or(0) != version {
        return Err(Error::conflict());
    }
    let mut registry = Registry::load(app).await?;
    if (version == 0) == registry.models.contains_key(id) {
        return Err(Error::conflict());
    }
    registry.models.insert(id.into(), model.clone());
    preflight(app, &registry).await?;
    let definition =
        serde_json::to_string(&model).map_err(|_| Error::invalid("Invalid content model."))?;
    if version == 0 {
        sqlx::query("INSERT INTO content_models(id,definition,version) VALUES($1,$2,1)")
            .bind(id)
            .bind(definition)
            .execute(&app.db.pool)
            .await?;
    } else {
        let result = sqlx::query(
            "UPDATE content_models SET definition=$1,version=version+1 WHERE id=$2 AND version=$3",
        )
        .bind(definition)
        .bind(id)
        .bind(version)
        .execute(&app.db.pool)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Error::conflict());
        }
    }
    tracing::info!(event="content_model_saved",model_id=%id,version=version+1);
    Ok(version + 1)
}
pub async fn save_common(app: &App, definition: Definition, version: i64) -> Result<i64> {
    let _guard = app.mutation().await;
    check_design_version(app, version).await?;
    let mut registry = Registry::load(app).await?;
    registry.common = definition.clone();
    preflight(app, &registry).await?;
    let mut tx = app.db.pool.begin().await?;
    let result = sqlx::query("UPDATE site_design SET version=version+1 WHERE id=1 AND version=$1")
        .bind(version)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() != 1 {
        return Err(Error::conflict());
    }
    sqlx::query("UPDATE settings SET field_schema=$1 WHERE id=1")
        .bind(
            serde_json::to_string(&definition)
                .map_err(|_| Error::invalid("Invalid field definitions."))?,
        )
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    app.themes.lock().await.clear();
    tracing::info!(event = "shared_schema_saved", version = version + 1);
    Ok(version + 1)
}
pub async fn save_options(app: &App, values: Value, version: i64, publish: bool) -> Result<i64> {
    let _guard = app.mutation().await;
    check_design_version(app, version).await?;
    let registry = Registry::load(app).await?;
    registry.validate_values(&registry.common.options, &values)?;
    registry
        .validate_references(app, &registry.common.options, &values)
        .await?;
    let data =
        serde_json::to_string(&values).map_err(|_| Error::invalid("Invalid shared options."))?;
    if data.len() > 32 * 1024 {
        return Err(Error::invalid("Shared options exceed the size limit."));
    }
    let result = if publish {
        sqlx::query("UPDATE site_design SET draft_options=$1,live_options=$1,version=version+1,published_version=version+1 WHERE id=1 AND version=$2").bind(data).bind(version).execute(&app.db.pool).await?
    } else {
        sqlx::query(
            "UPDATE site_design SET draft_options=$1,version=version+1 WHERE id=1 AND version=$2",
        )
        .bind(data)
        .bind(version)
        .execute(&app.db.pool)
        .await?
    };
    if result.rows_affected() != 1 {
        return Err(Error::conflict());
    }
    tracing::info!(
        event = "shared_options_saved",
        version = version + 1,
        published = publish
    );
    Ok(version + 1)
}

async fn check_design_version(app: &App, version: i64) -> Result<()> {
    let actual: i64 = sqlx::query_scalar("SELECT version FROM site_design WHERE id=1")
        .fetch_one(&app.db.pool)
        .await?;
    if actual != version {
        return Err(Error::conflict());
    }
    Ok(())
}
