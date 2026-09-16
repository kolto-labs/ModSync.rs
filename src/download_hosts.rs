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
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    Ok(if host.contains("deadlystream.com") {
        Provider::DeadlyStream
    } else if host.contains("mega.nz") {
        Provider::Mega
    } else if host.contains("nexusmods.com") {
        Provider::Nexus
    } else if host.contains("gamefront.com") {
        Provider::GameFront
    } else {
        Provider::Direct
    })
}
