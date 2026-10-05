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
pub(super) fn lesson(post: String, title: &str) -> Lesson {
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
        assert!(h["x-robots-tag"].to_str().unwrap().contains("noindex"));
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
        let (outsider_token, outsider) = member(&site, "outsider@example.test").await;
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
        m::grant(
            &site.app,
            &outsider.user.id,
            "academy",
            wpalt::now() - 10,
            0,
            "local-test",
        )
        .await
        .unwrap();
        for _ in 0..3 {
            assert!(
                !m::assess(
                    &site.app,
                    &outsider,
                    &id,
                    &first.id,
                    m::AttemptInput {
                        version: v,
                        key: &uuid::Uuid::new_v4().to_string(),
                        answers: &[1],
                        assignment: ""
                    }
                )
                .await
                .unwrap()
                .passed
            );
        }
        assert!(
            m::assess(
                &site.app,
                &outsider,
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
        let exhausted_page = get(
            &site.app,
            &format!("/members/courses/{id}/lessons/{}", first.id),
            Some(&outsider_token),
        )
        .await;
        assert_eq!(exhausted_page.0, StatusCode::OK);
        assert!(exhausted_page.1.contains("Attempt limit reached"));
        assert!(!exhausted_page.1.contains("Submit assessment"));
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
            "SELECT attempts FROM member_progress WHERE course_id=$1 AND lesson_id=$2 AND user_id=$3",
        )
        .bind(&id)
        .bind(&first.id)
        .bind(&learner.user.id)
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
        // Reset revokes the old proof permanently; reviewed replacement work
        // receives a new certificate identity, not a revived old link.
        m::reset_progress(&site.app, &id, v, &second.id, &learner.user.id)
            .await
            .unwrap();
        assert_eq!(
            get(
                &site.app,
                &format!("/members/certificates/{cert}"),
                Some(&token)
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
        assert!(m::certificate(&site.app, &learner, &id).await.is_err());
        m::assess(
            &site.app,
            &learner,
            &id,
            &second.id,
            m::AttemptInput {
                version: v,
                key: &uuid::Uuid::new_v4().to_string(),
                answers: &[],
                assignment: "Replacement work after an explicit reset",
            },
        )
        .await
        .unwrap();
        let assignment_version: i64 =
            sqlx::query_scalar("SELECT version FROM member_assignments WHERE id=$1")
                .bind(&assignment)
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        m::grade(
            &site.app,
            &assignment,
            assignment_version,
            true,
            "Approved replacement",
        )
        .await
        .unwrap();
        let replacement = m::certificate(&site.app, &learner, &id).await.unwrap();
        assert_ne!(replacement, cert);
        assert_eq!(
            get(
                &site.app,
                &format!("/members/certificates/{cert}"),
                Some(&token)
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
        let cert = replacement;
        // Draft titles/policies must not leak or change authority before publication.
        let mut draft = course.clone();
        draft.title = "PRIVATE_FUTURE_TITLE".into();
        draft.policy_id = m::policy(&site.app, "Future restriction", "future", "")
            .await
            .unwrap();
        let draft_version = m::save_course(&site.app, &id, v, &draft, false)
            .await
            .unwrap();
        let dashboard = get(&site.app, "/members", Some(&token)).await.1;
        assert!(dashboard.contains("Local learning"));
        assert!(!dashboard.contains("PRIVATE_FUTURE_TITLE"));
        assert_eq!(m::live(&site.app, &id).await.unwrap().1, v);
        // Republishing is deliberate and cannot reuse progress for changed assessment meaning.
        let next = m::save_course(&site.app, &id, draft_version, &course, true)
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
        assert!(
            m::release(&site.app, "post", &first.post_id).await.is_err(),
            "Active lessons cannot be made public independently"
        );
        let mut shorter = course.clone();
        shorter.lessons.remove(0);
        m::save_course(&site.app, &id, next, &shorter, true)
            .await
            .unwrap();
        assert_eq!(
            get(&site.app, "/lesson-one", Some(&token)).await.0,
            StatusCode::FORBIDDEN
        );
        m::release(&site.app, "post", &first.post_id).await.unwrap();
        assert_eq!(get(&site.app, "/lesson-one", None).await.0, StatusCode::OK);
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
        // Browsers redact Origin on no-referrer gift pages. Only a same-origin
        // document navigation may proceed, and it still needs session + CSRF.
        let browser_gift = m::gift(&site.app, "browser-gift", wpalt::now() + 100, 60)
            .await
            .unwrap();
        let browser_path = format!("/members/gifts/{browser_gift}");
        for (fetch_site, csrf, expected) in [
            ("cross-site", manager.csrf.as_str(), StatusCode::FORBIDDEN),
            ("same-origin", "forged", StatusCode::FORBIDDEN),
            ("same-origin", manager.csrf.as_str(), StatusCode::SEE_OTHER),
        ] {
            let body = serde_urlencoded::to_string([("csrf", csrf)]).unwrap();
            let request = Request::builder()
                .method("POST")
                .uri(&browser_path)
                .header("cookie", format!("wpalt_session={token}"))
                .header("origin", "null")
                .header("sec-fetch-site", fetch_site)
                .header("sec-fetch-mode", "navigate")
                .header("sec-fetch-dest", "document")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap();
            assert_eq!(
                wpalt::web::router(site.app.clone())
                    .oneshot(request)
                    .await
                    .unwrap()
                    .status(),
                expected
            );
        }
        assert_eq!(
            get(&site.app, &browser_path, Some(&token)).await.0,
            StatusCode::NOT_FOUND
        );
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
        assert_eq!(
            form(
                &site.app,
                "/members/profile",
                Some(&token),
                &[
                    ("csrf", &manager.csrf),
                    ("name", "Manager"),
                    ("biography", "Private recovered biography")
                ]
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
        let referral =
            m::referrals::create(&site.app, &manager.user.id, "Recovered local referral")
                .await
                .unwrap();
        m::referrals::commission(&site.app, &referral, "one-local-obligation", 1200, "USD")
            .await
            .unwrap();
        assert!(
            m::referrals::commission(&site.app, &referral, "one-local-obligation", 9999, "USD")
                .await
                .is_err()
        );
        let archive = backup::capture(&site.app).await.unwrap();
        let recovered = Site::new(pg, false).await;
        backup::restore(&recovered.app, &archive).await.unwrap();
        let (recovered_token, _) = auth::login(&recovered.app, "manager@example.test", PASSWORD)
            .await
            .unwrap();
        assert!(
            get(&recovered.app, "/members/profile", Some(&recovered_token))
                .await
                .1
                .contains("Private recovered biography")
        );
        let discussion = get(
            &recovered.app,
            &format!("/members/groups/{group}"),
            Some(&recovered_token),
        )
        .await
        .1;
        assert!(
            discussion.contains("PENDING_TEAM_DISCUSSION") && !discussion.contains("<script>alert")
        );
        assert!(
            m::claim_gift(&recovered.app, &manager, &browser_gift)
                .await
                .is_err(),
            "Recovery must not resurrect a spent gift"
        );
        let amount: i64 = sqlx::query_scalar(
            "SELECT amount_minor FROM member_commissions WHERE reference='one-local-obligation'",
        )
        .fetch_one(&recovered.app.db.pool)
        .await
        .unwrap();
        assert_eq!(
            amount, 1200,
            "Duplicate reference must not change the original obligation"
        );
        recovered.close().await;
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
        let (_, active) = member(&site, "binding-revocation@example.test").await;
        sqlx::query(
            "INSERT INTO member_identities(issuer,subject,user_id) VALUES($1,'remove-me',$2)",
        )
        .bind(&cfg.issuer)
        .bind(&active.user.id)
        .execute(&site.app.db.pool)
        .await
        .unwrap();
        let (active_token, _) = identity_session(&site.app, &cfg.issuer, "remove-me")
            .await
            .unwrap();
        assert_eq!(
            form(
                &site.app,
                "/admin/members",
                Some(&site.token),
                &[
                    ("csrf", &site.session().csrf),
                    ("operation", "identity-remove"),
                    ("title", &cfg.issuer),
                    ("key", "remove-me")
                ]
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
        assert_eq!(
            get(&site.app, "/members", Some(&active_token)).await.0,
            StatusCode::UNAUTHORIZED
        );
        assert!(
            identity_session(&site.app, &cfg.issuer, "remove-me")
                .await
                .is_err()
        );
        let mut config = site.app.config.as_ref().clone();
        config.identity.client_secret = "fixture-secret-must-be-redacted".into();
        assert!(
            !config
                .redacted()
                .to_string()
                .contains("fixture-secret-must-be-redacted")
        );
        // A wrong browser cannot consume another browser's flow; the owning
        // browser consumes it once even if the configured provider is unavailable.
        let mut app = site.app.clone();
        let mut config = app.config.as_ref().clone();
        config.base_url = "https://localhost".into();
        config.identity = Config {
            enabled: true,
            issuer: cfg.issuer.clone(),
            client_id: cfg.client_id.clone(),
            authorization_url: "https://identity.example.test/authorize".into(),
            token_url: "https://127.0.0.1:1/token".into(),
            jwks_url: "https://127.0.0.1:1/keys".into(),
            client_secret: "private-test-secret".into(),
            ca_cert_file: String::new(),
        };
        config.validate().unwrap();
        app.config = std::sync::Arc::new(config);
        let (_, headers, _) =
            request(&app, "GET", "/members/identity/start", None, "", vec![]).await;
        let location = url::Url::parse(headers["location"].to_str().unwrap()).unwrap();
        let values: std::collections::HashMap<_, _> = location.query_pairs().into_owned().collect();
        assert_eq!(values["code_challenge_method"], "S256");
        assert!(!location.as_str().contains("private-test-secret"));
        let cookie = headers["set-cookie"].to_str().unwrap();
        assert!(
            cookie.contains("Secure")
                && cookie.contains("HttpOnly")
                && cookie.contains("SameSite=Lax")
        );
        let cookie = cookie.split(';').next().unwrap();
        let callback = format!(
            "/members/identity/callback?state={}&code=fixture-code",
            values["state"]
        );
        let wrong = Request::builder()
            .uri(&callback)
            .header("cookie", format!("wpalt_identity={}", "0".repeat(64)))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            wpalt::web::router(app.clone())
                .oneshot(wrong)
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM identity_flows")
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
        let own = Request::builder()
            .uri(&callback)
            .header("cookie", cookie)
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            wpalt::web::router(app.clone())
                .oneshot(own)
                .await
                .unwrap()
                .status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        let replay = Request::builder()
            .uri(&callback)
            .header("cookie", cookie)
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            wpalt::web::router(app.clone())
                .oneshot(replay)
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
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

#[tokio::test]
async fn populated_learning_projection_keeps_prerequisites_and_indexed_lookup_paths() {
    let mut evidence = vec![];
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let (token, learner) = member(&site, "volume-learner@example.test").await;
        let policy = m::policy(&site.app, "Measured academy", "measured", "")
            .await
            .unwrap();
        m::grant(
            &site.app,
            &learner.user.id,
            "measured",
            wpalt::now() - 60,
            0,
            "local-test",
        )
        .await
        .unwrap();
        let mut lessons = vec![];
        for index in 0..100 {
            let post = publish(
                &site,
                &format!("measured-lesson-{index}"),
                "Measured shared lesson content",
            )
            .await;
            lessons.push(lesson(post, &format!("Lesson {}", index + 1)));
        }
        let course = Course {
            title: "Measured academy".into(),
            policy_id: policy,
            sequential: true,
            lessons,
        };
        let id = m::create_course(&site.app, &course).await.unwrap();
        let version = m::save_course(&site.app, &id, 1, &course, true)
            .await
            .unwrap();
        let mut tx = site.app.db.pool.begin().await.unwrap();
        for index in 0..1000 {
            sqlx::query("INSERT INTO member_grants(id,user_id,entitlement,starts_at,expires_at,origin,created_at) VALUES($1,$2,$3,1,2,'volume-fixture',1)")
                .bind(uuid::Uuid::new_v4().to_string()).bind(&learner.user.id).bind(format!("unrelated-{index}"))
                .execute(&mut *tx).await.unwrap();
        }
        tx.commit().await.unwrap();
        let mut times = vec![];
        for _ in 0..30 {
            let start = std::time::Instant::now();
            let (_, edition, states) = m::learner_state(&site.app, &learner, &id).await.unwrap();
            times.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(edition, version);
            assert_eq!(states.len(), 100);
            assert_eq!(states.iter().filter(|s| s.unlocked).count(), 1);
            assert!(
                states.iter().skip(1).all(|s| s.title == "Locked lesson"),
                "Locked titles stay private at maximum course size"
            );
        }
        assert!(
            !m::allowed(
                &site.app,
                "post",
                &course.lessons[99].post_id,
                Some(&learner.user.id),
                wpalt::now()
            )
            .await
            .unwrap()
        );
        // More than one full catalog page, including a restricted course, must
        // remain reachable without gaps, duplicates or post-pagination filtering.
        let denied = m::policy(&site.app, "Restricted catalog", "not-assigned", "")
            .await
            .unwrap();
        for index in 0..41 {
            let post = publish(&site, &format!("catalog-{index}"), "Catalog lesson").await;
            let c = Course {
                title: if index == 40 {
                    "PRIVATE_CATALOG_TITLE".into()
                } else {
                    format!("Catalog course {index}")
                },
                policy_id: if index == 40 {
                    denied.clone()
                } else {
                    course.policy_id.clone()
                },
                sequential: true,
                lessons: vec![lesson(post, "Catalog lesson")],
            };
            let cid = m::create_course(&site.app, &c).await.unwrap();
            m::save_course(&site.app, &cid, 1, &c, true).await.unwrap();
        }
        for (base, session, expected) in [
            ("/members", token.as_str(), 41),
            ("/admin/courses", site.token.as_str(), 42),
        ] {
            let mut path = base.to_owned();
            let mut found = HashSet::new();
            let mut pages = 0;
            loop {
                let (status, html) = get(&site.app, &path, Some(session)).await;
                assert_eq!(status, StatusCode::OK);
                if base == "/members" {
                    assert!(!html.contains("PRIVATE_CATALOG_TITLE"));
                }
                let prefix = format!("href=\"{base}/courses/");
                let prefix = if base == "/admin/courses" {
                    "href=\"/admin/courses/".to_owned()
                } else {
                    prefix
                };
                let links: Vec<_> = html
                    .split(&prefix)
                    .skip(1)
                    .map(|tail| tail.split('"').next().unwrap().to_owned())
                    .collect();
                assert!(links.len() <= 40);
                for id in links {
                    assert!(found.insert(id), "Catalog cursor duplicated a course");
                }
                pages += 1;
                let more = format!("href=\"{base}?before=");
                if let Some((_, tail)) = html.split_once(&more) {
                    path = format!("{base}?before={}", tail.split('"').next().unwrap());
                    assert!(pages < 3, "Catalog cursor failed to make progress");
                } else {
                    break;
                }
            }
            assert_eq!(pages, 2);
            assert_eq!(found.len(), expected);
        }
        assert_eq!(
            get(&site.app, "/members?before=invalid", Some(&token))
                .await
                .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        // A full pending queue must advance after grading, with historical lesson context.
        let mut review_course = course.clone();
        review_course.sequential = false;
        for lesson in review_course.lessons.iter_mut().take(42) {
            lesson.assignment = "Show your work".into();
        }
        let review_version = m::save_course(&site.app, &id, version, &review_course, true)
            .await
            .unwrap();
        for lesson in review_course.lessons.iter().take(42) {
            m::assess(
                &site.app,
                &learner,
                &id,
                &lesson.id,
                m::AttemptInput {
                    version: review_version,
                    key: &uuid::Uuid::new_v4().to_string(),
                    answers: &[],
                    assignment: "Work awaiting human review",
                },
            )
            .await
            .unwrap();
        }
        let pending = sqlx::query("SELECT id,lesson_title FROM member_assignments WHERE state='submitted' ORDER BY created_at,id")
            .fetch_all(&site.app.db.pool).await.unwrap();
        assert_eq!(pending.len(), 42);
        let (status, html) = get(&site.app, "/admin/members", Some(&site.token)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(html.matches("value=\"grade\"").count(), 40);
        for (index, row) in pending.iter().enumerate() {
            let assignment: String = row.get("id");
            assert_eq!(html.contains(&assignment), index < 40);
            if index < 40 {
                assert!(html.contains(&row.get::<String, _>("lesson_title")));
                let (status, _) = form(
                    &site.app,
                    "/admin/members",
                    Some(&site.token),
                    &[
                        ("csrf", &site.session().csrf),
                        ("operation", "grade"),
                        ("id", &assignment),
                        ("version", "1"),
                        ("decision", "approve"),
                    ],
                )
                .await;
                assert_eq!(status, StatusCode::SEE_OTHER);
            }
        }
        let (_, html) = get(&site.app, "/admin/members", Some(&site.token)).await;
        assert_eq!(html.matches("value=\"grade\"").count(), 2);
        for (index, row) in pending.iter().enumerate() {
            assert_eq!(html.contains(&row.get::<String, _>("id")), index >= 40);
        }
        sqlx::raw_sql("ANALYZE")
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        let prefix = if pg {
            "EXPLAIN "
        } else {
            "EXPLAIN QUERY PLAN "
        };
        let user = &learner.user.id;
        let queries = [
            format!(
                "SELECT policy_id FROM member_resources WHERE kind='course' AND resource_id='{id}'"
            ),
            format!(
                "SELECT MIN(starts_at) FROM member_grants WHERE user_id='{user}' AND entitlement='measured' AND revoked=0 AND starts_at<=9999999999 AND (expires_at=0 OR expires_at>9999999999)"
            ),
            format!(
                "SELECT lesson_id,completed_at FROM member_progress WHERE course_id='{id}' AND course_version={version} AND user_id='{user}'"
            ),
        ];
        let mut plans = vec![];
        for query in queries {
            let rows = sqlx::query(&format!("{prefix}{query}"))
                .fetch_all(&site.app.db.pool)
                .await
                .unwrap();
            plans.push(
                rows.iter()
                    .map(|r| {
                        if pg {
                            r.get::<String, _>(0)
                        } else {
                            r.get::<String, _>("detail")
                        }
                    })
                    .collect::<Vec<_>>(),
            );
        }
        times.sort_by(f64::total_cmp);
        evidence.push(serde_json::json!({"engine":if pg {"postgres"}else{"sqlite"},"conditions":"Debug integration API, maximum 100-lesson sequential course, 1,000 unrelated grants; 42-course catalog pagination/filtering and 42-submission review queue advancement; no timing thresholds or production-capacity claim","p50_ms":times[14],"p95_ms":times[28],"plans":plans,"projection":"One progress batch; no per-lesson authorization query loop"}));
        site.close().await;
    }
    std::fs::create_dir_all("work").unwrap();
    std::fs::write(
        "work/m5-volume.json",
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn identity_https_code_exchange_validates_tls_pkce_claims_and_local_binding() {
    use std::io::BufRead;
    use wpalt::membership::identity::{Claims, Config};
    struct Provider(std::process::Child);
    impl Drop for Provider {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let (_, learner) = member(&site, "https-identity@example.test").await;
        let directory = tempfile::tempdir().unwrap();
        let spec = directory.path().join("provider.json");
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let child = std::process::Command::new("python3")
            .arg("-u")
            .arg(fixtures.join("identity_provider.py"))
            .arg(&spec)
            .arg(fixtures.join("identity-fixture-server.pem"))
            .arg(fixtures.join("identity-fixture-server.key"))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut provider = Provider(child);
        let mut port = String::new();
        std::io::BufReader::new(provider.0.stdout.take().unwrap())
            .read_line(&mut port)
            .unwrap();
        let port: u16 = port.trim().parse().expect("HTTPS adapter started");
        let issuer = format!("https://127.0.0.1:{port}");
        let mut app = site.app.clone();
        let mut cfg = app.config.as_ref().clone();
        cfg.base_url = "https://localhost".into();
        cfg.identity = Config {
            enabled: true,
            issuer: issuer.clone(),
            client_id: "test client:local".into(),
            client_secret: "fixture+private:secret".into(),
            authorization_url: format!("{issuer}/authorize"),
            token_url: format!("{issuer}/token"),
            jwks_url: format!("{issuer}/keys"),
            ..Default::default()
        };
        cfg.validate().unwrap();
        app.config = std::sync::Arc::new(cfg.clone());
        sqlx::query(
            "INSERT INTO member_identities(issuer,subject,user_id) VALUES($1,'fixture-subject',$2)",
        )
        .bind(&issuer)
        .bind(&learner.user.id)
        .execute(&app.db.pool)
        .await
        .unwrap();
        let keys: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/identity-test-jwks.json")).unwrap();
        let signing = jsonwebtoken::EncodingKey::from_rsa_der(include_bytes!(
            "../fixtures/identity-test-key.der"
        ));
        for trusted in [false, true] {
            if trusted {
                cfg.identity.ca_cert_file = fixtures
                    .join("identity-fixture-ca.pem")
                    .to_string_lossy()
                    .into();
                app.config = std::sync::Arc::new(cfg.clone());
            }
            let (_, headers, _) =
                request(&app, "GET", "/members/identity/start", None, "", vec![]).await;
            let location = url::Url::parse(headers["location"].to_str().unwrap()).unwrap();
            let fields: std::collections::HashMap<_, _> =
                location.query_pairs().into_owned().collect();
            let claims = Claims {
                iss: issuer.clone(),
                sub: "fixture-subject".into(),
                aud: serde_json::json!("test client:local"),
                exp: wpalt::now() as u64 + 300,
                iat: wpalt::now() as u64,
                nonce: fields["nonce"].clone(),
                at_hash: String::new(),
                azp: String::new(),
            };
            let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
            header.kid = Some("fixture-key".into());
            let token = jsonwebtoken::encode(&header, &claims, &signing).unwrap();
            std::fs::write(&spec,serde_json::to_vec(&serde_json::json!({"keys":keys,"token":token,"client_id":"test client:local","secret":"fixture+private:secret","challenge":fields["code_challenge"],"callback":"https://localhost/members/identity/callback"})).unwrap()).unwrap();
            let cookie = headers["set-cookie"]
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap();
            let path = format!(
                "/members/identity/callback?code=fixture-code&state={}",
                fields["state"]
            );
            let req = Request::builder()
                .uri(&path)
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap();
            let response = wpalt::web::router(app.clone()).oneshot(req).await.unwrap();
            if !trusted {
                assert_eq!(
                    response.status(),
                    StatusCode::SERVICE_UNAVAILABLE,
                    "An untrusted local certificate must not bypass TLS validation"
                );
                continue;
            }
            assert_eq!(
                response.status(),
                StatusCode::SEE_OTHER,
                "The real HTTPS token/JWKS exchange should issue the bound local session"
            );
            assert_eq!(response.headers()["location"], "/members");
            let session_cookie = response
                .headers()
                .get_all("set-cookie")
                .iter()
                .filter_map(|h| h.to_str().ok())
                .find(|h| h.starts_with("wpalt_session="))
                .unwrap();
            let session = session_cookie
                .strip_prefix("wpalt_session=")
                .unwrap()
                .split(';')
                .next()
                .unwrap();
            assert_eq!(get(&app, "/members", Some(session)).await.0, StatusCode::OK);
            let replay = Request::builder()
                .uri(&path)
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap();
            assert_eq!(
                wpalt::web::router(app.clone())
                    .oneshot(replay)
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
        }
        drop(provider);
        site.close().await;
    }
}
