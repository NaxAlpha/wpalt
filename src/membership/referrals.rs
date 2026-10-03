//! Local referral/commission records; no invented purchase or payout state.
use super::*;
use axum::{
    Router,
    extract::{Path, State},
    response::Redirect,
    routing::get,
};
pub fn routes() -> Router<App> {
    Router::new().route("/r/{id}", get(visit))
}
async fn visit(State(app): State<App>, Path(id): Path<String>) -> Result<Redirect> {
    uuid(&id)?;
    if sqlx::query("UPDATE member_referrals SET visits=visits+1 WHERE id=$1 AND visits<1000000000")
        .bind(&id)
        .execute(&app.db.pool)
        .await?
        .rows_affected()
        != 1
    {
        return Err(Error::not_found());
    }
    Ok(Redirect::to("/"))
}
pub async fn create(app: &App, user: &str, title: &str) -> Result<String> {
    uuid(user)?;
    label(title, 160)?;
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO member_referrals(id,user_id,title,created_at) VALUES($1,$2,$3,$4)")
        .bind(&id)
        .bind(user)
        .bind(title)
        .bind(now())
        .execute(&app.db.pool)
        .await?;
    Ok(id)
}
pub async fn commission(
    app: &App,
    referral: &str,
    reference: &str,
    amount: i64,
    currency: &str,
) -> Result<String> {
    uuid(referral)?;
    label(reference, 160)?;
    if !(0..=1_000_000_000_000).contains(&amount)
        || currency.len() != 3
        || !currency.bytes().all(|b| b.is_ascii_uppercase())
    {
        return Err(Error::invalid(
            "Use a nonnegative integer minor-unit amount and three-letter currency.",
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO member_commissions(id,referral_id,reference,amount_minor,currency,state,created_at) VALUES($1,$2,$3,$4,$5,'recorded',$6)").bind(&id).bind(referral).bind(reference).bind(amount).bind(currency).bind(now()).execute(&app.db.pool).await?;
    Ok(id)
}
