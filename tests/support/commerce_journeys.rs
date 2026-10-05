//! M6 readable financial and reservation journeys on the real supported database engines.
use super::*;
use wpalt::commerce::{billing, booking, catalog, orders};
pub(super) async fn shopper(site: &Site, email: &str) -> (String, Session) {
    auth::add_user(&site.app, email, "Shopper", "subscriber", PASSWORD)
        .await
        .unwrap();
    auth::login(&site.app, email, PASSWORD).await.unwrap()
}
pub(super) async fn product(
    site: &Site,
    kind: &str,
    price: i64,
    stock: i64,
    interval: &str,
) -> (String, String) {
    let slug = format!("item-{}", uuid::Uuid::new_v4().simple());
    let p = catalog::save_product(
        &site.app,
        site.session(),
        None,
        0,
        &catalog::ProductInput {
            slug: slug.clone(),
            title: format!("Test {kind}"),
            description: "Clear local purchase terms.".into(),
            kind: kind.into(),
            entitlement: if kind == "membership" {
                "academy".into()
            } else {
                String::new()
            },
            access_seconds: 0,
            download_id: String::new(),
            published: true,
        },
    )
    .await
    .unwrap();
    let v = catalog::save_variant(
        &site.app,
        site.session(),
        &p,
        None,
        0,
        &catalog::VariantInput {
            title: "Standard".into(),
            sku: slug,
            price_minor: price,
            member_price_minor: -1,
            member_key: String::new(),
            stock_total: stock,
            billing_interval: interval.into(),
            active: true,
        },
    )
    .await
    .unwrap();
    (p, v)
}
pub(super) async fn cart(
    site: &Site,
    s: &Session,
    variant: &str,
    slot: &str,
    quantity: i64,
) -> orders::Checkout {
    let version = orders::cart_version(&site.app, &s.user.id).await.unwrap();
    orders::set_cart(&site.app, s, version, variant, slot, quantity)
        .await
        .unwrap();
    let q = orders::quote(&site.app, s, "").await.unwrap();
    orders::Checkout {
        request_key: uuid::Uuid::new_v4().to_string(),
        cart_version: q.cart_version,
        quote_hash: q.hash,
        shipping_address: "Synthetic address".into(),
        provider: "offline".into(),
        ..Default::default()
    }
}
async fn order_version(site: &Site, id: &str) -> i64 {
    sqlx::query_scalar("SELECT version FROM shop_orders WHERE id=$1")
        .bind(id)
        .fetch_one(&site.app.db.pool)
        .await
        .unwrap()
}
async fn state(site: &Site, id: &str) -> String {
    sqlx::query_scalar("SELECT payment_state FROM shop_orders WHERE id=$1")
        .bind(id)
        .fetch_one(&site.app.db.pool)
        .await
        .unwrap()
}
pub(super) async fn pay(site: &Site, id: &str, reference: &str) {
    orders::record_offline(
        &site.app,
        site.session(),
        id,
        order_version(site, id).await,
        reference,
    )
    .await
    .unwrap()
}
#[tokio::test]
async fn commerce_checkout_snapshots_prices_refunds_stock_and_preserves_financial_graph() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let (token, buyer) = shopper(&site, "buyer@example.test").await;
        let (other, _) = shopper(&site, "outsider@example.test").await;
        let (_, variant) = product(&site, "physical", 1001, 3, "").await;
        catalog::set_rules(&site.app, site.session(), 1, 750, 200, true)
            .await
            .unwrap();
        let input = cart(&site, &buyer, &variant, "", 2).await;
        let quote = orders::quote(&site.app, &buyer, "").await.unwrap();
        assert_eq!(
            (
                quote.subtotal_minor,
                quote.tax_minor,
                quote.shipping_minor,
                quote.total_minor
            ),
            (2002, 165, 200, 2367)
        );
        let id = orders::checkout(&site.app, &buyer, &input).await.unwrap();
        assert_eq!(
            orders::checkout(&site.app, &buyer, &input).await.unwrap(),
            id
        );
        let mut changed = input.clone();
        changed.shipping_address = "Changed".into();
        assert!(orders::checkout(&site.app, &buyer, &changed).await.is_err());
        assert_eq!(state(&site, &id).await, "awaiting");
        assert!(
            orders::record_offline(&site.app, &buyer, &id, 1, "forged")
                .await
                .is_err()
        );
        assert_eq!(
            get(&site.app, &format!("/shop/orders/{id}"), Some(&other))
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        let (status, headers, body) = request(
            &site.app,
            "GET",
            &format!("/shop/orders/{id}"),
            Some(&token),
            "",
            vec![],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers["cache-control"], "no-store");
        assert!(String::from_utf8(body).unwrap().contains("USD 23.67")); // UI uses human currency, not raw minor units.
        pay(&site, &id, "actual-bank-1").await;
        assert_eq!(state(&site, &id).await, "paid");
        catalog::set_rules(&site.app, site.session(), 2, 0, 0, false)
            .await
            .unwrap();
        let old: i64 = sqlx::query_scalar("SELECT total_minor FROM shop_orders WHERE id=$1")
            .bind(&id)
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(old, 2367);
        let request = orders::RefundInput {
            request_key: uuid::Uuid::new_v4().to_string(),
            amount_minor: 500,
            reason: "Partial adjustment".into(),
            restock: false,
        };
        let refund = orders::refund_request(
            &site.app,
            site.session(),
            &id,
            order_version(&site, &id).await,
            &request,
        )
        .await
        .unwrap();
        assert_eq!(
            orders::refund_request(&site.app, site.session(), &id, 0, &request)
                .await
                .unwrap(),
            refund
        );
        orders::record_offline_refund(&site.app, site.session(), &refund, "actual-refund-1", 500)
            .await
            .unwrap();
        orders::record_offline_refund(&site.app, site.session(), &refund, "actual-refund-1", 500)
            .await
            .unwrap();
        assert_eq!(state(&site, &id).await, "partially_refunded");
        let excessive = orders::RefundInput {
            request_key: uuid::Uuid::new_v4().to_string(),
            amount_minor: 2000,
            ..request.clone()
        };
        assert!(
            orders::refund_request(
                &site.app,
                site.session(),
                &id,
                order_version(&site, &id).await,
                &excessive
            )
            .await
            .is_err()
        );
        let final_refund = orders::refund_request(
            &site.app,
            site.session(),
            &id,
            order_version(&site, &id).await,
            &orders::RefundInput {
                request_key: uuid::Uuid::new_v4().to_string(),
                amount_minor: 1867,
                reason: "Final returned purchase".into(),
                restock: true,
            },
        )
        .await
        .unwrap();
        orders::record_offline_refund(
            &site.app,
            site.session(),
            &final_refund,
            "actual-refund-2",
            1867,
        )
        .await
        .unwrap();
        assert_eq!(state(&site, &id).await, "refunded");
        let v = sqlx::query("SELECT held,sold FROM shop_variants WHERE id=$1")
            .bind(&variant)
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!((v.get::<i64, _>("held"), v.get::<i64, _>("sold")), (0, 0));
        let encoded = backup::capture(&site.app).await.unwrap();
        let restored = Site::new(pg, false).await;
        backup::restore(&restored.app, &encoded).await.unwrap();
        assert_eq!(state(&restored, &id).await, "refunded");
        assert!(
            get(&restored.app, &format!("/shop/orders/{id}"), Some(&token))
                .await
                .0
                != StatusCode::OK
        );
        restored.close().await;
        site.close().await;
    }
}
#[tokio::test]
async fn commerce_last_unit_and_last_slot_have_one_winner_and_expiring_holds_are_reusable() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let (_, a) = shopper(&site, "a@example.test").await;
        let (_, b) = shopper(&site, "b@example.test").await;
        let (_, v) = product(&site, "physical", 900, 1, "").await;
        let ia = cart(&site, &a, &v, "", 1).await;
        let ib = cart(&site, &b, &v, "", 1).await;
        let (ra, rb) = tokio::join!(
            orders::checkout(&site.app, &a, &ia),
            orders::checkout(&site.app, &b, &ib)
        );
        assert_eq!(usize::from(ra.is_ok()) + usize::from(rb.is_ok()), 1);
        let id = ra.or(rb).unwrap();
        sqlx::query("UPDATE shop_orders SET expires_at=$1 WHERE id=$2")
            .bind(wpalt::now() - 1)
            .bind(&id)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(orders::expire(&site.app).await.unwrap(), 1);
        assert_eq!(orders::expire(&site.app).await.unwrap(), 0);
        let (_, v) = product(&site, "booking", 1200, -1, "").await;
        let resource = booking::resource(
            &site.app,
            site.session(),
            "Consultation room",
            &site.session().user.id,
        )
        .await
        .unwrap();
        let slot_input = booking::SlotInput {
            resource_id: resource.clone(),
            variant_id: v.clone(),
            starts_at: wpalt::now() + 3 * 86400,
            ends_at: wpalt::now() + 3 * 86400 + 3600,
            capacity: 1,
        };
        let slot = booking::slot(&site.app, site.session(), &slot_input)
            .await
            .unwrap();
        assert!(
            booking::slot(&site.app, site.session(), &slot_input)
                .await
                .is_err()
        );
        let resource2 = booking::resource(
            &site.app,
            site.session(),
            "Another room",
            &site.session().user.id,
        )
        .await
        .unwrap();
        assert!(
            booking::slot(
                &site.app,
                site.session(),
                &booking::SlotInput {
                    resource_id: resource2,
                    ..slot_input
                }
            )
            .await
            .is_err()
        ); // Same assigned staff cannot be double booked.
        // Remove the previous loser's stale physical cart before choosing the booking.
        for s in [&a, &b] {
            let ver = orders::cart_version(&site.app, &s.user.id).await.unwrap();
            let old: Vec<String> =
                sqlx::query_scalar("SELECT variant_id FROM shop_cart_lines WHERE user_id=$1")
                    .bind(&s.user.id)
                    .fetch_all(&site.app.db.pool)
                    .await
                    .unwrap();
            if let Some(v) = old.first() {
                orders::set_cart(&site.app, s, ver, v, "", 0).await.unwrap();
            }
        }
        let ia = cart(&site, &a, &v, &slot, 1).await;
        let ib = cart(&site, &b, &v, &slot, 1).await;
        let (ra, rb) = tokio::join!(
            orders::checkout(&site.app, &a, &ia),
            orders::checkout(&site.app, &b, &ib)
        );
        assert_eq!(usize::from(ra.is_ok()) + usize::from(rb.is_ok()), 1);
        let id = ra.or(rb).unwrap();
        pay(&site, &id, "booking-bank").await;
        let notifications: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM shop_notifications WHERE order_id=$1")
                .bind(&id)
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        assert_eq!(notifications, 2);
        let r = orders::refund_request(
            &site.app,
            site.session(),
            &id,
            order_version(&site, &id).await,
            &orders::RefundInput {
                request_key: uuid::Uuid::new_v4().to_string(),
                amount_minor: 1200,
                reason: "Reservation cancelled".into(),
                restock: false,
            },
        )
        .await
        .unwrap();
        orders::record_offline_refund(&site.app, site.session(), &r, "booking-return", 1200)
            .await
            .unwrap();
        let booked: i64 = sqlx::query_scalar("SELECT booked FROM shop_slots WHERE id=$1")
            .bind(slot)
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(booked, 0);
        let n:i64=sqlx::query_scalar("SELECT COUNT(*) FROM shop_notifications WHERE order_id=$1 AND kind IN ('confirmation','reminder') AND state='cancelled'").bind(id).fetch_one(&site.app.db.pool).await.unwrap();
        assert_eq!(n, 2);
        site.close().await;
    }
}
#[tokio::test]
async fn commerce_recurring_access_needs_payment_and_refunds_revoke_only_the_purchase() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let (_, buyer) = shopper(&site, "subscriber@example.test").await;
        let (p, v) = product(&site, "membership", 1000, -1, "month").await;
        let i = cart(&site, &buyer, &v, "", 1).await;
        let id = orders::checkout(&site.app, &buyer, &i).await.unwrap();
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM member_grants WHERE user_id=$1")
            .bind(&buyer.user.id)
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(before, 0);
        pay(&site, &id, "membership-payment").await;
        let sub = sqlx::query("SELECT * FROM shop_subscriptions WHERE user_id=$1")
            .bind(&buyer.user.id)
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        let sid: String = sub.get("id");
        assert_eq!(sub.get::<String, _>("state"), "active");
        let higher = catalog::save_variant(
            &site.app,
            site.session(),
            &p,
            None,
            0,
            &catalog::VariantInput {
                title: "Plus".into(),
                sku: "PLAN-PLUS".into(),
                price_minor: 2000,
                member_price_minor: -1,
                member_key: String::new(),
                stock_total: -1,
                billing_interval: "month".into(),
                active: true,
            },
        )
        .await
        .unwrap();
        let upgrade = billing::change(&site.app, &buyer, &sid, sub.get("version"), &higher)
            .await
            .unwrap()
            .unwrap();
        let amount: i64 = sqlx::query_scalar("SELECT subtotal_minor FROM shop_orders WHERE id=$1")
            .bind(&upgrade)
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert!((999..=1000).contains(&amount));
        pay(&site, &upgrade, "upgrade-payment").await;
        let sub = sqlx::query("SELECT * FROM shop_subscriptions WHERE id=$1")
            .bind(&sid)
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(sub.get::<String, _>("variant_id"), higher);
        assert_eq!(sub.get::<i64, _>("price_minor"), 2000);
        let independent = wpalt::membership::grant(
            &site.app,
            &buyer.user.id,
            "academy",
            wpalt::now() - 1,
            0,
            "independent-owner-grant",
        )
        .await
        .unwrap();
        let r = orders::refund_request(
            &site.app,
            site.session(),
            &upgrade,
            order_version(&site, &upgrade).await,
            &orders::RefundInput {
                request_key: uuid::Uuid::new_v4().to_string(),
                amount_minor: amount,
                reason: "Upgrade reverted".into(),
                restock: false,
            },
        )
        .await
        .unwrap();
        orders::record_offline_refund(&site.app, site.session(), &r, "upgrade-refund", amount)
            .await
            .unwrap();
        let revoked: i64 = sqlx::query_scalar("SELECT revoked FROM member_grants WHERE id=$1")
            .bind(independent)
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(revoked, 0);
        let original_revoked:i64=sqlx::query_scalar("SELECT g.revoked FROM member_grants g JOIN shop_order_lines l ON l.grant_id=g.id WHERE l.order_id=$1").bind(id).fetch_one(&site.app.db.pool).await.unwrap();
        assert_eq!(original_revoked, 0);
        let recovered = Site::new(pg, false).await;
        backup::restore(&recovered.app, &backup::capture(&site.app).await.unwrap())
            .await
            .unwrap();
        recovered.close().await;
        site.close().await;
    }
}

