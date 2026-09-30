//! Read-only verification of the same hot-query shapes used by M1.
//! DATABASE_URL must identify an existing, isolated review fixture.
use sqlx::Row;
use wpalt::{config::Config, db::Db};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config {
        database_url: std::env::var("DATABASE_URL")?,
        ..Config::default()
    };
    let db = Db::open(&config).await?;
    let explain = if db.postgres {
        "EXPLAIN (ANALYZE, BUFFERS) "
    } else {
        "EXPLAIN QUERY PLAN "
    };
    let queries = [
        (
            "public keyset",
            "SELECT id,published_title FROM posts WHERE status='published' AND (published_at,id)<(2000000000,'ffffffff') ORDER BY published_at DESC,id DESC LIMIT 21",
        ),
        (
            "due schedule",
            "SELECT * FROM posts WHERE status='scheduled' AND publish_at<=2000000000 ORDER BY publish_at LIMIT 50",
        ),
        (
            "session",
            "SELECT u.id,u.email,u.name,u.role,s.csrf FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash='verification-missing' AND s.expires_at>0 AND u.role<>'disabled'",
        ),
        (
            "search",
            if db.postgres {
                "SELECT id,published_title FROM posts WHERE status='published' AND to_tsvector('simple',published_title || ' ' || published_body) @@ plainto_tsquery('simple','publishing') ORDER BY published_at DESC,id DESC LIMIT 21"
            } else {
                "SELECT id,published_title FROM posts WHERE status='published' AND id IN (SELECT id FROM post_search WHERE post_search MATCH 'publishing') ORDER BY published_at DESC,id DESC LIMIT 21"
            },
        ),
    ];
    println!(
        "Engine: {}",
        if db.postgres { "PostgreSQL" } else { "SQLite" }
    );
    for (name, query) in queries {
        println!("\n{name}\n{query}");
        for row in sqlx::query(&format!("{explain}{query}"))
            .fetch_all(&db.pool)
            .await?
        {
            let index = if db.postgres { 0 } else { 3 };
            println!("{}", row.try_get::<String, _>(index)?);
        }
    }
    Ok(())
}
