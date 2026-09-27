//! Selected-guide download orchestration: one library owns which components qualify,
//! which URLs and filenames are wanted, what already exists, source order, retries,
//! cancellation, verification, guide write-back and FOMOD follow-up eligibility.
//! Frontends only format `AcquisitionEvent`s and the reports returned here.
use crate::{
    Component, Guide,
    acquisition::{
        Acquire, Acquired, AcquisitionEvent, ArchiveTarget, ManualReason, OriginRequirement,
        PeerEndpoint, Phase, SkipReason, Source, SourceKind,
    },
    archive,
    download::{self, Destination, FailureClass, FetchProgress},
    download_hosts::{self, Provider},
    download_plan, download_queue, protocol,
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
        atomic::{AtomicBool, Ordering},
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
}

impl AcquisitionPlan {
    /// A one-target plan, as used by the nxm handoff.
    pub fn single(target: ArchiveTarget, source_dir: &Path) -> Self {
        Self {
            source_dir: source_dir.to_path_buf(),
            peer_mode: PeerMode::Off,
            targets: vec![target],
            skipped: Vec::new(),
        }
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

/// Plans acquisition for `candidates` (indexes into `guide.components`, in guide order).
///
/// Merge-time follow-up: switch `candidates` to `&[selection::DownloadCandidate]` when the
/// selection branch lands. Only `ResourceRegistry` keys are acquisition input; metadata
/// `Link`/`DownloadLink` never are.
pub fn plan(guide: &Guide, candidates: &[usize], options: &PlanOptions) -> Result<AcquisitionPlan> {
    let mut result = AcquisitionPlan {
        source_dir: options.source_dir.clone(),
        peer_mode: options.peer_mode,
        targets: Vec::new(),
        skipped: Vec::new(),
    };
    let mut seen = BTreeSet::new();
    for &index in candidates {
        let component = guide
            .components
            .get(index)
            .with_context(|| format!("Download candidate {index} is not a guide component"))?;
        if !seen.insert(index) {
            continue;
        }
        for (url, metadata) in &component.resource_registry {
            plan_url(component, url, metadata, options, &mut result)?;
        }
    }
    Ok(result)
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
        &expected,
        metadata_str(metadata, "Magnet"),
    ) {
        (PeerMode::Race | PeerMode::Fallback, Some(sha256), Some(magnet)) => Some(Source::Peer {
            magnet: magnet.to_owned(),
            sha256: sha256.clone(),
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
    mut observe: impl FnMut(AcquisitionEvent) + Send,
) -> Result<RunReport> {
    ensure!(
        settings.workers > 0,
        "Download worker count must be positive"
    );
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
        Some(Mutex::new(match &settings.nexus_api_base {
            Some(base) => NexusClient::with_base_url(key, base)?,
            None => NexusClient::new(key)?,
        }))
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
            download_queue::run_ordered(
                plan.targets.iter().collect(),
                settings.workers,
                |index, target| acquire_target(context, index, target),
            )
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
    }
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
}

fn acquire_target(context: &RunContext<'_>, index: usize, target: &ArchiveTarget) -> TargetOutcome {
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
            race_origin_peer(context, index, target, source)
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
        }
    }
    TargetOutcome {
        error: last_error.or_else(|| Some("No acquisition source".into())),
        ..outcome(index, target, TargetStatus::Failed)
    }
}

fn step_from_error(error: anyhow::Error, kind: SourceKind) -> Step {
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
        Source::Existing { path } => Step::Done(
            Acquired {
                path: path.clone(),
                sha256: sha256_file(path).unwrap_or_default(),
            },
            kind,
        ),
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
                    let _ = context.events.send(event);
                };
                match peer.acquire(
                    target,
                    &target.destination(&context.plan.source_dir),
                    context.cancelled,
                    &mut on,
                ) {
                    Ok(acquired) => Step::Done(acquired, kind),
                    Err(error) => step_from_error(error, kind),
                }
            }
        },
    }
}

