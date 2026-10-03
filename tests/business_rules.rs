//! Business rules protect collected data before any storage or queued action.
use serde_json::json;
use wpalt::{business::forms::FormDefinition, schema::Definition};

fn quotation_form() -> FormDefinition {
    serde_json::from_value(json!({
        "title":"Request a quotation",
        "fields":[
            {"name":"business","schema":{"kind":"boolean","required":true}},
            {"name":"company","schema":{"kind":"string","required":true},
             "visible_when":{"operation":"Equal","field":"business","value":true}},
            {"name":"quantity","schema":{"kind":"number","required":true},"step":1},
            {"name":"rate","schema":{"kind":"number","required":true},"step":1},
            {"name":"total","schema":{"kind":"number","required":true},"step":1,
             "calculation":{"operation":"Product","fields":["quantity","rate"]}}
        ]
    }))
    .unwrap()
}

#[test]
fn hidden_values_and_forged_totals_cannot_enter_the_authoritative_submission() {
    let form = quotation_form();
    let common = Definition::default();
    let result = form
        .evaluate(
            &common,
            &json!({"business":false,"company":"discard me","quantity":3,"rate":12.5,"total":0}),
            false,
        )
        .unwrap();
    assert_eq!(
        result,
        json!({"business":false,"quantity":3,"rate":12.5,"total":37.5})
    );
    assert!(
        form.evaluate(
            &common,
            &json!({"business":true,"quantity":3,"rate":12.5}),
            false
        )
        .is_err(),
        "A newly visible required company cannot be skipped."
    );
    assert!(
        form.evaluate(
            &common,
            &json!({"business":false,"quantity":"3","rate":12.5}),
            false
        )
        .is_err()
    );
    assert!(
        form.evaluate(
            &common,
            &json!({"business":false,"quantity":3,"rate":12.5,"admin":true}),
            false
        )
        .is_err()
    );
}

#[test]
fn partial_drafts_preserve_valid_work_without_bypassing_final_validation() {
    let form = quotation_form();
    let common = Definition::default();
    let draft = json!({"business":true,"company":"Example"});
    assert_eq!(form.evaluate(&common, &draft, true).unwrap(), draft);
    assert!(form.evaluate(&common, &draft, false).is_err());
    assert!(
        form.evaluate(&common, &json!({"business":"true"}), true)
            .is_err(),
        "Partial does not mean untyped."
    );
    assert!(
        form.evaluate(
            &common,
            &json!({"business":false,"quantity":1e308,"rate":1e308}),
            false
        )
        .is_err(),
        "Non-finite derived values must fail."
    );
}

#[test]
fn invalid_dependencies_and_shared_repeater_limits_fail_before_actions() {
    let common = Definition::default();
    let mut form = quotation_form();
    form.fields.swap(0, 1);
    assert!(
        form.validate(&common).is_err(),
        "Forward conditions and cycles cannot execute."
    );
    let form: FormDefinition = serde_json::from_value(json!({"title":"Household","fields":[
        {"name":"members","schema":{"kind":"repeater","max_items":2,"required":true,
         "fields":{"name":{"kind":"string","required":true}}}}
    ]}))
    .unwrap();
    assert!(
        form.evaluate(&common, &json!({"members":[{}, {}, {}]}), true)
            .is_err()
    );
    assert!(
        form.evaluate(&common, &json!({"members":[{}]}), false)
            .is_err()
    );
    assert_eq!(
        form.evaluate(&common, &json!({"members":[{}]}), true)
            .unwrap(),
        json!({"members":[{}]})
    );
    assert!(
        form.evaluate(
            &common,
            &json!({"members":[{"name":"Ada","extra":"unregistered"}]}),
            false
        )
        .is_err()
    );
}

#[tokio::test]
async fn publication_retries_and_fresh_restore_preserve_one_authoritative_entry() {
    use wpalt::{App, auth, backup, business::store};
    let postgres = std::env::var("TEST_DATABASE_URL").ok();
    if std::env::var("WPALT_REQUIRE_POSTGRES").is_ok() {
        assert!(postgres.is_some(), "Real PostgreSQL coverage is required.");
    }
    for engine in std::iter::once(None).chain(postgres.as_deref().map(Some)) {
        let data = tempfile::tempdir().unwrap();
        let (config, source_schema) = database_config(&data, "source", engine).await;
        let app = App::open(config).await.unwrap();
        auth::initialize(
            &app,
            "owner@example.test",
            "Owner",
            "a strong local password",
        )
        .await
        .unwrap();
        let (_, session) = auth::login(&app, "owner@example.test", "a strong local password")
            .await
            .unwrap();
        let mut common = wpalt::schema::Registry::load(&app).await.unwrap().common;
        common.groups.insert(
            "person".into(),
            serde_json::from_value(json!({"name":{"kind":"string","required":true}})).unwrap(),
        );
        wpalt::schema::save_common(&app, common.clone(), 1)
            .await
            .unwrap();
        let mut definition = quotation_form();
        definition.max_entries = 2;
        definition.fields.push(
            serde_json::from_value(
                json!({"name":"person","step":1,"schema":{"kind":"group","group":"person"}}),
            )
            .unwrap(),
        );
        let form = store::create(&app, &session.user.id, &definition)
            .await
            .unwrap();
        let input = json!({"business":false,"quantity":2,"rate":7,"person":{"name":"Ada"}});
        let key = uuid::Uuid::new_v4().to_string();
        assert!(
            store::submit(&app, &form, 1, &key, &input).await.is_err(),
            "Draft forms are private."
        );
        store::save(&app, &form, 1, &definition, true)
            .await
            .unwrap();
        assert!(
            store::save(&app, &form, 1, &definition, true)
                .await
                .is_err(),
            "Stale editors cannot overwrite publication."
        );
        let draft_token = auth::random_token();
        let draft = wpalt::business::drafts::save(
            &app,
            &form,
            wpalt::business::drafts::Save {
                token: draft_token.clone(),
                version: 2,
                revision: 0,
                values: json!({"quantity":2}),
            },
        )
        .await
        .unwrap();
        assert_eq!(draft.revision, 1);
        assert!(
            wpalt::business::drafts::save(
                &app,
                &form,
                wpalt::business::drafts::Save {
                    token: draft_token.clone(),
                    version: 2,
                    revision: 9,
                    values: json!({"quantity":3})
                }
            )
            .await
            .is_err(),
            "A stale recovery editor cannot overwrite saved work."
        );
        assert!(
            wpalt::business::drafts::load(&app, &uuid::Uuid::new_v4().to_string(), &draft_token)
                .await
                .is_err(),
            "Recovery capabilities are form scoped."
        );
        // Independent pools demonstrate that idempotency does not rely on App's mutex.
        common.groups.get_mut("person").unwrap().insert(
            "country".into(),
            serde_json::from_value(json!({"kind":"string","required":true})).unwrap(),
        );
        wpalt::schema::save_common(&app, common, 2).await.unwrap();
        // The newer required country must not alter an already published form.
        let other = App::open((*app.config).clone()).await.unwrap();
        let (first, retry) = tokio::join!(
            store::submit(&app, &form, 2, &key, &input),
            store::submit(&other, &form, 2, &key, &input)
        );
        assert_eq!(first.unwrap(), retry.unwrap());
        assert!(
            store::submit(
                &app,
                &form,
                2,
                &key,
                &json!({"business":false,"quantity":3,"rate":7})
            )
            .await
            .is_err()
        );
        let bytes = backup::capture(&app).await.unwrap();
        let (restored_config, restored_schema) = database_config(&data, "restored", engine).await;
        let restored = App::open(restored_config).await.unwrap();
        backup::restore(&restored, &bytes).await.unwrap();
        assert_eq!(
            wpalt::business::drafts::load(&restored, &form, &draft_token)
                .await
                .unwrap()
                .values,
            json!({"quantity":2})
        );
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM form_entries")
            .fetch_one(&restored.db.pool)
            .await
            .unwrap();
        assert_eq!(rows, 1);
        let values: String = sqlx::query_scalar("SELECT values_json FROM form_entries")
            .fetch_one(&restored.db.pool)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&values).unwrap()["total"],
            14.0
        );
        store::save(&app, &form, 2, &definition, true)
            .await
            .unwrap();
        assert!(
            store::submit(&app, &form, 2, &uuid::Uuid::new_v4().to_string(), &input)
                .await
                .is_err(),
            "Stale public schemas need review."
        );
        assert!(
            store::submit(&app, &form, 2, &key, &input).await.is_ok(),
            "An accepted retry remains accepted after republishing."
        );
        let current = json!({"business":false,"quantity":2,"rate":7,"person":{"name":"Grace","country":"Example"}});
        let left_key = uuid::Uuid::new_v4().to_string();
        let right_key = uuid::Uuid::new_v4().to_string();
        let (left, right) = tokio::join!(
            store::submit(&app, &form, 3, &left_key, &current),
            store::submit(&other, &form, 3, &right_key, &current)
        );
        assert_eq!(
            usize::from(left.is_ok()) + usize::from(right.is_ok()),
            1,
            "Only one independent connection can claim the last response slot."
        );
        let count: i64 = sqlx::query_scalar("SELECT entry_count FROM business_forms WHERE id=$1")
            .bind(&form)
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
        use tower::ServiceExt;
        let response = wpalt::web::router(app.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/admin/forms/{form}/entries"))
                    .header("accept", "application/json")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::SEE_OTHER);
        assert_eq!(response.headers()["location"], "/login");
        assert!(
            response.headers()["cache-control"]
                .to_str()
                .unwrap()
                .contains("no-store")
        );
        let response = wpalt::web::router(app.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/api/admin/forms/{form}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::UNAUTHORIZED,
            "Unauthenticated API requests receive no form data."
        );
        let mut disabled_config = (*app.config).clone();
        disabled_config.business_enabled = false;
        let disabled = App::open(disabled_config).await.unwrap();
        let response = wpalt::web::router(disabled.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri("/assets/forms.js")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::NOT_FOUND,
            "Disabled forms do not serve their frontend asset."
        );
        disabled.db.pool.close().await;
        other.db.pool.close().await;
        restored.db.pool.close().await;
        app.db.pool.close().await;
        if let Some(url) = engine {
            let pool = sqlx::PgPool::connect(url).await.unwrap();
            for name in [source_schema, restored_schema].into_iter().flatten() {
                sqlx::query(&format!("DROP SCHEMA {name} CASCADE"))
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            pool.close().await;
        }
    }
}

