//! End-user guarantees across SQLite and required real PostgreSQL, not implementation counts.
use serde_json::json;
use sqlx::Row;
use wpalt::{
    App, auth, backup,
    business::{forms::FormDefinition, recovery, store},
    config::Config,
};
struct Site {
    app: App,
    _root: tempfile::TempDir,
    schema: Option<String>,
    owner: String,
}
impl Site {
    async fn new(postgres: Option<&str>, limit: i64) -> Self {
        Self::build(postgres, limit, true).await
    }
    async fn build(postgres: Option<&str>, limit: i64, initialize: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let mut config = Config {
            data_dir: root.path().join("data"),
            database_url: format!(
                "sqlite://{}?mode=rwc",
                root.path().join("site.db").display()
            ),
            ..Default::default()
        };
        config.business_limits.drafts = limit;
        let schema = if let Some(pg) = postgres {
            let name = format!("wpalt_d12_{}", uuid::Uuid::new_v4().simple());
            let pool = sqlx::PgPool::connect(pg).await.unwrap();
            sqlx::query(&format!("CREATE SCHEMA {name}"))
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
            let mut url = url::Url::parse(pg).unwrap();
            url.query_pairs_mut()
                .append_pair("options", &format!("-c search_path={name}"));
            config.database_url = url.to_string();
            Some(name)
        } else {
            None
        };
        let app = App::open(config).await.unwrap();
        let owner = if initialize {
            auth::initialize(
                &app,
                "owner@example.test",
                "Owner",
                "Test password for D12 fixtures",
            )
            .await
            .unwrap();
            sqlx::query_scalar("SELECT id FROM users WHERE role='admin'")
                .fetch_one(&app.db.pool)
                .await
                .unwrap()
        } else {
            String::new()
        };
        Self {
            app,
            _root: root,
            schema,
            owner,
        }
    }
    async fn finish(self) {
        self.app.db.pool.close().await;
        if let Some(schema) = self.schema {
            let pool = sqlx::PgPool::connect(&std::env::var("TEST_DATABASE_URL").unwrap())
                .await
                .unwrap();
            sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }
    }
    async fn form(&self) -> String {
        let definition = form();
        let id = store::create(&self.app, &self.owner, &definition)
            .await
            .unwrap();
        store::save(&self.app, &id, 1, &definition, true)
            .await
            .unwrap();
        id
    }
}
fn engines() -> Vec<Option<String>> {
    let pg = std::env::var("TEST_DATABASE_URL").ok();
    if std::env::var("WPALT_REQUIRE_POSTGRES").is_ok() {
        assert!(pg.is_some(), "Real PostgreSQL is required");
    }
    std::iter::once(None).chain(pg.map(Some)).collect()
}
fn form() -> FormDefinition {
    serde_json::from_value(json!({"title":"A thoughtful request","fields":[{"name":"name","schema":{"kind":"string","required":true}},{"name":"quantity","schema":{"kind":"number"}},{"name":"private_note","schema":{"kind":"string"}}]})).unwrap()
}
async fn enable(site: &Site, id: &str) -> recovery::PublicPolicy {
    recovery::publish_policy(
        &site.app,
        id,
        2,
        Some(recovery::Policy {
            purpose:
                "Help complete this particular quotation; one requested reminder, no marketing."
                    .into(),
            fields: vec!["name".into(), "quantity".into()],
        }),
    )
    .await
    .unwrap();
    recovery::policy(&site.app, id).await.unwrap().unwrap()
}
fn request(policy: &recovery::PublicPolicy, token: &str, key: &str) -> recovery::Request {
    serde_json::from_value(json!({"token":token,"version":2,"policy_id":policy.id,"request_key":key,"email":"visitor@example.test","values":{"name":"Zoë · 日本語","quantity":3},"consent":true})).unwrap()
}
async fn confirmation(site: &Site, id: &str) -> String {
    let text: String = sqlx::query_scalar("SELECT plain FROM mail_jobs WHERE dedupe=$1")
        .bind(format!("recovery-confirm:{id}"))
        .fetch_one(&site.app.db.pool)
        .await
        .unwrap();
    let suffix = text.split("&confirm=").nth(1).unwrap();
    suffix.split_whitespace().next().unwrap().into()
}
async fn count(site: &Site, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&site.app.db.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn anonymous_snapshots_exclude_private_working_copies_actions_and_protected_publications() {
    for pg in engines() {
        let site = Site::new(pg.as_deref(), 10).await;
        let id = site.form().await;
        let s = wpalt::operations::offline::project(&site.app, "form", &id)
            .await
            .unwrap();
        let raw = serde_json::to_string(&s).unwrap();
        assert!(!raw.contains("visitor@example.test"));
        assert_eq!(s.form_version, 2);
        let mut unsafe_form = form();
        unsafe_form.fields[0].widget = Some(wpalt::business::forms::Widget::Upload);
        store::save(&site.app, &id, 2, &unsafe_form, true)
            .await
            .unwrap();
        assert!(
            wpalt::operations::offline::project(&site.app, "form", &id)
                .await
                .is_err(),
            "Private attachment inputs must require a live online visit"
        );
        let post = uuid::Uuid::new_v4().to_string();
        let doc = wpalt::document::import(
            "Public multilingual text · 日本語\n\n<script>literal</script>",
            "[]",
        )
        .unwrap()
        .encode();
        // This cluster isolates anonymous projection; ordinary publishing is covered cumulatively.
        sqlx::query("INSERT INTO posts(id,slug,kind,title,body,document,fields,blocks,status,version,published_slug,published_title,published_body,published_document,published_fields,published_blocks,publish_at,published_at,updated_at,author_id,locale,translation_group,seo,published_locale,published_translation_group,published_seo) VALUES($1,'article','post','PRIVATE WORKING TITLE','PRIVATE WORKING BODY',$2,'{}','[]','published',1,'article','Published title','Published text',$2,'{}','[]',0,1,1,$3,'en','','{}','en','','{}')").bind(&post).bind(doc).bind(&site.owner).execute(&site.app.db.pool).await.unwrap();
        let article = wpalt::operations::offline::project(&site.app, "article", &post)
            .await
            .unwrap();
        assert_eq!(article.title, "Published title");
        assert!(
            !serde_json::to_string(&article)
                .unwrap()
                .contains("PRIVATE WORKING")
        );
        assert!(article.text.contains("日本語"));
        let policy = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO member_policies(id,title) VALUES($1,'Members only')")
            .bind(&policy)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO member_resources(kind,resource_id,policy_id) VALUES('post',$1,$2)",
        )
        .bind(&post)
        .bind(policy)
        .execute(&site.app.db.pool)
        .await
        .unwrap();
        assert!(
            wpalt::operations::offline::project(&site.app, "article", &post)
                .await
                .is_err(),
            "Cookies never authorize a private offline snapshot"
        );
        sqlx::query("DELETE FROM member_resources WHERE resource_id=$1")
            .bind(&post)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE posts SET status='draft' WHERE id=$1")
            .bind(&post)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        assert!(
            wpalt::operations::offline::project(&site.app, "article", &post)
                .await
                .is_err(),
            "Unpublished snapshots refuse"
        );
        site.finish().await;
    }
}

