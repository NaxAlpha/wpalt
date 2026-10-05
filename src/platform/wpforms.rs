//! Independently implemented selected WPForms definition adapter. Source code is
//! reference only: no PHP execution, imported submissions, mail or inferred consent.
use crate::{
    business::forms::{FormDefinition, FormField, Widget},
    error::{Error, Result},
    schema::{Definition, Field},
};
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, SeqAccess, Visitor},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_BYTES: usize = 512 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    format: String,
    pub source_site: String,
    plugin_version: String,
    forms: Vec<SourceForm>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceForm {
    source_id: String,
    definition: RawForm,
}
#[derive(Deserialize)]
struct RawForm {
    #[serde(default)]
    id: Value,
    settings: BTreeMap<String, Value>,
    fields: OrderedFields,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}
/// JSON object insertion order is field presentation order, not numeric ID order.
struct OrderedFields(Vec<(Option<String>, Value)>);
impl<'de> Deserialize<'de> for OrderedFields {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Fields;
        impl<'de> Visitor<'de> for Fields {
            type Value = OrderedFields;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("bounded ordered field object or array")
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, Value>()? {
                    if out.len() >= 128 {
                        return Err(serde::de::Error::custom("field count limit"));
                    }
                    out.push((Some(key), value));
                }
                Ok(OrderedFields(out))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(value) = seq.next_element::<Value>()? {
                    if out.len() >= 128 {
                        return Err(serde::de::Error::custom("field count limit"));
                    }
                    out.push((None, value));
                }
                Ok(OrderedFields(out))
            }
        }
        d.deserialize_any(Fields)
    }
}
pub struct ProjectedForm {
    pub source_id: String,
    pub definition: Option<FormDefinition>,
    pub report: Value,
}
pub struct Projection {
    pub source_site: String,
    pub source_sha256: String,
    pub forms: Vec<ProjectedForm>,
    pub report: Value,
}
fn invalid() -> Error {
    Error::invalid(
        "Review the bounded WPForms source export and selected primitive field structures.",
    )
}
fn id(value: &Value) -> Result<u64> {
    match value {
        Value::String(s) => s.parse().map_err(|_| invalid()),
        Value::Number(n) => n.as_u64().ok_or_else(invalid),
        _ => Err(invalid()),
    }
}
pub fn project(bytes: &[u8], common: &Definition) -> Result<Projection> {
    if bytes.len() > MAX_BYTES {
        return Err(invalid());
    }
    let source: Source = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if source.format != "wpalt-wpforms-source-v1"
        || source.plugin_version.is_empty()
        || source.plugin_version.len() > 50
        || !source
            .plugin_version
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-')
        || source.forms.len() > 32
    {
        return Err(invalid());
    }
    let origin = url::Url::parse(&source.source_site).map_err(|_| invalid())?;
    if !["http", "https"].contains(&origin.scheme())
        || origin.host_str().is_none()
        || !origin.username().is_empty()
        || origin.password().is_some()
        || origin.query().is_some()
        || origin.fragment().is_some()
        || !["", "/"].contains(&origin.path())
    {
        return Err(invalid());
    }
    let source_site = origin.origin().ascii_serialization();
    let mut source_ids = BTreeSet::new();
    let mut forms = Vec::new();
    for source_form in source.forms {
        let numeric = source_form
            .source_id
            .parse::<u64>()
            .map_err(|_| invalid())?;
        if numeric == 0
            || !source_ids.insert(numeric)
            || (!source_form.definition.id.is_null() && id(&source_form.definition.id)? != numeric)
        {
            return Err(invalid());
        }
        let raw = source_form.definition;
        let title = raw
            .settings
            .get("form_title")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        if title.trim().is_empty() || title.len() > 160 {
            return Err(invalid());
        }
        let mut ids = BTreeSet::new();
        let mut fields = Vec::new();
        let mut reports = Vec::new();
        let mut unsupported = false;
        for (key, field) in &raw.fields.0 {
            let object = field.as_object().ok_or_else(invalid)?;
            let field_id = id(object.get("id").ok_or_else(invalid)?)?;
            if !ids.insert(field_id)
                || key
                    .as_ref()
                    .is_some_and(|k| k.parse::<u64>().ok() != Some(field_id))
            {
                return Err(invalid());
            }
            let kind = object
                .get("type")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            let label = object
                .get("label")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            let required = match object.get("required") {
                None | Some(Value::Null) => false,
                Some(Value::Bool(value)) => *value,
                Some(Value::String(value)) if ["", "0", "1"].contains(&value.as_str()) => {
                    value == "1"
                }
                Some(Value::Number(value)) if value.as_u64().is_some_and(|n| n <= 1) => {
                    value.as_u64() == Some(1)
                }
                _ => return Err(invalid()),
            };
            let omitted: Vec<_> = object
                .keys()
                .filter(|k| !["id", "type", "label", "required"].contains(&k.as_str()))
                .cloned()
                .collect();
            let dynamic = object
                .get("conditional_logic")
                .is_some_and(|v| !v.is_null() && v != &json!({}) && v != &json!([]));
            let supported = ["text", "email", "textarea"].contains(&kind) && !dynamic;
            reports.push(json!({"source_field":field_id,"type":kind,"target_name":format!("wpforms_{field_id}"),"supported":supported,"omitted_setting_keys":omitted}));
            if !supported {
                unsupported = true;
                continue;
            }
            let mut schema = Field::primitive("string");
            schema.label = label.to_owned();
            schema.required = required;
            fields.push(FormField {
                name: format!("wpforms_{field_id}"),
                schema,
                step: 0,
                visible_when: None,
                calculation: None,
                widget: match kind {
                    "email" => Some(Widget::Email),
                    "textarea" => Some(Widget::TextArea),
                    _ => None,
                },
            });
        }
        let definition = if unsupported || fields.is_empty() || fields.len() > 32 {
            None
        } else {
            let definition = FormDefinition {
                title: title.to_owned(),
                fields,
                max_entries: 10000,
                subscription: None,
                notifications: vec![],
                draft_post: None,
                registration: None,
            };
            definition.validate(common)?;
            Some(definition)
        };
        let report = json!({"source_id":source_form.source_id,"title":title,"supported":definition.is_some(),"fields":reports,"omitted_form_setting_keys":raw.settings.keys().filter(|k|k.as_str()!="form_title").collect::<Vec<_>>(),"omitted_top_level_keys":raw.extra.keys().collect::<Vec<_>>(),"boundary":"Draft definition only. Text/email/textarea order, labels and required flags map. Unsupported controls or conditional logic retain the whole form unsupported. Additional source settings require review; no notifications, registrations, submissions, consent or publication are imported."});
        forms.push(ProjectedForm {
            source_id: source_form.source_id,
            definition,
            report,
        });
    }
    let source_sha256 = crate::auth::digest(bytes);
    let report = json!({"adapter":"wpforms-primitives-v1","source_site":source_site,"source_sha256":source_sha256,"declared_plugin_version":source.plugin_version,"source_forms":forms.len(),"supported_draft_forms":forms.iter().filter(|f|f.definition.is_some()).count(),"forms":forms.iter().map(|f|f.report.clone()).collect::<Vec<_>>(),"boundary":"Independent source export required: ordinary WordPress WXR excludes WPForms definitions. Source version is owner-declared provenance, not a vendor attestation. Retain original source and review all omitted settings before native publication."});
    Ok(Projection {
        source_site,
        source_sha256,
        forms,
        report,
    })
}
