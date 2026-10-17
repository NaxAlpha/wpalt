//! D10 scenarios preserve canonical branching/calculation authority.
use serde_json::{Value, json};
use wpalt::{business::forms::FormDefinition, schema::Definition};

fn calculator(expression: Value) -> FormDefinition {
    serde_json::from_value(json!({"title":"Conditional quotation","fields":[
        {"name":"business","schema":{"kind":"boolean","required":true}},
        {"name":"quantity","schema":{"kind":"number","required":true}},
        {"name":"rate","schema":{"kind":"number","required":true}},
        {"name":"company","schema":{"kind":"string","required":true},"visible_when":{
            "operation":"All","conditions":[{"operation":"Equal","field":"business","value":true},
                {"operation":"Any","conditions":[{"operation":"AtLeast","field":"quantity","value":10},
                    {"operation":"OneOf","field":"rate","values":[5,12.5]}]}]}},
        {"name":"total","schema":{"kind":"number","required":true},"calculation":{"operation":"Expression","expression":expression}}
    ]})).unwrap()
}

#[test]
fn branching_calculators_discard_forgery_and_evaluate_only_the_selected_branch() {
    let expression = json!({"operation":"If","condition":{"operation":"Present","field":"company"},
        "then":{"operation":"Round","decimals":2,"operand":{"operation":"Divide",
            "left":{"operation":"Multiply","operands":[{"operation":"Field","field":"quantity"},{"operation":"Field","field":"rate"}]},
            "right":{"operation":"Literal","value":3}}},
        "otherwise":{"operation":"Literal","value":-2.5}});
    let form = calculator(expression);
    let common = Definition::default();
    assert_eq!(
        form.evaluate(
            &common,
            &json!({"business":true,"quantity":2,"rate":12.5,"company":"Local","total":999}),
            false
        )
        .unwrap()["total"],
        8.33
    );
    let hidden = form
        .evaluate(
            &common,
            &json!({"business":false,"quantity":2,"rate":12.5,"company":"forged","total":999}),
            false,
        )
        .unwrap();
    assert!(hidden.get("company").is_none());
    assert_eq!(hidden["total"], -2.5);
    assert!(
        form.evaluate(
            &common,
            &json!({"business":true,"quantity":2,"rate":12.5}),
            false
        )
        .is_err(),
        "Visible required fields are still required"
    );
    let lazy = calculator(
        json!({"operation":"If","condition":{"operation":"Equal","field":"business","value":false},
        "then":{"operation":"Literal","value":7},"otherwise":{"operation":"Divide","left":{"operation":"Literal","value":1},"right":{"operation":"Literal","value":0}}}),
    );
    assert_eq!(
        lazy.evaluate(
            &common,
            &json!({"business":false,"quantity":2,"rate":1}),
            false
        )
        .unwrap()["total"],
        7.0
    );
    assert!(
        lazy.evaluate(
            &common,
            &json!({"business":true,"quantity":2,"rate":1}),
            true
        )
        .is_err(),
        "Partial drafts cannot store division by zero"
    );
}

