//! Selected-guide download orchestration: one library owns which components qualify,
//! which URLs and filenames are wanted, what already exists, source order, retries,
//! cancellation, verification, guide write-back and FOMOD follow-up eligibility.
//! Frontends only format `AcquisitionEvent`s and the reports returned here.
pub mod filenames;
use crate::{
    Component, Guide,
    acquisition::{
        Acquire, Acquired, AcquisitionEvent, ArchiveTarget, ManualReason, OriginRequirement,
        PeerEndpoint, Phase, SharedDestination, SkipReason, Source, SourceKind,
    },
    archive,
    download::{self, Destination, FailureClass, FetchProgress},
    download_hosts::{self, Provider, deadlystream, mega},
    download_plan, download_queue, protocol,
    selection::DownloadCandidate,
    updates::NexusClient,
};
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

/// Minimum spacing of `Transferring` events per target; the first and last are always sent.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PeerMode {
    Off,
    /// Default (lead ruling R1). Effectively `Off` while the tunnel is down.
    #[default]
    Race,
    Fallback,
}

#[derive(Clone, Debug, Default)]
pub struct PlanOptions {
    /// Mod workspace. Existing detection reads it; `plan` never writes.
    pub source_dir: PathBuf,
    /// Explicit screen size for the resolution filter; never probed per call.
    pub resolution: Option<(u32, u32)>,
    pub peer_mode: PeerMode,
    /// User-entered peers only; guide-authored peers are ignored.
    pub user_peers: Vec<PeerEndpoint>,
    /// Presence only. Premium status is resolved once inside `run`.
    pub nexus_key_present: bool,
    /// Bounds for verifying existing archives.
    pub archive: archive::Options,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedSkip {
    pub component_guid: String,
    pub registry_url: String,
    pub filename: Option<String>,
    pub reason: SkipReason,
    /// The verified file for `AlreadyPresent`.
    pub path: Option<PathBuf>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquisitionPlan {
    pub source_dir: PathBuf,
    pub peer_mode: PeerMode,
    pub targets: Vec<ArchiveTarget>,
    pub skipped: Vec<PlannedSkip>,
    /// Authoring problems that do not stop the plan, e.g. malformed peer metadata.
    pub warnings: Vec<String>,
}

impl AcquisitionPlan {
    /// A one-target plan, as used by the nxm handoff. The target's filename must already
    /// satisfy the one-component rule.
    pub fn single(target: ArchiveTarget, source_dir: &Path) -> Result<Self> {
        target.destination(source_dir)?;
        Ok(Self {
            source_dir: source_dir.to_path_buf(),
            peer_mode: PeerMode::Off,
            targets: vec![target],
            skipped: Vec::new(),
            warnings: Vec::new(),
        })
    }
}

fn metadata_str<'a>(metadata: &'a Value, key: &str) -> Option<&'a str> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// C# `ResolutionFilterService`: the first `WxH` tag must equal the screen; untagged passes.
fn resolution_allows(resolution: Option<(u32, u32)>, text: &str) -> bool {
    let Some(screen) = resolution else {
        return true;
    };
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern = PATTERN.get_or_init(|| regex::Regex::new(r"(?i)(\d{3,4})x(\d{3,4})").unwrap());
    let Some(captures) = pattern.captures(text) else {
        return true;
    };
    match (captures[1].parse::<u32>(), captures[2].parse::<u32>()) {
        (Ok(w), Ok(h)) => (w, h) == screen,
        _ => true,
    }
}

fn sha256_file_with_cancel(path: &Path, cancelled: &AtomicBool) -> Result<String> {
    ensure!(!cancelled.load(Ordering::Relaxed), "Acquisition cancelled");
    let mut file = crate::paths::open_regular_read(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        ensure!(!cancelled.load(Ordering::Relaxed), "Acquisition cancelled");
        let n = file.read(&mut buffer)?;
        if n == 0 {
            return Ok(format!("{:x}", hash.finalize()));
        }
        hash.update(&buffer[..n]);
    }
}

/// ASCII case-insensitive, like C#'s `OrdinalIgnoreCase` `Files` dictionary.
fn find_existing(source_dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = source_dir.join(name);
    if std::fs::symlink_metadata(&exact).is_ok_and(|metadata| metadata.file_type().is_file()) {
        return Some(exact);
    }
    std::fs::read_dir(source_dir)
        .ok()?
        .flatten()
        .find(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
                && entry.file_type().is_ok_and(|t| t.is_file())
        })
        .map(|entry| entry.path())
}

/// A name match plus `archive::verify` (and the authored hash when present).
fn existing_valid(
    path: &Path,
    expected: Option<&str>,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> bool {
    if !std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file()) {
        return false;
    }
    if let Some(expected) = expected
        && !sha256_file_with_cancel(path, cancelled)
            .is_ok_and(|actual| actual.eq_ignore_ascii_case(expected))
    {
        return false;
    }
    archive::verify(path, path, options, cancelled).is_ok()
}

fn sources_for(
    url: &str,
    provider: Provider,
    peer: Option<Source>,
    key_present: bool,
) -> Vec<Source> {
    let origin = |requires| Source::Origin {
        url: url.to_owned(),
        provider,
        requires,
    };
    let manual = |reason| Source::Manual {
        url: url.to_owned(),
        reason,
    };
    let primary = match provider {
        Provider::Direct => origin(OriginRequirement::None),
        Provider::Nexus if key_present => origin(OriginRequirement::NexusPremium),
        Provider::Nexus => manual(ManualReason::FreeUserWait),
        // The anonymous page/session flow runs in `origin_once`; a gate it finds is terminal.
        Provider::DeadlyStream => origin(OriginRequirement::None),
        // Anonymous public single-file decryption runs in `origin_once`. A quota or access
        // gate it finds is terminal (no Peer, no Manual after it); any other failure
        // (folder, password, bad key, permanent origin error) falls to Peer, then to a
        // final Manual so the link stays reachable by hand.
        Provider::Mega => origin(OriginRequirement::None),
        // Not automated (Task 1c / open question 1).
        Provider::GameFront => manual(ManualReason::Unsupported),
    };
    match primary {
        Source::Origin { .. } if provider == Provider::Mega => std::iter::once(primary)
            .chain(peer)
            .chain(std::iter::once(manual(ManualReason::Unsupported)))
            .collect(),
        Source::Origin { .. } => std::iter::once(primary).chain(peer).collect(),
        Source::Manual {
            reason: ManualReason::Unsupported,
            ..
        } => peer.into_iter().chain(std::iter::once(primary)).collect(),
        // Gated: no Peer fallback (lead ruling R2).
        _ => vec![primary],
    }
}

/// Diagnostics identify a resource without retaining its authored credentials or URL.
fn diagnostic_resource(url: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(url.as_bytes());
    format!("Resource {:x}", digest)[..25].to_owned()
}

/// Share-eligible only with a 64-hex `ContentHashSHA256`, a `magnet:?xt=urn:btih:` magnet
/// and at most one archive under `Files`, since a torrent holds exactly one archive.
/// Anything malformed is a warning and the resource stays Origin-only.
fn peer_metadata(
    url: &str,
    metadata: &Value,
    file_count: usize,
    warnings: &mut Vec<String>,
) -> Option<(String, String)> {
    let url = diagnostic_resource(url);
    let sha256 = metadata_str(metadata, "ContentHashSHA256");
    let magnet = metadata_str(metadata, "Magnet")?;
    let Some(sha256) = sha256 else {
        warnings.push(format!(
            "{url}: Magnet without ContentHashSHA256; peers not used"
        ));
        return None;
    };
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        warnings.push(format!(
            "{url}: ContentHashSHA256 is not 64 hex digits; peers not used"
        ));
        return None;
    }
    let lower = magnet.to_ascii_lowercase();
    let btih = lower.strip_prefix("magnet:?").and_then(|query| {
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix("xt=urn:btih:"))
    });
    let valid_btih = btih.is_some_and(|hash| {
        (hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            || (hash.len() == 32
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b)))
    });
    if !valid_btih {
        warnings.push(format!(
            "{url}: Magnet is not a magnet:?xt=urn:btih: link; peers not used"
        ));
        return None;
    }
    if file_count > 1 {
        warnings.push(format!(
            "{url}: Files lists {file_count} archives but a torrent holds one; peers not used"
        ));
        return None;
    }
    Some((sha256.to_ascii_lowercase(), magnet.to_owned()))
}

/// Plans acquisition for `candidates`, which come only from
/// `selection::download_candidates(&guide.components, Scope::All)`. Each candidate must still
/// name the same selected component (index and GUID), so a stale or hand-built list cannot
/// download an unselected or reordered component. Only `ResourceRegistry` keys are
/// acquisition input; metadata `Link`/`DownloadLink` never are.
pub fn plan(
    guide: &Guide,
    candidates: &[DownloadCandidate],
    options: &PlanOptions,
) -> Result<AcquisitionPlan> {
    plan_with_cancel(guide, candidates, options, &AtomicBool::new(false))
}

/// Planning with cancellation during present-file hashing and archive verification.
pub fn plan_with_cancel(
    guide: &Guide,
    candidates: &[DownloadCandidate],
    options: &PlanOptions,
    cancelled: &AtomicBool,
) -> Result<AcquisitionPlan> {
    let mut filename_selection =
        download_plan::SelectionContext::new(Some(&options.source_dir), cancelled)?;
    let mut result = AcquisitionPlan {
        source_dir: options.source_dir.clone(),
        peer_mode: options.peer_mode,
        targets: Vec::new(),
        skipped: Vec::new(),
        warnings: Vec::new(),
    };
    let mut seen = BTreeSet::new();
    for candidate in candidates {
        let index = candidate.index;
        let component = guide
            .components
            .get(index)
            .with_context(|| format!("Download candidate {index} is not a guide component"))?;
        ensure!(
            component.guid == candidate.guid,
            "Download candidate {index} no longer names component {}",
            candidate.guid
        );
        ensure!(
            component.is_selected,
            "Download candidate {} is not selected",
            candidate.guid
        );
        if !seen.insert(index) {
            continue;
        }
        for (url, metadata) in &component.resource_registry {
            ensure!(
                !cancelled.load(Ordering::Relaxed),
                "Acquisition planning cancelled"
            );
            plan_url(
                component,
                url,
                metadata,
                options,
                &mut result,
                &mut filename_selection,
            )?;
            ensure!(
                !cancelled.load(Ordering::Relaxed),
                "Acquisition planning cancelled"
            );
        }
    }
    let links = shared_destinations(&result.targets, cancelled)?;
    for (target, link) in result.targets.iter_mut().zip(links) {
        target.shared_destination = link;
    }
    Ok(result)
}

/// Shared cancellable grouping for planning, discovery and execution admission.
/// Known names join globally; different known names on one URL join only through
/// an unknown-name member. Each transitive group retains its lowest plan index.
fn shared_destinations(
    targets: &[ArchiveTarget],
    cancelled: &AtomicBool,
) -> Result<Vec<Option<SharedDestination>>> {
    ensure!(
        targets.len() <= 100_000,
        "Acquisition exceeds 100000 targets"
    );
    fn root(parent: &mut [usize], mut index: usize) -> usize {
        while parent[index] != index {
            parent[index] = parent[parent[index]];
            index = parent[index];
        }
        index
    }
    #[derive(Default)]
    struct UrlGroup {
        unknown: Option<usize>,
        known: Vec<usize>,
    }
    let mut parent: Vec<usize> = (0..targets.len()).collect();
    let mut names = std::collections::BTreeMap::<String, usize>::new();
    let mut urls = std::collections::BTreeMap::<&str, UrlGroup>::new();
    let mut edges = 0usize;
    let mut join = |left, right| -> Result<()> {
        ensure!(!cancelled.load(Ordering::Acquire), "Acquisition cancelled");
        edges += 1;
        ensure!(
            edges <= 2_000_000,
            "Acquisition destination grouping exceeds 2000000 edges; reduce or split the plan"
        );
        let (a, b) = (root(&mut parent, left), root(&mut parent, right));
        parent[a.max(b)] = a.min(b);
        Ok(())
    };
    for (index, target) in targets.iter().enumerate() {
        ensure!(!cancelled.load(Ordering::Acquire), "Acquisition cancelled");
        if let Some(name) = &target.filename {
            let owner = *names.entry(name.to_ascii_lowercase()).or_insert(index);
            if owner != index {
                join(index, owner)?;
            }
        }
        let url = urls.entry(target.registry_url.as_str()).or_default();
        match (target.filename.is_some(), url.unknown) {
            (_, Some(owner)) => join(index, owner)?,
            (true, None) => url.known.push(index),
            (false, None) => {
                for owner in url.known.drain(..) {
                    join(index, owner)?;
                }
                url.unknown = Some(index);
            }
        }
    }
    let mut links = Vec::with_capacity(targets.len());
    for index in 0..targets.len() {
        ensure!(!cancelled.load(Ordering::Acquire), "Acquisition cancelled");
        let owner = root(&mut parent, index);
        links.push((owner != index).then(|| {
            if targets[owner].registry_url == targets[index].registry_url {
                SharedDestination::SameUrl(owner)
            } else {
                SharedDestination::DifferentUrl(owner)
            }
        }));
    }
    Ok(links)
}

