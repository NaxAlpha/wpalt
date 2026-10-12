//! Bounded, server-authoritative form rules. No executable expression language.
use crate::{
    error::{Error, Result},
    schema::{Definition, Field, Registry, identifier},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

fn entry_limit() -> i64 {
    10000
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormDefinition {
    pub title: String,
    pub fields: Vec<FormField>,
    #[serde(default = "entry_limit")]
    pub max_entries: i64,
    #[serde(default)]
    pub subscription: Option<super::audience::SubscriptionAction>,
    #[serde(default)]
    pub notifications: Vec<super::workflows::Notification>,
    #[serde(default)]
    pub draft_post: Option<super::workflows::DraftPost>,
    #[serde(default)]
    pub registration: Option<super::registration::Action>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormField {
    pub name: String,
    pub schema: Field,
    #[serde(default)]
    pub step: usize,
    #[serde(default)]
    pub visible_when: Option<Condition>,
    #[serde(default)]
    pub calculation: Option<Calculation>,
    #[serde(default)]
    pub widget: Option<Widget>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Widget {
    Email,
    Upload,
    TextArea,
    Choice { options: Vec<Choice> },
    Acknowledgment { statement: String },
    Signature { statement: String },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub label: String,
    pub value: String,
    pub score: i32,
}
pub use super::rules::{Calculation, Condition, Expression};
impl FormDefinition {
    /// Dependencies must precede the field: evaluation is linear, with no cycles.
    pub fn validate(&self, common: &Definition) -> Result<Registry> {
        if !(1..=1000000).contains(&self.max_entries)
            || self.title.trim().is_empty()
            || self.title.len() > 160
            || self.fields.is_empty()
            || self.fields.len() > 32
        {
            return Err(Error::invalid(
                "A form needs a title and between 1 and 32 fields.",
            ));
        }
        if let Some(action) = &self.registration {
            for name in [&action.email_field, &action.name_field] {
                if !self.fields.iter().any(|f| {
                    &f.name == name && f.schema.kind == "string" && f.visible_when.is_none()
                }) {
                    return Err(Error::invalid(
                        "Registration requires unconditional email/name text fields.",
                    ));
                }
            }
        }
        if self.notifications.len() > 8 {
            return Err(Error::invalid(
                "At most eight conditional notification routes are supported.",
            ));
        }
        for rule in &self.notifications {
            rule.validate(&self.fields)?;
        }
        if let Some(action) = &self.draft_post {
            for name in [&action.title_field, &action.body_field] {
                if !self.fields.iter().any(|f| {
                    &f.name == name && f.schema.kind == "string" && f.visible_when.is_none()
                }) {
                    return Err(Error::invalid(
                        "Draft post actions require unconditional title/body text fields.",
                    ));
                }
            }
        }
        if let Some(action) = &self.subscription {
            action.validate(&self.fields)?;
        }
        let mut fields: BTreeMap<String, Field> = BTreeMap::new();
        let mut previous_step = 0;
        for field in &self.fields {
            if !identifier(&field.name)
                || fields.contains_key(&field.name)
                || field.step > 7
                || field.step < previous_step
                || field.step > previous_step + 1
            {
                return Err(Error::invalid(
                    "Use unique safe field names and consecutive ordered steps.",
                ));
            }
            if fields.is_empty() && field.step != 0 {
                return Err(Error::invalid("The first form step must be zero."));
            }
            let earlier = &self.fields[..fields.len()];
            if let Some(condition) = &field.visible_when {
                condition.validate(earlier)?;
            }
            if let Some(calculation) = &field.calculation {
                if field.schema.kind != "number" {
                    return Err(Error::invalid(
                        "A calculated result must be a number field.",
                    ));
                }
                calculation.validate(earlier)?;
            }
            if let Some(widget) = &field.widget {
                let valid = match widget {
                    Widget::Email | Widget::TextArea | Widget::Upload => {
                        field.schema.kind == "string"
                    }
                    Widget::Choice { options } => {
                        field.schema.kind == "string"
                            && !options.is_empty()
                            && options.len() <= 32
                            && {
                                let mut keys = std::collections::HashSet::new();
                                options.iter().all(|o| {
                                    !o.label.trim().is_empty()
                                        && o.label.len() <= 160
                                        && !o.value.is_empty()
                                        && o.value.len() <= 100
                                        && keys.insert(&o.value)
                                        && (-10000..=10000).contains(&o.score)
                                })
                            }
                    }
                    Widget::Acknowledgment { statement } => {
                        field.schema.kind == "boolean"
                            && !statement.trim().is_empty()
                            && statement.len() <= 4000
                    }
                    Widget::Signature { statement } => {
                        field.schema.kind == "object"
                            && field.schema.group.is_empty()
                            && field.schema.fields.len() == 2
                            && field
                                .schema
                                .fields
                                .get("name")
                                .is_some_and(|f| f.kind == "string" && f.required)
                            && field
                                .schema
                                .fields
                                .get("accepted")
                                .is_some_and(|f| f.kind == "boolean" && f.required)
                            && !statement.trim().is_empty()
                            && statement.len() <= 4000
                    }
                };
                if !valid {
                    return Err(Error::invalid(
                        "An input widget must match its type and bounded choices or statement.",
                    ));
                }
            }
            previous_step = field.step;
            fields.insert(field.name.clone(), field.schema.clone());
        }
        let registry = Registry {
            common: Definition {
                fields,
                groups: common.groups.clone(),
                options: BTreeMap::new(),
            },
            models: BTreeMap::new(),
        };
        registry.validate()?;
        fn public_fields(fields: &BTreeMap<String, Field>, common: &Definition) -> Result<()> {
            for field in fields.values() {
                if !["string", "number", "boolean", "group", "object", "repeater"]
                    .contains(&field.kind.as_str())
                {
                    return Err(Error::invalid(
                        "Public forms accept scalar fields and bounded groups or repeaters; private content references cannot be collected.",
                    ));
                }
                public_fields(&field.fields, common)?;
                if !field.group.is_empty() {
                    public_fields(&common.groups[&field.group], common)?;
                }
            }
            Ok(())
        }
        public_fields(&registry.common.fields, &registry.common)?;
        Ok(registry)
    }

    /// Discard hidden/derived inputs, recalculate on the server, then apply the
    /// existing shared value grammar. Partial drafts may omit required fields.
    pub fn evaluate(&self, common: &Definition, input: &Value, partial: bool) -> Result<Value> {
        let registry = self.validate(common)?;
        self.evaluate_validated(&registry, input, partial)
    }
    pub(super) fn evaluate_validated(
        &self,
        registry: &Registry,
        input: &Value,
        partial: bool,
    ) -> Result<Value> {
        let input = input
            .as_object()
            .ok_or(Error::invalid("Form values must be an object."))?;
        if input.len() > 32
            || input
                .keys()
                .any(|name| !registry.common.fields.contains_key(name))
        {
            return Err(Error::invalid("Unknown form fields are not accepted."));
        }
        let mut values = Map::new();
        let mut active = BTreeMap::new();
        for field in &self.fields {
            if field
                .visible_when
                .as_ref()
                .is_some_and(|rule| !rule.matches(&values))
            {
                continue;
            }
            let value = if let Some(calculation) = &field.calculation {
                match calculation.evaluate(&self.fields, &values)? {
                    Some(result) => Some(Value::Number(
                        serde_json::Number::from_f64(result).ok_or_else(|| {
                            Error::invalid("A calculation exceeded its numeric range.")
                        })?,
                    )),
                    None if partial => None,
                    None => {
                        return Err(Error::invalid(
                            "A calculation requires all its numeric inputs.",
                        ));
                    }
                }
            } else {
                input.get(&field.name).cloned()
            };
            if let (Some(widget), Some(value)) = (&field.widget, &value) {
                match widget {
                    Widget::Email => {
                        super::mail::email(
                            value
                                .as_str()
                                .ok_or_else(|| Error::invalid("Use a valid email address."))?,
                        )?;
                    }
                    Widget::Choice { options }
                        if !options
                            .iter()
                            .any(|o| Some(o.value.as_str()) == value.as_str()) =>
                    {
                        return Err(Error::invalid("Choose a published option."));
                    }
                    Widget::Acknowledgment { .. }
                        if !partial && field.schema.required && value.as_bool() != Some(true) =>
                    {
                        return Err(Error::invalid("Confirm the required acknowledgment."));
                    }
                    Widget::Signature { .. }
                        if !partial
                            && (value.get("accepted").and_then(Value::as_bool) != Some(true)
                                || value
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .is_none_or(|v| v.trim().is_empty() || v.len() > 100)) =>
                    {
                        return Err(Error::invalid(
                            "A signature requires your name and explicit acceptance.",
                        ));
                    }
                    _ => {}
                }
            }
            if let Some(value) = value {
                values.insert(field.name.clone(), value);
            }
            let mut schema = field.schema.clone();
            if partial {
                clear_required(&mut schema);
            }
            active.insert(field.name.clone(), schema);
        }
        let mut validation = registry.clone();
        if partial {
            for fields in validation.common.groups.values_mut() {
                for field in fields.values_mut() {
                    clear_required(field);
                }
            }
        }
        let values = Value::Object(values);
        validation.validate_values(&active, &values)?;
        Ok(values)
    }
}
fn clear_required(field: &mut Field) {
    field.required = false;
    for child in field.fields.values_mut() {
        clear_required(child);
    }
}