async fn database_config(
    data: &tempfile::TempDir,
    label: &str,
    postgres: Option<&str>,
) -> (wpalt::config::Config, Option<String>) {
    let mut config = wpalt::config::Config {
        data_dir: data.path().join(label),
        database_url: format!(
            "sqlite://{}?mode=rwc",
            data.path().join(format!("{label}.db")).display()
        ),
        ..wpalt::config::Config::default()
    };
    let mut schema = None;
    if let Some(root) = postgres {
        let name = format!("wpalt_business_{}", uuid::Uuid::new_v4().simple());
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
        schema = Some(name);
    }
    (config, schema)
}

#[tokio::test]
async fn consent_confirmation_withdrawal_and_mail_recovery_are_one_durable_journey() {
    use sqlx::Row;
    use wpalt::{
        App, auth, backup,
        business::{audience, mail, store},
    };
    let postgres = std::env::var("TEST_DATABASE_URL").ok();
    if std::env::var("WPALT_REQUIRE_POSTGRES").is_ok() {
        assert!(postgres.is_some());
    }
    for engine in std::iter::once(None).chain(postgres.as_deref().map(Some)) {
        let data = tempfile::tempdir().unwrap();
        let (config, source_schema) = database_config(&data, "consent", engine).await;
        let app = App::open(config).await.unwrap();
        auth::initialize(
            &app,
            "owner@example.test",
            "Owner",
            "a strong local password",
        )
        .await
        .unwrap();
        let (_, session) = auth::login(&app, "owner@example.test", "a strong local password")
            .await
            .unwrap();
        let list = audience::create_list(&app, "Newsletter", "Monthly product news", "2026-10")
            .await
            .unwrap();
        let definition:FormDefinition=serde_json::from_value(json!({"title":"Subscribe","fields":[{"name":"email","schema":{"kind":"string","required":true}},{"name":"consent","schema":{"kind":"boolean","label":"I agree to monthly product news"}}],"subscription":{"list":list,"policy":"2026-10","email_field":"email","consent_field":"consent"}})).unwrap();
        let form = store::create(&app, &session.user.id, &definition)
            .await
            .unwrap();
        store::save(&app, &form, 1, &definition, true)
            .await
            .unwrap();
        store::submit(
            &app,
            &form,
            2,
            &uuid::Uuid::new_v4().to_string(),
            &json!({"email":"person@example.test","consent":false}),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audience_contacts")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            0,
            "Declining consent creates no audience contact."
        );
        let input = json!({"email":"person@example.test","consent":true});
        let other = App::open((*app.config).clone()).await.unwrap();
        let left = uuid::Uuid::new_v4().to_string();
        let right = uuid::Uuid::new_v4().to_string();
        let (a, b) = tokio::join!(
            store::submit(&app, &form, 2, &left, &input),
            store::submit(&other, &form, 2, &right, &input)
        );
        a.unwrap();
        b.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM mail_jobs")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            1,
            "Parallel requests cannot flood confirmation mail."
        );
        let row = sqlx::query("SELECT id,plain FROM mail_jobs")
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        let job: String = row.get("id");
        let plain: String = row.get("plain");
        let token = plain
            .lines()
            .find(|line| line.contains("/confirm/"))
            .unwrap()
            .rsplit('/')
            .next()
            .unwrap();
        audience::review(&app, token, false).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT state FROM audience_memberships")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            "pending",
            "GET review must not confirm email-prefetches."
        );
        let (a, b) = tokio::join!(mail::tick(&app), mail::tick(&other));
        assert_eq!(
            a.unwrap() + b.unwrap(),
            1,
            "Independent workers claim a job once."
        );
        audience::decide(&app, token, false).await.unwrap();
        audience::decide(&app, token, false).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM audience_consent_events WHERE action='confirmed'"
            )
            .fetch_one(&app.db.pool)
            .await
            .unwrap(),
            1
        );
        let expected = mail::download(&app, &job).await.unwrap();
        assert_eq!(
            tokio::fs::read(
                app.config
                    .data_dir
                    .join("outbox")
                    .join(format!("{job}.eml"))
            )
            .await
            .unwrap(),
            expected
        );
        assert_eq!(
            mail::download(&app, &job).await.unwrap(),
            expected,
            "Stable MIME bytes support safe local spool recovery."
        );
        let archive = backup::capture(&app).await.unwrap();
        let (config, restored_schema) = database_config(&data, "consent_restored", engine).await;
        let restored = App::open(config).await.unwrap();
        backup::restore(&restored, &archive).await.unwrap();
        assert_eq!(
            mail::download(&restored, &job).await.unwrap(),
            expected,
            "Mail can be reconstructed from a fresh restore."
        );
        let withdrawal = plain
            .lines()
            .find(|line| line.contains("/withdraw/"))
            .unwrap()
            .rsplit('/')
            .next()
            .unwrap();
        let contact: String = sqlx::query_scalar(
            "SELECT id FROM audience_contacts WHERE email='person@example.test'",
        )
        .fetch_one(&app.db.pool)
        .await
        .unwrap();
        audience::update_contact(
            &app,
            &contact,
            1,
            "Person",
            audience::Attributes {
                company: "Local team".into(),
                source: "Form".into(),
                score: 12.0,
            },
            false,
        )
        .await
        .unwrap();
        assert!(
            audience::update_contact(
                &app,
                &contact,
                1,
                "Stale editor",
                audience::Attributes::default(),
                false
            )
            .await
            .is_err()
        );
        let campaign = wpalt::business::campaigns::create(&app, "Monthly news", &list)
            .await
            .unwrap();
        wpalt::business::campaigns::save(
            &app,
            &campaign,
            1,
            "Monthly news",
            &wpalt::document::empty(),
            wpalt::now(),
            audience::Segment {
                company: Some("Local team".into()),
                minimum_score: Some(10.0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let (a, b) = tokio::join!(
            wpalt::business::campaigns::tick(&app),
            wpalt::business::campaigns::tick(&other)
        );
        assert_eq!(
            a.unwrap() + b.unwrap(),
            1,
            "Concurrent expansion cannot enqueue the same recipient twice."
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM mail_jobs WHERE kind='campaign'")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            1
        );
        audience::decide(&app, withdrawal, true).await.unwrap();
        store::submit(&app, &form, 2, &uuid::Uuid::new_v4().to_string(), &input)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT state FROM audience_memberships")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            "withdrawn",
            "An anonymous claim cannot reverse a withdrawal."
        );
        let race_key = uuid::Uuid::new_v4().to_string();
        let (deleted, claim) = tokio::join!(
            audience::delete_contact(&app, &contact),
            store::submit(&other, &form, 2, &race_key, &input)
        );
        deleted.unwrap();
        claim.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audience_contacts")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            0,
            "A concurrent anonymous claim cannot recreate an erased contact."
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM mail_jobs")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            0
        );
        let suppressed: String =
            sqlx::query_scalar("SELECT hash FROM audience_suppressions WHERE suppressed=1")
                .fetch_one(&app.db.pool)
                .await
                .unwrap();
        assert_ne!(
            suppressed,
            auth::digest(b"person@example.test"),
            "Suppression retains a keyed digest."
        );
        for pool in [&other.db.pool, &restored.db.pool, &app.db.pool] {
            pool.close().await;
        }
        if let Some(url) = engine {
            let pool = sqlx::PgPool::connect(url).await.unwrap();
            for schema in [source_schema, restored_schema].into_iter().flatten() {
                sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            pool.close().await;
        }
    }
}