fn plan_url(
    component: &Component,
    url: &str,
    metadata: &Value,
    options: &PlanOptions,
    plan: &mut AcquisitionPlan,
    filename_selection: &mut download_plan::SelectionContext<'_>,
) -> Result<()> {
    let cancelled = filename_selection.cancelled();
    let skip = |filename: Option<&str>, reason, path: Option<PathBuf>, detail: Option<String>| {
        PlannedSkip {
            component_guid: component.guid.clone(),
            registry_url: url.to_owned(),
            filename: filename.map(str::to_owned),
            reason,
            path,
            detail,
        }
    };
    if url.trim().is_empty() {
        plan.skipped
            .push(skip(None, SkipReason::InvalidUrl, None, None));
        return Ok(());
    }
    if !download_plan::url_enabled(component, url) {
        plan.skipped
            .push(skip(None, SkipReason::AllFilesFlaggedFalse, None, None));
        return Ok(());
    }
    if !resolution_allows(options.resolution, url) {
        plan.skipped
            .push(skip(None, SkipReason::ResolutionMismatch, None, None));
        return Ok(());
    }
    let provider = match download_hosts::provider(url) {
        Ok(provider) => provider,
        Err(error) => {
            plan.skipped.push(skip(
                None,
                SkipReason::InvalidUrl,
                None,
                Some(format!("{error:#}")),
            ));
            return Ok(());
        }
    };
    let keys: Vec<String> = metadata
        .get("Files")
        .and_then(Value::as_object)
        .map(|files| files.keys().cloned().collect())
        .unwrap_or_default();
    let mut names = Vec::new();
    for key in &keys {
        // The one-component rule runs before any join, for Existing detection or publish.
        if let Err(error) = download::one_component(key) {
            plan.skipped.push(skip(
                Some(key),
                SkipReason::UnsafeFilename,
                None,
                Some(format!("{error:#}")),
            ));
        } else if !resolution_allows(options.resolution, key) {
            plan.skipped
                .push(skip(Some(key), SkipReason::ResolutionMismatch, None, None));
        } else {
            names.push(key.clone());
        }
    }
    let expected = metadata_str(metadata, "ContentHashSHA256").map(str::to_owned);
    let peer = match (
        options.peer_mode,
        peer_metadata(url, metadata, keys.len(), &mut plan.warnings),
    ) {
        (PeerMode::Race | PeerMode::Fallback, Some((sha256, magnet))) => Some(Source::Peer {
            magnet,
            sha256,
            peers: options.user_peers.clone(),
        }),
        _ => None,
    };
    let sources = sources_for(url, provider, peer, options.nexus_key_present);
    let filenames: Vec<Option<String>> = if keys.is_empty() {
        vec![None]
    } else {
        filename_selection
            .select(component, url, &names)?
            .into_iter()
            .map(Some)
            .collect()
    };
    for filename in filenames {
        if let Some(name) = &filename
            && let Some(path) = find_existing(&options.source_dir, name)
            && existing_valid(&path, expected.as_deref(), &options.archive, cancelled)
        {
            plan.skipped.push(skip(
                Some(name),
                SkipReason::AlreadyPresent,
                Some(path),
                None,
            ));
            continue;
        }
        plan.targets.push(ArchiveTarget {
            component_guid: component.guid.clone(),
            registry_url: url.to_owned(),
            filename,
            expected_sha256: expected.clone(),
            sources: sources.clone(),
            shared_destination: None,
        });
    }
    Ok(())
}

/// Nexus file categories a bare mod page downloads, as C# `ResolveDownloadLinksAsync`:
/// OPTIONAL and every other category are left out.
const NEXUS_PAGE_CATEGORIES: [&str; 3] = ["MAIN", "UPDATE", "MISCELLANEOUS"];

/// Game domain and mod id of a Nexus mod-page URL, plus its `file_id` query, if any.
fn nexus_page(url: &str) -> Result<(String, u64, Option<u64>)> {
    let parsed = reqwest::Url::parse(url)?;
    let segments: Vec<&str> = parsed
        .path_segments()
        .map(Iterator::collect)
        .unwrap_or_default();
    ensure!(
        segments.len() >= 3 && segments[1].eq_ignore_ascii_case("mods"),
        "Nexus URL is not a mod page"
    );
    let mod_id = segments[2].parse().context("Nexus URL has no mod id")?;
    let file_id = parsed
        .query_pairs()
        .find(|(key, _)| key.eq_ignore_ascii_case("file_id"))
        .and_then(|(_, value)| value.parse::<u64>().ok());
    Ok((segments[0].to_ascii_lowercase(), mod_id, file_id))
}

/// The page URL of an unnamed target whose Nexus origin names no file: the only kind
/// `discover` expands.
fn bare_nexus_page(target: &ArchiveTarget) -> Option<(&str, OriginRequirement)> {
    if target.filename.is_some() {
        return None;
    }
    target.sources.iter().find_map(|source| match source {
        Source::Origin {
            url,
            provider: Provider::Nexus,
            requires,
        } if !url.trim_start().to_ascii_lowercase().starts_with("nxm://")
            && nexus_page(url).is_ok()
            // Any `file_id` key, even an unparsable one, names a file: never expand it.
            && !reqwest::Url::parse(url).is_ok_and(|parsed| {
                parsed
                    .query_pairs()
                    .any(|(key, _)| key.eq_ignore_ascii_case("file_id"))
            }) =>
        {
            Some((url.as_str(), *requires))
        }
        _ => None,
    })
}

/// The page URL of an unnamed target whose DeadlyStream origin names no file: the only
/// DeadlyStream kind `discover` expands.
fn bare_deadlystream_page(target: &ArchiveTarget) -> Option<&str> {
    if target.filename.is_some() {
        return None;
    }
    target.sources.iter().find_map(|source| match source {
        Source::Origin {
            url,
            provider: Provider::DeadlyStream,
            ..
        } if deadlystream::page_url(url).is_ok() => Some(url.as_str()),
        _ => None,
    })
}

/// Expands one bare DeadlyStream page target into one named target per selected archive
/// its page offers, as `expand_nexus_page` does. Each named target keeps the page as its
/// Origin, which `run` reads again to reach that archive, since links and cookies belong
/// to one session. `Err` leaves the target as planned.
fn expand_deadlystream_page(
    guide: &Guide,
    target: &ArchiveTarget,
    options: &PlanOptions,
    settings: &RunSettings,
    loopback_ca: Option<&download::LoopbackCa>,
    result: &mut AcquisitionPlan,
    filename_selection: &mut download_plan::SelectionContext<'_>,
) -> Result<()> {
    let cancelled = filename_selection.cancelled();
    let page = bare_deadlystream_page(target).context("Target has no DeadlyStream page")?;
    let component = guide
        .components
        .iter()
        .find(|component| component.guid == target.component_guid)
        .with_context(|| {
            format!(
                "Component {} is no longer in the guide",
                target.component_guid
            )
        })?;
    let files = deadlystream::resolve(
        page,
        settings.deadlystream_base.as_deref(),
        loopback_ca,
        cancelled,
    )?;
    let names: Vec<String> = files
        .iter()
        .map(|file| file.name.clone())
        .filter(|name| resolution_allows(options.resolution, name))
        .collect();
    let selected = filename_selection.select(component, &target.registry_url, &names)?;
    if selected.is_empty() {
        // As `plan` drops named files the component's Extract patterns do not need.
        result.warnings.push(format!(
            "{}: no archive of this DeadlyStream page is needed by this component's Extract instructions",
            diagnostic_resource(&target.registry_url)
        ));
        return Ok(());
    }
    // One authored hash identifies one archive; which sibling it means is unknown.
    ensure!(
        selected.len() == 1 || target.expected_sha256.is_none(),
        "The authored ContentHashSHA256 identifies one archive, but {} archives of this DeadlyStream page match; name the file under Files",
        selected.len()
    );
    let single = selected.len() == 1;
    for name in selected {
        let sources = target
            .sources
            .iter()
            .filter(|source| single || !matches!(source, Source::Peer { .. }))
            .cloned()
            .collect();
        if let Some(path) = find_existing(&result.source_dir, &name)
            && existing_valid(
                &path,
                target.expected_sha256.as_deref(),
                &options.archive,
                cancelled,
            )
        {
            result.skipped.push(PlannedSkip {
                component_guid: target.component_guid.clone(),
                registry_url: target.registry_url.clone(),
                filename: Some(name),
                reason: SkipReason::AlreadyPresent,
                path: Some(path),
                detail: None,
            });
            continue;
        }
        result.targets.push(ArchiveTarget {
            component_guid: target.component_guid.clone(),
            registry_url: target.registry_url.clone(),
            filename: Some(name),
            expected_sha256: target.expected_sha256.clone(),
            sources,
            shared_destination: None,
        });
    }
    if !single
        && target
            .sources
            .iter()
            .any(|s| matches!(s, Source::Peer { .. }))
    {
        result.warnings.push(format!(
            "{}: the page has several archives but a torrent holds one; peers not used",
            diagnostic_resource(&target.registry_url)
        ));
    }
    Ok(())
}

/// Expands one bare Nexus page target. `Err` leaves the target as planned.
fn expand_nexus_page(
    guide: &Guide,
    target: &ArchiveTarget,
    page: &str,
    options: &PlanOptions,
    client: &mut NexusClient,
    result: &mut AcquisitionPlan,
    filename_selection: &mut download_plan::SelectionContext<'_>,
) -> Result<()> {
    let cancelled = filename_selection.cancelled();
    let (game_domain, mod_id, _) = nexus_page(page)?;
    let component = guide
        .components
        .iter()
        .find(|component| component.guid == target.component_guid)
        .with_context(|| {
            format!(
                "Component {} is no longer in the guide",
                target.component_guid
            )
        })?;
    let files = client.get_mod_files_with_cancel(&game_domain, &mod_id.to_string(), cancelled)?;
    let eligible: Vec<_> = files
        .iter()
        .filter(|file| {
            NEXUS_PAGE_CATEGORIES
                .iter()
                .any(|category| file.category_name.eq_ignore_ascii_case(category))
        })
        .collect();
    ensure!(
        !eligible.is_empty(),
        "Nexus mod {mod_id} lists no MAIN, UPDATE or MISCELLANEOUS file"
    );
    for (index, file) in eligible.iter().enumerate() {
        download::one_component(&file.file_name).with_context(|| {
            format!(
                "Nexus mod {mod_id} lists an unsafe file name {:?}",
                file.file_name
            )
        })?;
        ensure!(
            file.file_id > 0,
            "Nexus mod {mod_id} lists {} without a file id",
            file.file_name
        );
        // Two eligible files with one name would publish to one path: never pick one.
        ensure!(
            !eligible[..index]
                .iter()
                .any(|other| other.file_name.eq_ignore_ascii_case(&file.file_name)),
            "Nexus mod {mod_id} lists more than one eligible file named {}; name the file id under the URL",
            file.file_name
        );
    }
    let names: Vec<String> = eligible
        .iter()
        .map(|file| file.file_name.clone())
        .filter(|name| resolution_allows(options.resolution, name))
        .collect();
    let selected = filename_selection.select(component, &target.registry_url, &names)?;
    if selected.is_empty() {
        // As `plan` drops named files the component's Extract patterns do not need.
        result.warnings.push(format!(
            "{}: no eligible file of Nexus mod {mod_id} is needed by this component's Extract instructions",
            diagnostic_resource(&target.registry_url)
        ));
        return Ok(());
    }
    // One authored hash identifies one archive; which sibling it means is unknown.
    ensure!(
        selected.len() == 1 || target.expected_sha256.is_none(),
        "The authored ContentHashSHA256 identifies one archive, but {} files of Nexus mod {mod_id} match; name the file under Files",
        selected.len()
    );
    let single = selected.len() == 1;
    for name in selected {
        let file = eligible
            .iter()
            .find(|file| file.file_name == name)
            .context("Selected file is not an eligible Nexus file")?;
        let mut url = reqwest::Url::parse(page)?;
        url.query_pairs_mut()
            .append_pair("file_id", &file.file_id.to_string());
        let sources = target
            .sources
            .iter()
            .filter(|source| single || !matches!(source, Source::Peer { .. }))
            .map(|source| match source {
                Source::Origin {
                    provider: Provider::Nexus,
                    requires,
                    ..
                } => Source::Origin {
                    url: url.to_string(),
                    provider: Provider::Nexus,
                    requires: *requires,
                },
                other => other.clone(),
            })
            .collect();
        if let Some(path) = find_existing(&result.source_dir, &name)
            && existing_valid(
                &path,
                target.expected_sha256.as_deref(),
                &options.archive,
                cancelled,
            )
        {
            result.skipped.push(PlannedSkip {
                component_guid: target.component_guid.clone(),
                registry_url: target.registry_url.clone(),
                filename: Some(name),
                reason: SkipReason::AlreadyPresent,
                path: Some(path),
                detail: None,
            });
            continue;
        }
        result.targets.push(ArchiveTarget {
            component_guid: target.component_guid.clone(),
            registry_url: target.registry_url.clone(),
            filename: Some(name),
            expected_sha256: target.expected_sha256.clone(),
            sources,
            shared_destination: None,
        });
    }
    if !single
        && target
            .sources
            .iter()
            .any(|s| matches!(s, Source::Peer { .. }))
    {
        result.warnings.push(format!(
            "{}: the page has several archives but a torrent holds one; peers not used",
            diagnostic_resource(&target.registry_url)
        ));
    }
    Ok(())
}

/// Execution-time page discovery; `plan` and dry-run never call it. DeadlyStream pages
/// expand too, see [`discover_with_loopback_ca`]. Each unnamed
/// target whose Nexus origin is a mod page without `file_id` becomes one named target per
/// selected MAIN, UPDATE or MISCELLANEOUS file, filtered by the component's `Files` flags
/// and Extract patterns as `plan` filters named files. Callers show and run the returned
/// plan, so events, rows and outcomes share its indices. A page that cannot be expanded
/// safely (API failure, no or duplicate eligible names, one authored hash for several
/// files) stays as planned, fails at run, and adds its reason to `warnings`.
pub fn discover(
    guide: &Guide,
    plan: &AcquisitionPlan,
    options: &PlanOptions,
    settings: &RunSettings,
    cancelled: &AtomicBool,
) -> Result<AcquisitionPlan> {
    discover_with_loopback_ca(guide, plan, options, settings, cancelled, None)
}

