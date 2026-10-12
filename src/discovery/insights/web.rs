//! Native editor observations; all extraction happens only after editor authority.
use crate::{App, auth, error::Result, model::Session, view};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Form, Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use maud::html;
use serde::Deserialize;

pub fn routes() -> Router<App> {
    Router::new()
        .route("/admin/insights/{id}", get(page).post(submit))
        .route("/api/admin/insights/{id}", axum::routing::post(api))
        .layer(DefaultBodyLimit::max(4096))
}
async fn session(app: &App, headers: &HeaderMap) -> Result<Session> {
    let s = auth::session(app, headers).await?;
    if !s.can_edit() || s.hash.starts_with("integration:") {
        return Err(crate::error::Error::forbidden());
    }
    Ok(s)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    csrf: String,
    copy: String,
    #[serde(default)]
    focus: String,
}
async fn render(
    app: &App,
    s: &Session,
    id: &str,
    input: &Input,
    result: Option<&super::Analysis>,
    error: Option<&str>,
) -> Result<Html<String>> {
    let p = crate::content::get(app, id).await?;
    Ok(Html(view::layout(
        "Editorial insights",
        &app.db.settings().await?,
        Some(s),
        html! {
            (view::heading("Publishing", "Editorial insights", "Review the text readers receive, or your saved work, without a ranking score."))
            p {a href=(format!("/admin/posts/{id}")) {"Return to editor"} " · " (p.title)}
        @if s.is_admin(){p {a href="/admin/analysis" {"Analyze the complete public site"}}}
            @if let Some(error)=error {p class="notice error" role="alert" {(error)}}
            section class="panel" {
                form method="post" action=(format!("/admin/insights/{id}")) {
                    (view::csrf(s))
                    div class="field-row" {
                        div {label for="insight-copy" {"Source copy"} select id="insight-copy" name="copy" {
                            option value="working" selected[input.copy=="working"] {"Saved working copy · private draft preview"}
                            option value="public" selected[input.copy=="public"] {"Public publication · anonymous reader"}
                        }}
                        label {"Focus phrase (optional)" input name="focus" maxlength="100" value=(input.focus);}
                    }
                    p class="muted" {"Save changes in the editor first. Working analysis includes the saved theme draft; public analysis uses the live theme and anonymous access rules."}
                    button {"Analyze selected copy"}
                }
            }
            @if let Some(r)=result {
                section class="panel" {
                    h2 { @if r.copy=="working" {"Saved working copy"} @else {"Public publication"} }
                    p {"Content version " (r.content_version) " · " (r.html_bytes) " rendered bytes"}
                    p {a href=(r.path) target="_blank" rel="noopener" {"Open analyzed source"}}
                    div class="table-scroll" {table {caption {"Text observations"}thead {tr {th {"Measure"}th {"Observed"}}}tbody {
                        tr {th scope="row" {"Unicode word segments"}td {(r.observations.word_segments)}}
                        tr {th scope="row" {"Grapheme clusters"}td {(r.observations.graphemes)}}
                        tr {th scope="row" {"Sentence segments"}td {(r.observations.sentence_segments)}}
                        tr {th scope="row" {"Text blocks"}td {(r.observations.blocks)}}
                    }}}
                    @if !r.observations.focus.is_empty() {
                        h3 {"Focus phrase: " (r.observations.focus)}
                        div class="table-scroll" {table {caption {"Whole-segment phrase occurrences"}thead {tr {th {"Location"}th {"Occurrences"}}}tbody {@for l in &r.observations.locations {tr {th scope="row" {(l.label)}td {(l.occurrences)}}}}}}
                    }
                    details {summary {"Analyzed main text"}pre class="font-license" {(r.text.body)}}
                    details {summary {"Method and limits"}p {(r.observations.scope)}p {"Algorithm " (r.observations.algorithm) " · segmentation Unicode " (r.observations.unicode_version) " · NFC " (r.observations.normalization_unicode_version) " · lowercase " (r.observations.lowercase_unicode_version)}p {"HTML digest " code class="font-digest" {(r.html_sha256)} " · source epoch " (r.source_epoch)} @if !r.observations.main_element {p {"No main element: the document body was used."}}}
                }
            }
        },
    )))
}
async fn page(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let s = session(&app, &headers).await?;
    render(
        &app,
        &s,
        &id,
        &Input {
            csrf: String::new(),
            copy: "working".into(),
            focus: String::new(),
        },
        None,
        None,
    )
    .await
}
async fn submit(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(input): Form<Input>,
) -> Result<Response> {
    let s = session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    match super::analyze(&app, &s, &id, &input.copy, &input.focus).await {
        Ok(result) => Ok(render(&app, &s, &id, &input, Some(&result), None)
            .await?
            .into_response()),
        Err(error)
            if error.0 == StatusCode::UNPROCESSABLE_ENTITY
                || error.0 == StatusCode::CONFLICT
                || error.0 == StatusCode::NOT_FOUND
                || error.0 == StatusCode::FORBIDDEN =>
        {
            Ok((
                error.0,
                render(&app, &s, &id, &input, None, Some(error.1)).await?,
            )
                .into_response())
        }
        Err(error) => Err(error),
    }
}
async fn api(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Input>,
) -> Result<Json<super::Analysis>> {
    let s = session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    Ok(Json(
        super::analyze(&app, &s, &id, &input.copy, &input.focus).await?,
    ))
}
