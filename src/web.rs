use crate::{
    App, auth, backup, content,
    error::{Error, Result},
    model::{Post, PostInput, Session, Settings},
    now, view,
};
use axum::{
    Extension, Json, Router,
    body::{Body, HttpBody},
    extract::{ConnectInfo, DefaultBodyLimit, Form, MatchedPath, Multipart, Path, Query, State},
    http::{HeaderMap, HeaderValue, Request, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use maud::{Markup, html};
use serde::{Deserialize, Serialize};
use sqlx::{Any, QueryBuilder, Row};
use std::{io::Cursor, net::SocketAddr};

pub fn router(app: App) -> Router {
    let limit = app.config.max_upload_bytes + 64 * 1024;
    let timeout = app.config.request_timeout_seconds;
    let gzip = app.config.cache.gzip;
    let business = if app.config.business_enabled {
        crate::business::web::routes(&app)
    } else {
        Router::new()
    };
    Router::new()
        .merge(crate::builder_web::routes())
        .merge(crate::platform::web::routes())
        .merge(crate::platform::integrations::routes())
        .merge(crate::platform::events::routes())
        .merge(crate::platform::integration_web::routes())
        .merge(crate::operations::privacy::routes())
        .merge(business)
        .merge(crate::discovery::routes())
        .merge(crate::membership::web::routes(&app))
        .merge(crate::commerce::web::routes(&app))
        .route("/", get(home))
        .route("/search", get(home))
        .route("/health", get(health))
        .route(
            "/api/spam/challenge",
            post(spam_challenge).layer(DefaultBodyLimit::max(1024)),
        )
        .route("/assets/spam.js", get(spam_js))
        .route("/account", get(account))
        .route(
            "/account/passkeys/start",
            post(passkey_register_start).layer(DefaultBodyLimit::max(128 * 1024)),
        )
        .route(
            "/account/passkeys/finish",
            post(passkey_register_finish).layer(DefaultBodyLimit::max(128 * 1024)),
        )
        .route("/account/passkeys/remove", post(passkey_remove))
        .route(
            "/passkeys/login/start",
            post(passkey_login_start).layer(DefaultBodyLimit::max(128 * 1024)),
        )
        .route(
            "/passkeys/login/finish",
            post(passkey_login_finish).layer(DefaultBodyLimit::max(128 * 1024)),
        )
        .route("/assets/auth.js", get(auth_js))
        .route("/account/security", get(factor_page))
        .route("/account/security/begin", post(factor_begin))
        .route("/account/security/confirm", post(factor_confirm))
        .route("/account/security/disable", post(factor_disable))
        .route("/login", get(login_page).post(login))
        .route("/logout", post(logout))
        .route("/feed.xml", get(feed))
        .route("/assets/app.css", get(css))
        .route("/assets/admin-ui.css", get(admin_css))
        .route("/assets/admin.js", get(js))
        .route("/assets/editor.js", get(editor_js))
        .route("/assets/form-embed.js", get(form_embed_js))
        .route("/admin", get(dashboard))
        .route("/admin/posts", get(post_list))
        .route("/admin/posts/new", get(new_post).post(create_post))
        .route("/admin/posts/{id}", get(edit_post).post(update_post))
        .route(
            "/admin/posts/{id}/revisions/{revision}",
            post(restore_revision),
        )
        .route("/admin/preview/{id}", get(preview))
        .route("/admin/media", get(media_list).post(upload))
        .route("/admin/media/video", post(upload_video))
        .route("/admin/media/{id}", post(update_media))
        .route("/media/{id}", get(media_file))
        .route("/media/{id}/resize/{width}", get(media_derivative))
        .route(
            "/media/{id}/resize/{width}/{format}",
            get(media_derivative_format),
        )
        .route("/admin/comments", get(comments))
        .route("/admin/comments/{id}", post(moderate))
        .route("/admin/settings", get(settings_page).post(save_settings))
        .route("/admin/users", get(users).post(add_user))
        .route("/admin/users/{id}", post(update_user))
        .route("/admin/operations", get(operations))
        .route("/admin/backup", post(download_backup))
        .route("/admin/recovery/run", post(run_recovery))
        .route("/admin/operations/integrity", post(integrity_scan))
        .route("/admin/operations/cleanup", post(cleanup_preview))
        .route("/admin/operations/cleanup/execute", post(cleanup_execute))
        .route("/admin/operations/cache/purge", post(cache_purge))
        .route("/admin/operations/cache/preload", post(cache_preload))
        .route("/admin/operations/audit", get(audit_history))
        .route("/admin/export", get(export_content))
        .route("/api/content", get(public_api))
        .route("/api/admin/media", get(media_picker))
        .route("/api/admin/content", post(create_api))
        .route("/api/admin/content/{id}", post(update_api))
        .route("/{slug}", get(public_post))
        .route("/{locale}/", get(localized_home))
        .route("/{locale}/search", get(localized_home))
        .route("/{locale}/{slug}", get(localized_post))
        .route("/{slug}/comments", post(comment))
        .fallback(|| async { Error::not_found() })
        .layer(DefaultBodyLimit::max(limit))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(timeout),
        ))
        .layer(middleware::from_fn_with_state(
            app.clone(),
            security_and_trace,
        ))
        .layer(tower_http::compression::CompressionLayer::new().gzip(gzip))
        .with_state(app)
}
async fn security_and_trace(
    State(app): State<App>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let started = std::time::Instant::now();
    let id = uuid::Uuid::new_v4().to_string();
    let method = request.method().clone();
    let requested_path = request.uri().path().to_owned();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".into());
    let span = tracing::info_span!("request",request_id=%id,method=%method,route=%route);
    let privileged_write = method != axum::http::Method::GET
        && method != axum::http::Method::HEAD
        && (route.starts_with("/admin")
            || route.starts_with("/api/admin")
            || route.starts_with("/api/v1/")
            || route.starts_with("/account/security")
            || route.starts_with("/account/passkeys")
            || route.starts_with("/account/privacy"));
    let permit = app.request_work.clone().try_acquire_owned();
    let protection = app
        .protection_limits
        .lock()
        .await
        .check(&app.config.protection, &request);
    let admitted = permit.is_ok() && protection.is_ok();
    let actor = if privileged_write && admitted {
        if route.starts_with("/api/v1/") {
            crate::platform::integrations::authenticate_with_identity(&app, request.headers(), true)
                .await
                .ok()
                .map(|(s, credential_id)| format!("{}:integration:{credential_id}", s.user.id))
                .unwrap_or_default()
        } else {
            auth::session(&app, request.headers())
                .await
                .ok()
                .map(|s| s.user.id)
                .unwrap_or_default()
        }
    } else {
        String::new()
    };
    let mut audit_started = false;
    let audit_failure = if privileged_write && admitted {
        let intent = crate::operations::audit::Event {
            at: now(),
            request_id: id.clone(),
            actor: actor.clone(),
            route: route.clone(),
            phase: "intent".into(),
            status: 0,
        };
        match crate::operations::audit::append(&app, intent).await {
            Ok(()) => {
                audit_started = true;
                None
            }
            Err(_) => Some(Error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Cannot persist privileged action history. Check site storage before retrying.",
            )),
        }
    } else {
        None
    };
    let clone_blocked = app.clone_held.load(std::sync::atomic::Ordering::SeqCst)
        && ((method != axum::http::Method::GET
            && method != axum::http::Method::HEAD
            && route != "/login"
            && route != "/logout")
            || requested_path.starts_with("/members/identity/")
            || requested_path.starts_with("/api/engagement/scripts/")
            || requested_path == "/shop/cart");
    let mut response = if clone_blocked {
        Error(StatusCode::SERVICE_UNAVAILABLE,"This recovered clone is read-only. Review source shutdown, queues, external payment ownership and credentials, then activate it using the stopped-host CLI.").into_response()
    } else if let Some(error) = audit_failure {
        error.into_response()
    } else if let Err(error) = protection {
        tracing::warn!(event="local_request_blocked", route=%route, status=error.0.as_u16());
        error.into_response()
    } else if permit.is_err() {
        Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Server is busy. Try again shortly.",
        )
        .into_response()
    } else if method != axum::http::Method::GET
        && method != axum::http::Method::HEAD
        && auth::same_origin(&app, request.headers()).is_err()
        && !capability_navigation(&route, request.headers())
        && !(method == axum::http::Method::POST && route == "/commerce/stripe/webhook")
        && !((route == "/api/v1/content" && method == axum::http::Method::POST)
            || (route == "/api/v1/content/{id}" && method == axum::http::Method::PUT))
    {
        Error::forbidden().into_response()
    } else {
        use tracing::Instrument;
        // Narrow allowlist: only anonymous discovery/listing output. Private
        // content, forms, carts, previews and account routes are never stored.
        let bucket = crate::operations::variants::annotate(&app, &mut request).await;
        let variant_cookie = app.config.variants.role_variants
            && crate::operations::variants::only_session_cookie(&request)
            && !bucket.ends_with(":anonymous");
        let candidate = app.config.cache.enabled
            && method == axum::http::Method::GET
            && (!request.headers().contains_key("cookie") || variant_cookie)
            && !request.headers().contains_key("authorization")
            && !request.headers().contains_key("range")
            && !request.headers().contains_key("if-none-match")
            && !request.headers().contains_key("if-modified-since")
            && !request.headers().contains_key("cache-control")
            && matches!(
                route.as_str(),
                "/" | "/search"
                    | "/sitemap.xml"
                    | "/sitemap-index.xml"
                    | "/{locale}/"
                    | "/{locale}/search"
                    | "/{slug}"
                    | "/{locale}/{slug}"
                    | "/api/content"
            )
            && request.uri().to_string().len() <= 2048;
        if candidate {
            // This is a read lock: do not increment the mutation generation.
            let _read_guard = app.mutations.lock().await;
            let generation = app
                .cache_generation
                .load(std::sync::atomic::Ordering::SeqCst);
            // Revalidate session role under the mutation lock before every hit.
            let bucket = crate::operations::variants::annotate(&app, &mut request).await;
            let key = format!("{}|{bucket}", request.uri());
            let cached = app
                .page_cache
                .lock()
                .await
                .get(&key, generation, &app.config.cache);
            if let Some(response) = cached {
                response
            } else {
                let response = next.run(request).instrument(span).await;
                let cacheable = response
                    .body()
                    .size_hint()
                    .upper()
                    .is_some_and(|n| n <= app.config.cache.max_bytes as u64)
                    && response.status() == StatusCode::OK
                    && !response.headers().contains_key("set-cookie")
                    && response
                        .headers()
                        .get("cache-control")
                        .and_then(|h| h.to_str().ok())
                        .is_none_or(|h| !h.contains("no-store") && !h.contains("private"));
                if cacheable {
                    let (parts, body) = response.into_parts();
                    match axum::body::to_bytes(body, app.config.cache.max_bytes).await {
                        Ok(bytes) => {
                            app.page_cache.lock().await.insert(key, generation, bytes.clone(), parts.headers.clone(), &app.config.cache);
                            let mut response = Response::from_parts(parts, Body::from(bytes));
                            response.headers_mut().insert("x-wpalt-cache", HeaderValue::from_static("miss"));
                            response
                        }
                        Err(_) => Error(StatusCode::SERVICE_UNAVAILABLE, "Public response exceeds the cache budget; disable caching or increase max_bytes.").into_response(),
                    }
                } else {
                    response
                }
            }
        } else {
            next.run(request).instrument(span).await
        }
    };
    if response.status() == StatusCode::NOT_FOUND
        && (method == axum::http::Method::GET || method == axum::http::Method::HEAD)
    {
        match crate::discovery::redirect_response(&app, &requested_path).await {
            Ok(Some(redirect)) => response = redirect,
            Ok(None) => {}
            Err(error) => response = error.into_response(),
        }
    }
    if route.starts_with("/admin")
        && method == axum::http::Method::GET
        && response.status() == StatusCode::UNAUTHORIZED
    {
        response = Redirect::to("/login").into_response();
    }
    if audit_started {
        let outcome = crate::operations::audit::Event {
            at: now(),
            request_id: id.clone(),
            actor,
            route: route.clone(),
            phase: "response".into(),
            status: response.status().as_u16(),
        };
        if crate::operations::audit::append(&app, outcome)
            .await
            .is_err()
        {
            // The action may have committed. Never replace its successful response
            // with an error that encourages blindly replaying a financial mutation.
            tracing::error!(event="audit_outcome_write_failed", request_id=%id, route=%route);
            response.headers_mut().insert(
                "x-wpalt-audit",
                HeaderValue::from_static("outcome-write-failed"),
            );
        }
    }
    let h = response.headers_mut();
    if route.starts_with("/assets/") {
        let policy = if app.config.cache.asset_cache_seconds == 0 {
            "no-cache".to_owned()
        } else {
            format!("public, max-age={}", app.config.cache.asset_cache_seconds)
        };
        h.insert("cache-control", HeaderValue::from_str(&policy).unwrap());
    }
    if route.starts_with("/admin")
        || route.starts_with("/api/admin")
        || route.starts_with("/api/v1/")
        || route == "/login"
        || route.starts_with("/account")
        || route.starts_with("/passkeys")
        || route.starts_with("/members")
        || route.starts_with("/api/members")
        || route.starts_with("/shop")
        || route.starts_with("/commerce/")
        || (route.starts_with("/audience/") || route.starts_with("/registration/"))
        || route.starts_with("/api/forms/")
        || route.starts_with("/api/engagement/")
    {
        h.insert(
            "x-robots-tag",
            HeaderValue::from_static("noindex, nofollow"),
        );
    }
    if route == "/{slug}"
        || route == "/{locale}/{slug}"
        || route.starts_with("/members")
        || route.starts_with("/api/members")
        || route.starts_with("/shop")
        || route.starts_with("/commerce/")
    {
        h.insert("cache-control", HeaderValue::from_static("no-store"));
    }
    h.insert("x-request-id", HeaderValue::from_str(&id).unwrap());
    app.security_headers.apply(&route, h);
    if route.starts_with("/admin")
        || route.starts_with("/api/admin")
        || route.starts_with("/api/v1/")
        || route == "/login"
        || route.starts_with("/account")
        || route.starts_with("/passkeys")
        || route.starts_with("/members")
        || route.starts_with("/api/members")
        || route.starts_with("/shop")
        || route.starts_with("/commerce/")
        || route == "/logout"
        || (route.starts_with("/audience/") || route.starts_with("/registration/"))
        || route.starts_with("/api/forms/")
        || route.starts_with("/api/engagement/")
    {
        h.insert("cache-control", HeaderValue::from_static("no-store"));
    }
    if response.status().is_server_error() {
        tracing::error!(event="request_failed",request_id=%id,route=%route);
    }
    tracing::info!(event="request_completed",request_id=%id,method=%method,route=%route,status=response.status().as_u16(),elapsed_us=started.elapsed().as_micros() as u64);
    response
}
/// No-referrer proof pages can produce Origin:null on native form navigation.
/// Accept only browser-asserted same-origin navigation on these random-capability
/// routes. Missing/cross-site metadata fails closed; admin/session routes retain
/// their Origin and CSRF checks.
fn capability_navigation(route: &str, headers: &HeaderMap) -> bool {
    [
        "/audience/confirm/{token}",
        "/audience/withdraw/{token}",
        "/registration/{token}",
        "/members/gifts/{token}",
    ]
    .contains(&route)
        && headers.get("origin").is_some_and(|v| v == "null")
        && headers
            .get("sec-fetch-site")
            .is_some_and(|v| v == "same-origin")
        && headers
            .get("sec-fetch-mode")
            .is_some_and(|v| v == "navigate")
        && headers
            .get("sec-fetch-dest")
            .is_some_and(|v| v == "document")
}
async fn admin_session(app: &App, headers: &HeaderMap) -> Result<Session> {
    let session = auth::session(app, headers).await?;
    if session.user.role == "subscriber" {
        return Err(Error::forbidden());
    }
    Ok(session)
}
fn editor(s: &Session) -> Result<()> {
    if s.can_edit() {
        Ok(())
    } else {
        Err(Error::forbidden())
    }
}
fn admin(s: &Session) -> Result<()> {
    if s.is_admin() {
        Ok(())
    } else {
        Err(Error::forbidden())
    }
}
fn html_page(title: &str, settings: &Settings, s: Option<&Session>, body: Markup) -> Html<String> {
    Html(view::layout(title, settings, s, body))
}
async fn admin_css() -> Response {
    (
        [(axum::http::header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../assets/generated/admin-ui.css"),
    )
        .into_response()
}
async fn css() -> impl IntoResponse {
    (
        [
            ("content-type", "text/css; charset=utf-8"),
            ("cache-control", "public, max-age=3600"),
        ],
        include_str!("../assets/app.css"),
    )
}
async fn js() -> impl IntoResponse {
    (
        [
            ("content-type", "text/javascript; charset=utf-8"),
            ("cache-control", "public, max-age=3600"),
        ],
        include_str!("../assets/admin.js"),
    )
}
async fn editor_js() -> impl IntoResponse {
    (
        [
            ("content-type", "text/javascript; charset=utf-8"),
            ("cache-control", "no-cache"),
        ],
        include_str!("../assets/generated/editor.js"),
    )
}
async fn spam_js() -> impl IntoResponse {
    (
        [("content-type", "text/javascript; charset=utf-8")],
        include_str!("../assets/generated/spam.js"),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpamResource {
    resource: String,
}
async fn spam_challenge(
    State(app): State<App>,
    Json(input): Json<SpamResource>,
) -> Result<Json<crate::operations::spam::Challenge>> {
    Ok(Json(crate::operations::spam::issue(&app, &input.resource)?))
}
async fn health(State(app): State<App>) -> Result<Json<serde_json::Value>> {
    let initialized: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings")
        .fetch_one(&app.db.pool)
        .await?;
    if initialized != 1 {
        return Err(Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Site is not initialized.",
        ));
    }
    Ok(Json(
        serde_json::json!({"status":"ok","version":env!("CARGO_PKG_VERSION")}),
    ))
}
async fn login_page(State(app): State<App>, headers: HeaderMap) -> Result<Response> {
    if let Ok(session) = auth::session(&app, &headers).await {
        return Ok(Redirect::to(if session.user.role == "subscriber" {
            "/account"
        } else {
            "/admin"
        })
        .into_response());
    }
    Ok(Html(view::login(
        &app.db.settings().await?,
        app.config.membership_enabled && app.config.identity.enabled,
    ))
    .into_response())
}
#[derive(Deserialize)]
struct Login {
    email: String,
    password: String,
    #[serde(default)]
    code: String,
}
async fn login(State(app): State<App>, Form(input): Form<Login>) -> Result<Response> {
    let (token, session) =
        auth::login_with_code(&app, &input.email, &input.password, &input.code).await?;
    let mut response = Redirect::to(if session.user.role == "subscriber" {
        "/account"
    } else {
        "/admin"
    })
    .into_response();
    response.headers_mut().insert(
        "set-cookie",
        HeaderValue::from_str(&auth::cookie(&app, &token)).unwrap(),
    );
    Ok(response)
}
#[derive(Deserialize)]
struct Csrf {
    csrf: String,
}
async fn logout(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<Csrf>,
) -> Result<Response> {
    let s = auth::session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    let _guard = app.mutation().await;
    sqlx::query("DELETE FROM sessions WHERE token_hash=$1")
        .bind(s.hash)
        .execute(&app.db.pool)
        .await?;
    let mut r = Redirect::to("/login").into_response();
    r.headers_mut().insert(
        "set-cookie",
        HeaderValue::from_static("wpalt_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"),
    );
    Ok(r)
}
async fn dashboard(State(app): State<App>, headers: HeaderMap) -> Result<Response> {
    let s = match admin_session(&app, &headers).await {
        Ok(s) => s,
        Err(error) if error.0 == StatusCode::UNAUTHORIZED => {
            return Ok(Redirect::to("/login").into_response());
        }
        Err(error) => return Err(error),
    };
    let r=sqlx::query("SELECT (SELECT COUNT(*) FROM posts) AS content,(SELECT COUNT(*) FROM posts WHERE status='published') AS published,(SELECT COUNT(*) FROM media) AS media,(SELECT COUNT(*) FROM comments WHERE status='pending') AS pending").fetch_one(&app.db.pool).await?;
    Ok(html_page("Overview",&app.db.settings().await?,Some(&s),html!{
        (view::heading("Workspace","Overview","Create useful content, shape your website and keep your data on your own server."))
        div class="grid" {div class="panel stat" {strong {(r.get::<i64,_>("content"))}span {"Content items"}}div class="panel stat" {strong {(r.get::<i64,_>("published"))}span {"Published"}}div class="panel stat" {strong {(r.get::<i64,_>("media"))}span {"Media files"}}}
        div class="split" {section class="panel" {h2 {"Start with a story."}p class="muted" {"Draft privately, preview your changes, and publish when you are ready. Autosave preserves your working copy without changing the live page."}
            @if s.can_edit(){a class="button" href="/admin/posts/new" {"Create content"} " " a class="button secondary" href="/admin/posts" {"Browse content"}}
        }section class="panel" {p class="eyebrow" {"Owner-controlled"}h2 {"Everything stays here."}p class="muted" {"Content, media, moderation and configuration share one application. No vendor account or cloud activation."}
            @if s.is_admin(){a href="/admin/operations" {"Inspect health & recovery →"}}
        }}
        @if s.can_moderate(){p class="muted" {(r.get::<i64,_>("pending")) " comments await moderation. " a href="/admin/comments" {"Review comments"}}}
    }).into_response())
}
#[derive(Deserialize, Default)]
struct ListQuery {
    q: Option<String>,
    lang: Option<String>,
    category: Option<String>,
    tag: Option<String>,
    after: Option<String>,
}
#[derive(Serialize)]
struct PublicItem {
    id: String,
    slug: String,
    kind: String,
    title: String,
    summary: String,
    #[serde(skip_serializing)]
    fields: serde_json::Value,
    published_at: i64,
}
async fn published_list(app: &App, query: &ListQuery) -> Result<Vec<PublicItem>> {
    let (discovery, _) = crate::discovery::load(app).await?;
    let locale = query.lang.as_deref().unwrap_or(&discovery.default_language);
    discovery.language(locale)?;
    let mut sql = QueryBuilder::<Any>::new(
        "WITH candidates AS MATERIALIZED (SELECT id,published_at FROM posts WHERE status='published'",
    );
    sql.push(crate::membership::PUBLIC_POST);
    sql.push(" AND published_locale=")
        .push_bind(locale)
        .push(if app.db.postgres {
            " AND NOT COALESCE((published_seo::jsonb->>'noindex')::boolean,false)"
        } else {
            " AND COALESCE(json_extract(published_seo,'$.noindex'),0)=0"
        });
    if let Some(q) = query.q.as_ref().filter(|s| !s.trim().is_empty()) {
        if q.len() > 200 {
            return Err(Error::invalid("Search must be 200 characters or fewer."));
        }
        if app.db.postgres {
            sql.push(" AND to_tsvector('simple',published_title || ' ' || published_body) @@ plainto_tsquery('simple',").push_bind(q).push(")");
        } else {
            let phrase = format!("\"{}\"", q.replace('"', "\"\""));
            sql.push(" AND id IN (SELECT id FROM post_search WHERE post_search MATCH ")
                .push_bind(phrase)
                .push(")");
        }
    }
    for (kind, value) in [("category", &query.category), ("tag", &query.tag)] {
        if let Some(value) = value {
            if value.len() > 120 {
                return Err(Error::invalid("Term is too long."));
            }
            sql.push(" AND EXISTS(SELECT 1 FROM published_post_terms pt JOIN terms t ON t.id=pt.term_id WHERE pt.post_id=posts.id AND t.kind=").push_bind(kind).push(" AND t.slug=").push_bind(value).push(")");
        }
    }
    if let Some(cursor) = &query.after {
        let (time, id) = cursor
            .split_once(':')
            .ok_or(Error::invalid("Invalid page cursor."))?;
        let time: i64 = time
            .parse()
            .map_err(|_| Error::invalid("Invalid page cursor."))?;
        if uuid::Uuid::parse_str(id).is_err() {
            return Err(Error::invalid("Invalid page cursor."));
        }
        sql.push(" AND (published_at,id)<(")
            .push_bind(time)
            .push(",")
            .push_bind(id)
            .push(")");
    }
    sql.push(" ORDER BY published_at DESC,id DESC LIMIT 21) SELECT p.id,p.published_locale,p.published_slug AS slug,p.kind,p.published_title AS title,substr(p.published_body,1,220) AS summary,p.published_fields,p.published_at FROM candidates c JOIN posts p ON p.id=c.id ORDER BY c.published_at DESC,c.id DESC");
    let rows = app.db.fetch_builder(&mut sql).await?;
    Ok(rows
        .into_iter()
        .map(|r| PublicItem {
            id: r.get("id"),
            slug: discovery
                .path(
                    &r.get::<String, _>("published_locale"),
                    &r.get::<String, _>("slug"),
                )
                .trim_start_matches('/')
                .to_owned(),
            kind: r.get("kind"),
            title: r.get("title"),
            summary: view::excerpt(&r.get::<String, _>("summary")),
            fields: serde_json::from_str(&r.get::<String, _>("published_fields"))
                .unwrap_or_default(),
            published_at: r.get("published_at"),
        })
        .collect())
}
async fn home(
    State(app): State<App>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Html<String>> {
    render_home(app, query, headers).await
}
async fn localized_home(
    State(app): State<App>,
    headers: HeaderMap,
    Path(locale): Path<String>,
    Query(mut query): Query<ListQuery>,
) -> Result<Html<String>> {
    query.lang = Some(locale);
    render_home(app, query, headers).await
}
async fn render_home(app: App, query: ListQuery, headers: HeaderMap) -> Result<Html<String>> {
    let (discovery, _) = crate::discovery::load(&app).await?;
    let locale = query.lang.as_deref().unwrap_or(&discovery.default_language);
    let language = discovery.language(locale)?;
    let settings = app.db.settings().await?;
    let items = published_list(&app, &query).await?;
    let search = query.q.clone().unwrap_or_default();
    let stored = crate::theme::published(&app, &settings.theme).await?;
    let listing=items.iter().take(20).map(|p|serde_json::json!({"id":p.id,"kind":p.kind,"title":p.title,"url":format!("/{}",p.slug),"body":p.summary,"fields":p.fields})).collect();
    let mut ctx = crate::theme::context_with_discovery(
        &app,
        &settings,
        &stored.package,
        None,
        listing,
        false,
        if search.is_empty() { "home" } else { "search" },
        Some(locale),
        &discovery,
    )
    .await?;
    crate::operations::variants::presentation(&headers, &mut ctx);
    let path = if query.q.is_some() {
        discovery.path(locale, "search")
    } else {
        discovery.path(locale, "")
    };
    ctx.root["_discovery"] = crate::discovery::metadata_with_settings(
        &app,
        None,
        locale,
        &path,
        query.q.is_some(),
        &discovery,
        &settings,
    )
    .await?;
    ctx.root["navigation"] = ctx.root["_discovery"]["navigation"].clone();
    let extra = html! {(crate::discovery::language_nav(&ctx.root["_discovery"]))form class="toolbar" method="get" action=(discovery.path(locale,"search")){label for="search"{"Find something"}input id="search" type="search" name="q" value=(search);button{(language.search_label)}}
    @if items.is_empty(){p class="empty"{"No published content matches yet."}}
    @if items.len()>20{@let last=&items[19];a class="button secondary" href=(format!("{}{}",discovery.path(locale,""),next_url(&query,&format!("{}:{}",last.published_at,last.id)).trim_start_matches('/'))){"Older content →"}}};
    Ok(Html(crate::theme::document(
        &stored,
        &settings,
        &ctx,
        None,
        false,
        if search.is_empty() { "home" } else { "search" },
        extra,
    )?))
}
fn next_url(query: &ListQuery, cursor: &str) -> String {
    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params.append_pair("after", cursor);
    for (name, value) in [
        ("q", &query.q),
        ("lang", &query.lang),
        ("category", &query.category),
        ("tag", &query.tag),
    ] {
        if let Some(value) = value {
            params.append_pair(name, value);
        }
    }
    format!("/?{}", params.finish())
}
async fn public_api(
    State(app): State<App>,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>> {
    let mut items = published_list(&app, &query).await?;
    let next = if items.len() > 20 {
        let p = &items[19];
        Some(format!("{}:{}", p.published_at, p.id))
    } else {
        None
    };
    items.truncate(20);
    Ok(Json(
        serde_json::json!({"items":items,"next_url":next.as_ref().map(|c|format!("/api/content{}",next_url(&query,c).trim_start_matches('/'))),"next":next}),
    ))
}
async fn public_post(
    State(app): State<App>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<Response> {
    let (d, _) = crate::discovery::load(&app).await?;
    if d.languages.iter().any(|l| l.code == slug) {
        return Ok(Redirect::permanent(&d.path(&slug, "")).into_response());
    }
    let candidate = sqlx::query(
        "SELECT published_locale FROM posts WHERE published_slug=$1 AND status='published'",
    )
    .bind(&slug)
    .fetch_optional(&app.db.pool)
    .await?
    .ok_or_else(Error::not_found)?;
    let language: String = candidate.get("published_locale");
    if language != d.default_language {
        return Ok(Redirect::permanent(&d.path(&language, &slug)).into_response());
    }
    render_post(app, headers, language, slug, d).await
}
async fn localized_post(
    State(app): State<App>,
    headers: HeaderMap,
    Path((locale, slug)): Path<(String, String)>,
) -> Result<Response> {
    let (d, _) = crate::discovery::load(&app).await?;
    d.language(&locale)?;
    if locale == d.default_language {
        return Ok(Redirect::permanent(&d.path(&locale, &slug)).into_response());
    }
    render_post(app, headers, locale, slug, d).await
}
async fn render_post(
    app: App,
    headers: HeaderMap,
    locale: String,
    slug: String,
    discovery: crate::discovery::Definition,
) -> Result<Response> {
    let access_id:String=sqlx::query_scalar("SELECT id FROM posts WHERE published_slug=$1 AND published_locale=$2 AND status='published'").bind(&slug).bind(&locale).fetch_optional(&app.db.pool).await?.ok_or_else(Error::not_found)?;
    let viewer = crate::membership::viewer(&app, &headers).await?;
    crate::membership::require(
        &app,
        "post",
        &access_id,
        viewer.as_ref().map(|s| s.user.id.as_str()),
    )
    .await?;
    let p = sqlx::query("SELECT id,kind,author_id,version,status,publish_at,published_at,updated_at,'' AS slug,'' AS title,'' AS body,'{}' AS fields,'[]' AS blocks,'en' AS locale,'' AS translation_group,'{}' AS seo,'' AS document,published_slug,published_title,published_body,published_fields,published_blocks,published_locale,published_translation_group,published_seo,published_document FROM posts WHERE published_slug=$1 AND published_locale=$2 AND status='published'")
        .bind(&slug).bind(&locale)
        .fetch_optional(&app.db.pool)
        .await?
        .map(Post::from_row)
        .ok_or_else(Error::not_found)?;
    let protected: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM member_resources WHERE kind='post' AND resource_id=$1",
    )
    .bind(&p.id)
    .fetch_one(&app.db.pool)
    .await?;
    let comments=sqlx::query("SELECT name,body FROM comments WHERE post_id=$1 AND status='approved' ORDER BY created_at LIMIT 100").bind(&p.id).fetch_all(&app.db.pool).await?;
    let terms=sqlx::query("SELECT t.kind,t.name,t.slug FROM terms t JOIN published_post_terms pt ON pt.term_id=t.id WHERE pt.post_id=$1 ORDER BY t.name").bind(&p.id).fetch_all(&app.db.pool).await?;
    let settings = app.db.settings().await?;
    let stored = crate::theme::published(&app, &settings.theme).await?;
    let mut ctx = crate::theme::context_with_discovery(
        &app,
        &settings,
        &stored.package,
        Some(&p),
        Vec::new(),
        false,
        &p.kind,
        None,
        &discovery,
    )
    .await?;
    let path = discovery.path(&locale, &slug);
    crate::operations::variants::presentation(&headers, &mut ctx);
    ctx.root["_discovery"] = crate::discovery::metadata_with_settings(
        &app,
        Some(&p),
        &locale,
        &path,
        false,
        &discovery,
        &settings,
    )
    .await?;
    ctx.root["navigation"] = ctx.root["_discovery"]["navigation"].clone();
    let extra = html! {(crate::discovery::language_nav(&ctx.root["_discovery"]))            section class="comments" {p class="muted" {@for t in terms {a href=(format!("{}?{}={}",discovery.path(&locale,""),t.get::<String,_>("kind"),t.get::<String,_>("slug"))) {(t.get::<String,_>("name"))} " · "}}
                    h2 {"Conversation"}
                    @for c in comments {article class="comment" {strong {(c.get::<String,_>("name"))}p {(c.get::<String,_>("body"))}}}
                    @if protected == 0 {form method="post" action=(format!("/{slug}/comments")) data-spam-resource=[app.config.spam.enabled.then(||format!("comment:{slug}"))] {
                        @if app.config.spam.enabled {input type="hidden" name="website" value="";p role="status" aria-live="polite" {}}label {"Your name" input name="name" required maxlength="100";}label {"Comment" textarea name="body" required maxlength="4000" {}}
                        p class="muted" {"Comments are reviewed before publication."}button {"Submit for review"}}}
                    @if app.config.spam.enabled {script defer src="/assets/spam.js"{}}
                }
    };
    let mut response = Html(crate::theme::document(
        &stored,
        &settings,
        &ctx,
        Some(&p),
        false,
        &p.kind,
        extra,
    )?)
    .into_response();
    if protected > 0 {
        response
            .headers_mut()
            .insert("cache-control", HeaderValue::from_static("no-store"));
        response.headers_mut().insert(
            "x-robots-tag",
            HeaderValue::from_static("noindex, nofollow"),
        );
    }
    Ok(response)
}
async fn feed(State(app): State<App>) -> Result<Response> {
    let settings = app.db.settings().await?;
    let items = published_list(&app, &ListQuery::default()).await?;
    let markup = html! {rss version="2.0" {channel {title {(settings.title)}link {(app.config.origin())}description {(settings.description)}@for item in items.iter().take(20) {item {title {(item.title)}link {(format!("{}/{}",app.config.origin(),item.slug))}guid {(format!("{}/{}",app.config.origin(),item.slug))}description {(item.summary)}}}}}};
    Ok((
        [("content-type", "application/rss+xml; charset=utf-8")],
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>{}",
            markup.into_string()
        ),
    )
        .into_response())
}
async fn post_list(
    State(app): State<App>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Html<String>> {
    let s = admin_session(&app, &headers).await?;
    editor(&s)?;
    let mut sql =
        QueryBuilder::<Any>::new("SELECT id,title,kind,status,version,updated_at FROM posts");
    if let Some(cursor) = &query.after {
        let (time, id) = cursor
            .split_once(':')
            .ok_or(Error::invalid("Invalid page cursor."))?;
        let time: i64 = time
            .parse()
            .map_err(|_| Error::invalid("Invalid page cursor."))?;
        if uuid::Uuid::parse_str(id).is_err() {
            return Err(Error::invalid("Invalid page cursor."));
        }
        sql.push(" WHERE (updated_at,id)<(")
            .push_bind(time)
            .push(",")
            .push_bind(id)
            .push(")");
    }
    sql.push(" ORDER BY updated_at DESC,id DESC LIMIT 51");
    let rows = app.db.fetch_builder(&mut sql).await?;
    Ok(html_page(
        "Content",
        &app.db.settings().await?,
        Some(&s),
        html! {
            (view::heading("Publishing","Content","Working drafts and published pages live together, with separate public snapshots."))
            div class="toolbar" {a class="button" href="/admin/posts/new" {"Create content"}}
            section class="panel table-wrap" {table {thead {tr {th {"Title"}th {"Type"}th {"Status"}th {"Revision"}}}tbody {@for row in rows.iter().take(50) {tr {td class="title" {a href=(format!("/admin/posts/{}",row.get::<String,_>("id"))) {(row.get::<String,_>("title"))}}td {(row.get::<String,_>("kind"))}td {span class=(format!("status {}",row.get::<String,_>("status"))) {(row.get::<String,_>("status"))}}td {(row.get::<i64,_>("version"))}}}}}
                @if rows.is_empty(){p class="empty" {"Start with your first page or story."}}
                @if rows.len()>50 {@let last=&rows[49];a class="button secondary" href=(format!("/admin/posts?after={}:{}",last.get::<i64,_>("updated_at"),last.get::<String,_>("id"))) {"Older content →"}}
            }
        },
    ))
}
async fn new_post(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let s = admin_session(&app, &headers).await?;
    editor(&s)?;
    let input = PostInput {
        import_markdown: false,
        locale: crate::discovery::load(&app).await?.0.default_language,
        translation_group: String::new(),
        seo: "{}".into(),
        title: String::new(),
        slug: String::new(),
        kind: "post".into(),
        body: String::new(),
        document: String::new(),
        fields: "{}".into(),
        blocks: "[]".into(),
        categories: String::new(),
        tags: String::new(),
        taxonomies: "{}".into(),
        version: 0,
        action: "save".into(),
        publish_at: 0,
        csrf: String::new(),
    };
    Ok(html_page(
        "Create content",
        &app.db.settings().await?,
        Some(&s),
        html! {(editor_form(&s, None, &input, &[], None, &crate::discovery::load(&app).await?.0))script defer src="/assets/builder.js"{}},
    ))
}
fn editor_form(
    s: &Session,
    id: Option<&str>,
    p: &PostInput,
    revisions: &[sqlx::any::AnyRow],
    error: Option<&str>,
    discovery: &crate::discovery::Definition,
) -> Markup {
    let seo = crate::discovery::Seo::parse(&p.seo).unwrap_or_default();
    html! {
        (view::heading("Publishing",if id.is_some(){"Edit content"}else{"New content"},"Write directly, organize your ideas, and publish when you are ready."))
        div class="notice error" data-editor-error hidden[error.is_none()] {(error.unwrap_or(""))}
        form method="post" action=(id.map(|id|format!("/admin/posts/{id}")).unwrap_or_else(||"/admin/posts/new".into())) data-editor data-autosave data-owner=(s.user.id) data-new=(if id.is_some(){"false"}else{"true"}) {
            (view::csrf(s)) input type="hidden" name="version" value=(p.version);input type="hidden" name="publish_at" value=(p.publish_at);
            div class="split" {section class="panel" {label {"Title" input name="title" value=(p.title) required maxlength="300";}input type="hidden" name="document" value=(if p.document.is_empty(){crate::document::import(&p.body,&p.blocks).map(|d|d.encode()).unwrap_or_else(|_|crate::document::empty())}else{p.document.clone()});div data-writing-canvas hidden {}label {"Content" textarea class="editor-body" name="body" aria-label="Content" maxlength="524288" {(p.body)}small {"Markdown import replaces the rich document. Select replacement below to apply edits; use the direct editor to preserve rich blocks."}}label data-markdown-replacement {input type="checkbox" name="import_markdown" value="true" checked[p.document.is_empty()];"Replace content using Markdown"}
                details {summary {"Language & discovery"}
                    label {"Language" select name="locale" aria-label="Language" {@for l in &discovery.languages {option value=(l.code) selected[p.locale==l.code] {(l.label)}}}}
                    label {"Translation group" input name="translation_group" aria-label="Translation group" value=(p.translation_group) maxlength="80";small {"Use the same short identifier for related translations. Each language publishes independently."}}
                    input type="hidden" name="seo" value=(p.seo) data-seo-json;
                    label {"Search title" input name="seo_title" aria-label="Search title" data-seo-title value=(seo.title) maxlength="300";small {"Leave blank to use the published content title."}}
                    label {"Search description" textarea name="seo_description" data-seo-description maxlength="1000" {(seo.description)}}
                    label {input type="checkbox" name="seo_noindex" value="true" data-seo-noindex checked[seo.noindex];"Exclude from search indexing"}
                    label {"Structured content" select name="seo_type" aria-label="Structured content" data-seo-type {option value="WebPage" selected[seo.schema_type=="WebPage"] {"Web page"}option value="Article" selected[seo.schema_type=="Article"] {"Article"}}}
                }
                details {summary {"Typed fields & composition"}label {"Fields (JSON)" textarea name="fields" {(p.fields)}small {"Defined in Site & theme. Example: {\"subtitle\":\"A fresh start\",\"featured\":true}"}}
                    input type="hidden" name="blocks" value="[]";
                }
            }aside class="panel" {h2 {"Publication"}label {"URL slug" input name="slug" aria-label="URL slug" value=(p.slug) required pattern="[a-z0-9-]+" maxlength="120";small {"Lowercase ASCII letters, digits and hyphens."}}
                label {"Content type" select name="kind" aria-label="Content type" {option value="post" selected[p.kind=="post"] {"Post"}option value="page" selected[p.kind=="page"] {"Page"}@if p.kind!="post"&&p.kind!="page"{option value=(p.kind) selected{(p.kind)}}}small {"Type is fixed after creation. Define additional models in Design studio."}}
                label {"Categories" input name="categories" aria-label="Categories" value=(p.categories);small {"Comma-separated names."}}
                label {"Tags" input name="tags" value=(p.tags);}
                label {"Custom taxonomies (JSON)" textarea name="taxonomies" {(p.taxonomies)}small {"Declared taxonomy identifiers mapped to arrays of term names."}}
                label {"Schedule time" input type="datetime-local" data-schedule-time;small {"Uses your browser's local time. Scheduling removes this item from the live site until publication."}}
                div class="toolbar" {button name="action" value="save" {"Save draft"}button class="secondary" name="action" value="publish" {"Publish now"}button class="secondary" name="action" value="schedule" {"Schedule"}}
                @if let Some(id)=id {div class="toolbar" {a class="button secondary" href=(format!("/admin/preview/{id}")) target="_blank" rel="noopener" {"Preview"}button class="secondary" name="action" value="unpublish" {"Unpublish"}}}
                p class="save-status" data-save-status {"Saved working copies do not update the live page."}
            }}
        }
        script defer src="/assets/editor.js"{}
        @if let Some(id)=id {section class="panel" {h2 {"Revision history"}p class="muted" {"Restore a previous working copy, then preview and publish it deliberately."}
            @for r in revisions {form class="toolbar" method="post" action=(format!("/admin/posts/{id}/revisions/{}",r.get::<String,_>("id"))) {(view::csrf(s))input type="hidden" name="version" value=(p.version);span {"Revision " (r.get::<i64,_>("version"))}button class="secondary" {"Restore working copy"}}}
        }}
    }
}
async fn edit_post(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let s = admin_session(&app, &headers).await?;
    editor(&s)?;
    let p = content::get(&app, &id).await?;
    let terms=sqlx::query("SELECT t.name,t.kind FROM terms t JOIN post_terms pt ON pt.term_id=t.id WHERE pt.post_id=$1 ORDER BY t.name").bind(&id).fetch_all(&app.db.pool).await?;
    let names = |kind: &str| {
        terms
            .iter()
            .filter(|r| r.get::<String, _>("kind") == kind)
            .map(|r| r.get::<String, _>("name"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let revisions = sqlx::query(
        "SELECT id,version FROM revisions WHERE post_id=$1 ORDER BY version DESC LIMIT 20",
    )
    .bind(&id)
    .fetch_all(&app.db.pool)
    .await?;
    let input = PostInput {
        import_markdown: false,
        locale: p.locale,
        translation_group: p.translation_group,
        seo: p.seo,
        title: p.title,
        slug: p.slug,
        kind: p.kind,
        body: p.body,
        document: p.document,
        fields: p.fields,
        blocks: p.blocks,
        categories: names("category"),
        tags: names("tag"),
        taxonomies: {
            let mut custom = std::collections::BTreeMap::<String, Vec<String>>::new();
            for row in &terms {
                let kind: String = row.get("kind");
                if kind != "category" && kind != "tag" {
                    custom.entry(kind).or_default().push(row.get("name"));
                }
            }
            serde_json::to_string(&custom).unwrap()
        },
        version: p.version,
        action: "save".into(),
        publish_at: p.publish_at,
        csrf: String::new(),
    };
    Ok(html_page(
        "Edit content",
        &app.db.settings().await?,
        Some(&s),
        html! {(editor_form(&s, Some(&id), &input, &revisions, None, &crate::discovery::load(&app).await?.0))script defer src="/assets/builder.js"{}},
    ))
}
async fn save_form(
    app: &App,
    headers: &HeaderMap,
    id: Option<&str>,
    input: PostInput,
) -> Result<Response> {
    let s = admin_session(app, headers).await?;
    auth::csrf(&s, &input.csrf)?;
    let json = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("application/json"));
    match content::save(app, &s, id, input.clone()).await {
        Ok(p) => {
            if json {
                Ok(
                    Json(serde_json::json!({"id":p.id,"version":p.version,"status":p.status}))
                        .into_response(),
                )
            } else {
                Ok(Redirect::to(&format!("/admin/posts/{}", p.id)).into_response())
            }
        }
        Err(e) => {
            if json {
                Err(e)
            } else {
                Ok((
                    e.0,
                    html_page(
                        "Save needs attention",
                        &app.db.settings().await?,
                        Some(&s),
                        editor_form(
                            &s,
                            id,
                            &input,
                            &[],
                            Some(e.1),
                            &crate::discovery::load(app).await?.0,
                        ),
                    ),
                )
                    .into_response())
            }
        }
    }
}
fn native_content_form(
    mut fields: std::collections::BTreeMap<String, String>,
) -> Result<PostInput> {
    if fields.contains_key("seo_title") {
        let seo = crate::discovery::Seo {
            title: fields.remove("seo_title").unwrap_or_default(),
            description: fields.remove("seo_description").unwrap_or_default(),
            noindex: fields.remove("seo_noindex").is_some(),
            schema_type: fields
                .remove("seo_type")
                .unwrap_or_else(|| "WebPage".into()),
        };
        fields.insert("seo".into(), serde_json::to_string(&seo).unwrap());
    }
    let encoded =
        serde_urlencoded::to_string(fields).map_err(|_| Error::invalid("Invalid content form."))?;
    serde_urlencoded::from_str(&encoded).map_err(|_| Error::invalid("Invalid content form."))
}
async fn create_post(
    State(app): State<App>,
    headers: HeaderMap,
    Form(fields): Form<std::collections::BTreeMap<String, String>>,
) -> Result<Response> {
    save_form(&app, &headers, None, native_content_form(fields)?).await
}
async fn update_post(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(fields): Form<std::collections::BTreeMap<String, String>>,
) -> Result<Response> {
    save_form(&app, &headers, Some(&id), native_content_form(fields)?).await
}
async fn create_api(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<PostInput>,
) -> Result<Json<Post>> {
    let s = admin_session(&app, &headers).await?;
    auth::csrf(
        &s,
        headers
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or(""),
    )?;
    Ok(Json(content::save(&app, &s, None, input).await?))
}
async fn update_api(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<PostInput>,
) -> Result<Json<Post>> {
    let s = admin_session(&app, &headers).await?;
    auth::csrf(
        &s,
        headers
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or(""),
    )?;
    Ok(Json(content::save(&app, &s, Some(&id), input).await?))
}
#[derive(Deserialize)]
struct RevisionInput {
    csrf: String,
    version: i64,
}
async fn restore_revision(
    State(app): State<App>,
    headers: HeaderMap,
    Path((id, revision)): Path<(String, String)>,
    Form(input): Form<RevisionInput>,
) -> Result<Redirect> {
    let s = admin_session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    content::restore_revision(&app, &s, &id, &revision, input.version).await?;
    Ok(Redirect::to(&format!("/admin/posts/{id}")))
}
async fn preview(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let s = admin_session(&app, &headers).await?;
    editor(&s)?;
    let p = content::get(&app, &id).await?;
    let settings = app.db.settings().await?;
    let stored = crate::theme::load(&app, &settings.theme, true).await?;
    let mut ctx = crate::theme::context(
        &app,
        &settings,
        &stored.package,
        Some(&p),
        Vec::new(),
        true,
        &p.kind,
        None,
    )
    .await?;
    let mut working = p.clone();
    working.published_title = p.title.clone();
    working.published_body = p.body.clone();
    working.published_document = p.document.clone();
    working.published_seo = p.seo.clone();
    working.published_translation_group.clear();
    ctx.root["_discovery"] =
        crate::discovery::metadata(&app, Some(&working), &p.locale, "/", false).await?;
    Ok(Html(crate::theme::document(
        &stored,
        &settings,
        &ctx,
        Some(&p),
        true,
        &p.kind,
        html! {p class="notice"{"Private content and theme draft preview. " a href=(format!("/admin/posts/{}",p.id)){"Back to editor"}}},
    )?))
}
/// Bounded editor picker; names/visibility are available only to content editors.
async fn media_picker(
    State(app): State<App>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>> {
    let session = admin_session(&app, &headers).await?;
    editor(&session)?;
    let after = q.after.unwrap_or_default();
    if !after.is_empty() && uuid::Uuid::parse_str(&after).is_err() {
        return Err(Error::invalid("Invalid media cursor."));
    }
    let rows = sqlx::query(
        "SELECT id,original_name,alt,visibility FROM media WHERE mime<>'video/mp4' AND id>$1 ORDER BY id LIMIT 41",
    )
    .bind(after)
    .fetch_all(&app.db.pool)
    .await?;
    let items=rows.iter().take(40).map(|r|serde_json::json!({"id":r.get::<String,_>("id"),"name":r.get::<String,_>("original_name"),"alt":r.get::<String,_>("alt"),"visibility":r.get::<String,_>("visibility")})).collect::<Vec<_>>();
    let next = if rows.len() > 40 {
        Some(rows[39].get::<String, _>("id"))
    } else {
        None
    };
    Ok(Json(serde_json::json!({"items":items,"next":next})))
}
async fn media_list(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let s = admin_session(&app, &headers).await?;
    editor(&s)?;
    let rows=sqlx::query("SELECT id,original_name,mime,visibility,alt,size FROM media ORDER BY created_at DESC,id DESC LIMIT 100").fetch_all(&app.db.pool).await?;
    Ok(html_page(
        "Media",
        &app.db.settings().await?,
        Some(&s),
        html! {
            (view::heading("Assets","Media library","Upload images, describe them and choose who can access them. SVG and executable uploads are not accepted."))
            section class="panel" {form method="post" action="/admin/media" enctype="multipart/form-data" {(view::csrf(&s))div class="field-row" {label {"Image" input type="file" name="file" accept="image/png,image/jpeg,image/webp,image/gif" required;}label {"Visibility" select name="visibility" aria-label="Visibility" {option value="public" {"Public"}option value="private" {"Editors only"}}}}label {"Alternative text" input name="alt" maxlength="500";}button {"Upload image"}}}
            @if s.is_admin(){section class="panel" {h2 {"Local video processing"}p {"Optional owner-installed FFmpeg. MP4/WebM, two minutes maximum, source up to 1920×1080. Produces a bounded 720p MP4; subtitles and metadata are excluded."}form method="post" action="/admin/media/video" enctype="multipart/form-data" data-local-video="true" {(view::csrf(&s))label {"Video source" input type="file" name="file" accept="video/mp4,video/webm" required disabled[!app.config.video.enabled];}label {"Video visibility" select name="visibility" {option value="private" {"Editors only"}option value="public" {"Public"}}}button disabled[!app.config.video.enabled] {"Process local video"}p data-video-status="true" role="status" aria-live="polite" hidden {}} @if !app.config.video.enabled{p class="muted" {"Disabled. Configure absolute ffmpeg/ffprobe paths and enable [video] to use the local worker."}}}}
            div class="cards" {@for r in rows {@let id=r.get::<String,_>("id");section class="panel media-card" {@if r.get::<String,_>("mime")=="video/mp4" {video controls muted preload="metadata" aria-label=(r.get::<String,_>("original_name")) {source src=(format!("/media/{id}")) type="video/mp4";}} @else {img src=(format!("/media/{id}")) alt=(r.get::<String,_>("alt")) loading="lazy";}h3 {(r.get::<String,_>("original_name"))}p class="muted" {(r.get::<i64,_>("size")/1024) " KiB"}@if r.get::<String,_>("mime")=="video/mp4"{p {a href=(format!("/media/{id}")) {"Open processed video"}}} @else {code {(format!("![description](/media/{id})"))}details {summary {"Optimized image sizes"}p class="muted" {"Original access controls apply to every size. GIF animations retain their original file."}p {a href=(format!("/media/{id}/resize/320")) {"320px WebP"} " · " a href=(format!("/media/{id}/resize/640")) {"640px WebP"} " · " a href=(format!("/media/{id}/resize/1280/avif")) {"1280px AVIF"}}}}
                form method="post" action=(format!("/admin/media/{id}")) {(view::csrf(&s))label {"Alternative text" input name="alt" value=(r.get::<String,_>("alt")) maxlength="500";}label {"Visibility" select name="visibility" aria-label="Visibility" {option value="public" selected[r.get::<String,_>("visibility")=="public"] {"Public"}option value="private" selected[r.get::<String,_>("visibility")=="private"] {"Editors only"}}}button class="secondary" {"Save details"}}
            }}}
        },
    ))
}
async fn upload_video(
    State(app): State<App>,
    headers: HeaderMap,
    mut form: Multipart,
) -> Result<Redirect> {
    let s = admin_session(&app, &headers).await?;
    admin(&s)?;
    let mut csrf = String::new();
    let mut name = String::new();
    let mut file = None;
    let mut visibility = "private".to_string();
    while let Some(field) = form
        .next_field()
        .await
        .map_err(|_| Error::invalid("Invalid video upload."))?
    {
        match field.name().unwrap_or("") {
            "csrf" => {
                csrf = field
                    .text()
                    .await
                    .map_err(|_| Error::invalid("Invalid video form."))?
            }
            "visibility" => {
                visibility = field
                    .text()
                    .await
                    .map_err(|_| Error::invalid("Invalid visibility."))?
            }
            "file" => {
                if file.is_some() {
                    return Err(Error::invalid("Upload one video."));
                }
                name = field.file_name().unwrap_or("video").to_string();
                file = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|_| Error::invalid("Video exceeds request limit."))?,
                );
            }
            _ => return Err(Error::invalid("Unknown video upload field.")),
        }
    }
    auth::csrf(&s, &csrf)?;
    if name.len() > 255 || !["public", "private"].contains(&visibility.as_str()) {
        return Err(Error::invalid("Invalid video details."));
    }
    let output = crate::operations::video::transcode(
        &app,
        &file.ok_or_else(|| Error::invalid("Choose a video."))?,
    )
    .await?;
    let _guard = app.mutation().await;
    let current = admin_session(&app, &headers).await?;
    admin(&current)?;
    auth::csrf(&current, &csrf)?;
    let id = uuid::Uuid::new_v4().to_string();
    let filename = format!("{id}.mp4");
    let path = app.config.data_dir.join("media").join(&filename);
    backup::write_private(&path, &output)
        .map_err(|_| Error::invalid("Cannot publish private video output."))?;
    let result=sqlx::query("INSERT INTO media(id,filename,original_name,mime,alt,visibility,size,sha256,created_at) VALUES($1,$2,$3,'video/mp4','',$4,$5,$6,$7)").bind(id).bind(filename).bind(name).bind(visibility).bind(output.len() as i64).bind(auth::digest(&output)).bind(now()).execute(&app.db.pool).await;
    if let Err(e) = result {
        let _ = tokio::fs::remove_file(path).await;
        return Err(e.into());
    }
    Ok(Redirect::to("/admin/media"))
}
async fn upload(
    State(app): State<App>,
    headers: HeaderMap,
    mut form: Multipart,
) -> Result<Redirect> {
    let s = admin_session(&app, &headers).await?;
    editor(&s)?;
    let permit = app.media_work.clone().try_acquire_owned().map_err(|_| {
        Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Image workers are busy. Try again shortly.",
        )
    })?;
    let mut csrf = String::new();
    let mut alt = String::new();
    let mut visibility = "public".to_string();
    let mut file = None;
    while let Some(field) = form
        .next_field()
        .await
        .map_err(|_| Error::invalid("Invalid upload."))?
    {
        match field.name().unwrap_or("") {
            "csrf" => {
                csrf = field
                    .text()
                    .await
                    .map_err(|_| Error::invalid("Invalid upload."))?
            }
            "alt" => {
                alt = field
                    .text()
                    .await
                    .map_err(|_| Error::invalid("Invalid upload."))?
            }
            "visibility" => {
                visibility = field
                    .text()
                    .await
                    .map_err(|_| Error::invalid("Invalid upload."))?
            }
            "file" => {
                if file.is_some() {
                    return Err(Error::invalid("Upload one file at a time."));
                }
                let name = field.file_name().unwrap_or("image").to_owned();
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|_| Error::invalid("Upload exceeds the request limit."))?;
                file = Some((name, bytes));
            }
            _ => return Err(Error::invalid("Unknown upload field.")),
        }
    }
    auth::csrf(&s, &csrf)?;
    if alt.len() > 500 || !["public", "private"].contains(&visibility.as_str()) {
        return Err(Error::invalid("Invalid media details."));
    }
    let (name, bytes) = file.ok_or(Error::invalid("Choose an image."))?;
    if bytes.len() > app.config.max_upload_bytes || name.len() > 255 {
        return Err(Error::invalid("Image exceeds the configured size limit."));
    }
    let format =
        image::guess_format(&bytes).map_err(|_| Error::invalid("Unsupported image format."))?;
    let (ext, mime) = match format {
        image::ImageFormat::Png => ("png", "image/png"),
        image::ImageFormat::Jpeg => ("jpg", "image/jpeg"),
        image::ImageFormat::WebP => ("webp", "image/webp"),
        image::ImageFormat::Gif => ("gif", "image/gif"),
        _ => {
            return Err(Error::invalid(
                "Only PNG, JPEG, WebP and GIF images are accepted.",
            ));
        }
    };
    let copied = bytes.clone();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut reader = image::ImageReader::with_format(Cursor::new(copied), format);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        reader.decode().map(|_| ()).map_err(|_| {
            Error::invalid("Image is invalid or exceeds the 4096-pixel/64-MiB decoding limits.")
        })
    })
    .await
    .map_err(|_| Error::invalid("Image worker failed."))??;
    let id = uuid::Uuid::new_v4().to_string();
    let filename = format!("{id}.{ext}");
    let hash = auth::digest(&bytes);
    let _guard = app.mutation().await;
    let current = admin_session(&app, &headers).await?;
    editor(&current)?;
    auth::csrf(&current, &csrf)?;
    let path = app.config.data_dir.join("media").join(&filename);
    tokio::fs::write(&path, &bytes).await?;
    let result=sqlx::query("INSERT INTO media(id,filename,original_name,mime,alt,visibility,size,sha256,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(id).bind(filename).bind(name).bind(mime).bind(alt).bind(visibility).bind(bytes.len() as i64).bind(hash).bind(now()).execute(&app.db.pool).await;
    if let Err(e) = result {
        let _ = tokio::fs::remove_file(path).await;
        return Err(e.into());
    }
    Ok(Redirect::to("/admin/media"))
}
#[derive(Deserialize)]
struct MediaInput {
    csrf: String,
    alt: String,
    visibility: String,
}
async fn update_media(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(input): Form<MediaInput>,
) -> Result<Redirect> {
    let s = admin_session(&app, &headers).await?;
    editor(&s)?;
    auth::csrf(&s, &input.csrf)?;
    if input.alt.len() > 500 || !["public", "private"].contains(&input.visibility.as_str()) {
        return Err(Error::invalid("Invalid media details."));
    }
    let _guard = app.mutation().await;
    let current = admin_session(&app, &headers).await?;
    editor(&current)?;
    auth::csrf(&current, &input.csrf)?;
    let result = sqlx::query("UPDATE media SET alt=$1,visibility=$2 WHERE id=$3")
        .bind(input.alt)
        .bind(input.visibility)
        .bind(id)
        .execute(&app.db.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(Error::not_found());
    }
    Ok(Redirect::to("/admin/media"))
}
async fn media_derivative(
    State(app): State<App>,
    headers: HeaderMap,
    Path((id, width)): Path<(String, u32)>,
) -> Result<Response> {
    media_derivative_format(State(app), headers, Path((id, width, "webp".into()))).await
}
async fn media_derivative_format(
    State(app): State<App>,
    headers: HeaderMap,
    Path((id, width, format)): Path<(String, u32, String)>,
) -> Result<Response> {
    if ![320, 640, 1280, 1920].contains(&width) || !["webp", "avif"].contains(&format.as_str()) {
        return Err(Error::invalid("Unsupported image derivative."));
    }
    if format == "avif" && width > 1280 {
        return Err(Error::invalid("AVIF supports widths 320,640 and1280."));
    }
    let read_permit = app.media_reads.clone().try_acquire_owned().map_err(|_| {
        Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Image reads are busy. Try again shortly.",
        )
    })?;
    // Same policy and current role checks as original delivery, including private
    // editor-only images and paid entitlement revocation. Never expose a filesystem path.
    let original = media_file(State(app.clone()), headers, Path(id)).await?;
    let bytes = axum::body::to_bytes(original.into_body(), 32 * 1024 * 1024)
        .await
        .map_err(|_| Error::invalid("Cannot read bounded source image."))?;
    let key_format = format.clone();
    let (bytes, key) = tokio::task::spawn_blocking(move || {
        let _permit = read_permit;
        let key = format!("image:{}:{width}:{key_format}", auth::digest(&bytes));
        (bytes, key)
    })
    .await
    .map_err(|_| Error::invalid("Image read worker interrupted."))?;
    let storage = app.config.media.storage();
    if storage.enabled
        && let Some(response) = app.media_cache.lock().await.get(&key, 0, &storage)
    {
        return Ok(response);
    }
    let permit = app.media_work.clone().try_acquire_owned().map_err(|_| {
        Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Image processing is busy. Try again shortly.",
        )
    })?;
    let encoding = format.clone();
    let output = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        crate::operations::media::derivative_format(&bytes, width, &encoding)
    })
    .await
    .map_err(|_| Error::invalid("Image worker interrupted."))??;
    let mut output_headers = HeaderMap::new();
    output_headers.insert(
        "content-type",
        HeaderValue::from_static(if format == "avif" {
            "image/avif"
        } else {
            "image/webp"
        }),
    );
    output_headers.insert("cache-control", HeaderValue::from_static("no-store"));
    let output = axum::body::Bytes::from(output);
    if storage.enabled {
        app.media_cache.lock().await.insert(
            key,
            0,
            output.clone(),
            output_headers.clone(),
            &storage,
        );
    }
    Ok((output_headers, output).into_response())
}
async fn media_file(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response> {
    if uuid::Uuid::parse_str(&id).is_err() {
        return Err(Error::not_found());
    }
    let row = sqlx::query("SELECT filename,mime,visibility FROM media WHERE id=$1")
        .bind(&id)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
    let viewer = crate::membership::viewer(&app, &headers).await?;
    crate::membership::require(
        &app,
        "media",
        &id,
        viewer.as_ref().map(|s| s.user.id.as_str()),
    )
    .await?;
    let gated: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM member_resources WHERE kind='media' AND resource_id=$1",
    )
    .bind(&id)
    .fetch_one(&app.db.pool)
    .await?;
    if row.get::<String, _>("visibility") == "private" && gated == 0 {
        let s = admin_session(&app, &headers).await?;
        editor(&s)?;
    }
    let name: String = row.get("filename");
    if !backup::safe_filename(&name) {
        return Err(Error::not_found());
    }
    let mime: String = row.get("mime");
    if ![
        "image/png",
        "image/jpeg",
        "image/webp",
        "image/gif",
        "video/mp4",
    ]
    .contains(&mime.as_str())
    {
        return Err(Error::invalid("Unsupported stored media type."));
    }
    use tokio::io::AsyncReadExt;
    let file = tokio::fs::File::open(app.config.data_dir.join("media").join(name)).await?;
    let metadata = file.metadata().await?;
    let length = metadata.len();
    if !metadata.is_file() || length == 0 || length > 32 * 1024 * 1024 {
        return Err(Error::invalid("Stored media exceeds its size limit."));
    }
    // Keep slow downloads from retaining a complete image after handler admission ends.
    let stream =
        futures_util::stream::try_unfold((file, length), |(mut file, remaining)| async move {
            if remaining == 0 {
                return Ok::<_, std::io::Error>(None);
            }
            let mut buffer = vec![0; remaining.min(8192) as usize];
            let read = file.read(&mut buffer).await?;
            if read == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Stored media was truncated.",
                ));
            }
            buffer.truncate(read);
            Ok(Some((buffer, (file, remaining - read as u64))))
        });
    Ok((
        [
            ("content-type", mime),
            ("cache-control", "no-store".into()),
            ("content-length", length.to_string()),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}
#[derive(Deserialize)]
struct CommentInput {
    name: String,
    body: String,
    #[serde(default)]
    token: String,
    #[serde(default)]
    solution: String,
    #[serde(default)]
    website: String,
}
async fn comment(
    State(app): State<App>,
    Path(slug): Path<String>,
    connection: Option<Extension<ConnectInfo<SocketAddr>>>,
    Form(input): Form<CommentInput>,
) -> Result<Html<String>> {
    if input.name.trim().is_empty()
        || input.name.len() > 100
        || input.body.trim().is_empty()
        || input.body.len() > 4000
    {
        return Err(Error::invalid(
            "Use a name up to 100 characters and a comment up to 4000 characters.",
        ));
    }
    crate::operations::spam::verify(
        &app,
        &format!("comment:{slug}"),
        &crate::operations::spam::Proof {
            token: input.token,
            solution: input.solution,
            website: input.website,
        },
        &input.body,
    )
    .await?;
    let client = connection
        .map(|c| c.0.0.ip().to_string())
        .unwrap_or_else(|| "local-test".into());
    let key = auth::digest(client.as_bytes());
    let _guard = app.mutation().await;
    let mut tx = app.db.pool.begin().await?;
    let id: String =
        sqlx::query_scalar("SELECT id FROM posts WHERE published_slug=$1 AND status='published'")
            .bind(&slug)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(Error::not_found)?;
    crate::membership::require(&app, "post", &id, None).await?;
    let last: Option<i64> =
        sqlx::query_scalar("SELECT last_at FROM comment_limits WHERE client_hash=$1")
            .bind(&key)
            .fetch_optional(&mut *tx)
            .await?;
    if last.is_some_and(|t| t > now() - 60) {
        return Err(Error(
            StatusCode::TOO_MANY_REQUESTS,
            "Please wait a minute before commenting again.",
        ));
    }
    sqlx::query("INSERT INTO comment_limits(client_hash,last_at) VALUES($1,$2) ON CONFLICT(client_hash) DO UPDATE SET last_at=excluded.last_at").bind(key).bind(now()).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO comments(id,post_id,name,body,status,created_at) VALUES($1,$2,$3,$4,'pending',$5)").bind(uuid::Uuid::new_v4().to_string()).bind(id).bind(input.name.trim()).bind(input.body.trim()).bind(now()).execute(&mut *tx).await?;
    tx.commit().await?;
    drop(_guard);
    Ok(html_page(
        "Comment received",
        &app.db.settings().await?,
        None,
        html! {section class="panel" {h1 {"Thank you for joining in."}p {"Your comment is awaiting review."}a href=(format!("/{slug}")) {"Return to the story →"}}},
    ))
}
async fn comments(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let s = admin_session(&app, &headers).await?;
    if !s.can_moderate() {
        return Err(Error::forbidden());
    }
    let rows=sqlx::query("SELECT c.id,c.name,c.body,c.status,p.title FROM comments c JOIN posts p ON p.id=c.post_id ORDER BY c.created_at DESC LIMIT 100").fetch_all(&app.db.pool).await?;
    Ok(html_page(
        "Comments",
        &app.db.settings().await?,
        Some(&s),
        html! {
            (view::heading("Community","Comments","All public submissions start in the moderation queue."))
            @if rows.is_empty(){section class="panel empty" {"No comments yet."}}
            @for r in rows {section class="panel" {h3 {(r.get::<String,_>("name")) " · " (r.get::<String,_>("title"))}p {(r.get::<String,_>("body"))}span class=(format!("status {}",r.get::<String,_>("status"))) {(r.get::<String,_>("status"))}
                form class="toolbar" method="post" action=(format!("/admin/comments/{}",r.get::<String,_>("id"))) {(view::csrf(&s))button name="status" value="approved" {"Approve"}button class="secondary" name="status" value="rejected" {"Reject"}button class="secondary" name="status" value="pending" {"Hold"}}
            }}
        },
    ))
}
#[derive(Deserialize)]
struct ModerationInput {
    csrf: String,
    status: String,
}
async fn moderate(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(input): Form<ModerationInput>,
) -> Result<Redirect> {
    let s = admin_session(&app, &headers).await?;
    if !s.can_moderate() {
        return Err(Error::forbidden());
    }
    auth::csrf(&s, &input.csrf)?;
    if !["approved", "rejected", "pending"].contains(&input.status.as_str()) {
        return Err(Error::invalid("Invalid moderation decision."));
    }
    let _guard = app.mutation().await;
    let result = sqlx::query("UPDATE comments SET status=$1 WHERE id=$2")
        .bind(input.status)
        .bind(id)
        .execute(&app.db.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(Error::not_found());
    }
    Ok(Redirect::to("/admin/comments"))
}
async fn settings_page(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let s = admin_session(&app, &headers).await?;
    admin(&s)?;
    let settings = app.db.settings().await?;
    let themes = sqlx::query("SELECT id,name FROM themes WHERE published_version>0 ORDER BY name")
        .fetch_all(&app.db.pool)
        .await?;
    Ok(html_page(
        "Site & theme",
        &settings,
        Some(&s),
        html! {
            (view::heading("Configuration","Site settings","Site identity, theme, navigation and typed content definitions share one configuration."))
            form class="panel" method="post" action="/admin/settings" {(view::csrf(&s))label {"Site title" input name="title" value=(settings.title) required maxlength="200";}label {"Description" textarea name="description" maxlength="1000" {(settings.description)}}
                label {"Theme" select name="theme" aria-label="Theme" {@for theme in themes{option value=(theme.get::<String,_>("id")) selected[settings.theme==theme.get::<String,_>("id")] {(theme.get::<String,_>("name"))}}}}
                label {"Navigation (JSON)" textarea name="navigation" {(settings.navigation)}small {"Example: [{\"label\":\"About\",\"url\":\"/about\"}]"}}
                p {a href="/admin/builder" {"Manage typed definitions and advanced themes in Design studio →"}}
                button {"Save site settings"}
            }p {a href="/admin/users" {"Manage administrators, editors & moderators →"}}
        },
    ))
}
#[derive(Deserialize)]
struct SettingsInput {
    csrf: String,
    title: String,
    description: String,
    theme: String,
    navigation: String,
}
async fn save_settings(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<SettingsInput>,
) -> Result<Redirect> {
    let s = admin_session(&app, &headers).await?;
    admin(&s)?;
    auth::csrf(&s, &input.csrf)?;
    let settings = Settings {
        business_enabled: app.config.business_enabled,
        membership_enabled: app.config.membership_enabled,
        commerce_enabled: app.config.commerce.enabled,
        engagement_available: app.config.engagement.enabled,
        analytics: None,
        title: input.title,
        description: input.description,
        theme: input.theme,
        navigation: input.navigation,
        field_schema: app.db.settings().await?.field_schema,
    };
    content::validate_settings(&settings)?;
    crate::theme::load(&app, &settings.theme, false).await?;
    let _guard = app.mutation().await;
    sqlx::query("UPDATE settings SET title=$1,description=$2,theme=$3,navigation=$4 WHERE id=1")
        .bind(settings.title)
        .bind(settings.description)
        .bind(settings.theme)
        .bind(settings.navigation)
        .execute(&app.db.pool)
        .await?;
    Ok(Redirect::to("/admin/settings"))
}
async fn users(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let s = admin_session(&app, &headers).await?;
    admin(&s)?;
    let rows = sqlx::query("SELECT id,name,email,role FROM users ORDER BY name LIMIT 100")
        .fetch_all(&app.db.pool)
        .await?;
    Ok(html_page(
        "Users",
        &app.db.settings().await?,
        Some(&s),
        html! {
            (view::heading("Access","People & access","Administrators manage the site, editors manage content/media, and moderators review comments."))
            section class="panel table-wrap" {table {thead {tr {th {"Name"}th {"Email"}th {"Access"}}}tbody {@for r in rows {tr {td {(r.get::<String,_>("name"))}td {(r.get::<String,_>("email"))}td {
                form method="post" action=(format!("/admin/users/{}",r.get::<String,_>("id"))) {(view::csrf(&s))input type="hidden" name="name" value=(r.get::<String,_>("name"));
                    select name="role" aria-label="Account role" {@for role in ["admin","editor","moderator","subscriber","disabled"] {option value=(role) selected[r.get::<String,_>("role")==role] {(role)}}}
                    input type="password" name="new_password" placeholder="Optional new password" aria-label="New password" autocomplete="new-password" maxlength="256";
                    button class="secondary" {"Update & revoke sessions"}
                }
            }}}}}}
            form class="panel" method="post" action="/admin/users" {(view::csrf(&s))h2 {"Add an account"}div class="field-row" {label {"Name" input name="name" required maxlength="100";}label {"Email" input type="email" name="email" required maxlength="254";}}
                div class="field-row" {label {"Role" select name="role" aria-label="Role" {option value="subscriber" {"Member"}option value="editor" {"Editor"}option value="moderator" {"Moderator"}option value="admin" {"Administrator"}}}label {"Initial password" input type="password" name="password" required minlength="12" maxlength="256" autocomplete="new-password";}}
                button {"Create account"}
            }
        },
    ))
}
#[derive(Deserialize)]
struct UserInput {
    csrf: String,
    name: String,
    email: String,
    role: String,
    password: String,
}
async fn add_user(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<UserInput>,
) -> Result<Redirect> {
    let s = admin_session(&app, &headers).await?;
    admin(&s)?;
    auth::csrf(&s, &input.csrf)?;
    auth::add_user(
        &app,
        &input.email,
        &input.name,
        &input.role,
        &input.password,
    )
    .await
    .map_err(|_| {
        Error::invalid(
            "Account could not be created. Check fields, password length and duplicate email.",
        )
    })?;
    Ok(Redirect::to("/admin/users"))
}
fn wants_html(headers: &HeaderMap) -> bool {
    headers
        .get("accept")
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| h.contains("text/html"))
}
async fn audit_history(State(app): State<App>, headers: HeaderMap) -> Result<Response> {
    let s = admin_session(&app, &headers).await?;
    admin(&s)?;
    let events = crate::operations::audit::read(&app).await?;
    if !wants_html(&headers) {
        return Ok(Json(events).into_response());
    }
    Ok(html_page("Action history",&app.db.settings().await?,Some(&s),html!{
        (view::heading("Operations","Action history","Recent privileged actions. An intent without an outcome may have been interrupted."))
        p {a href="/admin/operations" {"Return to Operations"}}
        div class="table-wrap" {table {thead {tr {th {"Time"}th {"Action"}th {"Phase"}th {"Status"}th {"Actor"}}}tbody {@for event in events {tr {td {(view::timestamp(event.at))}td {(event.route)}td {(event.phase)}td {(event.status)}td {(event.actor)}}}}}}
        p class="muted" {"Private rotating journal, bounded to two 1 MiB files. This history is operational evidence, not a tamper-proof ledger."}
    }).into_response())
}
#[derive(Deserialize)]
struct IntegrityInput {
    csrf: String,
    #[serde(default)]
    after_image: String,
    #[serde(default)]
    after_attachment: String,
}
#[derive(Deserialize)]
struct CleanupInput {
    csrf: String,
    #[serde(default)]
    hash: String,
    #[serde(default)]
    cutoff: i64,
}
async fn cleanup_preview(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<CleanupInput>,
) -> Result<Response> {
    let s = admin_session(&app, &headers).await?;
    admin(&s)?;
    auth::csrf(&s, &input.csrf)?;
    let plan = crate::operations::cleanup::preview(&app).await?;
    if !wants_html(&headers) {
        return Ok(Json(plan).into_response());
    }
    Ok(html_page(
        "Cleanup preview",
        &app.db.settings().await?,
        Some(&s),
        html! {
            (view::heading("Operations","Cleanup preview","Inspect unused media before permanent removal. Drafts, publications, revisions and module records retain referenced images."))
            p { (plan.retained) " referenced files retained · " (plan.candidates.len()) " unused files · " (plan.expired_sessions) " expired sessions" }
            p class="notice" {"Download a verified recovery copy first. Files linked only from external websites or unpublished files on your computer cannot be detected. This preview expires after ten minutes; any changed candidate set requires a new preview."}
            ul {@for item in &plan.candidates {li {code {(item.filename)} " · " (item.bytes) " bytes · " (if item.registered {"library image"} else {"unregistered file / interrupted deletion"})}}}
            form method="post" action="/admin/operations/cleanup/execute" {
                (view::csrf(&s)) input type="hidden" name="hash" value=(plan.hash); input type="hidden" name="cutoff" value=(plan.cutoff);
                button class="danger" {"Permanently remove listed files and expired sessions"}
            }
            p {a class="button secondary" href="/admin/operations" {"Return to Operations"}}
        },
    ).into_response())
}
async fn cleanup_execute(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<CleanupInput>,
) -> Result<Response> {
    let s = admin_session(&app, &headers).await?;
    admin(&s)?;
    auth::csrf(&s, &input.csrf)?;
    let result = crate::operations::cleanup::execute(&app, &input.hash, input.cutoff).await?;
    if !wants_html(&headers) {
        return Ok(Json(result).into_response());
    }
    Ok(html_page(
        "Cleanup result",
        &app.db.settings().await?,
        Some(&s),
        html! {
            (view::heading("Operations","Cleanup result","Review completed removal and retryable storage failures."))
            p {(result.removed_files) " files removed · " (result.removed_bytes) " bytes · " (result.removed_sessions) " expired sessions removed"}
            @if !result.pending_files.is_empty() {p class="notice" {"Some files could not be unlinked. Their library entries have been removed; inspect storage permissions and preview again to retry."}ul {@for name in result.pending_files {li {code {(name)}}}}}
            p {a class="button secondary" href="/admin/operations" {"Return to Operations"}}
        },
    ).into_response())
}
async fn integrity_scan(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<IntegrityInput>,
) -> Result<Response> {
    let s = admin_session(&app, &headers).await?;
    admin(&s)?;
    auth::csrf(&s, &input.csrf)?;
    let report =
        crate::operations::integrity::scan_page(&app, &input.after_image, &input.after_attachment)
            .await?;
    if !wants_html(&headers) {
        return Ok(Json(report).into_response());
    }
    Ok(html_page("Stored-file inspection",&app.db.settings().await?,Some(&s),html!{
        (view::heading("Operations","Stored-file inspection","Validate recorded checksums and flag embedded executable patterns for owner review."))
        p {"Checked " (report.checked) " files · " (report.failed.len()) " integrity findings · " (report.pattern_warnings.len()) " pattern warnings"}
        @if report.limited {p class="notice" {"The bounded scan has more files to inspect."}form method="post" action="/admin/operations/integrity" {(view::csrf(&s))input type="hidden" name="after_image" value=(report.next_image);input type="hidden" name="after_attachment" value=(report.next_attachment);button {"Continue inspection"}}}
        @for finding in report.failed.iter().chain(report.pattern_warnings.iter()) {section class="panel" {h2 {(finding.kind) " · " (finding.id)}p {(finding.reason)}}}
        p {"Patterns can be false positives and are not current malware intelligence. No files were modified."}
        p {a href="/admin/operations" {"Return to Operations"}}
    }).into_response())
}
async fn cache_preload(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<Csrf>,
) -> Result<Redirect> {
    let s = admin_session(&app, &headers).await?;
    admin(&s)?;
    auth::csrf(&s, &input.csrf)?;
    if !app.config.cache.enabled {
        return Err(Error::invalid("Enable public caching before preloading."));
    }
    let _permit = app
        .media_work
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::invalid("Maintenance workers are busy."))?;
    // Explicit bounded owner action: anonymous/public rows only, no network fetch,
    // no authentication variants and no persistent cache after process restart.
    let rows=sqlx::query("SELECT published_slug,published_locale FROM posts WHERE status='published' AND NOT EXISTS(SELECT 1 FROM member_resources r WHERE r.kind='post' AND r.resource_id=posts.id) ORDER BY published_at DESC,id DESC LIMIT 20").fetch_all(&app.db.pool).await?;
    for row in rows {
        let _read = app.mutations.lock().await;
        let (discovery, _) = crate::discovery::load(&app).await?;
        let generation = app
            .cache_generation
            .load(std::sync::atomic::Ordering::SeqCst);
        let locale: String = row.get("published_locale");
        let slug: String = row.get("published_slug");
        let key = format!("{}|unknown:anonymous", discovery.path(&locale, &slug));
        let response = render_post(
            app.clone(),
            HeaderMap::new(),
            locale,
            slug,
            discovery.clone(),
        )
        .await?;
        if response.status() != StatusCode::OK || response.headers().contains_key("cache-control") {
            continue;
        }
        let (parts, body) = response.into_parts();
        let bytes = axum::body::to_bytes(body, app.config.cache.max_bytes)
            .await
            .map_err(|_| Error::invalid("Preload response exceeds cache budget."))?;
        app.page_cache.lock().await.insert(
            key,
            generation,
            bytes,
            parts.headers,
            &app.config.cache,
        );
    }
    Ok(Redirect::to("/admin/operations"))
}
async fn cache_purge(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<Csrf>,
) -> Result<Redirect> {
    let session = admin_session(&app, &headers).await?;
    admin(&session)?;
    auth::csrf(&session, &input.csrf)?;
    let _guard = app.mutation().await;
    app.page_cache.lock().await.clear();
    Ok(Redirect::to("/admin/operations"))
}
async fn run_recovery(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<Csrf>,
) -> Result<Redirect> {
    let session = admin_session(&app, &headers).await?;
    admin(&session)?;
    auth::csrf(&session, &input.csrf)?;
    crate::operations::recovery::run(&app).await?;
    Ok(Redirect::to("/admin/operations"))
}
async fn operations(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let s = admin_session(&app, &headers).await?;
    admin(&s)?;
    let recovery = crate::operations::recovery::status(&app).await?;
    let jobs = crate::operations::jobs::read(&app).await?;
    let cache = app.page_cache.lock().await.statistics();
    Ok(html_page(
        "Operations",
        &app.db.settings().await?,
        Some(&s),
        html! {
            (view::heading("Operations","Operations","Manual snapshots, portable content and useful diagnostics without cloud dependencies."))
            @if app.clone_held.load(std::sync::atomic::Ordering::SeqCst) {
                section class="panel" {h2 {"Read-only recovered clone"}p {"Background work and HTTP writes are paused. Review source shutdown, message queues, external payment ownership, identity callbacks and credentials before stopped-host activation. Presentation previews remain available; source sessions and passkeys were removed."}}
            }
            div class="split" {section class="panel" {h2 {"Back up & move"}p {a href="/admin/migration" {"Assess a WordPress migration"}}p {a href="/admin/integrations" {"Manage integrations"}}p class="muted" {"Download a consistent database-and-media snapshot. It includes password hashes and private content; keep it secure. This download is unencrypted."}
                form method="post" action="/admin/backup" {(view::csrf(&s))button {"Download full backup"}}
                p class="muted" {"Restore with the CLI into an empty database/data directory while the server is stopped. Keep an independent copy to recover from losing this host."}
                a href="/admin/export" {"Export portable content JSON →"}
            }section class="panel" {h2 {"Runtime"}p {"Database: " strong {(if app.db.postgres{"PostgreSQL"}else{"SQLite · WAL"})}}p {"Version: " (env!("CARGO_PKG_VERSION"))}p {"Scheduler: " (app.config.scheduler_seconds) " seconds"}p {"Debug: " (app.config.debug)}p {a href="/health" {"Readiness endpoint →"}}}}
            section class="panel" {
                h2 {"Managed recovery"}
                @if app.config.recovery.enabled {
                    p {"Encrypted copies every " (app.config.recovery.interval_seconds) " seconds. Retain " (app.config.recovery.retain) " packages per destination."}
                    p {"Last attempt: " (view::timestamp(recovery.last_attempt)) ". Last complete set: " (view::timestamp(recovery.last_complete)) "."}
                    @if !recovery.package.is_empty() {p class="muted" {(recovery.package)}}
                    @for copy in &recovery.copies {p {(copy.destination.display()) " · " strong {(copy.state)}}}
                    form method="post" action="/admin/recovery/run" {(view::csrf(&s))button {"Create encrypted recovery copies"}}
                } @else {
                    p {"Configure a private recovery key, existing destinations, interval and retention in [recovery] to enable scheduled encrypted copies."}
                }
                p class="muted" {"Keep the recovery key separately. Verify an independent copy by restoring into a fresh instance. A pending attempt after restart may have been interrupted; inspect destination packages before retrying."}
            }
            section class="panel" {
                h2 {"Background work"}
                p {a class="button secondary" href="/admin/privacy" {"Review data requests"}}
                p {"Latest 64 scheduler cycles retained locally. Unresolved cycles may be interrupted; history never replays payments or deliveries. The next scheduled poll uses each feature’s own retry rules."}
                @if jobs.is_empty() {p class="muted" {"No scheduler cycles recorded yet."}}
                @for job in jobs.iter().take(8) {
                    details {summary {(view::timestamp(job.started_at)) " · " strong {(&job.state)} " · " (job.elapsed_ms) " ms"}
                        @for stage in &job.stages {p {(&stage.name) " · " (if stage.succeeded {"completed"} else {"failed; inspect feature status"}) @if let Some(count) = stage.count {" · " (count) " processed"} " · " (stage.elapsed_ms) " ms"}}
                    }
                }
            }
            section class="panel" {h2 {"Browser security"}p {"Strict same-origin script/style policy, inactive script-free previews and denied camera/microphone/location access."}p {"HSTS on configured HTTPS: " (app.config.headers.hsts_seconds) " seconds · Include subdomains: " (app.config.headers.hsts_include_subdomains) " · Opener isolation: " (app.config.headers.isolate_opener)}p class="muted" {"Review TLS for every subdomain before enabling include-subdomains. Configuration is compiled at startup; inspect the redacted effective settings below."}}
            section class="panel" {h2 {"Presentation variants"}p {"Current-role cache buckets: " (if app.config.variants.role_variants {"enabled"} else {"disabled"}) " · " (app.config.variants.region_networks.len()) " native-peer region networks"}p {"Role and region labels customize presentation; protected-resource authorization remains separate. Extra cookies bypass shared cache, and forwarded-IP headers are ignored."}}
            section class="panel" {h2 {"Asset loading"}p {"Template-scoped CSS: " (if app.config.assets.scoped_theme_css {"enabled"} else {"disabled"}) " · Theme preload: " (if app.config.assets.preload_theme_css {"enabled"} else {"disabled"})}p {"Local compiled assets and render-reachable styles preserve responsive/conditional presentation. Image insertion records dimensions; choose early loading for an important lead image."}}
            section class="panel" {h2 {"Public response cache"}p {(if app.config.cache.enabled {"Enabled"}else{"Disabled"}) " · " (cache.0) " entries · " (cache.1) " bytes retained"}p {"Public publications, listings, content projections and sitemaps only. Extra cookies, credentials and protected resources bypass shared storage; optional current-role variants use separate buckets. Browser page caching remains disabled so access changes take effect."}form method="post" action="/admin/operations/cache/purge" {(view::csrf(&s))button class="secondary" {"Purge public cache"}} form method="post" action="/admin/operations/cache/preload" {(view::csrf(&s))button class="secondary" disabled[!app.config.cache.enabled] {"Preload recent public pages"}}}
            section class="panel" {h2 {"Local submission guard"}p {(if app.config.spam.enabled {"Enabled"} else {"Disabled"}) " for public comments and forms."}p class="muted" {"When enabled, submissions need a short same-origin computation. Honeypots, link limits, moderation and existing rate limits work together. This does not identify humans or use shared reputation. Configure [spam] to adjust the policy."}}
            section class="panel" {h2 {"Stored-file integrity"}p {a href="/admin/operations/audit" {"Inspect privileged action history"}}
                p {"Check database-recorded image and private attachment checksums without changing files. A bounded scan reports incomplete work; it does not certify malware-free content."}
                form method="post" action="/admin/operations/integrity" {(view::csrf(&s))button class="secondary" {"Inspect stored-file integrity"}} form method="post" action="/admin/operations/cleanup" {(view::csrf(&s))button class="secondary" {"Preview unused media cleanup"}}
            }
            details class="panel" {summary {"Effective configuration · secrets redacted"}pre class="inline-code" {(serde_json::to_string_pretty(&app.config.redacted()).unwrap())}}
        },
    ))
}
async fn download_backup(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<Csrf>,
) -> Result<Response> {
    let s = admin_session(&app, &headers).await?;
    admin(&s)?;
    auth::csrf(&s, &input.csrf)?;
    let bytes = backup::capture(&app).await?;
    Ok((
        [
            ("content-type", "application/json"),
            (
                "content-disposition",
                "attachment; filename=wpalt-backup.json",
            ),
            ("cache-control", "no-store"),
        ],
        bytes,
    )
        .into_response())
}
async fn export_content(State(app): State<App>, headers: HeaderMap) -> Result<Response> {
    let s = admin_session(&app, &headers).await?;
    admin(&s)?;
    use futures_util::TryStreamExt;
    let mut rows = sqlx::query("SELECT * FROM posts ORDER BY id LIMIT 10001").fetch(&app.db.pool);
    let mut bytes = br#"{"format":"wpalt-content-v2","posts":["#.to_vec();
    let mut count = 0;
    while let Some(row) = rows.try_next().await? {
        count += 1;
        if count > 10000 {
            return Err(Error::invalid(
                "Content export is limited to 10,000 items; use full backup.",
            ));
        }
        let encoded = serde_json::to_vec(&Post::from_row(row))
            .map_err(|_| Error::invalid("Content export serialization failed."))?;
        if bytes.len().saturating_add(encoded.len()).saturating_add(3) > app.config.max_backup_bytes
        {
            return Err(Error::invalid(
                "Content export exceeds max_backup_bytes; increase the configured budget before retrying.",
            ));
        }
        if count > 1 {
            bytes.push(b',');
        }
        bytes.extend(encoded);
    }
    bytes.extend(b"]}");
    Ok((
        [
            ("content-type", "application/json"),
            (
                "content-disposition",
                "attachment; filename=wpalt-content.json",
            ),
            ("cache-control", "no-store"),
        ],
        bytes,
    )
        .into_response())
}

