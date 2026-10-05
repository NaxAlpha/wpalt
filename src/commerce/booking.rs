use super::*;
use sqlx::{Any, Row, Transaction};
pub async fn resource(app: &App, s: &Session, title: &str, staff: &str) -> Result<String> {
    text(title, 160)?;
    let _guard = app.mutation().await?;
    owner(app, s).await?;
    if !staff.is_empty() {
        crate::membership::uuid(staff)?;
        let role: Option<String> = sqlx::query_scalar("SELECT role FROM users WHERE id=$1")
            .bind(staff)
            .fetch_optional(&app.db.pool)
            .await?;
        if !matches!(role.as_deref(), Some("admin" | "editor")) {
            return Err(Error::invalid("Choose an active staff account."));
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO shop_resources(id,title,staff_id,created_at) VALUES($1,$2,$3,$4)")
        .bind(&id)
        .bind(title.trim())
        .bind(staff)
        .bind(crate::now())
        .execute(&app.db.pool)
        .await?;
    Ok(id)
}
#[derive(Clone, Serialize, Deserialize)]
pub struct SlotInput {
    pub resource_id: String,
    pub variant_id: String,
    pub starts_at: i64,
    pub ends_at: i64,
    pub capacity: i64,
}
pub async fn slot(app: &App, s: &Session, input: &SlotInput) -> Result<String> {
    let now = crate::now();
    if input.starts_at <= now
        || input.starts_at > now + 31536000
        || input.ends_at <= input.starts_at
        || input.ends_at - input.starts_at > 86400
        || !(1..=1000).contains(&input.capacity)
    {
        return Err(Error::invalid(
            "Choose a future UTC slot up to one day long and a supported capacity.",
        ));
    }
    let _guard = app.mutation().await?;
    owner(app, s).await?;
    let mut tx = app.db.pool.begin().await?;
    let staff: Option<String> =
        sqlx::query_scalar("SELECT staff_id FROM shop_resources WHERE id=$1 AND active=1")
            .bind(&input.resource_id)
            .fetch_optional(&mut *tx)
            .await?;
    let staff = staff.ok_or_else(Error::not_found)?;
    if !staff.is_empty() {
        let valid: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE id=$1 AND role IN ('admin','editor')",
        )
        .bind(&staff)
        .fetch_one(&mut *tx)
        .await?;
        if valid != 1 {
            return Err(Error::invalid(
                "The assigned staff account is no longer active.",
            ));
        }
    }
    let kind:Option<String>=sqlx::query_scalar("SELECT p.kind FROM shop_variants v JOIN shop_products p ON p.id=v.product_id WHERE v.id=$1 AND v.active=1 AND p.published=1").bind(&input.variant_id).fetch_optional(&mut *tx).await?;
    if kind.as_deref() != Some("booking") {
        return Err(Error::invalid(
            "Slots need an active published booking variant.",
        ));
    }
    let overlap:i64=sqlx::query_scalar("SELECT COUNT(*) FROM shop_slots s JOIN shop_resources r ON r.id=s.resource_id WHERE (s.active=1 OR s.held+s.booked>0) AND s.starts_at<$1 AND s.ends_at>$2 AND (s.resource_id=$3 OR ($4<>'' AND r.staff_id=$4))").bind(input.ends_at).bind(input.starts_at).bind(&input.resource_id).bind(&staff).fetch_one(&mut *tx).await?;
    if overlap != 0 {
        return Err(Error::invalid(
            "This resource or staff member already has an overlapping slot.",
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO shop_slots(id,resource_id,variant_id,starts_at,ends_at,capacity) VALUES($1,$2,$3,$4,$5,$6)").bind(&id).bind(&input.resource_id).bind(&input.variant_id).bind(input.starts_at).bind(input.ends_at).bind(input.capacity).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(id)
}
pub async fn edit_slot(
    app: &App,
    s: &Session,
    id: &str,
    version: i64,
    capacity: i64,
    active: bool,
) -> Result<()> {
    if !(1..=1000).contains(&capacity) {
        return Err(Error::invalid(
            "Capacity must be between one and one thousand.",
        ));
    }
    let _guard = app.mutation().await?;
    owner(app, s).await?;
    if sqlx::query("UPDATE shop_slots SET capacity=$1,active=$2,version=version+1 WHERE id=$3 AND version=$4 AND held+booked<=$1").bind(capacity).bind(i64::from(active)).bind(id).bind(version).execute(&app.db.pool).await?.rows_affected()!=1{return Err(Error::conflict())}
    Ok(())
}
pub async fn schedule(
    tx: &mut Transaction<'_, Any>,
    order: &str,
    slot: &str,
    starts: i64,
) -> Result<()> {
    for (kind, due) in [("confirmation", crate::now()), ("reminder", starts - 86400)]
        .into_iter()
        .filter(|(kind, due)| *kind != "reminder" || *due > crate::now())
    {
        sqlx::query("INSERT INTO shop_notifications(id,order_id,kind,slot_id,due_at,created_at) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(order_id,kind,slot_id) DO NOTHING").bind(uuid::Uuid::new_v4().to_string()).bind(order).bind(kind).bind(slot).bind(due).bind(crate::now()).execute(&mut **tx).await?;
    }
    Ok(())
}
pub async fn cancel_messages(tx: &mut Transaction<'_, Any>, order: &str) -> Result<()> {
    sqlx::query("UPDATE mail_jobs SET state='cancelled' WHERE dedupe IN (SELECT 'commerce:' || id FROM shop_notifications WHERE order_id=$1) AND state IN ('pending','retry')").bind(order).execute(&mut **tx).await?;
    sqlx::query("UPDATE shop_notifications SET state='cancelled' WHERE order_id=$1 AND kind IN ('confirmation','reminder')").bind(order).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO shop_notifications(id,order_id,kind,due_at,created_at) VALUES($1,$2,'cancellation',$3,$3) ON CONFLICT(order_id,kind,slot_id) DO NOTHING").bind(uuid::Uuid::new_v4().to_string()).bind(order).bind(crate::now()).execute(&mut **tx).await?;
    Ok(())
}
pub async fn notify(app: &App) -> Result<usize> {
    if !app.config.business_enabled || !app.config.mail.enabled {
        return Ok(0);
    }
    let _guard = app.mutation().await?;
    let mut tx = app.db.pool.begin().await?;
    let rows=sqlx::query("SELECT n.id,n.order_id,n.kind,n.slot_id,o.customer_email,o.customer_name,o.payment_state,s.starts_at,s.ends_at,r.title AS resource FROM shop_notifications n JOIN shop_orders o ON o.id=n.order_id LEFT JOIN shop_slots s ON s.id=n.slot_id LEFT JOIN shop_resources r ON r.id=s.resource_id WHERE n.state='pending' AND n.due_at<=$1 ORDER BY n.due_at,n.id LIMIT 40").bind(crate::now()).fetch_all(&mut *tx).await?;
    let count = rows.len();
    for row in rows {
        let id: String = row.get("id");
        let order: String = row.get("order_id");
        let kind: String = row.get("kind");
        let payment: String = row.get("payment_state");
        if kind == "reminder"
            && (!["paid", "partially_refunded"].contains(&payment.as_str())
                || row
                    .try_get::<i64, _>("starts_at")
                    .is_ok_and(|t| t <= crate::now()))
        {
            sqlx::query("UPDATE shop_notifications SET state='cancelled' WHERE id=$1")
                .bind(&id)
                .execute(&mut *tx)
                .await?;
            continue;
        }
        let title = match kind.as_str() {
            "reminder" => "Reservation reminder",
            "cancellation" => "Order cancellation",
            "billing" => "Subscription invoice ready",
            _ => "Order confirmation",
        };
        let url = format!(
            "{}/shop/orders/{order}",
            app.config.base_url.trim_end_matches('/')
        );
        let details = if row.get::<String, _>("slot_id").is_empty() {
            String::new()
        } else {
            format!(
                "{} · {} UTC",
                row.get::<String, _>("resource"),
                super::billing::utc(row.get("starts_at"))
            )
        };
        let plain = format!("{title}\n{details}\nReview your private order: {url}");
        let body = maud::html! {p {(title)}p {(&details)}p {a href=(&url){"Review your order"}}}
            .into_string();
        let message = crate::business::mail::enqueue(
            app,
            &mut tx,
            crate::business::mail::MessageInput {
                dedupe: &format!("commerce:{id}"),
                contact: "",
                list: "",
                kind: "notification",
                recipient: &row.get::<String, _>("customer_email"),
                subject: title,
                html: &body,
                plain: &plain,
            },
        )
        .await?;
        sqlx::query("UPDATE shop_notifications SET state='queued',message_id=$1 WHERE id=$2")
            .bind(message)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(count)
}

pub async fn edit_resource(
    app: &App,
    s: &Session,
    id: &str,
    version: i64,
    title: &str,
    staff: &str,
    active: bool,
) -> Result<()> {
    text(title, 160)?;
    if !staff.is_empty() {
        crate::membership::uuid(staff)?;
    }
    let _guard = app.mutation().await?;
    owner(app, s).await?;
    let mut tx = app.db.pool.begin().await?;
    if !staff.is_empty() {
        let role: Option<String> = sqlx::query_scalar("SELECT role FROM users WHERE id=$1")
            .bind(staff)
            .fetch_optional(&mut *tx)
            .await?;
        if !matches!(role.as_deref(), Some("admin" | "editor")) {
            return Err(Error::invalid("Choose an active staff account."));
        }
        let overlap:i64=sqlx::query_scalar("SELECT COUNT(*) FROM shop_slots a JOIN shop_slots b ON a.starts_at<b.ends_at AND a.ends_at>b.starts_at JOIN shop_resources r ON r.id=b.resource_id WHERE a.resource_id=$1 AND b.resource_id<>$1 AND r.staff_id=$2 AND (a.active=1 OR a.held+a.booked>0) AND (b.active=1 OR b.held+b.booked>0)").bind(id).bind(staff).fetch_one(&mut *tx).await?;
        if overlap != 0 {
            return Err(Error::invalid(
                "The new staff assignment would overlap existing reservations.",
            ));
        }
    }
    if sqlx::query("UPDATE shop_resources SET title=$1,staff_id=$2,active=$3,version=version+1 WHERE id=$4 AND version=$5").bind(title.trim()).bind(staff).bind(i64::from(active)).bind(id).bind(version).execute(&mut *tx).await?.rows_affected()!=1{return Err(Error::conflict())}
    tx.commit().await?;
    Ok(())
}
