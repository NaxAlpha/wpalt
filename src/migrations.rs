//! One-off data preservation for pre-adoption schema 1, never a runtime parser branch.
use crate::{
    db::{Db, SCHEMA},
    schema::{Definition, Field, Model},
};
use sqlx::{Any, Connection, Row, Transaction};

pub async fn initialize_design(tx: &mut Transaction<'_, Any>) -> anyhow::Result<()> {
    for (id, label) in [("post", "Posts"), ("page", "Pages")] {
        sqlx::query("INSERT INTO content_models(id,definition,version) VALUES($1,$2,1)")
            .bind(id)
            .bind(serde_json::to_string(&Model::initial(label))?)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query("INSERT INTO site_design(id,draft_options,live_options,version,published_version) VALUES(1,'{}','{}',1,1)").execute(&mut **tx).await?;
    for (id, package) in [
        ("paper", include_str!("../assets/themes/paper.json")),
        ("ink", include_str!("../assets/themes/ink.json")),
    ] {
        let name = if id == "paper" { "Paper" } else { "Ink" };
        sqlx::query("INSERT INTO themes(id,name,draft,live,version,published_version,updated_at) VALUES($1,$2,$3,$3,1,1,$4)").bind(id).bind(name).bind(package).bind(crate::now()).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO theme_revisions(id,theme_id,version,package,published,created_at) VALUES($1,$2,1,$3,1,$4)").bind(uuid::Uuid::new_v4().to_string()).bind(id).bind(package).bind(crate::now()).execute(&mut **tx).await?;
    }
    Ok(())
}
pub async fn from_m1(db: &Db) -> anyhow::Result<()> {
    let mut connection = db.pool.acquire().await?;
    if !db.postgres {
        sqlx::query("PRAGMA foreign_keys=OFF")
            .execute(&mut *connection)
            .await?;
    }
    let result=async {
        let mut tx=connection.begin().await?;
        if db.postgres {
            for statement in ["ALTER TABLE posts DROP CONSTRAINT posts_kind_check","ALTER TABLE settings DROP CONSTRAINT settings_theme_check","ALTER TABLE terms DROP CONSTRAINT terms_kind_check"] {sqlx::query(statement).execute(&mut *tx).await?;}
        }else{
            // Never rename the old parent first: that rewrites child FK targets.
            for table in ["posts","settings","terms"] {
                let prefix=format!("CREATE TABLE IF NOT EXISTS {table}(");
                let create=SCHEMA.lines().find(|line|line.starts_with(&prefix)).ok_or_else(||anyhow::anyhow!("migration table definition missing"))?;
                let create=create.replace(",document TEXT NOT NULL DEFAULT '',published_document TEXT NOT NULL DEFAULT ''", "");
                let create=create.replace(",locale TEXT NOT NULL DEFAULT 'en',translation_group TEXT NOT NULL DEFAULT '',seo TEXT NOT NULL DEFAULT '{}',published_locale TEXT NOT NULL DEFAULT 'en',published_translation_group TEXT NOT NULL DEFAULT '',published_seo TEXT NOT NULL DEFAULT '{}'", "");
                sqlx::raw_sql(&create.replacen(&prefix,&format!("CREATE TABLE {table}_m2("),1)).execute(&mut *tx).await?;
                sqlx::query(&format!("INSERT INTO {table}_m2 SELECT * FROM {table}")).execute(&mut *tx).await?;
                sqlx::query(&format!("DROP TABLE {table}")).execute(&mut *tx).await?;
                sqlx::query(&format!("ALTER TABLE {table}_m2 RENAME TO {table}")).execute(&mut *tx).await?;
            }
        }
        sqlx::raw_sql(&SCHEMA.lines().filter(|line| !line.contains("language_posts") && !line.contains("translation_drafts") && !line.contains("translation_live")).collect::<Vec<_>>().join("\n")).execute(&mut *tx).await?;
        if let Some(row)=sqlx::query("SELECT field_schema FROM settings WHERE id=1").fetch_optional(&mut *tx).await? {
            let old:std::collections::BTreeMap<String,String>=serde_json::from_str(&row.get::<String,_>("field_schema"))?;
            let mut definition=Definition::default();
            for (name,kind) in old {anyhow::ensure!(crate::schema::identifier(&name)&&["string","number","boolean"].contains(&kind.as_str()),"M1 field definitions need operator correction before migration");definition.fields.insert(name,Field::primitive(&kind));}
            sqlx::query("UPDATE settings SET field_schema=$1 WHERE id=1").bind(serde_json::to_string(&definition)?).execute(&mut *tx).await?;
            initialize_design(&mut tx).await?;
        }
        // Convert preserved revision envelopes once, rather than retaining an old
        // format branch in ordinary restore requests. Bounded keyset batches.
        let mut after=String::new();
        loop {
            let rows=sqlx::query("SELECT id,snapshot FROM revisions WHERE id>$1 ORDER BY id LIMIT 10")
                .bind(&after).fetch_all(&mut *tx).await?;
            if rows.is_empty(){break;}
            for row in rows {
                let id:String=row.get("id");
                let mut snapshot:serde_json::Value=serde_json::from_str(&row.get::<String,_>("snapshot"))?;
                snapshot.as_object_mut().ok_or_else(||anyhow::anyhow!("M1 revision is not an object"))?.insert("taxonomies".into(),serde_json::json!({}));
                sqlx::query("UPDATE revisions SET snapshot=$1 WHERE id=$2").bind(snapshot.to_string()).bind(&id).execute(&mut *tx).await?;
                after=id;
            }
        }
        if !db.postgres {
            sqlx::raw_sql(crate::db::SQLITE_SEARCH).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM post_search").execute(&mut *tx).await?;
            sqlx::query("INSERT INTO post_search(id,title,body) SELECT id,published_title,published_body FROM posts WHERE status='published'").execute(&mut *tx).await?;
            anyhow::ensure!(sqlx::query("PRAGMA foreign_key_check").fetch_all(&mut *tx).await?.is_empty(),"migration foreign-key validation failed");
        }
        sqlx::query("UPDATE schema_version SET version=2 WHERE id=1").execute(&mut *tx).await?;
        tx.commit().await?;
        Ok::<(),anyhow::Error>(())
    }.await;
    if !db.postgres {
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&mut *connection)
            .await?;
    }
    result
}

