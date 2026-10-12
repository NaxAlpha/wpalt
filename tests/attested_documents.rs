//! A cryptographic guarantee is checked by an independent implementation as well
//! as the native verifier; it does not establish identity or legal certification.
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;
use wpalt::business::documents::{self, Authority, Config};

#[test]
fn exact_document_bytes_require_independent_key_trust_and_survive_key_rotation() {
    let directory = tempfile::tempdir().unwrap();
    let key = directory.path().join("signing.pk8");
    let trust = documents::create_key(&key).unwrap();
    assert!(
        documents::create_key(&key).is_err(),
        "Existing private keys are never overwritten"
    );
    let config = Config {
        enabled: true,
        key_file: key.clone(),
        ..Default::default()
    };
    let signer = Authority::load(&config).unwrap();
    assert_eq!(signer.identity().as_deref(), Some(trust.as_str()));
    let original = "<html><body>Accepted: Zoë · 日本語 · مرحبا</body></html>".as_bytes();
    let source = json!({"statement":"An original frozen acknowledgment","accepted":true});
    let bundle = signer
        .attest(source.clone(), "text/html", original)
        .unwrap();
    let (manifest, artifact) = documents::verify(&bundle, &trust).unwrap();
    assert_eq!(manifest.source, source);
    assert_eq!(artifact, original);
    let other = documents::create_key(&directory.path().join("rotated.pk8")).unwrap();
    assert!(
        documents::verify(&bundle, &other).is_err(),
        "A self-supplied key cannot replace independently configured trust"
    );
    assert!(
        documents::verify(&bundle, &trust).is_ok(),
        "Rotation does not invalidate retained original public-key trust"
    );
    let raw = STANDARD.decode(&bundle.manifest).unwrap();
    let message = [
        b"wpalt-response-document-attestation-v1\0".as_slice(),
        raw.as_slice(),
    ]
    .concat();
    // RFC 8410 SubjectPublicKeyInfo: Ed25519 algorithm OID and 32-byte public key.
    let mut public = hex::decode("302a300506032b6570032100").unwrap();
    public.extend(hex::decode(&bundle.public_key).unwrap());
    std::fs::write(directory.path().join("public.der"), public).unwrap();
    std::fs::write(directory.path().join("message"), message).unwrap();
    std::fs::write(
        directory.path().join("signature"),
        hex::decode(&bundle.signature).unwrap(),
    )
    .unwrap();
    let openssl = std::env::var("WPALT_TEST_OPENSSL").unwrap_or_else(|_| "openssl".into());
    let independent = std::process::Command::new(openssl)
        .current_dir(directory.path())
        .args([
            "pkeyutl",
            "-verify",
            "-pubin",
            "-keyform",
            "DER",
            "-inkey",
            "public.der",
            "-rawin",
            "-in",
            "message",
            "-sigfile",
            "signature",
        ])
        .output()
        .expect("Actual OpenSSL with Ed25519 support is required for this guarantee");
    assert!(
        independent.status.success(),
        "Independent OpenSSL rejected the Ed25519 attestation: {}",
        String::from_utf8_lossy(&independent.stderr)
    );
    for change in ["artifact", "manifest", "signature"] {
        let mut altered: documents::DocumentBundle =
            serde_json::from_value(serde_json::to_value(&bundle).unwrap()).unwrap();
        match change {
            "artifact" => altered.artifact = STANDARD.encode(b"Different document bytes"),
            "manifest" => altered.manifest = STANDARD.encode(b"Changed frozen statement"),
            _ => altered.signature = "00".repeat(64),
        }
        assert!(
            documents::verify(&altered, &trust).is_err(),
            "Tampered {change} must refuse"
        );
    }
    assert!(
        signer
            .attest(
                json!({}),
                "text/html",
                &vec![0; documents::MAX_ARTIFACT + 1]
            )
            .is_err()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let alias = directory.path().join("alias.pk8");
        symlink(&key, &alias).unwrap();
        assert!(
            Authority::load(&Config {
                key_file: alias,
                ..config.clone()
            })
            .is_err()
        );
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(
            Authority::load(&config).is_err(),
            "Readable private keys cannot be admitted"
        );
    }
}

