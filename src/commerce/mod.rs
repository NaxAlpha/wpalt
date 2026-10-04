//! Integrated local commerce. Database authority and integer financial records.
pub mod backup;
pub mod billing;
pub mod booking;
pub mod budget;
pub mod catalog;
pub mod orders;
pub mod payments;
pub mod web;
use crate::{
    App,
    error::{Error, Result},
    model::Session,
};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = include_str!("schema.sql");
pub const MAX_MONEY: i64 = 1_000_000_000_000;
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub currency: String,
    pub hold_seconds: i64,
    pub max_records: i64,
    pub referral_bps: i64,
    pub stripe: payments::StripeConfig,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            currency: "USD".into(),
            hold_seconds: 900,
            max_records: 1_000_000,
            referral_bps: 500,
            stripe: Default::default(),
        }
    }
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            ["USD", "EUR", "GBP", "JPY"].contains(&self.currency.as_str()),
            "Commerce currency must be USD, EUR, GBP or JPY"
        );
        anyhow::ensure!(
            (60..=3600).contains(&self.hold_seconds),
            "Commerce holds must be between 60 and 3600 seconds"
        );
        anyhow::ensure!(
            (1..=1_000_000_000).contains(&self.max_records),
            "Commerce quota out of range"
        );
        anyhow::ensure!(
            (0..=10000).contains(&self.referral_bps),
            "Referral rate out of range"
        );
        self.stripe.validate()?;
        Ok(())
    }
}
pub async fn customer(app: &App, s: &Session) -> Result<()> {
    if !app.config.commerce.enabled {
        return Err(Error::forbidden());
    }
    let role: Option<String> = sqlx::query_scalar("SELECT role FROM users WHERE id=$1")
        .bind(&s.user.id)
        .fetch_optional(&app.db.pool)
        .await?;
    if role.is_none() || role.as_deref() == Some("disabled") {
        return Err(Error::forbidden());
    }
    Ok(())
}
pub async fn owner(app: &App, s: &Session) -> Result<()> {
    customer(app, s).await?;
    crate::membership::staff(app, s).await
}
pub fn text(s: &str, max: usize) -> Result<()> {
    if s.trim().is_empty() || s.len() > max {
        Err(Error::invalid("Check the text length."))
    } else {
        Ok(())
    }
}
pub fn money(value: i64) -> Result<i64> {
    if !(0..=MAX_MONEY).contains(&value) {
        Err(Error::invalid(
            "Financial amount is outside the supported range.",
        ))
    } else {
        Ok(value)
    }
}
pub fn basis(value: i64, bps: i64) -> Result<i64> {
    money(value)?;
    if !(0..=10000).contains(&bps) {
        return Err(Error::invalid("Invalid percentage."));
    }
    let result = (i128::from(value) * i128::from(bps) + 5000) / 10000;
    money(i64::try_from(result).map_err(|_| Error::invalid("Amount overflow."))?)
}
pub fn amount(value: i64, currency: &str) -> String {
    if currency == "JPY" {
        format!("{currency} {value}")
    } else {
        format!("{currency} {}.{:02}", value / 100, value % 100)
    }
}
pub async fn settings(app: &App) -> Result<sqlx::any::AnyRow> {
    Ok(sqlx::query("SELECT * FROM shop_settings WHERE id=1")
        .fetch_one(&app.db.pool)
        .await?)
}

pub async fn tick(app: &App) -> Result<usize> {
    if !app.config.commerce.enabled {
        return Ok(0);
    }
    let expired = orders::expire(app).await?;
    let events = payments::process(app).await?;
    let billing = billing::tick(app).await?;
    let notifications = booking::notify(app).await?;
    Ok(expired + events + billing + notifications)
}