/// Preserve M2 content and revision metadata once; no old parser in request paths.
pub async fn from_m2(db: &Db) -> anyhow::Result<()> {
    let mut tx = db.pool.begin().await?;
    for (name, default) in [
        ("locale", "en"),
        ("translation_group", ""),
        ("seo", "{}"),
        ("published_locale", "en"),
        ("published_translation_group", ""),
        ("published_seo", "{}"),
    ] {
        sqlx::query(&format!(
            "ALTER TABLE posts ADD COLUMN {name} TEXT NOT NULL DEFAULT '{default}'"
        ))
        .execute(&mut *tx)
        .await?;
    }
    let mut after = String::new();
    loop {
        let rows =
            sqlx::query("SELECT id,snapshot FROM revisions WHERE id>$1 ORDER BY id LIMIT 20")
                .bind(&after)
                .fetch_all(&mut *tx)
                .await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            let id: String = row.get("id");
            let mut value: serde_json::Value =
                serde_json::from_str(&row.get::<String, _>("snapshot"))?;
            let post = value["post"]
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("invalid stored revision"))?;
            for (name, default) in [
                ("locale", "en"),
                ("translation_group", ""),
                ("seo", "{}"),
                ("published_locale", "en"),
                ("published_translation_group", ""),
                ("published_seo", "{}"),
            ] {
                post.insert(name.into(), default.into());
            }
            sqlx::query("UPDATE revisions SET snapshot=$1 WHERE id=$2")
                .bind(value.to_string())
                .bind(&id)
                .execute(&mut *tx)
                .await?;
            after = id;
        }
    }
    sqlx::query("UPDATE schema_version SET version=3 WHERE id=1")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Atomic, bounded conversion of working/live documents and retained revisions.
