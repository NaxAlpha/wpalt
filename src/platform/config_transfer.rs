//! Offline current-version configuration transfer. Never opens a site or contacts a provider.
use crate::{auth, backup, config::Config};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MAX_BYTES: usize = 256 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Package {
    format: String,
    application_version: String,
    database_schema: i64,
    contains_secrets: bool,
    configuration: serde_json::Value,
}

pub fn export(config: &Config, output: &Path, secrets: bool) -> Result<()> {
    let package = Package {
        format: "wpalt-configuration-v1".into(),
        application_version: env!("CARGO_PKG_VERSION").into(),
        database_schema: crate::db::SCHEMA_VERSION,
        contains_secrets: secrets,
        configuration: if secrets {
            serde_json::to_value(config)?
        } else {
            config.redacted()
        },
    };
    let bytes = serde_json::to_vec_pretty(&package)?;
    ensure!(
        bytes.len() <= MAX_BYTES,
        "Configuration package exceeds limit."
    );
    backup::write_private(output, &bytes)?;
    Ok(())
}

/// Metadata and validation failures never echo private field values.
pub fn preview(bytes: &[u8], accept_secrets: bool) -> Result<(Config, serde_json::Value)> {
    ensure!(
        bytes.len() <= MAX_BYTES,
        "Configuration package exceeds limit."
    );
    let package: Package = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("Invalid configuration package."))?;
    ensure!(
        package.format == "wpalt-configuration-v1"
            && package.application_version == env!("CARGO_PKG_VERSION")
            && package.database_schema == crate::db::SCHEMA_VERSION,
        "Unsupported configuration version; use the documented migration procedure."
    );
    ensure!(
        package.contains_secrets && accept_secrets,
        "Redacted exports are inspection records. Import requires explicit private secret transfer and --accept-secrets."
    );
    let config: Config = serde_json::from_value(package.configuration)
        .map_err(|_| anyhow::anyhow!("Invalid configuration fields."))?;
    config
        .validate()
        .map_err(|_| anyhow::anyhow!("Transferred configuration fails validation."))?;
    let plan = auth::digest(bytes);
    let report = serde_json::json!({
        "format":"wpalt-configuration-review-v1", "plan":plan,
        "application_version":env!("CARGO_PKG_VERSION"), "database_schema":crate::db::SCHEMA_VERSION,
        "configuration":config.redacted(),
        "boundary":"Writes a NEW private configuration file only. Data, media, server-side challenges and provider accounts are not transferred. Inspect paths, origin and credentials before starting the target. Environment/CLI overrides remain effective at runtime."
    });
    Ok((config, report))
}

pub fn execute(
    config: &Config,
    report: &serde_json::Value,
    plan: &str,
    output: &Path,
) -> Result<()> {
    ensure!(
        report["plan"].as_str() == Some(plan),
        "Review a fresh exact configuration plan."
    );
    let bytes = toml::to_string_pretty(config)?.into_bytes();
    backup::write_private(output, &bytes)?;
    Ok(())
}
