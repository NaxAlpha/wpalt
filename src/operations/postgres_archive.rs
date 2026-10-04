//! Native PostgreSQL 17 archive_command / restore_command helpers. These are
//! file tools: they do not open the application database or acquire its server lock.
use crate::{
    backup,
    config::Config as ApplicationConfig,
    error::{Error, Result},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const PREFIX: &[u8] = b"wpalt-postgres-wal-v1";
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub system_id: String,
    pub key_file: PathBuf,
    pub directory: PathBuf,
    pub max_segment_bytes: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            system_id: String::new(),
            key_file: PathBuf::new(),
            directory: PathBuf::new(),
            max_segment_bytes: 32 * 1024 * 1024,
        }
    }
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.max_segment_bytes.is_power_of_two()
                && (1024 * 1024..=1024 * 1024 * 1024).contains(&self.max_segment_bytes),
            "postgres_archive max_segment_bytes must be a power of two between 1 MiB and 1 GiB"
        );
        if self.enabled {
            let id = self.system_id.parse::<u64>()?;
            anyhow::ensure!(
                id > 0 && id.to_string() == self.system_id,
                "postgres_archive system_id must be the canonical cluster identifier"
            );
            anyhow::ensure!(
                self.key_file.is_absolute() && self.directory.is_absolute(),
                "postgres_archive requires absolute key_file and existing directory"
            );
        }
        Ok(())
    }
}
fn uppercase_hex(value: &str, count: usize) -> bool {
    value.len() == count
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
}
pub fn valid_name(name: &str) -> bool {
    uppercase_hex(name, 24)
        || name
            .strip_suffix(".history")
            .is_some_and(|s| uppercase_hex(s, 8))
        || name
            .strip_suffix(".backup")
            .and_then(|s| s.split_once('.'))
            .is_some_and(|(wal, offset)| uppercase_hex(wal, 24) && uppercase_hex(offset, 8))
}
fn validate_record(config: &Config, name: &str, data: &[u8]) -> Result<()> {
    if !valid_name(name) {
        return Err(Error::invalid("Invalid PostgreSQL archive filename."));
    }
    if name.len() != 24 {
        if data.is_empty() || data.len() > 1024 * 1024 || std::str::from_utf8(data).is_err() {
            return Err(Error::invalid("Invalid PostgreSQL history record."));
        }
        return Ok(());
    }
    if data.len() < 1024 * 1024
        || !data.len().is_power_of_two()
        || data.len() > config.max_segment_bytes
    {
        return Err(Error::invalid(
            "WAL segment size exceeds its configured budget.",
        ));
    }
    // PostgreSQL 17's native long page header. Physical backups are architecture/
    // engine-version specific; detect native byte order rather than inventing WAL decoding.
    let little = u16::from_le_bytes(data[0..2].try_into().unwrap()) == 0xD116;
    let big = u16::from_be_bytes(data[0..2].try_into().unwrap()) == 0xD116;
    if !little && !big {
        return Err(Error::invalid(
            "Unsupported PostgreSQL WAL version; this helper verifies PostgreSQL 17 headers.",
        ));
    }
    let u16_at = |offset| {
        let bytes = data[offset..offset + 2].try_into().unwrap();
        if little {
            u16::from_le_bytes(bytes)
        } else {
            u16::from_be_bytes(bytes)
        }
    };
    let u32_at = |offset| {
        let bytes = data[offset..offset + 4].try_into().unwrap();
        if little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        }
    };
    let u64_at = |offset| {
        let bytes = data[offset..offset + 8].try_into().unwrap();
        if little {
            u64::from_le_bytes(bytes)
        } else {
            u64::from_be_bytes(bytes)
        }
    };
    let segment = u32_at(32) as usize;
    let block = u32_at(36);
    if u16_at(2) & 2 == 0
        || segment != data.len()
        || u64_at(24).to_string() != config.system_id
        || !block.is_power_of_two()
        || !(512..=65536).contains(&block)
    {
        return Err(Error::invalid(
            "WAL header does not match its cluster or segment size.",
        ));
    }
    let log = u32::from_str_radix(&name[8..16], 16).unwrap() as u64;
    let index = u32::from_str_radix(&name[16..24], 16).unwrap() as u64;
    let per_log = (1u64 << 32) / segment as u64;
    if index >= per_log || u64_at(8) != (log * per_log + index) * segment as u64 {
        return Err(Error::invalid(
            "WAL filename does not match its first page address.",
        ));
    }
    // Do not require page timeline == filename timeline: promotion can copy a
    // segment whose first pages legitimately belong to an earlier timeline.
    Ok(())
}
async fn settings(application: &ApplicationConfig) -> Result<(&Config, [u8; 32])> {
    let config = &application.postgres_archive;
    config
        .validate()
        .map_err(|_| Error::invalid("Invalid native PostgreSQL archive configuration."))?;
    if !config.enabled {
        return Err(Error::invalid(
            "Configure and enable postgres_archive first.",
        ));
    }
    let metadata = tokio::fs::symlink_metadata(&config.directory).await?;
    if !metadata.is_dir() {
        return Err(Error::invalid(
            "WAL archive must be an existing regular directory.",
        ));
    }
    let directory = tokio::fs::canonicalize(&config.directory).await?;
    if tokio::fs::canonicalize(&config.key_file)
        .await?
        .starts_with(&directory)
    {
        return Err(Error::invalid(
            "Keep the WAL recovery key outside its archive.",
        ));
    }
    Ok((config, super::encryption::read_key(&config.key_file).await?))
}
fn decode(config: &Config, key: &[u8; 32], name: &str, encoded: &[u8]) -> Result<Vec<u8>> {
    let plaintext = super::encryption::open(key, encoded, config.max_segment_bytes + 128)?;
    let mut parts = plaintext.splitn(4, |b| *b == b'\n');
    if parts.next() != Some(PREFIX)
        || parts.next() != Some(config.system_id.as_bytes())
        || parts.next() != Some(name.as_bytes())
    {
        return Err(Error::invalid(
            "WAL archive identity does not match this cluster and filename.",
        ));
    }
    let data = parts
        .next()
        .ok_or_else(|| Error::invalid("Invalid WAL archive record."))?;
    validate_record(config, name, data)?;
    Ok(data.to_vec())
}
pub async fn store(application: &ApplicationConfig, input: &Path, name: &str) -> Result<()> {
    if !valid_name(name) {
        return Err(Error::invalid("Invalid PostgreSQL archive filename."));
    }
    let (config, key) = settings(application).await?;
    if !tokio::fs::symlink_metadata(input).await?.is_file() {
        return Err(Error::invalid("WAL input must be a regular file."));
    }
    let data = backup::read_bounded(
        input,
        if name.len() == 24 {
            config.max_segment_bytes
        } else {
            1024 * 1024
        },
    )
    .await?;
    validate_record(config, name, &data)?;
    let output = config.directory.join(format!("{name}.wpwal"));
    if let Ok(metadata) = tokio::fs::symlink_metadata(&output).await {
        if !metadata.is_file() {
            return Err(Error::invalid("Unsafe WAL archive path."));
        }
        let existing = backup::read_bounded(
            &output,
            config.max_segment_bytes + 128 + super::encryption::OVERHEAD,
        )
        .await?;
        if decode(config, &key, name, &existing)? == data {
            return Ok(());
        }
        return Err(Error::invalid(
            "A different WAL record already occupies this filename.",
        ));
    }
    let mut plaintext = Vec::with_capacity(data.len() + 128);
    plaintext.extend_from_slice(PREFIX);
    plaintext.push(b'\n');
    plaintext.extend_from_slice(config.system_id.as_bytes());
    plaintext.push(b'\n');
    plaintext.extend_from_slice(name.as_bytes());
    plaintext.push(b'\n');
    plaintext.extend_from_slice(&data);
    let encoded = super::encryption::seal(&key, &plaintext)?;
    drop(plaintext);
    drop(data);
    super::recovery::publish(&output, &encoded, false)
        .map_err(|_| Error::invalid("WAL publication failed; the native archiver must retry."))?;
    let stored = backup::read_bounded(
        &output,
        config.max_segment_bytes + 128 + super::encryption::OVERHEAD,
    )
    .await?;
    decode(config, &key, name, &stored)?;
    Ok(())
}
pub async fn restore(application: &ApplicationConfig, name: &str, output: &Path) -> Result<()> {
    if !valid_name(name) {
        return Err(Error::invalid("Invalid PostgreSQL archive filename."));
    }
    let (config, key) = settings(application).await?;
    let path = config.directory.join(format!("{name}.wpwal"));
    if !tokio::fs::symlink_metadata(&path).await?.is_file() {
        return Err(Error::invalid("Unsafe WAL archive path."));
    }
    let encoded = backup::read_bounded(
        &path,
        config.max_segment_bytes + 128 + super::encryption::OVERHEAD,
    )
    .await?;
    let bytes = decode(config, &key, name, &encoded)?;
    // PostgreSQL's %p may already exist. Atomic replacement is intentional, after
    // complete authentication; no partial output is presented to the engine.
    super::recovery::publish(output, &bytes, true)
        .map_err(|_| Error::invalid("Cannot publish authenticated WAL for PostgreSQL."))?;
    Ok(())
}
