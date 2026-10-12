//! Native survey review and equivalent private JSON, using current editor authority.
use super::{surveys, web::editor_session};
use crate::{
    App,
    error::{Error, Result},
    view,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use maud::html;
use serde::Deserialize;
use sqlx::Row;

pub fn routes() -> Router<App> {
    Router::new()
        .route("/admin/forms/{id}/report", get(page))
        .route("/api/admin/forms/{id}/report", get(data))
        .route("/admin/forms/{id}/report.csv", get(csv))
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Selection {
    version: i64,
    before_version: i64,
    from: String,
    until: String,
}
impl Selection {
    fn range(&self) -> Result<(i64, i64)> {
        fn time(raw: &str, fallback: i64) -> Result<i64> {
            if raw.is_empty() {
                return Ok(fallback);
            }
            chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M")
                .map(|time| time.and_utc().timestamp())
                .map_err(|_| Error::invalid("Use a valid UTC date and time."))
        }
        Ok((time(&self.from, 0)?, time(&self.until, i64::MAX)?))
    }
}
async fn version(app: &App, id: &str, requested: i64) -> Result<i64> {
    if requested > 0 {
        return Ok(requested);
    }
    if requested < 0 {
        return Err(Error::invalid("Select a published version."));
    }
    sqlx::query_scalar(
        "SELECT published_version FROM business_forms WHERE id=$1 AND published_version>0",
    )
    .bind(id)
    .fetch_optional(&app.db.pool)
    .await?
    .ok_or_else(Error::not_found)
}
async fn data(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<Selection>,
) -> Result<Json<surveys::Report>> {
    editor_session(&app, &headers).await?;
    let version = version(&app, &id, query.version).await?;
    let (from, until) = query.range()?;
    let report = surveys::report_range(&app, &id, version, from, until).await?;
    editor_session(&app, &headers).await?;
    Ok(Json(report))
}
async fn csv(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<Selection>,
) -> Result<Response> {
    editor_session(&app, &headers).await?;
    let selected = version(&app, &id, query.version).await?;
    let (from, until) = query.range()?;
    let report = surveys::report_range(&app, &id, selected, from, until).await?;
    let mut result = String::from(
        "version,field,label,row_kind,eligible,answered,unanswered,skipped,option_value,option_label,option_responses,option_score,numeric_min,numeric_max,numeric_mean\r\n",
    );
    for field in &report.fields {
        let numbers = field
            .numeric
            .as_ref()
            .map(|number| format!("{},{},{}", number.minimum, number.maximum, number.mean))
            .unwrap_or_else(|| ",,".into());
        result.push_str(&format!(
            "{},{},{},field,{},{},{},{},,,,,{}\r\n",
            report.version,
            super::entries::cell(&field.name),
            super::entries::cell(&field.label),
            field.eligible,
            field.answered,
            field.unanswered,
            field.skipped,
            numbers
        ));
        for option in &field.choices {
            result.push_str(&format!(
                "{},{},{},option,{},{},{},{},{},{},{},{},,,\r\n",
                report.version,
                super::entries::cell(&field.name),
                super::entries::cell(&field.label),
                field.eligible,
                field.answered,
                field.unanswered,
                field.skipped,
                super::entries::cell(&option.value),
                super::entries::cell(&option.label),
                option.responses,
                option
                    .score
                    .map(|score| score.to_string())
                    .unwrap_or_default()
            ));
        }
    }
    editor_session(&app, &headers).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=survey-report.csv",
            ),
        ],
        result,
    )
        .into_response())
}
async fn page(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<Selection>,
) -> Result<Html<String>> {
    editor_session(&app, &headers).await?;
    if query.before_version < 0 {
        return Err(Error::invalid("Use a valid version cursor."));
    }
    let selected = version(&app, &id, query.version).await?;
    let (from, until) = query.range()?;
    let report = surveys::report_range(&app, &id, selected, from, until).await;
    let versions = sqlx::query("SELECT version,created_at FROM form_publications WHERE form_id=$1 AND version<$2 ORDER BY version DESC LIMIT 41")
        .bind(&id).bind(if query.before_version > 0 { query.before_version } else { i64::MAX }).fetch_all(&app.db.pool).await?;
    let settings = app.db.settings().await?;
    let session = editor_session(&app, &headers).await?;
    let download_query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("version", &selected.to_string())
        .append_pair("from", &query.from)
        .append_pair("until", &query.until)
        .finish();
    let selected_on_page = versions
        .iter()
        .take(40)
        .any(|row| row.get::<i64, _>("version") == selected);
    Ok(Html(view::layout(
        "Survey report",
        &settings,
        Some(&session),
        html! {
            (view::heading("Business", "Survey report", "Review one published version and response period at a time."))
            p {a href=(format!("/admin/forms/{id}")) {"Edit form"} " · " a href=(format!("/admin/forms/{id}/entries")) {"Individual responses"}}
            form method="get" {
                label {"Published version" select name="version" {
                    @if !selected_on_page {option value=(selected) selected {"Version " (selected)}}
                    @for row in versions.iter().take(40) {@let number = row.get::<i64, _>("version"); option value=(number) selected[number==selected] {"Version " (number)}}
                }}
                label {"From (UTC, inclusive)" input type="datetime-local" name="from" value=(&query.from);}
                label {"Until (UTC, exclusive)" input type="datetime-local" name="until" value=(&query.until);}
                button {"Review responses"}
            }
            @if versions.len()>40 {p {a href=(format!("/admin/forms/{id}/report?version={selected}&before_version={}",versions[39].get::<i64,_>("version"))) {"Browse older published versions"}}}
            @match report {
                Ok(report) => {
                    h2 {(report.title)}
                p {a href=(format!("/admin/forms/{id}/report.csv?{download_query}")) {"Download this aggregate report"}}
                    p {"Version " (report.version) " · " (report.responses) " responses in this snapshot."}
                    p class="help" {(report.scope)}
                    @if report.responses==0 {p {"No responses in this period."}}
                    @for field in &report.fields {section class="panel" {
                        h2 {(field.label)}
                        p {(field.eligible) " eligible · " (field.answered) " answered · " (field.unanswered) " eligible without an answer · " (field.skipped) " skipped by a visibility rule"}
                        @if !field.choices.is_empty() {ul {@for choice in &field.choices {li {(choice.label) ": " (choice.responses) " responses" @if let Some(score)=choice.score {" · score " (score)}}}}}
                        @if let Some(numbers)=&field.numeric {p {"Minimum " (numbers.minimum) " · maximum " (numbers.maximum) " · mean " (numbers.mean) " across " (numbers.count) " numeric answers"}}
                    }}
                }
                Err(error) => {div class="notice error" role="alert" {(error.1)} p {"Your version and date selections are preserved. No partial summary is displayed."}}
            }
        },
    )))
}
