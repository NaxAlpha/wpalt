//! Owner-managed analysis uses native forms and current session authority.
use super::jobs;
use crate::{
    App, auth,
    error::{Error, Result},
    model::Session,
    view,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Form, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use maud::html;
use serde::Deserialize;
pub fn routes() -> Router<App> {
    Router::new()
        .route("/admin/analysis", get(page).post(start_form))
        .route("/admin/analysis/{id}", get(sources).post(action_form))
        .route("/admin/analysis/{id}/sources/{ordinal}", get(details))
        .route("/api/admin/analysis", get(api_list).post(api_start))
        .route("/api/admin/analysis/{id}", axum::routing::post(api_action))
        .route("/api/admin/analysis/{id}/sources", get(api_sources))
        .route(
            "/api/admin/analysis/{id}/sources/{ordinal}",
            get(api_details),
        )
        .layer(DefaultBodyLimit::max(4096))
}
async fn owner(app: &App, headers: &HeaderMap) -> Result<Session> {
    let s = auth::session(app, headers).await?;
    if !s.is_admin() || s.hash.starts_with("integration:") {
        return Err(Error::forbidden());
    }
    Ok(s)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Start {
    csrf: String,
    #[serde(default)]
    focus: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Action {
    csrf: String,
    version: i64,
    action: String,
}
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Cursor {
    after: i64,
}
impl Default for Cursor {
    fn default() -> Self {
        Self { after: -1 }
    }
}
async fn render(app: &App, s: &Session, focus: &str, error: Option<&str>) -> Result<Html<String>> {
    let runs = jobs::list(app).await?;
    jobs::current_owner(app, s).await?;
    Ok(Html(view::layout(
        "Public analysis",
        &app.db.settings().await?,
        Some(s),
        html! {
            (view::heading("Publishing","Public analysis","Analyze the actual server-rendered public site in finite, resumable batches."))
            p {a href="/admin/editorial" {"Editorial queue"} " · " a href="/admin/posts" {"Content and individual editorial insights"}}
            @if let Some(error)=error {p class="notice error" role="alert" {(error)}}
            section class="panel" {h2 {"Start a fresh run"}
                p {"Canonical anonymous publications and language homes. No external fetches, client JavaScript, embedded iframe expansion or query-state crawl. At most two runs are retained; older runs are removed when another starts."}
                form method="post" action="/admin/analysis" {(view::csrf(s))label {"Focus phrase (optional)" input name="focus" value=(focus) maxlength="100";}button disabled[!app.config.analysis.enabled] {"Start public analysis"}}
                p class="muted" {"Starting a run declares its public source state. Publication, permission or live design changes make its results stale. No orphan conclusions are shown until every declared source and edge is processed."}
                @if !app.config.analysis.enabled {p class="notice" {"Background analysis is disabled in configuration."}}
            }
            section class="panel" {h2 {"Recent runs"}
                @if runs.is_empty(){p {"No retained analysis. Start one to inspect your published site."}}
                @for run in runs {
                    article {h3 {(run.state)}
                        p { @if run.state=="rendering" {"Rendering "} @else if run.state=="classifying" {"Resolving edges "} @else {"Recorded source progress "} (run.cursor) " / " (run.total)}
                        p class="muted" {(run.html_bytes) " HTML bytes · " (run.edge_count) " edges · " (run.reused) " unchanged pages reused"}
                        @if let Some(focus)=&run.focus { @if !focus.is_empty(){p {"Focus phrase: " (focus)}} }
                        @if !run.error.is_empty() {p class="notice" {"Run stopped: " (run.error) ". Review configuration and start a fresh run."}}
                        @if !run.current {p class="notice" {"The public source has changed. Retained text and links are hidden; start a fresh run."}}
                        @if run.current && run.state=="complete" {p {a class="button secondary" href=(format!("/admin/analysis/{}",run.id)) {"Review completed sources"}}}
                        form method="post" action=(format!("/admin/analysis/{}",run.id)) {(view::csrf(s))input type="hidden" name="version" value=(run.version);div class="toolbar" {
                            @if run.current && ["rendering","classifying"].contains(&run.state.as_str()) {button class="secondary" name="action" value="advance" {"Process next batch"}button class="secondary" name="action" value="cancel" {"Cancel run"}}
                            button class="secondary" name="action" value="remove" {"Remove retained run"}
                        }}
                    }
                }
                p class="muted" {"The normal local worker advances active runs automatically. Reload to see committed progress. Expired claims can resume after restart; cancellation prevents a late worker from restoring progress."}
            }
            details {summary {"Limits and privacy"}p {"Configured: " (app.config.analysis.max_publications) " publications, " (app.config.analysis.max_edges) " edges, " (app.config.analysis.batch_sources) " sources per batch. Retained derived data expires after " (app.config.analysis.retention_seconds) " seconds while the worker runs."}p {"Previously public text may remain in the private derived cache until removal or expiry. Suppression is not immediate disk erasure. Portable recovery excludes these jobs and rebuilds observations on a fresh site."}}
        },
    )))
}
async fn page(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let s = owner(&app, &headers).await?;
    render(&app, &s, "", None).await
}
async fn start_form(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<Start>,
) -> Result<Response> {
    let s = owner(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    match jobs::start_for_owner(&app, &input.focus, &s).await {
        Ok(_) => Ok(Redirect::to("/admin/analysis").into_response()),
        Err(e) if [StatusCode::UNPROCESSABLE_ENTITY, StatusCode::CONFLICT].contains(&e.0) => {
            Ok((e.0, render(&app, &s, &input.focus, Some(e.1)).await?).into_response())
        }
        Err(e) => Err(e),
    }
}
async fn apply(app: &App, s: &Session, id: &str, input: &Action) -> Result<()> {
    match input.action.as_str() {
        "remove" => jobs::change_for_owner(app, id, input.version, true, s).await,
        "cancel" => jobs::change_for_owner(app, id, input.version, false, s).await,
        "advance" => {
            jobs::tick_for_owner(app, s, id, input.version).await?;
            Ok(())
        }
        _ => Err(Error::invalid("Choose advance, cancel or remove.")),
    }
}
async fn action_form(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(input): Form<Action>,
) -> Result<Response> {
    let s = owner(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    match apply(&app, &s, &id, &input).await {
        Ok(()) => Ok(Redirect::to("/admin/analysis").into_response()),
        Err(e) if [StatusCode::UNPROCESSABLE_ENTITY, StatusCode::CONFLICT].contains(&e.0) => {
            Ok((e.0, render(&app, &s, "", Some(e.1)).await?).into_response())
        }
        Err(e) => Err(e),
    }
}
async fn sources(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(cursor): Query<Cursor>,
) -> Result<Html<String>> {
    let s = owner(&app, &headers).await?;
    let pages = jobs::sources(&app, &id, cursor.after).await?;
    jobs::current_owner(&app, &s).await?;
    Ok(Html(view::layout(
        "Completed public sources",
        &app.db.settings().await?,
        Some(&s),
        html! {
            (view::heading("Publishing","Completed public sources","Inbound conclusions cover only the declared server-rendered graph. External and unscanned routes are outside this result."))
            p {a href="/admin/analysis" {"Return to public analysis"}}
            section class="panel" {
                @for source in pages.iter().take(40) {
                    article {
                        h2 {a class="font-digest" href=(format!("/admin/analysis/{id}/sources/{}",source.ordinal)) {(source.path)}}
                        @if source.without_inbound {p {"No inbound anchor in this complete declared graph."}}
                        @if !source.publication.is_empty() {p {a href=(format!("/admin/posts/{}",source.publication)) {"Edit publication"}}}
                    }
                }
            }
            @if pages.len()>40 {a class="button secondary" href=(format!("/admin/analysis/{id}?after={}",pages[39].ordinal)) {"Next sources"}}
        },
    )))
}
async fn details(
    State(app): State<App>,
    headers: HeaderMap,
    Path((id, ordinal)): Path<(String, i64)>,
) -> Result<Html<String>> {
    let s = owner(&app, &headers).await?;
    let page = jobs::details(&app, &id, ordinal).await?;
    jobs::current_owner(&app, &s).await?;
    Ok(Html(view::layout(
        "Public source observations",
        &app.db.settings().await?,
        Some(&s),
        html! {
            (view::heading("Publishing","Public source observations","Recorded from a completed, current anonymous server-rendered run."))
            p {a href=(format!("/admin/analysis/{id}")) {"All completed sources"} " · " a class="font-digest" href=(page.path) {(page.path)}}
            @if !page.publication.is_empty(){p {a class="button secondary" href=(format!("/admin/insights/{}",page.publication)) {"Review current copy in editorial insights"}}}
            section class="panel" {h2 {"Text observations"}p {(page.observations.word_segments) " Unicode word segments · " (page.observations.graphemes) " grapheme clusters · " (page.observations.sentence_segments) " sentence segments"}
                @if !page.observations.focus.is_empty() {h3 {"Focus phrase: " (page.observations.focus)} @for location in &page.observations.locations {p {(location.label) ": " (location.occurrences)}}}
                details {summary {"Analyzed main text"}pre class="font-license" {(page.text.body)}}
            }
            section class="panel" {h2 {"Link issues"}p {(page.issue_count) " observed issues. Up to 100 are shown for this source."} @for edge in &page.issues {p {strong {(edge.state)} " · " span class="font-digest" {(edge.target)} " · " (edge.placement)}}}
            details {summary {"Method and limits"}p {(page.observations.scope)}p {"Segmentation Unicode " (page.observations.unicode_version) " · NFC " (page.observations.normalization_unicode_version) " · lowercase " (page.observations.lowercase_unicode_version) " · " (page.observations.algorithm)}p {"HTML digest " code class="font-digest" {(page.html_sha256)}}}
        },
    )))
}
async fn api_list(State(app): State<App>, headers: HeaderMap) -> Result<Json<Vec<jobs::Job>>> {
    let s = owner(&app, &headers).await?;
    let result = jobs::list(&app).await?;
    jobs::current_owner(&app, &s).await?;
    Ok(Json(result))
}
async fn api_start(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<Start>,
) -> Result<Json<serde_json::Value>> {
    let s = owner(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    Ok(Json(
        serde_json::json!({"id":jobs::start_for_owner(&app,&input.focus,&s).await?}),
    ))
}
async fn api_action(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Action>,
) -> Result<Json<serde_json::Value>> {
    let s = owner(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    apply(&app, &s, &id, &input).await?;
    Ok(Json(serde_json::json!({"applied":true})))
}
async fn api_sources(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(cursor): Query<Cursor>,
) -> Result<Json<Vec<jobs::SourcePage>>> {
    let s = owner(&app, &headers).await?;
    let result = jobs::sources(&app, &id, cursor.after).await?;
    jobs::current_owner(&app, &s).await?;
    Ok(Json(result))
}
async fn api_details(
    State(app): State<App>,
    headers: HeaderMap,
    Path((id, ordinal)): Path<(String, i64)>,
) -> Result<Json<jobs::SourceDetails>> {
    let s = owner(&app, &headers).await?;
    let result = jobs::details(&app, &id, ordinal).await?;
    jobs::current_owner(&app, &s).await?;
    Ok(Json(result))
}
