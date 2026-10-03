//! Versioned response follow-up and bounded exports.
use crate::{
    App,
    error::{Error, Result},
    now,
};
use sqlx::Row;
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS form_entry_workflows(entry_id TEXT PRIMARY KEY REFERENCES form_entries(id) ON DELETE CASCADE,notes TEXT NOT NULL DEFAULT '',assignee TEXT NOT NULL DEFAULT '',version BIGINT NOT NULL DEFAULT 1,updated_at BIGINT NOT NULL);
"#;
pub async fn follow_up(
    app: &App,
    form: &str,
    entry: &str,
    version: i64,
    notes: &str,
    assignee: &str,
) -> Result<()> {
    if notes.len() > 8000 || version < 1 {
        return Err(Error::invalid("Notes are limited to 8,000 bytes."));
    }
    if !assignee.is_empty() {
        let valid: Option<String> =
            sqlx::query_scalar("SELECT id FROM users WHERE id=$1 AND role IN ('admin','editor')")
                .bind(assignee)
                .fetch_optional(&app.db.pool)
                .await?;
        valid.ok_or_else(|| {
            Error::invalid("Assign responses to an active editor or administrator.")
        })?;
    }
    let mut tx = app.db.pool.begin().await?;
    let valid: Option<String> =
        sqlx::query_scalar("SELECT id FROM form_entries WHERE id=$1 AND form_id=$2")
            .bind(entry)
            .bind(form)
            .fetch_optional(&mut *tx)
            .await?;
    valid.ok_or_else(Error::not_found)?;
    sqlx::query("INSERT INTO form_entry_workflows(entry_id,updated_at) VALUES($1,$2) ON CONFLICT(entry_id) DO NOTHING").bind(entry).bind(now()).execute(&mut *tx).await?;
    if sqlx::query("UPDATE form_entry_workflows SET notes=$1,assignee=$2,version=version+1,updated_at=$3 WHERE entry_id=$4 AND version=$5").bind(notes).bind(assignee).bind(now()).bind(entry).bind(version).execute(&mut *tx).await?.rows_affected()!=1{return Err(Error::conflict());}
    tx.commit().await?;
    Ok(())
}
fn cell(value: &str) -> String {
    // Spreadsheet import must not interpret visitor-controlled text as a formula.
    let prefixed = if value.trim_start().starts_with(['=', '+', '-', '@']) {
        format!("'{value}")
    } else {
        value.into()
    };
    format!("\"{}\"", prefixed.replace('"', "\"\""))
}
pub async fn export(app: &App, form: &str, cursor: &str) -> Result<String> {
    if !cursor.is_empty() && uuid::Uuid::parse_str(cursor).is_err() {
        return Err(Error::invalid("Invalid export cursor."));
    }
    let rows=sqlx::query("SELECT id,form_version,values_json,created_at FROM form_entries WHERE form_id=$1 AND id>$2 ORDER BY id LIMIT 500").bind(form).bind(cursor).fetch_all(&app.db.pool).await?;
    let mut csv = String::from("id,form_version,created_at,values_json\r\n");
    for row in rows {
        csv.push_str(&format!(
            "{},{},{},{}\r\n",
            cell(&row.get::<String, _>("id")),
            row.get::<i64, _>("form_version"),
            row.get::<i64, _>("created_at"),
            cell(&row.get::<String, _>("values_json"))
        ));
    }
    Ok(csv)
}

pub async fn search(
    app: &App,
    form: &str,
    q: &str,
    before: i64,
    after: &str,
) -> Result<Vec<sqlx::any::AnyRow>> {
    if q.len() > 100
        || q.split_whitespace().count() > 10
        || (!after.is_empty() && uuid::Uuid::parse_str(after).is_err())
        || before < 0
    {
        return Err(Error::invalid(
            "Use a search up to 100 bytes and a valid page cursor.",
        ));
    }
    let filter = if before > 0 {
        " AND (e.created_at<$2 OR (e.created_at=$2 AND e.id<$3))"
    } else {
        ""
    };
    let query = if q.trim().is_empty() {
        format!(
            "SELECT e.id,e.created_at,e.form_version FROM form_entries e WHERE e.form_id=$1{filter} ORDER BY e.created_at DESC,e.id DESC LIMIT 41"
        )
    } else if app.db.postgres {
        format!(
            "SELECT e.id,e.created_at,e.form_version FROM form_entries e JOIN form_entry_search s ON s.entry_id=e.id WHERE e.form_id=$1 AND to_tsvector('simple',s.search_text)@@plainto_tsquery('simple',${}){filter} ORDER BY e.created_at DESC,e.id DESC LIMIT 41",
            if before > 0 { 4 } else { 2 }
        )
    } else {
        format!(
            "SELECT e.id,e.created_at,e.form_version FROM form_entry_search_fts JOIN form_entry_search s ON s.rowid=form_entry_search_fts.rowid JOIN form_entries e ON e.id=s.entry_id WHERE e.form_id=$1 AND form_entry_search_fts MATCH ${}{filter} ORDER BY e.created_at DESC,e.id DESC LIMIT 41",
            if before > 0 { 4 } else { 2 }
        )
    };
    let query = sqlx::query(&query).bind(form);
    let query = if before > 0 {
        query.bind(before).bind(after)
    } else {
        query
    };
    let query = if q.trim().is_empty() {
        query
    } else {
        query.bind(if app.db.postgres {
            q.to_string()
        } else {
            q.split_whitespace()
                .map(|word| format!("\"{}\"", word.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(" AND ")
        })
    };
    Ok(query.fetch_all(&app.db.pool).await?)
}
pub const SEARCH_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS form_entry_search(entry_id TEXT PRIMARY KEY REFERENCES form_entries(id) ON DELETE CASCADE,search_text TEXT NOT NULL);";
pub async fn prepare_search(db: &crate::db::Db) -> anyhow::Result<()> {
    sqlx::raw_sql(SEARCH_SCHEMA).execute(&db.pool).await?;
    if db.postgres {
        sqlx::raw_sql("CREATE INDEX IF NOT EXISTS form_entry_search_vector ON form_entry_search USING GIN(to_tsvector('simple',search_text))").execute(&db.pool).await?;
    } else {
        sqlx::raw_sql("CREATE VIRTUAL TABLE IF NOT EXISTS form_entry_search_fts USING fts5(search_text,content='form_entry_search',content_rowid='rowid'); CREATE TRIGGER IF NOT EXISTS form_search_insert AFTER INSERT ON form_entry_search BEGIN INSERT INTO form_entry_search_fts(rowid,search_text) VALUES(new.rowid,new.search_text); END; CREATE TRIGGER IF NOT EXISTS form_search_delete AFTER DELETE ON form_entry_search BEGIN INSERT INTO form_entry_search_fts(form_entry_search_fts,rowid,search_text) VALUES('delete',old.rowid,old.search_text); END; CREATE TRIGGER IF NOT EXISTS form_search_update AFTER UPDATE ON form_entry_search BEGIN INSERT INTO form_entry_search_fts(form_entry_search_fts,rowid,search_text) VALUES('delete',old.rowid,old.search_text); INSERT INTO form_entry_search_fts(rowid,search_text) VALUES(new.rowid,new.search_text); END;").execute(&db.pool).await?;
    }
    sqlx::query("INSERT INTO form_entry_search(entry_id,search_text) SELECT id,values_json FROM form_entries WHERE NOT EXISTS(SELECT 1 FROM form_entry_search s WHERE s.entry_id=form_entries.id)").execute(&db.pool).await?;
    Ok(())
}
