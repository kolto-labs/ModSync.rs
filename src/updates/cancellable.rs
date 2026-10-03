//! Owned request futures: cancellation drops actual HTTP/body work, not just the
//! receiver for an orphaned blocking request. Used from application workers.
use super::{NEXUS_RETRY_AFTER_CAP, NexusClient, api_redirect_policy, redact_query};
use anyhow::{Result, ensure};
use serde_json::Value;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub(super) fn get(client: &mut NexusClient, path: &str, cancelled: &AtomicBool) -> Result<Value> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Nexus update check cancelled"
    );
    let url = client.base_url.join(path)?;
    get_url(client, url, cancelled)
}

pub(super) fn get_url(
    client: &mut NexusClient,
    url: reqwest::Url,
    cancelled: &AtomicBool,
) -> Result<Value> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Nexus request cancelled"
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        let transport = reqwest::Client::builder()
            .use_rustls_tls()
            .timeout(Duration::from_secs(60))
            .redirect(api_redirect_policy(&client.base_url))
            .build()?;
        let operation = async {
            for attempt in 0..2 {
                let mut request = transport
                    .get(url.clone())
                    .header("Application-Name", "ModSync")
                    .header("Application-Version", "2.0.0a1")
                    .header(reqwest::header::ACCEPT, "application/json")
                    .header(
                        reqwest::header::USER_AGENT,
                        "ModSync/2.0.0a1 (https://github.com/th3w1zard1/ModSync)",
                    );
                if let Some(key) = client
                    .api_key
                    .as_deref()
                    .filter(|key| !key.trim().is_empty())
                {
                    request = request.header("apikey", key);
                }
                let response = request.send().await.map_err(redact_query)?;
                let remaining = |name: &str| {
                    response
                        .headers()
                        .get(name)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse().ok())
                };
                client.daily_remaining =
                    remaining("X-RL-Daily-Remaining").or(client.daily_remaining);
                client.hourly_remaining =
                    remaining("X-RL-Hourly-Remaining").or(client.hourly_remaining);
                if attempt == 0 && response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    let delay = response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.trim().parse::<u64>().ok())
                        .map_or(NEXUS_RETRY_AFTER_CAP, Duration::from_secs)
                        .min(NEXUS_RETRY_AFTER_CAP);
                    drop(response);
                    if client.defer_rate_limits {
                        return Err(crate::download::transient(
                            Some(delay.as_secs()),
                            anyhow::anyhow!("Nexus Mods rate limit (HTTP 429) for {}", url.path()),
                        ));
                    }
                    tokio::time::sleep(delay).await;
                    continue;
                }
                let mut response = response.error_for_status().map_err(redact_query)?;
                const LIMIT: usize = 1024 * 1024;
                ensure!(
                    response
                        .content_length()
                        .is_none_or(|size| size <= LIMIT as u64),
                    "Nexus update response exceeds 1 MiB"
                );
                let mut body = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(redact_query)? {
                    ensure!(
                        chunk.len() <= LIMIT.saturating_sub(body.len()),
                        "Nexus update response exceeds 1 MiB"
                    );
                    body.extend_from_slice(&chunk);
                }
                ensure!(
                    !cancelled.load(Ordering::Acquire),
                    "Nexus update check cancelled"
                );
                return Ok(serde_json::from_slice(&body)?);
            }
            unreachable!("the second HTTP attempt returns a result")
        };
        tokio::pin!(operation);
        loop {
            ensure!(
                !cancelled.load(Ordering::Acquire),
                "Nexus update check cancelled"
            );
            tokio::select! {
                result = &mut operation => return result,
                _ = tokio::time::sleep(Duration::from_millis(40)) => (),
            }
        }
    });
    // OS resolver jobs cannot be forcibly interrupted. They hold no guide or
    // request future after cancellation; do not block the application worker on
    // their platform-dependent shutdown latency.
    runtime.shutdown_timeout(Duration::from_millis(100));
    result
}
