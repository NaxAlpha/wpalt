//! Frozen-version response aggregates from a bounded consistent database snapshot.
use super::{forms::Widget, store::PublishedForm};
use crate::{
    App,
    error::{Error, Result},
};
use futures_util::TryStreamExt;
use serde::Serialize;
use serde_json::Value;
use sqlx::Row;
use std::time::{Duration, Instant};

pub const MAX_RESPONSES: usize = 10_000;
pub const MAX_VALUE_BYTES: usize = 32 * 1024 * 1024;
#[derive(Debug, Serialize)]
pub struct Report {
    pub form_id: String,
    pub version: i64,
    pub from: i64,
    pub until: i64,
    pub title: String,
    pub definition_sha256: String,
    pub responses: usize,
    pub value_bytes: usize,
    pub fields: Vec<FieldReport>,
    pub scope: &'static str,
}
#[derive(Debug, Serialize)]
pub struct FieldReport {
    pub name: String,
    pub label: String,
    pub eligible: usize,
    pub answered: usize,
    pub unanswered: usize,
    pub skipped: usize,
    pub choices: Vec<ChoiceReport>,
    pub numeric: Option<NumericReport>,
}
#[derive(Debug, Serialize)]
pub struct ChoiceReport {
    pub value: String,
    pub label: String,
    pub responses: usize,
    pub score: Option<i32>,
}
#[derive(Debug, Serialize)]
pub struct NumericReport {
    pub count: usize,
    pub minimum: f64,
    pub maximum: f64,
    pub mean: f64,
}
fn answered(value: &Value) -> bool {
    !value.is_null()
        && value.as_str().is_none_or(|s| !s.trim().is_empty())
        && value.as_array().is_none_or(|a| !a.is_empty())
}
/// A summary contains no free-text, attachments, signatures or individual IDs.
/// Trusted callers handle current editor authority immediately before disclosure.
pub async fn report(app: &App, form: &str, version: i64) -> Result<Report> {
    report_range(app, form, version, 0, i64::MAX).await
}
pub async fn report_range(
    app: &App,
    form: &str,
    version: i64,
    from: i64,
    until: i64,
) -> Result<Report> {
    if version < 1 || uuid::Uuid::parse_str(form).is_err() || from < 0 || until <= from {
        return Err(Error::invalid("Select a published form version."));
    }
    let started = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        snapshot(app, form, version, from, until, started),
    )
    .await
    .map_err(|_| {
        Error::invalid(
            "This report exceeds the ten-second work budget. Narrow the response cohort.",
        )
    })??;
    tracing::info!(
        event = "survey_report",
        version,
        responses = result.responses,
        value_bytes = result.value_bytes,
        duration_ms = started.elapsed().as_millis() as u64
    );
    Ok(result)
}
async fn snapshot(
    app: &App,
    form: &str,
    version: i64,
    from: i64,
    until: i64,
    started: Instant,
) -> Result<Report> {
    let mut tx = app.db.pool.begin().await?;
    if app.db.postgres {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
    }
    let raw: String = sqlx::query_scalar(
        "SELECT definition FROM form_publications WHERE form_id=$1 AND version=$2",
    )
    .bind(form)
    .bind(version)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(Error::not_found)?;
    let publication: PublishedForm = serde_json::from_str(&raw)
        .map_err(|_| Error::invalid("The published definition needs repair."))?;
    let registry = publication.form.validate(&publication.common())?;
    let mut report = Report {
        form_id: form.into(),
        version,
        from,
        until,
        title: publication.form.title.clone(),
        definition_sha256: crate::auth::digest(raw.as_bytes()),
        responses: 0,
        value_bytes: 0,
        fields: publication
            .form
            .fields
            .iter()
            .map(|field| {
                let choices = match &field.widget {
                    Some(Widget::Choice { options }) => options
                        .iter()
                        .map(|o| ChoiceReport {
                            value: o.value.clone(),
                            label: o.label.clone(),
                            responses: 0,
                            score: Some(o.score),
                        })
                        .collect(),
                    _ if field.schema.kind == "boolean" => vec![
                        ChoiceReport {
                            value: "true".into(),
                            label: "Yes".into(),
                            responses: 0,
                            score: None,
                        },
                        ChoiceReport {
                            value: "false".into(),
                            label: "No".into(),
                            responses: 0,
                            score: None,
                        },
                    ],
                    _ => field
                        .schema
                        .choices
                        .iter()
                        .map(|(value, label)| ChoiceReport {
                            value: value.clone(),
                            label: label.clone(),
                            responses: 0,
                            score: None,
                        })
                        .collect(),
                };
                FieldReport {
                    name: field.name.clone(),
                    label: if field.schema.label.is_empty() {
                        field.name.clone()
                    } else {
                        field.schema.label.clone()
                    },
                    eligible: 0,
                    answered: 0,
                    unanswered: 0,
                    skipped: 0,
                    choices,
                    numeric: None,
                }
            })
            .collect(),
        scope: "One frozen form publication and consistent database snapshot; eligible excludes conditional skips. Answered excludes null/blank/empty selection. Free text and individual identities are omitted. Multiple-choice option counts may exceed the answer count. Numeric mean uses binary64 and is not commerce pricing or a population estimate.",
    };
    // The version/id index bounds the cohort and avoids loading unrelated versions.
    // Streaming keeps only one bounded response plus fixed field/option summaries.
    let mut rows = sqlx::query("SELECT values_json FROM form_entries WHERE form_id=$1 AND form_version=$2 AND created_at>=$3 AND created_at<$4 ORDER BY created_at,id LIMIT 10001")
        .bind(form).bind(version).bind(from).bind(until).fetch(&mut *tx);
    while let Some(row) = rows.try_next().await? {
        report.responses += 1;
        let raw: String = row.get("values_json");
        report.value_bytes = report
            .value_bytes
            .checked_add(raw.len())
            .ok_or_else(|| Error::invalid("This report exceeds its byte budget."))?;
        if report.responses > MAX_RESPONSES
            || report.value_bytes > MAX_VALUE_BYTES
            || raw.len() > 128 * 1024
            || started.elapsed() > Duration::from_secs(10)
        {
            return Err(Error::invalid(
                "This report exceeds 10,000 responses, 32 MiB or ten seconds. No partial summary is returned.",
            ));
        }
        let value: Value = serde_json::from_str(&raw)
            .map_err(|_| Error::invalid("A stored response needs repair."))?;
        if publication
            .form
            .evaluate_validated(&registry, &value, false)?
            != value
        {
            return Err(Error::invalid(
                "A stored response is not canonical; repair it before reporting.",
            ));
        }
        let values = value
            .as_object()
            .ok_or_else(|| Error::invalid("A stored response needs repair."))?;
        for (field, summary) in publication.form.fields.iter().zip(&mut report.fields) {
            if field
                .visible_when
                .as_ref()
                .is_some_and(|condition| !condition.matches(values))
            {
                summary.skipped += 1;
                continue;
            }
            summary.eligible += 1;
            let Some(value) = values.get(&field.name).filter(|v| answered(v)) else {
                summary.unanswered += 1;
                continue;
            };
            summary.answered += 1;
            for choice in &mut summary.choices {
                let selected = value.as_str() == Some(choice.value.as_str())
                    || value
                        .as_bool()
                        .is_some_and(|v| v.to_string() == choice.value)
                    || value.as_array().is_some_and(|items| {
                        items
                            .iter()
                            .any(|item| item.as_str() == Some(choice.value.as_str()))
                    });
                choice.responses += usize::from(selected);
            }
            if matches!(field.schema.kind.as_str(), "number" | "integer") {
                let n = value
                    .as_f64()
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| Error::invalid("A stored numeric response needs repair."))?;
                if let Some(numbers) = &mut summary.numeric {
                    numbers.count += 1;
                    numbers.minimum = numbers.minimum.min(n);
                    numbers.maximum = numbers.maximum.max(n);
                    // Weighted summation avoids overflow in mean + (n-mean)/count
                    // for mixed opposite-sign finite extremes.
                    numbers.mean = numbers.mean
                        * ((numbers.count - 1) as f64 / numbers.count as f64)
                        + n / numbers.count as f64;
                    if !numbers.mean.is_finite() {
                        return Err(Error::invalid("A numeric summary exceeds its range."));
                    }
                } else {
                    summary.numeric = Some(NumericReport {
                        count: 1,
                        minimum: n,
                        maximum: n,
                        mean: n,
                    });
                }
            }
        }
    }
    drop(rows);
    tx.commit().await?;
    Ok(report)
}