#[derive(Deserialize)]
struct UserUpdate {
    csrf: String,
    name: String,
    role: String,
    #[serde(default)]
    new_password: String,
}
async fn update_user(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(input): Form<UserUpdate>,
) -> Result<Redirect> {
    let s = admin_session(&app, &headers).await?;
    admin(&s)?;
    auth::csrf(&s, &input.csrf)?;
    auth::update_user(&app, &id, &input.name, &input.role, &input.new_password).await?;
    Ok(Redirect::to("/admin/users"))
}

#[derive(Deserialize)]
struct FactorInput {
    csrf: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    code: String,
}
#[derive(Deserialize)]
struct PasskeyStart {
    #[serde(default)]
    csrf: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    email: String,
}
#[derive(Deserialize)]
struct PasskeyRegistrationInput {
    csrf: String,
    id: String,
    credential: webauthn_rs::prelude::RegisterPublicKeyCredential,
}
#[derive(Deserialize)]
struct PasskeyRemove {
    csrf: String,
    credential_id: String,
    password: String,
    #[serde(default)]
    code: String,
}
async fn passkey_register_start(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<PasskeyStart>,
) -> Result<Json<crate::operations::passkeys::Challenge>> {
    let s = auth::session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    Ok(Json(
        crate::operations::passkeys::register_start(&app, &s, &input.password, &input.code).await?,
    ))
}
async fn passkey_register_finish(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<PasskeyRegistrationInput>,
) -> Result<Json<serde_json::Value>> {
    let s = auth::session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    crate::operations::passkeys::register_finish(
        &app,
        &s,
        crate::operations::passkeys::Registration {
            id: input.id,
            credential: input.credential,
        },
    )
    .await?;
    Ok(Json(serde_json::json!({"redirect":"/account/security"})))
}
async fn passkey_login_start(
    State(app): State<App>,
    Json(input): Json<PasskeyStart>,
) -> Result<Json<crate::operations::passkeys::Challenge>> {
    Ok(Json(
        crate::operations::passkeys::authenticate_start(&app, &input.email).await?,
    ))
}
async fn passkey_login_finish(
    State(app): State<App>,
    Json(input): Json<crate::operations::passkeys::Authentication>,
) -> Result<Response> {
    let (token, s) = crate::operations::passkeys::authenticate_finish(&app, input).await?;
    let mut response = Json(
        serde_json::json!({"redirect":if s.user.role=="subscriber"{"/account"}else{"/admin"}}),
    )
    .into_response();
    response.headers_mut().insert(
        "set-cookie",
        HeaderValue::from_str(&auth::cookie(&app, &token)).unwrap(),
    );
    Ok(response)
}
async fn passkey_remove(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<PasskeyRemove>,
) -> Result<Redirect> {
    let s = auth::session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    let hash =
        crate::operations::factor::authorize_change(&app, &s, &input.password, &input.code).await?;
    let _guard = app.mutation().await;
    crate::operations::factor::current_credential(&app, &s, &hash).await?;
    let mut tx = app.db.pool.begin().await?;
    if sqlx::query("DELETE FROM user_passkeys WHERE credential_id=$1 AND user_id=$2")
        .bind(input.credential_id)
        .bind(&s.user.id)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        != 1
    {
        return Err(Error::not_found());
    }
    sqlx::query("DELETE FROM sessions WHERE user_id=$1 AND token_hash<>$2")
        .bind(&s.user.id)
        .bind(&s.hash)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Redirect::to("/account/security"))
}
async fn auth_js() -> impl IntoResponse {
    (
        [
            ("content-type", "text/javascript; charset=utf-8"),
            ("cache-control", "no-cache"),
        ],
        include_str!("../assets/generated/auth.js"),
    )
}
fn security_page(
    title: &str,
    settings: &Settings,
    session: &Session,
    body: Markup,
) -> Html<String> {
    if session.user.role == "subscriber" {
        Html(view::member_layout(title, settings, body))
    } else {
        html_page(title, settings, Some(session), body)
    }
}
async fn factor_page(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let session = auth::session(&app, &headers).await?;
    let enabled: Option<String> =
        sqlx::query_scalar("SELECT secret FROM user_factors WHERE user_id=$1")
            .bind(&session.user.id)
            .fetch_optional(&app.db.pool)
            .await?;
    let passkey_rows = sqlx::query(
        "SELECT credential_id FROM user_passkeys WHERE user_id=$1 ORDER BY credential_id LIMIT 8",
    )
    .bind(&session.user.id)
    .fetch_all(&app.db.pool)
    .await?;
    Ok(security_page(
        "Account security",
        &app.db.settings().await?,
        &session,
        html! {
            (view::heading("Account","Account security","Protect local sign-in with an authenticator you control."))
            @if enabled.is_some_and(|s|!s.is_empty()) {
                p {"Authenticator enabled. Sign in with password plus a fresh six-digit code or one-use recovery code."}
                form class="panel" method="post" action="/account/security/disable" {(view::csrf(&session))label {"Current password" input type="password" name="password" required autocomplete="current-password";}label {"Authenticator or recovery code" input name="code" required autocomplete="one-time-code" maxlength="24";}button class="secondary" {"Disable authenticator & revoke other sessions"}}
            } @else {
                form class="panel" method="post" action="/account/security/begin" {(view::csrf(&session))label {"Current password" input type="password" name="password" required autocomplete="current-password";}button {"Set up authenticator"}}
            }
            section class="panel" {h2 {"Passkeys"}p {"Register a device with user verification. Passkeys can sign in independently; no vendor account is required by wpalt."}
            form data-passkey="register" {(view::csrf(&session))label {"Current password" input type="password" name="password" required autocomplete="current-password";}label {"Authenticator or recovery code · if enabled" input name="code" autocomplete="one-time-code" maxlength="24";}button {"Register a passkey"}p role="status" aria-live="polite" {}}
            @for key in &passkey_rows {
                form method="post" action="/account/passkeys/remove" {(view::csrf(&session))input type="hidden" name="credential_id" value=(key.get::<String,_>("credential_id"));p {"Registered passkey " (key.get::<String,_>("credential_id").chars().take(12).collect::<String>())}label {"Current password" input type="password" name="password" required autocomplete="current-password";}label {"Authenticator or recovery code · if enabled" input name="code" autocomplete="one-time-code" maxlength="24";}button class="secondary" {"Remove passkey & revoke sessions"}}
            }
        }
        script defer src="/assets/auth.js" {}
        p {a href="/account" {"Return to account"}}
        },
    ))
}
async fn factor_begin(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<FactorInput>,
) -> Result<Html<String>> {
    let s = auth::session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    let origin = url::Url::parse(&app.config.base_url).map_err(|_| Error::forbidden())?;
    if !app.config.secure_cookie()
        && !matches!(origin.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
    {
        return Err(Error::invalid(
            "Authenticator enrollment requires HTTPS outside localhost.",
        ));
    }
    let uri = crate::operations::factor::begin(&app, &s, &input.password).await?;
    Ok(security_page(
        "Confirm authenticator",
        &app.db.settings().await?,
        &s,
        html! {
            h1 {"Confirm authenticator"}p {"Add this setup URI to your authenticator. Keep it private; it expires in ten minutes."}
            pre class="inline-code" {(uri)}
            form class="panel" method="post" action="/account/security/confirm" {(view::csrf(&s))label {"Six-digit code" input name="code" required inputmode="numeric" autocomplete="one-time-code" minlength="6" maxlength="6";}button {"Enable authenticator"}}
        },
    ))
}
async fn factor_confirm(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<FactorInput>,
) -> Result<Html<String>> {
    let s = auth::session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    let codes = crate::operations::factor::confirm(&app, &s, &input.code).await?;
    Ok(security_page(
        "Recovery codes",
        &app.db.settings().await?,
        &s,
        html! {
            h1 {"Save recovery codes"}p {"Authenticator enabled. Each code works once. Keep these codes offline; they are shown only now."}
            pre class="inline-code" {(codes.join("\n"))}p {a href="/account/security" {"Return to account security"}}
        },
    ))
}
async fn factor_disable(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<FactorInput>,
) -> Result<Redirect> {
    let s = auth::session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    crate::operations::factor::disable(&app, &s, &input.password, &input.code).await?;
    Ok(Redirect::to("/account/security"))
}
async fn account(State(app): State<App>, headers: HeaderMap) -> Result<Response> {
    let s = auth::session(&app, &headers).await?;
    Ok(html_page("Your account",&app.db.settings().await?,None,html!{h1 {"Your account"}p {"Signed in as " (&s.user.name)}p {"Role: " (&s.user.role)}p {a href="/account/security" {"Account security"} " · " a href="/account/privacy" {"Data and privacy"}}@if s.user.role=="subscriber" {p {"This subscriber account does not grant access to site administration."}} @else {p {a href="/admin" {"Open site administration"}}}@if app.config.membership_enabled {p {a href="/members" {"Open my learning and communities"}}}form method="post" action="/logout" {(view::csrf(&s))button {"Sign out"}}}).into_response())
}

async fn form_embed_js(State(app): State<App>) -> Result<Response> {
    if !app.config.business_enabled {
        return Err(Error::not_found());
    }
    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "text/javascript; charset=utf-8",
        )],
        include_str!("../assets/form-embed.js"),
    )
        .into_response())
}
