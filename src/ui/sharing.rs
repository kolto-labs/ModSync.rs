//! Desktop ownership and asynchronous barriers for completed-archive sharing.
use anyhow::Result;
use eframe::egui;
use modsync::{
    Guide, archive,
    download_orchestrator::{AcquisitionPlan, RunReport},
    peer_sharing::{self, Barrier, Catalog, Enrollment, PauseReason, SharingOwner},
    profiles::SettingsFile,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
};

type Background<T> = (Receiver<Result<T>>, thread::JoinHandle<()>);

struct Admission {
    context: String,
    guide: Guide,
    path: Option<PathBuf>,
    result: Result<Enrollment>,
}
struct AdmissionJob {
    cancelled: Arc<AtomicBool>,
    receiver: Receiver<Admission>,
    join: thread::JoinHandle<()>,
}
#[derive(Clone, Copy)]
enum After {
    Pause,
    Authorize,
    Admit,
    RevalidateMutation,
    ResumeAuthorization,
    Resume,
    ResumeMutation,
}
struct Pending {
    receiver: Barrier,
    after: After,
}

pub(super) struct View {
    owner: Option<SharingOwner>,
    starting: Option<Background<Option<SharingOwner>>>,
    stopping: Option<Background<()>>,
    pending: Option<Pending>,
    admissions: Vec<AdmissionJob>,
    waiting: Vec<Admission>,
    authorized: Vec<String>,
    directory: Option<String>,
    generation: u64,
    initialized: bool,
    mutation_ready: bool,
    startup_pause: Option<PauseReason>,
    shutdown_failed: bool,
    manual_resume_required: bool,
    pub closing: bool,
    pub close_confirm: bool,
    pub status: String,
}
impl Default for View {
    fn default() -> Self {
        Self {
            owner: None,
            starting: None,
            stopping: None,
            pending: None,
            admissions: Vec::new(),
            waiting: Vec::new(),
            authorized: Vec::new(),
            directory: None,
            generation: 0,
            initialized: false,
            mutation_ready: true,
            startup_pause: None,
            shutdown_failed: false,
            manual_resume_required: false,
            closing: false,
            close_confirm: false,
            status: String::new(),
        }
    }
}
impl View {
    pub fn require_manual_resume(&mut self) {
        self.manual_resume_required = true;
    }
    pub fn requires_manual_resume(&self) -> bool {
        self.manual_resume_required
    }
    pub fn ready(&self) -> bool {
        !self.shutdown_failed
            && self.mutation_ready
            && self.starting.is_none()
            && self.stopping.is_none()
            && self.pending.is_none()
            && self.admissions.is_empty()
            && self.waiting.is_empty()
    }
    pub fn busy(&self) -> bool {
        self.starting.is_some()
            || self.stopping.is_some()
            || self.pending.is_some()
            || !self.admissions.is_empty()
            || !self.waiting.is_empty()
    }
    pub fn available(&self) -> bool {
        self.owner.is_some()
    }
    fn cancel_admissions(&self) {
        for job in &self.admissions {
            job.cancelled.store(true, Ordering::Release);
        }
    }
    pub fn pause(&mut self, reason: PauseReason) {
        self.cancel_admissions();
        self.waiting.clear();
        self.mutation_ready = false;
        if let Some(owner) = &self.owner {
            self.pending = Some(Pending {
                receiver: owner.request_pause(reason),
                after: After::Pause,
            });
            self.status = "Stopping sharing; waiting for owned work and cleanup…".into();
        } else if self.starting.is_some() {
            self.startup_pause = Some(reason);
        } else {
            self.mutation_ready = true;
        }
    }
    pub fn mutation(&mut self) -> bool {
        if self.ready() {
            return true;
        }
        if self.pending.is_none() && self.starting.is_none() && self.stopping.is_none() {
            self.pause(PauseReason::Mutation);
        }
        false
    }
    pub fn edit_gate(&mut self, ui: &mut egui::Ui) -> bool {
        if self.ready() {
            return true;
        }
        ui.label("Pause sharing before editing the guide, folders, or install settings.");
        if ui
            .add_enabled(!self.busy(), egui::Button::new("Pause sharing to edit"))
            .clicked()
        {
            self.pause(PauseReason::Mutation);
        }
        if self.busy() {
            ui.spinner();
            ui.label(&self.status);
        }
        false
    }
    pub fn resume(&mut self, guide: Option<&Guide>) {
        if self.busy() {
            return;
        }
        if let Some(owner) = &self.owner {
            // This path is called only by the enabled explicit Resume control.
            self.manual_resume_required = false;
            self.mutation_ready = false;
            self.pending = Some(match guide {
                Some(guide) => Pending {
                    receiver: owner.authorize_enrollment_from(guide, &Enrollment::default(), None),
                    after: After::ResumeAuthorization,
                },
                None => Pending {
                    receiver: owner.request_resume(),
                    after: After::Resume,
                },
            });
            self.status = "Revalidating sharing authorization…".into();
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn enroll(
        &mut self,
        context: String,
        guide: Guide,
        path: Option<PathBuf>,
        directory: String,
        plan: AcquisitionPlan,
        report: RunReport,
        options: archive::Options,
    ) {
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || {
            let result = (|| -> Result<Enrollment> {
                let store = SettingsFile::resolve(Some(Path::new(&directory)))?;
                let catalog = Catalog::open(&store)?;
                Ok(peer_sharing::enroll_report_cancelled(
                    &catalog, &guide, &plan, &report, &options, None, true, &flag,
                ))
            })();
            let _ = tx.send(Admission {
                context,
                guide,
                path,
                result,
            });
        });
        self.admissions.push(AdmissionJob {
            cancelled,
            receiver: rx,
            join,
        });
    }
    fn shutdown_owner(&mut self) {
        self.cancel_admissions();
        self.waiting.clear();
        self.pending = None;
        if let Some(mut owner) = self.owner.take() {
            let (tx, rx) = mpsc::channel();
            let join = thread::spawn(move || {
                let _ = tx.send(owner.shutdown());
            });
            self.stopping = Some((rx, join));
        }
    }
    pub fn close(&mut self) {
        self.closing = true;
        self.close_confirm = false;
        self.status = "Closing: waiting for downloads, sharing, and private-copy cleanup…".into();
        self.shutdown_owner();
    }
    pub fn closed(&self) -> bool {
        self.closing && !self.shutdown_failed && self.owner.is_none() && !self.busy()
    }
    pub fn poll(&mut self, directory: &str, context: &str, logs: &mut Vec<String>) {
        if let Some((rx, join)) = &self.starting
            && join.is_finished()
        {
            match rx.try_recv() {
                Ok(result) => {
                    let (_, join) = self.starting.take().unwrap();
                    let _ = join.join();
                    match result {
                        Ok(owner) => {
                            self.owner = owner;
                            self.mutation_ready = self.owner.is_none();
                            if self.owner.is_none() {
                                self.status =
                                    "Completed sharing is owned by another application instance."
                                        .into();
                            } else if !self.closing {
                                if self.manual_resume_required {
                                    self.startup_pause = None;
                                    self.pause(PauseReason::Mutation);
                                } else if let Some(reason) = self.startup_pause.take() {
                                    self.pause(reason);
                                } else if let Some(owner) = &self.owner {
                                    self.pending = Some(Pending {
                                        receiver: owner.request_resume_after_mutation(),
                                        after: After::ResumeMutation,
                                    });
                                }
                            }
                        }
                        Err(error) => {
                            self.status = format!("Sharing unavailable: {error:#}");
                            logs.push(self.status.clone());
                            self.mutation_ready = true;
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    let (_, join) = self.starting.take().unwrap();
                    let _ = join.join();
                    self.status = "Sharing startup worker stopped unexpectedly".into();
                    logs.push(self.status.clone());
                    self.mutation_ready = true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some((rx, join)) = &self.stopping
            && join.is_finished()
        {
            match rx.try_recv() {
                Ok(result) => {
                    let (_, join) = self.stopping.take().unwrap();
                    let _ = join.join();
                    self.mutation_ready = true;
                    if let Err(error) = result {
                        self.shutdown_failed = true;
                        self.mutation_ready = false;
                        self.status =
                            format!("Sharing cleanup failed; close remains blocked: {error:#}");
                        logs.push(self.status.clone());
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    let (_, join) = self.stopping.take().unwrap();
                    let _ = join.join();
                    self.shutdown_failed = true;
                    self.mutation_ready = false;
                    self.status =
                        "Sharing shutdown worker stopped unexpectedly; close remains blocked."
                            .into();
                    logs.push(self.status.clone());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.closing
            || self
                .directory
                .as_deref()
                .is_some_and(|old| old != directory)
        {
            self.shutdown_owner();
        }
        if !self.closing
            && !self.shutdown_failed
            && self.starting.is_none()
            && self.stopping.is_none()
            && (!self.initialized || self.directory.as_deref() != Some(directory))
        {
            self.initialized = true;
            // Admission identities belong to exactly one catalog owner. A new
            // settings directory must not inherit identities from the old catalog.
            self.authorized.clear();
            self.directory = Some(directory.into());
            self.generation = self.generation.wrapping_add(1);
            self.mutation_ready = false;
            let generation = self.generation;
            let directory = directory.to_owned();
            let (tx, rx) = mpsc::channel();
            let join = thread::spawn(move || {
                let result = SettingsFile::resolve(Some(Path::new(&directory)))
                    .and_then(|store| SharingOwner::start_paused(store, generation));
                let _ = tx.send(result);
            });
            self.starting = Some((rx, join));
        }
        let mut index = 0;
        while index < self.admissions.len() {
            if !self.admissions[index].join.is_finished() {
                index += 1;
                continue;
            }
            match self.admissions[index].receiver.try_recv() {
                Ok(receipt) => {
                    let job = self.admissions.remove(index);
                    let _ = job.join.join();
                    if !job.cancelled.load(Ordering::Acquire)
                        && !self.closing
                        && receipt.context == context
                    {
                        self.waiting.push(receipt);
                    } else {
                        logs.push("Sharing enrollment remained paused because its captured context changed or stopped.".into());
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    let job = self.admissions.remove(index);
                    let _ = job.join.join();
                    logs.push("Sharing enrollment worker stopped unexpectedly; acquisition results remain unchanged.".into());
                }
                Err(mpsc::TryRecvError::Empty) => index += 1,
            }
        }
        if let Some(pending) = &self.pending {
            let result = match pending.receiver.try_recv() {
                Ok(value) => Some(value),
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                    "Sharing owner stopped before acknowledging cleanup".into(),
                )),
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                let pending = self.pending.take().unwrap();
                match result {
                    Err(error) => {
                        self.status = error;
                        logs.push(self.status.clone());
                        self.shutdown_owner();
                    }
                    Ok(()) => match pending.after {
                        After::Pause => {
                            self.mutation_ready = true;
                            self.status = "Sharing paused; owned worker cleanup completed.".into();
                        }
                        After::ResumeAuthorization => {
                            if self.manual_resume_required {
                                self.pause(PauseReason::Mutation);
                            } else if let Some(owner) = &self.owner {
                                self.pending = Some(Pending {
                                    receiver: owner.request_resume(),
                                    after: After::Resume,
                                });
                            }
                        }
                        After::Resume => {
                            if self.manual_resume_required {
                                self.pause(PauseReason::Mutation);
                            } else {
                                self.mutation_ready = false;
                                self.status = "Sharing resumed with remaining budgets; each source is revalidated.".into();
                            }
                        }
                        After::Authorize => {
                            self.status =
                                "Sharing enrollment authorized for the current guide.".into();
                        }
                        After::Admit | After::RevalidateMutation => {
                            if self.manual_resume_required {
                                self.pause(PauseReason::Mutation);
                            } else {
                                self.mutation_ready = false;
                                if let Some(owner) = &self.owner {
                                    self.pending = Some(Pending {
                                        receiver: owner.request_resume_after_mutation(),
                                        after: After::ResumeMutation,
                                    });
                                }
                            }
                        }
                        After::ResumeMutation => {
                            if self.manual_resume_required {
                                self.pause(PauseReason::Mutation);
                            } else {
                                self.mutation_ready = false;
                                self.status = "Sharing context checked; user stops and settings pauses remain in effect.".into();
                            }
                        }
                    },
                }
            }
        }
        if self.pending.is_none()
            && self.starting.is_none()
            && !self.closing
            && !self.waiting.is_empty()
        {
            let receipt = self.waiting.remove(0);
            if receipt.context != context {
                logs.push("Sharing enrollment retained paused after guide or root change.".into());
            } else {
                match receipt.result {
                    Err(error) => logs.push(format!("Sharing enrollment failed: {error:#}")),
                    Ok(enrollment) => {
                        logs.push(format!(
                            "Sharing enrollment: {} admitted, {} duplicates, {} rejected.",
                            enrollment.admitted, enrollment.duplicates, enrollment.rejected
                        ));
                        logs.extend(
                            enrollment
                                .failures
                                .iter()
                                .map(|error| format!("Sharing enrollment failed: {error}")),
                        );
                        if let Some(owner) = &self.owner {
                            for id in &enrollment.identities {
                                if !self.authorized.contains(id) {
                                    self.authorized.push(id.clone());
                                }
                            }
                            self.pending = Some(Pending {
                                receiver: owner.authorize_enrollment_from(
                                    &receipt.guide,
                                    &enrollment,
                                    receipt.path,
                                ),
                                after: After::Admit,
                            });
                        } else {
                            logs.push("Sharing enrollment is paused until a desktop owner can approve this guide.".into());
                        }
                    }
                }
            }
        }
    }
    pub fn resume_after_mutation(&mut self, guide: Option<&Guide>) {
        if self.closing || self.busy() || self.manual_resume_required {
            return;
        }
        if let (Some(owner), Some(guide)) = (&self.owner, guide) {
            self.mutation_ready = false;
            self.pending = Some(Pending {
                receiver: owner.authorize_enrollment_from(guide, &Enrollment::default(), None),
                after: After::RevalidateMutation,
            });
        }
    }
    pub fn promote(&mut self, guide: &Guide, path: PathBuf) {
        if !self.ready() {
            self.status = "Guide saved; pause sharing before promoting saved provenance.".into();
            return;
        }
        if let Some(owner) = &self.owner {
            let enrollment = Enrollment {
                identities: self.authorized.clone(),
                ..Enrollment::default()
            };
            self.pending = Some(Pending {
                receiver: owner.promote_saved_enrollment(guide, &enrollment, path),
                after: After::Authorize,
            });
            self.status = "Checking saved guide provenance for sharing…".into();
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui, guide: Option<&Guide>, allow_resume: bool) {
        ui.horizontal_wrapped(|ui| {
            ui.strong("Completed sharing");
            if let Some(owner) = &self.owner {
                let snapshot = owner.snapshot();
                if snapshot.generation == self.generation {
                    if snapshot.owner_stopped {
                        ui.label("Sharing controller stopped.");
                    }
                    ui.label(format!(
                        "{:?}: {} bytes uploaded, {:.1} active minutes",
                        snapshot.state,
                        snapshot.uploaded_bytes,
                        snapshot.active_millis as f64 / 60_000.0
                    ));
                    if let Some(identity) = snapshot.identity {
                        ui.monospace(identity.chars().take(12).collect::<String>());
                    }
                    if let Some(reason) = snapshot.reason {
                        ui.label(reason);
                    }
                }
            }
            if ui
                .add_enabled(
                    self.available() && !self.closing,
                    egui::Button::new("Stop sharing"),
                )
                .clicked()
            {
                self.pause(PauseReason::User);
            }
            if ui
                .add_enabled(
                    allow_resume && self.available() && !self.busy() && !self.closing,
                    egui::Button::new("Resume sharing"),
                )
                .clicked()
            {
                self.resume(guide);
            }
        });
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
        ui.small("The upload cap applies to completed sharing. Concurrent acquisition sessions have separate caps.");
    }
}

// Normal close drains these handles asynchronously in poll. This fallback is
// only for abnormal application teardown (for example a renderer failure):
// ownership must not silently detach a controller or archive verifier.
impl Drop for View {
    fn drop(&mut self) {
        self.cancel_admissions();
        if let Some(mut owner) = self.owner.take() {
            let _ = owner.shutdown();
        }
        if let Some((receiver, join)) = self.starting.take() {
            let _ = join.join();
            if let Ok(Ok(Some(mut owner))) = receiver.recv() {
                let _ = owner.shutdown();
            }
        }
        if let Some((_, join)) = self.stopping.take() {
            let _ = join.join();
        }
        for job in self.admissions.drain(..) {
            let _ = job.join.join();
        }
    }
}
