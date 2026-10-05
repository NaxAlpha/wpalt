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
        // A request can hold an authenticated Session while logout/revocation wins.
        // The domain write must recheck it after acquiring its mutation boundary.
        assert!(
            content::save(
                &site.app,
                &editor,
                None,
                input("revoked-editor-write", "publish")
            )
            .await
            .is_err()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM posts WHERE slug=$1")
                .bind("revoked-editor-write")
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap(),
            0
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
    assert!(
        !c.redacted()["database_url"]
            .as_str()
            .unwrap()
            .contains("secret")
    );
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

#[path = "support/commerce_journeys.rs"]
mod commerce_journeys;

#[tokio::test]
async fn encrypted_recovery_survives_original_loss_and_reports_failed_independent_copies() {
    use wpalt::operations::{encryption, recovery};
    for pg in engines() {
        let mut original = Site::new(pg, true).await;
        let story = content::save(
            &original.app,
            original.session(),
            None,
            input("encrypted-story", "publish"),
        )
        .await
        .unwrap();
        assert_eq!(
            upload(&original, "green.png", &png(), "private").await,
            StatusCode::SEE_OTHER
        );
        let (_, buyer) =
            commerce_journeys::shopper(&original, "recovery-learner@example.test").await;
        let (_, variant) = commerce_journeys::product(&original, "membership", 1200, -1, "").await;
        let checkout = commerce_journeys::cart(&original, &buyer, &variant, "", 1).await;
        let order = wpalt::commerce::orders::checkout(&original.app, &buyer, &checkout)
            .await
            .unwrap();
        commerce_journeys::pay(&original, &order, "independent-recovery-payment").await;
        let policy = wpalt::membership::policy(&original.app, "Recovered academy", "academy", "")
            .await
            .unwrap();
        let lesson = membership_journeys::lesson(story.id, "Recovered lesson");
        let course = wpalt::membership::Course {
            title: "Independent recovery academy".into(),
            policy_id: policy,
            sequential: true,
            lessons: vec![lesson.clone()],
        };
        let course_id = wpalt::membership::create_course(&original.app, &course)
            .await
            .unwrap();
        let edition = wpalt::membership::save_course(&original.app, &course_id, 1, &course, true)
            .await
            .unwrap();
        let attempt_key = uuid::Uuid::new_v4().to_string();
        assert!(
            wpalt::membership::assess(
                &original.app,
                &buyer,
                &course_id,
                &lesson.id,
                wpalt::membership::AttemptInput {
                    version: edition,
                    key: &attempt_key,
                    answers: &[],
                    assignment: ""
                }
            )
            .await
            .unwrap()
            .completed
        );
        let independent = tempfile::tempdir().unwrap();
        let key_store = tempfile::tempdir().unwrap();
        let key = encryption::generate_key();
        let key_path = key_store.path().join("recovery.key");
        backup::write_private(&key_path, hex::encode(key).as_bytes()).unwrap();
        let mut config = (*original.app.config).clone();
        config.recovery = recovery::Config {
            enabled: true,
            incremental: false,
            key_file: key_path,
            destinations: vec![independent.path().to_owned()],
            interval_seconds: 60,
            retain: 1,
        };
        original.app.config = std::sync::Arc::new(config);
        let history_id = uuid::Uuid::new_v4().to_string();
        wpalt::operations::audit::append(
            &original.app,
            wpalt::operations::audit::Event {
                at: wpalt::now(),
                request_id: history_id.clone(),
                actor: "host-owner".into(),
                route: "cli:recovery-check".into(),
                phase: "outcome".into(),
                status: 200,
            },
        )
        .await
        .unwrap();
        let first = recovery::run(&original.app).await.unwrap();
        assert_eq!(first.copies[0].state, "verified");
        let second = recovery::run(&original.app).await.unwrap();
        assert_ne!(first.package, second.package);
        assert!(
            !independent.path().join(first.package).exists(),
            "retention follows a verified replacement"
        );
        let package = std::fs::read(independent.path().join(&second.package)).unwrap();
        assert!(
            !package
                .windows(PASSWORD.len())
                .any(|w| w == PASSWORD.as_bytes())
        );
        assert!(
            encryption::open(&encryption::generate_key(), &package, 256 * 1024 * 1024).is_err()
        );
        let mut tampered = package.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(encryption::open(&key, &tampered, 256 * 1024 * 1024).is_err());
        assert!(encryption::open(&key, &package[..package.len() - 1], 256 * 1024 * 1024).is_err());
        assert!(encryption::open(&key, &package, 1).is_err());
        // A missing mount must not be recreated on the origin disk and reported
        // as an independent successful copy.
        let mut config = (*original.app.config).clone();
        let missing = independent.path().join("missing-mount");
        config.recovery.destinations.push(missing.clone());
        original.app.config = std::sync::Arc::new(config);
        let partial = recovery::run(&original.app).await.unwrap();
        assert_eq!(partial.copies[0].state, "verified");
        assert_eq!(partial.copies[1].state, "failed");
        assert!(!missing.exists());
        assert_eq!(
            recovery::status(&original.app).await.unwrap().copies[1].state,
            "failed"
        );
        // The original site/database becomes unavailable. Recovery uses only the
        // independently stored package and separately held key.
        original.close().await;
        let fresh = Site::new(pg, false).await;
        let plaintext =
            encryption::open(&key, &package, fresh.app.config.max_backup_bytes).unwrap();
        backup::restore(&fresh.app, &plaintext).await.unwrap();
        let (_, owner) = auth::login(&fresh.app, "owner@example.test", PASSWORD)
            .await
            .unwrap();
        assert_eq!(owner.user.role, "admin");
        let (_, buyer) = auth::login(&fresh.app, "recovery-learner@example.test", PASSWORD)
            .await
            .unwrap();
        assert!(
            wpalt::membership::learner_state(&fresh.app, &buyer, &course_id)
                .await
                .unwrap()
                .2[0]
                .completed
        );
        assert!(
            wpalt::operations::audit::read(&fresh.app)
                .await
                .unwrap()
                .iter()
                .any(|event| event.request_id == history_id),
            "original loss must not discard recent operational evidence"
        );
        let payment: String =
            sqlx::query_scalar("SELECT payment_state FROM shop_orders WHERE id=$1")
                .bind(&order)
                .fetch_one(&fresh.app.db.pool)
                .await
                .unwrap();
        assert_eq!(payment, "paid");
        let receipts: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM shop_payments WHERE order_id=$1")
                .bind(&order)
                .fetch_one(&fresh.app.db.pool)
                .await
                .unwrap();
        assert_eq!(
            receipts, 1,
            "restoring learning/access must not replay a payment"
        );

        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM posts WHERE published_slug='encrypted-story'"
            )
            .fetch_one(&fresh.app.db.pool)
            .await
            .unwrap(),
            1
        );
        let filename: String =
            sqlx::query_scalar("SELECT filename FROM media WHERE visibility='private'")
                .fetch_one(&fresh.app.db.pool)
                .await
                .unwrap();
        assert!(
            fresh
                .app
                .config
                .data_dir
                .join("media")
                .join(filename)
                .is_file()
        );

        assert!(
            backup::restore(&fresh.app, &plaintext).await.is_err(),
            "never overwrite an existing site"
        );
        fresh.close().await;
    }
}

#[tokio::test]
async fn anonymous_cache_never_reuses_sessions_and_invalidates_published_access_changes() {
    for pg in engines() {
        let mut site = Site::new(pg, true).await;
        let mut config = (*site.app.config).clone();
        config.cache.enabled = true;
        site.app.config = std::sync::Arc::new(config);
        let published = content::save(
            &site.app,
            site.session(),
            None,
            input("cached-public-story", "publish"),
        )
        .await
        .unwrap();
        let router = wpalt::web::router(site.app.clone());
        let request = || {
            Request::builder()
                .uri("/sitemap.xml")
                .body(Body::empty())
                .unwrap()
        };
        let first = router.clone().oneshot(request()).await.unwrap();
        assert_eq!(first.headers()["x-wpalt-cache"], "miss");
        assert!(
            String::from_utf8(
                axum::body::to_bytes(first.into_body(), 1_000_000)
                    .await
                    .unwrap()
                    .to_vec()
            )
            .unwrap()
            .contains("cached-public-story")
        );
        let public_page = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/cached-public-story")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(public_page.headers()["x-wpalt-cache"], "miss");
        let page_hit = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/cached-public-story")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page_hit.headers()["x-wpalt-cache"], "hit");
        let second = router.clone().oneshot(request()).await.unwrap();
        assert_eq!(second.headers()["x-wpalt-cache"], "hit");
        let cookie = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/sitemap.xml")
                    .header("cookie", format!("wpalt_session={}", site.token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(!cookie.headers().contains_key("x-wpalt-cache"));
        let preload = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/operations/cache/preload")
                    .header("origin", site.app.config.origin())
                    .header("cookie", format!("wpalt_session={}", site.token))
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(format!("csrf={}", site.session().csrf)))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(preload.status(), StatusCode::SEE_OTHER);
        let gzip = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/cached-public-story")
                    .header("accept-encoding", "gzip")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(gzip.headers()["x-wpalt-cache"], "hit");
        assert_eq!(gzip.headers()["content-encoding"], "gzip");
        let encoded = axum::body::to_bytes(gzip.into_body(), 1_000_000)
            .await
            .unwrap();
        use std::io::Read;
        let mut decoded = String::new();
        flate2::read::GzDecoder::new(encoded.as_ref())
            .read_to_string(&mut decoded)
            .unwrap();
        assert!(decoded.contains("cached-public-story"));
        let identity = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/cached-public-story")
                    .header("accept-encoding", "gzip;q=0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(identity.headers()["x-wpalt-cache"], "hit");
        assert!(!identity.headers().contains_key("content-encoding"));
        assert_eq!(
            axum::body::to_bytes(identity.into_body(), 1_000_000)
                .await
                .unwrap()
                .as_ref(),
            decoded.as_bytes()
        );
        let policy = wpalt::membership::policy(&site.app, "Private stories", "paid-reader", "")
            .await
            .unwrap();
        wpalt::membership::protect(&site.app, "post", &published.id, &policy, 0, 0)
            .await
            .unwrap();
        let protected = router.clone().oneshot(request()).await.unwrap();
        assert_eq!(protected.headers()["x-wpalt-cache"], "miss");
        assert!(
            !String::from_utf8(
                axum::body::to_bytes(protected.into_body(), 1_000_000)
                    .await
                    .unwrap()
                    .to_vec()
            )
            .unwrap()
            .contains("cached-public-story")
        );
        let private = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/cached-public-story")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(!private.headers().contains_key("x-wpalt-cache"));
        wpalt::membership::release(&site.app, "post", &published.id)
            .await
            .unwrap();
        let reopened = router.oneshot(request()).await.unwrap();
        assert!(
            String::from_utf8(
                axum::body::to_bytes(reopened.into_body(), 1_000_000)
                    .await
                    .unwrap()
                    .to_vec()
            )
            .unwrap()
            .contains("cached-public-story")
        );
        site.close().await;
    }
}

#[tokio::test]
async fn local_request_rules_cannot_be_bypassed_with_forwarded_headers_or_cached_pages() {
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;
    let mut site = Site::new(false, true).await;
    let mut config = (*site.app.config).clone();
    config.cache.enabled = true;
    config.protection = wpalt::operations::protection::Config {
        enabled: true,
        denied_prefixes: vec!["/admin".into()],
        denied_peers: vec!["192.0.2.9".parse().unwrap()],
        requests_per_window: 2,
        window_seconds: 60,
    };
    site.app.config = std::sync::Arc::new(config);
    let router = wpalt::web::router(site.app.clone());
    let request = |path: &str, peer: &str| {
        let mut request = Request::builder()
            .uri(path)
            .header("x-forwarded-for", "198.51.100.100")
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
        request
    };
    assert_eq!(
        router
            .clone()
            .oneshot(request("/admin", "192.0.2.1:1234"))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request("/", "192.0.2.9:1234"))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request("/", "192.0.2.1:1234"))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request("/", "192.0.2.1:1235"))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request("/", "192.0.2.1:1236"))
            .await
            .unwrap()
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request("/", "192.0.2.2:1234"))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        router
            .oneshot(request("/health", "192.0.2.1:1234"))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    site.close().await;
}

#[tokio::test]
async fn native_image_derivatives_preserve_private_authority_and_validate_processing_limits() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        assert_eq!(
            upload(&site, "private.png", &png(), "private").await,
            StatusCode::SEE_OTHER
        );
        let id: String = sqlx::query_scalar("SELECT id FROM media LIMIT 1")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        let router = wpalt::web::router(site.app.clone());
        let path = format!("/media/{id}/resize/320");
        assert!(
            !router
                .clone()
                .oneshot(Request::builder().uri(&path).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status()
                .is_success()
        );
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&path)
                    .header("cookie", format!("wpalt_session={}", site.token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.headers()["content-type"], "image/webp");
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap();
        assert_eq!(
            image::guess_format(&bytes).unwrap(),
            image::ImageFormat::WebP
        );
        assert_eq!(site.app.media_cache.lock().await.statistics().0, 1);
        // Saturating encoders must not prevent reuse, but cached bytes never bypass authority.
        let permits = site
            .app
            .media_work
            .clone()
            .acquire_many_owned(site.app.media_work.available_permits() as u32)
            .await
            .unwrap();
        let reused = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&path)
                    .header("cookie", format!("wpalt_session={}", site.token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reused.status(), StatusCode::OK);
        assert_eq!(
            axum::body::to_bytes(reused.into_body(), 1_000_000)
                .await
                .unwrap(),
            bytes
        );
        let denied = router
            .clone()
            .oneshot(Request::builder().uri(&path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(
            !denied.status().is_success(),
            "a populated derivative cache cannot grant access"
        );
        drop(permits);
        let decoded = image::load_from_memory(&bytes).unwrap();
        let source = image::load_from_memory(&png()).unwrap();
        assert_eq!(
            decoded.width(),
            source.width(),
            "small images are never enlarged"
        );
        let avif = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("{path}/avif"))
                    .header("cookie", format!("wpalt_session={}", site.token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(avif.status(), StatusCode::OK);
        assert_eq!(avif.headers()["content-type"], "image/avif");
        let avif_bytes = axum::body::to_bytes(avif.into_body(), 1_000_000)
            .await
            .unwrap();
        assert_eq!(
            image::guess_format(&avif_bytes).unwrap(),
            image::ImageFormat::Avif
        );
        assert_eq!(
            site.app.media_cache.lock().await.statistics().0,
            2,
            "formats cannot collide"
        );
        assert!(wpalt::operations::media::derivative_format(&png(), 320, "jpeg").is_err());
        assert!(wpalt::operations::media::derivative(&png(), 999).is_err());
        assert!(wpalt::operations::media::derivative(b"not an image", 320).is_err());
        let mut chunks = Vec::new();
        fn chunk(output: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
            output.extend_from_slice(tag);
            output.extend_from_slice(&(data.len() as u32).to_le_bytes());
            output.extend_from_slice(data);
            if data.len() % 2 == 1 {
                output.push(0);
            }
        }
        let mut extended = vec![2, 0, 0, 0];
        extended.extend_from_slice(&(source.width() - 1).to_le_bytes()[..3]);
        extended.extend_from_slice(&(source.height() - 1).to_le_bytes()[..3]);
        chunk(&mut chunks, b"VP8X", &extended);
        chunk(&mut chunks, b"ANIM", &[0; 6]);
        for _ in 0..2 {
            let mut frame = vec![0; 6];
            frame.extend_from_slice(&(source.width() - 1).to_le_bytes()[..3]);
            frame.extend_from_slice(&(source.height() - 1).to_le_bytes()[..3]);
            frame.extend_from_slice(&[244, 1, 0, 0]);
            frame.extend_from_slice(&bytes[12..]);
            chunk(&mut chunks, b"ANMF", &frame);
        }
        let mut animated_webp = b"RIFF".to_vec();
        animated_webp.extend_from_slice(&(chunks.len() as u32 + 4).to_le_bytes());
        animated_webp.extend_from_slice(b"WEBP");
        animated_webp.extend_from_slice(&chunks);
        assert!(
            image::codecs::webp::WebPDecoder::new(Cursor::new(&animated_webp))
                .unwrap()
                .has_animation()
        );
        for animated in [
            include_bytes!("fixtures/animated.png").as_slice(),
            animated_webp.as_slice(),
        ] {
            assert!(
                wpalt::operations::media::derivative_format(animated, 320, "avif").is_err(),
                "animation must not silently become a still image"
            );
        }
        site.close().await;
    }
}

#[tokio::test]
async fn integrity_scan_reports_damaged_private_files_without_modifying_them() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        assert_eq!(
            upload(&site, "private.png", &png(), "private").await,
            StatusCode::SEE_OTHER
        );
        let clean = wpalt::operations::integrity::scan(&site.app).await.unwrap();
        assert_eq!(clean.checked, 1);
        assert!(clean.failed.is_empty() && !clean.limited);
        let filename: String = sqlx::query_scalar("SELECT filename FROM media LIMIT 1")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        let path = site.app.config.data_dir.join("media").join(filename);
        std::fs::write(&path, b"damaged-image").unwrap();
        let damaged = wpalt::operations::integrity::scan(&site.app).await.unwrap();
        assert_eq!(damaged.failed.len(), 1);
        assert_eq!(std::fs::read(&path).unwrap(), b"damaged-image");
        site.close().await;
    }
}

