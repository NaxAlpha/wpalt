use crate::{
    App, auth, backup, content,
    error::{Error, Result},
    model::{Post, PostInput, Session, Settings},
    now, view,
};
use axum::{
    Extension, Json, Router,
    body::Body,
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
    let business = if app.config.business_enabled {
        crate::business::web::routes()
    } else {
        Router::new()
    };
    Router::new()
        .merge(crate::builder_web::routes())
        .merge(business)
        .merge(crate::discovery::routes())
        .route("/", get(home))
        .route("/search", get(home))
        .route("/health", get(health))
        .route("/login", get(login_page).post(login))
        .route("/logout", post(logout))
        .route("/feed.xml", get(feed))
        .route("/assets/app.css", get(css))
        .route("/assets/admin-ui.css", get(admin_css))
        .route("/assets/admin.js", get(js))
        .route("/assets/editor.js", get(editor_js))
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
        .route("/admin/media/{id}", post(update_media))
        .route("/media/{id}", get(media_file))
        .route("/admin/comments", get(comments))
        .route("/admin/comments/{id}", post(moderate))
        .route("/admin/settings", get(settings_page).post(save_settings))
        .route("/admin/users", get(users).post(add_user))
        .route("/admin/users/{id}", post(update_user))
        .route("/admin/operations", get(operations))
        .route("/admin/backup", post(download_backup))
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
        .layer(tower_http::compression::CompressionLayer::new().gzip(true))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(timeout),
        ))
        .layer(middleware::from_fn_with_state(
            app.clone(),
            security_and_trace,
        ))
        .with_state(app)
}
async fn security_and_trace(
    State(app): State<App>,
    request: Request<Body>,
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
    let permit = app.request_work.clone().try_acquire_owned();
    let mut response = if permit.is_err() {
        Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Server is busy. Try again shortly.",
        )
        .into_response()
    } else if method != axum::http::Method::GET
        && method != axum::http::Method::HEAD
        && auth::same_origin(&app, request.headers()).is_err()
    {
        Error::forbidden().into_response()
    } else {
        use tracing::Instrument;
        next.run(request).instrument(span).await
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
    let h = response.headers_mut();
    if route.starts_with("/admin") || route.starts_with("/api/admin") || route == "/login" {
        h.insert(
            "x-robots-tag",
            HeaderValue::from_static("noindex, nofollow"),
        );
    }
    h.insert("x-request-id", HeaderValue::from_str(&id).unwrap());
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        "referrer-policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    h.insert("content-security-policy",HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'"));
    if route == "/admin/design/{id}/preview" || route == "/admin/preview/{id}" {
        h.insert("content-security-policy",HeaderValue::from_static("default-src 'self'; script-src 'none'; style-src 'self'; img-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'self'; form-action 'none'"));
    }
    h.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    if app.config.secure_cookie() {
        h.insert(
            "strict-transport-security",
            HeaderValue::from_static("max-age=31536000"),
        );
    }
    if route.starts_with("/admin") || route.starts_with("/api/admin") || route == "/login" {
        h.insert("cache-control", HeaderValue::from_static("no-store"));
    }
    if response.status().is_server_error() {
        tracing::error!(event="request_failed",request_id=%id,route=%route);
    }
    tracing::info!(event="request_completed",request_id=%id,method=%method,route=%route,status=response.status().as_u16(),elapsed_us=started.elapsed().as_micros() as u64);
    response
}
async fn admin_session(app: &App, headers: &HeaderMap) -> Result<Session> {
    auth::session(app, headers).await
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
    if auth::session(&app, &headers).await.is_ok() {
        return Ok(Redirect::to("/admin").into_response());
    }
    Ok(Html(view::login(&app.db.settings().await?)).into_response())
}
#[derive(Deserialize)]
struct Login {
    email: String,
    password: String,
}
async fn login(State(app): State<App>, Form(input): Form<Login>) -> Result<Response> {
    let (token, _) = auth::login(&app, &input.email, &input.password).await?;
    let mut response = Redirect::to("/admin").into_response();
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
    let s = admin_session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    let _guard = app.mutations.lock().await;
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
        Err(_) => return Ok(Redirect::to("/login").into_response()),
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
async fn home(State(app): State<App>, Query(query): Query<ListQuery>) -> Result<Html<String>> {
    render_home(app, query).await
}
async fn localized_home(
    State(app): State<App>,
    Path(locale): Path<String>,
    Query(mut query): Query<ListQuery>,
) -> Result<Html<String>> {
    query.lang = Some(locale);
    render_home(app, query).await
}
async fn render_home(app: App, query: ListQuery) -> Result<Html<String>> {
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
async fn public_post(State(app): State<App>, Path(slug): Path<String>) -> Result<Response> {
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
    render_post(app, language, slug, d).await
}
async fn localized_post(
    State(app): State<App>,
    Path((locale, slug)): Path<(String, String)>,
) -> Result<Response> {
    let (d, _) = crate::discovery::load(&app).await?;
    d.language(&locale)?;
    if locale == d.default_language {
        return Ok(Redirect::permanent(&d.path(&locale, &slug)).into_response());
    }
    render_post(app, locale, slug, d).await
}
async fn render_post(
    app: App,
    locale: String,
    slug: String,
    discovery: crate::discovery::Definition,
) -> Result<Response> {
    let p = sqlx::query("SELECT id,kind,author_id,version,status,publish_at,published_at,updated_at,'' AS slug,'' AS title,'' AS body,'{}' AS fields,'[]' AS blocks,'en' AS locale,'' AS translation_group,'{}' AS seo,'' AS document,published_slug,published_title,published_body,published_fields,published_blocks,published_locale,published_translation_group,published_seo,published_document FROM posts WHERE published_slug=$1 AND published_locale=$2 AND status='published'")
        .bind(&slug).bind(&locale)
        .fetch_optional(&app.db.pool)
        .await?
        .map(Post::from_row)
        .ok_or_else(Error::not_found)?;
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
                    form method="post" action=(format!("/{slug}/comments")) {label {"Your name" input name="name" required maxlength="100";}label {"Comment" textarea name="body" required maxlength="4000" {}}
                        p class="muted" {"Comments are reviewed before publication."}button {"Submit for review"}}
                }
    };
    Ok(Html(crate::theme::document(
        &stored,
        &settings,
        &ctx,
        Some(&p),
        false,
        &p.kind,
        extra,
    )?)
    .into_response())
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
        form method="post" action=(id.map(|id|format!("/admin/posts/{id}")).unwrap_or_else(||"/admin/posts/new".into())) data-editor data-owner=(s.user.id) data-new=(if id.is_some(){"false"}else{"true"}) {
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
        "SELECT id,original_name,alt,visibility FROM media WHERE id>$1 ORDER BY id LIMIT 41",
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
    let rows=sqlx::query("SELECT id,original_name,visibility,alt,size FROM media ORDER BY created_at DESC,id DESC LIMIT 100").fetch_all(&app.db.pool).await?;
    Ok(html_page(
        "Media",
        &app.db.settings().await?,
        Some(&s),
        html! {
            (view::heading("Assets","Media library","Upload images, describe them and choose who can access them. SVG and executable uploads are not accepted."))
            section class="panel" {form method="post" action="/admin/media" enctype="multipart/form-data" {(view::csrf(&s))div class="field-row" {label {"Image" input type="file" name="file" accept="image/png,image/jpeg,image/webp,image/gif" required;}label {"Visibility" select name="visibility" aria-label="Visibility" {option value="public" {"Public"}option value="private" {"Editors only"}}}}label {"Alternative text" input name="alt" maxlength="500";}button {"Upload image"}}}
            div class="cards" {@for r in rows {@let id=r.get::<String,_>("id");section class="panel media-card" {img src=(format!("/media/{id}")) alt=(r.get::<String,_>("alt"));h3 {(r.get::<String,_>("original_name"))}p class="muted" {(r.get::<i64,_>("size")/1024) " KiB"}code {(format!("![description](/media/{id})"))}
                form method="post" action=(format!("/admin/media/{id}")) {(view::csrf(&s))label {"Alternative text" input name="alt" value=(r.get::<String,_>("alt")) maxlength="500";}label {"Visibility" select name="visibility" aria-label="Visibility" {option value="public" selected[r.get::<String,_>("visibility")=="public"] {"Public"}option value="private" selected[r.get::<String,_>("visibility")=="private"] {"Editors only"}}}button class="secondary" {"Save details"}}
            }}}
        },
    ))
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
    let _guard = app.mutations.lock().await;
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
    let _guard = app.mutations.lock().await;
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
async fn media_file(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response> {
    if uuid::Uuid::parse_str(&id).is_err() {
        return Err(Error::not_found());
    }
    let row = sqlx::query("SELECT filename,mime,visibility FROM media WHERE id=$1")
        .bind(id)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
    if row.get::<String, _>("visibility") == "private" {
        let s = admin_session(&app, &headers).await?;
        editor(&s)?;
    }
    let name: String = row.get("filename");
    if !backup::safe_filename(&name) {
        return Err(Error::not_found());
    }
    let bytes = tokio::fs::read(app.config.data_dir.join("media").join(name)).await?;
    let mime: String = row.get("mime");
    if !["image/png", "image/jpeg", "image/webp", "image/gif"].contains(&mime.as_str()) {
        return Err(Error::invalid("Unsupported stored media type."));
    }
    Ok((
        [
            ("content-type", mime.as_str()),
            ("cache-control", "no-store"),
        ],
        bytes,
    )
        .into_response())
}
#[derive(Deserialize)]
struct CommentInput {
    name: String,
    body: String,
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
    let client = connection
        .map(|c| c.0.0.ip().to_string())
        .unwrap_or_else(|| "local-test".into());
    let key = auth::digest(client.as_bytes());
    let _guard = app.mutations.lock().await;
    let mut tx = app.db.pool.begin().await?;
    let id: String =
        sqlx::query_scalar("SELECT id FROM posts WHERE published_slug=$1 AND status='published'")
            .bind(&slug)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(Error::not_found)?;
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
    let _guard = app.mutations.lock().await;
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
        title: input.title,
        description: input.description,
        theme: input.theme,
        navigation: input.navigation,
        field_schema: app.db.settings().await?.field_schema,
    };
    content::validate_settings(&settings)?;
    crate::theme::load(&app, &settings.theme, false).await?;
    let _guard = app.mutations.lock().await;
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
                    select name="role" aria-label="Account role" {@for role in ["admin","editor","moderator","disabled"] {option value=(role) selected[r.get::<String,_>("role")==role] {(role)}}}
                    input type="password" name="new_password" placeholder="Optional new password" aria-label="New password" autocomplete="new-password" maxlength="256";
                    button class="secondary" {"Update & revoke sessions"}
                }
            }}}}}}
            form class="panel" method="post" action="/admin/users" {(view::csrf(&s))h2 {"Add a collaborator"}div class="field-row" {label {"Name" input name="name" required maxlength="100";}label {"Email" input type="email" name="email" required maxlength="254";}}
                div class="field-row" {label {"Role" select name="role" aria-label="Role" {option value="editor" {"Editor"}option value="moderator" {"Moderator"}option value="admin" {"Administrator"}}}label {"Initial password" input type="password" name="password" required minlength="12" maxlength="256" autocomplete="new-password";}}
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
async fn operations(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let s = admin_session(&app, &headers).await?;
    admin(&s)?;
    Ok(html_page(
        "Operations",
        &app.db.settings().await?,
        Some(&s),
        html! {
            (view::heading("Operations","Operations","Manual snapshots, portable content and useful diagnostics without cloud dependencies."))
            div class="split" {section class="panel" {h2 {"Back up & move"}p class="muted" {"Download a consistent database-and-media snapshot. It includes password hashes and private content; keep it secure. M1 snapshots are not encrypted."}
                form method="post" action="/admin/backup" {(view::csrf(&s))button {"Download full backup"}}
                p class="muted" {"Restore with the CLI into an empty database/data directory while the server is stopped. Keep an independent copy to recover from losing this host."}
                a href="/admin/export" {"Export portable content JSON →"}
            }section class="panel" {h2 {"Runtime"}p {"Database: " strong {(if app.db.postgres{"PostgreSQL"}else{"SQLite · WAL"})}}p {"Version: " (env!("CARGO_PKG_VERSION"))}p {"Scheduler: " (app.config.scheduler_seconds) " seconds"}p {"Debug: " (app.config.debug)}p {a href="/health" {"Readiness endpoint →"}}}}
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
    let rows = sqlx::query("SELECT * FROM posts ORDER BY id LIMIT 10001")
        .fetch_all(&app.db.pool)
        .await?;
    if rows.len() > 10000 {
        return Err(Error::invalid(
            "M1 content export is limited to 10,000 items; use full backup for this site.",
        ));
    }
    let posts: Vec<Post> = rows.into_iter().map(Post::from_row).collect();
    Ok((
        [
            (
                "content-disposition",
                "attachment; filename=wpalt-content.json",
            ),
            ("cache-control", "no-store"),
        ],
        Json(serde_json::json!({"format":"wpalt-content-v2","posts":posts})),
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
