mod checkpoints;
mod completion_pages;
mod deployment_panel;
mod desktop_settings;
mod downloads_panel;
mod editor_panel;
mod installing_page;
mod main_window;
mod maintenance;
mod merge_panel;
mod protocol_inbox;
mod protocol_load;
mod protocol_settings;
mod resource_watch;
mod rich_content;
mod selection_panel;
mod sharing;
mod standalone_patcher;
mod structured_log;
mod summary_page;
mod tools_panel;
mod validation_page;
use anyhow::Result;
use eframe::egui;
use modsync::wizard::{Gate, Page, Wizard, metadata_text};
use modsync::{Component, Engine, Guide, PatcherEngine, Settings, download, recovery};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};

enum Message {
    ShellTerminal(Box<main_window::Finished>),
    DeploymentTerminal(Box<deployment_panel::Finished>),
    MergeTerminal(Box<merge_panel::Finished>),
    EditorTerminal(Box<editor_panel::Finished>),
    EditorProgress(u64, String, String),
    CheckpointProgress(u64, String),
    CheckpointTerminal(Box<checkpoints::Finished>),
    /// A direct URL download finished; the run binds its FOMOD follow-up.
    Downloaded(u64, PathBuf, modsync::archive::Options),
    /// Shared acquisition (Fetch or nxm handoff) facts for one run.
    Acquisition(u64, downloads_panel::Update),
    ProtocolTerminal(Box<protocol_load::Finished>),
    Conflicts(modsync::conflicts::Report),
    ValidationProgress(u64, modsync::validation::StageProgress),
    ValidationTerminal(u64, modsync::validation::Report),
    ValidationDone(u64, Result<()>),
    InstallProgress(u64, modsync::install_progress::Progress),
    InstallPrepared(u64, Box<Guide>),
    /// Run, engine result, pending recovery for the captured roots, and the
    /// completion receipt read after an engine success released its lock.
    InstallTerminal(u64, Result<()>, bool, Option<completion_pages::Terminal>),
    Log(String),
    Updates(Box<tools_panel::UpdateResult>),
    UpdateSummary(String, modsync::updates::StoredSummaryState),
    Done(Result<()>),
}
#[derive(Clone, Copy, PartialEq)]
enum Job {
    Editor,
    Checkpoint,
    UpdateSummary,
    Conflicts,
    Validate,
    Install,
    Restore,
    Download,
    NxmDownload,
    Fetch,
    ProtocolOpen,
    Update,
}

/// Hidden probe launch context: explicit key, API base and loopback CA only.
/// Never persisted, and never combined with the per-user instance socket.
pub struct Probe {
    pub nexus_api_key: Option<String>,
    pub nexus_api_base: Option<String>,
    pub extra_ca_file: Option<PathBuf>,
}

struct ProbeContext {
    nexus_api_key: Option<String>,
    nexus_api_base: Option<String>,
    loopback_ca: Option<modsync::download::LoopbackCa>,
}

#[derive(Clone)]
struct FomodPending {
    run: u64,
    context: String,
    path: PathBuf,
    options: modsync::archive::Options,
    dismiss_non_fomod: bool,
    consumer_guids: Vec<String>,
}

#[derive(Default)]
struct PeerView {
    directory: Option<String>,
    settings: Option<modsync::p2p::PeerSettings>,
    error: Option<String>,
    receiver: Option<Receiver<Result<modsync::p2p::PeerSettings>>>,
    saving: bool,
    queued: Vec<(String, serde_json::Value)>,
}

enum SharingContinuation {
    Checkpoint(checkpoints::Action, String),
    Start(Job),
    Restore,
    Load,
    Update,
    Protocol(Box<protocol_load::Ready>),
    Terminal(u64, Result<Box<downloads_panel::Finished>>),
    MergeSave(String),
    Deployment(modsync::deployment::ManagementAction, String),
    Shell(main_window::Work, String),
    Standalone(standalone_patcher::Request),
    Maintenance(maintenance::Request),
}

pub struct App {
    checkpoints: checkpoints::State,
    tools: tools_panel::State,
    editor: editor_panel::State,
    desktop: desktop_settings::State,
    merge: merge_panel::State,
    deployment: deployment_panel::State,
    shell: main_window::State,
    standalone: standalone_patcher::State,
    maintenance: maintenance::State,
    resource_watch: resource_watch::State,
    selection_panel: selection_panel::State,
    sharing: sharing::View,
    sharing_continuation: Option<SharingContinuation>,
    theme: String,
    input: String,
    loaded_input: Option<PathBuf>,
    profile_directory: String,
    profile_name: String,
    active_profile_name: String,
    profile_names: Vec<String>,
    profile_target: String,
    profile_delete: Option<(String, String)>,
    nexus_api_key: String,
    game: String,
    mods: String,
    patcher: String,
    patcher_engine: PatcherEngine,
    scratch: String,
    fomod_choices: String,
    fomod_archive: String,
    fomod_editor: crate::fomod_ui::Editor,
    fomod_queue: std::collections::VecDeque<FomodPending>,
    fomod_owner: Option<(u64, String)>,
    fomod_active: Option<FomodPending>,
    fomod_failure: Option<(FomodPending, String)>,
    fomod_import: Option<modsync::fomod::ChoicesFile>,
    peer_view: PeerView,
    expanded_budget: String,
    allow_processes: bool,
    guide: Option<Guide>,
    page: usize,
    wizard: Option<Wizard>,
    utility: bool,
    input_stamp: String,
    guide_generation: u64,
    validation_input_stamp: Option<String>,
    validation_page: validation_page::State,
    installing_page: installing_page::State,
    /// Phase receipts copied at each accepted install terminal.
    completion: completion_pages::State,
    stop_confirmation: bool,
    selected: usize,
    spoiler_free: bool,
    search: String,
    tier_filter: String,
    category_filter: String,
    all_expanded: bool,
    category_selection: Option<std::collections::BTreeMap<String, bool>>,
    logs: Vec<String>,
    receiver: Option<Receiver<Message>>,
    protocol_load: protocol_load::State,
    #[cfg(target_os = "macos")]
    protocol_inbox: protocol_inbox::State,
    protocol_receiver: Option<Receiver<String>>,
    #[cfg(target_os = "macos")]
    native_protocol_receiver: Option<Receiver<String>>,
    _instance_guard: Option<modsync::single_instance::Guard>,
    cancelled: Arc<AtomicBool>,
    job: Job,
    validated: bool,
    conflicts: Option<modsync::conflicts::Report>,
    validation_report: Option<modsync::validation::Report>,
    confirm_install: bool,
    completed_base: Option<Vec<String>>,
    pending_base: Vec<Component>,
    install_context: Option<(String, String, String)>,
    finished: bool,
    phase_failed: bool,
    /// Roots of the running restore job, compared against a failed install's roots.
    restore_roots: Option<modsync::paths::Roots>,
    download_url: String,
    download_name: String,
    download_hash: String,
    pending_protocol: std::collections::VecDeque<String>,
    pending_nxm: std::collections::VecDeque<String>,
    telemetry: modsync::telemetry::Config,
    /// Shared acquisition runs (Fetch and nxm) and their presentation.
    downloads: downloads_panel::Panel,
    /// The direct URL download's run and the acquisition context it was started for.
    direct_download: Option<(u64, String, Vec<String>)>,
    /// Settings directory given at launch, used as the Peer cache like the CLI's.
    explicit_settings: Option<PathBuf>,
    probe: Option<ProbeContext>,
    repaint: Option<egui::Context>,
}

impl App {
    fn handle_message(&mut self, message: Message) {
        match message {
            Message::ShellTerminal(result) => self.shell_terminal(*result),
            Message::DeploymentTerminal(result) => self.deployment_terminal(*result),
            Message::MergeTerminal(result) => self.merge_terminal(*result),
            Message::EditorTerminal(result) => self.editor_terminal(*result),
            Message::EditorProgress(run, context, text) => self.easy_progress(run, &context, text),
            Message::CheckpointProgress(run, text) => self.checkpoints.progress(run, text),
            Message::CheckpointTerminal(result) => self.checkpoint_terminal(*result),
            Message::Downloaded(run, path, options) => {
                // Offer the editor only for the guide and folders this download started
                // with; a changed context never receives another guide's follow-up.
                self.refresh_input_stamp();
                let bound = self
                    .direct_download
                    .as_ref()
                    .is_some_and(|(bound, context, _)| {
                        *bound == run && *context == self.acquisition_context()
                    });
                if !bound {
                    self.logs.push(format!(
                        "Downloaded {}, but the guide or folders changed; no FOMOD prompt was offered.",
                        path.display()
                    ));
                } else if self.guide.is_some() && self.should_offer_fomod_editor(&path) {
                    let consumers = self
                        .direct_download
                        .as_ref()
                        .map(|(_, _, guids)| guids.clone())
                        .unwrap_or_default();
                    self.enqueue_bound_fomod(run, path, options, true, consumers);
                }
            }
            Message::Acquisition(run, update) => self.acquisition_update(run, update),
            Message::ProtocolTerminal(finished) => self.protocol_terminal(*finished),
            Message::Conflicts(report) => {
                self.refresh_input_stamp();
                if self.tools.conflict_context.as_ref() == Some(&self.input_stamp)
                    && !self.cancelled.load(Ordering::Acquire)
                {
                    self.conflicts = Some(report);
                } else {
                    self.tools.conflict_error =
                        Some("Inputs changed or analysis was cancelled; result discarded.".into());
                }
            }
            Message::ValidationProgress(run, progress) => {
                self.refresh_input_stamp();
                if self.validation_page.accepts(run) && !self.cancelled.load(Ordering::Relaxed) {
                    self.validation_page.progress = Some(progress);
                }
            }
            Message::ValidationTerminal(run, report) => {
                self.refresh_input_stamp();
                if self.validation_page.accepts(run) && !self.cancelled.load(Ordering::Relaxed) {
                    self.validation_page.complete(&report);
                    self.validation_report = Some(report);
                }
            }
            Message::ValidationDone(run, result) => {
                if self.validation_page.run == run {
                    self.handle_message(Message::Done(result));
                }
            }
            Message::InstallProgress(run, progress) => {
                // The stamp is refreshed every frame; the terminal refreshes it again.
                if self.installing_page.accepts(run, &self.input_stamp) {
                    self.installing_page.progress(progress);
                }
            }
            Message::InstallPrepared(run, guide) => {
                self.refresh_input_stamp();
                if self.installing_page.accepts(run, &self.input_stamp) {
                    self.installing_page.prepared(&guide);
                    if self.completed_base.is_none() {
                        self.pending_base = guide
                            .ordered()
                            .unwrap_or_default()
                            .into_iter()
                            .cloned()
                            .collect();
                    }
                }
            }
            Message::InstallTerminal(run, result, pending, terminal) => {
                // `pending` was read from the roots captured for this run, never
                // from the editable paths. The engine result wins over a late Stop.
                self.receiver = None;
                self.stop_confirmation = false;
                self.refresh_input_stamp();
                let accepted = self.installing_page.accepts(run, &self.input_stamp);
                let engine_succeeded = result.is_ok();
                // Read before installed_phase records the base completion.
                let base_phase = self.completed_base.is_none();
                let final_result = if accepted && engine_succeeded {
                    result.and_then(|()| self.installed_phase())
                } else {
                    result
                };
                self.installing_page.finish(
                    run,
                    &self.input_stamp,
                    engine_succeeded,
                    &final_result,
                    pending,
                );
                // Only a success accepted for this run and stamp fills a receipt.
                // Copy time and log now: the next phase replaces the page state.
                if accepted
                    && final_result.is_ok()
                    && let Some(terminal) = terminal
                {
                    self.completion.record(
                        base_phase,
                        terminal,
                        self.installing_page.elapsed(),
                        self.installing_page
                            .log_lines()
                            .map(str::to_owned)
                            .collect(),
                    );
                }
                if accepted && final_result.is_ok() {
                    self.sharing.resume_after_mutation(self.guide.as_ref());
                }
                if self.checkpoints.resumed_run == Some(run) {
                    if accepted && final_result.is_ok() {
                        self.finished = true;
                    }
                    self.checkpoints.resumed_run = None;
                }
                self.pending_base.clear();
                self.install_context = None;
                self.phase_failed = !engine_succeeded && pending;
                self.logs.push(match &final_result {
                    Ok(()) if accepted => "Installation complete.".into(),
                    Ok(()) => "Installation finished for the captured folders, but the guide or folders changed. Use Restart wizard to start again with the current guide.".into(),
                    Err(error) if engine_succeeded => {
                        format!("Installed, but not applied here: {error:#}")
                    }
                    Err(error) => format!("Stopped: {error:#}"),
                });
                self.emit_telemetry(
                    "operation.completed",
                    serde_json::Map::from_iter([
                        ("operation".into(), serde_json::json!("installation")),
                        ("success".into(), serde_json::json!(engine_succeeded)),
                    ]),
                );
                self.start_pending_nxm_if_ready();
            }
            Message::Log(line) => self.logs.push(line),
            Message::Updates(result) => self.update_result(*result),
            Message::UpdateSummary(context, state) => {
                self.receiver = None;
                self.refresh_input_stamp();
                if self.input_stamp == context {
                    self.tools.update_state = Some(state);
                    self.tools.update_note =
                        "Saved result is presentation only; no guide metadata was applied.".into();
                } else {
                    self.tools.update_note = "Inputs changed while reading the saved result. Reload for the current guide.".into();
                }
            }
            Message::Done(result) => {
                if self.job == Job::Conflicts
                    && let Err(error) = &result
                {
                    self.tools.conflict_error = Some(format!("{error:#}"));
                }
                if self.job == Job::Update
                    && let Err(error) = &result
                {
                    self.tools.update_note = format!("Update check stopped: {error:#}");
                }
                self.receiver = None;
                if self.job == Job::Download {
                    self.direct_download = None;
                }
                let succeeded = result.is_ok();
                if succeeded && self.job == Job::Restore {
                    self.sharing.resume_after_mutation(self.guide.as_ref());
                }
                // Install terminals arrive only as InstallTerminal. A restore
                // clears a failed install's pending state only when it restored
                // that install's own captured roots.
                if self.job == Job::Restore {
                    let roots = self.restore_roots.take();
                    if result.is_ok() {
                        let own = self.installing_page.restore_target().is_some();
                        if !own
                            || roots
                                .is_some_and(|roots| self.installing_page.record_restore(&roots))
                        {
                            self.phase_failed = false;
                        }
                    }
                }
                self.refresh_input_stamp();
                if self.job == Job::Validate {
                    let stopped = if self.cancelled.load(Ordering::Relaxed) {
                        Some("Validation cancelled. Run validation again.".to_owned())
                    } else if self.validation_input_stamp.as_ref() != Some(&self.input_stamp) {
                        Some("Inputs changed during validation; run validation again.".to_owned())
                    } else if self.validation_report.is_none() {
                        Some(match &result {
                            Err(error) => format!("Validation could not complete: {error:#}"),
                            Ok(()) => {
                                "Validation ended without a report. Run validation again.".into()
                            }
                        })
                    } else {
                        None
                    };
                    if let Some(message) = stopped {
                        validation_page::clear_report(
                            &mut self.validation_report,
                            &mut self.validation_page,
                        );
                        self.validation_page.fail(&message);
                        self.logs.push(message);
                    }
                }
                self.validated = result.is_ok()
                    && !self.cancelled.load(Ordering::Relaxed)
                    && self.validation_report.as_ref().is_some_and(|r| r.success)
                    && self.job == Job::Validate
                    && self.validation_input_stamp.as_ref() == Some(&self.input_stamp);
                self.logs.push(match result { Ok(()) => match self.job { Job::Editor => unreachable!("editor has dedicated terminal"), Job::UpdateSummary => unreachable!("summary has dedicated terminal"), Job::Checkpoint => unreachable!("checkpoint terminals use their own path"), Job::Conflicts => "Conflict analysis complete. Review the report and its limitations.", Job::Validate if !self.validated => "Validation result discarded. Run validation again.", Job::Validate => "Validation passed. External program effects, if any, remain unverified.", Job::Install => unreachable!("install terminals arrive as InstallTerminal"), Job::NxmDownload | Job::Fetch => unreachable!("acquisition terminals arrive as Acquisition"), Job::Restore => "Snapshot restored and file hashes verified.", Job::Download => "Archive downloaded and integrity checked.", Job::ProtocolOpen => "ModSync build link loaded.", Job::Update => "Nexus update check complete." }.into(), Err(e) => format!("Stopped: {}", self.downloads.redact(&format!("{e:#}"))) });
                let job = match self.job {
                    Job::Checkpoint => "checkpoint",
                    Job::UpdateSummary => "update_summary",
                    Job::Editor => "editor",
                    Job::Conflicts => "conflicts",
                    Job::Validate => "validation",
                    Job::Install => "installation",
                    Job::Restore => "restore",
                    Job::Download => "download",
                    Job::NxmDownload | Job::Fetch => {
                        unreachable!("acquisition terminals arrive as Acquisition")
                    }
                    Job::ProtocolOpen => "protocol_open",
                    Job::Update => "update",
                };
                self.emit_telemetry(
                    "operation.completed",
                    serde_json::Map::from_iter([
                        ("operation".into(), serde_json::json!(job)),
                        ("success".into(), serde_json::json!(succeeded)),
                    ]),
                );
                self.start_pending_nxm_if_ready();
            }
        }
    }

    fn invalidate_validation(&mut self) {
        self.validated = false;
        validation_page::clear_report(&mut self.validation_report, &mut self.validation_page);
        self.conflicts = None;
        self.confirm_install = false;
    }

    fn refresh_input_stamp(&mut self) {
        // This includes nested options, profile/editor changes and every engine setting.
        // Filter/search/theme changes do not change installation inputs.
        let stamp = format!(
            "{:?}",
            (
                serde_json::to_string(&self.guide).unwrap_or_default(),
                &self.game,
                &self.mods,
                &self.patcher,
                self.patcher_engine,
                &self.scratch,
                &self.expanded_budget,
                &self.fomod_choices,
                Self::choices_file_stamp(Path::new(&self.fomod_choices)),
                self.allow_processes,
                (
                    &self.checkpoints.config,
                    &self.profile_directory,
                    &self.active_profile_name,
                    self.resource_watch.epoch,
                    self.desktop.settings.case_insensitive_pathing
                ),
            )
        );
        if self.input_stamp != stamp {
            self.invalidate_validation();
            self.input_stamp = stamp;
        }
    }

