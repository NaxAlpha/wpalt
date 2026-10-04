//! Versioned authenticated backup envelope. Recovery keys are random 256-bit keys,
//! not passwords. Keep them independently from both the host and backup destination.
use crate::error::{Error, Result};
use rand::RngCore;
use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, Nonce, UnboundKey};
use std::path::Path;

const MAGIC: &[u8] = b"wpalt-encrypted-backup-v1\0";
const NONCE_BYTES: usize = 12;
const TAG_BYTES: usize = 16;
pub const OVERHEAD: usize = MAGIC.len() + NONCE_BYTES + TAG_BYTES;

pub fn generate_key() -> [u8; 32] {
    let mut key = [0; 32];
    rand::rngs::OsRng.fill_bytes(&mut key);
    key
}

pub async fn read_key(path: &Path) -> Result<[u8; 32]> {
    let metadata = tokio::fs::symlink_metadata(path).await?;
    if !metadata.is_file() {
        return Err(Error::invalid("Recovery key must be a regular file."));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(Error::invalid("Recovery key must be private (chmod 600)."));
        }
    }
    let raw = crate::backup::read_bounded(path, 65).await?;
    let text = std::str::from_utf8(&raw)
        .map_err(|_| Error::invalid("Invalid recovery key."))?
        .trim_end_matches(['\n', '\r']);
    let decoded = hex::decode(text).map_err(|_| Error::invalid("Invalid recovery key."))?;
    decoded
        .try_into()
        .map_err(|_| Error::invalid("Recovery key must contain 64 hexadecimal characters."))
}

fn cipher(key: &[u8; 32]) -> Result<LessSafeKey> {
    Ok(LessSafeKey::new(
        UnboundKey::new(&CHACHA20_POLY1305, key)
            .map_err(|_| Error::invalid("Cannot initialize backup encryption."))?,
    ))
}

pub fn seal(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    // A fresh cryptographic random 96-bit nonce for every package. Retention bounds
    // package counts; never derive a nonce from the clock, destination or filename.
    let mut nonce = [0; NONCE_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let mut ciphertext = plaintext.to_vec();
    cipher(key)?
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(MAGIC),
            &mut ciphertext,
        )
        .map_err(|_| Error::invalid("Backup encryption failed."))?;
    let mut encoded = Vec::with_capacity(plaintext.len() + OVERHEAD);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&nonce);
    encoded.extend_from_slice(&ciphertext);
    Ok(encoded)
}

pub fn open(key: &[u8; 32], encoded: &[u8], max_plaintext: usize) -> Result<Vec<u8>> {
    if encoded.len() < OVERHEAD
        || encoded.len() > max_plaintext.saturating_add(OVERHEAD)
        || !encoded.starts_with(MAGIC)
    {
        return Err(Error::invalid(
            "Invalid, unsupported or oversized encrypted backup.",
        ));
    }
    let nonce: [u8; NONCE_BYTES] = encoded[MAGIC.len()..MAGIC.len() + NONCE_BYTES]
        .try_into()
        .map_err(|_| Error::invalid("Invalid encrypted backup."))?;
    let mut ciphertext = encoded[MAGIC.len() + NONCE_BYTES..].to_vec();
    let plaintext = cipher(key)?
        .open_in_place(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(MAGIC),
            &mut ciphertext,
        )
        .map_err(|_| {
            Error::invalid("Backup authentication failed: wrong key or damaged package.")
        })?;
    let length = plaintext.len();
    ciphertext.truncate(length);
    Ok(ciphertext)
}
