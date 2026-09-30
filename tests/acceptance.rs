//! Reviewable user journeys, using the real HTTP router and real databases.
//! Set TEST_DATABASE_URL to add PostgreSQL; CI requires and runs that path.
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use sqlx::Row;
use std::{collections::HashSet, io::Cursor};
use tower::ServiceExt;
use wpalt::{
    App, auth, backup,
    config::Config,
    content,
    model::{PostInput, Session},
};
const PASSWORD: &str = "test-only-correct-password";

struct Site {
    app: App,
    _directory: tempfile::TempDir,
    schema: Option<String>,
    root_url: Option<String>,
    token: String,
    session: Option<Session>,
}
impl Site {
    async fn new(postgres: bool, initialize: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config {
            data_dir: directory.path().join("data"),
            database_url: format!(
                "sqlite://{}?mode=rwc",
                directory.path().join("site.db").display()
            ),
            ..Config::default()
        };
        let mut schema = None;
        let mut root_url = None;
        if postgres {
            let root = std::env::var("TEST_DATABASE_URL").expect("PostgreSQL fixture URL");
            let name = format!("wpalt_test_{}", uuid::Uuid::new_v4().simple());
            let pool = sqlx::PgPool::connect(&root).await.unwrap();
            sqlx::query(&format!("CREATE SCHEMA {name}"))
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
            let mut url = url::Url::parse(&root).unwrap();
            url.query_pairs_mut()
                .append_pair("options", &format!("-c search_path={name}"));
            config.database_url = url.to_string();
            schema = Some(name);
            root_url = Some(root);
        }
        let app = App::open(config).await.unwrap();
        let (token, session) = if initialize {
            auth::initialize(&app, "owner@example.test", "Site owner", PASSWORD)
                .await
                .unwrap();
            let (token, s) = auth::login(&app, "owner@example.test", PASSWORD)
                .await
                .unwrap();
            (token, Some(s))
        } else {
            (String::new(), None)
        };
        Self {
            app,
            _directory: directory,
            schema,
            root_url,
            token,
            session,
        }
    }
    fn session(&self) -> &Session {
        self.session.as_ref().unwrap()
    }
    async fn close(self) {
        self.app.db.pool.close().await;
        if let (Some(schema), Some(root)) = (self.schema, self.root_url) {
            let pool = sqlx::PgPool::connect(&root).await.unwrap();
            sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }
    }
}
fn engines() -> Vec<bool> {
    let pg = std::env::var("TEST_DATABASE_URL").is_ok();
    if std::env::var("WPALT_REQUIRE_POSTGRES").is_ok() {
        assert!(
            pg,
            "PostgreSQL coverage was required but TEST_DATABASE_URL is missing"
        );
    }
    if pg { vec![false, true] } else { vec![false] }
}
fn input(slug: &str, action: &str) -> PostInput {
    PostInput {
        title: "A story worth sharing".into(),
        slug: slug.into(),
        kind: "post".into(),
        body: "A quiet garden and independent publishing.".into(),
        fields: r#"{"subtitle":"From our garden","featured":true}"#.into(),
        blocks: r#"[{"kind":"callout","text":"Made here"}]"#.into(),
        categories: "Field notes".into(),
        tags: "Gardens, Publishing".into(),
        version: 0,
        action: action.into(),
        publish_at: 0,
        csrf: String::new(),
    }
}
async fn request(
    app: &App,
    method: &str,
    path: &str,
    token: Option<&str>,
    content_type: &str,
    body: Vec<u8>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut r = Request::builder()
        .method(method)
        .uri(path)
        .header("origin", app.config.origin())
        .header("accept", "application/json");
    if let Some(token) = token {
        r = r.header("cookie", format!("wpalt_session={token}"));
    }
    if !content_type.is_empty() {
        r = r.header("content-type", content_type);
    }
    let response = wpalt::web::router(app.clone())
        .oneshot(r.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, headers, bytes)
}
async fn get(app: &App, path: &str, token: Option<&str>) -> (StatusCode, String) {
    let (s, _, b) = request(app, "GET", path, token, "", vec![]).await;
    (s, String::from_utf8(b).unwrap())
}
async fn form(
    app: &App,
    path: &str,
    token: Option<&str>,
    fields: &[(&str, &str)],
) -> (StatusCode, Vec<u8>) {
    let body = serde_urlencoded::to_string(fields).unwrap().into_bytes();
    let (s, _, b) = request(
        app,
        "POST",
        path,
        token,
        "application/x-www-form-urlencoded",
        body,
    )
    .await;
    (s, b)
}
fn png() -> Vec<u8> {
    let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        8,
        8,
        image::Rgb([40, 130, 90]),
    ));
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}
async fn upload(site: &Site, filename: &str, bytes: &[u8], visibility: &str) -> StatusCode {
    let boundary = "wpalt-test-boundary";
    let mut body = Vec::new();
    for (key, value) in [
        ("csrf", site.session().csrf.as_str()),
        ("visibility", visibility),
        ("alt", "A green square"),
    ] {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{key}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: image/png\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    request(
        &site.app,
        "POST",
        "/admin/media",
        Some(&site.token),
        &format!("multipart/form-data; boundary={boundary}"),
        body,
    )
    .await
    .0
}

#[tokio::test]
async fn author_preview_publish_autosave_and_restore_without_leaking_drafts() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let mut draft = input("a-story", "save");
        draft.body = "Private first draft".into();
        let p = content::save(&site.app, site.session(), None, draft.clone())
            .await
            .unwrap();
        assert_eq!(
            get(&site.app, "/a-story", None).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get(&site.app, &format!("/admin/preview/{}", p.id), None)
                .await
                .0,
            StatusCode::SEE_OTHER
        );
        assert!(
            get(
                &site.app,
                &format!("/admin/preview/{}", p.id),
                Some(&site.token)
            )
            .await
            .1
            .contains("Private first draft")
        );
        draft.version = p.version;
        draft.action = "publish".into();
        draft.body = "Public garden story".into();
        let published = content::save(&site.app, site.session(), Some(&p.id), draft.clone())
            .await
            .unwrap();
        assert!(
            get(&site.app, "/a-story", None)
                .await
                .1
                .contains("Public garden story")
        );
        let first_revision: String =
            sqlx::query_scalar("SELECT id FROM revisions WHERE post_id=$1 AND version=1")
                .bind(&p.id)
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        draft.version = published.version;
        draft.action = "autosave".into();
        draft.slug = "private-new-url".into();
        draft.title = "Secret future title".into();
        draft.body = "SECRET_FUTURE_CONTENT".into();
        draft.tags = "Secret topic".into();
        let working = content::save(&site.app, site.session(), Some(&p.id), draft.clone())
            .await
            .unwrap();
        let live = get(&site.app, "/a-story", None).await.1;
        assert!(live.contains("Public garden story"));
        assert!(!live.contains("SECRET_FUTURE_CONTENT"));
        assert_eq!(
            get(&site.app, "/private-new-url", None).await.0,
            StatusCode::NOT_FOUND
        );
        let public_api = get(&site.app, "/api/content", None).await.1;
        assert!(!public_api.contains("Secret future title"));
        assert!(
            get(&site.app, "/search?q=Public", None)
                .await
                .1
                .contains("A story worth sharing")
        );
        assert!(
            !get(&site.app, "/search?q=SECRET_FUTURE_CONTENT", None)
                .await
                .1
                .contains("Read more")
        );
        assert!(
            get(&site.app, "/?tag=gardens", None)
                .await
                .1
                .contains("Read more")
        );
        assert!(
            !get(&site.app, "/?tag=secret-topic", None)
                .await
                .1
                .contains("Read more")
        );
        let restored = content::restore_revision(
            &site.app,
            site.session(),
            &p.id,
            &first_revision,
            working.version,
        )
        .await
        .unwrap();
        assert_eq!(restored.body, "Private first draft");
        assert!(
            get(&site.app, "/a-story", None)
                .await
                .1
                .contains("Public garden story")
        );
        assert_eq!(get(&site.app, "/feed.xml", None).await.0, StatusCode::OK);
        site.close().await;
    }
}