#[tokio::test]
async fn commerce_renewal_dunning_and_downgrade_keep_agreed_prices_and_require_real_collection() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let (_, buyer) = shopper(&site, "renewing@example.test").await;
        let (p, v) = product(&site, "membership", 2000, -1, "month").await;
        let i = cart(&site, &buyer, &v, "", 1).await;
        let id = orders::checkout(&site.app, &buyer, &i).await.unwrap();
        pay(&site, &id, "initial-subscription").await;
        let sub = sqlx::query("SELECT * FROM shop_subscriptions WHERE user_id=$1")
            .bind(&buyer.user.id)
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        let sid: String = sub.get("id");
        let lower = catalog::save_variant(
            &site.app,
            site.session(),
            &p,
            None,
            0,
            &catalog::VariantInput {
                title: "Basic".into(),
                sku: "PLAN-BASIC".into(),
                price_minor: 1000,
                member_price_minor: -1,
                member_key: String::new(),
                stock_total: -1,
                billing_interval: "month".into(),
                active: true,
            },
        )
        .await
        .unwrap();
        assert!(
            billing::change(&site.app, &buyer, &sid, sub.get("version"), &lower)
                .await
                .unwrap()
                .is_none()
        );
        // Moving the synthetic period to its boundary makes time-based behavior deterministic.
        let at = wpalt::now();
        sqlx::query("UPDATE shop_subscriptions SET period_start=$1,period_end=$2 WHERE id=$3")
            .bind(at - 30 * 86400)
            .bind(at - 1)
            .bind(&sid)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE member_grants SET starts_at=$1,expires_at=$2 WHERE id=$3")
            .bind(at - 30 * 86400)
            .bind(at - 1)
            .bind(sub.get::<String, _>("grant_id"))
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE shop_orders SET period_start=$1,period_end=$2 WHERE id=$3")
            .bind(at - 30 * 86400)
            .bind(at - 1)
            .bind(&id)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(billing::tick(&site.app).await.unwrap(), 2);
        assert_eq!(billing::tick(&site.app).await.unwrap(), 0);
        let subscription_state: String =
            sqlx::query_scalar("SELECT state FROM shop_subscriptions WHERE id=$1")
                .bind(&sid)
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        assert_eq!(subscription_state, "past_due");
        let renewal=sqlx::query("SELECT id,subtotal_minor FROM shop_orders WHERE subscription_id=$1 AND purpose='renewal'").bind(&sid).fetch_one(&site.app.db.pool).await.unwrap();
        assert_eq!(renewal.get::<i64, _>("subtotal_minor"), 1000);
        let rid: String = renewal.get("id");
        assert_eq!(state(&site, &rid).await, "awaiting"); // Scheduler did not invent a payment.
        // A later catalog edit cannot change the accepted downgrade or renewal invoice.
        sqlx::query("UPDATE shop_variants SET price_minor=5000 WHERE id=$1")
            .bind(&lower)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        pay(&site, &rid, "actual-renewal").await;
        let sub = sqlx::query("SELECT * FROM shop_subscriptions WHERE id=$1")
            .bind(&sid)
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(sub.get::<String, _>("state"), "active");
        assert_eq!(sub.get::<i64, _>("price_minor"), 1000);
        assert_eq!(sub.get::<String, _>("variant_id"), lower);
        billing::cancel(&site.app, &buyer, &sid, sub.get("version"))
            .await
            .unwrap();
        let subscription_state: String =
            sqlx::query_scalar("SELECT state FROM shop_subscriptions WHERE id=$1")
                .bind(&sid)
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        assert_eq!(subscription_state, "cancel_at_end");
        let recovered = Site::new(pg, false).await;
        backup::restore(&recovered.app, &backup::capture(&site.app).await.unwrap())
            .await
            .unwrap();
        recovered.close().await;
        site.close().await;
    }
}

