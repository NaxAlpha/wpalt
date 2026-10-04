//! Bounded in-process public-response cache. Authentication and personalized routes
//! are excluded, never used as cache variants. M9 will add distributed coordination.
use axum::{
    body::{Body, Bytes},
    http::HeaderMap,
    response::Response,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub max_bytes: usize,
    pub max_entries: usize,
    pub ttl_seconds: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            max_bytes: 8 * 1024 * 1024,
            max_entries: 128,
            ttl_seconds: 60,
        }
    }
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (64 * 1024..=256 * 1024 * 1024).contains(&self.max_bytes),
            "cache max_bytes must be 64 KiB..256 MiB"
        );
        anyhow::ensure!(
            (1..=4096).contains(&self.max_entries),
            "cache max_entries must be 1..4096"
        );
        anyhow::ensure!(
            (1..=3600).contains(&self.ttl_seconds),
            "cache ttl_seconds must be 1..3600"
        );
        Ok(())
    }
}
struct Entry {
    bytes: Bytes,
    headers: HeaderMap,
    at: Instant,
    generation: u64,
}
#[derive(Default)]
pub struct Cache {
    entries: BTreeMap<String, Entry>,
    bytes: usize,
}
impl Cache {
    pub fn get(&mut self, path: &str, generation: u64, config: &Config) -> Option<Response> {
        let entry = self.entries.get(path)?;
        if entry.generation != generation
            || entry.at.elapsed() >= Duration::from_secs(config.ttl_seconds)
        {
            let old = self.entries.remove(path).unwrap();
            self.bytes -= old.bytes.len();
            return None;
        }
        let mut response = Response::new(Body::from(entry.bytes.clone()));
        *response.headers_mut() = entry.headers.clone();
        response
            .headers_mut()
            .insert("x-wpalt-cache", "hit".parse().unwrap());
        Some(response)
    }
    pub fn insert(
        &mut self,
        path: String,
        generation: u64,
        bytes: Bytes,
        mut headers: HeaderMap,
        config: &Config,
    ) {
        if bytes.len() > config.max_bytes {
            return;
        }
        headers.remove("x-request-id");
        headers.remove("content-length");
        if let Some(old) = self.entries.remove(&path) {
            self.bytes -= old.bytes.len();
        }
        while self.bytes + bytes.len() > config.max_bytes
            || self.entries.len() >= config.max_entries
        {
            let Some(key) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.at)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            let old = self.entries.remove(&key).unwrap();
            self.bytes -= old.bytes.len();
        }
        self.bytes += bytes.len();
        self.entries.insert(
            path,
            Entry {
                bytes,
                headers,
                at: Instant::now(),
                generation,
            },
        );
    }
    pub fn statistics(&self) -> (usize, usize) {
        (self.entries.len(), self.bytes)
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
}
// Mutation guard invalidates at both boundaries, including failed transactions.
// Holding the underlying lock for cacheable rendering prevents a commit between
// snapshot/render and cache insertion. No cached page outlives committed revocation.
pub struct Mutation<'a> {
    pub(crate) _guard: tokio::sync::MutexGuard<'a, ()>,
    pub(crate) generation: &'a AtomicU64,
}
impl Drop for Mutation<'_> {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
}
