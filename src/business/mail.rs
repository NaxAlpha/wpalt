//! Durable local mail with conservative handling of uncertain SMTP outcomes.
use crate::{
    App, auth,
    error::{Error, Result},
    now,
};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
};
use serde::{Deserialize, Serialize};
use sqlx::{Any, Row, Transaction};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MailConfig {
    pub from: String,
    pub smtp: Vec<Connection>,
    pub enabled: bool,
    pub max_attempts: i64,
    pub batch_size: usize,
}
impl Default for MailConfig {
    fn default() -> Self {
        Self {
            from: "site@localhost.test".into(),
            smtp: vec![],
            enabled: true,
            max_attempts: 5,
            batch_size: 64,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub host: String,
    pub port: u16,
    pub tls: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}
pub fn email(value: &str) -> Result<String> {
    if !value.is_ascii() || value.len() > 254 || value.contains(['\r', '\n']) {
        return Err(Error::invalid(
            "Use a valid ASCII email address of at most 254 bytes.",
        ));
    }
    value
        .parse::<lettre::Address>()
        .map_err(|_| Error::invalid("Use a valid email address."))?;
    Ok(value.to_ascii_lowercase())
}
impl MailConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        email(&self.from).map_err(|_| anyhow::anyhow!("invalid mail.from"))?;
        anyhow::ensure!(
            self.smtp.len() <= 4
                && (1..=10).contains(&self.max_attempts)
                && (1..=128).contains(&self.batch_size),
            "Use at most four SMTP routes and 1..10 attempts."
        );
        for c in &self.smtp {
            anyhow::ensure!(
                !c.host.is_empty()
                    && c.host.len() <= 253
                    && c.port > 0
                    && ["starttls", "tls", "local"].contains(&c.tls.as_str())
                    && c.username.len() <= 254
                    && c.password.len() <= 1024,
                "Invalid SMTP connection."
            );
            if c.tls == "local" {
                anyhow::ensure!(
                    c.host
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback()),
                    "Plain SMTP requires an explicit loopback IP."
                );
            }
        }
        Ok(())
    }
}
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS mail_jobs(id TEXT PRIMARY KEY,dedupe TEXT NOT NULL UNIQUE,contact_id TEXT NOT NULL DEFAULT '',list_id TEXT NOT NULL DEFAULT '',kind TEXT NOT NULL,recipient TEXT NOT NULL,sender TEXT NOT NULL,subject TEXT NOT NULL,html TEXT NOT NULL,plain TEXT NOT NULL,message_id TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN ('pending','leased','retry','spooled','sent','uncertain','dead','cancelled')),attempts BIGINT NOT NULL DEFAULT 0,next_at BIGINT NOT NULL,lease_owner TEXT NOT NULL DEFAULT '',lease_until BIGINT NOT NULL DEFAULT 0,last_code TEXT NOT NULL DEFAULT '',created_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS mail_ready ON mail_jobs(next_at,id) WHERE state IN ('pending','retry');
CREATE INDEX IF NOT EXISTS mail_lease_expiry ON mail_jobs(lease_until,id) WHERE state='leased';
CREATE INDEX IF NOT EXISTS mail_contact ON mail_jobs(contact_id,state);
CREATE INDEX IF NOT EXISTS mail_terminal_retention ON mail_jobs(created_at,id) WHERE state IN ('sent','spooled','dead','cancelled');
CREATE TABLE IF NOT EXISTS mail_attempts(id TEXT PRIMARY KEY,job_id TEXT NOT NULL REFERENCES mail_jobs(id) ON DELETE CASCADE,outcome TEXT NOT NULL,created_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS mail_attempt_history ON mail_attempts(job_id,created_at DESC,id DESC);
"#;
#[derive(Clone)]
pub struct MessageInput<'a> {
    pub dedupe: &'a str,
    pub contact: &'a str,
    pub list: &'a str,
    pub kind: &'a str,
    pub recipient: &'a str,
    pub subject: &'a str,
    pub html: &'a str,
    pub plain: &'a str,
}
pub async fn enqueue(
    app: &App,
    tx: &mut Transaction<'_, Any>,
    input: MessageInput<'_>,
) -> Result<String> {
    let recipient = email(input.recipient)?;
    if input.subject.len() > 200
        || input.subject.contains(['\r', '\n'])
        || input.html.len() > 2 * 1024 * 1024
        || input.plain.len() > 512 * 1024
        || !["confirmation", "campaign", "notification"].contains(&input.kind)
    {
        return Err(Error::invalid("Invalid mail content."));
    }
    let bytes = (input.html.len() + input.plain.len() + input.subject.len()) as i64;
    // Serialize deduplication with admission; a replay still succeeds at capacity.
    super::quotas::release(tx, "mail", 0, 0).await?;
    if let Some(id) = sqlx::query_scalar("SELECT id FROM mail_jobs WHERE dedupe=$1")
        .bind(input.dedupe)
        .fetch_optional(&mut **tx)
        .await?
    {
        return Ok(id);
    }
    super::quotas::reserve(app, tx, "mail", 1, bytes).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let host = url::Url::parse(&app.config.base_url)
        .map_err(|_| Error::invalid("Invalid mail origin."))?
        .host_str()
        .unwrap_or("localhost")
        .to_owned();
    let message_id = format!("<{id}@{host}>");
    let inserted=sqlx::query("INSERT INTO mail_jobs(id,dedupe,contact_id,list_id,kind,recipient,sender,subject,html,plain,message_id,state,next_at,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'pending',$12,$12) ON CONFLICT(dedupe) DO NOTHING")
        .bind(&id).bind(input.dedupe).bind(input.contact).bind(input.list).bind(input.kind).bind(recipient).bind(&app.config.mail.from).bind(input.subject).bind(input.html).bind(input.plain).bind(message_id).bind(now()).execute(&mut **tx).await?;
    if inserted.rows_affected() == 0 {
        super::quotas::release(tx, "mail", 1, bytes).await?;
    }
    Ok(
        sqlx::query_scalar("SELECT id FROM mail_jobs WHERE dedupe=$1")
            .bind(input.dedupe)
            .fetch_one(&mut **tx)
            .await?,
    )
}
fn message(row: &sqlx::any::AnyRow) -> Result<Message> {
    let sender: Mailbox = row
        .get::<String, _>("sender")
        .parse()
        .map_err(|_| Error::invalid("Invalid stored sender."))?;
    let recipient: Mailbox = row
        .get::<String, _>("recipient")
        .parse()
        .map_err(|_| Error::invalid("Invalid stored recipient."))?;
    Message::builder()
        .from(sender)
        .to(recipient)
        .subject(row.get::<String, _>("subject"))
        .message_id(Some(row.get("message_id")))
        .date(
            std::time::UNIX_EPOCH
                + std::time::Duration::from_secs(row.get::<i64, _>("created_at").max(0) as u64),
        )
        .multipart(
            MultiPart::alternative()
                .boundary(format!("wpalt-{}", row.get::<String, _>("id")))
                .singlepart(SinglePart::plain(row.get::<String, _>("plain")))
                .singlepart(SinglePart::html(row.get::<String, _>("html"))),
        )
        .map_err(|_| Error::invalid("Stored mail cannot be rendered."))
}
pub async fn claim(app: &App) -> Result<Option<sqlx::any::AnyRow>> {
    let owner = auth::random_token();
    let selection = if app.db.postgres {
        "UPDATE mail_jobs SET state='leased',lease_owner=$1,lease_until=$2,attempts=attempts+1 WHERE id=(SELECT id FROM mail_jobs WHERE state IN ('pending','retry') AND next_at<=$3 ORDER BY next_at,id LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING *"
    } else {
        "UPDATE mail_jobs SET state='leased',lease_owner=$1,lease_until=$2,attempts=attempts+1 WHERE id=(SELECT id FROM mail_jobs WHERE state IN ('pending','retry') AND next_at<=$3 ORDER BY next_at,id LIMIT 1) RETURNING *"
    };
    Ok(sqlx::query(selection)
        .bind(owner)
        .bind(now() + 120)
        .bind(now())
        .fetch_optional(&app.db.pool)
        .await?)
}
async fn eligible(app: &App, row: &sqlx::any::AnyRow) -> Result<bool> {
    let active: Option<String> = sqlx::query_scalar(
        "SELECT id FROM mail_jobs WHERE id=$1 AND state='leased' AND lease_owner=$2",
    )
    .bind(row.get::<String, _>("id"))
    .bind(row.get::<String, _>("lease_owner"))
    .fetch_optional(&app.db.pool)
    .await?;
    if active.is_none() {
        return Ok(false);
    }
    let contact: String = row.get("contact_id");
    if !contact.is_empty() {
        let exists: Option<String> =
            sqlx::query_scalar("SELECT id FROM audience_contacts WHERE id=$1")
                .bind(&contact)
                .fetch_optional(&app.db.pool)
                .await?;
        if exists.is_none() {
            return Ok(false);
        }
    }
    if let Some(id) = row.get::<String, _>("dedupe").strip_prefix("registration:") {
        let pending:Option<String>=sqlx::query_scalar("SELECT id FROM registration_requests WHERE id=$1 AND state='pending' AND expires_at>$2").bind(id).bind(now()).fetch_optional(&app.db.pool).await?;
        return Ok(pending.is_some());
    }
    if row.get::<String, _>("kind") == "confirmation" {
        let pending:Option<String>=sqlx::query_scalar("SELECT contact_id FROM audience_memberships WHERE contact_id=$1 AND list_id=$2 AND state='pending' AND expires_at>$3").bind(&contact).bind(row.get::<String,_>("list_id")).bind(now()).fetch_optional(&app.db.pool).await?;
        return Ok(pending.is_some());
    }
    if row.get::<String, _>("kind") != "campaign" {
        return Ok(true);
    }
    let valid:Option<i64>=sqlx::query_scalar("SELECT 1 FROM audience_memberships m JOIN audience_lists l ON l.id=m.list_id JOIN audience_contacts c ON c.id=m.contact_id WHERE m.contact_id=$1 AND m.list_id=$2 AND m.state='confirmed' AND m.policy=l.policy AND c.suppressed=0")
        .bind(row.get::<String,_>("contact_id")).bind(row.get::<String,_>("list_id")).fetch_optional(&app.db.pool).await?;
    Ok(valid.is_some())
}
async fn spool(app: &App, row: &sqlx::any::AnyRow, data: &[u8]) -> Result<()> {
    let dir = app.config.data_dir.join("outbox");
    tokio::fs::create_dir_all(&dir).await?;
    let path = dir.join(format!("{}.eml", row.get::<String, _>("id")));
    if let Ok(existing) = tokio::fs::read(&path).await {
        if existing != data {
            return Err(Error::invalid(
                "Outbox record differs from its durable message.",
            ));
        }
        return Ok(());
    }
    let temporary = dir.join(format!("{}.tmp", uuid::Uuid::new_v4()));
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .await?;
    }
    file.write_all(data).await?;
    file.sync_all().await?;
    drop(file);
    if let Err(error) = tokio::fs::hard_link(&temporary, &path).await {
        if error.kind() != std::io::ErrorKind::AlreadyExists {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(error.into());
        }
        if tokio::fs::read(&path).await? != data {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(Error::invalid("Outbox collision."));
        }
    }
    tokio::fs::remove_file(temporary).await?;
    #[cfg(unix)]
    tokio::fs::File::open(&dir).await?.sync_all().await?;
    Ok(())
}
pub async fn tick(app: &App) -> Result<usize> {
    if !app.config.business_enabled || !app.config.mail.enabled {
        return Ok(0);
    }
    // An expired network lease may already have delivered. Do not blindly replay.
    sqlx::query("UPDATE mail_jobs SET state='uncertain',last_code='expired_lease',lease_owner='',lease_until=0 WHERE state='leased' AND lease_until<$1").bind(now()).execute(&app.db.pool).await?;
    let mut processed = 0;
    let batch = if app.config.mail.smtp.is_empty() {
        app.config.mail.batch_size.min(64)
    } else {
        app.config.mail.batch_size.min(8)
    };
    for _ in 0..batch {
        let Some(row) = claim(app).await? else {
            break;
        };
        let id: String = row.get("id");
        let lease: String = row.get("lease_owner");
        let outcome = if !eligible(app, &row).await? {
            "cancelled"
        } else {
            let msg = message(&row)?;
            if app.config.mail.smtp.is_empty() {
                match spool(app, &row, &msg.formatted()).await {
                    Ok(()) => "spooled",
                    Err(_) => "retry",
                }
            } else {
                let mut outcome = "retry";
                for connection in &app.config.mail.smtp {
                    let builder = match connection.tls.as_str() {
                        "tls" => AsyncSmtpTransport::<Tokio1Executor>::relay(&connection.host),
                        "starttls" => {
                            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&connection.host)
                        }
                        _ => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(
                            &connection.host,
                        )),
                    }
                    .map_err(|_| Error::invalid("Invalid SMTP host."))?;
                    let mut builder = builder
                        .port(connection.port)
                        .timeout(Some(std::time::Duration::from_secs(15)));
                    if !connection.username.is_empty() {
                        builder = builder.credentials(Credentials::new(
                            connection.username.clone(),
                            connection.password.clone(),
                        ));
                    }
                    if !eligible(app, &row).await? {
                        outcome = "cancelled";
                        break;
                    }
                    match builder.build().send(msg.clone()).await {
                        Ok(_) => {
                            outcome = "sent";
                            break;
                        }
                        Err(error) if error.is_permanent() => {
                            outcome = "dead";
                            break;
                        }
                        Err(error) if error.is_transient() || error.is_tls() => {
                            outcome = "retry";
                        }
                        Err(_) => {
                            outcome = "uncertain";
                            break;
                        }
                    }
                }
                outcome
            }
        };
        let attempts: i64 = row.get("attempts");
        let outcome = if outcome == "retry" && attempts >= app.config.mail.max_attempts {
            "dead"
        } else {
            outcome
        };
        let mut tx = app.db.pool.begin().await?;
        let result=sqlx::query("UPDATE mail_jobs SET state=$1,last_code=$1,next_at=$2,lease_owner='',lease_until=0 WHERE id=$3 AND lease_owner=$4 AND state='leased'").bind(outcome).bind(now()+(30*(1i64<<attempts.min(5)))).bind(&id).bind(lease).execute(&mut *tx).await?;
        if result.rows_affected() == 1 {
            sqlx::query(
                "INSERT INTO mail_attempts(id,job_id,outcome,created_at) VALUES($1,$2,$3,$4)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&id)
            .bind(outcome)
            .bind(now())
            .execute(&mut *tx)
            .await?;
        }
        let recorded = result.rows_affected() == 1;
        tx.commit().await?;
        if !recorded && app.config.mail.smtp.is_empty() {
            let _ = tokio::fs::remove_file(
                app.config.data_dir.join("outbox").join(format!("{id}.eml")),
            )
            .await;
        }
        tracing::info!(event="mail_attempt_finished",job_id=%id,outcome,attempts);
        processed += 1;
    }
    Ok(processed)
}
pub async fn download(app: &App, id: &str) -> Result<Vec<u8>> {
    let row = sqlx::query("SELECT * FROM mail_jobs WHERE id=$1")
        .bind(id)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
    Ok(message(&row)?.formatted())
}