#[tokio::test]
async fn explicit_confirmed_recovery_can_be_withdrawn_or_completed_without_marketing_or_duplicate_reminders()
 {
    for pg in engines() {
        let site = Site::new(pg.as_deref(), 10).await;
        let id = site.form().await;
        let policy = enable(&site, &id).await;
        let token = auth::random_token();
        let key = uuid::Uuid::new_v4().to_string();
        let mut denied = request(&policy, &token, &key);
        denied.consent = false;
        assert!(recovery::request(&site.app, &id, denied).await.is_err());
        let mut unsafe_values = request(&policy, &token, &key);
        unsafe_values.values["private_note"] = json!("never shared");
        assert!(
            recovery::request(&site.app, &id, unsafe_values)
                .await
                .is_err()
        );
        assert_eq!(count(&site, "form_recoveries").await, 0);
        assert_eq!(count(&site, "mail_jobs").await, 0);
        let saved = recovery::request(&site.app, &id, request(&policy, &token, &key))
            .await
            .unwrap();
        assert_eq!(saved.state, "pending");
        assert_eq!(count(&site, "audience_contacts").await, 0);
        assert_eq!(count(&site, "form_entries").await, 0);
        assert_eq!(count(&site, "mail_jobs").await, 1);
        assert!(
            recovery::load(&site.app, &id, &auth::random_token())
                .await
                .is_err()
        );
        assert!(
            recovery::confirm(&site.app, &id, &token, &auth::random_token())
                .await
                .is_err()
        );
        let confirm = confirmation(&site, &saved.id).await;
        assert_eq!(wpalt::business::mail::tick(&site.app).await.unwrap(), 1);
        let confirmation_job: String =
            sqlx::query_scalar("SELECT id FROM mail_jobs WHERE dedupe=$1")
                .bind(format!("recovery-confirm:{}", saved.id))
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        let delivered = wpalt::business::mail::download(&site.app, &confirmation_job)
            .await
            .unwrap();
        assert!(
            !String::from_utf8_lossy(&delivered).contains("Zoë"),
            "Confirmation mail never includes selected answers"
        );
        recovery::confirm(&site.app, &id, &token, &confirm)
            .await
            .unwrap();
        recovery::confirm(&site.app, &id, &token, &confirm)
            .await
            .unwrap();
        assert_eq!(
            count(&site, "mail_jobs").await,
            2,
            "One confirmation and at most one reminder"
        );
        assert_eq!(
            recovery::load(&site.app, &id, &token).await.unwrap().state,
            "confirmed"
        );
        assert_eq!(
            recovery::mail_permitted(&site.app, &format!("recovery-reminder:{}", saved.id))
                .await
                .unwrap(),
            Some(true)
        );
        sqlx::query("UPDATE mail_jobs SET recipient='unrelated@example.test' WHERE dedupe=$1")
            .bind(format!("recovery-reminder:{}", saved.id))
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            recovery::mail_permitted(&site.app, &format!("recovery-reminder:{}", saved.id))
                .await
                .unwrap(),
            Some(false),
            "Queue recipient cannot redirect consent to another person"
        );
        sqlx::query("UPDATE mail_jobs SET recipient='visitor@example.test' WHERE dedupe=$1")
            .bind(format!("recovery-reminder:{}", saved.id))
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        let archive = backup::capture(&site.app).await.unwrap();
        assert_eq!(
            backup::inspect(&site.app.config, &archive).unwrap()["schema"],
            20
        );
        let private_export = wpalt::operations::privacy::export(&site.app, &site.owner)
            .await
            .unwrap();
        assert!(
            !String::from_utf8(private_export)
                .unwrap()
                .contains("visitor@example.test"),
            "Unrelated accounts cannot export someone else's recovery"
        );
        recovery::remove(&site.app, &id, &token).await.unwrap();
        assert!(recovery::load(&site.app, &id, &token).await.is_err());
        assert_eq!(
            recovery::mail_permitted(&site.app, &format!("recovery-reminder:{}", saved.id))
                .await
                .unwrap(),
            Some(false)
        );
        assert_eq!(count(&site, "form_entries").await, 0);
        let token = auth::random_token();
        let key = uuid::Uuid::new_v4().to_string();
        let saved = recovery::request(&site.app, &id, request(&policy, &token, &key))
            .await
            .unwrap();
        let confirm = confirmation(&site, &saved.id).await;
        recovery::confirm(&site.app, &id, &token, &confirm)
            .await
            .unwrap();
        let input = json!({"name":"Completed reviewed response","quantity":4,"private_note":"Submission only"});
        let entry = store::submit(&site.app, &id, 2, &key, &input)
            .await
            .unwrap();
        assert_eq!(
            store::submit(&site.app, &id, 2, &key, &input)
                .await
                .unwrap(),
            entry
        );
        assert!(recovery::load(&site.app, &id, &token).await.is_err());
        assert_eq!(
            recovery::mail_permitted(&site.app, &format!("recovery-reminder:{}", saved.id))
                .await
                .unwrap(),
            Some(false)
        );
        let usage = sqlx::query("SELECT items,bytes FROM business_usage WHERE kind='drafts'")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            (usage.get::<i64, _>("items"), usage.get::<i64, _>("bytes")),
            (0, 0)
        );
        site.finish().await;
    }
}

