//! Content-addressed encrypted chunks. Every retained manifest independently
//! describes a complete recovery point; no parent snapshot chain is required.
use super::encryption;
use crate::{
    auth, backup,
    error::{Error, Result},
};
use ring::hmac;
use serde::{Deserialize, Serialize};
use std::path::Path;
const CHUNK: usize = 64 * 1024;
const PREFIX: &[u8] = b"wpalt-incremental-v1\n";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    length: usize,
    sha256: String,
    chunks: Vec<String>,
}
fn identity(key: &[u8; 32], bytes: &[u8]) -> String {
    hex::encode(hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), bytes).as_ref())
}
pub fn is_manifest(bytes: &[u8]) -> bool {
    bytes.starts_with(PREFIX)
}
fn object_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
pub async fn write(directory: &Path, key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let objects = directory.join("wpalt-objects");
    match tokio::fs::symlink_metadata(&objects).await {
        Ok(m) if !m.is_dir() => {
            return Err(Error::invalid(
                "Incremental object destination must be a real directory.",
            ));
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tokio::fs::create_dir(&objects).await?;
        }
        Err(e) => return Err(e.into()),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&objects, std::fs::Permissions::from_mode(0o700)).await?;
    }
    let mut manifest = Manifest {
        length: plaintext.len(),
        sha256: auth::digest(plaintext),
        chunks: vec![],
    };
    for bytes in plaintext.chunks(CHUNK) {
        let id = identity(key, bytes);
        let path = objects.join(&id);
        match tokio::fs::symlink_metadata(&path).await {
            Ok(m) => {
                if !m.is_file() {
                    return Err(Error::invalid("Unsafe incremental object path."));
                }
                let encoded = backup::read_bounded(&path, CHUNK + encryption::OVERHEAD).await?;
                if encryption::open(key, &encoded, CHUNK)? != bytes {
                    return Err(Error::invalid("Existing encrypted object is damaged."));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let encoded = encryption::seal(key, bytes)?;
                super::recovery::publish(&path, &encoded, false)
                    .map_err(|_| Error::invalid("Cannot publish incremental object."))?;
            }
            Err(e) => return Err(e.into()),
        }
        manifest.chunks.push(id);
    }
    let mut encoded = PREFIX.to_vec();
    encoded.extend(
        serde_json::to_vec(&manifest)
            .map_err(|_| Error::invalid("Invalid incremental manifest."))?,
    );
    Ok(encoded)
}
pub async fn assemble(
    directory: &Path,
    key: &[u8; 32],
    manifest: &[u8],
    limit: usize,
) -> Result<Vec<u8>> {
    if !is_manifest(manifest) || manifest.len() > 2 * 1024 * 1024 {
        return Err(Error::invalid("Invalid incremental manifest."));
    }
    let manifest: Manifest = serde_json::from_slice(&manifest[PREFIX.len()..])
        .map_err(|_| Error::invalid("Invalid incremental manifest."))?;
    if manifest.length > limit
        || manifest.chunks.len() != manifest.length.div_ceil(CHUNK)
        || manifest.chunks.iter().any(|s| !object_id(s))
    {
        return Err(Error::invalid(
            "Incremental recovery exceeds its budget or has invalid references.",
        ));
    }
    let objects = directory.join("wpalt-objects");
    if !tokio::fs::symlink_metadata(&objects).await?.is_dir() {
        return Err(Error::invalid(
            "Missing independent incremental object directory.",
        ));
    }
    let mut plaintext = Vec::with_capacity(manifest.length);
    for (i, id) in manifest.chunks.iter().enumerate() {
        let path = objects.join(id);
        if !tokio::fs::symlink_metadata(&path).await?.is_file() {
            return Err(Error::invalid("Missing or unsafe incremental object."));
        }
        let encoded = backup::read_bounded(&path, CHUNK + encryption::OVERHEAD).await?;
        let bytes = encryption::open(key, &encoded, CHUNK)?;
        let expected = (manifest.length - i * CHUNK).min(CHUNK);
        if bytes.len() != expected || identity(key, &bytes) != *id {
            return Err(Error::invalid("Incremental object identity mismatch."));
        }
        plaintext.extend(bytes);
    }
    if auth::digest(&plaintext) != manifest.sha256 {
        return Err(Error::invalid("Incremental recovery checksum mismatch."));
    }
    Ok(plaintext)
}

