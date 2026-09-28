//! Desktop presentation of the shared acquisition run (downloads plan, step 1).
//!
//! The orchestrator owns planning, sources, retries, cancellation, verification and
//! write-back. This module only projects its facts: the bound `AcquisitionPlan`, the
//! `AcquisitionEvent`s of one run and that run's authoritative `RunReport`. Every
//! worker message carries its run; only the active run may change rows, and a run whose
//! guide/folder context changed is frozen and never applied to the current guide.
use eframe::egui;
use modsync::{
    acquisition::{AcquisitionEvent, ManualReason, Phase, SkipReason, SourceKind},
    download_hosts::Provider,
    download_orchestrator::{
        AcquisitionPlan, FomodFollowups, PeerMode, RecordSummary, RunReport, TargetStatus,
    },
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// Longest diagnostic text kept per row; the activity log is bounded separately.
const MAX_TEXT: usize = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// Selected-guide Fetch Downloads.
    Fetch,
    /// One queued nxm:// browser handoff.
    Nxm,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Self::Fetch => "Fetch Downloads",
            Self::Nxm => "Nexus handoff",
        }
    }
}

/// Worker to UI. The run identity travels beside it in `Message::Acquisition`.
pub(super) enum Update {
    /// Planning (including existing-archive verification) finished off the UI thread.
    Prepared {
        plan: Box<AcquisitionPlan>,
        /// Selected top-level components with resources (Scope::All), not archives.
        selected_components: usize,
        /// Proxy endpoints from the effective peer settings, redacted from diagnostics.
        secrets: Vec<String>,
    },
    Event(Box<AcquisitionEvent>),
    /// Always the last message of a run, sent even when the worker panics.
    Terminal(anyhow::Result<Box<Finished>>),
}

pub(super) struct Finished {
    pub report: RunReport,
    /// Computed off the UI thread for a noncancelled run; shown only when accepted.
    pub fomod: Option<FomodFollowups>,
}

/// The context a run was started for, captured before any asynchronous work.
pub(super) struct Binding {
    /// `App::acquisition_context` at start: guide revision, folders, completed base.
    pub context: String,
    /// The guide the run planned from; `record` applies to a clone of it.
    pub guide: modsync::Guide,
    /// Normalized workspace the run writes into.
    pub source_dir: PathBuf,
    /// Display names by component GUID, captured with the guide.
    pub names: HashMap<String, String>,
}

/// A settled run handed back to the app, which alone may change the guide.
pub(super) struct Settled {
    pub kind: Kind,
    pub binding: Binding,
    pub context_changed: bool,
}

struct Active {
    run: u64,
    kind: Kind,
    binding: Binding,
    cancelled: Arc<AtomicBool>,
    stopping: bool,
    context_changed: bool,
    prepared: bool,
}

#[derive(Clone, Debug)]
enum RowPhase {
    Resolving,
    Transferring,
    Verifying,
    Completed,
    Failed { error: String, will_fallback: bool },
    Superseded,
    Manual { reason: ManualReason, url: String },
    Cancelled,
    Skipped(SkipReason),
}

/// One source of one target. Created only when that source reports an event, so a
/// Peer that never started is never shown as active.
struct SourceRow {
    kind: SourceKind,
    phase: RowPhase,
    bytes: Option<u64>,
    total: Option<u64>,
    peers: Option<u32>,
}

/// One archive target of the bound plan: run + target index is its key.
struct TargetRow {
    index: usize,
    component: String,
    /// Registry URL from the bound plan (events carry none), redacted for display.
    registry_url: String,
    filename: Option<String>,
    planned: Vec<SourceKind>,
    sources: Vec<SourceRow>,
    outcome: Option<OutcomeView>,
}

struct OutcomeView {
    status: TargetStatus,
    source: Option<SourceKind>,
    shared_with: Option<usize>,
    error: Option<String>,
    manual: Option<(ManualReason, String)>,
}

/// What a click in the panel asks the app to do.
pub(super) enum Action {
    Open(String),
}

#[derive(Default)]
pub(super) struct Panel {
    next_run: u64,
    active: Option<Active>,
    /// Rows of the last prepared run only; replaced when a new run starts.
    rows: Vec<TargetRow>,
    header: Vec<String>,
    skipped: Vec<String>,
    warnings: Vec<String>,
    summary: Vec<String>,
    headline: Option<String>,
    fomod: Vec<String>,
    redactor: Redactor,
    /// Accepted resource records not yet written to a guide file.
    pub unsaved_records: bool,
}

