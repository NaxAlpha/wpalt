//! Independent owner-operated, read-only fleet observation. No origin-owned alerts.
use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    nodes: Vec<Node>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Node {
    name: String,
    origin: String,
    token_file: PathBuf,
}

fn private(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    let file = options
        .open(path)
        .map_err(|_| anyhow::anyhow!("Cannot read private fleet input."))?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= limit as u64,
        "Use a bounded private regular fleet input."
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            metadata.mode() & 0o077 == 0 && metadata.nlink() == 1,
            "Fleet input must be private without hard links."
        );
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "Fleet input exceeds budget.");
    Ok(bytes)
}

async fn json_response(
    client: &reqwest::Client,
    url: String,
    token: Option<&str>,
) -> Result<Value> {
    let mut request = client.get(url);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let mut response = request.send().await?;
    if response.status().is_redirection() {
        anyhow::bail!("Redirect refused.");
    }
    response = response.error_for_status()?;
    ensure!(response.status().is_success(), "Node refused observation.");
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            body.len() + chunk.len() <= 1024 * 1024,
            "Node response exceeds budget."
        );
        body.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&body)?)
}

/// All inputs are validated before networking. Failures isolate nodes and never echo transport secrets.
pub async fn inspect(path: &Path) -> Result<Value> {
    let manifest: Manifest = serde_json::from_slice(&private(path, 32768)?)
        .map_err(|_| anyhow::anyhow!("Invalid private fleet manifest."))?;
    ensure!(
        manifest.format == "wpalt-fleet-v1"
            && !manifest.nodes.is_empty()
            && manifest.nodes.len() <= 32,
        "Fleet supports one to 32 explicitly selected nodes."
    );
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut names = BTreeSet::new();
    let mut inputs = Vec::new();
    for node in manifest.nodes {
        ensure!(
            !node.name.is_empty()
                && node.name.len() <= 64
                && node
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && names.insert(node.name.clone()),
            "Use unique bounded fleet names."
        );
        let origin =
            url::Url::parse(&node.origin).map_err(|_| anyhow::anyhow!("Invalid fleet origin."))?;
        let local = origin.host_str().is_some_and(|host| {
            host.parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
        });
        ensure!(
            origin.origin().ascii_serialization() == node.origin
                && origin.username().is_empty()
                && origin.password().is_none()
                && (origin.scheme() == "https" || (origin.scheme() == "http" && local)),
            "Use HTTPS origins or literal loopback HTTP; no credentials, paths, queries or fragments."
        );
        let token = String::from_utf8(private(&parent.join(node.token_file), 128)?)
            .map_err(|_| anyhow::anyhow!("Invalid fleet credential."))?;
        let token = token.trim().to_owned();
        ensure!(
            token.len() == 64
                && token
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "Use an owner-issued native content-read credential."
        );
        inputs.push((node.name, node.origin, token));
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(5))
        .build()?;
    let mut tasks = tokio::task::JoinSet::new();
    let mut observations = Vec::new();
    for (name, origin, token) in inputs {
        while tasks.len() >= 4 {
            observations.push(
                tasks
                    .join_next()
                    .await
                    .ok_or_else(|| anyhow::anyhow!("Fleet task unavailable."))??,
            );
        }
        let client = client.clone();
        tasks.spawn(async move {
            let start = Instant::now();
            let observed = async {
                let health = json_response(&client, format!("{origin}/health"), None).await?;
                ensure!(health["status"]=="ok" && health["version"].as_str().is_some_and(|v| !v.is_empty() && v.len() <= 32 && v.bytes().all(|b|b.is_ascii_alphanumeric() || b".-+".contains(&b))), "Invalid native health.");
                // Existing grant verifies local account/password/origin/revocation, without copying bodies.
                let inventory = json_response(&client, format!("{origin}/api/v1/content"), Some(&token)).await?;
                ensure!(inventory["api_version"]==1 && inventory["content"].as_array().is_some_and(|v|v.len()<=25), "Invalid native API.");
                Ok::<Value,anyhow::Error>(json!({"version":health["version"],"authenticated":true,
                    "sample_items":inventory["content"].as_array().map(Vec::len),"more_items":!inventory["next"].is_null()}))
            }.await;
            let failure = observed.as_ref().err().map(|error| {
                if let Some(transport) = error.downcast_ref::<reqwest::Error>() {
                    if transport.is_timeout() { "timeout" }
                    else if transport.is_connect() { "connection-failed" }
                    else if transport.status().is_some_and(|s| matches!(s.as_u16(),401|403)) { "credential-refused" }
                    else if transport.status().is_some_and(|s|s.as_u16()==503) { "node-not-ready" }
                    else { "transport-or-http-failed" }
                } else {
                    match error.to_string().as_str() {
                        "Redirect refused." => "redirect-refused",
                        "Node response exceeds budget." => "oversized-response",
                        "Invalid native health." => "invalid-health",
                        "Invalid native API." => "invalid-api",
                        _ => "invalid-observation",
                    }
                }
            });
            json!({"name":name,"status":if observed.is_ok(){"ready"}else{"unavailable-or-unauthorized"},"failure_code":failure,
                "elapsed_ms":start.elapsed().as_millis() as u64,"observation":observed.ok()})
        });
    }
    while let Some(observation) = tasks.join_next().await {
        observations.push(observation?);
    }
    observations.sort_by_cached_key(|v| v["name"].as_str().unwrap_or_default().to_owned());
    let ready = observations.iter().all(|v| v["status"] == "ready");
    Ok(
        json!({"format":"wpalt-fleet-observation-v1","all_ready":ready,"nodes":observations,
        "boundary":"Independent read-only owner controller. Run outside monitored origins for outage observation. Uses explicitly delegated content-read credentials; may read private metadata but never includes content, credentials, endpoints or raw errors in reports. Does not mutate sites, orchestrate upgrades, repair nodes or send alerts. Schedule externally and inspect unsuccessful readiness exit status."}),
    )
}
