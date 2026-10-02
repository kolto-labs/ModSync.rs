use crate::Component;
use crate::Guide;
use anyhow::{Result, bail, ensure};
use reqwest::Url;
use serde::Serialize;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

mod fetch;
pub use fetch::fetch_instruction_with_cancel;

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

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum NxmResolveStatus {
    NotFound,
    Ambiguous,
    Matched,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct NxmComponentMatch {
    pub component_guid: String,
    pub component_name: String,
    pub registry_url: String,
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
    fetch_instruction_with_cancel(raw, destination, &std::sync::atomic::AtomicBool::new(false))
}

pub fn fetch_modsync_guide(raw: &str) -> Result<(Link, Guide)> {
    fetch_modsync_guide_with_cancel(raw, &AtomicBool::new(false))
}

/// Open a qualified, bounded handoff with cancellation through HTTP and around
/// guide decoding. The temporary document never survives this operation.
pub fn fetch_modsync_guide_with_cancel(raw: &str, cancelled: &AtomicBool) -> Result<(Link, Guide)> {
    let check_cancel = || -> Result<()> {
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "Protocol guide load cancelled"
        );
        Ok(())
    };
    check_cancel()?;
    crate::single_instance::validate_protocol_request(raw)
        .map_err(|_| anyhow::anyhow!("Invalid or unsupported protocol guide request"))?;
    check_cancel()?;
    let link = parse(raw)?;
    let Link::Modsync {
        instruction_url, ..
    } = &link
    else {
        bail!("Only modsync:// links contain an instruction URL");
    };
    check_cancel()?;
    let directory = tempfile::tempdir()?;
    let destination = directory.path().join("instruction.toml");
    check_cancel()?;
    fetch_instruction_with_cancel(instruction_url, &destination, cancelled)?;
    check_cancel()?;
    let guide = Guide::load_for_install(&destination)?;
    check_cancel()?;
    Ok((link, guide))
}

/// Resolves an nxm handoff against top-level guide components, like the C# resolver.
/// A link that matches more than one component is deliberately not assigned.
pub fn resolve_nxm(
    link: &Link,
    components: &[Component],
) -> (NxmResolveStatus, Option<NxmComponentMatch>) {
    let Link::Nxm {
        game_domain,
        mod_id,
        ..
    } = link
    else {
        return (NxmResolveStatus::NotFound, None);
    };

    let mut matched = None;
    for component in components {
        let Some(registry_url) = component
            .resource_registry
            .keys()
            .find(|url| matches_nexus_url(url, game_domain, *mod_id))
        else {
            continue;
        };
        if matched.is_some() {
            return (NxmResolveStatus::Ambiguous, None);
        }
        matched = Some(NxmComponentMatch {
            component_guid: component.guid.clone(),
            component_name: component.name.clone(),
            registry_url: registry_url.clone(),
        });
    }
    match matched {
        Some(found) => (NxmResolveStatus::Matched, Some(found)),
        None => (NxmResolveStatus::NotFound, None),
    }
}

fn matches_nexus_url(url: &str, game_domain: &str, mod_id: u64) -> bool {
    let Ok(url) = Url::parse(url) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    if !host.eq_ignore_ascii_case("nexusmods.com")
        && !host.to_ascii_lowercase().ends_with(".nexusmods.com")
    {
        return false;
    }
    let segments: Vec<_> = url
        .path_segments()
        .map(Iterator::collect)
        .unwrap_or_default();
    segments.len() >= 3
        && segments[1].eq_ignore_ascii_case("mods")
        && segments[0].eq_ignore_ascii_case(game_domain)
        && segments[2].parse::<u64>().ok() == Some(mod_id)
}

fn ensure_public_ip(ip: IpAddr) -> Result<()> {
    let blocked = match ip {
        IpAddr::V4(ip) => {
            ip.is_loopback()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ip.octets()[0] == 0
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || (ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1]))
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return ensure_public_ip(IpAddr::V4(mapped));
            }
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
