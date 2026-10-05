//! Explicit scalar ACF references -> native shared fields, without PHP execution.
use crate::{
    error::{Error, Result},
    schema::{Definition, Field},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_BYTES: usize = 128 * 1024;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    format: String,
    fields: Vec<Selection>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    source_name: String,
    source_key: String,
    target_name: String,
    kind: String,
}
fn invalid() -> Error {
    Error::invalid("Review the explicit ACF scalar field mapping and source references.")
}
impl Mapping {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err(invalid());
        }
        let mapping: Self = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        mapping.validate()?;
        Ok(mapping)
    }
    pub(crate) fn validate(&self) -> Result<()> {
        if self.format != "wpalt-acf-scalar-map-v1" || self.fields.len() > 32 {
            return Err(invalid());
        }
        let mut names = BTreeSet::new();
        let mut keys = BTreeSet::new();
        let mut targets = BTreeSet::new();
        for field in &self.fields {
            if !crate::schema::identifier(&field.source_name)
                || !crate::schema::identifier(&field.target_name)
                || !field.source_key.starts_with("field_")
                || !(7..=100).contains(&field.source_key.len())
                || !field
                    .source_key
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_')
                || !["string", "number", "boolean"].contains(&field.kind.as_str())
                || !names.insert(&field.source_name)
                || !keys.insert(&field.source_key)
                || !targets.insert(&field.target_name)
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
    pub(crate) fn schema(&self, raw: &str) -> Result<String> {
        self.validate()?;
        let mut definition: Definition = serde_json::from_str(raw).map_err(|_| invalid())?;
        for selected in &self.fields {
            if let Some(existing) = definition.fields.get(&selected.target_name) {
                if existing.kind != selected.kind
                    || existing.required
                    || !existing.fields.is_empty()
                {
                    return Err(Error::invalid(
                        "ACF mapping conflicts with the template's native field definition.",
                    ));
                }
            } else {
                definition.fields.insert(
                    selected.target_name.clone(),
                    Field::primitive(&selected.kind),
                );
            }
        }
        serde_json::to_string(&definition).map_err(|_| invalid())
    }
    pub(crate) fn values(&self, item: &super::wordpress::Element) -> Result<String> {
        let mut source = BTreeMap::<&str, Vec<&str>>::new();
        for meta in item.all("wp:postmeta") {
            source
                .entry(meta.value("wp:meta_key"))
                .or_default()
                .push(meta.value("wp:meta_value"));
        }
        let mut out = BTreeMap::new();
        for field in &self.fields {
            let Some(value) = source.get(field.source_name.as_str()) else {
                continue;
            };
            let reference = format!("_{}", field.source_name);
            let Some(keys) = source.get(reference.as_str()) else {
                return Err(invalid());
            };
            if value.len() != 1 || keys.len() != 1 || keys[0] != field.source_key {
                return Err(invalid());
            }
            let raw = value[0];
            let value = match field.kind.as_str() {
                "string" if raw.len() <= 8000 => Value::String(raw.to_owned()),
                "boolean" if raw == "1" => Value::Bool(true),
                "boolean" if raw == "0" => Value::Bool(false),
                "number" if raw.len() <= 100 => {
                    let number = if raw.contains(['.', 'e', 'E']) {
                        raw.parse::<serde_json::Number>().map_err(|_| invalid())?
                    } else if raw.starts_with('-') {
                        raw.parse::<i64>().map_err(|_| invalid())?.into()
                    } else {
                        raw.parse::<u64>().map_err(|_| invalid())?.into()
                    };
                    if number
                        .as_f64()
                        .is_none_or(|n| n.abs() > 9_007_199_254_740_991.0)
                    {
                        return Err(Error::invalid(
                            "Mapped numeric value exceeds the editor's precision-safe range; explicitly map precise identifiers as strings.",
                        ));
                    }
                    Value::Number(number)
                }
                _ => return Err(invalid()),
            };
            out.insert(field.target_name.clone(), value);
        }
        serde_json::to_string(&out).map_err(|_| invalid())
    }
    pub(crate) fn report(&self) -> Value {
        json!({"adapter":"acf-scalars-v1","mapping_sha256":crate::auth::digest(&serde_json::to_vec(self).expect("serializable mapping")),"fields":self.fields,"boundary":"Explicit registered scalar references only; no PHP serialized object, repeater, flexible content, media/relationship inference, authorization or automatic publication."})
    }
}
