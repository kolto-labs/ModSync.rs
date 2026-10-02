//! Owned desktop settings I/O and telemetry lifecycle. No detached exporter.
use super::App;
use anyhow::Result;
use eframe::egui;
use modsync::{
    profiles::{DesktopSettings, PatcherPreferences, Store},
    telemetry::{Config, lifecycle::Lifecycle},
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
};

enum Output {
    Loaded(DesktopSettings, PatcherPreferences, Config, Vec<String>),
    TelemetrySaved(u64, Config),
    SettingsSaved(DesktopSettings, PatcherPreferences),
    KeySaved,
    Diagnostics(String),
}
struct Finished {
    directory: String,
    result: Result<Output>,
}

struct Consent {
    draft: Config,
    // A save revision is followed by its matching lifecycle ticket. The prompt
    // stays owned until both receipts settle, and remains retryable on failure.
    pending: Option<(u64, Option<u64>)>,
}

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    pub(super) protocol: super::protocol_settings::State,
    pub directory: String,
    pub settings: DesktopSettings,
    owner: Option<Lifecycle>,
    receiver: Option<Receiver<Finished>>,
    held: Option<Finished>,
    cancelled: Arc<AtomicBool>,
    queued: Option<(u64, Config)>,
    revision: u64,
    allowed: bool,
    initialized: bool,
    loaded: bool,
    explicit: Option<PathBuf>,
    guide_generation: Option<u64>,
    pub logged: usize,
    status: String,
    diagnostics: String,
    consent: Option<Consent>,
}
impl State {
    pub(super) fn settings_source(&self) -> Option<PathBuf> {
        self.explicit.clone()
    }
    pub fn preferences_ready(&self, directory: &str) -> bool {
        self.loaded && self.directory == directory
    }
    pub fn watch_enabled(&self) -> bool {
        self.loaded && self.settings.enable_file_watcher
    }
    pub fn busy(&self) -> bool {
        self.receiver.is_some()
            || self.held.is_some()
            || self.queued.is_some()
            || self.protocol.busy()
    }
    pub fn closed(&self) -> bool {
        !self.busy()
            && self
                .owner
                .as_ref()
                .is_none_or(|owner| owner.status().stopped)
    }
    pub fn ready(&self, config: &Config, directory: &str) -> bool {
        self.allowed
            && self.consent.is_none()
            && self.directory == directory
            && config.exporter_ready()
            && self.owner.as_ref().is_some_and(|owner| {
                let status = owner.status();
                status.running && !status.stopping && status.requested == status.completed
            })
    }
}

impl App {
    fn start_desktop_owner(&mut self) -> Result<u64> {
        let total = self.guide.as_ref().map_or(0, |g| g.components.len());
        let selected = self
            .guide
            .as_ref()
            .map_or(0, |g| g.components.iter().filter(|c| c.is_selected).count());
        let owner = Lifecycle::start(
            PathBuf::from(&self.profile_directory),
            modsync::logging::Options {
                debug_logging: self.desktop.settings.debug_logging,
                ..Default::default()
            },
            self.telemetry.clone(),
            total,
            selected,
        )?;
        let ticket = owner.status().requested;
        self.desktop.owner = Some(owner);
        Ok(ticket)
    }