    fn choices_file_stamp(path: &Path) -> String {
        // Never open the payload from the repaint loop: paths can name FIFOs or
        // arbitrarily large files. On Unix ctime also detects edits that preserve
        // file length and mtime. Validation itself parses choices at dispatch.
        match std::fs::metadata(path) {
            Ok(metadata) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    format!(
                        "{:?}/{}/{:?}/{}/{}/{}/{}",
                        metadata.file_type(),
                        metadata.len(),
                        metadata.modified(),
                        metadata.dev(),
                        metadata.ino(),
                        metadata.ctime(),
                        metadata.ctime_nsec()
                    )
                }
                #[cfg(not(unix))]
                {
                    format!(
                        "{:?}/{}/{:?}/{:?}",
                        metadata.file_type(),
                        metadata.len(),
                        metadata.modified(),
                        metadata.created()
                    )
                }
            }
            Err(error) => format!("{:?}", error.kind()),
        }
    }

    fn fomod_pending(&self) -> bool {
        self.fomod_editor.is_open() || !self.fomod_queue.is_empty() || self.fomod_failure.is_some()
    }

    fn enqueue_fomod(
        &mut self,
        run: u64,
        path: PathBuf,
        options: modsync::archive::Options,
        dismiss_non_fomod: bool,
    ) {
        let Some(component) = self
            .guide
            .as_ref()
            .and_then(|guide| guide.components.get(self.selected))
        else {
            self.logs
                .push("Select a component before configuring its FOMOD archive.".into());
            return;
        };
        if !component.is_selected
            || self.completed_base.as_ref().is_some_and(|ids| {
                ids.iter()
                    .any(|id| id.eq_ignore_ascii_case(&component.guid))
            })
        {
            self.logs
                .push("FOMOD configuration requires a selected, unfinished component.".into());
            return;
        }
        let consumers = vec![component.guid.clone()];
        self.enqueue_bound_fomod(run, path, options, dismiss_non_fomod, consumers);
    }

    fn enqueue_bound_fomod(
        &mut self,
        run: u64,
        path: PathBuf,
        options: modsync::archive::Options,
        dismiss_non_fomod: bool,
        consumer_guids: Vec<String>,
    ) {
        if consumer_guids.is_empty() {
            return;
        }
        self.refresh_input_stamp();
        let context = self.acquisition_context();
        let run = if run == 0 {
            self.downloads.direct_run()
        } else {
            run
        };
        if self
            .fomod_queue
            .iter()
            .any(|item| item.run == run && item.path == path && item.context == context)
        {
            return;
        }
        self.fomod_queue.push_back(FomodPending {
            run,
            context,
            path,
            options,
            dismiss_non_fomod,
            consumer_guids,
        });
        self.invalidate_validation();
    }

    fn advance_fomod_queue(&mut self) {
        self.refresh_input_stamp();
        let context = self.acquisition_context();
        if self.fomod_failure.as_ref().is_some_and(|(item, _)| {
            item.context != context || !self.downloads.latest_run_is(item.run)
        }) {
            self.fomod_failure = None;
            self.logs
                .push("Discarded stale FOMOD configuration error after context changed.".into());
        }
        let before = self.fomod_queue.len();
        self.fomod_queue
            .retain(|item| item.context == context && self.downloads.latest_run_is(item.run));
        if before != self.fomod_queue.len() {
            self.logs
                .push("Discarded FOMOD prompts because the guide or folders changed.".into());
        }
        if self.receiver.is_some() || self.fomod_editor.is_open() || self.fomod_failure.is_some() {
            return;
        }
        if let Some(item) = self.fomod_queue.pop_front() {
            self.fomod_active = Some(item.clone());
            let saved = if item.consumer_guids.len() == 1 {
                self.saved_fomod_choices(&item.path)
                    .map(|value| value.into_iter().collect())
            } else {
                self.saved_fomod_sessions(&item.path)
            };
            match saved {
                Ok(mut saved) => {
                    self.fomod_archive = item.path.to_string_lossy().into_owned();
                    self.fomod_active = Some(item.clone());
                    self.fomod_owner = Some((item.run, item.context));
                    let sidecar = (!self.fomod_choices.trim().is_empty())
                        .then(|| PathBuf::from(&self.fomod_choices));
                    if saved.len() <= 1 && sidecar.is_none() {
                        self.fomod_editor.launch(
                            item.path,
                            item.options,
                            saved.pop(),
                            item.dismiss_non_fomod,
                        );
                    } else {
                        self.fomod_editor.launch_bound(
                            item.path,
                            item.options,
                            saved,
                            item.dismiss_non_fomod,
                            sidecar,
                        );
                    }
                }
                Err(error) => {
                    let error = self.downloads.redact(&format!("FOMOD choices: {error:#}"));
                    self.logs.push(error.clone());
                    self.fomod_failure = Some((item, error));
                    self.fomod_active = None;
                }
            }
        }
    }

    fn show_fomod(&mut self, ctx: &egui::Context) {
        if self.sharing.closing {
            return;
        }
        if self.fomod_pending() && (self.sharing.busy() || !self.sharing.mutation()) {
            return;
        }
        self.advance_fomod_queue();
        let was_open = self.fomod_editor.is_open();
        let outcome = self.fomod_editor.show(ctx);
        self.refresh_input_stamp();
        let context = self.acquisition_context();
        let owner = self.fomod_owner.clone();
        let accepted = owner
            .as_ref()
            .is_some_and(|(run, bound)| *bound == context && self.downloads.latest_run_is(*run));
        if let Some(outcome) = outcome {
            if accepted {
                match outcome {
                    crate::fomod_ui::Outcome::Choices(choices) => {
                        self.fomod_import = self.fomod_editor.take_imported_choices();
                        if let Err(error) = self.apply_fomod_choices(&choices) {
                            let error = self.downloads.redact(&format!("FOMOD choices: {error:#}"));
                            self.logs.push(error.clone());
                            if let Some(item) = self.fomod_active.clone() {
                                self.fomod_failure = Some((item, error));
                            }
                        } else {
                            self.downloads.unsaved_records = true;
                        }
                    }
                    crate::fomod_ui::Outcome::Dismissed(filename) => {
                        if let (Some(guide), Some(item)) = (&mut self.guide, &self.fomod_active) {
                            match modsync::fomod::apply_bound_archive(
                                guide,
                                &item.consumer_guids,
                                &filename,
                                None,
                                modsync::fomod::PromptStatus::Dismissed,
                                self.completed_base.as_deref().unwrap_or_default(),
                            ) {
                                Ok(()) => self.downloads.unsaved_records = true,
                                Err(error) => {
                                    let error = format!("FOMOD dismissal: {error:#}");
                                    self.logs.push(error.clone());
                                    self.fomod_failure = Some((item.clone(), error));
                                }
                            }
                        }
                        self.invalidate_validation();
                        self.logs
                            .push(format!("FOMOD configuration dismissed for {filename}."));
                    }
                }
                // Only our accepted choice edit may carry the remaining batch into
                // the new guide revision. Unrelated context changes discard it.
                self.refresh_input_stamp();
                let updated = self.acquisition_context();
                for item in &mut self.fomod_queue {
                    if item.context == context
                        && owner.as_ref().is_some_and(|(run, _)| *run == item.run)
                    {
                        item.context = updated.clone();
                    }
                }
            } else {
                self.fomod_queue.clear();
                self.logs
                    .push("Ignored FOMOD outcome because its guide or folders changed.".into());
            }
        }
        // Inspection/error Cancel may return None: closure still releases ownership.
        if was_open && !self.fomod_editor.is_open() {
            self.fomod_import = None;
            self.fomod_owner = None;
            self.fomod_active = None;
            ctx.request_repaint();
        }
        if let Some((item, error)) = self.fomod_failure.clone() {
            egui::Modal::new(egui::Id::new("fomod_queue_error")).show(ctx, |ui| {
                ui.set_width((ctx.screen_rect().width() - 48.0).clamp(180.0, 540.0));
                ui.heading("FOMOD choices need attention");
                egui::ScrollArea::vertical().max_height((ctx.screen_rect().height() - 150.0).max(60.0)).show(ui, |ui| {
                    ui.colored_label(egui::Color32::RED, error);
                    ui.label("Validation and installation are blocked. Correct the saved choices and retry, or explicitly dismiss this prompt.");
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Retry FOMOD choices").clicked() {
                        self.fomod_failure = None;
                        self.fomod_queue.push_front(item.clone());
                        ctx.request_repaint();
                    }
                    if ui.button("Dismiss FOMOD prompt").clicked() {
                        self.fomod_failure = None;
                        self.logs.push("FOMOD configuration error explicitly dismissed; validation will diagnose missing choices.".into());
                        ctx.request_repaint();
                    }
                });
            });
        }
        self.start_pending_nxm_if_ready();
    }

    fn selected_widescreen(&self) -> usize {
        self.guide.as_ref().map_or(0, |g| {
            g.components
                .iter()
                .filter(|c| {
                    c.is_selected
                        && c.metadata
                            .get("WidescreenOnly")
                            .and_then(serde_json::Value::as_bool)
                            == Some(true)
                })
                .count()
        })
    }

    fn selected_base(&self) -> usize {
        self.guide.as_ref().map_or(0, |g| {
            g.components
                .iter()
                .filter(|c| {
                    c.is_selected
                        && c.metadata
                            .get("WidescreenOnly")
                            .and_then(serde_json::Value::as_bool)
                            != Some(true)
                })
                .count()
        })
    }

    fn wizard_gate(&self) -> Gate {
        let Some(wizard) = &self.wizard else {
            return Gate::default();
        };
        // Downloads can continue while reading pages; changing inputs or beginning
        // another engine operation remains serialized with the current worker.
        let reading = self.receiver.is_none()
            || matches!(self.job, Job::Download | Job::NxmDownload | Job::Fetch);
        let passed = match wizard.current() {
            Page::ModDirectory => !self.mods.trim().is_empty() && Path::new(&self.mods).is_dir(),
            Page::GameDirectory => {
                !self.game.trim().is_empty()
                    && modsync::game::detect(&modsync::game::resolve_install_root(Path::new(
                        &self.game,
                    ))) != modsync::game::Detected::Unknown
            }
            Page::ModSelection => self.selected_base() > 0,
            Page::Validate => {
                !self.fomod_pending()
                    && self.validated
                    && self.validation_report.as_ref().is_some_and(|r| r.success)
                    && self.validation_input_stamp.as_ref() == Some(&self.input_stamp)
            }
            Page::InstallStart => {
                !self.fomod_pending()
                    && self.validated
                    && self.validation_report.as_ref().is_some_and(|r| r.success)
                    && self.validation_input_stamp.as_ref() == Some(&self.input_stamp)
                    && self.selected_base() > 0
                    && Path::new(&self.mods).is_dir()
                    && self.receiver.is_none()
            }
            Page::Installing => self.completed_base.is_some() && !self.phase_failed,
            Page::WidescreenModSelection => {
                !self.phase_failed
                    && !self.fomod_pending()
                    && self.receiver.is_none()
                    && (self.selected_widescreen() == 0
                        || (self.validated
                            && self.validation_report.as_ref().is_some_and(|r| r.success)
                            && self.validation_input_stamp.as_ref() == Some(&self.input_stamp)))
            }
            Page::WidescreenInstalling => self.finished && !self.phase_failed,
            Page::Finished => false,
            _ => true,
        };
        Gate {
            can_navigate_forward: reading,
            validation_passed: passed,
        }
    }

    fn advance_wizard(&mut self, ctx: &egui::Context) {
        self.refresh_input_stamp();
        let gate = self.wizard_gate();
        if !self.wizard.as_mut().is_some_and(|w| w.next(gate)) {
            return;
        }
        match self.wizard.as_ref().unwrap().current() {
            Page::Installing => self.start(Job::Install, ctx),
            Page::WidescreenModSelection => self.invalidate_validation(),
            Page::WidescreenInstalling if self.selected_widescreen() == 0 => {
                // An empty phase has no operation to dispatch and no installed mods.
                self.installing_page.skip();
                self.completion.skip_remaining();
                self.finished = true;
                self.logs
                    .push("Widescreen phase skipped: no widescreen mods selected.".into());
            }
            Page::WidescreenInstalling => self.start(Job::Install, ctx),
            _ => {}
        }
    }

    /// A direct Install control enters the same review and confirmation path as
    /// the wizard footer. Recovery and remaining-phase installs keep their own
    /// page-specific routes.
    fn install_review_ready(&self) -> bool {
        let route_available = self.wizard.as_ref().is_none_or(|wizard| {
            wizard
                .pages()
                .iter()
                .position(|page| *page == Page::InstallStart)
                .is_some_and(|review| wizard.current_index() <= review)
        });
        self.receiver.is_none()
            && !self.fomod_pending()
            && self.completed_base.is_none()
            && !self.phase_failed
            && !self.installing_page.active()
            && !self.installing_page.failed()
            && !self.installing_page.not_applied()
            && self.guide.is_some()
            && self.selected_base() > 0
            && self.validated
            && self
                .validation_report
                .as_ref()
                .is_some_and(|report| report.success)
            && self.validation_input_stamp.as_ref() == Some(&self.input_stamp)
            && route_available
    }

    fn open_install_review(&mut self, ctx: &egui::Context) {
        self.refresh_input_stamp();
        if !self.install_review_ready() {
            self.logs.push(
                "Installation review blocked: finish any recovery and run successful validation for the current guide and folders."
                    .into(),
            );
            return;
        }
        if self.wizard.is_none() {
            self.wizard = self.guide.as_ref().map(Wizard::new);
        }
        // Fresh validation already checked the current guide and folders. Advance
        // earlier reading pages to Validate without launching the engine, then use
        // the ordinary validated transition to the InstallStart review page.
        while self.wizard.as_ref().is_some_and(|wizard| {
            wizard.current() != Page::Validate && wizard.current() != Page::InstallStart
        }) {
            if !self.wizard.as_mut().is_some_and(|wizard| {
                wizard.next(Gate {
                    can_navigate_forward: true,
                    validation_passed: true,
                })
            }) {
                self.logs
                    .push("Installation review page is unavailable.".into());
                return;
            }
        }
        self.utility = false;
        if self
            .wizard
            .as_ref()
            .is_some_and(|wizard| wizard.current() == Page::Validate)
        {
            self.advance_wizard(ctx);
        }
    }

    /// A failed phase cannot reuse the validation cleared when it started.
    /// Once its own snapshot is restored, return to the matching phase's
    /// validation page without clearing the completed base receipt.
    fn reopen_failed_phase_validation(&mut self) {
        if self.receiver.is_some()
            || self.phase_failed
            || !self.installing_page.failed()
            || self.installing_page.pending()
            || !self.installing_page.same_folders(&self.mods, &self.game)
        {
            return;
        }
        let Some(guide) = self.guide.as_ref() else {
            return;
        };
        let target = if self.completed_base.is_some() {
            Page::WidescreenModSelection
        } else {
            Page::Validate
        };
        let mut wizard = Wizard::new(guide);
        while wizard.current() != target {
            if !wizard.next(Gate {
                can_navigate_forward: true,
                validation_passed: true,
            }) {
                self.logs
                    .push("Retry validation page is unavailable.".into());
                return;
            }
        }
        self.installing_page.reset();
        self.invalidate_validation();
        self.wizard = Some(wizard);
        self.utility = false;
        self.logs
            .push("Run validation again before reviewing and retrying this phase.".into());
    }

    fn downloads_visible(&self) -> bool {
        self.downloads.has_history()
            || self
                .wizard
                .as_ref()
                .is_some_and(|w| w.pages()[..=w.current_index()].contains(&Page::DownloadsExplain))
    }

    fn download_status(&self) -> String {
        if self.receiver.is_some() && self.job == Job::Download {
            "Direct archive download in progress — see activity for results".into()
        } else {
            self.downloads.status()
        }
    }

    /// What an acquisition run is bound to: the input stamp (guide revision, workspace
    /// and game folders among the installation inputs), the normalized workspace and the
    /// completed-base context. Callers refresh the input stamp first; this never
    /// serializes the guide again.
    fn acquisition_context(&self) -> String {
        format!(
            "{:?}",
            (
                &self.input_stamp,
                self.guide_generation,
                &self.loaded_input,
                std::fs::canonicalize(self.mods.trim()).ok(),
                std::fs::canonicalize(self.game.trim()).ok(),
                &self.completed_base,
            )
        )
    }

    /// Why Fetch cannot start now, if it cannot.
    fn fetch_blocker(&self) -> Option<&'static str> {
        if self.sharing.closing || self.sharing_continuation.is_some() {
            return Some("Wait for sharing cleanup.");
        }
        if self.receiver.is_some() || self.desktop.busy() {
            Some("Wait for the current operation or desktop settings to finish.")
        } else if self.peer_view.receiver.is_some() {
            Some("Wait for peer settings to finish loading or saving.")
        } else if self.peer_view.settings.is_none()
            || self.peer_view.directory.as_deref() != Some(self.profile_directory.as_str())
        {
            Some(
                "Load peer settings successfully before fetching; use Refresh peer settings after correcting an error.",
            )
        } else if self.fomod_pending() {
            Some("Finish or dismiss the pending FOMOD choices first.")
        } else if self.guide.is_none() {
            Some("Load a guide first.")
        } else if self.mods.trim().is_empty() || !Path::new(self.mods.trim()).is_dir() {
            Some("Choose an existing mod workspace first.")
        } else {
            None
        }
    }

    fn archive_options(&self) -> Result<modsync::archive::Options> {
        Ok(modsync::archive::Options {
            scratch_dir: (!self.scratch.is_empty()).then(|| PathBuf::from(&self.scratch)),
            max_expanded_bytes: if self.expanded_budget.trim().is_empty() {
                None
            } else {
                Some(self.expanded_budget.trim().parse().map_err(|_| {
                    anyhow::anyhow!("Expanded-byte budget must be a nonnegative integer.")
                })?)
            },
        })
    }

    /// The Nexus key for a run: a probe launch uses only its explicit key; otherwise the
    /// key loaded from (or typed for) the selected settings directory.
    fn acquisition_key(&self) -> Option<String> {
        match &self.probe {
            Some(probe) => probe.nexus_api_key.clone(),
            None => Some(self.nexus_api_key.clone()).filter(|key| !key.trim().is_empty()),
        }
    }

    fn acquisition_secrets(&self) -> Vec<String> {
        [self.nexus_api_key.clone()]
            .into_iter()
            .chain(self.probe.as_ref().and_then(|p| p.nexus_api_key.clone()))
            .collect()
    }

    /// Capture guide, normalized workspace and names once, for a new run.
    fn acquisition_binding(&self) -> Result<downloads_panel::Binding> {
        let guide = self
            .guide
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Load a guide first"))?;
        let source_dir = std::fs::canonicalize(self.mods.trim())
            .ok()
            .filter(|path| path.is_dir())
            .ok_or_else(|| anyhow::anyhow!("Choose an existing mod workspace first"))?;
        let names = guide
            .components
            .iter()
            .map(|c| {
                (
                    c.guid.clone(),
                    modsync::selection::display_name(c, self.spoiler_free),
                )
            })
            .collect();
        Ok(downloads_panel::Binding {
            context: self.acquisition_context(),
            guide,
            source_dir,
            names,
        })
    }

    /// Starts the selected-guide Fetch: shared plan/run/record, Scope::All candidates,
    /// four workers with one retry and a per-run Peer adapter, all off the UI thread.
    fn start_fetch(&mut self) {
        self.start_fetch_scope(None);
    }

    fn start_fetch_scope(&mut self, component_guid: Option<String>) {
        if let Some(reason) = self.fetch_blocker() {
            self.logs.push(format!("Fetch Downloads: {reason}"));
            return;
        }
        self.refresh_input_stamp();
        let prepared = self
            .archive_options()
            .and_then(|archive| Ok((archive, self.acquisition_binding()?)));
        let (archive, binding) = match prepared {
            Ok(value) => value,
            Err(error) => {
                self.logs.push(format!("Fetch Downloads: {error:#}"));
                return;
            }
        };
        let guide = binding.guide.clone();
        let completed = self.completed_base.clone().unwrap_or_default();
        let source_dir = binding.source_dir.clone();
        let key = self.acquisition_key();
        let (api_base, loopback_ca) = self.probe.as_ref().map_or((None, None), |probe| {
            (probe.nexus_api_base.clone(), probe.loopback_ca.clone())
        });
        let settings_dir = PathBuf::from(&self.profile_directory);
        let resolution = if self.desktop.settings.filter_downloads_by_resolution {
            self.repaint.as_ref().and_then(|ctx| {
                ctx.input(|input| {
                    let viewport = input.viewport();
                    let size = viewport.monitor_size?;
                    let scale = viewport.native_pixels_per_point?;
                    let width = size.x * scale;
                    let height = size.y * scale;
                    (scale.is_finite()
                        && scale > 0.0
                        && width.is_finite()
                        && height.is_finite()
                        && width >= 1.0
                        && height >= 1.0
                        && width < u32::MAX as f32
                        && height < u32::MAX as f32)
                        .then_some((width.round() as u32, height.round() as u32))
                })
            })
        } else {
            None
        };
        if self.desktop.settings.filter_downloads_by_resolution && resolution.is_none() {
            self.logs.push(
                "Monitor resolution unavailable; downloads will not be filtered by resolution."
                    .into(),
            );
        }
        // Like the CLI's explicit --settings-dir: the Peer cache follows an explicit
        // settings directory, and the default share root is used otherwise.
        let cache_dir = self.explicit_settings.clone();
        self.invalidate_validation();
        let cancelled = Arc::new(AtomicBool::new(false));
        self.cancelled = cancelled.clone();
        let secrets = self.acquisition_secrets();
        let run = self.downloads.begin(
            downloads_panel::Kind::Fetch,
            binding,
            cancelled.clone(),
            &secrets,
        );
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.job = Job::Fetch;
        self.logs
            .push("Fetch Downloads: planning selected archives…".into());
        let repaint = self.repaint.clone();
        std::thread::spawn(move || {
            use modsync::download_orchestrator as orchestrator;
            let send = |update| {
                let _ = tx.send(Message::Acquisition(run, update));
                if let Some(ctx) = &repaint {
                    ctx.request_repaint();
                }
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || -> Result<Box<downloads_panel::Finished>> {
                    let stored = modsync::profiles::SettingsFile::resolve(Some(&settings_dir))?
                        .load()
                        .map_err(|e| anyhow::anyhow!("Peer settings: {e:#}"))?;
                    let peer_settings = modsync::p2p::PeerSettings::from_settings(&stored);
                    let peer_mode = match peer_settings.effective_mode() {
                        modsync::p2p::PeerMode::Off => orchestrator::PeerMode::Off,
                        modsync::p2p::PeerMode::Race => orchestrator::PeerMode::Race,
                        modsync::p2p::PeerMode::Fallback => orchestrator::PeerMode::Fallback,
                    };
                    let mut candidates = modsync::selection::download_candidates(
                        &guide.components,
                        modsync::selection::Scope::All,
                    );
                    if let Some(guid) = &component_guid {
                        candidates.retain(|candidate| candidate.guid == *guid);
                        anyhow::ensure!(
                            !candidates.is_empty(),
                            "Select this component before fetching its registered archives"
                        );
                    }
                    let plan_options = orchestrator::PlanOptions {
                        source_dir,
                        resolution,
                        peer_mode,
                        user_peers: Vec::new(),
                        nexus_key_present: key.is_some(),
                        archive: archive.clone(),
                    };
                    let plan = orchestrator::plan_with_cancel(
                        &guide,
                        &candidates,
                        &plan_options,
                        &cancelled,
                    )?;
                    // Nexus page discovery runs before the rows exist, so every discovered
                    // archive gets its own row and the run reports into the same indices.
                    let plan = orchestrator::discover(
                        &guide,
                        &plan,
                        &plan_options,
                        &orchestrator::RunSettings {
                            nexus_api_key: key.clone(),
                            nexus_api_base: api_base.clone(),
                            ..Default::default()
                        },
                        &cancelled,
                    )?;
                    send(downloads_panel::Update::Prepared {
                        plan: Box::new(plan.clone()),
                        selected_components: candidates.len(),
                        secrets: peer_settings.proxies.clone(),
                    });
                    // Stop pressed during planning: transfer nothing. Present archives
                    // remain unrecorded until revalidated under the acquisition lock.
                    if cancelled.load(Ordering::SeqCst) {
                        return Ok(Box::new(downloads_panel::Finished {
                            report: orchestrator::cancelled_report(&plan),
                            metadata: Default::default(),
                            fomod: None,
                        }));
                    }
                    let peer: Option<Arc<dyn modsync::acquisition::Acquire>> = match peer_mode {
                        orchestrator::PeerMode::Off => None,
                        _ => Some(Arc::new(modsync::peer_acquire::PeerAcquire::new(
                            &guide,
                            peer_settings,
                            archive.clone(),
                            cache_dir,
                        ))),
                    };
                    let settings = orchestrator::RunSettings {
                        workers: 4,
                        retries: 1,
                        archive: archive.clone(),
                        nexus_api_key: key,
                        nexus_api_base: api_base,
                        deadlystream_base: None,
                        mega_api_base: None,
                        peer,
                        paused: None,
                        deadlystream_session: None,
                    };
                    let report = orchestrator::run_with_source_context(
                        &plan,
                        &settings,
                        &guide,
                        &cancelled,
                        |event| send(downloads_panel::Update::Event(Box::new(event))),
                        loopback_ca.as_ref(),
                    )?;
                    let metadata = orchestrator::collect_metadata(
                        &plan,
                        &report,
                        &settings,
                        &cancelled,
                        loopback_ca.as_ref(),
                    );
                    let stopped = report.cancelled || cancelled.load(Ordering::SeqCst);
                    let fomod = (!stopped).then(|| {
                        orchestrator::bound_fomod_followups(
                            &guide, &report, &archive, &cancelled, &completed,
                        )
                    });
                    Ok(Box::new(downloads_panel::Finished {
                        report,
                        metadata,
                        fomod,
                    }))
                },
            ))
            .unwrap_or_else(|_| Err(anyhow::anyhow!("The download worker stopped unexpectedly.")));
            send(downloads_panel::Update::Terminal(result));
        });
    }

    fn acquisition_update(&mut self, run: u64, update: downloads_panel::Update) {
        // Another run's message changes nothing: no rows, guide, FOMOD or validation.
        if !self.downloads.accepts(run) {
            return;
        }
        self.refresh_input_stamp();
        let current = self.acquisition_context();
        self.downloads.check_context(&current);
        match update {
            downloads_panel::Update::Terminal(result) => self.acquisition_terminal(run, result),
            update => self.downloads.apply(update),
        }
    }

    /// The run terminal owns the report. Only an unchanged context records, on a clone
    /// of the bound guide that replaces the current guide only when `record` succeeds.
    fn acquisition_terminal(&mut self, run: u64, result: Result<Box<downloads_panel::Finished>>) {
        if !self.sharing.ready() && !self.sharing.closing {
            self.sharing.mutation();
            self.receiver = None;
            self.sharing_continuation = Some(SharingContinuation::Terminal(run, result));
            return;
        }
        // A sharing barrier can outlive changes to external guide/choice files
        // or root bindings. Recompute at acceptance, including the close path.
        self.refresh_input_stamp();
        let current = self.acquisition_context();
        let Some(settled) = self.downloads.settle(run, &current) else {
            return;
        };
        self.receiver = None;
        self.stop_confirmation = false;
        let kind = settled.kind;
        let label = match kind {
            downloads_panel::Kind::Fetch => "Fetch Downloads",
            downloads_panel::Kind::Nxm => "Nexus handoff",
        };
        let mut enrollment = None;
        let succeeded = match result {
            Err(error) => {
                let text = format!("{error:#}");
                self.downloads.failed(kind, &text);
                self.logs
                    .push(format!("{label} stopped: {}", self.downloads.redact(&text)));
                false
            }
            Ok(finished) if settled.context_changed => {
                let text = format!(
                    "Files from this run stay in {}, but the guide or folders changed; nothing was recorded in the current guide.",
                    settled.binding.source_dir.display()
                );
                self.downloads
                    .finish(kind, &finished, Err(&text), &settled.binding.names);
                self.logs.push(format!("{label}: {text}"));
                false
            }
            Ok(finished) => {
                let mut guide = settled.binding.guide.clone();
                for warning in &finished.metadata.warnings {
                    self.logs.push(warning.clone());
                }
                let recorded = modsync::download_orchestrator::record(&mut guide, &finished.report)
                    .and_then(|summary| {
                        modsync::download_orchestrator::record_metadata(
                            &mut guide,
                            &finished.metadata,
                        )?;
                        Ok(summary)
                    });
                let succeeded = !finished.report.cancelled
                    && !finished.metadata.cancelled
                    && finished.report.outcomes.iter().all(|o| {
                        o.status == modsync::download_orchestrator::TargetStatus::Completed
                    });
                match &recorded {
                    Ok(summary) => {
                        let changed = serde_json::to_string(&guide).ok()
                            != serde_json::to_string(&settled.binding.guide).ok();
                        if !finished.metadata.cancelled
                            && let Some(plan) = settled.plan.clone()
                            && let Ok(options) = self.archive_options()
                        {
                            enrollment =
                                Some((guide.clone(), plan, finished.report.clone(), options));
                        }
                        self.guide = Some(guide);
                        if changed {
                            self.downloads.unsaved_records = true;
                            self.invalidate_validation();
                        }
                        self.refresh_input_stamp();
                        if !finished.report.cancelled
                            && let Some(followups) = &finished.fomod
                        {
                            // A prompt-eligible consumer whose saved choices need repair
                            // stays queued with the positively detected follow-ups, so
                            // the queue raises the blocking Retry/Dismiss error and keeps
                            // Validate/Install blocked (downloads-gui-plan.md G7/G8;
                            // downloads-gui-step2-review.md P1). Archives that could not
                            // be inspected are not follow-ups and stay log-only.
                            let repair =
                                |error: &&modsync::download_orchestrator::FomodInspectionError| {
                                    error.repair
                                        && self.guide.as_ref().is_some_and(|guide| {
                                            guide.components.iter().any(|c| {
                                                c.guid == error.component_guid
                                                    && error
                                                        .archive
                                                        .file_name()
                                                        .and_then(|n| n.to_str())
                                                        .is_some_and(|name| {
                                                            modsync::fomod::should_prompt(c, name)
                                                        })
                                            })
                                        })
                                };
                            let pending: Vec<(&Path, &str)> =
                                followups
                                    .followups
                                    .iter()
                                    .map(|f| (f.archive.as_path(), f.component_guid.as_str()))
                                    .chain(
                                        followups.inspection_errors.iter().filter(repair).map(
                                            |e| (e.archive.as_path(), e.component_guid.as_str()),
                                        ),
                                    )
                                    .collect();
                            // Shared acquisition order, so one archive's error does not
                            // jump ahead of an earlier archive's editor.
                            let mut archives: Vec<&Path> = finished
                                .report
                                .outcomes
                                .iter()
                                .filter_map(|o| o.path.as_deref())
                                .chain(
                                    finished
                                        .report
                                        .skipped
                                        .iter()
                                        .filter_map(|s| s.path.as_deref()),
                                )
                                .chain(pending.iter().map(|(archive, _)| *archive))
                                .filter(|archive| pending.iter().any(|(a, _)| a == archive))
                                .collect();
                            let mut seen = Vec::new();
                            archives.retain(|archive| {
                                let first = !seen.contains(archive);
                                seen.push(*archive);
                                first
                            });
                            if let Ok(options) = self.archive_options() {
                                for archive in archives {
                                    let mut consumers: Vec<String> = Vec::new();
                                    for (_, guid) in pending.iter().filter(|(a, _)| *a == archive) {
                                        if !consumers.iter().any(|c| c == guid)
                                            && !self.completed_base.as_ref().is_some_and(|ids| {
                                                ids.iter().any(|id| id.eq_ignore_ascii_case(guid))
                                            })
                                        {
                                            consumers.push((*guid).to_owned());
                                        }
                                    }
                                    self.enqueue_bound_fomod(
                                        run,
                                        archive.to_path_buf(),
                                        options.clone(),
                                        true,
                                        consumers,
                                    );
                                }
                            }
                            for error in &followups.inspection_errors {
                                self.logs.push(format!(
                                    "FOMOD inspection failed for {}: {}",
                                    error.archive.display(),
                                    self.downloads.redact(&error.error)
                                ));
                            }
                        }
                        self.downloads
                            .finish(kind, &finished, Ok(summary), &settled.binding.names);
                    }
                    Err(error) => {
                        let text = format!("Guide records were not changed: {error:#}");
                        self.downloads
                            .finish(kind, &finished, Err(&text), &settled.binding.names);
                    }
                }
                self.logs.push(self.downloads.status());
                succeeded
            }
        };
        self.refresh_input_stamp();
        if !self.sharing.closing
            && let Some((guide, plan, report, options)) = enrollment
        {
            self.sharing.enroll(
                self.acquisition_context(),
                guide,
                if self.downloads.unsaved_records {
                    None
                } else {
                    self.loaded_input.clone()
                },
                self.profile_directory.clone(),
                plan,
                report,
                options,
            );
        }
        self.emit_telemetry(
            "operation.completed",
            serde_json::Map::from_iter([
                (
                    "operation".into(),
                    serde_json::json!(match kind {
                        downloads_panel::Kind::Fetch => "fetch_downloads",
                        downloads_panel::Kind::Nxm => "nxm_download",
                    }),
                ),
                ("success".into(), serde_json::json!(succeeded)),
            ]),
        );
        self.start_pending_nxm_if_ready();
    }

    fn open_folder(&mut self, ctx: &egui::Context, folder: &str) {
        match std::fs::canonicalize(folder) {
            Ok(path) => ctx.open_url(egui::OpenUrl::new_tab(format!("file://{}", path.display()))),
            Err(error) => self.logs.push(format!("Open folder failed: {error}")),
        }
    }

    fn request_peer_settings(&mut self, changes: Vec<(String, serde_json::Value)>) {
        let off = changes.iter().any(|(key, value)| {
            (key == "peerSharing" && value == &serde_json::json!(false))
                || (key == "peerMode" && value == &serde_json::json!("off"))
        });
        let keep_off = changes
            .iter()
            .any(|(key, value)| key == "peerKeepSharing" && value == &serde_json::json!(false));
        if off || keep_off {
            self.sharing
                .pause(modsync::peer_sharing::PauseReason::Settings);
        }
        if off
            && self.receiver.is_some()
            && matches!(self.job, Job::Download | Job::NxmDownload | Job::Fetch)
        {
            self.cancelled.store(true, Ordering::Release);
            self.downloads.request_stop();
        }
        if self.peer_view.receiver.is_some() {
            self.peer_view.queued.extend(changes);
            return;
        }
        let directory = self.profile_directory.clone();
        self.peer_view.directory = Some(directory.clone());
        self.peer_view.saving = !changes.is_empty();
        self.peer_view.error = None;
        let (tx, rx) = mpsc::channel();
        self.peer_view.receiver = Some(rx);
        let repaint = self.repaint.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<_> {
                let store = modsync::profiles::SettingsFile::resolve(Some(Path::new(&directory)))?;
                if !changes.is_empty() {
                    store.update_peer_settings(&changes)?;
                }
                Ok(modsync::p2p::PeerSettings::from_settings(&store.load()?))
            })();
            let _ = tx.send(result);
            if let Some(ctx) = repaint {
                ctx.request_repaint();
            }
        });
    }

    fn refresh_peer_presentation(&mut self) {
        let result = self
            .peer_view
            .receiver
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(anyhow::anyhow!(
                    "Peer settings worker stopped unexpectedly"
                ))),
            });
        if let Some(result) = result {
            self.peer_view.receiver = None;
            self.peer_view.saving = false;
            if self.peer_view.directory.as_deref() == Some(self.profile_directory.as_str()) {
                match result {
                    Ok(settings) => {
                        self.peer_view.settings = Some(settings);
                        self.peer_view.error = None;
                    }
                    Err(error) => {
                        self.peer_view.settings = None;
                        let error = self.downloads.redact(&format!(
                            "Peer settings could not be read or saved: {error}"
                        ));
                        self.logs.push(error.clone());
                        self.peer_view.error = Some(error);
                    }
                }
            }
        }
        if self.peer_view.receiver.is_none() && !self.peer_view.queued.is_empty() {
            let changes = std::mem::take(&mut self.peer_view.queued);
            self.request_peer_settings(changes);
        }
        if self.peer_view.directory.as_deref() != Some(self.profile_directory.as_str())
            && self.peer_view.receiver.is_none()
        {
            self.peer_view.settings = None;
            self.request_peer_settings(Vec::new());
        }
    }

    fn peer_settings_content(&mut self, ui: &mut egui::Ui) {
        ui.label("Peer sharing defaults to Race: Origin and Peer compete. Peer transfers use public proxies only and fail closed if no proxy works.");
        ui.label("When enabled, discovery can fetch public proxy and tracker lists. Sharing Off prevents peer discovery and transfers. Changes apply to the next run. Each run reads saved settings; refresh here after edits outside the app.");
        if let Some(error) = &self.peer_view.error {
            ui.colored_label(egui::Color32::RED, error);
        }
        if self.peer_view.receiver.is_some() {
            ui.label("Reading or saving peer settings…");
        }
        if ui
            .add_enabled(
                self.receiver.is_none() && self.peer_view.receiver.is_none(),
                egui::Button::new("Refresh peer settings"),
            )
            .clicked()
        {
            self.request_peer_settings(Vec::new());
        }
        let Some(mut settings) = self.peer_view.settings.clone().filter(|_| {
            self.peer_view.directory.as_deref() == Some(self.profile_directory.as_str())
        }) else {
            return;
        };
        ui.collapsing("Peer-sharing settings", |ui| {
            let mut changes = Vec::new();
            ui.add_enabled_ui(
                self.receiver.is_none() && self.peer_view.receiver.is_none(),
                |ui| {
                    if ui
                        .checkbox(&mut settings.sharing, "Sharing enabled")
                        .changed()
                    {
                        changes.push((
                            "peerSharing".to_owned(),
                            serde_json::json!(settings.sharing),
                        ));
                    }
                    let prior = settings.mode;
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Mode:");
                        for (mode, label) in [
                            (modsync::p2p::PeerMode::Off, "Off"),
                            (modsync::p2p::PeerMode::Race, "Race"),
                            (modsync::p2p::PeerMode::Fallback, "Fallback"),
                        ] {
                            ui.radio_value(&mut settings.mode, mode, label);
                        }
                    });
                    if prior != settings.mode {
                        let mode = match settings.mode {
                            modsync::p2p::PeerMode::Off => "off",
                            modsync::p2p::PeerMode::Race => "race",
                            modsync::p2p::PeerMode::Fallback => "fallback",
                        };
                        changes.push(("peerMode".to_owned(), serde_json::json!(mode)));
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Completed-sharing upload cap (KiB/s):");
                        if ui
                            .add(
                                egui::DragValue::new(&mut settings.upload_kibps)
                                    .range(16..=u32::MAX)
                                    .clamp_existing_to_range(false),
                            )
                            .changed()
                        {
                            changes.push((
                                "peerUploadKiBps".to_owned(),
                                serde_json::json!(settings.upload_kibps),
                            ));
                        }
                    });
                },
            );
            ui.label(format!(
                "Effective mode: {:?}. Upload cap: {} KiB/s (minimum 16).",
                settings.effective_mode(),
                settings.upload_kibps.max(16)
            ));
            if ui
                .add_enabled(
                    self.peer_view.receiver.is_none(),
                    egui::Checkbox::new(
                        &mut settings.keep_sharing,
                        "Keep sharing verified completed archives",
                    ),
                )
                .changed()
            {
                changes.push((
                    "peerKeepSharing".to_owned(),
                    serde_json::json!(settings.keep_sharing),
                ));
            }
            if !changes.is_empty() {
                self.request_peer_settings(changes);
            }
        });
        ui.label(format!(
            "Peer: {:?}; upload cap {} KiB/s.",
            settings.effective_mode(),
            settings.upload_kibps.max(16)
        ));
    }

    fn downloads_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Downloads");
        self.peer_settings_content(ui);
        if self.fomod_pending() {
            ui.label(format!("FOMOD choices pending: {} queued. Finish or dismiss these before validation or installation.", self.fomod_queue.len()));
        }
        ui.label("Fetch Downloads plans every selected mod's archives (base and widescreen), keeps verified archives already in the workspace and downloads the rest. Nothing starts until you select it.");
        let blocker = self.fetch_blocker();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(blocker.is_none(), egui::Button::new("📥 Fetch Downloads"))
                .on_disabled_hover_text(blocker.unwrap_or_default())
                .clicked()
            {
                self.start_fetch();
            }
            let acquiring = self.receiver.is_some()
                && matches!(self.job, Job::Download | Job::NxmDownload | Job::Fetch);
            if acquiring
                && ui
                    .add_enabled(
                        !self.downloads.stopping(),
                        egui::Button::new("⏹ Stop Downloads"),
                    )
                    .clicked()
            {
                self.stop_confirmation = true;
            }
            if ui.button("Open workspace").clicked() {
                self.open_folder(ctx, &self.mods.clone());
            }
        });
        if let Some(reason) = blocker
            && !self.downloads.is_active()
        {
            ui.label(reason);
        }
        if self.receiver.is_some()
            && matches!(self.job, Job::Download | Job::NxmDownload | Job::Fetch)
        {
            ui.spinner();
        }
        if self.receiver.is_some() && self.job == Job::Download {
            ui.label(self.download_status());
        }
        if let Some(downloads_panel::Action::Open(url)) = self.downloads.show(ui) {
            // Manual links open only from this explicit click.
            ctx.open_url(egui::OpenUrl::new_tab(url));
        }
        ui.separator();
        ui.label("Direct archive download");
        ui.label("For provider pages, use the source links below and download in your browser.");
        ui.add_enabled_ui(self.receiver.is_none(), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.download_url)
                    .hint_text("https://…/mod.zip")
                    .desired_width(290.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.download_name).hint_text("Archive filename"),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.download_hash)
                    .hint_text("Expected SHA-256 (optional)"),
            );
            if ui.button("Download archive").clicked() {
                self.start(Job::Download, ctx);
            }
            if !self.pending_nxm.is_empty() && ui.button("Download linked Nexus file").clicked() {
                self.download_pending_nxm();
            }
        });
        ui.collapsing("Selected mod sources", |ui| {
            if let Some(guide) = &self.guide {
                for component in guide.components.iter().filter(|c| c.is_selected) {
                    ui.strong(&component.name);
                    for url in component.resource_registry.keys() {
                        ui.hyperlink(url);
                    }
                    for key in ["Link", "DownloadLink"] {
                        if let Some(value) = component.metadata.get(key) {
                            if let Some(url) = value.as_str() {
                                ui.hyperlink(url);
                            }
                            if let Some(urls) = value.as_array() {
                                for url in urls.iter().filter_map(|v| v.as_str()) {
                                    ui.hyperlink(url);
                                }
                            }
                        }
                    }
                }
            }
        });
        ui.label("Every archive is verified before it is kept. Progress shows measured bytes; a percentage appears only when the host reports a size.");
    }

    fn wizard_footer(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.downloads_visible() {
            ui.label(self.download_status());
        }
        let gate = self.wizard_gate();
        let wizard = self.wizard.as_ref().unwrap();
        let page = wizard.current();
        let back = wizard.can_back()
            && (self.receiver.is_none()
                || matches!(self.job, Job::Download | Job::NxmDownload | Job::Fetch));
        let next = wizard.can_next(gate);
        if self.downloads_visible() {
            ui.add_space(12.0);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().button_padding = egui::vec2(12.0, 8.0);
            ui.spacing_mut().item_spacing.x = 8.0;
            if page == Page::Finished {
                if ui
                    .add(egui::Button::new("Finish").min_size(egui::vec2(100.0, 32.0)))
                    .clicked()
                {
                    self.finish_wizard();
                }
            } else if ui
                .add_enabled(
                    next,
                    egui::Button::new(if page.install_phase().is_some() {
                        "Continue"
                    } else {
                        "Next"
                    })
                    .min_size(egui::vec2(100.0, 32.0)),
                )
                .clicked()
            {
                if matches!(page, Page::InstallStart | Page::WidescreenModSelection)
                    && !(page == Page::WidescreenModSelection && self.selected_widescreen() == 0)
                {
                    self.confirm_install = true;
                } else {
                    self.advance_wizard(ctx);
                }
            }
            if ui
                .add_enabled(
                    back,
                    egui::Button::new("Back").min_size(egui::vec2(100.0, 32.0)),
                )
                .clicked()
            {
                self.wizard.as_mut().unwrap().back();
            }
            if page.install_phase().is_some()
                && self.installing_page.active()
                && ui
                    .add(egui::Button::new("Stop Install").min_size(egui::vec2(100.0, 32.0)))
                    .clicked()
            {
                self.stop_confirmation = true;
            }
            // The only recovery actions for this page. The footer stays visible
            // in small windows. Restore targets the failed run's own roots.
            if page.install_phase().is_some()
                && self.installing_page.restore_target().is_some()
                && ui
                    .add_enabled(
                        self.receiver.is_none(),
                        egui::Button::new("Restore snapshot").min_size(egui::vec2(100.0, 32.0)),
                    )
                    .on_hover_text("Restore the folders this install used")
                    .clicked()
            {
                self.restore_captured(ctx);
            }
            if page.install_phase().is_some()
                && self.installing_page.failed()
                && !self.installing_page.pending()
                && ui
                    .add_enabled(
                        self.receiver.is_none()
                            && !self.phase_failed
                            && self.installing_page.same_folders(&self.mods, &self.game),
                        egui::Button::new("Revalidate phase").min_size(egui::vec2(100.0, 32.0)),
                    )
                    .on_disabled_hover_text(
                        "Set the game and mod folders back to the ones this install used",
                    )
                    .clicked()
            {
                self.reopen_failed_phase_validation();
            }
            // A stale success was not applied to this project and has no
            // Back or Continue; restart through the existing wizard path.
            if page.install_phase().is_some()
                && self.installing_page.not_applied()
                && ui
                    .add_enabled(
                        self.receiver.is_none(),
                        egui::Button::new("Restart wizard").min_size(egui::vec2(100.0, 32.0)),
                    )
                    .on_hover_text("Start again with the current guide and folders")
                    .clicked()
            {
                self.reset_installation();
                self.wizard = self.guide.as_ref().map(Wizard::new);
                self.utility = false;
            }
            if !next && page != Page::Finished {
                ui.label("Complete this page before continuing.");
            }
        });
    }

    fn wizard_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let page = self.wizard.as_ref().unwrap().current();
        if page == Page::Validate {
            ui.add_enabled_ui(!self.fomod_pending(), |ui| self.validation_content(ui, ctx));
            return;
        }
        if page.install_phase().is_some() {
            self.installing_content(ui, ctx);
            return;
        }
        let idle = self.receiver.is_none();
        ui.add_enabled_ui(idle, |ui| match page {
            Page::Welcome => {
                ui.vertical_centered(|ui| {
                    ui.heading("Welcome to ModSync");
                    ui.label("The complete multi-mod installation solution for Knights of the Old Republic");
                    ui.add_space(8.0);
                    ui.label("✓ Automatic Dependencies    ✓ Cross-Platform    ✓ One-Click Install");
                });
                ui.add_space(12.0);
                ui.columns(2, |columns| {
                    columns[0].group(|ui| {
                        ui.strong("The Problem");
                        ui.label("Installing multiple KOTOR mods is tedious and error-prone:");
                        for text in ["Manual file copying and deletion", "Complex install order requirements", "One mistake = start over from scratch", "Hours spent clicking through installers", "Confusing compatibility instructions"] { ui.label(format!("• {text}")); }
                    });
                    columns[1].group(|ui| {
                        ui.strong("The Solution");
                        ui.label("ModSync automates installation for you:");
                        for text in ["Select mods you want, click Install", "Automatic dependency resolution", "Smart install order management", "No manual file operations needed", "Built-in compatibility checking"] { ui.label(format!("• {text}")); }
                    });
                });
                ui.add_space(12.0);
                ui.group(|ui| {
                    ui.strong("Installation Process");
                    for (title, detail) in [
                        ("1. Configure Directories", "Point to your KOTOR installation and mod workspace folder"),
                        ("2. Load Instruction File", "Choose a curated mod build from popular configurations"),
                        ("3. Select Your Mods", "Browse by category or tier — dependencies auto-select"),
                        ("4. Download Mod Archives", "Select Fetch Downloads to acquire missing archives from direct and supported hosts. Hosts that need a login, CAPTCHA or wait stay manual in your browser."),
                        ("5. Validation & Install", "Verify everything is ready, then install automatically"),
                    ] { ui.strong(title); ui.label(detail); ui.add_space(6.0); }
                });
                ui.add_space(12.0);
                ui.group(|ui| {
                    ui.strong("Key Features");
                    ui.columns(2, |columns| {
                        for (column, title, detail) in [(0, "Smart Dependencies", "Resolves and installs required mods in the correct order"), (1, "Pre-Install Validation", "Catches compatibility issues before installation starts"), (0, "Checkpoint System", "Roll back to previous states if something goes wrong"), (1, "Cross-Platform", "Works on Windows, Linux, and macOS without Wine")] {
                            columns[column].vertical_centered(|ui| { ui.strong(title); ui.label(detail); ui.add_space(6.0); });
                        }
                    });
                });
                ui.add_space(12.0);
                ui.group(|ui| {
                    ui.strong("Important: Fresh Installation Required");
                    ui.label("For best results, start with a clean KOTOR install. If you previously installed mods manually, uninstall and reinstall the game before proceeding.");
                });
                ui.vertical_centered(|ui| {
                    ui.strong("Ready to transform your KOTOR experience?");
                    ui.label("Click Next to begin the installation wizard");
                });
                if let Some(g) = &self.guide { ui.label(format!("{} components loaded", g.components.len())); }
            }
            Page::Preamble | Page::AspyrNotice => {
                let (camel, pascal) = match page {
                    Page::Preamble => ("preambleContent", "PreambleContent"),
                    Page::AspyrNotice => ("aspyrExclusiveWarningContent", "AspyrExclusiveWarningContent"),
                    _ => ("widescreenWarningContent", "WidescreenWarningContent"),
                };
                if let Some(content) = self.guide.as_ref().and_then(|g| metadata_text(g, camel, pascal)) {
                    if page == Page::Preamble {
                        rich_content::show(ui, camel, content);
                    } else {
                        if ui.small_button("Copy warning text").clicked() { ui.ctx().copy_text(content.to_owned()); }
                        ui.add(egui::Label::new(content).wrap());
                    }
                }
            }
            Page::ModDirectory => {
                if !self.sharing.edit_gate(ui) { return; }
                ui.label("Choose an existing workspace for downloaded archives and processed mod files. Keep it separate from your game installation.");
                if Self::path_row(ui, "Mod workspace", &mut self.mods, true) { self.invalidate_validation(); }
                ui.label(if Path::new(&self.mods).is_dir() { "Directory exists" } else { "Choose an existing directory to continue." });
            }
            Page::GameDirectory => {
                if !self.sharing.edit_gate(ui) { return; }
                ui.label("Choose the folder containing your supported PC KOTOR installation.");
                if Self::path_row(ui, "Game directory", &mut self.game, true) { self.invalidate_validation(); }
                ui.label(format!("Detected game: {:?}", modsync::game::detect(&modsync::game::resolve_install_root(Path::new(&self.game)))));
            }
            Page::ModSelection => self.surface(ui, ctx, 1),
            Page::DownloadsExplain => {
                ui.heading("Download your mod files");
                ui.label(format!(
                    "{} base and {} widescreen mods selected",
                    self.selected_base(),
                    self.selected_widescreen()
                ));
                ui.label("Nothing downloads automatically. Select Fetch Downloads in the Downloads panel to acquire the missing archives of every selected mod into your workspace; archives already there are verified and kept.");
                ui.label("Downloads continue while you read later pages, and the Downloads panel keeps their status. Changing the guide, selection or folders waits until downloads finish. Run validation after downloads finish; installation still requires a fresh validation.");
            }
            Page::Validate => self.validation_content(ui, ctx),
            Page::InstallStart => {
                ui.label(format!("Install {} selected base mods", self.selected_base()));
                ui.label(format!("Game: {}", self.game)); ui.label(format!("Workspace: {}", self.mods));
                if let Some(guide) = self.phase_guide() && let Ok(ordered) = guide.ordered() { for c in ordered { ui.label(&c.name); } }
                ui.label("Next asks you to confirm. Installation creates a recovery snapshot of both folders and stops on the first failure.");
                if !self.validated { ui.label("Inputs changed. Go Back and run validation again."); }
            }
            Page::WidescreenModSelection => {
                if !self.sharing.edit_gate(ui) { return; }
                let mut change = None;
                let detected = modsync::game::detect(&modsync::game::resolve_install_root(Path::new(&self.game)));
                if let Some(guide) = &mut self.guide { for (index, c) in guide.components.iter().enumerate() {
                    if c.metadata.get("WidescreenOnly").and_then(serde_json::Value::as_bool) == Some(true) {
                        let mut selected = c.is_selected;
                        let reason = (self.selection_panel.stamp == self.input_stamp)
                            .then(|| self.selection_panel.availability.reason(&c.guid))
                            .flatten();
                        let compatible = modsync::game::selectable(c, detected);
                        if ui.add_enabled(reason.is_none() && compatible && !self.selection_panel.busy(), egui::Checkbox::new(&mut selected, &c.name)).changed() {
                            change = Some((index, selected));
                        }
                        if let Some(reason) = reason { ui.label(reason); }
                        else if !compatible { ui.label("This mod is not compatible with the detected game."); }
                    }
                } }
                if let Some((index, selected)) = change {
                    let guid = self.guide.as_ref().unwrap().components[index].guid.clone();
                    self.start_selection(Some(modsync::selection::Action::SetWidescreen { guid, selected }));
                }
                ui.label(format!("{} widescreen mods selected", self.selected_widescreen()));
                if self.selected_widescreen() > 0 {
                    ui.label("Validate the remaining phase against the verified base completion before continuing.");
                    self.surface(ui, ctx, 2);
                } else { ui.label("Next skips the empty widescreen phase. No additional mods will be installed."); }
            }
            // Rendered by installing_content before this match.
            Page::Installing | Page::WidescreenInstalling => {}
            Page::WidescreenNotice => self.widescreen_notice_content(ui),
            Page::BaseInstallComplete => self.base_install_complete_content(ui, ctx),
            Page::WidescreenComplete => self.widescreen_complete_content(ui),
            Page::Finished => self.finished_content(ui),
        });
        self.refresh_input_stamp();
    }

    fn surface(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, surface: usize) {
        let mut summary_action = None;
        let mut preference_save = false;
        let mut patcher_save = false;
        let mut selection_action = None;
        let mut refresh_selection = false;
        let mut prepare_receipt = false;
        let deployed_guid = self
            .guide
            .as_ref()
            .and_then(|g| g.components.get(self.selected))
            .filter(|c| self.deployment_badge(&c.guid))
            .map(|c| c.guid.clone());
        if !self.sharing.edit_gate(ui) {
            return;
        }
        match surface {
            0 => {
                ui.heading("Load an instruction file");
                if ui
                    .button("Validation tools")
                    .on_hover_text(
                        "Trust, FOMOD choices, patcher settings, conflict analysis and recovery",
                    )
                    .clicked()
                {
                    self.page = 2;
                }
                ui.label("Read ModSync TOML, JSON, YAML, XML, or Markdown. Review generated instructions before installing.");
                if Self::path_row(ui, "Guide", &mut self.input, false) {
                    self.validated = false;
                    self.conflicts = None;
                    validation_page::clear_report(
                        &mut self.validation_report,
                        &mut self.validation_page,
                    );
                }
                if ui.button("Load guide").clicked() {
                    self.load();
                }
                if let Some(url) = self.pending_protocol.front().cloned() {
                    ui.separator();
                    ui.strong("Open ModSync build link?");
                    ui.label(self.downloads.redact(&url));
                    ui.label("Only continue for links you trust.");
                    ui.horizontal(|ui| {
                        if ui.button("Download & Open").clicked() {
                            self.open_pending_protocol();
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending_protocol.pop_front();
                            self.logs.push("Declined ModSync build link.".into());
                        }
                    });
                }
                if let Some(url) = self.pending_nxm.front() {
                    ui.separator();
                    ui.strong("Nexus Mod Manager link received");
                    ui.label(self.downloads.redact(url));
                    if ui
                        .add_enabled(
                            self.guide.is_some()
                                && !self.mods.trim().is_empty()
                                && self.receiver.is_none(),
                            egui::Button::new("Download linked Nexus file"),
                        )
                        .clicked()
                    {
                        self.download_pending_nxm();
                    }
                    if ui.button("Dismiss Nexus link").clicked() {
                        self.pending_nxm.pop_front();
                    }
                }
                if ui
                    .add_enabled(
                        self.guide.is_some(),
                        egui::Button::new("Draft instructions from prose"),
                    )
                    .clicked()
                    && let Some(guide) = &mut self.guide
                {
                    let results = modsync::draft::generate(guide);
                    self.logs.push(format!("Drafted {} instructions; inspect each component before accepting its draft.", results.iter().map(|r| r.draft_instruction_count).sum::<usize>()));
                    self.validated = false;
                    self.conflicts = None;
                    validation_page::clear_report(
                        &mut self.validation_report,
                        &mut self.validation_page,
                    );
                }
                ui.separator();
                if Self::path_row(ui, "Game directory", &mut self.game, true)
                    | Self::path_row(ui, "Mod directory", &mut self.mods, true)
                {
                    self.validated = false;
                    self.conflicts = None;
                    validation_page::clear_report(
                        &mut self.validation_report,
                        &mut self.validation_page,
                    );
                }
                let game_root = modsync::game::resolve_install_root(Path::new(&self.game));
                ui.label(format!(
                    "Detected game: {:?}",
                    modsync::game::detect(&game_root)
                ));
                ui.label("Use separate folders. Put downloaded archives in the mod directory.");
                if ui.button("Save directories for next launch").clicked() {
                    self.desktop.settings.source_path = Some(self.mods.clone());
                    self.desktop.settings.destination_path = Some(self.game.clone());
                    preference_save = true;
                }
                if self.probe.is_some() {
                    ui.label("Probe launch: the Nexus key comes only from the launch arguments and is never stored.");
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Nexus API key");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.nexus_api_key)
                                .password(true)
                                .desired_width(220.0),
                        );
                        if ui
                            .add_enabled(
                                !self.desktop.busy(),
                                egui::Button::new("Validate and save key"),
                            )
                            .clicked()
                        {
                            self.validate_nexus_key();
                        }
                        ui.label("Key validation status is shown in Desktop settings.");
                    });
                }
                if ui
                    .button("Desktop settings, privacy and diagnostics")
                    .clicked()
                {
                    self.desktop.open = true;
                }
                if let Some(g) = &self.guide {
                    ui.label(format!("{} components loaded", g.components.len()));
                }
            }
            1 => {
                if ui
                    .add_enabled(
                        self.guide.is_some(),
                        egui::Button::new("Check Nexus updates"),
                    )
                    .clicked()
                {
                    self.start_update_check(ctx);
                }
                let detected_game = modsync::game::detect(&modsync::game::resolve_install_root(
                    Path::new(&self.game),
                ));
                let Some(g) = &mut self.guide else {
                    ui.label("Load a guide first.");
                    return;
                };
                ui.label(&self.selection_panel.status);
                if ui
                    .button("Prepare selection receipt")
                    .on_hover_text(
                        "Hash mod files and record selection evidence; large folders can take time",
                    )
                    .clicked()
                {
                    prepare_receipt = true;
                }
                if let Some(receipt) = &self.selection_panel.receipt
                    && ui.button("Copy selection receipt").clicked()
                {
                    ctx.copy_text(receipt.clone());
                }
                if ui.button("Refresh archive availability").clicked() {
                    refresh_selection = true;
                }
                ui.horizontal(|ui| {
                    if ui.button("Select all").clicked() {
                        selection_action = Some(modsync::selection::Action::SelectVisible(
                            modsync::selection::Filter {
                                search: self.search.clone(),
                                tier: self.tier_filter.clone(),
                                category: self.category_filter.clone(),
                                spoiler_free: self.spoiler_free,
                            },
                        ));
                        self.validated = false;
                        self.conflicts = None;
                        validation_page::clear_report(
                            &mut self.validation_report,
                            &mut self.validation_page,
                        );
                        self.confirm_install = false;
                    }
                    if ui.button("Deselect all").clicked() {
                        selection_action = Some(modsync::selection::Action::DeselectBase);
                        self.validated = false;
                        self.conflicts = None;
                        validation_page::clear_report(
                            &mut self.validation_report,
                            &mut self.validation_page,
                        );
                        self.confirm_install = false;
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("Search displayed name or description"),
                    );
                    if ui
                        .add_enabled(
                            !self.search.is_empty()
                                || !self.tier_filter.is_empty()
                                || !self.category_filter.is_empty(),
                            egui::Button::new("Clear filters"),
                        )
                        .clicked()
                    {
                        self.search.clear();
                        self.tier_filter.clear();
                        self.category_filter.clear();
                    }
                });
                let mut spoiler_free = self.spoiler_free;
                if ui
                    .checkbox(&mut spoiler_free, "Spoiler-free names and descriptions")
                    .changed()
                {
                    self.desktop.settings.spoiler_free_mode = spoiler_free;
                    preference_save = true;
                }
                ui.horizontal_wrapped(|ui| {
                    let tiers: std::collections::BTreeSet<_> = g
                        .components
                        .iter()
                        .map(|c| c.tier.clone())
                        .filter(|s| !s.is_empty())
                        .collect();
                    let categories: std::collections::BTreeSet<_> = g
                        .components
                        .iter()
                        .flat_map(|c| c.category.clone())
                        .collect();
                    egui::ComboBox::from_id_salt("tier_filter")
                        .selected_text(if self.tier_filter.is_empty() {
                            "All tiers"
                        } else {
                            &self.tier_filter
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.tier_filter, String::new(), "All tiers");
                            for tier in tiers {
                                ui.selectable_value(&mut self.tier_filter, tier.clone(), tier);
                            }
                        });
                    if ui
                        .add_enabled(
                            !self.tier_filter.is_empty(),
                            egui::Button::new("Select through tier"),
                        )
                        .clicked()
                    {
                        selection_action = Some(modsync::selection::Action::SelectThroughTier(
                            self.tier_filter.clone(),
                        ));
                        self.validated = false;
                        self.conflicts = None;
                        validation_page::clear_report(
                            &mut self.validation_report,
                            &mut self.validation_page,
                        );
                        self.confirm_install = false;
                    }
                    egui::ComboBox::from_id_salt("category_filter")
                        .selected_text(if self.category_filter.is_empty() {
                            "All categories"
                        } else {
                            &self.category_filter
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.category_filter,
                                String::new(),
                                "All categories",
                            );
                            for category in categories {
                                ui.selectable_value(
                                    &mut self.category_filter,
                                    category.clone(),
                                    category,
                                );
                            }
                        });
                    if ui
                        .add_enabled(
                            !self.category_filter.is_empty(),
                            egui::Button::new("Select category"),
                        )
                        .clicked()
                    {
                        selection_action =
                            Some(modsync::selection::Action::SelectCategories(vec![
                                self.category_filter.clone(),
                            ]));
                        self.validated = false;
                        self.conflicts = None;
                        validation_page::clear_report(
                            &mut self.validation_report,
                            &mut self.validation_page,
                        );
                        self.confirm_install = false;
                    }
                });
                ui.label(format!(
                    "Base build: {}",
                    modsync::selection::selection_summary(&g.components)
                ));
                let (required, optional) = modsync::selection::selection_badges(&g.components);
                ui.horizontal(|ui| {
                    ui.strong(format!("Required: {required}"))
                        .on_hover_text("Selected base mods that declare dependencies.");
                    ui.strong(format!("Optional: {optional}"))
                        .on_hover_text("Selected base mods with no declared dependencies.");
                });
                ui.label(format!(
                    "Base build: {}",
                    modsync::selection::filter_summary(
                        &g.components,
                        &self.search,
                        &self.tier_filter,
                        &self.category_filter,
                        self.spoiler_free
                    )
                ));
                if ui.button("Select multiple categories…").clicked() {
                    self.category_selection = Some(
                        g.components
                            .iter()
                            .filter(|c| {
                                c.metadata
                                    .get("WidescreenOnly")
                                    .and_then(serde_json::Value::as_bool)
                                    != Some(true)
                            })
                            .flat_map(|c| c.category.iter().cloned())
                            .map(|category| (category, false))
                            .collect(),
                    );
                }
                ui.separator();
                let mut groups = modsync::selection::category_groups(
                    &g.components,
                    &self.search,
                    &self.tier_filter,
                    &self.category_filter,
                    self.spoiler_free,
                );
                for (_, indices) in &mut groups {
                    indices.retain(|i| {
                        g.components[*i]
                            .metadata
                            .get("WidescreenOnly")
                            .and_then(serde_json::Value::as_bool)
                            != Some(true)
                    });
                }
                groups.retain(|(_, indices)| !indices.is_empty());
                let force_open = if ui
                    .button(if self.all_expanded {
                        "Collapse all"
                    } else {
                        "Expand all"
                    })
                    .clicked()
                {
                    self.all_expanded = !self.all_expanded;
                    Some(self.all_expanded)
                } else {
                    None
                };
                if groups.is_empty() {
                    ui.heading("No mods match your filters");
                    ui.label("Try adjusting your search or clearing filters");
                }
                ui.columns(2, |columns| {
                    egui::ScrollArea::vertical()
                        .id_salt("mods")
                        .show(&mut columns[0], |ui| {
                            for (category, indices) in &groups {
                                let selected_count = indices
                                    .iter()
                                    .filter(|i| g.components[**i].is_selected)
                                    .count();
                                let has_unselected_eligible = indices.iter().any(|index| {
                                    let component = &g.components[*index];
                                    !component.is_selected
                                        && modsync::game::selectable(component, detected_game)
                                        && (self.selection_panel.stamp != self.input_stamp
                                            || self
                                                .selection_panel
                                                .availability
                                                .reason(&component.guid)
                                                .is_none())
                                });
                                let select_group = selected_count == 0 || has_unselected_eligible;
                                ui.horizontal(|ui| {
                                    if ui
                                        .small_button(if !select_group {
                                            "Deselect group"
                                        } else {
                                            "Select group"
                                        })
                                        .clicked()
                                    {
                                        let selected = select_group;
                                        selection_action =
                                            Some(modsync::selection::Action::SetGroup {
                                                category: category.clone(),
                                                selected,
                                                filter: modsync::selection::Filter {
                                                    search: self.search.clone(),
                                                    tier: self.tier_filter.clone(),
                                                    category: self.category_filter.clone(),
                                                    spoiler_free: self.spoiler_free,
                                                },
                                            });
                                        self.validated = false;
                                        self.conflicts = None;
                                        validation_page::clear_report(
                                            &mut self.validation_report,
                                            &mut self.validation_page,
                                        );
                                        self.confirm_install = false;
                                    }
                                    ui.label(format!(
                                        "{selected_count} / {} selected",
                                        indices.len()
                                    ));
                                });
                                egui::CollapsingHeader::new(format!(
                                    "{category} ({} mods)",
                                    indices.len()
                                ))
                                .id_salt(("category_group", category))
                                .default_open(self.all_expanded)
                                .open(force_open)
                                .show(ui, |ui| {
                                    for index in indices {
                                        let c = &mut g.components[*index];
                                        let mut toggled = None;
                                        let mut checked = c.is_selected;
                                        let reason = (self.selection_panel.stamp
                                            == self.input_stamp)
                                            .then(|| {
                                                self.selection_panel.availability.reason(&c.guid)
                                            })
                                            .flatten();
                                        ui.horizontal(|ui| {
                                            if ui
                                                .add_enabled(
                                                    reason.is_none()
                                                        && modsync::game::selectable(
                                                            c,
                                                            detected_game,
                                                        ),
                                                    egui::Checkbox::new(&mut checked, ""),
                                                )
                                                .on_hover_text(reason.unwrap_or("Select component"))
                                                .changed()
                                            {
                                                toggled = Some(checked);
                                                self.validated = false;
                                                self.conflicts = None;
                                                validation_page::clear_report(
                                                    &mut self.validation_report,
                                                    &mut self.validation_page,
                                                );
                                            }
                                            if ui
                                                .selectable_label(
                                                    self.selected == *index,
                                                    modsync::selection::display_name(
                                                        c,
                                                        self.spoiler_free,
                                                    ),
                                                )
                                                .on_hover_text(modsync::selection::tooltip(
                                                    c,
                                                    self.spoiler_free,
                                                ))
                                                .clicked()
                                            {
                                                self.selected = *index;
                                            }
                                            if modsync::updates::component_update_available(c) {
                                                ui.label("Update").on_hover_text(
                                                    "A newer version is available on Nexus Mods",
                                                );
                                            }
                                        });
                                        if let Some(reason) = reason {
                                            ui.small(reason);
                                        }
                                        if let Some(selected) = toggled {
                                            selection_action =
                                                Some(modsync::selection::Action::SetComponent {
                                                    guid: g.components[*index].guid.clone(),
                                                    selected,
                                                });
                                        }
                                    }
                                });
                            }
                        });
                    egui::ScrollArea::vertical()
                        .id_salt("details")
                        .show(&mut columns[1], |ui| {
                            let mut option_change = None;
                            if let Some(c) = g
                                .components
                                .get_mut(self.selected)
                                .filter(|c| {
                                    c.metadata
                                        .get("WidescreenOnly")
                                        .and_then(serde_json::Value::as_bool)
                                        != Some(true)
                                })
                                .filter(|c| {
                                    modsync::selection::matches_filters(
                                        c,
                                        &self.search,
                                        &self.tier_filter,
                                        &self.category_filter,
                                        self.spoiler_free,
                                    )
                                })
                            {
                                if deployed_guid.as_deref() == Some(c.guid.as_str()) {
                                    ui.strong("Deployed in the reviewed managed inventory");
                                }
                                let actions = summary_page::show(ui, c, self.spoiler_free, true);
                                if actions.download || actions.jump_to_instruction.is_some() {
                                    summary_action = Some((c.guid.clone(), actions));
                                }
                                option_change = Self::options(
                                    ui,
                                    c,
                                    self.spoiler_free,
                                    (self.selection_panel.stamp == self.input_stamp)
                                        .then_some(&self.selection_panel.availability),
                                );
                                if option_change.is_some() {
                                    self.validated = false;
                                    self.conflicts = None;
                                    validation_page::clear_report(
                                        &mut self.validation_report,
                                        &mut self.validation_page,
                                    );
                                }
                                if c.metadata.contains_key("DraftReview") {
                                    ui.collapsing("Draft instructions and review notes", |ui| {
                                        ui.monospace(
                                            serde_json::to_string_pretty(&c.instructions)
                                                .unwrap_or_default(),
                                        );
                                        ui.monospace(
                                            serde_json::to_string_pretty(
                                                &c.metadata["DraftReview"],
                                            )
                                            .unwrap_or_default(),
                                        );
                                    });
                                    if modsync::draft::requires_review(c)
                                        && ui.button("Mark these instructions reviewed").clicked()
                                    {
                                        if let Err(e) = modsync::draft::mark_reviewed(c) {
                                            self.logs.push(format!("Review remains open: {e:#}"));
                                        }
                                        self.validated = false;
                                        self.conflicts = None;
                                        validation_page::clear_report(
                                            &mut self.validation_report,
                                            &mut self.validation_page,
                                        );
                                    }
                                }
                            }
                            if let Some((option, selected)) = option_change {
                                selection_action = Some(modsync::selection::Action::SetOption {
                                    guid: option,
                                    selected,
                                });
                            }
                        });
                });
            }
            2 => {
                ui.label("Validation simulates file changes. Installation keeps a recovery copy of both folders.");
                ui.label(format!("Game: {}", self.game));
                ui.label(format!("Mods: {}", self.mods));
                if ui
                    .checkbox(
                        &mut self.allow_processes,
                        "Trust this guide to run external programs",
                    )
                    .changed()
                {
                    self.validated = false;
                    self.conflicts = None;
                    validation_page::clear_report(
                        &mut self.validation_report,
                        &mut self.validation_page,
                    );
                }
                Self::path_row(ui, "FOMOD archive", &mut self.fomod_archive, false);
                if let Some(component) = self
                    .guide
                    .as_ref()
                    .and_then(|guide| guide.components.get(self.selected))
                {
                    ui.label(format!("FOMOD configuration target: {}", component.name));
                }
                if ui
                    .add_enabled(
                        !self.fomod_archive.trim().is_empty(),
                        egui::Button::new("Configure FOMOD archive…"),
                    )
                    .clicked()
                {
                    let path = PathBuf::from(&self.fomod_archive);
                    let budget = if self.expanded_budget.trim().is_empty() {
                        Ok(None)
                    } else {
                        self.expanded_budget.trim().parse::<u64>().map(Some)
                    };
                    match budget {
                        Ok(max_expanded_bytes) => {
                            self.enqueue_fomod(
                                0,
                                path,
                                modsync::archive::Options {
                                    scratch_dir: (!self.scratch.is_empty())
                                        .then(|| PathBuf::from(&self.scratch)),
                                    max_expanded_bytes,
                                },
                                false,
                            );
                        }
                        Err(_) => self
                            .logs
                            .push("Expanded-byte budget must be a nonnegative integer.".into()),
                    }
                }
                if Self::path_row(
                    ui,
                    "FOMOD choices file (optional)",
                    &mut self.fomod_choices,
                    false,
                ) {
                    self.validated = false;
                    self.conflicts = None;
                    validation_page::clear_report(
                        &mut self.validation_report,
                        &mut self.validation_page,
                    );
                    self.confirm_install = false;
                }
                ui.collapsing("Archive storage", |ui| {
                        if Self::path_row(ui, "Scratch folder (optional)", &mut self.scratch, true) { self.validated = false; self.conflicts = None; validation_page::clear_report(&mut self.validation_report, &mut self.validation_page); }
                        ui.label("Use an existing folder outside the game and mod folders. Blank uses system temp.");
                        ui.horizontal(|ui| {
                            ui.label("Maximum expanded bytes per archive (optional)");
                            if ui.text_edit_singleline(&mut self.expanded_budget).changed() { self.validated = false; self.conflicts = None; validation_page::clear_report(&mut self.validation_report, &mut self.validation_page); }
                        });
                    });
                if self.allow_processes {
                    ui.horizontal(|ui| {
                        ui.label("Patcher backend");
                        egui::ComboBox::from_id_salt("patcher-engine")
                            .selected_text(match self.patcher_engine {
                                PatcherEngine::HoloPatcher => "HoloPatcher",
                                PatcherEngine::KPatcher => "KPatcher",
                                PatcherEngine::OdyPatcher => "OdyPatcher",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.patcher_engine,
                                    PatcherEngine::HoloPatcher,
                                    "HoloPatcher",
                                );
                                ui.selectable_value(
                                    &mut self.patcher_engine,
                                    PatcherEngine::KPatcher,
                                    "KPatcher",
                                );
                                ui.selectable_value(
                                    &mut self.patcher_engine,
                                    PatcherEngine::OdyPatcher,
                                    "OdyPatcher",
                                );
                            });
                    });
                    if Self::path_row(ui, "Patcher executable", &mut self.patcher, false) {
                        self.validated = false;
                        self.conflicts = None;
                        validation_page::clear_report(
                            &mut self.validation_report,
                            &mut self.validation_page,
                        );
                    }
                    if ui.button("Save patcher setting").clicked() {
                        patcher_save = true;
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("Analyze conflicts").clicked() {
                        self.start(Job::Conflicts, ctx);
                    }
                    if ui.button("Run validation").clicked() {
                        self.start(Job::Validate, ctx);
                    }
                    if ui
                        .add_enabled(
                            self.install_review_ready(),
                            egui::Button::new("Install selected mods"),
                        )
                        .on_disabled_hover_text("Run successful validation with the current guide and folders before installing.")
                        .on_hover_text("Review the install, then confirm the snapshot and installation.")
                        .clicked()
                    {
                        self.open_install_review(ctx);
                    }

                    let captured = self
                        .installing_page
                        .restore_target()
                        .map(|roots| roots.game.display().to_string());
                    let hover = match &captured {
                        Some(game) => format!("Restores the failed install's own folders: {game}"),
                        None => "Restores the pending snapshot for the folders above".into(),
                    };
                    if ui
                        .button("Restore failed install")
                        .on_hover_text(hover)
                        .clicked()
                    {
                        // start() routes to the captured roots when a failed
                        // install is pending.
                        self.start(Job::Restore, ctx);
                    }
                    if ui.button("Save selection…").clicked() {
                        self.shell_work(main_window::Work::SaveSelection, None, ctx);
                    }
                });
                ui.separator();
                if let Some(report) = &self.validation_report {
                    ui.collapsing("Validation report", |ui| {
                        let first_issue = report.first_issue();
                        let mut scroll_to_issue = false;
                        ui.horizontal(|ui| {
                            ui.strong(format!("Errors: {}", report.error_count));
                            ui.strong(format!("Warnings: {}", report.warning_count));
                            ui.strong(format!("Passed: {}", report.passed_count));
                            if ui.button("Copy validation report").clicked() {
                                match serde_json::to_string_pretty(report) {
                                    Ok(text) => ui.ctx().copy_text(text),
                                    Err(error) => self.logs.push(format!("Copy failed: {error}")),
                                }
                            }
                        });
                        if first_issue.is_some()
                            && ui.button("Go to first issue").clicked()
                            && let Some(message) = first_issue
                        {
                            scroll_to_issue = true;
                            self.logs.push(format!("First validation issue: {message}"));
                        }
                        for stage in &report.stages {
                            let color = if !stage.passed {
                                egui::Color32::RED
                            } else if stage.has_warnings {
                                egui::Color32::YELLOW
                            } else {
                                egui::Color32::GREEN
                            };
                            ui.colored_label(
                                color,
                                format!("{:?}: {}", stage.stage, stage.summary),
                            );
                            for message in &stage.messages {
                                let response = ui.label(message);
                                if scroll_to_issue && Some(message.as_str()) == first_issue {
                                    response.scroll_to_me(Some(egui::Align::Center));
                                    scroll_to_issue = false;
                                }
                            }
                        }
                    });
                }
                if self.conflicts.is_some() && ui.button("Show conflict details").clicked() {
                    self.tools.conflicts_open = true;
                }
            }
            _ => self.guide_editor(ui),
        }
        if let Some(action) = selection_action {
            self.start_selection(Some(action));
        } else if prepare_receipt {
            self.start_selection_with_receipt(None, true);
        } else if refresh_selection {
            self.start_selection(None);
        }
        if patcher_save {
            self.save_patcher_preferences();
        } else if preference_save {
            self.save_desktop_preferences();
        }
        if let Some((guid, actions)) = summary_action {
            if actions.download {
                self.start_fetch_scope(Some(guid));
            } else if let Some(index) = actions.jump_to_instruction {
                self.editor_jump(&guid, index);
            }
        }
    }

    fn emit_telemetry(&self, name: &str, attributes: serde_json::Map<String, serde_json::Value>) {
        if self.desktop.ready(&self.telemetry, &self.profile_directory) && !self.sharing.closing {
            modsync::telemetry::global().record_event(name, attributes.into_iter().collect());
        }
    }

    /// The frozen tests' constructor: default settings directory, no probe.
    #[cfg(test)]
    fn new(input: Option<PathBuf>, game: Option<PathBuf>, mods: Option<PathBuf>) -> Self {
        Self::with_launch(input, game, mods, None, None)
    }

    /// The settings directory and any probe credentials are fixed before the first
    /// store read, telemetry load, guide load or protocol dispatch.
    fn with_launch(
        input: Option<PathBuf>,
        game: Option<PathBuf>,
        mods: Option<PathBuf>,
        settings_dir: Option<PathBuf>,
        probe: Option<ProbeContext>,
    ) -> Self {
        let string = |p: Option<PathBuf>| {
            p.map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let profile_directory = match &settings_dir {
            Some(directory) => directory.to_string_lossy().into_owned(),
            None => modsync::profiles::settings_directory(None)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        };
        let mut app = Self {
            checkpoints: Default::default(),
            tools: Default::default(),
            editor: Default::default(),
            desktop: Default::default(),
            merge: Default::default(),
            deployment: Default::default(),
            shell: Default::default(),
            standalone: Default::default(),
            maintenance: Default::default(),
            resource_watch: Default::default(),
            selection_panel: Default::default(),
            sharing: Default::default(),
            sharing_continuation: None,
            theme: "/Styles/LightStyle.axaml".into(),
            input: string(input),
            loaded_input: None,
            profile_directory,
            profile_name: String::new(),
            active_profile_name: String::new(),
            profile_names: Vec::new(),
            profile_target: String::new(),
            profile_delete: None,
            nexus_api_key: String::new(),
            game: string(game),
            mods: string(mods),
            patcher: String::new(),
            patcher_engine: PatcherEngine::default(),
            scratch: String::new(),
            fomod_choices: String::new(),
            fomod_archive: String::new(),
            fomod_editor: Default::default(),
            fomod_queue: Default::default(),
            fomod_owner: None,
            fomod_active: None,
            fomod_failure: None,
            fomod_import: None,
            peer_view: Default::default(),
            expanded_budget: String::new(),
            allow_processes: false,
            guide: None,
            page: 0,
            wizard: None,
            utility: false,
            input_stamp: String::new(),
            guide_generation: 0,
            validation_input_stamp: None,
            validation_page: validation_page::State::default(),
            installing_page: installing_page::State::default(),
            completion: completion_pages::State::default(),
            stop_confirmation: false,
            selected: 0,
            spoiler_free: false,
            search: String::new(),
            tier_filter: String::new(),
            category_filter: String::new(),
            all_expanded: false,
            category_selection: None,
            logs: Vec::new(),
            receiver: None,
            protocol_receiver: None,
            #[cfg(target_os = "macos")]
            native_protocol_receiver: None,
            _instance_guard: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            job: Job::Validate,
            validated: false,
            conflicts: None,
            validation_report: None,
            confirm_install: false,
            completed_base: None,
            pending_base: Vec::new(),
            install_context: None,
            finished: false,
            phase_failed: false,
            restore_roots: None,
            download_url: String::new(),
            download_name: String::new(),
            download_hash: String::new(),
            protocol_load: Default::default(),
            #[cfg(target_os = "macos")]
            protocol_inbox: Default::default(),
            pending_protocol: Default::default(),
            pending_nxm: Default::default(),
            telemetry: Default::default(),
            downloads: Default::default(),
            direct_download: None,
            explicit_settings: settings_dir,
            probe,
            repaint: None,
        };
        match &app.probe {
            // A probe never reads a stored key: only its explicit one is ever sent.
            Some(probe) => app.nexus_api_key = probe.nexus_api_key.clone().unwrap_or_default(),
            None => {
                if let Ok(store) = modsync::profiles::Store::from_settings_directory(
                    app.explicit_settings.as_deref(),
                ) {
                    app.nexus_api_key = store.nexus_api_key().ok().flatten().unwrap_or_default();
                }
            }
        }
        if !app.input.is_empty() {
            app.load();
        }
        app
    }

    fn reset_installation(&mut self) {
        self.guide_generation = self.guide_generation.wrapping_add(1);
        self.completed_base = None;
        self.pending_base.clear();
        self.install_context = None;
        self.confirm_install = false;
        self.finished = false;
        self.phase_failed = false;
        self.validated = false;
        self.conflicts = None;
        validation_page::clear_report(&mut self.validation_report, &mut self.validation_page);
        self.installing_page.reset();
        self.completion = completion_pages::State::default();
    }

    fn finish_wizard(&mut self) {
        self.wizard = None;
        self.utility = false;
        self.page = 0;
        self.confirm_install = false;
    }

    fn sharing_before(&mut self, action: SharingContinuation) -> bool {
        if self.sharing.mutation() {
            return true;
        }
        if self.sharing_continuation.is_none() {
            self.sharing_continuation = Some(action);
        }
        false
    }

    fn load(&mut self) {
        if !self.sharing_before(SharingContinuation::Load) {
            return;
        }
        self.reset_installation();
        match Guide::load_for_install_preserving_selection(Path::new(&self.input)) {
            Ok(g) => {
                self.logs
                    .push(format!("Loaded {} components", g.components.len()));
                self.loaded_input = Some(PathBuf::from(&self.input));
                self.downloads.unsaved_records = false;
                self.wizard = (!g.components.is_empty()).then(|| Wizard::new(&g));
                self.utility = false;
                self.guide = Some(g);
                // The lifecycle poll records guide load after its ready receipt.
                self.selected = 0;
                self.tier_filter.clear();
                self.category_filter.clear();
                self.category_selection = None;
                self.start_pending_nxm_if_ready();
            }
            Err(e) => {
                self.loaded_input = None;
                self.guide = None;
                self.wizard = None;
                self.logs.push(format!("Load failed: {e:#}"));
            }
        }
    }

    fn start_pending_nxm_if_ready(&mut self) {
        if self.sharing.closing || self.desktop.busy() || self.sharing_continuation.is_some() {
            return;
        }
        if self.guide.is_some()
            && !self.mods.trim().is_empty()
            && self.receiver.is_none()
            && !self.fomod_pending()
            && !self.peer_view.saving
            && !self.pending_nxm.is_empty()
        {
            self.download_pending_nxm();
        }
    }

    /// Dispatches the oldest queued nxm:// link through the shared path: re-resolved
    /// against the guide bound now, nxm_target, a single Peer-off plan, run and record.
    fn download_pending_nxm(&mut self) {
        if self.sharing.closing || self.sharing_continuation.is_some() {
            return;
        }
        if self.receiver.is_some()
            || self.desktop.busy()
            || self.fomod_pending()
            || self.peer_view.saving
        {
            return;
        }
        let Some(raw) = self.pending_nxm.pop_front() else {
            return;
        };
        self.refresh_input_stamp();
        let prepared = (|| -> Result<_> {
            let link = modsync::protocol::parse(&raw)?;
            let archive = self.archive_options()?;
            let binding = self.acquisition_binding()?;
            let target = modsync::download_orchestrator::nxm_target(&binding.guide, &link)?;
            let plan = modsync::download_orchestrator::AcquisitionPlan::single(
                target,
                &binding.source_dir,
            )?;
            Ok((plan, archive, binding))
        })();
        let (plan, archive, binding) = match prepared {
            Ok(value) => value,
            Err(error) => {
                let text = format!("{error:#}");
                self.logs.push(format!(
                    "Nexus handoff rejected: {}",
                    self.downloads.redact(&text)
                ));
                return;
            }
        };
        let key = self.acquisition_key();
        let (api_base, loopback_ca) = self.probe.as_ref().map_or((None, None), |probe| {
            (probe.nexus_api_base.clone(), probe.loopback_ca.clone())
        });
        self.invalidate_validation();
        let cancelled = Arc::new(AtomicBool::new(false));
        self.cancelled = cancelled.clone();
        let mut secrets = self.acquisition_secrets();
        secrets.push(raw);
        let run = self.downloads.begin(
            downloads_panel::Kind::Nxm,
            binding,
            cancelled.clone(),
            &secrets,
        );
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.job = Job::NxmDownload;
        self.logs.push("Downloading linked Nexus file…".into());
        let source_guide = self.guide.clone();
        let repaint = self.repaint.clone();
        std::thread::spawn(move || {
            use modsync::download_orchestrator as orchestrator;
            let send = |update| {
                let _ = tx.send(Message::Acquisition(run, update));
                if let Some(ctx) = &repaint {
                    ctx.request_repaint();
                }
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || -> Result<Box<downloads_panel::Finished>> {
                    send(downloads_panel::Update::Prepared {
                        plan: Box::new(plan.clone()),
                        selected_components: 0,
                        secrets: Vec::new(),
                    });
                    // As the CLI's `nxm-download --input`: one worker, the plan's Peer Off.
                    let settings = orchestrator::RunSettings {
                        workers: 1,
                        archive,
                        nexus_api_key: key,
                        nexus_api_base: api_base,
                        ..Default::default()
                    };
                    let report = orchestrator::run_with_source_context(
                        &plan,
                        &settings,
                        source_guide.as_ref().ok_or_else(|| {
                            anyhow::anyhow!("Load a guide before acquiring its linked resource")
                        })?,
                        &cancelled,
                        |event| send(downloads_panel::Update::Event(Box::new(event))),
                        loopback_ca.as_ref(),
                    )?;
                    let metadata = orchestrator::collect_metadata(
                        &plan,
                        &report,
                        &settings,
                        &cancelled,
                        loopback_ca.as_ref(),
                    );
                    Ok(Box::new(downloads_panel::Finished {
                        report,
                        metadata,
                        fomod: None,
                    }))
                },
            ))
            .unwrap_or_else(|_| Err(anyhow::anyhow!("The download worker stopped unexpectedly.")));
            send(downloads_panel::Update::Terminal(result));
        });
    }

    /// Marks one resolved registry file downloaded in the current guide (test surface of
    /// the pre-orchestrator NXM completion path).
    #[cfg(test)]
    fn record_nxm_download(
        &mut self,
        matched: modsync::protocol::NxmComponentMatch,
        file_name: &str,
    ) -> Result<()> {
        let component = self
            .guide
            .as_mut()
            .and_then(|guide| {
                guide
                    .components
                    .iter_mut()
                    .find(|component| component.guid == matched.component_guid)
            })
            .ok_or_else(|| anyhow::anyhow!("Resolved component changed before download"))?;
        modsync::updates::record_nxm_download(component, &matched.registry_url, file_name)
    }

    fn apply_category_selection(&mut self) {
        if !self.sharing.mutation() {
            return;
        }
        let Some(categories) = self.category_selection.take() else {
            return;
        };
        let selected: Vec<_> = categories
            .into_iter()
            .filter_map(|(name, selected)| selected.then_some(name))
            .collect();
        if selected.is_empty() {
            return;
        }
        self.start_selection(Some(modsync::selection::Action::SelectCategories(selected)));
    }

    fn sync_target_presentation(&mut self, ctx: &egui::Context) {
        let fallback = self.desktop.settings.target_game();
        let mut theme = self.desktop.settings.effective_theme().to_owned();
        if let Some(guide) = &self.guide {
            let target = modsync::wizard::presentation_target(guide, fallback);
            theme = match target.as_deref() {
                Some("K1" | "KOTOR1") => "/Styles/KotorStyle.axaml".into(),
                Some("TSL" | "KOTOR2") => "/Styles/Kotor2Style.axaml".into(),
                Some(_) => "/Styles/LightStyle.axaml".into(),
                None => theme,
            };
            if let Some(wizard) = &mut self.wizard {
                wizard.apply_target_context(guide, fallback);
            }
        }
        if self.theme != theme {
            self.theme = theme;
            self.apply_theme(ctx);
        }
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        let path = self.theme.to_ascii_lowercase();
        let mut visuals = egui::Visuals::light();
        visuals.panel_fill = egui::Color32::WHITE;
        visuals.override_text_color = Some(egui::Color32::BLACK);
        if !path.contains("lightstyle") && path.contains("kotor") {
            visuals = egui::Visuals::dark();
            let accent = if path.contains("kotor2") {
                egui::Color32::from_rgb(0x18, 0xae, 0x88)
            } else {
                egui::Color32::from_rgb(0x3a, 0xaa, 0xff)
            };
            visuals.panel_fill = if path.contains("kotor2") {
                egui::Color32::from_rgb(2, 17, 13)
            } else {
                egui::Color32::from_rgb(0, 1, 22)
            };
            visuals.override_text_color = Some(accent);
            visuals.hyperlink_color = accent;
            visuals.selection.stroke.color = accent;
            visuals.widgets.active.fg_stroke.color = accent;
            visuals.widgets.hovered.fg_stroke.color = accent;
        }
        ctx.set_visuals(visuals);
    }

    #[allow(dead_code)] // Retained test-facing synchronous helper; desktop saves use the owned worker.
    fn change_theme(&mut self, theme: &str, ctx: &egui::Context) -> Result<()> {
        anyhow::ensure!(
            !self.profile_directory.is_empty(),
            "Choose a settings directory"
        );
        modsync::profiles::Store::new(Path::new(&self.profile_directory))?
            .save_theme_preference(theme)?;
        self.theme = theme.into();
        self.apply_theme(ctx);
        Ok(())
    }

    #[allow(dead_code)] // Retained for the frozen legacy settings API callers.
    fn restore_directory_preferences(&mut self) -> Result<()> {
        anyhow::ensure!(
            !self.profile_directory.is_empty(),
            "Settings directory unavailable"
        );
        let store = modsync::profiles::Store::new(Path::new(&self.profile_directory))?;
        let (source, destination) = store.directory_preferences()?;
        if self.mods.is_empty() {
            self.mods = source.unwrap_or_default();
        }
        if self.game.is_empty() {
            self.game = destination.unwrap_or_default();
        }
        self.validated = false;
        self.conflicts = None;
        validation_page::clear_report(&mut self.validation_report, &mut self.validation_page);
        self.confirm_install = false;
        Ok(())
    }

    fn profile_action(&mut self, activate: bool) -> Result<()> {
        anyhow::ensure!(
            !self.profile_directory.trim().is_empty(),
            "Choose a profile settings directory"
        );
        let store = modsync::profiles::Store::new(Path::new(&self.profile_directory))?;
        let guide = self
            .guide
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Load a guide first"))?;
        if activate {
            // Persist activation before changing the displayed install state.
            let profile = store.activate(&self.profile_name)?;
            self.guide_generation = self.guide_generation.wrapping_add(1);
            let mut guide = guide.clone();
            profile.apply(&mut guide);
            if let Some(game) = profile.kotor_directory.filter(|p| !p.is_empty()) {
                self.game = game;
            }
            if let Some(mods) = profile.mod_directory.filter(|p| !p.is_empty()) {
                self.mods = mods;
            }
            self.guide = Some(guide);
            self.validated = false;
            self.conflicts = None;
            validation_page::clear_report(&mut self.validation_report, &mut self.validation_page);
            self.confirm_install = false;
        } else {
            store.capture(
                &self.profile_name,
                guide,
                self.loaded_input.as_deref(),
                (!self.game.is_empty()).then(|| Path::new(&self.game)),
                (!self.mods.is_empty()).then(|| Path::new(&self.mods)),
            )?;
        }
        Ok(())
    }

    fn manage_profile(&mut self, action: &str) -> Result<()> {
        anyhow::ensure!(
            !self.profile_directory.trim().is_empty(),
            "Choose a profile settings directory"
        );
        let store = modsync::profiles::Store::new(Path::new(&self.profile_directory))?;
        match action {
            "Clone" => {
                self.profile_name = store
                    .clone_profile(&self.profile_name, &self.profile_target)?
                    .name;
            }
            "Rename" => {
                self.profile_name = store.rename(&self.profile_name, &self.profile_target)?.name;
            }
            "Delete" => {
                anyhow::ensure!(
                    self.profile_delete.as_ref()
                        == Some(&(self.profile_directory.clone(), self.profile_name.clone())),
                    "Confirm this profile deletion first"
                );
                store.delete_and_deactivate(&self.profile_name)?;
                self.profile_delete = None;
                self.profile_name.clear();
            }
            "Refresh" => {}
            _ => anyhow::bail!("Unknown profile action"),
        }
        self.profile_names = store.list()?.into_iter().map(|p| p.name).collect();
        Ok(())
    }

    fn refresh_active_profile_binding(&mut self) -> Result<()> {
        let active = match modsync::profiles::Store::new(Path::new(&self.profile_directory))
            .and_then(|store| store.active_name())
        {
            Ok(active) => active,
            Err(error) => {
                self.begin_desktop_load(Some(PathBuf::from(&self.profile_directory)));
                return Err(error);
            }
        };
        self.active_profile_name = active.clone().unwrap_or_default();
        self.desktop.settings.active_profile_name = active;
        self.invalidate_validation();
        Ok(())
    }

    fn profile_ui_action(&mut self, activate: bool) -> Result<()> {
        let result = self.profile_action(activate);
        self.refresh_active_profile_binding()?;
        result
    }

    fn manage_profile_ui(&mut self, action: &str) -> Result<()> {
        let result = self.manage_profile(action);
        self.refresh_active_profile_binding()?;
        result
    }

    fn profiles(&mut self, ui: &mut egui::Ui) {
        if !self.desktop.preferences_ready(&self.profile_directory) || self.desktop.busy() {
            ui.label("Wait for desktop preferences to finish loading or saving.");
            return;
        }
        if self.peer_view.receiver.is_some() || !self.peer_view.queued.is_empty() {
            ui.label("Wait for peer settings to finish saving or loading.");
            return;
        }
        if !self.sharing.edit_gate(ui) {
            return;
        }
        ui.collapsing("Install profiles", |ui| {
            if Self::path_row(ui, "Settings directory", &mut self.profile_directory, true) {
                self.profile_names.clear();
                self.profile_delete = None;
                self.nexus_api_key = modsync::profiles::Store::new(Path::new(&self.profile_directory))
                    .and_then(|store| store.nexus_api_key())
                    .ok()
                    .flatten()
                    .unwrap_or_default();
            }
            ui.horizontal(|ui| {
                if ui.button("Refresh profiles").clicked() && let Err(e) = self.manage_profile_ui("Refresh") {
                    self.logs.push(format!("Profile failed: {e:#}"));
                }
                egui::ComboBox::from_id_salt("stored_profiles").selected_text("Choose stored profile").show_ui(ui, |ui| {
                    for name in &self.profile_names { ui.selectable_value(&mut self.profile_name, name.clone(), name); }
                });
            });
            ui.horizontal(|ui| {
                ui.label("Profile name");
                ui.text_edit_singleline(&mut self.profile_name);
            });
            ui.horizontal(|ui| {
                ui.label("New name for clone or rename");
                ui.text_edit_singleline(&mut self.profile_target);
                for action in ["Clone", "Rename"] {
                    if ui.button(action).clicked() && let Err(e) = self.manage_profile_ui(action) {
                        self.logs.push(format!("Profile failed: {e:#}"));
                    }
                }
                if ui.button("Delete profile…").clicked() {
                    self.profile_delete = Some((self.profile_directory.clone(), self.profile_name.clone()));
                }
            });
            if self.profile_delete.as_ref() == Some(&(self.profile_directory.clone(), self.profile_name.clone())) {
                ui.label(format!("Delete '{}'? This cannot be undone. Its active setting will be cleared; installed files are unchanged.", self.profile_name));
                ui.horizontal(|ui| {
                    if ui.button("Confirm delete").clicked() && let Err(e) = self.manage_profile_ui("Delete") {
                        self.logs.push(format!("Profile failed: {e:#}"));
                    }
                    if ui.button("Cancel deletion").clicked() { self.profile_delete = None; }
                });
            }
            ui.label("Activation applies selections to the loaded guide and restores stored game/mod directories.");
            ui.horizontal(|ui| {
                for (label, activate) in [("Save current profile", false), ("Activate profile", true)] {
                    if ui.add_enabled(self.guide.is_some(), egui::Button::new(label)).clicked() {
                        let result = self.profile_ui_action(activate);
                        self.logs.push(match result {
                            Ok(()) => format!("Profile '{}' {}.", self.profile_name, if activate { "activated" } else { "saved" }),
                            Err(e) => format!("Profile failed: {e:#}"),
                        });
                    }
                }
            });
        });
    }

    fn phase_guide(&self) -> Option<Guide> {
        self.prepare_phase(Job::Install, None).ok().map(|p| p.guide)
    }

    fn prepare_phase(
        &self,
        job: Job,
        choices: Option<&modsync::fomod::ChoicesFile>,
    ) -> Result<modsync::phase::PreparedPhase> {
        use modsync::phase::{CompletionContext, Phase};
        let guide = self
            .guide
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Load a guide first"))?;
        let (phase, context) = match self.completed_base.as_deref() {
            _ if self.checkpoints.resume.is_some() => (Phase::All, CompletionContext::None),
            Some(ids) => (Phase::Remaining, CompletionContext::Retained { ids }),
            None if job == Job::Validate => (Phase::All, CompletionContext::None),
            None => (Phase::Base, CompletionContext::None),
        };
        modsync::phase::prepare(guide, phase, context, choices)
    }

    fn installed_phase(&mut self) -> Result<()> {
        // Keep the exact effective worker definitions, not a reconstruction from
        // an external choices file that may already have changed. A replacement
        // guide/reset or changed roots must never receive the old completion.
        let context = self.install_context.take();
        anyhow::ensure!(
            context
                == Some((
                    serde_json::to_string(&self.guide)?,
                    self.game.clone(),
                    self.mods.clone()
                )),
            "Install completed for a previous guide or directories; completion was not applied to the current project"
        );
        if self.completed_base.is_none() {
            let guide = self
                .guide
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("Loaded guide is missing"))?;
            for effective in &self.pending_base {
                let component = guide
                    .components
                    .iter_mut()
                    .find(|c| c.guid == effective.guid)
                    .ok_or_else(|| {
                        anyhow::anyhow!("Completed component is missing from loaded guide")
                    })?;
                *component = effective.clone();
            }
            self.completed_base = Some(self.pending_base.iter().map(|c| c.guid.clone()).collect());
            self.pending_base.clear();
        } else {
            self.finished = true;
        }
        Ok(())
    }

    fn saved_fomod_choices(&self, path: &Path) -> Result<Option<modsync::fomod::ArchiveChoices>> {
        let mut sessions = self.saved_fomod_sessions(path)?;
        anyhow::ensure!(
            sessions.len() <= 1,
            "Multiple bound FOMOD sessions require normalized editor preparation"
        );
        Ok(sessions.pop())
    }

    fn saved_fomod_sessions(&self, path: &Path) -> Result<Vec<modsync::fomod::ArchiveChoices>> {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid archive filename"))?;
        let mut sessions = Vec::new();
        if let Some(guide) = &self.guide {
            for guid in self.fomod_bound_guids()? {
                let component = guide
                    .components
                    .iter()
                    .find(|c| c.guid == guid)
                    .ok_or_else(|| anyhow::anyhow!("FOMOD consumer is missing"))?;
                if let Some(choices) = modsync::fomod::saved_choices_file(component)?
                    .and_then(|file| file.archive(filename).cloned())
                {
                    sessions.push(choices);
                }
            }
        }
        Ok(sessions)
    }

    fn fomod_bound_guids(&self) -> Result<Vec<String>> {
        let guide = self
            .guide
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Load a guide first"))?;
        let guids = if let Some(item) = &self.fomod_active {
            item.consumer_guids.clone()
        } else {
            vec![
                guide
                    .components
                    .get(self.selected)
                    .ok_or_else(|| anyhow::anyhow!("Select a FOMOD component"))?
                    .guid
                    .clone(),
            ]
        };
        anyhow::ensure!(!guids.is_empty(), "FOMOD session has no bound consumers");
        for guid in &guids {
            anyhow::ensure!(
                guide
                    .components
                    .iter()
                    .filter(|c| c.guid == *guid && c.is_selected)
                    .count()
                    == 1,
                "FOMOD consumer is missing, unselected or ambiguous"
            );
            anyhow::ensure!(
                !self
                    .completed_base
                    .as_ref()
                    .is_some_and(|ids| ids.iter().any(|id| id.eq_ignore_ascii_case(guid))),
                "Completed FOMOD consumer cannot be edited"
            );
        }
        Ok(guids)
    }

    fn should_offer_fomod_editor(&self, path: &Path) -> bool {
        let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
            return true;
        };
        fn visit(component: &Component, filename: &str) -> (bool, bool) {
            let matches = modsync::fomod::resource_matches_archive(component, filename);
            let pending = matches && modsync::fomod::should_prompt(component, filename);
            component
                .options
                .iter()
                .fold((matches, pending), |(found, offer), option| {
                    let (child_found, child_offer) = visit(option, filename);
                    (found || child_found, offer || child_offer)
                })
        }
        let Some(guide) = &self.guide else {
            return true;
        };
        let (matched, pending) =
            guide
                .components
                .iter()
                .fold((false, false), |(found, offer), component| {
                    let (component_found, component_offer) = visit(component, filename);
                    (found || component_found, offer || component_offer)
                });
        !matched || pending
    }

    fn apply_fomod_choices(&mut self, choices: &modsync::fomod::ChoicesFile) -> Result<()> {
        let mut guide = self
            .guide
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Load a guide first"))?;
        let guids = self.fomod_bound_guids()?;
        anyhow::ensure!(
            choices.archives.len() == 1,
            "A bound editor must return exactly one archive session"
        );
        let edit = &choices.archives[0];
        if let Some(item) = &self.fomod_active {
            let filename = item
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| anyhow::anyhow!("Invalid archive filename"))?;
            anyhow::ensure!(
                modsync::fomod::eq_ignore_case(&edit.archive_file_name, filename),
                "Editor archive differs from the captured binding"
            );
        }
        let imported = if !self.fomod_choices.trim().is_empty() {
            let imported = self.fomod_import.as_ref().ok_or_else(|| {
                anyhow::anyhow!("External choices were not prepared; reopen the FOMOD editor")
            })?;
            imported.apply_to_remaining(
                &mut guide,
                self.completed_base.as_deref().unwrap_or_default(),
            )?;
            true
        } else {
            false
        };
        modsync::fomod::apply_bound_archive(
            &mut guide,
            &guids,
            &edit.archive_file_name,
            Some(edit),
            modsync::fomod::PromptStatus::Configured,
            self.completed_base.as_deref().unwrap_or_default(),
        )?;
        if imported {
            self.logs.push("Imported the selected external choices file; editor changes apply only to its bound components.".into());
        }
        self.guide = Some(guide);
        self.fomod_choices.clear();
        self.validated = false;
        self.conflicts = None;
        validation_page::clear_report(&mut self.validation_report, &mut self.validation_page);
        self.confirm_install = false;
        self.logs
            .push("FOMOD choices applied. Run validation before installing.".into());
        Ok(())
    }

    #[allow(dead_code)] // Retained for the frozen compatibility callers.
    fn save_selection(&self, path: &Path) -> Result<()> {
        let mut guide = self
            .guide
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Load a guide first"))?;
        if !self.fomod_choices.trim().is_empty() {
            modsync::fomod::ChoicesFile::parse(&std::fs::read_to_string(&self.fomod_choices)?)?
                .apply_to_remaining(
                    &mut guide,
                    self.completed_base.as_deref().unwrap_or_default(),
                )?;
        }
        guide.save(path)
    }

    /// End an install run that failed before the engine was dispatched.
    /// Pending recovery is read from this run's roots when they exist.
    fn install_not_started(
        &mut self,
        run: u64,
        error: anyhow::Error,
        roots: Option<&modsync::paths::Roots>,
    ) {
        self.checkpoints.resume = None;
        // Without roots nothing can be read for this run: report no pending
        // state for it, and keep the app-level state from earlier runs.
        let earlier = self.phase_failed;
        let pending =
            roots.is_some_and(|roots| recovery::state_dir(roots).join("pending").exists());
        self.job = Job::Install;
        self.handle_message(Message::InstallTerminal(run, Err(error), pending, None));
        if roots.is_none() {
            self.phase_failed = earlier;
        }
    }

    /// Restore the snapshot of the failed install shown on the page, using the
    /// roots captured for that run rather than the editable folder paths.
    fn restore_captured(&mut self, ctx: &egui::Context) {
        if !self.sharing_before(SharingContinuation::Restore) {
            return;
        }
        let Some(roots) = self.installing_page.restore_target().cloned() else {
            return;
        };
        if self.receiver.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.job = Job::Restore;
        self.restore_roots = Some(roots.clone());
        self.logs
            .push(format!("Restoring snapshot for {}…", roots.game.display()));
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result =
                std::panic::catch_unwind(|| recovery::restore(&roots)).unwrap_or_else(|_| {
                    Err(anyhow::anyhow!(
                        "Worker stopped unexpectedly. Inspect recovery state before retrying."
                    ))
                });
            let _ = tx.send(Message::Done(result));
            ctx.request_repaint();
        });
    }

    fn start(&mut self, job: Job, ctx: &egui::Context) {
        if job == Job::Install
            && self.checkpoints.config.managed
            && (!self.desktop.preferences_ready(&self.profile_directory) || self.desktop.busy())
        {
            self.logs
                .push("Wait for active profile preferences to finish loading or saving.".into());
            return;
        }
        if matches!(job, Job::Install | Job::Restore | Job::Update)
            && !self.sharing_before(SharingContinuation::Start(job))
        {
            return;
        }
        if self.receiver.is_some() || self.desktop.busy() || self.fomod_pending() {
            self.logs
                .push("Wait for the current operation and FOMOD choices to finish.".into());
            return;
        }
        // Every Restore entry point uses a failed install's captured roots while
        // one is pending. Live-path restore is only for other recovery.
        if job == Job::Restore && self.installing_page.restore_target().is_some() {
            self.restore_captured(ctx);
            return;
        }
        self.refresh_input_stamp();
        if job == Job::Conflicts {
            self.tools.conflict_context = Some(self.input_stamp.clone());
            self.tools.conflict_error = None;
            self.tools.conflicts_open = true;
        }
        if job == Job::Install
            && self.checkpoints.resume.is_none()
            && (!self.validated
                || !self.validation_report.as_ref().is_some_and(|r| r.success)
                || self.validation_input_stamp.as_ref() != Some(&self.input_stamp))
        {
            self.logs.push(
                "Installation blocked: run successful validation for the current guide and folders first."
                    .into(),
            );
            return;
        }
        // Bind this run to the exact inputs before anything can fail.
        let install_run =
            (job == Job::Install).then(|| self.installing_page.begin(&self.input_stamp));
        if install_run.is_some() {
            self.installing_page.bind_folders(&self.mods, &self.game);
        }
        self.validation_input_stamp = (job == Job::Validate).then(|| self.input_stamp.clone());
        self.validated = false;
        self.conflicts = None;
        validation_page::clear_report(&mut self.validation_report, &mut self.validation_page);
        if job == Job::Validate {
            self.validation_page.begin(
                self.guide
                    .as_ref()
                    .map_or(0, |g| g.components.iter().filter(|c| c.is_selected).count()),
            );
        }
        let validation_run = self.validation_page.run;
        self.confirm_install = false;
        let settings = Settings::new(Path::new(&self.mods), Path::new(&self.game));
        let mut settings = match settings {
            Ok(s) => s,
            Err(e) => {
                if let Some(run) = install_run {
                    self.install_not_started(run, e.context("Check directories"), None);
                    return;
                }
                if job == Job::Validate {
                    self.validation_page
                        .fail(&format!("Check directories: {e:#}"));
                }
                self.logs.push(format!("Check directories: {e:#}"));
                return;
            }
        };
        if install_run.is_some() {
            self.installing_page.bind_roots(&settings.roots);
        }
        if job == Job::Restore {
            self.restore_roots = Some(settings.roots.clone());
        }
        settings.allow_processes = self.allow_processes;
        settings.patcher_engine = self.patcher_engine;
        settings.patcher = if self.patcher.is_empty() {
            None
        } else {
            Some(PathBuf::from(&self.patcher))
        };
        settings.archive.scratch_dir = if self.scratch.is_empty() {
            None
        } else {
            Some(PathBuf::from(&self.scratch))
        };
        settings.archive.max_expanded_bytes = if self.expanded_budget.trim().is_empty() {
            None
        } else {
            match self.expanded_budget.trim().parse() {
                Ok(value) => Some(value),
                Err(_) => {
                    if let Some(run) = install_run {
                        self.install_not_started(
                            run,
                            anyhow::anyhow!("Expanded-byte budget must be a nonnegative integer."),
                            Some(&settings.roots),
                        );
                        return;
                    }
                    if job == Job::Validate {
                        self.validation_page
                            .fail("Expanded-byte budget must be a nonnegative integer.");
                    }
                    self.logs
                        .push("Expanded-byte budget must be a nonnegative integer.".into());
                    return;
                }
            }
        };
        self.cancelled = settings.cancelled.clone();
        let (guide, completed) = if matches!(job, Job::Validate | Job::Install | Job::Conflicts) {
            // Paint-time phase projection is pure. Archive attribution and the
            // external choices file are resolved by the captured worker below.
            match self.prepare_phase(job, None) {
                Ok(prepared) => {
                    if install_run.is_some() {
                        // Directions come from the exact guide handed to the engine.
                        self.installing_page.prepared(&prepared.guide);
                    }
                    (
                        Some(prepared.guide),
                        self.completed_base.as_ref().map(|_| prepared.completed),
                    )
                }
                Err(error) => {
                    if let Some(run) = install_run {
                        self.install_not_started(run, error, Some(&settings.roots));
                        return;
                    }
                    self.job = job;
                    self.handle_message(Message::Done(Err(error)));
                    return;
                }
            }
        } else {
            (self.guide.clone(), self.completed_base.clone())
        };
        if job == Job::Install {
            self.install_context = Some((
                serde_json::to_string(&self.guide).expect("guide serializes"),
                self.game.clone(),
                self.mods.clone(),
            ));
        }
        if job == Job::Install && completed.is_none() {
            self.pending_base = guide
                .as_ref()
                .and_then(|g| g.ordered().ok())
                .unwrap_or_default()
                .iter()
                .map(|c| (**c).clone())
                .collect();
        }
        let url = self.download_url.clone();
        let filename = self.download_name.clone();
        let hash = self.download_hash.clone();
        // A direct download gets a run identity too; its FOMOD follow-up is bound to it.
        let direct_run = (job == Job::Download).then(|| {
            let run = self.downloads.direct_run();
            let consumers = self
                .guide
                .as_ref()
                .map(|guide| {
                    guide
                        .components
                        .iter()
                        .filter(|component| {
                            component.is_selected
                                && !self.completed_base.as_ref().is_some_and(|ids| {
                                    ids.iter()
                                        .any(|id| id.eq_ignore_ascii_case(&component.guid))
                                })
                                && modsync::fomod::should_prompt(component, &filename)
                                && modsync::fomod::saved_choices_file(component).is_ok_and(
                                    |saved| {
                                        saved
                                            .as_ref()
                                            .and_then(|file| file.archive(&filename))
                                            .is_none()
                                    },
                                )
                                && component.resource_registry.values().any(|resource| {
                                    resource
                                        .get("Files")
                                        .or_else(|| resource.get("files"))
                                        .and_then(serde_json::Value::as_object)
                                        .is_some_and(|files| {
                                            files.keys().any(|name| name.trim() == filename)
                                        })
                                })
                        })
                        .map(|component| component.guid.clone())
                        .collect()
                })
                .unwrap_or_default();
            self.direct_download = Some((run, self.acquisition_context(), consumers));
            run
        });
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.job = job;
        self.logs.push(
            match job {
                Job::Checkpoint => "Checkpoint operation…",
                Job::UpdateSummary => "Reading update result…",
                Job::Editor => "Editor operation…",
                Job::Conflicts => "Analyzing file conflicts…",
                Job::Validate => "Validating…",
                Job::Install => "Installing…",
                Job::Restore => "Restoring snapshot…",
                Job::Download => "Downloading…",
                Job::NxmDownload | Job::Fetch => {
                    unreachable!("acquisition runs start in their own entry points")
                }
                Job::ProtocolOpen => "Downloading ModSync build link…",
                Job::Update => "Checking Nexus updates…",
            }
            .into(),
        );
        let ctx = ctx.clone();
        // Captured before the engine takes ownership of settings.
        let install_roots = settings.roots.clone();
        // The completed prefix the engine is given; the receipt minus it is this phase.
        let prefix = completed.as_ref().map_or(0, Vec::len);
        let resume = if job == Job::Install {
            self.checkpoints.resume.take()
        } else {
            None
        };
        if resume.is_some() {
            self.checkpoints.resumed_run = install_run;
        }
        let checkpoint_config = self.checkpoints.config.clone();
        let managed_settings = self.profile_directory.clone();
        let managed_profile = self.active_profile_name.clone();
        let case_insensitive_pathing = self.desktop.settings.case_insensitive_pathing;
        let preparation_guide = self.guide.clone();
        let retained_prefix = self.completed_base.clone();
        let choices_path = self.fomod_choices.clone();
        let preparation_phase = if resume.is_some() {
            modsync::phase::Phase::All
        } else if retained_prefix.is_some() {
            modsync::phase::Phase::Remaining
        } else if job == Job::Validate {
            modsync::phase::Phase::All
        } else {
            modsync::phase::Phase::Base
        };
        std::thread::spawn(move || {
            let mut install_report = None;
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
                    if job == Job::Restore {
                        return recovery::restore(&settings.roots);
                    }
                    if job == Job::Download {
                        let name = modsync::paths::relative(&filename, false)?;
                        anyhow::ensure!(
                            name.components().count() == 1,
                            "Enter one archive filename"
                        );
                        let output = settings.roots.mods.join(name);
                        let guard = modsync::acquisition_guard::Scope::acquire(
                            std::slice::from_ref(&output),
                            Some(&settings.roots.mods),
                            &settings.cancelled,
                        )?;
                        let output = guard.destinations()[0].clone();
                        download::fetch_with_options(
                            &url,
                            &output,
                            if hash.is_empty() { None } else { Some(&hash) },
                            &settings.cancelled,
                            &settings.archive,
                        )?;
                        let _ = tx.send(Message::Downloaded(
                            direct_run.unwrap_or_default(),
                            output,
                            settings.archive.clone(),
                        ));
                        return Ok(());
                    }
                    let guide = if matches!(job, Job::Validate | Job::Install | Job::Conflicts) {
                        let original = preparation_guide
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!("Load a guide first"))?;
                        let choices = if choices_path.trim().is_empty() {
                            None
                        } else {
                            Some(modsync::fomod::ChoicesFile::load(Path::new(&choices_path))?)
                        };
                        let context = if preparation_phase == modsync::phase::Phase::Remaining {
                            modsync::phase::CompletionContext::Retained {
                                ids: retained_prefix
                                    .as_deref()
                                    .ok_or_else(|| anyhow::anyhow!("Missing retained prefix"))?,
                            }
                        } else {
                            modsync::phase::CompletionContext::None
                        };
                        let prepared = modsync::phase::prepare_with_roots(
                            original,
                            preparation_phase,
                            context,
                            choices.as_ref(),
                            &settings.roots,
                            case_insensitive_pathing,
                            &settings.cancelled,
                        )?;
                        if let Some(run) = install_run {
                            let _ = tx.send(Message::InstallPrepared(
                                run,
                                Box::new(prepared.guide.clone()),
                            ));
                            ctx.request_repaint();
                        }
                        prepared.guide
                    } else {
                        guide.ok_or_else(|| anyhow::anyhow!("Load a guide first"))?
                    };
                    let emit = |e: modsync::Event| {
                        let _ = tx.send(Message::Log(format!(
                            "{} / {}: {}",
                            e.component, e.step, e.message
                        )));
                        ctx.request_repaint();
                    };
                    let report_settings = (job == Job::Validate).then(|| settings.clone());
                    let mut engine = match Engine::new_with_case_insensitive_pathing(
                        settings,
                        case_insensitive_pathing,
                    ) {
                        Ok(engine) => engine,
                        Err(error) => {
                            // As the CLI's full report: C#'s Environment stage fails
                            // for case duplicates in the game folder.
                            let Some(report) = report_settings.and_then(|settings| {
                                Engine::case_collision_report(
                                    &settings,
                                    case_insensitive_pathing,
                                    &error,
                                    &guide,
                                    completed.as_deref().unwrap_or_default(),
                                )
                            }) else {
                                return Err(error);
                            };
                            let _ = tx.send(Message::ValidationTerminal(validation_run, report));
                            anyhow::bail!("Validation pipeline reported errors");
                        }
                    };
                    if job == Job::Install {
                        let mut config = checkpoint_config;
                        if resume.is_some() {
                            config.enabled = true;
                        }
                        engine = config.engine(
                            engine,
                            &install_roots,
                            &managed_settings,
                            &managed_profile,
                        )?;
                        if let Some(request) = resume {
                            engine = engine.with_checkpoint_resume(request)?;
                        }
                    }
                    if job == Job::Conflicts {
                        let report = match completed {
                            Some(ids) => engine.analyze_conflicts_remaining(&guide, &ids)?,
                            None => engine.analyze_conflicts(&guide)?,
                        };
                        let _ = tx.send(Message::Conflicts(report));
                        return Ok(());
                    }
                    if job == Job::Validate {
                        // C# ValidatePage uses WizardFull: the same full pipeline
                        // and dry run requested by CLI --full --dry-run.
                        let observe = |progress| {
                            let _ = tx.send(Message::ValidationProgress(validation_run, progress));
                            ctx.request_repaint();
                        };
                        let report = match completed {
                            Some(ids) => engine
                                .validate_report_remaining_observed(&guide, &ids, emit, observe)?,
                            None => engine.validate_report_with_options_observed(
                                &guide, emit, true, true, false, observe,
                            ),
                        };
                        let success = report.success;
                        let _ = tx.send(Message::ValidationTerminal(validation_run, report));
                        anyhow::ensure!(success, "Validation pipeline reported errors");
                        return Ok(());
                    }
                    let observe = |progress| {
                        if let Some(run) = install_run {
                            let _ = tx.send(Message::InstallProgress(run, progress));
                            ctx.request_repaint();
                        }
                    };
                    match (job, completed) {
                        (Job::Install, Some(ids)) => {
                            install_report =
                                Some(engine.install_remaining_observed_report(
                                    &guide, &ids, emit, observe,
                                )?);
                            Ok(())
                        }
                        (Job::Install, None) => {
                            install_report =
                                Some(engine.install_observed_report(&guide, emit, observe)?);
                            Ok(())
                        }
                        (_, Some(ids)) => engine.validate_remaining(&guide, &ids, emit),
                        (_, None) => engine.validate(&guide, emit),
                    }
                }))
                .unwrap_or_else(|_| {
                    Err(anyhow::anyhow!(
                        "Worker stopped unexpectedly. Inspect recovery state before retrying."
                    ))
                });
            let _ = tx.send(match install_run {
                Some(run) => {
                    let pending = recovery::state_dir(&install_roots).join("pending").exists()
                        || modsync::checkpoint::Store::open(&install_roots)
                            .and_then(|store| {
                                Ok(!store.pending_operations()?.is_empty()
                                    || store.external_process_status()?.present)
                            })
                            .unwrap_or(true);
                    // The engine and its recovery lock are gone by now.
                    let terminal = result.is_ok().then(|| {
                        completion_pages::Terminal::read_report(
                            prefix,
                            install_roots.clone(),
                            install_report,
                        )
                    });
                    Message::InstallTerminal(run, result, pending, terminal)
                }
                None if job == Job::Validate => Message::ValidationDone(validation_run, result),
                None => Message::Done(result),
            });
            ctx.request_repaint();
        });
    }

    fn start_update_check(&mut self, ctx: &egui::Context) {
        if self.receiver.is_some() || self.fomod_pending() || self.sharing.closing {
            return;
        }
        if !self.sharing_before(SharingContinuation::Update) {
            return;
        }
        self.tools.updates_open = true;
        if self.probe.is_some() {
            self.tools.update_note = "Update checks are unavailable in a probe launch.".into();
            return;
        }
        let Some(original) = self.guide.clone() else {
            self.tools.update_note = "Load a guide first.".into();
            return;
        };
        if self.desktop.busy() || !self.desktop.preferences_ready(&self.profile_directory) {
            self.tools.update_note =
                "Wait for the current settings directory to finish loading.".into();
            return;
        }
        let explicit = self.desktop.settings_source();
        self.refresh_input_stamp();
        self.tools.update_context = Some(self.input_stamp.clone());
        let directory = PathBuf::from(&self.profile_directory);
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.job = Job::Update;
        self.cancelled = Arc::new(AtomicBool::new(false));
        let flag = self.cancelled.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
                anyhow::ensure!(!directory.as_os_str().is_empty(), "Choose a settings directory");
                let resolved = modsync::profiles::settings_directory(explicit.as_deref())?;
                anyhow::ensure!(std::path::absolute(&resolved)? == std::path::absolute(&directory)?, "Settings directory changed before update check");
                let key = modsync::profiles::Store::from_settings_directory(explicit.as_deref())?.nexus_api_key()?.filter(|key| !key.trim().is_empty()).ok_or_else(|| anyhow::anyhow!("Set nexusModsApiKey in settings.json before checking for updates"))?;
                let mut client = modsync::updates::NexusClient::new(Some(key))?;
                let mut guide = original.clone();
                let outcome = modsync::updates::check_with_cancel(&mut guide, &mut client, &flag);
                let snapshot = modsync::updates::StoredSummary::new(&original, &guide, outcome.summary)?;
                // Publication is the final cancellation boundary. A later Stop
                // cannot turn an already-published report into an unsaved result.
                let cancelled = outcome.cancelled || flag.load(Ordering::Acquire);
                let persistence_error = if cancelled { None } else { snapshot.save(&directory).err().map(|error| format!("{error:#}")) };
                let _ = tx.send(Message::Updates(Box::new(tools_panel::UpdateResult { guide, snapshot, cancelled, persistence_error })));
                anyhow::ensure!(!cancelled, "Update check cancelled; partial result retained in this window, previous saved result unchanged");
                Ok(())
            })).unwrap_or_else(|_| Err(anyhow::anyhow!("Update worker stopped unexpectedly")));
            let _ = tx.send(Message::Done(result));
            ctx.request_repaint();
        });
    }

    fn path_row(ui: &mut egui::Ui, label: &str, value: &mut String, folder: bool) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(label);
            let response = ui.add(egui::TextEdit::singleline(value).desired_width(520.0));
            changed |= response.changed();
            if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                *value = value.trim().to_owned();
                changed = true;
            }
            if folder && !value.trim().is_empty() && !Path::new(value).is_dir() {
                ui.colored_label(egui::Color32::YELLOW, "Directory not found");
            }
            if ui.button("Browse…").clicked() {
                let dialog = rfd::FileDialog::new();
                let path = if folder {
                    dialog.pick_folder()
                } else {
                    dialog.pick_file()
                };
                if let Some(p) = path {
                    *value = p.to_string_lossy().into_owned();
                    changed = true;
                }
            }
        });
        changed
    }

    fn options(
        ui: &mut egui::Ui,
        component: &Component,
        spoiler_free: bool,
        availability: Option<&modsync::selection::Availability>,
    ) -> Option<(String, bool)> {
        let mut changed = None;
        for option in &component.options {
            let name = modsync::selection::display_name(option, spoiler_free);
            let reason = availability.and_then(|available| available.reason(&option.guid));
            let mut selected = option.is_selected;
            if ui
                .add_enabled(reason.is_none(), egui::Checkbox::new(&mut selected, name))
                .on_hover_text(
                    reason
                        .map(str::to_owned)
                        .unwrap_or_else(|| modsync::selection::tooltip(option, spoiler_free)),
                )
                .changed()
            {
                changed = Some((option.guid.clone(), selected));
            }
            if let Some(reason) = reason {
                ui.small(reason);
            }
            let description = modsync::selection::display_description(option, spoiler_free);
            if !description.trim().is_empty() {
                ui.label(description);
            }
            if option.is_selected {
                changed = Self::options(ui, option, spoiler_free, availability).or(changed);
            }
        }
        changed
    }

    fn edit_lines(ui: &mut egui::Ui, label: &str, values: &mut Vec<String>) {
        ui.label(label);
        let mut text = values.join("\n");
        if ui
            .add(egui::TextEdit::multiline(&mut text).desired_rows(2))
            .changed()
        {
            *values = text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect();
        }
    }

    fn guide_editor(&mut self, ui: &mut egui::Ui) {
        self.editor_content(ui);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // The footer can open this modal during the current input frame. Its
        // opening click is outside the modal and must not dismiss it immediately.
        let install_confirmation_was_open = self.confirm_install;
        self.poll_resource_watch();
        self.poll_selection();
        self.poll_protocol_inbox(ctx);
        let messages: Vec<_> = self
            .receiver
            .as_ref()
            .map(|r| r.try_iter().collect())
            .unwrap_or_default();
        for message in messages {
            self.handle_message(message);
        }
        self.poll_protocol_worker();
        self.poll_editor_worker();
        self.poll_easy_resume(ctx);
        self.poll_easy_prediction(ctx);
        self.poll_shell_worker();
        self.poll_standalone();
        self.poll_maintenance();
        if self.logs.len() > 5000 {
            let removed = self.logs.len() - 5000;
            self.logs.drain(..removed);
            self.desktop.logged = self.desktop.logged.saturating_sub(removed);
        }
        self.refresh_peer_presentation();
        let sharing_context = self.acquisition_context();
        self.sharing
            .poll(&self.profile_directory, &sharing_context, &mut self.logs);
        if self.sharing_continuation.is_some() && !self.sharing.closing {
            self.sharing.mutation();
        }
        if self.sharing.ready()
            && self.receiver.is_none()
            && !self.sharing.closing
            && let Some(action) = self.sharing_continuation.take()
        {
            match action {
                SharingContinuation::Maintenance(request) => self.maintenance_start(request, ctx),
                SharingContinuation::Standalone(request) => self.standalone_start(request, ctx),
                SharingContinuation::Shell(work, context) => {
                    self.shell_work(work, Some(context), ctx)
                }
                SharingContinuation::Deployment(action, context) => {
                    self.deployment_start(Some(action), Some(context), ctx)
                }
                SharingContinuation::MergeSave(context) => {
                    self.merge_save_after_barrier(context, ctx)
                }
                SharingContinuation::Checkpoint(action, context) => {
                    self.checkpoint_start(action, context, ctx)
                }
                SharingContinuation::Start(job) => self.start(job, ctx),
                SharingContinuation::Restore => self.restore_captured(ctx),
                SharingContinuation::Load => self.load(),
                SharingContinuation::Update => self.start_update_check(ctx),
                SharingContinuation::Protocol(ready) => self.apply_protocol_ready(*ready),
                SharingContinuation::Terminal(run, result) => {
                    self.acquisition_terminal(run, result)
                }
            }
        }
        self.poll_desktop(ctx);
        self.rehydrate_update_summary(ctx);
        self.sync_target_presentation(ctx);
        if self.sharing.closing
            && self.resource_watch.closed()
            && !self.selection_panel.busy()
            && self.standalone.closed()
            && self.maintenance.closed()
            && self.protocol_load.closed()
            && self.editor.settled()
            && self.shell.settled()
            && self.desktop.closed()
            && self.receiver.is_none()
            && self.sharing.closed()
            && self.peer_view.receiver.is_none()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.shell_ui(ctx);
        self.desktop_window(ctx);
        self.merge_window(ctx);
        self.deployment_window(ctx);
        self.standalone_window(ctx);
        self.maintenance_window(ctx);
        self.tools_windows(ctx);
        self.checkpoint_window(ctx);
        self.show_fomod(ctx);
        ctx.request_repaint_after(Duration::from_millis(100));
        if self.peer_view.receiver.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        let busy = self.receiver.is_some()
            || self.selection_panel.busy()
            || self.desktop.busy()
            || self.sharing.closing
            || self.sharing_continuation.is_some();
        if ctx.input(|i| i.viewport().close_requested())
            && !(self.sharing.closing
                && self.resource_watch.closed()
                && !self.selection_panel.busy()
                && self.standalone.closed()
                && self.maintenance.closed()
                && self.protocol_load.closed()
                && self.editor.settled()
                && self.shell.settled()
                && self.desktop.closed()
                && self.sharing.closed()
                && self.receiver.is_none()
                && self.peer_view.receiver.is_none())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.sharing.closing {
                self.sharing.close_confirm = true;
            }
        }
        if self.sharing.close_confirm {
            egui::Modal::new(egui::Id::new("close_sharing_workers")).show(ctx, |ui| {
                ui.heading("Stop work and close?");
                ui.label("Downloads, sharing, editor work, settings saves and telemetry finish cleanup before the window closes. Pending protocol links are discarded when the app closes. Dismiss any open file dialog to let cancellation finish.");
                if ui.button("Stop and close").clicked() {
                    self.cancelled.store(true, Ordering::Release);
                    self.downloads.request_stop();
                    self.sharing.close();
                    if let Some(SharingContinuation::Terminal(run, result)) = self.sharing_continuation.take() { self.acquisition_terminal(run, result); }
                }
                if ui.button("Keep open").clicked() { self.sharing.close_confirm = false; }
            });
        }
        egui::TopBottomPanel::top("completed_sharing").show(ctx, |ui| {
            let allow_resume = (self.receiver.is_none()
                || matches!(self.job, Job::Download | Job::NxmDownload | Job::Fetch))
                && self.sharing_continuation.is_none()
                && self.peer_view.receiver.is_none()
                && self.peer_view.queued.is_empty()
                && !self.fomod_pending()
                && self
                    .maintenance
                    .resume_allowed(&self.input_stamp, self.sharing.requires_manual_resume());
            self.sharing.show(ui, self.guide.as_ref(), allow_resume);
            if let Some(error) = &self.peer_view.error {
                ui.colored_label(egui::Color32::RED, error);
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.sharing.closing, egui::Button::new("Sharing Off"))
                    .clicked()
                {
                    self.request_peer_settings(vec![(
                        "peerSharing".into(),
                        serde_json::json!(false),
                    )]);
                }
                if ui
                    .add_enabled(!self.sharing.closing, egui::Button::new("KeepSharing Off"))
                    .clicked()
                {
                    self.request_peer_settings(vec![(
                        "peerKeepSharing".into(),
                        serde_json::json!(false),
                    )]);
                }
            });
        });
        egui::TopBottomPanel::top("title").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Managed deployments").clicked() {
                    self.deployment.open = true;
                }
                if ui.button("Merge guides").clicked() {
                    self.merge.open = true;
                }
                if ui.button("Settings / privacy").clicked() {
                    self.desktop.open = true;
                }
                if ui.button("Conflicts").clicked() {
                    self.tools.conflicts_open = true;
                }
                if ui.button("Provider updates").clicked() {
                    self.tools.updates_open = true;
                }
                if ui.button("Checkpoint history").clicked() {
                    self.checkpoints.open = true;
                }
                ui.heading("ModSync.rs");
                ui.label("Install mods in guide order");
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !busy && (self.completed_base.is_none() || self.wizard.is_none()),
                        egui::Button::new("Guide and settings"),
                    )
                    .clicked()
                {
                    if self.wizard.is_none() {
                        self.reset_installation();
                    }
                    self.utility = true;
                    self.page = 0;
                }
                if ui
                    .add_enabled(
                        !busy && (self.completed_base.is_none() || self.wizard.is_none()),
                        egui::Button::new("Guide editor"),
                    )
                    .clicked()
                {
                    if self.wizard.is_none() {
                        self.reset_installation();
                    }
                    self.utility = true;
                    self.page = 3;
                }
                if self.wizard.is_some() && ui.button("Return to wizard").clicked() {
                    self.utility = false;
                }
                if self.wizard.is_none()
                    && self.guide.is_some()
                    && !busy
                    && ui.button("Start wizard").clicked()
                {
                    self.reset_installation();
                    self.wizard = self.guide.as_ref().map(Wizard::new);
                    self.utility = false;
                }
            });
            if self.utility && !busy && self.completed_base.is_none() {
                self.profiles(ui);
            }
        });
        if !self.utility && self.wizard.is_some() {
            egui::TopBottomPanel::top("wizard_header")
                .frame(
                    egui::Frame::default()
                        .fill(ctx.style().visuals.panel_fill)
                        .inner_margin(egui::Margin::symmetric(12, 4)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            for (label, tip, path) in [
                                ("K2", "KOTOR 2 Theme", "/Styles/Kotor2Style.axaml"),
                                ("K1", "KOTOR 1 Theme", "/Styles/KotorStyle.axaml"),
                                ("☀", "Light Theme", "/Styles/LightStyle.axaml"),
                            ] {
                                if ui
                                    .add_sized(
                                        [36.0, 28.0],
                                        egui::Button::new(label).selected(self.theme == path),
                                    )
                                    .on_hover_text(tip)
                                    .clicked()
                                {
                                    self.desktop.settings.theme = Some(path.into());
                                    self.save_desktop_preferences();
                                }
                            }
                        });
                    });
                    ui.add_space(8.0);
                    let wizard = self.wizard.as_ref().unwrap();
                    ui.horizontal(|ui| {
                        let title_width = (ui.available_width() - 208.0).max(100.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(title_width, 58.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_min_width(title_width);
                                ui.label(
                                    egui::RichText::new(wizard.current().title())
                                        .size(24.0)
                                        .strong(),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(wizard.current().subtitle()).size(14.0),
                                );
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(200.0, 44.0),
                            egui::Layout::top_down(egui::Align::Max),
                            |ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Step {} of {}",
                                        wizard.current_index() + 1,
                                        wizard.pages().len()
                                    ))
                                    .size(12.0),
                                );
                                ui.add(
                                    egui::ProgressBar::new(
                                        (wizard.current_index() + 1) as f32
                                            / wizard.pages().len() as f32,
                                    )
                                    .desired_width(200.0)
                                    .desired_height(6.0),
                                );
                            },
                        );
                    });
                });
        }
        let validation_visible = !self.utility
            && self
                .wizard
                .as_ref()
                .is_some_and(|w| w.current() == Page::Validate);
        if !validation_visible {
            egui::TopBottomPanel::bottom("activity")
                .resizable(true)
                .default_height(if ctx.screen_rect().height() < 600.0 {
                    45.0
                } else {
                    100.0
                })
                .max_height(ctx.screen_rect().height() * 0.25)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong("Activity");
                        if busy {
                            ui.spinner();
                            if matches!(self.job, Job::NxmDownload | Job::Fetch) {
                                ui.label(self.downloads.status());
                            }
                            if self.job == Job::ProtocolOpen {
                                ui.label("Downloading ModSync build link…");
                            }
                            if ui.button("Stop").clicked() {
                                self.stop_confirmation = true;
                            }
                        }
                        if ui.button("Copy report").clicked() {
                            ctx.copy_text(self.logs.join("\n"));
                        }
                        if ui
                            .add_enabled(!busy, egui::Button::new("Save report…"))
                            .clicked()
                        {
                            self.shell_work(main_window::Work::SaveLog, None, ctx);
                        }
                        if ui.button("Select / inspect log").clicked() {
                            self.shell.log_open = true;
                        }
                    });
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in &self.logs {
                                ui.monospace(line);
                            }
                        });
                });
        }
        self.refresh_input_stamp();
        // Checked every frame (after the stamp refresh) as well as at the terminal.
        if self.downloads.is_active() {
            let context = self.acquisition_context();
            self.downloads.check_context(&context);
        }
        if !self.utility && self.wizard.is_some() {
            egui::TopBottomPanel::bottom("wizard_navigation")
                .frame(
                    egui::Frame::default()
                        .fill(ctx.style().visuals.panel_fill)
                        .inner_margin(egui::Margin::symmetric(12, 4)),
                )
                .show(ctx, |ui| self.wizard_footer(ui, ctx));
            if self.downloads_visible() && ctx.screen_rect().width() >= 900.0 {
                egui::SidePanel::right("wizard_downloads")
                    .exact_width(320.0)
                    .resizable(false)
                    .show(ctx, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| self.downloads_content(ui, ctx));
                    });
            }
        }
        egui::CentralPanel::default().frame(egui::Frame::default().fill(ctx.style().visuals.panel_fill).inner_margin(16)).show(ctx, |ui| {
            if validation_visible && ctx.screen_rect().width() >= 900.0 && ui.available_height() > 420.0 && ui.available_width() > 500.0 {
                self.validation_content(ui, ctx);
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("wizard_content")
                .show(ui, |ui| {
                    if self.utility || self.wizard.is_none() {
                        if self.guide.is_some() && !(self.wizard.is_none() && self.completed_base.is_some()) {
                            // Utility view: Fetch, status and Stop stay reachable while busy.
                            egui::CollapsingHeader::new("Downloads")
                                .default_open(self.downloads.is_active())
                                .show(ui, |ui| self.downloads_content(ui, ctx));
                        }
                        if self.wizard.is_none() && self.completed_base.is_some() {
                        ui.heading("Loaded project");
                        ui.label("Installation completed. The guide and completion context remain loaded.");
                        if let Some(g) = &self.guide { for c in &g.components { ui.label(&c.name); } }
                        ui.collapsing("Download tools", |ui| self.downloads_content(ui, ctx));
                    } else { ui.add_enabled_ui(!busy, |ui| self.surface(ui, ctx, self.page)); }
                    } else {
                        if self.downloads_visible() && ctx.screen_rect().width() < 900.0 {
                            egui::CollapsingHeader::new("Downloads and peer settings")
                                .default_open(true)
                                .show(ui, |ui| self.downloads_content(ui, ctx));
                            ui.separator();
                        }
                        self.wizard_content(ui, ctx);
                    }
                });
        });
        if let Some(categories) = &mut self.category_selection {
            let mut apply = false;
            let mut cancel = false;
            let response =
                egui::Modal::new(egui::Id::new("category_selection_modal")).show(ctx, |ui| {
                    ui.heading("Select mods by category");
                    ui.add_enabled_ui(!busy, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(320.0)
                            .show(ui, |ui| {
                                for (category, selected) in categories.iter_mut() {
                                    ui.checkbox(selected, category);
                                }
                            });
                        ui.horizontal(|ui| {
                            apply = ui
                                .add_enabled(
                                    categories.values().any(|v| *v),
                                    egui::Button::new("Select categories"),
                                )
                                .clicked();
                            cancel = ui.button("Cancel").clicked();
                        });
                    });
                });
            if apply {
                self.apply_category_selection();
            } else if cancel || response.should_close() {
                self.category_selection = None;
            }
        }
        if self.stop_confirmation {
            let install = self.job == Job::Install && self.installing_page.active();
            let acquisition = self.receiver.is_some()
                && matches!(self.job, Job::Download | Job::NxmDownload | Job::Fetch);
            let mut stop_downloads = false;
            egui::Modal::new(egui::Id::new("stop_operation")).show(ctx, |ui| {
                if acquisition {
                    ui.heading("Stop downloads?");
                    ui.label("Transfers stop at the next safe point and unfinished files are removed. Archives that already finished and verified stay in the workspace and are still recorded. Downloading installs nothing, so there is no install rollback or snapshot restore.");
                    ui.horizontal(|ui| {
                        if ui.button("Stop downloads").clicked() {
                            stop_downloads = true;
                            self.stop_confirmation = false;
                        }
                        if ui.button("Keep downloading").clicked() {
                            self.stop_confirmation = false;
                        }
                    });
                    return;
                }
                if install {
                    ui.heading("Stop the install?");
                    ui.label(self.installing_page.stop_warning());
                } else if self.job == Job::ProtocolOpen {
                    ui.heading("Stop loading the guide?");
                    ui.label("The guide download is cancelled and its temporary document is removed. Your open guide stays available.");
                } else {
                    ui.heading("Stop current operation?");
                    ui.label("Cancellation stops at the next safe engine checkpoint. A failed installation requires restoring its verified snapshot before retrying.");
                }
                ui.horizontal(|ui| {
                    if ui.button(if install { "Stop install" } else { "Stop operation" }).clicked() { self.cancelled.store(true, Ordering::Relaxed); self.stop_confirmation = false; }
                    if ui.button(if install { "Keep installing" } else { "Keep running" }).clicked() { self.stop_confirmation = false; }
                });
            });
            if stop_downloads {
                self.sharing.pause(modsync::peer_sharing::PauseReason::User);
                // The run's own token; the run stays busy and shows stopping until its
                // workers join and the terminal arrives.
                self.cancelled.store(true, Ordering::Relaxed);
                self.downloads.request_stop();
            }
        }
        if self.confirm_install {
            let response = egui::Modal::new(egui::Id::new("install_confirmation_modal")).show(ctx, |ui| {
                ui.heading("Install selected mods?");
                ui.label(format!("Modify {}", self.game));
                ui.label("The snapshot needs space for both folders. The install stops on the first error.");
                ui.horizontal(|ui| {
                    if ui.button("Create snapshot and install").clicked() { self.confirm_install = false; self.advance_wizard(ctx); }
                    if ui.button("Cancel").clicked() { self.confirm_install = false; }
                });
            });
            if install_confirmation_was_open && response.should_close() {
                self.confirm_install = false;
            }
        }
    }
}