#[tokio::test]
async fn commerce_member_pricing_referrals_discounts_and_free_orders_share_local_accounts() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let (_, buyer) = shopper(&site, "member-price@example.test").await;
        let (_, referrer) = shopper(&site, "affiliate@example.test").await;
        let referral =
            wpalt::membership::referrals::create(&site.app, &referrer.user.id, "Local affiliate")
                .await
                .unwrap();
        let (_, v) = product(&site, "physical", 1000, 5, "").await;
        sqlx::query(
            "UPDATE shop_variants SET member_key='club',member_price_minor=800 WHERE id=$1",
        )
        .bind(&v)
        .execute(&site.app.db.pool)
        .await
        .unwrap();
        let i = cart(&site, &buyer, &v, "", 1).await;
        let initial = orders::quote(&site.app, &buyer, "").await.unwrap();
        assert_eq!(initial.total_minor, 1000);
        wpalt::membership::grant(
            &site.app,
            &buyer.user.id,
            "club",
            wpalt::now() - 1,
            0,
            "local-club",
        )
        .await
        .unwrap();
        assert!(orders::checkout(&site.app, &buyer, &i).await.is_err()); // Previously displayed price changed.
        catalog::discount(
            &site.app,
            site.session(),
            &catalog::DiscountInput {
                code: "SAVE10".into(),
                title: "Welcome".into(),
                bps: 1000,
                starts_at: wpalt::now() - 1,
                expires_at: wpalt::now() + 86400,
                max_uses: 1,
                member_key: "club".into(),
                product_id: String::new(),
                reward_id: String::new(),
                active: true,
            },
            0,
        )
        .await
        .unwrap();
        let q = orders::quote(&site.app, &buyer, "SAVE10").await.unwrap();
        assert_eq!(
            (q.subtotal_minor, q.discount_minor, q.total_minor),
            (800, 80, 720)
        );
        let i = orders::Checkout {
            request_key: uuid::Uuid::new_v4().to_string(),
            cart_version: q.cart_version,
            quote_hash: q.hash,
            discount_code: "SAVE10".into(),
            referral_id: referral,
            shipping_address: "Synthetic destination".into(),
            provider: "offline".into(),
            ..Default::default()
        };
        let id = orders::checkout(&site.app, &buyer, &i).await.unwrap();
        let earned: i64 = sqlx::query_scalar(
            "SELECT CAST(COALESCE(SUM(amount_minor),0) AS BIGINT) FROM member_commissions",
        )
        .fetch_one(&site.app.db.pool)
        .await
        .unwrap();
        assert_eq!(earned, 0);
        pay(&site, &id, "affiliate-payment").await;
        let earned: i64 =
            sqlx::query_scalar("SELECT CAST(SUM(amount_minor) AS BIGINT) FROM member_commissions")
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        assert_eq!(earned, 36);
        orders::record_payout(
            &site.app,
            site.session(),
            &referrer.user.id,
            36,
            "USD",
            "actual-affiliate-payout",
        )
        .await
        .unwrap();
        assert!(
            orders::record_payout(
                &site.app,
                site.session(),
                &referrer.user.id,
                1,
                "USD",
                "another-payout"
            )
            .await
            .is_err()
        );
        let subscribed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audience_memberships")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(subscribed, 0); // Purchase never signs customers up to marketing.
        let (_, free) = product(&site, "digital", 0, -1, "").await;
        let input = cart(&site, &buyer, &free, "", 1).await;
        let id = orders::checkout(&site.app, &buyer, &input).await.unwrap();
        assert_eq!(state(&site, &id).await, "paid");
        let paid: i64 = sqlx::query_scalar("SELECT paid_minor FROM shop_orders WHERE id=$1")
            .bind(&id)
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(paid, 0);
        let recovered = Site::new(pg, false).await;
        backup::restore(&recovered.app, &backup::capture(&site.app).await.unwrap())
            .await
            .unwrap();
        recovered.close().await;
        site.close().await;
    }
}

#[tokio::test]
async fn commerce_archive_rejects_financial_and_allocation_tampering_before_writes() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let (_, buyer) = shopper(&site, "archive@example.test").await;
        let (_, v) = product(&site, "physical", 1300, 2, "").await;
        let i = cart(&site, &buyer, &v, "", 1).await;
        let id = orders::checkout(&site.app, &buyer, &i).await.unwrap();
        pay(&site, &id, "archive-payment").await;
        let bytes = backup::capture(&site.app).await.unwrap();
        for change in ["stock", "total", "payment", "grant", "slot"] {
            let mut envelope: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let mut payload: serde_json::Value =
                serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
            match change {
                "stock" => payload["tables"]["shop_variants"][0]["sold"] = 0.into(),
                "total" => payload["tables"]["shop_orders"][0]["tax_minor"] = 100.into(),
                "payment" => payload["tables"]["shop_payments"][0]["amount_minor"] = 1.into(),
                "grant" => {
                    payload["tables"]["shop_order_lines"][0]["grant_id"] =
                        uuid::Uuid::new_v4().to_string().into()
                }
                "slot" => {
                    payload["tables"]["shop_order_lines"][0]["slot_id"] =
                        uuid::Uuid::new_v4().to_string().into()
                }
                _ => unreachable!(),
            }
            let raw = serde_json::to_string(&payload).unwrap();
            envelope["payload"] = raw.clone().into();
            envelope["sha256"] = auth::digest(raw.as_bytes()).into();
            let fresh = Site::new(pg, false).await;
            assert!(
                backup::restore(&fresh.app, &serde_json::to_vec(&envelope).unwrap())
                    .await
                    .is_err(),
                "tampering accepted: {change}"
            );
            let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
                .fetch_one(&fresh.app.db.pool)
                .await
                .unwrap();
            assert_eq!(users, 0);
            fresh.close().await;
        }
        site.close().await;
    }
}

