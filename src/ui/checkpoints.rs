//! Native checkpoint presentation. All store access runs in the shared worker.
use super::{App, Job, Message, SharingContinuation};
use anyhow::Result;
use eframe::egui;
use modsync::checkpoint::{
    ExpectedHead, HistoryReport, ListReport, ResumeRequest, Store, TreeRoot,
};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

#[derive(Clone, Debug, Default)]
pub(super) struct Config {
    pub enabled: bool,
    pub managed: bool,
    pub staging: String,
    pub manifests: String,
}
impl Config {
    pub fn engine(
        &self,
        mut engine: modsync::Engine,
        roots: &modsync::paths::Roots,
        settings: &str,
        profile: &str,
    ) -> Result<modsync::Engine> {
        engine = engine.with_component_checkpoints(self.enabled);
        if self.managed {
            anyhow::ensure!(
                self.enabled,
                "Managed installation requires component checkpoints"
            );
            let service = modsync::deployment::Service::resolve_readonly(
                &roots.game,
                (!self.staging.trim().is_empty()).then(|| Path::new(&self.staging)),
                (!self.manifests.trim().is_empty()).then(|| Path::new(&self.manifests)),
                (!settings.trim().is_empty()).then(|| Path::new(settings)),
                (!profile.trim().is_empty()).then_some(profile),
            )?;
            engine = engine.with_managed_deployment(service)?;
        }
        Ok(engine)
    }
}

#[derive(Clone)]
pub(super) enum Action {
    List,
    History(String),
    Inspect(String, String),
    Diff(String, String, String),
    Provenance(String, String, bool, String),
    Verify(String, String),
    Capture,
    Gc,
    Delete(ExpectedHead),
    Repair(String, String),
    Restore(ExpectedHead, String),
    Recover(Option<String>),
    Resume(ResumeRequest),
}
impl Action {
    fn writes(&self) -> bool {
        matches!(
            self,
            Self::Capture
                | Self::Gc
                | Self::Delete(..)
                | Self::Repair(..)
                | Self::Restore(..)
                | Self::Recover(..)
                | Self::Resume(..)
        )
    }
    fn changes_install(&self) -> bool {
        matches!(
            self,
            Self::Restore(..) | Self::Recover(..) | Self::Resume(..)
        )
    }
}

pub(super) struct Finished {
    pub run: u64,
    pub context: String,
    pub folders: String,
    pub result: Result<Output>,
}
pub(super) struct Output {
    release_sharing: bool,
    list: Option<ListReport>,
    history: Option<HistoryReport>,
    pending: Option<usize>,
    text: String,
}

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    pub config: Config,
    pub resume: Option<ResumeRequest>,
    pub resumed_run: Option<u64>,
    run: u64,
    context: String,
    list: Option<ListReport>,
    history: Option<HistoryReport>,
    pending: Option<usize>,
    session: String,
    selected: String,
    from: String,
    path: String,
    source_tree: bool,
    accept_sources: bool,
    text: String,
    progress: String,
    confirm: Option<(Action, String)>,
}

impl State {
    pub fn progress(&mut self, run: u64, text: String) {
        if self.run == run {
            self.progress = text;
        }
    }
}

impl App {
    fn checkpoint_invalidate(&mut self) {
        self.invalidate_validation();
        self.completed_base = None;
        self.pending_base.clear();
        self.install_context = None;
        self.finished = false;
        self.phase_failed = false;
        self.completion = Default::default();
        self.checkpoints.pending = None;
    }