pub async fn from_m3(db: &Db) -> anyhow::Result<()> {
    let mut tx = db.pool.begin().await?;
    for col in ["document", "published_document"] {
        sqlx::query(&format!(
            "ALTER TABLE posts ADD COLUMN {col} TEXT NOT NULL DEFAULT ''"
        ))
        .execute(&mut *tx)
        .await?;
    }
    let mut after = String::new();
    loop {
        let rows=sqlx::query("SELECT id,body,blocks,published_body,published_blocks FROM posts WHERE id>$1 ORDER BY id LIMIT 20").bind(&after).fetch_all(&mut *tx).await?;
        if rows.is_empty() {
            break;
        }
        for r in rows {
            let id: String = r.get("id");
            let draft =
                crate::document::import(&r.get::<String, _>("body"), &r.get::<String, _>("blocks"))
                    .map_err(|_| {
                        anyhow::anyhow!(
                            "document migration failed for {id}; original transaction rolled back"
                        )
                    })?;
            let live = crate::document::import(
                &r.get::<String, _>("published_body"),
                &r.get::<String, _>("published_blocks"),
            )
            .map_err(|_| anyhow::anyhow!("live document migration failed for {id}"))?;
            sqlx::query("UPDATE posts SET document=$1,published_document=$2 WHERE id=$3")
                .bind(draft.encode())
                .bind(live.encode())
                .bind(&id)
                .execute(&mut *tx)
                .await?;
            after = id;
        }
    }
    after.clear();
    loop {
        let rows =
            sqlx::query("SELECT id,snapshot FROM revisions WHERE id>$1 ORDER BY id LIMIT 20")
                .bind(&after)
                .fetch_all(&mut *tx)
                .await?;
        if rows.is_empty() {
            break;
        }
        for r in rows {
            let id: String = r.get("id");
            let mut v: serde_json::Value = serde_json::from_str(&r.get::<String, _>("snapshot"))?;
            for (doc, body, blocks) in [
                ("document", "body", "blocks"),
                ("published_document", "published_body", "published_blocks"),
            ] {
                let p = &mut v["post"];
                let d = crate::document::import(
                    p[body]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("missing revision body"))?,
                    p[blocks]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("missing revision blocks"))?,
                )
                .map_err(|_| anyhow::anyhow!("revision document migration failed for {id}"))?;
                p[doc] = d.encode().into();
            }
            sqlx::query("UPDATE revisions SET snapshot=$1 WHERE id=$2")
                .bind(v.to_string())
                .bind(&id)
                .execute(&mut *tx)
                .await?;
            after = id;
        }
    }
    sqlx::query("UPDATE schema_version SET version=4 WHERE id=1")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// One-time M4 account-role upgrade. Startup runs before accepting requests.
pub async fn from_m4(db: &Db) -> anyhow::Result<()> {
    let mut connection = db.pool.acquire().await?;
    if db.postgres {
        let present:Option<String>=sqlx::query_scalar("SELECT table_name FROM information_schema.tables WHERE table_schema=current_schema() AND table_name='users'").fetch_optional(&mut *connection).await?;
        if present.is_some() {
            let mut tx = connection.begin().await?;
            sqlx::raw_sql("ALTER TABLE users DROP CONSTRAINT IF EXISTS users_role_check; ALTER TABLE users ADD CONSTRAINT users_role_check CHECK(role IN ('admin','editor','moderator','subscriber','disabled'));").execute(&mut *tx).await?;
            tx.commit().await?;
        }
    } else {
        let sql: Option<String> =
            sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type='table' AND name='users'")
                .fetch_optional(&mut *connection)
                .await?;
        if sql.is_some_and(|s| !s.contains("'subscriber'")) {
            sqlx::query("PRAGMA foreign_keys=OFF")
                .execute(&mut *connection)
                .await?;
            let result=async {let mut tx=connection.begin().await?;sqlx::raw_sql("CREATE TABLE users_m4(id TEXT PRIMARY KEY,email TEXT NOT NULL UNIQUE,name TEXT NOT NULL,role TEXT NOT NULL CHECK(role IN ('admin','editor','moderator','subscriber','disabled')),password_hash TEXT NOT NULL,created_at BIGINT NOT NULL); INSERT INTO users_m4 SELECT * FROM users; DROP TABLE users; ALTER TABLE users_m4 RENAME TO users;").execute(&mut *tx).await?;tx.commit().await?;Ok::<(),anyhow::Error>(())}.await;
            sqlx::query("PRAGMA foreign_keys=ON")
                .execute(&mut *connection)
                .await?;
            result?;
            let violations = sqlx::query("PRAGMA foreign_key_check")
                .fetch_all(&mut *connection)
                .await?;
            anyhow::ensure!(
                violations.is_empty(),
                "M4 role migration encountered inconsistent foreign keys"
            );
        }
    }
    let columns = if db.postgres {
        sqlx::query("SELECT column_name AS name FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='business_campaigns'").fetch_all(&mut *connection).await?
    } else {
        sqlx::query("PRAGMA table_info(business_campaigns)")
            .fetch_all(&mut *connection)
            .await?
    };
    if !columns.is_empty()
        && !columns
            .iter()
            .any(|r| r.get::<String, _>("name") == "trigger_kind")
    {
        sqlx::query(
            "ALTER TABLE business_campaigns ADD COLUMN trigger_kind TEXT NOT NULL DEFAULT ''",
        )
        .execute(&mut *connection)
        .await?;
    }
    Ok(())
}