#[tokio::test]
async fn commerce_https_provider_requires_tls_raw_signatures_exact_money_and_idempotent_refunds() {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use std::io::BufRead;
    use wpalt::commerce::payments;
    struct Provider(std::process::Child);
    impl Drop for Provider {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn signature(secret: &str, at: i64, raw: &[u8]) -> String {
        let mut m = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        m.update(format!("{at}.").as_bytes());
        m.update(raw);
        format!("t={at},v1={}", hex::encode(m.finalize().into_bytes()))
    }
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let (_, buyer) = shopper(&site, "provider@example.test").await;
        let (_, v) = product(&site, "digital", 1500, -1, "").await;
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let dir = tempfile::tempdir().unwrap();
        let spec = dir.path().join("provider.json");
        std::fs::write(&spec, b"{}").unwrap();
        let child = std::process::Command::new("python3")
            .arg("-u")
            .arg(fixtures.join("stripe_provider.py"))
            .arg(&spec)
            .arg(fixtures.join("identity-fixture-server.pem"))
            .arg(fixtures.join("identity-fixture-server.key"))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut provider = Provider(child);
        let mut port = String::new();
        std::io::BufReader::new(provider.0.stdout.take().unwrap())
            .read_line(&mut port)
            .unwrap();
        let port: u16 = port.trim().parse().expect("HTTPS provider fixture started");
        let mut app = site.app.clone();
        let mut cfg = app.config.as_ref().clone();
        cfg.base_url = "https://localhost".into();
        cfg.commerce.stripe = payments::StripeConfig {
            enabled: true,
            secret_key: "sk_fixture".into(),
            webhook_secret: "whsec_fixture".into(),
            api_url: format!("https://127.0.0.1:{port}"),
            ca_cert_file: String::new(),
        };
        cfg.validate().unwrap();
        assert!(
            !serde_json::to_string(&cfg.redacted())
                .unwrap()
                .contains("sk_fixture")
        );
        app.config = std::sync::Arc::new(cfg.clone());
        let mut input = cart(&site, &buyer, &v, "", 1).await;
        input.provider = "stripe".into();
        let id = orders::checkout(&app, &buyer, &input).await.unwrap();
        let mut data = serde_json::json!({"POST":{"/checkout/sessions":{"key":format!("wpalt-checkout-{id}"),"fields":{"mode":"payment","client_reference_id":id,"metadata[wpalt_order]":id,"line_items[0][price_data][unit_amount]":"1500","line_items[0][price_data][currency]":"usd"},"body":{"id":"cs_fixture","client_reference_id":id,"amount_total":1500,"currency":"usd","url":"https://checkout.stripe.com/c/pay/test"}}},"GET":{"/checkout/sessions/cs_fixture":{"body":{"id":"cs_fixture","client_reference_id":id,"payment_status":"paid","status":"complete","payment_intent":"pi_fixture","subscription":null}},"/payment_intents/pi_fixture":{"body":{"id":"pi_fixture","status":"succeeded","amount_received":1400,"currency":"usd"}}}});
        std::fs::write(&spec, serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(
            payments::checkout(&app, &buyer, &id).await.is_err(),
            "Untrusted TLS fixture must fail"
        );
        cfg.commerce.stripe.ca_cert_file = fixtures
            .join("identity-fixture-ca.pem")
            .to_str()
            .unwrap()
            .into();
        app.config = std::sync::Arc::new(cfg);
        let url = payments::checkout(&app, &buyer, &id).await.unwrap();
        assert!(url.starts_with("https://checkout.stripe.com/"));
        assert_eq!(payments::checkout(&app, &buyer, &id).await.unwrap(), url);
        let event = serde_json::json!({"id":"evt_fixture","api_version":payments::API_VERSION,"type":"checkout.session.completed","created":wpalt::now(),"data":{"object":{"id":"cs_fixture","amount_total":1,"payment_status":"paid"}}});
        let raw = serde_json::to_vec(&event).unwrap();
        let at = wpalt::now();
        assert!(
            payments::receive(&app, &signature("wrong", at, &raw), &raw)
                .await
                .is_err()
        );
        assert!(
            payments::receive(&app, &signature("whsec_fixture", at - 301, &raw), &raw)
                .await
                .is_err()
        );
        let sig = signature("whsec_fixture", at, &raw);
        let unsigned = Request::builder()
            .method("POST")
            .uri("/commerce/stripe/webhook")
            .header("content-type", "application/json")
            .body(Body::from(raw.clone()))
            .unwrap();
        assert!(
            !wpalt::web::router(app.clone())
                .oneshot(unsigned)
                .await
                .unwrap()
                .status()
                .is_success()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM shop_provider_events")
                .fetch_one(&app.db.pool)
                .await
                .unwrap(),
            0
        );
        let signed = Request::builder()
            .method("POST")
            .uri("/commerce/stripe/webhook")
            .header("origin", "https://provider.example")
            .header("stripe-signature", &sig)
            .header("content-type", "application/json")
            .body(Body::from(raw.clone()))
            .unwrap();
        assert_eq!(
            wpalt::web::router(app.clone())
                .oneshot(signed)
                .await
                .unwrap()
                .status(),
            StatusCode::NO_CONTENT,
            "Only the exact signed processor route bypasses browser origin enforcement."
        );
        payments::receive(&app, &sig, &raw).await.unwrap();
        assert_eq!(payments::process(&app).await.unwrap(), 0);
        assert_eq!(state(&site, &id).await, "awaiting"); // Signed object's fake amount never grants access; retrieved mismatch also fails.
        data["GET"]["/payment_intents/pi_fixture"]["body"]["amount_received"] = 1500.into();
        std::fs::write(&spec, serde_json::to_vec(&data).unwrap()).unwrap();
        sqlx::query("UPDATE shop_provider_events SET next_at=0 WHERE id='evt_fixture'")
            .execute(&app.db.pool)
            .await
            .unwrap();
        assert_eq!(payments::process(&app).await.unwrap(), 1);
        assert_eq!(state(&site, &id).await, "paid");
        payments::receive(&app, &sig, &raw).await.unwrap();
        assert_eq!(payments::process(&app).await.unwrap(), 0);
        let paid: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM shop_payments WHERE order_id=$1")
            .bind(&id)
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        assert_eq!(paid, 1);
        let mut edited = event.clone();
        edited["data"]["object"]["amount_total"] = 2.into();
        let bytes = serde_json::to_vec(&edited).unwrap();
        assert!(
            payments::receive(&app, &signature("whsec_fixture", at, &bytes), &bytes)
                .await
                .is_err()
        );
        edited["id"] = "evt_old_version".into();
        edited["api_version"] = "2025-03-31.basil".into();
        let bytes = serde_json::to_vec(&edited).unwrap();
        assert!(
            payments::receive(&app, &signature("whsec_fixture", at, &bytes), &bytes)
                .await
                .is_err()
        );
        let rid = orders::refund_request(
            &app,
            site.session(),
            &id,
            order_version(&site, &id).await,
            &orders::RefundInput {
                request_key: uuid::Uuid::new_v4().to_string(),
                amount_minor: 1500,
                reason: "Fixture authorized return".into(),
                restock: false,
            },
        )
        .await
        .unwrap();
        data["POST"]["/refunds"] = serde_json::json!({"key":format!("wpalt-refund-{rid}"),"fields":{"payment_intent":"pi_fixture","amount":"1500","metadata[wpalt_refund]":rid},"body":{"id":"re_fixture","payment_intent":"pi_fixture","amount":1500,"currency":"usd","status":"pending","metadata":{"wpalt_refund":rid}}});
        std::fs::write(&spec, serde_json::to_vec(&data).unwrap()).unwrap();
        payments::refund(&app, site.session(), &rid).await.unwrap();
        assert_eq!(state(&site, &id).await, "paid");
        data["POST"]["/refunds"]["body"]["status"] = "succeeded".into();
        std::fs::write(&spec, serde_json::to_vec(&data).unwrap()).unwrap();
        payments::refund(&app, site.session(), &rid).await.unwrap();
        assert_eq!(state(&site, &id).await, "refunded");
        // An invoice can arrive before Checkout completion. Canonical subscription
        // metadata establishes the mapping without granting access on a redirect.
        let (_, plan) = product(&site, "membership", 1000, -1, "month").await;
        let mut recurring = cart(&site, &buyer, &plan, "", 1).await;
        recurring.provider = "stripe".into();
        let initial = orders::checkout(&app, &buyer, &recurring).await.unwrap();
        data["POST"]["/checkout/sessions"] = serde_json::json!({"key":format!("wpalt-checkout-{initial}"),"fields":{"mode":"subscription","line_items[0][price_data][recurring][interval]":"month","subscription_data[metadata][wpalt_order]":initial},"body":{"id":"cs_recurring","client_reference_id":initial,"amount_total":1000,"currency":"usd","url":"https://checkout.stripe.com/c/pay/recurring"}});
        data["GET"]["/checkout/sessions/cs_recurring"] = serde_json::json!({"body":{"id":"cs_recurring","client_reference_id":initial,"payment_status":"paid","status":"complete","subscription":"sub_recurring","invoice":"in_initial"}});
        data["GET"]["/subscriptions/sub_recurring"] = serde_json::json!({"body":{"id":"sub_recurring","metadata":{"wpalt_order":initial},"latest_invoice":"in_initial","status":"active","cancel_at_period_end":false}});
        let first_start = wpalt::now();
        let first_end = billing::period_end(first_start, "month").unwrap();
        let invoice = |start, end, intent: &str, reason: &str| serde_json::json!({"status":"paid","currency":"usd","total":1000,"amount_due":1000,"amount_paid":1000,"amount_remaining":0,"billing_reason":reason,"status_transitions":{"paid_at":wpalt::now()},"parent":{"subscription_details":{"subscription":"sub_recurring"}},"lines":{"data":[{"period":{"start":start,"end":end}}],"has_more":false},"payments":{"data":[{"status":"paid","amount_paid":1000,"payment":{"type":"payment_intent","payment_intent":intent}}],"has_more":false}});
        data["GET"]["/invoices/in_initial"] = serde_json::json!({"body":invoice(first_start,first_end,"pi_initial","subscription_create")});
        std::fs::write(&spec, serde_json::to_vec(&data).unwrap()).unwrap();
        payments::checkout(&app, &buyer, &initial).await.unwrap();
        let deliver = |id: &str, kind: &str, object: &str| {
            serde_json::to_vec(&serde_json::json!({"id":id,"api_version":payments::API_VERSION,"type":kind,"created":wpalt::now(),"data":{"object":{"id":object}}})).unwrap()
        };
        let e = deliver("evt_invoice_first", "invoice.paid", "in_initial");
        payments::receive(&app, &signature("whsec_fixture", wpalt::now(), &e), &e)
            .await
            .unwrap();
        assert_eq!(payments::process(&app).await.unwrap(), 1);
        assert_eq!(state(&site, &initial).await, "paid");
        let e = deliver(
            "evt_checkout_later",
            "checkout.session.completed",
            "cs_recurring",
        );
        payments::receive(&app, &signature("whsec_fixture", wpalt::now(), &e), &e)
            .await
            .unwrap();
        assert_eq!(payments::process(&app).await.unwrap(), 1);
        let sid: String = sqlx::query_scalar("SELECT subscription_id FROM shop_orders WHERE id=$1")
            .bind(&initial)
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        let next_end = billing::period_end(first_end, "month").unwrap();
        data["GET"]["/invoices/in_renewal"] = serde_json::json!({"body":invoice(first_end,next_end,"pi_renewal","subscription_cycle")});
        data["GET"]["/subscriptions/sub_recurring"]["body"]["latest_invoice"] = "in_renewal".into();
        std::fs::write(&spec, serde_json::to_vec(&data).unwrap()).unwrap();
        let e = deliver("evt_renewal", "invoice.paid", "in_renewal");
        payments::receive(&app, &signature("whsec_fixture", wpalt::now(), &e), &e)
            .await
            .unwrap();
        assert_eq!(payments::process(&app).await.unwrap(), 1);
        // A delayed initial session must use its own invoice, never latest_invoice.
        let e = deliver(
            "evt_old_checkout",
            "checkout.session.completed",
            "cs_recurring",
        );
        payments::receive(&app, &signature("whsec_fixture", wpalt::now(), &e), &e)
            .await
            .unwrap();
        assert_eq!(payments::process(&app).await.unwrap(), 1);
        let periods:i64=sqlx::query_scalar("SELECT COUNT(*) FROM shop_payments p JOIN shop_orders o ON o.id=p.order_id WHERE o.subscription_id=$1").bind(&sid).fetch_one(&app.db.pool).await.unwrap();
        assert_eq!(periods, 2);
        let version: i64 = sqlx::query_scalar("SELECT version FROM shop_subscriptions WHERE id=$1")
            .bind(&sid)
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        data["POST"]["/subscriptions/sub_recurring"] = serde_json::json!({"key":format!("wpalt-cancel-{sid}-{version}"),"fields":{"cancel_at_period_end":"true"},"body":{"id":"sub_recurring","cancel_at_period_end":true}});
        std::fs::write(&spec, serde_json::to_vec(&data).unwrap()).unwrap();
        payments::cancel_subscription(&app, &buyer, &sid, version)
            .await
            .unwrap();
        let status: String = sqlx::query_scalar("SELECT state FROM shop_subscriptions WHERE id=$1")
            .bind(&sid)
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
        assert_eq!(status, "cancel_at_end");
        // Full refund persists the obligation to stop external future billing,
        // even when that provider operation is temporarily unavailable.
        let renewal: String = sqlx::query_scalar(
            "SELECT id FROM shop_orders WHERE subscription_id=$1 AND purpose='renewal'",
        )
        .bind(&sid)
        .fetch_one(&app.db.pool)
        .await
        .unwrap();
        let refund = orders::refund_request(
            &app,
            site.session(),
            &renewal,
            order_version(&site, &renewal).await,
            &orders::RefundInput {
                request_key: uuid::Uuid::new_v4().to_string(),
                amount_minor: 1000,
                reason: "Return recurring period".into(),
                restock: false,
            },
        )
        .await
        .unwrap();
        data["POST"]["/refunds"] = serde_json::json!({"key":format!("wpalt-refund-{refund}"),"fields":{"payment_intent":"pi_renewal","amount":"1000","metadata[wpalt_refund]":refund},"body":{"id":"re_recurring","payment_intent":"pi_renewal","amount":1000,"currency":"usd","status":"succeeded","metadata":{"wpalt_refund":refund}}});
        data["POST"]["/subscriptions/sub_recurring"] = serde_json::json!({"key":format!("wpalt-stop-billing-{sid}"),"fields":{"cancel_at_period_end":"true"},"status":503,"body":{}});
        std::fs::write(&spec, serde_json::to_vec(&data).unwrap()).unwrap();
        payments::refund(&app, site.session(), &refund)
            .await
            .unwrap();
        assert_eq!(state(&site, &renewal).await, "refunded");
        assert_eq!(payments::process(&app).await.unwrap(), 0);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT provider_cancel_pending FROM shop_subscriptions WHERE id=$1"
            )
            .bind(&sid)
            .fetch_one(&app.db.pool)
            .await
            .unwrap(),
            1
        );
        let waiting = Site::new(pg, false).await;
        backup::restore(&waiting.app, &backup::capture(&app).await.unwrap())
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT provider_cancel_pending FROM shop_subscriptions WHERE id=$1"
            )
            .bind(&sid)
            .fetch_one(&waiting.app.db.pool)
            .await
            .unwrap(),
            1
        );
        waiting.close().await;
        data["POST"]["/subscriptions/sub_recurring"]["status"] = 200.into();
        data["POST"]["/subscriptions/sub_recurring"]["body"] =
            serde_json::json!({"id":"sub_recurring","cancel_at_period_end":true});
        std::fs::write(&spec, serde_json::to_vec(&data).unwrap()).unwrap();
        sqlx::query("UPDATE shop_subscriptions SET provider_cancel_at=0 WHERE id=$1")
            .bind(&sid)
            .execute(&app.db.pool)
            .await
            .unwrap();
        assert_eq!(payments::process(&app).await.unwrap(), 1);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT provider_cancel_pending FROM shop_subscriptions WHERE id=$1"
            )
            .bind(&sid)
            .fetch_one(&app.db.pool)
            .await
            .unwrap(),
            0
        );
        let third_end = billing::period_end(next_end, "month").unwrap();
        data["GET"]["/invoices/in_after_cancel"] = serde_json::json!({"body":invoice(next_end,third_end,"pi_after_cancel","subscription_cycle")});
        std::fs::write(&spec, serde_json::to_vec(&data).unwrap()).unwrap();
        let e = deliver("evt_money_after_cancel", "invoice.paid", "in_after_cancel");
        payments::receive(&app, &signature("whsec_fixture", wpalt::now(), &e), &e)
            .await
            .unwrap();
        assert_eq!(
            payments::process(&app).await.unwrap(),
            2,
            "Record late money and retry the durable billing stop, never reactivate access."
        );
        let late: String = sqlx::query_scalar(
            "SELECT payment_state FROM shop_orders WHERE subscription_id=$1 AND period_start=$2",
        )
        .bind(&sid)
        .bind(next_end)
        .fetch_one(&app.db.pool)
        .await
        .unwrap();
        assert_eq!(late, "needs_refund");
        let mut pending = cart(&site, &buyer, &plan, "", 1).await;
        pending.provider = "stripe".into();
        let unpaid = orders::checkout(&app, &buyer, &pending).await.unwrap();
        let pending_sid: String =
            sqlx::query_scalar("SELECT subscription_id FROM shop_orders WHERE id=$1")
                .bind(&unpaid)
                .fetch_one(&app.db.pool)
                .await
                .unwrap();
        payments::cancel_subscription(&app, &buyer, &pending_sid, 1)
            .await
            .unwrap();
        assert_eq!(state(&site, &unpaid).await, "cancelled");
        let recovered = Site::new(pg, false).await;
        backup::restore(&recovered.app, &backup::capture(&app).await.unwrap())
            .await
            .unwrap();
        recovered.close().await;
        site.close().await;
    }
}

