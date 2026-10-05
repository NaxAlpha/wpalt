//! Local rules and bounded peer rate limits. Forwarded headers are never trusted.
use crate::error::Error;
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Method, Request, StatusCode},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub denied_prefixes: Vec<String>,
    pub denied_peers: Vec<IpAddr>,
    pub requests_per_window: u32,
    pub window_seconds: u64,
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.denied_prefixes.len() <= 64 && self.denied_peers.len() <= 1024,
            "too many local protection rules"
        );
        for prefix in &self.denied_prefixes {
            anyhow::ensure!(
                prefix.starts_with('/')
                    && prefix.len() <= 256
                    && !prefix.contains(['?', '#', '%', '\\'])
                    && !prefix.contains(".."),
                "request prefixes must be literal safe paths"
            );
        }
        if self.enabled {
            anyhow::ensure!(
                (1..=10000).contains(&self.requests_per_window),
                "protection requests_per_window must be 1..10000"
            );
            anyhow::ensure!(
                (1..=86400).contains(&self.window_seconds),
                "protection window_seconds must be 1..86400"
            );
        }
        Ok(())
    }
}
#[derive(Default, Serialize, Deserialize)]
pub struct Limits {
    peers: BTreeMap<String, (i64, u32)>,
}
impl Limits {
    pub fn check(&mut self, config: &Config, request: &Request<Body>) -> crate::error::Result<()> {
        if !config.enabled {
            return Ok(());
        }
        let peer = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|info| info.0.ip());
        if peer.is_some_and(|ip| config.denied_peers.contains(&ip))
            || config.denied_prefixes.iter().any(|p| {
                request.uri().path() == p
                    || request
                        .uri()
                        .path()
                        .strip_prefix(p)
                        .is_some_and(|tail| p.ends_with('/') || tail.starts_with('/'))
            })
        {
            return Err(Error::forbidden());
        }
        // Health monitoring is bounded by the global concurrency/timeout guard.
        // Keep it available during operator diagnostics and local rate pressure.
        if request.method() == Method::GET && request.uri().path() == "/health" {
            return Ok(());
        }
        let key = peer.map_or_else(|| "unknown-local-peer".into(), |ip| ip.to_string());
        let now = crate::now();
        let window = config.window_seconds as i64;
        if !self.peers.contains_key(&key) && self.peers.len() >= 4096 {
            self.peers
                .retain(|_, (start, _)| now.saturating_sub(*start) < window);
            if self.peers.len() >= 4096 {
                // Fail closed instead of evicting an active attacker's counter.
                return Err(Error(
                    StatusCode::TOO_MANY_REQUESTS,
                    "Local request capacity reached. Try again later.",
                ));
            }
        }
        let (start, used) = self.peers.entry(key).or_insert((now, 0));
        if now.saturating_sub(*start) >= window {
            *start = now;
            *used = 0;
        }
        if *used >= config.requests_per_window {
            return Err(Error(
                StatusCode::TOO_MANY_REQUESTS,
                "Too many requests. Try again after the configured window.",
            ));
        }
        *used += 1;
        Ok(())
    }
}