#[tokio::test]
async fn smtp_acceptance_and_lost_receipts_have_distinct_recovery_states() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use wpalt::{App, auth, business::mail};
    let data = tempfile::tempdir().unwrap();
    for accepted in [true, false] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let (read, mut write) = socket.into_split();
            let mut read = BufReader::new(read);
            write.write_all(b"220 localhost ESMTP\r\n").await.unwrap();
            let mut line = String::new();
            let mut data = false;
            loop {
                line.clear();
                if read.read_line(&mut line).await.unwrap() == 0 {
                    break;
                }
                if data {
                    if line == ".\r\n" {
                        if !accepted {
                            break;
                        }
                        write.write_all(b"250 queued\r\n").await.unwrap();
                        data = false;
                    }
                    continue;
                }
                if line.starts_with("EHLO") {
                    write.write_all(b"250 localhost\r\n").await.unwrap();
                } else if line.starts_with("MAIL FROM")
                    || line.starts_with("RCPT TO")
                    || line.starts_with("RSET")
                {
                    write.write_all(b"250 OK\r\n").await.unwrap();
                } else if line.starts_with("DATA") {
                    write.write_all(b"354 send message\r\n").await.unwrap();
                    data = true;
                } else if line.starts_with("QUIT") {
                    let _ = write.write_all(b"221 goodbye\r\n").await;
                    break;
                } else {
                    panic!("unexpected loopback SMTP command");
                }
            }
        });
        let (mut config, _) =
            database_config(&data, if accepted { "accepted" } else { "lost" }, None).await;
        config.mail.smtp.push(mail::Connection {
            host: "127.0.0.1".into(),
            port,
            tls: "local".into(),
            username: "".into(),
            password: "".into(),
        });
        let app = App::open(config).await.unwrap();
        auth::initialize(
            &app,
            "owner@example.test",
            "Owner",
            "a strong local password",
        )
        .await
        .unwrap();
        let mut tx = app.db.pool.begin().await.unwrap();
        let id = mail::enqueue(
            &app,
            &mut tx,
            mail::MessageInput {
                dedupe: "test:notification",
                contact: "",
                list: "",
                kind: "notification",
                recipient: "recipient@example.test",
                subject: "Local transport test",
                html: "<p>Hello</p>",
                plain: "Hello",
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), mail::tick(&app))
            .await
            .unwrap()
            .unwrap();
        server.await.unwrap();
        let state: String = sqlx::query_scalar("SELECT state FROM mail_jobs WHERE id=$1")
            .bind(&id)
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        assert_eq!(state, if accepted { "sent" } else { "uncertain" });
        assert_eq!(
            mail::tick(&app).await.unwrap(),
            0,
            "An uncertain outcome must not be blindly resent."
        );
        app.db.pool.close().await;
    }
}