/// As [`discover`], with the run's per-invocation loopback CA, which a DeadlyStream
/// fixture base needs. An unnamed target whose DeadlyStream origin is a page becomes one
/// named target per archive the page offers that the component's `Files` flags and
/// Extract patterns select, over an anonymous session made here (never in `plan`). A page
/// a gate, missing CSRF key or changed layout stops stays as planned, fails or ends
/// manual at run, and adds its reason to `warnings`.
pub fn discover_with_loopback_ca(
    guide: &Guide,
    plan: &AcquisitionPlan,
    options: &PlanOptions,
    settings: &RunSettings,
    cancelled: &AtomicBool,
    loopback_ca: Option<&download::LoopbackCa>,
) -> Result<AcquisitionPlan> {
    let mut filename_selection =
        download_plan::SelectionContext::new(Some(&plan.source_dir), cancelled)?;
    let mut result = AcquisitionPlan {
        targets: Vec::new(),
        ..plan.clone()
    };
    let mut nexus: Option<NexusClient> = None;
    let mut premium: Option<bool> = None;
    for target in &plan.targets {
        if bare_deadlystream_page(target).is_some() {
            if cancelled.load(Ordering::SeqCst) {
                result.targets.push(target.clone());
                continue;
            }
            // All or nothing per page: a failure keeps no partial expansion.
            let mut expanded = AcquisitionPlan {
                source_dir: result.source_dir.clone(),
                peer_mode: result.peer_mode,
                targets: Vec::new(),
                skipped: Vec::new(),
                warnings: Vec::new(),
            };
            match expand_deadlystream_page(
                guide,
                target,
                options,
                settings,
                loopback_ca,
                &mut expanded,
                &mut filename_selection,
            ) {
                Ok(()) => {
                    result.targets.extend(expanded.targets);
                    result.skipped.extend(expanded.skipped);
                    result.warnings.extend(expanded.warnings);
                }
                Err(error) => {
                    if error.is::<download_plan::QualificationError>() {
                        return Err(error);
                    }
                    result
                        .warnings
                        .push(format!("Provider filename discovery failed: {error:#}"));
                    result.targets.push(target.clone());
                }
            }
            continue;
        }
        let Some((page, requires)) = bare_nexus_page(target) else {
            result.targets.push(target.clone());
            continue;
        };
        if cancelled.load(Ordering::SeqCst) {
            result.targets.push(target.clone());
            continue;
        }
        if nexus.is_none() {
            let key = settings.nexus_api_key.clone();
            nexus = Some(match &settings.nexus_api_base {
                Some(base) => NexusClient::with_base_url(key, base)?,
                None => NexusClient::new(key)?,
            });
        }
        let Some(client) = nexus.as_mut() else {
            unreachable!("the Nexus client was just created");
        };
        // A free, invalid or unvalidated key cannot download these files anyway: leave the
        // page as planned, so `run` reports its one gate, with no file-list request.
        if requires == OriginRequirement::NexusPremium {
            let premium = *premium.get_or_insert_with(|| {
                client
                    .validate_with_cancel(cancelled)
                    .is_ok_and(|result| result.is_valid && result.is_premium)
            });
            if !premium {
                result.targets.push(target.clone());
                continue;
            }
        }
        // All or nothing per page: a failure keeps no partial expansion.
        let mut expanded = AcquisitionPlan {
            source_dir: result.source_dir.clone(),
            peer_mode: result.peer_mode,
            targets: Vec::new(),
            skipped: Vec::new(),
            warnings: Vec::new(),
        };
        match expand_nexus_page(
            guide,
            target,
            page,
            options,
            client,
            &mut expanded,
            &mut filename_selection,
        ) {
            Ok(()) => {
                result.targets.extend(expanded.targets);
                result.skipped.extend(expanded.skipped);
                result.warnings.extend(expanded.warnings);
            }
            Err(error) => {
                if error.is::<download_plan::QualificationError>() {
                    return Err(error);
                }
                result
                    .warnings
                    .push(format!("Provider filename discovery failed: {error:#}"));
                result.targets.push(target.clone());
            }
        }
    }
    let links = shared_destinations(&result.targets, cancelled)?;
    for (target, link) in result.targets.iter_mut().zip(links) {
        target.shared_destination = link;
    }
    Ok(result)
}

pub struct RunSettings {
    pub workers: usize,
    /// Extra attempts for transient failures only.
    pub retries: usize,
    pub archive: archive::Options,
    pub nexus_api_key: Option<String>,
    /// Probe-only API base override (HTTPS, or loopback HTTP, enforced by `NexusClient`).
    pub nexus_api_base: Option<String>,
    /// Probe-only DeadlyStream fixture base: every DeadlyStream page, selection and archive
    /// request goes to this loopback HTTPS origin instead, and needs the run's loopback CA.
    /// Provider identity stays with the guide's URL. Never persisted.
    pub deadlystream_base: Option<String>,
    /// Probe-only MEGA API base (HTTPS loopback only): the API request and every download
    /// address it returns must be loopback HTTPS under the run's loopback CA, with
    /// proxies off. Provider identity stays with the guide's URL. Never persisted.
    pub mega_api_base: Option<String>,
    /// The Peer source (BitTorrent slice). `None` while the tunnel is down or sharing is
    /// off: every Peer step then fails closed and opens no sockets.
    pub peer: Option<Arc<dyn Acquire>>,
}

impl Default for RunSettings {
    fn default() -> Self {
        Self {
            workers: 4,
            retries: 1,
            archive: archive::Options::default(),
            nexus_api_key: None,
            nexus_api_base: None,
            deadlystream_base: None,
            mega_api_base: None,
            peer: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetStatus {
    Completed,
    ManualRequired,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetOutcome {
    pub target_index: usize,
    pub component_guid: String,
    pub registry_url: String,
    /// The published name; for a planned-unknown filename, the name the origin revealed.
    pub filename: Option<String>,
    pub status: TargetStatus,
    pub source: Option<SourceKind>,
    pub path: Option<PathBuf>,
    pub sha256: Option<String>,
    pub manual: Option<ManualRequirement>,
    pub error: Option<String>,
    /// The plan index of the earlier target in the same group whose result this one took
    /// without a request: the file it published, or its unfinished outcome for an
    /// identical source list. `None` when this target ran its own sources.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared_with: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualRequirement {
    pub url: String,
    pub reason: ManualReason,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunReport {
    /// One per target, in plan order.
    pub outcomes: Vec<TargetOutcome>,
    /// The plan's skips, carried so `record` and `fomod_followups` see present archives.
    pub skipped: Vec<PlannedSkip>,
    pub cancelled: bool,
}

struct RunContext<'a> {
    loopback_ca: Option<&'a download::LoopbackCa>,
    plan: &'a AcquisitionPlan,
    settings: &'a RunSettings,
    cancelled: &'a AtomicBool,
    events: mpsc::Sender<AcquisitionEvent>,
    nexus: Option<Mutex<NexusClient>>,
    /// One run-wide decision: Ok(true) premium, Ok(false) valid free, Err(text) failed.
    premium: Option<std::result::Result<bool, String>>,
}

fn validate_premium_with_retries(
    client: &Mutex<NexusClient>,
    settings: &RunSettings,
    cancelled: &AtomicBool,
) -> std::result::Result<bool, String> {
    let mut attempt = 0;
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err("Nexus key validation cancelled".into());
        }
        attempt += 1;
        // Release the client lock before a cancel-aware retry delay.
        let result = nexus_client_lock(client, cancelled)
            .and_then(|mut client| client.validate_with_cancel(cancelled));
        if cancelled.load(Ordering::Acquire) {
            return Err("Nexus key validation cancelled".into());
        }
        match result {
            Ok(result) if result.is_valid => return Ok(result.is_premium),
            Ok(_) => return Err("Nexus API key is missing, invalid or unauthorized".into()),
            Err(error) => {
                if cancelled.load(Ordering::Acquire) {
                    return Err("Nexus key validation cancelled".into());
                }
                let error = download::request_failure(error);
                let delay = (attempt <= settings.retries)
                    .then(|| download::transient_delay(&error, attempt))
                    .flatten();
                match delay {
                    Some(delay) if download_queue::sleep_unless_cancelled(delay, cancelled) => {}
                    _ => return Err(format!("Nexus key validation failed: {error:#}")),
                }
            }
        }
    }
}

pub fn run(
    plan: &AcquisitionPlan,
    settings: &RunSettings,
    cancelled: &AtomicBool,
    observe: impl FnMut(AcquisitionEvent) + Send,
) -> Result<RunReport> {
    run_with_loopback_ca(plan, settings, cancelled, observe, None)
}

/// Run with a per-invocation CA restricted to loopback archive HTTPS transfers.
pub fn run_with_loopback_ca(
    plan: &AcquisitionPlan,
    settings: &RunSettings,
    cancelled: &AtomicBool,
    observe: impl FnMut(AcquisitionEvent) + Send,
    loopback_ca: Option<&download::LoopbackCa>,
) -> Result<RunReport> {
    run_guarded(plan, settings, None, cancelled, observe, loopback_ca)
}

/// Execute with current guide authority for revalidating planning-time existing files.
/// Invalid or missing existing files require a fresh plan; no network or sweep starts.
pub fn run_with_source_context(
    plan: &AcquisitionPlan,
    settings: &RunSettings,
    guide: &Guide,
    cancelled: &AtomicBool,
    observe: impl FnMut(AcquisitionEvent) + Send,
    loopback_ca: Option<&download::LoopbackCa>,
) -> Result<RunReport> {
    run_guarded(plan, settings, Some(guide), cancelled, observe, loopback_ca)
}

fn run_guarded(
    plan: &AcquisitionPlan,
    settings: &RunSettings,
    guide: Option<&Guide>,
    cancelled: &AtomicBool,
    mut observe: impl FnMut(AcquisitionEvent) + Send,
    loopback_ca: Option<&download::LoopbackCa>,
) -> Result<RunReport> {
    ensure!(!cancelled.load(Ordering::Acquire), "Acquisition cancelled");
    ensure!(
        (1..=32).contains(&settings.workers),
        "Orchestrated acquisition requires 1 to 32 workers"
    );
    ensure!(
        settings.retries <= 10,
        "Orchestrated acquisition allows at most 10 retries"
    );
    ensure!(
        plan.targets.len().saturating_add(plan.skipped.len()) <= 100_000,
        "Acquisition plan exceeds 100000 entries"
    );
    let mut owners = std::collections::BTreeMap::new();
    if let Some(guide) = guide {
        ensure!(
            guide.components.len() <= 100_000,
            "Acquisition guide exceeds 100000 components"
        );
        for component in &guide.components {
            ensure!(!cancelled.load(Ordering::Acquire), "Acquisition cancelled");
            owners
                .entry(component.guid.to_ascii_lowercase())
                .and_modify(|owner| *owner = None)
                .or_insert(Some(component));
        }
    }
    let mut visits = 0usize;
    let mut visit = || -> Result<()> {
        ensure!(!cancelled.load(Ordering::Acquire), "Acquisition cancelled");
        visits += 1;
        ensure!(
            visits <= 2_000_000,
            "Existing-file ownership verification exceeds 2000000 visits"
        );
        Ok(())
    };
    let root_lock = crate::recovery::ModRootLock::acquire(&plan.source_dir)?;
    let original_root = std::path::absolute(&plan.source_dir)?;
    let mut canonical = plan.clone();
    canonical.source_dir = root_lock.root().to_path_buf();
    for skip in &mut canonical.skipped {
        if skip.reason != SkipReason::AlreadyPresent {
            continue;
        }
        ensure!(!cancelled.load(Ordering::Acquire), "Acquisition cancelled");
        ensure!(
            guide.is_some(),
            "Existing-file skips require run_with_source_context with the current guide; replan before acquisition"
        );
        let component = owners
            .get(&skip.component_guid.to_ascii_lowercase())
            .copied()
            .flatten()
            .context("Existing-file consumer is absent or ambiguous; replan acquisition")?;
        ensure!(
            component.is_selected,
            "Existing-file consumer is no longer selected; replan acquisition"
        );
        let metadata = component
            .resource_registry
            .get(&skip.registry_url)
            .context("Existing-file resource ownership changed; replan acquisition")?;
        let filename = download::one_component(
            skip.filename
                .as_deref()
                .context("Existing-file skip has no filename")?,
        )?;
        let authored = std::path::absolute(
            skip.path
                .as_ref()
                .context("Existing-file skip has no path")?,
        )?;
        let relative = authored
            .strip_prefix(&original_root)
            .context("Existing-file skip is outside its planned root")?;
        ensure!(
            relative.components().count() == 1
                && relative
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.eq_ignore_ascii_case(filename)),
            "Existing-file skip path does not identify its declared archive"
        );
        let path = root_lock.root().join(relative);
        root_lock.contains_destination(&path)?;
        ensure!(
            std::fs::symlink_metadata(&path)?.file_type().is_file(),
            "Existing-file skip is no longer an ordinary file; replan acquisition"
        );
        // Recheck authored ownership, not the planner's *unsatisfied* patterns:
        // this file now exists and can itself satisfy the Extract pattern.
        let mut known = None;
        if let Some(files) = metadata.get("Files").and_then(Value::as_object) {
            let mut enabled = files.is_empty();
            for (name, flag) in files {
                visit()?;
                enabled |= flag != &Value::Bool(false);
                if known.is_none() && name.eq_ignore_ascii_case(filename) {
                    known = Some((name, flag));
                }
            }
            ensure!(
                enabled,
                "Existing-file resource is no longer enabled; replan acquisition"
            );
        }
        let allowed = if let Some((_, flag)) = known {
            flag != &Value::Bool(false)
        } else {
            let mut has_extract = false;
            let mut matched = false;
            for instruction in component.instructions.iter().chain(
                component
                    .options
                    .iter()
                    .flat_map(|option| &option.instructions),
            ) {
                visit()?;
                if instruction.action != "Extract" {
                    continue;
                }
                has_extract = true;
                for source in &instruction.source {
                    visit()?;
                    let pattern = source
                        .replace("<<modDirectory>>\\", "")
                        .replace("<<modDirectory>>/", "");
                    let pattern = pattern.trim_start_matches(['\\', '/']);
                    let basename = Path::new(pattern)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("");
                    matched |= crate::reference_paths::pattern_matches(
                        &format!("<<modDirectory>>\\{filename}"),
                        source,
                    ) || crate::reference_paths::pattern_matches(filename, basename);
                }
            }
            !has_extract || matched
        };
        ensure!(
            allowed,
            "Existing-file archive is no longer selected by its consumer; replan acquisition"
        );
        let before = std::fs::symlink_metadata(&path)?;
        let valid = existing_valid(
            &path,
            metadata_str(metadata, "ContentHashSHA256"),
            &settings.archive,
            cancelled,
        );
        ensure!(!cancelled.load(Ordering::Acquire), "Acquisition cancelled");
        ensure!(
            valid,
            "Existing-file hash or archive integrity changed; replan acquisition"
        );
        root_lock.contains_destination(&path)?;
        let after = std::fs::symlink_metadata(&path)?;
        ensure!(
            after.file_type().is_file()
                && before.len() == after.len()
                && before.modified()? == after.modified()?,
            "Existing-file archive changed during revalidation; replan acquisition"
        );
        skip.path = Some(path);
    }
    let plan = &canonical;
    // A hand-built plan must link exactly as `plan` would, or two workers could write one
    // file again.
    let links = shared_destinations(&plan.targets, cancelled)?;
    for (index, (target, link)) in plan.targets.iter().zip(links).enumerate() {
        ensure!(
            target.shared_destination == link,
            "Download target {index} is linked as {:?}, but its file name and URL link it as {link:?}",
            target.shared_destination
        );
    }
    // Before any race of this run exists, so nothing swept here can be ours.
    sweep_stale_peer_staging(&plan.source_dir, std::time::SystemTime::now(), cancelled);
    let origins = || {
        plan.targets
            .iter()
            .flat_map(|t| &t.sources)
            .filter_map(|s| match s {
                Source::Origin {
                    provider: Provider::Nexus,
                    requires,
                    ..
                } => Some(*requires),
                _ => None,
            })
    };
    let nexus = if origins().next().is_some() {
        let key = settings.nexus_api_key.clone();
        let mut client = match &settings.nexus_api_base {
            Some(base) => NexusClient::with_base_url(key, base)?,
            None => NexusClient::new(key)?,
        };
        // A 429 comes back as a typed transient error; the cancel-aware retry loop waits
        // outside this mutex, so other workers are never blocked by one rate limit.
        client.defer_rate_limits();
        Some(Mutex::new(client))
    } else {
        None
    };
    // Resolve one premium decision per run after bounded transient retries.
    let premium = match &nexus {
        Some(client) if origins().any(|r| r == OriginRequirement::NexusPremium) => {
            Some(validate_premium_with_retries(client, settings, cancelled))
        }
        _ => None,
    };
    let (events, receiver) = mpsc::channel();
    let context = RunContext {
        loopback_ca,
        plan,
        settings,
        cancelled,
        events,
        nexus,
        premium,
    };
    let outcomes = thread::scope(|scope| {
        let context = &context;
        let pool = scope.spawn(move || {
            // Only group owners are queued. The owner's worker settles every other member
            // of its group after it, and any two targets that may publish one file are in
            // one group (`shared_destinations`), so they never run at the same time.
            let owners: Vec<_> = plan
                .targets
                .iter()
                .enumerate()
                .filter(|(_, target)| target.shared_destination.is_none())
                .collect();
            let active_workers = settings.workers.min(owners.len().max(1));
            download_queue::run_ordered(owners, active_workers, |_, (index, target)| {
                acquire_group(context, index, target)
            })
            .map(|finished| {
                let mut outcomes: Vec<TargetOutcome> = finished.into_iter().flatten().collect();
                outcomes.sort_by_key(|outcome| outcome.target_index);
                outcomes
            })
        });
        // Workers only send; this thread alone calls `observe`, so each target's events
        // arrive in the order its worker produced them.
        loop {
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(event) => observe(event),
                Err(mpsc::RecvTimeoutError::Timeout) if !pool.is_finished() => {}
                Err(_) => {
                    for event in receiver.try_iter() {
                        observe(event);
                    }
                    break;
                }
            }
        }
        pool.join().expect("download worker panicked")
    })?;
    Ok(RunReport {
        outcomes,
        skipped: plan.skipped.clone(),
        cancelled: cancelled.load(Ordering::Relaxed),
    })
}