impl Panel {
    /// A new run identity, assigned before any asynchronous preparation. The token is
    /// the run's own; a later run never reuses it.
    pub fn begin(
        &mut self,
        kind: Kind,
        binding: Binding,
        cancelled: Arc<AtomicBool>,
        secrets: &[String],
    ) -> u64 {
        self.next_run += 1;
        self.rows.clear();
        self.header.clear();
        self.skipped.clear();
        self.warnings.clear();
        self.summary.clear();
        self.fomod.clear();
        self.headline = None;
        self.redactor = Redactor::new(secrets);
        self.header.push(format!(
            "{}: workspace {}",
            kind.label(),
            binding.source_dir.display()
        ));
        self.active = Some(Active {
            run: self.next_run,
            kind,
            binding,
            cancelled,
            stopping: false,
            context_changed: false,
            prepared: false,
        });
        self.next_run
    }

    /// A run identity for a direct URL download, from the same monotonic counter.
    pub fn direct_run(&mut self) -> u64 {
        self.next_run += 1;
        self.next_run
    }

    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub fn accepts(&self, run: u64) -> bool {
        self.active.as_ref().is_some_and(|active| active.run == run)
    }

    pub fn stopping(&self) -> bool {
        self.active.as_ref().is_some_and(|active| active.stopping)
    }

    /// Stop the active run. Rows stay until its terminal arrives: workers still join and
    /// clean up, so the panel shows stopping, never idle.
    pub fn request_stop(&mut self) {
        if let Some(active) = &mut self.active {
            active.cancelled.store(true, Ordering::SeqCst);
            active.stopping = true;
        }
    }

    /// Per frame and at terminal. A change is sticky for the rest of the run.
    pub fn check_context(&mut self, current: &str) {
        if let Some(active) = &mut self.active
            && !active.context_changed
            && active.binding.context != current
        {
            active.context_changed = true;
        }
    }

    pub fn redact(&self, text: &str) -> String {
        self.redactor.redact(text)
    }

