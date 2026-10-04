//! Hosted collection only. Browser redirects and webhook objects are never payment authority.
use super::*;
use crate::now;
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use sqlx::Row;
pub const API_VERSION: &str = "2026-09-30.endive";
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StripeConfig {
    pub enabled: bool,
    pub secret_key: String,
    pub webhook_secret: String,
    pub api_url: String,
    pub ca_cert_file: String,
}
impl StripeConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        anyhow::ensure!(
            self.secret_key.starts_with("sk_")
                && self.secret_key.len() <= 4096
                && self.webhook_secret.starts_with("whsec_")
                && self.webhook_secret.len() <= 4096,
            "Stripe needs bounded secret and webhook keys"
        );
        if !self.api_url.is_empty() {
            let u = url::Url::parse(&self.api_url)?;
            anyhow::ensure!(
                u.scheme() == "https"
                    && u.host_str().is_some()
                    && u.username().is_empty()
                    && u.password().is_none()
                    && u.query().is_none()
                    && u.fragment().is_none(),
                "Stripe API endpoint must be HTTPS without credentials/query/fragment"
            );
        }
        if !self.ca_cert_file.is_empty() {
            certificate(&self.ca_cert_file)?;
        }
        Ok(())
    }
}
fn certificate(path: &str) -> anyhow::Result<reqwest::Certificate> {
    use std::io::Read;
    let f = std::fs::File::open(path)?;
    anyhow::ensure!(
        f.metadata()?.is_file(),
        "Stripe CA must be a regular PEM file"
    );
    let mut bytes = Vec::new();
    f.take(32769).read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() <= 32768, "Stripe CA exceeds 32 KiB");
    Ok(reqwest::Certificate::from_pem(&bytes)?)
}
fn unavailable() -> Error {
    Error(
        axum::http::StatusCode::BAD_GATEWAY,
        "Payment provider is unavailable. The local record is retained; retry safely.",
    )
}
async fn api(
    app: &App,
    method: reqwest::Method,
    path: &str,
    key: &str,
    form: &[(String, String)],
) -> Result<Value> {
    let cfg = &app.config.commerce.stripe;
    if !app.config.commerce.enabled || !cfg.enabled {
        return Err(Error::forbidden());
    }
    let mut builder = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(8));
    if !cfg.ca_cert_file.is_empty() {
        builder =
            builder.add_root_certificate(certificate(&cfg.ca_cert_file).map_err(|_| unavailable())?)
    }
    let client = builder.build().map_err(|_| unavailable())?;
    let base = if cfg.api_url.is_empty() {
        "https://api.stripe.com/v1"
    } else {
        cfg.api_url.trim_end_matches('/')
    };
    let mut request = client
        .request(method, format!("{base}/{path}"))
        .bearer_auth(&cfg.secret_key)
        .header("Stripe-Version", API_VERSION);
    if !key.is_empty() {
        request = request.header("Idempotency-Key", key).form(form);
    }
    let started = std::time::Instant::now();
    let mut response = request.send().await.map_err(|_| unavailable())?;
    let status = response.status();
    tracing::info!(
        event = "commerce_provider_request",
        status = status.as_u16(),
        duration_ms = started.elapsed().as_millis() as u64
    );
    if !status.is_success() {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    while let Some(part) = response.chunk().await.map_err(|_| unavailable())? {
        if bytes.len() + part.len() > 256 * 1024 {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&part);
    }
    serde_json::from_slice(&bytes).map_err(|_| unavailable())
}
fn provider_id<'a>(value: &'a str, prefix: &str) -> Result<&'a str> {
    if !value.starts_with(prefix)
        || value.len() > 160
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(Error::invalid("Invalid provider identifier."));
    }
    Ok(value)
}
fn string<'a>(v: &'a Value, p: &str) -> Result<&'a str> {
    v.pointer(p)
        .and_then(Value::as_str)
        .ok_or(Error::invalid("Provider mapping is missing."))
}
fn integer(v: &Value, p: &str) -> Result<i64> {
    v.pointer(p).and_then(Value::as_i64).ok_or(Error::invalid(
        "Provider amount or billing period is missing.",
    ))
}
fn form(values: &[(&str, String)]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}
pub async fn checkout(app: &App, s: &Session, id: &str) -> Result<String> {
    customer(app, s).await?;
    let order =
        sqlx::query("SELECT * FROM shop_orders WHERE id=$1 AND user_id=$2 AND provider='stripe'")
            .bind(id)
            .bind(&s.user.id)
            .fetch_optional(&app.db.pool)
            .await?
            .ok_or_else(Error::not_found)?;
    if order.get::<String, _>("payment_state") != "awaiting"
        || order.get::<i64, _>("expires_at") <= now()
    {
        return Err(Error::conflict());
    }
    let sub: String = order.get("subscription_id");
    let mut f = form(&[
        (
            "mode",
            if sub.is_empty() {
                "payment"
            } else {
                "subscription"
            }
            .into(),
        ),
        ("client_reference_id", id.into()),
        ("metadata[wpalt_order]", id.into()),
        (
            "success_url",
            format!("{}/shop/orders/{id}", app.config.base_url),
        ),
        (
            "cancel_url",
            format!("{}/shop/orders/{id}", app.config.base_url),
        ),
        ("expires_at", order.get::<i64, _>("expires_at").to_string()),
        ("line_items[0][quantity]", "1".into()),
        (
            "line_items[0][price_data][currency]",
            order.get::<String, _>("currency").to_lowercase(),
        ),
        (
            "line_items[0][price_data][unit_amount]",
            order.get::<i64, _>("total_minor").to_string(),
        ),
        (
            "line_items[0][price_data][product_data][name]",
            format!("wpalt order {id}"),
        ),
    ]);
    if !sub.is_empty() {
        let interval: String =
            sqlx::query_scalar("SELECT billing_interval FROM shop_subscriptions WHERE id=$1")
                .bind(&sub)
                .fetch_one(&app.db.pool)
                .await?;
        f.extend(form(&[
            ("line_items[0][price_data][recurring][interval]", interval),
            ("subscription_data[metadata][wpalt_order]", id.into()),
        ]));
    }
    let value = api(
        app,
        reqwest::Method::POST,
        "checkout/sessions",
        &format!("wpalt-checkout-{id}"),
        &f,
    )
    .await?;
    let reference = provider_id(string(&value, "/id")?, "cs_")?;
    if string(&value, "/client_reference_id")? != id
        || integer(&value, "/amount_total")? != order.get::<i64, _>("total_minor")
        || string(&value, "/currency")?.to_uppercase() != order.get::<String, _>("currency")
    {
        return Err(Error::conflict());
    }
    let target = url::Url::parse(string(&value, "/url")?).map_err(|_| unavailable())?;
    if target.scheme() != "https"
        || target.host_str() != Some("checkout.stripe.com")
        || !target.username().is_empty()
        || target.password().is_some()
    {
        return Err(unavailable());
    }
    let _guard = app.mutation().await;
    customer(app, s).await?;
    let changed=sqlx::query("UPDATE shop_orders SET provider_ref=$1 WHERE id=$2 AND (provider_ref='' OR provider_ref=$1)").bind(reference).bind(id).execute(&app.db.pool).await?;
    if changed.rows_affected() != 1 {
        return Err(Error::conflict());
    }
    Ok(target.to_string())
}
/// The only unauthenticated mutation endpoint verifies original bytes before parsing/persisting.
pub async fn receive(app: &App, signature: &str, raw: &[u8]) -> Result<()> {
    if !app.config.commerce.enabled || !app.config.commerce.stripe.enabled {
        return Err(Error::forbidden());
    }
    if raw.len() > 256 * 1024 || signature.len() > 2048 {
        return Err(Error::forbidden());
    }
    let mut timestamp = None;
    let mut signatures = Vec::new();
    for part in signature.split(',') {
        if let Some((k, v)) = part.split_once('=') {
            if k == "t" {
                if timestamp.is_some() {
                    return Err(Error::forbidden());
                }
                timestamp = v.parse::<i64>().ok();
            } else if k == "v1" {
                signatures.push(v)
            }
        }
    }
    let at = timestamp.ok_or_else(Error::forbidden)?;
    if now().abs_diff(at) > 300 {
        return Err(Error::forbidden());
    }
    let mut mac =
        Hmac::<Sha256>::new_from_slice(app.config.commerce.stripe.webhook_secret.as_bytes())
            .map_err(|_| Error::forbidden())?;
    mac.update(format!("{at}.").as_bytes());
    mac.update(raw);
    if !signatures.iter().any(|s| {
        hex::decode(s)
            .ok()
            .is_some_and(|b| mac.clone().verify_slice(&b).is_ok())
    }) {
        return Err(Error::forbidden());
    }
    let value: Value = serde_json::from_slice(raw).map_err(|_| Error::invalid("Invalid event."))?;
    let id = provider_id(string(&value, "/id")?, "evt_")?;
    if string(&value, "/api_version")? != API_VERSION {
        return Err(Error::invalid(
            "Webhook endpoint API version differs from the pinned version.",
        ));
    }
    let kind = string(&value, "/type")?;
    let state = if matches!(
        kind,
        "checkout.session.completed"
            | "checkout.session.async_payment_succeeded"
            | "checkout.session.async_payment_failed"
            | "checkout.session.expired"
            | "invoice.paid"
            | "invoice.payment_failed"
            | "customer.subscription.updated"
            | "customer.subscription.deleted"
            | "refund.updated"
            | "refund.created"
    ) {
        "pending"
    } else {
        "ignored"
    };
    let digest = crate::auth::digest(raw);
    let body = std::str::from_utf8(raw).map_err(|_| Error::invalid("Invalid event encoding."))?;
    let _guard = app.mutation().await;
    if let Some(old) =
        sqlx::query_scalar::<_, String>("SELECT digest FROM shop_provider_events WHERE id=$1")
            .bind(id)
            .fetch_optional(&app.db.pool)
            .await?
    {
        if old != digest {
            return Err(Error::conflict());
        }
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO shop_provider_events(id,digest,body,state,created_at) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(id)
    .bind(digest)
    .bind(body)
    .bind(state)
    .bind(now())
    .execute(&app.db.pool)
    .await?;
    Ok(())
}
async fn invoice_payment(app: &App, value: &Value, order: &str, sub_ref: &str) -> Result<()> {
    if string(value, "/status")? != "paid" || integer(value, "/amount_remaining")? != 0 {
        return Err(Error::conflict());
    }
    if string(value, "/parent/subscription_details/subscription")? != sub_ref
        || integer(value, "/amount_paid")? != integer(value, "/total")?
        || integer(value, "/amount_due")? != integer(value, "/amount_paid")?
        || value.pointer("/payments/has_more").and_then(Value::as_bool) != Some(false)
    {
        return Err(Error::invalid(
            "Subscription invoice requires one exact uncredited collection.",
        ));
    }
    let lines = value
        .pointer("/lines/data")
        .and_then(Value::as_array)
        .ok_or_else(unavailable)?;
    if lines.len() != 1 || value.pointer("/lines/has_more").and_then(Value::as_bool) != Some(false)
    {
        return Err(Error::invalid(
            "Only one fixed subscription line is supported.",
        ));
    }
    let start = integer(&lines[0], "/period/start")?;
    let end = integer(&lines[0], "/period/end")?;
    let payments = value
        .pointer("/payments/data")
        .and_then(Value::as_array)
        .ok_or_else(unavailable)?;
    if payments.len() != 1
        || string(&payments[0], "/status")? != "paid"
        || integer(&payments[0], "/amount_paid")? != integer(value, "/amount_paid")?
    {
        return Err(Error::invalid(
            "Split or credited subscription payments require operator reconciliation.",
        ));
    }
    let reference = provider_id(string(&payments[0], "/payment/payment_intent")?, "pi_")?;
    orders::confirm_payment(
        app,
        &orders::Payment {
            order_id: order.into(),
            provider: "stripe".into(),
            reference: reference.into(),
            amount_minor: integer(value, "/amount_paid")?,
            currency: string(value, "/currency")?.to_uppercase(),
            paid_at: integer(value, "/status_transitions/paid_at")?,
            subscription_ref: sub_ref.into(),
            period_start: start,
            period_end: end,
        },
    )
    .await?;
    Ok(())
}
async fn reconcile(app: &App, event: &Value) -> Result<()> {
    let kind = string(event, "/type")?;
    let object = string(event, "/data/object/id")?;
    if kind.starts_with("checkout.session.") {
        provider_id(object, "cs_")?;
        let value = api(
            app,
            reqwest::Method::GET,
            &format!("checkout/sessions/{object}"),
            "",
            &[],
        )
        .await?;
        let order = string(&value, "/client_reference_id")?;
        crate::membership::uuid(order)?;
        let mapped: Option<String> = sqlx::query_scalar(
            "SELECT provider_ref FROM shop_orders WHERE id=$1 AND provider='stripe'",
        )
        .bind(order)
        .fetch_optional(&app.db.pool)
        .await?;
        let mapped = mapped.ok_or_else(Error::not_found)?;
        if mapped != object {
            return Err(Error::conflict());
        }
        if string(&value, "/payment_status")? == "paid" {
            let sub = value
                .get("subscription")
                .and_then(Value::as_str)
                .unwrap_or("");
            if sub.is_empty() {
                let intent = provider_id(string(&value, "/payment_intent")?, "pi_")?;
                let pi = api(
                    app,
                    reqwest::Method::GET,
                    &format!("payment_intents/{intent}"),
                    "",
                    &[],
                )
                .await?;
                if string(&pi, "/status")? != "succeeded" {
                    return Err(Error::conflict());
                }
                orders::confirm_payment(
                    app,
                    &orders::Payment {
                        order_id: order.into(),
                        provider: "stripe".into(),
                        reference: intent.into(),
                        amount_minor: integer(&pi, "/amount_received")?,
                        currency: string(&pi, "/currency")?.to_uppercase(),
                        paid_at: integer(event, "/created")?,
                        subscription_ref: String::new(),
                        period_start: 0,
                        period_end: 0,
                    },
                )
                .await?;
            } else {
                provider_id(sub, "sub_")?;
                let inv = provider_id(string(&value, "/invoice")?, "in_")?;
                let invoice = api(
                    app,
                    reqwest::Method::GET,
                    &format!("invoices/{inv}"),
                    "",
                    &[],
                )
                .await?;
                invoice_payment(app, &invoice, order, sub).await?;
            }
        } else if string(&value, "/status")? == "expired" {
            let _g = app.mutation().await;
            let mut tx = app.db.pool.begin().await?;
            let row =
                sqlx::query("SELECT * FROM shop_orders WHERE id=$1 AND payment_state='awaiting'")
                    .bind(order)
                    .fetch_optional(&mut *tx)
                    .await?;
            if let Some(row) = row {
                orders::cancel_tx(&mut tx, &row, "failed").await?;
            }
            tx.commit().await?;
        }
        return Ok(());
    }
    if kind.starts_with("invoice.") {
        provider_id(object, "in_")?;
        let value = api(
            app,
            reqwest::Method::GET,
            &format!("invoices/{object}"),
            "",
            &[],
        )
        .await?;
        let reference = provider_id(
            string(&value, "/parent/subscription_details/subscription")?,
            "sub_",
        )?;
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT id FROM shop_subscriptions WHERE provider='stripe' AND provider_ref=$1",
        )
        .bind(reference)
        .fetch_optional(&app.db.pool)
        .await?;
        if existing.is_none() {
            let remote = api(
                app,
                reqwest::Method::GET,
                &format!("subscriptions/{reference}"),
                "",
                &[],
            )
            .await?;
            let initial = string(&remote, "/metadata/wpalt_order")?;
            crate::membership::uuid(initial)?;
            let _guard = app.mutation().await;
            let changed=sqlx::query("UPDATE shop_subscriptions SET provider_ref=$1,version=version+1 WHERE provider='stripe' AND (provider_ref='' OR provider_ref=$1) AND id=(SELECT subscription_id FROM shop_orders WHERE id=$2 AND provider='stripe' AND provider_ref<>'')").bind(reference).bind(initial).execute(&app.db.pool).await?;
            if changed.rows_affected() != 1 {
                return Err(Error::not_found());
            }
        }
        let sub = sqlx::query(
            "SELECT * FROM shop_subscriptions WHERE provider='stripe' AND provider_ref=$1",
        )
        .bind(reference)
        .fetch_one(&app.db.pool)
        .await?;
        if string(&value, "/status")? == "paid" {
            let line = value.pointer("/lines/data/0").ok_or_else(unavailable)?;
            let start = integer(line, "/period/start")?;
            let end = integer(line, "/period/end")?;
            let reason = string(&value, "/billing_reason")?;
            let order = if reason == "subscription_create" {
                sqlx::query_scalar("SELECT id FROM shop_orders WHERE subscription_id=$1 AND purpose='purchase' ORDER BY created_at,id LIMIT 1").bind(sub.get::<String,_>("id")).fetch_one(&app.db.pool).await?
            } else if reason == "subscription_cycle" && !sub.get::<String, _>("grant_id").is_empty()
            {
                billing::provider_invoice(app, reference, start, end).await?
            } else {
                return Err(Error::conflict());
            };
            invoice_payment(app, &value, &order, reference).await?;
        } else if kind == "invoice.payment_failed" {
            let _g = app.mutation().await;
            sqlx::query("UPDATE shop_subscriptions SET state='past_due',version=version+1 WHERE id=$1 AND period_end<=$2 AND state='active'").bind(sub.get::<String,_>("id")).bind(now()).execute(&app.db.pool).await?;
        }
        return Ok(());
    }
    if kind.starts_with("customer.subscription.") {
        provider_id(object, "sub_")?;
        let value = api(
            app,
            reqwest::Method::GET,
            &format!("subscriptions/{object}"),
            "",
            &[],
        )
        .await?;
        let status = string(&value, "/status")?;
        let _g = app.mutation().await;
        let state = if ["canceled", "unpaid", "incomplete_expired"].contains(&status) {
            Some("cancelled")
        } else if value.get("cancel_at_period_end").and_then(Value::as_bool) == Some(true) {
            Some("cancel_at_end")
        } else {
            None
        };
        if let Some(state) = state {
            sqlx::query(
                "UPDATE shop_subscriptions SET state=$1,version=version+1 WHERE provider_ref=$2 AND (state<>'cancelled' OR $1='cancelled')",
            )
            .bind(state)
            .bind(object)
            .execute(&app.db.pool)
            .await?;
        }
        return Ok(());
    }
    if kind.starts_with("refund.") {
        provider_id(object, "re_")?;
        let value = api(
            app,
            reqwest::Method::GET,
            &format!("refunds/{object}"),
            "",
            &[],
        )
        .await?;
        settle_refund(app, &value).await?;
        return Ok(());
    }
    Ok(())
}
pub async fn process(app: &App) -> Result<usize> {
    if !app.config.commerce.enabled || !app.config.commerce.stripe.enabled {
        return Ok(0);
    }
    let rows=sqlx::query("SELECT id,body FROM shop_provider_events WHERE state='pending' AND next_at<=$1 ORDER BY next_at,created_at,id LIMIT 40").bind(now()).fetch_all(&app.db.pool).await?;
    let mut done = 0;
    for row in rows {
        let v: Value = serde_json::from_str(&row.get::<String, _>("body"))
            .map_err(|_| Error::invalid("Invalid stored provider event."))?;
        match reconcile(app, &v).await {
            Ok(()) => {
                let _guard = app.mutation().await;
                sqlx::query("UPDATE shop_provider_events SET state='processed' WHERE id=$1 AND state='pending'").bind(row.get::<String,_>("id")).execute(&app.db.pool).await?;
                done += 1;
            }
            Err(e) => {
                let _guard = app.mutation().await;
                sqlx::query("UPDATE shop_provider_events SET attempts=attempts+1,next_at=$1 WHERE id=$2 AND state='pending'").bind(now()+30).bind(row.get::<String,_>("id")).execute(&app.db.pool).await?;
                tracing::warn!(event="commerce_reconciliation_pending",event_id=%row.get::<String,_>("id"),status=e.0.as_u16());
            }
        }
    }
    // A completed refund or late recurring payment must also stop future provider
    // collections. Persisted work survives transport failure and fresh recovery.
    let cancellations = sqlx::query("SELECT id,provider_ref FROM shop_subscriptions WHERE provider_cancel_pending=1 AND provider_cancel_at<=$1 ORDER BY provider_cancel_at,id LIMIT 40").bind(now()).fetch_all(&app.db.pool).await?;
    for sub in cancellations {
        let id: String = sub.get("id");
        let reference: String = sub.get("provider_ref");
        let result = async {
            provider_id(&reference, "sub_")?;
            let value = api(
                app,
                reqwest::Method::POST,
                &format!("subscriptions/{reference}"),
                &format!("wpalt-stop-billing-{id}"),
                &form(&[("cancel_at_period_end", "true".into())]),
            )
            .await?;
            if string(&value, "/id")? != reference
                || value.get("cancel_at_period_end").and_then(Value::as_bool) != Some(true)
            {
                return Err(Error::conflict());
            }
            Ok(())
        }
        .await;
        let _guard = app.mutation().await;
        if result.is_ok() {
            sqlx::query("UPDATE shop_subscriptions SET provider_cancel_pending=0,provider_cancel_at=0 WHERE id=$1 AND provider_ref=$2").bind(&id).bind(&reference).execute(&app.db.pool).await?;
            done += 1;
        } else {
            sqlx::query("UPDATE shop_subscriptions SET provider_cancel_at=$1 WHERE id=$2 AND provider_cancel_pending=1").bind(now()+30).bind(&id).execute(&app.db.pool).await?;
            tracing::warn!(event="commerce_billing_stop_pending",subscription_id=%id);
        }
    }
    Ok(done)
}
async fn settle_refund(app: &App, v: &Value) -> Result<()> {
    let id = string(v, "/metadata/wpalt_refund")?;
    crate::membership::uuid(id)?;
    let row=sqlx::query("SELECT r.amount_minor,o.payment_ref,o.currency FROM shop_refunds r JOIN shop_orders o ON o.id=r.order_id WHERE r.id=$1 AND o.provider='stripe'").bind(id).fetch_optional(&app.db.pool).await?.ok_or_else(Error::not_found)?;
    if string(v, "/payment_intent")? != row.get::<String, _>("payment_ref")
        || integer(v, "/amount")? != row.get::<i64, _>("amount_minor")
        || string(v, "/currency")?.to_uppercase() != row.get::<String, _>("currency")
    {
        return Err(Error::conflict());
    }
    if string(v, "/status")? == "succeeded" {
        orders::confirm_refund(
            app,
            id,
            provider_id(string(v, "/id")?, "re_")?,
            integer(v, "/amount")?,
        )
        .await?;
    } else if ["failed", "canceled"].contains(&string(v, "/status")?) {
        let _g = app.mutation().await;
        sqlx::query("UPDATE shop_refunds SET state='failed',provider_ref=$1 WHERE id=$2 AND state='pending'").bind(string(v,"/id")?).bind(id).execute(&app.db.pool).await?;
    }
    Ok(())
}
pub async fn refund(app: &App, s: &Session, id: &str) -> Result<()> {
    owner(app, s).await?;
    let r=sqlx::query("SELECT r.*,o.payment_ref,o.provider FROM shop_refunds r JOIN shop_orders o ON o.id=r.order_id WHERE r.id=$1").bind(id).fetch_optional(&app.db.pool).await?.ok_or_else(Error::not_found)?;
    if r.get::<String, _>("provider") != "stripe" || r.get::<String, _>("state") != "pending" {
        return Err(Error::conflict());
    }
    let f = form(&[
        ("payment_intent", r.get("payment_ref")),
        ("amount", r.get::<i64, _>("amount_minor").to_string()),
        ("metadata[wpalt_refund]", id.into()),
    ]);
    let v = api(
        app,
        reqwest::Method::POST,
        "refunds",
        &format!("wpalt-refund-{id}"),
        &f,
    )
    .await?;
    settle_refund(app, &v).await
}
pub async fn cancel_subscription(app: &App, s: &Session, id: &str, version: i64) -> Result<()> {
    customer(app, s).await?;
    let sub=sqlx::query("SELECT provider_ref,version,state FROM shop_subscriptions WHERE id=$1 AND user_id=$2 AND provider='stripe'").bind(id).bind(&s.user.id).fetch_optional(&app.db.pool).await?.ok_or_else(Error::not_found)?;
    if sub.get::<i64, _>("version") != version {
        return Err(Error::conflict());
    }
    let reference: String = sub.get("provider_ref");
    if reference.is_empty() && sub.get::<String, _>("state") == "pending" {
        return billing::cancel(app, s, id, version).await;
    }
    provider_id(&reference, "sub_")?;
    let value = api(
        app,
        reqwest::Method::POST,
        &format!("subscriptions/{reference}"),
        &format!("wpalt-cancel-{id}-{version}"),
        &form(&[("cancel_at_period_end", "true".into())]),
    )
    .await?;
    if string(&value, "/id")? != reference
        || value.get("cancel_at_period_end").and_then(Value::as_bool) != Some(true)
    {
        return Err(Error::conflict());
    }
    let _g = app.mutation().await;
    customer(app, s).await?;
    let mut tx = app.db.pool.begin().await?;
    let current: String = sqlx::query_scalar(
        "SELECT state FROM shop_subscriptions WHERE id=$1 AND user_id=$2 AND provider_ref=$3",
    )
    .bind(id)
    .bind(&s.user.id)
    .bind(&reference)
    .fetch_one(&mut *tx)
    .await?;
    let due = sqlx::query(
        "SELECT * FROM shop_orders WHERE subscription_id=$1 AND payment_state='awaiting'",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    for order in due {
        orders::cancel_tx(&mut tx, &order, "cancelled").await?;
    }
    sqlx::query("UPDATE shop_subscriptions SET state=$1,next_variant='',next_price_minor=-1,provider_cancel_pending=0,provider_cancel_at=0,version=version+1 WHERE id=$2 AND provider_ref=$3").bind(if current=="active" || current=="cancel_at_end" {"cancel_at_end"} else {"cancelled"}).bind(id).bind(reference).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