/// The report of a run cancelled before its first target: one `Cancelled` outcome per
/// target, as `run` reports targets it never started. Existing-file skips are
/// omitted because this path has not revalidated them under the mod-root lock.
pub fn cancelled_report(plan: &AcquisitionPlan) -> RunReport {
    RunReport {
        outcomes: plan
            .targets
            .iter()
            .enumerate()
            .map(|(index, target)| outcome(index, target, TargetStatus::Cancelled))
            .collect(),
        skipped: plan
            .skipped
            .iter()
            .filter(|skip| skip.reason != SkipReason::AlreadyPresent)
            .cloned()
            .collect(),
        cancelled: true,
    }
}

fn outcome(index: usize, target: &ArchiveTarget, status: TargetStatus) -> TargetOutcome {
    TargetOutcome {
        target_index: index,
        component_guid: target.component_guid.clone(),
        registry_url: target.registry_url.clone(),
        filename: target.filename.clone(),
        status,
        source: None,
        path: None,
        sha256: None,
        manual: None,
        error: None,
        shared_with: None,
    }
}

/// Acquires a group owner, then each target linked to it, in plan order and in this
/// worker. A linked target takes, without a request, the file an earlier member
/// published under its name (for a name only the transfer reveals, from its URL), as a
/// present file is accepted by name. Otherwise it takes the unfinished outcome of an
/// earlier member that ran the same source list and asked for the same name (ignoring
/// ASCII case), or, like this target, for no name, since a repeat would ask the same
/// sources for the same file again. Otherwise it runs its own sources; a name its
/// transfer reveals that an earlier member already published is then adopted as a
/// present file ([`adopt_present`]).
fn acquire_group(
    context: &RunContext<'_>,
    owner: usize,
    target: &ArchiveTarget,
) -> Vec<TargetOutcome> {
    let first = acquire_target(context, owner, target);
    // The members that ran their own sources, with their outcomes.
    let mut ran = vec![(target, first.clone())];
    let mut outcomes = vec![first];
    for (index, linked) in context.plan.targets.iter().enumerate().skip(owner + 1) {
        if linked.shared_destination.map(SharedDestination::owner) != Some(owner) {
            continue;
        }
        let published = ran.iter().find(|(member, result)| {
            result.status == TargetStatus::Completed
                && match &linked.filename {
                    Some(name) => result
                        .filename
                        .as_deref()
                        .is_some_and(|published| published.eq_ignore_ascii_case(name)),
                    None => member.registry_url == linked.registry_url,
                }
        });
        let repeated = || {
            ran.iter().find(|(member, result)| {
                result.status != TargetStatus::Completed
                    && member.sources == linked.sources
                    // A known name selects the file (Nexus picks it by name), so only a
                    // member that asked for this very name made this request. A target
                    // without a name made a request without one: a member that asked
                    // for a name may have failed for that name alone.
                    && match (&linked.filename, &member.filename) {
                        (Some(name), Some(asked)) => asked.eq_ignore_ascii_case(name),
                        (None, None) => true,
                        _ => false,
                    }
            })
        };
        let outcome = match published.or_else(repeated) {
            Some((_, result)) => shared_outcome(context, index, linked, result),
            None => {
                let result = acquire_target(context, index, linked);
                ran.push((linked, result.clone()));
                result
            }
        };
        outcomes.push(outcome);
    }
    outcomes
}

/// A linked target's outcome from an earlier member's `result`. It keeps its own index,
/// component and registry URL, records `sharedWith`, and gets one event with its final
/// phase. Its own authored hash must also match a published file.
fn shared_outcome(
    context: &RunContext<'_>,
    index: usize,
    target: &ArchiveTarget,
    result: &TargetOutcome,
) -> TargetOutcome {
    let shared_with = Some(result.target_index);
    let mismatch = result.status == TargetStatus::Completed
        && target
            .expected_sha256
            .as_deref()
            .zip(result.sha256.as_deref())
            .is_some_and(|(expected, actual)| !expected.eq_ignore_ascii_case(actual));
    let outcome = if mismatch {
        TargetOutcome {
            error: Some(format!(
                "SHA-256 mismatch: the shared download {} does not match this component's ContentHashSHA256",
                result.path.as_deref().unwrap_or(Path::new("")).display()
            )),
            shared_with,
            ..outcome(index, target, TargetStatus::Failed)
        }
    } else {
        TargetOutcome {
            target_index: index,
            component_guid: target.component_guid.clone(),
            registry_url: target.registry_url.clone(),
            filename: match result.status {
                TargetStatus::Completed => result.filename.clone(),
                _ => target.filename.clone(),
            },
            shared_with,
            ..result.clone()
        }
    };
    let phase = match (outcome.status, &outcome.manual) {
        (TargetStatus::Completed, _) => Phase::Completed {
            path: outcome.path.clone().unwrap_or_default(),
            sha256: outcome.sha256.clone().unwrap_or_default(),
        },
        (TargetStatus::ManualRequired, Some(manual)) => Phase::ManualRequired {
            url: manual.url.clone(),
            reason: manual.reason,
        },
        (TargetStatus::Cancelled, _) => Phase::Cancelled,
        _ => Phase::SourceFailed {
            error: outcome.error.clone().unwrap_or_default(),
            will_fallback: false,
        },
    };
    let source = outcome
        .source
        .or_else(|| target.sources.first().map(Source::kind))
        .unwrap_or(SourceKind::Manual);
    emit(context, index, target, source, phase);
    outcome
}

fn emit(
    context: &RunContext<'_>,
    index: usize,
    target: &ArchiveTarget,
    source: SourceKind,
    phase: Phase,
) {
    let _ = context.events.send(AcquisitionEvent {
        target_index: index,
        component_guid: target.component_guid.clone(),
        filename: target.filename.clone(),
        source,
        phase,
    });
}

enum Step {
    Done(Acquired, SourceKind),
    Manual(String, ManualReason, SourceKind),
    Cancelled(SourceKind),
    /// Try the next source. The kind is the source that failed last.
    Failed(String, SourceKind),
    /// Ends the target now; never falls through to a later source (O3: a failed or
    /// unknown premium validation must not let Peer run; or a present file that
    /// [`adopt_present`] refused when a later source would reveal that same name again,
    /// see [`reveals_again`]).
    Terminal(String, SourceKind),
    /// The name the source revealed is already present. `acquire_sources` adopts that
    /// file ([`adopt_present`]) or refuses it; it is never replaced.
    Present(download::DestinationExists, SourceKind),
}

/// Premium validation for a Nexus origin returned an error rather than a known
/// free-or-premium status. Distinguished from `ManualReason` gates (nothing to click
/// through) and from ordinary transient failures (Peer must not run around it either).
#[derive(Debug)]
struct PremiumValidationFailed(String);

impl std::fmt::Display for PremiumValidationFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PremiumValidationFailed {}

/// A failed or unknown premium validation, or an nxm handoff whose key cannot authorize
/// its download. Both are terminal: no retry, no other source, no peer.
fn nexus_authorization_failed(error: &anyhow::Error) -> bool {
    error.chain().any(|e| {
        e.downcast_ref::<PremiumValidationFailed>().is_some()
            || e.downcast_ref::<crate::updates::NexusAuthorizationRefused>()
                .is_some()
            || e.downcast_ref::<crate::updates::NxmAuthorization>()
                .is_some()
    })
}

/// A never-started Peer racer (O2 denied or cancelled it before `Acquire::acquire` was
/// ever called). Carries no user-visible text: the coordinator must not emit any event
/// or report `Superseded` for it.
#[derive(Debug)]
struct PeerNeverStarted;

impl std::fmt::Display for PeerNeverStarted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Peer not started")
    }
}

impl std::error::Error for PeerNeverStarted {}

fn is_never_started(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|e| e.downcast_ref::<PeerNeverStarted>().is_some())
}

/// Whether an Origin error must keep a still-waiting Peer from starting: a gate (login,
/// captcha, free-wait, HTML landing), or a failed/unknown premium validation (O3).
/// A present filename is settled through verification; refusal leaves Peer eligible.
fn origin_denies_peer(error: &anyhow::Error) -> bool {
    if nexus_authorization_failed(error) {
        return true;
    }
    matches!(
        download::classify(error),
        Some(FailureClass::ManualRequired { reason, .. }) if reason.is_gate()
    )
}

fn acquire_target(context: &RunContext<'_>, index: usize, target: &ArchiveTarget) -> TargetOutcome {
    // Cleanup failures from a race; reported on whatever outcome the target reaches.
    let mut notes = Vec::new();
    let mut outcome = acquire_sources(context, index, target, &mut notes);
    if !notes.is_empty() {
        outcome.error = Some(
            outcome
                .error
                .into_iter()
                .chain(notes)
                .collect::<Vec<_>>()
                .join("; "),
        );
    }
    outcome
}

fn acquire_sources(
    context: &RunContext<'_>,
    index: usize,
    target: &ArchiveTarget,
    notes: &mut Vec<String>,
) -> TargetOutcome {
    let first = target
        .sources
        .first()
        .map_or(SourceKind::Manual, Source::kind);
    if context.cancelled.load(Ordering::Relaxed) {
        emit(context, index, target, first, Phase::Cancelled);
        return outcome(index, target, TargetStatus::Cancelled);
    }
    emit(context, index, target, first, Phase::Resolving);
    let mut last_error = None;
    let mut position = 0;
    while position < target.sources.len() {
        let source = &target.sources[position];
        let rest = &target.sources[position + 1..];
        let peer_next = matches!(rest.first(), Some(Source::Peer { .. }));
        let race = matches!(source, Source::Origin { .. })
            && peer_next
            && context.plan.peer_mode == PeerMode::Race
            && context.settings.peer.is_some();
        let step = if context.cancelled.load(Ordering::Relaxed) {
            Step::Cancelled(source.kind())
        } else if race {
            position += 1;
            let (step, race_notes) = race_origin_peer(context, index, target, source);
            notes.extend(race_notes);
            step
        } else {
            single_source(context, index, target, source)
        };
        position += 1;
        let step = match step {
            // A cancelled run never completes a target by adoption.
            Step::Present(_, kind) if context.cancelled.load(Ordering::SeqCst) => {
                Step::Cancelled(kind)
            }
            Step::Present(present, kind) => match adopt_present(context, target, &present) {
                _ if context.cancelled.load(Ordering::SeqCst) => Step::Cancelled(kind),
                Ok(acquired) => Step::Done(acquired, kind),
                Err(error) if reveals_again(source, &target.sources[position..]) => {
                    Step::Terminal(error, kind)
                }
                // A later source that reveals its own name may find another file.
                Err(error) => Step::Failed(error, kind),
            },
            step => step,
        };
        let more = position < target.sources.len();
        match step {
            Step::Done(acquired, kind) => {
                emit(
                    context,
                    index,
                    target,
                    kind,
                    Phase::Completed {
                        path: acquired.path.clone(),
                        sha256: acquired.sha256.clone(),
                    },
                );
                return TargetOutcome {
                    filename: acquired
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(str::to_owned),
                    source: Some(kind),
                    path: Some(acquired.path),
                    sha256: Some(acquired.sha256),
                    ..outcome(index, target, TargetStatus::Completed)
                };
            }
            Step::Manual(url, reason, kind) => {
                // Gated reasons end the target; Unsupported continues only to a later source.
                if !reason.is_gate() && more {
                    last_error = Some(format!("{reason:?}"));
                    continue;
                }
                emit(
                    context,
                    index,
                    target,
                    kind,
                    Phase::ManualRequired {
                        url: url.clone(),
                        reason,
                    },
                );
                return TargetOutcome {
                    source: Some(kind),
                    manual: Some(ManualRequirement { url, reason }),
                    error: last_error,
                    ..outcome(index, target, TargetStatus::ManualRequired)
                };
            }
            Step::Cancelled(kind) => {
                emit(context, index, target, kind, Phase::Cancelled);
                return TargetOutcome {
                    source: Some(kind),
                    ..outcome(index, target, TargetStatus::Cancelled)
                };
            }
            Step::Failed(error, kind) => {
                emit(
                    context,
                    index,
                    target,
                    kind,
                    Phase::SourceFailed {
                        error: error.clone(),
                        will_fallback: more,
                    },
                );
                last_error = Some(error);
            }
            Step::Present(..) => unreachable!("a present file is adopted or refused above"),
            Step::Terminal(error, kind) => {
                emit(
                    context,
                    index,
                    target,
                    kind,
                    Phase::SourceFailed {
                        error: error.clone(),
                        will_fallback: false,
                    },
                );
                return TargetOutcome {
                    error: Some(error),
                    ..outcome(index, target, TargetStatus::Failed)
                };
            }
        }
    }
    TargetOutcome {
        error: last_error.or_else(|| Some("No acquisition source".into())),
        ..outcome(index, target, TargetStatus::Failed)
    }
}