    /// Applies a non-terminal update of the active run. Callers check `accepts` first.
    pub fn apply(&mut self, update: Update) {
        let Some(active) = &mut self.active else {
            return;
        };
        // Secrets (proxy endpoints) reach the redactor even when the rows are frozen below.
        if let Update::Prepared { secrets, .. } = &update {
            let mut all = self.redactor.secrets.clone();
            all.extend(secrets.iter().cloned());
            self.redactor = Redactor::new(&all);
        }
        // A changed context freezes the rows: they describe the captured folders only.
        if active.context_changed {
            return;
        }
        match update {
            Update::Prepared {
                plan,
                selected_components,
                ..
            } => {
                active.prepared = true;
                let names = &active.binding.names;
                if active.kind == Kind::Fetch {
                    self.header.push(format!(
                        "{selected_components} selected components with resources (base and widescreen)"
                    ));
                }
                self.header.push(format!(
                    "{} archive targets planned, {} skipped before transfer; peer mode {}",
                    plan.targets.len(),
                    plan.skipped.len(),
                    match plan.peer_mode {
                        PeerMode::Off => "Off",
                        PeerMode::Race => "Race",
                        PeerMode::Fallback => "Fallback",
                    }
                ));
                let name = |guid: &str| names.get(guid).cloned().unwrap_or_else(|| guid.into());
                self.rows = plan
                    .targets
                    .iter()
                    .enumerate()
                    .map(|(index, target)| TargetRow {
                        index,
                        component: name(&target.component_guid),
                        registry_url: self.redactor.redact(&target.registry_url),
                        filename: target.filename.clone(),
                        planned: target.sources.iter().map(|s| s.kind()).collect(),
                        sources: Vec::new(),
                        outcome: None,
                    })
                    .collect();
                self.skipped = plan
                    .skipped
                    .iter()
                    .map(|skip| {
                        format!(
                            "{} — {}: {}{}",
                            name(&skip.component_guid),
                            skip.filename.as_deref().unwrap_or("(no file name)"),
                            skip_text(skip.reason),
                            skip.detail
                                .as_deref()
                                .map(|d| format!(" ({})", self.redactor.redact(d)))
                                .unwrap_or_default()
                        )
                    })
                    .collect();
                self.warnings = plan
                    .warnings
                    .iter()
                    .map(|w| bounded(self.redactor.redact(w)))
                    .collect();
            }
            Update::Event(event) => {
                let redactor = &self.redactor;
                let Some(row) = self.rows.get_mut(event.target_index) else {
                    return;
                };
                if row.filename.is_none() {
                    row.filename = event.filename.clone();
                }
                let position = match row.sources.iter().position(|s| s.kind == event.source) {
                    Some(position) => position,
                    None => {
                        row.sources.push(SourceRow {
                            kind: event.source,
                            phase: RowPhase::Resolving,
                            bytes: None,
                            total: None,
                            peers: None,
                        });
                        row.sources.len() - 1
                    }
                };
                let source = &mut row.sources[position];
                source.phase = match event.phase {
                    Phase::Resolving => RowPhase::Resolving,
                    Phase::Transferring {
                        bytes,
                        total,
                        peers,
                    } => {
                        source.bytes = Some(bytes);
                        source.total = total;
                        if peers.is_some() {
                            source.peers = peers;
                        }
                        RowPhase::Transferring
                    }
                    Phase::Verifying => RowPhase::Verifying,
                    Phase::Completed { path, .. } => {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            row.filename = Some(name.to_owned());
                        }
                        RowPhase::Completed
                    }
                    Phase::Skipped { reason } => RowPhase::Skipped(reason),
                    Phase::SourceFailed {
                        error,
                        will_fallback,
                    } => RowPhase::Failed {
                        error: bounded(redactor.redact(&error)),
                        will_fallback,
                    },
                    Phase::Superseded => RowPhase::Superseded,
                    Phase::ManualRequired { url, reason } => RowPhase::Manual { reason, url },
                    Phase::Cancelled => RowPhase::Cancelled,
                };
            }
            Update::Terminal(_) => {}
        }
    }

    /// Ends the active run. Only the matching run settles; its receiver is then cleared
    /// by the caller.
    pub fn settle(&mut self, run: u64, current: &str) -> Option<Settled> {
        if !self.accepts(run) {
            return None;
        }
        self.check_context(current);
        let active = self.active.take()?;
        Some(Settled {
            kind: active.kind,
            binding: active.binding,
            context_changed: active.context_changed,
        })
    }

    /// The authoritative terminal. Final counts come from outcomes and plan skips only.
    pub fn finish(
        &mut self,
        kind: Kind,
        finished: &Finished,
        applied: Result<&RecordSummary, &str>,
        names: &HashMap<String, String>,
    ) {
        let report = &finished.report;
        for outcome in &report.outcomes {
            if let Some(row) = self.rows.get_mut(outcome.target_index) {
                if outcome.filename.is_some() {
                    row.filename = outcome.filename.clone();
                }
                row.outcome = Some(OutcomeView {
                    status: outcome.status,
                    source: outcome.source,
                    shared_with: outcome.shared_with,
                    error: outcome
                        .error
                        .as_deref()
                        .map(|e| bounded(self.redactor.redact(e))),
                    manual: outcome.manual.as_ref().map(|m| (m.reason, m.url.clone())),
                });
            }
        }
        let count = |status| {
            report
                .outcomes
                .iter()
                .filter(|o| o.status == status)
                .count()
        };
        let (completed, manual, failed, cancelled) = (
            count(TargetStatus::Completed),
            count(TargetStatus::ManualRequired),
            count(TargetStatus::Failed),
            count(TargetStatus::Cancelled),
        );
        let shared = report
            .outcomes
            .iter()
            .filter(|o| o.status == TargetStatus::Completed && o.shared_with.is_some())
            .count();
        let present = report
            .skipped
            .iter()
            .filter(|s| s.reason == SkipReason::AlreadyPresent)
            .count();
        let other_skips = report.skipped.len() - present;
        self.headline = Some(if report.cancelled {
            format!(
                "{} stopped: {completed} of {} archive targets finished before Stop",
                kind.label(),
                report.outcomes.len()
            )
        } else if failed > 0 {
            format!(
                "{} finished with failures: {failed} of {} archive targets failed",
                kind.label(),
                report.outcomes.len()
            )
        } else if manual > 0 {
            format!(
                "{} needs manual downloads: {manual} archive targets must be downloaded in a browser",
                kind.label()
            )
        } else {
            format!("{} finished: every planned archive is ready", kind.label())
        });
        self.summary.push(format!(
            "Archive targets: {} planned; {completed} completed{}, {manual} manual, {failed} failed, {cancelled} cancelled.",
            report.outcomes.len(),
            if shared > 0 {
                format!(" ({shared} reused another target's transfer)")
            } else {
                String::new()
            }
        ));
        self.summary.push(format!(
            "Skipped before transfer: {present} already present and verified, {other_skips} not eligible."
        ));
        match applied {
            Ok(summary) => {
                self.summary.push(format!(
                    "Guide resource records: {} files marked downloaded, {} kept as authored false.",
                    summary.recorded.len(),
                    summary.kept_false.len()
                ));
            }
            Err(reason) => self.summary.push(bounded(self.redactor.redact(reason))),
        }
        self.summary.push(
            "Downloads never validate or install. Run validation after downloads finish.".into(),
        );
        self.fomod.clear();
        if applied.is_ok()
            && let Some(fomod) = &finished.fomod
        {
            for followup in &fomod.followups {
                self.fomod.push(format!(
                    "{}: FOMOD installer in {}. Configure it from Validation tools with Configure FOMOD archive… before validating.",
                    names
                        .get(&followup.component_guid)
                        .map_or(followup.component_guid.as_str(), String::as_str),
                    followup
                        .archive
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                ));
            }
            for error in &fomod.inspection_errors {
                self.fomod.push(bounded(format!(
                    "FOMOD inspection failed for {}: {}",
                    error
                        .archive
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    self.redactor.redact(&error.error)
                )));
            }
        }
    }

    /// A terminal that did not produce a report (preparation or worker failure).
    pub fn failed(&mut self, kind: Kind, error: &str) {
        self.headline = Some(format!("{} could not run", kind.label()));
        self.summary.push(bounded(self.redactor.redact(error)));
    }

    /// Status for the sidebar, footer and utility view; never a measured percent.
    pub fn status(&self) -> String {
        match &self.active {
            Some(active) if active.stopping => {
                "Stopping downloads… waiting for transfers to stop and clean up".into()
            }
            Some(active) if active.context_changed => {
                "Guide or folders changed: this run's results will not be applied".into()
            }
            Some(active) if !active.prepared => format!(
                "{}: preparing (planning and verifying existing archives)…",
                active.kind.label()
            ),
            Some(active) => {
                let settled = self
                    .rows
                    .iter()
                    .filter(|row| row.sources.iter().any(|s| s.settled()))
                    .count();
                let transferring = self
                    .rows
                    .iter()
                    .filter(|row| {
                        row.sources
                            .iter()
                            .any(|s| matches!(s.phase, RowPhase::Transferring))
                    })
                    .count();
                format!(
                    "{}: {transferring} transferring, {settled} of {} archive targets settled",
                    active.kind.label(),
                    self.rows.len()
                )
            }
            None => self
                .headline
                .clone()
                .unwrap_or_else(|| "No downloads started".into()),
        }
    }

    pub fn show(&self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        ui.label(egui::RichText::new(self.status()).strong());
        for line in &self.header {
            ui.label(line);
        }
        for line in &self.summary {
            ui.label(line);
        }
        if self.unsaved_records {
            ui.label("Unsaved resource records: open Guide editor, then Save guide as…, to keep them. The guide file is never saved automatically.");
        }
        for line in &self.fomod {
            ui.label(line);
        }
        for line in &self.warnings {
            ui.label(format!("Guide warning: {line}"));
        }
        let mut groups: [Vec<&TargetRow>; 4] = Default::default();
        for row in &self.rows {
            groups[row.group()].push(row);
        }
        for (group, title, open) in [
            (1, "Active", true),
            (0, "Needs attention", true),
            (2, "Queued", false),
            (3, "Finished", false),
        ] {
            if groups[group].is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(format!("{title} ({})", groups[group].len()))
                .id_salt(("downloads_group", group))
                .default_open(open)
                .show(ui, |ui| {
                    for row in &groups[group] {
                        if let Some(next) = row.show(ui, &self.redactor) {
                            action = Some(next);
                        }
                    }
                });
        }
        if !self.skipped.is_empty() {
            egui::CollapsingHeader::new(format!(
                "Skipped before transfer ({})",
                self.skipped.len()
            ))
            .id_salt("downloads_skipped")
            .show(ui, |ui| {
                for line in &self.skipped {
                    ui.label(line);
                }
            });
        }
        action
    }
}

