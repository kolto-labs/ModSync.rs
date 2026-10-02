//! Reviewed maintenance over the shared service, with an owned desktop worker.
use super::{App, Job, Message, SharingContinuation};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use modsync::maintenance::{self, Action, Preview, Status};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
};

#[derive(Default)]
pub(super) struct State {
    open: bool,
    permissions: bool,
    mods: bool,
    subtree: String,
    preview: Option<Preview>,
    preview_context: String,
    status: Option<Status>,
    status_context: String,
    text: String,
    error: String,
    check_status: bool,
    focus_cancel: bool,
    worker: Option<JoinHandle<()>>,
    terminal: Option<mpsc::Receiver<Finished>>,
    cancelled: Option<Arc<AtomicBool>>,
    // Keep the original roots bound until their recovery status is known idle.
    unsettled_roots: Option<(String, String)>,
}

impl State {
    pub(super) fn closed(&self) -> bool {
        self.worker.is_none()
    }
    pub(super) fn recovery_pending(&self) -> bool {
        self.status.as_ref().is_some_and(|status| status.pending)
    }
    pub(super) fn resume_allowed(&self, context: &str, manual_resume_required: bool) -> bool {
        self.closed()
            && self.unsettled_roots.is_none()
            && !self.recovery_pending()
            && (!manual_resume_required
                || (self.status_context == context && self.status.is_some()))
    }
}

impl Drop for State {
    fn drop(&mut self) {
        if let Some(flag) = &self.cancelled {
            flag.store(true, Ordering::Release);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Clone)]
pub(super) enum Work {
    Preview,
    Apply(String),
    Status,
    Recover(String),
}
impl Work {
    fn writes(&self) -> bool {
        matches!(self, Self::Apply(_) | Self::Recover(_))
    }
}

#[derive(Clone)]
pub(super) struct Request {
    work: Work,
    context: String,
    game: String,
    source: String,
    permissions: bool,
    mods: bool,
    subtree: String,
}

enum Output {
    Preview(Preview),
    Outcome(maintenance::Outcome),
    Status,
}
struct Finished {
    request: Request,
    result: Result<Output>,
    status: Result<Option<Status>>,
}

impl App {
    pub(super) fn maintenance_open(&mut self, permissions: bool) {
        if !self.maintenance.closed() {
            return;
        }
        if self.maintenance.permissions != permissions {
            self.maintenance.preview = None;
            self.maintenance.text.clear();
        }
        self.maintenance.permissions = permissions;
        self.maintenance.open = true;
        self.maintenance.focus_cancel = true;
        self.maintenance.check_status = true;
    }

    fn maintenance_request(&mut self, work: Work) -> Request {
        self.refresh_input_stamp();
        Request {
            work,
            context: self.input_stamp.clone(),
            game: self.game.clone(),
            source: self.mods.clone(),
            permissions: self.maintenance.permissions,
            mods: self.maintenance.mods,
            subtree: self.maintenance.subtree.clone(),
        }
    }

