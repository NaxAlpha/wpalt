//! Optional local abuse friction. Not a shared reputation/classification service.
use crate::{
    App, auth,
    error::{Error, Result},
};
use ring::{digest, hmac};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub proof_bits: u8,
    pub max_links: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            proof_bits: 12,
            max_links: 10,
        }
    }
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.proof_bits <= 16, "spam proof_bits must be 0..16");
        anyhow::ensure!(
            (1..=100).contains(&self.max_links),
            "spam max_links must be 1..100"
        );
        Ok(())
    }
}
#[derive(Default, Serialize, Deserialize)]
pub struct Used {
    entries: BTreeMap<String, i64>,
}
#[derive(Serialize)]
pub struct Challenge {
    pub token: String,
    pub bits: u8,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proof {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub solution: String,
    #[serde(default)]
    pub website: String,
}
fn message(resource: &str, expiry: i64, nonce: &str) -> String {
    format!("{resource}\n{expiry}\n{nonce}")
}
pub fn issue(app: &App, resource: &str) -> Result<Challenge> {
    if !app.config.spam.enabled {
        return Err(Error::not_found());
    }
    if resource.len() > 256 || !resource.starts_with("comment:") && !resource.starts_with("form:") {
        return Err(Error::invalid("Unsupported spam resource."));
    }
    let expiry = crate::now() + 600;
    let nonce = auth::random_token();
    let tag = hmac::sign(
        &hmac::Key::new(hmac::HMAC_SHA256, app.spam_secret.as_ref()),
        message(resource, expiry, &nonce).as_bytes(),
    );
    Ok(Challenge {
        token: format!("{expiry}.{nonce}.{}", hex::encode(tag.as_ref())),
        bits: app.config.spam.proof_bits,
    })
}
pub fn solved(token: &str, solution: &str, bits: u8) -> bool {
    if bits > 16
        || solution.len() > 10
        || solution.is_empty()
        || !solution.bytes().all(|b| b.is_ascii_digit())
    {
        return false;
    }
    let hash = digest::digest(&digest::SHA256, format!("{token}:{solution}").as_bytes());
    let first = u16::from_be_bytes([hash.as_ref()[0], hash.as_ref()[1]]);
    bits == 0 || first >> (16 - bits) == 0
}
pub async fn verify(app: &App, resource: &str, proof: &Proof, text: &str) -> Result<()> {
    if !app.config.spam.enabled {
        return Ok(());
    }
    if !proof.website.is_empty() || text.len() > 1024 * 1024 {
        return Err(Error::invalid(
            "Submission did not pass the local abuse check.",
        ));
    }
    let lower = text.to_ascii_lowercase();
    let links = lower.match_indices("https://").count() + lower.match_indices("http://").count();
    if links > app.config.spam.max_links {
        return Err(Error::invalid(
            "Too many links for this site's submission policy.",
        ));
    }
    if proof.token.len() > 256 {
        return Err(Error::forbidden());
    }
    let parts: Vec<_> = proof.token.split('.').collect();
    if parts.len() != 3 || parts[1].len() != 64 || parts[2].len() != 64 {
        return Err(Error::forbidden());
    }
    let expiry: i64 = parts[0].parse().map_err(|_| Error::forbidden())?;
    if expiry < crate::now() || expiry > crate::now() + 600 {
        return Err(Error::forbidden());
    }
    let tag = hex::decode(parts[2]).map_err(|_| Error::forbidden())?;
    hmac::verify(
        &hmac::Key::new(hmac::HMAC_SHA256, app.spam_secret.as_ref()),
        message(resource, expiry, parts[1]).as_bytes(),
        &tag,
    )
    .map_err(|_| Error::forbidden())?;
    if !solved(&proof.token, &proof.solution, app.config.spam.proof_bits) {
        return Err(Error::forbidden());
    }
    let mut used = app.spam_used.lock().await;
    used.entries.retain(|_, expiry| *expiry >= crate::now());
    if used.entries.len() >= 4096 || used.entries.contains_key(&proof.token) {
        return Err(Error::forbidden());
    }
    used.entries.insert(proof.token.clone(), expiry);
    Ok(())
}
