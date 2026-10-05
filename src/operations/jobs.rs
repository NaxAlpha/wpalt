//! Bounded scheduler evidence. This journal never acts as a replay queue.
use crate::{
    App, backup,
    error::{Error, Result},
};
use serde::{Deserialize, Serialize};
use std::{future::Future, time::Instant};

const LIMIT: usize = 128 * 1024;
const RETAIN: usize = 64;
const STAGES: &[&str] = &[
    "recovery",
    "publication",
    "campaigns",
    "commerce",
    "mail",
    "engagement-retention",
    "quota-retention",
    "maintenance",
];

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    pub name: String,
    pub succeeded: bool,
    pub count: Option<usize>,
    pub elapsed_ms: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle {
    pub id: String,
    pub started_at: i64,
    pub finished_at: i64,
    pub state: String,
    pub elapsed_ms: u64,
    pub stages: Vec<Stage>,
}
pub async fn stage<F>(name: &'static str, work: F) -> Stage
where
    F: Future<Output = Result<usize>>,
{
    let start = Instant::now();
    let result = work.await;
    if result.is_err() {
        // Errors can contain submitted data or credentials; store only a code.
        tracing::error!(event = "background_stage_failed", stage = name);
    }
    Stage {
        name: name.into(),
        succeeded: result.is_ok(),
        count: result.ok(),
        elapsed_ms: start.elapsed().as_millis().min(u64::MAX as u128) as u64,
    }
}
pub async fn stage_unit<F>(name: &'static str, work: F) -> Stage
where
    F: Future<Output = Result<()>>,
{
    let mut outcome = stage(name, async { work.await.map(|_| 0) }).await;
    outcome.count = None;
    outcome
}
pub async fn read(app: &App) -> Result<Vec<Cycle>> {
    let _io = app.job_io.lock().await;
    let path = app.config.data_dir.join("background-jobs.json");
    match tokio::fs::symlink_metadata(&path).await {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Ok(m) if m.is_file() => {}
        _ => return Err(Error::invalid("Unsafe background history path.")),
    }
    let bytes = backup::read_bounded(&path, LIMIT).await?;
    let cycles: Vec<Cycle> = serde_json::from_slice(&bytes)
        .map_err(|_| Error::invalid("Background history is damaged; inspect the private file."))?;
    if cycles.len() > RETAIN
        || cycles.iter().any(|c| {
            uuid::Uuid::parse_str(&c.id).is_err()
                || c.started_at < 0
                || c.finished_at < 0
                || !["running", "succeeded", "failed", "interrupted"].contains(&c.state.as_str())
                || c.stages.len() > STAGES.len()
                || c.stages.iter().any(|s| !STAGES.contains(&s.name.as_str()))
        })
    {
        return Err(Error::invalid("Invalid background history."));
    }
    Ok(cycles)
}
async fn persist(app: &App, cycles: &[Cycle]) -> Result<()> {
    let bytes =
        serde_json::to_vec(cycles).map_err(|_| Error::invalid("Invalid background history."))?;
    if bytes.len() > LIMIT {
        return Err(Error::invalid("Background history exceeds its budget."));
    }
    let path = app.config.data_dir.join("background-jobs.json");
    let io = app.job_io.clone().lock_owned().await;
    tokio::task::spawn_blocking(move || {
        // Blocking writes outlive task abort; retain I/O ownership until sync finishes.
        let _io = io;
        super::recovery::publish(&path, &bytes, true)
    })
    .await
    .map_err(|_| Error::invalid("Background history worker interrupted."))?
    .map_err(|_| Error::invalid("Cannot persist background history; check site storage."))
}
pub async fn run_cycle<F>(app: &App, work: F) -> Result<()>
where
    F: Future<Output = Vec<Stage>>,
{
    let mut coordinator = if let Some(c) = &app.local_coordinator {
        Some(c.begin(app).await?)
    } else {
        None
    };
    if app.clone_held.load(std::sync::atomic::Ordering::SeqCst)
        && let Some(guard) = coordinator.take()
    {
        guard.complete(app).await?;
        return Ok(());
    }
    if coordinator.is_some()
        && read(app).await?.first().is_some_and(|cycle| {
            cycle.started_at > crate::now() - app.config.scheduler_seconds as i64
        })
    {
        return coordinator
            .take()
            .ok_or_else(|| Error::invalid("Missing local process ownership."))?
            .complete(app)
            .await;
    }
    run_cycle_inner(app, work).await?;
    if let Some(guard) = coordinator {
        guard.complete(app).await?;
    }
    Ok(())
}
async fn run_cycle_inner<F>(app: &App, work: F) -> Result<()>
where
    F: Future<Output = Vec<Stage>>,
{
    let _guard = app.job_work.lock().await;
    if app.clone_held.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(Error::invalid(
            "Background work is paused for this read-only clone.",
        ));
    }
    let mut cycles = read(app).await?;
    for cycle in &mut cycles {
        if cycle.state == "running" {
            cycle.state = "interrupted".into();
        }
    }
    cycles.truncate(RETAIN - 1);
    cycles.insert(
        0,
        Cycle {
            id: uuid::Uuid::new_v4().to_string(),
            started_at: crate::now(),
            finished_at: 0,
            state: "running".into(),
            elapsed_ms: 0,
            stages: Vec::new(),
        },
    );
    // If this fails, the future is never polled and no domain work is dispatched.
    persist(app, &cycles).await?;
    let start = Instant::now();
    let stages = work.await;
    if stages.len() > STAGES.len() || stages.iter().any(|s| !STAGES.contains(&s.name.as_str())) {
        return Err(Error::invalid("Invalid scheduler stage inventory."));
    }
    cycles[0].state = if stages.iter().all(|s| s.succeeded) {
        "succeeded"
    } else {
        "failed"
    }
    .into();
    cycles[0].finished_at = crate::now();
    cycles[0].elapsed_ms = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
    cycles[0].stages = stages;
    // A failed final write leaves an unresolved intent. Never replay committed
    // payments or deliveries from this journal; domain state governs retries.
    persist(app, &cycles).await
}
