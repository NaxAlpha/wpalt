use crate::{config::Config, error::Result, model::Settings};
use sqlx::{Any, AnyPool, ConnectOptions, Execute, QueryBuilder, Row, any::AnyPoolOptions};
use std::str::FromStr;

#[derive(Clone)]
pub struct Db {
    pub pool: AnyPool,
    pub postgres: bool,
    pub business_enabled: bool,
    pub membership_enabled: bool,
    pub commerce_enabled: bool,
    pub membership_max_records: i64,
    pub engagement_available: bool,
    pub respect_dnt: bool,
    pub commerce_currency: String,
    pub commerce_max_records: i64,
}
impl Db {
    pub async fn open(config: &Config) -> anyhow::Result<Self> {
        sqlx::any::install_default_drivers();
        let postgres = config.database_url.starts_with("postgres");
        let options = sqlx::any::AnyConnectOptions::from_str(&config.database_url)?
            .log_statements(if config.debug {
                tracing::log::LevelFilter::Debug
            } else {
                tracing::log::LevelFilter::Off
            })
            .log_slow_statements(
                tracing::log::LevelFilter::Warn,
                std::time::Duration::from_millis(100),
            );
        let pool = AnyPoolOptions::new()
            .max_connections(config.database_connections)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .after_connect(move |conn, _| {
                Box::pin(async move {
                    if !postgres {
                        sqlx::query("PRAGMA foreign_keys=ON")
                            .execute(&mut *conn)
                            .await?;
                        sqlx::query("PRAGMA busy_timeout=5000")
                            .execute(&mut *conn)
                            .await?;
                        sqlx::query("PRAGMA journal_mode=WAL")
                            .execute(&mut *conn)
                            .await?;
                    } else {
                        sqlx::query("SET statement_timeout = '10s'")
                            .execute(&mut *conn)
                            .await?;
                    }
                    Ok(())
                })
            })
            .connect_with(options)
            .await?;
        Ok(Self {
            pool,
            postgres,
            business_enabled: config.business_enabled,
            membership_enabled: config.membership_enabled,
            commerce_enabled: config.commerce.enabled,
            membership_max_records: config.membership_max_records,
            engagement_available: config.business_enabled && config.engagement.enabled,
            respect_dnt: config.engagement.respect_dnt,
            commerce_currency: config.commerce.currency.clone(),
            commerce_max_records: config.commerce.max_records,
        })
    }
    pub async fn migrate(&self) -> anyhow::Result<()> {
        sqlx::query("CREATE TABLE IF NOT EXISTS schema_version(id BIGINT PRIMARY KEY CHECK(id=1),version BIGINT NOT NULL)").execute(&self.pool).await?;
        let version: Option<i64> =
            sqlx::query_scalar("SELECT version FROM schema_version WHERE id=1")
                .fetch_optional(&self.pool)
                .await?;
        if version == Some(1) {
            crate::migrations::from_m1(self).await?;
        } else {
            anyhow::ensure!(
                version.is_none()
                    || version == Some(2)
                    || version == Some(3)
                    || version == Some(4)
                    || version == Some(5)
                    || version == Some(6)
                    || version == Some(7)
                    || version == Some(8)
                    || version == Some(9),
                "unsupported schema version; use the documented migration/reset path"
            );
        }
        if version == Some(1) || version == Some(2) {
            crate::migrations::from_m2(self).await?;
        }
        if matches!(version, Some(1..=3)) {
            crate::migrations::from_m3(self).await?;
        }
        if version.is_some_and(|v| v < 7) {
            crate::migrations::from_m4(self).await?;
        }
        let mut tx = self.pool.begin().await?;
        sqlx::raw_sql(SCHEMA).execute(&mut *tx).await?;
        sqlx::raw_sql(crate::business::store::SCHEMA)
            .execute(&mut *tx)
            .await?;
        sqlx::raw_sql(crate::business::audience::SCHEMA)
            .execute(&mut *tx)
            .await?;
        sqlx::raw_sql(crate::business::campaigns::SCHEMA)
            .execute(&mut *tx)
            .await?;
        sqlx::raw_sql(crate::business::drafts::SCHEMA)
            .execute(&mut *tx)
            .await?;
        sqlx::raw_sql(crate::business::attachments::SCHEMA)
            .execute(&mut *tx)
            .await?;
        sqlx::raw_sql(crate::business::entries::SCHEMA)
            .execute(&mut *tx)
            .await?;
        sqlx::raw_sql(crate::business::entries::SEARCH_SCHEMA)
            .execute(&mut *tx)
            .await?;
        sqlx::raw_sql(crate::business::engagement::SCHEMA)
            .execute(&mut *tx)
            .await?;
        sqlx::raw_sql(crate::business::engagement::DIMENSION_SCHEMA)
            .execute(&mut *tx)
            .await?;
        sqlx::raw_sql(crate::business::promotions::SCHEMA)
            .execute(&mut *tx)
            .await?;
        // SQLite metadata columns after large documents can still require overflow-page
        // traversal even when documents are absent from SELECT. Cover the bounded
        // targeting/listing projections. PostgreSQL keeps large values out of line;
        // do not put large target JSON in its size-limited B-tree tuples.
        let targeting_index = if self.postgres {
            "CREATE INDEX IF NOT EXISTS promotion_targeting ON business_promotions(active,created_at,id)"
        } else {
            "CREATE INDEX IF NOT EXISTS promotion_targeting ON business_promotions(active,created_at,id,title,target,experiment,wheel)"
        };
        let listing_index = if self.postgres {
            "CREATE INDEX IF NOT EXISTS promotion_listing ON business_promotions(created_at DESC,id)"
        } else {
            "CREATE INDEX IF NOT EXISTS promotion_listing ON business_promotions(created_at DESC,id,title,active)"
        };
        for statement in [targeting_index, listing_index] {
            sqlx::query(statement).execute(&mut *tx).await?;
        }
        sqlx::raw_sql(crate::business::registration::SCHEMA)
            .execute(&mut *tx)
            .await?;
        sqlx::raw_sql(crate::business::workflows::SCHEMA)
            .execute(&mut *tx)
            .await?;
        sqlx::raw_sql(crate::business::mail::SCHEMA)
            .execute(&mut *tx)
            .await?;
        sqlx::raw_sql(crate::business::quotas::SCHEMA)
            .execute(&mut *tx)
            .await?;
        crate::business::quotas::initialize(&mut tx, self.postgres)
            .await
            .map_err(|e| anyhow::anyhow!(e.1))?;
        sqlx::raw_sql(crate::membership::identity::SCHEMA)
            .execute(&mut *tx)
            .await?;
        sqlx::raw_sql(crate::membership::SCHEMA)
            .execute(&mut *tx)
            .await?;
        let reserved:i64=sqlx::query_scalar("SELECT COUNT(*) FROM posts WHERE slug IN ('shop','commerce') OR published_slug IN ('shop','commerce')").fetch_one(&mut *tx).await?;
        anyhow::ensure!(
            reserved == 0,
            "Commerce reserves /shop and /commerce. Rename conflicting content with the matching older runtime before upgrading"
        );
        sqlx::raw_sql(crate::commerce::SCHEMA)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO shop_settings(id,currency) VALUES(1,$1) ON CONFLICT(id) DO NOTHING",
        )
        .bind(&self.commerce_currency)
        .execute(&mut *tx)
        .await?;
        let currency: String = sqlx::query_scalar("SELECT currency FROM shop_settings WHERE id=1")
            .fetch_one(&mut *tx)
            .await?;
        anyhow::ensure!(
            currency == self.commerce_currency,
            "Commerce currency differs from stored catalog; use matching configuration or an explicit migration"
        );
        sqlx::query("UPDATE schema_version SET version=9 WHERE id=1")
            .execute(&mut *tx)
            .await?;
        if self.postgres {
            sqlx::query("CREATE INDEX IF NOT EXISTS public_search ON posts USING GIN(to_tsvector('simple',published_title || ' ' || published_body)) WHERE status='published'").execute(&mut *tx).await?;
        } else {
            sqlx::raw_sql(SQLITE_SEARCH).execute(&mut *tx).await?;
            // Rank compact indexable identities before reading rich content rows.
            sqlx::raw_sql("CREATE INDEX IF NOT EXISTS indexable_identity ON posts(id,published_locale,published_at DESC) WHERE status='published' AND COALESCE(json_extract(published_seo,'$.noindex'),0)=0;").execute(&mut *tx).await?;
        }
        sqlx::query("INSERT INTO discovery_settings(id,definition,version) VALUES(1,$1,1) ON CONFLICT(id) DO NOTHING").bind(serde_json::to_string(&crate::discovery::Definition::default())?).execute(&mut *tx).await?;
        tx.commit().await?;
        crate::business::entries::prepare_search(self).await?;
        crate::membership::budget::prepare(self)
            .await
            .map_err(|e| anyhow::anyhow!(e.1))?;
        crate::commerce::budget::prepare(self)
            .await
            .map_err(|e| anyhow::anyhow!(e.1))?;
        Ok(())
    }
    /// Run bounded planner/index maintenance after an offline bulk operation.
    /// The imported data is already committed: callers must report failure as a warning.
    pub async fn optimize_after_bulk_write(&self) -> anyhow::Result<()> {
        if self.postgres {
            sqlx::query("VACUUM (ANALYZE) posts")
                .execute(&self.pool)
                .await?;
        } else {
            sqlx::query("PRAGMA optimize").execute(&self.pool).await?;
        }
        Ok(())
    }
    // Any's QueryBuilder emits '?' regardless of the chosen driver. These builders
    // contain only static SQL fragments; values are separate bound arguments.
    pub fn numbered(sql: &str) -> String {
        let mut n = 0;
        sql.chars()
            .map(|c| {
                if c == '?' {
                    n += 1;
                    format!("${n}")
                } else {
                    c.to_string()
                }
            })
            .collect()
    }
    pub async fn fetch_builder(
        &self,
        builder: &mut QueryBuilder<'_, Any>,
    ) -> std::result::Result<Vec<sqlx::any::AnyRow>, sqlx::Error> {
        let mut query = builder.build();
        let sql = Self::numbered(query.sql());
        let args = query
            .take_arguments()
            .map_err(sqlx::Error::Encode)?
            .unwrap_or_default();
        sqlx::query_with(&sql, args).fetch_all(&self.pool).await
    }
    pub async fn settings(&self) -> Result<Settings> {
        let r = sqlx::query(
            "SELECT s.title,s.description,s.theme,s.navigation,s.field_schema,e.enabled AS analytics_enabled,e.recording AS analytics_recording,e.purpose AS analytics_purpose,e.version AS analytics_policy FROM settings s JOIN engagement_settings e ON e.id=1 WHERE s.id=1",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(Settings {
            business_enabled: self.business_enabled,
            membership_enabled: self.membership_enabled,
            commerce_enabled: self.commerce_enabled,
            engagement_available: self.engagement_available,
            analytics: if self.engagement_available && r.get::<i64, _>("analytics_enabled") == 1 {
                Some(crate::business::engagement::PublicState {
                    purpose: r.get("analytics_purpose"),
                    policy: r.get("analytics_policy"),
                    recording: r.get::<i64, _>("analytics_recording") == 1,
                    respect_dnt: self.respect_dnt,
                })
            } else {
                None
            },
            title: r.get("title"),
            description: r.get("description"),
            theme: r.get("theme"),
            navigation: r.get("navigation"),
            field_schema: r.get("field_schema"),
        })
    }
}

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS schema_version(id BIGINT PRIMARY KEY CHECK(id=1), version BIGINT NOT NULL);
INSERT INTO schema_version(id,version) VALUES(1,4) ON CONFLICT(id) DO NOTHING;
CREATE TABLE IF NOT EXISTS settings(id BIGINT PRIMARY KEY CHECK(id=1),title TEXT NOT NULL,description TEXT NOT NULL,theme TEXT NOT NULL,navigation TEXT NOT NULL,field_schema TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS users(id TEXT PRIMARY KEY,email TEXT NOT NULL UNIQUE,name TEXT NOT NULL,role TEXT NOT NULL CHECK(role IN ('admin','editor','moderator','subscriber','disabled')),password_hash TEXT NOT NULL,created_at BIGINT NOT NULL);
CREATE TABLE IF NOT EXISTS sessions(token_hash TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,csrf TEXT NOT NULL,expires_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS user_sessions ON sessions(user_id,expires_at);
CREATE INDEX IF NOT EXISTS session_expiry ON sessions(expires_at);
CREATE TABLE IF NOT EXISTS posts(id TEXT PRIMARY KEY,slug TEXT NOT NULL UNIQUE,kind TEXT NOT NULL,title TEXT NOT NULL,body TEXT NOT NULL,fields TEXT NOT NULL,blocks TEXT NOT NULL,status TEXT NOT NULL CHECK(status IN ('draft','published','scheduled')),version BIGINT NOT NULL,published_slug TEXT NOT NULL,published_title TEXT NOT NULL,published_body TEXT NOT NULL,published_fields TEXT NOT NULL,published_blocks TEXT NOT NULL,publish_at BIGINT NOT NULL,published_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,author_id TEXT NOT NULL REFERENCES users(id),locale TEXT NOT NULL DEFAULT 'en',translation_group TEXT NOT NULL DEFAULT '',seo TEXT NOT NULL DEFAULT '{}',published_locale TEXT NOT NULL DEFAULT 'en',published_translation_group TEXT NOT NULL DEFAULT '',published_seo TEXT NOT NULL DEFAULT '{}',document TEXT NOT NULL DEFAULT '',published_document TEXT NOT NULL DEFAULT '');
CREATE UNIQUE INDEX IF NOT EXISTS published_slugs ON posts(published_slug) WHERE published_slug<>'';
CREATE INDEX IF NOT EXISTS public_posts ON posts(status,published_at DESC,id DESC);
CREATE INDEX IF NOT EXISTS public_model_posts ON posts(kind,status,published_at DESC,id DESC);
CREATE INDEX IF NOT EXISTS language_posts ON posts(published_locale,status,published_at DESC,id DESC);
CREATE UNIQUE INDEX IF NOT EXISTS translation_drafts ON posts(translation_group,locale) WHERE translation_group<>'';
CREATE UNIQUE INDEX IF NOT EXISTS translation_live ON posts(published_translation_group,published_locale) WHERE status='published' AND published_translation_group<>'';
CREATE INDEX IF NOT EXISTS admin_posts ON posts(updated_at DESC,id DESC);
CREATE INDEX IF NOT EXISTS scheduled_posts ON posts(publish_at) WHERE status='scheduled';
CREATE TABLE IF NOT EXISTS revisions(id TEXT PRIMARY KEY,post_id TEXT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,version BIGINT NOT NULL,snapshot TEXT NOT NULL,created_at BIGINT NOT NULL, UNIQUE(post_id,version));
CREATE INDEX IF NOT EXISTS published_sitemap ON posts(status,id);
CREATE INDEX IF NOT EXISTS revision_history ON revisions(post_id,version DESC);
CREATE TABLE IF NOT EXISTS terms(id TEXT PRIMARY KEY,name TEXT NOT NULL,slug TEXT NOT NULL,kind TEXT NOT NULL,UNIQUE(kind,slug));
CREATE TABLE IF NOT EXISTS post_terms(post_id TEXT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,term_id TEXT NOT NULL REFERENCES terms(id) ON DELETE CASCADE,PRIMARY KEY(post_id,term_id));
CREATE TABLE IF NOT EXISTS published_post_terms(post_id TEXT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,term_id TEXT NOT NULL REFERENCES terms(id) ON DELETE CASCADE,PRIMARY KEY(post_id,term_id));
CREATE INDEX IF NOT EXISTS published_term_posts ON published_post_terms(term_id,post_id);
CREATE INDEX IF NOT EXISTS term_posts ON post_terms(term_id,post_id);
CREATE TABLE IF NOT EXISTS media(id TEXT PRIMARY KEY,filename TEXT NOT NULL UNIQUE,original_name TEXT NOT NULL,mime TEXT NOT NULL,alt TEXT NOT NULL,visibility TEXT NOT NULL CHECK(visibility IN ('public','private')),size BIGINT NOT NULL,sha256 TEXT NOT NULL,created_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS recent_media ON media(created_at DESC,id DESC);
CREATE TABLE IF NOT EXISTS comments(id TEXT PRIMARY KEY,post_id TEXT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,name TEXT NOT NULL,body TEXT NOT NULL,status TEXT NOT NULL CHECK(status IN ('pending','approved','rejected')),created_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS public_comments ON comments(post_id,status,created_at);
CREATE INDEX IF NOT EXISTS pending_comments ON comments(status,created_at);
CREATE TABLE IF NOT EXISTS content_models(id TEXT PRIMARY KEY,definition TEXT NOT NULL,version BIGINT NOT NULL);
CREATE TABLE IF NOT EXISTS site_design(id BIGINT PRIMARY KEY CHECK(id=1),draft_options TEXT NOT NULL,live_options TEXT NOT NULL,version BIGINT NOT NULL,published_version BIGINT NOT NULL);
CREATE TABLE IF NOT EXISTS themes(id TEXT PRIMARY KEY,name TEXT NOT NULL,draft TEXT NOT NULL,live TEXT NOT NULL,version BIGINT NOT NULL,published_version BIGINT NOT NULL,updated_at BIGINT NOT NULL);
CREATE TABLE IF NOT EXISTS theme_revisions(id TEXT PRIMARY KEY,theme_id TEXT NOT NULL REFERENCES themes(id) ON DELETE CASCADE,version BIGINT NOT NULL,package TEXT NOT NULL,published BIGINT NOT NULL CHECK(published IN (0,1)),created_at BIGINT NOT NULL,UNIQUE(theme_id,version));
CREATE INDEX IF NOT EXISTS theme_history ON theme_revisions(theme_id,version DESC);
CREATE TABLE IF NOT EXISTS discovery_settings(id BIGINT PRIMARY KEY CHECK(id=1),definition TEXT NOT NULL,version BIGINT NOT NULL);
CREATE TABLE IF NOT EXISTS redirects(source TEXT PRIMARY KEY,target TEXT NOT NULL,code BIGINT NOT NULL CHECK(code IN (301,302)),version BIGINT NOT NULL);
CREATE TABLE IF NOT EXISTS comment_limits(client_hash TEXT PRIMARY KEY,last_at BIGINT NOT NULL);
"#;

pub(crate) const SQLITE_SEARCH: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS post_search USING fts5(id UNINDEXED,title,body);
CREATE TRIGGER IF NOT EXISTS post_search_insert AFTER INSERT ON posts WHEN new.status='published' BEGIN
 INSERT INTO post_search(id,title,body) VALUES(new.id,new.published_title,new.published_body); END;
CREATE TRIGGER IF NOT EXISTS post_search_update AFTER UPDATE OF status,published_title,published_body ON posts WHEN old.status<>new.status OR old.published_title<>new.published_title OR old.published_body<>new.published_body BEGIN
 DELETE FROM post_search WHERE id=old.id;
 INSERT INTO post_search(id,title,body) SELECT new.id,new.published_title,new.published_body WHERE new.status='published'; END;
CREATE TRIGGER IF NOT EXISTS post_search_delete AFTER DELETE ON posts BEGIN DELETE FROM post_search WHERE id=old.id; END;
"#;
