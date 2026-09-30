pub mod auth;
pub mod backup;
pub mod config;
pub mod content;
pub mod db;
pub mod error;
pub mod model;
pub mod view;
pub mod web;

use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};

#[derive(Clone)]
pub struct App {
    pub config: Arc<config::Config>,
    pub db: db::Db,
    pub mutations: Arc<Mutex<()>>,
    pub password_work: Arc<Semaphore>,
    pub media_work: Arc<Semaphore>,
    pub request_work: Arc<Semaphore>,
    pub login_limits: Arc<Mutex<auth::LoginLimits>>,
    pub dummy_hash: Arc<String>,
}

impl App {
    pub async fn open(config: config::Config) -> anyhow::Result<Self> {
        config.validate()?;
        config.prepare_directories()?;
        let workers = config.worker_concurrency;
        let requests = config.request_concurrency;
        let db = db::Db::open(&config).await?;
        db.migrate().await?;
        let dummy_hash = tokio::task::spawn_blocking(|| {
            auth::hash_password("unused-dummy-credential-not-an-account")
        })
        .await??;
        Ok(Self {
            config: Arc::new(config),
            db,
            mutations: Arc::new(Mutex::new(())),
            password_work: Arc::new(Semaphore::new(workers)),
            media_work: Arc::new(Semaphore::new(workers)),
            request_work: Arc::new(Semaphore::new(requests)),
            login_limits: Arc::new(Mutex::new(auth::LoginLimits::default())),
            dummy_hash: Arc::new(dummy_hash),
        })
    }
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock before Unix epoch")
        .as_secs() as i64
}