#[tokio::test]
async fn recovery_races_quota_and_publication_changes_preserve_original_authority() {
    for pg in engines() {
        let site = Site::new(pg.as_deref(), 1).await;
        let id = site.form().await;
        let policy = enable(&site, &id).await;
        let token = auth::random_token();
        let key = uuid::Uuid::new_v4().to_string();
        let (one, two) = tokio::join!(
            recovery::request(&site.app, &id, request(&policy, &token, &key)),
            recovery::request(&site.app, &id, request(&policy, &token, &key))
        );
        assert_eq!(one.unwrap().id, two.unwrap().id);
        assert_eq!(count(&site, "form_recoveries").await, 1);
        assert_eq!(count(&site, "mail_jobs").await, 1);
        assert!(
            recovery::request(
                &site.app,
                &id,
                request(
                    &policy,
                    &auth::random_token(),
                    &uuid::Uuid::new_v4().to_string()
                )
            )
            .await
            .is_err()
        );
        assert_eq!(
            count(&site, "mail_jobs").await,
            1,
            "Quota refusal cannot queue a confirmation"
        );
        let saved = recovery::load(&site.app, &id, &token).await.unwrap();
        let confirmation = confirmation(&site, &saved.id).await;
        store::save(&site.app, &id, 2, &form(), true).await.unwrap();
        assert!(
            recovery::confirm(&site.app, &id, &token, &confirmation)
                .await
                .is_err()
        );
        assert_eq!(
            recovery::load(&site.app, &id, &token)
                .await
                .unwrap()
                .version,
            2,
            "Old source remains private rather than silently remapped"
        );
        assert_eq!(
            recovery::mail_permitted(&site.app, &format!("recovery-confirm:{}", saved.id))
                .await
                .unwrap(),
            Some(false)
        );
        recovery::remove(&site.app, &id, &token).await.unwrap();
        let usage: i64 = sqlx::query_scalar("SELECT items FROM business_usage WHERE kind='drafts'")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(usage, 0);
        let mut dangerous = recovery::Policy {
            purpose: "Specific help".into(),
            fields: vec!["name".into()],
        };
        let mut f = form();
        f.fields[0].widget = Some(wpalt::business::forms::Widget::Upload);
        assert!(dangerous.validate(&f).is_err());
        dangerous.fields.push("name".into());
        assert!(dangerous.validate(&form()).is_err());
        site.finish().await;
    }
}

