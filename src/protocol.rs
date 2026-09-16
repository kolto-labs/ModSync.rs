use anyhow::{Result, bail};
use reqwest::Url;
use serde::Serialize;
use std::net::{IpAddr, ToSocketAddrs};
use std::{
    io::{Read, Write},
    path::Path,
    time::Duration,
};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "scheme", rename_all = "lowercase")]
pub enum Link {
    Nxm {
        game_domain: String,
        mod_id: u64,
        file_id: u64,
        key: Option<String>,
        expires: u64,
        user_id: u64,
    },
    Modsync {
        action: String,
        game: Option<String>,
        instruction_url: String,
    },
}

pub fn parse(raw: &str) -> Result<Link> {
    let raw = raw.trim();
    let (scheme, remainder) = raw
        .split_once("://")
        .ok_or_else(|| anyhow::anyhow!("Unsupported protocol URL"))?;
    // url::Url does not consistently expose an authority for custom schemes.
    // Parse the shared authority/path/query syntax through https instead.
    let url = Url::parse(&format!("https://{remainder}"))?;
    match scheme.to_ascii_lowercase().as_str() {
        "nxm" => nxm(&url),
        "modsync" => modsync(&url),
        _ => bail!("Unsupported protocol URL"),
    }
}

pub fn safe_instruction_url(raw: &str) -> Result<Url> {
    let url = Url::parse(raw)?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("Instruction URL must use HTTP(S)");
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("Instruction URL host is required"))?;
    if host.eq_ignore_ascii_case("localhost")
        || host.ends_with(".localhost")
        || host.ends_with(".local")
    {
        bail!("Instruction URL cannot target a local host");
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        ensure_public_ip(ip)?;
    } else {
        let addresses: Vec<_> = (host, url.port_or_known_default().unwrap_or(443))
            .to_socket_addrs()?
            .collect();
        if addresses.is_empty() {
            bail!("Instruction URL host could not be resolved");
        }
        for address in addresses {
            ensure_public_ip(address.ip())?;
        }
    }
    Ok(url)
}

pub fn fetch_instruction(raw: &str, destination: &Path) -> Result<()> {
    const MAX_BYTES: u64 = 5 * 1024 * 1024;
    let url = safe_instruction_url(raw)?;
    if destination.exists() {
        bail!("Instruction destination already exists");
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;
    let mut response = client.get(url).send()?.error_for_status()?;
    safe_instruction_url(response.url().as_str())?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_BYTES)
    {
        bail!("Instruction file exceeds the 5242880 byte download limit");
    }
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Choose an instruction destination folder"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    let mut total = 0_u64;
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = response.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > MAX_BYTES {
            bail!("Instruction file exceeds the 5242880 byte download limit");
        }
        temporary.write_all(&buffer[..count])?;
    }
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(destination)
        .map_err(|error| error.error)?;
    Ok(())
}

fn ensure_public_ip(ip: IpAddr) -> Result<()> {
    let blocked = match ip {
        IpAddr::V4(ip) => {
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || (ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1]))
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_multicast()
        }
    };
    if blocked {
        bail!("Instruction URL cannot target a private address");
    }
    Ok(())
}

fn nxm(url: &Url) -> Result<Link> {
    let game_domain = url
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| anyhow::anyhow!("nxm URL game domain is required"))?
        .to_ascii_lowercase();
    let segments: Vec<_> = url
        .path_segments()
        .ok_or_else(|| anyhow::anyhow!("Invalid nxm URL"))?
        .collect();
    if segments.len() != 4
        || !segments[0].eq_ignore_ascii_case("mods")
        || !segments[2].eq_ignore_ascii_case("files")
    {
        bail!("Invalid nxm URL path");
    }
    let mod_id = segments[1].parse()?;
    let file_id = segments[3].parse()?;
    let mut key = None;
    let mut expires = 0;
    let mut user_id = 0;
    for (name, value) in url.query_pairs() {
        let value = value.into_owned();
        if name.eq_ignore_ascii_case("key") {
            key = Some(value.clone());
        }
        if name.eq_ignore_ascii_case("expires") {
            expires = value.parse().unwrap_or(0);
        }
        if name.eq_ignore_ascii_case("user_id") {
            user_id = value.parse().unwrap_or(0);
        }
    }
    Ok(Link::Nxm {
        game_domain,
        mod_id,
        file_id,
        key,
        expires,
        user_id,
    })
}

fn modsync(url: &Url) -> Result<Link> {
    let host = url
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| anyhow::anyhow!("modsync URL action is required"))?
        .to_ascii_lowercase();
    let segments: Vec<_> = url
        .path_segments()
        .ok_or_else(|| anyhow::anyhow!("Invalid modsync URL"))?
        .filter(|s| !s.is_empty())
        .collect();
    let games = ["kotor", "kotor2"];
    let actions = ["install", "open"];
    let (action, mut game) = if actions.contains(&host.as_str()) {
        if segments.len() > 1
            || segments
                .first()
                .is_some_and(|game| !games.iter().any(|known| known.eq_ignore_ascii_case(game)))
        {
            bail!("Invalid modsync URL path");
        }
        (
            host,
            segments.first().map(|game| (*game).to_ascii_lowercase()),
        )
    } else if games.contains(&host.as_str()) {
        if segments.len() != 1
            || !actions
                .iter()
                .any(|known| known.eq_ignore_ascii_case(segments[0]))
        {
            bail!("Invalid modsync URL path");
        }
        (segments[0].to_ascii_lowercase(), Some(host))
    } else {
        bail!("Unsupported modsync action or game");
    };
    let mut instruction = None;
    for (name, value) in url.query_pairs() {
        let value = value.into_owned();
        if (name.eq_ignore_ascii_case("url") || name.eq_ignore_ascii_case("instruction"))
            && instruction.is_none()
        {
            instruction = Some(value.clone());
        }
        if name.eq_ignore_ascii_case("game") {
            if !games.iter().any(|known| known.eq_ignore_ascii_case(&value)) {
                bail!("Unsupported modsync game");
            }
            if game.as_deref().is_some_and(|known| known != value) {
                bail!("Conflicting modsync game");
            }
            game = Some(value.to_ascii_lowercase());
        }
    }
    let instruction_url = Url::parse(
        &instruction.ok_or_else(|| anyhow::anyhow!("modsync instruction URL is required"))?,
    )?;
    if !matches!(instruction_url.scheme(), "http" | "https") {
        bail!("Instruction URL must use http or https");
    }
    Ok(Link::Modsync {
        action,
        game,
        instruction_url: instruction_url.into(),
    })
}
