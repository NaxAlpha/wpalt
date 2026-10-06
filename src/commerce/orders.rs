use super::*;
use crate::now;
use sqlx::{Any, Row, Transaction};

pub type Tx<'a> = Transaction<'a, Any>;
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Line {
    pub variant_id: String,
    pub product_id: String,
    pub title: String,
    pub sku: String,
    pub kind: String,
    pub quantity: i64,
    pub unit_minor: i64,
    pub line_minor: i64,
    pub slot_id: String,
    pub entitlement: String,
    pub access_seconds: i64,
    pub billing_interval: String,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Quote {
    pub cart_version: i64,
    pub currency: String,
    pub lines: Vec<Line>,
    pub subtotal_minor: i64,
    pub discount_minor: i64,
    pub discount_bps: i64,
    pub discount_base_minor: i64,
    pub tax_minor: i64,
    pub shipping_minor: i64,
    pub total_minor: i64,
    pub tax_bps: i64,
    pub tax_shipping: bool,
    pub hash: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Checkout {
    pub request_key: String,
    pub cart_version: i64,
    pub quote_hash: String,
    pub shipping_address: String,
    pub discount_code: String,
    pub reward_code: String,
    pub referral_id: String,
    pub provider: String,
}
pub async fn cart_version(app: &App, user: &str) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT version FROM shop_carts WHERE user_id=$1")
            .bind(user)
            .fetch_optional(&app.db.pool)
            .await?
            .unwrap_or(0),
    )
}
pub async fn set_cart(
    app: &App,
    s: &Session,
    version: i64,
    variant: &str,
    slot: &str,
    quantity: i64,
) -> Result<i64> {
    if !(0..=100).contains(&quantity) {
        return Err(Error::invalid("Choose between one and one hundred units."));
    }
    crate::membership::uuid(variant)?;
    if !slot.is_empty() {
        crate::membership::uuid(slot)?
    }
    let _guard = app.mutation().await?;
    customer(app, s).await?;
    let mut tx = app.db.pool.begin().await?;
    let kind:Option<String>=sqlx::query_scalar("SELECT p.kind FROM shop_variants v JOIN shop_products p ON p.id=v.product_id WHERE v.id=$1 AND ($2=0 OR (v.active=1 AND p.published=1))").bind(variant).bind(quantity).fetch_optional(&mut *tx).await?;
    let kind = kind.ok_or_else(Error::not_found)?;
    if quantity > 0
        && (kind == "booking" && slot.is_empty()
            || kind != "booking" && !slot.is_empty()
            || ["digital", "membership"].contains(&kind.as_str()) && quantity > 1)
    {
        return Err(Error::invalid(
            "Choose a booking slot or a single digital/membership item.",
        ));
    }
    if !slot.is_empty() && quantity > 0 {
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM shop_slots s JOIN shop_resources r ON r.id=s.resource_id WHERE s.id=$1 AND s.variant_id=$2 AND s.active=1 AND r.active=1 AND (r.staff_id='' OR EXISTS(SELECT 1 FROM users staff WHERE staff.id=r.staff_id AND staff.role IN ('admin','editor'))) AND s.starts_at>$3").bind(slot).bind(variant).bind(now()).fetch_one(&mut *tx).await?;
        if count != 1 {
            return Err(Error::invalid("This slot is not available."));
        }
    }
    let current: Option<i64> =
        sqlx::query_scalar("SELECT version FROM shop_carts WHERE user_id=$1")
            .bind(&s.user.id)
            .fetch_optional(&mut *tx)
            .await?;
    if current.unwrap_or(0) != version {
        return Err(Error::conflict());
    }
    if current.is_none() {
        sqlx::query("INSERT INTO shop_carts(user_id,updated_at) VALUES($1,$2)")
            .bind(&s.user.id)
            .bind(now())
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query(
            "UPDATE shop_carts SET version=version+1,updated_at=$1 WHERE user_id=$2 AND version=$3",
        )
        .bind(now())
        .bind(&s.user.id)
        .bind(version)
        .execute(&mut *tx)
        .await?;
    }
    if quantity == 0 {
        sqlx::query(
            "DELETE FROM shop_cart_lines WHERE user_id=$1 AND variant_id=$2 AND slot_id=$3",
        )
        .bind(&s.user.id)
        .bind(variant)
        .bind(slot)
        .execute(&mut *tx)
        .await?;
    } else {
        let existing:i64=sqlx::query_scalar("SELECT COUNT(*) FROM shop_cart_lines WHERE user_id=$1 AND variant_id=$2 AND slot_id=$3").bind(&s.user.id).bind(variant).bind(slot).fetch_one(&mut *tx).await?;
        if existing == 0 {
            let count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM shop_cart_lines WHERE user_id=$1")
                    .bind(&s.user.id)
                    .fetch_one(&mut *tx)
                    .await?;
            if count >= 20 {
                return Err(Error::invalid(
                    "A cart supports up to twenty distinct lines.",
                ));
            }
        }
        sqlx::query("INSERT INTO shop_cart_lines(user_id,variant_id,slot_id,quantity) VALUES($1,$2,$3,$4) ON CONFLICT(user_id,variant_id,slot_id) DO UPDATE SET quantity=excluded.quantity").bind(&s.user.id).bind(variant).bind(slot).bind(quantity).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(version + 1)
}
pub async fn quote(app: &App, s: &Session, code: &str) -> Result<Quote> {
    customer(app, s).await?;
    let mut tx = app.db.pool.begin().await?;
    if app.db.postgres {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
    }
    quote_tx(&mut tx, &s.user.id, code).await
}
pub(crate) async fn quote_tx(tx: &mut Tx<'_>, user: &str, code: &str) -> Result<Quote> {
    if code.len() > 40 {
        return Err(Error::invalid("Coupon code too long."));
    }
    let at = now();
    let version: i64 = sqlx::query_scalar("SELECT version FROM shop_carts WHERE user_id=$1")
        .bind(user)
        .fetch_optional(&mut **tx)
        .await?
        .unwrap_or(0);
    let settings = sqlx::query("SELECT * FROM shop_settings WHERE id=1")
        .fetch_one(&mut **tx)
        .await?;
    let rows=sqlx::query("SELECT c.variant_id,c.slot_id,c.quantity,v.title AS variant_title,v.sku,v.price_minor,v.member_price_minor,v.member_key,v.stock_total,v.held,v.sold,v.billing_interval,v.active AS variant_active,p.id AS product_id,p.title,p.kind,p.entitlement,p.access_seconds,p.published,s.id AS slot_exists,s.variant_id AS slot_variant,s.starts_at,s.capacity,s.held AS slot_held,s.booked,s.active AS slot_active,CASE WHEN r.active=1 AND (r.staff_id='' OR EXISTS(SELECT 1 FROM users staff WHERE staff.id=r.staff_id AND staff.role IN ('admin','editor'))) THEN 1 ELSE 0 END AS resource_active,CASE WHEN v.member_price_minor>=0 AND EXISTS(SELECT 1 FROM member_grants g WHERE g.user_id=$1 AND g.entitlement=v.member_key AND g.revoked=0 AND g.starts_at<=$2 AND (g.expires_at=0 OR g.expires_at>$2)) THEN v.member_price_minor ELSE v.price_minor END AS unit_minor FROM shop_cart_lines c JOIN shop_variants v ON v.id=c.variant_id JOIN shop_products p ON p.id=v.product_id LEFT JOIN shop_slots s ON s.id=c.slot_id LEFT JOIN shop_resources r ON r.id=s.resource_id WHERE c.user_id=$1 ORDER BY c.variant_id,c.slot_id LIMIT 21").bind(user).bind(at).fetch_all(&mut **tx).await?;
    if rows.is_empty() || rows.len() > 20 {
        return Err(Error::invalid(
            "Your cart is empty or exceeds the supported size.",
        ));
    }
    let mut lines = vec![];
    let mut subtotal = 0i64;
    let mut physical = false;
    let mut recurring = 0;
    for r in rows {
        if r.get::<i64, _>("variant_active") != 1 || r.get::<i64, _>("published") != 1 {
            return Err(Error::invalid("A cart item is no longer available."));
        }
        let quantity: i64 = r.get("quantity");
        let stock: i64 = r.get("stock_total");
        if stock >= 0 && stock - r.get::<i64, _>("held") - r.get::<i64, _>("sold") < quantity {
            return Err(Error::invalid("There is not enough stock for your cart."));
        }
        let kind: String = r.get("kind");
        let slot: String = r.get("slot_id");
        if kind == "booking"
            && (slot.is_empty()
                || r.try_get::<String, _>("slot_variant").ok().as_deref()
                    != Some(r.get::<String, _>("variant_id").as_str())
                || r.try_get::<i64, _>("slot_active").ok() != Some(1)
                || r.try_get::<i64, _>("resource_active").ok() != Some(1)
                || r.try_get::<i64, _>("starts_at").unwrap_or(0) <= at
                || r.try_get::<i64, _>("capacity").unwrap_or(0)
                    - r.try_get::<i64, _>("slot_held").unwrap_or(0)
                    - r.try_get::<i64, _>("booked").unwrap_or(0)
                    < quantity)
        {
            return Err(Error::invalid(
                "There is not enough capacity for the selected slot.",
            ));
        }
        if kind != "booking" && !slot.is_empty()
            || ["digital", "membership"].contains(&kind.as_str()) && quantity != 1
        {
            return Err(Error::invalid("Invalid cart item quantity or slot."));
        }
        let unit: i64 = r.get("unit_minor");
        let line = money(
            unit.checked_mul(quantity)
                .ok_or(Error::invalid("Amount overflow."))?,
        )?;
        subtotal = money(
            subtotal
                .checked_add(line)
                .ok_or(Error::invalid("Amount overflow."))?,
        )?;
        physical |= kind == "physical";
        let interval: String = r.get("billing_interval");
        recurring += usize::from(!interval.is_empty());
        lines.push(Line {
            variant_id: r.get("variant_id"),
            product_id: r.get("product_id"),
            title: format!(
                "{} · {}",
                r.get::<String, _>("title"),
                r.get::<String, _>("variant_title")
            ),
            sku: r.get("sku"),
            kind,
            quantity,
            unit_minor: unit,
            line_minor: line,
            slot_id: slot,
            entitlement: r.get("entitlement"),
            access_seconds: r.get("access_seconds"),
            billing_interval: interval,
        });
    }
    if recurring > 0 && (lines.len() != 1 || !code.is_empty()) {
        return Err(Error::invalid(
            "Recurring memberships use a separate single-item checkout without coupons.",
        ));
    }
    let mut discount_bps = 0;
    let mut discount_base = 0;
    if !code.is_empty() {
        let d=sqlx::query("SELECT * FROM shop_discounts WHERE code=$1 AND active=1 AND starts_at<=$2 AND expires_at>$2 AND (max_uses=0 OR held+used<max_uses)").bind(code).bind(at).fetch_optional(&mut **tx).await?.ok_or(Error::invalid("This coupon is unavailable or has reached its use limit."))?;
        let key: String = d.get("member_key");
        if !key.is_empty() {
            let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM member_grants WHERE user_id=$1 AND entitlement=$2 AND revoked=0 AND starts_at<=$3 AND (expires_at=0 OR expires_at>$3)").bind(user).bind(key).bind(at).fetch_one(&mut **tx).await?;
            if count == 0 {
                return Err(Error::forbidden());
            }
        }
        let product: String = d.get("product_id");
        discount_base = lines
            .iter()
            .filter(|l| product.is_empty() || l.product_id == product)
            .map(|l| l.line_minor)
            .sum();
        if discount_base == 0 {
            return Err(Error::invalid(
                "This coupon does not apply to these products.",
            ));
        }
        discount_bps = d.get("bps");
    }
    let discount = basis(discount_base, discount_bps)?;
    let shipping = if physical {
        settings.get("shipping_minor")
    } else {
        0
    };
    let tax_bps = settings.get("tax_bps");
    let tax_shipping = settings.get::<i64, _>("tax_shipping") == 1;
    let tax = basis(
        subtotal - discount + if tax_shipping { shipping } else { 0 },
        tax_bps,
    )?;
    let total = money(subtotal - discount + shipping + tax)?;
    let mut q = Quote {
        cart_version: version,
        currency: settings.get("currency"),
        lines,
        subtotal_minor: subtotal,
        discount_minor: discount,
        discount_bps,
        discount_base_minor: discount_base,
        tax_minor: tax,
        shipping_minor: shipping,
        total_minor: total,
        tax_bps,
        tax_shipping,
        hash: String::new(),
    };
    q.hash = crate::auth::digest(
        &serde_json::to_vec(&q).map_err(|_| Error::invalid("Invalid checkout."))?,
    );
    Ok(q)
}
pub async fn checkout(app: &App, s: &Session, input: &Checkout) -> Result<String> {
    crate::membership::uuid(&input.request_key)?;
    if input.shipping_address.len() > 2000
        || input.reward_code.len() > 80
        || input.quote_hash.len() != 64
        || !["offline", "stripe"].contains(&input.provider.as_str())
        || input.provider == "stripe" && !app.config.commerce.stripe.enabled
    {
        return Err(Error::invalid(
            "Check checkout details and configured payment method.",
        ));
    }
    if !input.referral_id.is_empty() {
        crate::membership::uuid(&input.referral_id)?
    }
    let _guard = app.mutation().await?;
    customer(app, s).await?;
    let mut tx = app.db.pool.begin().await?;
    let digest = crate::auth::digest(
        &serde_json::to_vec(input).map_err(|_| Error::invalid("Invalid checkout."))?,
    );
    if let Some(old) =
        sqlx::query("SELECT id,request_digest FROM shop_orders WHERE user_id=$1 AND request_key=$2")
            .bind(&s.user.id)
            .bind(&input.request_key)
            .fetch_optional(&mut *tx)
            .await?
    {
        if old.get::<String, _>("request_digest") != digest {
            return Err(Error::conflict());
        }
        return Ok(old.get("id"));
    }
    expire_tx(&mut tx, now()).await?;
    let q = quote_tx(&mut tx, &s.user.id, &input.discount_code).await?;
    if q.cart_version != input.cart_version || q.hash != input.quote_hash {
        return Err(Error::conflict());
    }
    if input.provider == "stripe" && q.total_minor == 0 {
        return Err(Error::invalid(
            "Choose offline for a free order; no payment is collected.",
        ));
    }
    if q.lines.iter().any(|l| l.kind == "physical") && input.shipping_address.trim().is_empty() {
        return Err(Error::invalid("Physical products need a shipping address."));
    }
    if q.lines.iter().any(|l| !l.entitlement.is_empty()) && !app.config.membership_enabled {
        return Err(Error::invalid(
            "Membership must be enabled before selling protected access.",
        ));
    }
    let mut claim = String::new();
    if !input.discount_code.is_empty() {
        let d = sqlx::query("SELECT reward_id FROM shop_discounts WHERE code=$1")
            .bind(&input.discount_code)
            .fetch_one(&mut *tx)
            .await?;
        let reward: String = d.get("reward_id");
        if !reward.is_empty() {
            claim = sqlx::query_scalar(
                "SELECT id FROM promotion_claims WHERE code=$1 AND reward_id=$2",
            )
            .bind(&input.reward_code)
            .bind(reward)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::invalid(
                "This offer needs its single-use reward code.",
            ))?;
            let used:i64=sqlx::query_scalar("SELECT COUNT(*) FROM shop_reward_redemptions WHERE claim_id=$1 AND state IN ('held','used')").bind(&claim).fetch_one(&mut *tx).await?;
            if used != 0 {
                return Err(Error::invalid(
                    "This reward is already reserved or redeemed.",
                ));
            }
        } else if !input.reward_code.is_empty() {
            return Err(Error::invalid("This coupon does not use a reward code."));
        }
        if sqlx::query("UPDATE shop_discounts SET held=held+1 WHERE code=$1 AND active=1 AND starts_at<=$2 AND expires_at>$2 AND (max_uses=0 OR held+used<max_uses)").bind(&input.discount_code).bind(now()).execute(&mut *tx).await?.rows_affected()!=1{return Err(Error::conflict())}
    } else if !input.reward_code.is_empty() {
        return Err(Error::invalid(
            "Choose the coupon associated with this reward.",
        ));
    }
    if !input.referral_id.is_empty() {
        let referrer: Option<String> =
            sqlx::query_scalar("SELECT user_id FROM member_referrals WHERE id=$1")
                .bind(&input.referral_id)
                .fetch_optional(&mut *tx)
                .await?;
        if referrer.is_none() || referrer.as_deref() == Some(&s.user.id) {
            return Err(Error::invalid(
                "Choose a valid referral belonging to another member.",
            ));
        }
    }
    let identity = sqlx::query("SELECT name,email FROM users WHERE id=$1")
        .bind(&s.user.id)
        .fetch_one(&mut *tx)
        .await?;
    let id = uuid::Uuid::new_v4().to_string();
    let expires = now()
        + if input.provider == "stripe" {
            app.config.commerce.hold_seconds.max(1800)
        } else {
            app.config.commerce.hold_seconds
        };
    let mut subscription = String::new();
    let mut period_start = 0;
    let mut period_end = 0;
    if !q.lines[0].billing_interval.is_empty() {
        subscription = uuid::Uuid::new_v4().to_string();
        period_start = now();
        period_end = super::billing::period_end(period_start, &q.lines[0].billing_interval)?;
        sqlx::query("INSERT INTO shop_subscriptions(id,user_id,variant_id,entitlement,price_minor,billing_interval,period_start,period_end,state,provider,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'pending',$9,$10)").bind(&subscription).bind(&s.user.id).bind(&q.lines[0].variant_id).bind(&q.lines[0].entitlement).bind(q.lines[0].unit_minor).bind(&q.lines[0].billing_interval).bind(period_start).bind(period_end).bind(&input.provider).bind(now()).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO shop_orders(id,user_id,request_key,request_digest,cart_version,customer_name,customer_email,shipping_address,currency,subtotal_minor,discount_minor,discount_bps,discount_base_minor,tax_minor,shipping_minor,total_minor,tax_bps,tax_shipping,discount_code,reward_claim,referral_id,commission_bps,provider,payment_state,expires_at,subscription_id,period_start,period_end,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,'awaiting',$24,$25,$26,$27,$28)").bind(&id).bind(&s.user.id).bind(&input.request_key).bind(&digest).bind(q.cart_version).bind(identity.get::<String,_>("name")).bind(identity.get::<String,_>("email")).bind(input.shipping_address.trim()).bind(&q.currency).bind(q.subtotal_minor).bind(q.discount_minor).bind(q.discount_bps).bind(q.discount_base_minor).bind(q.tax_minor).bind(q.shipping_minor).bind(q.total_minor).bind(q.tax_bps).bind(i64::from(q.tax_shipping)).bind(&input.discount_code).bind(&claim).bind(&input.referral_id).bind(app.config.commerce.referral_bps).bind(&input.provider).bind(expires).bind(&subscription).bind(period_start).bind(period_end).bind(now()).execute(&mut *tx).await?;
    for line in &q.lines {
        let variant=sqlx::query("UPDATE shop_variants SET held=held+CASE WHEN stock_total>=0 THEN $1 ELSE 0 END WHERE id=$2 AND active=1 AND (stock_total=-1 OR stock_total-held-sold>=$1)").bind(line.quantity).bind(&line.variant_id).execute(&mut *tx).await?;
        if variant.rows_affected() != 1 {
            return Err(Error::invalid(
                "The last available unit was taken; no order or payment was created.",
            ));
        }
        if !line.slot_id.is_empty()
            && sqlx::query("UPDATE shop_slots SET held=held+$1 WHERE id=$2 AND variant_id=$3 AND active=1 AND starts_at>$4 AND capacity-held-booked>=$1 AND EXISTS(SELECT 1 FROM shop_resources r WHERE r.id=shop_slots.resource_id AND r.active=1 AND (r.staff_id='' OR EXISTS(SELECT 1 FROM users staff WHERE staff.id=r.staff_id AND staff.role IN ('admin','editor'))))").bind(line.quantity).bind(&line.slot_id).bind(&line.variant_id).bind(now()).execute(&mut *tx).await?.rows_affected()!=1{return Err(Error::invalid("The last slot was taken; no order or payment was created."))}

        sqlx::query("INSERT INTO shop_order_lines(id,order_id,variant_id,product_id,title,sku,kind,quantity,unit_minor,line_minor,slot_id,entitlement,access_seconds,allocation,billing_interval) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,'held',$14)").bind(uuid::Uuid::new_v4().to_string()).bind(&id).bind(&line.variant_id).bind(&line.product_id).bind(&line.title).bind(&line.sku).bind(&line.kind).bind(line.quantity).bind(line.unit_minor).bind(line.line_minor).bind(&line.slot_id).bind(&line.entitlement).bind(line.access_seconds).bind(&line.billing_interval).execute(&mut *tx).await?;
    }
    if !claim.is_empty() {
        sqlx::query("INSERT INTO shop_reward_redemptions(claim_id,order_id,state) VALUES($1,$2,'held') ON CONFLICT(claim_id) DO UPDATE SET order_id=excluded.order_id,state='held' WHERE shop_reward_redemptions.state='released'").bind(&claim).bind(&id).execute(&mut *tx).await?;
    }
    sqlx::query("DELETE FROM shop_cart_lines WHERE user_id=$1")
        .bind(&s.user.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE shop_carts SET version=version+1,updated_at=$1 WHERE user_id=$2")
        .bind(now())
        .bind(&s.user.id)
        .execute(&mut *tx)
        .await?;
    let contact: Option<String> =
        sqlx::query_scalar("SELECT id FROM audience_contacts WHERE email=$1")
            .bind(identity.get::<String, _>("email"))
            .fetch_optional(&mut *tx)
            .await?;
    if contact.is_none() {
        crate::business::quotas::reserve(app, &mut tx, "contacts", 1, 0).await?;
        sqlx::query("INSERT INTO audience_contacts(id,email,name,created_at) VALUES($1,$2,$3,$4)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(identity.get::<String, _>("email"))
            .bind(identity.get::<String, _>("name"))
            .bind(now())
            .execute(&mut *tx)
            .await?;
    }
    audit(&mut tx, &id, &s.user.id, "checkout", q.total_minor).await?;
    if q.total_minor == 0 {
        payment_tx(
            app,
            &mut tx,
            &Payment {
                order_id: id.clone(),
                provider: input.provider.clone(),
                reference: format!("free:{id}"),
                amount_minor: 0,
                currency: q.currency.clone(),
                paid_at: now(),
                subscription_ref: String::new(),
                period_start: 0,
                period_end: 0,
            },
            None,
        )
        .await?;
    }
    tx.commit().await?;
    tracing::info!(event="commerce_checkout_created",order_id=%id,total_minor=q.total_minor);
    Ok(id)
}
pub(crate) async fn release_allocations(tx: &mut Tx<'_>, id: &str, restock: bool) -> Result<()> {
    let lines=sqlx::query("SELECT id,variant_id,slot_id,quantity,allocation FROM shop_order_lines WHERE order_id=$1 ORDER BY id").bind(id).fetch_all(&mut **tx).await?;
    for line in lines {
        let state: String = line.get("allocation");
        let quantity: i64 = line.get("quantity");
        let slot: String = line.get("slot_id");
        if state == "held" {
            sqlx::query("UPDATE shop_variants SET held=held-CASE WHEN stock_total>=0 THEN $1 ELSE 0 END WHERE id=$2").bind(quantity).bind(line.get::<String,_>("variant_id")).execute(&mut **tx).await?;
            if !slot.is_empty() {
                sqlx::query("UPDATE shop_slots SET held=held-$1 WHERE id=$2")
                    .bind(quantity)
                    .bind(&slot)
                    .execute(&mut **tx)
                    .await?;
            }
            sqlx::query("UPDATE shop_order_lines SET allocation='released' WHERE id=$1")
                .bind(line.get::<String, _>("id"))
                .execute(&mut **tx)
                .await?;
        } else if state == "sold" {
            if restock {
                sqlx::query("UPDATE shop_variants SET sold=sold-CASE WHEN stock_total>=0 THEN $1 ELSE 0 END WHERE id=$2").bind(quantity).bind(line.get::<String,_>("variant_id")).execute(&mut **tx).await?;
            }
            if !slot.is_empty() {
                sqlx::query("UPDATE shop_slots SET booked=booked-$1 WHERE id=$2")
                    .bind(quantity)
                    .bind(&slot)
                    .execute(&mut **tx)
                    .await?;
            }
            if restock || !slot.is_empty() {
                sqlx::query("UPDATE shop_order_lines SET allocation='restocked' WHERE id=$1")
                    .bind(line.get::<String, _>("id"))
                    .execute(&mut **tx)
                    .await?;
            }
        }
    }
    Ok(())
}
pub(crate) async fn cancel_tx(
    tx: &mut Tx<'_>,
    order: &sqlx::any::AnyRow,
    state: &str,
) -> Result<()> {
    let id: String = order.get("id");
    release_allocations(tx, &id, false).await?;
    audit(tx, &id, "system", state, 0).await?;
    let code: String = order.get("discount_code");
    if !code.is_empty() {
        sqlx::query("UPDATE shop_discounts SET held=held-1 WHERE code=$1")
            .bind(code)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query(
        "UPDATE shop_reward_redemptions SET state='released' WHERE order_id=$1 AND state='held'",
    )
    .bind(&id)
    .execute(&mut **tx)
    .await?;
    sqlx::query("UPDATE shop_orders SET payment_state=$1,fulfillment='cancelled',version=version+1 WHERE id=$2 AND payment_state='awaiting'").bind(state).bind(&id).execute(&mut **tx).await?;
    let subscription: String = order.get("subscription_id");
    if !subscription.is_empty() && order.get::<String, _>("purpose") == "purchase" {
        sqlx::query("UPDATE shop_subscriptions SET state='cancelled',version=version+1 WHERE id=$1 AND state='pending'").bind(&subscription).execute(&mut **tx).await?;
    }
    if !subscription.is_empty() && order.get::<String, _>("purpose") == "upgrade" {
        sqlx::query("UPDATE shop_subscriptions SET next_variant='',next_price_minor=-1,version=version+1 WHERE id=$1")
            .bind(&subscription)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
pub(crate) async fn expire_tx(tx: &mut Tx<'_>, at: i64) -> Result<usize> {
    let rows=sqlx::query("SELECT * FROM shop_orders WHERE payment_state='awaiting' AND purpose='purchase' AND expires_at<=$1 ORDER BY expires_at,id LIMIT 40").bind(at).fetch_all(&mut **tx).await?;
    let count = rows.len();
    for row in rows {
        cancel_tx(tx, &row, "cancelled").await?;
    }
    Ok(count)
}
pub async fn expire(app: &App) -> Result<usize> {
    let _guard = app.mutation().await?;
    let mut tx = app.db.pool.begin().await?;
    let count = expire_tx(&mut tx, now()).await?;
    tx.commit().await?;
    Ok(count)
}
pub(crate) async fn audit(
    tx: &mut Tx<'_>,
    order: &str,
    actor: &str,
    action: &str,
    amount: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO shop_history(id,order_id,actor,action,amount_minor,created_at) VALUES($1,$2,$3,$4,$5,$6)").bind(uuid::Uuid::new_v4().to_string()).bind(order).bind(actor).bind(action).bind(amount).bind(now()).execute(&mut **tx).await?;
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Payment {
    pub order_id: String,
    pub provider: String,
    pub reference: String,
    pub amount_minor: i64,
    pub currency: String,
    pub paid_at: i64,
    pub subscription_ref: String,
    pub period_start: i64,
    pub period_end: i64,
}
pub async fn record_offline(
    app: &App,
    s: &Session,
    order: &str,
    version: i64,
    reference: &str,
) -> Result<()> {
    owner(app, s).await?;
    let row =
        sqlx::query("SELECT provider,total_minor,currency,version FROM shop_orders WHERE id=$1")
            .bind(order)
            .fetch_optional(&app.db.pool)
            .await?
            .ok_or_else(Error::not_found)?;
    if row.get::<String, _>("provider") != "offline" || row.get::<i64, _>("version") != version {
        return Err(Error::conflict());
    }
    confirm_payment_as(
        app,
        &Payment {
            order_id: order.into(),
            provider: "offline".into(),
            reference: reference.into(),
            amount_minor: row.get("total_minor"),
            currency: row.get("currency"),
            paid_at: now(),
            subscription_ref: String::new(),
            period_start: 0,
            period_end: 0,
        },
        Some((s, version)),
    )
    .await
}
pub async fn confirm_payment(app: &App, p: &Payment) -> Result<()> {
    confirm_payment_as(app, p, None).await
}
async fn confirm_payment_as(app: &App, p: &Payment, actor: Option<(&Session, i64)>) -> Result<()> {
    text(&p.reference, 200)?;
    money(p.amount_minor)?;
    if p.paid_at <= 0 || p.paid_at > now() + 60 || p.subscription_ref.len() > 200 {
        return Err(Error::invalid("Invalid payment record."));
    }
    let _guard = app.mutation().await?;
    if let Some((s, _)) = actor {
        owner(app, s).await?;
    }
    if !app.config.commerce.enabled {
        return Err(Error::forbidden());
    }
    let mut tx = app.db.pool.begin().await?;
    payment_tx(app, &mut tx, p, actor).await?;
    tx.commit().await?;
    tracing::info!(event="commerce_payment_reconciled",order_id=%p.order_id,amount_minor=p.amount_minor);
    Ok(())
}
async fn payment_tx(
    app: &App,
    tx: &mut Tx<'_>,
    p: &Payment,
    actor: Option<(&Session, i64)>,
) -> Result<()> {
    let order = sqlx::query("SELECT * FROM shop_orders WHERE id=$1")
        .bind(&p.order_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(Error::not_found)?;
    if let Some((_, version)) = actor
        && order.get::<i64, _>("version") != version
    {
        return Err(Error::conflict());
    }
    if order.get::<String, _>("provider") != p.provider
        || order.get::<String, _>("currency") != p.currency
        || order.get::<i64, _>("total_minor") != p.amount_minor
    {
        return Err(Error::invalid(
            "Payment does not match the authoritative order.",
        ));
    }
    if let Some(old)=sqlx::query("SELECT order_id,amount_minor,currency FROM shop_payments WHERE provider=$1 AND reference=$2").bind(&p.provider).bind(&p.reference).fetch_optional(&mut **tx).await?{if old.get::<String,_>("order_id")!=p.order_id||old.get::<i64,_>("amount_minor")!=p.amount_minor||old.get::<String,_>("currency")!=p.currency{return Err(Error::conflict())}return Ok(())}
    if order.get::<i64, _>("paid_minor") != 0
        || !["awaiting", "cancelled", "failed"]
            .contains(&order.get::<String, _>("payment_state").as_str())
    {
        return Err(Error::conflict());
    }
    let lines = sqlx::query("SELECT * FROM shop_order_lines WHERE order_id=$1 ORDER BY id")
        .bind(&p.order_id)
        .fetch_all(&mut **tx)
        .await?;
    let user: String = order.get("user_id");
    let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id=$1")
        .bind(&user)
        .fetch_one(&mut **tx)
        .await?;
    let mut late = order.get::<String, _>("payment_state") != "awaiting"
        || order.get::<i64, _>("expires_at") <= p.paid_at
        || role == "disabled";
    if !order.get::<String, _>("subscription_id").is_empty() {
        let sub = sqlx::query("SELECT state,period_end FROM shop_subscriptions WHERE id=$1")
            .bind(order.get::<String, _>("subscription_id"))
            .fetch_one(&mut **tx)
            .await?;
        let state: String = sub.get("state");
        late |= state == "cancelled"
            || state == "cancel_at_end"
                && order.get::<String, _>("purpose") == "renewal"
                && order.get::<i64, _>("period_start") >= sub.get::<i64, _>("period_end");
    }
    for l in &lines {
        if l.get::<String, _>("allocation") != "held" {
            late = true
        }
        let slot: String = l.get("slot_id");
        if !slot.is_empty() {
            let good:i64=sqlx::query_scalar("SELECT COUNT(*) FROM shop_slots s JOIN shop_resources r ON r.id=s.resource_id WHERE s.id=$1 AND s.active=1 AND r.active=1 AND (r.staff_id='' OR EXISTS(SELECT 1 FROM users staff WHERE staff.id=r.staff_id AND staff.role IN ('admin','editor'))) AND s.starts_at>$2").bind(slot).bind(now()).fetch_one(&mut **tx).await?;
            late |= good != 1;
        }
    }
    sqlx::query("INSERT INTO shop_payments(id,order_id,provider,reference,amount_minor,currency,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(uuid::Uuid::new_v4().to_string()).bind(&p.order_id).bind(&p.provider).bind(&p.reference).bind(p.amount_minor).bind(&p.currency).bind(now()).execute(&mut **tx).await?;
    if late {
        if order.get::<String, _>("payment_state") == "awaiting" {
            cancel_tx(tx, &order, "cancelled").await?;
        }
        if !p.subscription_ref.is_empty() && !order.get::<String, _>("subscription_id").is_empty() {
            sqlx::query("UPDATE shop_subscriptions SET provider_ref=$1,state='cancelled',provider_cancel_pending=1,provider_cancel_at=0,version=version+1 WHERE id=$2 AND (provider_ref='' OR provider_ref=$1)").bind(&p.subscription_ref).bind(order.get::<String,_>("subscription_id")).execute(&mut **tx).await?;
        }
        sqlx::query("UPDATE shop_orders SET paid_minor=$1,payment_ref=$2,payment_state='needs_refund',fulfillment='cancelled',version=version+1 WHERE id=$3").bind(p.amount_minor).bind(&p.reference).bind(&p.order_id).execute(&mut **tx).await?;
        audit(
            tx,
            &p.order_id,
            &p.provider,
            "late_payment_needs_refund",
            p.amount_minor,
        )
        .await?;
        tracing::warn!(event="commerce_late_payment",order_id=%p.order_id);
        return Ok(());
    }
    let subscription: String = order.get("subscription_id");
    let purpose: String = order.get("purpose");
    let mut starts = now();
    let mut ends = 0;
    if !subscription.is_empty() {
        let sub = sqlx::query("SELECT * FROM shop_subscriptions WHERE id=$1")
            .bind(&subscription)
            .fetch_one(&mut **tx)
            .await?;
        if purpose == "purchase" {
            ends = billing::period_end(starts, &sub.get::<String, _>("billing_interval"))?;
        } else {
            starts = order.get("period_start");
            ends = order.get("period_end");
        }
        if p.provider == "stripe" {
            if p.subscription_ref.is_empty()
                || p.period_start <= 0
                || p.period_end <= p.period_start
                || p.period_end - p.period_start > 370 * 86400
            {
                return Err(Error::invalid(
                    "Provider billing period or subscription mapping is missing.",
                ));
            }
            starts = p.period_start;
            ends = p.period_end;
        }
        sqlx::query("UPDATE shop_subscriptions SET state='active',period_start=$1,period_end=$2,provider_ref=CASE WHEN $3<>'' THEN $3 ELSE provider_ref END,version=version+1 WHERE id=$4").bind(starts).bind(ends).bind(&p.subscription_ref).bind(&subscription).execute(&mut **tx).await?;
        if purpose != "purchase" {
            let variant: String = lines[0].get("variant_id");
            let price: i64 = order.get("target_price_minor");
            sqlx::query("UPDATE shop_subscriptions SET variant_id=$1,price_minor=$2,next_variant='',next_price_minor=-1,version=version+1 WHERE id=$3").bind(variant).bind(price).bind(&subscription).execute(&mut **tx).await?;
        }
        sqlx::query("UPDATE shop_orders SET period_start=$1,period_end=$2 WHERE id=$3")
            .bind(starts)
            .bind(ends)
            .bind(&p.order_id)
            .execute(&mut **tx)
            .await?;
    }
    for l in &lines {
        let quantity: i64 = l.get("quantity");
        sqlx::query("UPDATE shop_variants SET held=held-CASE WHEN stock_total>=0 THEN $1 ELSE 0 END,sold=sold+CASE WHEN stock_total>=0 THEN $1 ELSE 0 END WHERE id=$2").bind(quantity).bind(l.get::<String,_>("variant_id")).execute(&mut **tx).await?;
        let slot: String = l.get("slot_id");
        if !slot.is_empty() {
            let at:i64=sqlx::query_scalar("UPDATE shop_slots SET held=held-$1,booked=booked+$1 WHERE id=$2 RETURNING starts_at").bind(quantity).bind(&slot).fetch_one(&mut **tx).await?;
            booking::schedule(tx, &p.order_id, &slot, at).await?;
        }
        let entitlement: String = l.get("entitlement");
        let mut grant = String::new();
        if !entitlement.is_empty() {
            if !app.config.membership_enabled {
                return Err(Error::invalid(
                    "Enable membership to reconcile paid access.",
                ));
            }
            grant = uuid::Uuid::new_v4().to_string();
            let expiry = if ends > 0 {
                ends
            } else {
                let duration: i64 = l.get("access_seconds");
                if duration == 0 {
                    0
                } else {
                    now()
                        .checked_add(duration)
                        .ok_or(Error::invalid("Access timestamp overflow."))?
                }
            };
            sqlx::query("INSERT INTO member_grants(id,user_id,entitlement,starts_at,expires_at,origin,created_at) VALUES($1,$2,$3,$4,$5,$6,$4)").bind(&grant).bind(&user).bind(&entitlement).bind(starts).bind(expiry).bind(format!("order:{}",p.order_id)).execute(&mut **tx).await?;
            if !subscription.is_empty() {
                sqlx::query("UPDATE shop_subscriptions SET grant_id=$1 WHERE id=$2")
                    .bind(&grant)
                    .bind(&subscription)
                    .execute(&mut **tx)
                    .await?;
            }
        }
        sqlx::query("UPDATE shop_order_lines SET allocation='sold',grant_id=$1 WHERE id=$2 AND allocation='held'").bind(grant).bind(l.get::<String,_>("id")).execute(&mut **tx).await?;
    }
    let code: String = order.get("discount_code");
    if !code.is_empty() {
        sqlx::query("UPDATE shop_discounts SET held=held-1,used=used+1 WHERE code=$1")
            .bind(code)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query(
        "UPDATE shop_reward_redemptions SET state='used' WHERE order_id=$1 AND state='held'",
    )
    .bind(&p.order_id)
    .execute(&mut **tx)
    .await?;
    let referral: String = order.get("referral_id");
    if !referral.is_empty() {
        let commission = basis(
            order.get::<i64, _>("subtotal_minor") - order.get::<i64, _>("discount_minor"),
            order.get("commission_bps"),
        )?;
        sqlx::query("INSERT INTO member_commissions(id,referral_id,reference,amount_minor,currency,state,created_at) VALUES($1,$2,$3,$4,$5,'recorded',$6) ON CONFLICT(reference) DO NOTHING").bind(uuid::Uuid::new_v4().to_string()).bind(referral).bind(format!("order:{}",p.order_id)).bind(commission).bind(&p.currency).bind(now()).execute(&mut **tx).await?;
    }
    let fulfilled = lines.iter().all(|l| {
        matches!(
            l.get::<String, _>("kind").as_str(),
            "digital" | "membership"
        )
    });
    sqlx::query("UPDATE shop_orders SET payment_state='paid',paid_minor=$1,payment_ref=$2,fulfillment=$3,version=version+1 WHERE id=$4").bind(p.amount_minor).bind(&p.reference).bind(if fulfilled{"fulfilled"}else{"unfulfilled"}).bind(&p.order_id).execute(&mut **tx).await?;
    if lines
        .iter()
        .all(|l| l.get::<String, _>("slot_id").is_empty())
    {
        sqlx::query("INSERT INTO shop_notifications(id,order_id,kind,due_at,created_at) VALUES($1,$2,'confirmation',$3,$3) ON CONFLICT(order_id,kind,slot_id) DO NOTHING").bind(uuid::Uuid::new_v4().to_string()).bind(&p.order_id).bind(now()).execute(&mut **tx).await?;
    }
    audit(
        tx,
        &p.order_id,
        &p.provider,
        "payment_confirmed",
        p.amount_minor,
    )
    .await?;
    Ok(())
}
pub async fn cancel(app: &App, s: &Session, id: &str, version: i64) -> Result<()> {
    let _guard = app.mutation().await?;
    customer(app, s).await?;
    let mut tx = app.db.pool.begin().await?;
    // Lock before reading the decision snapshot: SQLite cannot promote a
    // deferred read after an independent writer commits; PostgreSQL must not
    // decide cancellation from a row another payment transaction can change.
    let order = sqlx::query("UPDATE shop_orders SET version=version WHERE id=$1 RETURNING *")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(Error::not_found)?;
    let admin = crate::membership::staff(app, s).await.is_ok();
    if !admin && order.get::<String, _>("user_id") != s.user.id {
        return Err(Error::not_found());
    }
    if order.get::<i64, _>("version") != version {
        return Err(Error::conflict());
    }
    let state: String = order.get("payment_state");
    if state == "awaiting" {
        cancel_tx(&mut tx, &order, "cancelled").await?;
    } else if ["paid", "partially_refunded"].contains(&state.as_str())
        && order.get::<String, _>("fulfillment") != "cancel_requested"
    {
        sqlx::query(
            "UPDATE shop_orders SET fulfillment='cancel_requested',version=version+1 WHERE id=$1",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
        audit(&mut tx, id, &s.user.id, "cancellation_requested", 0).await?;
    } else {
        return Err(Error::invalid(
            "This order cannot be cancelled in its current state.",
        ));
    }
    tx.commit().await?;
    Ok(())
}
pub async fn fulfill(app: &App, s: &Session, id: &str, version: i64) -> Result<()> {
    let _guard = app.mutation().await?;
    owner(app, s).await?;
    let mut tx = app.db.pool.begin().await?;
    if sqlx::query("UPDATE shop_orders SET fulfillment='fulfilled',version=version+1 WHERE id=$1 AND version=$2 AND payment_state IN ('paid','partially_refunded') AND fulfillment='unfulfilled'").bind(id).bind(version).execute(&mut *tx).await?.rows_affected()!=1{return Err(Error::conflict())}
    audit(&mut tx, id, &s.user.id, "fulfilled", 0).await?;
    tx.commit().await?;
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
pub struct RefundInput {
    pub request_key: String,
    pub amount_minor: i64,
    pub reason: String,
    pub restock: bool,
}
pub async fn refund_request(
    app: &App,
    s: &Session,
    order: &str,
    version: i64,
    input: &RefundInput,
) -> Result<String> {
    crate::membership::uuid(&input.request_key)?;
    text(&input.reason, 500)?;
    money(input.amount_minor)?;
    if input.amount_minor == 0 {
        return Err(Error::invalid("Refund amount must be positive."));
    }
    let _guard = app.mutation().await?;
    owner(app, s).await?;
    let mut tx = app.db.pool.begin().await?;
    if let Some(old)=sqlx::query("SELECT id,amount_minor,restock,reason FROM shop_refunds WHERE order_id=$1 AND request_key=$2").bind(order).bind(&input.request_key).fetch_optional(&mut *tx).await?{if old.get::<i64,_>("amount_minor")!=input.amount_minor||old.get::<i64,_>("restock")!=i64::from(input.restock)||old.get::<String,_>("reason")!=input.reason{return Err(Error::conflict())}return Ok(old.get("id"))}
    let row = sqlx::query("SELECT * FROM shop_orders WHERE id=$1")
        .bind(order)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(Error::not_found)?;
    if row.get::<i64, _>("version") != version
        || !["paid", "partially_refunded", "needs_refund"]
            .contains(&row.get::<String, _>("payment_state").as_str())
    {
        return Err(Error::conflict());
    }
    let pending:i64=sqlx::query_scalar("SELECT CAST(COALESCE(SUM(amount_minor),0) AS BIGINT) FROM shop_refunds WHERE order_id=$1 AND state='pending'").bind(order).fetch_one(&mut *tx).await?;
    let remaining = row.get::<i64, _>("paid_minor") - row.get::<i64, _>("refunded_minor") - pending;
    if input.amount_minor > remaining
        || input.restock && input.amount_minor != remaining
        || input.restock && pending > 0
    {
        return Err(Error::invalid(
            "Refund exceeds the available paid balance; restocking requires the final full-order refund with no outstanding refund.",
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO shop_refunds(id,order_id,request_key,amount_minor,reason,restock,state,created_at) VALUES($1,$2,$3,$4,$5,$6,'pending',$7)").bind(&id).bind(order).bind(&input.request_key).bind(input.amount_minor).bind(&input.reason).bind(i64::from(input.restock)).bind(now()).execute(&mut *tx).await?;
    audit(
        &mut tx,
        order,
        &s.user.id,
        "refund_requested",
        input.amount_minor,
    )
    .await?;
    sqlx::query("UPDATE shop_orders SET version=version+1 WHERE id=$1")
        .bind(order)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(id)
}
pub async fn confirm_refund(app: &App, id: &str, reference: &str, amount: i64) -> Result<()> {
    confirm_refund_as(app, id, reference, amount, None).await
}
pub async fn record_offline_refund(
    app: &App,
    s: &Session,
    id: &str,
    reference: &str,
    amount: i64,
) -> Result<()> {
    confirm_refund_as(app, id, reference, amount, Some(s)).await
}
async fn confirm_refund_as(
    app: &App,
    id: &str,
    reference: &str,
    amount: i64,
    staff: Option<&Session>,
) -> Result<()> {
    text(reference, 200)?;
    let _guard = app.mutation().await?;
    if let Some(s) = staff {
        owner(app, s).await?;
    }
    if !app.config.commerce.enabled {
        return Err(Error::forbidden());
    }
    let mut tx = app.db.pool.begin().await?;
    let refund = sqlx::query("SELECT * FROM shop_refunds WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(Error::not_found)?;
    if refund.get::<i64, _>("amount_minor") != amount {
        return Err(Error::invalid(
            "Provider refund amount differs from the authorized refund.",
        ));
    }
    if refund.get::<String, _>("state") == "confirmed" {
        if refund.get::<String, _>("provider_ref") != reference {
            return Err(Error::conflict());
        }
        return Ok(());
    }
    if refund.get::<String, _>("state") != "pending" {
        return Err(Error::conflict());
    }
    let order_id: String = refund.get("order_id");
    let order = sqlx::query("SELECT * FROM shop_orders WHERE id=$1")
        .bind(&order_id)
        .fetch_one(&mut *tx)
        .await?;
    if let Some(s) = staff {
        owner(app, s).await?;
        if order.get::<String, _>("provider") != "offline" {
            return Err(Error::forbidden());
        }
    }
    let cumulative = order.get::<i64, _>("refunded_minor") + amount;
    let paid: i64 = order.get("paid_minor");
    if cumulative > paid {
        return Err(Error::invalid("Refund exceeds paid amount."));
    }
    let full = cumulative == paid;
    sqlx::query(
        "UPDATE shop_refunds SET state='confirmed',provider_ref=$1 WHERE id=$2 AND state='pending'",
    )
    .bind(reference)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    if full {
        release_allocations(&mut tx, &order_id, refund.get::<i64, _>("restock") == 1).await?;
        sqlx::query("UPDATE member_grants SET revoked=1,version=version+1 WHERE id IN (SELECT grant_id FROM shop_order_lines WHERE order_id=$1 AND grant_id<>'')").bind(&order_id).execute(&mut *tx).await?;
        let subscription: String = order.get("subscription_id");
        if !subscription.is_empty() {
            let grant: Option<String> =
                sqlx::query_scalar("SELECT grant_id FROM shop_subscriptions WHERE id=$1")
                    .bind(&subscription)
                    .fetch_optional(&mut *tx)
                    .await?;
            if let Some(grant) = grant {
                let affected:i64=sqlx::query_scalar("SELECT COUNT(*) FROM shop_order_lines WHERE order_id=$1 AND grant_id=$2 AND grant_id<>''").bind(&order_id).bind(&grant).fetch_one(&mut *tx).await?;
                sqlx::query("UPDATE shop_subscriptions SET state=$1,provider_cancel_pending=CASE WHEN provider='stripe' AND provider_ref<>'' THEN 1 ELSE 0 END,provider_cancel_at=0,version=version+1 WHERE id=$2")
                    .bind(if affected == 1 {
                        "cancelled"
                    } else {
                        "cancel_at_end"
                    })
                    .bind(&subscription)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        booking::cancel_messages(&mut tx, &order_id).await?;
    }
    let original = basis(
        order.get::<i64, _>("subtotal_minor") - order.get::<i64, _>("discount_minor"),
        order.get("commission_bps"),
    )?;
    let commission = if paid == 0 {
        0
    } else {
        i64::try_from(
            (i128::from(original) * i128::from(paid - cumulative) + i128::from(paid) / 2)
                / i128::from(paid),
        )
        .map_err(|_| Error::invalid("Commission overflow."))?
    };
    sqlx::query("UPDATE member_commissions SET amount_minor=$1,state=CASE WHEN $2=1 THEN 'void' ELSE state END WHERE reference=$3").bind(commission).bind(i64::from(full)).bind(format!("order:{order_id}")).execute(&mut *tx).await?;
    sqlx::query("UPDATE shop_orders SET refunded_minor=$1,payment_state=$2,fulfillment=CASE WHEN $3=1 THEN 'cancelled' ELSE fulfillment END,version=version+1 WHERE id=$4").bind(cumulative).bind(if full{"refunded"}else if order.get::<String,_>("payment_state")=="needs_refund"{"needs_refund"}else{"partially_refunded"}).bind(i64::from(full)).bind(&order_id).execute(&mut *tx).await?;
    audit(&mut tx, &order_id, "payment", "refund_confirmed", amount).await?;
    tx.commit().await?;
    tracing::info!(event="commerce_refund_confirmed",order_id=%order_id,amount_minor=amount);
    Ok(())
}
pub async fn record_payout(
    app: &App,
    s: &Session,
    user: &str,
    amount: i64,
    currency: &str,
    reference: &str,
) -> Result<String> {
    money(amount)?;
    text(reference, 160)?;
    crate::membership::uuid(user)?;
    if amount == 0 || currency != app.config.commerce.currency {
        return Err(Error::invalid("Check payout amount and store currency."));
    }
    let _guard = app.mutation().await?;
    owner(app, s).await?;
    let mut tx = app.db.pool.begin().await?;
    if let Some(old) =
        sqlx::query("SELECT id,user_id,amount_minor,currency FROM shop_payouts WHERE reference=$1")
            .bind(reference)
            .fetch_optional(&mut *tx)
            .await?
    {
        if old.get::<String, _>("user_id") != user
            || old.get::<i64, _>("amount_minor") != amount
            || old.get::<String, _>("currency") != currency
        {
            return Err(Error::conflict());
        }
        return Ok(old.get("id"));
    }
    let earned:i64=sqlx::query_scalar("SELECT CAST(COALESCE(SUM(c.amount_minor),0) AS BIGINT) FROM member_commissions c JOIN member_referrals r ON r.id=c.referral_id WHERE r.user_id=$1 AND c.currency=$2 AND c.state='recorded'").bind(user).bind(currency).fetch_one(&mut *tx).await?;
    let paid: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(amount_minor),0) AS BIGINT) FROM shop_payouts WHERE user_id=$1 AND currency=$2",
    )
    .bind(user)
    .bind(currency)
    .fetch_one(&mut *tx)
    .await?;
    if amount > earned - paid {
        return Err(Error::invalid(
            "Payout exceeds the outstanding commission balance.",
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO shop_payouts(id,user_id,amount_minor,currency,reference,created_at) VALUES($1,$2,$3,$4,$5,$6)").bind(&id).bind(user).bind(amount).bind(currency).bind(reference).bind(now()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(id)
}