    pub(super) fn begin_desktop_load(&mut self, explicit: Option<PathBuf>) {
        if self.desktop.busy() {
            return;
        }
        self.sharing.mutation();
        self.desktop.directory = self.profile_directory.clone();
        self.desktop.allowed = false;
        self.desktop.initialized = true;
        self.desktop.loaded = false;
        self.desktop.consent = None;
        self.desktop.explicit = explicit.clone();
        let directory = self.desktop.directory.clone();
        let (tx, rx) = mpsc::channel();
        self.desktop.receiver = Some(rx);
        self.desktop.status = "Loading desktop preferences…".into();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(|| -> Result<Output> {
                let store = Store::from_settings_directory(explicit.as_deref())?;
                let migration_failed = store.nexus_api_key().is_err();
                let (mut report, patcher) = store.desktop_settings_with_patcher();
                if migration_failed {
                    report.settings.nexus_mods_api_key = None;
                    report.warnings.push("Nexus API key migration failed; the key remains unset for this session.".into());
                }
                let mut warnings = report.warnings;
                if report.used_defaults { warnings.push("Invalid stored settings were replaced by the typed defaults for this session.".into()); }
                let config = Config::load_from_settings_directory(explicit.as_deref()).unwrap_or_else(|error| { warnings.push(format!("Telemetry disabled: {error:#}")); Config { is_first_run: false, ..Config::default() } });
                Ok(Output::Loaded(report.settings, patcher, config, warnings))
            }).unwrap_or_else(|_| Err(anyhow::anyhow!("Settings reader stopped unexpectedly")));
            let _ = tx.send(Finished { directory, result });
        });
    }

    fn apply_desktop_settings(
        &mut self,
        settings: DesktopSettings,
        patcher: PatcherPreferences,
        startup: bool,
        ctx: &egui::Context,
    ) {
        self.theme = settings.effective_theme().into();
        self.spoiler_free = settings.spoiler_free_mode;
        if startup {
            if self.mods.is_empty() {
                self.mods = settings.source_path.clone().unwrap_or_default();
            }
            if self.game.is_empty() {
                self.game = settings.destination_path.clone().unwrap_or_default();
            }
        }
        if startup && self.probe.is_none() {
            self.nexus_api_key = settings.nexus_mods_api_key.clone().unwrap_or_default();
        }
        self.active_profile_name = settings.active_profile_name.clone().unwrap_or_default();
        if startup {
            self.profile_name = self.active_profile_name.clone();
        }
        self.checkpoints.config.managed = settings.managed_deployment_enabled;
        if self.checkpoints.config.managed {
            self.checkpoints.config.enabled = true;
        }
        self.patcher_engine = patcher.engine;
        self.patcher = patcher.executable.unwrap_or_default();
        modsync::logging::set_debug_logging(settings.debug_logging);
        self.desktop.settings = settings;
        self.apply_theme(ctx);
        self.invalidate_validation();
    }

    pub(super) fn queue_telemetry_save(&mut self) {
        if self.probe.is_some() || self.sharing.closing {
            return;
        }
        self.desktop.allowed = false;
        let Some(revision) = self.desktop.revision.checked_add(1) else {
            self.desktop.status =
                "Telemetry preference identity exhausted; restart before saving.".into();
            modsync::telemetry::global().request_stop();
            return;
        };
        self.desktop.revision = revision;
        // Stop old transport immediately; persistence and the joined receipt are
        // separate, and enabling requires the matching successful save.
        let mut disabled = self.telemetry.clone();
        disabled.set_consent(false);
        if let Some(owner) = &self.desktop.owner
            && let Err(error) = owner.configure(disabled)
        {
            self.desktop.status = format!("Telemetry stop request failed: {error:#}");
        }
        self.desktop.queued = Some((self.desktop.revision, self.telemetry.clone()));
        self.desktop.status =
            "Saving telemetry preference; export is gated until the matching receipt.".into();
    }

    pub(super) fn validate_nexus_key(&mut self) {
        if self.desktop.busy()
            || self.receiver.is_some()
            || self.sharing.closing
            || self.probe.is_some()
        {
            return;
        }
        let directory = self.profile_directory.clone();
        let key = self.nexus_api_key.clone();
        let explicit = self.desktop.explicit.clone();
        self.desktop.cancelled = Arc::new(AtomicBool::new(false));
        let cancelled = self.desktop.cancelled.clone();
        let (tx, rx) = mpsc::channel();
        self.desktop.receiver = Some(rx);
        self.desktop.status = "Validating Nexus key before saving…".into();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(|| -> Result<Output> {
                anyhow::ensure!(
                    modsync::profiles::settings_directory(explicit.as_deref())?
                        == Path::new(&directory),
                    "Settings root changed"
                );
                Store::from_settings_directory(explicit.as_deref())?
                    .save_validated_nexus_api_key(&key, &cancelled)?;
                Ok(Output::KeySaved)
            })
            .unwrap_or_else(|_| Err(anyhow::anyhow!("Key validation stopped unexpectedly")));
            let _ = tx.send(Finished { directory, result });
        });
    }

    pub(super) fn save_desktop_preferences(&mut self) {
        if self.desktop.busy() || self.receiver.is_some() || self.sharing.closing {
            return;
        }
        self.sharing.mutation();
        let directory = self.profile_directory.clone();
        let settings = self.desktop.settings.clone();
        let explicit = self.desktop.explicit.clone();
        // The shared writer preserves the stored key under its settings lock.
        let (tx, rx) = mpsc::channel();
        self.desktop.receiver = Some(rx);
        self.desktop.status = "Saving desktop preferences…".into();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(|| -> Result<Output> {
                anyhow::ensure!(
                    modsync::profiles::settings_directory(explicit.as_deref())?
                        == Path::new(&directory),
                    "Settings root changed"
                );
                let store = Store::from_settings_directory(explicit.as_deref())?;
                let (settings, patcher) =
                    store.save_desktop_settings_preserving_key_with_patcher(&settings)?;
                Ok(Output::SettingsSaved(settings, patcher))
            })
            .unwrap_or_else(|_| Err(anyhow::anyhow!("Settings save stopped unexpectedly")));
            let _ = tx.send(Finished { directory, result });
        });
    }

    pub(super) fn poll_desktop(&mut self, ctx: &egui::Context) {
        self.poll_protocol_settings(ctx);
        if !self.desktop.initialized {
            return;
        }
        if self.desktop.directory != self.profile_directory {
            self.desktop.allowed = false;
            self.desktop.loaded = false;
            self.desktop.cancelled.store(true, Ordering::Release);
            self.desktop.queued = None;
            self.desktop.consent = None;
            if let Some(owner) = &self.desktop.owner {
                let _ = owner.shutdown(false, false);
            }
        }
        if self.sharing.closing {
            self.desktop.cancelled.store(true, Ordering::Release);
        }
        if self.desktop.held.is_some() && !self.sharing.closing {
            self.sharing.mutation();
        }
        let message =
            self.desktop
                .receiver
                .as_ref()
                .and_then(|receiver| match receiver.try_recv() {
                    Ok(value) => Some(value),
                    Err(mpsc::TryRecvError::Disconnected) => Some(Finished {
                        directory: self.desktop.directory.clone(),
                        result: Err(anyhow::anyhow!("Settings worker disconnected")),
                    }),
                    Err(mpsc::TryRecvError::Empty) => None,
                });
        let message = message.or_else(|| {
            if self.sharing.ready() || self.sharing.closing {
                self.desktop.held.take()
            } else {
                None
            }
        });
        let message = match message {
            Some(message)
                if matches!(
                    &message.result,
                    Ok(Output::Loaded(..) | Output::SettingsSaved(..))
                ) && !self.sharing.ready()
                    && !self.sharing.closing =>
            {
                self.desktop.receiver = None;
                self.desktop.held = Some(message);
                self.sharing.mutation();
                None
            }
            other => other,
        };
        if let Some(message) = message {
            self.desktop.receiver = None;
            if message.directory != self.profile_directory {
                self.desktop.status = "Previous settings-directory operation finished; its result was not applied here.".into();
            } else {
                match message.result {
                    Ok(Output::Loaded(settings, patcher, config, warnings)) => {
                        self.logs.extend(warnings);
                        if !self.sharing.closing {
                            self.desktop.loaded = true;
                            self.apply_desktop_settings(settings, patcher, true, ctx);
                            self.sharing.resume_after_mutation(self.guide.as_ref());
                            self.telemetry = if self.probe.is_some() {
                                Config::default()
                            } else {
                                config
                            };
                            if self.probe.is_none() && self.telemetry.is_first_run {
                                self.desktop.consent = Some(Consent {
                                    draft: self.telemetry.clone(),
                                    pending: None,
                                });
                                // Missing configuration requires an explicit answer;
                                // the log owner may start, but transport stays off.
                                self.telemetry.set_consent(false);
                            }
                            match self.start_desktop_owner() {
                                Ok(_) => {
                                    self.desktop.allowed = self.probe.is_none();
                                    self.desktop.status =
                                        "Preferences loaded; telemetry initialization is settling."
                                            .into();
                                }
                                Err(error) => {
                                    self.desktop.status =
                                        format!("Telemetry/log owner unavailable: {error:#}")
                                }
                            }
                        }
                    }
                    Ok(Output::TelemetrySaved(revision, config)) => {
                        if revision == self.desktop.revision {
                            self.telemetry = config;
                            self.desktop.status =
                                "Telemetry preference saved; waiting for exporter receipt.".into();
                            if !self.sharing.closing {
                                let configured = if let Some(owner) = &self.desktop.owner {
                                    owner.configure(self.telemetry.clone())
                                } else {
                                    // Retry a failed initial owner spawn only after
                                    // the requested choice has a verified save.
                                    self.start_desktop_owner()
                                };
                                match configured {
                                    Ok(ticket) => {
                                        self.desktop.allowed = true;
                                        if let Some(consent) = &mut self.desktop.consent
                                            && consent
                                                .pending
                                                .is_some_and(|(saved, _)| saved == revision)
                                        {
                                            consent.pending = Some((revision, Some(ticket)));
                                        }
                                    }
                                    Err(error) => {
                                        self.desktop.status =
                                            format!("Telemetry configuration failed: {error:#}");
                                        self.desktop.allowed = false;
                                        if let Some(consent) = &mut self.desktop.consent {
                                            consent.pending = None;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(Output::SettingsSaved(settings, patcher)) => {
                        if !self.sharing.closing {
                            self.apply_desktop_settings(settings, patcher, false, ctx);
                            self.sharing.resume_after_mutation(self.guide.as_ref());
                        }
                        self.desktop.status = "Desktop preferences saved.".into();
                    }
                    Ok(Output::KeySaved) => {
                        self.desktop.status = "Nexus key validated and saved.".into();
                    }
                    Ok(Output::Diagnostics(report)) => {
                        self.desktop.diagnostics = report;
                        self.desktop.status = "Diagnostics ready. Review before sharing.".into();
                    }
                    Err(error) => {
                        self.desktop.status = format!("Settings operation failed: {error:#}");
                        self.logs.push(self.desktop.status.clone());
                        if let Some(consent) = &mut self.desktop.consent {
                            consent.pending = None;
                            self.desktop.allowed = false;
                        }
                    }
                }
            }
        }
        if let Some((revision, Some(ticket))) =
            self.desktop.consent.as_ref().and_then(|c| c.pending)
            && revision == self.desktop.revision
            && let Some(owner) = &self.desktop.owner
        {
            let status = owner.status();
            if status.requested == ticket && status.completed == ticket && !status.stopping {
                if status.configuration_error.is_none() {
                    self.desktop.consent = None;
                    self.desktop.status = if self.telemetry.exporter_ready() && status.running {
                        "Telemetry choice saved; exporter ready.".into()
                    } else {
                        "Telemetry choice saved; remote export is disabled or unconfigured.".into()
                    };
                    if let Some(error) = status.error {
                        self.logs.push(format!("Telemetry/log owner: {error}"));
                    }
                } else {
                    self.desktop.allowed = false;
                    if let Some(consent) = &mut self.desktop.consent {
                        consent.pending = None;
                    }
                    modsync::telemetry::global().request_stop();
                }
            }
        }
        if self.desktop.receiver.is_none()
            && !self.desktop.protocol.busy()
            && let Some((revision, config)) = self.desktop.queued.take()
        {
            let directory = self.profile_directory.clone();
            let explicit = self.desktop.explicit.clone();
            let (tx, rx) = mpsc::channel();
            self.desktop.receiver = Some(rx);
            std::thread::spawn(move || {
                let result = std::panic::catch_unwind(|| -> Result<Output> {
                    anyhow::ensure!(
                        modsync::profiles::settings_directory(explicit.as_deref())?
                            == Path::new(&directory),
                        "Settings root changed"
                    );
                    config.save(Path::new(&directory))?;
                    let mut saved = Config::load_from_settings_directory(explicit.as_deref())?;
                    saved.session_id = config.session_id.clone();
                    let mut expected = config;
                    expected.is_first_run = false;
                    anyhow::ensure!(
                        serde_json::to_value(&saved)? == serde_json::to_value(&expected)?,
                        "Telemetry configuration changed during save verification"
                    );
                    Ok(Output::TelemetrySaved(revision, saved))
                })
                .unwrap_or_else(|_| {
                    Err(anyhow::anyhow!(
                        "Telemetry preference writer stopped unexpectedly"
                    ))
                });
                let _ = tx.send(Finished { directory, result });
            });
        }
        if self.desktop.directory != self.profile_directory
            && self.desktop.closed()
            && !self.sharing.closing
        {
            self.desktop.owner = None;
            self.begin_desktop_load(Some(PathBuf::from(&self.profile_directory)));
        }
        if self.sharing.closing
            && !self.desktop.busy()
            && self.receiver.is_none()
            && self.sharing.closed()
            && self.resource_watch.closed()
            && (self.desktop.logged >= self.logs.len()
                || self
                    .desktop
                    .owner
                    .as_ref()
                    .is_some_and(|owner| owner.status().stopped))
            && let Some(owner) = &self.desktop.owner
        {
            let _ = owner.shutdown(
                self.finished,
                self.desktop.allowed && self.telemetry.exporter_ready(),
            );
        }
        if self.desktop.ready(&self.telemetry, &self.profile_directory)
            && self.guide.is_some()
            && self.desktop.guide_generation != Some(self.guide_generation)
        {
            self.emit_telemetry(
                "guide.loaded",
                serde_json::Map::from_iter([(
                    "component_count".into(),
                    serde_json::json!(self.guide.as_ref().map_or(0, |g| g.components.len())),
                )]),
            );
            self.desktop.guide_generation = Some(self.guide_generation);
        }
        if self
            .desktop
            .owner
            .as_ref()
            .is_some_and(|owner| owner.status().completed > 0)
        {
            let mut batch = Vec::new();
            let mut bytes = 0usize;
            for line in self.logs.iter().skip(self.desktop.logged).take(64) {
                let line = self.downloads.redact(line);
                if line.len() > 1024 * 1024 {
                    if batch.is_empty() {
                        self.desktop.status = "A log line exceeds the 1 MiB persistent-log queue limit; it remains in the desktop log only.".into();
                        self.desktop.logged += 1;
                    }
                    break;
                }
                if bytes + line.len() > 1024 * 1024 {
                    break;
                }
                bytes += line.len();
                batch.push(line);
            }
            if !batch.is_empty() {
                let count = batch.len();
                if self
                    .desktop
                    .owner
                    .as_ref()
                    .is_some_and(|owner| owner.log_lines(batch).is_ok())
                {
                    self.desktop.logged += count;
                }
            }
        }
        if self.desktop.busy()
            || self.desktop.owner.as_ref().is_some_and(|owner| {
                let s = owner.status();
                s.requested != s.completed || s.stopping && !s.stopped
            })
        {
            ctx.request_repaint();
        }
    }

    pub(super) fn desktop_window(&mut self, ctx: &egui::Context) {
        self.first_run_consent_ui(ctx);
        if !self.desktop.open {
            return;
        }
        let mut open = true;
        let mut save = false;
        let mut telemetry_save = false;
        let mut diagnostics = false;
        let mut logs = false;
        let available = self.desktop.loaded
            && !self.desktop.busy()
            && self.receiver.is_none()
            && !self.sharing.closing;
        egui::Window::new("Desktop settings and privacy").open(&mut open).default_size([650.0,650.0]).vscroll(true).show(ctx, |ui| {
            ui.label(&self.desktop.status);
            if self.desktop.busy() { ui.spinner(); if ui.button("Cancel key validation / wait for save").clicked() { self.desktop.cancelled.store(true, Ordering::Release); } }
            ui.collapsing("Desktop preferences", |ui| {
                ui.add_enabled_ui(available, |ui| {
                    let settings = &mut self.desktop.settings;
                    ui.checkbox(&mut settings.debug_logging, "Debug logging");
                    ui.checkbox(&mut settings.enable_file_watcher, "Observe registered resource changes");
                    ui.label(&self.resource_watch.status);
                    ui.checkbox(&mut settings.case_insensitive_pathing, "Case-insensitive file paths (managed/checkpoint installs require this)");
                    ui.checkbox(&mut settings.spoiler_free_mode, "Spoiler-free presentation");
                    ui.checkbox(&mut settings.filter_downloads_by_resolution, "Filter downloads for physical monitor resolution");
                    ui.checkbox(&mut settings.managed_deployment_enabled, "Managed deployment (also enables component checkpoints)");
                    for (label, value) in [("Theme", &mut settings.theme), ("Default mod directory", &mut settings.source_path), ("Default game directory", &mut settings.destination_path), ("Patcher engine", &mut settings.patcher_engine), ("KPatcher executable", &mut settings.kpatcher_executable_path)] {
                        ui.horizontal(|ui| { ui.label(label); ui.text_edit_singleline(value.get_or_insert_with(String::new)); });
                    }
                    ui.label(format!("Active profile: {} (change through Activate profile)", self.active_profile_name));
                    ui.label(format!("Theme target hint: {:?} (not installed-game detection)", settings.target_game()));
                    ui.label(format!("Stored encoding: {}; FOMOD preference: {}", settings.effective_file_encoding(), settings.effective_fomod_mode()));
                    ui.label("Stored compatibility fields are preserved. Guide files use UTF-8; the legacy encoding preference is retained. Rust never auto-elevates and Move already copies then deletes. Parallel I/O and automatic fixes have no desktop consumer yet. Resource observation checks registered file metadata; changes invalidate validation without running it. FOMOD remains the verified prompt flow.");
                    ui.collapsing("All stored compatibility settings (redacted)", |ui| {
                        match settings.output_value(false).and_then(|value| Ok(serde_json::to_string_pretty(&value)?)) {
                            Ok(text) => { ui.monospace(&text); if ui.button("Copy redacted settings").clicked() { ctx.copy_text(text); } }
                            Err(error) => { ui.label(error.to_string()); }
                        }
                    });
                    save = ui.button("Save desktop preferences").clicked();
                });
            });
            self.protocol_settings_ui(ui);
            ui.separator(); ui.heading("Telemetry consent");
            ui.add_enabled_ui(self.desktop.loaded && self.probe.is_none() && !self.sharing.closing && self.desktop.consent.is_none(), |ui| {
                let mut consent = self.telemetry.user_consented;
                if ui.checkbox(&mut consent, "Share anonymous telemetry").changed() { self.telemetry.set_consent(consent); telemetry_save = true; }
                let mut changed = false;
                changed |= ui.checkbox(&mut self.telemetry.collect_usage_data, "Usage events").changed();
                changed |= ui.checkbox(&mut self.telemetry.collect_performance_metrics, "Performance metrics").changed();
                changed |= ui.checkbox(&mut self.telemetry.collect_crash_reports, "Hashed crash reports").changed();
                changed |= ui.checkbox(&mut self.telemetry.enable_otlp_exporter, "Enable OTLP exporter").changed();
                ui.horizontal(|ui| { ui.label("OTLP endpoint"); changed |= ui.text_edit_singleline(&mut self.telemetry.endpoint).changed(); });
                telemetry_save |= changed || ui.button("Save telemetry configuration").clicked();
                if let Some(error) = self.telemetry.endpoint_error() { ui.label(error); }
            });
            if self.desktop.ready(&self.telemetry, &self.profile_directory) { ui.label("Exporter ready: consent and matching lifecycle receipt verified."); }
            else { ui.label("Export is disabled or awaiting configuration/stop completion."); }
            if let Some(owner) = &self.desktop.owner { let status=owner.status(); ui.label(format!("Configuration {}/{}; running {}; stopped {}",status.completed,status.requested,status.running,status.stopped)); if let Some(error)=status.error { ui.colored_label(egui::Color32::RED,error); } }
            ui.collapsing("What is collected", |ui| { ui.label(self.telemetry.privacy_summary()); });
            ui.horizontal(|ui| { diagnostics=ui.add_enabled(available,egui::Button::new("Collect diagnostics")).clicked(); logs=ui.button("Open log folder").clicked(); });
            if !self.desktop.diagnostics.is_empty() { ui.label("Diagnostics include local paths and recent logs. Review before copying."); if ui.button("Copy diagnostics").clicked() { ctx.copy_text(self.desktop.diagnostics.clone()); } ui.add(egui::TextEdit::multiline(&mut self.desktop.diagnostics).interactive(false).desired_rows(12)); }
        });
        self.desktop.open = open;
        if telemetry_save {
            self.queue_telemetry_save();
        }
        if save {
            self.save_desktop_preferences();
        }
        if logs {
            let path = modsync::logging::log_directory_for(Path::new(&self.profile_directory));
            self.open_folder(ctx, &path.to_string_lossy());
        }
        if diagnostics && !self.desktop.busy() {
            let directory = self.profile_directory.clone();
            let config = self.telemetry.clone();
            let (tx, rx) = mpsc::channel();
            self.desktop.receiver = Some(rx);
            std::thread::spawn(move || {
                let report = modsync::logging::diagnostics_report(
                    Some(Path::new(&directory)),
                    Some(&config),
                    100,
                );
                let _ = tx.send(Finished {
                    directory,
                    result: Ok(Output::Diagnostics(report)),
                });
            });
        }
    }

    fn first_run_consent_ui(&mut self, ctx: &egui::Context) {
        if self.sharing.closing
            || self.probe.is_some()
            || self.desktop.directory != self.profile_directory
        {
            return;
        }
        let available = self.desktop.loaded && !self.desktop.busy() && self.receiver.is_none();
        let Some(consent) = &mut self.desktop.consent else {
            return;
        };
        let mut answer = None;
        egui::Modal::new(egui::Id::new(("first_run_telemetry", &self.desktop.directory))).show(ctx, |ui| {
            ui.set_max_width(540.0);
            ui.heading("Telemetry preference");
            ui.label("Choose whether ModSync may send anonymous usage, performance and hashed error reports. You can change this later in Desktop settings and privacy.");
            ui.label("Remote export also requires a configured endpoint and signing key. No remote server is selected by default.");
            ui.add_enabled_ui(available && consent.pending.is_none(), |ui| {
                ui.checkbox(&mut consent.draft.collect_usage_data, "Usage events");
                ui.checkbox(&mut consent.draft.collect_performance_metrics, "Performance metrics");
                ui.checkbox(&mut consent.draft.collect_crash_reports, "Hashed error reports");
                ui.collapsing("Privacy details", |ui| {
                    let mut summary = consent.draft.clone();
                    summary.enabled = true;
                    summary.user_consented = true;
                    ui.label(summary.privacy_summary());
                });
                ui.horizontal(|ui| {
                    if ui.button("Keep telemetry off").clicked() { answer = Some(false); }
                    if ui.button("Allow configured telemetry").clicked() { answer = Some(true); }
                });
            });
            if consent.pending.is_some() { ui.spinner(); }
            ui.label(&self.desktop.status);
        });
        if let Some(enabled) = answer {
            self.telemetry = consent.draft.clone();
            self.telemetry.set_consent(enabled);
            self.telemetry.is_first_run = false;
            self.queue_telemetry_save();
            if let Some((revision, _)) = &self.desktop.queued
                && let Some(consent) = &mut self.desktop.consent
            {
                consent.pending = Some((*revision, None));
            }
        }
    }
}
