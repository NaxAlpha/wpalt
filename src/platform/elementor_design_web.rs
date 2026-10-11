//! Current-owner review/apply endpoints reuse native draft authority.
use super::elementor_design::{self, Review};
use crate::{
    App, auth,
    error::{Error, Result},
    schema, theme,
};
use axum::{
    Router,
    body::to_bytes,
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
    routing::post,
};
use serde::Deserialize;
use serde_json::json;
const BODY_BYTES: usize = elementor_design::MAX_BYTES + 65536;
pub fn routes() -> Router<App> {
    Router::new()
        .route("/api/admin/design/{id}/elementor/review", post(review))
        .route("/api/admin/design/{id}/elementor/apply", post(apply))
        .layer(DefaultBodyLimit::max(BODY_BYTES))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    csrf: String,
    version: i64,
    request: elementor_design::Request,
    #[serde(default)]
    fingerprint: String,
    #[serde(default)]
    acknowledge_losses: bool,
}
async fn prepare(
    app: &App,
    headers: &HeaderMap,
    id: &str,
    request: Request,
) -> Result<(
    crate::model::Session,
    Input,
    Review,
    tokio::sync::OwnedSemaphorePermit,
)> {
    let started = std::time::Instant::now();
    let actor = auth::session(app, headers).await?;
    if !actor.is_admin() || actor.hash.starts_with("integration:") {
        return Err(Error::forbidden());
    }
    auth::current_editor(app, &actor).await?;
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(';').next())
        != Some("application/json")
    {
        return Err(Error::invalid("Choose a JSON design export."));
    }
    let permit = app
        .media_work
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| Error::invalid("Design review work unavailable."))?;
    let bytes = to_bytes(request.into_body(), BODY_BYTES)
        .await
        .map_err(|_| Error::invalid("Design review input exceeds its byte budget."))?;
    let (input, permit) = tokio::task::spawn_blocking(move || {
        let input: Input = serde_json::from_slice(&bytes)
            .map_err(|_| Error::invalid("Invalid design review request."))?;
        Ok::<_, Error>((input, permit))
    })
    .await
    .map_err(|_| Error::invalid("Design input inspection failed."))??;
    auth::csrf(&actor, &input.csrf)?;
    let registry = schema::Registry::load(app).await?;
    let stored = theme::load_with_registry(app, id, true, &registry).await?;
    if stored.version != input.version {
        return Err(Error::invalid(
            "The theme has changed. Reload and review the import again.",
        ));
    }
    let compiler_request = input.request.clone();
    let theme_id = id.to_owned();
    let (review, permit) = tokio::task::spawn_blocking(move || {
        let reviewed = elementor_design::review(
            &theme_id,
            stored.package,
            stored.version,
            compiler_request,
            &registry,
        )?;
        Ok::<_, Error>((reviewed, permit))
    })
    .await
    .map_err(|_| Error::invalid("Design review failed."))??;
    elementor_design::validate_destination(app, &review).await?;
    theme::validate_literal_references(app, &review.package).await?;
    tracing::debug!(
        event = "design_import_review",
        elapsed_ms = started.elapsed().as_millis() as u64,
        elements = review.report["elements"].as_u64().unwrap_or(0),
        losses = review.report["losses"].as_array().map_or(0, Vec::len),
        target = input.request.target.as_str(),
        "Bounded private design review completed"
    );
    Ok((actor, input, review, permit))
}
async fn private(
    value: impl serde::Serialize + Send + 'static,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> Result<Response> {
    let bytes = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        serde_json::to_vec(&value).map_err(|_| Error::invalid("Invalid design review output."))
    })
    .await
    .map_err(|_| Error::invalid("Design response serialization failed."))??;
    if bytes.len() > 3 * 1024 * 1024 {
        return Err(Error::invalid(
            "Design review output exceeds its byte budget.",
        ));
    }
    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        bytes,
    )
        .into_response())
}
async fn review(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    request: Request,
) -> Result<Response> {
    let (_, _, review, permit) = prepare(&app, &headers, &id, request).await?;
    private(review, permit).await
}
async fn apply(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    request: Request,
) -> Result<Response> {
    let (actor, input, review, permit) = prepare(&app, &headers, &id, request).await?;
    if input.fingerprint != review.fingerprint
        || (!review.report["losses"]
            .as_array()
            .is_some_and(Vec::is_empty)
            && !input.acknowledge_losses)
    {
        return Err(Error::invalid(
            "Review this exact import and acknowledge every reported loss before saving its draft.",
        ));
    }
    let version = theme::save_as(&app, &actor, &id, review.package, input.version, false).await?;
    private(
        json!({"version":version,"published":false,"component":input.request.component}),
        permit,
    )
    .await
}
