//! Native integrated merchant and shopper workflows; all prices and authority stay server-side.
use super::*;
use crate::{auth, view};
use axum::{
    Router,
    body::Bytes,
    extract::{Form, Path, Query, State},
    http::HeaderMap,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use maud::{Markup, html};
use sqlx::Row;
use std::collections::BTreeMap;
type Fields = BTreeMap<String, String>;
fn field<'a>(f: &'a Fields, k: &str) -> &'a str {
    f.get(k).map(String::as_str).unwrap_or("")
}
fn number(f: &Fields, k: &str) -> Result<i64> {
    field(f, k)
        .parse()
        .map_err(|_| Error::invalid("Enter a whole number."))
}
fn on(f: &Fields, k: &str) -> bool {
    field(f, k) == "on"
}
fn hidden(k: &str, v: &str) -> Markup {
    html! {input type="hidden" name=(k) value=(v);}
}
fn input(k: &str, label: &str, value: &str, kind: &str) -> Markup {
    html! {label {(label)input name=(k) value=(value) type=(kind);}}
}
fn check(k: &str, label: &str, value: bool) -> Markup {
    html! {label {input type="checkbox" name=(k) checked[value];(label)}}
}
fn minor(raw: &str, currency: &str) -> Result<i64> {
    if raw.len() > 32 || raw.is_empty() {
        return Err(Error::invalid("Enter a price in the store currency."));
    }
    let parts: Vec<_> = raw.split('.').collect();
    if parts.len() > 2
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
        || currency == "JPY" && parts.len() != 1
        || parts.get(1).is_some_and(|p| p.len() > 2)
    {
        return Err(Error::invalid(
            "Use a positive decimal price; JPY uses whole units.",
        ));
    }
    let whole: i64 = parts[0]
        .parse()
        .map_err(|_| Error::invalid("Price out of range."))?;
    let scale = if currency == "JPY" { 1 } else { 100 };
    let cents = match parts.get(1) {
        Some(p) => {
            p.parse::<i64>()
                .map_err(|_| Error::invalid("Invalid price."))?
                * if p.len() == 1 { 10 } else { 1 }
        }
        None => 0,
    };
    money(
        whole
            .checked_mul(scale)
            .and_then(|v| v.checked_add(cents))
            .ok_or(Error::invalid("Price out of range."))?,
    )
}
fn decimal(v: i64, currency: &str) -> String {
    if currency == "JPY" {
        v.to_string()
    } else {
        format!("{}.{:02}", v / 100, v % 100)
    }
}
fn date(raw: &str) -> Result<i64> {
    chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M")
        .map(|d| d.and_utc().timestamp())
        .map_err(|_| Error::invalid("Enter a valid date and time in UTC."))
}
pub fn routes(app: &App) -> Router<App> {
    if !app.config.commerce.enabled {
        return Router::new();
    }
    Router::new()
        .route("/admin/shop", get(admin).post(admin_action))
        .route(
            "/admin/shop/products/{id}",
            get(product_editor).post(product_action),
        )
        .route(
            "/admin/shop/orders/{id}",
            get(admin_order).post(order_action),
        )
        .route("/shop", get(catalog))
        .route("/shop/products/{id}", get(product))
        .route("/shop/cart", get(cart).post(cart_action))
        .route("/shop/checkout", post(checkout))
        .route("/shop/orders", get(my_orders))
        .route("/shop/orders/{id}", get(my_order).post(order_action))
        .route("/shop/subscriptions", get(subscriptions))
        .route("/shop/subscriptions/{id}", post(subscription_action))
        .route("/commerce/stripe/webhook", post(webhook))
}
async fn session(app: &App, h: &HeaderMap) -> Result<Session> {
    let s = auth::session(app, h).await?;
    customer(app, &s).await?;
    Ok(s)
}
async fn merchant(app: &App, h: &HeaderMap) -> Result<Session> {
    let s = session(app, h).await?;
    owner(app, &s).await?;
    Ok(s)
}
async fn page(app: &App, s: Option<&Session>, title: &str, body: Markup) -> Result<Html<String>> {
    let settings = app.db.settings().await?;
    Ok(Html(if let Some(s) = s {
        view::layout(title, &settings, Some(s), body)
    } else {
        view::member_layout(
            title,
            &settings,
            html! {nav aria-label="Shop"{a href="/shop"{"Catalog"}"· " a href="/shop/cart"{"Cart"}"· " a href="/shop/orders"{"My orders"}"· " a href="/shop/subscriptions"{"Subscriptions"}}(body)},
        )
    }))
}
#[derive(Default, Deserialize)]
struct Paging {
    #[serde(default)]
    after: String,
    #[serde(default)]
    before: i64,
    #[serde(default)]
    discount: String,
    #[serde(default)]
    view: String,
}
fn cursor(q: &Paging) -> Result<()> {
    if !q.after.is_empty() {
        crate::membership::uuid(&q.after)?;
    }
    if q.before < 0 {
        return Err(Error::invalid("Invalid cursor."));
    }
    Ok(())
}
async fn admin(
    State(app): State<App>,
    h: HeaderMap,
    Query(q): Query<Paging>,
) -> Result<Html<String>> {
    let s = merchant(&app, &h).await?;
    cursor(&q)?;
    let settings = settings(&app).await?;
    let products =
        sqlx::query("SELECT * FROM shop_products WHERE ($1='' OR id>$1) ORDER BY id LIMIT 41")
            .bind(&q.after)
            .fetch_all(&app.db.pool)
            .await?;
    let orders = if q.view == "all" {
        sqlx::query("SELECT id,customer_name,payment_state,fulfillment,total_minor,currency,created_at FROM shop_orders WHERE ($1='' OR id>$1) ORDER BY id LIMIT 41").bind(&q.after).fetch_all(&app.db.pool).await?
    } else {
        sqlx::query("SELECT id,customer_name,payment_state,fulfillment,total_minor,currency,created_at FROM shop_orders WHERE payment_state IN ('awaiting','needs_refund') OR fulfillment IN ('unfulfilled','cancel_requested') AND payment_state IN ('paid','partially_refunded') ORDER BY created_at,id LIMIT 41").fetch_all(&app.db.pool).await?
    };
    let resources =
        sqlx::query("SELECT id,title FROM shop_resources WHERE active=1 ORDER BY id LIMIT 100")
            .fetch_all(&app.db.pool)
            .await?;
    let variants=sqlx::query("SELECT v.id,p.title,v.title AS variant FROM shop_variants v JOIN shop_products p ON p.id=v.product_id WHERE p.kind='booking' AND p.published=1 AND v.active=1 ORDER BY v.id LIMIT 100").fetch_all(&app.db.pool).await?;
    let slots=sqlx::query("SELECT s.*,r.title FROM shop_slots s JOIN shop_resources r ON r.id=s.resource_id WHERE s.ends_at>$1 ORDER BY s.starts_at,s.id LIMIT 40").bind(crate::now()).fetch_all(&app.db.pool).await?;
    let discounts = sqlx::query("SELECT * FROM shop_discounts ORDER BY code LIMIT 100")
        .fetch_all(&app.db.pool)
        .await?;
    page(&app,Some(&s),"Commerce",html!{(view::heading("Store & reservations","Commerce","Products, orders and calendars share your local accounts, access rules and mail outbox."))nav aria-label="Commerce sections"{a href="#catalog"{"Catalog"}"· " a href="#orders"{"Orders"}"· " a href="#calendar"{"Calendar"}"· " a href="#discounts"{"Discounts"}"· " a href="#rules"{"Store rules"}"· " a href="/shop"{"View store ↗"}}
section id="catalog" {h2{"Catalog"}ul class="content-list"{@for p in products.iter().take(40){li{a href=(format!("/admin/shop/products/{}",p.get::<String,_>("id"))){(p.get::<String,_>("title"))} "· "(p.get::<String,_>("kind"))"· "(if p.get::<i64,_>("published")==1{"Published"}else{"Draft"})}}}@if products.len()>40{a href=(format!("/admin/shop?after={}",products[39].get::<String,_>("id"))){"Next products"}}details{summary{"Create product"}form method="post" action="/admin/shop"{(view::csrf(&s))(hidden("action","product"))(input("title","Product title","","text"))(input("slug","URL slug","","text"))label{"Product type" select aria-label="Product type" name="kind"{option value="physical"{"Physical"}option value="digital"{"Digital"}option value="membership"{"Membership"}option value="booking"{"Booking"}}}(input("entitlement","Access key (membership required)","","text"))(input("access_seconds","Access duration in seconds (0 = ongoing)","0","number"))(input("download_id","Private media ID (digital file)","","text"))label{"Description" textarea name="description"{}}(check("published","Publish product",false))button{"Create product"}}}}
section id="orders"{h2{"Orders needing attention"}p{a href="/admin/shop?view=all#orders"{"Browse all orders"}}ul class="content-list"{@for o in orders.iter().take(40){li{a href=(format!("/admin/shop/orders/{}",o.get::<String,_>("id"))){(o.get::<String,_>("customer_name"))"· "(amount(o.get("total_minor"),&o.get::<String,_>("currency")))}"· "(o.get::<String,_>("payment_state"))"· "(o.get::<String,_>("fulfillment"))}}}@if orders.len()>40{p{"More orders remain; process these or browse all records." a href=(format!("/admin/shop?view=all&after={}#orders",orders[39].get::<String,_>("id"))){"Next orders"}}}
section id="calendar"{h2{"Reservations · UTC"}ul class="content-list"{@for slot in &slots{li{(slot.get::<String,_>("title"))"· "(billing::utc(slot.get("starts_at")))"UTC · "(slot.get::<i64,_>("booked"))"booked / "(slot.get::<i64,_>("held"))"held / "(slot.get::<i64,_>("capacity"))"capacity" form method="post" action="/admin/shop"{(view::csrf(&s))(hidden("action","edit_slot"))(hidden("id",&slot.get::<String,_>("id")))(hidden("version",&slot.get::<i64,_>("version").to_string()))(input("capacity","Capacity",&slot.get::<i64,_>("capacity").to_string(),"number"))(check("active","Accept new reservations",slot.get::<i64,_>("active")==1))button{"Update slot"}}}}}details{summary{"Create resource"}form method="post" action="/admin/shop"{(view::csrf(&s))(hidden("action","resource"))(input("title","Resource name","","text"))(input("staff_id","Assigned staff ID (optional)","","text"))button{"Create resource"}}}details{summary{"Create slot"}form method="post" action="/admin/shop"{(view::csrf(&s))(hidden("action","slot"))label{"Resource" select aria-label="Resource" name="resource_id"{@for r in &resources{option value=(r.get::<String,_>("id")){(r.get::<String,_>("title"))}}}}label{"Booking variant" select aria-label="Booking variant" name="variant_id"{@for v in &variants{option value=(v.get::<String,_>("id")){(v.get::<String,_>("title"))"· "(v.get::<String,_>("variant"))}}}}(input("starts_at","Starts at (UTC)","","datetime-local"))(input("ends_at","Ends at (UTC)","","datetime-local"))(input("capacity","Group capacity","1","number"))button{"Create slot"}}}}
section id="discounts"{h2{"Discounts"}ul class="content-list"{@for d in &discounts{li{(d.get::<String,_>("code"))"· "(d.get::<i64,_>("bps"))"basis points · "(d.get::<i64,_>("used"))"redeemed"}}}details{summary{"Create discount"}form method="post" action="/admin/shop"{(view::csrf(&s))(hidden("action","discount"))(input("code","Coupon code","","text"))(input("title","Offer name","","text"))(input("bps","Discount in basis points (100 = 1%)","1000","number"))(input("starts_at","Starts at (UTC)","","datetime-local"))(input("expires_at","Expires at (UTC)","","datetime-local"))(input("max_uses","Maximum uses (0 = unlimited)","0","number"))(input("member_key","Member access key (optional)","","text"))(input("product_id","Eligible product ID (optional)","","text"))(input("reward_id","Promotion reward ID (optional)","","text"))button{"Create discount"}}}}
section id="rules"{h2{"Store rules"}p{"Currency: "(app.config.commerce.currency)". Prices use integer minor units internally. Set your applicable tax rules; external tax filing and datasets are separate services."}form method="post" action="/admin/shop"{(view::csrf(&s))(hidden("action","rules"))(hidden("version",&settings.get::<i64,_>("version").to_string()))(input("tax_bps","Tax basis points",&settings.get::<i64,_>("tax_bps").to_string(),"number"))(input("shipping","Flat physical shipping price",&decimal(settings.get("shipping_minor"),&app.config.commerce.currency),"text"))(check("tax_shipping","Tax shipping",settings.get::<i64,_>("tax_shipping")==1))button{"Save store rules"}}details{summary{"Record affiliate payout"}form method="post" action="/admin/shop"{(view::csrf(&s))(hidden("action","payout"))(input("user_id","Affiliate account ID","","text"))(input("amount","Actual payout amount","","text"))(input("reference","External/manual payout reference","","text"))button{"Record actual payout"}}}}}}).await
}
async fn admin_action(
    State(app): State<App>,
    h: HeaderMap,
    Form(f): Form<Fields>,
) -> Result<Redirect> {
    let s = merchant(&app, &h).await?;
    auth::csrf(&s, field(&f, "csrf"))?;
    match field(&f, "action") {
        "product" => {
            let id = catalog::save_product(&app, &s, None, 0, &product_fields(&f)?).await?;
            return Ok(Redirect::to(&format!("/admin/shop/products/{id}")));
        }
        "resource" => {
            booking::resource(&app, &s, field(&f, "title"), field(&f, "staff_id")).await?;
        }
        "slot" => {
            booking::slot(
                &app,
                &s,
                &booking::SlotInput {
                    resource_id: field(&f, "resource_id").into(),
                    variant_id: field(&f, "variant_id").into(),
                    starts_at: date(field(&f, "starts_at"))?,
                    ends_at: date(field(&f, "ends_at"))?,
                    capacity: number(&f, "capacity")?,
                },
            )
            .await?;
        }
        "edit_slot" => {
            booking::edit_slot(
                &app,
                &s,
                field(&f, "id"),
                number(&f, "version")?,
                number(&f, "capacity")?,
                on(&f, "active"),
            )
            .await?
        }
        "rules" => {
            catalog::set_rules(
                &app,
                &s,
                number(&f, "version")?,
                number(&f, "tax_bps")?,
                minor(field(&f, "shipping"), &app.config.commerce.currency)?,
                on(&f, "tax_shipping"),
            )
            .await?
        }
        "discount" => {
            catalog::discount(
                &app,
                &s,
                &catalog::DiscountInput {
                    code: field(&f, "code").into(),
                    title: field(&f, "title").into(),
                    bps: number(&f, "bps")?,
                    starts_at: date(field(&f, "starts_at"))?,
                    expires_at: date(field(&f, "expires_at"))?,
                    max_uses: number(&f, "max_uses")?,
                    member_key: field(&f, "member_key").into(),
                    product_id: field(&f, "product_id").into(),
                    reward_id: field(&f, "reward_id").into(),
                    active: true,
                },
                0,
            )
            .await?;
        }
        "payout" => {
            orders::record_payout(
                &app,
                &s,
                field(&f, "user_id"),
                minor(field(&f, "amount"), &app.config.commerce.currency)?,
                &app.config.commerce.currency,
                field(&f, "reference"),
            )
            .await?;
        }
        _ => return Err(Error::invalid("Choose a supported commerce action.")),
    };
    Ok(Redirect::to("/admin/shop"))
}
fn product_fields(f: &Fields) -> Result<catalog::ProductInput> {
    Ok(catalog::ProductInput {
        slug: field(f, "slug").into(),
        title: field(f, "title").into(),
        description: field(f, "description").into(),
        kind: field(f, "kind").into(),
        entitlement: field(f, "entitlement").into(),
        access_seconds: number(f, "access_seconds")?,
        download_id: field(f, "download_id").into(),
        published: on(f, "published"),
    })
}
async fn product_editor(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let s = merchant(&app, &h).await?;
    let p = sqlx::query("SELECT * FROM shop_products WHERE id=$1")
        .bind(&id)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
    let variants =
        sqlx::query("SELECT * FROM shop_variants WHERE product_id=$1 ORDER BY id LIMIT 100")
            .bind(&id)
            .fetch_all(&app.db.pool)
            .await?;
    page(&app,Some(&s),"Product",html!{(view::heading("Commerce","Product","Publish clear terms and manage prices without changing existing order records."))a href="/admin/shop"{"← Commerce"}form method="post"{(view::csrf(&s))(hidden("action","product"))(hidden("version",&p.get::<i64,_>("version").to_string()))(hidden("kind",&p.get::<String,_>("kind")))(hidden("entitlement",&p.get::<String,_>("entitlement")))(hidden("download_id",&p.get::<String,_>("download_id")))(input("title","Product title",&p.get::<String,_>("title"),"text"))(input("slug","URL slug",&p.get::<String,_>("slug"),"text"))label{"Description" textarea name="description"{(p.get::<String,_>("description"))}}(input("access_seconds","Access duration in seconds (0 = ongoing)",&p.get::<i64,_>("access_seconds").to_string(),"number"))(check("published","Publish product",p.get::<i64,_>("published")==1))button{"Save product"}}
h2{"Variants"}@for v in &variants{details{summary{(v.get::<String,_>("title"))"· "(amount(v.get("price_minor"),&app.config.commerce.currency))}form method="post"{(view::csrf(&s))(hidden("action","variant"))(hidden("variant_id",&v.get::<String,_>("id")))(hidden("version",&v.get::<i64,_>("version").to_string()))(variant_fields(Some(v),&app.config.commerce.currency))button{"Save variant"}}}}details open{summary{"Add variant"}form method="post"{(view::csrf(&s))(hidden("action","variant"))(hidden("version","0"))(variant_fields(None,&app.config.commerce.currency))button{"Create variant"}}}}).await
}
fn variant_fields(v: Option<&sqlx::any::AnyRow>, currency: &str) -> Markup {
    let value = |k: &str| v.map(|r| r.get::<String, _>(k)).unwrap_or_default();
    let n = |k: &str, d: i64| v.map(|r| r.get::<i64, _>(k)).unwrap_or(d);
    html! {(input("title","Variant title",&value("title"),"text"))(input("sku","Unique SKU",&value("sku"),"text"))(input("price","Price",&decimal(n("price_minor",0),currency),"text"))(input("member_price","Member price (blank = disabled)",&if n("member_price_minor",-1)<0{String::new()}else{decimal(n("member_price_minor",-1),currency)},"text"))(input("member_key","Member pricing access key",&value("member_key"),"text"))(input("stock","Total physical stock (-1 for nonphysical)",&n("stock_total",-1).to_string(),"number"))label{"Billing interval (fixed-price memberships only)" select aria-label="Billing interval (fixed-price memberships only)" name="billing_interval"{@for (k,label) in [("","One-time"),("day","Daily"),("week","Weekly"),("month","Monthly"),("year","Yearly")]{option value=(k) selected[value("billing_interval")==k]{(label)}}}}(check("active","Available for purchase",n("active",1)==1))}
}
async fn product_action(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<Fields>,
) -> Result<Redirect> {
    let s = merchant(&app, &h).await?;
    auth::csrf(&s, field(&f, "csrf"))?;
    match field(&f, "action") {
        "product" => {
            catalog::save_product(
                &app,
                &s,
                Some(&id),
                number(&f, "version")?,
                &product_fields(&f)?,
            )
            .await?;
        }
        "variant" => {
            catalog::save_variant(
                &app,
                &s,
                &id,
                if field(&f, "variant_id").is_empty() {
                    None
                } else {
                    Some(field(&f, "variant_id"))
                },
                number(&f, "version")?,
                &catalog::VariantInput {
                    title: field(&f, "title").into(),
                    sku: field(&f, "sku").into(),
                    price_minor: minor(field(&f, "price"), &app.config.commerce.currency)?,
                    member_price_minor: if field(&f, "member_price").is_empty() {
                        -1
                    } else {
                        minor(field(&f, "member_price"), &app.config.commerce.currency)?
                    },
                    member_key: field(&f, "member_key").into(),
                    stock_total: number(&f, "stock")?,
                    billing_interval: field(&f, "billing_interval").into(),
                    active: on(&f, "active"),
                },
            )
            .await?;
        }
        _ => return Err(Error::invalid("Choose a product action.")),
    };
    Ok(Redirect::to(&format!("/admin/shop/products/{id}")))
}
async fn catalog(State(app): State<App>, Query(q): Query<Paging>) -> Result<Html<String>> {
    cursor(&q)?;
    let products=sqlx::query("SELECT p.id,p.title,p.description,p.kind,MIN(v.price_minor) AS price FROM shop_products p JOIN shop_variants v ON v.product_id=p.id AND v.active=1 WHERE p.published=1 AND ($1='' OR p.id>$1) GROUP BY p.id,p.title,p.description,p.kind ORDER BY p.id LIMIT 41").bind(&q.after).fetch_all(&app.db.pool).await?;
    page(&app,None,"Store",html!{(view::heading("Your local store","Store","Thoughtfully selected products and reservations."))ul class="content-list"{@for p in products.iter().take(40){li{h2{a href=(format!("/shop/products/{}",p.get::<String,_>("id"))){(p.get::<String,_>("title"))}}p{(p.get::<String,_>("description"))}p{(p.get::<String,_>("kind"))"· from "(amount(p.get("price"),&app.config.commerce.currency))}}}@if products.len()>40{a href=(format!("/shop?after={}",products[39].get::<String,_>("id"))){"Next products"}}}}).await
}
async fn product(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let p = sqlx::query("SELECT * FROM shop_products WHERE id=$1 AND published=1")
        .bind(&id)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
    let variants = sqlx::query(
        "SELECT * FROM shop_variants WHERE product_id=$1 AND active=1 ORDER BY id LIMIT 100",
    )
    .bind(&id)
    .fetch_all(&app.db.pool)
    .await?;
    let s = auth::session(&app, &h).await.ok();
    let version = if let Some(s) = &s {
        customer(&app, s).await?;
        orders::cart_version(&app, &s.user.id).await?
    } else {
        0
    };
    let slots=sqlx::query("SELECT s.*,r.title FROM shop_slots s JOIN shop_resources r ON r.id=s.resource_id JOIN shop_variants v ON v.id=s.variant_id WHERE v.product_id=$1 AND s.active=1 AND r.active=1 AND s.starts_at>$2 AND s.capacity>s.held+s.booked ORDER BY s.starts_at,s.id LIMIT 100").bind(&id).bind(crate::now()).fetch_all(&app.db.pool).await?;
    page(&app,None,&p.get::<String,_>("title"),html!{(view::heading("Store",&p.get::<String,_>("title"),&p.get::<String,_>("description")))a href="/shop"{"← Catalog"}@for v in &variants{section{h2{(v.get::<String,_>("title"))}p{(amount(v.get("price_minor"),&app.config.commerce.currency))@if !v.get::<String,_>("billing_interval").is_empty(){"/ "(v.get::<String,_>("billing_interval"))} @if v.get::<i64,_>("stock_total")>=0{"· "(v.get::<i64,_>("stock_total")-v.get::<i64,_>("held")-v.get::<i64,_>("sold"))"available"}}@if let Some(s)=&s{form method="post" action="/shop/cart"{(view::csrf(s))(hidden("variant_id",&v.get::<String,_>("id")))(hidden("version",&version.to_string()))@if p.get::<String,_>("kind")=="booking"{label{"Reservation time · UTC" select aria-label="Reservation time · UTC" name="slot_id" required{@for slot in slots.iter().filter(|slot|slot.get::<String,_>("variant_id")==v.get::<String,_>("id")){option value=(slot.get::<String,_>("id")){(slot.get::<String,_>("title"))"· "(billing::utc(slot.get("starts_at")))"UTC"}}}}} @else{(hidden("slot_id",""))}@if ["digital","membership"].contains(&p.get::<String,_>("kind").as_str()){(hidden("quantity","1"))} @else{label{"Quantity" input type="number" name="quantity" min="1" max="100" value="1";}}button{"Add to cart"}}} @else{a href="/login"{"Sign in to purchase"}}}}}).await
}
async fn cart(
    State(app): State<App>,
    h: HeaderMap,
    Query(q): Query<Paging>,
) -> Result<Html<String>> {
    let s = session(&app, &h).await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM shop_cart_lines WHERE user_id=$1")
        .bind(&s.user.id)
        .fetch_one(&app.db.pool)
        .await?;
    if count == 0 {
        return page(&app,None,"Cart",html!{(view::heading("Store","Your cart","Your cart is empty."))a href="/shop"{"Explore the catalog"}}).await;
    }
    let quote = match orders::quote(&app, &s, &q.discount).await {
        Ok(q) => q,
        Err(e)
            if e.0 == axum::http::StatusCode::UNPROCESSABLE_ENTITY
                || e.0 == axum::http::StatusCode::CONFLICT =>
        {
            let lines=sqlx::query("SELECT c.variant_id,c.slot_id,c.quantity,p.title FROM shop_cart_lines c JOIN shop_variants v ON v.id=c.variant_id JOIN shop_products p ON p.id=v.product_id WHERE c.user_id=$1 ORDER BY c.variant_id,c.slot_id LIMIT 20").bind(&s.user.id).fetch_all(&app.db.pool).await?;
            let version = orders::cart_version(&app, &s.user.id).await?;
            return page(&app,None,"Review cart",html!{(view::heading("Store","Review your cart",e.1))p {"An item, price, stock or coupon changed. Remove unavailable items or return to the cart without a coupon."}a href="/shop/cart" {"Review without coupon"}ul class="content-list" {@for l in &lines {li {(l.get::<String,_>("title"))form method="post" action="/shop/cart" {(view::csrf(&s))(hidden("variant_id",&l.get::<String,_>("variant_id")))(hidden("slot_id",&l.get::<String,_>("slot_id")))(hidden("version",&version.to_string()))(hidden("quantity","0"))button class="quiet" {"Remove "(l.get::<String,_>("title"))}}}}}}).await;
        }
        Err(e) => return Err(e),
    };
    let key = uuid::Uuid::new_v4().to_string();
    page(&app,None,"Cart",html!{(view::heading("Store","Your cart","Review current terms. Stock and reservations are held only after placing your order."))ul class="content-list"{@for l in &quote.lines{li{h2{(l.title)}p{(l.quantity)"× "(amount(l.unit_minor,&quote.currency))"= "(amount(l.line_minor,&quote.currency))}form method="post" action="/shop/cart"{(view::csrf(&s))(hidden("variant_id",&l.variant_id))(hidden("slot_id",&l.slot_id))(hidden("version",&quote.cart_version.to_string()))(hidden("quantity","0"))button class="quiet"{"Remove "(l.title)}}}}}form method="get" action="/shop/cart"{(input("discount","Coupon code",&q.discount,"text"))button{"Apply coupon"}}dl{dt{"Subtotal"}dd{(amount(quote.subtotal_minor,&quote.currency))}dt{"Discount"}dd{(amount(quote.discount_minor,&quote.currency))}dt{"Shipping"}dd{(amount(quote.shipping_minor,&quote.currency))}dt{"Tax"}dd{(amount(quote.tax_minor,&quote.currency))}dt{"Total"}dd{strong{(amount(quote.total_minor,&quote.currency))}}}form method="post" action="/shop/checkout"{(view::csrf(&s))(hidden("request_key",&key))(hidden("cart_version",&quote.cart_version.to_string()))(hidden("quote_hash",&quote.hash))(hidden("discount_code",&q.discount))@if quote.lines.iter().any(|l|l.kind=="physical"){label{"Shipping address" textarea name="shipping_address" required{}}} @else{(hidden("shipping_address",""))}(input("reward_code","Reward code (only for reward-linked coupons)","","text"))(input("referral_id","Referral ID (optional)","","text"))label{"Payment method" select aria-label="Payment method" name="provider"{option value="offline"{"Offline — payment remains pending until merchant confirmation"}@if app.config.commerce.stripe.enabled{option value="stripe"{"Secure hosted card checkout"}}}}p{"Placing an order accepts the displayed price and billing interval. Offline orders are not marked paid automatically."}button{"Place order"}}}).await
}
async fn cart_action(
    State(app): State<App>,
    h: HeaderMap,
    Form(f): Form<Fields>,
) -> Result<Redirect> {
    let s = session(&app, &h).await?;
    auth::csrf(&s, field(&f, "csrf"))?;
    orders::set_cart(
        &app,
        &s,
        number(&f, "version")?,
        field(&f, "variant_id"),
        field(&f, "slot_id"),
        number(&f, "quantity")?,
    )
    .await?;
    Ok(Redirect::to("/shop/cart"))
}
async fn checkout(State(app): State<App>, h: HeaderMap, Form(f): Form<Fields>) -> Result<Redirect> {
    let s = session(&app, &h).await?;
    auth::csrf(&s, field(&f, "csrf"))?;
    let id = orders::checkout(
        &app,
        &s,
        &orders::Checkout {
            request_key: field(&f, "request_key").into(),
            cart_version: number(&f, "cart_version")?,
            quote_hash: field(&f, "quote_hash").into(),
            shipping_address: field(&f, "shipping_address").into(),
            discount_code: field(&f, "discount_code").into(),
            reward_code: field(&f, "reward_code").into(),
            referral_id: field(&f, "referral_id").into(),
            provider: field(&f, "provider").into(),
        },
    )
    .await?;
    Ok(Redirect::to(&format!("/shop/orders/{id}")))
}
async fn my_orders(
    State(app): State<App>,
    h: HeaderMap,
    Query(q): Query<Paging>,
) -> Result<Html<String>> {
    let s = session(&app, &h).await?;
    cursor(&q)?;
    let rows=sqlx::query("SELECT id,total_minor,currency,payment_state,created_at FROM shop_orders WHERE user_id=$1 AND ($2='' OR id>$2) ORDER BY id LIMIT 41").bind(&s.user.id).bind(&q.after).fetch_all(&app.db.pool).await?;
    page(&app,None,"My orders",html!{(view::heading("Store","My orders","Your purchases, payment records and reservations."))ul class="content-list"{@for r in rows.iter().take(40){li{a href=(format!("/shop/orders/{}",r.get::<String,_>("id"))){(billing::utc(r.get("created_at")))"UTC · "(amount(r.get("total_minor"),&r.get::<String,_>("currency")))}"· "(r.get::<String,_>("payment_state"))}}}@if rows.len()>40{a href=(format!("/shop/orders?after={}",rows[39].get::<String,_>("id"))){"Next orders"}}}).await
}
async fn admin_order(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let s = merchant(&app, &h).await?;
    order_page(&app, &s, &id, true).await
}
async fn my_order(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let s = session(&app, &h).await?;
    order_page(&app, &s, &id, false).await
}
async fn order_page(app: &App, s: &Session, id: &str, admin: bool) -> Result<Html<String>> {
    let o = sqlx::query("SELECT * FROM shop_orders WHERE id=$1 AND ($2=1 OR user_id=$3)")
        .bind(id)
        .bind(i64::from(admin))
        .bind(&s.user.id)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
    let lines=sqlx::query("SELECT l.*,s.starts_at,r.title AS resource,p.download_id,d.filename AS download_filename FROM shop_order_lines l JOIN shop_products p ON p.id=l.product_id LEFT JOIN media d ON d.id=p.download_id LEFT JOIN shop_slots s ON s.id=l.slot_id LEFT JOIN shop_resources r ON r.id=s.resource_id WHERE l.order_id=$1 ORDER BY l.id").bind(id).fetch_all(&app.db.pool).await?;
    let refunds = sqlx::query(
        "SELECT * FROM shop_refunds WHERE order_id=$1 ORDER BY created_at,id LIMIT 100",
    )
    .bind(id)
    .fetch_all(&app.db.pool)
    .await?;
    let history=sqlx::query("SELECT action,amount_minor,created_at FROM shop_history WHERE order_id=$1 ORDER BY created_at DESC,id DESC LIMIT 100").bind(id).fetch_all(&app.db.pool).await?;
    let state: String = o.get("payment_state");
    let provider: String = o.get("provider");
    let currency: String = o.get("currency");
    let version = o.get::<i64, _>("version").to_string();
    let action = if admin {
        format!("/admin/shop/orders/{id}")
    } else {
        format!("/shop/orders/{id}")
    };
    page(app,if admin{Some(s)}else{None},"Order",html!{(view::heading("Commerce","Order","A retained local financial record. Payment and fulfillment are separate states."))p class="meta"{(id)}p{strong{(state)}"· "(o.get::<String,_>("fulfillment"))"· "(provider)}p{(o.get::<String,_>("customer_name"))"· "(o.get::<String,_>("customer_email"))}p{(o.get::<String,_>("shipping_address"))}ul class="content-list"{@for l in &lines{li{h2{(l.get::<String,_>("title"))}p{(l.get::<i64,_>("quantity"))"× "(amount(l.get("unit_minor"),&currency))}@if !l.get::<String,_>("slot_id").is_empty(){p{(l.get::<Option<String>,_>("resource").unwrap_or_default())"· "(billing::utc(l.get::<Option<i64>,_>("starts_at").unwrap_or_default()))"UTC"}}@if state=="paid"&&!l.get::<String,_>("download_id").is_empty(){a href=(format!("/media/{}",l.get::<Option<String>,_>("download_filename").unwrap_or_default())){"Protected download"}}}}}dl{dt{"Subtotal"}dd{(amount(o.get("subtotal_minor"),&currency))}dt{"Discount"}dd{(amount(o.get("discount_minor"),&currency))}dt{"Tax"}dd{(amount(o.get("tax_minor"),&currency))}dt{"Shipping"}dd{(amount(o.get("shipping_minor"),&currency))}dt{"Total"}dd{strong{(amount(o.get("total_minor"),&currency))}}dt{"Recorded paid"}dd{(amount(o.get("paid_minor"),&currency))}dt{"Confirmed refunded"}dd{(amount(o.get("refunded_minor"),&currency))}}
@if state=="awaiting"{p{"Payment is pending. Hold expires: "(billing::utc(o.get("expires_at")))"UTC."}@if provider=="stripe"&&!admin{form method="post" action=(&action){(view::csrf(s))(hidden("action","pay"))(hidden("version",&version))button{"Continue to secure payment"}}} @else if admin&&provider=="offline"{form method="post" action=(&action){(view::csrf(s))(hidden("action","paid"))(hidden("version",&version))(input("reference","Actual received payment reference","","text"))button{"Record received payment"}}}}
@if ["awaiting","paid","partially_refunded"].contains(&state.as_str()){form method="post" action=(&action){(view::csrf(s))(hidden("action","cancel"))(hidden("version",&version))button class="quiet"{(if state=="awaiting"{"Cancel unpaid order"}else{"Request cancellation"})}}}
@if admin&&["paid","partially_refunded"].contains(&state.as_str())&&o.get::<String,_>("fulfillment")=="unfulfilled"{form method="post" action=(&action){(view::csrf(s))(hidden("action","fulfill"))(hidden("version",&version))button{"Mark fulfilled"}}}
@if admin&&["paid","partially_refunded","needs_refund"].contains(&state.as_str()){details{summary{"Request refund"}form method="post" action=(&action){(view::csrf(s))(hidden("action","refund"))(hidden("version",&version))(hidden("request_key",&uuid::Uuid::new_v4().to_string()))(input("amount","Actual refund amount","","text"))(input("reason","Refund reason","","text"))(check("restock","Restock physical items (final full refund only)",false))button{"Authorize refund"}}}}
h2{"Refund records"}ul class="content-list"{@for r in &refunds{li{(amount(r.get("amount_minor"),&currency))"· "(r.get::<String,_>("state"))"· "(r.get::<String,_>("reason"))@if admin&&r.get::<String,_>("state")=="pending"{form method="post" action=(&action){(view::csrf(s))(hidden("action","settle_refund"))(hidden("refund_id",&r.get::<String,_>("id")))@if provider=="offline"{(input("reference","Actual refund payment reference","","text"))}button{(if provider=="offline"{"Record completed refund"}else{"Send/retry provider refund"})}}}}}}
details{summary{"Financial history"}ul{@for r in &history{li{(billing::utc(r.get("created_at")))"UTC · "(r.get::<String,_>("action"))"· "(amount(r.get("amount_minor"),&currency))}}}}p{"This printable record retains agreed totals. It is not a jurisdiction-certified tax invoice."}}).await
}
async fn order_action(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<Fields>,
) -> Result<Response> {
    let s = session(&app, &h).await?;
    auth::csrf(&s, field(&f, "csrf"))?;
    match field(&f, "action") {
        "cancel" => orders::cancel(&app, &s, &id, number(&f, "version")?).await?,
        "fulfill" => orders::fulfill(&app, &s, &id, number(&f, "version")?).await?,
        "paid" => {
            orders::record_offline(
                &app,
                &s,
                &id,
                number(&f, "version")?,
                field(&f, "reference"),
            )
            .await?
        }
        "pay" => {
            return Ok(Redirect::to(&payments::checkout(&app, &s, &id).await?).into_response());
        }
        "refund" => {
            orders::refund_request(
                &app,
                &s,
                &id,
                number(&f, "version")?,
                &orders::RefundInput {
                    request_key: field(&f, "request_key").into(),
                    amount_minor: minor(field(&f, "amount"), &app.config.commerce.currency)?,
                    reason: field(&f, "reason").into(),
                    restock: on(&f, "restock"),
                },
            )
            .await?;
        }
        "settle_refund" => {
            owner(&app, &s).await?;
            let rid = field(&f, "refund_id");
            let r=sqlx::query("SELECT r.amount_minor,o.provider FROM shop_refunds r JOIN shop_orders o ON o.id=r.order_id WHERE r.id=$1 AND r.order_id=$2").bind(rid).bind(&id).fetch_optional(&app.db.pool).await?.ok_or_else(Error::not_found)?;
            if r.get::<String, _>("provider") == "stripe" {
                payments::refund(&app, &s, rid).await?;
            } else {
                orders::record_offline_refund(
                    &app,
                    &s,
                    rid,
                    field(&f, "reference"),
                    r.get("amount_minor"),
                )
                .await?;
            }
        }
        _ => return Err(Error::invalid("Choose an order action.")),
    }
    let admin = crate::membership::staff(&app, &s).await.is_ok();
    Ok(Redirect::to(&format!(
        "{}/orders/{id}",
        if admin { "/admin/shop" } else { "/shop" }
    ))
    .into_response())
}
async fn subscriptions(
    State(app): State<App>,
    h: HeaderMap,
    Query(q): Query<Paging>,
) -> Result<Html<String>> {
    let s = session(&app, &h).await?;
    cursor(&q)?;
    let subs=sqlx::query("SELECT s.*,p.title FROM shop_subscriptions s JOIN shop_variants v ON v.id=s.variant_id JOIN shop_products p ON p.id=v.product_id WHERE s.user_id=$1 AND ($2='' OR s.id>$2) ORDER BY s.id LIMIT 41").bind(&s.user.id).bind(&q.after).fetch_all(&app.db.pool).await?;
    page(&app,None,"Subscriptions",html!{(view::heading("Store","My subscriptions","Access follows settled billing periods. Cancelling preserves the current paid period."))ul class="content-list"{@for sub in subs.iter().take(40){li{h2{(sub.get::<String,_>("title"))}p{(amount(sub.get("price_minor"),&app.config.commerce.currency))"/ "(sub.get::<String,_>("billing_interval"))"· "(sub.get::<String,_>("state"))}p{"Paid period ends "(billing::utc(sub.get("period_end")))"UTC"}@if ["active","past_due","pending"].contains(&sub.get::<String,_>("state").as_str()){form method="post" action=(format!("/shop/subscriptions/{}",sub.get::<String,_>("id"))){(view::csrf(&s))(hidden("action","cancel"))(hidden("version",&sub.get::<i64,_>("version").to_string()))button class="quiet"{"Cancel future billing"}}}@if sub.get::<String,_>("state")=="active"&&sub.get::<String,_>("provider")=="offline"{details{summary{"Change plan"}form method="post" action=(format!("/shop/subscriptions/{}",sub.get::<String,_>("id"))){(view::csrf(&s))(hidden("action","change"))(hidden("version",&sub.get::<i64,_>("version").to_string()))(input("variant_id","New plan variant ID","","text"))p{"Same access key and interval. Upgrades require a prorated payment; downgrades apply next period."}button{"Review plan change"}}}}}}}@if subs.len()>40{a href=(format!("/shop/subscriptions?after={}",subs[39].get::<String,_>("id"))){"Next subscriptions"}}}).await
}
async fn subscription_action(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<Fields>,
) -> Result<Redirect> {
    let s = session(&app, &h).await?;
    auth::csrf(&s, field(&f, "csrf"))?;
    match field(&f, "action") {
        "cancel" => {
            let provider: Option<String> = sqlx::query_scalar(
                "SELECT provider FROM shop_subscriptions WHERE id=$1 AND user_id=$2",
            )
            .bind(&id)
            .bind(&s.user.id)
            .fetch_optional(&app.db.pool)
            .await?;
            if provider.as_deref() == Some("stripe") {
                payments::cancel_subscription(&app, &s, &id, number(&f, "version")?).await?;
            } else {
                billing::cancel(&app, &s, &id, number(&f, "version")?).await?;
            }
        }
        "change" => {
            if let Some(order) = billing::change(
                &app,
                &s,
                &id,
                number(&f, "version")?,
                field(&f, "variant_id"),
            )
            .await?
            {
                return Ok(Redirect::to(&format!("/shop/orders/{order}")));
            }
        }
        _ => return Err(Error::invalid("Choose a subscription action.")),
    }
    Ok(Redirect::to("/shop/subscriptions"))
}
async fn webhook(State(app): State<App>, h: HeaderMap, raw: Bytes) -> Result<Response> {
    let signature = h
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(Error::forbidden)?;
    payments::receive(&app, signature, &raw).await?;
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}
