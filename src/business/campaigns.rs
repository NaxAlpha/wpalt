//! Bounded, durable audience expansion with canonical content shared with posts.
use super::mail::{self, MessageInput};
use crate::{
    App,
    document::Document,
    error::{Error, Result},
    now,
};
use sqlx::Row;
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS business_campaigns(id TEXT PRIMARY KEY,title TEXT NOT NULL,subject TEXT NOT NULL,document TEXT NOT NULL,segment TEXT NOT NULL DEFAULT '{}',trigger_kind TEXT NOT NULL DEFAULT '',list_id TEXT NOT NULL REFERENCES audience_lists(id),state TEXT NOT NULL CHECK(state IN ('draft','scheduled','expanding','complete','cancelled')),version BIGINT NOT NULL DEFAULT 1,send_at BIGINT NOT NULL DEFAULT 0,cutoff BIGINT NOT NULL DEFAULT 0,cursor TEXT NOT NULL DEFAULT '',created_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS campaign_due ON business_campaigns(state,send_at,id);
CREATE TABLE IF NOT EXISTS audience_withdrawal_tokens(hash TEXT PRIMARY KEY,contact_id TEXT NOT NULL,list_id TEXT NOT NULL,created_at BIGINT NOT NULL,FOREIGN KEY(contact_id,list_id) REFERENCES audience_memberships(contact_id,list_id) ON DELETE CASCADE);
"#;
pub async fn create(app: &App, title: &str, list: &str) -> Result<String> {
    if title.trim().is_empty() || title.len() > 160 {
        return Err(Error::invalid(
            "A campaign needs a title up to 160 characters.",
        ));
    }
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM audience_lists WHERE id=$1")
        .bind(list)
        .fetch_optional(&app.db.pool)
        .await?;
    exists.ok_or_else(Error::not_found)?;
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO business_campaigns(id,title,subject,document,list_id,state,created_at) VALUES($1,$2,$2,$3,$4,'draft',$5)").bind(&id).bind(title).bind(crate::document::empty()).bind(list).bind(now()).execute(&app.db.pool).await?;
    Ok(id)
}
pub async fn save(
    app: &App,
    id: &str,
    version: i64,
    subject: &str,
    document: &str,
    send_at: i64,
    segment: super::audience::Segment,
) -> Result<()> {
    save_with_trigger(app, id, version, subject, document, send_at, segment, None).await
}
#[allow(clippy::too_many_arguments)] // Shared save operation carries an optional owner-selected trigger.
pub async fn save_with_trigger(
    app: &App,
    id: &str,
    version: i64,
    subject: &str,
    document: &str,
    send_at: i64,
    segment: super::audience::Segment,
    trigger: Option<bool>,
) -> Result<()> {
    if subject.trim().is_empty()
        || subject.len() > 200
        || subject.contains(['\r', '\n'])
        || send_at < 0
    {
        return Err(Error::invalid(
            "Use a subject up to 200 characters and a valid schedule.",
        ));
    }
    segment.validate()?;
    let segment =
        serde_json::to_string(&segment).map_err(|_| Error::invalid("Invalid segment."))?;
    let doc = Document::parse(document)?;
    let mut tx = app.db.pool.begin().await?;
    sqlx::query("UPDATE audience_lists SET policy=policy WHERE id=(SELECT list_id FROM business_campaigns WHERE id=$1)").bind(id).execute(&mut *tx).await?;
    let old: Option<String> =
        sqlx::query_scalar("SELECT trigger_kind FROM business_campaigns WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let trigger_kind = trigger
        .map(|value| if value { "confirmation" } else { "" })
        .map(str::to_owned)
        .unwrap_or(old.ok_or_else(Error::not_found)?);
    if trigger_kind == "confirmation" {
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM business_campaigns WHERE trigger_kind='confirmation' AND id<>$1 AND list_id=(SELECT list_id FROM business_campaigns WHERE id=$1)").bind(id).fetch_one(&mut *tx).await?;
        if count >= 16 {
            return Err(Error::invalid(
                "At most sixteen confirmation templates per list are supported.",
            ));
        }
    }
    let result=sqlx::query("UPDATE business_campaigns SET subject=$1,document=$2,send_at=$3,state=$4,segment=$7,trigger_kind=$8,version=version+1 WHERE id=$5 AND version=$6 AND state IN ('draft','scheduled')").bind(subject).bind(doc.encode()).bind(send_at).bind(if trigger_kind=="confirmation"{"draft"}else if send_at>0{"scheduled"}else{"draft"}).bind(id).bind(version).bind(segment).bind(trigger_kind).execute(&mut *tx).await?;
    if result.rows_affected() != 1 {
        return Err(Error::conflict());
    }
    tx.commit().await?;
    Ok(())
}
pub async fn tick(app: &App) -> Result<usize> {
    if !app.config.business_enabled || !app.config.mail.enabled {
        return Ok(0);
    }
    let mut tx = app.db.pool.begin().await?;
    let sql = if app.db.postgres {
        "UPDATE business_campaigns SET state='expanding',cutoff=CASE WHEN cutoff=0 THEN $1 ELSE cutoff END WHERE id=(SELECT id FROM business_campaigns WHERE state='expanding' OR (state='scheduled' AND send_at<=$1) ORDER BY send_at,id LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING *"
    } else {
        "UPDATE business_campaigns SET state='expanding',cutoff=CASE WHEN cutoff=0 THEN $1 ELSE cutoff END WHERE id=(SELECT id FROM business_campaigns WHERE state='expanding' OR (state='scheduled' AND send_at<=$1) ORDER BY send_at,id LIMIT 1) RETURNING *"
    };
    let Some(campaign) = sqlx::query(sql)
        .bind(now())
        .fetch_optional(&mut *tx)
        .await?
    else {
        return Ok(0);
    };
    let id: String = campaign.get("id");
    let list: String = campaign.get("list_id");
    let doc = Document::parse(&campaign.get::<String, _>("document"))?;
    let rows=sqlx::query("SELECT c.id,c.email,c.attributes,l.purpose FROM audience_memberships m JOIN audience_contacts c ON c.id=m.contact_id JOIN audience_lists l ON l.id=m.list_id WHERE m.list_id=$1 AND m.state='confirmed' AND m.policy=l.policy AND c.suppressed=0 AND m.confirmed_at<=$2 AND c.id>$3 ORDER BY c.id LIMIT 50")
 .bind(&list).bind(campaign.get::<i64,_>("cutoff")).bind(campaign.get::<String,_>("cursor")).fetch_all(&mut *tx).await?;
    let segment: super::audience::Segment =
        serde_json::from_str(&campaign.get::<String, _>("segment"))
            .map_err(|_| Error::invalid("Stored segment needs repair."))?;
    segment.validate()?;
    let mut queued = 0;
    for row in &rows {
        let attributes: super::audience::Attributes =
            serde_json::from_str(&row.get::<String, _>("attributes"))
                .map_err(|_| Error::invalid("Stored contact attributes need repair."))?;
        if !segment.matches(&attributes) {
            continue;
        }
        queued += 1;
        let contact: String = row.get("id");
        let token = crate::auth::random_token();
        sqlx::query("INSERT INTO audience_withdrawal_tokens(hash,contact_id,list_id,created_at) VALUES($1,$2,$3,$4)").bind(crate::auth::digest(token.as_bytes())).bind(&contact).bind(&list).bind(now()).execute(&mut *tx).await?;
        let url = format!(
            "{}/audience/withdraw/{token}",
            app.config.base_url.trim_end_matches('/')
        );
        let footer=maud::html!{hr; p {(row.get::<String,_>("purpose"))} p {a href=(&url){"Withdraw subscription"}}}.into_string();
        let html = format!("{}{}", doc.mail_html(&app.config.base_url), footer);
        let plain = format!("{}\nWithdraw subscription: {url}", doc.markdown());
        mail::enqueue(
            app,
            &mut tx,
            MessageInput {
                dedupe: &format!("campaign:{id}:{contact}"),
                contact: &contact,
                list: &list,
                kind: "campaign",
                recipient: &row.get::<String, _>("email"),
                subject: &campaign.get::<String, _>("subject"),
                html: &html,
                plain: &plain,
            },
        )
        .await?;
    }
    let cursor = rows
        .last()
        .map(|r| r.get::<String, _>("id"))
        .unwrap_or_else(|| campaign.get("cursor"));
    sqlx::query("UPDATE business_campaigns SET cursor=$1,state=$2 WHERE id=$3")
        .bind(cursor)
        .bind(if rows.len() < 50 {
            "complete"
        } else {
            "expanding"
        })
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(queued)
}
