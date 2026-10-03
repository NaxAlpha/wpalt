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
        let (a, b) = tokio::join!(mail::tick(&app), mail::tick(&other));
        assert_eq!(
            a.unwrap() + b.unwrap(),
            1,
            "Independent workers claim a job once."
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
