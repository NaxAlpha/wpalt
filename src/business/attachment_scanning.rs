//! Optional owner-hosted ClamAV admission. Stream bytes; never daemon-visible paths.
use crate::{
    App,
    error::{Error, Result},
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub socket: PathBuf,
    pub deadline_seconds: u64,
    pub concurrency: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            socket: PathBuf::new(),
            deadline_seconds: 5,
            concurrency: 2,
        }
    }
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (1..=15).contains(&self.deadline_seconds),
            "attachment scanner deadline_seconds must be 1..15"
        );
        anyhow::ensure!(
            (1..=8).contains(&self.concurrency),
            "attachment scanner concurrency must be 1..8"
        );
        anyhow::ensure!(
            !self.enabled || (cfg!(unix) && self.socket.is_absolute()),
            "attachment scanning requires an absolute owner-hosted Unix socket"
        );
        Ok(())
    }
}
const UNAVAILABLE: &str = "The configured attachment scanner is unavailable or returned an invalid result. No file was admitted; retry after the operator restores scanning.";

#[cfg(unix)]
async fn scan_socket(config: &Config, bytes: &[u8]) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut connection = tokio::net::UnixStream::connect(&config.socket)
        .await
        .map_err(|_| Error::invalid(UNAVAILABLE))?;
    connection
        .write_all(b"zINSTREAM\0")
        .await
        .map_err(|_| Error::invalid(UNAVAILABLE))?;
    for chunk in bytes.chunks(16 * 1024) {
        connection
            .write_all(&(chunk.len() as u32).to_be_bytes())
            .await
            .map_err(|_| Error::invalid(UNAVAILABLE))?;
        connection
            .write_all(chunk)
            .await
            .map_err(|_| Error::invalid(UNAVAILABLE))?;
    }
    connection
        .write_all(&[0; 4])
        .await
        .map_err(|_| Error::invalid(UNAVAILABLE))?;
    // A non-session scan closes after its reply. Require the exact complete
    // result, so appended/truncated records cannot turn a failure into success.
    let mut reply = Vec::new();
    connection
        .take(1025)
        .read_to_end(&mut reply)
        .await
        .map_err(|_| Error::invalid(UNAVAILABLE))?;
    if reply == b"stream: OK\0" {
        Ok(())
    } else if reply.len() <= 1024 && reply.starts_with(b"stream: ") && reply.ends_with(b" FOUND\0")
    {
        Err(Error::invalid(
            "The configured scanner detected this attachment. The file was not admitted.",
        ))
    } else {
        Err(Error::invalid(UNAVAILABLE))
    }
}

pub async fn scan(app: &App, bytes: &[u8]) -> Result<()> {
    let config = &app.config.attachment_scanning;
    if !config.enabled {
        return Ok(());
    }
    if bytes.is_empty() || bytes.len() > 2 * 1024 * 1024 {
        return Err(Error::invalid("Attachment scanning accepts at most 2 MiB."));
    }
    let _permit = app
        .attachment_scan_work
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            Error::invalid("Attachment scanning is busy. No file was admitted; retry shortly.")
        })?;
    let started = Instant::now();
    #[cfg(unix)]
    let result = match tokio::time::timeout(
        Duration::from_secs(config.deadline_seconds),
        scan_socket(config, bytes),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(Error::invalid(
            "Attachment scanning exceeded its deadline. No file was admitted; retry shortly.",
        )),
    };
    #[cfg(not(unix))]
    let result = Err(Error::invalid(UNAVAILABLE));
    tracing::debug!(
        event = "attachment_scan",
        bytes = bytes.len(),
        duration_us = started.elapsed().as_micros() as u64,
        accepted = result.is_ok()
    );
    result
}
