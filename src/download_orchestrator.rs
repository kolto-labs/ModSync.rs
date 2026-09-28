//! Selected-guide download orchestration: one library owns which components qualify,
//! which URLs and filenames are wanted, what already exists, source order, retries,
//! cancellation, verification, guide write-back and FOMOD follow-up eligibility.
//! Frontends only format `AcquisitionEvent`s and the reports returned here.
use crate::{
    Component, Guide,
    acquisition::{
        Acquire, Acquired, AcquisitionEvent, ArchiveTarget, ManualReason, OriginRequirement,
        PeerEndpoint, Phase, SharedDestination, SkipReason, Source, SourceKind,
    },
    archive,
    download::{self, Destination, FailureClass, FetchProgress},
    download_hosts::{self, Provider},
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

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
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
    if exact.is_file() {
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
fn existing_valid(path: &Path, expected: Option<&str>, options: &archive::Options) -> bool {
    if let Some(expected) = expected
        && !sha256_file(path).is_ok_and(|actual| actual.eq_ignore_ascii_case(expected))
    {
        return false;
    }
    archive::verify(path, path, options, &AtomicBool::new(false)).is_ok()
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
        // Not automated yet (Task 1c / open questions 1 and 2).
        Provider::GameFront | Provider::Mega | Provider::DeadlyStream => {
            manual(ManualReason::Unsupported)
        }
    };
    match primary {
        Source::Origin { .. } => std::iter::once(primary).chain(peer).collect(),
        Source::Manual {
            reason: ManualReason::Unsupported,
            ..
        } => peer.into_iter().chain(std::iter::once(primary)).collect(),
        // Gated: no Peer fallback (lead ruling R2).
        _ => vec![primary],
    }
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
            plan_url(component, url, metadata, options, &mut result)?;
        }
    }
    let links = shared_destinations(&result.targets);
    for (target, link) in result.targets.iter_mut().zip(links) {
        target.shared_destination = link;
    }
    Ok(result)
}

/// Whether two targets may publish one file: equal known names (ASCII
/// case-insensitive, like `find_existing` and the C# `Files` map), or the same registry
/// URL, and so the same bytes, when either name is revealed only by the transfer.
fn may_share_file(a: &ArchiveTarget, b: &ArchiveTarget) -> bool {
    match (&a.filename, &b.filename) {
        (Some(a_name), Some(b_name)) => a_name.eq_ignore_ascii_case(b_name),
        _ => a.registry_url == b.registry_url,
    }
}

/// Two targets that publish one file must not transfer concurrently: the loser fails on
/// the existing destination and is neither recorded nor bound. Each target that may
/// share a file with an earlier one joins that target's group, owned by the first
/// target of the group in guide order. `run` rechecks a plan against this.
fn shared_destinations(targets: &[ArchiveTarget]) -> Vec<Option<SharedDestination>> {
    let mut links: Vec<Option<SharedDestination>> = Vec::with_capacity(targets.len());
    for target in targets {
        let owner = targets
            .iter()
            .zip(&links)
            .enumerate()
            .find_map(|(index, (earlier, link))| {
                may_share_file(earlier, target)
                    .then(|| link.map_or(index, SharedDestination::owner))
            });
        links.push(owner.map(|owner| {
            if targets[owner].registry_url == target.registry_url {
                SharedDestination::SameUrl(owner)
            } else {
                SharedDestination::DifferentUrl(owner)
            }
        }));
    }
    links
}