impl SourceRow {
    fn settled(&self) -> bool {
        matches!(
            self.phase,
            RowPhase::Completed | RowPhase::Manual { .. } | RowPhase::Cancelled
        ) || matches!(
            self.phase,
            RowPhase::Failed {
                will_fallback: false,
                ..
            }
        )
    }

    fn text(&self) -> String {
        let progress = || match (self.bytes, self.total) {
            (Some(bytes), Some(total)) if total > 0 => format!(
                "{} of {} ({}%)",
                size(bytes),
                size(total),
                (bytes.min(total) * 100) / total
            ),
            (Some(bytes), _) => format!("{} (total size unknown)", size(bytes)),
            (None, _) => "waiting for data".into(),
        };
        let peers = self
            .peers
            .map(|p| format!(", {p} peers"))
            .unwrap_or_default();
        match &self.phase {
            RowPhase::Resolving => "resolving".into(),
            RowPhase::Transferring => format!("transferring {}{peers}", progress()),
            RowPhase::Verifying => "verifying SHA-256".into(),
            RowPhase::Completed => "completed and verified".into(),
            RowPhase::Failed {
                error,
                will_fallback,
            } => format!(
                "failed: {error}{}",
                if *will_fallback {
                    "; trying the next source"
                } else {
                    ""
                }
            ),
            RowPhase::Superseded if self.bytes.is_none() => {
                "not used: another source finished first".into()
            }
            RowPhase::Superseded => format!(
                "stopped after {}: another source finished first",
                size(self.bytes.unwrap_or(0))
            ),
            RowPhase::Manual { reason, .. } => format!("manual: {}", manual_text(*reason)),
            RowPhase::Cancelled => "cancelled".into(),
            RowPhase::Skipped(reason) => format!("skipped: {}", skip_text(*reason)),
        }
    }
}

