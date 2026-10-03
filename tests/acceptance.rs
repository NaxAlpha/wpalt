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
        Self::with_connections(postgres, initialize, 4).await
    }
    async fn with_connections(postgres: bool, initialize: bool, connections: u32) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config {
            database_connections: connections,
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
        import_markdown: false,
        locale: "en".into(),
        translation_group: String::new(),
        seo: "{}".into(),
        title: "A story worth sharing".into(),
        slug: slug.into(),
        kind: "post".into(),
        body: "A quiet garden and independent publishing.".into(),
        document: String::new(),
        fields: r#"{"subtitle":"From our garden","featured":true}"#.into(),
        blocks: r#"[{"kind":"callout","text":"Made here"}]"#.into(),
        categories: "Field notes".into(),
        tags: "Gardens, Publishing".into(),
        taxonomies: "{}".into(),
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
        assert!(restored.body.starts_with("Private first draft"));
        assert!(
            wpalt::document::Document::parse(&restored.document)
                .unwrap()
                .html()
                .contains("Made here")
        );
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
        auth::add_user(
            &site.app,
            "reader@example.test",
            "Reader",
            "subscriber",
            PASSWORD,
        )
        .await
        .unwrap();
        let (reader_token, _) = auth::login(&site.app, "reader@example.test", PASSWORD)
            .await
            .unwrap();
        let (status, headers, _) = request(
            &site.app,
            "GET",
            "/login",
            Some(&reader_token),
            "",
            Vec::new(),
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            headers["location"], "/account",
            "A signed-in subscriber must not enter a login/admin redirect loop."
        );
        assert_eq!(
            get(&site.app, "/admin", Some(&reader_token)).await.0,
            StatusCode::FORBIDDEN
        );
        let account = get(&site.app, "/account", Some(&reader_token)).await.1;
        assert!(account.contains("Your account"));
        assert!(
            !account.contains("href=\"/admin\""),
            "Account navigation must stay inside the subscriber's permitted experience."
        );
        let inventory: serde_json::Value =
            serde_json::from_str(include_str!("../docs/evidence/adversarial-coverage.json"))
                .unwrap();
        for route in inventory["routes"].as_array().unwrap() {
            let pattern = route["route"].as_str().unwrap();
            if !pattern.starts_with("/admin") && !pattern.starts_with("/api/admin") {
                continue;
            }
            let mut path = pattern.to_owned();
            while let Some(start) = path.find('{') {
                let end = path[start..].find('}').unwrap() + start;
                path.replace_range(start..=end, "00000000-0000-4000-8000-000000000001");
            }
            for token in [None, Some(reader_token.as_str())] {
                let (status, _, _) = request(&site.app, "GET", &path, token, "", Vec::new()).await;
                assert!(
                    matches!(
                        status,
                        StatusCode::SEE_OTHER
                            | StatusCode::UNAUTHORIZED
                            | StatusCode::FORBIDDEN
                            | StatusCode::METHOD_NOT_ALLOWED
                    ),
                    "Administrative route {pattern} must deny anonymous/subscriber reads before looking up data: {status}"
                );
            }
        }
        let (etoken, editor) = auth::login(&site.app, "editor@example.test", PASSWORD)
            .await
            .unwrap();
        let editor_page = get(&site.app, "/admin/posts", Some(&etoken)).await.1;
        assert!(
            !editor_page.contains("href=\"/admin/engagement\""),
            "Navigation must not advertise owner-only reports to editors."
        );
        assert!(
            get(&site.app, "/admin/posts", Some(&site.token))
                .await
                .1
                .contains("href=\"/admin/engagement\"")
        );
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
        assert_eq!(
            get(&site.app, "/api/admin/media", Some(&mtoken)).await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            get(&site.app, "/api/admin/media?after=unsafe", Some(&etoken))
                .await
                .0,
            StatusCode::UNPROCESSABLE_ENTITY
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
        for path in ["/account", "/logout"] {
            let (_, headers, _) = request(&site.app, "GET", path, Some(&etoken), "", vec![]).await;
            assert_eq!(
                headers["cache-control"], "no-store",
                "private response: {path}"
            );
        }
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
        // A real larger image must arrive incrementally, with identical bytes.
        let image = image::RgbImage::from_fn(256, 256, |x, y| {
            let mut value = (x + y * 256).wrapping_mul(0x45d9f3b);
            value = (value ^ (value >> 16)).wrapping_mul(0x45d9f3b);
            image::Rgb([value as u8, (value >> 8) as u8, (value >> 16) as u8])
        });
        let mut encoded = Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let expected = encoded.into_inner();
        assert!(expected.len() > 16_384);
        assert_eq!(
            upload(&site, "large.png", &expected, "public").await,
            StatusCode::SEE_OTHER
        );
        let large_id: String = sqlx::query_scalar(
            "SELECT id FROM media WHERE filename <> (SELECT filename FROM media WHERE id=$1)",
        )
        .bind(&id)
        .fetch_one(&site.app.db.pool)
        .await
        .unwrap();
        let response = wpalt::web::router(site.app.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/media/{large_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let mut body = response.into_body();
        let mut received = Vec::new();
        while let Some(frame) = body.frame().await {
            if let Ok(bytes) = frame.unwrap().into_data() {
                assert!(
                    bytes.len() <= 8192,
                    "media retained a whole-image response buffer"
                );
                received.extend_from_slice(&bytes);
            }
        }
        assert_eq!(received, expected);
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
        let filename: String = sqlx::query_scalar("SELECT filename FROM media LIMIT 1")
            .fetch_one(&source.app.db.pool)
            .await
            .unwrap();
        let stored_path = source.app.config.data_dir.join("media").join(filename);
        let original = std::fs::read(&stored_path).unwrap();
        // Corrupted disk data must not turn a small upload into an unbounded response.
        std::fs::OpenOptions::new()
            .write(true)
            .open(&stored_path)
            .unwrap()
            .set_len(33 * 1024 * 1024)
            .unwrap();
        let media_id: String = sqlx::query_scalar("SELECT id FROM media LIMIT 1")
            .fetch_one(&source.app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            get(
                &source.app,
                &format!("/media/{media_id}"),
                Some(&source.token)
            )
            .await
            .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert!(backup::capture(&source.app).await.is_err());
        std::fs::write(&stored_path, original).unwrap();
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
        let mut hostile: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let mut payload: serde_json::Value =
            serde_json::from_str(hostile["payload"].as_str().unwrap()).unwrap();
        let expensive = payload["tables"]["users"][0]["password_hash"]
            .as_str()
            .unwrap()
            .replace("m=19456", "m=4294967295");
        assert!(
            argon2::password_hash::PasswordHash::new(&expensive).is_ok(),
            "This attack is valid syntax, not a malformed hash."
        );
        payload["tables"]["users"][0]["password_hash"] = expensive.into();
        let payload = payload.to_string();
        hostile["payload"] = payload.clone().into();
        hostile["sha256"] = auth::digest(payload.as_bytes()).into();
        assert!(
            backup::restore(&target.app, &serde_json::to_vec(&hostile).unwrap())
                .await
                .is_err(),
            "A recomputed archive checksum cannot authorize unbounded password work."
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
                .fetch_one(&target.app.db.pool)
                .await
                .unwrap(),
            0
        );
        for (field, value) in [("role", "editor"), ("email", "Owner@Example.TEST")] {
            let mut inaccessible: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let mut payload: serde_json::Value =
                serde_json::from_str(inaccessible["payload"].as_str().unwrap()).unwrap();
            payload["tables"]["users"][0][field] = value.into();
            let payload = payload.to_string();
            inaccessible["payload"] = payload.clone().into();
            inaccessible["sha256"] = auth::digest(payload.as_bytes()).into();
            assert!(
                backup::restore(&target.app, &serde_json::to_vec(&inaccessible).unwrap())
                    .await
                    .is_err(),
                "Recovery must preserve a usable administrator login."
            );
        }
        let mut shadowed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let mut payload: serde_json::Value =
            serde_json::from_str(shadowed["payload"].as_str().unwrap()).unwrap();
        payload["tables"]["posts"][0]["published_slug"] = "account".into();
        let payload = payload.to_string();
        shadowed["payload"] = payload.clone().into();
        shadowed["sha256"] = auth::digest(payload.as_bytes()).into();
        assert!(
            backup::restore(&target.app, &serde_json::to_vec(&shadowed).unwrap())
                .await
                .is_err(),
            "Restore must not bypass reserved public route validation."
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
    let canonical = Config {
        base_url: "HTTPS://Example.TEST:443/".into(),
        ..Config::default()
    };
    canonical.validate().unwrap();
    assert_eq!(canonical.origin(), "https://example.test");
    assert!(
        canonical.secure_cookie(),
        "Accepted HTTPS URL syntax cannot bypass Secure cookies or HSTS."
    );
    for protected in [
        "/account",
        "/audience/confirm/private",
        "/registration/private",
    ] {
        assert!(
            !wpalt::discovery::safe_path(protected),
            "Proof/session routes are not public redirect rules."
        );
    }
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
    let registry = wpalt::schema::Registry {
        common: serde_json::from_str(&s.field_schema).unwrap(),
        models: [("post".into(), wpalt::schema::Model::initial("Posts"))].into(),
    };
    assert!(
        registry
            .validate_values(
                &registry.fields_for("post").unwrap(),
                &serde_json::from_str(&p.fields).unwrap()
            )
            .is_err()
    );
    p.fields = "{}".into();
    p.blocks = r#"[{"kind":"executable","text":"code"}]"#.into();
    assert!(content::validate_input(&p, &s).is_err());
    assert!(!backup::safe_filename("../../image.png"));
    for reserved in [
        "admin",
        "account",
        "forms",
        "audience",
        "registration",
        "themes",
    ] {
        assert!(
            !content::valid_slug(reserved),
            "A content route must not be shadowed by a system route: {reserved}"
        );
    }
}

// M2 cluster 1: one schema connects structured authoring, relationships, options and templates.
#[tokio::test]
async fn typed_models_and_reusable_components_render_only_published_data() {
    use serde_json::json;
    use wpalt::{schema, theme};
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let mut common = schema::Registry::load(&site.app).await.unwrap().common;
        common.groups.insert(
            "hero".into(),
            std::collections::BTreeMap::from([(
                "headline".into(),
                schema::Field::primitive("string"),
            )]),
        );
        schema::save_common(&site.app, common, 1).await.unwrap();
        let mut model: schema::Model =
            serde_json::from_str(include_str!("fixtures/project-model.json")).unwrap();
        model.fields.insert(
            "inline-group".into(),
            serde_json::from_value(json!({"kind":"group","fields":{"headline":{"kind":"string"}}}))
                .unwrap(),
        );
        model.fields.insert(
            "shared-object".into(),
            serde_json::from_value(json!({"kind":"object","group":"hero"})).unwrap(),
        );
        assert_eq!(
            upload(&site, "public.png", &png(), "public").await,
            StatusCode::SEE_OTHER
        );
        assert_eq!(
            upload(&site, "private.png", &png(), "private").await,
            StatusCode::SEE_OTHER
        );
        let public_media: String =
            sqlx::query_scalar("SELECT id FROM media WHERE visibility='public'")
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        let private_media: String =
            sqlx::query_scalar("SELECT id FROM media WHERE visibility='private'")
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        schema::save_model(&site.app, "project", model, 0)
            .await
            .unwrap();
        let mut related = input("client", "publish");
        related.title = "Published client".into();
        let client = content::save(&site.app, site.session(), None, related.clone())
            .await
            .unwrap();
        related.version = client.version;
        related.title = "PRIVATE_CLIENT_DRAFT".into();
        related.action = "save".into();
        content::save(&site.app, site.session(), Some(&client.id), related)
            .await
            .unwrap();
        let public_identifier = uuid::Uuid::new_v4().to_string();
        let mut project = input("project-one", "publish");
        project.kind = "project".into();
        project.fields=json!({"subtitle":public_identifier,"client":client.id,"steps":[{"label":"First useful step"},{"label":"Second useful step"}],"gallery":[public_media,private_media],"photo":public_media,"sections":[{"type":"hero","values":{"headline":"FLEXIBLE_HERO"}}],"shared":{"headline":"REUSED_GROUP"},"details":{"count":7},"inline-group":{"headline":"INLINE_GROUP"},"shared-object":{"headline":"SHARED_OBJECT"}}).to_string();
        project.taxonomies = json!({"sector":["Local businesses"]}).to_string();
        let record = content::save(&site.app, site.session(), None, project.clone())
            .await
            .unwrap();
        let mut common = schema::Registry::load(&site.app).await.unwrap().common;
        common
            .options
            .insert("announcement".into(), schema::Field::primitive("string"));
        let version = schema::save_common(&site.app, common, 2).await.unwrap();
        let version = schema::save_options(
            &site.app,
            json!({"announcement":"Published announcement"}),
            version,
            true,
        )
        .await
        .unwrap();
        schema::save_options(
            &site.app,
            json!({"announcement":"PRIVATE_OPTION_DRAFT"}),
            version,
            false,
        )
        .await
        .unwrap();
        let mut package = theme::load(&site.app, "paper", true).await.unwrap().package;
        package.components.insert("card".into(),serde_json::from_value(json!({"parameters":{"title":"string"},"root":{"id":"card-root","kind":"heading","text":{"bind":"params.title"}}})).unwrap());
        package.templates.insert(
            "project".into(),
            serde_json::from_str(include_str!("fixtures/project-template.json")).unwrap(),
        );
        for (id, path) in [
            ("inline-group-output", "post.fields.inline-group.headline"),
            ("shared-object-output", "post.fields.shared-object.headline"),
            ("gallery-projection", "post.fields.gallery"),
        ] {
            package.templates.get_mut("project").unwrap().children.push(
                serde_json::from_value(json!({"id":id,"kind":"text","text":{"bind":path}}))
                    .unwrap(),
            );
        }
        package.templates.get_mut("home").unwrap().children.push(serde_json::from_value(json!({"id":"project-list","kind":"collection","source":"project","limit":30,"children":[{"id":"project-card","kind":"component","component":"card","arguments":{"title":{"bind":"item.fields.client.title"}}}]})).unwrap());
        theme::save(&site.app, "paper", package, 1, true)
            .await
            .unwrap();
        let (status, public) = get(&site.app, "/project-one", None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            public.contains("Published client")
                && public.contains("Published announcement")
                && public.contains("First useful step")
                && public.contains("Second useful step")
        );
        assert!(!public.contains("PRIVATE_"));
        assert!(public.contains("INLINE_GROUP") && public.contains("SHARED_OBJECT"));
        assert!(
            !public.contains(&private_media),
            "Binding an entire gallery must not leak its private identifiers as JSON text."
        );
        assert!(
            public.contains(&public_identifier),
            "Ordinary UUID-shaped strings are not mistaken for private relationship references"
        );
        assert!(public.contains("FLEXIBLE_HERO") && public.contains("REUSED_GROUP"));
        assert!(
            public.contains(&format!("/media/{public_media}"))
                && !public.contains(&format!("/media/{private_media}"))
        );
        let package = theme::load(&site.app, "paper", false)
            .await
            .unwrap()
            .package;
        let settings = site.app.db.settings().await.unwrap();
        let before = theme::context(
            &site.app,
            &settings,
            &package,
            None,
            vec![],
            false,
            "home",
            None,
        )
        .await
        .unwrap()
        .queries;
        for index in 0..29 {
            let mut extra = project.clone();
            extra.slug = format!("project-{index}");
            extra.version = 0;
            content::save(&site.app, site.session(), None, extra)
                .await
                .unwrap();
        }
        let after = theme::context(
            &site.app,
            &settings,
            &package,
            None,
            vec![],
            false,
            "home",
            None,
        )
        .await
        .unwrap()
        .queries;
        assert_eq!(
            before, after,
            "Thirty related cards use the same number of bulk queries as one card"
        );
        assert!(after <= 6);

        let edit = get(
            &site.app,
            &format!("/admin/posts/{}", record.id),
            Some(&site.token),
        )
        .await
        .1;
        assert!(
            edit.contains("Local businesses"),
            "Custom terms survive editing"
        );
        // A target which has never been published must resolve to empty in visitor output.
        let mut private_input = input("private-client", "save");
        private_input.title = "NEVER_PUBLISHED_CLIENT".into();
        let private = content::save(&site.app, site.session(), None, private_input)
            .await
            .unwrap();
        project.version = record.version;
        project.fields = json!({"client":private.id,"steps":[]}).to_string();
        content::save(&site.app, site.session(), Some(&record.id), project)
            .await
            .unwrap();
        let public = get(&site.app, "/project-one", None).await.1;
        assert!(!public.contains("NEVER_PUBLISHED_CLIENT"));
        site.close().await;
    }
}

// M2 cluster 2: draft publication, optimistic conflicts and revision restore are distinct operations.
#[tokio::test]
async fn theme_drafts_publish_restore_and_switch_without_content_loss() {
    use wpalt::theme;
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let post = content::save(
            &site.app,
            site.session(),
            None,
            input("preserved", "publish"),
        )
        .await
        .unwrap();
        let old = theme::load(&site.app, "paper", true).await.unwrap();
        let mut package = old.package.clone();
        package.footer.text = serde_json::json!("PRIVATE_THEME_DRAFT");
        package.tokens.insert("accent".into(), "#803355".into());
        let v = theme::save(&site.app, "paper", package.clone(), old.version, false)
            .await
            .unwrap();
        assert_eq!(
            theme::save(&site.app, "paper", package.clone(), old.version, false)
                .await
                .unwrap_err()
                .0,
            StatusCode::CONFLICT
        );
        assert!(
            !get(&site.app, "/preserved", None)
                .await
                .1
                .contains("PRIVATE_THEME_DRAFT")
        );
        let preview = format!(
            "/admin/design/paper/preview?template=content&post={}",
            post.id
        );
        let (_, headers, body) =
            request(&site.app, "GET", &preview, Some(&site.token), "", vec![]).await;
        assert!(
            String::from_utf8(body)
                .unwrap()
                .contains("PRIVATE_THEME_DRAFT")
        );
        assert!(
            headers["content-security-policy"]
                .to_str()
                .unwrap()
                .contains("script-src 'none'")
        );
        assert_eq!(
            get(&site.app, "/themes/paper/2/style.css", None).await.0,
            StatusCode::NOT_FOUND
        );
        let v = theme::save(&site.app, "paper", package, v, true)
            .await
            .unwrap();
        assert!(
            get(&site.app, "/preserved", None)
                .await
                .1
                .contains("PRIVATE_THEME_DRAFT")
        );
        assert_eq!(
            get(&site.app, "/themes/paper/1/style.css", None).await.0,
            StatusCode::OK,
            "In-flight old HTML retains its public style revision"
        );
        theme::save(&site.app, "paper", old.package, v, false)
            .await
            .unwrap();
        assert!(
            get(&site.app, "/preserved", None)
                .await
                .1
                .contains("PRIVATE_THEME_DRAFT"),
            "Restore only changes the working draft"
        );
        theme::activate(&site.app, "ink").await.unwrap();
        assert!(
            get(&site.app, "/preserved", None)
                .await
                .1
                .contains("A story worth sharing")
        );
        assert_eq!(
            content::get(&site.app, &post.id)
                .await
                .unwrap()
                .published_title,
            "A story worth sharing"
        );
        let current = theme::load(&site.app, "paper", true).await.unwrap();
        let mut left = current.package.clone();
        let mut right = current.package;
        left.footer.text = serde_json::json!("Concurrent writer A");
        right.footer.text = serde_json::json!("Concurrent writer B");
        let (a, b) = tokio::join!(
            theme::save(&site.app, "paper", left, current.version, false),
            theme::save(&site.app, "paper", right, current.version, false)
        );
        assert_eq!(
            usize::from(a.is_ok()) + usize::from(b.is_ok()),
            1,
            "Exactly one concurrent theme writer succeeds"
        );
        assert_eq!(a.err().or_else(|| b.err()).unwrap().0, StatusCode::CONFLICT);
        assert_eq!(
            theme::load(&site.app, "paper", true).await.unwrap().version,
            current.version + 1
        );
        site.close().await;
    }
}

// M2 cluster 3: enforce grammar, work limits, privileges and data-bearing schema safety.
#[tokio::test]
async fn composition_rejects_unsafe_cycles_overwork_and_invalidating_schema_changes() {
    use serde_json::json;
    use wpalt::{schema, theme};
    for pg in engines() {
        let site = Site::new(pg, true).await;
        content::save(
            &site.app,
            site.session(),
            None,
            input("existing", "publish"),
        )
        .await
        .unwrap();
        let registry = schema::Registry::load(&site.app).await.unwrap();
        let base = theme::load(&site.app, "paper", true).await.unwrap().package;
        let mut bad = base.clone();
        bad.footer.kind = "link".into();
        bad.footer.href = json!("javascript:alert(1)");
        assert!(bad.validate(&registry).is_err());
        let mut bad = base.clone();
        bad.components.insert("loop".into(),serde_json::from_value(json!({"parameters":{},"root":{"id":"loop-root","kind":"component","component":"loop"}})).unwrap());
        assert!(bad.validate(&registry).is_err());
        let mut bad = base.clone();
        bad.footer.style.background = "url(https://evil.example)".into();
        assert!(bad.validate(&registry).is_err());
        let mut bad = base.clone();
        bad.templates.insert("post".into(),serde_json::from_value(json!({"id":"outer","kind":"collection","source":"post","limit":50,"children":[{"id":"middle","kind":"collection","source":"post","limit":50,"children":[{"id":"inner","kind":"collection","source":"post","limit":50}]}]})).unwrap());
        assert!(bad.validate(&registry).is_err());
        let mut common = registry.common.clone();
        common.fields.get_mut("subtitle").unwrap().kind = "number".into();
        assert!(schema::save_common(&site.app, common, 1).await.is_err());
        assert_eq!(
            schema::Registry::load(&site.app)
                .await
                .unwrap()
                .common
                .fields["subtitle"]
                .kind,
            "string"
        );
        let body = json!({"csrf":site.session().csrf,"version":1,"package":base,"publish":true})
            .to_string()
            .into_bytes();
        assert_eq!(
            request(
                &site.app,
                "POST",
                "/api/admin/design/paper",
                None,
                "application/json",
                body
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        auth::add_user(
            &site.app,
            "editor@example.test",
            "Editor",
            "editor",
            PASSWORD,
        )
        .await
        .unwrap();
        let (token, editor) = auth::login(&site.app, "editor@example.test", PASSWORD)
            .await
            .unwrap();
        let body = json!({"csrf":editor.csrf,"version":1,"package":base,"publish":true})
            .to_string()
            .into_bytes();
        assert_eq!(
            request(
                &site.app,
                "POST",
                "/api/admin/design/paper",
                Some(&token),
                "application/json",
                body
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        let body = json!({"csrf":"wrong","version":1,"package":base,"publish":true})
            .to_string()
            .into_bytes();
        assert_eq!(
            request(
                &site.app,
                "POST",
                "/api/admin/design/paper",
                Some(&site.token),
                "application/json",
                body
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        let too_large=json!({"csrf":site.session().csrf,"version":1,"package":base,"padding":"x".repeat(340*1024)}).to_string().into_bytes();
        assert_eq!(
            request(
                &site.app,
                "POST",
                "/api/admin/design/paper",
                Some(&site.token),
                "application/json",
                too_large
            )
            .await
            .0,
            StatusCode::PAYLOAD_TOO_LARGE,
            "Design JSON uses a tighter limit than image upload bodies"
        );
        let ctx = theme::context(
            &site.app,
            &site.app.db.settings().await.unwrap(),
            &base,
            None,
            vec![],
            false,
            "home",
            None,
        )
        .await
        .unwrap();
        assert!(
            ctx.queries <= 5,
            "A default page uses a fixed query count independent of listing size"
        );
        site.close().await;
    }
}

// M2 cluster 4: recover complete design/schema state into a fresh engine.
#[tokio::test]
async fn design_backups_preserve_models_options_themes_and_revisions() {
    use serde_json::json;
    use wpalt::{schema, theme};
    for pg in engines() {
        migrate_meaningful_m1_site(pg).await;
        let source = Site::new(pg, true).await;
        let mut common = schema::Registry::load(&source.app).await.unwrap().common;
        common
            .options
            .insert("banner".into(), schema::Field::primitive("string"));
        common
            .options
            .insert("obsolete".into(), schema::Field::primitive("string"));
        schema::save_common(&source.app, common, 1).await.unwrap();
        schema::save_options(
            &source.app,
            json!({"banner":"Recover this value","obsolete":"Old option"}),
            2,
            true,
        )
        .await
        .unwrap();
        let mut package = theme::load(&source.app, "paper", true)
            .await
            .unwrap()
            .package;
        package.footer.text = json!({"bind":"options.obsolete"});
        theme::save(&source.app, "paper", package.clone(), 1, true)
            .await
            .unwrap();
        package.footer.text = json!({"bind":"options.banner"});
        theme::save(&source.app, "paper", package, 2, true)
            .await
            .unwrap();
        schema::save_options(&source.app, json!({"banner":"Recover this value"}), 3, true)
            .await
            .unwrap();
        let mut common = schema::Registry::load(&source.app).await.unwrap().common;
        common.options.remove("obsolete");
        schema::save_common(&source.app, common, 4).await.unwrap();
        let bytes = backup::capture(&source.app).await.unwrap();
        let recovered = Site::new(!pg && std::env::var("TEST_DATABASE_URL").is_ok(), false).await;
        backup::restore(&recovered.app, &bytes).await.unwrap();
        assert!(
            get(&recovered.app, "/", None)
                .await
                .1
                .contains("Recover this value")
        );
        assert_eq!(
            theme::load(&recovered.app, "paper", true)
                .await
                .unwrap()
                .version,
            3
        );
        assert!(
            schema::Registry::load(&recovered.app)
                .await
                .unwrap()
                .common
                .options
                .contains_key("banner")
        );
        assert_eq!(
            get(&recovered.app, "/themes/paper/2/style.css", None)
                .await
                .0,
            StatusCode::OK,
            "Historical styles do not require obsolete field definitions"
        );
        let historical: String = sqlx::query_scalar(
            "SELECT package FROM theme_revisions WHERE theme_id='paper' AND version=2",
        )
        .fetch_one(&recovered.app.db.pool)
        .await
        .unwrap();
        assert!(
            theme::Package::parse(
                &historical,
                &schema::Registry::load(&recovered.app).await.unwrap()
            )
            .is_err(),
            "An obsolete dependency must be repaired before restoring it into the current working graph"
        );
        source.close().await;
        recovered.close().await;
    }
}

/// A real schema-1 fixture from the merged M1 source, with working/live content,
/// terms, authentication and a restorable revision. Migration runs on both engines.
async fn migrate_meaningful_m1_site(pg: bool) {
    let fixture = Site::new(pg, false).await;
    let config = (*fixture.app.config).clone();
    fixture.app.db.pool.close().await;
    if let (Some(name), Some(root)) = (&fixture.schema, &fixture.root_url) {
        let pool = sqlx::PgPool::connect(root).await.unwrap();
        sqlx::query(&format!("DROP SCHEMA {name} CASCADE"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(&format!("CREATE SCHEMA {name}"))
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    } else {
        std::fs::remove_file(fixture._directory.path().join("site.db")).unwrap();
    }
    let db = wpalt::db::Db::open(&config).await.unwrap();
    sqlx::raw_sql(include_str!("fixtures/m1-schema.sql"))
        .execute(&db.pool)
        .await
        .unwrap();
    if !pg {
        sqlx::raw_sql(include_str!("fixtures/m1-search.sql"))
            .execute(&db.pool)
            .await
            .unwrap();
    }
    let user = uuid::Uuid::new_v4().to_string();
    let id = uuid::Uuid::new_v4().to_string();
    let revision = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO settings(id,title,description,theme,navigation,field_schema) VALUES(1,'Migrated journal','Preserve this site','ink','[]',$1)").bind(r#"{"subtitle":"string","featured":"boolean"}"#).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO users(id,email,name,role,password_hash,created_at) VALUES($1,'owner@example.test','Owner','admin',$2,1)").bind(&user).bind(auth::hash_password(PASSWORD).unwrap()).execute(&db.pool).await.unwrap();
    let post = serde_json::json!({"id":id,"slug":"legacy-story","kind":"post","title":"Private M1 title","body":"PRIVATE_M1_WORKING","fields":"{\"subtitle\":\"Preserved fields\",\"featured\":true}","blocks":"[]","status":"published","version":2,"published_slug":"legacy-story","published_title":"M1 public title","published_body":"M1_PUBLIC_BODY","published_fields":"{\"subtitle\":\"Public fields\"}","published_blocks":"[]","publish_at":0,"published_at":1,"updated_at":2,"author_id":user});
    sqlx::query("INSERT INTO posts(id,slug,kind,title,body,fields,blocks,status,version,published_slug,published_title,published_body,published_fields,published_blocks,publish_at,published_at,updated_at,author_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)").bind(&id).bind("legacy-story").bind("post").bind("Private M1 title").bind("PRIVATE_M1_WORKING").bind(post["fields"].as_str().unwrap()).bind("[]").bind("published").bind(2i64).bind("legacy-story").bind("M1 public title").bind("M1_PUBLIC_BODY").bind(post["published_fields"].as_str().unwrap()).bind("[]").bind(0i64).bind(1i64).bind(2i64).bind(&user).execute(&db.pool).await.unwrap();
    let term = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO terms(id,kind,name,slug) VALUES($1,'category','Legacy notes','legacy-notes')",
    )
    .bind(&term)
    .execute(&db.pool)
    .await
    .unwrap();
    for table in ["post_terms", "published_post_terms"] {
        sqlx::query(&format!(
            "INSERT INTO {table}(post_id,term_id) VALUES($1,$2)"
        ))
        .bind(&id)
        .bind(&term)
        .execute(&db.pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO revisions(id,post_id,version,snapshot,created_at) VALUES($1,$2,2,$3,2)",
    )
    .bind(&revision)
    .bind(&id)
    .bind(serde_json::json!({"post":post,"categories":"Legacy notes","tags":""}).to_string())
    .bind(&user)
    .execute(&db.pool)
    .await
    .unwrap();
    db.pool.close().await;
    let app = App::open(config.clone()).await.unwrap();
    let (_, session) = auth::login(&app, "owner@example.test", PASSWORD)
        .await
        .unwrap();
    let html = get(&app, "/legacy-story", None).await.1;
    assert!(html.contains("M1_PUBLIC_BODY") && !html.contains("PRIVATE_M1_WORKING"));
    let restored = content::restore_revision(&app, &session, &id, &revision, 2)
        .await
        .unwrap();
    assert_eq!(restored.body.trim(), "PRIVATE_M1_WORKING");
    assert_eq!(restored.version, 3);
    assert_eq!(
        wpalt::theme::load(&app, "ink", true)
            .await
            .unwrap()
            .published_version,
        1
    );
    wpalt::schema::save_model(&app, "project", wpalt::schema::Model::initial("Project"), 0)
        .await
        .unwrap();
    app.db.pool.close().await;
    let reopened = App::open(config).await.unwrap();
    assert_eq!(
        content::get(&reopened, &id).await.unwrap().version,
        3,
        "Migration is one-off; reopening preserves subsequent data"
    );
    reopened.db.pool.close().await;
    fixture.close().await;
}

async fn multilingual(site: &Site) {
    use wpalt::discovery::{Definition, Language};
    let mut d = Definition::default();
    d.languages.push(Language {
        code: "fr".into(),
        label: "Français".into(),
        search_label: "Rechercher".into(),
        ..Language::default()
    });
    d.languages.push(Language {
        code: "ar".into(),
        label: "العربية".into(),
        direction: "rtl".into(),
        ..Language::default()
    });
    wpalt::discovery::configure(&site.app, d, 1).await.unwrap();
}

#[tokio::test]
async fn multilingual_publication_keeps_drafts_private_and_variants_reciprocal() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        multilingual(&site).await;
        let mut english = input("garden", "publish");
        english.translation_group = "garden-story".into();
        english.seo = r#"{"title":"Garden discovery","schema_type":"Article"}"#.into();
        let en = content::save(&site.app, site.session(), None, english.clone())
            .await
            .unwrap();
        let mut french = input("jardin", "save");
        french.locale = "fr".into();
        french.translation_group = "garden-story".into();
        french.body = "Une histoire de jardin".into();
        let fr = content::save(&site.app, site.session(), None, french.clone())
            .await
            .unwrap();
        assert!(
            !get(&site.app, "/garden", None)
                .await
                .1
                .contains("hreflang=\"fr\"")
        );
        assert_eq!(
            get(&site.app, "/fr/jardin", None).await.0,
            StatusCode::NOT_FOUND
        );
        french.version = fr.version;
        french.action = "publish".into();
        let fr = content::save(&site.app, site.session(), Some(&fr.id), french.clone())
            .await
            .unwrap();
        for path in ["/garden", "/fr/jardin"] {
            let (status, html) = get(&site.app, path, None).await;
            assert_eq!(status, StatusCode::OK);
            assert!(html.contains("hreflang=\"en\"") && html.contains("hreflang=\"fr\""));
            assert!(!html.contains("hreflang=\"ar\""));
            assert_eq!(html.matches("rel=\"canonical\"").count(), 1);
            if path.starts_with("/fr/") {
                assert!(
                    html.contains("/fr/?category=field-notes"),
                    "Taxonomy navigation retains the current language"
                );
            }
        }
        let (status, headers, _) = request(&site.app, "GET", "/jardin", None, "", vec![]).await;
        assert_eq!(status, StatusCode::PERMANENT_REDIRECT);
        assert_eq!(headers["location"], "/fr/jardin");
        french.version = fr.version;
        french.action = "save".into();
        french.seo = r#"{"title":"PRIVATE_SEARCH_TITLE","noindex":true}"#.into();
        content::save(&site.app, site.session(), Some(&fr.id), french.clone())
            .await
            .unwrap();
        assert!(
            !get(&site.app, "/fr/jardin", None)
                .await
                .1
                .contains("PRIVATE_SEARCH_TITLE")
        );
        let preview = get(
            &site.app,
            &format!("/admin/preview/{}", fr.id),
            Some(&site.token),
        )
        .await
        .1;
        assert!(preview.contains("PRIVATE_SEARCH_TITLE") && preview.contains("noindex"));
        assert!(!preview.contains("rel=\"canonical\""));
        english.version = en.version;
        english.action = "save".into();
        english.title = "Concurrent edit".into();
        let (a, b) = tokio::join!(
            content::save(&site.app, site.session(), Some(&en.id), english.clone()),
            content::save(&site.app, site.session(), Some(&en.id), english)
        );
        assert!(a.is_ok() ^ b.is_ok());
        let home = get(&site.app, "/fr/", None).await.1;
        assert!(home.contains("jardin") && !home.contains("href=\"/garden\""));
        let search = get(&site.app, "/fr/search?q=jardin", None).await.1;
        assert!(search.contains("/fr/jardin") && search.contains("noindex,follow"));
        assert!(get(&site.app, "/ar/", None).await.1.contains("dir=\"rtl\""));
        let mut scheduled = input("arabic-scheduled", "schedule");
        scheduled.locale = "ar".into();
        scheduled.publish_at = wpalt::now() + 60;
        scheduled.seo = r#"{"title":"Scheduled discovery"}"#.into();
        let scheduled = content::save(&site.app, site.session(), None, scheduled)
            .await
            .unwrap();
        assert!(
            wpalt::discovery::save_redirect(&site.app, "/ar/arabic-scheduled", "/garden", 301, 0)
                .await
                .is_err()
        );
        assert_eq!(
            get(&site.app, "/ar/arabic-scheduled", None).await.0,
            StatusCode::NOT_FOUND
        );
        sqlx::query("UPDATE posts SET publish_at=$1 WHERE id=$2")
            .bind(wpalt::now() - 1)
            .bind(&scheduled.id)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(content::publish_due(&site.app).await.unwrap(), 1);
        assert!(
            get(&site.app, "/ar/arabic-scheduled", None)
                .await
                .1
                .contains("Scheduled discovery")
        );

        let mut wrong = french.clone();
        wrong.slug = "other-fr".into();
        wrong.version = 0;
        assert!(
            content::save(&site.app, site.session(), None, wrong)
                .await
                .is_err()
        );
        let mut d = wpalt::discovery::load(&site.app).await.unwrap().0;
        d.languages.retain(|l| l.code != "fr");
        assert!(wpalt::discovery::configure(&site.app, d, 2).await.is_err());
        site.close().await;
    }
}

#[tokio::test]
async fn discovery_metadata_redirects_links_and_permissions_are_one_local_system() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        multilingual(&site).await;
        let mut d = wpalt::discovery::load(&site.app).await.unwrap().0;
        d.business = wpalt::discovery::Business {
            name: "Quiet & Co".into(),
            street: "10 Garden Lane".into(),
            city: "Paris".into(),
            country: "FR".into(),
            ..Default::default()
        };
        wpalt::discovery::configure(&site.app, d, 2).await.unwrap();
        let mut p = input("public-story", "publish");
        p.seo=serde_json::json!({"title":"Garden </script> & discovery","description":"A truthful description","schema_type":"Article"}).to_string();
        p.body="[Known](/public-story) [Missing](/missing-local) [External](https://example.test/untested)".into();
        content::save(&site.app, site.session(), None, p)
            .await
            .unwrap();
        let html = get(&site.app, "/public-story", None).await.1;
        let raw = html
            .split("<script type=\"application/ld+json\">")
            .nth(1)
            .unwrap()
            .split("</script>")
            .next()
            .unwrap();
        let graph: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert_eq!(graph[0]["headline"], "Garden </script> & discovery");
        assert_eq!(graph[1]["@type"], "LocalBusiness");
        assert!(html.contains("10 Garden Lane") && html.contains("Quiet &amp; Co"));
        let mut hidden = input("unlisted", "publish");
        hidden.seo = r#"{"noindex":true}"#.into();
        content::save(&site.app, site.session(), None, hidden)
            .await
            .unwrap();
        let map = get(&site.app, "/sitemap.xml", None).await.1;
        assert!(map.contains("/public-story") && !map.contains("/unlisted"));
        assert!(
            get(&site.app, "/sitemap-index.xml", None)
                .await
                .1
                .contains("/sitemap.xml")
        );
        assert!(
            get(&site.app, "/robots.txt", None)
                .await
                .1
                .contains("Sitemap:")
        );
        use wpalt::discovery::save_redirect;
        save_redirect(&site.app, "/old-story", "/public-story", 301, 0)
            .await
            .unwrap();
        let (status, headers, _) = request(&site.app, "GET", "/old-story", None, "", vec![]).await;
        assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
        assert_eq!(headers["location"], "/public-story");
        assert!(
            save_redirect(&site.app, "/public-story", "/elsewhere", 301, 0)
                .await
                .is_err()
        );
        assert!(
            save_redirect(&site.app, "/outside", "https://example.test/", 302, 0)
                .await
                .is_err()
        );
        save_redirect(&site.app, "/a", "/b", 302, 0).await.unwrap();
        assert!(save_redirect(&site.app, "/b", "/a", 301, 0).await.is_err());
        assert!(
            content::save(
                &site.app,
                site.session(),
                None,
                input("old-story", "publish")
            )
            .await
            .is_err()
        );
        let links = get(&site.app, "/admin/discovery/links", Some(&site.token))
            .await
            .1;
        assert!(
            links.contains("/missing-local")
                && !links.contains("/public-story</")
                && !links.contains("https://example.test/untested")
        );
        assert_eq!(
            get(&site.app, "/admin/discovery", None).await.0,
            StatusCode::SEE_OTHER
        );
        let status = form(
            &site.app,
            "/admin/discovery/redirects",
            Some(&site.token),
            &[
                ("csrf", "wrong"),
                ("source", "/bad"),
                ("target", "/public-story"),
                ("code", "301"),
                ("version", "0"),
                ("action", "save"),
            ],
        )
        .await
        .0;
        assert_eq!(status, StatusCode::FORBIDDEN);
        site.close().await;
    }
}

#[tokio::test]
async fn discovery_recovery_preserves_configuration_and_rejects_malicious_rules() {
    for pg in engines() {
        let site = Site::with_connections(pg, true, 1).await;
        multilingual(&site).await;
        let mut p = input("bonjour", "publish");
        p.locale = "fr".into();
        p.seo = r#"{"title":"Bonjour discovery"}"#.into();
        content::save(&site.app, site.session(), None, p)
            .await
            .unwrap();
        wpalt::discovery::save_redirect(&site.app, "/ancien", "/fr/bonjour", 301, 0)
            .await
            .unwrap();
        let archive = backup::capture(&site.app).await.unwrap();
        let target = Site::with_connections(pg, false, 1).await;
        backup::restore(&target.app, &archive).await.unwrap();
        assert!(
            get(&target.app, "/fr/bonjour", None)
                .await
                .1
                .contains("Bonjour discovery")
        );
        assert_eq!(
            request(&target.app, "GET", "/ancien", None, "", vec![])
                .await
                .1["location"],
            "/fr/bonjour"
        );
        let mut envelope: serde_json::Value = serde_json::from_slice(&archive).unwrap();
        let mut payload: serde_json::Value =
            serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
        payload["tables"]["redirects"][0]["target"] = serde_json::json!("https://attacker.test/");
        let payload = payload.to_string();
        envelope["sha256"] = serde_json::json!(auth::digest(payload.as_bytes()));
        envelope["payload"] = serde_json::json!(payload);
        let fresh = Site::with_connections(pg, false, 1).await;
        assert!(
            backup::restore(&fresh.app, &serde_json::to_vec(&envelope).unwrap())
                .await
                .is_err()
        );
        let mut broken: serde_json::Value =
            serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
        broken["tables"]["redirects"][0]
            .as_object_mut()
            .unwrap()
            .remove("source");
        let broken = broken.to_string();
        envelope["sha256"] = serde_json::json!(auth::digest(broken.as_bytes()));
        envelope["payload"] = serde_json::json!(broken);
        assert!(
            backup::restore(&fresh.app, &serde_json::to_vec(&envelope).unwrap())
                .await
                .is_err(),
            "Missing fields are rejected before semantic access, without panic"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
                .fetch_one(&fresh.app.db.pool)
                .await
                .unwrap(),
            0
        );
        fresh.close().await;
        target.close().await;
        site.close().await;
    }
}

/// One volume journey verifies crawler pagination, exclusion and actual engine plans.
/// Timings are observations, never machine-dependent pass thresholds.
#[tokio::test]
async fn discovery_volume_has_bounded_pages_and_indexed_language_search() {
    let mut evidence = Vec::new();
    for pg in engines() {
        let site = Site::new(pg, true).await;
        multilingual(&site).await;
        let base = content::save(
            &site.app,
            site.session(),
            None,
            input("volume-base", "publish"),
        )
        .await
        .unwrap();
        let mut tx = site.app.db.pool.begin().await.unwrap();
        for n in 0..2005 {
            let slug = format!("volume-{n}");
            sqlx::query("INSERT INTO posts(id,slug,kind,title,body,fields,blocks,status,version,published_slug,published_title,published_body,published_fields,published_blocks,publish_at,published_at,updated_at,author_id,locale,published_locale,seo,published_seo,document,published_document) SELECT $1,$2,kind,title,body,fields,blocks,status,version,$2,published_title,published_body,published_fields,published_blocks,publish_at,published_at,updated_at,author_id,$3,$3,$4,$4,document,published_document FROM posts WHERE id=$5")
                .bind(uuid::Uuid::new_v4().to_string()).bind(slug).bind(if n%2==0 {"fr"} else {"en"}).bind(if n%7==0 {r#"{"noindex":true}"#}else{"{}"}).bind(&base.id).execute(&mut *tx).await.unwrap();
        }
        tx.commit().await.unwrap();
        let (status, rendered) = get(&site.app, "/fr/volume-2", None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            rendered.contains("A quiet garden") && rendered.contains("Made here"),
            "Volume rows retain and render their canonical text and callout, not empty legacy columns"
        );
        let index = get(&site.app, "/sitemap-index.xml", None).await.1;
        assert_eq!(index.matches("<sitemap>").count(), 3);
        let mut all = HashSet::new();
        for loc in index
            .split("<loc>")
            .skip(1)
            .map(|part| part.split("</loc>").next().unwrap())
        {
            let path = loc.strip_prefix(&site.app.config.origin()).unwrap();
            let (status, body) = get(&site.app, path, None).await;
            assert_eq!(status, StatusCode::OK);
            assert!(body.matches("<url>").count() <= 1000);
            for url in body
                .split("<loc>")
                .skip(1)
                .map(|part| part.split("</loc>").next().unwrap())
            {
                assert!(
                    all.insert(url.to_owned()),
                    "No duplicate sitemap URLs across stable pages"
                );
            }
        }
        assert_eq!(all.len(), 2006 - 287);
        assert!(
            get(&site.app, "/fr/", None).await.1.contains("/fr/?after="),
            "Pagination retains the language route"
        );
        let plan_sql = if pg {
            "EXPLAIN (ANALYZE,BUFFERS) SELECT id FROM posts WHERE status='published' AND id>'' ORDER BY id LIMIT 1001"
        } else {
            "EXPLAIN QUERY PLAN SELECT id FROM posts WHERE status='published' AND id>'' ORDER BY id LIMIT 1001"
        };
        let plan = sqlx::query(plan_sql)
            .fetch_all(&site.app.db.pool)
            .await
            .unwrap()
            .iter()
            .map(|r| r.get::<String, _>(if pg { 0 } else { 3 }))
            .collect::<Vec<_>>();
        let search_sql = if pg {
            "EXPLAIN (ANALYZE,BUFFERS) SELECT id FROM posts WHERE status='published' AND published_locale='fr' AND NOT COALESCE((published_seo::jsonb->>'noindex')::boolean,false) AND to_tsvector('simple',published_title || ' ' || published_body) @@ plainto_tsquery('simple','garden') ORDER BY published_at DESC,id DESC LIMIT 21"
        } else {
            "EXPLAIN QUERY PLAN SELECT id FROM posts WHERE status='published' AND published_locale='fr' AND COALESCE(json_extract(published_seo,'$.noindex'),0)=0 AND id IN (SELECT id FROM post_search WHERE post_search MATCH 'garden') ORDER BY published_at DESC,id DESC LIMIT 21"
        };
        let search_plan = sqlx::query(search_sql)
            .fetch_all(&site.app.db.pool)
            .await
            .unwrap()
            .iter()
            .map(|r| r.get::<String, _>(if pg { 0 } else { 3 }))
            .collect::<Vec<_>>();
        let search = get(&site.app, "/api/content?lang=fr&q=garden", None).await;
        assert_eq!(search.0, StatusCode::OK);
        assert!(search.1.contains("fr/volume"));
        assert!(!search.1.contains("journal"));
        let mut samples = Vec::new();
        for path in ["/sitemap.xml", "/sitemap-index.xml", "/fr/search?q=garden"] {
            let mut millis = Vec::new();
            for _ in 0..12 {
                let start = std::time::Instant::now();
                assert_eq!(get(&site.app, path, None).await.0, StatusCode::OK);
                millis.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            millis.sort_by(f64::total_cmp);
            samples.push(serde_json::json!({"path":path,"median_ms":millis[6],"max_ms":millis[11],"samples":12}));
        }
        evidence.push(serde_json::json!({"engine":if pg {"postgres"}else{"sqlite"},"published_rows":2006,"sitemap_urls":all.len(),"sitemap_plan":plan,"search_plan":search_plan,"http_in_process":samples,"conditions":"debug build; real DB; HTTP router in-process; no network/browser latency; observations not budgets"}));
        site.close().await;
    }
    std::fs::create_dir_all("work").unwrap();
    std::fs::write(
        "work/m3-volume.json",
        serde_json::to_string_pretty(&evidence).unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn schema_two_upgrade_preserves_publication_and_restorable_editor_history() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let published = content::save(
            &site.app,
            site.session(),
            None,
            input("m2-history", "publish"),
        )
        .await
        .unwrap();
        let mut working = input("m2-history", "save");
        working.version = published.version;
        working.body = "PRIVATE_M2_HISTORY".into();
        let working = content::save(&site.app, site.session(), Some(&published.id), working)
            .await
            .unwrap();
        let history = sqlx::query(
            "SELECT id,snapshot FROM revisions WHERE post_id=$1 ORDER BY version DESC LIMIT 1",
        )
        .bind(&published.id)
        .fetch_one(&site.app.db.pool)
        .await
        .unwrap();
        let revision: String = history.get("id");
        let mut snapshot: serde_json::Value =
            serde_json::from_str(&history.get::<String, _>("snapshot")).unwrap();
        let metadata = [
            "locale",
            "translation_group",
            "seo",
            "published_locale",
            "published_translation_group",
            "published_seo",
        ];
        for key in metadata {
            snapshot["post"].as_object_mut().unwrap().remove(key);
        }
        sqlx::query("UPDATE revisions SET snapshot=$1 WHERE id=$2")
            .bind(snapshot.to_string())
            .bind(&revision)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        for index in [
            "language_posts",
            "translation_drafts",
            "translation_live",
            "indexable_identity",
        ] {
            sqlx::query(&format!("DROP INDEX IF EXISTS {index}"))
                .execute(&site.app.db.pool)
                .await
                .unwrap();
        }
        for column in metadata
            .into_iter()
            .chain(["document", "published_document"])
        {
            sqlx::query(&format!("ALTER TABLE posts DROP COLUMN {column}"))
                .execute(&site.app.db.pool)
                .await
                .unwrap();
        }
        sqlx::query("UPDATE schema_version SET version=2 WHERE id=1")
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        let config = (*site.app.config).clone();
        site.app.db.pool.close().await;
        let upgraded = App::open(config.clone()).await.unwrap();
        let html = get(&upgraded, "/m2-history", None).await.1;
        assert!(html.contains("quiet garden") && !html.contains("PRIVATE_M2_HISTORY"));
        let (_, session) = auth::login(&upgraded, "owner@example.test", PASSWORD)
            .await
            .unwrap();
        let restored = content::restore_revision(
            &upgraded,
            &session,
            &published.id,
            &revision,
            working.version,
        )
        .await
        .unwrap();
        assert!(restored.body.starts_with("PRIVATE_M2_HISTORY"));
        assert_eq!(restored.locale, "en");
        assert_eq!(restored.seo, "{}");
        upgraded.db.pool.close().await;
        let reopened = App::open(config).await.unwrap();
        assert_eq!(
            content::get(&reopened, &published.id)
                .await
                .unwrap()
                .version,
            restored.version
        );
        reopened.db.pool.close().await;
        site.close().await;
    }
}

// M3.5: the same canonical document survives conflicts, snapshots, revisions and recovery.
#[tokio::test]
async fn structured_authoring_preserves_live_isolation_conflicts_and_fresh_recovery() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        assert_eq!(
            get(&site.app, "/api/admin/media", None).await.0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            get(&site.app, "/api/admin/media", Some(&site.token))
                .await
                .0,
            StatusCode::OK
        );
        let mut p = input("structured-story", "publish");
        p.document = wpalt::document::import(
            "## Public café\n\n日本語 **story**",
            r#"[{"kind":"callout","text":"Remember this"}]"#,
        )
        .unwrap()
        .encode();
        p.body = "ATTACKER_PROJECTION_MUST_NOT_WIN".into();
        let published = content::save(&site.app, site.session(), None, p.clone())
            .await
            .unwrap();
        let html = get(&site.app, "/structured-story", None).await.1;
        assert!(
            html.contains("<h2>Public café</h2>")
                && html.contains("class=\"callout\"")
                && !html.contains("ATTACKER_PROJECTION")
        );
        let revision: String =
            sqlx::query_scalar("SELECT id FROM revisions WHERE post_id=$1 AND version=1")
                .bind(&published.id)
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        p.version = published.version;
        p.action = "autosave".into();
        p.document = wpalt::document::import("PRIVATE_TYPED_DOCUMENT", "[]")
            .unwrap()
            .encode();
        let draft = content::save(&site.app, site.session(), Some(&published.id), p.clone())
            .await
            .unwrap();
        assert!(
            !get(&site.app, "/structured-story", None)
                .await
                .1
                .contains("PRIVATE_TYPED_DOCUMENT")
        );
        assert!(
            content::save(&site.app, site.session(), Some(&published.id), p.clone())
                .await
                .is_err()
        );
        p.version = draft.version;
        let mut invalid: serde_json::Value = serde_json::from_str(&p.document).unwrap();
        invalid["root"]["content"][0]["attrs"] = serde_json::json!({"onclick":"unsafe"});
        p.document = invalid.to_string();
        assert!(
            content::save(&site.app, site.session(), Some(&published.id), p)
                .await
                .is_err()
        );
        let restored = content::restore_revision(
            &site.app,
            site.session(),
            &published.id,
            &revision,
            draft.version,
        )
        .await
        .unwrap();
        assert_eq!(restored.document, published.document);
        assert_eq!(
            content::get(&site.app, &published.id)
                .await
                .unwrap()
                .published_document,
            published.document
        );
        let file = wpalt::backup::capture(&site.app).await.unwrap();
        let fresh = Site::new(pg, false).await;
        wpalt::backup::restore(&fresh.app, &file).await.unwrap();
        let recovered = content::get(&fresh.app, &published.id).await.unwrap();
        assert_eq!(recovered.document, published.document);
        assert_eq!(recovered.published_document, published.document);
        assert!(
            get(&fresh.app, "/structured-story", None)
                .await
                .1
                .contains("Remember this")
        );
        site.close().await;
        fresh.close().await;
    }
}

// Upgrade failure must leave meaningful source intact, with an explicit retry path.
#[tokio::test]
async fn structured_upgrade_rolls_back_unsupported_source_and_retries_after_correction() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let post = content::save(
            &site.app,
            site.session(),
            None,
            input("upgrade-source", "publish"),
        )
        .await
        .unwrap();
        let source = format!("{}nested legacy source", "> ".repeat(20));
        sqlx::query("UPDATE posts SET body=$1 WHERE id=$2")
            .bind(&source)
            .bind(&post.id)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        for col in ["document", "published_document"] {
            sqlx::query(&format!("ALTER TABLE posts DROP COLUMN {col}"))
                .execute(&site.app.db.pool)
                .await
                .unwrap();
        }
        sqlx::query("UPDATE schema_version SET version=3 WHERE id=1")
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        let config = (*site.app.config).clone();
        site.app.db.pool.close().await;
        assert!(App::open(config.clone()).await.is_err());
        let db = wpalt::db::Db::open(&config).await.unwrap();
        let version: i64 = sqlx::query_scalar("SELECT version FROM schema_version WHERE id=1")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(version, 3);
        let preserved: String = sqlx::query_scalar("SELECT body FROM posts WHERE id=$1")
            .bind(&post.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(preserved, source);
        // The operator corrects the unsupported source with the old runtime/offline tooling.
        sqlx::query("UPDATE posts SET body='Corrected legacy source' WHERE id=$1")
            .bind(&post.id)
            .execute(&db.pool)
            .await
            .unwrap();
        db.pool.close().await;
        let upgraded = App::open(config).await.unwrap();
        let migrated = content::get(&upgraded, &post.id).await.unwrap();
        assert!(
            wpalt::document::Document::parse(&migrated.document)
                .unwrap()
                .html()
                .contains("Corrected legacy source")
        );
        assert!(
            get(&upgraded, "/upgrade-source", None)
                .await
                .1
                .contains("quiet garden")
        );
        upgraded.db.pool.close().await;
        site.close().await;
    }
}

/// The administrative export must be complete or reject, with bounded buffering.
#[tokio::test]
async fn content_export_enforces_byte_budget_without_truncating_or_mutating_content() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        for n in 0..3 {
            let mut post = input(&format!("large-export-{n}"), "publish");
            post.body = "Readable large story. ".repeat(10000);
            content::save(&site.app, site.session(), None, post)
                .await
                .unwrap();
        }
        let mut bounded = site.app.clone();
        let mut config = (*bounded.config).clone();
        config.max_backup_bytes = 1024 * 1024;
        bounded.config = std::sync::Arc::new(config.clone());
        assert_eq!(
            get(&bounded, "/admin/export", Some(&site.token)).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM posts")
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap(),
            3
        );
        config.max_backup_bytes = 8 * 1024 * 1024;
        bounded.config = std::sync::Arc::new(config);
        let (status, _, bytes) = request(
            &bounded,
            "GET",
            "/admin/export",
            Some(&site.token),
            "",
            Vec::new(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let exported: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(exported["format"], "wpalt-content-v2");
        assert_eq!(exported["posts"].as_array().unwrap().len(), 3);
        assert!(
            exported["posts"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p["published_body"].as_str().unwrap().len() > 200000)
        );
        site.close().await;
    }
}

#[path = "support/membership_journeys.rs"]
mod membership_journeys;
