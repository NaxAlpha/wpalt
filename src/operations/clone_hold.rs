//! A copied site remains read-only until a stopped-host operator reviews it.
use crate::{
    App,
    error::{Error, Result},
};
use sqlx::Row;
pub const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS recovery_mode(id BIGINT PRIMARY KEY CHECK(id=1),held BIGINT NOT NULL DEFAULT 0 CHECK(held IN (0,1)),source_origin TEXT NOT NULL DEFAULT '',target_origin TEXT NOT NULL DEFAULT '',review TEXT NOT NULL DEFAULT ''); INSERT INTO recovery_mode(id) VALUES(1) ON CONFLICT(id) DO NOTHING;";
pub async fn held(app: &App) -> Result<bool> {
    Ok(
        sqlx::query_scalar::<_, i64>("SELECT held FROM recovery_mode WHERE id=1")
            .fetch_one(&app.db.pool)
            .await?
            == 1,
    )
}
pub async fn activate(app: &App, review: &str) -> Result<()> {
    if review.trim().len() < 40 || review.len() > 2000 || review.chars().any(char::is_control) {
        return Err(Error::invalid(
            "Provide a 40–2,000 character review of source shutdown, queues, external payment ownership, identities and credentials.",
        ));
    }
    let _guard = app.mutation().await?;
    let row = sqlx::query("SELECT held,target_origin FROM recovery_mode WHERE id=1")
        .fetch_one(&app.db.pool)
        .await?;
    if row.get::<i64, _>("held") != 1
        || row.get::<String, _>("target_origin") != app.config.origin()
    {
        return Err(Error::invalid(
            "This is not a held clone at its reviewed target origin.",
        ));
    }
    sqlx::query("UPDATE recovery_mode SET held=0,review=$1 WHERE id=1 AND held=1")
        .bind(review.trim())
        .execute(&app.db.pool)
        .await?;
    app.clone_held
        .store(false, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}
