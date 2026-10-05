//! Local presentation/cache buckets. Neither region nor role labels authorize
//! resources; current session and entitlement checks remain authoritative.
use crate::{App, auth};
use axum::{body::Body, extract::ConnectInfo, http::Request};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub network: String,
    pub region: String,
}
fn network(value: &str) -> Option<(IpAddr, u32)> {
    let (address, prefix) = value.split_once('/')?;
    let address: IpAddr = address.parse().ok()?;
    let prefix = prefix.parse().ok()?;
    if prefix > if address.is_ipv4() { 32 } else { 128 } {
        return None;
    }
    Some((address, prefix))
}
fn contains(address: IpAddr, range: (IpAddr, u32)) -> bool {
    match (address, range.0) {
        (IpAddr::V4(a), IpAddr::V4(b)) => {
            let mask = u32::MAX.checked_shl(32 - range.1).unwrap_or(0);
            u32::from(a) & mask == u32::from(b) & mask
        }
        (IpAddr::V6(a), IpAddr::V6(b)) => {
            let mask = u128::MAX.checked_shl(128 - range.1).unwrap_or(0);
            u128::from(a) & mask == u128::from(b) & mask
        }
        _ => false,
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub role_variants: bool,
    pub region_networks: Vec<Region>,
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.region_networks.len() <= 64,
            "at most 64 local region networks"
        );
        for item in &self.region_networks {
            anyhow::ensure!(
                network(&item.network).is_some(),
                "region network must be IPv4/IPv6 CIDR"
            );
            anyhow::ensure!(
                !item.region.is_empty()
                    && item.region.len() <= 32
                    && item
                        .region
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                "region identifiers use at most 32 lowercase ASCII letters, digits or hyphens"
            );
        }
        Ok(())
    }
}
pub fn only_session_cookie(request: &Request<Body>) -> bool {
    request
        .headers()
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|value| {
            let parts: Vec<_> = value.split(';').filter(|p| !p.trim().is_empty()).collect();
            parts.len() == 1
                && parts[0]
                    .trim()
                    .strip_prefix("wpalt_session=")
                    .is_some_and(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        })
}
pub async fn annotate(app: &App, request: &mut Request<Body>) -> String {
    let region =
        request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .and_then(|peer| {
                app.config.variants.region_networks.iter().find(|r| {
                    contains(peer.0.ip(), network(&r.network).expect("validated network"))
                })
            })
            .map(|r| r.region.as_str())
            .unwrap_or("unknown");
    let role = if app.config.variants.role_variants && request.headers().contains_key("cookie") {
        auth::session(app, request.headers())
            .await
            .map(|s| s.user.role)
            .unwrap_or_else(|_| "anonymous".into())
    } else {
        "anonymous".into()
    };
    // Strip supplied labels even with optimization disabled. Only trusted native
    // peer information and current server-side session state supply these values.
    request.headers_mut().insert(
        "x-wpalt-local-region",
        region.parse().expect("validated region"),
    );
    request.headers_mut().insert(
        "x-wpalt-local-role",
        role.parse().expect("database constrained role"),
    );
    format!("{region}:{role}")
}
pub fn presentation(headers: &axum::http::HeaderMap, ctx: &mut crate::theme::Context) {
    for (key, header) in [
        ("region", "x-wpalt-local-region"),
        ("role", "x-wpalt-local-role"),
    ] {
        ctx.root["site"][key] = headers
            .get(header)
            .and_then(|v| v.to_str().ok())
            .unwrap_or(if key == "region" {
                "unknown"
            } else {
                "anonymous"
            })
            .into();
    }
}