/// Origin races Peer. Each racer has its own flag, linked to the parent cancel by this
/// thread. The first `Ok` wins; the loser is stopped, joined, and reported `Superseded`.
/// An Origin gate stops the Peer and ends the target: a peer copy never satisfies a gate.
fn race_origin_peer(
    context: &RunContext<'_>,
    index: usize,
    target: &ArchiveTarget,
    origin: &Source,
) -> Step {
    let peer = context
        .settings
        .peer
        .as_ref()
        .expect("race requires a peer source");
    let origin_kind = origin.kind();
    let flags = [AtomicBool::new(false), AtomicBool::new(false)];
    let (results, receiver) = mpsc::channel::<(usize, Result<Acquired>)>();
    let destination = target.destination(&context.plan.source_dir);
    thread::scope(|scope| {
        {
            let results = results.clone();
            let flag = &flags[0];
            scope.spawn(move || {
                let mut on = |event: AcquisitionEvent| {
                    let _ = context.events.send(event);
                };
                let _ = results.send((
                    0,
                    origin_with_retries(context, index, target, origin, flag, &mut on),
                ));
            });
        }
        {
            let flag = &flags[1];
            let destination = &destination;
            scope.spawn(move || {
                let mut on = |event: AcquisitionEvent| {
                    let _ = context.events.send(event);
                };
                let _ = results.send((1, peer.acquire(target, destination, flag, &mut on)));
            });
        }
        let kinds = [origin_kind, SourceKind::Peer];
        let mut finished: [Option<Result<Acquired>>; 2] = [None, None];
        let mut decided: Option<Step> = None;
        let mut received = 0;
        while received < 2 {
            let (racer, result) = match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(message) => message,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if context.cancelled.load(Ordering::Relaxed) {
                        flags.iter().for_each(|f| f.store(true, Ordering::Relaxed));
                    }
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            received += 1;
            let other = 1 - racer;
            if decided.is_some() {
                // The loser after a decision, joined here by receiving its result.
                match (&result, &decided) {
                    (Ok(acquired), Some(Step::Manual(..))) => {
                        // Published before the gate stop reached it; a gated target keeps
                        // no peer copy.
                        let _ = std::fs::remove_file(&acquired.path);
                    }
                    (_, Some(Step::Done(..))) => {
                        emit(context, index, target, kinds[racer], Phase::Superseded);
                    }
                    _ => {}
                }
                finished[racer] = Some(result);
                continue;
            }
            match result {
                Ok(acquired) => {
                    flags[other].store(true, Ordering::Relaxed);
                    decided = Some(Step::Done(acquired, kinds[racer]));
                }
                Err(error) => match step_from_error(error, kinds[racer]) {
                    gate @ Step::Manual(_, reason, _) if racer == 0 && reason.is_gate() => {
                        flags[other].store(true, Ordering::Relaxed);
                        decided = Some(gate);
                    }
                    Step::Cancelled(kind) if context.cancelled.load(Ordering::Relaxed) => {
                        flags[other].store(true, Ordering::Relaxed);
                        decided = Some(Step::Cancelled(kind));
                    }
                    step => {
                        let message = match &step {
                            Step::Failed(message, _) => message.clone(),
                            _ => "Source stopped".into(),
                        };
                        if finished[other].is_none() {
                            // The other racer is still running; the caller reports the last.
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
                            finished[racer] = Some(Err(anyhow::anyhow!(message)));
                        } else {
                            decided = Some(Step::Failed(message, kinds[racer]));
                        }
                    }
                },
            }
        }
        decided.unwrap_or_else(|| {
            Step::Failed("Download race ended without a result".into(), origin_kind)
        })
    })
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
                // A free or invalid key: manual, with no download request (gate, R2).
                Some(Ok(false)) | None => {
                    return Err(download::manual_required(
                        url,
                        ManualReason::FreeUserWait,
                        "Nexus Mods needs a premium account for automatic downloads; download it on the website.",
                    ));
                }
                Some(Err(error)) => bail!("{error}"),
            }
        }
        let nexus = context
            .nexus
            .as_ref()
            .context("Nexus client missing for a Nexus origin")?;
        let link = nexus_link(url, target, nexus)?;
        let resolved = nexus.lock().unwrap().nxm_download_link(&link)?;
        (resolved.uri, Some(resolved.file_name))
    } else {
        (url.clone(), None)
    };
    let destination = target.destination(&context.plan.source_dir);
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
    let fetched = download::fetch_to(
        &fetch_url,
        destination,
        target.expected_sha256.as_deref(),
        cancelled,
        &context.settings.archive,
        &mut progress,
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
    let mut url = format!("nxm://{game_domain}/mods/{mod_id}/files/{file_id}");
    let mut query = Vec::new();
    if let Some(key) = key.as_deref().filter(|k| !k.is_empty()) {
        query.push(format!("key={key}"));
    }
    if *expires > 0 {
        query.push(format!("expires={expires}"));
    }
    if *user_id > 0 {
        query.push(format!("user_id={user_id}"));
    }
    if !query.is_empty() {
        url.push('?');
        url.push_str(&query.join("&"));
    }
    Ok(url)
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

fn component_ref<'a>(components: &'a [Component], guid: &str) -> Option<&'a Component> {
    components.iter().find_map(|component| {
        (component.guid == guid)
            .then_some(component)
            .or_else(|| component_ref(&component.options, guid))
    })
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
    /// Set when inspection failed; the archive still needs a person's attention.
    pub error: Option<String>,
}

/// Mirrors C# `FomodPostDownloadOrchestrator`: selected components only, archives
/// acquired or present in this run, positive FOMOD detection, and `fomod::should_prompt`
/// (configured/warned suppress; dismissed prompts again). Nothing after a cancelled run.
pub fn fomod_followups(
    guide: &Guide,
    report: &RunReport,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Vec<FomodFollowup> {
    if report.cancelled {
        return Vec::new();
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
    let mut seen = BTreeSet::new();
    let mut followups = Vec::new();
    for (guid, path) in archives {
        if cancelled.load(Ordering::Relaxed) {
            return Vec::new();
        }
        if !seen.insert(path.clone()) {
            continue;
        }
        let Some(component) = component_ref(&guide.components, guid).filter(|c| c.is_selected)
        else {
            continue;
        };
        let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !offer_prompt(component, filename) {
            continue;
        }
        match crate::fomod::inspect_archive(path, options, cancelled) {
            Ok(Some(_)) => followups.push(FomodFollowup {
                component_guid: guid.clone(),
                archive: path.clone(),
                error: None,
            }),
            Ok(None) => {}
            Err(error) => followups.push(FomodFollowup {
                component_guid: guid.clone(),
                archive: path.clone(),
                error: Some(format!("{error:#}")),
            }),
        }
    }
    followups
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