/// `cancelled` is the run's flag: a present file found after a cancel is never offered
/// for adoption.
fn step_from_error(error: anyhow::Error, kind: SourceKind, cancelled: &AtomicBool) -> Step {
    if cancelled.load(Ordering::SeqCst) {
        return Step::Cancelled(kind);
    }
    if nexus_authorization_failed(&error) {
        return Step::Terminal(format!("{error:#}"), kind);
    }
    if let Some(present) = download::destination_exists(&error) {
        if cancelled.load(Ordering::SeqCst) {
            return Step::Cancelled(kind);
        }
        return Step::Present(present.clone(), kind);
    }
    match download::classify(&error) {
        Some(FailureClass::ManualRequired { url, reason }) => {
            Step::Manual(url.clone(), *reason, kind)
        }
        Some(FailureClass::Cancelled) => Step::Cancelled(kind),
        _ => Step::Failed(format!("{error:#}"), kind),
    }
}

/// Takes the file already present under the name a source revealed, by the rule a named
/// target follows when it is `AlreadyPresent`: the target's own `ContentHashSHA256` must
/// match, and the archive must verify. With no authored hash, the present file must hold
/// exactly the bytes the source delivered; otherwise nothing says which bytes this
/// component wants, so it fails. The present file is never replaced or removed.
fn adopt_present(
    context: &RunContext<'_>,
    target: &ArchiveTarget,
    present: &download::DestinationExists,
) -> std::result::Result<Acquired, String> {
    let path = &present.path;
    let contained = || -> Result<()> {
        ensure!(
            std::fs::symlink_metadata(path)?.file_type().is_file(),
            "Existing destination is a symlink or special file"
        );
        let root = std::fs::canonicalize(&context.plan.source_dir)?;
        let actual = std::fs::canonicalize(path)?;
        ensure!(
            actual.parent() == Some(root.as_path()),
            "Existing destination is outside the captured mod workspace"
        );
        Ok(())
    };
    contained().map_err(|error| format!("Existing destination cannot be adopted: {error:#}"))?;
    let actual = sha256_file_with_cancel(path, context.cancelled).map_err(|error| {
        format!(
            "The present file {} is unreadable: {error:#}",
            path.display()
        )
    })?;
    match target.expected_sha256.as_deref() {
        Some(expected) if !actual.eq_ignore_ascii_case(expected) => {
            return Err(format!(
                "SHA-256 mismatch: the present file {} does not match this component's ContentHashSHA256; it was not replaced",
                path.display()
            ));
        }
        None if !actual.eq_ignore_ascii_case(&present.sha256) => {
            return Err(format!(
                "The present file {} differs from the downloaded bytes, and this component has no ContentHashSHA256 to choose between them; it was not replaced",
                path.display()
            ));
        }
        _ => {}
    }
    archive::verify(path, path, &context.settings.archive, context.cancelled).map_err(|error| {
        format!(
            "The present file {} failed verification: {error:#}",
            path.display()
        )
    })?;
    Ok(Acquired {
        path: path.clone(),
        sha256: actual,
    })
}

/// Whether a source after `source` would reveal the same name again and so find the
/// same present file: a repeat of the same Origin URL. A Peer source learns its own name
/// from the torrent and checks it before any transfer, adopting or refusing a present
/// file itself; a Manual source reveals no name. Either may still end the target well,
/// so a refused adoption falls through to it.
fn reveals_again(source: &Source, later: &[Source]) -> bool {
    let Source::Origin { url, .. } = source else {
        return false;
    };
    later
        .iter()
        .any(|later| matches!(later, Source::Origin { url: again, .. } if again == url))
}

fn single_source(
    context: &RunContext<'_>,
    index: usize,
    target: &ArchiveTarget,
    source: &Source,
) -> Step {
    let kind = source.kind();
    match source {
        Source::Existing { path } => match sha256_file_with_cancel(path, context.cancelled) {
            Ok(sha256) => Step::Done(
                Acquired {
                    path: path.clone(),
                    sha256,
                },
                kind,
            ),
            Err(error) => Step::Failed(
                format!(
                    "Existing archive {} is unreadable: {error:#}",
                    path.display()
                ),
                kind,
            ),
        },
        Source::Manual { url, reason } => Step::Manual(url.clone(), *reason, kind),
        Source::Origin { .. } => {
            let mut on = |event: AcquisitionEvent| {
                let _ = context.events.send(event);
            };
            match origin_with_retries(context, index, target, source, context.cancelled, &mut on) {
                Ok(acquired) => Step::Done(acquired, kind),
                Err(error) => step_from_error(error, kind, context.cancelled),
            }
        }
        Source::Peer { .. } => match &context.settings.peer {
            // Fail closed: no tunnel, no peer sockets.
            None => Step::Failed(
                "Peer sharing paused: the private tunnel is not available".into(),
                kind,
            ),
            Some(peer) => {
                let mut on = |event: AcquisitionEvent| {
                    let _ = context.events.send(AcquisitionEvent {
                        target_index: index,
                        component_guid: target.component_guid.clone(),
                        filename: target.filename.clone().or(event.filename),
                        source: SourceKind::Peer,
                        phase: event.phase,
                    });
                };
                let destination = match target.destination(&context.plan.source_dir) {
                    Ok(destination) => destination,
                    Err(error) => return Step::Failed(format!("{error:#}"), kind),
                };
                match peer.acquire(target, &destination, context.cancelled, &mut on) {
                    Ok(acquired) => Step::Done(acquired, kind),
                    Err(error) => step_from_error(error, kind, context.cancelled),
                }
            }
        },
    }
}

/// Removes a racer's published file. The error is kept for the report, never dropped.
fn discard(acquired: &Acquired, notes: &mut Vec<String>) {
    if let Err(error) = std::fs::remove_file(&acquired.path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        notes.push(format!(
            "Could not remove {}: {error}",
            acquired.path.display()
        ));
    }
}

const PEER_STAGING_PREFIX: &str = ".peer-";
const PEER_STAGING_SUFFIX: &str = ".part";

/// True for the name of a Peer staging folder (`.peer-<nonce>.part`). A leftover one holds
/// an unaccepted copy and must never count as a present archive.
pub(crate) fn is_peer_staging(name: &std::ffi::OsStr) -> bool {
    name.to_str().is_some_and(|name| {
        name.len() > PEER_STAGING_PREFIX.len() + PEER_STAGING_SUFFIX.len()
            && name.starts_with(PEER_STAGING_PREFIX)
            && name.ends_with(PEER_STAGING_SUFFIX)
    })
}

/// Reclaim only empty, canonically named old staging directories. A reserved
/// name and timestamp do not establish authority to delete retained contents.
/// Nonempty directories remain ignored by planning and available for inspection.
fn sweep_stale_peer_staging(
    source_dir: &Path,
    started: std::time::SystemTime,
    cancelled: &AtomicBool,
) {
    let Ok(entries) = std::fs::read_dir(source_dir) else {
        return;
    };
    for entry in entries.take(100_000).flatten() {
        if cancelled.load(Ordering::Acquire) {
            return;
        }
        let name = entry.file_name();
        let canonical = name.to_str().is_some_and(|name| {
            name.strip_prefix(PEER_STAGING_PREFIX)
                .and_then(|name| name.strip_suffix(PEER_STAGING_SUFFIX))
                .is_some_and(|nonce| {
                    uuid::Uuid::parse_str(nonce)
                        .is_ok_and(|id| id.simple().to_string() == nonce && !id.is_nil())
                })
        });
        let stale = canonical
            && entry.file_type().is_ok_and(|kind| kind.is_dir())
            && entry.metadata().is_ok_and(|metadata| {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if metadata.file_attributes() & 0x400 != 0 {
                        // FILE_ATTRIBUTE_REPARSE_POINT includes cloud/custom tags
                        // that Windows FileType::is_dir alone does not exclude.
                        return false;
                    }
                }
                metadata.is_dir() && metadata.modified().is_ok_and(|modified| modified < started)
            });
        if stale {
            // remove_dir cannot remove a nonempty tree or follow a link.
            let _ = std::fs::remove_dir(entry.path());
        }
    }
}

/// Peer's private staging directory for one race, created beside the final path.
///
/// Peer publishes only inside it, so nothing Peer writes is visible at the final path
/// until the race accepts it. Origin's retries therefore never trip the "destination
/// already exists" check on a Peer copy and always reach the host, where a gate shows up.
struct PeerStaging {
    directory: PathBuf,
    /// What Peer is handed: the staged file path when the name is known, else the directory.
    destination: PathBuf,
}

impl PeerStaging {
    fn create(target: &ArchiveTarget, source_dir: &Path) -> Result<Self> {
        // Short and fixed-length, so a long archive name cannot push it past a name limit.
        let directory = source_dir.join(format!(
            "{PEER_STAGING_PREFIX}{}{PEER_STAGING_SUFFIX}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir(&directory)
            .with_context(|| format!("Could not create {}", directory.display()))?;
        let destination = match &target.filename {
            Some(name) => directory.join(download::one_component(name)?),
            None => directory.clone(),
        };
        Ok(Self {
            directory,
            destination,
        })
    }

    /// Moves an accepted, already verified Peer copy to the final path without clobbering.
    fn publish(&self, staged: Acquired, source_dir: &Path) -> Result<Acquired> {
        ensure!(
            staged.path.parent() == Some(self.directory.as_path()),
            "Peer published outside its staging folder: {}",
            staged.path.display()
        );
        let name = staged
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .context("Peer copy has no file name")?;
        let path = source_dir.join(download::one_component(name)?);
        // For an unnamed target the name came from the torrent. When it is already
        // present, the copy is not published (as an Origin transfer is not); the caller
        // adopts the present file or refuses it, and the staged copy goes with staging.
        let unnamed = self.destination == self.directory;
        let exists = || {
            anyhow::Error::new(download::DestinationExists {
                path: path.clone(),
                sha256: staged.sha256.clone(),
            })
        };
        if unnamed && path.exists() {
            return Err(exists());
        }
        match tempfile::TempPath::try_from_path(&staged.path)?.persist_noclobber(&path) {
            Ok(()) => {}
            Err(error) if unnamed && error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(exists());
            }
            Err(error) => {
                return Err(anyhow::Error::new(error.error).context(format!(
                    "Could not publish the peer copy to {}",
                    path.display()
                )));
            }
        }
        Ok(Acquired {
            path,
            sha256: staged.sha256,
        })
    }

    /// Runs after both racers are joined. Removal failures go into the notes.
    fn remove(&self, notes: &mut Vec<String>) {
        if let Err(error) = std::fs::remove_dir_all(&self.directory)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            notes.push(format!(
                "Could not remove {}: {error}",
                self.directory.display()
            ));
        }
    }
}