#[tokio::test]
async fn original_acknowledgments_generate_private_attested_documents_after_republication_and_recovery()
 {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    use wpalt::{
        App, auth, backup,
        business::{forms::FormDefinition, store},
    };
    let postgres = std::env::var("TEST_DATABASE_URL").ok();
    if std::env::var("WPALT_REQUIRE_POSTGRES").is_ok() {
        assert!(postgres.is_some());
    }
    for engine in std::iter::once(None).chain(postgres.as_deref().map(Some)) {
        let data = tempfile::tempdir().unwrap();
        let key = data.path().join("documents.pk8");
        let trust = documents::create_key(&key).unwrap();
        let mut config = wpalt::config::Config {
            data_dir: data.path().join("source"),
            database_url: format!(
                "sqlite://{}?mode=rwc",
                data.path().join("source.db").display()
            ),
            documents: Config {
                enabled: true,
                key_file: key,
                ..Default::default()
            },
            ..Default::default()
        };
        let schema = if let Some(root) = engine {
            let name = format!("wpalt_d11_{}", uuid::Uuid::new_v4().simple());
            let pool = sqlx::PgPool::connect(root).await.unwrap();
            sqlx::query(&format!("CREATE SCHEMA {name}"))
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
            let mut url = url::Url::parse(root).unwrap();
            url.query_pairs_mut()
                .append_pair("options", &format!("-c search_path={name}"));
            config.database_url = url.to_string();
            Some(name)
        } else {
            None
        };
        let app = App::open(config.clone()).await.unwrap();
        auth::initialize(
            &app,
            "owner@example.test",
            "Owner",
            "a strong local password",
        )
        .await
        .unwrap();
        let (token, session) = auth::login(&app, "owner@example.test", "a strong local password")
            .await
            .unwrap();
        let mut form:FormDefinition=serde_json::from_value(json!({"title":"Original private application","fields":[
            {"name":"note","schema":{"kind":"string","label":"Private note","required":true}},
            {"name":"signature","schema":{"kind":"object","label":"Acceptance","required":true,"fields":{"name":{"kind":"string","required":true},"accepted":{"kind":"boolean","required":true}}},"widget":{"kind":"Signature","statement":"Original statement: I accept these exact terms."}}
        ]})).unwrap();
        let id = store::create(&app, &session.user.id, &form).await.unwrap();
        store::save(&app, &id, 1, &form, true).await.unwrap();
        let entry=store::submit(&app,&id,2,&uuid::Uuid::new_v4().to_string(),&json!({"note":"Private Zoë · 日本語 · مرحبا <script>forged()</script>","signature":{"name":"Zoë","accepted":true}})).await.unwrap();
        form.fields[1].widget = Some(wpalt::business::forms::Widget::Signature {
            statement: "Changed terms for later visitors".into(),
        });
        store::save(&app, &id, 2, &form, true).await.unwrap();
        let route = format!("/admin/forms/{id}/entries/{entry}/document");
        let request = |csrf: &str| {
            Request::builder()
                .method("POST")
                .uri(&route)
                .header("cookie", format!("wpalt_session={token}"))
                .header("origin", config.origin())
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    url::form_urlencoded::Serializer::new(String::new())
                        .append_pair("csrf", csrf)
                        .append_pair("kind", "html")
                        .finish(),
                ))
                .unwrap()
        };
        let router = wpalt::web::router(app.clone());
        assert_eq!(
            router
                .clone()
                .oneshot(request("invalid"))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        let response = router
            .clone()
            .oneshot(request(&session.csrf))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let bytes = to_bytes(response.into_body(), 12 * 1024 * 1024)
            .await
            .unwrap();
        let bundle: documents::DocumentBundle = serde_json::from_slice(&bytes).unwrap();
        let (manifest, artifact) = documents::verify(&bundle, &trust).unwrap();
        assert_eq!(manifest.source["form_version"], 2);
        assert_eq!(manifest.source["entry_id"], entry);
        let text = String::from_utf8(artifact).unwrap();
        assert!(text.contains("Original statement: I accept these exact terms."));
        assert!(!text.contains("Changed terms for later visitors"));
        assert!(text.contains("&lt;script&gt;forged()&lt;/script&gt;"));
        assert!(!text.contains("<script>"));
        assert!(text.contains("日本語"));
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM form_entries")
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            before, 1,
            "Generation records no response or new acknowledgment"
        );
        let archive = backup::capture(&app).await.unwrap();
        let restored = App::open(wpalt::config::Config {
            data_dir: data.path().join("restored"),
            database_url: format!(
                "sqlite://{}?mode=rwc",
                data.path().join("restored.db").display()
            ),
            ..config.clone()
        })
        .await
        .unwrap();
        backup::restore(&restored, &archive).await.unwrap();
        let (restored_token, restored_session) =
            auth::login(&restored, "owner@example.test", "a strong local password")
                .await
                .unwrap();
        let restored_request = Request::builder()
            .method("POST")
            .uri(&route)
            .header("cookie", format!("wpalt_session={restored_token}"))
            .header("origin", config.origin())
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(format!(
                "csrf={}&kind=html",
                restored_session.csrf
            )))
            .unwrap();
        let response = wpalt::web::router(restored.clone())
            .oneshot(restored_request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 12 * 1024 * 1024)
            .await
            .unwrap();
        let restored_bundle = serde_json::from_slice(&bytes).unwrap();
        let (saved, rendered) = documents::verify(&restored_bundle, &trust).unwrap();
        assert_eq!(saved.source, manifest.source);
        assert_eq!(
            String::from_utf8(rendered).unwrap(),
            text,
            "Source records and unchanged renderer/key reproduce the HTML after fresh recovery"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // The controlled worker pauses after authority was admitted. Revoke
            // that role while rendering; its bytes must never be disclosed.
            let started = data.path().join("renderer.started");
            let release = data.path().join("renderer.release");
            let renderer = data.path().join("controlled-renderer");
            std::fs::write(&renderer,format!("#!/bin/sh\nfor arg in \"$@\"; do case \"$arg\" in --print-to-pdf=*) out=${{arg#--print-to-pdf=}};; esac; done\n: > '{}'\nwhile [ ! -f '{}' ]; do /bin/sleep 0.02; done\nprintf '%%PDF-controlled-refusal-fixture' > \"$out\"\n",started.display(),release.display())).unwrap();
            std::fs::set_permissions(&renderer, std::fs::Permissions::from_mode(0o700)).unwrap();
            let mut rendering_config = config.clone();
            rendering_config.documents.pdf_enabled = true;
            rendering_config.documents.chromium = renderer;
            let rendering_app = App::open(rendering_config).await.unwrap();
            let request = Request::builder()
                .method("POST")
                .uri(&route)
                .header("cookie", format!("wpalt_session={token}"))
                .header("origin", config.origin())
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("csrf={}&kind=pdf", session.csrf)))
                .unwrap();
            let pending = tokio::spawn(wpalt::web::router(rendering_app.clone()).oneshot(request));
            tokio::time::timeout(std::time::Duration::from_secs(3), async {
                while !started.exists() {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            sqlx::query("UPDATE users SET role='subscriber' WHERE id=$1")
                .bind(&session.user.id)
                .execute(&app.db.pool)
                .await
                .unwrap();
            std::fs::write(release, b"finish controlled worker").unwrap();
            assert_eq!(
                pending.await.unwrap().unwrap().status(),
                StatusCode::FORBIDDEN,
                "Revocation during rendering refuses private output on either database"
            );
            assert!(std::fs::read_dir(&config.data_dir).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".document-")
            }));
            rendering_app.db.pool.close().await;
        }
        sqlx::query("UPDATE users SET role='subscriber' WHERE id=$1")
            .bind(&session.user.id)
            .execute(&app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            router
                .oneshot(request(&session.csrf))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN,
            "Current roles govern generation despite the previous editor session"
        );
        restored.db.pool.close().await;
        app.db.pool.close().await;
        if let Some(root) = engine {
            let pool = sqlx::PgPool::connect(root).await.unwrap();
            sqlx::query(&format!("DROP SCHEMA {} CASCADE", schema.unwrap()))
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }
    }
}

