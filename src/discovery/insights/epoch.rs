//! Durable public-render invalidation. Operational queues and 404 counters are
//! deliberately absent; no-op worker cycles must not starve incremental analysis.
use crate::error::Result;
use sqlx::{Any, Transaction};

pub const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS analysis_epoch(id BIGINT PRIMARY KEY CHECK(id=1),version BIGINT NOT NULL CHECK(version>0 AND version<9223372036854775807)); INSERT INTO analysis_epoch(id,version) VALUES(1,1) ON CONFLICT(id) DO NOTHING;";

pub async fn initialize(tx: &mut Transaction<'_, Any>, postgres: bool) -> Result<()> {
    sqlx::raw_sql(SCHEMA).execute(&mut **tx).await?;
    // This inventory is a versioned rendering dependency contract. Adding a new
    // public data source requires extending its triggers and race regression.
    let tables = [
        (
            "posts",
            "status,kind,author_id,published_slug,published_title,published_body,published_fields,published_blocks,published_document,published_locale,published_translation_group,published_seo,published_at",
            "status='published'",
        ),
        (
            "settings",
            "title,description,theme,navigation,field_schema",
            "",
        ),
        ("content_models", "definition,version", ""),
        (
            "engagement_settings",
            "enabled,recording,purpose,version",
            "",
        ),
        ("discovery_settings", "definition,version", ""),
        ("themes", "live,published_version", "published_version>0"),
        ("site_design", "live_options,published_version", ""),
        ("redirects", "source,target,code,version", ""),
        ("redirect_policy", "definition,version", ""),
        ("media", "filename,mime,alt,visibility,size,sha256", ""),
        (
            "member_resources",
            "kind,resource_id,policy_id,opens_at,delay_seconds,course_id,lesson_id",
            "",
        ),
        ("terms", "name,slug,kind", ""),
        ("published_post_terms", "post_id,term_id", ""),
        (
            "comments",
            "post_id,name,body,status,created_at",
            "status='approved'",
        ),
    ];
    if postgres {
        // PostgreSQL WHEN clauses cannot contain subqueries. Unused draft terms
        // have no public rendering effect; inspect their references in the function.
        sqlx::raw_sql("CREATE OR REPLACE FUNCTION wpalt_analysis_changed() RETURNS trigger LANGUAGE plpgsql AS $$ DECLARE analysis_term_identity TEXT; BEGIN IF TG_TABLE_NAME='terms' THEN IF TG_OP='DELETE' THEN analysis_term_identity=OLD.id; ELSE analysis_term_identity=NEW.id; END IF; IF NOT EXISTS(SELECT 1 FROM published_post_terms pt JOIN posts p ON p.id=pt.post_id WHERE pt.term_id=analysis_term_identity AND p.status='published') THEN RETURN NULL; END IF; END IF; UPDATE analysis_epoch SET version=version+1 WHERE id=1; RETURN NULL; END $$;").execute(&mut **tx).await?;
    }
    for (table, columns, visible) in tables {
        for (operation, prefix) in [("INSERT", "NEW"), ("UPDATE", "NEW"), ("DELETE", "OLD")] {
            let changed = if operation == "UPDATE" {
                columns
                    .split(',')
                    .map(|column| {
                        format!(
                            "OLD.{column} {} NEW.{column}",
                            if postgres {
                                "IS DISTINCT FROM"
                            } else {
                                "IS NOT"
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" OR ")
            } else {
                "1=1".into()
            };
            let visibility = if visible.is_empty() {
                "1=1".into()
            } else if operation == "UPDATE" {
                format!("OLD.{visible} OR NEW.{visible}")
            } else {
                format!("{prefix}.{visible}")
            };
            let mut condition = format!("({changed}) AND ({visibility})");
            if table == "terms" && !postgres {
                condition.push_str(&format!(" AND EXISTS(SELECT 1 FROM published_post_terms pt JOIN posts p ON p.id=pt.post_id WHERE pt.term_id={prefix}.id AND p.status='published')"));
            }
            let name = format!("wpalt_analysis_{table}_{}", operation.to_ascii_lowercase());
            let sql = if postgres {
                format!(
                    "DROP TRIGGER IF EXISTS {name} ON {table}; CREATE TRIGGER {name} AFTER {operation} ON {table} FOR EACH ROW WHEN ({condition}) EXECUTE FUNCTION wpalt_analysis_changed();"
                )
            } else {
                format!(
                    "DROP TRIGGER IF EXISTS {name}; CREATE TRIGGER {name} AFTER {operation} ON {table} WHEN {condition} BEGIN UPDATE analysis_epoch SET version=version+1 WHERE id=1; END;"
                )
            };
            sqlx::raw_sql(&sql).execute(&mut **tx).await?;
        }
    }
    Ok(())
}

pub async fn current(app: &crate::App) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT version FROM analysis_epoch WHERE id=1")
            .fetch_one(&app.db.pool)
            .await?,
    )
}