#[tokio::test]
async fn commerce_populated_catalog_cart_and_order_history_remain_bounded_and_indexed() {
    let mut evidence = vec![];
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let (token, buyer) = shopper(&site, "volume-store@example.test").await;
        let mut tx = site.app.db.pool.begin().await.unwrap();
        let mut chosen = vec![];
        let at = wpalt::now();
        for i in 0..1000 {
            let p = uuid::Uuid::new_v4().to_string();
            let v = uuid::Uuid::new_v4().to_string();
            let order = uuid::Uuid::new_v4().to_string();
            if i < 20 {
                chosen.push(v.clone())
            }
            sqlx::query("INSERT INTO shop_products(id,slug,title,description,kind,published,created_at) VALUES($1,$2,$3,'Populated store fixture','physical',1,$4)").bind(&p).bind(format!("volume-{i}")).bind(format!("Product {i}")).bind(at).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO shop_variants(id,product_id,title,sku,price_minor,stock_total) VALUES($1,$2,'Standard',$3,500,100)").bind(&v).bind(&p).bind(format!("SKU-{i}")).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO shop_orders(id,user_id,request_key,request_digest,cart_version,customer_name,customer_email,shipping_address,currency,subtotal_minor,discount_minor,tax_minor,shipping_minor,total_minor,tax_bps,tax_shipping,provider,payment_state,fulfillment,expires_at,created_at) VALUES($1,$2,$1,$3,0,'Shopper','volume-store@example.test','Synthetic','USD',500,0,0,0,500,0,0,'offline','cancelled','cancelled',$4,$4)").bind(&order).bind(&buyer.user.id).bind(auth::digest(order.as_bytes())).bind(at).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO shop_order_lines(id,order_id,variant_id,product_id,title,sku,kind,quantity,unit_minor,line_minor,allocation) VALUES($1,$2,$3,$4,$5,$6,'physical',1,500,500,'released')").bind(uuid::Uuid::new_v4().to_string()).bind(&order).bind(v).bind(p).bind(format!("Product {i}")).bind(format!("SKU-{i}")).execute(&mut *tx).await.unwrap();
        }
        tx.commit().await.unwrap();
        for v in &chosen {
            let ver = orders::cart_version(&site.app, &buyer.user.id)
                .await
                .unwrap();
            orders::set_cart(&site.app, &buyer, ver, v, "", 1)
                .await
                .unwrap();
        }
        let mut quote_times = vec![];
        let mut request_times = vec![];
        for _ in 0..30 {
            let start = std::time::Instant::now();
            let q = orders::quote(&site.app, &buyer, "").await.unwrap();
            quote_times.push(start.elapsed().as_secs_f64() * 1000.);
            assert_eq!(q.lines.len(), 20);
            assert_eq!(q.total_minor, 10000);
            let start = std::time::Instant::now();
            let (status, html) = get(&site.app, "/shop/orders", Some(&token)).await;
            request_times.push(start.elapsed().as_secs_f64() * 1000.);
            assert_eq!(status, StatusCode::OK);
            assert_eq!(html.matches("href=\"/shop/orders/").count(), 40);
        }
        let (status, html) = get(&site.app, "/shop", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(html.matches("href=\"/shop/products/").count(), 40);
        let mut page = "/shop/orders".to_string();
        let mut seen = HashSet::new();
        loop {
            let (_, html) = get(&site.app, &page, Some(&token)).await;
            for tail in html.split("href=\"/shop/orders/").skip(1) {
                assert!(seen.insert(tail.split('"').next().unwrap().to_string()));
            }
            if let Some((_, tail)) = html.split_once("href=\"/shop/orders?after=") {
                page = format!("/shop/orders?after={}", tail.split('"').next().unwrap());
                assert!(seen.len() <= 1000);
            } else {
                break;
            }
        }
        assert_eq!(seen.len(), 1000);
        sqlx::raw_sql("ANALYZE")
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        let prefix = if pg {
            "EXPLAIN "
        } else {
            "EXPLAIN QUERY PLAN "
        };
        let mut plans = vec![];
        for query in [format!("SELECT id FROM shop_orders WHERE user_id='{}' AND id>'00000000-0000-0000-0000-000000000000' ORDER BY id LIMIT 41",buyer.user.id),"SELECT p.id,p.title,p.description,p.kind,(SELECT MIN(v.price_minor) FROM shop_variants v WHERE v.product_id=p.id AND v.active=1) AS price FROM shop_products p WHERE p.published=1 AND p.id>'00000000-0000-0000-0000-000000000000' AND EXISTS(SELECT 1 FROM shop_variants v WHERE v.product_id=p.id AND v.active=1) ORDER BY p.id LIMIT 41".into(),format!("SELECT variant_id FROM shop_cart_lines WHERE user_id='{}' ORDER BY variant_id,slot_id LIMIT 21",buyer.user.id),"SELECT id FROM shop_slots WHERE variant_id='00000000-0000-0000-0000-000000000000' AND active=1 AND starts_at>1 ORDER BY starts_at,id LIMIT 41".into()]{let rows=sqlx::query(&format!("{prefix}{query}")).fetch_all(&site.app.db.pool).await.unwrap();plans.push(rows.iter().map(|r|if pg{r.get::<String,_>(0)}else{r.get::<String,_>("detail")}).collect::<Vec<_>>());}
        quote_times.sort_by(f64::total_cmp);
        request_times.sort_by(f64::total_cmp);
        evidence.push(serde_json::json!({"engine":if pg{"postgres"}else{"sqlite"},"conditions":"Debug integration router/domain calls, 1000 products/variants and consistent cancelled orders/lines; 20-line cart; 30 warm samples; no production-capacity claim or brittle timing threshold","quote_p50_ms":quote_times[14],"quote_p95_ms":quote_times[28],"private_order_request_p50_ms":request_times[14],"private_order_request_p95_ms":request_times[28],"plans":plans,"pagination":"All 1000 private orders reached once through bounded 40-row pages"}));
        site.close().await;
    }
    std::fs::create_dir_all("work").unwrap();
    std::fs::write(
        "work/m6-volume.json",
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn commerce_protected_download_authority_quota_and_late_money_remain_safe() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let (token, buyer) = shopper(&site, "protected-shopper@example.test").await;
        assert_eq!(
            upload(&site, "purchase.png", &png(), "private").await,
            StatusCode::SEE_OTHER
        );
        let media: String = sqlx::query_scalar("SELECT id FROM media")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        let p = catalog::save_product(
            &site.app,
            site.session(),
            None,
            0,
            &catalog::ProductInput {
                slug: "paid-download".into(),
                title: "Protected guide".into(),
                description: "Private purchased media".into(),
                kind: "digital".into(),
                entitlement: "guide".into(),
                access_seconds: 0,
                download_id: media.clone(),
                published: true,
            },
        )
        .await
        .unwrap();
        let v = catalog::save_variant(
            &site.app,
            site.session(),
            &p,
            None,
            0,
            &catalog::VariantInput {
                title: "Guide".into(),
                sku: "PRIVATE-GUIDE".into(),
                price_minor: 900,
                member_price_minor: -1,
                member_key: String::new(),
                stock_total: -1,
                billing_interval: String::new(),
                active: true,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            get(&site.app, &format!("/media/{media}"), Some(&token))
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        let input = cart(&site, &buyer, &v, "", 1).await;
        let id = orders::checkout(&site.app, &buyer, &input).await.unwrap();
        pay(&site, &id, "guide-payment").await;
        // Every new native mutation consumes CSRF before its operation; shared
        // middleware also rejects a foreign-origin request with a correct token.
        for path in [
            "/shop/cart".to_string(),
            "/shop/checkout".into(),
            format!("/shop/orders/{id}"),
            "/shop/subscriptions/00000000-0000-0000-0000-000000000001".into(),
            "/admin/shop".into(),
            format!("/admin/shop/products/{p}"),
            format!("/admin/shop/orders/{id}"),
        ] {
            let t = if path.starts_with("/admin/") {
                &site.token
            } else {
                &token
            };
            assert_eq!(
                request(
                    &site.app,
                    "POST",
                    &path,
                    Some(t),
                    "application/x-www-form-urlencoded",
                    b"csrf=wrong&action=payment".to_vec()
                )
                .await
                .0,
                StatusCode::FORBIDDEN,
                "Missing commerce CSRF boundary on {path}"
            );
        }
        assert_eq!(
            get(&site.app, "/admin/shop", Some(&token)).await.0,
            StatusCode::FORBIDDEN
        );
        let cross_origin = Request::builder()
            .method("POST")
            .uri("/shop/cart")
            .header("origin", "https://foreign.example")
            .header("cookie", format!("wpalt_session={token}"))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(format!("csrf={}", buyer.csrf)))
            .unwrap();
        assert_eq!(
            wpalt::web::router(site.app.clone())
                .oneshot(cross_origin)
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            state(&site, &id).await,
            "paid",
            "Rejected requests must not mutate the financial state."
        );
        assert_eq!(
            request(
                &site.app,
                "GET",
                &format!("/media/{media}"),
                Some(&token),
                "",
                vec![]
            )
            .await
            .0,
            StatusCode::OK
        );
        let (_, html) = get(&site.app, &format!("/shop/orders/{id}"), Some(&token)).await;
        assert!(html.contains(&format!("href=\"/media/{media}\"")));
        let refund = orders::refund_request(
            &site.app,
            site.session(),
            &id,
            order_version(&site, &id).await,
            &orders::RefundInput {
                request_key: uuid::Uuid::new_v4().to_string(),
                amount_minor: 900,
                reason: "Returned guide".into(),
                restock: false,
            },
        )
        .await
        .unwrap();
        orders::record_offline_refund(&site.app, site.session(), &refund, "guide-return", 900)
            .await
            .unwrap();
        assert_eq!(
            get(&site.app, &format!("/media/{media}"), Some(&token))
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        let (_, physical) = product(&site, "physical", 700, 1, "").await;
        let input = cart(&site, &buyer, &physical, "", 1).await;
        let held = orders::checkout(&site.app, &buyer, &input).await.unwrap();
        orders::cancel(&site.app, &buyer, &held, 1).await.unwrap();
        orders::confirm_payment(
            &site.app,
            &orders::Payment {
                order_id: held.clone(),
                provider: "offline".into(),
                reference: "late-actual-payment".into(),
                amount_minor: 700,
                currency: "USD".into(),
                paid_at: wpalt::now(),
                subscription_ref: String::new(),
                period_start: 0,
                period_end: 0,
            },
        )
        .await
        .unwrap();
        assert_eq!(state(&site, &held).await, "needs_refund");
        let stock: i64 = sqlx::query_scalar("SELECT held+sold FROM shop_variants WHERE id=$1")
            .bind(&physical)
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(stock, 0);
        let input = cart(&site, &buyer, &physical, "", 1).await;
        sqlx::query("UPDATE shop_usage SET limit_records=records WHERE id=1")
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        assert!(orders::checkout(&site.app, &buyer, &input).await.is_err());
        let present: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM shop_orders WHERE request_key=$1")
                .bind(&input.request_key)
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        assert_eq!(present, 0);
        let held_stock: i64 = sqlx::query_scalar("SELECT held FROM shop_variants WHERE id=$1")
            .bind(&physical)
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(held_stock, 0);
        sqlx::query("UPDATE shop_usage SET limit_records=1000000 WHERE id=1")
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        let mut disabled = site.app.clone();
        let mut cfg = disabled.config.as_ref().clone();
        cfg.commerce.enabled = false;
        disabled.config = std::sync::Arc::new(cfg);
        assert_eq!(
            get(&disabled, "/shop", Some(&token)).await.0,
            StatusCode::NOT_FOUND
        );
        assert!(orders::quote(&disabled, &buyer, "").await.is_err());
        assert_eq!(wpalt::commerce::tick(&disabled).await.unwrap(), 0);
        sqlx::query("UPDATE users SET role='disabled' WHERE id=$1")
            .bind(&buyer.user.id)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        assert!(orders::checkout(&site.app, &buyer, &input).await.is_err());
        sqlx::query("UPDATE users SET role='subscriber' WHERE id=$1")
            .bind(&buyer.user.id)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        let fresh = Site::new(pg, false).await;
        backup::restore(&fresh.app, &backup::capture(&site.app).await.unwrap())
            .await
            .unwrap();
        fresh.close().await;
        site.close().await;
    }
}

#[tokio::test]
async fn commerce_reward_redemption_survives_privacy_erasure_without_reusable_codes() {
    use wpalt::business::{engagement, promotions};
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let (_, buyer) = shopper(&site, "reward-buyer@example.test").await;
        sqlx::query("UPDATE engagement_settings SET enabled=1 WHERE id=1")
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        let promotion = promotions::create(&site.app, "Store reward").await.unwrap();
        promotions::save(
            &site.app,
            &promotion,
            1,
            "a",
            &wpalt::document::empty(),
            promotions::Target::default(),
            true,
            false,
            true,
        )
        .await
        .unwrap();
        promotions::reward(&site.app, &promotion, "Store discount", 1, 1)
            .await
            .unwrap();
        let policy: i64 = sqlx::query_scalar("SELECT version FROM engagement_settings WHERE id=1")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        let token = engagement::consent(
            &site.app,
            &axum::http::HeaderMap::new(),
            true,
            false,
            policy,
        )
        .await
        .unwrap()
        .unwrap();
        let mut h = axum::http::HeaderMap::new();
        h.insert("cookie", format!("wpalt_visitor={token}").parse().unwrap());
        promotions::visit(
            &site.app,
            &h,
            promotions::Visit {
                path: "/".into(),
                device: "desktop".into(),
                referrer: "direct".into(),
            },
        )
        .await
        .unwrap();
        let claim = promotions::claim(&site.app, &h, &promotion).await.unwrap();
        let code = claim["code"].as_str().unwrap();
        let reward: String =
            sqlx::query_scalar("SELECT reward_id FROM promotion_claims WHERE code=$1")
                .bind(code)
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        catalog::discount(
            &site.app,
            site.session(),
            &catalog::DiscountInput {
                code: "REWARD".into(),
                title: "Earned offer".into(),
                bps: 1000,
                starts_at: wpalt::now() - 1,
                expires_at: wpalt::now() + 86400,
                max_uses: 0,
                member_key: String::new(),
                product_id: String::new(),
                reward_id: reward,
                active: true,
            },
            0,
        )
        .await
        .unwrap();
        let (_, v) = product(&site, "physical", 1000, 3, "").await;
        let input = cart(&site, &buyer, &v, "", 1).await;
        let q = orders::quote(&site.app, &buyer, "REWARD").await.unwrap();
        let input = orders::Checkout {
            quote_hash: q.hash,
            discount_code: "REWARD".into(),
            reward_code: code.into(),
            ..input
        };
        let id = orders::checkout(&site.app, &buyer, &input).await.unwrap();
        pay(&site, &id, "reward-payment").await;
        let input = cart(&site, &buyer, &v, "", 1).await;
        let q = orders::quote(&site.app, &buyer, "REWARD").await.unwrap();
        assert!(
            orders::checkout(
                &site.app,
                &buyer,
                &orders::Checkout {
                    quote_hash: q.hash,
                    discount_code: "REWARD".into(),
                    reward_code: code.into(),
                    ..input
                }
            )
            .await
            .is_err()
        );
        engagement::consent(&site.app, &h, false, false, policy)
            .await
            .unwrap();
        let erased: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM promotion_claims")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(erased, 0);
        let used: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM shop_reward_redemptions WHERE state='used'")
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        assert_eq!(used, 1);
        let restored = Site::new(pg, false).await;
        backup::restore(&restored.app, &backup::capture(&site.app).await.unwrap())
            .await
            .unwrap();
        restored.close().await;
        site.close().await;
    }
}