#[test]
fn surveys_scores_and_signature_acknowledgments_preserve_published_meaning() {
    let form:FormDefinition=serde_json::from_value(json!({"title":"Survey","fields":[
        {"name":"answer","schema":{"kind":"string","required":true},"widget":{"kind":"Choice","options":[{"label":"Yes","value":"yes","score":5},{"label":"No","value":"no","score":0}]}},
        {"name":"score","schema":{"kind":"number"},"calculation":{"operation":"Score","fields":["answer"]}},
        {"name":"signed","schema":{"kind":"object","required":true,"fields":{"name":{"kind":"string","required":true},"accepted":{"kind":"boolean","required":true}}},"widget":{"kind":"Signature","statement":"I confirm this response is accurate."}}
    ]})).unwrap();
    let common = Definition::default();
    let result = form
        .evaluate(
            &common,
            &json!({"answer":"yes","score":999,"signed":{"name":"Ada","accepted":true}}),
            false,
        )
        .unwrap();
    assert_eq!(result["score"], 5.0);
    assert!(
        form.evaluate(
            &common,
            &json!({"answer":"invented","signed":{"name":"Ada","accepted":true}}),
            false
        )
        .is_err()
    );
    assert!(
        form.evaluate(
            &common,
            &json!({"answer":"yes","signed":{"name":"Ada","accepted":false}}),
            false
        )
        .is_err()
    );
    assert!(
        form.evaluate(&common, &json!({"answer":"yes"}), true)
            .is_ok(),
        "An unsigned partial draft may be resumed, but cannot submit as signed."
    );
    assert!(
        form.evaluate(&common, &json!({"answer":"yes"}), false)
            .is_err()
    );
}

#[tokio::test]
async fn attachments_follow_up_and_search_remain_private_and_survive_fresh_restore() {
    use axum::{
        body::Bytes,
        extract::{Path, State},
        http::HeaderMap,
    };
    use sqlx::Row;
    use wpalt::{
        App, auth, backup,
        business::{attachments, entries, store},
    };
    let postgres = std::env::var("TEST_DATABASE_URL").ok();
    if std::env::var("WPALT_REQUIRE_POSTGRES").is_ok() {
        assert!(postgres.is_some());
    }
    for engine in std::iter::once(None).chain(postgres.as_deref().map(Some)) {
        let data = tempfile::tempdir().unwrap();
        let (config, source_schema) = database_config(&data, "attachments", engine).await;
        let app = App::open(config).await.unwrap();
        auth::initialize(
            &app,
            "owner@example.test",
            "Owner",
            "a strong local password",
        )
        .await
        .unwrap();
        let (_, session) = auth::login(&app, "owner@example.test", "a strong local password")
            .await
            .unwrap();
        let definition:FormDefinition=serde_json::from_value(json!({"title":"Private documents","fields":[{"name":"name","schema":{"kind":"string","required":true}},{"name":"file","schema":{"kind":"string","required":true},"widget":{"kind":"Upload"}}]})).unwrap();
        let form = store::create(&app, &session.user.id, &definition)
            .await
            .unwrap();
        store::save(&app, &form, 1, &definition, true)
            .await
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("content-type", "text/plain".parse().unwrap());
        headers.insert("x-form-version", "2".parse().unwrap());
        headers.insert("x-file-name", "private.txt".parse().unwrap());
        let uploaded = attachments::upload(
            State(app.clone()),
            Path((form.clone(), "file".into())),
            headers.clone(),
            Bytes::from_static(b"private application notes"),
        )
        .await
        .unwrap()
        .0;
        assert!(
            attachments::upload(
                State(app.clone()),
                Path((form.clone(), "name".into())),
                headers.clone(),
                Bytes::from_static(b"wrong field")
            )
            .await
            .is_err()
        );
        let mut unsafe_headers = headers.clone();
        unsafe_headers.insert("x-file-name", "../secret.txt".parse().unwrap());
        assert!(
            attachments::upload(
                State(app.clone()),
                Path((form.clone(), "file".into())),
                unsafe_headers,
                Bytes::from_static(b"unsafe")
            )
            .await
            .is_err()
        );
        let input = json!({"name":"Ada Lovelace","file":uploaded["capability"]});
        let key = uuid::Uuid::new_v4().to_string();
        let entry = store::submit(&app, &form, 2, &key, &input).await.unwrap();
        assert_eq!(
            store::submit(&app, &form, 2, &key, &input).await.unwrap(),
            entry
        );
        assert!(
            store::submit(&app, &form, 2, &uuid::Uuid::new_v4().to_string(), &input)
                .await
                .is_err(),
            "An attachment capability belongs to one accepted entry."
        );
        entries::follow_up(
            &app,
            &form,
            &entry,
            1,
            "Needs a follow-up",
            &session.user.id,
        )
        .await
        .unwrap();
        assert!(
            entries::follow_up(&app, &form, &entry, 1, "stale overwrite", "")
                .await
                .is_err()
        );
        assert_eq!(
            entries::search(&app, &form, "Lovelace", 0, "")
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            entries::search(&app, &form, "nonexistent", 0, "")
                .await
                .unwrap()
                .is_empty()
        );
        let export = entries::export(&app, &form, "").await.unwrap();
        assert!(export.contains("Ada Lovelace"));
        assert!(
            !export.contains(uploaded["capability"].as_str().unwrap()),
            "Accepted data retains the attachment ID, not its upload capability."
        );
        let file = sqlx::query("SELECT id,filename FROM form_attachments WHERE entry_id=$1")
            .bind(&entry)
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        let id: String = file.get("id");
        let filename: String = file.get("filename");
        use tower::ServiceExt;
        let response = wpalt::web::router(app.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!(
                        "/admin/forms/{form}/entries/{entry}/attachments/{id}"
                    ))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::SEE_OTHER);
        let response = wpalt::web::router(app.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/media/{id}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::NOT_FOUND,
            "Private form files cannot appear in the public media route."
        );
        let archive = backup::capture(&app).await.unwrap();
        let (config, restored_schema) =
            database_config(&data, "attachments_restored", engine).await;
        let restored = App::open(config).await.unwrap();
        backup::restore(&restored, &archive).await.unwrap();
        assert_eq!(
            tokio::fs::read(restored.config.data_dir.join("attachments").join(&filename))
                .await
                .unwrap(),
            b"private application notes"
        );
        assert_eq!(
            entries::search(&restored, &form, "Lovelace", 0, "")
                .await
                .unwrap()
                .len(),
            1
        );
        let notes: String =
            sqlx::query_scalar("SELECT notes FROM form_entry_workflows WHERE entry_id=$1")
                .bind(&entry)
                .fetch_one(&restored.db.pool)
                .await
                .unwrap();
        assert_eq!(notes, "Needs a follow-up");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(restored.config.data_dir.join("attachments").join(&filename))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        for pool in [&restored.db.pool, &app.db.pool] {
            pool.close().await;
        }
        if let Some(url) = engine {
            let pool = sqlx::PgPool::connect(url).await.unwrap();
            for schema in [source_schema, restored_schema].into_iter().flatten() {
                sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            pool.close().await;
        }
    }
}

