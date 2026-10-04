//! Derived database-enforced row budget: real inserts/deletes update it atomically,
//! including bulk recovery and upserts, with no COUNT(*) on a request's write path.
use crate::{db::Db, error::Result};
pub const TABLES: &[&str] = &[
    "shop_products",
    "shop_variants",
    "shop_discounts",
    "shop_carts",
    "shop_cart_lines",
    "shop_resources",
    "shop_slots",
    "shop_subscriptions",
    "shop_orders",
    "shop_order_lines",
    "shop_payments",
    "shop_refunds",
    "shop_notifications",
    "shop_provider_events",
    "shop_reward_redemptions",
    "shop_payouts",
    "shop_history",
];
pub async fn prepare(db: &Db) -> Result<()> {
    let mut tx = db.pool.begin().await?;
    sqlx::raw_sql("CREATE TABLE IF NOT EXISTS shop_usage(id BIGINT PRIMARY KEY CHECK(id=1),records BIGINT NOT NULL CHECK(records>=0),limit_records BIGINT NOT NULL CHECK(limit_records>0));").execute(&mut *tx).await?;
    let count = TABLES
        .iter()
        .map(|t| format!("(SELECT COUNT(*) FROM {t})"))
        .collect::<Vec<_>>()
        .join("+");
    sqlx::query(&format!("INSERT INTO shop_usage(id,records,limit_records) SELECT 1,{count},$1 WHERE NOT EXISTS(SELECT 1 FROM shop_usage WHERE id=1)")).bind(db.commerce_max_records).execute(&mut *tx).await?;
    sqlx::query("UPDATE shop_usage SET limit_records=$1 WHERE id=1")
        .bind(db.commerce_max_records)
        .execute(&mut *tx)
        .await?;
    if db.postgres {
        sqlx::raw_sql("CREATE OR REPLACE FUNCTION shop_record_budget() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF TG_OP='INSERT' THEN UPDATE shop_usage SET records=records+1 WHERE id=1 AND records<limit_records; IF NOT FOUND THEN RAISE EXCEPTION 'shop_record_limit'; END IF; RETURN NEW; ELSE UPDATE shop_usage SET records=records-1 WHERE id=1; RETURN OLD; END IF; END $$;").execute(&mut *tx).await?;
    }
    for table in TABLES {
        if db.postgres {
            sqlx::raw_sql(&format!("DROP TRIGGER IF EXISTS {table}_budget ON {table}; CREATE TRIGGER {table}_budget AFTER INSERT OR DELETE ON {table} FOR EACH ROW EXECUTE FUNCTION shop_record_budget();")).execute(&mut *tx).await?;
        } else {
            sqlx::raw_sql(&format!("CREATE TRIGGER IF NOT EXISTS {table}_insert_budget AFTER INSERT ON {table} BEGIN UPDATE shop_usage SET records=records+1 WHERE id=1 AND records<limit_records; SELECT CASE WHEN changes()!=1 THEN RAISE(ABORT,'shop_record_limit') END; END; CREATE TRIGGER IF NOT EXISTS {table}_delete_budget AFTER DELETE ON {table} BEGIN UPDATE shop_usage SET records=records-1 WHERE id=1; END;")).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}