#[tokio::test]
async fn commerce_upgrade_preserves_existing_site_and_rejects_route_and_currency_conflicts() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let post = content::save(
            &site.app,
            site.session(),
            None,
            input("before-commerce", "publish"),
        )
        .await
        .unwrap();
        // Reconstruct the merged M5 schema boundary, keeping its real publishing/account data.
        for table in wpalt::commerce::budget::TABLES.iter().rev() {
            sqlx::query(&format!("DROP TABLE {table}"))
                .execute(&site.app.db.pool)
                .await
                .unwrap();
        }
        for table in ["shop_settings", "shop_usage"] {
            sqlx::query(&format!("DROP TABLE {table}"))
                .execute(&site.app.db.pool)
                .await
                .unwrap();
        }
        sqlx::query("UPDATE schema_version SET version=8 WHERE id=1")
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE posts SET slug='shop',published_slug='shop' WHERE id=$1")
            .bind(&post.id)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        let err = site.app.db.migrate().await.unwrap_err().to_string();
        assert!(
            err.to_lowercase().contains("rename"),
            "The old runtime can resolve a route collision before upgrade: {err}"
        );
        let version: i64 = sqlx::query_scalar("SELECT version FROM schema_version WHERE id=1")
            .fetch_one(&site.app.db.pool)
            .await
            .unwrap();
        assert_eq!(
            version, 8,
            "Rejected migration must preserve the original schema boundary."
        );
        sqlx::query(
            "UPDATE posts SET slug='before-commerce',published_slug='before-commerce' WHERE id=$1",
        )
        .bind(&post.id)
        .execute(&site.app.db.pool)
        .await
        .unwrap();
        site.app.db.migrate().await.unwrap();
        assert!(
            get(&site.app, "/before-commerce", None)
                .await
                .1
                .contains("independent publishing")
        );
        let cfg = (*site.app.config).clone();
        site.app.db.pool.close().await;
        let upgraded = App::open(cfg.clone()).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT version FROM schema_version WHERE id=1")
                .fetch_one(&upgraded.db.pool)
                .await
                .unwrap(),
            14
        );
        let (token, owner) = auth::login(&upgraded, "owner@example.test", PASSWORD)
            .await
            .unwrap();
        assert!(!token.is_empty());
        assert_eq!(owner.user.id, site.session().user.id);
        upgraded.db.pool.close().await;
        let mut changed = cfg;
        changed.commerce.currency = "EUR".into();
        assert!(
            App::open(changed).await.is_err(),
            "A currency setting cannot reinterpret existing money."
        );
        site.close().await;
    }
}