#[test]
fn numeric_boundaries_missing_values_and_rule_budgets_refuse_unsafe_declarations() {
    let common = Definition::default();
    let rounded = calculator(
        json!({"operation":"Round","decimals":0,"operand":{"operation":"Literal","value":-2.5}}),
    );
    assert_eq!(
        rounded
            .evaluate(
                &common,
                &json!({"business":false,"quantity":1,"rate":1}),
                false
            )
            .unwrap()["total"],
        -3.0,
        "Declared ties-away policy differs intentionally from JS Math.round"
    );
    for value in [0.49999999999999994, -0.49999999999999994] {
        let below_tie = calculator(
            json!({"operation":"Round","decimals":0,"operand":{"operation":"Literal","value":value}}),
        );
        assert_eq!(
            below_tie
                .evaluate(
                    &common,
                    &json!({"business":false,"quantity":1,"rate":1}),
                    false
                )
                .unwrap()["total"]
                .as_f64(),
            Some(0.0),
            "A representable value below the rounding tie must remain below it"
        );
    }
    let numeric_equal = calculator(
        json!({"operation":"If","condition":{"operation":"Equal","field":"quantity","value":2.0},"then":{"operation":"Literal","value":7},"otherwise":{"operation":"Literal","value":9}}),
    );
    assert_eq!(
        numeric_equal
            .evaluate(
                &common,
                &json!({"business":false,"quantity":2,"rate":1}),
                false
            )
            .unwrap()["total"],
        7.0,
        "Numeric equality agrees across integer JSON and binary64 literals"
    );
    let missing = calculator(json!({"operation":"Field","field":"rate"}));
    assert!(
        missing
            .evaluate(&common, &json!({"business":false,"quantity":1}), true)
            .unwrap()
            .get("total")
            .is_none(),
        "Missing never means zero"
    );
    assert!(
        missing
            .evaluate(&common, &json!({"business":false,"quantity":1}), false)
            .is_err()
    );
    let overflow = calculator(
        json!({"operation":"Multiply","operands":[{"operation":"Literal","value":1e308},{"operation":"Literal","value":2}]}),
    );
    assert!(
        overflow
            .evaluate(
                &common,
                &json!({"business":false,"quantity":1,"rate":1}),
                false
            )
            .is_err()
    );
    for expression in [
        json!({"operation":"Field","field":"total"}),
        json!({"operation":"Field","field":"company"}),
        json!({"operation":"Score","field":"rate"}),
        json!({"operation":"Min","operands":[]}),
        json!({"operation":"Round","operand":{"operation":"Literal","value":1},"decimals":9}),
    ] {
        assert!(calculator(expression).validate(&common).is_err());
    }
    let mut deep = json!({"operation":"Literal","value":1});
    for _ in 0..8 {
        deep = json!({"operation":"Abs","operand":deep});
    }
    assert!(calculator(deep).validate(&common).is_err());
    let wide = json!({"operation":"Add","operands":(0..5).map(|_|json!({"operation":"Add","operands":(0..32).map(|_|json!({"operation":"Literal","value":1})).collect::<Vec<_>>()})).collect::<Vec<_>>()});
    assert!(calculator(wide).validate(&common).is_err());
    let mut invalid = missing.clone();
    invalid.fields[3].visible_when = Some(
        serde_json::from_value(json!({"operation":"Greater","field":"business","value":1}))
            .unwrap(),
    );
    assert!(invalid.validate(&common).is_err());
}

