pub mod auth;
pub mod backup;
pub mod builder_web;
pub mod business;
pub mod config;
pub mod content;
pub mod db;
pub mod discovery;
pub mod error;
pub mod migrations;
pub mod model;
pub mod schema;
pub mod theme;
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
    pub themes: Arc<Mutex<std::collections::BTreeMap<(String, i64), theme::Package>>>,
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
            themes: Arc::new(Mutex::new(std::collections::BTreeMap::new())),
        })
    }
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock before Unix epoch")
        .as_secs() as i64
}

pub mod document;