impl TargetRow {
    /// 0 needs attention, 1 active, 2 queued, 3 finished.
    fn group(&self) -> usize {
        match &self.outcome {
            Some(o) if o.status == TargetStatus::Completed => 3,
            Some(_) => 0,
            None if self.sources.is_empty() => 2,
            None if self.sources.iter().any(|s| {
                matches!(s.phase, RowPhase::Manual { .. })
                    || matches!(
                        s.phase,
                        RowPhase::Failed {
                            will_fallback: false,
                            ..
                        }
                    )
            }) =>
            {
                0
            }
            None if self
                .sources
                .iter()
                .any(|s| matches!(s.phase, RowPhase::Completed)) =>
            {
                3
            }
            None => 1,
        }
    }

    fn show(&self, ui: &mut egui::Ui, redactor: &Redactor) -> Option<Action> {
        let mut action = None;
        ui.group(|ui| {
            ui.strong(format!("{}. {}", self.index + 1, self.component));
            ui.label(format!(
                "Archive: {}",
                self.filename.as_deref().unwrap_or("(name comes from the host)")
            ));
            ui.label(format!("Registry: {}", self.registry_url));
            if self.sources.is_empty() && self.outcome.is_none() {
                ui.label(format!(
                    "Queued; sources in order: {}",
                    self.planned
                        .iter()
                        .map(|k| source_text(*k))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            for source in &self.sources {
                ui.label(format!("{}: {}", source_text(source.kind), source.text()));
                if let (SourceKind::Peer | SourceKind::Origin(_), RowPhase::Transferring) =
                    (source.kind, &source.phase)
                    && let (Some(bytes), Some(total)) = (source.bytes, source.total)
                    && total > 0
                {
                    ui.add(
                        egui::ProgressBar::new(bytes.min(total) as f32 / total as f32)
                            .desired_height(6.0),
                    );
                }
            }
            let manual = self
                .outcome
                .as_ref()
                .and_then(|o| o.manual.clone())
                .or_else(|| {
                    self.sources.iter().find_map(|s| match &s.phase {
                        RowPhase::Manual { reason, url } => Some((*reason, url.clone())),
                        _ => None,
                    })
                });
            if let Some(outcome) = &self.outcome {
                ui.label(format!(
                    "Result: {}{}{}",
                    match outcome.status {
                        TargetStatus::Completed => "completed",
                        TargetStatus::ManualRequired => "manual download required",
                        TargetStatus::Failed => "failed",
                        TargetStatus::Cancelled => "cancelled",
                    },
                    outcome
                        .source
                        .map(|k| format!(" via {}", source_text(k)))
                        .unwrap_or_default(),
                    outcome
                        .shared_with
                        .map(|t| format!(", reused target {}", t + 1))
                        .unwrap_or_default()
                ));
                if let Some(error) = &outcome.error {
                    ui.label(format!("Reason: {error}"));
                }
            }
            if let Some((reason, url)) = manual {
                ui.label(format!(
                    "Download this archive in your browser ({}), save it in the workspace, then fetch again: {}",
                    manual_text(reason),
                    redactor.redact(&url)
                ));
                let openable = url.starts_with("https://") || url.starts_with("http://");
                if ui
                    .add_enabled(openable, egui::Button::new("Open download page"))
                    .clicked()
                {
                    action = Some(Action::Open(url));
                }
            }
        });
        action
    }
}

fn source_text(kind: SourceKind) -> String {
    match kind {
        SourceKind::Existing => "Existing file".into(),
        SourceKind::Origin(provider) => format!(
            "Origin ({})",
            match provider {
                Provider::Direct => "direct HTTPS",
                Provider::Nexus => "Nexus Mods",
                Provider::DeadlyStream => "DeadlyStream",
                Provider::Mega => "MEGA",
                Provider::GameFront => "GameFront",
            }
        ),
        SourceKind::Peer => "Peer".into(),
        SourceKind::Manual => "Manual".into(),
    }
}

fn manual_text(reason: ManualReason) -> &'static str {
    match reason {
        ManualReason::LoginRequired => "the host requires a login",
        ManualReason::Captcha => "the host requires a CAPTCHA",
        ManualReason::HtmlLanding => "the link opens a web page, not an archive",
        ManualReason::FreeUserWait => "the host requires a free-user wait",
        ManualReason::Unsupported => "this host is not automated",
    }
}

fn skip_text(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::AlreadyPresent => "already in the workspace and verified",
        SkipReason::UrlDisabled => "the guide disables this link",
        SkipReason::AllFilesFlaggedFalse => "every listed file is marked not needed",
        SkipReason::ResolutionMismatch => "made for a different screen resolution",
        SkipReason::UnsafeFilename => "unsafe file name",
        SkipReason::InvalidUrl => "invalid link",
    }
}

fn size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = "B";
    for next in UNITS {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next;
    }
    format!("{value:.1} {unit}")
}

