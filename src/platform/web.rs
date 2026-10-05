//! Owner-facing offline export assessment; uploaded source is never persisted or fetched.
use crate::{
    App, auth,
    error::{Error, Result},
    view,
};
use axum::{
    Router,
    extract::{DefaultBodyLimit, Multipart, State},
    http::{HeaderMap, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use maud::html;
use serde_json::Value;
pub fn routes() -> Router<App> {
    Router::new().route(
        "/admin/migration",
        get(page).post(assess).layer(DefaultBodyLimit::max(
            super::wordpress::MAX_BYTES + 64 * 1024,
        )),
    )
}
async fn owner(app: &App, headers: &HeaderMap) -> Result<crate::model::Session> {
    let s = auth::session(app, headers).await?;
    if !s.is_admin() {
        return Err(Error::forbidden());
    }
    Ok(s)
}
async fn page(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let session = owner(&app, &headers).await?;
    Ok(Html(render(&app, &session, None, None).await?))
}
async fn render(
    app: &App,
    session: &crate::model::Session,
    report: Option<&Value>,
    error: Option<&str>,
) -> Result<String> {
    Ok(view::layout(
        "Migration",
        &app.db.settings().await?,
        Some(session),
        html! {
            (view::heading("Operations","Migration","Assess your WordPress export before moving content into a separate site."))
            a href="/admin/operations" {"Back to operations"}
            @if let Some(error)=error {p role="alert" class="notice error" {(error)}}
            section class="panel" {h2 {"Review an export"}p {"Choose a WordPress WXR 1.2 XML file. It stays in memory for this request; nothing is imported, stored or fetched. Titles, URLs and metadata keys appear in the private assessment."}
                form method="post" action="/admin/migration" enctype="multipart/form-data" {
                    (view::csrf(session))
                    label for="migration-source" {"WordPress export"}
                    input id="migration-source" type="file" name="source" accept=".xml,application/xml,text/xml" required aria-describedby="migration-help";
                    p id="migration-help" class="muted" {"UTF-8 WXR 1.2, at most 32 MiB. This export may omit plugin, payment or access data; keep your original site and uploads."}
                    div class="actions" {button type="submit" name="action" value="preview" {"Assess export"}button type="submit" name="action" value="download" class="secondary" {"Download full assessment"}}
                }
            }
            @if let Some(report)=report {
                section class="panel migration-review" {h2 {"Assessment"}p {(report["source_items"].as_u64().unwrap_or(0)) " source records · " (report["supported_core_items"].as_u64().unwrap_or(0)) " supported core content records"}
                    p {"Source site: " (report["source_site"].as_str().unwrap_or(""))}
                    p class="muted" {"The core package uses an explicit owner mapping. Private, password-protected or ambiguous plugin access remains draft. Unsupported orders never establish payment settlement. Review every warning before planning cutover."}
                    h3 {"Records"}
                    @for item in report["items"].as_array().into_iter().flatten().take(50) {div class="list-row" {strong {(item["title"].as_str().unwrap_or("Untitled"))}p class="muted" {(item["type"].as_str().unwrap_or("unknown")) " · " (item["source_status"].as_str().unwrap_or("unknown"))}p {(item["source_url"].as_str().unwrap_or(""))}}}
                    @if report["items"].as_array().is_some_and(|a|a.len()>50){p {"Showing the first 50 records. Download the full assessment to review all records."}}
                    h3 {"Warnings"}
                    @for warning in report["warnings"].as_array().into_iter().flatten().take(100) {p {(warning_label(warning["code"].as_str().unwrap_or("review_mapping"))) @if let Some(id)=warning["source_id"].as_str(){" · source " (id)} @if let Some(reason)=warning["reason"].as_str(){": " (reason)}}}
                    @if report["warnings"].as_array().is_some_and(|a|a.len()>100){p {"More warnings are present in the full assessment."}}
                }
            }
            section class="panel" {h2 {"Prepare a separate target"}p {"Core package creation currently uses the stopped-host CLI. Initialize an empty migration template, provide an independently copied media directory, review the exact preview plan, then create a new private package. Recover it into an empty target and verify the site before changing traffic."}p class="muted" {"M8 is still in development. Additional plugin adapters and external integration tooling remain in progress."}}
        },
    ))
}
async fn assess(
    State(app): State<App>,
    headers: HeaderMap,
    mut upload: Multipart,
) -> Result<Response> {
    let session = owner(&app, &headers).await?;
    let permit = app.media_work.clone().try_acquire_owned().map_err(|_| {
        Error(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "Local processing is busy. Retry this assessment shortly.",
        )
    })?;
    let mut csrf = None;
    let mut source = None;
    let mut action = None;
    while let Some(field) = upload
        .next_field()
        .await
        .map_err(|_| Error::invalid("Invalid export upload."))?
    {
        match field.name().unwrap_or("") {
            "csrf" if csrf.is_none() => {
                csrf = Some(
                    field
                        .text()
                        .await
                        .map_err(|_| Error::invalid("Invalid CSRF field."))?,
                );
            }
            "action" if action.is_none() => {
                action = Some(
                    field
                        .text()
                        .await
                        .map_err(|_| Error::invalid("Invalid assessment action."))?,
                );
            }
            "source" if source.is_none() => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|_| Error::invalid("Export exceeds its upload boundary."))?;
                if bytes.len() > super::wordpress::MAX_BYTES {
                    return Err(Error::invalid("Export exceeds 32 MiB."));
                }
                source = Some(bytes);
            }
            _ => {
                return Err(Error::invalid(
                    "Use one export, assessment action and CSRF field.",
                ));
            }
        }
    }
    auth::csrf(&session, csrf.as_deref().unwrap_or(""))?;
    let download = match action.as_deref() {
        Some("download") => true,
        Some("preview") => false,
        _ => return Err(Error::invalid("Choose assessment preview or download.")),
    };
    let bytes = source.ok_or(Error::invalid("Choose a WordPress XML export."))?;
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        super::wordpress::assess(&bytes).map(|a| a.report)
    })
    .await
    .map_err(|_| Error::invalid("Export assessment failed."))?;
    let current = owner(&app, &headers).await?;
    auth::csrf(&current, csrf.as_deref().unwrap_or(""))?;
    match result {
        Ok(report) if download => {
            let data = serde_json::to_vec_pretty(&report)
                .map_err(|_| Error::invalid("Assessment serialization failed."))?;
            Ok((
                [
                    (header::CONTENT_TYPE, "application/json"),
                    (header::CACHE_CONTROL, "no-store"),
                    (
                        header::CONTENT_DISPOSITION,
                        "attachment; filename=wordpress-assessment.json",
                    ),
                ],
                data,
            )
                .into_response())
        }
        Ok(report) => Ok(Html(render(&app, &current, Some(&report), None).await?).into_response()),
        Err(error) => Ok((
            error.0,
            Html(render(&app, &current, None, Some(error.1)).await?),
        )
            .into_response()),
    }
}

fn warning_label(code: &str) -> &str {
    match code {
        "unsupported_record" => "Not supported by the core importer",
        "retain_as_draft" => "Retained as draft",
        "metadata_requires_adapter" => "Plugin metadata needs review",
        "review_shortcodes" => "Shortcode review required",
        "access_mapping_required" => "Access mapping required",
        "source_queue_not_replayed" => "Source queue not replayed",
        _ => "Mapping review required",
    }
}
