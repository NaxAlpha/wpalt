//! Bounded, server-authoritative form rules. No executable expression language.
use crate::{
    error::{Error, Result},
    schema::{Definition, Field, Registry, identifier},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormDefinition {
    pub title: String,
    pub fields: Vec<FormField>,
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
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", deny_unknown_fields)]
pub enum Condition {
    Equal { field: String, value: Value },
    Present { field: String },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", deny_unknown_fields)]
pub enum Calculation {
    Sum { fields: Vec<String> },
    Product { fields: Vec<String> },
}
impl FormDefinition {
    /// Dependencies must precede the field: evaluation is linear, with no cycles.
    pub fn validate(&self, common: &Definition) -> Result<Registry> {
        if self.title.trim().is_empty()
            || self.title.len() > 160
            || self.fields.is_empty()
            || self.fields.len() > 32
        {
            return Err(Error::invalid(
                "A form needs a title and between 1 and 32 fields.",
            ));
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
            if let Some(condition) = &field.visible_when {
                let name = match condition {
                    Condition::Equal { field, .. } | Condition::Present { field } => field,
                };
                if !fields.contains_key(name) {
                    return Err(Error::invalid(
                        "Conditions must reference an earlier field.",
                    ));
                }
                if let Condition::Equal { value, .. } = condition {
                    if value.is_object() || value.is_array() || value.to_string().len() > 8000 {
                        return Err(Error::invalid("Conditions compare bounded scalar values."));
                    }
                }
            }
            if let Some(calculation) = &field.calculation {
                let names = match calculation {
                    Calculation::Sum { fields } | Calculation::Product { fields } => fields,
                };
                if field.schema.kind != "number"
                    || names.is_empty()
                    || names.len() > 32
                    || names
                        .iter()
                        .any(|name| fields.get(name).is_none_or(|s| s.kind != "number"))
                {
                    return Err(Error::invalid(
                        "Calculations require earlier numeric fields and a numeric result.",
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
        Ok(registry)
    }

    /// Discard hidden/derived inputs, recalculate on the server, then apply the
    /// existing shared value grammar. Partial drafts may omit required fields.
    pub fn evaluate(&self, common: &Definition, input: &Value, partial: bool) -> Result<Value> {
        let registry = self.validate(common)?;
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
            let visible = match &field.visible_when {
                None => true,
                Some(Condition::Equal { field, value }) => values.get(field) == Some(value),
                Some(Condition::Present { field }) => values.get(field).is_some_and(|v| {
                    !v.is_null() && v.as_str().is_none_or(|s| !s.trim().is_empty())
                }),
            };
            if !visible {
                continue;
            }
            let value = if let Some(calculation) = &field.calculation {
                let names = match calculation {
                    Calculation::Sum { fields } | Calculation::Product { fields } => fields,
                };
                let operands: Option<Vec<f64>> = names
                    .iter()
                    .map(|name| values.get(name).and_then(Value::as_f64))
                    .collect();
                match operands {
                    Some(operands) => {
                        let result = match calculation {
                            Calculation::Sum { .. } => operands.iter().sum(),
                            Calculation::Product { .. } => operands.iter().product(),
                        };
                        Some(Value::Number(serde_json::Number::from_f64(result).ok_or(
                            Error::invalid("A calculation exceeded its numeric range."),
                        )?))
                    }
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
            if let Some(value) = value {
                values.insert(field.name.clone(), value);
            }
            let mut schema = field.schema.clone();
            if partial {
                clear_required(&mut schema);
            }
            active.insert(field.name.clone(), schema);
        }
        let mut validation = registry;
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