#[tokio::test]
async fn fresh_recovery_preserves_requested_scope_and_refuses_forged_partial_values() {
    for pg in engines() {
        let source = Site::new(pg.as_deref(), 10).await;
        let id = source.form().await;
        let policy = enable(&source, &id).await;
        let token = auth::random_token();
        let key = uuid::Uuid::new_v4().to_string();
        let original = recovery::request(&source.app, &id, request(&policy, &token, &key))
            .await
            .unwrap();
        let confirm = confirmation(&source, &original.id).await;
        let archive = backup::capture(&source.app).await.unwrap();
        let mut clone_config = (*source.app.config).clone();
        clone_config.base_url = "http://127.0.0.1:4000".into();
        let clone =
            backup::selection::clone_package(&clone_config, &archive, &source.app.config.origin())
                .unwrap();
        let clone_report = backup::inspect(&clone_config, &clone.bytes).unwrap();
        assert_eq!(clone_report["tables"]["form_recoveries"], 0);
        assert_eq!(clone_report["tables"]["form_recovery_policies"], 0);
        let clone_envelope: serde_json::Value = serde_json::from_slice(&clone.bytes).unwrap();
        let clone_graph: serde_json::Value =
            serde_json::from_str(clone_envelope["payload"].as_str().unwrap()).unwrap();
        assert!(
            clone_graph["tables"]["mail_jobs"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["state"] == "cancelled"),
            "Different-origin held clones must obtain fresh recovery consent"
        );
        let mut envelope: serde_json::Value = serde_json::from_slice(&archive).unwrap();
        let mut graph: serde_json::Value =
            serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
        graph["tables"]["form_recoveries"][0]["values_json"] =
            json!("{\"private_note\":\"forged owner permission\"}");
        let payload = serde_json::to_string(&graph).unwrap();
        envelope["sha256"] = json!(auth::digest(payload.as_bytes()));
        envelope["payload"] = json!(payload);
        assert!(
            backup::inspect(&source.app.config, &serde_json::to_vec(&envelope).unwrap()).is_err(),
            "Rehashed archives cannot bypass selected-field consent grammar"
        );
        let destination = Site::build(pg.as_deref(), 10, false).await;
        backup::restore(&destination.app, &archive).await.unwrap();
        let restored = recovery::load(&destination.app, &id, &token).await.unwrap();
        assert_eq!(restored.values, original.values);
        assert_eq!(restored.policy.purpose, original.policy.purpose);
        assert_eq!(restored.expires_at, original.expires_at);
        assert_eq!(
            restored.state, "pending",
            "Recovery cannot manufacture address-confirmed permission"
        );
        recovery::confirm(&destination.app, &id, &token, &confirm)
            .await
            .unwrap();
        assert_eq!(
            recovery::load(&source.app, &id, &token)
                .await
                .unwrap()
                .state,
            "pending",
            "Fresh recovery does not mutate the source"
        );
        let fingerprint = recovery::policy(&destination.app, &id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fingerprint.id, policy.id);
        recovery::publish_policy(&destination.app, &id, 2, None)
            .await
            .unwrap();
        assert_eq!(
            recovery::mail_permitted(
                &destination.app,
                &format!("recovery-reminder:{}", original.id)
            )
            .await
            .unwrap(),
            Some(false)
        );
        destination.finish().await;
        source.finish().await;
    }
}
