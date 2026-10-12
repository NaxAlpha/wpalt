//! Finite declarative form rules, shared by visibility, routing and calculations.
use super::forms::{FormField, Widget};
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", deny_unknown_fields)]
pub enum Condition {
    Equal { field: String, value: Value },
    Present { field: String },
    OneOf { field: String, values: Vec<Value> },
    Greater { field: String, value: f64 },
    AtLeast { field: String, value: f64 },
    Less { field: String, value: f64 },
    AtMost { field: String, value: f64 },
    All { conditions: Vec<Condition> },
    Any { conditions: Vec<Condition> },
    Not { condition: Box<Condition> },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", deny_unknown_fields)]
pub enum Calculation {
    Sum { fields: Vec<String> },
    Product { fields: Vec<String> },
    Score { fields: Vec<String> },
    Expression { expression: Expression },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", deny_unknown_fields)]
pub enum Expression {
    Literal {
        value: f64,
    },
    Field {
        field: String,
    },
    Score {
        field: String,
    },
    Add {
        operands: Vec<Expression>,
    },
    Multiply {
        operands: Vec<Expression>,
    },
    Min {
        operands: Vec<Expression>,
    },
    Max {
        operands: Vec<Expression>,
    },
    Average {
        operands: Vec<Expression>,
    },
    Subtract {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Divide {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Abs {
        operand: Box<Expression>,
    },
    Floor {
        operand: Box<Expression>,
    },
    Ceil {
        operand: Box<Expression>,
    },
    Round {
        operand: Box<Expression>,
        decimals: u8,
    },
    If {
        condition: Condition,
        then: Box<Expression>,
        otherwise: Box<Expression>,
    },
}

fn visit(nodes: &mut usize, depth: usize) -> Result<()> {
    *nodes += 1;
    if *nodes > 128 || depth > 8 {
        return Err(Error::invalid(
            "Rules allow at most 128 nodes and eight nesting levels.",
        ));
    }
    Ok(())
}
fn scalar(value: &Value) -> bool {
    !value.is_object() && !value.is_array() && value.to_string().len() <= 8000
}
fn equal(left: &Value, right: &Value) -> bool {
    match (left.as_f64(), right.as_f64()) {
        (Some(a), Some(b)) => a == b,
        _ => left == right,
    }
}
fn numeric(field: &FormField) -> bool {
    matches!(field.schema.kind.as_str(), "number" | "integer")
}
impl Condition {
    pub fn validate(&self, fields: &[FormField]) -> Result<()> {
        self.validate_at(fields, &mut 0, 1)
    }
    fn validate_at(&self, fields: &[FormField], nodes: &mut usize, depth: usize) -> Result<()> {
        visit(nodes, depth)?;
        match self {
            Self::All { conditions } | Self::Any { conditions } => {
                if conditions.is_empty() || conditions.len() > 32 {
                    return Err(Error::invalid(
                        "Condition groups need between one and 32 rules.",
                    ));
                }
                for condition in conditions {
                    condition.validate_at(fields, nodes, depth + 1)?;
                }
            }
            Self::Not { condition } => condition.validate_at(fields, nodes, depth + 1)?,
            _ => {
                let name = match self {
                    Self::Equal { field, .. }
                    | Self::Present { field }
                    | Self::OneOf { field, .. }
                    | Self::Greater { field, .. }
                    | Self::AtLeast { field, .. }
                    | Self::Less { field, .. }
                    | Self::AtMost { field, .. } => field,
                    _ => unreachable!(),
                };
                let declared = fields.iter().find(|f| &f.name == name).ok_or_else(|| {
                    Error::invalid("Rules must reference a declared available field.")
                })?;
                match self {
                    Self::Equal { value, .. } if !scalar(value) => {
                        return Err(Error::invalid("Compare bounded scalar values."));
                    }
                    Self::OneOf { values, .. }
                        if values.is_empty()
                            || values.len() > 32
                            || values.iter().any(|v| !scalar(v)) =>
                    {
                        return Err(Error::invalid(
                            "Membership needs one to 32 bounded scalar values.",
                        ));
                    }
                    Self::Greater { value, .. }
                    | Self::AtLeast { value, .. }
                    | Self::Less { value, .. }
                    | Self::AtMost { value, .. }
                        if !numeric(declared) || !value.is_finite() =>
                    {
                        return Err(Error::invalid(
                            "Numeric comparisons need a numeric field and finite value.",
                        ));
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
    /// Missing values do not match leaf comparisons. Negation remains Boolean:
    /// Not(Present) deliberately matches an absent or skipped earlier field.
    pub fn matches(&self, values: &Map<String, Value>) -> bool {
        match self {
            Self::Equal { field, value } => values
                .get(field)
                .is_some_and(|candidate| equal(candidate, value)),
            Self::Present { field } => values
                .get(field)
                .is_some_and(|v| !v.is_null() && v.as_str().is_none_or(|s| !s.trim().is_empty())),
            Self::OneOf {
                field,
                values: options,
            } => values
                .get(field)
                .is_some_and(|v| options.iter().any(|option| equal(v, option))),
            Self::Greater { field, value } => values
                .get(field)
                .and_then(Value::as_f64)
                .is_some_and(|n| n > *value),
            Self::AtLeast { field, value } => values
                .get(field)
                .and_then(Value::as_f64)
                .is_some_and(|n| n >= *value),
            Self::Less { field, value } => values
                .get(field)
                .and_then(Value::as_f64)
                .is_some_and(|n| n < *value),
            Self::AtMost { field, value } => values
                .get(field)
                .and_then(Value::as_f64)
                .is_some_and(|n| n <= *value),
            Self::All { conditions } => conditions.iter().all(|c| c.matches(values)),
            Self::Any { conditions } => conditions.iter().any(|c| c.matches(values)),
            Self::Not { condition } => !condition.matches(values),
        }
    }
}
fn score(fields: &[FormField], values: &Map<String, Value>, name: &str) -> Option<f64> {
    let value = values.get(name)?.as_str()?;
    let field = fields.iter().find(|f| f.name == name)?;
    if let Some(Widget::Choice { options }) = &field.widget {
        options
            .iter()
            .find(|o| o.value == value)
            .map(|o| f64::from(o.score))
    } else {
        None
    }
}
fn finite(value: f64) -> Result<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Error::invalid("A calculation exceeded its numeric range."))
    }
}
impl Calculation {
    pub fn validate(&self, fields: &[FormField]) -> Result<()> {
        match self {
            Self::Expression { expression } => expression.validate_at(fields, &mut 0, 1),
            Self::Sum { fields: names }
            | Self::Product { fields: names }
            | Self::Score { fields: names } => {
                if names.is_empty()
                    || names.len() > 32
                    || names.iter().any(|name| {
                        fields.iter().find(|f| &f.name == name).is_none_or(|f| {
                            if matches!(self, Self::Score { .. }) {
                                !matches!(f.widget, Some(Widget::Choice { .. }))
                            } else {
                                !numeric(f)
                            }
                        })
                    })
                {
                    return Err(Error::invalid(
                        "Calculations need one to 32 available numeric fields or scored choices.",
                    ));
                }
                Ok(())
            }
        }
    }
    pub fn evaluate(
        &self,
        fields: &[FormField],
        values: &Map<String, Value>,
    ) -> Result<Option<f64>> {
        match self {
            Self::Expression { expression } => expression.evaluate(fields, values),
            Self::Sum { fields: names }
            | Self::Product { fields: names }
            | Self::Score { fields: names } => {
                let operands: Option<Vec<f64>> = names
                    .iter()
                    .map(|name| {
                        if matches!(self, Self::Score { .. }) {
                            score(fields, values, name)
                        } else {
                            values.get(name).and_then(Value::as_f64)
                        }
                    })
                    .collect();
                operands
                    .map(|items| {
                        finite(if matches!(self, Self::Product { .. }) {
                            items.iter().product()
                        } else {
                            items.iter().sum()
                        })
                    })
                    .transpose()
            }
        }
    }
}
impl Expression {
    fn validate_at(&self, fields: &[FormField], nodes: &mut usize, depth: usize) -> Result<()> {
        visit(nodes, depth)?;
        match self {
            Self::Literal { value } => {
                finite(*value)?;
            }
            Self::Field { field } | Self::Score { field } => {
                let valid = fields.iter().find(|f| &f.name == field).is_some_and(|f| {
                    if matches!(self, Self::Score { .. }) {
                        matches!(f.widget, Some(Widget::Choice { .. }))
                    } else {
                        numeric(f)
                    }
                });
                if !valid {
                    return Err(Error::invalid(
                        "Expressions must reference available numeric fields or scored choices.",
                    ));
                }
            }
            Self::Add { operands }
            | Self::Multiply { operands }
            | Self::Min { operands }
            | Self::Max { operands }
            | Self::Average { operands } => {
                if operands.is_empty() || operands.len() > 32 {
                    return Err(Error::invalid("Expressions need one to 32 operands."));
                }
                for operand in operands {
                    operand.validate_at(fields, nodes, depth + 1)?;
                }
            }
            Self::Subtract { left, right } | Self::Divide { left, right } => {
                left.validate_at(fields, nodes, depth + 1)?;
                right.validate_at(fields, nodes, depth + 1)?;
            }
            Self::Round { operand, decimals } => {
                if *decimals > 8 {
                    return Err(Error::invalid(
                        "Rounding allows zero to eight decimal places.",
                    ));
                }
                operand.validate_at(fields, nodes, depth + 1)?;
            }
            Self::Abs { operand } | Self::Floor { operand } | Self::Ceil { operand } => {
                operand.validate_at(fields, nodes, depth + 1)?
            }
            Self::If {
                condition,
                then,
                otherwise,
            } => {
                condition.validate_at(fields, nodes, depth + 1)?;
                then.validate_at(fields, nodes, depth + 1)?;
                otherwise.validate_at(fields, nodes, depth + 1)?;
            }
        }
        Ok(())
    }
    pub fn evaluate(
        &self,
        fields: &[FormField],
        values: &Map<String, Value>,
    ) -> Result<Option<f64>> {
        let result = match self {
            Self::Literal { value } => Some(*value),
            Self::Field { field } => values.get(field).and_then(Value::as_f64),
            Self::Score { field } => score(fields, values, field),
            Self::If {
                condition,
                then,
                otherwise,
            } => {
                return if condition.matches(values) {
                    then
                } else {
                    otherwise
                }
                .evaluate(fields, values);
            }
            Self::Add { operands }
            | Self::Multiply { operands }
            | Self::Min { operands }
            | Self::Max { operands }
            | Self::Average { operands } => {
                let mut items = Vec::with_capacity(operands.len());
                for operand in operands {
                    let Some(n) = operand.evaluate(fields, values)? else {
                        return Ok(None);
                    };
                    items.push(n);
                }
                Some(match self {
                    Self::Add { .. } => items.iter().sum(),
                    Self::Multiply { .. } => items.iter().product(),
                    Self::Min { .. } => items.iter().copied().reduce(f64::min).unwrap(),
                    Self::Max { .. } => items.iter().copied().reduce(f64::max).unwrap(),
                    Self::Average { .. } => items.iter().map(|n| n / items.len() as f64).sum(),
                    _ => unreachable!(),
                })
            }
            Self::Subtract { left, right } | Self::Divide { left, right } => {
                let Some(a) = left.evaluate(fields, values)? else {
                    return Ok(None);
                };
                let Some(b) = right.evaluate(fields, values)? else {
                    return Ok(None);
                };
                if matches!(self, Self::Divide { .. }) && b == 0.0 {
                    return Err(Error::invalid("A calculation cannot divide by zero."));
                }
                Some(if matches!(self, Self::Subtract { .. }) {
                    a - b
                } else {
                    a / b
                })
            }
            Self::Abs { operand }
            | Self::Floor { operand }
            | Self::Ceil { operand }
            | Self::Round { operand, .. } => {
                let Some(n) = operand.evaluate(fields, values)? else {
                    return Ok(None);
                };
                Some(match self {
                    Self::Abs { .. } => n.abs(),
                    Self::Floor { .. } => n.floor(),
                    Self::Ceil { .. } => n.ceil(),
                    Self::Round { decimals, .. } => {
                        let scale = 10_f64.powi(i32::from(*decimals));
                        finite(n * scale)?.round() / scale
                    }
                    _ => unreachable!(),
                })
            }
        };
        result.map(finite).transpose()
    }
}
