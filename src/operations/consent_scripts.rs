//! Owner-reviewed local analytics scripts. These execute with page privileges, not in a sandbox.
use crate::{
    App, auth,
    business::engagement,
    error::{Error, Result},
};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::PathBuf,
};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub scripts: Vec<Script>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    pub id: String,
    pub label: String,
    pub purpose: String,
    pub path: PathBuf,
    pub sha256: String,
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.scripts.len() <= 8,
            "Consent supports at most eight local scripts"
        );
        let mut ids = BTreeSet::new();
        for script in &self.scripts {
            anyhow::ensure!(
                (1..=32).contains(&script.id.len())
                    && script
                        .id
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                    && ids.insert(&script.id),
                "Use unique lowercase script identifiers up to 32 bytes"
            );
            anyhow::ensure!(
                !script.label.trim().is_empty()
                    && script.label.len() <= 80
                    && !script.purpose.trim().is_empty()
                    && script.purpose.len() <= 500
                    && !script.label.chars().any(char::is_control)
                    && !script.purpose.chars().any(char::is_control),
                "Declare a readable script label and purpose"
            );
            anyhow::ensure!(
                script.path.is_absolute()
                    && script.sha256.len() == 64
                    && script
                        .sha256
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "Local scripts need an absolute path and lowercase SHA-256"
            );
        }
        Ok(())
    }
}
#[derive(Default)]
pub struct Scripts {
    pub manifest: String,
    entries: BTreeMap<String, Bytes>,
    declarations: Vec<serde_json::Value>,
}
impl Scripts {
    pub fn compile(config: &Config) -> anyhow::Result<Self> {
        config.validate()?;
        let mut output = Self::default();
        let mut total = 0usize;
        let mut manifest = Vec::new();
        for script in &config.scripts {
            let metadata = std::fs::symlink_metadata(&script.path)?;
            anyhow::ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "Local script must be a regular owner-managed file"
            );
            let mut bytes = Vec::new();
            std::fs::File::open(&script.path)?
                .take(256 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            total += bytes.len();
            anyhow::ensure!(
                !bytes.is_empty() && bytes.len() <= 256 * 1024 && total <= 1024 * 1024,
                "Local scripts exceed the 256 KiB/file or 1 MiB combined budget"
            );
            anyhow::ensure!(
                std::str::from_utf8(&bytes).is_ok() && auth::digest(&bytes) == script.sha256,
                "Local script checksum or UTF-8 is invalid"
            );
            let declaration = serde_json::json!({"id":script.id,"label":script.label,"purpose":script.purpose,"sha256":script.sha256});
            manifest.push(declaration.clone());
            output.declarations.push(declaration);
            output.entries.insert(script.id.clone(), Bytes::from(bytes));
        }
        if !manifest.is_empty() {
            output.manifest = auth::digest(&serde_json::to_vec(&manifest)?);
        }
        Ok(output)
    }
    pub fn purpose(&self, base: &str) -> String {
        if self.manifest.is_empty() {
            return base.to_owned();
        }
        // Full digest binds the durable grant to code, identifiers and declared purposes.
        format!("{base}\nOptional script manifest: {}", self.manifest)
    }
    pub fn declarations(&self) -> &[serde_json::Value] {
        &self.declarations
    }
}
pub async fn serve(
    State(app): State<App>,
    headers: HeaderMap,
    Path((manifest, id)): Path<(String, String)>,
) -> Result<Response> {
    if manifest != app.consent_scripts.manifest || manifest.is_empty() {
        return Err(Error::not_found());
    }
    let _guard = app.mutations.lock().await;
    let status = engagement::status(&app, &headers).await?;
    if status["consented"] != true {
        return Err(Error::forbidden());
    }
    let bytes = app
        .consent_scripts
        .entries
        .get(&id)
        .ok_or_else(Error::not_found)?
        .clone();
    Ok((
        [
            ("content-type", "text/javascript; charset=utf-8"),
            ("cache-control", "no-store"),
            ("cross-origin-resource-policy", "same-origin"),
        ],
        bytes,
    )
        .into_response())
}