/// Origin races Peer.
///
/// - Each racer has its own stop flag. This thread copies the parent cancel into both on
///   every pass, and re-checks the parent before accepting any result, so nothing is
///   accepted after a cancel; a file published in that window is removed.
/// - Peer writes only into a private staging folder ([`PeerStaging`]). Origin never sees
///   a Peer file, so every Origin retry reaches the host.
/// - Peer may win only after Origin has passed gate classification (its first
///   `Transferring` event, sent after the status/body checks) or has failed with a non-gate
///   error. Until then a finished Peer copy is held in staging. If Origin then reveals a
///   gate, the copy is removed and the target ends `ManualRequired`: a peer copy never
///   satisfies a gate (R2). An accepted copy is moved to the final path before Origin is
///   stopped; if that move fails, Origin keeps running. When the copy's torrent name is
///   already present, that file is adopted ([`adopt_present`]) before Origin is stopped;
///   if adoption refuses it, that is a Peer failure and Origin keeps running.
/// - The first accepted `Ok` wins. The loser is stopped, joined, has any published file
///   removed, and is reported `Superseded`. The staging folder is removed after the join.
///   Removal failures go into the returned notes.
fn race_origin_peer(
    context: &RunContext<'_>,
    index: usize,
    target: &ArchiveTarget,
    origin: &Source,
) -> (Step, Vec<String>) {
    const ORIGIN: usize = 0;
    const PEER: usize = 1;
    let mut notes = Vec::new();
    let peer = context
        .settings
        .peer
        .as_ref()
        .expect("race requires a peer source");
    let origin_kind = origin.kind();
    if let Err(error) = target.destination(&context.plan.source_dir) {
        return (Step::Failed(format!("{error:#}"), origin_kind), notes);
    }
    let source_dir = context.plan.source_dir.as_path();
    let staging = match PeerStaging::create(target, source_dir) {
        Ok(staging) => staging,
        Err(error) => {
            // No private place for Peer means no race: Origin runs alone.
            notes.push(format!("Peer skipped: {error:#}"));
            return (single_source(context, index, target, origin), notes);
        }
    };
    let flags = [AtomicBool::new(false), AtomicBool::new(false)];
    let origin_cleared = AtomicBool::new(false);
    // Peer's grant to call `Acquire::acquire` (O2): the thread below may spawn and wait
    // inertly, but must not start transferring until Origin passes gate classification
    // (its first `Transferring`) or fails with an error that does not deny Peer.
    const WAITING: u8 = 0;
    const GRANTED: u8 = 1;
    const DENIED: u8 = 2;
    const STARTED: u8 = 3;
    let grant = AtomicU8::new(WAITING);
    let peer_started = AtomicBool::new(false);
    let (results, receiver) = mpsc::channel::<(usize, Result<Acquired>)>();
    let step = thread::scope(|scope| {
        {
            let results = results.clone();
            let flag = &flags[ORIGIN];
            let origin_cleared = &origin_cleared;
            let grant = &grant;
            scope.spawn(move || {
                let mut on = |event: AcquisitionEvent| {
                    if matches!(event.phase, Phase::Transferring { .. }) {
                        origin_cleared.store(true, Ordering::SeqCst);
                        let _ = grant.compare_exchange(
                            WAITING,
                            GRANTED,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        );
                    }
                    let _ = context.events.send(event);
                };
                let result = origin_with_retries(context, index, target, origin, flag, &mut on);
                match &result {
                    // Revoke an unused grant when Origin finishes. The Peer claims
                    // GRANTED atomically, so completion cannot start a waiting Peer.
                    Ok(_) => {
                        grant.store(DENIED, Ordering::SeqCst);
                    }
                    Err(error) if origin_denies_peer(error) => {
                        let _ = grant.compare_exchange(
                            WAITING,
                            DENIED,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        );
                    }
                    Err(_) => grant.store(GRANTED, Ordering::SeqCst),
                }
                let _ = results.send((ORIGIN, result));
            });
        }
        {
            let flag = &flags[PEER];
            let destination = staging.destination.as_path();
            let grant = &grant;
            let peer_started = &peer_started;
            scope.spawn(move || {
                let mut on = |event: AcquisitionEvent| {
                    let _ = context.events.send(AcquisitionEvent {
                        target_index: index,
                        component_guid: target.component_guid.clone(),
                        filename: target.filename.clone().or(event.filename),
                        source: SourceKind::Peer,
                        phase: event.phase,
                    });
                };
                let granted = loop {
                    match grant.load(Ordering::SeqCst) {
                        GRANTED => {
                            if flag.load(Ordering::SeqCst)
                                || context.cancelled.load(Ordering::SeqCst)
                            {
                                break false;
                            }
                            if grant
                                .compare_exchange(
                                    GRANTED,
                                    STARTED,
                                    Ordering::SeqCst,
                                    Ordering::SeqCst,
                                )
                                .is_ok()
                            {
                                break true;
                            }
                        }
                        DENIED => break false,
                        _ => {}
                    }
                    if flag.load(Ordering::SeqCst) {
                        break false;
                    }
                    thread::sleep(Duration::from_millis(15));
                };
                let result = if granted {
                    peer_started.store(true, Ordering::SeqCst);
                    peer.acquire(target, destination, flag, &mut on)
                } else {
                    Err(anyhow::Error::new(PeerNeverStarted))
                };
                let _ = results.send((PEER, result));
            });
        }
        let kinds = [origin_kind, SourceKind::Peer];
        let stop = |racer: usize| flags[racer].store(true, Ordering::SeqCst);
        let parent_cancelled = || context.cancelled.load(Ordering::SeqCst);
        let mut running = [true, true];
        let mut origin_failed = false;
        let mut held_peer: Option<Acquired> = None;
        let mut decided: Option<Step> = None;
        let mut superseded = [false, false];
        let mut last_error: Option<(String, SourceKind)> = None;
        let mut winner_path: Option<PathBuf> = None;
        while running.iter().any(|r| *r) {
            if parent_cancelled() {
                stop(ORIGIN);
                stop(PEER);
                if decided.is_none() {
                    if let Some(held) = held_peer.take() {
                        discard(&held, &mut notes);
                    }
                    decided = Some(Step::Cancelled(origin_kind));
                }
            }
            if decided.is_none()
                && held_peer.is_some()
                && (origin_cleared.load(Ordering::SeqCst) || origin_failed)
            {
                // Publish first: if the move fails, Origin is still running and may finish.
                match staging.publish(held_peer.take().unwrap(), source_dir) {
                    Ok(published) => {
                        stop(ORIGIN);
                        winner_path = Some(published.path.clone());
                        decided = Some(Step::Done(published, SourceKind::Peer));
                    }
                    Err(error) => match download::destination_exists(&error).cloned() {
                        // Adopted before Origin is stopped. The present file may even be
                        // Origin's own copy not yet reported: then it is the winner, never
                        // discarded, and the outcome reports Peer. A refusal is a Peer
                        // failure only, and Origin keeps running.
                        Some(present) => match adopt_present(context, target, &present) {
                            _ if parent_cancelled() => {
                                stop(ORIGIN);
                                stop(PEER);
                                decided = Some(Step::Cancelled(origin_kind));
                            }
                            Ok(adopted) => {
                                stop(ORIGIN);
                                winner_path = Some(adopted.path.clone());
                                decided = Some(Step::Done(adopted, SourceKind::Peer));
                            }
                            Err(message) => {
                                emit(
                                    context,
                                    index,
                                    target,
                                    SourceKind::Peer,
                                    Phase::SourceFailed {
                                        error: message.clone(),
                                        will_fallback: running[ORIGIN],
                                    },
                                );
                                last_error = Some((message, SourceKind::Peer));
                            }
                        },
                        None => {
                            let message = format!("{error:#}");
                            emit(
                                context,
                                index,
                                target,
                                SourceKind::Peer,
                                Phase::SourceFailed {
                                    error: message.clone(),
                                    will_fallback: running[ORIGIN],
                                },
                            );
                            last_error = Some((message, SourceKind::Peer));
                        }
                    },
                }
            }
            let (racer, result) = match receiver.recv_timeout(Duration::from_millis(20)) {
                Ok(message) => message,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            running[racer] = false;
            let other = 1 - racer;
            if racer == PEER
                && let Err(error) = &result
                && is_never_started(error)
            {
                // Never called `Acquire::acquire`: no event was ever emitted for it, so
                // none is emitted now either (O2).
                continue;
            }
            if decided.is_some() || parent_cancelled() {
                // A loser, or a result that arrived after a cancel: never accepted.
                if let Ok(acquired) = &result
                    && winner_path.as_deref() != Some(acquired.path.as_path())
                {
                    discard(acquired, &mut notes);
                }
                if matches!(
                    decided,
                    Some(Step::Done(..) | Step::Manual(..) | Step::Present(..))
                ) && !superseded[racer]
                {
                    superseded[racer] = true;
                    emit(context, index, target, kinds[racer], Phase::Superseded);
                }
                if decided.is_none() {
                    stop(other);
                    decided = Some(Step::Cancelled(kinds[racer]));
                }
                continue;
            }
            match (racer, result) {
                (ORIGIN, Ok(acquired)) => {
                    stop(PEER);
                    if let Some(held) = held_peer.take() {
                        discard(&held, &mut notes);
                        superseded[PEER] = true;
                        emit(context, index, target, SourceKind::Peer, Phase::Superseded);
                    }
                    winner_path = Some(acquired.path.clone());
                    decided = Some(Step::Done(acquired, origin_kind));
                }
                (_, Ok(acquired)) => {
                    // Held in staging. It is published at the top of the next pass once
                    // Origin has cleared its gate checks, or after the loop once Origin
                    // has failed without a gate. A gate discards it.
                    held_peer = Some(acquired);
                }
                (_, Err(error)) => match step_from_error(error, kinds[racer], context.cancelled) {
                    gate @ Step::Manual(_, reason, _) if racer == ORIGIN && reason.is_gate() => {
                        stop(PEER);
                        if let Some(held) = held_peer.take() {
                            discard(&held, &mut notes);
                        }
                        if peer_started.load(Ordering::SeqCst) && !superseded[PEER] {
                            superseded[PEER] = true;
                            emit(context, index, target, SourceKind::Peer, Phase::Superseded);
                        }
                        decided = Some(gate);
                    }
                    Step::Present(present, kind) if racer == ORIGIN => {
                        // Origin has finished its gate checks, but a name collision
                        // alone is not a winning result. Verify before stopping Peer.
                        match adopt_present(context, target, &present) {
                            _ if parent_cancelled() => {
                                stop(PEER);
                                if let Some(held) = held_peer.take() {
                                    discard(&held, &mut notes);
                                }
                                decided = Some(Step::Cancelled(kind));
                            }
                            Ok(adopted) => {
                                stop(PEER);
                                if let Some(held) = held_peer.take() {
                                    discard(&held, &mut notes);
                                }
                                if peer_started.load(Ordering::SeqCst) && !superseded[PEER] {
                                    superseded[PEER] = true;
                                    emit(
                                        context,
                                        index,
                                        target,
                                        SourceKind::Peer,
                                        Phase::Superseded,
                                    );
                                }
                                winner_path = Some(adopted.path.clone());
                                decided = Some(Step::Done(adopted, kind));
                            }
                            Err(message) => {
                                origin_failed = true;
                                emit(
                                    context,
                                    index,
                                    target,
                                    kind,
                                    Phase::SourceFailed {
                                        error: message.clone(),
                                        will_fallback: running[PEER] || held_peer.is_some(),
                                    },
                                );
                                last_error = Some((message, kind));
                            }
                        }
                    }
                    terminal @ Step::Terminal(..) if racer == ORIGIN => {
                        stop(PEER);
                        if let Some(held) = held_peer.take() {
                            discard(&held, &mut notes);
                        }
                        if peer_started.load(Ordering::SeqCst) && !superseded[PEER] {
                            superseded[PEER] = true;
                            emit(context, index, target, SourceKind::Peer, Phase::Superseded);
                        }
                        decided = Some(terminal);
                    }
                    step => {
                        let message = match step {
                            Step::Failed(message, _) => message,
                            Step::Manual(_, reason, _) => format!("{reason:?}"),
                            _ => "Source stopped".into(),
                        };
                        if racer == ORIGIN {
                            origin_failed = true;
                        }
                        if running[other] || held_peer.is_some() {
                            emit(
                                context,
                                index,
                                target,
                                kinds[racer],
                                Phase::SourceFailed {
                                    error: message.clone(),
                                    will_fallback: true,
                                },
                            );
                        }
                        last_error = Some((message, kinds[racer]));
                    }
                },
            }
        }
        if decided.is_none()
            && let Some(held) = held_peer.take()
        {
            if parent_cancelled() {
                discard(&held, &mut notes);
                decided = Some(Step::Cancelled(SourceKind::Peer));
            } else {
                // Origin ended without a gate: the held copy is accepted.
                match staging.publish(held, source_dir) {
                    Ok(published) => decided = Some(Step::Done(published, SourceKind::Peer)),
                    Err(error) => match download::destination_exists(&error).cloned() {
                        Some(present) => match adopt_present(context, target, &present) {
                            _ if parent_cancelled() => {
                                decided = Some(Step::Cancelled(SourceKind::Peer));
                            }
                            Ok(adopted) => decided = Some(Step::Done(adopted, SourceKind::Peer)),
                            Err(message) => last_error = Some((message, SourceKind::Peer)),
                        },
                        None => last_error = Some((format!("{error:#}"), SourceKind::Peer)),
                    },
                }
            }
        }
        decided.unwrap_or_else(|| {
            let (message, kind) = last_error
                .unwrap_or_else(|| ("Download race ended without a result".into(), origin_kind));
            Step::Failed(message, kind)
        })
    });
    // Both racers are joined: nothing writes into staging any more.
    staging.remove(&mut notes);
    (step, notes)
}

fn origin_with_retries(
    context: &RunContext<'_>,
    index: usize,
    target: &ArchiveTarget,
    source: &Source,
    cancelled: &AtomicBool,
    on: &mut dyn FnMut(AcquisitionEvent),
) -> Result<Acquired> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match origin_once(context, index, target, source, cancelled, on) {
            Ok(acquired) => return Ok(acquired),
            Err(error) => {
                let delay = (attempt <= context.settings.retries)
                    .then(|| download::transient_delay(&error, attempt))
                    .flatten();
                match delay {
                    Some(delay) if download_queue::sleep_unless_cancelled(delay, cancelled) => {}
                    _ => return Err(error),
                }
            }
        }
    }
}

fn origin_once(
    context: &RunContext<'_>,
    index: usize,
    target: &ArchiveTarget,
    source: &Source,
    cancelled: &AtomicBool,
    on: &mut dyn FnMut(AcquisitionEvent),
) -> Result<Acquired> {
    let Source::Origin {
        url,
        provider,
        requires,
    } = source
    else {
        bail!("Not an origin source");
    };
    let kind = source.kind();
    let mut mega_node = None::<mega::Resolved>;
    let (fetch_url, name_hint, transfer) = if *provider == Provider::Nexus {
        if *requires == OriginRequirement::NexusPremium {
            match &context.premium {
                Some(Ok(true)) => {}
                // A known free key: manual, with no download request (gate, R2).
                Some(Ok(false)) => {
                    // The mod page, not an nxm link, is what a person can open.
                    return Err(download::manual_required(
                        &target.registry_url,
                        ManualReason::FreeUserWait,
                        "Nexus Mods needs a premium account for automatic downloads; download it on the website.",
                    ));
                }
                // A validation error, or (defensively) no validation result at all for a
                // target that requires one: terminal, not a known-free gate and not an
                // ordinary transient failure (O3). Peer must not run around either.
                Some(Err(error)) => {
                    return Err(anyhow::Error::new(PremiumValidationFailed(error.clone())));
                }
                None => {
                    return Err(anyhow::Error::new(PremiumValidationFailed(
                        "Nexus premium status is unknown".to_owned(),
                    )));
                }
            }
        }
        let nexus = context
            .nexus
            .as_ref()
            .context("Nexus client missing for a Nexus origin")?;
        // API failures take the same typed retry policy as transfers.
        let link = nexus_link(url, target, nexus, cancelled).map_err(download::request_failure)?;
        let resolved = nexus_client_lock(nexus, cancelled)?
            .nxm_download_link_with_cancel(&link, cancelled)
            .map_err(|error| {
                // Left unwrapped: a classified `FetchError` hides its inner error from
                // `chain()`, and this one must stay visible as terminal. Unclassified, it
                // is never retried.
                if nexus_authorization_failed(&error) {
                    error
                } else {
                    download::request_failure(error)
                }
            })?;
        (
            resolved.uri,
            Some(resolved.file_name),
            download::TransferContext::with_loopback_ca(context.loopback_ca),
        )
    } else if *provider == Provider::DeadlyStream {
        // The page, selection and link are read (and any gate found) before the first
        // `Transferring` event, so Peer is granted only once every provider gate is clear.
        let file = deadlystream_file(context, target, url, cancelled)?;
        (
            file.url,
            Some(file.name),
            download::TransferContext {
                loopback_ca: context.loopback_ca,
                cookie: file.cookie,
                referer: Some(file.referer),
                confine_origin: true,
            },
        )
    } else if *provider == Provider::Mega {
        // The API request and node attributes are read (and any quota or access gate
        // found) before the first `Transferring` event, as for DeadlyStream. The bytes
        // move through `mega::transfer`, so `fetch_url` and the context are unused.
        let node = mega_node.insert(mega_resolve(context, target, url, cancelled)?);
        (
            String::new(),
            node.name.clone(),
            download::TransferContext::default(),
        )
    } else {
        (
            url.clone(),
            None,
            download::TransferContext::with_loopback_ca(context.loopback_ca),
        )
    };
    let destination = target.destination(&context.plan.source_dir)?;
    let destination = match &target.filename {
        Some(_) => Destination::File(&destination),
        None => Destination::Directory {
            directory: &destination,
            name: name_hint.as_deref(),
        },
    };
    let mut last = None::<Instant>;
    let mut progress = |progress: FetchProgress| {
        let phase = match progress {
            FetchProgress::Transferring { bytes, total } => {
                let done = total == Some(bytes);
                if !done && bytes > 0 && last.is_some_and(|t| t.elapsed() < PROGRESS_INTERVAL) {
                    return;
                }
                last = Some(Instant::now());
                Phase::Transferring {
                    bytes,
                    total,
                    peers: None,
                }
            }
            FetchProgress::Verifying => Phase::Verifying,
        };
        on(AcquisitionEvent {
            target_index: index,
            component_guid: target.component_guid.clone(),
            filename: target.filename.clone(),
            source: kind,
            phase,
        });
    };
    let fetched = match &mega_node {
        Some(node) => mega::transfer(
            node,
            destination,
            target.expected_sha256.as_deref(),
            cancelled,
            &context.settings.archive,
            &mut progress,
        ),
        None => download::fetch_to_with_context(
            &fetch_url,
            destination,
            target.expected_sha256.as_deref(),
            cancelled,
            &context.settings.archive,
            &mut progress,
            &transfer,
        ),
    }
    .map_err(|error| match (provider, download::classify(&error)) {
        // A gate on a resolved Nexus CDN link or DeadlyStream archive link: point the
        // person at the page, not at a signed, expiring or session-bound URL.
        (
            Provider::Nexus | Provider::DeadlyStream,
            Some(FailureClass::ManualRequired { reason, .. }),
        ) => download::manual_required(&target.registry_url, *reason, &format!("{error:#}")),
        _ => error,
    })?;
    Ok(Acquired {
        path: fetched.path,
        sha256: fetched.sha256,
    })
}

