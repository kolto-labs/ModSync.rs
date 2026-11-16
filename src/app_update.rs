//! Check GitHub Releases for a newer version of an app and download its build
//! for this platform; read the notes for a version from a changelog.
//!
//! The caller names the repository and its current version, so any ModSync
//! frontend can use this. Downloads go through the engine's verified HTTPS
//! fetcher ([`crate::download::fetch_with_options`]) into a folder the caller
//! chooses; replacing the running program is left to the frontend.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const API_LIMIT: u64 = 2 * 1024 * 1024;
const NOTES_LIMIT: usize = 8000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// The version without a leading `v`.
    pub version: String,
    pub tag: String,
    /// The release page.
    pub url: String,
    pub notes: String,
    /// The download for this platform, when the release has one.
    pub asset_url: Option<String>,
    pub asset_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub current_version: String,
    pub available: bool,
    pub latest: Option<Release>,
}

#[derive(Deserialize)]
struct WireRelease {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<WireAsset>,
}

#[derive(Deserialize)]
struct WireAsset {
    #[serde(default)]
    name: String,
    #[serde(default)]
    browser_download_url: String,
}

/// Up to three numeric parts of a version (`v1.2.3-beta` is 1.2.3).
pub fn version_parts(version: &str) -> [u64; 3] {
    let mut parts = [0; 3];
    let numbers = version
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .take(3);
    for (slot, number) in parts.iter_mut().zip(numbers) {
        *slot = number.parse().unwrap_or(u64::MAX);
    }
    parts
}

/// Whether `latest` is a newer version than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    !latest.trim().is_empty() && version_parts(latest) > version_parts(current)
}

/// Whether a release file is the build for the platform this runs on.
pub fn is_platform_asset(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".sha256") || lower.ends_with(".sig") || lower.ends_with(".asc") {
        return false;
    }
    if cfg!(windows) {
        lower.ends_with(".exe")
    } else if cfg!(target_os = "macos") {
        lower.ends_with(".dmg") || (lower.contains("mac") && lower.ends_with(".zip"))
    } else {
        lower.contains("linux")
            && ![".deb", ".rpm", ".zip", ".tar.gz"]
                .iter()
                .any(|ext| lower.ends_with(ext))
    }
}

fn valid_repo(repo: &str) -> Result<()> {
    let parts: Vec<&str> = repo.split('/').collect();
    ensure!(
        parts.len() == 2
            && parts.iter().all(|part| {
                !part.is_empty()
                    && part.len() <= 100
                    && part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
                    && *part != "."
                    && *part != ".."
            }),
        "The repository must look like owner/name"
    );
    Ok(())
}

fn limit(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        format!(
            "{}...",
            text.chars().take(max).collect::<String>().trim_end()
        )
    }
}

/// The latest published (not draft, not pre-release) release of `repo`
/// compared with `current_version`.
pub fn check(repo: &str, current_version: &str, cancelled: &AtomicBool) -> Result<UpdateCheck> {
    valid_repo(repo)?;
    ensure!(!cancelled.load(Ordering::Acquire), "Cancelled");
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("ModSync/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .https_only(true)
        .build()?;
    let response = client
        .get(format!(
            "https://api.github.com/repos/{repo}/releases/latest"
        ))
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        // No release published yet.
        return Ok(UpdateCheck {
            current_version: current_version.to_owned(),
            available: false,
            latest: None,
        });
    }
    let response = response
        .error_for_status()
        .context("GitHub did not answer the update check")?;
    let mut body = Vec::new();
    response.take(API_LIMIT + 1).read_to_end(&mut body)?;
    ensure!(
        body.len() as u64 <= API_LIMIT,
        "The release list is too large"
    );
    ensure!(!cancelled.load(Ordering::Acquire), "Cancelled");
    let wire: WireRelease = serde_json::from_slice(&body).context("Unexpected release data")?;
    if wire.draft || wire.prerelease || wire.tag_name.trim().is_empty() {
        return Ok(UpdateCheck {
            current_version: current_version.to_owned(),
            available: false,
            latest: None,
        });
    }
    let version = wire
        .tag_name
        .trim()
        .trim_start_matches(['v', 'V'])
        .to_owned();
    let asset = wire.assets.iter().find(|asset| {
        is_platform_asset(&asset.name) && asset.browser_download_url.starts_with("https://")
    });
    let release = Release {
        version: version.clone(),
        tag: wire.tag_name.clone(),
        url: wire.html_url,
        notes: limit(wire.body.as_deref().unwrap_or_default().trim(), NOTES_LIMIT),
        asset_url: asset.map(|asset| asset.browser_download_url.clone()),
        asset_name: asset.map(|asset| asset.name.clone()),
    };
    Ok(UpdateCheck {
        current_version: current_version.to_owned(),
        available: is_newer(&version, current_version),
        latest: Some(release),
    })
}

/// Download a release's platform build into `directory` (created when
/// missing), replacing an earlier download of the same name. Returns the file.
pub fn download(release: &Release, directory: &Path, cancelled: &AtomicBool) -> Result<PathBuf> {
    let url = release
        .asset_url
        .as_deref()
        .context("This release has no download for your system")?;
    let name = release
        .asset_name
        .as_deref()
        .context("This release has no download for your system")?;
    let name = crate::download::one_component(name)?;
    std::fs::create_dir_all(directory)?;
    let destination = directory.join(name);
    match std::fs::remove_file(&destination) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    crate::download::fetch_with_options(
        url,
        &destination,
        None,
        cancelled,
        &crate::archive::Options::default(),
    )?;
    Ok(destination)
}

/// The `## [version]` (or `## version`) section of a Keep a Changelog style
/// file, without its heading. `None` when the version has no section.
pub fn changelog_section(changelog: &str, version: &str) -> Option<String> {
    let version = version.trim().trim_start_matches(['v', 'V']);
    let mut lines = changelog.lines();
    let heading = |line: &str| {
        let rest = line.strip_prefix("## ")?.trim();
        let rest = rest.strip_prefix('[').unwrap_or(rest);
        let rest = rest.strip_prefix(['v', 'V']).unwrap_or(rest);
        Some(
            rest.starts_with(version)
                && !rest[version.len()..].starts_with(|c: char| c.is_ascii_digit() || c == '.'),
        )
    };
    lines.find(|line| heading(line) == Some(true))?;
    let section: Vec<&str> = lines.take_while(|line| !line.starts_with("## ")).collect();
    let text = section.join("\n").trim().to_owned();
    (!text.is_empty()).then_some(text)
}
