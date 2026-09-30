use crate::{config::Config, error::Result, model::Settings};
use sqlx::{Any, AnyPool, ConnectOptions, Execute, QueryBuilder, Row, any::AnyPoolOptions};
use std::str::FromStr;

#[derive(Clone)]
pub struct Db {
    pub pool: AnyPool,
    pub postgres: bool,
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
        Ok(Self { pool, postgres })
    }
    pub async fn migrate(&self) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::raw_sql(SCHEMA).execute(&mut *tx).await?;
        let v: i64 = sqlx::query_scalar("SELECT version FROM schema_version WHERE id=1")
            .fetch_one(&mut *tx)
            .await?;
        anyhow::ensure!(
            v == 1,
            "unsupported schema version; use the documented migration/reset path"
        );
        if self.postgres {
            sqlx::query("CREATE INDEX IF NOT EXISTS public_search ON posts USING GIN(to_tsvector('simple',published_title || ' ' || published_body)) WHERE status='published'").execute(&mut *tx).await?;
        } else {
            sqlx::raw_sql(SQLITE_SEARCH).execute(&mut *tx).await?;
        }
        tx.commit().await?;
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
            "SELECT title,description,theme,navigation,field_schema FROM settings WHERE id=1",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(Settings {
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
INSERT INTO schema_version(id,version) VALUES(1,1) ON CONFLICT(id) DO NOTHING;
CREATE TABLE IF NOT EXISTS settings(id BIGINT PRIMARY KEY CHECK(id=1),title TEXT NOT NULL,description TEXT NOT NULL,theme TEXT NOT NULL CHECK(theme IN ('paper','ink')),navigation TEXT NOT NULL,field_schema TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS users(id TEXT PRIMARY KEY,email TEXT NOT NULL UNIQUE,name TEXT NOT NULL,role TEXT NOT NULL CHECK(role IN ('admin','editor','moderator','disabled')),password_hash TEXT NOT NULL,created_at BIGINT NOT NULL);
CREATE TABLE IF NOT EXISTS sessions(token_hash TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,csrf TEXT NOT NULL,expires_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS user_sessions ON sessions(user_id,expires_at);
CREATE INDEX IF NOT EXISTS session_expiry ON sessions(expires_at);
CREATE TABLE IF NOT EXISTS posts(id TEXT PRIMARY KEY,slug TEXT NOT NULL UNIQUE,kind TEXT NOT NULL CHECK(kind IN ('post','page')),title TEXT NOT NULL,body TEXT NOT NULL,fields TEXT NOT NULL,blocks TEXT NOT NULL,status TEXT NOT NULL CHECK(status IN ('draft','published','scheduled')),version BIGINT NOT NULL,published_slug TEXT NOT NULL,published_title TEXT NOT NULL,published_body TEXT NOT NULL,published_fields TEXT NOT NULL,published_blocks TEXT NOT NULL,publish_at BIGINT NOT NULL,published_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,author_id TEXT NOT NULL REFERENCES users(id));
CREATE UNIQUE INDEX IF NOT EXISTS published_slugs ON posts(published_slug) WHERE published_slug<>'';
CREATE INDEX IF NOT EXISTS public_posts ON posts(status,published_at DESC,id DESC);
CREATE INDEX IF NOT EXISTS admin_posts ON posts(updated_at DESC,id DESC);
CREATE INDEX IF NOT EXISTS scheduled_posts ON posts(publish_at) WHERE status='scheduled';
CREATE TABLE IF NOT EXISTS revisions(id TEXT PRIMARY KEY,post_id TEXT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,version BIGINT NOT NULL,snapshot TEXT NOT NULL,created_at BIGINT NOT NULL, UNIQUE(post_id,version));
CREATE INDEX IF NOT EXISTS revision_history ON revisions(post_id,version DESC);
CREATE TABLE IF NOT EXISTS terms(id TEXT PRIMARY KEY,name TEXT NOT NULL,slug TEXT NOT NULL,kind TEXT NOT NULL CHECK(kind IN ('category','tag')),UNIQUE(kind,slug));
CREATE TABLE IF NOT EXISTS post_terms(post_id TEXT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,term_id TEXT NOT NULL REFERENCES terms(id) ON DELETE CASCADE,PRIMARY KEY(post_id,term_id));
CREATE TABLE IF NOT EXISTS published_post_terms(post_id TEXT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,term_id TEXT NOT NULL REFERENCES terms(id) ON DELETE CASCADE,PRIMARY KEY(post_id,term_id));
CREATE INDEX IF NOT EXISTS published_term_posts ON published_post_terms(term_id,post_id);
CREATE INDEX IF NOT EXISTS term_posts ON post_terms(term_id,post_id);
CREATE TABLE IF NOT EXISTS media(id TEXT PRIMARY KEY,filename TEXT NOT NULL UNIQUE,original_name TEXT NOT NULL,mime TEXT NOT NULL,alt TEXT NOT NULL,visibility TEXT NOT NULL CHECK(visibility IN ('public','private')),size BIGINT NOT NULL,sha256 TEXT NOT NULL,created_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS recent_media ON media(created_at DESC,id DESC);
CREATE TABLE IF NOT EXISTS comments(id TEXT PRIMARY KEY,post_id TEXT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,name TEXT NOT NULL,body TEXT NOT NULL,status TEXT NOT NULL CHECK(status IN ('pending','approved','rejected')),created_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS public_comments ON comments(post_id,status,created_at);
CREATE INDEX IF NOT EXISTS pending_comments ON comments(status,created_at);
CREATE TABLE IF NOT EXISTS comment_limits(client_hash TEXT PRIMARY KEY,last_at BIGINT NOT NULL);
"#;

const SQLITE_SEARCH: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS post_search USING fts5(id UNINDEXED,title,body);
CREATE TRIGGER IF NOT EXISTS post_search_insert AFTER INSERT ON posts WHEN new.status='published' BEGIN
 INSERT INTO post_search(id,title,body) VALUES(new.id,new.published_title,new.published_body); END;
CREATE TRIGGER IF NOT EXISTS post_search_update AFTER UPDATE OF status,published_title,published_body ON posts WHEN old.status<>new.status OR old.published_title<>new.published_title OR old.published_body<>new.published_body BEGIN
 DELETE FROM post_search WHERE id=old.id;
 INSERT INTO post_search(id,title,body) SELECT new.id,new.published_title,new.published_body WHERE new.status='published'; END;
CREATE TRIGGER IF NOT EXISTS post_search_delete AFTER DELETE ON posts BEGIN DELETE FROM post_search WHERE id=old.id; END;
"#;
