//! Server-rendered scalar questionnaires, without bypassing configured abuse checks.
use super::{forms::Widget, store};
use crate::{
    App,
    error::{Error, Result},
    view,
};
use axum::{
    Form, Router,
    extract::{Path, State},
    response::Html,
    routing::get,
};
use maud::{Markup, html};
use serde_json::{Map, Value};
use sqlx::Row;
use std::collections::BTreeMap;

pub fn routes() -> Router<App> {
    Router::new().route("/forms/{id}/native", get(start).post(answer))
}
async fn publication(app: &App, id: &str) -> Result<(store::PublishedForm, i64)> {
    let row = sqlx::query(
        "SELECT live,published_version FROM business_forms WHERE id=$1 AND published_version>0",
    )
    .bind(id)
    .fetch_optional(&app.db.pool)
    .await?
    .ok_or_else(Error::not_found)?;
    let publication: store::PublishedForm = serde_json::from_str(&row.get::<String, _>("live"))
        .map_err(|_| Error::invalid("The published form needs repair."))?;
    publication.form.validate(&publication.common())?;
    Ok((publication, row.get("published_version")))
}
fn supported(app: &App, publication: &store::PublishedForm) -> bool {
    !app.config.spam.enabled
        && publication.form.fields.iter().all(|field| {
            matches!(
                field.schema.kind.as_str(),
                "string" | "number" | "integer" | "boolean" | "choice"
            ) && !matches!(
                field.widget,
                Some(Widget::Upload | Widget::Signature { .. })
            )
        })
}
async fn start(State(app): State<App>, Path(id): Path<String>) -> Result<Html<String>> {
    let (publication, version) = publication(&app, &id).await?;
    let values = publication
        .form
        .fields
        .iter()
        .filter(|field| field.schema.kind == "boolean")
        .map(|field| (field.name.clone(), Value::Bool(false)))
        .collect();
    page(
        &app,
        &id,
        &publication,
        version,
        &uuid::Uuid::new_v4().to_string(),
        &values,
        None,
        false,
    )
    .await
}
async fn answer(
    State(app): State<App>,
    Path(id): Path<String>,
    Form(input): Form<BTreeMap<String, String>>,
) -> Result<Html<String>> {
    let (mut publication, live_version) = publication(&app, &id).await?;
    let version = input
        .get("version")
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|version| *version > 0)
        .ok_or_else(|| Error::invalid("Use a published form version."))?;
    if version != live_version {
        let raw: String = sqlx::query_scalar(
            "SELECT definition FROM form_publications WHERE form_id=$1 AND version=$2",
        )
        .bind(&id)
        .bind(version)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
        publication = serde_json::from_str(&raw)
            .map_err(|_| Error::invalid("The original published form needs repair."))?;
        publication.form.validate(&publication.common())?;
    }
    if !supported(&app, &publication) {
        return Err(Error::forbidden());
    }
    let key = input
        .get("key")
        .filter(|key| uuid::Uuid::parse_str(key).is_ok())
        .ok_or_else(|| Error::invalid("Use a valid response identifier."))?;
    let action = input.get("action").map(String::as_str).unwrap_or("");
    if !matches!(action, "review" | "submit")
        || input.keys().any(|key| {
            !matches!(key.as_str(), "key" | "version" | "action")
                && !publication
                    .form
                    .fields
                    .iter()
                    .any(|field| *key == format!("value_{}", field.name))
        })
    {
        return Err(Error::invalid("Use the published form controls."));
    }
    let mut values = Map::new();
    let mut error = None;
    for field in &publication.form.fields {
        if field.calculation.is_some() {
            continue;
        }
        let raw = input
            .get(&format!("value_{}", field.name))
            .map(String::as_str)
            .unwrap_or("");
        let value = match field.schema.kind.as_str() {
            "boolean" => Some(Value::Bool(raw == "true")),
            "number" | "integer" if !raw.is_empty() => match serde_json::from_str::<Value>(raw) {
                Ok(value) if value.is_number() => Some(value),
                _ => {
                    error = Some("Use finite numeric values. Your entries are preserved.");
                    Some(Value::String(raw.into()))
                }
            },
            _ if !raw.is_empty() => Some(Value::String(raw.into())),
            _ => None,
        };
        if let Some(value) = value {
            values.insert(field.name.clone(), value);
        }
    }
    if action == "review" && version != live_version {
        error = Some(
            "The published form changed. These original entries are preserved; open the current form before starting a new response.",
        );
    }
    if error.is_none() {
        if let Err(failure) = publication.form.evaluate(
            &publication.common(),
            &Value::Object(values.clone()),
            action == "review",
        ) {
            error = Some(failure.1);
        } else if action == "submit" {
            match store::submit(&app, &id, version, key, &Value::Object(values.clone())).await {
                Ok(_) => {
                    return page(&app, &id, &publication, version, key, &values, None, true).await;
                }
                Err(failure) => error = Some(failure.1),
            }
        }
    }
    page(&app, &id, &publication, version, key, &values, error, false).await
}
#[allow(clippy::too_many_arguments)]
async fn page(
    app: &App,
    id: &str,
    publication: &store::PublishedForm,
    version: i64,
    key: &str,
    input: &Map<String, Value>,
    error: Option<&str>,
    accepted: bool,
) -> Result<Html<String>> {
    let partial = publication
        .form
        .evaluate(&publication.common(), &Value::Object(input.clone()), true)
        .ok();
    // Even invalid input is preserved; only valid canonical earlier values decide
    // visibility. Newly visible required fields appear after a review round trip.
    let canonical = partial.as_ref().and_then(Value::as_object);
    let empty = Map::new();
    let values = canonical.unwrap_or(&empty);
    let markup = html! {
        h1 {(publication.form.title)}
        @if accepted {div class="notice" role="status" {"Your response has been received. Thank you."}}
        @else if !supported(app, publication) {
            p {"This form requires the browser experience for its richer fields or the configured abuse check."}
            p {a href=(format!("/forms/{id}")) {"Open the browser form"}}
        } @else {
            p {"Review calculations and conditional fields before sending. Dates and values remain on this page; no device draft is stored."}
            @if let Some(error)=error {div class="notice error" role="alert" {(error)}}
            @if let Some(purpose)=&publication.subscription_purpose {p {"Optional subscription purpose: " (purpose)}}
            form method="post" action=(format!("/forms/{id}/native")) {
                input type="hidden" name="key" value=(key);
                input type="hidden" name="version" value=(version);
                @for field in &publication.form.fields {
                    @let shown = canonical.is_none() || field.visible_when.as_ref().is_none_or(|condition| condition.matches(values));
                    @if shown {
                        @let label = if field.schema.label.is_empty() {field.name.as_str()}else{field.schema.label.as_str()};
                        @if field.calculation.is_some() {p {strong {(label) ": "} output {(values.get(&field.name).map(Value::to_string).unwrap_or_else(|| "Complete earlier inputs and review calculations".into()))}}}
                        @else {(control(field, label, input.get(&field.name)))}
                    }
                }
                div class="toolbar" {button class="secondary" name="action" value="review" formnovalidate {"Review calculations and fields"} button name="action" value="submit" {"Send response"}}
            }
            p {a href=(format!("/forms/{id}/native")) {"Start a new response with the current published form"} " · " a href=(format!("/forms/{id}")) {"Use the browser form instead"}}
        }
    };
    Ok(Html(view::layout(
        &publication.form.title,
        &app.db.settings().await?,
        None,
        markup,
    )))
}
fn control(field: &super::forms::FormField, label: &str, value: Option<&Value>) -> Markup {
    let name = format!("value_{}", field.name);
    let text = value
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string())
        })
        .unwrap_or_default();
    if field.schema.kind == "boolean" {
        return html! {label {input type="checkbox" name=(name) value="true" checked[value.and_then(Value::as_bool)==Some(true)] required[matches!(field.widget,Some(Widget::Acknowledgment {..})) && field.schema.required];(label)} @if let Some(Widget::Acknowledgment {statement})=&field.widget {p {(statement)}}};
    }
    if let Some(Widget::Choice { options }) = &field.widget {
        return html! {label {(label) select name=(name) required[field.schema.required] {option value="" {"Choose an option"} @for option in options {option value=(&option.value) selected[text==option.value] {(option.label)}}}}};
    }
    if field.schema.kind == "choice" {
        return html! {label {(label) select name=(name) required[field.schema.required] {option value="" {"Choose an option"} @for (value,label) in &field.schema.choices {option value=(value) selected[text==*value] {(label)}}}}};
    }
    if matches!(field.widget, Some(Widget::TextArea)) {
        return html! {label {(label) textarea name=(name) required[field.schema.required] maxlength="8000" {(text)}}};
    }
    html! {label {(label) input name=(name) value=(text) type=(if matches!(field.schema.kind.as_str(),"number"|"integer") && value.is_none_or(|value|value.is_number()) {"number"} else if matches!(field.widget,Some(Widget::Email)) {"email"} else {"text"}) step=(if field.schema.kind=="integer" {"1"}else{"any"}) required[field.schema.required];}}
}