#[tokio::test]
async fn concurrent_editors_never_silently_overwrite_each_other() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let p = content::save(&site.app, site.session(), None, input("concurrent", "save"))
            .await
            .unwrap();
        let mut a = input("concurrent", "save");
        a.version = p.version;
        a.body = "Writer A".into();
        let mut b = a.clone();
        b.body = "Writer B".into();
        let (a, b) = tokio::join!(
            content::save(&site.app, site.session(), Some(&p.id), a),
            content::save(&site.app, site.session(), Some(&p.id), b)
        );
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        let conflict = if let Err(e) = a { e } else { b.unwrap_err() };
        assert_eq!(conflict.0, StatusCode::CONFLICT);
        assert_eq!(
            content::get(&site.app, &p.id).await.unwrap().version,
            p.version + 1
        );
        let revisions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM revisions WHERE post_id=$1")
            .bind(p.id)
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(revisions, 2);
        site.close().await;
    }
}

#[tokio::test]
async fn publication_schedule_survives_reopening_and_runs_only_once() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let mut p = input("tomorrow", "schedule");
        p.publish_at = wpalt::now() + 60;
        let mut p = content::save(&site.app, site.session(), None, p)
            .await
            .unwrap();
        // A scheduled story can keep changing before publication. On restart,
        // the configured retention bound applies to the publication revision too.
        for revision in 0..5 {
            let mut edit = input("tomorrow", "autosave");
            edit.version = p.version;
            edit.body = format!("Scheduled working revision {revision}");
            p = content::save(&site.app, site.session(), Some(&p.id), edit)
                .await
                .unwrap();
        }
        assert_eq!(content::publish_due(&site.app).await.unwrap(), 0);
        assert_eq!(
            get(&site.app, "/tomorrow", None).await.0,
            StatusCode::NOT_FOUND
        );
        // Controlled clock boundary without a timing-sensitive sleep.
        sqlx::query("UPDATE posts SET publish_at=$1 WHERE id=$2")
            .bind(wpalt::now() - 1)
            .bind(&p.id)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        site.app.db.pool.close().await;
        let mut config = site.app.config.as_ref().clone();
        config.revision_retention = 5;
        let reopened = App::open(config).await.unwrap();
        assert_eq!(content::publish_due(&reopened).await.unwrap(), 1);
        assert_eq!(content::publish_due(&reopened).await.unwrap(), 0);
        assert_eq!(get(&reopened, "/tomorrow", None).await.0, StatusCode::OK);
        assert_eq!(
            content::get(&reopened, &p.id).await.unwrap().version,
            p.version + 1
        );
        let kept: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM revisions WHERE post_id=$1")
            .bind(&p.id)
            .fetch_one(&reopened.db.pool)
            .await
            .unwrap();
        assert_eq!(
            kept, 5,
            "scheduled publication must honor the revision limit"
        );
        reopened.db.pool.close().await;
        site.close().await;
    }
}