fn plan_url(
    component: &Component,
    url: &str,
    metadata: &Value,
    options: &PlanOptions,
    plan: &mut AcquisitionPlan,
) -> Result<()> {
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
        download_plan::select_filenames(component, url, &names, Some(&options.source_dir))?
            .into_iter()
            .map(Some)
            .collect()
    };
    for filename in filenames {
        if let Some(name) = &filename
            && let Some(path) = find_existing(&options.source_dir, name)
            && existing_valid(&path, expected.as_deref(), &options.archive)
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

pub struct RunSettings {
    pub workers: usize,
    /// Extra attempts for transient failures only.
    pub retries: usize,
    pub archive: archive::Options,
    pub nexus_api_key: Option<String>,
    /// Probe-only API base override (HTTPS, or loopback HTTP, enforced by `NexusClient`).
    pub nexus_api_base: Option<String>,
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
    /// Validated once per run: Ok(true) premium, Ok(false) free/invalid, Err(text) failed.
    premium: Option<std::result::Result<bool, String>>,
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
    mut observe: impl FnMut(AcquisitionEvent) + Send,
    loopback_ca: Option<&download::LoopbackCa>,
) -> Result<RunReport> {
    ensure!(
        settings.workers > 0,
        "Download worker count must be positive"
    );
    // A hand-built plan must link exactly as `plan` would, or two workers could write one
    // file again.
    let links = shared_destinations(&plan.targets);
    for (index, (target, link)) in plan.targets.iter().zip(links).enumerate() {
        ensure!(
            target.shared_destination == link,
            "Download target {index} is linked as {:?}, but its file name and URL link it as {link:?}",
            target.shared_destination
        );
    }
    // Before any race of this run exists, so nothing swept here can be ours.
    sweep_stale_peer_staging(&plan.source_dir, std::time::SystemTime::now());
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
    // Premium is resolved exactly once per run, inside the library.
    let premium = match &nexus {
        Some(client) if origins().any(|r| r == OriginRequirement::NexusPremium) => Some(
            client
                .lock()
                .unwrap()
                .validate()
                .map(|result| result.is_valid && result.is_premium)
                .map_err(|error| format!("Nexus key validation failed: {error:#}")),
        ),
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
            // Only group owners are queued. The owner's worker settles its linked targets
            // after it, so targets that may publish one file never run at the same time.
            let owners = plan
                .targets
                .iter()
                .enumerate()
                .filter(|(_, target)| target.shared_destination.is_none())
                .collect();
            download_queue::run_ordered(owners, settings.workers, |_, (index, target)| {
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
/// earlier member that ran the same source list (and knew its name, when this target
/// does), since a repeat would ask the same sources again. Otherwise it runs its own
/// sources.
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
                    && (member.filename.is_some() || linked.filename.is_none())
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
    /// unknown premium validation must not let Peer run).
    Terminal(String, SourceKind),
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

/// Whether an Origin error must keep Peer from ever starting: a gate (login, captcha,
/// free-wait, HTML landing) or a failed/unknown premium validation (O3).
fn origin_denies_peer(error: &anyhow::Error) -> bool {
    if error
        .chain()
        .any(|e| e.downcast_ref::<PremiumValidationFailed>().is_some())
    {
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

fn step_from_error(error: anyhow::Error, kind: SourceKind) -> Step {
    if error
        .chain()
        .any(|e| e.downcast_ref::<PremiumValidationFailed>().is_some())
    {
        return Step::Terminal(format!("{error:#}"), kind);
    }
    match download::classify(&error) {
        Some(FailureClass::ManualRequired { url, reason }) => {
            Step::Manual(url.clone(), *reason, kind)
        }
        Some(FailureClass::Cancelled) => Step::Cancelled(kind),
        _ => Step::Failed(format!("{error:#}"), kind),
    }
}

fn single_source(
    context: &RunContext<'_>,
    index: usize,
    target: &ArchiveTarget,
    source: &Source,
) -> Step {
    let kind = source.kind();
    match source {
        Source::Existing { path } => match sha256_file(path) {
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
                Err(error) => step_from_error(error, kind),
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
                    Err(error) => step_from_error(error, kind),
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

/// Removes Peer staging folders left in `source_dir` by an earlier, killed run. Each race
/// makes a fresh folder after its run starts, so one last modified before `started`
/// belongs to no live race of this run. Best effort: a folder that stays is still
/// ignored by the planner.
fn sweep_stale_peer_staging(source_dir: &Path, started: std::time::SystemTime) {
    let Ok(entries) = std::fs::read_dir(source_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let stale = is_peer_staging(&entry.file_name())
            && entry.file_type().is_ok_and(|kind| kind.is_dir())
            && entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .is_ok_and(|modified| modified < started);
        if stale {
            let _ = std::fs::remove_dir_all(entry.path());
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
        tempfile::TempPath::try_from_path(&staged.path)?
            .persist_noclobber(&path)
            .map_err(|error| error.error)
            .with_context(|| format!("Could not publish the peer copy to {}", path.display()))?;
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
///   stopped; if that move fails, Origin keeps running.
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
                    Err(error) => {
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
                if matches!(decided, Some(Step::Done(..) | Step::Manual(..))) && !superseded[racer]
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
                (_, Err(error)) => match step_from_error(error, kinds[racer]) {
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
                    Err(error) => last_error = Some((format!("{error:#}"), SourceKind::Peer)),
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
    let (fetch_url, name_hint) = if *provider == Provider::Nexus {
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
        let link = nexus_link(url, target, nexus).map_err(download::request_failure)?;
        let resolved = nexus
            .lock()
            .unwrap()
            .nxm_download_link(&link)
            .map_err(download::request_failure)?;
        (resolved.uri, Some(resolved.file_name))
    } else {
        (url.clone(), None)
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
    let fetched = download::fetch_to_with_loopback_ca(
        &fetch_url,
        destination,
        target.expected_sha256.as_deref(),
        cancelled,
        &context.settings.archive,
        &mut progress,
        context.loopback_ca,
    )
    .map_err(|error| match (provider, download::classify(&error)) {
        // A gate on a resolved Nexus CDN link: point the person at the mod page, not
        // at a signed, expiring mirror URL.
        (Provider::Nexus, Some(FailureClass::ManualRequired { reason, .. })) => {
            download::manual_required(&target.registry_url, *reason, &format!("{error:#}"))
        }
        _ => error,
    })?;
    Ok(Acquired {
        path: fetched.path,
        sha256: fetched.sha256,
    })
}

/// An nxm URL carries its own file id. A nexusmods.com page URL names the file through a
/// `file_id` query parameter or an authored `Files` name; anything else is not guessed.
fn nexus_link(
    url: &str,
    target: &ArchiveTarget,
    nexus: &Mutex<NexusClient>,
) -> Result<protocol::Link> {
    if url.trim_start().to_ascii_lowercase().starts_with("nxm://") {
        return protocol::parse(url);
    }
    let parsed = reqwest::Url::parse(url)?;
    let segments: Vec<&str> = parsed
        .path_segments()
        .map(Iterator::collect)
        .unwrap_or_default();
    ensure!(
        segments.len() >= 3 && segments[1].eq_ignore_ascii_case("mods"),
        "Nexus URL is not a mod page: {url}"
    );
    let game_domain = segments[0].to_ascii_lowercase();
    let mod_id: u64 = segments[2]
        .parse()
        .with_context(|| format!("Nexus URL has no mod id: {url}"))?;
    let query_file = parsed
        .query_pairs()
        .find(|(key, _)| key.eq_ignore_ascii_case("file_id"))
        .and_then(|(_, value)| value.parse::<u64>().ok());
    let file_id = match (query_file, &target.filename) {
        (Some(id), _) => id,
        (None, Some(name)) => {
            let files = nexus
                .lock()
                .unwrap()
                .get_mod_files(&game_domain, &mod_id.to_string())?;
            let file = files
                .iter()
                .find(|file| file.file_name.eq_ignore_ascii_case(name))
                .with_context(|| format!("Nexus mod {mod_id} lists no file named {name}"))?;
            u64::try_from(file.file_id)?
        }
        (None, None) => {
            bail!("Nexus page URL does not name a file; add a Files entry or a file_id: {url}")
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
    let protocol::Link::Nxm { key, expires, .. } = link else {
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
    let keyed = key.as_deref().is_some_and(|k| !k.is_empty()) && *expires > 0;
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
    for (guid, url, path, sha256) in completed.chain(present) {
        let Some(name) = path.and_then(|p| p.file_name()).and_then(|n| n.to_str()) else {
            continue;
        };
        let name = download::one_component(name)?;
        let component = component_mut(&mut working.components, guid)
            .with_context(|| format!("Component {guid} is no longer in the guide"))?;
        let metadata = component
            .resource_registry
            .get_mut(url.as_str())
            .with_context(|| format!("Registry URL is no longer present: {url}"))?
            .as_object_mut()
            .context("Resource metadata must be an object")?;
        if record_hashes
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
