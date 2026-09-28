use anyhow::{Result, bail};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Provider {
    DeadlyStream,
    Mega,
    Nexus,
    GameFront,
    Direct,
}

pub fn provider(url: &str) -> Result<Provider> {
    let url = reqwest::Url::parse(url.trim())?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("Downloads require an HTTP(S) URL");
    }
    let host = url
        .host_str()
        .unwrap_or("")
        .trim_end_matches('.')
        .to_ascii_lowercase();
    // Exact host or a dot-separated subdomain only, so `evilmega.nz` is not MEGA.
    let on = |domain: &str| {
        host == domain
            || host
                .strip_suffix(domain)
                .is_some_and(|prefix| prefix.ends_with('.'))
    };
    Ok(if on("deadlystream.com") {
        Provider::DeadlyStream
    } else if on("mega.nz") || on("mega.co.nz") {
        Provider::Mega
    } else if on("nexusmods.com") {
        Provider::Nexus
    } else if on("gamefront.com") {
        Provider::GameFront
    } else {
        Provider::Direct
    })
}
