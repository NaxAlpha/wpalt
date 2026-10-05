//! Transactional bounded content-event journal. Independent workers own delivery;
//! the CMS never fetches webhook URLs or promises exactly-once remote side effects.
use crate::{
    App,
    error::{Error, Result},
};
use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Any, Row, Transaction};

pub const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS integration_event_state(id BIGINT PRIMARY KEY CHECK(id=1),epoch TEXT NOT NULL,sequence BIGINT NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS integration_events(sequence BIGINT PRIMARY KEY,payload TEXT NOT NULL);";
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub retained_events: i64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            retained_events: 4096,
        }
    }
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (32..=100000).contains(&self.retained_events),
            "Retain 32..100000 integration events."
        );
        Ok(())
    }
}
pub async fn initialize(tx: &mut Transaction<'_, Any>) -> Result<()> {
    sqlx::query("INSERT INTO integration_event_state(id,epoch,sequence) VALUES(1,$1,0) ON CONFLICT(id) DO NOTHING").bind(uuid::Uuid::new_v4().to_string()).execute(&mut **tx).await?;
    Ok(())
}
/// Content and event commit together. UUID epoch detects a rebuilt/recovered site;
/// the monotonic primary key admits replay without scanning rich content records.
pub async fn append(
    app: &App,
    tx: &mut Transaction<'_, Any>,
    post: &crate::model::Post,
    action: &str,
) -> Result<()> {
    let row = sqlx::query("UPDATE integration_event_state SET sequence=sequence+1 WHERE id=1 RETURNING epoch,sequence").fetch_one(&mut **tx).await?;
    let sequence: i64 = row.get("sequence");
    let epoch: String = row.get("epoch");
    let payload = json!({"format":"wpalt-content-event-v1","id":format!("{epoch}:{sequence}:{}",uuid::Uuid::new_v4()),"sequence":sequence,"kind":"content.changed","content_id":post.id,"version":post.version,"status":post.status,"action":action,"occurred_at":crate::now()});
    sqlx::query("INSERT INTO integration_events(sequence,payload) VALUES($1,$2)")
        .bind(sequence)
        .bind(payload.to_string())
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM integration_events WHERE sequence<=$1")
        .bind(sequence - app.config.integration_events.retained_events)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
pub fn routes() -> Router<App> {
    Router::new().route("/api/v1/events", get(feed))
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    #[serde(default)]
    after: String,
}
async fn feed(
    State(app): State<App>,
    headers: HeaderMap,
    Query(cursor): Query<Cursor>,
) -> Result<Json<Value>> {
    // Revalidate authority while serialized with mutations/revocation; never hold
    // this lock during client transport or external delivery.
    let _guard = app.mutation().await;
    super::integrations::authenticate(&app, &headers, false).await?;
    let mut tx = app.db.pool.begin().await?;
    let row = sqlx::query("SELECT epoch,sequence FROM integration_event_state WHERE id=1")
        .fetch_one(&mut *tx)
        .await?;
    let epoch: String = row.get("epoch");
    let latest: i64 = row.get("sequence");
    let oldest: Option<i64> = sqlx::query_scalar("SELECT MIN(sequence) FROM integration_events")
        .fetch_one(&mut *tx)
        .await?;
    let floor = oldest.unwrap_or(latest + 1) - 1;
    let after = if cursor.after.is_empty() {
        floor
    } else {
        let (source, remainder) = cursor
            .after
            .split_once(':')
            .ok_or_else(|| Error::invalid("Use the opaque cursor returned by the event feed."))?;
        let (number, anchor) = remainder
            .split_once(':')
            .ok_or_else(|| Error::invalid("Invalid event cursor."))?;
        let after: i64 = number
            .parse()
            .map_err(|_| Error::invalid("Invalid event cursor."))?;
        if source != epoch || after < floor {
            return Err(Error(
                StatusCode::CONFLICT,
                "Event replay gap or source reset. Reconcile source state before starting a new cursor.",
            ));
        }
        if after < 0 {
            return Err(Error::invalid(
                "Event cursor is outside this source journal.",
            ));
        }
        if after > latest {
            return Err(Error(
                StatusCode::CONFLICT,
                "Source journal rolled back; reconcile source state before continuing.",
            ));
        }
        if after == 0 {
            if anchor != "start" {
                return Err(Error::invalid("Invalid initial event cursor."));
            }
        } else {
            let payload: Option<String> =
                sqlx::query_scalar("SELECT payload FROM integration_events WHERE sequence=$1")
                    .bind(after)
                    .fetch_optional(&mut *tx)
                    .await?;
            let matched = payload
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                .is_some_and(|event| event["id"] == cursor.after);
            if !matched {
                return Err(Error(
                    StatusCode::CONFLICT,
                    "Event lineage changed or was evicted; reconcile source state before continuing.",
                ));
            }
        }
        after
    };
    let rows = sqlx::query("SELECT sequence,payload FROM integration_events WHERE sequence>$1 ORDER BY sequence LIMIT 26").bind(after).fetch_all(&mut *tx).await?;
    let has_more = rows.len() > 25;
    let mut next = if cursor.after.is_empty() {
        format!("{epoch}:0:start")
    } else {
        cursor.after
    };
    let mut events = Vec::new();
    for row in rows.into_iter().take(25) {
        let event = serde_json::from_str::<Value>(&row.get::<String, _>("payload"))
            .map_err(|_| Error::invalid("Stored event is invalid."))?;
        next = event["id"]
            .as_str()
            .ok_or_else(|| Error::invalid("Stored event identity is invalid."))?
            .to_owned();
        events.push(event);
    }
    tx.commit().await?;
    Ok(Json(
        json!({"format":"wpalt-event-feed-v1","source_origin":app.config.origin(),"epoch":epoch,"events":events,"next":next,"has_more":has_more,"oldest_sequence":oldest,"latest_sequence":latest,"retained_limit":app.config.integration_events.retained_events,"boundary":"Bounded metadata replay. Initial empty cursor starts at oldest retained event, not all historical content. Checkpoint only after idempotent delivery; gaps/reset require reconciliation."}),
    ))
}
