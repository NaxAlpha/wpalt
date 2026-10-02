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