#[tokio::test]
async fn permissions_csrf_sessions_and_origin_protect_every_write_surface() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        auth::add_user(
            &site.app,
            "editor@example.test",
            "Editor",
            "editor",
            PASSWORD,
        )
        .await
        .unwrap();
        auth::add_user(
            &site.app,
            "moderator@example.test",
            "Moderator",
            "moderator",
            PASSWORD,
        )
        .await
        .unwrap();
        let (etoken, editor) = auth::login(&site.app, "editor@example.test", PASSWORD)
            .await
            .unwrap();
        let (mtoken, moderator) = auth::login(&site.app, "moderator@example.test", PASSWORD)
            .await
            .unwrap();
        assert_eq!(
            get(&site.app, "/admin/settings", Some(&etoken)).await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            get(&site.app, "/admin/posts", Some(&mtoken)).await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            content::save(&site.app, &moderator, None, input("forbidden", "publish"))
                .await
                .unwrap_err()
                .0,
            StatusCode::FORBIDDEN
        );
        // Role checks protect the infrastructure and other users' write surfaces.
        for (path, fields) in [
            ("/admin/backup", vec![("csrf", editor.csrf.as_str())]),
            (
                "/admin/users",
                vec![
                    ("csrf", editor.csrf.as_str()),
                    ("email", "new@example.test"),
                    ("name", "New user"),
                    ("role", "admin"),
                    ("password", PASSWORD),
                ],
            ),
            (
                "/admin/settings",
                vec![
                    ("csrf", editor.csrf.as_str()),
                    ("title", "Denied"),
                    ("description", ""),
                    ("theme", "paper"),
                    ("navigation", "[]"),
                    ("field_schema", "[]"),
                ],
            ),
            (
                "/admin/comments/missing",
                vec![("csrf", editor.csrf.as_str()), ("status", "approved")],
            ),
        ] {
            assert_eq!(
                form(&site.app, path, Some(&etoken), &fields).await.0,
                StatusCode::FORBIDDEN,
                "editor write unexpectedly allowed: {path}"
            );
        }
        assert_eq!(
            form(
                &site.app,
                "/admin/media/missing",
                Some(&mtoken),
                &[
                    ("csrf", &moderator.csrf),
                    ("alt", "Denied"),
                    ("visibility", "public")
                ]
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        let p = input("csrf-target", "publish");
        let body = serde_json::to_vec(&p).unwrap();
        assert_eq!(
            request(
                &site.app,
                "POST",
                "/api/admin/content",
                Some(&etoken),
                "application/json",
                body.clone()
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        let response = wpalt::web::router(site.app.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/admin/content")
                    .header("origin", "https://attacker.example")
                    .header("cookie", format!("wpalt_session={etoken}"))
                    .header("x-csrf-token", &editor.csrf)
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let response = wpalt::web::router(site.app.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/admin/content")
                    .header("origin", site.app.config.origin())
                    .header("cookie", format!("wpalt_session={etoken}"))
                    .header("x-csrf-token", &editor.csrf)
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&p).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let (s, h, _) = request(&site.app, "GET", "/admin", Some(&etoken), "", vec![]).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(h["cache-control"], "no-store");
        assert!(
            h["content-security-policy"]
                .to_str()
                .unwrap()
                .contains("frame-ancestors 'none'")
        );
        assert_eq!(
            form(
                &site.app,
                "/logout",
                Some(&etoken),
                &[("csrf", &editor.csrf)]
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
        assert_eq!(
            get(&site.app, "/admin/posts", Some(&etoken)).await.0,
            StatusCode::SEE_OTHER
        );
        // Expired and explicitly revoked accounts cannot retain their old authority.
        sqlx::query("UPDATE sessions SET expires_at=0 WHERE token_hash=$1")
            .bind(&moderator.hash)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            get(&site.app, "/admin/comments", Some(&mtoken)).await.0,
            StatusCode::SEE_OTHER
        );
        let (replacement, _) = auth::login(&site.app, "editor@example.test", PASSWORD)
            .await
            .unwrap();
        assert_eq!(
            form(
                &site.app,
                &format!("/admin/users/{}", editor.user.id),
                Some(&site.token),
                &[
                    ("csrf", &site.session().csrf),
                    ("name", "Editor"),
                    ("role", "disabled"),
                    ("new_password", "")
                ]
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
        assert_eq!(
            get(&site.app, "/admin/posts", Some(&replacement)).await.0,
            StatusCode::SEE_OTHER
        );
        assert!(
            auth::login(&site.app, "editor@example.test", PASSWORD)
                .await
                .is_err()
        );
        assert_eq!(
            form(
                &site.app,
                &format!("/admin/users/{}", site.session().user.id),
                Some(&site.token),
                &[
                    ("csrf", &site.session().csrf),
                    ("name", "Owner"),
                    ("role", "disabled"),
                    ("new_password", "")
                ]
            )
            .await
            .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let unknown = auth::login(&site.app, "unknown@example.test", PASSWORD)
            .await
            .unwrap_err();
        let wrong = auth::login(&site.app, "owner@example.test", "wrong-password")
            .await
            .unwrap_err();
        assert_eq!(unknown.1, wrong.1);
        assert!(
            !site
                .app
                .config
                .redacted()
                .to_string()
                .contains("wpalt-local-test")
        );
        site.close().await;
    }
}

#[tokio::test]
async fn images_and_comments_obey_visibility_validation_and_moderation() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        assert_eq!(
            upload(&site, "green.png", &png(), "private").await,
            StatusCode::SEE_OTHER
        );
        let id: String = sqlx::query_scalar("SELECT id FROM media")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            request(&site.app, "GET", &format!("/media/{id}"), None, "", vec![])
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        let (s, _, bytes) = request(
            &site.app,
            "GET",
            &format!("/media/{id}"),
            Some(&site.token),
            "",
            vec![],
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(bytes, png());
        assert_eq!(
            form(
                &site.app,
                &format!("/admin/media/{id}"),
                Some(&site.token),
                &[
                    ("csrf", &site.session().csrf),
                    ("alt", "Safe description"),
                    ("visibility", "public")
                ]
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
        assert_eq!(
            request(&site.app, "GET", &format!("/media/{id}"), None, "", vec![])
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            upload(
                &site,
                "attack.svg",
                b"<svg onload='alert(1)'></svg>",
                "public"
            )
            .await,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            upload(&site, "broken.png", b"\x89PNG\r\n\x1a\n", "public").await,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let mut post = input("conversation", "publish");
        post.body = "<script>alert('unsafe')</script>\n\nA useful conversation".into();
        content::save(&site.app, site.session(), None, post)
            .await
            .unwrap();
        assert!(
            !get(&site.app, "/conversation", None)
                .await
                .1
                .contains("<script>alert")
        );
        assert_eq!(
            form(
                &site.app,
                "/conversation/comments",
                None,
                &[
                    ("name", "Reader"),
                    ("body", "PENDING_COMMENT <script>unsafe</script>")
                ]
            )
            .await
            .0,
            StatusCode::OK
        );
        assert!(
            !get(&site.app, "/conversation", None)
                .await
                .1
                .contains("PENDING_COMMENT")
        );
        assert_eq!(
            form(
                &site.app,
                "/conversation/comments",
                None,
                &[("name", "Reader"), ("body", "Repeated submission")]
            )
            .await
            .0,
            StatusCode::TOO_MANY_REQUESTS
        );
        let cid: String = sqlx::query_scalar("SELECT id FROM comments")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            form(
                &site.app,
                &format!("/admin/comments/{cid}"),
                Some(&site.token),
                &[("csrf", &site.session().csrf), ("status", "approved")]
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
        let page = get(&site.app, "/conversation", None).await.1;
        assert!(page.contains("PENDING_COMMENT"));
        assert!(page.contains("&lt;script&gt;unsafe&lt;/script&gt;"));
        site.close().await;
    }
}

#[tokio::test]
async fn backup_restores_content_users_media_and_revisions_into_a_fresh_engine() {
    for pg in engines() {
        let source = Site::new(pg, true).await;
        let post = content::save(
            &source.app,
            source.session(),
            None,
            input("recover-me", "publish"),
        )
        .await
        .unwrap();
        assert_eq!(
            upload(&source, "green.png", &png(), "private").await,
            StatusCode::SEE_OTHER
        );
        let bytes = backup::capture(&source.app).await.unwrap();
        let other_engine = !pg && std::env::var("TEST_DATABASE_URL").is_ok();
        let target = Site::new(other_engine, false).await;
        let mut broken: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        broken["sha256"] = "bad-checksum".into();
        assert!(
            backup::restore(&target.app, &serde_json::to_vec(&broken).unwrap())
                .await
                .is_err()
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&target.app.db.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        let mut malicious: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let mut payload: serde_json::Value =
            serde_json::from_str(malicious["payload"].as_str().unwrap()).unwrap();
        payload["files"][0]["filename"] = "../../outside.png".into();
        let payload = payload.to_string();
        malicious["payload"] = payload.clone().into();
        malicious["sha256"] = auth::digest(payload.as_bytes()).into();
        assert!(
            backup::restore(&target.app, &serde_json::to_vec(&malicious).unwrap())
                .await
                .is_err()
        );
        backup::restore(&target.app, &bytes).await.unwrap();
        assert_eq!(
            content::get(&target.app, &post.id)
                .await
                .unwrap()
                .published_body,
            post.published_body
        );
        let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(&target.app.db.pool)
            .await
            .unwrap();
        assert_eq!(sessions, 0);
        assert!(
            auth::login(&target.app, "owner@example.test", PASSWORD)
                .await
                .is_ok()
        );
        let media = sqlx::query("SELECT filename,sha256 FROM media")
            .fetch_one(&target.app.db.pool)
            .await
            .unwrap();
        let file = std::fs::read(
            target
                .app
                .config
                .data_dir
                .join("media")
                .join(media.get::<String, _>("filename")),
        )
        .unwrap();
        assert_eq!(file, png());
        assert_eq!(auth::digest(&file), media.get::<String, _>("sha256"));
        assert!(backup::restore(&target.app, &bytes).await.is_err());
        target.close().await;
        source.close().await;
    }
}

#[tokio::test]
async fn published_library_paginates_without_duplicates_and_searches_only_live_content() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        for i in 0..45 {
            content::save(
                &site.app,
                site.session(),
                None,
                input(&format!("story-{i}"), "publish"),
            )
            .await
            .unwrap();
        }
        content::save(
            &site.app,
            site.session(),
            None,
            input("unpublished-secret", "save"),
        )
        .await
        .unwrap();
        let mut seen = HashSet::new();
        let mut path = "/api/content".to_string();
        loop {
            let (s, json) = get(&site.app, &path, None).await;
            assert_eq!(s, StatusCode::OK);
            let json: serde_json::Value = serde_json::from_str(&json).unwrap();
            let items = json["items"].as_array().unwrap();
            assert!(items.len() <= 20);
            for item in items {
                assert!(seen.insert(item["id"].as_str().unwrap().to_owned()));
                assert_ne!(item["slug"], "unpublished-secret");
            }
            if let Some(next) = json["next"].as_str() {
                path = format!("/api/content?after={next}");
            } else {
                break;
            }
        }
        assert_eq!(seen.len(), 45);
        assert_eq!(
            get(&site.app, "/api/content?after=not-a-cursor", None)
                .await
                .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            get(&site.app, "/search?q=%22%20OR%20%22", None).await.0,
            StatusCode::OK
        );
        site.close().await;
    }
}

#[test]
fn configuration_and_composition_reject_unsafe_or_ambiguous_inputs() {
    let c = Config {
        database_url: "postgres://user:secret@localhost/database".into(),
        ..Config::default()
    };
    assert!(!c.redacted().to_string().contains("secret"));
    let mut c = Config {
        base_url: "http://public.example".into(),
        ..Config::default()
    };
    assert!(c.validate().is_err());
    c.base_url = "https://public.example/subpath".into();
    assert!(c.validate().is_err());
    let mut s = wpalt::model::Settings {
        navigation: r#"[{"label":"Unsafe","url":"javascript:alert(1)"}]"#.into(),
        ..wpalt::model::Settings::default()
    };
    assert!(content::validate_settings(&s).is_err());
    s.navigation = "[]".into();
    let mut p = input("safe-slug", "save");
    p.fields = r#"{"featured":"wrong type"}"#.into();
    assert!(content::validate_input(&p, &s).is_err());
    p.fields = "{}".into();
    p.blocks = r#"[{"kind":"executable","text":"code"}]"#.into();
    assert!(content::validate_input(&p, &s).is_err());
    assert!(!backup::safe_filename("../../image.png"));
    assert!(!content::valid_slug("admin"));
}
