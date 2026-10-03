//! M5 connected owner/member journeys, sharing the real installation/router harness.
use super::*;
use wpalt::membership::{self as m, Course, Lesson, Question};
async fn member(site: &Site, email: &str) -> (String, Session) {
    auth::add_user(&site.app, email, "Learner", "subscriber", PASSWORD)
        .await
        .unwrap();
    auth::login(&site.app, email, PASSWORD).await.unwrap()
}
async fn publish(site: &Site, slug: &str, body: &str) -> String {
    let mut p = input(slug, "publish");
    p.body = body.into();
    p.title = format!("Private {slug}");
    content::save(&site.app, site.session(), None, p)
        .await
        .unwrap()
        .id
}
fn lesson(post: String, title: &str) -> Lesson {
    Lesson {
        id: uuid::Uuid::new_v4().to_string(),
        title: title.into(),
        post_id: post,
        downloads: vec![],
        delay_seconds: 0,
        opens_at: 0,
        questions: vec![],
        assignment: String::new(),
        pass_percent: 70,
        max_attempts: 3,
    }
}

#[tokio::test]
async fn membership_policy_protects_every_delivery_surface_and_exact_time_boundaries() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let (token, learner) = member(&site, "first@example.test").await;
        let (other, outsider) = member(&site, "second@example.test").await;
        let id = publish(&site, "private-knowledge", "M5_SECRET_BODY").await;
        let policy = m::policy(&site.app, "Academy", "academy", "")
            .await
            .unwrap();
        m::protect(&site.app, "post", &id, &policy, 0, 0)
            .await
            .unwrap();
        let time = wpalt::now();
        let grant = m::grant(
            &site.app,
            &learner.user.id,
            "academy",
            time - 100,
            time + 100,
            "local-test",
        )
        .await
        .unwrap();
        assert!(
            m::allowed(&site.app, "post", &id, Some(&learner.user.id), time)
                .await
                .unwrap()
        );
        assert!(
            !m::allowed(&site.app, "post", &id, Some(&learner.user.id), time + 100)
                .await
                .unwrap()
        );
        assert!(
            !m::allowed(&site.app, "post", &id, Some(&outsider.user.id), time)
                .await
                .unwrap()
        );
        assert_eq!(
            get(&site.app, "/private-knowledge", None).await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            get(&site.app, "/private-knowledge", Some(&other)).await.0,
            StatusCode::FORBIDDEN
        );
        let (s, h, b) = request(
            &site.app,
            "GET",
            "/private-knowledge",
            Some(&token),
            "",
            vec![],
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(h["cache-control"], "no-store");
        assert!(String::from_utf8(b).unwrap().contains("M5_SECRET_BODY"));
        for path in [
            "/",
            "/api/content",
            "/search?q=SECRET",
            "/feed.xml",
            "/sitemap.xml",
        ] {
            let (s, body) = get(&site.app, path, None).await;
            assert_eq!(s, StatusCode::OK, "{path}: {body}");
            assert!(
                !body.contains("private-knowledge"),
                "discovery leaked protected content at {path}"
            );
            assert!(!body.contains("M5_SECRET_BODY"));
        }
        assert_eq!(
            form(
                &site.app,
                "/private-knowledge/comments",
                None,
                &[("name", "Visitor"), ("body", "Cannot bypass a policy")]
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            upload(&site, "lesson.png", &png(), "private").await,
            StatusCode::SEE_OTHER
        );
        let media: String = sqlx::query_scalar("SELECT id FROM media")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        m::protect(&site.app, "media", &media, &policy, 0, 0)
            .await
            .unwrap();
        assert_eq!(
            get(&site.app, &format!("/media/{media}"), None).await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            request(
                &site.app,
                "GET",
                &format!("/media/{media}"),
                Some(&token),
                "",
                vec![]
            )
            .await
            .2,
            png()
        );
        m::protect(&site.app, "post", &id, &policy, time + 10, 200)
            .await
            .unwrap();
        assert!(
            !m::allowed(&site.app, "post", &id, Some(&learner.user.id), time + 99)
                .await
                .unwrap()
        );
        assert!(
            !m::allowed(&site.app, "post", &id, Some(&learner.user.id), time + 100)
                .await
                .unwrap()
        );
        m::protect(&site.app, "post", &id, &policy, 0, 0)
            .await
            .unwrap();
        sqlx::query("UPDATE member_grants SET revoked=1 WHERE id=$1")
            .bind(grant)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            get(&site.app, "/private-knowledge", Some(&token)).await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            get(&site.app, &format!("/media/{media}"), Some(&token))
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        let mut disabled = site.app.config.as_ref().clone();
        disabled.membership_enabled = false;
        let off = App::open(disabled).await.unwrap();
        assert_eq!(
            get(&off, "/members", Some(&token)).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get(&off, "/private-knowledge", Some(&token)).await.0,
            StatusCode::FORBIDDEN
        );
        assert!(
            !get(&off, "/api/content", None)
                .await
                .1
                .contains("private-knowledge")
        );
        off.db.pool.close().await;
        site.close().await;
    }
}