    pub(super) fn maintenance_start(&mut self, mut request: Request, ctx: &egui::Context) {
        if self.receiver.is_some()
            || !self.maintenance.closed()
            || self.desktop.busy()
            || self.sharing_continuation.is_some()
            || self.sharing.closing
            || self.fomod_pending()
        {
            return;
        }
        self.refresh_input_stamp();
        if request.context != self.input_stamp
            || request.game != self.game
            || request.source != self.mods
            || request.permissions != self.maintenance.permissions
            || request.mods != self.maintenance.mods
            || request.subtree != self.maintenance.subtree
        {
            self.maintenance.preview = None;
            self.maintenance.error =
                "Inputs changed; inspect status and create a new preview.".into();
            return;
        }
        if request.work.writes()
            && self
                .maintenance
                .unsettled_roots
                .as_ref()
                .is_some_and(|(game, source)| game != &request.game || source != &request.source)
        {
            self.maintenance.error = "Restore the original configured roots and verify their recovery status before another mutation.".into();
            return;
        }
        if let Work::Apply(digest) = &request.work
            && (self.maintenance.preview_context != request.context
                || self
                    .maintenance
                    .preview
                    .as_ref()
                    .is_none_or(|preview| &preview.digest != digest)
                || self.maintenance.status_context != request.context
                || self
                    .maintenance
                    .status
                    .as_ref()
                    .is_none_or(|status| status.pending))
        {
            self.maintenance.error = "The reviewed preview is no longer current.".into();
            return;
        }
        if let Work::Recover(digest) = &request.work
            && (self.maintenance.status_context != request.context
                || self
                    .maintenance
                    .status
                    .as_ref()
                    .is_none_or(|status| !status.pending || status.digest.as_ref() != Some(digest)))
        {
            self.maintenance.error = "Refresh status before recovering this operation.".into();
            return;
        }
        if request.work.writes() {
            self.sharing.require_manual_resume();
        }
        if request.work.writes()
            && !self.sharing_before(SharingContinuation::Maintenance(request.clone()))
        {
            self.maintenance.error.clear();
            self.maintenance.text = "Waiting for sharing to stop before maintenance…".into();
            return;
        }
        if request.work.writes() {
            self.reset_installation();
            self.refresh_input_stamp();
            request.context = self.input_stamp.clone();
            self.maintenance.preview = None;
        }
        self.maintenance.error.clear();
        self.maintenance.text = "Inspecting maintenance state…".into();
        self.cancelled = Arc::new(AtomicBool::new(false));
        let flag = self.cancelled.clone();
        self.maintenance.cancelled = Some(flag.clone());
        let (busy, receiver) = mpsc::channel::<Message>();
        let (tx, terminal) = mpsc::channel();
        self.receiver = Some(receiver);
        self.job = if request.work.writes() {
            Job::Editor
        } else {
            Job::UpdateSummary
        };
        let ctx = ctx.clone();
        let request_writes = request.work.writes();
        let request_roots = (request.game.clone(), request.source.clone());
        let worker = std::thread::Builder::new().name("desktop-maintenance".into()).spawn(move || {
            // Hold the ordinary app gate until the owned worker has fully joined.
            let _busy = busy;
            let roots = crate::maintenance_cli::resolve_roots(Path::new(&request.source), Path::new(&request.game));
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<Output> {
                let roots = roots.as_ref().map_err(|error| anyhow::anyhow!("{error:#}"))?;
                ensure!(!flag.load(Ordering::Acquire), "Maintenance cancelled before starting");
                let core = maintenance::Request {
                    roots: roots.clone(),
                    target: if request.mods { maintenance::TreeRoot::Mods } else { maintenance::TreeRoot::Game },
                    subtree: request.subtree.clone(),
                    action: if request.permissions { Action::OwnerWritable } else { Action::LowercaseNames },
                };
                match &request.work {
                    Work::Preview => maintenance::preview(&core, &flag).map(Output::Preview),
                    Work::Apply(digest) => maintenance::apply(&core, digest, &flag).map(Output::Outcome),
                    Work::Status => Ok(Output::Status),
                    Work::Recover(digest) => maintenance::recover(roots, digest, &flag).map(Output::Outcome),
                }
            })).unwrap_or_else(|_| Err(anyhow::anyhow!("Maintenance worker stopped unexpectedly; inspect recovery status before further writes.")));
            let status = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                roots.as_ref().map_or_else(
                    |_| Ok(None),
                    |roots| maintenance::status(roots).map(Some),
                )
            })).unwrap_or_else(|_| Err(anyhow::anyhow!("Recovery status worker stopped unexpectedly")));
            let _ = tx.send(Finished { request, result, status });
            ctx.request_repaint();
        });
        match worker {
            Ok(worker) => {
                if request_writes {
                    self.maintenance.unsettled_roots = Some(request_roots);
                }
                self.maintenance.worker = Some(worker);
                self.maintenance.terminal = Some(terminal);
            }
            Err(error) => {
                self.receiver = None;
                self.maintenance.cancelled = None;
                self.maintenance.error = format!("Could not start maintenance worker: {error}");
            }
        }
    }

    pub(super) fn poll_maintenance(&mut self) {
        if !self
            .maintenance
            .worker
            .as_ref()
            .is_some_and(JoinHandle::is_finished)
        {
            return;
        }
        let joined = self
            .maintenance
            .worker
            .take()
            .expect("Finished maintenance worker")
            .join();
        let finished = self
            .maintenance
            .terminal
            .take()
            .and_then(|receiver| receiver.try_recv().ok());
        let tail_failed = joined.is_err();
        self.maintenance.cancelled = None;
        self.receiver = None;
        self.refresh_input_stamp();
        // The queued terminal owns mutation facts even if the worker panics
        // after sending it. Joining remains mandatory before releasing the gate.
        let finished = finished.context(if tail_failed {
            "Maintenance worker stopped unexpectedly without an outcome"
        } else {
            "Maintenance worker returned no outcome"
        });
        match finished {
            Err(error) => {
                self.maintenance.preview = None;
                self.maintenance.status = None;
                self.maintenance.error =
                    format!("{error:#}; refresh status before further writes.");
            }
            Ok(finished) => {
                let current = finished.request.context == self.input_stamp;
                let roots_binding = (
                    finished.request.game.clone(),
                    finished.request.source.clone(),
                );
                let mut recovery_required = false;
                self.maintenance.preview = None;
                let mut lines = Vec::new();
                if tail_failed {
                    lines.push("Worker stopped after returning the outcome; the recorded mutation result and recovery status are retained.".into());
                }
                match finished.result {
                    Ok(Output::Preview(preview)) => {
                        lines.push(format!(
                            "Preview: {} entries, {} planned operations\nDigest: {}",
                            preview.entry_count, preview.operation_count, preview.digest
                        ));
                        lines.push(serde_json::to_string_pretty(&preview).unwrap_or_default());
                        if current {
                            self.maintenance.preview_context = finished.request.context.clone();
                            self.maintenance.preview = Some(preview);
                        }
                    }
                    Ok(Output::Outcome(outcome)) => {
                        recovery_required =
                            outcome.status == maintenance::OutcomeStatus::RecoveryRequired;
                        if crate::maintenance_cli::outcome_exit(&outcome) != 0 {
                            self.maintenance.error = format!(
                                "Maintenance outcome: {:?}. Inspect the result and recovery state.",
                                outcome.status
                            );
                        }
                        lines.push(serde_json::to_string_pretty(&outcome).unwrap_or_default());
                    }
                    Ok(Output::Status) => {}
                    Err(error) => {
                        self.maintenance.error = format!("{error:#}");
                    }
                }
                match finished.status {
                    Ok(status) => {
                        if let Some(status) = &status {
                            lines.push(serde_json::to_string_pretty(status).unwrap_or_default());
                        }
                        if status.as_ref().is_some_and(|status| status.pending) || recovery_required
                        {
                            self.maintenance.unsettled_roots = Some(roots_binding.clone());
                        } else if status.is_some()
                            && self
                                .maintenance
                                .unsettled_roots
                                .as_ref()
                                .is_some_and(|roots| roots == &roots_binding)
                        {
                            self.maintenance.unsettled_roots = None;
                        }
                        self.maintenance.status = if current { status } else { None };
                        self.maintenance.status_context = finished.request.context.clone();
                    }
                    Err(error) => {
                        self.maintenance.status = None;
                        if self.maintenance.unsettled_roots.is_none() {
                            self.maintenance.unsettled_roots = Some(roots_binding.clone());
                        }
                        lines.push(format!("Recovery status unavailable: {error:#}"));
                    }
                }
                if !current {
                    lines.push(
                        "Result belongs to previous inputs; refresh status and preview.".into(),
                    );
                }
                if let Some((game, source)) = &self.maintenance.unsettled_roots {
                    lines.push(format!("Sharing stays paused until recovery is verified for game root {game} and mod root {source}."));
                }
                if finished.request.work.writes() {
                    lines.push(
                        "Sharing remains paused. Inspect the result before resuming sharing."
                            .into(),
                    );
                }
                self.maintenance.text = lines.join("\n\n");
                self.logs.push(self.maintenance.text.clone());
            }
        }
    }

    pub(super) fn maintenance_window(&mut self, ctx: &egui::Context) {
        if !self.maintenance.open {
            return;
        }
        self.refresh_input_stamp();
        let busy = self.receiver.is_some()
            || !self.maintenance.closed()
            || self.desktop.busy()
            || self.sharing_continuation.is_some()
            || self.sharing.closing
            || self.fomod_pending();
        let mut work = None;
        let mut close = false;
        egui::Modal::new(egui::Id::new("maintenance_dialog")).show(ctx, |ui| {
            ui.set_max_width(760.0);
            ui.heading(if self.maintenance.permissions { "Fix file/folder permissions" } else { "Fix iOS case sensitivity" });
            ui.label(if self.maintenance.permissions {
                "Make the selected subtree usable by its current owner. Ownership takeover and unsupported ACL repairs are refused; no Everyone Full Control or chmod 777 is applied."
            } else {
                "Preview ASCII lowercase names inside the selected subtree. Collisions stop the operation; the selected root's own name stays unchanged."
            });
            ui.label(format!("Game root: {}\nMod root: {}", self.game, self.mods));
            ui.add_enabled_ui(!busy, |ui| {
                let mut changed = false;
                ui.horizontal(|ui| {
                    changed |= ui.selectable_value(&mut self.maintenance.mods, false, "Game directory").changed();
                    changed |= ui.selectable_value(&mut self.maintenance.mods, true, "Mod directory").changed();
                });
                ui.label("Relative subtree (empty selects the configured root):");
                changed |= ui.text_edit_singleline(&mut self.maintenance.subtree).changed();
                if changed { self.maintenance.preview = None; }
            });
            ui.label("Create and inspect a preview before applying. These tools do not create installation receipts.");
            if !self.maintenance.error.is_empty() { ui.colored_label(egui::Color32::RED, &self.maintenance.error); }
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| { ui.monospace(&self.maintenance.text); });
            ui.horizontal(|ui| {
                let cancel = ui.button(if busy { "Cancel operation" } else { "Cancel" });
                if self.maintenance.focus_cancel { cancel.request_focus(); self.maintenance.focus_cancel = false; }
                if cancel.clicked() {
                    if let Some(flag) = &self.maintenance.cancelled { flag.store(true, Ordering::Release); }
                    else if matches!(self.sharing_continuation, Some(SharingContinuation::Maintenance(_))) {
                        self.sharing_continuation = None;
                        self.maintenance.error = "Maintenance cancelled before mutation. Sharing remains paused.".into();
                    } else if !busy { close = true; }
                }
                if ui.add_enabled(!busy, egui::Button::new("Refresh status")).clicked() { work = Some(Work::Status); }
                if ui.add_enabled(!busy && !self.maintenance.recovery_pending(), egui::Button::new("Create preview")).clicked() { work = Some(Work::Preview); }
                let ready = !busy && !self.maintenance.recovery_pending()
                    && self.maintenance.preview_context == self.input_stamp
                    && self.maintenance.preview.is_some()
                    && self.maintenance.status_context == self.input_stamp
                    && self.maintenance.status.is_some();
                if ui.add_enabled(ready, egui::Button::new("Apply reviewed preview")).clicked()
                    && let Some(preview) = &self.maintenance.preview {
                    work = Some(Work::Apply(preview.digest.clone()));
                }
            });
            if let Some(status) = &self.maintenance.status
                && status.pending {
                    ui.colored_label(egui::Color32::RED, "Pending maintenance fences further writes. Recover this operation first.");
                    if ui.add_enabled(!busy && self.maintenance.status_context == self.input_stamp && status.digest.is_some(), egui::Button::new("Recover pending operation")).clicked()
                        && let Some(digest) = &status.digest {
                        work = Some(Work::Recover(digest.clone()));
                    }
            }
            if busy { ui.spinner(); ui.label("The owned worker finishes cleanup before another action or window close."); }
        });
        if close {
            self.maintenance.open = false;
        }
        if !busy && self.maintenance.check_status {
            self.maintenance.check_status = false;
            if work.is_none() && !close {
                work = Some(Work::Status);
            }
        }
        if let Some(work) = work {
            let request = self.maintenance_request(work);
            self.maintenance_start(request, ctx);
        }
    }
}
