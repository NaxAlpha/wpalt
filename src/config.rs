use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub database_url: String,
    pub data_dir: PathBuf,
    pub listen: SocketAddr,
    pub base_url: String,
    pub debug: bool,
    pub business_enabled: bool,
    pub membership_enabled: bool,
    pub membership_max_records: i64,
    pub commerce: crate::commerce::Config,
    pub identity: crate::membership::identity::Config,
    pub business_limits: crate::business::quotas::Config,
    pub mail: crate::business::mail::MailConfig,
    pub engagement: crate::business::engagement::Config,
    pub database_connections: u32,
    pub scheduler_seconds: u64,
    pub session_seconds: i64,
    pub max_upload_bytes: usize,
    pub max_backup_bytes: usize,
    pub revision_retention: i64,
    pub request_concurrency: usize,
    pub worker_concurrency: usize,
    pub request_timeout_seconds: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            database_url: "sqlite://data/wpalt.db?mode=rwc".into(),
            data_dir: "data".into(),
            listen: "127.0.0.1:3000".parse().unwrap(),
            base_url: "http://127.0.0.1:3000".into(),
            debug: false,
            business_enabled: true,
            membership_enabled: true,
            membership_max_records: 1_000_000,
            commerce: Default::default(),
            identity: Default::default(),
            business_limits: Default::default(),
            mail: crate::business::mail::MailConfig::default(),
            engagement: crate::business::engagement::Config::default(),
            database_connections: 4,
            scheduler_seconds: 5,
            session_seconds: 28800,
            max_upload_bytes: 8 * 1024 * 1024,
            max_backup_bytes: 256 * 1024 * 1024,
            revision_retention: 50,
            request_concurrency: 32,
            worker_concurrency: 2,
            request_timeout_seconds: 30,
        }
    }
}
impl Config {
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        let mut c: Self = match path {
            Some(p) => {
                toml::from_str(&std::fs::read_to_string(p).context("cannot read config file")?)
                    .context("invalid configuration")?
            }
            None => Self::default(),
        };
        if let Ok(v) = std::env::var("WPALT_DATABASE_URL") {
            c.database_url = v;
        }
        if let Ok(v) = std::env::var("WPALT_DATA_DIR") {
            c.data_dir = v.into();
        }
        if let Ok(v) = std::env::var("WPALT_BASE_URL") {
            c.base_url = v;
        }
        if let Ok(v) = std::env::var("WPALT_LISTEN") {
            c.listen = v.parse().context("invalid WPALT_LISTEN")?;
        }
        if let Ok(v) = std::env::var("WPALT_DEBUG") {
            c.debug = v.parse().context("WPALT_DEBUG must be true or false")?;
        }
        if let Ok(v) = std::env::var("WPALT_BUSINESS_ENABLED") {
            c.business_enabled = v
                .parse()
                .context("WPALT_BUSINESS_ENABLED must be true or false")?;
        }
        if let Ok(v) = std::env::var("WPALT_MEMBERSHIP_ENABLED") {
            c.membership_enabled = v
                .parse()
                .context("WPALT_MEMBERSHIP_ENABLED must be true or false")?;
        }
        if let Ok(v) = std::env::var("WPALT_COMMERCE_ENABLED") {
            c.commerce.enabled = v
                .parse()
                .context("WPALT_COMMERCE_ENABLED must be true or false")?;
        }
        if let Ok(v) = std::env::var("WPALT_STRIPE_SECRET_KEY") {
            c.commerce.stripe.secret_key = v;
        }
        if let Ok(v) = std::env::var("WPALT_STRIPE_WEBHOOK_SECRET") {
            c.commerce.stripe.webhook_secret = v;
        }
        if let Ok(v) = std::env::var("WPALT_IDENTITY_CLIENT_SECRET") {
            c.identity.client_secret = v;
        }
        Ok(c)
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        self.commerce.validate()?;
        ensure!(
            !self.commerce.stripe.enabled || self.base_url.starts_with("https://"),
            "Hosted payments require an HTTPS base_url"
        );
        self.identity.validate()?;
        ensure!(
            (1..=1_000_000_000).contains(&self.membership_max_records),
            "membership_max_records must be between 1 and one billion"
        );
        ensure!(
            !self.identity.enabled || self.secure_cookie(),
            "OIDC sign-in requires an HTTPS site origin"
        );
        self.business_limits.validate()?;
        self.mail.validate()?;
        self.engagement.validate()?;
        ensure!(
            self.database_url.starts_with("sqlite:")
                || self.database_url.starts_with("postgres://")
                || self.database_url.starts_with("postgresql://"),
            "database must be SQLite or PostgreSQL"
        );
        let u = url::Url::parse(&self.base_url).context("invalid base_url")?;
        ensure!(
            ["http", "https"].contains(&u.scheme())
                && u.host_str().is_some()
                && u.username().is_empty()
                && u.password().is_none()
                && u.query().is_none()
                && u.fragment().is_none()
                && u.path() == "/",
            "base_url must be an HTTP(S) origin without credentials, path, query or fragment"
        );
        ensure!(
            u.scheme() == "https"
                || ["localhost", "127.0.0.1", "[::1]"].contains(&u.host_str().unwrap_or("")),
            "non-local sites require an HTTPS base_url behind a TLS reverse proxy"
        );
        ensure!(
            (1..=64).contains(&self.database_connections),
            "database_connections must be 1..64"
        );
        ensure!(
            (1..=3600).contains(&self.scheduler_seconds),
            "scheduler_seconds must be 1..3600"
        );
        ensure!(
            (300..=604800).contains(&self.session_seconds),
            "session_seconds must be 300..604800"
        );
        ensure!(
            (1024..=32 * 1024 * 1024).contains(&self.max_upload_bytes),
            "max_upload_bytes must be 1 KiB..32 MiB"
        );
        ensure!(
            (1024 * 1024..=1024 * 1024 * 1024).contains(&self.max_backup_bytes),
            "max_backup_bytes must be 1 MiB..1 GiB"
        );
        ensure!(
            !self.data_dir.as_os_str().is_empty(),
            "data_dir cannot be empty"
        );
        ensure!(
            (5..=10000).contains(&self.revision_retention),
            "revision_retention must be 5..10000"
        );
        ensure!(
            (1..=1024).contains(&self.request_concurrency),
            "request_concurrency must be 1..1024"
        );
        ensure!(
            (1..=32).contains(&self.worker_concurrency),
            "worker_concurrency must be 1..32"
        );
        ensure!(
            (1..=300).contains(&self.request_timeout_seconds),
            "request_timeout_seconds must be 1..300"
        );
        Ok(())
    }
    pub fn prepare_directories(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(self.data_dir.join("media"))?;
        std::fs::create_dir_all(self.data_dir.join("attachments"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.data_dir, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }
    pub fn origin(&self) -> String {
        url::Url::parse(&self.base_url)
            .map(|url| url.origin().ascii_serialization())
            .unwrap_or_else(|_| self.base_url.trim_end_matches('/').into())
    }
    pub fn secure_cookie(&self) -> bool {
        url::Url::parse(&self.base_url).is_ok_and(|url| url.scheme() == "https")
    }
    pub fn redacted(&self) -> serde_json::Value {
        let mut value = serde_json::to_value(self).expect("configuration serializes");
        value["database_url"] =
            serde_json::Value::String(if self.database_url.starts_with("sqlite:") {
                self.database_url.clone()
            } else {
                "postgres://[REDACTED]".into()
            });
        for connection in value["mail"]["smtp"].as_array_mut().unwrap() {
            connection["password"] = serde_json::Value::String("[REDACTED]".into());
            connection["username"] = serde_json::Value::String("[REDACTED]".into());
        }
        value["identity"]["client_secret"] = "[REDACTED]".into();
        value["commerce"]["stripe"]["secret_key"] = "[REDACTED]".into();
        value["commerce"]["stripe"]["webhook_secret"] = "[REDACTED]".into();
        value
    }
}