/// The node behind a MEGA public file link: its safe name and size, and the vetted storage
/// address. An unsupported link form (folder, password, missing or bad key) fails here
/// with a message that never contains the link. A quota or access gate is `ManualRequired`
/// at the target's authored link (display redaction hides the key), so Peer does not run
/// around it. A guide-supplied filename means a bad node name does not fail the resolve.
fn mega_resolve(
    context: &RunContext<'_>,
    target: &ArchiveTarget,
    url: &str,
    cancelled: &AtomicBool,
) -> Result<mega::Resolved> {
    let link = mega::parse_link(url).map_err(anyhow::Error::from)?;
    mega::resolve(
        &link,
        &target.registry_url,
        target.filename.is_none(),
        context.settings.mega_api_base.as_deref(),
        context.loopback_ca,
        cancelled,
    )
}

/// The archive of a DeadlyStream page that `target` wants: the one named by its filename
/// (ASCII case-insensitive), or, unnamed, the page's only one. Never a guess between several.
fn deadlystream_file(
    context: &RunContext<'_>,
    target: &ArchiveTarget,
    url: &str,
    cancelled: &AtomicBool,
) -> Result<deadlystream::File> {
    let mut files = deadlystream::resolve(
        url,
        context.settings.deadlystream_base.as_deref(),
        context.loopback_ca,
        cancelled,
    )?;
    match &target.filename {
        Some(name) => {
            let position = files
                .iter()
                .position(|file| file.name.eq_ignore_ascii_case(name));
            match position {
                Some(position) => Ok(files.swap_remove(position)),
                None => bail!(
                    "The DeadlyStream page lists no file named {name} (it lists {})",
                    files
                        .iter()
                        .map(|file| file.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }
        }
        None => {
            ensure!(
                files.len() == 1,
                "The DeadlyStream page lists {} files; add a Files entry for the one this component needs",
                files.len()
            );
            Ok(files.remove(0))
        }
    }
}

fn nexus_client_lock<'a>(
    nexus: &'a Mutex<NexusClient>,
    cancelled: &AtomicBool,
) -> Result<std::sync::MutexGuard<'a, NexusClient>> {
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(download::fail(
                FailureClass::Cancelled,
                anyhow::anyhow!("Nexus request cancelled"),
            ));
        }
        match nexus.try_lock() {
            Ok(client) => return Ok(client),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                anyhow::bail!("Nexus client stopped unexpectedly")
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
        }
    }
}

/// An nxm URL carries its own file id. A nexusmods.com page URL names the file through a
/// `file_id` query parameter or an authored `Files` name; anything else is not guessed.
fn nexus_link(
    url: &str,
    target: &ArchiveTarget,
    nexus: &Mutex<NexusClient>,
    cancelled: &AtomicBool,
) -> Result<protocol::Link> {
    if url.trim_start().to_ascii_lowercase().starts_with("nxm://") {
        return protocol::parse(url);
    }
    let (game_domain, mod_id, query_file) = nexus_page(url)?;
    let file_id = match (query_file, &target.filename) {
        (Some(id), _) => id,
        (None, Some(name)) => {
            let files = nexus_client_lock(nexus, cancelled)?.get_mod_files_with_cancel(
                &game_domain,
                &mod_id.to_string(),
                cancelled,
            )?;
            let matching: Vec<_> = files
                .iter()
                .filter(|file| file.file_name.eq_ignore_ascii_case(name))
                .collect();
            // An old or optional copy of a name yields to the live one; two live copies
            // are ambiguous and never resolved by list order.
            let eligible: Vec<_> = matching
                .iter()
                .copied()
                .filter(|file| {
                    NEXUS_PAGE_CATEGORIES
                        .iter()
                        .any(|category| file.category_name.eq_ignore_ascii_case(category))
                })
                .collect();
            let file = match (matching.as_slice(), eligible.as_slice()) {
                ([], _) => bail!("Nexus mod {mod_id} lists no file named {name}"),
                ([only], _) | (_, [only]) => *only,
                _ => bail!(
                    "Nexus mod {mod_id} lists more than one file named {name}; add a file_id to the URL"
                ),
            };
            u64::try_from(file.file_id)?
        }
        (None, None) => {
            // `discover` names every page it can; the plan warnings say why this one stayed.
            bail!(
                "Nexus page URL does not name a file; add a Files entry or a file_id (see the plan warnings)"
            )
        }
    };
    Ok(protocol::Link::Nxm {
        game_domain,
        mod_id,
        file_id,
        key: None,
        expires: 0,
        user_id: 0,
    })
}

fn nxm_url(link: &protocol::Link) -> Result<String> {
    let protocol::Link::Nxm {
        game_domain,
        mod_id,
        file_id,
        key,
        expires,
        user_id,
    } = link
    else {
        bail!("Expected an nxm:// link");
    };
    let mut url = reqwest::Url::parse(&format!(
        "nxm://{game_domain}/mods/{mod_id}/files/{file_id}"
    ))?;
    // Query-encoded (decoded again by `protocol::parse`), so a key's reserved characters
    // survive the round trip instead of becoming extra parameters. A space becomes form
    // `+`, which `protocol::parse`'s `query_pairs` decodes back; the API request built by
    // `updates::NexusClient::nxm_download_link` uses %20 instead, for the Nexus server.
    if let Some(key) = key.as_deref().filter(|k| !k.is_empty()) {
        url.query_pairs_mut().append_pair("key", key);
    }
    if *expires > 0 {
        url.query_pairs_mut()
            .append_pair("expires", &expires.to_string());
    }
    if *user_id > 0 {
        url.query_pairs_mut()
            .append_pair("user_id", &user_id.to_string());
    }
    Ok(url.into())
}

/// The only nxm path: resolves the component (Matched only) and returns one target whose
/// Origin is the nxm link. Callers wrap it with `AcquisitionPlan::single`, then `run` and
/// `record`.
pub fn nxm_target(guide: &Guide, link: &protocol::Link) -> Result<ArchiveTarget> {
    let protocol::Link::Nxm { key, .. } = link else {
        bail!("Expected an nxm:// link");
    };
    let (status, matched) = protocol::resolve_nxm(link, &guide.components);
    let matched = match (status, matched) {
        (protocol::NxmResolveStatus::Matched, Some(matched)) => matched,
        (status, _) => bail!("nxm link does not resolve to exactly one component: {status:?}"),
    };
    let expected_sha256 = guide
        .components
        .iter()
        .find(|c| c.guid == matched.component_guid)
        .and_then(|c| c.resource_registry.get(&matched.registry_url))
        .and_then(|metadata| metadata_str(metadata, "ContentHashSHA256"))
        .map(str::to_owned);
    // Any one-time key makes this a keyed handoff: its expiry is judged when the link
    // resolves, so a stale or malformed key reports why rather than a premium gate.
    let keyed = key.as_deref().is_some_and(|k| !k.is_empty());
    Ok(ArchiveTarget {
        component_guid: matched.component_guid,
        registry_url: matched.registry_url,
        filename: None,
        expected_sha256,
        sources: vec![Source::Origin {
            url: nxm_url(link)?,
            provider: Provider::Nexus,
            requires: if keyed {
                OriginRequirement::None
            } else {
                OriginRequirement::NexusPremium
            },
        }],
        shared_destination: None,
    })
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordSummary {
    /// (component guid, registry url, filename) newly set or confirmed `true`.
    pub recorded: Vec<(String, String, String)>,
    /// Entries left alone because the guide flags them `false`.
    pub kept_false: Vec<(String, String, String)>,
    pub hashes_recorded: usize,
}

fn component_mut<'a>(components: &'a mut [Component], guid: &str) -> Option<&'a mut Component> {
    for component in components {
        if component.guid == guid {
            return Some(component);
        }
        if let Some(found) = component_mut(&mut component.options, guid) {
            return Some(found);
        }
    }
    None
}

/// Sets `ResourceRegistry[url].Files[name] = true` for completed and already-present
/// archives. Never mutates a `false` entry. Hashes are not recorded; see `record_with`.
pub fn record(guide: &mut Guide, report: &RunReport) -> Result<RecordSummary> {
    record_with(guide, report, false)
}