#[tokio::test]
async fn engagement_requires_current_consent_masks_geometry_and_erases_on_withdrawal() {
    use axum::http::HeaderMap;
    use wpalt::{
        App, auth, backup,
        business::engagement::{self, Capture, Frame, Rectangle},
    };
    let postgres = std::env::var("TEST_DATABASE_URL").ok();
    if std::env::var("WPALT_REQUIRE_POSTGRES").is_ok() {
        assert!(postgres.is_some());
    }
    for engine in std::iter::once(None).chain(postgres.as_deref().map(Some)) {
        let data = tempfile::tempdir().unwrap();
        let (config, source_schema) = database_config(&data, "engagement", engine).await;
        let app = App::open(config).await.unwrap();
        auth::initialize(
            &app,
            "owner@example.test",
            "Owner",
            "a strong local password",
        )
        .await
        .unwrap();
        let event = || Capture {
            id: uuid::Uuid::new_v4().to_string(),
            path: "/".into(),
            name: "pageview".into(),
            dimensions: [("device".into(), "mobile".into())].into(),
            frame: None,
        };
        assert!(
            !engagement::capture(&app, &HeaderMap::new(), event())
                .await
                .unwrap()
        );
        assert!(
            app.db.settings().await.unwrap().analytics.is_none(),
            "A fresh blog has no analytics asset or consent UI."
        );
        sqlx::query("UPDATE engagement_settings SET enabled=1,recording=1,version=2 WHERE id=1")
            .execute(&app.db.pool)
            .await
            .unwrap();
        let token = engagement::consent(&app, &HeaderMap::new(), true, false, 2)
            .await
            .unwrap()
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("cookie", format!("wpalt_visitor={token}").parse().unwrap());
        let id = uuid::Uuid::new_v4().to_string();
        let mut input = event();
        input.id = id.clone();
        assert!(engagement::capture(&app, &headers, input).await.unwrap());
        let mut replay = event();
        replay.id = id;
        assert!(engagement::capture(&app, &headers, replay).await.unwrap());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM engagement_events")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            1,
            "Repeated event IDs do not inflate reports."
        );
        let frame = Frame {
            width: 320,
            height: 900,
            scroll_y: 0,
            elapsed: 1,
            rectangles: vec![Rectangle {
                x: 16,
                y: 40,
                width: 280,
                height: 44,
                kind: "control".into(),
            }],
            click: Some([40, 48]),
        };
        let mut recording = event();
        recording.name = "interaction".into();
        recording.frame = Some(frame.clone());
        assert!(
            !engagement::capture(&app, &headers, recording)
                .await
                .unwrap(),
            "Analytics consent alone never enables optional recording."
        );
        let token = engagement::consent(&app, &HeaderMap::new(), true, true, 2)
            .await
            .unwrap()
            .unwrap();
        let mut recording_headers = HeaderMap::new();
        recording_headers.insert("cookie", format!("wpalt_visitor={token}").parse().unwrap());
        let mut recording = event();
        recording.name = "interaction".into();
        recording.frame = Some(frame);
        assert!(
            engagement::capture(&app, &recording_headers, recording)
                .await
                .unwrap()
        );
        assert!(serde_json::from_value::<Frame>(json!({"width":320,"height":900,"scroll_y":0,"elapsed":1,"rectangles":[],"text":"must never capture"})).is_err());
        let mut forged = event();
        forged.name = "form_submit".into();
        assert!(
            engagement::capture(&app, &headers, forged).await.is_err(),
            "Conversion events require server-confirmed actions."
        );
        let mut private = event();
        private.path = "/admin/posts".into();
        assert!(engagement::capture(&app, &headers, private).await.is_err());
        let mut pii = event();
        pii.dimensions
            .insert("email".into(), "private@example.test".into());
        assert!(engagement::capture(&app, &headers, pii).await.is_err());
        let mut gpc = recording_headers.clone();
        gpc.insert("sec-gpc", "1".parse().unwrap());
        assert!(!engagement::capture(&app, &gpc, event()).await.unwrap());
        let archive = backup::capture(&app).await.unwrap();
        let (config, restored_schema) = database_config(&data, "engagement_restored", engine).await;
        let restored = App::open(config).await.unwrap();
        backup::restore(&restored, &archive).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM engagement_events")
                .fetch_one(&restored.db.pool)
                .await
                .unwrap(),
            2
        );
        engagement::consent(&app, &recording_headers, false, false, 2)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM engagement_events WHERE frame<>''")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            0,
            "Withdrawal erases recorded interaction geometry."
        );
        sqlx::query("UPDATE engagement_settings SET version=3 WHERE id=1")
            .execute(&app.db.pool)
            .await
            .unwrap();
        assert!(
            !engagement::capture(&app, &headers, event()).await.unwrap(),
            "Changed purpose/version requires renewed consent."
        );
        let other = App::open((*app.config).clone()).await.unwrap();
        let token = engagement::consent(&app, &HeaderMap::new(), true, false, 3)
            .await
            .unwrap()
            .unwrap();
        let mut race_headers = HeaderMap::new();
        race_headers.insert("cookie", format!("wpalt_visitor={token}").parse().unwrap());
        let (recorded, withdrawn) = tokio::join!(
            engagement::capture(&app, &race_headers, event()),
            engagement::consent(&other, &race_headers, false, false, 3)
        );
        recorded.unwrap();
        withdrawn.unwrap();
        let hash = engagement::token(&race_headers).unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM engagement_events WHERE session_hash=$1"
            )
            .bind(hash)
            .fetch_one(&app.db.pool)
            .await
            .unwrap(),
            0
        );
        for pool in [&other.db.pool, &restored.db.pool, &app.db.pool] {
            pool.close().await;
        }
        if let Some(url) = engine {
            let pool = sqlx::PgPool::connect(url).await.unwrap();
            for schema in [source_schema, restored_schema].into_iter().flatten() {
                sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            pool.close().await;
        }
    }
}