fn bounded(mut text: String) -> String {
    if text.len() > MAX_TEXT {
        let mut end = MAX_TEXT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push('…');
    }
    text
}

/// Removes credentials from diagnostics shown or copied from the desktop: literal
/// secrets (API keys, proxy endpoints), URL user info, queries and fragments (nxm
/// key/expires, signed CDN links, MEGA keys) and whole proxy URLs.
#[derive(Default)]
pub(super) struct Redactor {
    secrets: Vec<String>,
}

impl Redactor {
    pub fn new(secrets: &[String]) -> Self {
        let mut all: Vec<String> = Vec::new();
        for secret in secrets.iter().map(|s| s.trim()).filter(|s| s.len() >= 4) {
            all.push(secret.to_owned());
            // A proxy URL's endpoint also appears bare in transport errors.
            if let Some((_, rest)) = secret.split_once("://") {
                let endpoint = rest
                    .rsplit('@')
                    .next()
                    .unwrap_or(rest)
                    .trim_end_matches('/');
                if endpoint.len() >= 4 {
                    all.push(endpoint.to_owned());
                }
            }
        }
        // Longest first, so a secret containing another is removed whole.
        all.sort_by_key(|s| std::cmp::Reverse(s.len()));
        all.dedup();
        Self { secrets: all }
    }

    pub fn redact(&self, text: &str) -> String {
        static URL: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let url = URL.get_or_init(|| {
            regex::Regex::new(
                r#"(?i)\b([a-z][a-z0-9+.\-]*)://([^\s/?#<>"'@]*@)?([^\s/?#<>"']*)([^\s?#<>"']*)(\?[^\s#<>"']*)?(#[^\s<>"']*)?"#,
            )
            .expect("valid redaction pattern")
        });
        let mut text = text.to_owned();
        for secret in &self.secrets {
            text = text.replace(secret.as_str(), "<redacted>");
        }
        url.replace_all(&text, |captures: &regex::Captures<'_>| {
            let scheme = captures[1].to_ascii_lowercase();
            if scheme.starts_with("socks") {
                return format!("{scheme}://<proxy>");
            }
            format!(
                "{scheme}://{}{}{}{}{}",
                if captures.get(2).is_some() {
                    "<redacted>@"
                } else {
                    ""
                },
                &captures[3],
                &captures[4],
                if captures.get(5).is_some() {
                    "?<redacted>"
                } else {
                    ""
                },
                if captures.get(6).is_some() {
                    "#<redacted>"
                } else {
                    ""
                },
            )
        })
        .into_owned()
    }
}
