//! Explicit pre-upgrade recovery point. No automatic executable replacement.
use crate::{
    App, backup,
    error::{Error, Result},
};
use serde::Serialize;
use std::path::Path;
#[derive(Serialize)]
pub struct Receipt {
    pub format: &'static str,
    pub created_at: i64,
    pub executable_sha256: String,
    pub application_version: &'static str,
    pub schema: serde_json::Value,
    pub archive_sha256: String,
    pub archive_bytes: usize,
    pub recovery: &'static str,
}
pub async fn prepare(app: &App, output: &Path, key_file: &Path) -> Result<Receipt> {
    let executable = std::env::current_exe()
        .map_err(|_| Error::invalid("Cannot locate the pre-upgrade executable."))?;
    let executable_bytes = backup::read_bounded(&executable, 256 * 1024 * 1024).await?;
    let executable_sha256 = crate::auth::digest(&executable_bytes);
    drop(executable_bytes);
    let key = super::encryption::read_key(key_file).await?;
    let plaintext = backup::capture(app).await?;
    let graph = backup::inspect(&app.config, &plaintext)?;
    let encoded = super::encryption::seal(&key, &plaintext)?;
    backup::write_private(output, &encoded)
        .map_err(|_| Error::invalid("Cannot create a new private upgrade recovery point."))?;
    let stored = backup::read_bounded(
        output,
        app.config.max_backup_bytes + super::encryption::OVERHEAD,
    )
    .await?;
    let recovered = super::encryption::open(&key, &stored, app.config.max_backup_bytes)?;
    backup::inspect(&app.config, &recovered)?;
    if recovered != plaintext {
        return Err(Error::invalid(
            "Upgrade recovery point verification failed.",
        ));
    }
    Ok(Receipt {
        format: "wpalt-upgrade-receipt-v1",
        executable_sha256,
        created_at: crate::now(),
        application_version: env!("CARGO_PKG_VERSION"),
        schema: serde_json::to_value(graph)
            .map_err(|_| Error::invalid("Cannot describe recovery graph."))?["schema"]
            .clone(),
        archive_sha256: crate::auth::digest(&stored),
        archive_bytes: stored.len(),
        recovery: "Keep this archive, its independent key, the old executable and private config. Restore with the old executable into an empty database and data directory, verify, then switch the site. Do not start an older executable against upgraded data.",
    })
}