/// `record_hashes` is the user-approved flag for writing `ContentHashSHA256` when the
/// entry has none.
pub fn record_with(
    guide: &mut Guide,
    report: &RunReport,
    record_hashes: bool,
) -> Result<RecordSummary> {
    // All-or-nothing: entries apply to a copy that replaces the guide only on success.
    let mut working = guide.clone();
    let mut summary = RecordSummary::default();
    let completed = report
        .outcomes
        .iter()
        .filter(|o| o.status == TargetStatus::Completed)
        .map(|o| {
            (
                &o.component_guid,
                &o.registry_url,
                o.path.as_deref(),
                o.sha256.as_deref(),
            )
        });
    let present = report
        .skipped
        .iter()
        .filter(|s| s.reason == SkipReason::AlreadyPresent)
        .map(|s| (&s.component_guid, &s.registry_url, s.path.as_deref(), None));
    let entries: Vec<_> = completed.chain(present).collect();
    // `ContentHashSHA256` identifies a resource's only archive. A resource that yields
    // several archives (named Files, or a discovered Nexus page) never gets one archive's
    // digest as if it identified its siblings.
    let archives_of = |guid: &String, url: &String| {
        let mut names: Vec<&str> = entries
            .iter()
            .filter(|(g, u, _, _)| *g == guid && *u == url)
            .filter_map(|(_, _, path, _)| path.and_then(|p| p.file_name()).and_then(|n| n.to_str()))
            .collect();
        names.sort_by_key(|name| name.to_ascii_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        names.len()
    };
    for &(guid, url, path, sha256) in &entries {
        let Some(name) = path.and_then(|p| p.file_name()).and_then(|n| n.to_str()) else {
            continue;
        };
        let name = download::one_component(name)?;
        let only_archive = archives_of(guid, url) == 1;
        let component = component_mut(&mut working.components, guid)
            .with_context(|| format!("Component {guid} is no longer in the guide"))?;
        let metadata = component
            .resource_registry
            .get_mut(url.as_str())
            .context("Resource registry entry is no longer present")?
            .as_object_mut()
            .context("Resource metadata must be an object")?;
        let sibling_recorded = metadata
            .get("Files")
            .and_then(Value::as_object)
            .is_some_and(|files| {
                files.iter().any(|(key, value)| {
                    value == &Value::Bool(true) && !key.eq_ignore_ascii_case(name)
                })
            });
        if record_hashes
            && only_archive
            && !sibling_recorded
            && let Some(sha256) = sha256
            && metadata
                .get("ContentHashSHA256")
                .and_then(Value::as_str)
                .is_none_or(|s| s.trim().is_empty())
        {
            metadata.insert("ContentHashSHA256".into(), Value::String(sha256.into()));
            summary.hashes_recorded += 1;
        }
        let files = metadata
            .entry("Files")
            .or_insert_with(|| Value::Object(Default::default()))
            .as_object_mut()
            .context("Resource Files must be an object")?;
        let key = files
            .keys()
            .find(|key| key.eq_ignore_ascii_case(name))
            .cloned()
            .unwrap_or_else(|| name.to_owned());
        let entry = (guid.clone(), url.clone(), key.clone());
        if files.get(&key) == Some(&Value::Bool(false)) {
            summary.kept_false.push(entry);
        } else {
            files.insert(key, Value::Bool(true));
            summary.recorded.push(entry);
        }
    }
    *guide = working;
    Ok(summary)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FomodFollowup {
    pub component_guid: String,
    pub archive: PathBuf,
}

/// An archive that could not be inspected. It is not a follow-up: C# offers only archives
/// it positively detects as FOMOD. Frontends may log these.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FomodInspectionError {
    pub component_guid: String,
    pub archive: PathBuf,
    pub error: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FomodFollowups {
    pub followups: Vec<FomodFollowup>,
    pub inspection_errors: Vec<FomodInspectionError>,
}

/// Mirrors C# `FomodPostDownloadOrchestrator`: selected components only, archives
/// acquired or present in this run, positive FOMOD detection, and `fomod::should_prompt`
/// (configured/warned suppress; dismissed prompts again). Nothing after a cancelled run.
pub fn fomod_followups(
    guide: &Guide,
    report: &RunReport,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> FomodFollowups {
    // Preserve the desktop caller's recursive prompt policy and suppression of
    // inspection for ineligible consumers; Fetch uses the full assessment below.
    let eligible = |guid: &str, path: Option<&PathBuf>| {
        guide
            .components
            .iter()
            .find(|c| c.guid == guid && c.is_selected)
            .zip(path.and_then(|p| p.file_name()).and_then(|n| n.to_str()))
            .is_some_and(|(component, filename)| offer_prompt(component, filename))
    };
    let mut captured = report.clone();
    captured
        .outcomes
        .retain(|outcome| eligible(&outcome.component_guid, outcome.path.as_ref()));
    captured
        .skipped
        .retain(|skip| eligible(&skip.component_guid, skip.path.as_ref()));
    let scan = collect_fomod_consumers(guide, &captured, options, cancelled, &[]);
    // Eligibility here is the recursive `offer_prompt` filter above, and the desktop
    // offers its editor without assessing saved choices, so every consumer is pending.
    FomodFollowups {
        followups: pending_fomod_followups(scan.archives.iter().flat_map(|archive| {
            archive.consumers.iter().map(|consumer| {
                (
                    archive.archive.as_path(),
                    consumer.component_guid.as_str(),
                    true,
                    false,
                )
            })
        })),
        inspection_errors: scan.inspection_errors,
    }
}

/// Per-archive follow-up deduplication shared by desktop and Fetch: each archive is
/// offered once, bound to its first consumer that is eligible and still lacks usable
/// choices. Eligibility is the caller's: Fetch passes the top-level `should_prompt`
/// captured at collection, while the desktop pre-filters with recursive `offer_prompt`.
/// Items are `(archive, component GUID, prompt eligible, usable choices)` in capture order.
pub fn pending_fomod_followups<'a>(
    consumers: impl IntoIterator<Item = (&'a Path, &'a str, bool, bool)>,
) -> Vec<FomodFollowup> {
    let mut followups: Vec<FomodFollowup> = Vec::new();
    for (archive, guid, eligible, usable_choices) in consumers {
        if eligible && !usable_choices && !followups.iter().any(|f| f.archive == archive) {
            followups.push(FomodFollowup {
                component_guid: guid.to_owned(),
                archive: archive.to_path_buf(),
            });
        }
    }
    followups
}

/// Desktop queue bindings retain every authorized consumer, even when several
/// consume one archive. Valid saved sessions are preserved rather than replaced
/// by another consumer's post-download choices.
pub fn bound_fomod_followups(
    guide: &Guide,
    report: &RunReport,
    options: &archive::Options,
    cancelled: &AtomicBool,
    completed: &[String],
) -> FomodFollowups {
    let scan = collect_fomod_consumers(guide, report, options, cancelled, completed);
    let mut result = FomodFollowups {
        followups: Vec::new(),
        inspection_errors: scan.inspection_errors,
    };
    for archive in scan.archives {
        let Some(filename) = archive.archive.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        for consumer in archive.consumers {
            if cancelled.load(Ordering::Relaxed) {
                result.followups.clear();
                return result;
            }
            let component = guide
                .components
                .iter()
                .find(|c| c.guid == consumer.component_guid)
                .expect("consumer came from the captured guide");
            let saved = (|| -> Result<bool> {
                let choices = crate::fomod::saved_choices_file(component)?;
                let Some(choices) = choices.as_ref().and_then(|file| file.archive(filename)) else {
                    return Ok(false);
                };
                crate::fomod::current_archive_choices(&archive.module, choices, filename)?;
                Ok(true)
            })();
            match saved {
                Ok(false) if consumer.should_prompt => result.followups.push(FomodFollowup {
                    component_guid: consumer.component_guid,
                    archive: archive.archive.clone(),
                }),
                Ok(_) => {}
                Err(error) => result.inspection_errors.push(FomodInspectionError {
                    component_guid: consumer.component_guid,
                    archive: archive.archive.clone(),
                    error: format!("Saved FOMOD choices need repair: {error:#}"),
                }),
            }
        }
    }
    result
}

/// A GUID/path binding from the captured acquisition report, before inspection deduplication.
pub struct FomodConsumer {
    pub component_guid: String,
    pub should_prompt: bool,
}

pub struct FomodArchiveConsumers {
    pub archive: PathBuf,
    pub module: crate::fomod::ArchiveModule,
    pub consumers: Vec<FomodConsumer>,
}

#[derive(Default)]
pub struct FomodConsumerScan {
    pub archives: Vec<FomodArchiveConsumers>,
    pub inspection_errors: Vec<FomodInspectionError>,
}

/// Inspect each acquired path once, but retain every eligible consumer. Completed
/// parent definitions (including descendants) are excluded before they can own a path.
pub fn collect_fomod_consumers(
    guide: &Guide,
    report: &RunReport,
    options: &archive::Options,
    cancelled: &AtomicBool,
    completed: &[String],
) -> FomodConsumerScan {
    let mut result = FomodConsumerScan::default();
    if report.cancelled {
        return result;
    }
    let archives = report
        .outcomes
        .iter()
        .filter(|o| o.status == TargetStatus::Completed)
        .filter_map(|o| Some((&o.component_guid, o.path.as_ref()?)))
        .chain(
            report
                .skipped
                .iter()
                .filter(|s| s.reason == SkipReason::AlreadyPresent)
                .filter_map(|s| Some((&s.component_guid, s.path.as_ref()?))),
        );
    let mut bindings: Vec<(PathBuf, Vec<FomodConsumer>)> = Vec::new();
    for (guid, path) in archives {
        if cancelled.load(Ordering::Relaxed) {
            return FomodConsumerScan::default();
        }
        // Acquisition candidates are top-level registry owners. Do not enroll an
        // unrelated option merely because it mentions the same archive basename.
        let Some(component) = guide.components.iter().find(|c| {
            c.guid == *guid
                && c.is_selected
                && !completed.iter().any(|id| id.eq_ignore_ascii_case(&c.guid))
        }) else {
            continue;
        };
        let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let eligible = vec![FomodConsumer {
            component_guid: component.guid.clone(),
            should_prompt: crate::fomod::should_prompt(component, filename),
        }];
        if let Some((_, existing)) = bindings.iter_mut().find(|(archive, _)| archive == path) {
            for consumer in eligible {
                if !existing
                    .iter()
                    .any(|old| old.component_guid == consumer.component_guid)
                {
                    existing.push(consumer);
                }
            }
        } else {
            bindings.push((path.clone(), eligible));
        }
    }
    for (path, consumers) in bindings {
        if cancelled.load(Ordering::Relaxed) {
            return FomodConsumerScan::default();
        }
        match crate::fomod::inspect_archive(&path, options, cancelled) {
            Ok(Some(module)) => result.archives.push(FomodArchiveConsumers {
                archive: path,
                module,
                consumers,
            }),
            Ok(None) => {}
            Err(error) => result.inspection_errors.push(FomodInspectionError {
                component_guid: consumers[0].component_guid.clone(),
                archive: path,
                error: format!("{error:#}"),
            }),
        }
    }
    result
}

/// Lifted from the GUI's `should_offer_fomod_editor`: an archive no registry entry names
/// is offered; a named one is offered while any matching entry still wants a prompt.
fn offer_prompt(component: &Component, filename: &str) -> bool {
    fn visit(component: &Component, filename: &str) -> (bool, bool) {
        let matches = crate::fomod::resource_matches_archive(component, filename);
        let pending = matches && crate::fomod::should_prompt(component, filename);
        component
            .options
            .iter()
            .fold((matches, pending), |(found, offer), option| {
                let (child_found, child_offer) = visit(option, filename);
                (found || child_found, offer || child_offer)
            })
    }
    let (matched, pending) = visit(component, filename);
    !matched || pending
}

/// Provider observations carried separately from the stable acquisition report.
/// Lookup failures are advisory and never turn a failed transfer into success.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataReport {
    pub entries: Vec<ResourceMetadataObservation>,
    pub warnings: Vec<String>,
    pub cancelled: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMetadataObservation {
    pub component_guid: String,
    pub registry_url: String,
    pub metadata: crate::host_metadata::Metadata,
}

/// Fetch metadata only after successful origin transfers, never during planning or
/// dry-run and never to work around a detected gate or peer-only acquisition. GameFront
/// is the exception: its provider-only observation needs no network request. One
/// multi-file resource receives a metadata observation only when its origins agree;
/// a discovered Nexus page is not relabelled as one of its individual file IDs.
pub fn collect_metadata(
    plan: &AcquisitionPlan,
    report: &RunReport,
    settings: &RunSettings,
    cancelled: &AtomicBool,
    loopback_ca: Option<&download::LoopbackCa>,
) -> MetadataReport {
    let mut result = MetadataReport::default();
    if report.cancelled || cancelled.load(Ordering::Relaxed) {
        result.cancelled = true;
        return result;
    }
    let mut nexus = None;
    let mut seen = BTreeSet::new();
    for outcome in &report.outcomes {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        let identity = (outcome.component_guid.clone(), outcome.registry_url.clone());
        if seen.contains(&identity) {
            continue;
        }
        let gamefront = matches!(
            download_hosts::provider(&outcome.registry_url),
            Ok(Provider::GameFront)
        );
        if !(gamefront
            || outcome.status == TargetStatus::Completed
                && matches!(outcome.source, Some(SourceKind::Origin(_))))
        {
            continue;
        }
        let Some(target) = plan.targets.get(outcome.target_index) else {
            continue;
        };
        if target.component_guid != outcome.component_guid
            || target.registry_url != outcome.registry_url
        {
            continue;
        }
        seen.insert(identity.clone());
        let mut metadata_url = outcome.registry_url.clone();
        // A direct NXM handoff carries its identity in the path; never retain its key.
        if metadata_url.starts_with("nxm://") {
            if let Ok(protocol::Link::Nxm {
                game_domain,
                mod_id,
                file_id,
                ..
            }) = protocol::parse(&metadata_url)
            {
                metadata_url = format!(
                    "https://www.nexusmods.com/{game_domain}/mods/{mod_id}?file_id={file_id}"
                );
            } else {
                continue;
            }
        }
        let provider = download_hosts::provider(&metadata_url).ok();
        if provider == Some(Provider::Nexus) && nexus.is_none() && settings.nexus_api_key.is_some()
        {
            nexus = match settings.nexus_api_base.as_deref() {
                Some(base) => NexusClient::with_base_url(settings.nexus_api_key.clone(), base),
                None => NexusClient::new(settings.nexus_api_key.clone()),
            }
            .ok();
        }
        let observation = crate::host_metadata::resolve_with_endpoints(
            &metadata_url,
            nexus.as_mut(),
            settings.deadlystream_base.as_deref(),
            settings.mega_api_base.as_deref(),
            loopback_ca,
            cancelled,
        );
        if let Some(warning) = observation.warning {
            result
                .warnings
                .push(format!("Component {}: {warning}", outcome.component_guid));
        }
        if let Some(metadata) = observation.metadata {
            result.entries.push(ResourceMetadataObservation {
                component_guid: identity.0,
                registry_url: identity.1,
                metadata,
            });
        }
    }
    if cancelled.load(Ordering::Relaxed) {
        result.entries.clear();
        result.cancelled = true;
    }
    result
}

/// Atomically add provider observations to registry entries. Authored nonempty values
/// win, including `MetadataHash`, `FileSize`, provider keys and unrelated FOMOD state.
/// A mismatched existing provider prevents mixing two provider contracts.
pub fn record_metadata(guide: &mut Guide, report: &MetadataReport) -> Result<usize> {
    if report.cancelled {
        return Ok(0);
    }
    let mut working = guide.clone();
    let mut changed = 0;
    for observation in &report.entries {
        let component = component_mut(&mut working.components, &observation.component_guid)
            .context("Metadata component is no longer in the guide")?;
        let resource = component
            .resource_registry
            .get_mut(&observation.registry_url)
            .and_then(Value::as_object_mut)
            .context("Metadata resource is no longer an object")?;
        let before = resource.clone();
        // Only the exact migration placeholder is replaceable. A matching authored
        // copy is indistinguishable, so require both URL-SHA1 fields and no provider.
        let legacy_hash = format!(
            "{:x}",
            sha1::Sha1::digest(
                crate::resources::normalized_url(&observation.registry_url).as_bytes()
            )
        );
        let legacy_placeholder = ["ContentKey", "MetadataHash"].iter().all(|key| {
            resource
                .get(&metadata_key(resource, key))
                .and_then(Value::as_str)
                == Some(legacy_hash.as_str())
        }) && resource
            .get(&metadata_key(resource, "HandlerMetadata"))
            .and_then(Value::as_object)
            .is_some_and(|handler| {
                handler
                    .get(&metadata_key(handler, "provider"))
                    .is_none_or(empty_metadata_value)
            });
        let handler_key = metadata_key(resource, "HandlerMetadata");
        let handler = resource
            .entry(handler_key)
            .or_insert_with(|| Value::Object(Default::default()));
        if handler.is_null() {
            *handler = Value::Object(Default::default());
        }
        let handler = handler
            .as_object_mut()
            .context("HandlerMetadata must be an object")?;
        if handler
            .get(&metadata_key(handler, "provider"))
            .is_some_and(|v| {
                !empty_metadata_value(v) && Some(v) != observation.metadata.get("provider")
            })
        {
            continue;
        }
        for (key, value) in &observation.metadata {
            let existing_key = metadata_key(handler, key);
            if crate::host_metadata::PROVIDER_KEYS.contains(&key.as_str())
                && handler.get(&existing_key).is_none_or(empty_metadata_value)
            {
                handler.insert(existing_key, value.clone());
            }
        }
        let provider_fields: crate::host_metadata::Metadata = crate::host_metadata::PROVIDER_KEYS
            .iter()
            .filter_map(|key| {
                handler
                    .get(&metadata_key(handler, key))
                    .map(|value| ((*key).to_owned(), value.clone()))
            })
            .collect();
        let hash = crate::host_metadata::metadata_hash(&provider_fields);
        let hash_key = metadata_key(resource, "MetadataHash");
        if legacy_placeholder || resource.get(&hash_key).is_none_or(empty_metadata_value) {
            resource.insert(hash_key, hash.into());
        }
        let size_key = metadata_key(resource, "FileSize");
        if resource.get(&size_key).is_none_or(|v| {
            empty_metadata_value(v) || (legacy_placeholder && v.as_u64() == Some(0))
        }) && let Some(size) = crate::host_metadata::file_size(&provider_fields)
        {
            resource.insert(size_key, size.into());
        }
        if *resource != before {
            changed += 1;
        }
    }
    *guide = working;
    Ok(changed)
}

fn empty_metadata_value(value: &Value) -> bool {
    value.is_null() || value.as_str().is_some_and(|s| s.is_empty())
}

fn metadata_key(map: &serde_json::Map<String, Value>, key: &str) -> String {
    map.keys()
        .find(|existing| existing.eq_ignore_ascii_case(key))
        .cloned()
        .unwrap_or_else(|| key.into())
}