#[tokio::test]
async fn commerce_calendar_pages_and_resource_reassignment_preserve_bookable_capacity() {
    for pg in engines() {
        let site = Site::new(pg, true).await;
        let (p, v) = product(&site, "booking", 1200, -1, "").await;
        let r = booking::resource(&site.app, site.session(), "Room A", &site.session().user.id)
            .await
            .unwrap();
        let other = booking::resource(&site.app, site.session(), "Room B", "")
            .await
            .unwrap();
        let start = wpalt::now() + 4 * 86400;
        for i in 0..42 {
            booking::slot(
                &site.app,
                site.session(),
                &booking::SlotInput {
                    resource_id: r.clone(),
                    variant_id: v.clone(),
                    starts_at: start + i * 3600,
                    ends_at: start + i * 3600 + 1800,
                    capacity: 2,
                },
            )
            .await
            .unwrap();
        }
        booking::slot(
            &site.app,
            site.session(),
            &booking::SlotInput {
                resource_id: other.clone(),
                variant_id: v.clone(),
                starts_at: start,
                ends_at: start + 1800,
                capacity: 2,
            },
        )
        .await
        .unwrap();
        assert!(
            booking::edit_resource(
                &site.app,
                site.session(),
                &other,
                1,
                "Room B",
                &site.session().user.id,
                true
            )
            .await
            .is_err(),
            "Reassignment must not double-book a staff member."
        );
        booking::edit_resource(
            &site.app,
            site.session(),
            &other,
            1,
            "Room B paused",
            "",
            false,
        )
        .await
        .unwrap();
        assert!(
            booking::edit_resource(&site.app, site.session(), &other, 1, "Stale edit", "", true)
                .await
                .is_err()
        );
        let admin = get(&site.app, "/admin/shop", Some(&site.token)).await.1;
        assert!(admin.contains("Next reservations"));
        let public = get(&site.app, &format!("/shop/products/{p}"), None).await.1;
        assert!(public.contains("Next available times"));
        let last: String =
            sqlx::query_scalar("SELECT id FROM shop_slots WHERE resource_id=$1 AND starts_at=$2")
                .bind(&r)
                .bind(start + 39 * 3600)
                .fetch_one(&site.app.db.pool)
                .await
                .unwrap();
        let next = get(
            &site.app,
            &format!(
                "/shop/products/{p}?calendar_start={}&calendar_after={last}",
                start + 39 * 3600
            ),
            None,
        )
        .await
        .1;
        assert!(!next.contains("Next available times"));
        // Disabling the assigned staff after display must invalidate a fresh cart allocation.
        let slot: String = sqlx::query_scalar(
            "SELECT id FROM shop_slots WHERE resource_id=$1 ORDER BY starts_at LIMIT 1",
        )
        .bind(&r)
        .fetch_one(&site.app.db.pool)
        .await
        .unwrap();
        let (_, buyer) = shopper(&site, "calendar@example.test").await;
        sqlx::query("UPDATE users SET role='disabled' WHERE id=$1")
            .bind(&site.session().user.id)
            .execute(&site.app.db.pool)
            .await
            .unwrap();
        assert!(
            orders::set_cart(
                &site.app,
                &buyer,
                orders::cart_version(&site.app, &buyer.user.id)
                    .await
                    .unwrap(),
                &v,
                &slot,
                1
            )
            .await
            .is_err()
        );
        site.close().await;
    }
}