#[tokio::test]
async fn courses_preserve_quiz_assignment_progress_under_retries_republication_and_fresh_recovery()
{
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let (token, learner) = member(&site, "student@example.test").await;
        let (_, outsider) = member(&site, "outsider@example.test").await;
        let a = publish(&site, "lesson-one", "FIRST_PRIVATE_LESSON").await;
        let b = publish(&site, "lesson-two", "SECOND_PRIVATE_LESSON").await;
        let policy = m::policy(&site.app, "Academy access", "academy", "")
            .await
            .unwrap();
        m::grant(
            &site.app,
            &learner.user.id,
            "academy",
            wpalt::now() - 10,
            0,
            "local-test",
        )
        .await
        .unwrap();
        let mut first = lesson(a.clone(), "Introduction");
        first.questions = vec![Question {
            prompt: "Choose the safe authorization rule".into(),
            choices: vec!["Check every request".into(), "Trust hidden buttons".into()],
            correct: 0,
        }];
        let mut second = lesson(b.clone(), "Practice");
        second.assignment = "Explain your approach".into();
        assert_eq!(
            upload(&site, "course-download.png", &png(), "private").await,
            StatusCode::SEE_OTHER
        );
        let download: String =
            sqlx::query_scalar("SELECT id FROM media WHERE original_name='course-download.png'")
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        second.downloads.push(download.clone());
        let course = Course {
            title: "Local learning".into(),
            policy_id: policy,
            sequential: true,
            lessons: vec![first.clone(), second.clone()],
        };
        let id = m::create_course(&site.app, &course).await.unwrap();
        let v = m::save_course(&site.app, &id, 1, &course, true)
            .await
            .unwrap();
        assert!(m::learner_state(&site.app, &outsider, &id).await.is_err());
        assert_eq!(
            get(&site.app, "/lesson-two", Some(&token)).await.0,
            StatusCode::FORBIDDEN
        );
        assert!(!m::learner_state(&site.app, &learner, &id).await.unwrap().2[1].unlocked);
        assert_eq!(
            request(
                &site.app,
                "GET",
                &format!("/media/{download}"),
                Some(&token),
                "",
                vec![]
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        let api = get(
            &site.app,
            &format!("/api/members/courses/{id}"),
            Some(&token),
        )
        .await;
        assert_eq!(api.0, StatusCode::OK);
        assert!(!api.1.contains("correct"));
        assert!(!api.1.contains("Practice"), "locked lesson title leaked");
        let failed = m::assess(
            &site.app,
            &learner,
            &id,
            &first.id,
            m::AttemptInput {
                version: v,
                key: &uuid::Uuid::new_v4().to_string(),
                answers: &[1],
                assignment: "",
            },
        )
        .await
        .unwrap();
        assert!(!failed.passed);
        assert!(!failed.completed);
        let key = uuid::Uuid::new_v4().to_string();
        let (a, b) = tokio::join!(
            m::assess(
                &site.app,
                &learner,
                &id,
                &first.id,
                m::AttemptInput {
                    version: v,
                    key: &key,
                    answers: &[0],
                    assignment: ""
                }
            ),
            m::assess(
                &site.app,
                &learner,
                &id,
                &first.id,
                m::AttemptInput {
                    version: v,
                    key: &key,
                    answers: &[0],
                    assignment: ""
                }
            )
        );
        assert!(a.unwrap().completed);
        assert!(b.unwrap().completed);
        let attempts: i64 = sqlx::query_scalar(
            "SELECT attempts FROM member_progress WHERE course_id=$1 AND lesson_id=$2",
        )
        .bind(&id)
        .bind(&first.id)
        .fetch_one(&site.app.db.pool)
        .await
        .unwrap();
        assert_eq!(attempts, 2, "same request consumed another attempt");
        assert!(m::learner_state(&site.app, &learner, &id).await.unwrap().2[1].unlocked);
        let pending = m::assess(
            &site.app,
            &learner,
            &id,
            &second.id,
            m::AttemptInput {
                version: v,
                key: &uuid::Uuid::new_v4().to_string(),
                answers: &[],
                assignment: "I enforce authoritative policy before delivery.",
            },
        )
        .await
        .unwrap();
        assert!(pending.passed);
        assert!(!pending.completed);
        assert!(m::certificate(&site.app, &learner, &id).await.is_err());
        let assignment: String =
            sqlx::query_scalar("SELECT id FROM member_assignments WHERE course_id=$1")
                .bind(&id)
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        m::grade(
            &site.app,
            &assignment,
            1,
            true,
            "Approved: clear explanation",
        )
        .await
        .unwrap();
        assert!(
            m::grade(&site.app, &assignment, 1, true, "Stale overwrite")
                .await
                .is_err()
        );
        assert_eq!(
            request(
                &site.app,
                "GET",
                &format!("/media/{download}"),
                Some(&token),
                "",
                vec![]
            )
            .await
            .0,
            StatusCode::OK
        );
        // Approved work cannot silently return to submitted while retaining completion.
        assert!(
            m::assess(
                &site.app,
                &learner,
                &id,
                &second.id,
                m::AttemptInput {
                    version: v,
                    key: &uuid::Uuid::new_v4().to_string(),
                    answers: &[],
                    assignment: "Unreviewed replacement"
                }
            )
            .await
            .is_err()
        );
        let state: String = sqlx::query_scalar("SELECT state FROM member_assignments WHERE id=$1")
            .bind(&assignment)
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(state, "approved");
        let cert = m::certificate(&site.app, &learner, &id).await.unwrap();
        assert_eq!(
            get(
                &site.app,
                &format!("/members/certificates/{cert}"),
                Some(&token)
            )
            .await
            .0,
            StatusCode::OK
        );
        // Republishing is deliberate and cannot reuse progress for changed assessment meaning.
        let next = m::save_course(&site.app, &id, v, &course, true)
            .await
            .unwrap();
        assert!(next > v);
        assert!(!m::learner_state(&site.app, &learner, &id).await.unwrap().2[0].completed);
        assert!(
            m::assess(
                &site.app,
                &learner,
                &id,
                &first.id,
                m::AttemptInput {
                    version: v,
                    key: &uuid::Uuid::new_v4().to_string(),
                    answers: &[0],
                    assignment: ""
                }
            )
            .await
            .is_err()
        );
        let archive = backup::capture(&site.app).await.unwrap();
        let fresh = Site::new(pg, false).await;
        backup::restore(&fresh.app, &archive).await.unwrap();
        let (restored_token, restored) = auth::login(&fresh.app, "student@example.test", PASSWORD)
            .await
            .unwrap();
        assert_eq!(
            get(
                &fresh.app,
                &format!("/members/certificates/{cert}"),
                Some(&restored_token)
            )
            .await
            .0,
            StatusCode::OK
        );
        assert!(
            !m::learner_state(&fresh.app, &restored, &id)
                .await
                .unwrap()
                .2[0]
                .completed
        );
        assert_eq!(
            get(&fresh.app, "/lesson-one", None).await.0,
            StatusCode::FORBIDDEN
        );
        // A recomputed transport checksum cannot make a course's missing access rule valid.
        let mut envelope: serde_json::Value = serde_json::from_slice(&archive).unwrap();
        let mut snapshot: serde_json::Value =
            serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
        snapshot["tables"]["member_resources"] = serde_json::json!([]);
        let payload = serde_json::to_string(&snapshot).unwrap();
        envelope["sha256"] = auth::digest(payload.as_bytes()).into();
        envelope["payload"] = payload.into();
        let empty = Site::new(pg, false).await;
        assert!(
            backup::restore(&empty.app, &serde_json::to_vec(&envelope).unwrap())
                .await
                .is_err()
        );
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&empty.app.db.pool)
            .await
            .unwrap();
        assert_eq!(n, 0, "unsafe recovery partially wrote users");
        empty.close().await;
        fresh.close().await;
        site.close().await;
    }
}

