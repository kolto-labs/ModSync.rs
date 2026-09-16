use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

/// Download direct archive URLs only. Host landing pages remain a browser workflow.
pub fn fetch(
    url: &str,
    destination: &Path,
    sha256: Option<&str>,
    cancelled: &AtomicBool,
) -> Result<()> {
    fetch_with_options(
        url,
        destination,
        sha256,
        cancelled,
        &crate::archive::Options::default(),
    )
}

pub fn fetch_with_options(
    url: &str,
    destination: &Path,
    sha256: Option<&str>,
    cancelled: &AtomicBool,
    options: &crate::archive::Options,
) -> Result<()> {
    let url = reqwest::Url::parse(url)?;
    ensure!(url.scheme() == "https", "Downloads require HTTPS");
    ensure!(!destination.exists(), "Download destination already exists");
    let client = reqwest::blocking::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(300))
        .build()?;
    let mut response = client.get(url).send()?.error_for_status()?;
    ensure!(
        !response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|s| s.to_str().ok())
            .unwrap_or("")
            .contains("text/html"),
        "Server returned a web page. Use the host's browser download flow."
    );
    let mut temp = tempfile::NamedTempFile::new_in(
        destination
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Choose a destination folder"))?,
    )?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        ensure!(!cancelled.load(Ordering::Relaxed), "Download cancelled");
        let n = response.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
        temp.write_all(&buffer[..n])?;
    }
    if let Some(expected) = sha256 {
        ensure!(
            format!("{:x}", hash.finalize()).eq_ignore_ascii_case(expected),
            "SHA-256 mismatch; download discarded"
        );
    }
    crate::archive::verify(temp.path(), destination, options, cancelled)?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(destination).map_err(|e| e.error)?;
    Ok(())
}
