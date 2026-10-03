//! Derived database-enforced row budget: real inserts/deletes update it atomically,
//! including bulk recovery and upserts, with no COUNT(*) on a request's write path.
use crate::{db::Db, error::Result};
pub const TABLES: &[&str] = &[
    "member_groups",
    "member_policies",
    "member_group_users",
    "member_grants",
    "member_resources",
    "member_profiles",
    "member_courses",
    "member_course_versions",
    "member_progress",
    "member_attempts",
    "member_assignments",
    "member_certificates",
    "member_discussions",
    "member_gifts",
    "member_referrals",
    "member_commissions",
    "member_identities",
];
pub async fn prepare(db: &Db) -> Result<()> {
    let mut tx = db.pool.begin().await?;
    sqlx::raw_sql("CREATE TABLE IF NOT EXISTS member_usage(id BIGINT PRIMARY KEY CHECK(id=1),records BIGINT NOT NULL CHECK(records>=0),limit_records BIGINT NOT NULL CHECK(limit_records>0));").execute(&mut *tx).await?;
    let count = TABLES
        .iter()
        .map(|t| format!("(SELECT COUNT(*) FROM {t})"))
        .collect::<Vec<_>>()
        .join("+");
    sqlx::query(&format!("INSERT INTO member_usage(id,records,limit_records) SELECT 1,{count},$1 WHERE NOT EXISTS(SELECT 1 FROM member_usage WHERE id=1)")).bind(db.membership_max_records).execute(&mut *tx).await?;
    sqlx::query("UPDATE member_usage SET limit_records=$1 WHERE id=1")
        .bind(db.membership_max_records)
        .execute(&mut *tx)
        .await?;
    if db.postgres {
        sqlx::raw_sql("CREATE OR REPLACE FUNCTION member_record_budget() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF TG_OP='INSERT' THEN UPDATE member_usage SET records=records+1 WHERE id=1 AND records<limit_records; IF NOT FOUND THEN RAISE EXCEPTION 'member_record_limit'; END IF; RETURN NEW; ELSE UPDATE member_usage SET records=records-1 WHERE id=1; RETURN OLD; END IF; END $$;").execute(&mut *tx).await?;
    }
    for table in TABLES {
        if db.postgres {
            sqlx::raw_sql(&format!("DROP TRIGGER IF EXISTS {table}_budget ON {table}; CREATE TRIGGER {table}_budget AFTER INSERT OR DELETE ON {table} FOR EACH ROW EXECUTE FUNCTION member_record_budget();")).execute(&mut *tx).await?;
        } else {
            sqlx::raw_sql(&format!("CREATE TRIGGER IF NOT EXISTS {table}_insert_budget AFTER INSERT ON {table} BEGIN UPDATE member_usage SET records=records+1 WHERE id=1 AND records<limit_records; SELECT CASE WHEN changes()!=1 THEN RAISE(ABORT,'member_record_limit') END; END; CREATE TRIGGER IF NOT EXISTS {table}_delete_budget AFTER DELETE ON {table} BEGIN UPDATE member_usage SET records=records-1 WHERE id=1; END;")).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}