#[tokio::test]
async fn organizations_gifts_and_moderated_communities_do_not_expand_member_authority() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let (token, manager) = member(&site, "manager@example.test").await;
        let (other, learner) = member(&site, "seat@example.test").await;
        let (_, outsider) = member(&site, "outsider@example.test").await;
        let group = m::group(&site.app, "Team", &manager.user.id, 1)
            .await
            .unwrap();
        m::seat(&site.app, &manager, &group, "seat@example.test", false)
            .await
            .unwrap();
        assert!(
            m::seat(&site.app, &manager, &group, "manager@example.test", false)
                .await
                .is_err()
        );
        assert!(
            m::seat(&site.app, &outsider, &group, "manager@example.test", true)
                .await
                .is_err()
        );
        assert_eq!(
            get(&site.app, &format!("/members/groups/{group}"), Some(&other))
                .await
                .0,
            StatusCode::OK
        );
        m::discuss(
            &site.app,
            &learner,
            &group,
            "PENDING_TEAM_DISCUSSION <script>alert(1)</script>",
        )
        .await
        .unwrap();
        assert!(
            !get(&site.app, &format!("/members/groups/{group}"), Some(&token))
                .await
                .1
                .contains("PENDING_TEAM_DISCUSSION")
        );
        let discussion: String = sqlx::query_scalar("SELECT id FROM member_discussions")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            form(
                &site.app,
                "/admin/members",
                Some(&other),
                &[
                    ("csrf", &learner.csrf),
                    ("operation", "moderate"),
                    ("id", &discussion),
                    ("decision", "approve")
                ]
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            form(
                &site.app,
                "/admin/members",
                Some(&site.token),
                &[
                    ("csrf", &site.session().csrf),
                    ("operation", "moderate"),
                    ("id", &discussion),
                    ("decision", "approve")
                ]
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
        let page = get(&site.app, &format!("/members/groups/{group}"), Some(&token))
            .await
            .1;
        assert!(page.contains("PENDING_TEAM_DISCUSSION"));
        assert!(!page.contains("<script>alert"));
        let gift = m::gift(&site.app, "academy", wpalt::now() + 100, 3600)
            .await
            .unwrap();
        let (a, b) = tokio::join!(
            m::claim_gift(&site.app, &manager, &gift),
            m::claim_gift(&site.app, &learner, &gift)
        );
        assert_ne!(a.is_ok(), b.is_ok(), "gift claimed twice");
        let grants: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM member_grants WHERE entitlement='academy'")
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        assert_eq!(grants, 1);
        m::seat(&site.app, &manager, &group, "seat@example.test", true)
            .await
            .unwrap();
        assert_eq!(
            get(&site.app, &format!("/members/groups/{group}"), Some(&other))
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        assert!(
            m::discuss(
                &site.app,
                &learner,
                &group,
                "Revoked member cannot contribute"
            )
            .await
            .is_err()
        );
        assert_eq!(
            form(
                &site.app,
                "/members/profile",
                Some(&token),
                &[("csrf", "wrong"), ("name", "Hijack"), ("biography", "")]
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        site.close().await;
    }
}

#[tokio::test]
async fn identity_subject_binding_and_signed_claims_cannot_bypass_local_account_authority() {
    use wpalt::membership::identity::{Claims, Config, identity_session, verify_token};
    let cfg = Config {
        enabled: true,
        issuer: "https://identity.example.test".into(),
        client_id: "wpalt-test-client".into(),
        ..Default::default()
    };
    let keys: jsonwebtoken::jwk::JwkSet =
        serde_json::from_str(include_str!("../fixtures/identity-test-jwks.json")).unwrap();
    let signing = jsonwebtoken::EncodingKey::from_rsa_der(include_bytes!(
        "../fixtures/identity-test-key.der"
    ));
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some("fixture-key".into());
    let mut claims = Claims {
        iss: cfg.issuer.clone(),
        sub: "stable-subject".into(),
        aud: serde_json::json!(cfg.client_id),
        exp: wpalt::now() as u64 + 300,
        iat: wpalt::now() as u64,
        nonce: "browser-bound-nonce".into(),
        at_hash: String::new(),
        azp: String::new(),
    };
    let signed = jsonwebtoken::encode(&header, &claims, &signing).unwrap();
    assert!(verify_token(&cfg, &signed, &keys, "browser-bound-nonce", "").is_ok());
    assert!(verify_token(&cfg, &signed, &keys, "wrong-nonce", "").is_err());
    for attack in [
        "issuer",
        "audience",
        "expiration",
        "authorized-party",
        "future-issued",
        "access-hash",
    ] {
        let mut bad = serde_json::to_value(&claims).unwrap();
        match attack {
            "issuer" => bad["iss"] = "https://attacker.example.test".into(),
            "audience" => bad["aud"] = "another-client".into(),
            "expiration" => bad["exp"] = 0.into(),
            "authorized-party" => bad["azp"] = "another-client".into(),
            "future-issued" => bad["iat"] = (wpalt::now() + 3600).into(),
            "access-hash" => bad["at_hash"] = "wrong".into(),
            _ => unreachable!(),
        };
        let token = jsonwebtoken::encode(&header, &bad, &signing).unwrap();
        assert!(
            verify_token(&cfg, &token, &keys, "browser-bound-nonce", "").is_err(),
            "accepted {attack}"
        );
    }
    claims.aud = serde_json::json!([cfg.client_id, "another-audience"]);
    let signed = jsonwebtoken::encode(&header, &claims, &signing).unwrap();
    assert!(
        verify_token(&cfg, &signed, &keys, "browser-bound-nonce", "").is_err(),
        "multiple audiences lacked authorized-party binding"
    );
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let (_, learner) = member(&site, "identity@example.test").await;
        assert!(
            identity_session(&site.app, &cfg.issuer, "stable-subject")
                .await
                .is_err()
        );
        sqlx::query("INSERT INTO member_identities(issuer,subject,user_id) VALUES($1,$2,$3)")
            .bind(&cfg.issuer)
            .bind("stable-subject")
            .bind(&learner.user.id)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        let (token, s) = identity_session(&site.app, &cfg.issuer, "stable-subject")
            .await
            .unwrap();
        assert_eq!(s.user.id, learner.user.id);
        assert_eq!(
            get(&site.app, "/members", Some(&token)).await.0,
            StatusCode::OK
        );
        auth::update_user(&site.app, &learner.user.id, "Learner", "disabled", "")
            .await
            .unwrap();
        assert!(
            identity_session(&site.app, &cfg.issuer, "stable-subject")
                .await
                .is_err()
        );
        assert_eq!(
            get(&site.app, "/members", Some(&token)).await.0,
            StatusCode::UNAUTHORIZED
        );
        let mut config = site.app.config.as_ref().clone();
        config.identity.client_secret = "fixture-secret-must-be-redacted".into();
        assert!(
            !config
                .redacted()
                .to_string()
                .contains("fixture-secret-must-be-redacted")
        );
        site.close().await;
    }
}

#[tokio::test]
async fn membership_budget_rolls_back_publication_and_counts_only_persisted_rows() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let post = publish(&site, "budget-lesson", "Budget protected content").await;
        let policy = m::policy(&site.app, "Budget academy", "budget", "")
            .await
            .unwrap();
        let course = Course {
            title: "Budget course".into(),
            policy_id: policy,
            sequential: true,
            lessons: vec![lesson(post, "Lesson")],
        };
        let id = m::create_course(&site.app, &course).await.unwrap();
        let before: i64 = sqlx::query_scalar("SELECT records FROM member_usage WHERE id=1")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        // Publication writes a lesson rule, course rule and edition. A mid-transaction
        // quota failure must leave none of those writes or a changed course version.
        sqlx::query("UPDATE member_usage SET limit_records=$1 WHERE id=1")
            .bind(before + 1)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        assert!(
            m::save_course(&site.app, &id, 1, &course, true)
                .await
                .is_err()
        );
        assert!(m::live(&site.app, &id).await.is_err());
        let count: i64 = sqlx::query_scalar("SELECT records FROM member_usage WHERE id=1")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            count, before,
            "Failed publication rolls back the budget as well"
        );
        let resources: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM member_resources WHERE course_id=$1 OR resource_id=$1",
        )
        .bind(&id)
        .fetch_one(&site.app.db.pool)
        .await
        .unwrap();
        assert_eq!(resources, 0);
        sqlx::query("UPDATE member_usage SET limit_records=$1 WHERE id=1")
            .bind(before + 10)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        let edition = m::save_course(&site.app, &id, 1, &course, true)
            .await
            .unwrap();
        // Re-publication upserts existing rules; only the new edition consumes a row.
        let published: i64 = sqlx::query_scalar("SELECT records FROM member_usage WHERE id=1")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        m::save_course(&site.app, &id, edition, &course, true)
            .await
            .unwrap();
        let republished: i64 = sqlx::query_scalar("SELECT records FROM member_usage WHERE id=1")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(republished, published + 1);
        let archive = backup::capture(&site.app).await.unwrap();
        let fresh = Site::new(pg, false).await;
        backup::restore(&fresh.app, &archive).await.unwrap();
        for app in [&site.app, &fresh.app] {
            let mut actual = 0i64;
            for table in m::budget::TABLES {
                actual += sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                    .fetch_one(&app.db.pool)
                    .await
                    .unwrap();
            }
            let stored: i64 = sqlx::query_scalar("SELECT records FROM member_usage WHERE id=1")
                .fetch_one(&app.db.pool)
                .await
                .unwrap();
            assert_eq!(
                stored, actual,
                "Derived counters agree after publication and fresh recovery"
            );
        }
        fresh.close().await;
        site.close().await;
    }
}
