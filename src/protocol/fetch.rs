use super::ensure_public_ip;
use anyhow::{Context, Result, ensure};
use reqwest::Url;
use std::{
    io::Write,
    net::{IpAddr, SocketAddr},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const MAX_BYTES: usize = 5 * 1024 * 1024;

fn url_parts(url: &Url) -> Result<(String, u16)> {
    ensure!(
        matches!(url.scheme(), "http" | "https"),
        "Instruction URL must use HTTP(S)"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "Instruction URL must not contain credentials"
    );
    let host = url
        .host_str()
        .context("Instruction URL host is required")?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_owned();
    let lower = host.to_ascii_lowercase();
    ensure!(
        lower != "localhost" && !lower.ends_with(".localhost") && !lower.ends_with(".local"),
        "Instruction URL cannot target a local host"
    );
    Ok((
        host,
        url.port_or_known_default()
            .context("Instruction URL port is required")?,
    ))
}

fn redacted(mut error: reqwest::Error) -> reqwest::Error {
    if let Some(url) = error.url_mut() {
        url.set_query(None);
        url.set_fragment(None);
    }
    error
}

/// Read a bounded document through individually validated, pinned-address hops.
/// Cancellation drops HTTP/body futures and leaves no published destination.
/// OS DNS calls may finish after the bounded runtime shutdown; no request future
/// or destination file remains owned by them.
pub fn fetch_instruction_with_cancel(
    raw: &str,
    destination: &Path,
    cancelled: &AtomicBool,
) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Instruction fetch cancelled"
    );
    match std::fs::symlink_metadata(destination) {
        Ok(_) => anyhow::bail!("Instruction destination already exists"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let mut url = Url::parse(raw)?;
    url_parts(&url)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        let operation = async {
            for hop in 0..=10 {
                let (host, port) = url_parts(&url)?;
                let addresses: Vec<_> = if let Ok(ip) = host.parse::<IpAddr>() {
                    vec![SocketAddr::new(ip, port)]
                } else {
                    tokio::net::lookup_host((host.as_str(), port))
                        .await?
                        .collect()
                };
                ensure!(
                    !addresses.is_empty(),
                    "Instruction URL host could not be resolved"
                );
                ensure!(
                    addresses.len() <= 64,
                    "Instruction URL has too many resolved addresses"
                );
                for address in &addresses {
                    ensure_public_ip(address.ip())?;
                }
                // A proxy would perform its own resolution and bypass the pinned
                // public-address check. Instruction imports therefore connect direct.
                let client = reqwest::Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(60))
                    .redirect(reqwest::redirect::Policy::none())
                    .resolve_to_addrs(&host, &addresses)
                    .build()?;
                let response = client.get(url.clone()).send().await.map_err(redacted)?;
                if response.status().is_redirection() {
                    ensure!(hop < 10, "Instruction URL exceeded 10 redirects");
                    let location = response
                        .headers()
                        .get(reqwest::header::LOCATION)
                        .context("Instruction redirect has no Location header")?
                        .to_str()?;
                    let next = url.join(location)?;
                    ensure!(
                        url.scheme() != "https" || next.scheme() == "https",
                        "Instruction redirect cannot downgrade HTTPS"
                    );
                    url = next;
                    continue;
                }
                let mut response = response.error_for_status().map_err(redacted)?;
                ensure!(
                    response
                        .content_length()
                        .is_none_or(|length| length <= MAX_BYTES as u64),
                    "Instruction file exceeds the 5242880 byte download limit"
                );
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(redacted)? {
                    ensure!(
                        chunk.len() <= MAX_BYTES.saturating_sub(bytes.len()),
                        "Instruction file exceeds the 5242880 byte download limit"
                    );
                    bytes.extend_from_slice(&chunk);
                }
                return Ok(bytes);
            }
            unreachable!("redirect exhaustion returns an error")
        };
        tokio::pin!(operation);
        let deadline = tokio::time::sleep(Duration::from_secs(60));
        tokio::pin!(deadline);
        loop {
            ensure!(
                !cancelled.load(Ordering::Acquire),
                "Instruction fetch cancelled"
            );
            tokio::select! {
                result = &mut operation => return result,
                _ = &mut deadline => anyhow::bail!("Instruction fetch timed out"),
                _ = tokio::time::sleep(Duration::from_millis(40)) => (),
            }
        }
    });
    runtime.shutdown_timeout(Duration::from_millis(100));
    let bytes: Vec<u8> = result?;
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Instruction fetch cancelled"
    );
    let parent = destination
        .parent()
        .context("Choose an instruction destination folder")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Instruction fetch cancelled"
    );
    temporary
        .persist_noclobber(destination)
        .map_err(|error| error.error)?;
    Ok(())
}
