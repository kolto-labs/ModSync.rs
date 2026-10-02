//! Explicit standalone backend work, separate from guide installation authority.
use super::{App, Job, Message, SharingContinuation};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use modsync::{patcher::standalone, paths::Roots, recovery};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::Duration,
};

pub(super) struct State {
    pub open: bool,
    backend: String,
    arguments: String,
    directory: String,
    seconds: u64,
    trusted: bool,
    status: String,
    output: Option<standalone::Output>,
    worker: Option<JoinHandle<()>>,
    terminal: Option<mpsc::Receiver<Finished>>,
    cancelled: Option<Arc<AtomicBool>>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            backend: String::new(),
            arguments: String::new(),
            directory: String::new(),
            seconds: 3600,
            trusted: false,
            status: String::new(),
            output: None,
            worker: None,
            terminal: None,
            cancelled: None,
        }
    }
}
impl State {
    pub(super) fn closed(&self) -> bool {
        self.worker.is_none()
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
pub(super) struct Request {
    backend: String,
    arguments: String,
    directory: String,
    seconds: u64,
    context: String,
    game: String,
    mods: String,
}
struct Finished {
    context: String,
    result: Result<standalone::Output>,
}
impl App {
    pub(super) fn standalone_start(&mut self, request: Request, ctx: &egui::Context) {
        if self.receiver.is_some()
            || !self.standalone.closed()
            || self.desktop.busy()
            || self.sharing_continuation.is_some()
            || self.sharing.closing
            || self.fomod_pending()
        {
            return;
        }
        self.refresh_input_stamp();
        if request.context != self.input_stamp {
            self.standalone.status =
                "Inputs changed; review the tool request and confirm again.".into();
            return;
        }
        if !self.sharing.mutation() {
            self.sharing_continuation = Some(SharingContinuation::Standalone(request));
            self.standalone.status = "Waiting for sharing to stop…".into();
            return;
        }
        // An external program may change files even when it later fails or is cancelled.
        self.reset_installation();
        self.standalone.output = None;
        self.standalone.status = "Standalone HoloPatcher running…".into();
        self.cancelled = Arc::new(AtomicBool::new(false));
        let flag = self.cancelled.clone();
        self.standalone.cancelled = Some(flag.clone());
        let (busy, receiver) = mpsc::channel::<Message>();
        let (terminal, result) = mpsc::channel();
        self.receiver = Some(receiver);
        self.job = Job::Editor;
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("desktop-holopatcher".into()).spawn(move || {
            // Keep the ordinary app worker gate owned until all process cleanup finishes.
            let _busy = busy;
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<_> {
                ensure!(!flag.load(Ordering::Acquire), "Standalone HoloPatcher cancelled");
                let _lock = if request.game.trim().is_empty() && request.mods.trim().is_empty() {
                    None
                } else {
                    ensure!(!request.game.trim().is_empty() && !request.mods.trim().is_empty(),
                        "Set both game and mod directories, or clear both, before standalone execution");
                    let roots = Roots::new(Path::new(&request.mods),
                        &modsync::game::resolve_install_root(Path::new(&request.game)))?;
                    let lock = recovery::Lock::acquire(&roots)?;
                    recovery::ensure_clean(&roots)?;
                    Some(lock)
                };
                standalone::run(&standalone::Options {
                    executable: (!request.backend.trim().is_empty()).then(|| PathBuf::from(&request.backend)),
                    arguments: Vec::new(), legacy_arguments: Some(request.arguments.clone()),
                    working_directory: (!request.directory.trim().is_empty()).then(|| PathBuf::from(&request.directory)),
                    allow_processes: true, timeout: Duration::from_secs(request.seconds),
                }, &flag)
            })).unwrap_or_else(|_| Err(anyhow::anyhow!("Standalone worker stopped unexpectedly")));
            let _ = terminal.send(Finished { context: request.context, result: outcome });
            ctx.request_repaint();
        });
        match spawned {
            Ok(worker) => {
                self.standalone.worker = Some(worker);
                self.standalone.terminal = Some(result);
            }
            Err(error) => {
                self.receiver = None;
                self.standalone.cancelled = None;
                self.standalone.status = format!("Could not start standalone worker: {error}");
            }
        }
    }