#[derive(Serialize)]
pub struct Cleanup {
    pub retained_packages: usize,
    pub unused_objects: usize,
    pub unused_bytes: u64,
    pub plan: String,
    pub deleted: bool,
}
/// Authenticate every recovery point before proposing any deletion. Bounded and
/// offline-only: the caller must hold the owning process lock, and no other host
/// may write this destination concurrently.
pub async fn cleanup(
    directory: &Path,
    key: &[u8; 32],
    limit: usize,
    execute: Option<&str>,
) -> Result<Cleanup> {
    use std::collections::BTreeSet;
    let mut protected = BTreeSet::new();
    let mut package_count = 0usize;
    let mut work = 0usize;
    let mut entries = tokio::fs::read_dir(directory).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("wpalt-") || !name.ends_with(".wpbackup") {
            continue;
        }
        if !entry.file_type().await?.is_file() {
            return Err(Error::invalid("Unsafe recovery point blocks cleanup."));
        }
        package_count += 1;
        if package_count > 365 {
            return Err(Error::invalid("Cleanup package budget exceeded."));
        }
        let encoded = backup::read_bounded(&entry.path(), limit + encryption::OVERHEAD).await?;
        let decoded = encryption::open(key, &encoded, limit)?;
        work = work
            .checked_add(decoded.len())
            .ok_or_else(|| Error::invalid("Cleanup budget exceeded."))?;
        if work > limit {
            return Err(Error::invalid(
                "Cleanup scan byte budget exceeded; no objects removed.",
            ));
        }
        if is_manifest(&decoded) {
            let manifest: Manifest = serde_json::from_slice(&decoded[PREFIX.len()..])
                .map_err(|_| Error::invalid("Invalid retained manifest."))?;
            // Full validation protects independent restorability, not just paths.
            let _ = assemble(directory, key, &decoded, limit).await?;
            work = work
                .checked_add(manifest.length)
                .ok_or_else(|| Error::invalid("Cleanup budget exceeded."))?;
            if work > limit {
                return Err(Error::invalid(
                    "Cleanup verification byte budget exceeded; no objects removed.",
                ));
            }
            protected.extend(manifest.chunks);
        }
    }
    if package_count == 0 {
        return Err(Error::invalid(
            "Keep a verified recovery point before pruning objects.",
        ));
    }
    let objects = directory.join("wpalt-objects");
    if !tokio::fs::symlink_metadata(&objects).await?.is_dir() {
        return Err(Error::invalid("Unsafe object directory."));
    }
    let mut candidates = vec![];
    let mut bytes = 0u64;
    let mut seen = 0usize;
    let mut entries = tokio::fs::read_dir(&objects).await?;
    while let Some(entry) = entries.next_entry().await? {
        seen += 1;
        if seen > 100_000 {
            return Err(Error::invalid("Cleanup object budget exceeded."));
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        if !object_id(&id) || !entry.file_type().await?.is_file() {
            return Err(Error::invalid("Unexpected object path blocks cleanup."));
        }
        if !protected.contains(&id) {
            let m = entry.metadata().await?;
            bytes = bytes
                .checked_add(m.len())
                .ok_or_else(|| Error::invalid("Cleanup size overflow."))?;
            candidates.push((id, m.len()));
        }
    }
    candidates.sort();
    let plan = auth::digest(
        serde_json::to_vec(&(protected, &candidates))
            .map_err(|_| Error::invalid("Invalid cleanup plan."))?
            .as_slice(),
    );
    if let Some(expected) = execute {
        if expected != plan {
            return Err(Error::invalid("Cleanup plan changed; preview again."));
        }
        for (id, _) in &candidates {
            tokio::fs::remove_file(objects.join(id)).await?;
        }
    }
    Ok(Cleanup {
        retained_packages: package_count,
        unused_objects: candidates.len(),
        unused_bytes: bytes,
        plan,
        deleted: execute.is_some(),
    })
}
