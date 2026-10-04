//! Validate financial allocations and shared access before any recovery file/database writes.
use super::*;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
type Record = BTreeMap<String, Value>;
type Tables = BTreeMap<String, Vec<Record>>;
fn s<'a>(r: &'a Record, k: &str) -> &'a str {
    r[k].as_str().unwrap()
}
fn n(r: &Record, k: &str) -> i64 {
    r[k].as_i64().unwrap()
}
fn bad() -> Error {
    Error::invalid("Backup contains an invalid commerce or financial graph.")
}
fn index<'a>(t: &'a Tables, table: &str, key: &str) -> Result<BTreeMap<&'a str, &'a Record>> {
    let mut m = BTreeMap::new();
    for r in &t[table] {
        if m.insert(s(r, key), r).is_some() {
            return Err(bad());
        }
    }
    Ok(m)
}
fn flag(r: &Record, k: &str) -> Result<()> {
    if !(0..=1).contains(&n(r, k)) {
        return Err(bad());
    }
    Ok(())
}
pub fn validate(t: &Tables, c: &Config) -> Result<()> {
    let records: usize = budget::TABLES.iter().map(|name| t[*name].len()).sum();
    if records as i64 > c.max_records {
        return Err(bad());
    }
    for table in budget::TABLES {
        for r in &t[*table] {
            if r.contains_key("id") && *table != "shop_provider_events" {
                crate::membership::uuid(s(r, "id"))?;
            }
            if r.contains_key("version") && n(r, "version") < 1
                || r.contains_key("created_at") && n(r, "created_at") < 0
            {
                return Err(bad());
            }
        }
    }
    let users = index(t, "users", "id")?;
    let products = index(t, "shop_products", "id")?;
    let variants = index(t, "shop_variants", "id")?;
    let orders = index(t, "shop_orders", "id")?;
    let subs = index(t, "shop_subscriptions", "id")?;
    let slots = index(t, "shop_slots", "id")?;
    let resources = index(t, "shop_resources", "id")?;
    let grants = index(t, "member_grants", "id")?;
    let media = index(t, "media", "id")?;
    let discounts = index(t, "shop_discounts", "code")?;
    let referrals = index(t, "member_referrals", "id")?;
    let _claims = index(t, "promotion_claims", "code")?;
    if t["shop_settings"].len() != 1 {
        return Err(bad());
    }
    let settings = &t["shop_settings"][0];
    if n(settings, "id") != 1 || s(settings, "currency") != c.currency || n(settings, "version") < 1
    {
        return Err(bad());
    }
    basis(0, n(settings, "tax_bps"))?;
    money(n(settings, "shipping_minor"))?;
    flag(settings, "tax_shipping")?;
    for p in products.values() {
        catalog::product_valid(&catalog::ProductInput {
            slug: s(p, "slug").into(),
            title: s(p, "title").into(),
            description: s(p, "description").into(),
            kind: s(p, "kind").into(),
            entitlement: s(p, "entitlement").into(),
            access_seconds: n(p, "access_seconds"),
            download_id: s(p, "download_id").into(),
            published: n(p, "published") == 1,
        })?;
        flag(p, "published")?;
        let download = s(p, "download_id");
        if !download.is_empty() {
            let m = media.get(download).ok_or_else(bad)?;
            if s(m, "visibility") != "private" {
                return Err(bad());
            }
            let protected = t["member_resources"]
                .iter()
                .find(|r| s(r, "kind") == "media" && s(r, "resource_id") == download)
                .ok_or_else(bad)?;
            let policy = t["member_policies"]
                .iter()
                .find(|r| s(r, "id") == s(protected, "policy_id"))
                .ok_or_else(bad)?;
            if s(policy, "entitlement") != s(p, "entitlement") || !s(policy, "group_id").is_empty()
            {
                return Err(bad());
            }
        }
    }
    let mut skus = HashSet::new();
    for v in variants.values() {
        let p = products.get(s(v, "product_id")).ok_or_else(bad)?;
        money(n(v, "price_minor"))?;
        text(s(v, "title"), 120)?;
        text(s(v, "sku"), 80)?;
        catalog::key(s(v, "member_key"))?;
        flag(v, "active")?;
        let member = n(v, "member_price_minor");
        if !skus.insert(s(v, "sku"))
            || member < -1
            || member > n(v, "price_minor")
            || member >= 0 && s(v, "member_key").is_empty()
            || n(v, "held") < 0
            || n(v, "sold") < 0
            || n(v, "stock_total") < -1
            || n(v, "stock_total") >= 0
                && i128::from(n(v, "held")) + i128::from(n(v, "sold"))
                    > i128::from(n(v, "stock_total"))
            || s(p, "kind") != "physical" && n(v, "stock_total") != -1
        {
            return Err(bad());
        }
        let interval = s(v, "billing_interval");
        if !interval.is_empty()
            && (s(p, "kind") != "membership"
                || !matches!(interval, "day" | "week" | "month" | "year")
                || member != -1
                || n(v, "price_minor") == 0)
        {
            return Err(bad());
        }
    }
    for r in resources.values() {
        text(s(r, "title"), 160)?;
        flag(r, "active")?;
        if !s(r, "staff_id").is_empty() && !users.contains_key(s(r, "staff_id")) {
            return Err(bad());
        }
    }
    let mut calendar: BTreeMap<&str, Vec<(i64, i64)>> = BTreeMap::new();
    for slot in slots.values() {
        let resource = resources.get(s(slot, "resource_id")).ok_or_else(bad)?;
        let variant = variants.get(s(slot, "variant_id")).ok_or_else(bad)?;
        let product = products[s(variant, "product_id")];
        flag(slot, "active")?;
        if s(product, "kind") != "booking"
            || !(1..=1000).contains(&n(slot, "capacity"))
            || n(slot, "starts_at") < 0
            || n(slot, "ends_at") <= n(slot, "starts_at")
            || n(slot, "ends_at") - n(slot, "starts_at") > 86400
            || n(slot, "held") < 0
            || n(slot, "booked") < 0
            || n(slot, "held") + n(slot, "booked") > n(slot, "capacity")
        {
            return Err(bad());
        }
        if n(slot, "active") == 1 || n(slot, "held") + n(slot, "booked") > 0 {
            calendar
                .entry(s(slot, "resource_id"))
                .or_default()
                .push((n(slot, "starts_at"), n(slot, "ends_at")));
            if !s(resource, "staff_id").is_empty() {
                calendar
                    .entry(s(resource, "staff_id"))
                    .or_default()
                    .push((n(slot, "starts_at"), n(slot, "ends_at")));
            }
        }
    }
    for spans in calendar.values_mut() {
        spans.sort_unstable();
        if spans.windows(2).any(|p| p[0].1 > p[1].0) {
            return Err(bad());
        }
    }
    let mut cart_pairs = HashSet::new();
    let carts = index(t, "shop_carts", "user_id")?;
    let mut cart_count: BTreeMap<&str, i64> = BTreeMap::new();
    for cart in carts.values() {
        if !users.contains_key(s(cart, "user_id")) || n(cart, "updated_at") < 0 {
            return Err(bad());
        }
    }
    for l in &t["shop_cart_lines"] {
        if !carts.contains_key(s(l, "user_id"))
            || !variants.contains_key(s(l, "variant_id"))
            || !cart_pairs.insert((s(l, "user_id"), s(l, "variant_id"), s(l, "slot_id")))
            || !(1..=100).contains(&n(l, "quantity"))
        {
            return Err(bad());
        }
        let v = variants[s(l, "variant_id")];
        let p = products[s(v, "product_id")];
        if s(p, "kind") == "booking" {
            let slot = slots.get(s(l, "slot_id")).ok_or_else(bad)?;
            if s(slot, "variant_id") != s(l, "variant_id") {
                return Err(bad());
            }
        } else if !s(l, "slot_id").is_empty() {
            return Err(bad());
        }
        if ["membership", "digital"].contains(&s(p, "kind")) && n(l, "quantity") != 1 {
            return Err(bad());
        }
        *cart_count.entry(s(l, "user_id")).or_default() += 1;
    }
    if cart_count.values().any(|v| *v > 20) {
        return Err(bad());
    }
    let mut requests = HashSet::new();
    let mut provider_orders = HashSet::new();
    for o in orders.values() {
        if !users.contains_key(s(o, "user_id"))
            || !requests.insert((s(o, "user_id"), s(o, "request_key")))
            || s(o, "request_digest").len() != 64
            || s(o, "currency") != c.currency
            || !matches!(s(o, "provider"), "offline" | "stripe")
            || !matches!(
                s(o, "payment_state"),
                "awaiting"
                    | "paid"
                    | "partially_refunded"
                    | "refunded"
                    | "cancelled"
                    | "failed"
                    | "needs_refund"
            )
            || !matches!(
                s(o, "fulfillment"),
                "unfulfilled" | "fulfilled" | "cancel_requested" | "cancelled"
            )
            || !matches!(s(o, "purpose"), "purchase" | "renewal" | "upgrade")
            || n(o, "expires_at") <= 0
            || n(o, "refunded_minor") > n(o, "paid_minor")
        {
            return Err(bad());
        }
        crate::membership::uuid(s(o, "request_key"))?;
        text(s(o, "customer_name"), 160)?;
        if s(o, "customer_email").len() > 254 || s(o, "shipping_address").len() > 2000 {
            return Err(bad());
        }
        for k in [
            "subtotal_minor",
            "discount_minor",
            "discount_base_minor",
            "shipping_minor",
            "tax_minor",
            "total_minor",
            "paid_minor",
            "refunded_minor",
        ] {
            money(n(o, k))?;
        }
        flag(o, "tax_shipping")?;
        if basis(n(o, "discount_base_minor"), n(o, "discount_bps"))? != n(o, "discount_minor")
            || n(o, "discount_base_minor") > n(o, "subtotal_minor")
        {
            return Err(bad());
        }
        let net = n(o, "subtotal_minor") - n(o, "discount_minor");
        if net < 0
            || basis(
                money(
                    net.checked_add(if n(o, "tax_shipping") == 1 {
                        n(o, "shipping_minor")
                    } else {
                        0
                    })
                    .ok_or_else(bad)?,
                )?,
                n(o, "tax_bps"),
            )? != n(o, "tax_minor")
            || i128::from(net) + i128::from(n(o, "shipping_minor")) + i128::from(n(o, "tax_minor"))
                != i128::from(n(o, "total_minor"))
        {
            return Err(bad());
        }
        if !s(o, "discount_code").is_empty() && !discounts.contains_key(s(o, "discount_code"))
            || !s(o, "referral_id").is_empty() && !referrals.contains_key(s(o, "referral_id"))
        {
            return Err(bad());
        }
        if !s(o, "provider_ref").is_empty() && !provider_orders.insert(s(o, "provider_ref")) {
            return Err(bad());
        }
        let sid = s(o, "subscription_id");
        if !sid.is_empty() {
            let sub = subs.get(sid).ok_or_else(bad)?;
            if s(sub, "user_id") != s(o, "user_id")
                || n(o, "period_start") < 0
                || n(o, "period_end") <= n(o, "period_start")
            {
                return Err(bad());
            }
        }
    }
    let lines = index(t, "shop_order_lines", "id")?;
    let mut totals: BTreeMap<&str, i128> = BTreeMap::new();
    let mut stock: BTreeMap<&str, (i64, i64)> = BTreeMap::new();
    let mut capacities: BTreeMap<&str, (i64, i64)> = BTreeMap::new();
    let mut line_count: BTreeMap<&str, i64> = BTreeMap::new();
    for l in lines.values() {
        let o = orders.get(s(l, "order_id")).ok_or_else(bad)?;
        let v = variants.get(s(l, "variant_id")).ok_or_else(bad)?;
        let p = products.get(s(l, "product_id")).ok_or_else(bad)?;
        text(s(l, "title"), 300)?;
        text(s(l, "sku"), 80)?;
        if s(v, "product_id") != s(l, "product_id")
            || s(p, "kind") != s(l, "kind")
            || !matches!(
                s(l, "allocation"),
                "held" | "sold" | "released" | "restocked"
            )
            || !(1..=100).contains(&n(l, "quantity"))
            || i128::from(n(l, "quantity")) * i128::from(n(l, "unit_minor"))
                != i128::from(n(l, "line_minor"))
            || n(l, "access_seconds") < 0
        {
            return Err(bad());
        }
        money(n(l, "unit_minor"))?;
        money(n(l, "line_minor"))?;
        *totals.entry(s(l, "order_id")).or_default() += i128::from(n(l, "line_minor"));
        *line_count.entry(s(l, "order_id")).or_default() += 1;
        let q = n(l, "quantity");
        let state = s(l, "allocation");
        if n(v, "stock_total") >= 0 {
            let counts = stock.entry(s(l, "variant_id")).or_default();
            if state == "held" {
                counts.0 += q;
            }
            if state == "sold" {
                counts.1 += q;
            }
        }
        if !s(l, "slot_id").is_empty() {
            let slot = slots.get(s(l, "slot_id")).ok_or_else(bad)?;
            if s(slot, "variant_id") != s(l, "variant_id") || s(l, "kind") != "booking" {
                return Err(bad());
            }
            let counts = capacities.entry(s(l, "slot_id")).or_default();
            if state == "held" {
                counts.0 += q;
            }
            if state == "sold" {
                counts.1 += q;
            }
        } else if s(l, "kind") == "booking" {
            return Err(bad());
        }
        if state == "held" && s(o, "payment_state") != "awaiting" {
            return Err(bad());
        }
        let grant = s(l, "grant_id");
        if !grant.is_empty() {
            let g = grants.get(grant).ok_or_else(bad)?;
            if s(g, "user_id") != s(o, "user_id")
                || s(g, "entitlement") != s(l, "entitlement")
                || s(g, "origin") != format!("order:{}", s(o, "id"))
                || !matches!(
                    s(o, "payment_state"),
                    "paid" | "partially_refunded" | "refunded"
                )
            {
                return Err(bad());
            }
            if !s(o, "subscription_id").is_empty()
                && (n(g, "starts_at") != n(o, "period_start")
                    || n(g, "expires_at") != n(o, "period_end"))
            {
                return Err(bad());
            }
            if s(o, "payment_state") == "refunded" && n(g, "revoked") != 1 {
                return Err(bad());
            }
        }
    }
    for (id, o) in &orders {
        if totals.get(id).copied() != Some(i128::from(n(o, "subtotal_minor")))
            || line_count[id] > 20
        {
            return Err(bad());
        }
    }
    for (id, v) in &variants {
        if stock.get(id).copied().unwrap_or_default() != (n(v, "held"), n(v, "sold")) {
            return Err(bad());
        }
    }
    for (id, slot) in &slots {
        if capacities.get(id).copied().unwrap_or_default() != (n(slot, "held"), n(slot, "booked")) {
            return Err(bad());
        }
    }
    let mut payment_orders = HashSet::new();
    let mut refs = HashSet::new();
    for p in &t["shop_payments"] {
        let o = orders.get(s(p, "order_id")).ok_or_else(bad)?;
        if !payment_orders.insert(s(p, "order_id"))
            || !refs.insert((s(p, "provider"), s(p, "reference")))
            || s(p, "provider") != s(o, "provider")
            || s(p, "currency") != s(o, "currency")
            || n(p, "amount_minor") != n(o, "paid_minor")
            || n(p, "amount_minor") != n(o, "total_minor")
            || s(p, "reference") != s(o, "payment_ref")
        {
            return Err(bad());
        }
    }
    for (id, o) in &orders {
        if matches!(
            s(o, "payment_state"),
            "paid" | "partially_refunded" | "refunded" | "needs_refund"
        ) != payment_orders.contains(id)
            || s(o, "payment_state") == "refunded" && n(o, "paid_minor") != n(o, "refunded_minor")
        {
            return Err(bad());
        }
    }
    let refunds = index(t, "shop_refunds", "id")?;
    let mut refundkeys = HashSet::new();
    let mut balances: BTreeMap<&str, (i128, i128)> = BTreeMap::new();
    for r in refunds.values() {
        if !orders.contains_key(s(r, "order_id"))
            || !refundkeys.insert((s(r, "order_id"), s(r, "request_key")))
            || !matches!(s(r, "state"), "pending" | "confirmed" | "failed")
            || n(r, "amount_minor") <= 0
        {
            return Err(bad());
        }
        flag(r, "restock")?;
        money(n(r, "amount_minor"))?;
        text(s(r, "reason"), 500)?;
        crate::membership::uuid(s(r, "request_key"))?;
        let b = balances.entry(s(r, "order_id")).or_default();
        if s(r, "state") == "confirmed" {
            text(s(r, "provider_ref"), 200)?;
            b.0 += i128::from(n(r, "amount_minor"));
        }
        if s(r, "state") == "pending" {
            b.1 += i128::from(n(r, "amount_minor"));
        }
    }
    for (id, o) in &orders {
        let b = balances.get(id).copied().unwrap_or_default();
        if b.0 != i128::from(n(o, "refunded_minor")) || b.0 + b.1 > i128::from(n(o, "paid_minor")) {
            return Err(bad());
        }
    }
    let mut subrefs = HashSet::new();
    for sub in subs.values() {
        let v = variants.get(s(sub, "variant_id")).ok_or_else(bad)?;
        if !users.contains_key(s(sub, "user_id"))
            || !matches!(
                s(sub, "state"),
                "pending" | "active" | "past_due" | "cancel_at_end" | "cancelled"
            )
            || !matches!(s(sub, "provider"), "offline" | "stripe")
            || s(sub, "billing_interval") != s(v, "billing_interval")
            || s(sub, "entitlement") != s(products[s(v, "product_id")], "entitlement")
            || n(sub, "period_start") < 0
            || n(sub, "period_end") <= n(sub, "period_start")
        {
            return Err(bad());
        }
        money(n(sub, "price_minor"))?;
        if !s(sub, "provider_ref").is_empty() && !subrefs.insert(s(sub, "provider_ref")) {
            return Err(bad());
        }
        if n(sub, "next_price_minor") < -1
            || !s(sub, "next_variant").is_empty() && n(sub, "next_price_minor") < 0
        {
            return Err(bad());
        }
        if !s(sub, "next_variant").is_empty() && !variants.contains_key(s(sub, "next_variant")) {
            return Err(bad());
        }
        if !s(sub, "grant_id").is_empty() {
            let g = grants.get(s(sub, "grant_id")).ok_or_else(bad)?;
            if s(g, "user_id") != s(sub, "user_id") || s(g, "entitlement") != s(sub, "entitlement")
            {
                return Err(bad());
            }
        }
    }
    let settled: HashSet<&str> = t["shop_history"]
        .iter()
        .filter(|h| s(h, "action") == "payment_confirmed")
        .map(|h| s(h, "order_id"))
        .collect();
    let mut couponcounts: BTreeMap<&str, (i64, i64)> = BTreeMap::new();
    for o in orders.values() {
        if !s(o, "discount_code").is_empty() {
            let counts = couponcounts.entry(s(o, "discount_code")).or_default();
            if s(o, "payment_state") == "awaiting" {
                counts.0 += 1;
            }
            if settled.contains(s(o, "id")) {
                counts.1 += 1;
            }
        }
    }
    for (code, d) in &discounts {
        if n(d, "bps") < 1
            || n(d, "bps") > 10000
            || n(d, "starts_at") < 0
            || n(d, "expires_at") <= n(d, "starts_at")
            || n(d, "max_uses") < 0
            || couponcounts.get(code).copied().unwrap_or_default() != (n(d, "held"), n(d, "used"))
            || n(d, "max_uses") > 0 && n(d, "held") + n(d, "used") > n(d, "max_uses")
        {
            return Err(bad());
        }
        flag(d, "active")?;
        catalog::key(s(d, "member_key"))?;
        text(s(d, "title"), 160)?;
        if !s(d, "product_id").is_empty() && !products.contains_key(s(d, "product_id")) {
            return Err(bad());
        }
    }
    let mut rewardorders = HashSet::new();
    let rewards = index(t, "shop_reward_redemptions", "claim_id")?;
    for r in rewards.values() {
        let o = orders.get(s(r, "order_id")).ok_or_else(bad)?;
        if s(r, "claim_id") != s(o, "reward_claim")
            || !rewardorders.insert(s(r, "order_id"))
            || !matches!(s(r, "state"), "held" | "used" | "released")
        {
            return Err(bad());
        }
        if s(r, "state") == "held" && s(o, "payment_state") != "awaiting" {
            return Err(bad());
        }
    }
    let messages = index(t, "mail_jobs", "id")?;
    let mut notifications = HashSet::new();
    for r in &t["shop_notifications"] {
        if !orders.contains_key(s(r, "order_id"))
            || !notifications.insert((s(r, "order_id"), s(r, "kind"), s(r, "slot_id")))
            || !matches!(
                s(r, "kind"),
                "confirmation" | "reminder" | "cancellation" | "billing"
            )
            || !matches!(s(r, "state"), "pending" | "queued" | "cancelled")
            || n(r, "due_at") < 0
            || !s(r, "slot_id").is_empty() && !slots.contains_key(s(r, "slot_id"))
        {
            return Err(bad());
        }
        if s(r, "state") == "queued" && !messages.contains_key(s(r, "message_id")) {
            return Err(bad());
        }
    }
    let _events = index(t, "shop_provider_events", "id")?;
    for e in &t["shop_provider_events"] {
        if n(e, "attempts") < 0
            || n(e, "next_at") < 0
            || s(e, "body").len() > 256 * 1024
            || crate::auth::digest(s(e, "body").as_bytes()) != s(e, "digest")
            || !matches!(s(e, "state"), "pending" | "processed" | "ignored")
        {
            return Err(bad());
        }
        let v: Value = serde_json::from_str(s(e, "body")).map_err(|_| bad())?;
        if v.get("id").and_then(Value::as_str) != Some(s(e, "id"))
            || v.get("api_version").and_then(Value::as_str) != Some(payments::API_VERSION)
        {
            return Err(bad());
        }
    }
    let mut payouts = HashSet::new();
    for p in &t["shop_payouts"] {
        if !users.contains_key(s(p, "user_id"))
            || s(p, "currency") != c.currency
            || n(p, "amount_minor") <= 0
            || !payouts.insert(s(p, "reference"))
        {
            return Err(bad());
        }
        money(n(p, "amount_minor"))?;
        text(s(p, "reference"), 160)?;
    }
    for h in &t["shop_history"] {
        if !orders.contains_key(s(h, "order_id")) {
            return Err(bad());
        }
        text(s(h, "actor"), 160)?;
        text(s(h, "action"), 80)?;
        money(n(h, "amount_minor"))?;
    }
    Ok(())
}