#[tokio::test]
async fn local_offers_keep_variants_bound_frequency_and_allocate_last_reward_atomically() {
    use axum::http::HeaderMap;
    use wpalt::{
        App, auth, backup,
        business::{
            engagement,
            promotions::{self, Target, Visit},
        },
    };
    let postgres = std::env::var("TEST_DATABASE_URL").ok();
    if std::env::var("WPALT_REQUIRE_POSTGRES").is_ok() {
        assert!(postgres.is_some());
    }
    for engine in std::iter::once(None).chain(postgres.as_deref().map(Some)) {
        let data = tempfile::tempdir().unwrap();
        let (config, source_schema) = database_config(&data, "offers", engine).await;
        let app = App::open(config).await.unwrap();
        auth::initialize(
            &app,
            "owner@example.test",
            "Owner",
            "a strong local password",
        )
        .await
        .unwrap();
        sqlx::query("UPDATE engagement_settings SET enabled=1 WHERE id=1")
            .execute(&app.db.pool)
            .await
            .unwrap();
        let id = promotions::create(&app, "Local launch offer")
            .await
            .unwrap();
        promotions::save(
            &app,
            &id,
            1,
            "a",
            &wpalt::document::empty(),
            Target {
                device: "mobile".into(),
                max_impressions: 2,
                ..Default::default()
            },
            true,
            true,
            true,
        )
        .await
        .unwrap();
        promotions::reward(&app, &id, "Last invitation", 1, 1)
            .await
            .unwrap();
        let policy: i64 = sqlx::query_scalar("SELECT version FROM engagement_settings WHERE id=1")
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        let visit = |device: &str| Visit {
            path: "/".into(),
            device: device.into(),
            referrer: "direct".into(),
        };
        assert!(
            promotions::visit(&app, &HeaderMap::new(), visit("mobile"))
                .await
                .unwrap()
                .is_none()
        );
        let mut visitors = vec![];
        for _ in 0..2 {
            let token = engagement::consent(&app, &HeaderMap::new(), true, false, policy)
                .await
                .unwrap()
                .unwrap();
            let mut h = HeaderMap::new();
            h.insert("cookie", format!("wpalt_visitor={token}").parse().unwrap());
            visitors.push(h);
        }
        assert!(
            promotions::visit(&app, &visitors[0], visit("desktop"))
                .await
                .unwrap()
                .is_none(),
            "Device targeting is enforced before an impression is reserved."
        );
        let first = promotions::visit(&app, &visitors[0], visit("mobile"))
            .await
            .unwrap()
            .unwrap();
        let second = promotions::visit(&app, &visitors[0], visit("mobile"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            first["variant"], second["variant"],
            "Assignment remains stable for this consented session."
        );
        assert!(
            promotions::visit(&app, &visitors[0], visit("mobile"))
                .await
                .unwrap()
                .is_none(),
            "Repeated visits cannot bypass the frequency cap."
        );
        promotions::visit(&app, &visitors[1], visit("mobile"))
            .await
            .unwrap()
            .unwrap();
        let other = App::open((*app.config).clone()).await.unwrap();
        let (a, b) = tokio::join!(
            promotions::claim(&app, &visitors[0], &id),
            promotions::claim(&other, &visitors[1], &id)
        );
        assert_eq!(
            usize::from(a.is_ok()) + usize::from(b.is_ok()),
            1,
            "Two real pools may allocate the last reward exactly once."
        );
        let (winner, receipt) = if let Ok(receipt) = a {
            (&visitors[0], receipt)
        } else {
            (&visitors[1], b.unwrap())
        };
        assert_eq!(
            promotions::claim(&app, winner, &id).await.unwrap(),
            receipt,
            "Interrupted draw recovery returns the same receipt without consuming stock again."
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT remaining FROM promotion_rewards")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            0
        );
        let mut gpc = winner.clone();
        gpc.insert("sec-gpc", "1".parse().unwrap());
        assert!(promotions::claim(&app, &gpc, &id).await.is_err());
        let archive = backup::capture(&app).await.unwrap();
        let (config, restore_schema) = database_config(&data, "offers_restored", engine).await;
        let restored = App::open(config).await.unwrap();
        backup::restore(&restored, &archive).await.unwrap();
        assert_eq!(
            promotions::claim(&restored, winner, &id).await.unwrap(),
            receipt,
            "Current-format fresh restore preserves recoverable receipt and inventory."
        );
        engagement::consent(&app, winner, false, false, policy)
            .await
            .unwrap();
        assert!(promotions::claim(&app, winner, &id).await.is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM promotion_claims")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            0,
            "Withdrawal removes the session-linked receipt."
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT issued FROM promotion_rewards")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            1,
            "Withdrawal does not silently replenish already-issued inventory."
        );
        for pool in [&other.db.pool, &restored.db.pool, &app.db.pool] {
            pool.close().await;
        }
        if let Some(url) = engine {
            let pool = sqlx::PgPool::connect(url).await.unwrap();
            for schema in [source_schema, restore_schema].into_iter().flatten() {
                sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            pool.close().await;
        }
    }
}

#[tokio::test]
async fn accepted_response_routes_once_creates_only_a_draft_and_requires_verified_approved_account()
{
    use sqlx::Row;
    use wpalt::{
        App, auth, backup,
        business::{
            forms::Condition,
            registration, store,
            workflows::{DraftPost, Notification},
        },
    };
    let postgres = std::env::var("TEST_DATABASE_URL").ok();
    if std::env::var("WPALT_REQUIRE_POSTGRES").is_ok() {
        assert!(postgres.is_some());
    }
    for engine in std::iter::once(None).chain(postgres.as_deref().map(Some)) {
        let data = tempfile::tempdir().unwrap();
        let (config, source_schema) = database_config(&data, "actions", engine).await;
        let app = App::open(config).await.unwrap();
        auth::initialize(
            &app,
            "owner@example.test",
            "Owner",
            "a strong local password",
        )
        .await
        .unwrap();
        let owner: String = sqlx::query_scalar("SELECT id FROM users WHERE role='admin'")
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        let mut form:FormDefinition=serde_json::from_value(json!({"title":"Contribute","fields":[{"name":"name","schema":{"kind":"string","required":true}},{"name":"email","schema":{"kind":"string","required":true},"widget":{"kind":"Email"}},{"name":"title","schema":{"kind":"string","required":true}},{"name":"body","schema":{"kind":"string","required":true}},{"name":"priority","schema":{"kind":"boolean","required":true}}]})).unwrap();
        form.notifications.push(Notification {
            recipient: "editor@example.test".into(),
            subject: "Review contribution".into(),
            document: wpalt::document::empty(),
            when: Some(Condition::Equal {
                field: "priority".into(),
                value: json!(true),
            }),
        });
        form.draft_post = Some(DraftPost {
            title_field: "title".into(),
            body_field: "body".into(),
        });
        form.registration = Some(registration::Action {
            email_field: "email".into(),
            name_field: "name".into(),
        });
        let id = store::create(&app, &owner, &form).await.unwrap();
        store::save(&app, &id, 1, &form, true).await.unwrap();
        let key = uuid::Uuid::new_v4().to_string();
        let values = json!({"name":"Applicant","email":"applicant@example.test","title":"Visitor contribution","body":"<script>private literal</script>","priority":true});
        let entry = store::submit(&app, &id, 2, &key, &values).await.unwrap();
        assert_eq!(
            store::submit(&app, &id, 2, &key, &values).await.unwrap(),
            entry
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM mail_jobs")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            2,
            "A proof and one matching fixed-recipient notification are queued atomically, even after a retry."
        );
        let post=sqlx::query("SELECT p.status,p.published_title,p.document FROM posts p JOIN form_contributions c ON c.post_id=p.id WHERE c.entry_id=$1").bind(&entry).fetch_one(&app.db.pool).await.unwrap();
        assert_eq!(post.get::<String, _>("status"), "draft");
        assert_eq!(post.get::<String, _>("published_title"), "");
        assert!(
            wpalt::document::Document::parse(&post.get::<String, _>("document"))
                .unwrap()
                .html()
                .contains("&lt;script&gt;")
        );
        let request = sqlx::query("SELECT id,version FROM registration_requests WHERE entry_id=$1")
            .bind(&entry)
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        let request_id: String = request.get("id");
        assert!(
            registration::decide(&app, &request_id, 1, true)
                .await
                .is_err(),
            "Approval cannot bypass mailbox proof."
        );
        let plain: String = sqlx::query_scalar("SELECT plain FROM mail_jobs WHERE dedupe=$1")
            .bind(format!("registration:{request_id}"))
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        let token = plain
            .split("/registration/")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        registration::review(&app, token).await.unwrap();
        assert!(
            auth::login(&app, "applicant@example.test", "applicant password")
                .await
                .is_err()
        );
        registration::verify(&app, token, "applicant password")
            .await
            .unwrap();
        assert!(
            registration::verify(&app, token, "different password")
                .await
                .is_err()
        );
        assert!(
            auth::login(&app, "applicant@example.test", "applicant password")
                .await
                .is_err(),
            "Mailbox proof alone does not approve an account."
        );
        let archive = backup::capture(&app).await.unwrap();
        let (config, restored_schema) = database_config(&data, "actions_restored", engine).await;
        let restored = App::open(config).await.unwrap();
        backup::restore(&restored, &archive).await.unwrap();
        registration::decide(&restored, &request_id, 2, true)
            .await
            .unwrap();
        let (_, subscriber) =
            auth::login(&restored, "applicant@example.test", "applicant password")
                .await
                .unwrap();
        assert_eq!(subscriber.user.role, "subscriber");
        assert!(!subscriber.can_edit() && !subscriber.can_moderate() && !subscriber.is_admin());
        assert!(
            registration::decide(&restored, &request_id, 2, true)
                .await
                .is_err()
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT password_hash FROM registration_requests WHERE id=$1"
            )
            .bind(&request_id)
            .fetch_one(&restored.db.pool)
            .await
            .unwrap(),
            "",
            "After approval no duplicate password hash remains in the request record."
        );
        for pool in [&restored.db.pool, &app.db.pool] {
            pool.close().await;
        }
        if let Some(url) = engine {
            let pool = sqlx::PgPool::connect(url).await.unwrap();
            for schema in [source_schema, restored_schema].into_iter().flatten() {
                sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            pool.close().await;
        }
    }
}

/// Admission bounds must be shared by independent servers, not process mutexes.
#[tokio::test]
async fn site_wide_admission_is_atomic_replayable_and_preserves_unicode_bytes() {
    use wpalt::{
        App, auth, backup,
        business::{store, workflows::Notification},
    };
    let postgres = std::env::var("TEST_DATABASE_URL").ok();
    if std::env::var("WPALT_REQUIRE_POSTGRES").is_ok() {
        assert!(postgres.is_some());
    }
    for engine in std::iter::once(None).chain(postgres.as_deref().map(Some)) {
        let data = tempfile::tempdir().unwrap();
        let (mut config, schema) = database_config(&data, "quotas", engine).await;
        config.business_limits.entries = 1;
        config.business_limits.mail_jobs = 1;
        let app = App::open(config).await.unwrap();
        auth::initialize(
            &app,
            "owner@example.test",
            "Owner",
            "a strong local password",
        )
        .await
        .unwrap();
        let owner: String = sqlx::query_scalar("SELECT id FROM users WHERE role='admin'")
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        let mut form = quotation_form();
        form.notifications.push(Notification {
            recipient: "editor@example.test".into(),
            subject: "Review".into(),
            document: wpalt::document::empty(),
            when: None,
        });
        let a = store::create(&app, &owner, &form).await.unwrap();
        let b = store::create(&app, &owner, &form).await.unwrap();
        store::save(&app, &a, 1, &form, true).await.unwrap();
        store::save(&app, &b, 1, &form, true).await.unwrap();
        let other = App::open((*app.config).clone()).await.unwrap();
        let key_a = uuid::Uuid::new_v4().to_string();
        let key_b = uuid::Uuid::new_v4().to_string();
        let values = json!({"business":true,"company":"参考会社","quantity":2,"rate":3});
        let (left, right) = tokio::join!(
            store::submit(&app, &a, 2, &key_a, &values),
            store::submit(&other, &b, 2, &key_b, &values)
        );
        assert_eq!(
            usize::from(left.is_ok()) + usize::from(right.is_ok()),
            1,
            "Two server pools cannot overrun one global response slot."
        );
        let (id, key, accepted) = if let Ok(entry) = left {
            (a, key_a, entry)
        } else {
            (b, key_b, right.unwrap())
        };
        assert_eq!(
            store::submit(&app, &id, 2, &key, &values).await.unwrap(),
            accepted,
            "A full site still acknowledges its original accepted retry."
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM mail_jobs")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            1,
            "The rejected response leaves no workflow mail."
        );
        let archive = backup::capture(&app).await.unwrap();
        let (restore_config, restore_schema) =
            database_config(&data, "quotas_restore", engine).await;
        let restored = App::open(restore_config).await.unwrap();
        backup::restore(&restored, &archive).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT items FROM business_usage WHERE kind='entries'")
                .fetch_one(&restored.db.pool)
                .await
                .unwrap(),
            1
        );
        for pool in [&restored.db.pool, &other.db.pool, &app.db.pool] {
            pool.close().await;
        }
        if let Some(url) = engine {
            let pool = sqlx::PgPool::connect(url).await.unwrap();
            for name in [schema, restore_schema].into_iter().flatten() {
                sqlx::query(&format!("DROP SCHEMA {name} CASCADE"))
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            pool.close().await;
        }
    }
}

/// Measures a populated local business system without fragile timing assertions.
#[tokio::test]
async fn populated_business_paths_have_bounded_queries_and_reproducible_measurements() {
    use sqlx::Row;
    use wpalt::{
        App, auth,
        business::{engagement, mail, store, workflows::Notification},
    };
    let postgres = std::env::var("TEST_DATABASE_URL").ok();
    if std::env::var("WPALT_REQUIRE_POSTGRES").is_ok() {
        assert!(postgres.is_some());
    }
    let mut measurements = Vec::new();
    for engine in std::iter::once(None).chain(postgres.as_deref().map(Some)) {
        let data = tempfile::tempdir().unwrap();
        let (config, schema) = database_config(&data, "business_volume", engine).await;
        let app = App::open(config).await.unwrap();
        auth::initialize(
            &app,
            "owner@example.test",
            "Owner",
            "a strong local password",
        )
        .await
        .unwrap();
        let owner: String = sqlx::query_scalar("SELECT id FROM users WHERE role='admin'")
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        let mut form:FormDefinition=serde_json::from_value(json!({"title":"Measured response","fields":[{"name":"message","schema":{"kind":"string","required":true}}]})).unwrap();
        form.notifications.push(Notification {
            recipient: "fixture@example.test".into(),
            subject: "A local response".into(),
            document: wpalt::document::empty(),
            when: None,
        });
        let id = store::create(&app, &owner, &form).await.unwrap();
        store::save(&app, &id, 1, &form, true).await.unwrap();
        let mut times = Vec::new();
        for index in 0..1000 {
            let started = std::time::Instant::now();
            store::submit(
                &app,
                &id,
                2,
                &uuid::Uuid::new_v4().to_string(),
                &json!({"message":format!("参考 local response {index}")}),
            )
            .await
            .unwrap();
            times.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        sqlx::query("UPDATE engagement_settings SET enabled=1 WHERE id=1")
            .execute(&app.db.pool)
            .await
            .unwrap();
        for _ in 0..5 {
            let token = engagement::consent(&app, &axum::http::HeaderMap::new(), true, false, 1)
                .await
                .unwrap()
                .unwrap();
            let mut h = axum::http::HeaderMap::new();
            h.insert("cookie", format!("wpalt_visitor={token}").parse().unwrap());
            for _ in 0..400 {
                assert!(
                    engagement::capture(
                        &app,
                        &h,
                        engagement::Capture {
                            id: uuid::Uuid::new_v4().to_string(),
                            name: "pageview".into(),
                            path: "/".into(),
                            dimensions: Default::default(),
                            frame: None
                        }
                    )
                    .await
                    .unwrap()
                );
            }
        }
        let mut plans = serde_json::Map::new();
        for (name, sql) in [
            (
                "entries",
                "SELECT id,created_at FROM form_entries WHERE form_id=$1 ORDER BY created_at DESC,id DESC LIMIT 40",
            ),
            (
                "events",
                "SELECT name,path,COUNT(*) FROM engagement_events WHERE created_at>=$1 GROUP BY name,path LIMIT 50",
            ),
            (
                "mail",
                "SELECT id FROM mail_jobs WHERE state IN ('pending','retry') AND next_at<=$1 ORDER BY next_at,id LIMIT 64",
            ),
        ] {
            let prefix = if app.db.postgres {
                "EXPLAIN "
            } else {
                "EXPLAIN QUERY PLAN "
            };
            let statement = format!("{prefix}{sql}");
            let query = sqlx::query(&statement);
            let rows = if name == "entries" {
                query.bind(&id).fetch_all(&app.db.pool).await.unwrap()
            } else {
                query
                    .bind(wpalt::now() - 86400)
                    .fetch_all(&app.db.pool)
                    .await
                    .unwrap()
            };
            plans.insert(
                name.into(),
                json!(
                    rows.iter()
                        .map(|r| if app.db.postgres {
                            r.get::<String, _>(0)
                        } else {
                            r.get::<String, _>("detail")
                        })
                        .collect::<Vec<_>>()
                ),
            );
        }
        let started = std::time::Instant::now();
        let mut delivered = 0;
        while delivered < 1000 {
            let batch = mail::tick(&app).await.unwrap();
            assert!(batch > 0 && batch <= 64);
            delivered += batch;
        }
        let spool_seconds = started.elapsed().as_secs_f64();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM mail_jobs WHERE state='spooled'")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            1000
        );
        times.sort_by(f64::total_cmp);
        measurements.push(json!({"engine":if app.db.postgres{"postgres"}else{"sqlite"},"conditions":"Integration API, debug test binary, 1000 responses and fixed notification jobs, 2000 consented events; no timing assertions; not HTTP or production capacity","responses":1000,"events":2000,"submission_p50_ms":times[499],"submission_p95_ms":times[949],"local_spool_jobs_per_second":1000.0/spool_seconds,"plans":plans}));
        app.db.pool.close().await;
        if let Some(url) = engine {
            let pool = sqlx::PgPool::connect(url).await.unwrap();
            sqlx::query(&format!("DROP SCHEMA {} CASCADE", schema.unwrap()))
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }
    }
    std::fs::create_dir_all("work").unwrap();
    std::fs::write(
        "work/m4-volume.json",
        serde_json::to_vec_pretty(&measurements).unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn confirmed_subscribers_receive_matching_trigger_once_and_withdrawal_cancels_it() {
    use wpalt::{
        App, auth,
        business::{audience, campaigns, store},
    };
    let postgres = std::env::var("TEST_DATABASE_URL").ok();
    if std::env::var("WPALT_REQUIRE_POSTGRES").is_ok() {
        assert!(postgres.is_some());
    }
    for engine in std::iter::once(None).chain(postgres.as_deref().map(Some)) {
        let data = tempfile::tempdir().unwrap();
        let (config, schema) = database_config(&data, "trigger", engine).await;
        let app = App::open(config).await.unwrap();
        auth::initialize(
            &app,
            "owner@example.test",
            "Owner",
            "a strong local password",
        )
        .await
        .unwrap();
        let owner: String = sqlx::query_scalar("SELECT id FROM users WHERE role='admin'")
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        let list = audience::create_list(&app, "News", "Send news", "1")
            .await
            .unwrap();
        let campaign = campaigns::create(&app, "Welcome", &list).await.unwrap();
        campaigns::save_with_trigger(
            &app,
            &campaign,
            1,
            "Welcome",
            &wpalt::document::empty(),
            0,
            audience::Segment::default(),
            Some(true),
        )
        .await
        .unwrap();
        let excluded = campaigns::create(&app, "Other segment", &list)
            .await
            .unwrap();
        campaigns::save_with_trigger(
            &app,
            &excluded,
            1,
            "Excluded",
            &wpalt::document::empty(),
            0,
            audience::Segment {
                company: Some("Other".into()),
                ..Default::default()
            },
            Some(true),
        )
        .await
        .unwrap();
        let form:FormDefinition=serde_json::from_value(json!({"title":"Subscribe","fields":[{"name":"email","schema":{"kind":"string","required":true}},{"name":"consent","schema":{"kind":"boolean"}}],"subscription":{"list":list,"policy":"1","email_field":"email","consent_field":"consent"}})).unwrap();
        let id = store::create(&app, &owner, &form).await.unwrap();
        store::save(&app, &id, 1, &form, true).await.unwrap();
        store::submit(
            &app,
            &id,
            2,
            &uuid::Uuid::new_v4().to_string(),
            &json!({"email":"trigger@example.test","consent":true}),
        )
        .await
        .unwrap();
        let plain: String =
            sqlx::query_scalar("SELECT plain FROM mail_jobs WHERE kind='confirmation'")
                .fetch_one(&app.db.pool)
                .await
                .unwrap();
        let token = plain
            .split("/audience/confirm/")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        use tower::ServiceExt;
        for site in [
            None,
            Some("cross-site"),
            Some("same-site"),
            Some("same-origin"),
        ] {
            let mut request = axum::http::Request::builder()
                .method("POST")
                .uri(format!("/audience/confirm/{token}"))
                .header("origin", "null")
                .header("sec-fetch-mode", "navigate")
                .header("sec-fetch-dest", "document");
            if let Some(site) = site {
                request = request.header("sec-fetch-site", site);
            }
            let response = wpalt::web::router(app.clone())
                .oneshot(request.body(axum::body::Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                if site == Some("same-origin") {
                    axum::http::StatusCode::OK
                } else {
                    axum::http::StatusCode::FORBIDDEN
                },
                "No-referrer proof POSTs fail closed without exact same-origin browser metadata."
            );
        }
        let response = wpalt::web::router(app.clone())
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header("origin", "null")
                    .header("sec-fetch-site", "same-origin")
                    .header("sec-fetch-mode", "navigate")
                    .header("sec-fetch-dest", "document")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::FORBIDDEN,
            "The capability exception cannot weaken account/session routes."
        );
        audience::decide(&app, token, false).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM mail_jobs WHERE kind='campaign'")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            1,
            "Only the matching template runs; a confirmation replay cannot duplicate its mail."
        );
        let plain: String = sqlx::query_scalar("SELECT plain FROM mail_jobs WHERE kind='campaign'")
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        let withdrawal = plain
            .split("/audience/withdraw/")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        audience::decide(&app, withdrawal, true).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT state FROM mail_jobs WHERE kind='campaign'")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            "cancelled"
        );
        app.db.pool.close().await;
        if let Some(url) = engine {
            let pool = sqlx::PgPool::connect(url).await.unwrap();
            sqlx::query(&format!("DROP SCHEMA {} CASCADE", schema.unwrap()))
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }
    }
}