#[tokio::test]
async fn versioned_survey_reports_preserve_branch_denominators_and_canonical_notifications() {
    use sqlx::Row;
    use wpalt::{
        App, auth,
        business::{store, surveys},
    };
    let postgres = std::env::var("TEST_DATABASE_URL").ok();
    if std::env::var("WPALT_REQUIRE_POSTGRES").is_ok() {
        assert!(postgres.is_some());
    }
    let key_pattern = regex::Regex::new(r#"name="key" value="([^"]+)""#).unwrap();
    for engine in std::iter::once(None).chain(postgres.as_deref().map(Some)) {
        let (app, _directory, schema) = fixture(engine).await;
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
        let mut definition = calculator(
            json!({"operation":"Multiply","operands":[{"operation":"Field","field":"quantity"},{"operation":"Field","field":"rate"}]}),
        );
        definition.fields.push(serde_json::from_value(json!({"name":"rating","schema":{"kind":"string","label":"Original rating"},"widget":{"kind":"Choice","options":[{"label":"Helpful","value":"good","score":5},{"label":"Needs work","value":"bad","score":1}]}})).unwrap());
        definition.notifications.push(serde_json::from_value(json!({"recipient":"owner@example.test","subject":"Qualified response","document":wpalt::document::empty(),"when":{"operation":"All","conditions":[{"operation":"Present","field":"company"},{"operation":"Greater","field":"total","value":10}]}})).unwrap());
        let form = store::create(&app, &owner, &definition).await.unwrap();
        store::save(&app, &form, 1, &definition, true)
            .await
            .unwrap();
        for input in [
            json!({"business":false,"quantity":2,"rate":5,"company":"forged","total":999,"rating":"good"}),
            json!({"business":true,"quantity":4,"rate":5,"company":"Local","total":0}),
        ] {
            store::submit(&app, &form, 2, &uuid::Uuid::new_v4().to_string(), &input)
                .await
                .unwrap();
        }
        let first = surveys::report(&app, &form, 2).await.unwrap();
        assert_eq!(first.responses, 2);
        let company = first
            .fields
            .iter()
            .find(|field| field.name == "company")
            .unwrap();
        assert_eq!(
            (
                company.eligible,
                company.answered,
                company.unanswered,
                company.skipped
            ),
            (1, 1, 0, 1)
        );
        let rating = first
            .fields
            .iter()
            .find(|field| field.name == "rating")
            .unwrap();
        assert_eq!(
            (
                rating.eligible,
                rating.answered,
                rating.unanswered,
                rating.skipped
            ),
            (2, 1, 1, 0)
        );
        assert_eq!(rating.choices[0].responses, 1);
        assert_eq!(
            first
                .fields
                .iter()
                .find(|field| field.name == "total")
                .unwrap()
                .numeric
                .as_ref()
                .unwrap()
                .mean,
            15.0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM mail_jobs WHERE kind='notification'"
            )
            .fetch_one(&app.db.pool)
            .await
            .unwrap(),
            1,
            "Routing uses canonical total and hidden-field suppression"
        );
        definition.fields.last_mut().unwrap().schema.label = "Changed question".into();
        if let Some(wpalt::business::forms::Widget::Choice { options }) =
            &mut definition.fields.last_mut().unwrap().widget
        {
            options[0].label = "Different meaning".into();
            options[0].score = 9;
        }
        store::save(&app, &form, 2, &definition, true)
            .await
            .unwrap();
        store::submit(
            &app,
            &form,
            3,
            &uuid::Uuid::new_v4().to_string(),
            &json!({"business":false,"quantity":1,"rate":1,"rating":"good"}),
        )
        .await
        .unwrap();
        let historical = surveys::report(&app, &form, 2).await.unwrap();
        let current = surveys::report(&app, &form, 3).await.unwrap();
        assert_eq!(historical.responses, 2);
        assert_eq!(historical.fields.last().unwrap().label, "Original rating");
        assert_eq!(
            historical.fields.last().unwrap().choices[0].label,
            "Helpful"
        );
        assert_eq!(current.responses, 1);
        assert_eq!(current.fields.last().unwrap().label, "Changed question");
        assert_eq!(current.fields.last().unwrap().choices[0].score, Some(9));
        assert_ne!(current.definition_sha256, historical.definition_sha256);
        assert!(surveys::report(&app, &form, 999).await.is_err());
        let time: i64 = sqlx::query("SELECT created_at FROM form_entries WHERE form_version=3")
            .fetch_one(&app.db.pool)
            .await
            .unwrap()
            .get("created_at");
        assert_eq!(
            surveys::report_range(&app, &form, 3, time + 1, time + 2)
                .await
                .unwrap()
                .responses,
            0
        );
        assert!(surveys::report_range(&app, &form, 3, 2, 1).await.is_err());
        let serialized = serde_json::to_string(&historical).unwrap();
        assert!(
            !serialized.contains("Local") && !serialized.contains("owner@example"),
            "Reports expose aggregate counts, not private answers/actions"
        );
        // The native response path uses the same calculator and never persists
        // a forged hidden field or visitor-provided total.
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;
        let router = wpalt::web::router(app.clone());
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/forms/{form}/native"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body = String::from_utf8(
            axum::body::to_bytes(response.into_body(), 128 * 1024)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        let key = key_pattern.captures(&body).unwrap()[1].to_owned();
        let mut payload = url::form_urlencoded::Serializer::new(String::new());
        for (name, value) in [
            ("key", key.as_str()),
            ("version", "3"),
            ("action", "submit"),
            ("value_quantity", "2"),
            ("value_rate", "5"),
            ("value_company", "forged"),
            ("value_total", "999"),
        ] {
            payload.append_pair(name, value);
        }
        let payload = payload.finish();
        for _ in 0..2 {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/forms/{form}/native"))
                        .header("origin", app.config.origin())
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from(payload.clone()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let html = String::from_utf8(
                axum::body::to_bytes(response.into_body(), 128 * 1024)
                    .await
                    .unwrap()
                    .to_vec(),
            )
            .unwrap();
            assert!(html.contains("Your response has been received"));
        }
        assert_eq!(
            surveys::report(&app, &form, 3).await.unwrap().responses,
            2,
            "An accepted native retry cannot duplicate a response"
        );
        store::save(&app, &form, 3, &definition, true)
            .await
            .unwrap();
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/forms/{form}/native"))
                    .header("origin", app.config.origin())
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(payload))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let html = String::from_utf8(
            axum::body::to_bytes(response.into_body(), 128 * 1024)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(
            html.contains("Your response has been received"),
            "Accepted exact retry still works after publication changes"
        );
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/admin/forms/{form}/report?version=3"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let (token, session) = auth::login(&app, "owner@example.test", "a strong local password")
            .await
            .unwrap();
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/admin/forms/{form}/report?version=3"))
                    .header("cookie", format!("wpalt_session={token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let preview_response = router.clone().oneshot(
            Request::builder().method("POST").uri(format!("/api/admin/forms/{form}/preview"))
            .header("cookie", format!("wpalt_session={token}"))
            .header("origin", app.config.origin()).header("content-type", "application/json")
            .body(Body::from(json!({"csrf":session.csrf,"definition":definition,"values":{"business":false,"quantity":3,"rate":4,"total":999,"company":"hidden"}}).to_string())).unwrap()
        ).await.unwrap();
        assert_eq!(preview_response.status(), StatusCode::OK);
        let preview: Value = serde_json::from_slice(
            &axum::body::to_bytes(preview_response.into_body(), 128 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(preview["values"]["total"], 12.0);
        assert!(preview["values"].get("company").is_none());
        assert_eq!(
            surveys::report(&app, &form, 4).await.unwrap().responses,
            0,
            "Working preview must not collect data"
        );
        let csv_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/admin/forms/{form}/report.csv?version=2"))
                    .header("cookie", format!("wpalt_session={token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(csv_response.status(), StatusCode::OK);
        assert_eq!(csv_response.headers()["cache-control"], "no-store");
        let csv = String::from_utf8(
            axum::body::to_bytes(csv_response.into_body(), 128 * 1024)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(csv.contains(&historical.definition_sha256) && !csv.contains("Local"));
        // Each publication transition invalidates link availability; private draft
        // edits and accepted response counters have no effect on rendered sources.
        let (discovery, _) = wpalt::discovery::load(&app).await.unwrap();
        let epoch = wpalt::discovery::insights::epoch::current(&app)
            .await
            .unwrap();
        for path in [format!("/forms/{form}"), format!("/forms/{form}/native")] {
            let response = router
                .clone()
                .oneshot(Request::builder().uri(&path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.headers()["referrer-policy"],
                if path.ends_with("/native") {
                    "strict-origin"
                } else {
                    "no-referrer"
                }
            );
            assert!(matches!(
                wpalt::discovery::redirects::route(&app, &discovery, &path)
                    .await
                    .unwrap(),
                wpalt::discovery::redirects::RouteState::Public(None)
            ));
        }
        store::save(&app, &form, 4, &definition, false)
            .await
            .unwrap();
        assert_eq!(
            wpalt::discovery::insights::epoch::current(&app)
                .await
                .unwrap(),
            epoch
        );
        store::save(&app, &form, 5, &definition, true)
            .await
            .unwrap();
        assert!(
            wpalt::discovery::insights::epoch::current(&app)
                .await
                .unwrap()
                > epoch
        );
        let draft = store::create(&app, &owner, &definition).await.unwrap();
        assert!(matches!(
            wpalt::discovery::redirects::route(&app, &discovery, &format!("/forms/{draft}/native"))
                .await
                .unwrap(),
            wpalt::discovery::redirects::RouteState::Missing
        ));
        sqlx::query("UPDATE users SET role='subscriber' WHERE id=$1")
            .bind(&owner)
            .execute(&app.db.pool)
            .await
            .unwrap();
        for path in [
            format!("/api/admin/forms/{form}/report?version=3"),
            format!("/admin/forms/{form}/report"),
            format!("/admin/forms/{form}/report.csv"),
        ] {
            let denied = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header("cookie", format!("wpalt_session={token}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                denied.status(),
                StatusCode::FORBIDDEN,
                "The current role controls every report disclosure"
            );
        }
        sqlx::query("UPDATE users SET role='admin' WHERE id=$1")
            .bind(&owner)
            .execute(&app.db.pool)
            .await
            .unwrap();
        let numeric:FormDefinition=serde_json::from_value(json!({"title":"Native numeric notation","fields":[{"name":"count","schema":{"kind":"number","required":true}}]})).unwrap();
        let numeric_id = store::create(&app, &owner, &numeric).await.unwrap();
        store::save(&app, &numeric_id, 1, &numeric, true)
            .await
            .unwrap();
        for (raw, accepted) in [("inf", false), ("1e309", false), ("01.0", true)] {
            let mut input = url::form_urlencoded::Serializer::new(String::new());
            input
                .append_pair("version", "2")
                .append_pair("action", "submit")
                .append_pair("key", &uuid::Uuid::new_v4().to_string())
                .append_pair("value_count", raw);
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/forms/{numeric_id}/native"))
                        .header("origin", app.config.origin())
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from(input.finish()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = String::from_utf8(
                axum::body::to_bytes(response.into_body(), 128 * 1024)
                    .await
                    .unwrap()
                    .to_vec(),
            )
            .unwrap();
            assert_eq!(body.contains("Your response has been received"), accepted);
            if !accepted {
                assert!(
                    body.contains(raw) && body.contains("finite numbers"),
                    "Invalid numeric work stays visible without a response"
                );
            }
        }
        let value: String =
            sqlx::query_scalar("SELECT values_json FROM form_entries WHERE form_id=$1")
                .bind(&numeric_id)
                .fetch_one(&app.db.pool)
                .await
                .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&value).unwrap()["count"].as_f64(),
            Some(1.0),
            "Native numeric notation preserves the number authority used by browser JSON"
        );
        let mut blocked_config = (*app.config).clone();
        blocked_config.spam.enabled = true;
        let blocked = App::open(blocked_config).await.unwrap();
        let response = wpalt::web::router(blocked.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/forms/{form}/native"))
                    .header("origin", app.config.origin())
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "version=6&key=00000000-0000-4000-8000-000000000000&action=submit",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "A native fallback cannot bypass enabled proof-of-work policy"
        );
        blocked.db.pool.close().await;
        let archive = wpalt::backup::capture(&app).await.unwrap();
        assert_eq!(
            wpalt::backup::inspect(&app.config, &archive).unwrap()["schema"],
            20
        );
        let mut envelope: Value = serde_json::from_slice(&archive).unwrap();
        let mut graph: Value = serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
        graph["tables"]
            .as_object_mut()
            .unwrap()
            .remove("form_recovery_policies");
        graph["tables"]
            .as_object_mut()
            .unwrap()
            .remove("form_recoveries");
        graph["schema"] = json!(18);
        let graph = serde_json::to_string(&graph).unwrap();
        envelope["format"] = json!("wpalt-backup-v18");
        envelope["sha256"] = json!(wpalt::auth::digest(graph.as_bytes()));
        envelope["payload"] = json!(graph);
        assert!(
            wpalt::backup::migrate_v18(&app.config, &serde_json::to_vec(&envelope).unwrap())
                .is_err(),
            "New expression grammar cannot masquerade as original v18"
        );
        let restored_directory = tempfile::tempdir().unwrap();
        let mut restored_config = (*app.config).clone();
        restored_config.data_dir = restored_directory.path().join("data");
        restored_config.database_url = format!(
            "sqlite://{}?mode=rwc",
            restored_directory.path().join("restored.db").display()
        );
        let restored = App::open(restored_config).await.unwrap();
        wpalt::backup::restore(&restored, &archive).await.unwrap();
        let restored_report = surveys::report(&restored, &form, 2).await.unwrap();
        assert_eq!(
            serde_json::to_value(restored_report).unwrap(),
            serde_json::to_value(&historical).unwrap(),
            "A fresh portable restore preserves frozen conditions, scores and derived canonical answers"
        );
        restored.db.pool.close().await;
        app.db.pool.close().await;
        if let (Some(root), Some(name)) = (engine, schema) {
            let pool = sqlx::PgPool::connect(root).await.unwrap();
            sqlx::query(&format!("DROP SCHEMA {name} CASCADE"))
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }
    }
}

async fn fixture(engine: Option<&str>) -> (wpalt::App, tempfile::TempDir, Option<String>) {
    let directory = tempfile::tempdir().unwrap();
    let mut config = wpalt::config::Config {
        data_dir: directory.path().join("data"),
        database_url: format!(
            "sqlite://{}?mode=rwc",
            directory.path().join("site.db").display()
        ),
        database_connections: 1,
        ..Default::default()
    };
    let mut schema = None;
    if let Some(root) = engine {
        let name = format!("wpalt_d10_{}", uuid::Uuid::new_v4().simple());
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
    let app = wpalt::App::open(config).await.unwrap();
    (app, directory, schema)
}

#[tokio::test]
async fn reporting_reads_committed_cohorts_and_refuses_large_or_noncanonical_storage() {
    use wpalt::{
        auth,
        business::{store, surveys},
    };
    let postgres = std::env::var("TEST_DATABASE_URL").ok();
    if std::env::var("WPALT_REQUIRE_POSTGRES").is_ok() {
        assert!(postgres.is_some());
    }
    for engine in std::iter::once(None).chain(postgres.as_deref().map(Some)) {
        let (app, _directory, schema) = fixture(engine).await;
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
        let mut definition:FormDefinition=serde_json::from_value(json!({"title":"Stored cohort authority","max_entries":20000,"fields":[
            {"name":"quantity","schema":{"kind":"number","required":true}},
            {"name":"total","schema":{"kind":"number","required":true},"calculation":{"operation":"Expression","expression":{"operation":"Multiply","operands":[{"operation":"Field","field":"quantity"},{"operation":"Literal","value":2}]}}}
        ]})).unwrap();
        for index in 1..=5 {
            definition.fields.push(serde_json::from_value(json!({"name":format!("note{index}"),"schema":{"kind":"string","max_length":8000}})).unwrap());
        }
        let form = store::create(&app, &owner, &definition).await.unwrap();
        store::save(&app, &form, 1, &definition, true)
            .await
            .unwrap();
        store::submit(
            &app,
            &form,
            2,
            &uuid::Uuid::new_v4().to_string(),
            &json!({"quantity":1}),
        )
        .await
        .unwrap();
        // Explicit storage scale fixture: canonical rows only, not claimed as
        // HTTP/action/quotas throughput evidence (the benchmark covers that).
        let raw = json!({"quantity":1.0,"total":2.0}).to_string();
        for start in (1..10001).step_by(250) {
            let end = (start + 250).min(10001);
            let rows = (start..end)
                .map(|_| {
                    (
                        uuid::Uuid::new_v4().to_string(),
                        uuid::Uuid::new_v4().to_string(),
                    )
                })
                .collect::<Vec<_>>();
            let tuples = (0..rows.len())
                .map(|index| {
                    format!(
                        "({})",
                        (1..=7)
                            .map(|column| format!("${}", index * 7 + column))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let statement = format!(
                "INSERT INTO form_entries(id,form_id,request_key,request_hash,form_version,values_json,created_at) VALUES {tuples}"
            );
            let mut batch = sqlx::query(&statement);
            for (id, key) in &rows {
                batch = batch
                    .bind(id)
                    .bind(&form)
                    .bind(key)
                    .bind("0".repeat(64))
                    .bind(2_i64)
                    .bind(&raw)
                    .bind(1_i64);
            }
            batch.execute(&app.db.pool).await.unwrap();
        }
        let too_large = surveys::report(&app, &form, 2).await.unwrap_err();
        assert!(
            too_large.1.contains("10,000"),
            "An admitted record must never silently truncate a report"
        );
        let narrow = surveys::report_range(&app, &form, 2, 2, i64::MAX)
            .await
            .unwrap();
        assert_eq!(
            narrow.responses, 1,
            "The time cohort bounds work through the same index"
        );
        sqlx::query("DELETE FROM form_entries WHERE created_at=1 AND id NOT IN (SELECT id FROM form_entries WHERE form_id=$1 AND created_at=1 ORDER BY id LIMIT 1000)").bind(&form).execute(&app.db.pool).await.unwrap();
        // One independent writer is deliberately held between writes. A real
        // report must complete from committed rows instead of mixing revisions.
        let writer = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect(&app.config.database_url)
            .await
            .unwrap();
        let mut tx = writer.begin().await.unwrap();
        sqlx::query("UPDATE form_entries SET values_json=$1 WHERE form_id=$2 AND created_at=1")
            .bind(json!({"quantity":2.0,"total":4.0}).to_string())
            .bind(&form)
            .execute(&mut *tx)
            .await
            .unwrap();
        let before = surveys::report_range(&app, &form, 2, 0, 2).await.unwrap();
        assert_eq!(before.responses, 1000);
        assert_eq!(before.fields[1].numeric.as_ref().unwrap().mean, 2.0);
        tx.commit().await.unwrap();
        let after = surveys::report_range(&app, &form, 2, 0, 2).await.unwrap();
        assert_eq!(after.fields[1].numeric.as_ref().unwrap().mean, 4.0);
        sqlx::query("UPDATE form_entries SET values_json=$1 WHERE form_id=$2 AND created_at=1")
            .bind(json!({"quantity":2.0,"total":999.0}).to_string())
            .bind(&form)
            .execute(&app.db.pool)
            .await
            .unwrap();
        assert!(
            surveys::report(&app, &form, 2)
                .await
                .unwrap_err()
                .1
                .contains("canonical"),
            "Persisted forged totals cannot become a plausible partial summary"
        );
        let mut large = json!({"quantity":2.0,"total":4.0});
        for index in 1..=5 {
            large[format!("note{index}")] = json!("x".repeat(8000));
        }
        sqlx::query("UPDATE form_entries SET values_json=$1 WHERE form_id=$2")
            .bind(large.to_string())
            .bind(&form)
            .execute(&app.db.pool)
            .await
            .unwrap();
        assert!(
            surveys::report(&app, &form, 2)
                .await
                .unwrap_err()
                .1
                .contains("32 MiB"),
            "A smaller record cohort can still exceed the canonical value byte budget"
        );
        writer.close().await;
        app.db.pool.close().await;
        if let (Some(root), Some(name)) = (engine, schema) {
            let pool = sqlx::PgPool::connect(root).await.unwrap();
            sqlx::query(&format!("DROP SCHEMA {name} CASCADE"))
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }
    }
}
