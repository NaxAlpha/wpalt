//! Closed native bindings to published facts, using open-world vocabulary hints.
//! This is not an arbitrary JSON-LD processor or a provider eligibility validator.
use super::vocabulary::{self, Vocabulary};
use crate::{
    error::{Error, Result},
    schema::{Field, Registry},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub vocabulary: String,
    #[serde(default)]
    pub allow_pending: bool,
    pub primary: String,
    pub entities: BTreeMap<String, Entity>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entity {
    pub types: Vec<String>,
    /// An object/group or one repeater; properties are relative to its fields.
    #[serde(default)]
    pub scope: Vec<String>,
    pub properties: BTreeMap<String, Binding>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Binding {
    Field {
        path: Vec<String>,
        datatype: String,
    },
    Enumeration {
        path: Vec<String>,
        choices: BTreeMap<String, String>,
    },
    ConstantEnumeration {
        value: String,
    },
    Entity {
        entity: String,
    },
    Relationship {
        path: Vec<String>,
        entity: String,
    },
    Image {
        path: Vec<String>,
    },
    Publication {
        attribute: Attribute,
    },
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attribute {
    Title,
    Url,
    PublishedAt,
    Language,
}

fn bad(message: &'static str) -> Error {
    Error::invalid(message)
}
fn path_field<'a>(
    registry: &'a Registry,
    fields: &'a BTreeMap<String, Field>,
    path: &[String],
) -> Result<&'a Field> {
    if path.is_empty() || path.len() > 8 || path.iter().any(|n| !crate::schema::identifier(n)) {
        return Err(bad(
            "Structured bindings need a declared field path of one to eight safe names.",
        ));
    }
    let mut fields = fields;
    for (i, name) in path.iter().enumerate() {
        let field = fields
            .get(name)
            .ok_or_else(|| bad("A structured binding names an undeclared field."))?;
        if i + 1 == path.len() {
            return Ok(field);
        }
        if !["object", "group"].contains(&field.kind.as_str()) {
            return Err(bad(
                "Intermediate binding paths must be objects or groups; use an entity scope for a repeater.",
            ));
        }
        fields = registry.child_fields(field)?;
    }
    Err(bad("Missing structured field path."))
}
fn compatible(v: &Vocabulary, types: &[String], ranges: &[String]) -> bool {
    types.iter().any(|t| ranges.iter().any(|r| v.is_a(t, r)))
}
fn scalar_kind(field: &Field, datatype: &str) -> bool {
    match datatype {
        "Text" => {
            ["string", "email", "url", "date", "choice", "choices"].contains(&field.kind.as_str())
        }
        "URL" => field.kind == "url",
        "Date" => field.kind == "date" || field.kind == "string",
        "DateTime" | "Time" => field.kind == "string",
        "Integer" => field.kind == "integer",
        "Number" => field.kind == "number" || field.kind == "integer",
        "Boolean" => field.kind == "boolean",
        _ => false,
    }
}
impl Definition {
    pub fn validate(
        &self,
        registry: &Registry,
        model: &str,
        definitions: &BTreeMap<String, Definition>,
    ) -> Result<()> {
        if self.vocabulary != vocabulary::RELEASE
            || self.entities.is_empty()
            || self.entities.len() > 16
        {
            return Err(bad(
                "Choose the bundled vocabulary and one to sixteen named entities.",
            ));
        }
        let primary = self
            .entities
            .get(&self.primary)
            .ok_or_else(|| bad("Choose a declared primary entity."))?;
        if !primary.scope.is_empty() {
            return Err(bad(
                "The primary entity must describe the whole published record.",
            ));
        }
        if self
            .entities
            .values()
            .map(|e| e.properties.len())
            .sum::<usize>()
            > 128
        {
            return Err(bad(
                "A model may declare at most 128 structured properties.",
            ));
        }
        let v = vocabulary::get()?;
        let fields = registry.fields_for(model)?;
        for (name, entity) in &self.entities {
            if !crate::schema::identifier(name) || entity.types.is_empty() || entity.types.len() > 4
            {
                return Err(bad("Use safe entity names and one to four known types."));
            }
            let mut unique = std::collections::BTreeSet::new();
            for t in &entity.types {
                v.entity_type(t, self.allow_pending)?;
                if !unique.insert(t) {
                    return Err(bad("Entity types must be distinct."));
                }
            }
            let scoped = if entity.scope.is_empty() {
                &fields
            } else {
                let f = path_field(registry, &fields, &entity.scope)?;
                if !["object", "group", "repeater"].contains(&f.kind.as_str()) {
                    return Err(bad("Entity scopes must be objects, groups or repeaters."));
                }
                registry.child_fields(f)?
            };
            for (property, binding) in &entity.properties {
                if [
                    "price",
                    "priceCurrency",
                    "lowPrice",
                    "highPrice",
                    "availability",
                    "inventoryLevel",
                    "offers",
                    "priceSpecification",
                    "eligibleQuantity",
                ]
                .contains(&property.as_str())
                {
                    return Err(bad(
                        "Commerce state needs native catalog authority; editorial graph bindings cannot copy price, availability or offers.",
                    ));
                }
                let p = v.property(property, self.allow_pending)?;
                if !v.associated(&entity.types, p) {
                    return Err(bad(
                        "This property is not associated with the declared entity types.",
                    ));
                }
                match binding {
                    Binding::Field { path, datatype } => {
                        if !scalar_kind(path_field(registry, scoped, path)?, datatype) {
                            return Err(bad(
                                "Choose a compatible native field and supported scalar datatype.",
                            ));
                        }
                        // Schema.org explicitly permits Text/URL representations of values
                        // beyond rangeIncludes. Owners must choose this fallback explicitly.
                        if !["Text", "URL"].contains(&datatype.as_str())
                            && !compatible(v, std::slice::from_ref(datatype), &p.ranges)
                        {
                            return Err(bad(
                                "The scalar datatype does not match the property range.",
                            ));
                        }
                    }
                    Binding::Enumeration { path, choices } => {
                        let f = path_field(registry, scoped, path)?;
                        if !["choice", "choices"].contains(&f.kind.as_str())
                            || choices.len() != f.choices.len()
                            || choices.keys().ne(f.choices.keys())
                        {
                            return Err(bad(
                                "Map every declared choice to a known enumeration value.",
                            ));
                        }
                        for value in choices.values() {
                            v.enumeration(value, &p.ranges, self.allow_pending)?;
                        }
                    }
                    Binding::ConstantEnumeration { value } => {
                        v.enumeration(value, &p.ranges, self.allow_pending)?;
                    }
                    Binding::Entity { entity: target } => {
                        let target = self
                            .entities
                            .get(target)
                            .ok_or_else(|| bad("Choose a declared named entity."))?;
                        if !compatible(v, &target.types, &p.ranges) {
                            return Err(bad("The named entity does not match the property range."));
                        }
                    }
                    Binding::Relationship {
                        path,
                        entity: target,
                    } => {
                        let f = path_field(registry, scoped, path)?;
                        if !["relationship", "relationships"].contains(&f.kind.as_str()) {
                            return Err(bad(
                                "Entity relationships require a native relationship field.",
                            ));
                        }
                        let target = definitions
                            .get(&f.target)
                            .and_then(|d| d.entities.get(target))
                            .ok_or_else(|| {
                                bad("The target model must declare the selected entity.")
                            })?;
                        if !compatible(v, &target.types, &p.ranges) {
                            return Err(bad(
                                "The related entity does not match the property range.",
                            ));
                        }
                    }
                    Binding::Image { path } => {
                        if !["media", "gallery"]
                            .contains(&path_field(registry, scoped, path)?.kind.as_str())
                            || !compatible(v, &["ImageObject".into()], &p.ranges)
                        {
                            return Err(bad(
                                "Image bindings need media/gallery fields and an ImageObject property range.",
                            ));
                        }
                    }
                    Binding::Publication { attribute } => {
                        let datatype = match attribute {
                            Attribute::Title | Attribute::Language => "Text",
                            Attribute::Url => "URL",
                            Attribute::PublishedAt => "DateTime",
                        };
                        if !compatible(v, &[datatype.into()], &p.ranges) {
                            return Err(bad(
                                "The publication attribute does not match the property range.",
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
    /// Validate semantics without reading live accounts, assets or relationships.
    /// Publication projection separately enforces current public-resource authority.
    pub fn validate_values(&self, values: &Value) -> Result<()> {
        let mut nodes = 0;
        let mut bytes = 0;
        for entity in self.entities.values() {
            for scope in scopes(values, &entity.scope)? {
                nodes += 1;
                if nodes > 64 {
                    return Err(bad(
                        "Declared facts exceed the 64-entity publication budget.",
                    ));
                }
                for binding in entity.properties.values() {
                    if let Binding::Field { path, datatype } = binding {
                        let value = at(scope, path);
                        for item in items(value) {
                            let fact = scalar(item, datatype)?;
                            bytes += serde_json::to_vec(&fact)
                                .map_err(|_| bad("Invalid structured fact."))?
                                .len();
                            if bytes > 64 * 1024 {
                                return Err(bad(
                                    "Declared facts exceed the 64-KiB publication budget.",
                                ));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
pub fn at<'a>(root: &'a Value, path: &[String]) -> &'a Value {
    let mut value = root;
    for name in path {
        value = &value[name];
    }
    value
}
pub fn items(value: &Value) -> Vec<&Value> {
    if value.is_null() || value.as_str().is_some_and(str::is_empty) {
        Vec::new()
    } else if let Some(list) = value.as_array() {
        list.iter().collect()
    } else {
        vec![value]
    }
}
pub fn scopes<'a>(values: &'a Value, path: &[String]) -> Result<Vec<&'a Value>> {
    let value = at(values, path);
    if value.is_null() {
        return Ok(Vec::new());
    }
    let values = items(value);
    if values.len() > 50 || values.iter().any(|v| !v.is_object()) {
        return Err(bad("Entity scopes require at most fifty typed objects."));
    }
    Ok(values)
}
pub fn scalar(value: &Value, datatype: &str) -> Result<Value> {
    if let Some(s) = value.as_str() {
        if s.len() > 8000
            || s.chars().any(|c| {
                (c.is_control() && !['\n', '\r', '\t'].contains(&c))
                    || ['\u{fffe}', '\u{ffff}'].contains(&c)
            })
        {
            return Err(bad(
                "Structured facts need bounded text without unsupported control characters.",
            ));
        }
        let valid = match datatype {
            "Text" => true,
            "URL" => {
                url::Url::parse(s).is_ok_and(|u| {
                    ["http", "https"].contains(&u.scheme())
                        && u.username().is_empty()
                        && u.password().is_none()
                }) && !s.chars().any(char::is_whitespace)
            }
            "Date" => s.len() == 10 && chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok(),
            "DateTime" => chrono::DateTime::parse_from_rfc3339(s).is_ok(),
            "Time" => {
                ["%H:%M", "%H:%M:%S"]
                    .iter()
                    .any(|format| chrono::NaiveTime::parse_from_str(s, format).is_ok())
                    && [5, 8].contains(&s.len())
            }
            _ => false,
        };
        if valid {
            return Ok(value.clone());
        }
    } else if (datatype == "Integer" && value.as_i64().is_some())
        || (datatype == "Number" && value.as_f64().is_some_and(f64::is_finite))
        || (datatype == "Boolean" && value.is_boolean())
    {
        return Ok(value.clone());
    }
    Err(bad(
        "A structured fact does not match its declared scalar datatype.",
    ))
}