/// Controlled executables exercise refusal and cleanup; the browser journey uses
/// a real Chromium PDF, so these fixtures are never evidence of PDF correctness.
#[cfg(unix)]
#[tokio::test]
async fn optional_workers_refuse_failure_release_cancelled_work_and_bind_key_rotation() {
    use std::os::unix::fs::PermissionsExt;
    use wpalt::{App, config::Config as AppConfig, platform::local_processes::Coordinator};
    let directory = tempfile::tempdir().unwrap();
    let key = directory.path().join("documents.pk8");
    documents::create_key(&key).unwrap();
    let renderer = directory.path().join("controlled-renderer");
    let config = AppConfig {
        data_dir: directory.path().join("data"),
        database_url: format!(
            "sqlite://{}?mode=rwc",
            directory.path().join("site.db").display()
        ),
        documents: Config {
            enabled: true,
            key_file: key.clone(),
            pdf_enabled: true,
            chromium: renderer.clone(),
        },
        ..Default::default()
    };
    let app = App::open(config.clone()).await.unwrap();
    let write_worker = |body: &str| {
        std::fs::write(&renderer, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&renderer, std::fs::Permissions::from_mode(0o700)).unwrap();
    };
    let clean = || {
        assert!(
            std::fs::read_dir(&config.data_dir)
                .unwrap()
                .all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".document-")),
            "Failed or cancelled work must remove its private source/profile/output"
        );
    };
    assert!(
        wpalt::business::document_pdf::render(&app, b"<html>Private</html>")
            .await
            .is_err()
    );
    clean();
    for body in [
        "exit 1",
        "for arg in \"$@\"; do case \"$arg\" in --print-to-pdf=*) out=${arg#--print-to-pdf=};; esac; done\nprintf 'Not a PDF' > \"$out\"",
        "for arg in \"$@\"; do case \"$arg\" in --print-to-pdf=*) out=${arg#--print-to-pdf=};; esac; done\n/usr/bin/head -c 8388609 /dev/zero > \"$out\"",
    ] {
        write_worker(body);
        assert!(
            wpalt::business::document_pdf::render(&app, b"<html>Private</html>")
                .await
                .is_err()
        );
        clean();
    }
    let parent = directory.path().join("parent.pid");
    let child = directory.path().join("child.pid");
    write_worker(&format!(
        "echo $$ > '{}'\n/bin/sleep 60 &\necho $! > '{}'\nwait",
        parent.display(),
        child.display()
    ));
    let copy = app.clone();
    let task = tokio::spawn(async move {
        wpalt::business::document_pdf::render(&copy, b"<html>Private</html>").await
    });
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while !child.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    clean();
    for pid in [&parent, &child] {
        let id = std::fs::read_to_string(pid).unwrap();
        let output = std::process::Command::new("ps")
            .args(["-p", id.trim(), "-o", "stat="])
            .output()
            .unwrap();
        let state = String::from_utf8(output.stdout).unwrap();
        assert!(
            state.trim().is_empty() || state.trim().starts_with('Z'),
            "Owned cancelled process remains executing: {state}"
        );
    }
    // Exercise the actual complete-operation deadline, not a mocked clock.
    assert!(
        wpalt::business::document_pdf::render(&app, b"<html>Private</html>")
            .await
            .unwrap_err()
            .1
            .contains("deadline")
    );
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    clean();
    let original = Coordinator::new(&config).unwrap();
    original.initialize().unwrap();
    original.check().await.unwrap();
    let replacement = directory.path().join("replacement.pk8");
    documents::create_key(&replacement).unwrap();
    std::fs::rename(replacement, &key).unwrap();
    original.check().await.unwrap();
    assert!(
        Coordinator::new(&config).unwrap().check().await.is_err(),
        "A new node cannot join with changed key bytes at the same path"
    );
    app.db.pool.close().await;
}
