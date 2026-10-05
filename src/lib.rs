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
pub mod platform;
pub mod schema;
pub mod theme;
pub mod view;
pub mod web;

use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};

#[derive(Clone)]
pub struct App {
    pub config: Arc<config::Config>,
    pub node_id: Arc<String>,
    pub local_coordinator: Option<Arc<platform::local_processes::Coordinator>>,
    pub clone_held: Arc<std::sync::atomic::AtomicBool>,
    pub consent_scripts: Arc<operations::consent_scripts::Scripts>,
    pub security_headers: Arc<operations::headers::Policy>,
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

#[derive(Clone, Copy, PartialEq)]
enum Startup {
    Migrate,
    Runtime,
    Maintenance,
    RebindMaintenance,
}

impl App {
    pub async fn mutation(&self) -> error::Result<operations::cache::Mutation<'_>> {
        if let Some(c) = &self.local_coordinator {
            c.mark_intent()?;
        }
        let guard = self.mutations.lock().await;
        self.cache_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(operations::cache::Mutation {
            _guard: guard,
            generation: &self.cache_generation,
        })
    }
    pub async fn open(config: config::Config) -> anyhow::Result<Self> {
        Self::open_checked(config, Startup::Migrate).await
    }
    pub async fn open_runtime(config: config::Config) -> anyhow::Result<Self> {
        Self::open_checked(config, Startup::Runtime).await
    }
    /// Read the supported M8/M9 graph while stopped, without schema changes.
    pub async fn open_maintenance(
        mut config: config::Config,
        rebind: bool,
    ) -> anyhow::Result<Self> {
        config.local_processes = false;
        Self::open_checked(
            config,
            if rebind {
                Startup::RebindMaintenance
            } else {
                Startup::Maintenance
            },
        )
        .await
    }
    async fn open_checked(config: config::Config, startup_mode: Startup) -> anyhow::Result<Self> {
        config.validate()?;
        let consent_scripts = if config.business_enabled && config.engagement.enabled {
            operations::consent_scripts::Scripts::compile(&config.consent_scripts)?
        } else {
            operations::consent_scripts::Scripts::default()
        };
        config.prepare_directories()?;
        let local_coordinator = if config.local_processes {
            Some(Arc::new(
                platform::local_processes::Coordinator::new(&config)
                    .map_err(|e| anyhow::anyhow!(e.1))?,
            ))
        } else {
            None
        };
        let startup = if let Some(c) = &local_coordinator {
            Some(c.lock().await.map_err(|e| anyhow::anyhow!(e.1))?)
        } else {
            None
        };
        let spam_secret = if let Some(c) = &local_coordinator {
            c.initialize().map_err(|e| anyhow::anyhow!(e.1))?
        } else {
            operations::encryption::generate_key()
        };
        let workers = config.worker_concurrency;
        let requests = config.request_concurrency;
        let db = db::Db::open(&config).await?;
        if startup_mode != Startup::RebindMaintenance {
            platform::local_processes::verify_directory(&db, &config, false)
                .await
                .map_err(|e| anyhow::anyhow!(e.1))?;
        }
        if startup_mode != Startup::Migrate {
            let present: i64 = if db.postgres {
                sqlx::query_scalar(
                    "SELECT CASE WHEN to_regclass('schema_version') IS NULL THEN 0 ELSE 1 END",
                )
                .fetch_one(&db.pool)
                .await?
            } else {
                sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_version'").fetch_one(&db.pool).await?
            };
            anyhow::ensure!(
                present == 1,
                "site is not initialized; run init before serving"
            );
            let version: i64 = sqlx::query_scalar("SELECT version FROM schema_version WHERE id=1")
                .fetch_one(&db.pool)
                .await?;
            anyhow::ensure!(
                version == db::SCHEMA_VERSION
                    || (matches!(
                        startup_mode,
                        Startup::Maintenance | Startup::RebindMaintenance
                    ) && version == 14),
                "runtime schema is incompatible; stop all nodes and use the documented offline upgrade/recovery path"
            );
        } else {
            db.migrate().await?;
        }
        if !matches!(
            startup_mode,
            Startup::Maintenance | Startup::RebindMaintenance
        ) {
            platform::local_processes::verify_directory(&db, &config, true)
                .await
                .map_err(|e| anyhow::anyhow!(e.1))?;
        }
        let clone_held = sqlx::query_scalar::<_, i64>("SELECT held FROM recovery_mode WHERE id=1")
            .fetch_one(&db.pool)
            .await?
            == 1;
        let dummy_hash = tokio::task::spawn_blocking(|| {
            auth::hash_password("unused-dummy-credential-not-an-account")
        })
        .await??;
        drop(startup);
        Ok(Self {
            local_coordinator,
            node_id: Arc::new(uuid::Uuid::new_v4().to_string()),
            clone_held: Arc::new(std::sync::atomic::AtomicBool::new(clone_held)),
            consent_scripts: Arc::new(consent_scripts),
            security_headers: Arc::new(operations::headers::Policy::compile(&config)),
            config: Arc::new(config),
            db,
            mutations: Arc::new(Mutex::new(())),
            spam_secret: Arc::new(spam_secret),
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
