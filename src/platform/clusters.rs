//! Explicit supplemental definitions, never inferred consent, access or settlement.
use crate::error::{Error, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub const MAX_BYTES: usize = 2 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    format: String,
    source_site: String,
    #[serde(default)]
    mailpoet: Option<Audience>,
    #[serde(default)]
    pmpro: Option<Membership>,
    #[serde(default)]
    sensei: Option<Learning>,
    #[serde(default)]
    woocommerce: Option<Catalog>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Audience {
    version: String,
    subscribers: Vec<Subscriber>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Subscriber {
    id: String,
    email: String,
    name: String,
    status: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Membership {
    version: String,
    levels: Vec<Level>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Level {
    id: String,
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Learning {
    version: String,
    courses: Vec<SourceCourse>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceCourse {
    id: String,
    title: String,
    lessons: Vec<SourceLesson>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceLesson {
    id: String,
    title: String,
    content: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    version: String,
    currency: String,
    products: Vec<Product>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Product {
    id: String,
    name: String,
    slug: String,
    description: String,
    sku: String,
    regular_price: String,
    stock_quantity: Option<i64>,
    product_type: String,
    virtual_product: bool,
    downloadable: bool,
    backorders: String,
}
fn bad() -> Error {
    Error::invalid(
        "Review the bounded, explicit plugin-cluster export; no consent, grants or settlement are inferred.",
    )
}
fn version(v: &str) -> Result<()> {
    if v.is_empty()
        || v.len() > 50
        || !v
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    {
        return Err(bad());
    }
    Ok(())
}
fn identity(v: &str, ids: &mut BTreeSet<String>) -> Result<()> {
    if v.is_empty()
        || v.len() > 20
        || !v.bytes().all(|b| b.is_ascii_digit())
        || v.starts_with('0')
        || !ids.insert(v.to_owned())
    {
        return Err(bad());
    }
    Ok(())
}
fn title(v: &str, max: usize) -> Result<()> {
    if v.trim().is_empty() || v.len() > max || v.chars().any(char::is_control) {
        return Err(bad());
    }
    Ok(())
}
fn id(hash: &str, kind: &str, key: &str) -> String {
    super::wordpress::stable_id(hash, kind, key)
}
fn rows(t: &mut serde_json::Map<String, Value>, name: &str, values: Vec<Value>) {
    t.insert(name.into(), values.into());
}
/// Prices are decimal text; no float or rounding of source money.
fn price(v: &str, currency: &str) -> Result<i64> {
    let places = if currency == "JPY" { 0 } else { 2 };
    let mut p = v.split('.');
    let whole = p.next().ok_or_else(bad)?;
    let fraction = p.next().unwrap_or("");
    if p.next().is_some()
        || whole.is_empty()
        || whole.len() > 14
        || fraction.len() > places
        || v.ends_with('.')
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|c| c.is_ascii_digit())
    {
        return Err(bad());
    }
    let scale = if places == 0 { 1 } else { 100 };
    let amount = whole
        .parse::<i64>()
        .map_err(|_| bad())?
        .checked_mul(scale)
        .ok_or_else(bad)?;
    let minor = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<i64>().map_err(|_| bad())? * if fraction.len() == 1 { 10 } else { 1 }
    };
    let total = amount.checked_add(minor).ok_or_else(bad)?;
    crate::commerce::money(total)?;
    Ok(total)
}
pub fn project(
    config: &crate::config::Config,
    bytes: &[u8],
    origin: &str,
    owner: &str,
    t: &mut serde_json::Map<String, Value>,
) -> Result<Value> {
    if bytes.len() > MAX_BYTES {
        return Err(bad());
    }
    let source: Source = serde_json::from_slice(bytes).map_err(|_| bad())?;
    let url = url::Url::parse(&source.source_site).map_err(|_| bad())?;
    if source.format != "wpalt-plugin-clusters-v1"
        || !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || url.origin().ascii_serialization()
            != url::Url::parse(origin)
                .map_err(|_| bad())?
                .origin()
                .ascii_serialization()
    {
        return Err(bad());
    }
    let hash = crate::auth::digest(bytes);
    let mut report = json!({"source_sha256":hash,"versions":{},"counts":{},"unsupported":[],"boundary":"Selected definitions only. Contacts suppressed; policies disabled; courses and lesson posts draft; products unpublished and variants inactive. No source consent, memberships, progress, credentials, jobs, orders or payments imported."});
    if let Some(a) = source.mailpoet {
        version(&a.version)?;
        if !config.business_enabled
            || a.subscribers.len() > 10000
            || a.subscribers.len() as i64 > config.business_limits.contacts
        {
            return Err(bad());
        }
        let mut ids = BTreeSet::new();
        let mut emails = BTreeSet::new();
        let mut contacts = Vec::new();
        for s in a.subscribers {
            identity(&s.id, &mut ids)?;
            if s.name.len() > 100
                || !matches!(
                    s.status.as_str(),
                    "unconfirmed" | "subscribed" | "unsubscribed" | "bounced" | "inactive"
                )
            {
                return Err(bad());
            }
            let email = crate::business::mail::email(&s.email)?;
            if !emails.insert(email.clone()) {
                return Err(bad());
            }
            contacts.push(json!({"id":id(&hash,"mailpoet",&s.id),"email":email,"name":s.name,"attributes":"{}","version":1,"suppressed":1,"created_at":0}));
        }
        report["versions"]["mailpoet"] = a.version.into();
        report["counts"]["contacts"] = contacts.len().into();
        for r in t
            .get_mut("business_usage")
            .and_then(Value::as_array_mut)
            .ok_or_else(bad)?
        {
            if r["kind"] == "contacts" {
                r["items"] = contacts.len().into()
            }
        }
        rows(t, "audience_contacts", contacts);
    }
    // PMPro restrictions live in plugin tables, not necessarily WXR metadata.
    // Without a complete reviewed access mapping, hold all core publication and media.
    if source.pmpro.is_some() {
        for post in t
            .get_mut("posts")
            .and_then(Value::as_array_mut)
            .ok_or_else(bad)?
        {
            post["status"] = "draft".into();
            for key in ["published_slug", "published_title", "published_body"] {
                post[key] = "".into();
            }
            post["published_document"] = crate::document::empty().into();
            post["published_fields"] = "{}".into();
            post["published_blocks"] = "[]".into();
            post["published_seo"] = "{}".into();
            post["published_at"] = 0.into();
        }
        for media in t
            .get_mut("media")
            .and_then(Value::as_array_mut)
            .ok_or_else(bad)?
        {
            media["visibility"] = "private".into();
        }
        rows(t, "redirects", Vec::new());
        rows(t, "published_post_terms", Vec::new());
        rows(t, "comments", Vec::new());
        report["publication_hold"] = "PMPro access tables are outside WXR: all core content held as draft, media private; comments/redirects/public taxonomy withheld until explicit owner access review.".into();
    }
    let mut policies = Vec::new();
    if let Some(m) = source.pmpro {
        version(&m.version)?;
        if !config.membership_enabled || m.levels.len() > 128 {
            return Err(bad());
        }
        let mut ids = BTreeSet::new();
        for l in m.levels {
            identity(&l.id, &mut ids)?;
            title(&l.name, 160)?;
            policies.push(json!({"id":id(&hash,"pmpro",&l.id),"title":l.name,"entitlement":format!("pmpro_{}",l.id),"group_id":"","enabled":0,"version":1}));
        }
        report["versions"]["pmpro"] = m.version.into();
        report["counts"]["membership_policies"] = policies.len().into();
    }
    if let Some(l) = source.sensei {
        version(&l.version)?;
        if !config.membership_enabled || l.courses.len() > 100 {
            return Err(bad());
        }
        let mut course_ids = BTreeSet::new();
        let mut lesson_ids = BTreeSet::new();
        let mut courses = Vec::new();
        let mut resources = Vec::new();
        let mut posts = Vec::new();
        let existing: BTreeSet<String> = t["posts"]
            .as_array()
            .ok_or_else(bad)?
            .iter()
            .map(|r| r["slug"].as_str().unwrap_or("").into())
            .collect();
        for c in l.courses {
            identity(&c.id, &mut course_ids)?;
            title(&c.title, 160)?;
            if c.lessons.is_empty() || c.lessons.len() > 100 {
                return Err(bad());
            }
            let course_id = id(&hash, "sensei-course", &c.id);
            let policy_id = id(&hash, "sensei-policy", &c.id);
            policies.push(json!({"id":policy_id,"title":c.title,"entitlement":format!("sensei_{}",c.id),"group_id":"","enabled":0,"version":1}));
            let mut lessons = Vec::new();
            for source in c.lessons {
                identity(&source.id, &mut lesson_ids)?;
                title(&source.title, 160)?;
                let slug = format!("sensei-lesson-{}", source.id);
                if existing.contains(&slug) {
                    return Err(bad());
                }
                let post_id = id(&hash, "sensei-post", &source.id);
                let lesson_id = id(&hash, "sensei-lesson", &source.id);
                let doc = super::html::import(&source.content)?;
                let body = doc.markdown();
                let encoded = doc.encode();
                posts.push(json!({"id":post_id,"slug":slug,"kind":"post","title":source.title,"body":body,"document":encoded,"fields":"{}","blocks":"[]","status":"draft","version":1,"published_slug":"","published_title":"","published_body":"","published_document":crate::document::empty(),"published_fields":"{}","published_blocks":"[]","publish_at":0,"published_at":0,"updated_at":0,"author_id":owner,"locale":"en","translation_group":"","seo":"{}","published_locale":"en","published_translation_group":"","published_seo":"{}"}));
                lessons.push(json!({"id":lesson_id,"title":source.title,"post_id":post_id}));
                resources.push(json!({"kind":"post","resource_id":post_id,"policy_id":policy_id,"opens_at":0,"delay_seconds":0,"course_id":course_id,"lesson_id":lesson_id}));
            }
            let definition =
                json!({"title":c.title,"policy_id":policy_id,"sequential":true,"lessons":lessons});
            let native: crate::membership::Course =
                serde_json::from_value(definition.clone()).map_err(|_| bad())?;
            native.validate()?;
            courses.push(json!({"id":course_id,"title":c.title,"published_title":"","policy_id":policy_id,"draft":serde_json::to_string(&definition).map_err(|_|bad())?,"live":"","version":1,"published_version":0,"created_at":0}));
            resources.push(json!({"kind":"course","resource_id":course_id,"policy_id":policy_id,"opens_at":0,"delay_seconds":0,"course_id":"","lesson_id":""}));
        }
        if (courses.len() + resources.len() + policies.len()) as i64 > config.membership_max_records
        {
            return Err(bad());
        }
        report["versions"]["sensei"] = l.version.into();
        report["counts"]["courses"] = courses.len().into();
        report["counts"]["lessons"] = posts.len().into();
        t.get_mut("posts")
            .and_then(Value::as_array_mut)
            .ok_or_else(bad)?
            .extend(posts);
        rows(t, "member_courses", courses);
        rows(t, "member_resources", resources);
    }
    if policies.len() as i64 > config.membership_max_records {
        return Err(bad());
    }
    rows(t, "member_policies", policies);
    if let Some(c) = source.woocommerce {
        version(&c.version)?;
        if !config.commerce.enabled
            || c.currency != config.commerce.currency
            || c.products.len() > 1000
            || c.products.len() as i64 * 2 > config.commerce.max_records
        {
            return Err(bad());
        }
        let mut ids = BTreeSet::new();
        let mut slugs = BTreeSet::new();
        let mut skus = BTreeSet::new();
        let mut products = Vec::new();
        let mut variants = Vec::new();
        for p in c.products {
            identity(&p.id, &mut ids)?;
            if p.product_type != "simple"
                || p.virtual_product
                || p.downloadable
                || p.backorders != "no"
            {
                report["unsupported"].as_array_mut().unwrap().push(json!({"plugin":"woocommerce","source_id":p.id,"reason":"Only simple physical products without downloads/backorders are selected."}));
                continue;
            }
            title(&p.name, 120)?;
            if !crate::content::valid_slug(&p.slug)
                || !slugs.insert(p.slug.clone())
                || p.description.len() > 16000
                || p.sku.is_empty()
                || p.sku.len() > 80
                || !skus.insert(p.sku.clone())
                || p.stock_quantity.is_some_and(|s| s < 0)
            {
                return Err(bad());
            }
            let product_id = id(&hash, "woocommerce-product", &p.id);
            let amount = price(&p.regular_price, &c.currency)?;
            products.push(json!({"id":product_id,"slug":p.slug,"title":p.name,"description":p.description,"kind":"physical","entitlement":"","access_seconds":0,"download_id":"","published":0,"version":1,"created_at":0}));
            variants.push(json!({"id":id(&hash,"woocommerce-variant",&p.id),"product_id":product_id,"title":p.name,"sku":p.sku,"price_minor":amount,"member_price_minor":-1,"member_key":"","stock_total":p.stock_quantity.unwrap_or(-1),"held":0,"sold":0,"billing_interval":"","active":0,"version":1}));
        }
        report["versions"]["woocommerce"] = c.version.into();
        report["counts"]["products"] = products.len().into();
        rows(t, "shop_products", products);
        rows(t, "shop_variants", variants);
    }
    Ok(report)
}
