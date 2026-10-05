pub mod auth;
pub mod backup;
pub mod builder_web;
pub mod business;
pub mod commerce;
pub mod config;
pub mod content;
pub mod db;
pub mod discovery;
pub mod error;
pub mod membership;
pub mod migrations;
pub mod model;
pub mod operations;
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
    pub cache_generation: Arc<std::sync::atomic::AtomicU64>,
    pub protection_limits: Arc<Mutex<operations::protection::Limits>>,
    pub media_cache: Arc<Mutex<operations::cache::Cache>>,
    pub page_cache: Arc<Mutex<operations::cache::Cache>>,
    pub spam_secret: Arc<[u8; 32]>,
    pub spam_used: Arc<Mutex<operations::spam::Used>>,
    pub passkey_ceremonies: Arc<Mutex<operations::passkeys::Ceremonies>>,
    pub audit_work: Arc<Mutex<()>>,
    pub job_work: Arc<Mutex<()>>,
    pub job_io: Arc<Mutex<()>>,
    pub recovery_work: Arc<Mutex<()>>,
    pub password_work: Arc<Semaphore>,
    pub media_reads: Arc<Semaphore>,
    pub media_work: Arc<Semaphore>,
    pub request_work: Arc<Semaphore>,
    pub login_limits: Arc<Mutex<auth::LoginLimits>>,
    pub dummy_hash: Arc<String>,
    pub themes: Arc<Mutex<std::collections::BTreeMap<(String, i64), theme::Package>>>,
}

impl App {
    pub async fn mutation(&self) -> operations::cache::Mutation<'_> {
        let guard = self.mutations.lock().await;
        self.cache_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        operations::cache::Mutation {
            _guard: guard,
            generation: &self.cache_generation,
        }
    }
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
            spam_secret: Arc::new(operations::encryption::generate_key()),
            spam_used: Arc::new(Mutex::new(operations::spam::Used::default())),
            passkey_ceremonies: Arc::new(Mutex::new(operations::passkeys::Ceremonies::default())),
            audit_work: Arc::new(Mutex::new(())),
            job_work: Arc::new(Mutex::new(())),
            job_io: Arc::new(Mutex::new(())),
            recovery_work: Arc::new(Mutex::new(())),
            cache_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            protection_limits: Arc::new(Mutex::new(operations::protection::Limits::default())),
            media_cache: Arc::new(Mutex::new(operations::cache::Cache::default())),
            page_cache: Arc::new(Mutex::new(operations::cache::Cache::default())),
            password_work: Arc::new(Semaphore::new(workers)),
            media_reads: Arc::new(Semaphore::new(workers)),
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
