use super::*;
use sqlx::Row;
#[derive(Clone, Serialize, Deserialize)]
pub struct ProductInput {
    pub slug: String,
    pub title: String,
    pub description: String,
    pub kind: String,
    pub entitlement: String,
    pub access_seconds: i64,
    pub download_id: String,
    pub published: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct VariantInput {
    pub title: String,
    pub sku: String,
    pub price_minor: i64,
    pub member_price_minor: i64,
    pub member_key: String,
    pub stock_total: i64,
    pub billing_interval: String,
    pub active: bool,
}
pub fn key(s: &str) -> Result<()> {
    if s.len() > 80
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        Err(Error::invalid("Use a short entitlement key."))
    } else {
        Ok(())
    }
}
pub fn product_valid(p: &ProductInput) -> Result<()> {
    text(&p.slug, 120)?;
    text(&p.title, 160)?;
    if !p
        .slug
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || p.description.len() > 16000
        || !["physical", "digital", "membership", "booking"].contains(&p.kind.as_str())
        || !(0..=315360000).contains(&p.access_seconds)
    {
        return Err(Error::invalid("Check product fields."));
    }
    key(&p.entitlement)?;
    if p.kind == "membership" && p.entitlement.is_empty() {
        return Err(Error::invalid(
            "Membership products need an entitlement key.",
        ));
    }
    if !p.download_id.is_empty() {
        crate::membership::uuid(&p.download_id)?;
        if p.kind != "digital" {
            return Err(Error::invalid(
                "Only digital products have a protected download.",
            ));
        }
    }
    Ok(())
}
pub async fn save_product(
    app: &App,
    s: &Session,
    id: Option<&str>,
    version: i64,
    p: &ProductInput,
) -> Result<String> {
    product_valid(p)?;
    let _guard = app.mutation().await?;
    owner(app, s).await?;
    let id = id
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    crate::membership::uuid(&id)?;
    let entitlement = if p.kind == "digital" && p.entitlement.is_empty() {
        format!("product_{}", id)
    } else {
        p.entitlement.clone()
    };
    let mut tx = app.db.pool.begin().await?;
    if version == 0 {
        sqlx::query("INSERT INTO shop_products(id,slug,title,description,kind,entitlement,access_seconds,download_id,published,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(&id).bind(&p.slug).bind(p.title.trim()).bind(&p.description).bind(&p.kind).bind(&entitlement).bind(p.access_seconds).bind(&p.download_id).bind(i64::from(p.published)).bind(crate::now()).execute(&mut *tx).await?;
    } else {
        let previous =
            sqlx::query("SELECT kind,entitlement,download_id FROM shop_products WHERE id=$1")
                .bind(&id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(Error::not_found)?;
        if previous.get::<String, _>("kind") != p.kind
            || previous.get::<String, _>("entitlement") != entitlement
            || previous.get::<String, _>("download_id") != p.download_id
        {
            return Err(Error::invalid(
                "Create a new product to change its type, access key or protected file. Existing financial records retain their meaning.",
            ));
        }
        if sqlx::query("UPDATE shop_products SET slug=$1,title=$2,description=$3,access_seconds=$4,published=$5,version=version+1 WHERE id=$6 AND version=$7").bind(&p.slug).bind(p.title.trim()).bind(&p.description).bind(p.access_seconds).bind(i64::from(p.published)).bind(&id).bind(version).execute(&mut *tx).await?.rows_affected()!=1{return Err(Error::conflict())}
    }
    if !p.download_id.is_empty() {
        let private: Option<String> =
            sqlx::query_scalar("SELECT visibility FROM media WHERE id=$1")
                .bind(&p.download_id)
                .fetch_optional(&mut *tx)
                .await?;
        if private.as_deref() != Some("private") {
            return Err(Error::invalid(
                "Choose an existing private file; public assets cannot become paid downloads implicitly.",
            ));
        }
        let rule=sqlx::query("SELECT r.course_id,p.entitlement,p.group_id FROM member_resources r JOIN member_policies p ON p.id=r.policy_id WHERE r.kind='media' AND r.resource_id=$1").bind(&p.download_id).fetch_optional(&mut *tx).await?;
        if let Some(rule) = rule {
            if rule.get::<String, _>("entitlement") != entitlement
                || !rule.get::<String, _>("group_id").is_empty()
                || !rule.get::<String, _>("course_id").is_empty()
            {
                return Err(Error::invalid(
                    "This file already belongs to another access policy or course.",
                ));
            }
        } else {
            let policy = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO member_policies(id,title,entitlement,group_id) VALUES($1,$2,$3,'')",
            )
            .bind(&policy)
            .bind(&p.title)
            .bind(&entitlement)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO member_resources(kind,resource_id,policy_id) VALUES('media',$1,$2)",
            )
            .bind(&p.download_id)
            .bind(&policy)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    tracing::info!(event="commerce_product_saved",product_id=%id);
    Ok(id)
}
pub async fn save_variant(
    app: &App,
    s: &Session,
    product: &str,
    id: Option<&str>,
    version: i64,
    v: &VariantInput,
) -> Result<String> {
    text(&v.title, 120)?;
    text(&v.sku, 80)?;
    money(v.price_minor)?;
    if v.member_price_minor < -1 || v.member_price_minor > v.price_minor {
        return Err(Error::invalid(
            "Member price must not exceed the ordinary price and needs an access key.",
        ));
    }
    key(&v.member_key)?;
    if v.member_price_minor >= 0 && v.member_key.is_empty() {
        return Err(Error::invalid("Member pricing needs an entitlement key."));
    }
    if !["", "day", "week", "month", "year"].contains(&v.billing_interval.as_str())
        || !(-1..=1_000_000_000).contains(&v.stock_total)
    {
        return Err(Error::invalid("Check stock or billing interval."));
    }
    let _guard = app.mutation().await?;
    owner(app, s).await?;
    let mut tx = app.db.pool.begin().await?;
    let kind: String = sqlx::query_scalar("SELECT kind FROM shop_products WHERE id=$1")
        .bind(product)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(Error::not_found)?;
    if kind == "physical" && v.stock_total < 0
        || kind != "physical" && v.stock_total != -1
        || !v.billing_interval.is_empty()
            && (kind != "membership" || v.price_minor == 0 || v.member_price_minor >= 0)
    {
        return Err(Error::invalid(
            "Physical stock is finite; recurring billing is a fixed-price membership.",
        ));
    }
    let id = id
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    crate::membership::uuid(&id)?;
    if version == 0 {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM shop_variants WHERE product_id=$1")
                .bind(product)
                .fetch_one(&mut *tx)
                .await?;
        if count >= 100 {
            return Err(Error::invalid(
                "A product supports at most 100 variants; create a separate product for another collection.",
            ));
        }
        sqlx::query("INSERT INTO shop_variants(id,product_id,title,sku,price_minor,member_price_minor,member_key,stock_total,billing_interval,active) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(&id).bind(product).bind(v.title.trim()).bind(&v.sku).bind(v.price_minor).bind(v.member_price_minor).bind(&v.member_key).bind(v.stock_total).bind(&v.billing_interval).bind(i64::from(v.active)).execute(&mut *tx).await?;
    } else {
        let interval: String = sqlx::query_scalar(
            "SELECT billing_interval FROM shop_variants WHERE id=$1 AND product_id=$2",
        )
        .bind(&id)
        .bind(product)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(Error::not_found)?;
        if interval != v.billing_interval {
            return Err(Error::invalid(
                "Create a new variant to change its billing interval.",
            ));
        }
        if sqlx::query("UPDATE shop_variants SET title=$1,sku=$2,price_minor=$3,member_price_minor=$4,member_key=$5,stock_total=$6,active=$7,version=version+1 WHERE id=$8 AND product_id=$9 AND version=$10 AND (stock_total=-1 OR held+sold<=$6)").bind(v.title.trim()).bind(&v.sku).bind(v.price_minor).bind(v.member_price_minor).bind(&v.member_key).bind(v.stock_total).bind(i64::from(v.active)).bind(&id).bind(product).bind(version).execute(&mut *tx).await?.rows_affected()!=1{return Err(Error::conflict())}
    }
    tx.commit().await?;
    Ok(id)
}
#[derive(Clone, Serialize, Deserialize)]
pub struct DiscountInput {
    pub code: String,
    pub title: String,
    pub bps: i64,
    pub starts_at: i64,
    pub expires_at: i64,
    pub max_uses: i64,
    pub member_key: String,
    pub product_id: String,
    pub reward_id: String,
    pub active: bool,
}
pub async fn discount(app: &App, s: &Session, d: &DiscountInput, version: i64) -> Result<()> {
    text(&d.code, 40)?;
    text(&d.title, 160)?;
    key(&d.member_key)?;
    if !d
        .code
        .bytes()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
        || !(1..=10000).contains(&d.bps)
        || d.starts_at < 0
        || d.expires_at <= d.starts_at
        || !(0..=1_000_000_000).contains(&d.max_uses)
    {
        return Err(Error::invalid(
            "Check coupon code, dates, percentage and use limit.",
        ));
    }
    let _guard = app.mutation().await?;
    owner(app, s).await?;
    let mut tx = app.db.pool.begin().await?;
    for (id, table) in [
        (&d.product_id, "shop_products"),
        (&d.reward_id, "promotion_rewards"),
    ] {
        if !id.is_empty() {
            crate::membership::uuid(id)?;
            let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE id=$1"))
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
            if n != 1 {
                return Err(Error::not_found());
            }
        }
    }
    if version==0{sqlx::query("INSERT INTO shop_discounts(code,title,bps,starts_at,expires_at,max_uses,member_key,product_id,reward_id,active) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(&d.code).bind(&d.title).bind(d.bps).bind(d.starts_at).bind(d.expires_at).bind(d.max_uses).bind(&d.member_key).bind(&d.product_id).bind(&d.reward_id).bind(i64::from(d.active)).execute(&mut *tx).await?;}else if sqlx::query("UPDATE shop_discounts SET title=$1,bps=$2,starts_at=$3,expires_at=$4,max_uses=$5,member_key=$6,product_id=$7,reward_id=$8,active=$9,version=version+1 WHERE code=$10 AND version=$11 AND ($5=0 OR held+used<=$5)").bind(&d.title).bind(d.bps).bind(d.starts_at).bind(d.expires_at).bind(d.max_uses).bind(&d.member_key).bind(&d.product_id).bind(&d.reward_id).bind(i64::from(d.active)).bind(&d.code).bind(version).execute(&mut *tx).await?.rows_affected()!=1{return Err(Error::conflict())}
    tx.commit().await?;
    Ok(())
}
pub async fn set_rules(
    app: &App,
    s: &Session,
    version: i64,
    tax_bps: i64,
    shipping_minor: i64,
    tax_shipping: bool,
) -> Result<()> {
    money(shipping_minor)?;
    basis(0, tax_bps)?;
    let _guard = app.mutation().await?;
    owner(app, s).await?;
    if sqlx::query("UPDATE shop_settings SET tax_bps=$1,shipping_minor=$2,tax_shipping=$3,version=version+1 WHERE id=1 AND version=$4").bind(tax_bps).bind(shipping_minor).bind(i64::from(tax_shipping)).bind(version).execute(&app.db.pool).await?.rows_affected()!=1{return Err(Error::conflict())}
    Ok(())
}