    pub(super) fn poll_standalone(&mut self) {
        if !self
            .standalone
            .worker
            .as_ref()
            .is_some_and(JoinHandle::is_finished)
        {
            return;
        }
        let joined = self
            .standalone
            .worker
            .take()
            .expect("finished worker")
            .join();
        let terminal = self
            .standalone
            .terminal
            .take()
            .and_then(|receiver| receiver.try_recv().ok());
        self.standalone.cancelled = None;
        self.receiver = None;
        self.refresh_input_stamp();
        let result = joined
            .map_err(|_| anyhow::anyhow!("Standalone worker stopped unexpectedly"))
            .and_then(|_| terminal.context("Standalone worker returned no outcome"));
        match result {
            Ok(finished) => {
                let stale = finished.context != self.input_stamp;
                match finished.result {
                    Ok(output) => {
                        self.standalone.status = format!(
                            "Standalone backend exited with code {}. This is not an installation receipt.",
                            output.exit_code
                        );
                        self.standalone.output = Some(output);
                    }
                    Err(error) => {
                        self.standalone.status = format!("Standalone operation stopped: {error:#}")
                    }
                }
                if stale {
                    self.standalone
                        .status
                        .push_str(" Result belongs to the captured previous context.");
                }
            }
            Err(error) => {
                self.standalone.status = format!("Standalone operation stopped: {error:#}")
            }
        }
        self.standalone
            .status
            .push_str(" Sharing remains paused; inspect changes before resuming it.");
        // Deliberately no generic Done, validation success, receipt, or automatic resume.
    }

    pub(super) fn standalone_window(&mut self, ctx: &egui::Context) {
        if !self.standalone.open {
            return;
        }
        let mut open = true;
        let mut run = false;
        let busy = self.receiver.is_some()
            || self.desktop.busy()
            || self.sharing_continuation.is_some()
            || self.sharing.closing
            || !self.standalone.closed()
            || self.fomod_pending();
        egui::Window::new("Standalone HoloPatcher").open(&mut open).default_width(620.0).show(ctx, |ui| {
            ui.label("Runs outside guide installation. No rollback snapshot or installation receipt is created.");
            ui.label("Configured game/mod roots are locked and checked for unfinished recovery. Arguments may target other locations; those locations are not protected by this lock.");
            ui.add_enabled_ui(!busy, |ui| {
                let mut changed = false;
                ui.label("Backend path (empty uses installed backend discovery)");
                changed |= ui.add(egui::TextEdit::singleline(&mut self.standalone.backend).char_limit(4096)).changed();
                ui.label("Arguments (platform command-line quoting; no shell is executed)");
                changed |= ui.add(egui::TextEdit::multiline(&mut self.standalone.arguments).desired_rows(3).char_limit(16384)).changed();
                ui.label("Working directory (optional)");
                changed |= ui.add(egui::TextEdit::singleline(&mut self.standalone.directory).char_limit(4096)).changed();
                ui.horizontal(|ui| {
                    ui.label("Timeout in seconds");
                    changed |= ui.add(egui::DragValue::new(&mut self.standalone.seconds).range(1..=86400)).changed();
                });
                if changed { self.standalone.trusted = false; }
                ui.checkbox(&mut self.standalone.trusted, "I trust this backend and these arguments to run with my user permissions.");
                if ui.add_enabled(self.standalone.trusted, egui::Button::new("Run trusted backend")).clicked() { run = true; }
            });
            if matches!(self.sharing_continuation, Some(SharingContinuation::Standalone(_)))
                && ui.button("Cancel queued standalone operation").clicked() {
                self.sharing_continuation = None;
                self.standalone.status = "Standalone request cancelled before launch; sharing remains paused.".into();
            }
            if !self.standalone.closed() && ui.button("Cancel standalone operation").clicked() {
                self.cancelled.store(true, Ordering::Release);
                self.standalone.status = "Stopping standalone backend and waiting for cleanup…".into();
            }
            ui.label(&self.standalone.status);
            if let Some(output) = &self.standalone.output {
                for (label, text) in [("Standard output", &output.stdout), ("Standard error", &output.stderr)] {
                    ui.collapsing(label, |ui| {
                        if ui.button("Copy").clicked() { ui.ctx().copy_text(text.clone()); }
                        egui::ScrollArea::vertical().id_salt(label).max_height(220.0).show(ui, |ui| {
                            ui.add(egui::TextEdit::multiline(&mut text.as_str()).desired_width(f32::INFINITY));
                        });
                    });
                }
            }
        });
        self.standalone.open = open;
        if run {
            self.refresh_input_stamp();
            let request = Request {
                backend: self.standalone.backend.clone(),
                arguments: self.standalone.arguments.clone(),
                directory: self.standalone.directory.clone(),
                seconds: self.standalone.seconds,
                context: self.input_stamp.clone(),
                game: self.game.clone(),
                mods: self.mods.clone(),
            };
            self.standalone.trusted = false;
            self.standalone_start(request, ctx);
        }
    }
}
