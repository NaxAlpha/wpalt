//! Exact-byte server attestations. Respondent identity is never inferred from a key.
use crate::{
    auth, backup,
    error::{Error, Result},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::{
    rand::SystemRandom,
    signature::{self, KeyPair},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::Read,
    path::{Path, PathBuf},
};

const DOMAIN: &[u8] = b"wpalt-response-document-attestation-v1\0";
const FORMAT: &str = "wpalt-response-document-v1";
const BUNDLE: &str = "wpalt-attested-document-bundle-v1";
pub const MAX_ARTIFACT: usize = 8 * 1024 * 1024;
pub const MAX_MANIFEST: usize = 256 * 1024;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub key_file: PathBuf,
    pub pdf_enabled: bool,
    pub chromium: PathBuf,
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.enabled || self.key_file.is_absolute(),
            "Document signing requires an absolute private key_file"
        );
        anyhow::ensure!(
            !self.pdf_enabled || (self.enabled && cfg!(unix) && self.chromium.is_absolute()),
            "PDF documents require signing enabled and an absolute owner-installed chromium path"
        );
        Ok(())
    }
}
pub struct Authority {
    key: Option<signature::Ed25519KeyPair>,
}
impl Authority {
    pub fn load(config: &Config) -> Result<Self> {
        if !config.enabled {
            return Ok(Self { key: None });
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
        }
        let file = options
            .open(&config.key_file)
            .map_err(|_| Error::invalid("Cannot read the configured private document key."))?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > 4096 {
            return Err(Error::invalid(
                "Document key must be a bounded regular private file.",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.mode() & 0o077 != 0 || metadata.nlink() != 1 {
                return Err(Error::invalid(
                    "Document key must have private permissions and exactly one link.",
                ));
            }
        }
        let mut bytes = Vec::new();
        file.take(4097).read_to_end(&mut bytes)?;
        if bytes.len() > 4096 {
            return Err(Error::invalid("Document key exceeds its byte limit."));
        }
        let key = signature::Ed25519KeyPair::from_pkcs8(&bytes).map_err(|_| {
            Error::invalid("Use an Ed25519 PKCS#8 document key created by the native key command.")
        })?;
        Ok(Self { key: Some(key) })
    }
    pub fn identity(&self) -> Option<String> {
        self.key
            .as_ref()
            .map(|key| auth::digest(key.public_key().as_ref()))
    }
    pub fn attest(
        &self,
        source: Value,
        media_type: &str,
        artifact: &[u8],
    ) -> Result<DocumentBundle> {
        let key=self.key.as_ref().ok_or_else(||Error::invalid("Document signing is disabled. Configure an owner-controlled key to generate attested documents."))?;
        if !["text/html", "application/pdf"].contains(&media_type)
            || artifact.is_empty()
            || artifact.len() > MAX_ARTIFACT
        {
            return Err(Error::invalid(
                "Document artifact is unsupported or exceeds its byte limit.",
            ));
        }
        let manifest = Manifest {
            format: FORMAT.into(),
            source,
            media_type: media_type.into(),
            artifact_bytes: artifact.len(),
            artifact_sha256: auth::digest(artifact),
            authority: "Server attestation only; respondent identity is unverified.".into(),
        };
        let raw = serde_json::to_vec(&manifest)
            .map_err(|_| Error::invalid("Cannot encode the document manifest."))?;
        if raw.len() > MAX_MANIFEST {
            return Err(Error::invalid(
                "Document source exceeds its manifest byte limit.",
            ));
        }
        let message = [DOMAIN, raw.as_slice()].concat();
        Ok(DocumentBundle {
            format: BUNDLE.into(),
            manifest: STANDARD.encode(raw),
            artifact: STANDARD.encode(artifact),
            public_key: hex::encode(key.public_key().as_ref()),
            signature: hex::encode(key.sign(&message).as_ref()),
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub source: Value,
    pub media_type: String,
    pub artifact_bytes: usize,
    pub artifact_sha256: String,
    pub authority: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentBundle {
    pub format: String,
    pub manifest: String,
    pub artifact: String,
    pub public_key: String,
    pub signature: String,
}

pub fn create_key(output: &Path) -> Result<String> {
    let bytes = signature::Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
        .map_err(|_| Error::invalid("Cannot generate a document signing key."))?;
    let key = signature::Ed25519KeyPair::from_pkcs8(bytes.as_ref())
        .map_err(|_| Error::invalid("Cannot validate the generated signing key."))?;
    backup::write_private(output, bytes.as_ref()).map_err(|_| {
        Error::invalid(
            "Choose a new private document key file; existing files are never overwritten.",
        )
    })?;
    Ok(auth::digest(key.public_key().as_ref()))
}

/// Caller supplies trust independently; the embedded public key is not trusted
/// merely because the bundle supplies a matching signature.
pub fn verify(bundle: &DocumentBundle, trusted_key_sha256: &str) -> Result<(Manifest, Vec<u8>)> {
    let invalid = || {
        Error::invalid("Document signature, trusted key, format or artifact integrity is invalid.")
    };
    if bundle.format != BUNDLE
        || bundle.public_key.len() != 64
        || bundle.signature.len() != 128
        || trusted_key_sha256.len() != 64
        || bundle.manifest.len() > MAX_MANIFEST.div_ceil(3) * 4
        || bundle.artifact.len() > MAX_ARTIFACT.div_ceil(3) * 4
    {
        return Err(invalid());
    }
    let public_key = hex::decode(&bundle.public_key).map_err(|_| invalid())?;
    if auth::digest(&public_key) != trusted_key_sha256 {
        return Err(invalid());
    }
    let raw = STANDARD.decode(&bundle.manifest).map_err(|_| invalid())?;
    if raw.len() > MAX_MANIFEST {
        return Err(invalid());
    }
    let signature = hex::decode(&bundle.signature).map_err(|_| invalid())?;
    let message = [DOMAIN, raw.as_slice()].concat();
    signature::UnparsedPublicKey::new(&signature::ED25519, &public_key)
        .verify(&message, &signature)
        .map_err(|_| invalid())?;
    let manifest: Manifest = serde_json::from_slice(&raw).map_err(|_| invalid())?;
    let artifact = STANDARD.decode(&bundle.artifact).map_err(|_| invalid())?;
    if manifest.format != FORMAT
        || !["text/html", "application/pdf"].contains(&manifest.media_type.as_str())
        || artifact.is_empty()
        || artifact.len() > MAX_ARTIFACT
        || artifact.len() != manifest.artifact_bytes
        || auth::digest(&artifact) != manifest.artifact_sha256
        || manifest.authority != "Server attestation only; respondent identity is unverified."
    {
        return Err(invalid());
    }
    Ok((manifest, artifact))
}
