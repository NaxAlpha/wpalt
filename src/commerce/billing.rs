use super::*;
use chrono::{DateTime, Months, Utc};
use sqlx::Row;
pub fn utc(value: i64) -> String {
    DateTime::<Utc>::from_timestamp(value, 0)
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}
pub fn period_end(start: i64, interval: &str) -> Result<i64> {
    let t = DateTime::<Utc>::from_timestamp(start, 0)
        .ok_or(Error::invalid("Invalid billing timestamp."))?;
    let end = match interval {
        "day" => start.checked_add(86400),
        "week" => start.checked_add(604800),
        "month" => t.checked_add_months(Months::new(1)).map(|t| t.timestamp()),
        "year" => t.checked_add_months(Months::new(12)).map(|t| t.timestamp()),
        _ => None,
    }
    .ok_or(Error::invalid("Unsupported billing period."))?;
    if end <= start {
        return Err(Error::invalid("Invalid billing period."));
    }
    Ok(end)
}
pub fn proration(old: i64, new: i64, start: i64, end: i64, at: i64) -> Result<i64> {
    money(old)?;
    money(new)?;
    if new < old || start < 0 || end <= start || at < start || at >= end {
        return Err(Error::invalid(
            "Immediate proration supports upgrades within the current paid period.",
        ));
    }
    money(
        i64::try_from(
            (i128::from(new - old) * i128::from(end - at) + i128::from(end - start) / 2)
                / i128::from(end - start),
        )
        .map_err(|_| Error::invalid("Amount overflow."))?,
    )
}
/// Renewal and upgrade records reuse the original agreement's tax and contact snapshots.
async fn invoice(
    tx: &mut orders::Tx<'_>,
    sub: &sqlx::any::AnyRow,
    variant: &str,
    price: i64,
    target_price: i64,
    period: (i64, i64),
    purpose: &str,
) -> Result<String> {
    let (start, end) = period;
    let base=sqlx::query("SELECT * FROM shop_orders WHERE subscription_id=$1 AND purpose='purchase' ORDER BY created_at,id LIMIT 1").bind(sub.get::<String,_>("id")).fetch_one(&mut **tx).await?;
    let line=sqlx::query("SELECT p.id AS product_id,p.title,p.entitlement,v.sku,v.billing_interval FROM shop_variants v JOIN shop_products p ON p.id=v.product_id WHERE v.id=$1").bind(variant).fetch_one(&mut **tx).await?;
    let tax = basis(price, base.get("tax_bps"))?;
    let total = money(
        price
            .checked_add(tax)
            .ok_or(Error::invalid("Amount overflow."))?,
    )?;
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO shop_orders(id,user_id,request_key,request_digest,cart_version,customer_name,customer_email,shipping_address,currency,subtotal_minor,discount_minor,tax_minor,shipping_minor,total_minor,tax_bps,tax_shipping,provider,payment_state,expires_at,subscription_id,period_start,period_end,purpose,target_price_minor,created_at) VALUES($1,$2,$3,$4,0,$5,$6,'',$7,$8,0,$9,0,$10,$11,0,$12,'awaiting',$13,$14,$15,$16,$17,$18,$19)").bind(&id).bind(sub.get::<String,_>("user_id")).bind(&id).bind(crate::auth::digest(id.as_bytes())).bind(base.get::<String,_>("customer_name")).bind(base.get::<String,_>("customer_email")).bind(base.get::<String,_>("currency")).bind(price).bind(tax).bind(total).bind(base.get::<i64,_>("tax_bps")).bind(sub.get::<String,_>("provider")).bind(end).bind(sub.get::<String,_>("id")).bind(start).bind(end).bind(purpose).bind(target_price).bind(crate::now()).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO shop_order_lines(id,order_id,variant_id,product_id,title,sku,kind,quantity,unit_minor,line_minor,entitlement,allocation,billing_interval) VALUES($1,$2,$3,$4,$5,$6,'membership',1,$7,$7,$8,'held',$9)").bind(uuid::Uuid::new_v4().to_string()).bind(&id).bind(variant).bind(line.get::<String,_>("product_id")).bind(line.get::<String,_>("title")).bind(line.get::<String,_>("sku")).bind(price).bind(line.get::<String,_>("entitlement")).bind(line.get::<String,_>("billing_interval")).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO shop_notifications(id,order_id,kind,due_at,created_at) VALUES($1,$2,'billing',$3,$3)").bind(uuid::Uuid::new_v4().to_string()).bind(&id).bind(crate::now()).execute(&mut **tx).await?;
    orders::audit(tx, &id, "billing", purpose, total).await?;
    Ok(id)
}
pub async fn tick(app: &App) -> Result<usize> {
    if !app.config.commerce.enabled {
        return Ok(0);
    }
    let _guard = app.mutation().await?;
    let mut tx = app.db.pool.begin().await?;
    let at = crate::now();
    let mut count = 0;
    let ended=sqlx::query("SELECT id,state FROM shop_subscriptions WHERE state IN ('active','cancel_at_end') AND period_end<=$1 ORDER BY period_end,id LIMIT 40").bind(at).fetch_all(&mut *tx).await?;
    for s in ended {
        sqlx::query("UPDATE shop_subscriptions SET state=$1,version=version+1 WHERE id=$2")
            .bind(if s.get::<String, _>("state") == "cancel_at_end" {
                "cancelled"
            } else {
                "past_due"
            })
            .bind(s.get::<String, _>("id"))
            .execute(&mut *tx)
            .await?;
        count += 1;
    }
    let expired=sqlx::query("SELECT * FROM shop_orders WHERE purpose IN ('renewal','upgrade') AND payment_state='awaiting' AND expires_at<=$1 ORDER BY expires_at,id LIMIT 40").bind(at).fetch_all(&mut *tx).await?;
    for row in expired {
        orders::cancel_tx(&mut tx, &row, "failed").await?;
        count += 1;
    }
    // Exclude periods already invoiced, rather than repeatedly selecting completed work.
    let subs=sqlx::query("SELECT s.* FROM shop_subscriptions s WHERE s.provider='offline' AND s.state IN ('active','past_due') AND s.period_end<=$1 AND NOT EXISTS(SELECT 1 FROM shop_orders u WHERE u.subscription_id=s.id AND u.purpose='upgrade' AND u.payment_state='awaiting') AND NOT EXISTS(SELECT 1 FROM shop_orders o WHERE o.subscription_id=s.id AND o.purpose='renewal' AND o.period_start=s.period_end) ORDER BY s.period_end,s.id LIMIT 40").bind(at+3*86400).fetch_all(&mut *tx).await?;
    for s in subs {
        let start: i64 = s.get("period_end");
        let end = period_end(start, &s.get::<String, _>("billing_interval"))?;
        if end <= at {
            sqlx::query(
                "UPDATE shop_subscriptions SET state='cancelled',version=version+1 WHERE id=$1",
            )
            .bind(s.get::<String, _>("id"))
            .execute(&mut *tx)
            .await?;
            continue;
        }
        let next: String = s.get("next_variant");
        let variant = if next.is_empty() {
            s.get::<String, _>("variant_id")
        } else {
            next
        };
        let price = if variant == s.get::<String, _>("variant_id") {
            s.get::<i64, _>("price_minor")
        } else {
            s.get::<i64, _>("next_price_minor")
        };
        invoice(&mut tx, &s, &variant, price, price, (start, end), "renewal").await?;
        count += 1;
    }
    tx.commit().await?;
    Ok(count)
}
pub async fn cancel(app: &App, s: &Session, id: &str, version: i64) -> Result<()> {
    let _guard = app.mutation().await?;
    customer(app, s).await?;
    let mut tx = app.db.pool.begin().await?;
    let sub = sqlx::query("SELECT * FROM shop_subscriptions WHERE id=$1 AND user_id=$2")
        .bind(id)
        .bind(&s.user.id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(Error::not_found)?;
    if sub.get::<i64, _>("version") != version {
        return Err(Error::conflict());
    }
    if sub.get::<String, _>("provider") == "stripe"
        && !(sub.get::<String, _>("state") == "pending"
            && sub.get::<String, _>("provider_ref").is_empty())
    {
        return Err(Error::invalid(
            "Use the hosted subscription cancellation action.",
        ));
    }
    let state: String = sub.get("state");
    if !["active", "past_due", "pending"].contains(&state.as_str()) {
        return Err(Error::conflict());
    }
    let due = sqlx::query(
        "SELECT * FROM shop_orders WHERE subscription_id=$1 AND payment_state='awaiting'",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    for order in due {
        orders::cancel_tx(&mut tx, &order, "cancelled").await?;
    }
    sqlx::query(
        "UPDATE shop_subscriptions SET state=$1,next_variant='',next_price_minor=-1,version=version+1 WHERE id=$2",
    )
    .bind(if state == "active" {
        "cancel_at_end"
    } else {
        "cancelled"
    })
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
/// Same entitlement/interval plans only: paid upgrades now, scheduled downgrades next period.
pub async fn change(
    app: &App,
    s: &Session,
    id: &str,
    version: i64,
    variant: &str,
) -> Result<Option<String>> {
    let _guard = app.mutation().await?;
    customer(app, s).await?;
    let mut tx = app.db.pool.begin().await?;
    let sub = sqlx::query("SELECT * FROM shop_subscriptions WHERE id=$1 AND user_id=$2")
        .bind(id)
        .bind(&s.user.id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(Error::not_found)?;
    if sub.get::<i64, _>("version") != version
        || sub.get::<String, _>("state") != "active"
        || sub.get::<String, _>("provider") != "offline"
        || sub.get::<i64, _>("period_end") <= crate::now()
    {
        return Err(Error::conflict());
    }
    let v=sqlx::query("SELECT v.*,p.entitlement FROM shop_variants v JOIN shop_products p ON p.id=v.product_id WHERE v.id=$1 AND v.active=1 AND p.published=1 AND p.kind='membership'").bind(variant).fetch_optional(&mut *tx).await?.ok_or_else(Error::not_found)?;
    if v.get::<String, _>("entitlement") != sub.get::<String, _>("entitlement")
        || v.get::<String, _>("billing_interval") != sub.get::<String, _>("billing_interval")
        || v.get::<i64, _>("member_price_minor") != -1
        || v.get::<i64, _>("price_minor") == sub.get::<i64, _>("price_minor")
    {
        return Err(Error::invalid(
            "Choose another fixed-price plan with the same access and billing interval.",
        ));
    }
    let pending:i64=sqlx::query_scalar("SELECT COUNT(*) FROM shop_orders WHERE subscription_id=$1 AND payment_state='awaiting' AND purpose IN ('renewal','upgrade')").bind(id).fetch_one(&mut *tx).await?;
    if pending != 0 {
        return Err(Error::invalid(
            "Resolve the outstanding invoice before changing plan.",
        ));
    }
    let price: i64 = v.get("price_minor");
    let result = if price > sub.get::<i64, _>("price_minor") {
        let at = crate::now();
        let charge = proration(
            sub.get("price_minor"),
            price,
            sub.get("period_start"),
            sub.get("period_end"),
            at,
        )?;
        Some(
            invoice(
                &mut tx,
                &sub,
                variant,
                charge,
                price,
                (at, sub.get("period_end")),
                "upgrade",
            )
            .await?,
        )
    } else {
        None
    };
    sqlx::query("UPDATE shop_subscriptions SET next_variant=$1,next_price_minor=$2,version=version+1 WHERE id=$3")
        .bind(variant)
        .bind(price)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(result)
}
/// Called only after retrieving an authenticated provider invoice and its mapped subscription.
pub(crate) async fn provider_invoice(
    app: &App,
    reference: &str,
    start: i64,
    end: i64,
) -> Result<String> {
    let _guard = app.mutation().await?;
    let mut tx = app.db.pool.begin().await?;
    let s =
        sqlx::query("SELECT * FROM shop_subscriptions WHERE provider='stripe' AND provider_ref=$1")
            .bind(reference)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(Error::not_found)?;
    if let Some(id)=sqlx::query_scalar("SELECT id FROM shop_orders WHERE subscription_id=$1 AND period_start=$2 AND purpose='renewal'").bind(s.get::<String,_>("id")).bind(start).fetch_optional(&mut *tx).await?{return Ok(id)}
    if start < s.get::<i64, _>("period_end") || end <= start || end - start > 370 * 86400 {
        return Err(Error::conflict());
    }
    let id = invoice(
        &mut tx,
        &s,
        &s.get::<String, _>("variant_id"),
        s.get("price_minor"),
        s.get("price_minor"),
        (start, end),
        "renewal",
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}