pub struct LaunchContext {
    pub probe: Option<Probe>,
    pub easy_session: Option<PathBuf>,
}

pub fn launch(
    input: Option<PathBuf>,
    game: Option<PathBuf>,
    mods: Option<PathBuf>,
    settings_dir: Option<PathBuf>,
    protocol: Option<String>,
    allow_multiple_instances: bool,
    context: LaunchContext,
) -> Result<()> {
    let LaunchContext {
        probe,
        easy_session,
    } = context;
    // Probe context is settled before any other effect of the launch.
    let probe = match probe {
        Some(probe) => Some(probe_context(probe, settings_dir.as_deref())?),
        None => None,
    };
    // A probe is always a separate instance: no socket claim and no forwarding, since
    // forwarding carries only protocol text, never probe configuration.
    let separate = allow_multiple_instances || probe.is_some() || easy_session.is_some();
    let primary = if separate {
        None
    } else {
        let identity = modsync::protocol_registration::package_identity(&std::env::current_exe()?)?;
        modsync::single_instance::claim_or_forward_for_channel(
            protocol.as_deref(),
            &identity.channel,
        )?
    };
    if primary.is_none() && !separate {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    let (native_protocol_receiver, _native_url_guard) = modsync::native_url::install()?;
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([600.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ModSync.rs",
        native,
        Box::new(move |creation| {
            let mut app = App::with_launch(input, game, mods, settings_dir, probe);
            app.repaint = Some(creation.egui_ctx.clone());
            #[cfg(target_os = "macos")]
            {
                app.native_protocol_receiver = Some(native_protocol_receiver);
            }
            if let Some((protocol_receiver, instance_guard)) = primary {
                app.protocol_receiver = Some(protocol_receiver);
                app._instance_guard = Some(instance_guard);
            }
            app.begin_desktop_load(app.explicit_settings.clone());
            if let Some(directory) = easy_session {
                app.queue_easy_resume(directory);
            }
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "validation-symbols".into(),
                egui::FontData::from_static(include_bytes!(
                    "../assets/fonts/NotoEmoji-Regular.ttf"
                ))
                .into(),
            );
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .push("validation-symbols".into());
            }
            creation.egui_ctx.set_fonts(fonts);
            app.apply_theme(&creation.egui_ctx);
            // Dispatched only after the settings context and preferences are in place.
            if let Some(url) = protocol {
                app.accept_protocol(url);
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Desktop startup failed: {e}"))
}

/// Canonicalizes the nearest existing ancestor of `path` and re-appends the missing
/// rest, so a not-yet-created directory still compares through resolved aliases.
fn canonical_with_missing_tail(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut existing = absolute.as_path();
    let mut tail = Vec::new();
    loop {
        if let Ok(resolved) = std::fs::canonicalize(existing) {
            return tail.iter().rev().fold(resolved, |acc, part| acc.join(part));
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name.to_owned());
                existing = parent;
            }
            _ => return absolute,
        }
    }
}

/// Mirrors the CLI's probe rules: an API base needs an explicit nonblank key, the CA is
/// per invocation, and the settings directory must be explicit, existing and neither the
/// default settings directory nor nested with it, compared after resolving aliases.
fn probe_context(probe: Probe, settings_dir: Option<&Path>) -> Result<ProbeContext> {
    use anyhow::Context as _;
    let directory = settings_dir
        .filter(|path| !path.as_os_str().is_empty())
        .context("Probe options require --settings-dir with a private settings directory")?;
    let resolved = std::fs::canonicalize(directory)
        .context("The probe settings directory must be an existing directory")?;
    anyhow::ensure!(
        resolved.is_dir(),
        "The probe settings directory must be an existing directory"
    );
    if let Ok(default) = modsync::profiles::settings_directory(None) {
        let default = canonical_with_missing_tail(&default);
        anyhow::ensure!(
            !resolved.starts_with(&default) && !default.starts_with(&resolved),
            "Probe options require a settings directory separate from the default settings directory"
        );
    }
    let nexus_api_key = probe.nexus_api_key.filter(|key| !key.trim().is_empty());
    anyhow::ensure!(
        probe.nexus_api_base.is_none() || nexus_api_key.is_some(),
        "--nexus-api-base requires an explicit --nexus-mods-api-key"
    );
    let loopback_ca = probe
        .extra_ca_file
        .as_deref()
        .map(modsync::download::LoopbackCa::from_pem_file)
        .transpose()?;
    Ok(ProbeContext {
        nexus_api_key,
        nexus_api_base: probe.nexus_api_base,
        loopback_ca,
    })
}

#[cfg(test)]
mod tests {
    fn finish_worker(app: &mut App) {
        let receiver = app.receiver.take().expect("worker started");
        loop {
            let message = receiver.recv_timeout(Duration::from_secs(15)).unwrap();
            let done = matches!(message, Message::Done(_));
            app.handle_message(message);
            if done {
                break;
            }
        }
    }

    fn route_fixture(wide: bool) -> (tempfile::TempDir, App) {
        let root = tempfile::tempdir().unwrap();
        let game = root.path().join("game");
        let mods = root.path().join("mods");
        std::fs::create_dir(&game).unwrap();
        std::fs::create_dir(&mods).unwrap();
        std::fs::write(game.join("swkotor.exe"), b"detection fixture").unwrap();
        std::fs::write(mods.join("base.txt"), b"base fixture").unwrap();
        std::fs::write(mods.join("wide.txt"), b"wide fixture").unwrap();
        let guide = Guide::parse(r#"{"thisMod":[
            {"Guid":"base","Name":"Base","IsSelected":true,"Instructions":[{"Action":"Move","Source":["<<modDirectory>>/base.txt"],"Destination":"<<gameDirectory>>/Override","Overwrite":true}]},
            {"Guid":"wide","Name":"Wide","IsSelected":true,"WidescreenOnly":true,"Instructions":[{"Action":"Copy","Source":["<<modDirectory>>/wide.txt"],"Destination":"<<gameDirectory>>/Override","Overwrite":true}]}
        ]}"#, "json").unwrap();
        let mut app = App::new(None, Some(game), Some(mods));
        app.apply_protocol_guide(guide);
        app.guide.as_mut().unwrap().components[1].is_selected = wide;
        app.refresh_input_stamp();
        (root, app)
    }

    fn phase_manifest(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        walkdir::WalkDir::new(root)
            .into_iter()
            .map(Result::unwrap)
            .map(|entry| {
                let bytes = if entry.file_type().is_file() {
                    std::fs::read(entry.path()).unwrap()
                } else {
                    Vec::new()
                };
                (entry.path().strip_prefix(root).unwrap().to_owned(), bytes)
            })
            .collect()
    }

    #[test]
    fn desktop_preparation_matches_cli_all_and_base_after_selection() {
        use modsync::phase::{self, CompletionContext, Phase};
        let (root, mut app) = route_fixture(true);
        let choices =
            modsync::fomod::ChoicesFile::parse(r#"{"archives":[{"archiveFileName":"wide.zip"}]}"#)
                .unwrap();
        let before = phase_manifest(root.path());
        let selected = serde_json::to_value(&app.guide).unwrap();
        for (job, phase, expected) in [
            (Job::Validate, Phase::All, vec!["base", "wide"]),
            (Job::Conflicts, Phase::Base, vec!["base"]),
            (Job::Install, Phase::Base, vec!["base"]),
        ] {
            let desktop = app.prepare_phase(job, Some(&choices)).unwrap();
            let cli = phase::prepare(
                app.guide.as_ref().unwrap(),
                phase,
                CompletionContext::None,
                Some(&choices),
            )
            .unwrap();
            assert_eq!(
                serde_json::to_value(&desktop.guide).unwrap(),
                serde_json::to_value(cli.guide).unwrap()
            );
            assert!(desktop.completed.is_empty() && cli.completed.is_empty());
            assert_eq!(
                desktop
                    .guide
                    .ordered()
                    .unwrap()
                    .iter()
                    .map(|c| c.guid.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
        }
        assert_eq!(serde_json::to_value(&app.guide).unwrap(), selected);
        assert_eq!(phase_manifest(root.path()), before);

        // All leaves invalid ordering to the full report pipeline.
        app.guide.as_mut().unwrap().components[0].dependencies = vec!["missing".into()];
        assert!(app.prepare_phase(Job::Validate, None).is_ok());
        app.start(Job::Validate, &egui::Context::default());
        finish_worker(&mut app);
        assert!(!app.validated);
        assert!(!app.validation_report.as_ref().unwrap().success);
        assert_eq!(phase_manifest(root.path()), before);
    }

    #[test]
    fn desktop_retained_and_cli_reconstructed_choices_verify_real_completion() {
        use modsync::phase::{self, CompletionContext, Phase};
        fn choices(base: &str, wide: &str) -> modsync::fomod::ChoicesFile {
            modsync::fomod::ChoicesFile::parse(&serde_json::json!({"archives":[
                {"archiveFileName":"base.zip","selections":[{"stepName":"S","groupName":"G","plugins":[base]}]},
                {"archiveFileName":"wide.zip","selections":[{"stepName":"S","groupName":"G","plugins":[wide]}]}
            ]}).to_string()).unwrap()
        }
        for (base_choice, wide_choice, equal) in [
            ("original", "original", true),
            ("changed", "original", false),
            ("original", "changed", false),
        ] {
            let (root, mut app) = route_fixture(true);
            let selected = app.guide.clone().unwrap();
            let path = root.path().join("choices.json");
            std::fs::write(
                &path,
                serde_json::to_vec(&choices("original", "original")).unwrap(),
            )
            .unwrap();
            app.fomod_choices = path.to_string_lossy().into_owned();
            let ctx = egui::Context::default();
            app.start(Job::Install, &ctx);
            assert_eq!(app.pending_base.len(), 1);
            finish_worker(&mut app);
            assert_eq!(app.completed_base.as_deref().unwrap(), ["base"]);
            assert!(app.pending_base.is_empty() && app.install_context.is_none());
            assert!(!Path::new(&app.mods).join("base.txt").exists());
            let settings = Settings::new(Path::new(&app.mods), Path::new(&app.game)).unwrap();
            let receipt_path = recovery::state_dir(&settings.roots).join("completion.json");
            let base_receipt: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&receipt_path).unwrap()).unwrap();
            let historical =
                serde_json::to_value(&app.guide.as_ref().unwrap().components[0]).unwrap();
            assert_eq!(base_receipt["components"][0], historical);
            let current = choices(base_choice, wide_choice);
            std::fs::write(&path, serde_json::to_vec(&current).unwrap()).unwrap();
            let gui = app.prepare_phase(Job::Validate, Some(&current)).unwrap();
            let cli = phase::prepare(
                &selected,
                Phase::Remaining,
                CompletionContext::Reconstruct,
                Some(&current),
            )
            .unwrap();
            assert_eq!(gui.completed, cli.completed);
            assert_eq!(
                serde_json::to_value(&gui.guide.components[0]).unwrap(),
                historical
            );
            assert_eq!(
                serde_json::to_value(&gui.guide).unwrap()
                    == serde_json::to_value(&cli.guide).unwrap(),
                equal
            );
            let before = phase_manifest(root.path());
            let cli_report = Engine::new(settings.clone())
                .unwrap()
                .validate_report_remaining(&cli.guide, &cli.completed, |_| {});
            if equal {
                assert!(cli_report.as_ref().unwrap().success);
            } else {
                let report = cli_report.as_ref().unwrap();
                assert!(!report.success);
                assert!(
                    report
                        .stages
                        .iter()
                        .flat_map(|stage| &stage.messages)
                        .any(|message| message.contains("definitions differ"))
                );
                assert!(
                    Engine::new(settings.clone())
                        .unwrap()
                        .install_remaining(&cli.guide, &cli.completed, |_| {})
                        .is_err()
                );
            }
            app.start(Job::Validate, &ctx);
            finish_worker(&mut app);
            assert!(app.validated, "{:?}", app.logs);
            if equal {
                assert_eq!(
                    serde_json::to_value(app.validation_report.as_ref().unwrap()).unwrap(),
                    serde_json::to_value(cli_report.unwrap()).unwrap()
                );
            }
            app.start(Job::Conflicts, &ctx);
            finish_worker(&mut app);
            assert_eq!(app.conflicts.as_ref().unwrap().analyzed_component_count, 1);
            assert_eq!(phase_manifest(root.path()), before);
            app.start(Job::Install, &ctx);
            finish_worker(&mut app);
            assert!(app.finished, "{:?}", app.logs);
            assert_eq!(
                std::fs::read(Path::new(&app.game).join("Override/base.txt")).unwrap(),
                b"base fixture"
            );
            assert_eq!(
                std::fs::read(Path::new(&app.game).join("Override/wide.txt")).unwrap(),
                b"wide fixture"
            );
            assert!(!Path::new(&app.mods).join("base.txt").exists());
            let final_receipt: serde_json::Value =
                serde_json::from_slice(&std::fs::read(receipt_path).unwrap()).unwrap();
            assert_eq!(final_receipt["components"][0], historical);
            assert_eq!(final_receipt["components"].as_array().unwrap().len(), 2);
            assert!(
                !recovery::state_dir(&settings.roots)
                    .join("pending")
                    .exists()
            );
        }
    }

    #[test]
    fn desktop_invalid_retained_ids_and_failed_base_never_promote_or_mutate() {
        let (root, mut app) = route_fixture(true);
        let mut third = app.guide.as_ref().unwrap().components[1].clone();
        third.guid = "third".into();
        app.guide.as_mut().unwrap().components.push(third);
        let before = phase_manifest(root.path());
        for (ids, diagnostic) in [
            (vec![], "nonempty completed prefix"),
            (vec!["base", "BASE"], "Duplicate completed component ID"),
            (vec!["missing"], "selected installation prefix"),
            (vec!["wide"], "selected installation prefix"),
        ] {
            app.completed_base = Some(ids.into_iter().map(str::to_owned).collect());
            let completed = app.completed_base.clone();
            let guide = serde_json::to_value(&app.guide).unwrap();
            for job in [Job::Validate, Job::Conflicts, Job::Install] {
                app.start(job, &egui::Context::default());
                assert!(app.receiver.is_none());
                assert!(
                    app.logs.last().unwrap().contains(diagnostic),
                    "{:?}",
                    app.logs
                );
                assert_eq!(app.completed_base, completed);
                assert_eq!(serde_json::to_value(&app.guide).unwrap(), guide);
                assert!(app.pending_base.is_empty() && app.install_context.is_none());
                assert!(!app.finished && !app.validated);
                assert_eq!(phase_manifest(root.path()), before);
            }
        }
        app.completed_base = None;
        std::fs::remove_file(Path::new(&app.mods).join("base.txt")).unwrap();
        let game_before = phase_manifest(Path::new(&app.game));
        let mods_before = phase_manifest(Path::new(&app.mods));
        app.start(Job::Install, &egui::Context::default());
        assert_eq!(app.pending_base.len(), 1);
        finish_worker(&mut app);
        assert!(app.completed_base.is_none());
        assert!(app.pending_base.is_empty() && app.install_context.is_none());
        assert_eq!(phase_manifest(Path::new(&app.game)), game_before);
        assert_eq!(phase_manifest(Path::new(&app.mods)), mods_before);
        let settings = Settings::new(Path::new(&app.mods), Path::new(&app.game)).unwrap();
        assert!(
            !recovery::state_dir(&settings.roots)
                .join("completion.json")
                .exists()
        );
        assert!(
            !recovery::state_dir(&settings.roots)
                .join("pending")
                .exists()
        );
    }

    fn reach(app: &mut App, page: Page) {
        let ctx = egui::Context::default();
        for _ in 0..20 {
            if app.wizard.as_ref().unwrap().current() == page {
                return;
            }
            let previous = app.wizard.as_ref().unwrap().current();
            app.advance_wizard(&ctx);
            assert_ne!(
                app.wizard.as_ref().unwrap().current(),
                previous,
                "gate blocked at {previous:?}"
            );
        }
        panic!("page not reached");
    }

    #[test]
    fn choices_metadata_changes_and_inflight_edits_invalidate_validation() {
        let (root, mut app) = route_fixture(true);
        let choices = root.path().join("choices.json");
        std::fs::write(&choices, "{}").unwrap();
        app.fomod_choices = choices.to_string_lossy().into_owned();
        app.refresh_input_stamp();
        let original = app.input_stamp.clone();
        std::fs::write(&choices, "{\"changed\":true}").unwrap();
        app.refresh_input_stamp();
        assert_ne!(app.input_stamp, original);

        app.fomod_choices.clear();
        app.start(Job::Validate, &egui::Context::default());
        // A worker completion for the previous inputs must not make the new
        // inputs ready, even if a repaint already observed the edit.
        app.patcher = "changed while validation was running".into();
        app.refresh_input_stamp();
        finish_worker(&mut app);
        assert!(!app.validated);
        assert!(app.validation_report.is_none());
        assert!(
            app.logs
                .iter()
                .any(|line| line.contains("Inputs changed during validation"))
        );
    }

    #[test]
    #[cfg(unix)]
    fn choices_stamp_never_opens_a_fifo() {
        let root = tempfile::tempdir().unwrap();
        let fifo = root.path().join("choices");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        let stamp = App::choices_file_stamp(&fifo);
        assert!(!stamp.is_empty());
    }

    #[test]
    fn desktop_connected_route_uses_worker_success_and_verified_remaining_prefix() {
        let (_root, mut app) = route_fixture(true);
        let ctx = egui::Context::default();
        assert_eq!(app.wizard.as_ref().unwrap().current(), Page::Welcome);
        reach(&mut app, Page::Validate);
        assert!(app.downloads_visible());
        assert!(!app.wizard_gate().validation_passed);
        app.start(Job::Validate, &ctx);
        finish_worker(&mut app);
        assert!(app.validated, "{:?}", app.logs);
        reach(&mut app, Page::Installing);
        assert!(!app.wizard_gate().validation_passed);
        finish_worker(&mut app);
        assert!(app.wizard_gate().validation_passed, "{:?}", app.logs);
        assert!(!Path::new(&app.mods).join("base.txt").exists());
        assert!(!Path::new(&app.game).join("Override/wide.txt").exists());
        reach(&mut app, Page::WidescreenModSelection);
        assert!(!app.validated);
        app.start(Job::Validate, &ctx);
        finish_worker(&mut app);
        assert!(app.validated, "{:?}", app.logs);
        reach(&mut app, Page::WidescreenInstalling);
        finish_worker(&mut app);
        assert!(app.wizard_gate().validation_passed, "{:?}", app.logs);
        assert_eq!(
            std::fs::read(Path::new(&app.game).join("Override/base.txt")).unwrap(),
            b"base fixture"
        );
        assert_eq!(
            std::fs::read(Path::new(&app.game).join("Override/wide.txt")).unwrap(),
            b"wide fixture"
        );
        reach(&mut app, Page::Finished);
        let completed = app.completed_base.clone();
        app.finish_wizard();
        assert!(app.wizard.is_none());
        assert!(app.guide.is_some());
        assert_eq!(app.completed_base, completed);
        assert!(app.finished);
    }

    #[test]
    fn desktop_empty_widescreen_phase_retains_all_completion_pages_without_replay() {
        let (_root, mut app) = route_fixture(false);
        let ctx = egui::Context::default();
        reach(&mut app, Page::Validate);
        app.start(Job::Validate, &ctx);
        finish_worker(&mut app);
        reach(&mut app, Page::Installing);
        finish_worker(&mut app);
        reach(&mut app, Page::WidescreenModSelection);
        app.advance_wizard(&ctx);
        assert_eq!(
            app.wizard.as_ref().unwrap().current(),
            Page::WidescreenInstalling
        );
        assert!(app.receiver.is_none());
        assert!(app.finished);
        assert!(app.logs.last().unwrap().contains("skipped"));
        app.advance_wizard(&ctx);
        assert_eq!(
            app.wizard.as_ref().unwrap().current(),
            Page::WidescreenComplete
        );
        app.advance_wizard(&ctx);
        assert_eq!(app.wizard.as_ref().unwrap().current(), Page::Finished);
    }

    #[test]
    fn desktop_gates_reject_stale_validation_unknown_game_and_failed_worker() {
        let (_root, mut app) = route_fixture(true);
        let ctx = egui::Context::default();
        reach(&mut app, Page::GameDirectory);
        let game = app.game.clone();
        app.game = app.mods.clone();
        assert!(!app.wizard_gate().validation_passed);
        app.game = game;
        reach(&mut app, Page::Validate);
        app.start(Job::Validate, &ctx);
        finish_worker(&mut app);
        assert!(app.validated);
        app.patcher_engine = PatcherEngine::KPatcher;
        app.refresh_input_stamp();
        assert!(!app.validated);
        assert!(app.validation_report.is_none());
        app.start(Job::Validate, &ctx);
        finish_worker(&mut app);
        reach(&mut app, Page::InstallStart);
        app.guide.as_mut().unwrap().components[0].instructions[0].destination =
            "<<gameDirectory>>/Other".into();
        app.advance_wizard(&ctx);
        assert_eq!(app.wizard.as_ref().unwrap().current(), Page::InstallStart);
        app.job = Job::Install;
        app.handle_message(Message::Done(Err(anyhow::anyhow!("fixture failure"))));
        assert!(app.completed_base.is_none());
        assert!(!app.finished);
    }

    #[test]
    fn base_validation_includes_selected_widescreen_inputs() {
        let (_root, mut app) = route_fixture(true);
        std::fs::remove_file(Path::new(&app.mods).join("wide.txt")).unwrap();
        app.start(Job::Validate, &egui::Context::default());
        finish_worker(&mut app);
        assert!(!app.validated);
        assert!(!app.validation_report.as_ref().unwrap().success);
        assert!(app.completed_base.is_none());
    }

    #[test]
    fn protocol_guide_replaces_stale_wizard_state() {
        let mut app = App::new(None, None, None);
        app.loaded_input = Some(std::path::PathBuf::from("old.toml"));
        app.page = 7;
        app.selected = 4;
        app.validated = true;
        app.confirm_install = true;
        app.apply_protocol_guide(Guide {
            components: vec![Component {
                guid: "from-link".into(),
                ..Default::default()
            }],
            ..Default::default()
        });

        assert_eq!(app.wizard.as_ref().unwrap().current(), Page::Welcome);
        assert_eq!(app.selected, 0);
        assert!(app.loaded_input.is_none());
        assert!(!app.validated && !app.confirm_install);
        assert_eq!(app.guide.unwrap().components[0].guid, "from-link");
    }

    #[test]
    fn saved_desktop_selection_embeds_external_fomod_choices_and_reloads() {
        let temp = tempfile::tempdir().unwrap();
        let choices = temp.path().join("choices.json");
        std::fs::write(&choices, r#"{"archives":[{"archiveFileName":"mod.zip","selections":[{"stepName":"S","groupName":"G","plugins":["Second"]}]}]}"#).unwrap();
        let mut app = App::new(None, None, None);
        app.guide = Some(Guide {
            components: vec![Component {
                guid: "selected".into(),
                name: "Selected".into(),
                is_selected: true,
                ..Default::default()
            }],
            ..Default::default()
        });
        app.fomod_choices = choices.to_string_lossy().into_owned();
        let output = temp.path().join("selection.toml");
        app.save_selection(&output).unwrap();
        let reopened = Guide::load(&output).unwrap();
        assert!(reopened.components[0].is_selected);
        let saved = modsync::fomod::saved_archive_choices(&reopened, "mod.zip")
            .unwrap()
            .unwrap();
        assert_eq!(saved.selections[0].plugins, ["Second"]);
        assert!(
            !app.guide.as_ref().unwrap().components[0]
                .metadata
                .contains_key("FomodChoices")
        );
        let before = std::fs::read(&output).unwrap();
        std::fs::write(&choices, "invalid").unwrap();
        assert!(app.save_selection(&output).is_err());
        assert_eq!(std::fs::read(output).unwrap(), before);
    }

    #[test]
    fn stale_install_completion_cannot_promote_effective_definitions() {
        for mutation in 0..3 {
            let (_root, mut app) = route_fixture(true);
            app.start(Job::Install, &egui::Context::default());
            if mutation == 0 {
                // Even a replacement with identical contents clears dispatch context.
                app.reset_installation();
            } else if mutation == 1 {
                app.guide.as_mut().unwrap().components[0].name = "Replacement".into();
            } else {
                app.game = app.mods.clone();
            }
            let current = serde_json::to_value(&app.guide).unwrap();
            finish_worker(&mut app);
            assert_eq!(serde_json::to_value(&app.guide).unwrap(), current);
            assert!(app.completed_base.is_none());
            assert!(app.pending_base.is_empty() && app.install_context.is_none());
            assert!(!app.finished);
            assert!(
                app.logs
                    .iter()
                    .any(|line| line.contains("completion was not applied"))
            );
        }
    }

    #[test]
    fn remaining_fomod_editor_and_external_choices_preserve_consumed_base() {
        use std::io::Write;
        // Existing post-base paths plus unchanged, editor-edited, and file-edited
        // external choices supplied before the base worker.
        for (before_base, external, unchanged) in [
            (false, false, false),
            (false, true, false),
            (true, true, true),
            (true, false, false),
            (true, true, false),
        ] {
            let (root, mut app) = route_fixture(false);
            let archive = Path::new(&app.mods).join("wide.zip");
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
            for (name, bytes) in [
                ("fomod/ModuleConfig.xml", br#"<config><installSteps><installStep name="Style"><optionalFileGroups><group name="Texture" type="SelectExactlyOne"><plugins><plugin name="Default"><files><file source="default.txt" destination="Override"/></files></plugin><plugin name="Alternate"><files><file source="asset.txt" destination="Override"/></files></plugin></plugins></group></optionalFileGroups></installStep></installSteps></config>"#.as_slice()),
                ("asset.txt", b"configured wide".as_slice()),
                ("default.txt", b"unwanted default".as_slice()),
            ] {
                zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
                zip.write_all(bytes).unwrap();
            }
            zip.finish().unwrap();
            let guide = app.guide.as_mut().unwrap();
            guide.components[1].instructions = vec![
                serde_json::from_value(
                    serde_json::json!({"Action":"Extract","Source":["<<modDirectory>>/wide.zip"]}),
                )
                .unwrap(),
            ];
            guide.components[0].options.push(Component {
                guid: "base-option".into(),
                ..Default::default()
            });
            let choices = modsync::fomod::ChoicesFile::parse(r#"{"archives":[{"archiveFileName":"wide.zip","selections":[{"stepName":"Style","groupName":"Texture","plugins":["Alternate"]}]}]}"#).unwrap();
            let mut expected = guide.clone();
            let choices_path = root.path().join("remaining-choices.json");
            if before_base {
                let initial = if unchanged {
                    choices.clone()
                } else {
                    modsync::fomod::ChoicesFile::parse(
                        &serde_json::to_string(&choices)
                            .unwrap()
                            .replace("Alternate", "Default"),
                    )
                    .unwrap()
                };
                initial.apply_to_guide(&mut expected).unwrap();
                std::fs::write(&choices_path, serde_json::to_vec(&initial).unwrap()).unwrap();
                app.fomod_choices = choices_path.to_string_lossy().into_owned();
            }
            let base_definition = serde_json::to_value(&expected.components[0]).unwrap();
            let ctx = egui::Context::default();
            reach(&mut app, Page::Validate);
            app.start(Job::Validate, &ctx);
            finish_worker(&mut app);
            assert!(app.validated, "{:?}", app.logs);
            reach(&mut app, Page::Installing);
            finish_worker(&mut app);
            assert!(!Path::new(&app.mods).join("base.txt").exists());
            assert_eq!(
                serde_json::to_value(&app.guide.as_ref().unwrap().components[0]).unwrap(),
                base_definition
            );
            let settings = Settings::new(Path::new(&app.mods), Path::new(&app.game)).unwrap();
            let receipt: serde_json::Value = serde_json::from_slice(
                &std::fs::read(recovery::state_dir(&settings.roots).join("completion.json"))
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(receipt["components"][0], base_definition);
            reach(&mut app, Page::WidescreenModSelection);
            modsync::selection::set_component_selected(
                &mut app.guide.as_mut().unwrap().components,
                1,
                true,
            );
            if unchanged {
                // Leave the original external choices path and bytes untouched.
            } else if external {
                std::fs::write(&choices_path, serde_json::to_vec(&choices).unwrap()).unwrap();
                app.fomod_choices = choices_path.to_string_lossy().into_owned();
            } else {
                // Same handler as the native editor's committed Choices outcome.
                app.apply_fomod_choices(&choices).unwrap();
                assert!(app.saved_fomod_choices(&archive).unwrap().is_some());
            }
            assert_eq!(
                serde_json::to_value(&app.guide.as_ref().unwrap().components[0]).unwrap(),
                base_definition
            );
            app.start(Job::Validate, &ctx);
            finish_worker(&mut app);
            assert!(app.validated, "external={external}: {:?}", app.logs);
            reach(&mut app, Page::WidescreenInstalling);
            finish_worker(&mut app);
            assert!(app.finished, "external={external}: {:?}", app.logs);
            assert_eq!(
                std::fs::read(Path::new(&app.game).join("Override/base.txt")).unwrap(),
                b"base fixture"
            );
            assert_eq!(
                std::fs::read(Path::new(&app.game).join("Override/asset.txt")).unwrap(),
                b"configured wide"
            );
            assert!(!Path::new(&app.game).join("Override/default.txt").exists());
            assert!(!Path::new(&app.mods).join("base.txt").exists());
            assert_eq!(
                serde_json::to_value(&app.guide.as_ref().unwrap().components[0]).unwrap(),
                base_definition
            );
        }
    }

    #[test]
    fn desktop_fomod_edits_preserve_choices_file_archives_and_invalidate_readiness() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("choices.json");
        std::fs::write(&path, r#"{"archives":[{"archiveFileName":"other.zip"}]}"#).unwrap();
        let mut app = App::new(None, None, None);
        app.guide = Some(Guide {
            components: vec![Component::default()],
            ..Default::default()
        });
        app.fomod_choices = path.to_string_lossy().into_owned();
        app.validated = true;
        app.confirm_install = true;
        let edits = modsync::fomod::ChoicesFile::parse(
            r#"{"archives":[{"archiveFileName":"edited.zip"}]}"#,
        )
        .unwrap();
        app.apply_fomod_choices(&edits).unwrap();
        let saved = &app.guide.as_ref().unwrap().components[0].metadata["FomodChoices"];
        assert_eq!(saved["archives"].as_array().unwrap().len(), 2);
        assert!(!app.validated && !app.confirm_install);
        assert!(app.fomod_choices.is_empty());
    }

    #[test]
    fn desktop_validation_uses_the_shared_full_dry_run_pipeline() {
        let root = tempfile::tempdir().unwrap();
        let mods = root.path().join("mods");
        let game = root.path().join("game");
        std::fs::create_dir(&mods).unwrap();
        std::fs::create_dir(&game).unwrap();
        std::fs::write(mods.join("sample.tga"), b"fixture").unwrap();
        let guide = Guide::parse(
            include_str!("../docs/parity/validation-pipeline-oracle/guide.json"),
            "json",
        )
        .unwrap();
        let expected = Engine::new(Settings::new(&mods, &game).unwrap())
            .unwrap()
            .validate_report_with_options(&guide, |_| {}, true, true, false);
        let mut app = App::new(None, Some(game.clone()), Some(mods));
        app.guide = Some(guide);
        app.start(Job::Validate, &egui::Context::default());
        let receiver = app.receiver.take().unwrap();
        let mut actual = None;
        loop {
            match receiver.recv_timeout(Duration::from_secs(10)).unwrap() {
                Message::ValidationReport(report) => actual = Some(report),
                Message::Done(result) => {
                    result.unwrap();
                    break;
                }
                Message::Log(_) => {}
                _ => panic!("unexpected validation message"),
            }
        }
        let actual = actual.unwrap();
        assert!(actual.dry_run.as_ref().unwrap().is_valid);
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        assert_eq!(std::fs::read_dir(game).unwrap().count(), 0);
    }

    #[test]
    fn desktop_conflict_worker_reports_without_writing_and_clears_stale_results() {
        let root = tempfile::tempdir().unwrap();
        let mods = root.path().join("mods");
        let game = root.path().join("game");
        std::fs::create_dir(&mods).unwrap();
        std::fs::create_dir(&game).unwrap();
        std::fs::write(mods.join("asset.txt"), "asset").unwrap();
        let mut app = App::new(None, Some(game.clone()), Some(mods));
        app.guide = Some(Guide::parse(r#"{"thisMod":[
            {"Guid":"a","Name":"First","Instructions":[{"Action":"Copy","Source":["<<modDirectory>>/asset.txt"],"Destination":"<<gameDirectory>>/Override","Overwrite":true}]},
            {"Guid":"b","Name":"Last","Instructions":[{"Action":"Copy","Source":["<<modDirectory>>/asset.txt"],"Destination":"<<gameDirectory>>/Override","Overwrite":true}]}
        ]}"#, "json").unwrap());
        app.conflicts = Some(modsync::conflicts::Report::default());
        app.validated = true;
        app.start(Job::Conflicts, &egui::Context::default());
        assert!(app.conflicts.is_none());
        assert!(!app.validated);
        let receiver = app.receiver.take().unwrap();
        let report = match receiver.recv_timeout(Duration::from_secs(10)).unwrap() {
            Message::Conflicts(report) => report,
            _ => panic!("expected conflict report"),
        };
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].winner_component_guid, "b");
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(10)).unwrap(),
            Message::Done(Ok(()))
        ));
        assert_eq!(std::fs::read_dir(&game).unwrap().count(), 0);
    }

    #[test]
    fn nxm_completion_marks_only_the_resolved_registry_file() {
        let mut app = App::new(None, None, None);
        app.guide = Some(Guide::parse(
            r#"{"thisMod":[{"Guid":"component","Name":"Component","ResourceRegistry":{"https://www.nexusmods.com/kotor/mods/9":{"Files":{}}}}]}"#,
            "json",
        )
        .unwrap());
        app.record_nxm_download(
            modsync::protocol::NxmComponentMatch {
                component_guid: "component".into(),
                component_name: String::new(),
                registry_url: "https://www.nexusmods.com/kotor/mods/9".into(),
            },
            "archive.zip",
        )
        .unwrap();
        assert_eq!(
            app.guide.as_ref().unwrap().components[0].resource_registry["https://www.nexusmods.com/kotor/mods/9"]
                ["Files"]["archive.zip"],
            true
        );
    }

    #[test]
    fn queued_nxm_handoffs_keep_arrival_order_until_setup_is_ready() {
        let mut app = App::new(None, None, None);
        app.accept_protocol("nxm://kotor/mods/1/files/2?key=one&expires=1".into());
        app.accept_protocol("nxm://kotor/mods/3/files/4?key=two&expires=1".into());
        assert_eq!(app.pending_nxm.len(), 2);
        assert!(app.pending_nxm[0].contains("mods/1/files/2"));
        assert!(app.pending_nxm[1].contains("mods/3/files/4"));
    }

    #[test]
    fn failed_job_setup_invalidates_validation_and_confirmation() {
        let root = tempfile::tempdir().unwrap();
        let game = root.path().join("game");
        let mods = root.path().join("mods");
        std::fs::create_dir(&game).unwrap();
        std::fs::create_dir(&mods).unwrap();
        let mut app = App::new(None, Some(game.clone()), Some(mods));
        let ctx = egui::Context::default();
        for job in [
            Job::Conflicts,
            Job::Validate,
            Job::Install,
            Job::Restore,
            Job::Download,
        ] {
            app.validated = true;
            app.confirm_install = true;
            app.expanded_budget = "invalid".into();
            app.start(job, &ctx);
            assert!(!app.validated && !app.confirm_install);
            assert!(app.receiver.is_none());
            assert!(app.logs.last().unwrap().contains("nonnegative integer"));
        }
        app.expanded_budget.clear();
        app.fomod_choices = root
            .path()
            .join("missing-choices.json")
            .to_string_lossy()
            .into_owned();
        for job in [Job::Conflicts, Job::Validate, Job::Install] {
            app.validated = true;
            app.confirm_install = true;
            app.start(job, &ctx);
            assert!(!app.validated && !app.confirm_install);
            assert!(app.receiver.is_none());
            assert!(app.logs.last().unwrap().starts_with("FOMOD choices:"));
        }
        std::fs::remove_dir(&game).unwrap();
        app.expanded_budget.clear();
        app.validated = true;
        app.confirm_install = true;
        app.start(Job::Install, &ctx);
        assert!(!app.validated && !app.confirm_install);
        assert!(app.receiver.is_none());
        assert!(app.logs.last().unwrap().starts_with("Check directories:"));
    }

    #[test]
    fn option_without_raw_description_renders_spoiler_description() {
        for authored in [false, true] {
            let mut option = Component {
                name: "Option".into(),
                ..Default::default()
            };
            let expected = if authored {
                option.metadata.insert(
                    "DescriptionSpoilerFree".into(),
                    "Authored safe description".into(),
                );
                "Authored safe description"
            } else {
                "This modification enhances your gameplay."
            };
            let mut component = Component {
                options: vec![option],
                ..Default::default()
            };
            let ctx = egui::Context::default();
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    App::options(ui, &mut component, true);
                });
            });
            assert!(output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == expected)
            }), "missing rendered description: {expected}");
        }
    }

    use super::*;

    #[test]
    fn desktop_profile_capture_and_activation_preserve_loaded_guide_and_reset_validation() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("guide.json");
        std::fs::write(
            &input,
            r#"[{"Guid":"11111111-1111-4111-8111-111111111111","Name":"Mod","IsSelected":true}]"#,
        )
        .unwrap();
        let mut app = App::new(
            Some(input.clone()),
            Some(dir.path().join("game")),
            Some(dir.path().join("mods")),
        );
        app.profile_directory = dir.path().join("settings").to_string_lossy().into_owned();
        app.profile_name = "Desktop".into();
        app.guide.as_mut().unwrap().components[0].is_selected = false;
        app.input = "edited-but-not-loaded.json".into();
        app.profile_action(false).unwrap();
        let store = modsync::profiles::Store::new(Path::new(&app.profile_directory)).unwrap();
        assert_eq!(
            store
                .require("Desktop")
                .unwrap()
                .instruction_file_path
                .as_deref(),
            input.to_str()
        );
        app.guide.as_mut().unwrap().components[0].is_selected = true;
        app.game = "different".into();
        app.mods = "different".into();
        app.validated = true;
        app.confirm_install = true;
        app.profile_action(true).unwrap();
        assert!(!app.guide.as_ref().unwrap().components[0].is_selected);
        assert_eq!(Path::new(&app.game), dir.path().join("game"));
        assert_eq!(Path::new(&app.mods), dir.path().join("mods"));
        assert!(!app.validated && !app.confirm_install);
        assert_eq!(store.active_name().unwrap().as_deref(), Some("Desktop"));
        std::fs::write(dir.path().join("settings/settings.json"), "broken").unwrap();
        app.guide.as_mut().unwrap().components[0].is_selected = true;
        app.game = "unchanged".into();
        assert!(app.profile_action(true).is_err());
        assert!(app.guide.as_ref().unwrap().components[0].is_selected);
        assert_eq!(app.game, "unchanged");
    }
    #[test]
    fn desktop_profile_management_checks_confirmation_and_refreshes_names() {
        let dir = tempfile::tempdir().unwrap();
        let store = modsync::profiles::Store::new(dir.path()).unwrap();
        store.create("Original").unwrap();
        let mut app = App::new(None, None, None);
        app.profile_directory = dir.path().to_string_lossy().into_owned();
        app.manage_profile("Refresh").unwrap();
        assert_eq!(app.profile_names, ["Original"]);
        app.profile_name = "Original".into();
        app.profile_target = "Copy".into();
        app.manage_profile("Clone").unwrap();
        assert_eq!(app.profile_name, "Copy");
        app.profile_target = "Renamed".into();
        app.manage_profile("Rename").unwrap();
        assert_eq!(app.profile_names, ["Original", "Renamed"]);
        store.activate("Renamed").unwrap();
        assert!(app.manage_profile("Delete").is_err());
        app.profile_delete = Some((app.profile_directory.clone(), "Original".into()));
        assert!(app.manage_profile("Delete").is_err());
        app.profile_delete = Some((app.profile_directory.clone(), app.profile_name.clone()));
        app.manage_profile("Delete").unwrap();
        assert_eq!(app.profile_names, ["Original"]);
        assert_eq!(store.active_name().unwrap(), None);
        assert!(app.profile_delete.is_none());
    }
    #[test]
    fn desktop_restores_saved_directories_but_preserves_explicit_launch_paths() {
        let dir = tempfile::tempdir().unwrap();
        let store = modsync::profiles::Store::new(dir.path()).unwrap();
        store
            .save_directory_preferences("saved-mods", "saved-game")
            .unwrap();
        let mut app = App::new(None, None, Some(PathBuf::from("explicit-mods")));
        app.profile_directory = dir.path().to_string_lossy().into_owned();
        app.restore_directory_preferences().unwrap();
        assert_eq!(app.mods, "explicit-mods");
        assert_eq!(app.game, "saved-game");
        assert!(!app.validated && !app.confirm_install);
        std::fs::write(
            dir.path().join("settings.json"),
            r#"{"sourcePath":"other","destinationPath":42}"#,
        )
        .unwrap();
        assert!(app.restore_directory_preferences().is_err());
        assert_eq!(app.mods, "explicit-mods");
        assert_eq!(app.game, "saved-game");
    }
    #[test]
    fn themes_persist_and_failed_save_keeps_displayed_theme() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::new(None, None, None);
        app.profile_directory = dir.path().to_string_lossy().into_owned();
        let ctx = egui::Context::default();
        for path in [
            "/Styles/KotorStyle.axaml",
            "/Styles/Kotor2Style.axaml",
            "/Styles/LightStyle.axaml",
        ] {
            app.change_theme(path, &ctx).unwrap();
            assert_eq!(ctx.style().visuals.dark_mode, !path.contains("Light"));
            assert_eq!(
                modsync::profiles::Store::new(dir.path())
                    .unwrap()
                    .theme_preference()
                    .unwrap(),
                path
            );
        }
        std::fs::write(dir.path().join("settings.json"), "broken").unwrap();
        assert!(app.change_theme("/Styles/KotorStyle.axaml", &ctx).is_err());
        assert!(!ctx.style().visuals.dark_mode);
        assert_eq!(app.theme, "/Styles/LightStyle.axaml");
    }
    #[test]
    fn multiple_categories_apply_together_without_clearing_existing_selections() {
        let component = |category: &str| Component {
            category: vec![category.into()],
            ..Default::default()
        };
        let mut app = App::new(None, None, None);
        let mut guide = Guide {
            components: vec![
                component("Fix"),
                component("Texture"),
                component("Other"),
                component("Fix"),
            ],
            ..Default::default()
        };
        guide.components[2].is_selected = true;
        guide.components[3]
            .metadata
            .insert("WidescreenOnly".into(), true.into());
        app.guide = Some(guide);
        app.validated = true;
        app.confirm_install = true;
        app.category_selection = Some([("fix".into(), true), ("Texture".into(), true)].into());
        app.apply_category_selection();
        assert_eq!(
            app.guide
                .as_ref()
                .unwrap()
                .components
                .iter()
                .map(|c| c.is_selected)
                .collect::<Vec<_>>(),
            [true, true, true, false]
        );
        assert!(!app.validated && !app.confirm_install);
        assert!(app.category_selection.is_none());
    }

    #[test]
    fn post_download_fomod_prompt_obeys_registered_status() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("package.zip");
        let mut app = App::new(None, None, None);
        app.guide = Some(Guide::parse(
            r#"{"thisMod":[{"Guid":"a","Name":"Registered","IsSelected":true,"ResourceRegistry":{"source":{"Files":{"package.zip":true},"HandlerMetadata":{"fomodPromptStatus":{"package.zip":"configured"}}}}}]}"#,
            "json",
        ).unwrap());
        assert!(!app.should_offer_fomod_editor(&path));
        app.guide.as_mut().unwrap().components[0].resource_registry["source"]["HandlerMetadata"]
            ["fomodPromptStatus"]["package.zip"] = "dismissed".into();
        assert!(app.should_offer_fomod_editor(&path));
        assert!(app.should_offer_fomod_editor(&temp.path().join("unregistered.zip")));
    }
}