    pub(super) fn checkpoint_start(
        &mut self,
        action: Action,
        context: String,
        ctx: &egui::Context,
    ) {
        self.refresh_input_stamp();
        if self.input_stamp != context {
            self.checkpoints.text =
                "Inputs changed; refresh checkpoint history before acting.".into();
            return;
        }
        if self.receiver.is_some()
            || self.desktop.busy()
            || self.fomod_pending()
            || self.sharing.closing
        {
            return;
        }
        if action.writes()
            && !self.sharing_before(SharingContinuation::Checkpoint(
                action.clone(),
                context.clone(),
            ))
        {
            return;
        }
        if action.changes_install() {
            self.checkpoint_invalidate();
        }
        if action.writes() {
            self.checkpoints.history = None;
        }
        if let Action::Resume(request) = action {
            self.checkpoints.resume = Some(request);
            self.start(Job::Install, ctx);
            return;
        }
        self.checkpoints.run = self.checkpoints.run.wrapping_add(1);
        let run = self.checkpoints.run;
        self.checkpoints.context = context.clone();
        self.checkpoints.progress = "Working…".into();
        self.checkpoints.pending = None;
        let mods = self.mods.clone();
        let game = self.game.clone();
        let folders = format!("Mods: {mods}\nGame: {game}");
        self.cancelled = Arc::new(AtomicBool::new(false));
        let flag = self.cancelled.clone();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.job = Job::Checkpoint;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<Output> {
                let roots = modsync::paths::Roots::new(Path::new(&mods), Path::new(&game))?;
                let store = Store::open(&roots)?;
                let progress = |p: modsync::checkpoint::Progress| {
                    let _ = tx.send(Message::CheckpointProgress(run, format!("{}: {} files, {} bytes", p.phase, p.files, p.bytes)));
                    ctx.request_repaint();
                };
                let writes = action.writes();
                let mut list = None;
                let mut history = None;
                let value = match action {
                    Action::List => { list = Some(store.list()?); serde_json::to_value(&list)? }
                    Action::History(id) => {
                        history = Some(store.history(&id)?);
                        let mut value = serde_json::to_value(&history)?;
                        value["capability"] = serde_json::to_value(store.capability(&id)?)?;
                        value
                    }
                    Action::Inspect(id, checkpoint) => serde_json::to_value(store.inspect(&id, Some(&checkpoint))?)?,
                    Action::Diff(id, from, to) => serde_json::to_value(store.diff(&id, &from, &to)?)?,
                    Action::Provenance(id, checkpoint, source, path) => serde_json::to_value(store.provenance(&id, Some(&checkpoint), if source { TreeRoot::Mods } else { TreeRoot::Game }, &path)?)?,
                    Action::Verify(id, checkpoint) => serde_json::to_value(store.verify(&id, Some(&checkpoint), &flag)?)?,
                    Action::Capture => serde_json::to_value(store.capture(&flag, progress)?)?,
                    Action::Restore(expected, checkpoint) => {
                        store.prepare_write()?;
                        let lock = modsync::recovery::Lock::acquire(&roots)?;
                        serde_json::to_value(store.restore_checkpoint(&lock, &expected, &checkpoint, &flag, progress)?)?
                    }
                    Action::Recover(id) => {
                        store.prepare_write()?;
                        let lock = modsync::recovery::Lock::acquire(&roots)?;
                        serde_json::to_value(store.recover_with_process_quiescence(&lock, id.as_deref(), &flag, progress)?)?
                    }
                    Action::Gc => {
                        store.prepare_write()?;
                        let lock = modsync::recovery::Lock::acquire(&roots)?;
                        serde_json::to_value(store.gc(&lock, &flag, progress)?)?
                    }
                    Action::Delete(expected) => {
                        store.prepare_write()?;
                        let lock = modsync::recovery::Lock::acquire(&roots)?;
                        serde_json::to_value(store.delete_session(&lock, &expected, &flag)?)?
                    }
                    Action::Repair(id, checkpoint) => {
                        store.prepare_write()?;
                        let lock = modsync::recovery::Lock::acquire(&roots)?;
                        serde_json::to_value(store.repair(&lock, &id, Some(&checkpoint), &flag, progress)?)?
                    }
                    Action::Resume(_) => unreachable!(),
                };
                let mut text = format!("{}{}", if value.get("complete").and_then(serde_json::Value::as_bool) == Some(false) { "Checkpoint operation incomplete; inspect the recovery or unresolved-object details below.\n" } else { "" }, serde_json::to_string_pretty(&value)?);
                let pending = match store.pending_operations().and_then(|pending| {
                    let external = store.external_process_status()?;
                    text.push_str(&format!("\nExternal process status:\n{}", serde_json::to_string_pretty(&external)?));
                    Ok(pending.len() + usize::from(external.present))
                }) {
                    Ok(pending) => Some(pending),
                    Err(error) => { text.push_str(&format!("\nReadiness unavailable: {error:#}")); None }
                };
                Ok(Output { list, history, pending, text, release_sharing: writes && pending == Some(0) && value.get("complete").and_then(serde_json::Value::as_bool) != Some(false) })
            })).unwrap_or_else(|_| Err(anyhow::anyhow!("Checkpoint worker stopped unexpectedly; recover pending work before installing.")));
            let _ = tx.send(Message::CheckpointTerminal(Box::new(Finished {
                run,
                context,
                folders,
                result,
            })));
            ctx.request_repaint();
        });
    }

    pub(super) fn checkpoint_terminal(&mut self, result: Finished) {
        if result.run != self.checkpoints.run {
            return;
        }
        self.receiver = None;
        self.refresh_input_stamp();
        self.checkpoints.progress.clear();
        let accepted = result.context == self.input_stamp;
        match result.result {
            Ok(output) => {
                self.checkpoints.text = format!("{}\n{}", result.folders, output.text);
                if accepted {
                    if output.release_sharing {
                        self.sharing.resume_after_mutation(self.guide.as_ref());
                    }
                    self.checkpoints.context = result.context;
                    self.checkpoints.pending = output.pending;
                    if let Some(list) = output.list {
                        self.checkpoints.list = Some(list);
                        self.checkpoints.history = None;
                        self.checkpoints.selected.clear();
                    }
                    if let Some(history) = output.history {
                        self.checkpoints.session = history.session_id.clone();
                        self.checkpoints.selected.clear();
                        self.checkpoints.from.clear();
                        self.checkpoints.history = Some(history);
                    }
                } else {
                    self.checkpoints.pending = None;
                    self.checkpoints
                        .text
                        .push_str("\nCurrent inputs changed. Refresh to use these folders again.");
                }
            }
            Err(error) => {
                self.checkpoints.pending = None;
                self.checkpoints.text = format!(
                    "{}\nOperation stopped: {error:#}\nRefresh history or recover interrupted work before further mutation.",
                    result.folders
                );
            }
        }
    }

    pub(super) fn checkpoint_window(&mut self, ctx: &egui::Context) {
        if !self.checkpoints.open {
            return;
        }
        self.refresh_input_stamp();
        let stamp = self.input_stamp.clone();
        let available = !self.desktop.busy()
            && self.receiver.is_none()
            && self.sharing_continuation.is_none()
            && !self.sharing.closing
            && !self.fomod_pending();
        let state = &mut self.checkpoints;
        let mut open = true;
        let mut action = None;
        let mut action_context = stamp.clone();
        egui::Window::new("Checkpoint history").open(&mut open).default_size([780.0, 680.0]).vscroll(true).show(ctx, |ui| {
            ui.label(format!("Mods: {}\nGame: {}", self.mods, self.game));
            ui.add_enabled_ui(available, |ui| {
                ui.collapsing("Installation mode", |ui| {
                    ui.checkbox(&mut state.config.enabled, "Retain a checkpoint after every component");
                    ui.label("Default: whole-run recovery. Component checkpoints retain successful boundaries for ordered restore and resume.");
                    ui.add_enabled_ui(state.config.enabled, |ui| { ui.checkbox(&mut state.config.managed, "Track managed deployment"); });
                    if !state.config.enabled { state.config.managed = false; }
                    if state.config.managed {
                        ui.horizontal(|ui| { ui.label("Settings directory"); ui.text_edit_singleline(&mut self.profile_directory); });
                        ui.label(format!("Active managed profile: {}", if self.active_profile_name.is_empty() { "(none)" } else { &self.active_profile_name }));
                        ui.label("Activate a profile through Install profiles, or supply both directories:");
                        ui.horizontal(|ui| { ui.label("Staging"); ui.text_edit_singleline(&mut state.config.staging); });
                        ui.horizontal(|ui| { ui.label("Manifests"); ui.text_edit_singleline(&mut state.config.manifests); });
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Refresh sessions").clicked() { action = Some(Action::List); }
                    if ui.add_enabled(state.context == stamp && state.pending == Some(0), egui::Button::new("Capture baseline…")).clicked() { state.confirm = Some((Action::Capture, stamp.clone())); }
                    if ui.button("Recover pending work…").clicked() { state.confirm = Some((Action::Recover(None), stamp.clone())); }
                    if ui.add_enabled(state.context == stamp && state.pending == Some(0), egui::Button::new("Garbage collect…")).clicked() { state.confirm = Some((Action::Gc, stamp.clone())); }
                });
                if let Some(list) = &state.list {
                    egui::ScrollArea::vertical().id_salt("checkpoint_sessions").max_height(130.0).show(ui, |ui| {
                        for session in &list.sessions {
                            if ui.selectable_label(state.session == session.session_id, format!("{} — {}", session.session_id, session.status)).clicked() { action = Some(Action::History(session.session_id.clone())); }
                        }
                    });
                }
                let current = state.context == stamp;
                let clean = current && state.pending == Some(0);
                ui.label(match state.pending { Some(n) if current => format!("Pending operations: {n}"), _ => "Readiness unavailable; refresh history.".into() });
                if let Some(history) = &state.history {
                    ui.label(format!("Session {} — {} — generation {}", history.session_id, history.status, history.generation));
                    egui::ScrollArea::vertical().id_salt("checkpoint_entries").max_height(170.0).show(ui, |ui| {
                        for entry in &history.entries {
                            let name = entry.component.as_ref().map_or(entry.kind.as_str(), |c| c.name.as_str());
                            if ui.selectable_label(state.selected == entry.checkpoint_id, format!("{}: {} — {} completed — {} bytes", entry.sequence, name, entry.completed_components, entry.total_bytes)).clicked() {
                                state.selected = entry.checkpoint_id.clone();
                            }
                        }
                    });
                    let selected = !state.selected.is_empty() && current;
                    let id = &history.session_id;
                    ui.add_enabled_ui(selected, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Repair stored objects…").clicked() { state.confirm = Some((Action::Repair(id.clone(), state.selected.clone()), stamp.clone())); }
                            if ui.button("Inspect").clicked() { action = Some(Action::Inspect(id.clone(), state.selected.clone())); }
                            if ui.add_enabled(clean, egui::Button::new("Verify objects")).clicked() { action = Some(Action::Verify(id.clone(), state.selected.clone())); }
                            if ui.button("Use as diff origin").clicked() { state.from = state.selected.clone(); }
                            if ui.add_enabled(!state.from.is_empty(), egui::Button::new("Diff origin → selected")).clicked() { action = Some(Action::Diff(id.clone(), state.from.clone(), state.selected.clone())); }
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(&mut state.source_tree, "Source tree");
                            ui.text_edit_singleline(&mut state.path);
                            if ui.button("Path provenance").clicked() { action = Some(Action::Provenance(id.clone(), state.selected.clone(), state.source_tree, state.path.clone())); }
                        });
                        let expected = ExpectedHead { session_id: id.clone(), checkpoint_id: history.checkpoint_id.clone(), generation: history.generation };
                        ui.horizontal_wrapped(|ui| {
                            if ui.add_enabled(clean, egui::Button::new("Delete session…")).clicked() { state.confirm = Some((Action::Delete(expected.clone()), stamp.clone())); }
                            if ui.button("Recover this session…").clicked() { state.confirm = Some((Action::Recover(Some(id.clone())), stamp.clone())); }
                            if ui.add_enabled(clean && history.status != "Detached", egui::Button::new("Restore selected…")).clicked() { state.confirm = Some((Action::Restore(expected.clone(), state.selected.clone()), stamp.clone())); }
                            if ui.add_enabled(clean && matches!(history.status.as_str(), "Ready" | "Restored" | "Complete") && state.selected == history.checkpoint_id, egui::Button::new("Resume from HEAD…")).clicked() { state.confirm = Some((Action::Resume(ResumeRequest { expected, accept_source_changes: state.accept_sources }), stamp.clone())); }
                        });
                        ui.checkbox(&mut state.accept_sources, "Accept changed source files after a successful restore");
                        ui.label("Resume uses every selected component, including widescreen components. The engine verifies the saved prefix and runs only its remaining suffix.");
                    });
                }
                if let Some((pending, captured_context)) = state.confirm.clone() {
                    ui.separator();
                    ui.label("Proceed for these captured folders? Restore/recovery may replace files. Delete removes retained session history; garbage collection removes unreferenced stored data. Repair changes stored objects only. Resume executes the verified remaining guide. Cancellation retains any published recovery fence.");
                    ui.horizontal(|ui| {
                        if ui.button("Confirm operation").clicked() { action = Some(pending); action_context = captured_context; state.confirm = None; }
                        if ui.button("Cancel operation").clicked() { state.confirm = None; }
                    });
                }
            });
            if !state.progress.is_empty() { ui.label(&state.progress); if ui.button("Stop checkpoint work").clicked() { self.cancelled.store(true, Ordering::Release); } }
            if ui.button("Copy report").clicked() { ctx.copy_text(state.text.clone()); }
            ui.add(egui::TextEdit::multiline(&mut state.text).desired_rows(15).interactive(false).font(egui::TextStyle::Monospace));
        });
        self.checkpoints.open = open;
        self.refresh_input_stamp();
        if let Some(action) = action {
            self.checkpoint_start(action, action_context, ctx);
        }
    }
}