#[tokio::test]
async fn privileged_audit_records_attempt_and_outcome_without_passwords_or_capabilities() {
    let site = Site::new(false, true).await;
    let router = wpalt::web::router(site.app.clone());
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/users")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("origin", site.app.config.origin())
                .body(Body::from(
                    "password=secret-that-must-not-be-logged&csrf=private-capability",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(!response.status().is_success());
    let events = wpalt::operations::audit::read(&site.app).await.unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].phase, "response");
    assert_eq!(events[1].phase, "intent");
    assert_eq!(events[0].request_id, events[1].request_id);
    assert!(events[0].actor.is_empty());
    let raw =
        std::fs::read_to_string(site.app.config.data_dir.join("privileged-audit.jsonl")).unwrap();
    assert!(!raw.contains("secret-that-must-not-be-logged") && !raw.contains("private-capability"));
    let denied = router
        .oneshot(
            Request::builder()
                .uri("/admin/operations/audit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(denied.status(), StatusCode::OK);
    // Rotation must not make the previous file invisible to owner inspection.
    for n in 0..530 {
        wpalt::operations::audit::append(
            &site.app,
            wpalt::operations::audit::Event {
                at: wpalt::now(),
                request_id: format!("rotation-{n}"),
                actor: "host-owner".into(),
                route: format!("test:{}", "x".repeat(2000)),
                phase: "outcome".into(),
                status: 200,
            },
        )
        .await
        .unwrap();
    }
    assert!(
        site.app
            .config
            .data_dir
            .join("privileged-audit.previous.jsonl")
            .exists()
    );
    let rotated = wpalt::operations::audit::read(&site.app).await.unwrap();
    assert_eq!(rotated.len(), 200);
    assert_eq!(rotated.first().unwrap().request_id, "rotation-529");
    assert_eq!(rotated.last().unwrap().request_id, "rotation-330");
    // If intent cannot be persisted, no privileged handler is dispatched.
    let journal = site.app.config.data_dir.join("privileged-audit.jsonl");
    std::fs::remove_file(&journal).unwrap();
    std::fs::create_dir(&journal).unwrap();
    let blocked = wpalt::web::router(site.app.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/users")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("origin", site.app.config.origin())
                .header("cookie", format!("wpalt_session={}", site.token))
                .body(Body::from(
                    "name=Unwritten&email=new@example.test&role=admin&password=never-create-me",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap(),
        1
    );
    site.close().await;
}

#[tokio::test]
async fn local_authenticator_requires_proof_rejects_replay_and_restores_with_one_use_recovery() {
    use wpalt::operations::factor;
    // RFC 4226 counter vectors establish interoperable HMAC-SHA1 truncation.
    assert_eq!(factor::code(b"12345678901234567890", 0), "755224");
    assert_eq!(factor::code(b"12345678901234567890", 1), "287082");
    for pg in engines() {
        let source = Site::new(pg, true).await;
        let uri = factor::begin(&source.app, source.session(), PASSWORD)
            .await
            .unwrap();
        assert!(uri.starts_with("otpauth://totp/wpalt:"));
        let pending: String = sqlx::query_scalar("SELECT pending FROM user_factors")
            .fetch_one(&source.app.db.pool)
            .await
            .unwrap();
        let key = hex::decode(pending).unwrap();
        let code = factor::code(&key, wpalt::now() / 30);
        let recovery = factor::confirm(&source.app, source.session(), &code)
            .await
            .unwrap();
        assert_eq!(recovery.len(), 8);
        assert!(
            auth::login(&source.app, "owner@example.test", PASSWORD)
                .await
                .is_err()
        );
        assert!(
            auth::login_with_code(&source.app, "owner@example.test", PASSWORD, &code)
                .await
                .is_err(),
            "enrollment proof cannot be reused"
        );
        assert!(
            auth::login_with_code(&source.app, "owner@example.test", PASSWORD, &recovery[0])
                .await
                .is_ok()
        );
        assert!(
            auth::login_with_code(&source.app, "owner@example.test", PASSWORD, &recovery[0])
                .await
                .is_err()
        );
        let (first, second) = tokio::join!(
            auth::login_with_code(&source.app, "owner@example.test", PASSWORD, &recovery[3]),
            auth::login_with_code(&source.app, "owner@example.test", PASSWORD, &recovery[3])
        );
        assert_eq!(
            usize::from(first.is_ok()) + usize::from(second.is_ok()),
            1,
            "only one concurrent recovery-code login may commit"
        );
        let snapshot = backup::capture(&source.app).await.unwrap();
        let fresh = Site::new(pg, false).await;
        backup::restore(&fresh.app, &snapshot).await.unwrap();
        assert!(
            auth::login(&fresh.app, "owner@example.test", PASSWORD)
                .await
                .is_err()
        );
        assert!(
            auth::login_with_code(&fresh.app, "owner@example.test", PASSWORD, &recovery[0])
                .await
                .is_err()
        );
        let (_, session) =
            auth::login_with_code(&fresh.app, "owner@example.test", PASSWORD, &recovery[1])
                .await
                .unwrap();
        factor::disable(&fresh.app, &session, PASSWORD, &recovery[2])
            .await
            .unwrap();
        assert!(
            auth::login(&fresh.app, "owner@example.test", PASSWORD)
                .await
                .is_ok()
        );
        source.close().await;
        fresh.close().await;
    }
}

#[tokio::test]
async fn incremental_recovery_reuses_authenticated_chunks_and_rejects_incomplete_independent_sets()
{
    use wpalt::operations::{encryption, incremental};
    let directory = tempfile::tempdir().unwrap();
    let key = encryption::generate_key();
    let mut source = vec![0u8; 200_000];
    for (i, b) in source.iter_mut().enumerate() {
        *b = (i % 251) as u8;
    }
    let manifest = incremental::write(directory.path(), &key, &source)
        .await
        .unwrap();
    let count = std::fs::read_dir(directory.path().join("wpalt-objects"))
        .unwrap()
        .count();
    let repeat = incremental::write(directory.path(), &key, &source)
        .await
        .unwrap();
    assert_eq!(manifest, repeat);
    assert_eq!(
        std::fs::read_dir(directory.path().join("wpalt-objects"))
            .unwrap()
            .count(),
        count
    );
    assert_eq!(
        incremental::assemble(directory.path(), &key, &manifest, source.len())
            .await
            .unwrap(),
        source
    );
    let path = std::fs::read_dir(directory.path().join("wpalt-objects"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::remove_file(path).unwrap();
    assert!(
        incremental::assemble(directory.path(), &key, &manifest, source.len())
            .await
            .is_err()
    );
    assert!(
        incremental::assemble(directory.path(), &key, &manifest, 1)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn migration_and_incremental_restore_preserve_owned_graph_before_any_target_writes() {
    use wpalt::operations::{encryption, incremental};
    for pg in engines() {
        let original = Site::new(pg, true).await;
        content::save(
            &original.app,
            original.session(),
            None,
            input("incremental-owned-story", "publish"),
        )
        .await
        .unwrap();
        let current = backup::capture(&original.app).await.unwrap();
        let mut old: serde_json::Value = serde_json::from_slice(&current).unwrap();
        let mut payload: serde_json::Value =
            serde_json::from_str(old["payload"].as_str().unwrap()).unwrap();
        payload["schema"] = serde_json::json!(9);
        payload.as_object_mut().unwrap().remove("audit_history");
        payload["tables"]
            .as_object_mut()
            .unwrap()
            .remove("user_factors");
        payload["tables"]
            .as_object_mut()
            .unwrap()
            .remove("user_passkeys");
        payload["tables"]
            .as_object_mut()
            .unwrap()
            .remove("privacy_requests");
        payload["tables"]
            .as_object_mut()
            .unwrap()
            .remove("recovery_mode");
        let raw = serde_json::to_string(&payload).unwrap();
        old["payload"] = serde_json::json!(raw);
        old["sha256"] = serde_json::json!(auth::digest(raw.as_bytes()));
        old["format"] = serde_json::json!("wpalt-backup-v8");
        let legacy = serde_json::to_vec(&old).unwrap();
        assert!(
            backup::inspect(&original.app.config, &legacy).is_err(),
            "ordinary recovery does not carry old runtime formats"
        );
        let migrated = backup::migrate_m6(&original.app.config, &legacy).unwrap();
        assert_eq!(
            backup::inspect(&original.app.config, &migrated).unwrap()["schema"],
            12
        );
        let destination = tempfile::tempdir().unwrap();
        let key = encryption::generate_key();
        let manifest = incremental::write(destination.path(), &key, &migrated)
            .await
            .unwrap();
        let encrypted = encryption::seal(&key, &manifest).unwrap();
        // Lose the source; no parent snapshots or original DB are consulted.
        original.close().await;
        let fresh = Site::new(pg, false).await;
        let manifest =
            encryption::open(&key, &encrypted, fresh.app.config.max_backup_bytes).unwrap();
        let decoded = incremental::assemble(
            destination.path(),
            &key,
            &manifest,
            fresh.app.config.max_backup_bytes,
        )
        .await
        .unwrap();
        backup::restore(&fresh.app, &decoded).await.unwrap();
        assert!(
            auth::login(&fresh.app, "owner@example.test", PASSWORD)
                .await
                .is_ok()
        );
        assert_eq!(
            get(&fresh.app, "/incremental-owned-story", None).await.0,
            StatusCode::OK
        );
        assert!(
            backup::restore(&fresh.app, &decoded).await.is_err(),
            "selective tools never overwrite a live graph"
        );
        fresh.close().await;
    }
}

#[tokio::test]
async fn recovery_cleanup_requires_current_preview_and_preserves_every_retained_point() {
    use wpalt::operations::{encryption, incremental};
    let dir = tempfile::tempdir().unwrap();
    let key = encryption::generate_key();
    let keep = vec![7u8; 90_000];
    let discarded = vec![9u8; 90_000];
    let manifest = incremental::write(dir.path(), &key, &keep).await.unwrap();
    incremental::write(dir.path(), &key, &discarded)
        .await
        .unwrap();
    let point = dir.path().join("wpalt-retained.wpbackup");
    backup::write_private(&point, &encryption::seal(&key, &manifest).unwrap()).unwrap();
    let preview = incremental::cleanup(dir.path(), &key, 1024 * 1024, None)
        .await
        .unwrap();
    assert!(preview.unused_objects > 0);
    assert!(!preview.deleted);
    assert!(
        incremental::cleanup(dir.path(), &key, 1024 * 1024, Some("stale-plan"))
            .await
            .is_err()
    );
    let deleted = incremental::cleanup(dir.path(), &key, 1024 * 1024, Some(&preview.plan))
        .await
        .unwrap();
    assert!(deleted.deleted);
    assert_eq!(
        incremental::assemble(dir.path(), &key, &manifest, 1024 * 1024)
            .await
            .unwrap(),
        keep
    );
    assert_eq!(
        incremental::cleanup(dir.path(), &key, 1024 * 1024, None)
            .await
            .unwrap()
            .unused_objects,
        0
    );
    incremental::write(dir.path(), &key, &discarded)
        .await
        .unwrap();
    let before = std::fs::read_dir(dir.path().join("wpalt-objects"))
        .unwrap()
        .count();
    std::fs::write(&point, b"damaged recovery point").unwrap();
    assert!(
        incremental::cleanup(dir.path(), &key, 1024 * 1024, Some(&preview.plan))
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read_dir(dir.path().join("wpalt-objects"))
            .unwrap()
            .count(),
        before,
        "corrupt retained point stops deletion"
    );
}

#[tokio::test]
async fn paginated_integrity_reaches_late_damage_and_labels_patterns_without_deleting_files() {
    use wpalt::operations::integrity;
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let bytes = png();
        let mut tx = site.app.db.pool.begin().await.unwrap();
        let mut last = String::new();
        for i in 1..=1001u128 {
            let id = uuid::Uuid::from_u128(i).to_string();
            let filename = format!("{id}.png");
            std::fs::write(
                site.app.config.data_dir.join("media").join(&filename),
                &bytes,
            )
            .unwrap();
            sqlx::query("INSERT INTO media(id,filename,original_name,mime,alt,visibility,size,sha256,created_at) VALUES($1,$2,'fixture.png','image/png','','private',$3,$4,0)").bind(&id).bind(&filename).bind(bytes.len() as i64).bind(auth::digest(&bytes)).execute(&mut *tx).await.unwrap();
            last = filename;
        }
        tx.commit().await.unwrap();
        let path = site.app.config.data_dir.join("media").join(last);
        std::fs::write(&path, b"<?php late-damaged-file").unwrap();
        let first = integrity::scan(&site.app).await.unwrap();
        assert_eq!(first.checked, 1000);
        assert!(first.limited && first.failed.is_empty());
        assert_eq!(first.next_attachment, "done");
        let next = integrity::scan_page(&site.app, &first.next_image, &first.next_attachment)
            .await
            .unwrap();
        assert_eq!(next.checked, 1);
        assert!(!next.limited);
        assert_eq!(next.failed.len(), 1);
        // Size mismatch is already conclusive; no unnecessary file read/pattern work.
        assert!(next.pattern_warnings.is_empty());
        let bytes = b"<?php embedded payload for explicit owner review";
        std::fs::write(&path, bytes).unwrap();
        sqlx::query("UPDATE media SET size=$1,sha256=$2 WHERE filename=$3")
            .bind(bytes.len() as i64)
            .bind(auth::digest(bytes))
            .bind(path.file_name().unwrap().to_str().unwrap())
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        let warning = integrity::scan_page(&site.app, &first.next_image, "done")
            .await
            .unwrap();
        assert!(warning.failed.is_empty());
        assert_eq!(warning.pattern_warnings.len(), 1);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(
            integrity::scan_page(&site.app, "../unsafe", "")
                .await
                .is_err()
        );
        site.close().await;
    }
}

/// A local challenge admits one submission, binds its destination and rejects
/// forged/replayed work before moderation. Both supported database engines run this.
#[tokio::test]
async fn local_abuse_guard_binds_resource_and_admits_only_one_racing_submitter() {
    for pg in engines() {
        let mut site = Site::new(pg, true).await;
        let config = std::sync::Arc::make_mut(&mut site.app.config);
        config.spam.enabled = true;
        config.spam.proof_bits = 8;
        config.spam.max_links = 1;
        content::save(
            &site.app,
            site.session(),
            None,
            input("guarded-story", "publish"),
        )
        .await
        .unwrap();
        let resource = "comment:guarded-story";
        let (status, _, bytes) = request(
            &site.app,
            "POST",
            "/api/spam/challenge",
            None,
            "application/json",
            serde_json::to_vec(&serde_json::json!({"resource":resource})).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let challenge: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let token = challenge["token"].as_str().unwrap().to_owned();
        let solution = (0..1048576)
            .map(|n| n.to_string())
            .find(|n| wpalt::operations::spam::solved(&token, n, 8))
            .unwrap();
        let proof = wpalt::operations::spam::Proof {
            token: token.clone(),
            solution: solution.clone(),
            website: String::new(),
        };
        assert!(
            wpalt::operations::spam::verify(&site.app, "form:other", &proof, "hello")
                .await
                .is_err()
        );
        assert!(
            wpalt::operations::spam::verify(&site.app, resource, &proof, "HTTP://a HTTPS://b")
                .await
                .is_err()
        );
        let honeypot = wpalt::operations::spam::Proof {
            token: token.clone(),
            solution: solution.clone(),
            website: "bot.example".into(),
        };
        assert!(
            wpalt::operations::spam::verify(&site.app, resource, &honeypot, "hello")
                .await
                .is_err()
        );
        let body = format!(
            "name=Visitor&body=A+useful+comment&token={token}&solution={solution}&website="
        )
        .into_bytes();
        let (left, right) = tokio::join!(
            request(
                &site.app,
                "POST",
                "/guarded-story/comments",
                None,
                "application/x-www-form-urlencoded",
                body.clone()
            ),
            request(
                &site.app,
                "POST",
                "/guarded-story/comments",
                None,
                "application/x-www-form-urlencoded",
                body
            )
        );
        assert_eq!(
            [left.0, right.0]
                .iter()
                .filter(|s| **s == StatusCode::OK)
                .count(),
            1
        );
        assert_eq!(
            [left.0, right.0]
                .iter()
                .filter(|s| **s == StatusCode::FORBIDDEN)
                .count(),
            1
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM comments")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            count, 1,
            "a losing replay must not create a moderation entry"
        );
        site.close().await;
    }
}

/// A native archive retry preserves the same segment; authenticated filename and
/// cluster identity are checked before an engine output can be replaced.
#[tokio::test]
async fn native_wal_archive_is_idempotent_bound_to_cluster_and_preserves_output_on_corruption() {
    use wpalt::operations::{encryption, postgres_archive as wal};
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("archive");
    std::fs::create_dir(&archive).unwrap();
    let key_path = directory.path().join("key");
    backup::write_private(
        &key_path,
        hex::encode(encryption::generate_key()).as_bytes(),
    )
    .unwrap();
    let config = Config {
        postgres_archive: wal::Config {
            enabled: true,
            system_id: "123456789".into(),
            key_file: key_path,
            directory: archive.clone(),
            max_segment_bytes: 1024 * 1024,
        },
        ..Default::default()
    };
    let name = "000000010000000000000001";
    let mut segment = vec![0u8; 1024 * 1024];
    segment[0..2].copy_from_slice(&0xD116u16.to_le_bytes());
    segment[2..4].copy_from_slice(&2u16.to_le_bytes());
    segment[4..8].copy_from_slice(&1u32.to_le_bytes());
    segment[8..16].copy_from_slice(&1048576u64.to_le_bytes());
    segment[24..32].copy_from_slice(&123456789u64.to_le_bytes());
    segment[32..36].copy_from_slice(&1048576u32.to_le_bytes());
    segment[36..40].copy_from_slice(&8192u32.to_le_bytes());
    segment[128..136].copy_from_slice(b"WAL_DATA");
    let input = directory.path().join("input");
    std::fs::write(&input, &segment).unwrap();
    wal::store(&config, &input, name).await.unwrap();
    let package = archive.join(format!("{name}.wpwal"));
    let first = std::fs::read(&package).unwrap();
    wal::store(&config, &input, name).await.unwrap();
    assert_eq!(
        std::fs::read(&package).unwrap(),
        first,
        "idempotent retry cannot reseal or replace an existing segment"
    );
    let output = directory.path().join("engine-output");
    std::fs::write(&output, b"previous engine bytes").unwrap();
    wal::restore(&config, name, &output).await.unwrap();
    assert_eq!(std::fs::read(&output).unwrap(), segment);
    let mut swapped = config.clone();
    swapped.postgres_archive.system_id = "123456790".into();
    assert!(wal::restore(&swapped, name, &output).await.is_err());
    let other = "000000010000000000000002";
    std::fs::write(archive.join(format!("{other}.wpwal")), &first).unwrap();
    assert!(wal::restore(&config, other, &output).await.is_err());
    let mut damaged = first;
    *damaged.last_mut().unwrap() ^= 1;
    std::fs::write(&package, damaged).unwrap();
    assert!(wal::restore(&config, name, &output).await.is_err());
    assert_eq!(
        std::fs::read(&output).unwrap(),
        segment,
        "failed authentication cannot touch existing engine output"
    );
    assert!(wal::store(&config, &input, "../outside").await.is_err());
    assert!(!wal::valid_name("000000010000000000000001.partial"));
    assert!(
        wal::valid_name("00000002.history")
            && wal::valid_name("000000010000000000000001.00000028.backup")
    );
}

/// Cleanup must preserve draft/history references, reject stale plans and leave
/// unexpected filesystem objects untouched. Exercise both real database engines.
#[tokio::test]
async fn cleanup_preserves_references_and_rejects_stale_or_unsafe_plans() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let retained = uuid::Uuid::new_v4().to_string();
        let unused = uuid::Uuid::new_v4().to_string();
        let orphan = uuid::Uuid::new_v4().to_string();
        let disposable = uuid::Uuid::new_v4().to_string();
        for id in [&retained, &unused, &disposable] {
            let filename = format!("{id}.png");
            tokio::fs::write(
                site.app.config.data_dir.join("media").join(&filename),
                b"synthetic",
            )
            .await
            .unwrap();
            sqlx::query("INSERT INTO media(id,filename,original_name,mime,alt,visibility,size,sha256,created_at) VALUES($1,$2,'fixture.png','image/png','','private',9,$3,0)").bind(id).bind(filename).bind(auth::digest(b"synthetic")).execute(&site.app.db.pool).await.unwrap();
        }
        let orphan_path = site
            .app
            .config
            .data_dir
            .join("media")
            .join(format!("{orphan}.png"));
        tokio::fs::write(&orphan_path, b"orphan").await.unwrap();
        let mut post = input("cleanup-draft", "save");
        // Imported percent-encoded media URLs also retain the original.
        post.body = format!("![draft](/media/{})", retained.replace('-', "%2D"));
        content::save(&site.app, site.session(), None, post)
            .await
            .unwrap();
        let plan = wpalt::operations::cleanup::preview(&site.app)
            .await
            .unwrap();
        assert_eq!(plan.retained, 1);
        assert_eq!(plan.candidates.len(), 3);
        assert!(
            plan.candidates
                .iter()
                .any(|c| c.id == orphan && !c.registered)
        );
        // A concurrent editorial save adds a reference after the preview.
        let mut second = input("cleanup-new-reference", "save");
        second.body = format!("![new](/media/{unused})");
        content::save(&site.app, site.session(), None, second)
            .await
            .unwrap();
        assert_eq!(
            wpalt::operations::cleanup::execute(&site.app, &plan.hash, plan.cutoff)
                .await
                .unwrap_err()
                .0,
            StatusCode::CONFLICT
        );
        assert!(orphan_path.exists());
        let next = wpalt::operations::cleanup::preview(&site.app)
            .await
            .unwrap();
        assert_eq!(next.retained, 2);
        let result = wpalt::operations::cleanup::execute(&site.app, &next.hash, next.cutoff)
            .await
            .unwrap();
        assert_eq!(result.removed_files, 2);
        assert!(result.pending_files.is_empty());
        assert!(!orphan_path.exists());
        assert!(
            site.app
                .config
                .data_dir
                .join("media")
                .join(format!("{retained}.png"))
                .exists()
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM media")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
        let unexpected = site
            .app
            .config
            .data_dir
            .join("media")
            .join("owner-notes.txt");
        tokio::fs::write(&unexpected, b"must retain").await.unwrap();
        assert!(
            wpalt::operations::cleanup::preview(&site.app)
                .await
                .is_err()
        );
        assert_eq!(tokio::fs::read(unexpected).await.unwrap(), b"must retain");
        site.close().await;
    }
}

/// Reachable CSS includes reused components/responsive rules without shipping
/// unrelated templates; public and draft routes preserve publication boundaries.
#[tokio::test]
async fn theme_styles_are_template_scoped_and_preserve_reachable_components() {
    use wpalt::theme;
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let mut stored = theme::load(&site.app, "paper", true).await.unwrap();
        let component: theme::Node=serde_json::from_value(serde_json::json!({"id":"critical_component","kind":"text","text":"Included","style":{"color":"#123456","mobile_columns":1}})).unwrap();
        stored.package.components.insert(
            "critical".into(),
            theme::Component {
                parameters: Default::default(),
                root: component,
            },
        );
        stored
            .package
            .templates
            .get_mut("home")
            .unwrap()
            .children
            .push(
            serde_json::from_value(
                serde_json::json!({"id":"critical_use","kind":"component","component":"critical"}),
            )
            .unwrap(),
        );
        stored.package.templates.get_mut("search").unwrap().children.push(serde_json::from_value(serde_json::json!({"id":"unrelated_search","kind":"text","text":"Search only","style":{"color":"#654321"}})).unwrap());
        let full = stored.package.css();
        let scoped = stored.package.css_for("home").unwrap();
        assert!(
            scoped.contains(".n-critical_component{")
                && scoped.contains("@media(max-width:700px){.n-critical_component")
        );
        assert!(!scoped.contains(".n-unrelated_search{"));
        assert!(scoped.len() < full.len());
        theme::save(&site.app, "paper", stored.package, stored.version, true)
            .await
            .unwrap();
        let (_, html) = get(&site.app, "/", None).await;
        assert!(html.contains("style.css?template=home"));
        assert!(html.contains("rel=\"preload\"") && html.contains("as=\"style\""));
        let (status, css) = get(&site.app, "/themes/paper/2/style.css?template=home", None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(css.contains(".n-critical_component{") && !css.contains(".n-unrelated_search{"));
        assert_eq!(
            get(
                &site.app,
                "/themes/paper/2/style.css?template=unknown",
                None
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
        site.close().await;
    }
}

/// Actual native processing, HTTP authority and host-independent recovery. The
/// optional tool is required in CI, explicit skip only for local installations.
#[tokio::test]
async fn local_video_worker_preserves_authority_and_recovers_processed_media() {
    let ffmpeg = std::env::var("WPALT_TEST_FFMPEG").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/opt/homebrew/bin/ffmpeg".into()
        } else {
            "/usr/bin/ffmpeg".into()
        }
    });
    let ffprobe = std::env::var("WPALT_TEST_FFPROBE").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/opt/homebrew/bin/ffprobe".into()
        } else {
            "/usr/bin/ffprobe".into()
        }
    });
    if !std::path::Path::new(&ffmpeg).exists() || !std::path::Path::new(&ffprobe).exists() {
        assert!(
            std::env::var("CI").is_err(),
            "CI must install the native video fixture tools"
        );
        eprintln!("Local optional FFmpeg fixture unavailable; video journey not verified.");
        return;
    }
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("source.mp4");
    let status = std::process::Command::new(&ffmpeg)
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=green:s=64x48:r=5",
            "-t",
            "1",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-threads",
            "1",
        ])
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success());
    let source = std::fs::read(source).unwrap();
    for pg in engines() {
        let mut site = Site::new(pg, true).await;
        assert!(
            wpalt::operations::video::transcode(&site.app, &source)
                .await
                .is_err(),
            "disabled worker is inert"
        );
        let mut config = (*site.app.config).clone();
        config.video = wpalt::operations::video::Config {
            enabled: true,
            ffmpeg: ffmpeg.clone().into(),
            ffprobe: ffprobe.clone().into(),
        };
        site.app.config = std::sync::Arc::new(config);
        let mut permits = Vec::new();
        for _ in 0..site.app.config.worker_concurrency {
            permits.push(site.app.media_work.try_acquire().unwrap());
        }
        assert!(
            wpalt::operations::video::transcode(&site.app, &source)
                .await
                .is_err(),
            "saturated native pool rejects work"
        );
        drop(permits);
        let mut body=format!("--video-fixture\r\nContent-Disposition: form-data; name=\"csrf\"\r\n\r\n{}\r\n--video-fixture\r\nContent-Disposition: form-data; name=\"file\"; filename=\"short.mp4\"\r\nContent-Type: video/mp4\r\n\r\n",site.session().csrf).into_bytes();
        body.extend_from_slice(&source);
        body.extend_from_slice(b"\r\n--video-fixture--\r\n");
        let (status, _, _) = request(
            &site.app,
            "POST",
            "/admin/media/video",
            Some(&site.token),
            "multipart/form-data; boundary=video-fixture",
            body.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let id: String = sqlx::query_scalar("SELECT id FROM media WHERE mime='video/mp4'")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        let url = format!("/media/{id}");
        assert_eq!(get(&site.app, &url, None).await.0, StatusCode::UNAUTHORIZED);
        let (status, headers, encoded) =
            request(&site.app, "GET", &url, Some(&site.token), "", vec![]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers["content-type"], "video/mp4");
        assert_eq!(&encoded[4..8], b"ftyp");
        assert!(
            get(&site.app, "/admin/media", Some(&site.token))
                .await
                .1
                .contains("Open processed video")
        );
        let archive = backup::capture(&site.app).await.unwrap();
        auth::add_user(
            &site.app,
            "another-owner@example.test",
            "Another owner",
            "admin",
            PASSWORD,
        )
        .await
        .unwrap();
        // The other owner holds the write boundary while a valid native request
        // starts. Revoke its account/session before releasing the commit lock.
        let guard = site.app.mutation().await.unwrap();
        let app = site.app.clone();
        let token = site.token.clone();
        let pending = tokio::spawn(async move {
            request(
                &app,
                "POST",
                "/admin/media/video",
                Some(&token),
                "multipart/form-data; boundary=video-fixture",
                body,
            )
            .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while site.app.media_work.available_permits() == site.app.config.worker_concurrency {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("native work must reach its admitted processing stage");
        let mut tx = site.app.db.pool.begin().await.unwrap();
        sqlx::query("UPDATE users SET role='editor' WHERE id=$1")
            .bind(&site.session().user.id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("DELETE FROM sessions WHERE user_id=$1")
            .bind(&site.session().user.id)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        drop(guard);
        assert_eq!(
            pending.await.unwrap().0,
            StatusCode::UNAUTHORIZED,
            "revocation during processing wins before publication"
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM media WHERE mime='video/mp4'")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            count, 1,
            "revoked native request cannot leave another media record"
        );
        assert!(
            wpalt::operations::video::transcode(&site.app, b"not-a-video")
                .await
                .is_err()
        );
        assert!(
            std::fs::read_dir(&site.app.config.data_dir)
                .unwrap()
                .all(|p| !p
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".video-")),
            "completed and rejected workers leave no private intermediates"
        );
        site.close().await;
        let fresh = Site::new(pg, false).await;
        backup::restore(&fresh.app, &archive).await.unwrap();
        let (token, _) = auth::login(&fresh.app, "owner@example.test", PASSWORD)
            .await
            .unwrap();
        let (_, _, restored) = request(&fresh.app, "GET", &url, Some(&token), "", vec![]).await;
        assert_eq!(restored, encoded);
        assert_eq!(
            get(&fresh.app, &url, None).await.0,
            StatusCode::UNAUTHORIZED
        );
        fresh.close().await;
    }
}

/// Native peer/session-derived buckets preserve actual role and regional
/// presentation, without letting forged forwarding or stale account state win.
#[tokio::test]
async fn role_region_cache_variants_revalidate_authority_and_ignore_spoofed_headers() {
    use axum::extract::ConnectInfo;
    use wpalt::operations::variants;
    for pg in engines() {
        let mut site = Site::new(pg, true).await;
        let mut cfg = (*site.app.config).clone();
        cfg.cache.enabled = true;
        cfg.variants = variants::Config {
            role_variants: true,
            region_networks: vec![
                variants::Region {
                    network: "192.0.2.0/24".into(),
                    region: "local-east".into(),
                },
                variants::Region {
                    network: "2001:db8::/32".into(),
                    region: "local-west".into(),
                },
            ],
        };
        site.app.config = std::sync::Arc::new(cfg);
        let stored = wpalt::theme::load(&site.app, "paper", true).await.unwrap();
        let mut package = stored.package;
        for (id, bind) in [("region_label", "site.region"), ("role_label", "site.role")] {
            package.templates.get_mut("home").unwrap().children.push(
                serde_json::from_value(
                    serde_json::json!({"id":id,"kind":"text","text":{"bind":bind}}),
                )
                .unwrap(),
            );
        }
        wpalt::theme::save(&site.app, "paper", package, stored.version, true)
            .await
            .unwrap();
        async fn visit(
            site: &Site,
            peer: &str,
            cookie: Option<String>,
        ) -> (StatusCode, axum::http::HeaderMap, String) {
            let mut request = Request::builder()
                .uri("/")
                .header("x-wpalt-local-region", "forged")
                .header("x-wpalt-local-role", "admin")
                .header("x-forwarded-for", "192.0.2.55")
                .body(Body::empty())
                .unwrap();
            request
                .extensions_mut()
                .insert(ConnectInfo(peer.parse::<std::net::SocketAddr>().unwrap()));
            if let Some(cookie) = cookie {
                request
                    .headers_mut()
                    .insert("cookie", cookie.parse().unwrap());
            }
            let response = wpalt::web::router(site.app.clone())
                .oneshot(request)
                .await
                .unwrap();
            let status = response.status();
            let headers = response.headers().clone();
            let body = String::from_utf8(
                response
                    .into_body()
                    .collect()
                    .await
                    .unwrap()
                    .to_bytes()
                    .to_vec(),
            )
            .unwrap();
            (status, headers, body)
        }
        let (_, east_headers, east) = visit(&site, "192.0.2.7:1234", None).await;
        assert!(
            east.contains("local-east") && east.contains("anonymous") && !east.contains("forged")
        );
        assert_eq!(east_headers["x-wpalt-cache"], "miss");
        assert_eq!(
            visit(&site, "192.0.2.9:1234", None).await.1["x-wpalt-cache"],
            "hit"
        );
        let (_, west_headers, west) = visit(&site, "[2001:db8::1]:1234", None).await;
        assert!(west.contains("local-west"));
        assert_eq!(west_headers["x-wpalt-cache"], "miss");
        let cookie = format!("wpalt_session={}", site.token);
        let (_, headers, owner) = visit(&site, "192.0.2.7:1234", Some(cookie.clone())).await;
        assert!(owner.contains(">admin</p>"));
        assert_eq!(headers["x-wpalt-cache"], "miss");
        assert_eq!(
            visit(&site, "192.0.2.7:1234", Some(cookie.clone())).await.1["x-wpalt-cache"],
            "hit"
        );
        let (_, headers, _) = visit(
            &site,
            "192.0.2.7:1234",
            Some(format!("{cookie}; private_preference=1")),
        )
        .await;
        assert!(
            !headers.contains_key("x-wpalt-cache"),
            "unknown personalized cookies still bypass storage"
        );
        {
            let _guard = site.app.mutation().await.unwrap();
            sqlx::query("UPDATE users SET role='subscriber' WHERE id=$1")
                .bind(&site.session().user.id)
                .execute(&site.app.db.pool)
                .await
                .unwrap();
        }
        let (_, headers, subscriber) = visit(&site, "192.0.2.7:1234", Some(cookie)).await;
        assert!(subscriber.contains(">subscriber</p>") && !subscriber.contains(">admin</p>"));
        assert_eq!(headers["x-wpalt-cache"], "miss");
        let (_, _, unknown) = visit(&site, "198.51.100.7:1234", None).await;
        assert!(unknown.contains(">unknown</p>") && !unknown.contains("local-east"));
        site.close().await;
    }
}

#[tokio::test]
async fn background_history_reports_failure_interruption_and_blocks_unrecorded_dispatch() {
    use wpalt::operations::jobs::{read, run_cycle, stage};
    for postgres in engines() {
        let site = Site::new(postgres, true).await;
        run_cycle(&site.app, async {
            vec![
                stage("publication", async { Ok(3) }).await,
                stage("mail", async {
                    Err(wpalt::error::Error::invalid("secret-mail-token"))
                })
                .await,
            ]
        })
        .await
        .unwrap();
        let history = read(&site.app).await.unwrap();
        assert_eq!(history[0].state, "failed");
        assert_eq!(history[0].stages[0].count, Some(3));
        assert!(!history[0].stages[1].succeeded);
        let path = site.app.config.data_dir.join("background-jobs.json");
        assert!(
            !tokio::fs::read_to_string(&path)
                .await
                .unwrap()
                .contains("secret-mail-token")
        );

        // Kill a worker after durable intent but before any claimed outcome.
        let entered = std::sync::Arc::new(tokio::sync::Notify::new());
        let signal = entered.clone();
        let app = site.app.clone();
        let task = tokio::spawn(async move {
            run_cycle(&app, async {
                signal.notify_one();
                std::future::pending::<Vec<wpalt::operations::jobs::Stage>>().await
            })
            .await
        });
        entered.notified().await;
        assert_eq!(read(&site.app).await.unwrap()[0].state, "running");
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        run_cycle(&site.app, async {
            vec![stage("publication", async { Ok(0) }).await]
        })
        .await
        .unwrap();
        let history = read(&site.app).await.unwrap();
        assert_eq!(history[0].state, "succeeded");
        assert_eq!(history[1].state, "interrupted");
        assert_eq!(history[1].finished_at, 0);
        assert!(history[1].stages.is_empty());
        for _ in 0..65 {
            run_cycle(&site.app, async {
                vec![stage("publication", async { Ok(0) }).await]
            })
            .await
            .unwrap();
        }
        assert_eq!(read(&site.app).await.unwrap().len(), 64);
        assert!(tokio::fs::metadata(&path).await.unwrap().len() <= 128 * 1024);
        tokio::fs::remove_file(&path).await.unwrap();
        tokio::fs::create_dir(&path).await.unwrap();
        let dispatched = std::sync::atomic::AtomicBool::new(false);
        assert!(
            run_cycle(&site.app, async {
                dispatched.store(true, std::sync::atomic::Ordering::SeqCst);
                Vec::new()
            })
            .await
            .is_err()
        );
        assert!(!dispatched.load(std::sync::atomic::Ordering::SeqCst));
        site.close().await;
    }
}

#[tokio::test]
async fn local_layout_metadata_respects_media_authority_and_never_rewrites_publication() {
    use serde_json::json;
    use wpalt::theme;
    for pg in engines() {
        let site = Site::new(pg, true).await;
        assert_eq!(
            upload(&site, "layout.png", &png(), "public").await,
            StatusCode::SEE_OTHER
        );
        assert_eq!(
            upload(&site, "private.png", &png(), "private").await,
            StatusCode::SEE_OTHER
        );
        let public: String = sqlx::query_scalar("SELECT id FROM media WHERE visibility='public'")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        let private: String = sqlx::query_scalar("SELECT id FROM media WHERE visibility='private'")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        let mut package = theme::load(&site.app, "paper", true).await.unwrap().package;
        package.templates.get_mut("home").unwrap().children.extend([
            serde_json::from_value(
                json!({"id":"lead_local","kind":"image","image":public,"loading":"eager"}),
            )
            .unwrap(),
            serde_json::from_value(json!({"id":"private_local","kind":"image","image":private}))
                .unwrap(),
        ]);
        theme::save(&site.app, "paper", package, 1, true)
            .await
            .unwrap();
        let (_, home) = get(&site.app, "/", None).await;
        assert!(home.contains("width=\"8\" height=\"8\" loading=\"eager\" fetchpriority=\"high\""));
        assert!(home.contains(&format!("href=\"/media/{public}\" as=\"image\"")));
        assert!(!home.contains(&format!("/media/{private}")));
        let mut post = input("imported-local", "publish");
        post.body = format!(
            "![Imported local image](/media/{public})\n\n![External](https://external.invalid/image.png)"
        );
        let record = content::save(&site.app, site.session(), None, post)
            .await
            .unwrap();
        let original: String =
            sqlx::query_scalar("SELECT published_document FROM posts WHERE id=$1")
                .bind(&record.id)
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        assert!(!original.contains("\"width\""));
        let (_, rendered) = get(&site.app, "/imported-local", None).await;
        assert!(rendered.contains(&format!(
            "src=\"/media/{public}\" alt=\"Imported local image\" width=\"8\" height=\"8\""
        )));
        assert!(rendered.contains("https://external.invalid/image.png"));
        let unchanged: String =
            sqlx::query_scalar("SELECT published_document FROM posts WHERE id=$1")
                .bind(&record.id)
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        assert_eq!(original, unchanged);
        // Reuse metadata without permitting a cached private reference to render.
        let settings = site.app.db.settings().await.unwrap();
        let live = theme::load(&site.app, "paper", false)
            .await
            .unwrap()
            .package;
        let context = theme::context(
            &site.app,
            &settings,
            &live,
            None,
            Vec::new(),
            false,
            "home",
            None,
        )
        .await
        .unwrap();
        assert_eq!(context.media_dimensions.get(&public), Some(&(8, 8)));
        assert!(!context.media_dimensions.contains_key(&private));
        site.app.media_cache.lock().await.clear();
        let mut permits = Vec::new();
        for _ in 0..site.app.config.worker_concurrency {
            permits.push(site.app.media_work.try_acquire().unwrap());
        }
        let busy = theme::context(
            &site.app,
            &settings,
            &live,
            None,
            Vec::new(),
            false,
            "home",
            None,
        )
        .await
        .unwrap();
        assert!(busy.media.contains_key(&public));
        assert!(
            busy.media_dimensions.is_empty(),
            "optional layout metadata yields to a saturated worker pool"
        );
        drop(permits);
        let _guard = site.app.mutation().await.unwrap();
        sqlx::query("UPDATE media SET visibility='private' WHERE id=$1")
            .bind(&public)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        drop(_guard);
        let (_, changed) = get(&site.app, "/", None).await;
        assert!(!changed.contains(&format!("/media/{public}")));
        site.close().await;
    }
}

#[tokio::test]
async fn compiled_browser_security_controls_cover_cache_errors_and_secret_routes() {
    use wpalt::operations::headers::{Policy, Referrer};
    for pg in engines() {
        let mut site = Site::new(pg, true).await;
        let response = wpalt::web::router(site.app.clone())
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .header("x-forwarded-proto", "https")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(!response.headers().contains_key("strict-transport-security"));
        assert_eq!(
            response.headers()["cross-origin-opener-policy"],
            "same-origin"
        );
        let mut cfg = (*site.app.config).clone();
        cfg.base_url = "https://site.example.test".into();
        cfg.cache.enabled = true;
        cfg.headers.referrer = Referrer::SameOrigin;
        cfg.headers.hsts_seconds = 60;
        cfg.headers.hsts_include_subdomains = true;
        cfg.headers.isolate_opener = false;
        cfg.headers.upgrade_insecure_requests = true;
        cfg.validate().unwrap();
        site.app.security_headers = std::sync::Arc::new(Policy::compile(&cfg));
        site.app.config = std::sync::Arc::new(cfg);
        for expected in ["miss", "hit"] {
            let (status, headers, _) = request(&site.app, "GET", "/", None, "", Vec::new()).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(headers["x-wpalt-cache"], expected);
            assert_eq!(
                headers["strict-transport-security"],
                "max-age=60; includeSubDomains"
            );
            assert_eq!(headers["referrer-policy"], "same-origin");
            assert!(!headers.contains_key("cross-origin-opener-policy"));
            assert!(
                headers["content-security-policy"]
                    .to_str()
                    .unwrap()
                    .contains("upgrade-insecure-requests")
            );
            assert_eq!(headers["x-content-type-options"], "nosniff");
            assert!(!headers.contains_key("server") && !headers.contains_key("x-powered-by"));
        }
        let (status, headers, _) = request(
            &site.app,
            "POST",
            "/admin/operations/cache/purge",
            None,
            "application/x-www-form-urlencoded",
            b"csrf=forged".to_vec(),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(headers["cross-origin-resource-policy"], "same-origin");
        assert!(
            headers["content-security-policy"]
                .to_str()
                .unwrap()
                .contains("script-src 'self'")
        );
        let (_, headers, _) = request(
            &site.app,
            "GET",
            "/audience/confirm/not-a-valid-token",
            None,
            "",
            Vec::new(),
        )
        .await;
        assert_eq!(headers["referrer-policy"], "no-referrer");
        let (status, headers, _) = request(
            &site.app,
            "GET",
            "/admin/design/paper/preview",
            Some(&site.token),
            "",
            Vec::new(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let csp = headers["content-security-policy"].to_str().unwrap();
        assert!(
            csp.contains("script-src 'none'")
                && csp.contains("form-action 'none'")
                && csp.contains("frame-ancestors 'self'")
        );
        let mut invalid = (*site.app.config).clone();
        invalid.headers.hsts_seconds = 63072001;
        assert!(invalid.validate().is_err());
        site.close().await;
    }
}

#[tokio::test]
async fn optional_scripts_require_current_declared_consent_and_never_leak_cached_bytes() {
    use wpalt::operations::consent_scripts::{Config as ScriptConfig, Script, Scripts};
    async fn visitor(
        app: &App,
        method: &str,
        path: &str,
        cookie: &str,
        body: serde_json::Value,
        gpc: bool,
    ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("origin", app.config.origin())
            .header("cookie", cookie)
            .header("content-type", "application/json");
        if gpc {
            request = request.header("sec-gpc", "1");
        }
        let response = wpalt::web::router(app.clone())
            .oneshot(request.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        (
            status,
            headers,
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
    }
    for postgres in engines() {
        let mut site = Site::new(postgres, true).await;
        let file = site._directory.path().join("analytics.js");
        let code = b"window.localAnalyticsExample = true;";
        std::fs::write(&file, code).unwrap();
        let mut config = ScriptConfig {
            scripts: vec![Script {
                id: "local-example".into(),
                label: "Local example".into(),
                purpose: "Count locally consented interactions.".into(),
                path: file.clone(),
                sha256: auth::digest(code),
            }],
        };
        site.app.consent_scripts = std::sync::Arc::new(Scripts::compile(&config).unwrap());
        sqlx::query("UPDATE engagement_settings SET enabled=1 WHERE id=1")
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        let manifest = site.app.consent_scripts.manifest.clone();
        let url = format!("/api/engagement/scripts/{manifest}/local-example");
        assert_eq!(
            visitor(&site.app, "GET", &url, "", serde_json::json!({}), false)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        let stale = visitor(
            &site.app,
            "POST",
            "/api/engagement/consent",
            "",
            serde_json::json!({"allow":true,"policy":1,"manifest":"old"}),
            false,
        )
        .await;
        assert_eq!(stale.0, StatusCode::CONFLICT);
        let granted = visitor(
            &site.app,
            "POST",
            "/api/engagement/consent",
            "",
            serde_json::json!({"allow":true,"policy":1,"manifest":manifest}),
            false,
        )
        .await;
        assert_eq!(granted.0, StatusCode::OK);
        let cookie = granted.1["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        let served = visitor(&site.app, "GET", &url, cookie, serde_json::json!({}), false).await;
        assert_eq!(served.0, StatusCode::OK);
        assert_eq!(served.2, code);
        assert_eq!(served.1["cache-control"], "no-store");
        assert_eq!(
            visitor(&site.app, "GET", &url, cookie, serde_json::json!({}), true)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        // The same source cannot be read with another visitor's absent grant, even after a hit.
        assert_eq!(
            visitor(&site.app, "GET", &url, "", serde_json::json!({}), false)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        let archive = backup::capture(&site.app).await.unwrap();
        let mut recovered = Site::new(postgres, false).await;
        recovered.app.consent_scripts = std::sync::Arc::new(Scripts::compile(&config).unwrap());
        backup::restore(&recovered.app, &archive).await.unwrap();
        assert_eq!(
            visitor(
                &recovered.app,
                "GET",
                &url,
                cookie,
                serde_json::json!({}),
                false
            )
            .await
            .0,
            StatusCode::OK
        );
        // A purpose-only change requires a new explicit manifest acceptance after restart.
        config.scripts[0].purpose = "A newly declared local analytics purpose.".into();
        site.app.consent_scripts = std::sync::Arc::new(Scripts::compile(&config).unwrap());
        let new_url = format!(
            "/api/engagement/scripts/{}/local-example",
            site.app.consent_scripts.manifest
        );
        assert_eq!(
            visitor(&site.app, "GET", &url, cookie, serde_json::json!({}), false)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            visitor(
                &site.app,
                "GET",
                &new_url,
                cookie,
                serde_json::json!({}),
                false
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        let headers = axum::http::HeaderMap::from_iter([(
            axum::http::header::COOKIE,
            cookie.parse().unwrap(),
        )]);
        assert_eq!(
            wpalt::business::engagement::status(&site.app, &headers)
                .await
                .unwrap()["consented"],
            false
        );
        assert!(
            wpalt::business::promotions::visit(
                &site.app,
                &headers,
                serde_json::from_value(
                    serde_json::json!({"path":"/","device":"desktop","referrer":"direct"})
                )
                .unwrap()
            )
            .await
            .unwrap()
            .is_none()
        );
        let withdrew = visitor(
            &recovered.app,
            "POST",
            "/api/engagement/consent",
            cookie,
            serde_json::json!({"allow":false,"policy":1}),
            false,
        )
        .await;
        assert_eq!(withdrew.0, StatusCode::OK);
        assert_eq!(
            visitor(
                &recovered.app,
                "GET",
                &url,
                cookie,
                serde_json::json!({}),
                false
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        std::fs::write(&file, b"changed without updating reviewed checksum").unwrap();
        assert!(
            Scripts::compile(&config).is_err(),
            "Changed owner file fails closed at startup"
        );
        recovered.close().await;
        site.close().await;
    }
}

#[tokio::test]
async fn personal_data_requests_preserve_identity_isolation_decisions_and_fresh_recovery() {
    use serde_json::json;
    let mut engine_evidence = Vec::new();
    fn form(fields: &[(&str, &str)]) -> Vec<u8> {
        url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields.iter().copied())
            .finish()
            .into_bytes()
    }
    for postgres in engines() {
        let mut site = Site::new(postgres, true).await;
        auth::add_user(
            &site.app,
            "reader@example.test",
            "Private reader",
            "subscriber",
            PASSWORD,
        )
        .await
        .unwrap();
        let (reader, session) = auth::login(&site.app, "reader@example.test", PASSWORD)
            .await
            .unwrap();
        sqlx::query("INSERT INTO member_profiles(user_id,biography) VALUES($1,$2)")
            .bind(&session.user.id)
            .bind("MY_PRIVATE_BIOGRAPHY")
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        content::save(
            &site.app,
            site.session(),
            None,
            input("owner-private", "save"),
        )
        .await
        .unwrap();
        let proof = form(&[("csrf", &session.csrf), ("password", PASSWORD)]);
        let denied = request(
            &site.app,
            "POST",
            "/account/privacy/export",
            Some(&reader),
            "application/x-www-form-urlencoded",
            form(&[("csrf", &session.csrf), ("password", "incorrect")]),
        )
        .await;
        assert_eq!(denied.0, StatusCode::FORBIDDEN);
        let data = request(
            &site.app,
            "POST",
            "/account/privacy/export",
            Some(&reader),
            "application/x-www-form-urlencoded",
            proof.clone(),
        )
        .await;
        assert_eq!(data.0, StatusCode::OK);
        assert_eq!(data.1["cache-control"], "no-store");
        let exported: serde_json::Value = serde_json::from_slice(&data.2).unwrap();
        assert_eq!(
            exported["account_linked_records"]["users"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            exported["account_linked_records"]["member_profiles"][0]["biography"],
            "MY_PRIVATE_BIOGRAPHY"
        );
        assert!(
            exported["account_linked_records"]["posts"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let raw = String::from_utf8(data.2).unwrap();
        assert!(
            !raw.contains("owner@example.test")
                && !raw.contains("password_hash")
                && !raw.contains("$argon2")
                && !raw.contains(&reader)
        );
        let body = form(&[
            ("csrf", &session.csrf),
            ("password", PASSWORD),
            ("kind", "erase"),
        ]);
        let (a, b) = tokio::join!(
            request(
                &site.app,
                "POST",
                "/account/privacy",
                Some(&reader),
                "application/x-www-form-urlencoded",
                body.clone()
            ),
            request(
                &site.app,
                "POST",
                "/account/privacy",
                Some(&reader),
                "application/x-www-form-urlencoded",
                body
            )
        );
        assert_eq!(a.0, StatusCode::SEE_OTHER);
        assert_eq!(b.0, StatusCode::SEE_OTHER);
        let id: String = sqlx::query_scalar("SELECT id FROM privacy_requests WHERE user_id=$1")
            .bind(&session.user.id)
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM privacy_requests")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "Retry/race yields one open erasure request");
        assert_eq!(
            request(
                &site.app,
                "GET",
                &format!("/admin/privacy/{id}"),
                Some(&reader),
                "",
                vec![]
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        let decision = form(&[
            ("csrf", &site.session().csrf),
            ("version", "1"),
            ("state", "partial"),
            (
                "response",
                "Account profile reviewed; financial records retained for owner review. <script>never executable</script>",
            ),
        ]);
        assert_eq!(
            request(
                &site.app,
                "POST",
                &format!("/admin/privacy/{id}"),
                Some(&site.token),
                "application/x-www-form-urlencoded",
                decision.clone()
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
        assert_eq!(
            request(
                &site.app,
                "POST",
                &format!("/admin/privacy/{id}"),
                Some(&site.token),
                "application/x-www-form-urlencoded",
                decision
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        let history = request(
            &site.app,
            "GET",
            "/account/privacy",
            Some(&reader),
            "",
            vec![],
        )
        .await;
        let html = String::from_utf8(history.2).unwrap();
        assert!(html.contains("partial") && html.contains("&lt;script&gt;never executable"));
        let archive = backup::capture(&site.app).await.unwrap();
        let restored = Site::new(postgres, false).await;
        backup::restore(&restored.app, &archive).await.unwrap();
        let (restored_reader, restored_session) =
            auth::login(&restored.app, "reader@example.test", PASSWORD)
                .await
                .unwrap();
        let restored_data = request(
            &restored.app,
            "POST",
            "/account/privacy/export",
            Some(&restored_reader),
            "application/x-www-form-urlencoded",
            form(&[("csrf", &restored_session.csrf), ("password", PASSWORD)]),
        )
        .await;
        assert_eq!(restored_data.0, StatusCode::OK);
        let value: serde_json::Value = serde_json::from_slice(&restored_data.2).unwrap();
        assert_eq!(
            value["account_linked_records"]["privacy_requests"][0]["state"],
            "partial"
        );
        // Validate the foreign subject before any target records are written.
        let mut envelope: serde_json::Value = serde_json::from_slice(&archive).unwrap();
        let mut payload: serde_json::Value =
            serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
        payload["tables"]["privacy_requests"][0]["user_id"] =
            json!(uuid::Uuid::new_v4().to_string());
        let raw = payload.to_string();
        envelope["sha256"] = json!(auth::digest(raw.as_bytes()));
        envelope["payload"] = json!(raw);
        let empty = Site::new(postgres, false).await;
        assert!(
            backup::restore(&empty.app, &serde_json::to_vec(&envelope).unwrap())
                .await
                .is_err()
        );
        let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&empty.app.db.pool)
            .await
            .unwrap();
        assert_eq!(users, 0);
        // Fail explicitly instead of quietly returning a partial data archive.
        let mut config = (*site.app.config).clone();
        config.privacy.export_bytes = 64 * 1024;
        site.app.config = std::sync::Arc::new(config);
        sqlx::query("UPDATE member_profiles SET biography=$1 WHERE user_id=$2")
            .bind("PRIVATE_LONG_DATA".repeat(5000))
            .bind(&session.user.id)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            request(
                &site.app,
                "POST",
                "/account/privacy/export",
                Some(&reader),
                "application/x-www-form-urlencoded",
                proof
            )
            .await
            .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let new_request = form(&[
            ("csrf", &session.csrf),
            ("password", PASSWORD),
            ("kind", "erase"),
        ]);
        assert_eq!(
            request(
                &site.app,
                "POST",
                "/account/privacy",
                Some(&reader),
                "application/x-www-form-urlencoded",
                new_request
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
        let erase_id: String = sqlx::query_scalar(
            "SELECT id FROM privacy_requests WHERE user_id=$1 AND state='requested'",
        )
        .bind(&session.user.id)
        .fetch_one(&site.app.db.pool)
        .await
        .unwrap();
        let erase = form(&[
            ("csrf", &site.session().csrf),
            ("version", "1"),
            ("confirm", "true"),
            (
                "response",
                "Profile removed; independent backups require separate retention review.",
            ),
        ]);
        assert_eq!(
            request(
                &site.app,
                "POST",
                &format!("/admin/privacy/{erase_id}/erase-account"),
                Some(&site.token),
                "application/x-www-form-urlencoded",
                erase.clone()
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
        assert_eq!(
            request(
                &site.app,
                "POST",
                &format!("/admin/privacy/{erase_id}/erase-account"),
                Some(&site.token),
                "application/x-www-form-urlencoded",
                erase
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        assert!(
            auth::login(&site.app, "reader@example.test", PASSWORD)
                .await
                .is_err()
        );
        assert_eq!(
            request(
                &site.app,
                "GET",
                "/account/privacy",
                Some(&reader),
                "",
                vec![]
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM member_profiles WHERE user_id=$1")
                .bind(&session.user.id)
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
        let user = sqlx::query("SELECT email,name,role FROM users WHERE id=$1")
            .bind(&session.user.id)
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(user.get::<String, _>("role"), "disabled");
        assert!(
            !user
                .get::<String, _>("email")
                .contains("reader@example.test")
        );
        // Site ownership cannot disappear during an erasure workflow.
        let owner_request = form(&[
            ("csrf", &site.session().csrf),
            ("password", PASSWORD),
            ("kind", "erase"),
        ]);
        assert_eq!(
            request(
                &site.app,
                "POST",
                "/account/privacy",
                Some(&site.token),
                "application/x-www-form-urlencoded",
                owner_request
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
        let owner_id: String = sqlx::query_scalar(
            "SELECT id FROM privacy_requests WHERE user_id=$1 AND state='requested'",
        )
        .bind(&site.session().user.id)
        .fetch_one(&site.app.db.pool)
        .await
        .unwrap();
        assert_eq!(
            request(
                &site.app,
                "POST",
                &format!("/admin/privacy/{owner_id}/erase-account"),
                Some(&site.token),
                "application/x-www-form-urlencoded",
                form(&[
                    ("csrf", &site.session().csrf),
                    ("version", "1"),
                    ("confirm", "true"),
                    ("response", "Must preserve the last owner.")
                ])
            )
            .await
            .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert!(
            auth::login(&site.app, "owner@example.test", PASSWORD)
                .await
                .is_ok()
        );
        // A populated owner queue retains pending-first ordering and all later
        // records when the indexed page is selected before joining user names.
        auth::add_user(
            &site.app,
            "queue@example.test",
            "Queue fixture",
            "subscriber",
            PASSWORD,
        )
        .await
        .unwrap();
        let subjects: Vec<String> = sqlx::query_scalar("SELECT id FROM users ORDER BY id")
            .fetch_all(&site.app.db.pool)
            .await
            .unwrap();
        let mut expected = std::collections::HashSet::new();
        let mut tx = site.app.db.pool.begin().await.unwrap();
        for subject in subjects {
            for _ in 0..40 {
                let id = uuid::Uuid::new_v4().to_string();
                sqlx::query("INSERT INTO privacy_requests(id,user_id,kind,state,response,created_at,resolved_at) VALUES($1,$2,'access','fulfilled','Synthetic completed review',1,2)")
                    .bind(&id).bind(&subject).execute(&mut *tx).await.unwrap();
                expected.insert(id);
            }
        }
        tx.commit().await.unwrap();
        assert_eq!(expected.len(), 120);
        let mut seen = std::collections::HashSet::new();
        for page in 0..3 {
            let (status, _, body) = request(
                &site.app,
                "GET",
                &format!("/admin/privacy?page={page}"),
                Some(&site.token),
                "",
                Vec::new(),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            let html = String::from_utf8(body).unwrap();
            if page == 0 {
                assert!(
                    html.find(&owner_id).unwrap() < html.find("fulfilled").unwrap(),
                    "Pending cases must precede resolved history."
                );
                assert!(html.contains("More requests"));
            }
            for suffix in html.split("href=\"/admin/privacy/").skip(1) {
                let id = suffix.split('"').next().unwrap();
                if expected.contains(id) {
                    assert!(
                        seen.insert(id.to_owned()),
                        "A stable paginated queue must not repeat cases."
                    );
                }
            }
        }
        assert_eq!(
            seen, expected,
            "All completed cases must remain reachable beyond the first owner page."
        );
        let explain = if postgres {
            "EXPLAIN (ANALYZE,BUFFERS) "
        } else {
            "EXPLAIN QUERY PLAN "
        };
        let queue_sql = "SELECT p.id,p.kind,p.state,p.created_at,u.name FROM (SELECT id,user_id,kind,state,created_at FROM privacy_requests ORDER BY state DESC,created_at,id LIMIT 101 OFFSET 0) p JOIN users u ON u.id=p.user_id ORDER BY p.state DESC,p.created_at,p.id";
        let plan = sqlx::query(&format!("{explain}{queue_sql}"))
            .fetch_all(&site.app.db.pool)
            .await
            .unwrap()
            .iter()
            .map(|r| r.get::<String, _>(if postgres { 0 } else { 3 }))
            .collect::<Vec<_>>();
        engine_evidence.push(json!({"engine":if postgres {"PostgreSQL"} else {"SQLite"},"completed_fixture_records":120,"page_limit":101,"all_completed_records_reached_once":true,"queue_query_plan":plan}));
        empty.close().await;
        restored.close().await;
        site.close().await;
    }
    std::fs::create_dir_all("work").unwrap();
    std::fs::write(
        "work/m7-privacy-volume.json",
        serde_json::to_vec_pretty(&engine_evidence).unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn selective_recovery_preserves_link_dependencies_and_shared_domains_before_fresh_restore() {
    for postgres in engines() {
        let site = Site::new(postgres, true).await;
        let linked = content::save(
            &site.app,
            site.session(),
            None,
            input("linked-story", "publish"),
        )
        .await
        .unwrap();
        let unrelated = content::save(
            &site.app,
            site.session(),
            None,
            input("unrelated-story", "publish"),
        )
        .await
        .unwrap();
        let mut root = input("selected-story", "publish");
        root.body = "Read [the linked story](/linked-story?from=selection).".into();
        let root = content::save(&site.app, site.session(), None, root)
            .await
            .unwrap();
        let archive = backup::capture(&site.app).await.unwrap();
        let selected =
            backup::selection::prepare(&site.app.config, &archive, std::slice::from_ref(&root.id))
                .unwrap();
        let retained = selected.report["retained_posts"].as_array().unwrap();
        assert!(retained.iter().any(|id| id == &root.id));
        assert!(retained.iter().any(|id| id == &linked.id));
        assert!(!retained.iter().any(|id| id == &unrelated.id));
        let repeat =
            backup::selection::prepare(&site.app.config, &archive, std::slice::from_ref(&root.id))
                .unwrap();
        assert_eq!(
            selected.plan, repeat.plan,
            "The preview must bind a deterministic exact package."
        );
        assert_eq!(selected.bytes, repeat.bytes);
        let target = Site::new(postgres, false).await;
        backup::restore(&target.app, &selected.bytes).await.unwrap();
        assert_eq!(
            get(&target.app, "/selected-story", None).await.0,
            StatusCode::OK
        );
        assert_eq!(
            get(&target.app, "/linked-story", None).await.0,
            StatusCode::OK
        );
        assert_eq!(
            get(&target.app, "/unrelated-story", None).await.0,
            StatusCode::NOT_FOUND
        );
        assert!(
            auth::login(&target.app, "owner@example.test", PASSWORD)
                .await
                .is_ok()
        );
        assert!(
            backup::restore(&target.app, &selected.bytes).await.is_err(),
            "Selection cannot merge into an occupied target."
        );
        assert!(
            backup::selection::prepare(
                &site.app.config,
                &archive,
                &[uuid::Uuid::new_v4().to_string()]
            )
            .is_err()
        );
        let mut corrupt = archive.clone();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 1;
        assert!(backup::selection::prepare(&site.app.config, &corrupt, &[root.id]).is_err());
        target.close().await;
        site.close().await;
    }
}

#[tokio::test]
async fn url_aware_clone_stays_read_only_across_recovery_until_explicit_owner_review() {
    for postgres in engines() {
        let site = Site::new(postgres, true).await;
        let mut post = input("cloned-story", "publish");
        post.body = "[Local](https://source.example.test/about) [Unrelated](https://source.example.test.evil.invalid/about)".into();
        let post = content::save(&site.app, site.session(), None, post)
            .await
            .unwrap();
        let archive = backup::capture(&site.app).await.unwrap();
        let prepared = backup::selection::clone_package(
            &site.app.config,
            &archive,
            "https://source.example.test",
        )
        .unwrap();
        assert_eq!(prepared.report["held"], true);
        assert!(prepared.report["rewritten_occurrences"].as_u64().unwrap() > 0);
        assert!(
            backup::selection::clone_package(
                &site.app.config,
                &archive,
                "https://source.example.test/path"
            )
            .is_err()
        );
        let mut target = Site::new(postgres, false).await;
        backup::restore(&target.app, &prepared.bytes).await.unwrap();
        assert!(
            target
                .app
                .clone_held
                .load(std::sync::atomic::Ordering::SeqCst)
        );
        assert!(
            auth::session(
                &target.app,
                &axum::http::HeaderMap::from_iter([(
                    axum::http::header::COOKIE,
                    format!("wpalt_session={}", site.token).parse().unwrap()
                )])
            )
            .await
            .is_err()
        );
        let (token, session) = auth::login(&target.app, "owner@example.test", PASSWORD)
            .await
            .unwrap();
        let (status, html) = get(&target.app, "/cloned-story", None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("http://127.0.0.1:3000/about"));
        assert!(html.contains("https://source.example.test.evil.invalid/about"));
        let (_, operations) = get(&target.app, "/admin/operations", Some(&token)).await;
        assert!(operations.contains("Read-only recovered clone"));
        let mut update = input("cloned-story", "publish");
        update.version = post.version;
        update.csrf = session.csrf.clone();
        let body = serde_json::to_vec(&update).unwrap();
        assert_eq!(
            request(
                &target.app,
                "POST",
                &format!("/api/admin/content/{}", post.id),
                Some(&token),
                "application/json",
                body
            )
            .await
            .0,
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            request(
                &target.app,
                "POST",
                "/commerce/stripe/webhook",
                None,
                "application/json",
                b"{}".to_vec()
            )
            .await
            .0,
            StatusCode::SERVICE_UNAVAILABLE
        );
        let dispatched = std::sync::atomic::AtomicBool::new(false);
        assert!(
            wpalt::operations::jobs::run_cycle(&target.app, async {
                dispatched.store(true, std::sync::atomic::Ordering::SeqCst);
                vec![]
            })
            .await
            .is_err()
        );
        assert!(!dispatched.load(std::sync::atomic::Ordering::SeqCst));
        target.app.db.pool.close().await;
        target.app = App::open((*target.app.config).clone()).await.unwrap();
        assert!(
            target
                .app
                .clone_held
                .load(std::sync::atomic::Ordering::SeqCst),
            "Restart cannot remove a clone hold."
        );
        let held_archive = backup::capture(&target.app).await.unwrap();
        let recovered = Site::new(postgres, false).await;
        backup::restore(&recovered.app, &held_archive)
            .await
            .unwrap();
        assert!(
            wpalt::operations::clone_hold::held(&recovered.app)
                .await
                .unwrap()
        );
        assert!(
            wpalt::operations::clone_hold::activate(&target.app, "too short")
                .await
                .is_err()
        );
        wpalt::operations::clone_hold::activate(&target.app,"Reviewed source shutdown, message queues, payment ownership, identity callbacks and credentials in this isolated synthetic fixture.").await.unwrap();
        assert!(
            !target
                .app
                .clone_held
                .load(std::sync::atomic::Ordering::SeqCst)
        );
        assert!(
            !wpalt::operations::clone_hold::held(&target.app)
                .await
                .unwrap()
        );
        assert!(
            wpalt::operations::clone_hold::activate(
                &target.app,
                "Repeated activation must not silently pass an already activated clone."
            )
            .await
            .is_err()
        );
        recovered.close().await;
        target.close().await;
        site.close().await;
    }
}

#[tokio::test]
async fn wordpress_preview_package_recovers_content_without_inventing_private_access_or_payments() {
    use wpalt::platform::wordpress;
    let source = include_bytes!("fixtures/wordpress-core.xml");
    let assessment = wordpress::assess(source).unwrap();
    assert_eq!(assessment.report["source_items"], 4);
    assert_eq!(assessment.report["supported_core_items"], 2);
    assert!(
        assessment.report["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["code"] == "source_queue_not_replayed")
    );
    assert!(
        assessment.report["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["type"] == "shop_order")
    );
    // Namespace prefixes are aliases; namespace URIs determine interpretation.
    let aliased = String::from_utf8(source.to_vec())
        .unwrap()
        .replace("wp:", "export:")
        .replace("xmlns:wp=", "xmlns:export=");
    assert_eq!(
        wordpress::assess(aliased.as_bytes()).unwrap().report["supported_core_items"],
        2
    );
    for invalid in [
        String::from_utf8(source.to_vec()).unwrap().replace(
            "<channel>",
            "<!DOCTYPE channel [<!ENTITY remote SYSTEM 'file:///etc/passwd'>]><channel>",
        ),
        String::from_utf8(source.to_vec())
            .unwrap()
            .replace("<wp:post_id>13</wp:post_id>", "<wp:post_id>12</wp:post_id>"),
        String::from_utf8(source.to_vec()).unwrap().replace(
            "http://wordpress.org/export/1.2/",
            "https://attacker.invalid/export/",
        ),
        String::from_utf8(source.to_vec())
            .unwrap()
            .replace("garden &amp;", "garden &external;"),
        String::from_utf8(source.to_vec())
            .unwrap()
            .replace("</rss>", ""),
        format!("<rss>{}</rss>", "<x>".repeat(70)),
        String::from_utf8(source.to_vec()).unwrap().replace(
            "<wp:post_id>13</wp:post_id>",
            "<wp:post_id>012</wp:post_id>",
        ),
        format!(
            "<rss {}><channel/></rss>",
            (0..300)
                .map(|n| format!("xmlns:n{n}=\"urn:{n}\""))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        String::from_utf8(source.to_vec()).unwrap().replace(
            "<channel>",
            "<channel duplicated=\"one\" duplicated=\"two\">",
        ),
    ] {
        assert!(wordpress::assess(invalid.as_bytes()).is_err());
    }
    for postgres in engines() {
        let template = Site::new(postgres, true).await;
        let email = template.session.as_ref().unwrap().user.email.clone();
        let router = wpalt::web::router(template.app.clone());
        let anonymous = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/migration")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(anonymous.status(), StatusCode::OK);
        let multipart = |csrf: &str, action: &str, xml: &str| {
            format!(
                "--migration-boundary\r\nContent-Disposition: form-data; name=\"csrf\"\r\n\r\n{csrf}\r\n--migration-boundary\r\nContent-Disposition: form-data; name=\"action\"\r\n\r\n{action}\r\n--migration-boundary\r\nContent-Disposition: form-data; name=\"source\"; filename=\"export.xml\"\r\nContent-Type: application/xml\r\n\r\n{xml}\r\n--migration-boundary--\r\n"
            )
        };
        for (csrf, action, expected) in [
            ("wrong", "preview", StatusCode::FORBIDDEN),
            (template.session().csrf.as_str(), "preview", StatusCode::OK),
            (template.session().csrf.as_str(), "download", StatusCode::OK),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/admin/migration")
                        .header("cookie", format!("wpalt_session={}", template.token))
                        .header("origin", template.app.config.origin())
                        .header(
                            "content-type",
                            "multipart/form-data; boundary=migration-boundary",
                        )
                        .body(Body::from(multipart(
                            csrf,
                            action,
                            std::str::from_utf8(source).unwrap(),
                        )))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            if action == "download" && expected == StatusCode::OK {
                assert_eq!(response.headers()["cache-control"], "no-store");
                let report: serde_json::Value = serde_json::from_slice(
                    &response.into_body().collect().await.unwrap().to_bytes(),
                )
                .unwrap();
                assert_eq!(report["source_items"], 4);
            }
        }
        let prepared = wordpress::prepare(&template.app, source, &email)
            .await
            .unwrap();
        let repeated = wordpress::prepare(&template.app, source, &email)
            .await
            .unwrap();
        assert_eq!(prepared.plan, repeated.plan);
        assert_eq!(prepared.bytes, repeated.bytes);
        assert!(template.app.db.pool.size() > 0);
        let template_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM posts")
            .fetch_one(&template.app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            template_count, 0,
            "Preview/package creation must not mutate the template"
        );
        assert!(
            wordpress::prepare(&template.app, source, "unmapped@example.invalid")
                .await
                .is_err()
        );
        let media_root = tempfile::tempdir().unwrap();
        std::fs::create_dir(media_root.path().join("2025")).unwrap();
        std::fs::write(
            media_root.path().join("2025/garden.png"),
            include_bytes!("fixtures/animated.png"),
        )
        .unwrap();
        let prepared =
            wordpress::prepare_with_media(&template.app, source, &email, Some(media_root.path()))
                .await
                .unwrap();
        assert_eq!(prepared.report["media_mapped"], 1);
        let protected=String::from_utf8(source.to_vec()).unwrap().replace("<wp:post_id>12</wp:post_id>","<wp:post_id>12</wp:post_id><wp:post_password>source-secret-not-imported</wp:post_password>");
        let protected_package = wordpress::prepare_with_media(
            &template.app,
            protected.as_bytes(),
            &email,
            Some(media_root.path()),
        )
        .await
        .unwrap();
        assert!(
            protected_package.report["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w["code"] == "access_mapping_required")
        );
        let e: serde_json::Value = serde_json::from_slice(&protected_package.bytes).unwrap();
        let snapshot: serde_json::Value =
            serde_json::from_str(e["payload"].as_str().unwrap()).unwrap();
        assert!(
            snapshot["tables"]["posts"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p["status"] == "draft")
        );
        assert_eq!(snapshot["tables"]["media"][0]["visibility"], "private");
        // Selected ACF references are typed and recoverable, never an access grant.
        let acf_source = String::from_utf8(source.to_vec()).unwrap().replace(
            "<wp:comment>",
            &format!(
                "{}<wp:comment>",
                include_str!("fixtures/wordpress-acf-values.xml.fragment")
            ),
        );
        let mapping =
            wpalt::platform::acf::Mapping::parse(include_bytes!("fixtures/wordpress-acf-map.json"))
                .unwrap();
        let mapped = wordpress::prepare_with_mapping(
            &template.app,
            acf_source.as_bytes(),
            &email,
            None,
            Some(&mapping),
        )
        .await
        .unwrap();
        assert_ne!(mapped.plan, prepared.plan);
        let mapped_target = Site::new(postgres, false).await;
        backup::restore(&mapped_target.app, &mapped.bytes)
            .await
            .unwrap();
        let mapped_row =
            sqlx::query("SELECT fields,status,published_fields FROM posts WHERE slug='garden'")
                .fetch_one(&mapped_target.app.db.pool)
                .await
                .unwrap();
        let values: serde_json::Value =
            serde_json::from_str(&mapped_row.get::<String, _>("fields")).unwrap();
        assert_eq!(values["teaser"], "A field-owned garden story.");
        assert_eq!(values["reading_count"].as_u64(), Some(12));
        assert_eq!(values["show_marker"], false);
        assert_eq!(mapped_row.get::<String, _>("status"), "draft");
        assert_eq!(mapped_row.get::<String, _>("published_fields"), "{}");
        let registry = wpalt::schema::Registry::load(&mapped_target.app)
            .await
            .unwrap();
        assert_eq!(registry.common.fields["teaser"].kind, "string");
        for invalid_source in [
            acf_source.replace("field_garden_teaser", "field_wrong_reference"),
            acf_source.replace(
                "<wp:meta_value>12</wp:meta_value>",
                "<wp:meta_value>9007199254740993</wp:meta_value>",
            ),
        ] {
            assert!(
                wordpress::prepare_with_mapping(
                    &template.app,
                    invalid_source.as_bytes(),
                    &email,
                    None,
                    Some(&mapping)
                )
                .await
                .is_err()
            );
        }
        let mut invalid_map: serde_json::Value =
            serde_json::from_slice(include_bytes!("fixtures/wordpress-acf-map.json")).unwrap();
        invalid_map["fields"][1]["target_name"] = "teaser".into();
        assert!(
            wpalt::platform::acf::Mapping::parse(&serde_json::to_vec(&invalid_map).unwrap())
                .is_err()
        );
        // Large precise identifiers must be explicitly retained as strings.
        let big_source = acf_source.replace(
            "<wp:meta_value>12</wp:meta_value>",
            "<wp:meta_value>9007199254740993</wp:meta_value>",
        );
        let mut string_map: serde_json::Value =
            serde_json::from_slice(include_bytes!("fixtures/wordpress-acf-map.json")).unwrap();
        string_map["fields"][1]["kind"] = "string".into();
        let string_map =
            wpalt::platform::acf::Mapping::parse(&serde_json::to_vec(&string_map).unwrap())
                .unwrap();
        let exact = wordpress::prepare_with_mapping(
            &template.app,
            big_source.as_bytes(),
            &email,
            None,
            Some(&string_map),
        )
        .await
        .unwrap();
        let envelope: serde_json::Value = serde_json::from_slice(&exact.bytes).unwrap();
        let snapshot: serde_json::Value =
            serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
        let exact_values: serde_json::Value =
            serde_json::from_str(snapshot["tables"]["posts"][0]["fields"].as_str().unwrap())
                .unwrap();
        assert_eq!(exact_values["reading_count"], "9007199254740993");
        assert_ne!(exact.plan, mapped.plan);
        mapped_target.close().await;
        // The optional builder projection is draft-only and selection-bound.
        let elements = serde_json::json!([
            {"id":"projected1","elType":"widget","widgetType":"heading","settings":{"title":"A projected title","header_size":"h2"},"elements":[]},
            {"id":"projected2","elType":"widget","widgetType":"text-editor","settings":{"editor":"<p>Builder-owned content.</p>"},"elements":[]},
            {"id":"omitted1","elType":"widget","widgetType":"posts","settings":{},"elements":[]}]);
        let builder_source = String::from_utf8(source.to_vec()).unwrap().replace("<wp:comment>",
            &format!("<wp:postmeta><wp:meta_key>_elementor_data</wp:meta_key><wp:meta_value><![CDATA[{elements}]]></wp:meta_value></wp:postmeta><wp:comment>"));
        let projected = wordpress::prepare_with_adapters(
            &template.app,
            builder_source.as_bytes(),
            &email,
            None,
            wordpress::AdapterOptions {
                elementor_content: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let unselected = wordpress::prepare_with_mapping(
            &template.app,
            builder_source.as_bytes(),
            &email,
            None,
            None,
        )
        .await
        .unwrap();
        assert_ne!(projected.plan, unselected.plan);
        let report = projected.report["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["code"] == "elementor_content_projection")
            .unwrap();
        assert_eq!(report["report"]["elements"], 3);
        assert!(
            report["report"]["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w["element"] == "omitted1")
        );
        let builder_target = Site::new(postgres, false).await;
        backup::restore(&builder_target.app, &projected.bytes)
            .await
            .unwrap();
        let row = sqlx::query("SELECT body,status,published_body FROM posts WHERE slug='garden'")
            .fetch_one(&builder_target.app.db.pool)
            .await
            .unwrap();
        assert!(
            row.get::<String, _>("body")
                .contains("Builder-owned content.")
        );
        assert_eq!(row.get::<String, _>("status"), "draft");
        assert_eq!(row.get::<String, _>("published_body"), "");
        builder_target.close().await;
        // Separately exported free-plugin definitions preserve presentation order,
        // but cannot bring publication, notifications or consent into the target.
        let forms_source = br#"{"format":"wpalt-wpforms-source-v1","source_site":"https://garden.example","plugin_version":"2.0.2.1","forms":[{"source_id":"71","definition":{"id":"71","settings":{"form_title":"Contact","notifications":{"admin":{"email":"private@example.test"}}},"fields":{"9":{"id":"9","type":"email","label":"Your email","required":"1"},"2":{"id":"2","type":"textarea","label":"Message"}}}}]}"#;
        let form_package = wordpress::prepare_with_adapters(
            &template.app,
            source,
            &email,
            None,
            wordpress::AdapterOptions {
                wpforms_export: Some(forms_source),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(form_package.report["counts"]["forms"], 1);
        assert_ne!(form_package.plan, unselected.plan);
        assert!(
            !form_package
                .report
                .to_string()
                .contains("private@example.test")
        );
        let form_target = Site::new(postgres, false).await;
        backup::restore(&form_target.app, &form_package.bytes)
            .await
            .unwrap();
        let form =
            sqlx::query("SELECT draft,live,published_version,entry_count FROM business_forms")
                .fetch_one(&form_target.app.db.pool)
                .await
                .unwrap();
        let definition: wpalt::business::forms::FormDefinition =
            serde_json::from_str(&form.get::<String, _>("draft")).unwrap();
        assert_eq!(definition.fields[0].name, "wpforms_9");
        assert_eq!(definition.fields[1].name, "wpforms_2");
        assert!(definition.fields[0].schema.required);
        assert!(definition.notifications.is_empty());
        assert!(definition.subscription.is_none());
        assert!(definition.registration.is_none());
        assert_eq!(form.get::<String, _>("live"), "");
        assert_eq!(form.get::<i64, _>("published_version"), 0);
        assert_eq!(form.get::<i64, _>("entry_count"), 0);
        let usage: i64 = sqlx::query_scalar("SELECT items FROM business_usage WHERE kind='forms'")
            .fetch_one(&form_target.app.db.pool)
            .await
            .unwrap();
        assert_eq!(usage, 1);
        form_target.close().await;
        let unsupported = String::from_utf8(forms_source.to_vec())
            .unwrap()
            .replace("textarea", "payment");
        let unsupported = wordpress::prepare_with_adapters(
            &template.app,
            source,
            &email,
            None,
            wordpress::AdapterOptions {
                wpforms_export: Some(unsupported.as_bytes()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(unsupported.report["counts"]["forms"], 0);
        assert_eq!(
            unsupported.report["wpforms_mapping"]["forms"][0]["supported"],
            false
        );
        let foreign = String::from_utf8(forms_source.to_vec())
            .unwrap()
            .replace("garden.example", "foreign.example");
        assert!(
            wordpress::prepare_with_adapters(
                &template.app,
                source,
                &email,
                None,
                wordpress::AdapterOptions {
                    wpforms_export: Some(foreign.as_bytes()),
                    ..Default::default()
                }
            )
            .await
            .is_err()
        );

        let unsafe_path = String::from_utf8(source.to_vec()).unwrap().replace(
            "2025/garden.png</wp:meta_value>",
            "../outside.png</wp:meta_value>",
        );
        assert!(
            wordpress::prepare_with_media(
                &template.app,
                unsafe_path.as_bytes(),
                &email,
                Some(media_root.path())
            )
            .await
            .is_err()
        );
        #[cfg(unix)]
        {
            std::fs::remove_file(media_root.path().join("2025/garden.png")).unwrap();
            std::os::unix::fs::symlink(
                std::env::current_dir()
                    .unwrap()
                    .join("tests/fixtures/animated.png"),
                media_root.path().join("2025/garden.png"),
            )
            .unwrap();
            assert!(
                wordpress::prepare_with_media(
                    &template.app,
                    source,
                    &email,
                    Some(media_root.path())
                )
                .await
                .is_err()
            );
            std::fs::remove_file(media_root.path().join("2025/garden.png")).unwrap();
            std::fs::write(
                media_root.path().join("2025/garden.png"),
                include_bytes!("fixtures/animated.png"),
            )
            .unwrap();
        }
        let target = Site::new(postgres, false).await;
        backup::restore(&target.app, &prepared.bytes).await.unwrap();
        let story = wpalt::model::Post::from_row(
            sqlx::query("SELECT * FROM posts WHERE slug=$1")
                .bind("garden")
                .fetch_one(&target.app.db.pool)
                .await
                .unwrap(),
        );
        assert_eq!(story.title, "A garden & its people");
        assert_eq!(story.status, "published");
        let rendered = wpalt::web::router(target.app.clone())
            .oneshot(
                Request::builder()
                    .uri("/garden")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rendered.status(), StatusCode::OK);
        let html = String::from_utf8(
            rendered
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert_eq!(
            target.app.db.settings().await.unwrap().title,
            "A reference garden"
        );
        assert!(html.contains("<strong>quiet garden</strong>"));
        assert!(html.contains("<table>"));
        assert!(html.contains("Seasonal planting notes"));
        assert!(html.contains("First note"));
        assert!(html.contains("let example = 1 &lt; 2;"));
        assert!(!html.contains("never execute"));
        assert!(story.document.contains("/media/"));
        let media_id: String = sqlx::query_scalar("SELECT id FROM media")
            .fetch_one(&target.app.db.pool)
            .await
            .unwrap();
        let media_response = wpalt::web::router(target.app.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/media/{media_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(media_response.status(), StatusCode::OK);
        assert_eq!(
            media_response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .as_ref(),
            include_bytes!("fixtures/animated.png")
        );
        assert!(story.body.contains("quiet garden"));
        assert!(!story.document.contains("never execute"));
        assert!(story.seo.contains("Independent garden stories."));
        let private =
            sqlx::query("SELECT id FROM posts WHERE published_slug=$1 AND status='published'")
                .bind("private-page")
                .fetch_optional(&target.app.db.pool)
                .await
                .unwrap();
        assert!(
            private.is_none(),
            "Source private page must not become public without access mapping"
        );
        let counts=sqlx::query("SELECT (SELECT COUNT(*) FROM comments) AS comments,(SELECT COUNT(*) FROM terms) AS terms,(SELECT COUNT(*) FROM shop_orders) AS orders").fetch_one(&target.app.db.pool).await.unwrap();
        assert_eq!(counts.get::<i64, _>("comments"), 1);
        assert_eq!(counts.get::<i64, _>("terms"), 2);
        assert_eq!(counts.get::<i64, _>("orders"), 0);
        let response = wpalt::web::router(target.app.clone())
            .oneshot(
                Request::builder()
                    .uri("/2025/04/garden/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::MOVED_PERMANENTLY);
        assert_eq!(response.headers()["location"], "/garden");
        assert!(
            backup::restore(&target.app, &prepared.bytes).await.is_err(),
            "A retry cannot overwrite an occupied site"
        );
        template.close().await;
        target.close().await;
    }
}

/// External code can read selected content and propose drafts, without acquiring
/// publication or browser privileges; revocation and recovery close authority.
#[tokio::test]
async fn scoped_integration_drafts_revoke_without_publication_or_recovery_authority() {
    use std::future::Future;
    use wpalt::platform::integrations as api;
    for postgres in engines() {
        let site = Site::new(postgres, true).await;
        let owner = site.session();
        let reader = api::issue(
            &site.app,
            owner,
            &owner.user.email,
            "Read-only exporter",
            false,
            7,
        )
        .await
        .unwrap();
        let writer = api::issue(
            &site.app,
            owner,
            &owner.user.email,
            "Independent draft worker",
            true,
            7,
        )
        .await
        .unwrap();
        let headers = |token: &str| {
            let mut headers = axum::http::HeaderMap::new();
            headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
            headers
        };
        let actor = api::authenticate(&site.app, &headers(&writer.token), true)
            .await
            .unwrap();
        assert!(
            api::issue(&site.app, &actor, &owner.user.email, "Escalation", true, 7)
                .await
                .is_err()
        );
        assert!(
            api::authenticate(&site.app, &headers(&reader.token), true)
                .await
                .is_err()
        );
        let router = wpalt::web::router(site.app.clone());
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/content")
                    .header("authorization", format!("Bearer {}", reader.token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert!(response.headers().get("set-cookie").is_none());
        for (name, value) in [
            ("cookie", format!("wpalt_session={}", site.token)),
            ("origin", "https://untrusted.example".into()),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/api/v1/content")
                        .header("authorization", format!("Bearer {}", reader.token))
                        .header(name, value)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        let mut draft = input("external-draft", "save");
        draft.body = "An independent worker suggestion.".into();
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/content")
                    .header("authorization", format!("Bearer {}", reader.token))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&draft).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/content")
                    .header("authorization", format!("Bearer {}", writer.token))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&draft).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let created: serde_json::Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        let id = created["id"].as_str().unwrap();
        assert_eq!(created["status"], "draft");
        assert_eq!(
            wpalt::content::get(&site.app, id)
                .await
                .unwrap()
                .published_body,
            ""
        );
        draft.action = "publish".into();
        draft.version = 1;
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/v1/content/{id}"))
                    .header("authorization", format!("Bearer {}", writer.token))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&draft).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(
            content::save(&site.app, &actor, Some(id), draft.clone())
                .await
                .is_err()
        );
        draft.action = "save".into();
        let saved = content::save(&site.app, &actor, Some(id), draft.clone())
            .await
            .unwrap();
        assert_eq!(saved.version, 2);
        assert!(
            content::save(&site.app, &actor, Some(id), draft.clone())
                .await
                .is_err()
        );
        // A scheduled working copy must not become an indirect publication path.
        let mut scheduled_input = input("scheduled-integration-boundary", "schedule");
        scheduled_input.publish_at = wpalt::now() + 3600;
        let scheduled = content::save(&site.app, owner, None, scheduled_input.clone())
            .await
            .unwrap();
        scheduled_input.action = "save".into();
        scheduled_input.publish_at = 0;
        scheduled_input.version = scheduled.version;
        assert!(
            content::save(&site.app, &actor, Some(&scheduled.id), scheduled_input)
                .await
                .is_err()
        );
        // Coordinate queue order without sleeps: revocation owns the next turn
        // before an already-authenticated writer acquires the same mutex.
        draft.version = 2;
        let guard = site.app.mutation().await.unwrap();
        let mut revocation = Box::pin(api::revoke(&site.app, owner, &writer.id));
        assert!(matches!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(revocation.as_mut().poll(cx))).await,
            std::task::Poll::Pending
        ));
        let mut waiting_write = Box::pin(content::save(&site.app, &actor, Some(id), draft));
        assert!(matches!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(waiting_write.as_mut().poll(cx)))
                .await,
            std::task::Poll::Pending
        ));
        drop(guard);
        revocation.await.unwrap();
        assert!(
            waiting_write.await.is_err(),
            "A queued writer cannot pass a winning revocation"
        );
        assert!(
            api::authenticate(&site.app, &headers(&writer.token), false)
                .await
                .is_err()
        );
        // Credentials never become browser cookies or portable recovery grants.
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/posts")
                    .header("cookie", format!("wpalt_session={}", reader.token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let archive = backup::capture(&site.app).await.unwrap();
        assert!(
            !String::from_utf8(archive.clone())
                .unwrap()
                .contains(&auth::digest(reader.token.as_bytes()))
        );
        let target = Site::new(postgres, false).await;
        backup::restore(&target.app, &archive).await.unwrap();
        assert!(
            api::authenticate(&target.app, &headers(&reader.token), false)
                .await
                .is_err()
        );
        target.close().await;
        let mut different_origin = site.app.clone();
        let mut config = (*different_origin.config).clone();
        config.base_url = "https://recovered.example.test".into();
        different_origin.config = std::sync::Arc::new(config);
        assert!(
            api::authenticate(&different_origin, &headers(&reader.token), false)
                .await
                .is_err()
        );
        let expiring = api::issue(
            &site.app,
            owner,
            &owner.user.email,
            "Expiry fixture",
            false,
            1,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE integration_credentials SET expires_at=0 WHERE id=$1")
            .bind(&expiring.id)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        assert!(
            api::authenticate(&site.app, &headers(&expiring.token), false)
                .await
                .is_err()
        );
        // A changed account credential invalidates its delegated access immediately.
        sqlx::query("UPDATE users SET password_hash=$1 WHERE id=$2")
            .bind("changed-credential-fingerprint")
            .bind(&owner.user.id)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        assert!(
            api::authenticate(&site.app, &headers(&reader.token), false)
                .await
                .is_err()
        );
        site.close().await;
    }
}

#[tokio::test]
async fn integration_events_commit_with_content_and_report_bounded_replay_gaps() {
    for postgres in engines() {
        let mut site = Site::new(postgres, true).await;
        std::sync::Arc::make_mut(&mut site.app.config)
            .integration_events
            .retained_events = 32;
        let owner = site.session().clone();
        let grant = wpalt::platform::integrations::issue(
            &site.app,
            &owner,
            &owner.user.email,
            "Event consumer",
            false,
            7,
        )
        .await
        .unwrap();
        let router = wpalt::web::router(site.app.clone());
        async fn feed(router: axum::Router, token: &str, after: &str) -> axum::response::Response {
            router
                .oneshot(
                    Request::builder()
                        .uri(format!("/api/v1/events?after={after}"))
                        .header("authorization", format!("Bearer {token}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
        }
        let response = feed(router.clone(), &grant.token, "").await;
        assert_eq!(response.status(), StatusCode::OK);
        let initial: serde_json::Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(initial["events"], serde_json::json!([]));
        let mut draft = input("event-draft", "save");
        draft.body = "PRIVATE_EVENT_BODY".into();
        let post = content::save(&site.app, &owner, None, draft.clone())
            .await
            .unwrap();
        let response = feed(
            router.clone(),
            &grant.token,
            initial["next"].as_str().unwrap(),
        )
        .await;
        let page: serde_json::Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(page["events"].as_array().unwrap().len(), 1);
        assert_eq!(page["events"][0]["content_id"], post.id);
        assert_eq!(page["events"][0]["version"], 1);
        assert!(!page.to_string().contains("PRIVATE_EVENT_BODY"));
        // A physical rollback can reuse a sequence. Its new random event anchor
        // must invalidate the old cursor instead of skipping unrelated changes.
        let original = page["events"][0].to_string();
        let mut branch = page["events"][0].clone();
        branch["id"] = format!(
            "{}:1:{}",
            initial["epoch"].as_str().unwrap(),
            uuid::Uuid::new_v4()
        )
        .into();
        sqlx::query("UPDATE integration_events SET payload=$1 WHERE sequence=1")
            .bind(branch.to_string())
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            feed(router.clone(), &grant.token, page["next"].as_str().unwrap())
                .await
                .status(),
            StatusCode::CONFLICT
        );
        sqlx::query("UPDATE integration_events SET payload=$1 WHERE sequence=1")
            .bind(original)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        draft.version = 0;
        assert!(
            content::save(&site.app, &owner, Some(&post.id), draft.clone())
                .await
                .is_err()
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM integration_events")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
        // Fault after the domain writes: the entire transaction must roll back,
        // including revisions and content, when journal persistence fails.
        sqlx::query("DELETE FROM integration_event_state")
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        let mut failed = input("journal-failure", "save");
        failed.body = "Not committed".into();
        assert!(
            content::save(&site.app, &owner, None, failed)
                .await
                .is_err()
        );
        let missing: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM posts WHERE slug='journal-failure'")
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        assert_eq!(missing, 0);
        sqlx::query("INSERT INTO integration_event_state(id,epoch,sequence) VALUES(1,$1,1)")
            .bind(initial["epoch"].as_str().unwrap())
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        for version in 1..=34 {
            draft.version = version;
            content::save(&site.app, &owner, Some(&post.id), draft.clone())
                .await
                .unwrap();
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM integration_events")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(count, 32);
        assert_eq!(
            feed(router.clone(), &grant.token, page["next"].as_str().unwrap())
                .await
                .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            feed(router.clone(), &grant.token, "foreign:1:start")
                .await
                .status(),
            StatusCode::CONFLICT
        );
        let response = feed(router.clone(), &grant.token, "").await;
        let page: serde_json::Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(page["events"].as_array().unwrap().len(), 25);
        assert_eq!(page["has_more"], true);
        wpalt::platform::integrations::revoke(&site.app, &owner, &grant.id)
            .await
            .unwrap();
        assert_eq!(
            feed(router, &grant.token, page["next"].as_str().unwrap())
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
        site.close().await;
    }
}

#[tokio::test]
async fn selected_plugin_clusters_recover_definitions_without_consent_access_or_settlement() {
    use wpalt::{backup, platform::wordpress};
    for postgres in engines() {
        let template = Site::new(postgres, true).await;
        let email: String = sqlx::query_scalar("SELECT email FROM users")
            .fetch_one(&template.app.db.pool)
            .await
            .unwrap();
        let source = include_bytes!("fixtures/wordpress-core.xml");
        let cluster = include_bytes!("fixtures/wordpress-clusters.json");
        let package = wordpress::prepare_with_adapters(
            &template.app,
            source,
            &email,
            None,
            wordpress::AdapterOptions {
                cluster_export: Some(cluster),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(package.report["counts"]["posts"], 4);
        assert_eq!(package.report["counts"]["comments"], 0);
        assert_eq!(package.report["counts"]["redirects"], 0);
        assert_eq!(
            package.report["cluster_mapping"]["counts"],
            serde_json::json!({"contacts":1,"membership_policies":1,"courses":1,"lessons":2,"products":1})
        );
        assert_eq!(
            package.report["cluster_mapping"]["unsupported"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(
            !package
                .report
                .to_string()
                .contains("quarantined@example.test")
        );
        let mut target = Site::new(postgres, false).await;
        backup::restore(&target.app, &package.bytes).await.unwrap();
        for table in [
            "audience_memberships",
            "audience_consent_events",
            "mail_jobs",
            "member_grants",
            "member_progress",
            "shop_orders",
            "shop_payments",
        ] {
            let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&target.app.db.pool)
                .await
                .unwrap();
            assert_eq!(count, 0, "No inferred operational authority: {table}");
        }
        let suppressed: i64 = sqlx::query_scalar("SELECT suppressed FROM audience_contacts")
            .fetch_one(&target.app.db.pool)
            .await
            .unwrap();
        assert_eq!(suppressed, 1);
        let policies: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM member_policies WHERE enabled=0")
                .fetch_one(&target.app.db.pool)
                .await
                .unwrap();
        assert_eq!(policies, 2);
        let course: String = sqlx::query_scalar("SELECT draft FROM member_courses")
            .fetch_one(&target.app.db.pool)
            .await
            .unwrap();
        let course: serde_json::Value = serde_json::from_str(&course).unwrap();
        assert_eq!(course["lessons"][0]["title"], "First lesson");
        assert_eq!(course["lessons"][1]["title"], "Next lesson");
        let post_id = course["lessons"][0]["post_id"].as_str().unwrap();
        assert!(
            !wpalt::membership::allowed(&target.app, "post", post_id, None, 0)
                .await
                .unwrap()
        );
        let migrated_export = backup::capture(&target.app).await.unwrap();
        // Publishing an individual imported lesson still cannot bypass its disabled resource policy.
        sqlx::query("UPDATE posts SET status='published',published_slug=slug,published_title=title,published_body=body,published_document=document WHERE id=$1").bind(post_id).execute(&target.app.db.pool).await.unwrap();
        let response = wpalt::web::router(target.app.clone())
            .oneshot(
                Request::builder()
                    .uri("/sensei-lesson-4")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(response.status(), StatusCode::OK);
        let (minor, active): (i64, i64) =
            sqlx::query_as("SELECT price_minor,active FROM shop_variants")
                .fetch_one(&target.app.db.pool)
                .await
                .unwrap();
        assert_eq!((minor, active), (1234, 0));
        // Recover the untouched package again into another independent empty instance.
        let recovered = Site::new(postgres, false).await;
        backup::restore(&recovered.app, &migrated_export)
            .await
            .unwrap();
        let contacts: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM audience_contacts WHERE suppressed=1")
                .fetch_one(&recovered.app.db.pool)
                .await
                .unwrap();
        assert_eq!(contacts, 1);
        let live: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM posts WHERE status='published'")
            .fetch_one(&recovered.app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            live, 0,
            "Unknown PMPro table restrictions cannot expose WXR core content"
        );
        recovered.close().await;
        for alter in [
            "foreign-origin",
            "precision",
            "duplicate-email",
            "unknown-field",
        ] {
            let mut value: serde_json::Value = serde_json::from_slice(cluster).unwrap();
            match alter {
                "foreign-origin" => value["source_site"] = "https://foreign.example".into(),
                "precision" => {
                    value["woocommerce"]["products"][0]["regular_price"] = "12.345".into()
                }
                "duplicate-email" => {
                    let mut duplicate = value["mailpoet"]["subscribers"][0].clone();
                    duplicate["id"] = "9".into();
                    value["mailpoet"]["subscribers"]
                        .as_array_mut()
                        .unwrap()
                        .push(duplicate);
                }
                _ => value["pmpro"]["active_grants"] = serde_json::json!(["untrusted"]),
            }
            let raw = serde_json::to_vec(&value).unwrap();
            assert!(
                wordpress::prepare_with_adapters(
                    &template.app,
                    source,
                    &email,
                    None,
                    wordpress::AdapterOptions {
                        cluster_export: Some(&raw),
                        ..Default::default()
                    }
                )
                .await
                .is_err(),
                "Reject {alter}"
            );
        }
        let unchanged: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM posts")
            .fetch_one(&template.app.db.pool)
            .await
            .unwrap();
        assert_eq!(unchanged, 0);
        // Owner review connects the imported catalog to ordinary native checkout.
        let (token, session) = auth::login(&target.app, &email, PASSWORD).await.unwrap();
        target.token = token;
        target.session = Some(session);
        use wpalt::commerce::catalog::{self, ProductInput, VariantInput};
        let (product_id, variant_id): (String, String) = sqlx::query_as(
            "SELECT p.id,v.id FROM shop_products p JOIN shop_variants v ON v.product_id=p.id",
        )
        .fetch_one(&target.app.db.pool)
        .await
        .unwrap();
        catalog::save_product(
            &target.app,
            target.session(),
            Some(&product_id),
            1,
            &ProductInput {
                slug: "garden-kit".into(),
                title: "Garden kit".into(),
                description: "A physical kit.".into(),
                kind: "physical".into(),
                entitlement: "".into(),
                access_seconds: 0,
                download_id: "".into(),
                published: true,
            },
        )
        .await
        .unwrap();
        catalog::save_variant(
            &target.app,
            target.session(),
            &product_id,
            Some(&variant_id),
            1,
            &VariantInput {
                title: "Garden kit".into(),
                sku: "KIT-6".into(),
                price_minor: 1234,
                member_price_minor: -1,
                member_key: "".into(),
                stock_total: 4,
                billing_interval: "".into(),
                active: true,
            },
        )
        .await
        .unwrap();
        let (_, buyer) = commerce_journeys::shopper(&target, "new-buyer@example.test").await;
        let checkout = commerce_journeys::cart(&target, &buyer, &variant_id, "", 1).await;
        let order = wpalt::commerce::orders::checkout(&target.app, &buyer, &checkout)
            .await
            .unwrap();
        let payments: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM shop_payments")
            .fetch_one(&target.app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            payments, 0,
            "Native checkout has no inferred source settlement"
        );
        commerce_journeys::pay(&target, &order, "explicit-owner-receipt-after-import").await;
        let export = backup::capture(&target.app).await.unwrap();
        let business_recovery = Site::new(postgres, false).await;
        backup::restore(&business_recovery.app, &export)
            .await
            .unwrap();
        let state: String = sqlx::query_scalar("SELECT payment_state FROM shop_orders WHERE id=$1")
            .bind(&order)
            .fetch_one(&business_recovery.app.db.pool)
            .await
            .unwrap();
        assert_eq!(state, "paid");
        let sold: i64 = sqlx::query_scalar("SELECT sold FROM shop_variants WHERE id=$1")
            .bind(&variant_id)
            .fetch_one(&business_recovery.app.db.pool)
            .await
            .unwrap();
        assert_eq!(sold, 1);
        business_recovery.close().await;
        target.close().await;
        template.close().await;
    }
}

/// Editorial graph uses published public snapshots, not private drafts or rankings.
#[tokio::test]
async fn content_audit_connects_public_links_and_reports_language_sensitive_advice() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let session = site.session.as_ref().unwrap();
        let target = content::save(&site.app, session, None, input("audit-target", "publish"))
            .await
            .unwrap();
        let orphan = content::save(&site.app, session, None, input("audit-orphan", "publish"))
            .await
            .unwrap();
        let draft = content::save(&site.app, session, None, input("audit-draft", "save"))
            .await
            .unwrap();
        let mut source_input = input("audit-source", "publish");
        source_input.body = "Garden notes. Useful information! [Read on](/audit-target?from=notes#detail) [Same target](/audit-target) [External](https://example.org/audit-orphan) [Self](#here)".into();
        let source = content::save(&site.app, session, None, source_input)
            .await
            .unwrap();
        // An unpublished edit must not invent a public link or alter editorial counts.
        let mut edit = input("audit-source", "save");
        edit.version = source.version;
        edit.body = "Unpublished secret [draft link](/audit-orphan)".into();
        content::save(&site.app, session, Some(&source.id), edit)
            .await
            .unwrap();
        let report = wpalt::platform::content_audit::report(&site.app, "garden")
            .await
            .unwrap();
        assert_eq!(report["items"].as_array().unwrap().len(), 3);
        assert_eq!(report["edges"], serde_json::json!([[source.id, target.id]]));
        let orphans = report["without_inbound_content_links"].as_array().unwrap();
        assert!(orphans.contains(&serde_json::json!(orphan.id)));
        assert!(!orphans.contains(&serde_json::json!(target.id)));
        assert!(!report.to_string().contains("Unpublished secret"));
        assert!(!report.to_string().contains(&draft.id));
        let source_advice = report["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == source.id)
            .unwrap();
        assert_eq!(source_advice["keyword_occurrences"], 1);
        assert!(source_advice["sentences_approximate"].as_u64().unwrap() >= 2);
        assert!(
            wpalt::platform::content_audit::report(&site.app, &"x".repeat(101))
                .await
                .is_err()
        );
        // Huge sites are refused before body loading, never reported as complete.
        sqlx::query("UPDATE posts SET published_body=$1 WHERE id=$2")
            .bind("x".repeat(8 * 1024 * 1024 + 1))
            .bind(&orphan.id)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        assert!(
            wpalt::platform::content_audit::report(&site.app, "")
                .await
                .is_err()
        );
        site.close().await;
    }
}

/// Translation operations preserve local text/publication and reject stale reviewed plans.
#[tokio::test]
async fn translation_duplication_and_selected_sync_preserve_reviewed_language_authority() {
    use wpalt::platform::translations::{self, Request};
    for pg in engines() {
        let site = Site::new(pg, true).await;
        multilingual(&site).await;
        let mut original = input("translation-source", "publish");
        original.translation_group = "translation-family".into();
        let source = content::save(&site.app, site.session(), None, original.clone())
            .await
            .unwrap();
        let preview = translations::prepare(
            &site.app,
            Request {
                source: &source.id,
                locale: "fr",
                slug: "translation-fr",
                target: None,
                fields: &[],
                execute: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(preview["executed"], false);
        assert_eq!(
            get(&site.app, "/fr/translation-fr", None).await.0,
            StatusCode::NOT_FOUND
        );
        assert!(
            translations::prepare(
                &site.app,
                Request {
                    source: &source.id,
                    locale: "fr",
                    slug: "translation-fr",
                    target: None,
                    fields: &[],
                    execute: Some("stale")
                }
            )
            .await
            .is_err()
        );
        let created = translations::prepare(
            &site.app,
            Request {
                source: &source.id,
                locale: "fr",
                slug: "translation-fr",
                target: None,
                fields: &[],
                execute: preview["plan"].as_str(),
            },
        )
        .await
        .unwrap();
        let target_id = created["result"]["id"].as_str().unwrap();
        let draft = content::get(&site.app, target_id).await.unwrap();
        assert_eq!(draft.status, "draft");
        assert_eq!(draft.translation_group, source.translation_group);
        assert_eq!(draft.document, source.document);
        assert_eq!(draft.seo, "{}");
        // Publish the owner's actual translation; later sync must preserve that public snapshot.
        let mut french = input("translation-fr", "publish");
        french.locale = "fr".into();
        french.translation_group = source.translation_group.clone();
        french.version = draft.version;
        french.body = "Un jardin calme.".into();
        french.seo = r#"{"title":"Jardin français"}"#.into();
        let french = content::save(&site.app, site.session(), Some(target_id), french)
            .await
            .unwrap();
        original.version = source.version;
        original.action = "save".into();
        original.fields = r#"{"subtitle":"Updated source metadata","featured":false}"#.into();
        let source = content::save(
            &site.app,
            site.session(),
            Some(&source.id),
            original.clone(),
        )
        .await
        .unwrap();
        let fields = vec!["featured".into()];
        let preview = translations::prepare(
            &site.app,
            Request {
                source: &source.id,
                locale: "fr",
                slug: "translation-fr",
                target: Some(target_id),
                fields: &fields,
                execute: None,
            },
        )
        .await
        .unwrap();
        // A source edit invalidates even a plan selecting an otherwise unchanged field.
        original.version = source.version;
        original.body = "Changed source draft".into();
        let source = content::save(&site.app, site.session(), Some(&source.id), original)
            .await
            .unwrap();
        assert!(
            translations::prepare(
                &site.app,
                Request {
                    source: &source.id,
                    locale: "fr",
                    slug: "translation-fr",
                    target: Some(target_id),
                    fields: &fields,
                    execute: preview["plan"].as_str()
                }
            )
            .await
            .is_err()
        );
        let preview = translations::prepare(
            &site.app,
            Request {
                source: &source.id,
                locale: "fr",
                slug: "translation-fr",
                target: Some(target_id),
                fields: &fields,
                execute: None,
            },
        )
        .await
        .unwrap();
        translations::prepare(
            &site.app,
            Request {
                source: &source.id,
                locale: "fr",
                slug: "translation-fr",
                target: Some(target_id),
                fields: &fields,
                execute: preview["plan"].as_str(),
            },
        )
        .await
        .unwrap();
        let after = content::get(&site.app, target_id).await.unwrap();
        assert_eq!(after.document, french.document);
        assert_eq!(after.seo, french.seo);
        assert_eq!(after.published_body, french.published_body);
        assert_eq!(after.published_fields, french.published_fields);
        assert_eq!(after.status, "published");
        let fields: serde_json::Value = serde_json::from_str(&after.fields).unwrap();
        assert_eq!(fields["featured"], false);
        assert_eq!(fields["subtitle"], "From our garden");
        assert_eq!(
            get(&site.app, "/fr/translation-fr", None).await.0,
            StatusCode::OK
        );
        sqlx::query("INSERT INTO member_policies(id,title) VALUES('translation-protected','Private translation')").execute(&site.app.db.pool).await.unwrap();
        sqlx::query("INSERT INTO member_resources(kind,resource_id,policy_id) VALUES('post',$1,'translation-protected')").bind(&source.id).execute(&site.app.db.pool).await.unwrap();
        assert!(
            translations::prepare(
                &site.app,
                Request {
                    source: &source.id,
                    locale: "ar",
                    slug: "translation-ar",
                    target: None,
                    fields: &[],
                    execute: None
                }
            )
            .await
            .is_err(),
            "Duplication must not silently lose private-source access rules"
        );
        site.close().await;
    }
}
