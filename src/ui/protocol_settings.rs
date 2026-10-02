//! Desktop ownership of protocol operations and their separate preference receipt.
use super::App;
use anyhow::{Context, Result, ensure};
use eframe::egui;
use modsync::{
    profiles::Store,
    protocol_registration::{
        self,
        owned::{self, RegistrationReport, Request, Scheme, State as Ownership},
    },
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

#[derive(Default)]
pub(super) struct State {
    worker: Option<JoinHandle<Finished>>,
    cancel: Arc<AtomicBool>,
    directory: String,
    startup: Option<String>,
    report: Option<RegistrationReport>,
    message: String,
    replace_confirmed: bool,
    bundle_confirmed: bool,
}
impl State {
    pub(super) fn busy(&self) -> bool {
        self.worker.is_some()
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[derive(Clone, Copy)]
enum Action {
    Status,
    Startup,
    Register,
    Unregister,
    DisablePreference,
}
struct Finished {
    report: Option<RegistrationReport>,
    saved: Option<bool>,
    message: String,
}
fn fingerprint(report: &RegistrationReport) -> Vec<(Scheme, Ownership, Option<String>)> {
    report
        .entries
        .iter()
        .map(|entry| (entry.scheme, entry.state.clone(), entry.handler.clone()))
        .collect()
}
fn execute(
    action: Action,
    explicit: Option<PathBuf>,
    directory: String,
    confirmation: Option<RegistrationReport>,
    bundle_confirmed: bool,
    flag: &AtomicBool,
) -> Finished {
    let mut output = Finished {
        report: None,
        saved: None,
        message: String::new(),
    };
    let result = (|| -> Result<()> {
        ensure!(
            modsync::profiles::settings_directory(explicit.as_deref())?
                == std::path::Path::new(&directory),
            "Settings directory changed"
        );
        if matches!(action, Action::DisablePreference) {
            ensure!(
                !flag.load(Ordering::Acquire),
                "Preference operation cancelled"
            );
            Store::from_settings_directory(explicit.as_deref())?
                .save_nxm_registration_preference(false)?;
            output.saved = Some(false);
            output.message =
                "Startup registration preference disabled; native handlers were not changed."
                    .into();
            return Ok(());
        }
        let executable = std::env::current_exe()?;
        let identity = protocol_registration::package_identity(&executable)?;
        let mut request = Request {
            executable,
            package_token: identity.package_token,
            channel: identity.channel,
            schemes: vec![Scheme::Nxm],
            replace: false,
        };
        let before = owned::status(&request, flag)?;
        let state = before
            .entries
            .first()
            .context("Protocol status is empty")?
            .state
            .clone();
        if let Some(expected) = confirmation.as_ref() {
            ensure!(
                fingerprint(&before) == fingerprint(expected),
                "Protocol handler changed since confirmation; refresh and review it again"
            );
        }
        output.report = Some(before);
        if matches!(action, Action::Status) {
            output.message = "Current protocol ownership refreshed.".into();
            return Ok(());
        }
        if matches!(action, Action::Startup) && state == Ownership::Owned {
            output.message =
                "Stored NXM startup preference is already satisfied by this package.".into();
            return Ok(());
        }
        ensure!(
            state != Ownership::Unsupported,
            "Protocol registration is unsupported on this platform"
        );
        if matches!(action, Action::Startup)
            && !matches!(state, Ownership::Absent | Ownership::Owned)
        {
            output.message = "Stored NXM preference is enabled; another handler was preserved. Explicit replacement requires confirmation.".into();
            return Ok(());
        }
        let enabled = !matches!(action, Action::Unregister);
        if enabled {
            request.replace = matches!(state, Ownership::Foreign | Ownership::OtherModSync);
            ensure!(
                !request.replace || confirmation.is_some(),
                "Replacing another handler requires explicit confirmation"
            );
        } else {
            ensure!(
                matches!(state, Ownership::Absent | Ownership::Owned),
                "Another handler is selected; no registration or preference was removed"
            );
            if cfg!(target_os = "macos") && state == Ownership::Owned {
                ensure!(
                    bundle_confirmed,
                    "Whole-bundle removal needs explicit confirmation"
                );
                // Launch Services removes an entire bundle; the UI explicitly
                // confirms this scope. The service checks both owned schemes.
                request.schemes = vec![Scheme::Modsync, Scheme::Nxm];
            }
        }
        ensure!(
            !flag.load(Ordering::Acquire),
            "Protocol operation cancelled"
        );
        let report = if enabled {
            if let Some(expected) = confirmation.as_ref() {
                owned::register_confirmed(&request, expected, flag)?
            } else {
                owned::register(&request, flag)?
            }
        } else {
            owned::unregister(&request, flag)?
        };
        let success = report.entries.iter().all(|entry| {
            if enabled {
                entry.state == Ownership::Owned
            } else {
                entry.state != Ownership::Owned && entry.state != Ownership::Unsupported
            }
        });
        output.report = Some(report);
        ensure!(
            success,
            "Protocol operation did not establish the requested ownership"
        );
        // Keep a successful native receipt even if the subsequent settings merge
        // fails. A late cancellation cannot erase a completed native operation.
        output.message = "Protocol ownership change completed; preference was not saved.".into();
        Store::from_settings_directory(explicit.as_deref())?
            .save_nxm_registration_preference(enabled)?;
        output.saved = Some(enabled);
        output.message = if enabled {
            "NXM registration verified; enabled preference saved."
        } else {
            "Owned registration removed or already absent; disabled preference saved."
        }
        .into();
        Ok(())
    })();
    if let Err(error) = result {
        if !output.message.is_empty() {
            output.message.push(' ');
        }
        output.message.push_str(&format!("Operation incomplete: {error:#}. Refresh status before retrying; native changes are not automatically rolled back."));
    }
    output
}
impl App {
    fn begin_protocol_action(&mut self, action: Action) {
        if self.desktop.busy()
            || self.receiver.is_some()
            || self.sharing.closing
            || self.probe.is_some()
            || !self.desktop.preferences_ready(&self.profile_directory)
        {
            return;
        }
        let explicit = self.desktop.settings_source();
        let directory = self.profile_directory.clone();
        let confirmation =
            if matches!(action, Action::Register) && self.desktop.protocol.replace_confirmed {
                self.desktop.protocol.report.clone()
            } else {
                None
            };
        let state = &mut self.desktop.protocol;
        let bundle_confirmed = state.bundle_confirmed;
        state.directory = directory.clone();
        state.cancel = Arc::new(AtomicBool::new(false));
        let flag = state.cancel.clone();
        state.message = "Reading protocol ownership / applying requested change…".into();
        state.replace_confirmed = false;
        state.bundle_confirmed = false;
        state.worker = Some(std::thread::spawn(move || {
            std::panic::catch_unwind(|| execute(action, explicit, directory, confirmation, bundle_confirmed, &flag)).unwrap_or_else(|_| Finished { report: None, saved: None, message: "Protocol worker stopped unexpectedly; native state may have changed. Refresh status before retrying.".into() })
        }));
    }
    pub(super) fn poll_protocol_settings(&mut self, ctx: &egui::Context) {
        let state = &mut self.desktop.protocol;
        if self.sharing.closing
            || (!state.directory.is_empty() && state.directory != self.profile_directory)
        {
            state.cancel.store(true, Ordering::Release);
        }
        if state.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            let result = state.worker.take().unwrap().join();
            match result {
                Ok(finished) => {
                    self.logs.push(finished.message.clone());
                    if let Some(report) = &finished.report
                        && let Ok(receipt) = serde_json::to_string(report)
                    {
                        self.logs.push(format!("Protocol receipt: {receipt}"));
                    }
                    if state.directory == self.profile_directory {
                        state.report = finished.report;
                        state.message = finished.message;
                        if let Some(saved) = finished.saved {
                            self.desktop.settings.register_nxm_protocol_handler = saved;
                        }
                    } else {
                        state.message = "Previous settings-directory protocol operation settled; its receipt was retained in the log.".into();
                        state.report = None;
                    }
                }
                Err(_) => {
                    state.message =
                        "Protocol worker join failed; refresh ownership before retrying.".into();
                }
            }
        }
        if state.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        if !state.busy() && state.directory != self.profile_directory {
            state.report = None;
            state.replace_confirmed = false;
            state.bundle_confirmed = false;
            state.startup = None;
            state.directory = self.profile_directory.clone();
        }
        if !self.desktop.busy()
            && self.receiver.is_none()
            && !self.sharing.closing
            && self.probe.is_none()
            && self.desktop.preferences_ready(&self.profile_directory)
            && self.desktop.protocol.startup.as_deref() != Some(&self.profile_directory)
        {
            self.desktop.protocol.startup = Some(self.profile_directory.clone());
            if self.desktop.settings.register_nxm_protocol_handler
                && cfg!(any(windows, target_os = "linux"))
            {
                self.begin_protocol_action(Action::Startup);
            }
        }
    }
    pub(super) fn protocol_settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("NXM protocol handler");
        ui.label(format!(
            "Stored startup preference: {}",
            self.desktop.settings.register_nxm_protocol_handler
        ));
        ui.label(&self.desktop.protocol.message);
        let state = &mut self.desktop.protocol;
        if let Some(report) = &state.report {
            for entry in &report.entries {
                ui.label(format!(
                    "{}: {:?} — {}",
                    entry.scheme.name(),
                    entry.state,
                    entry.handler.as_deref().unwrap_or("no selected handler")
                ));
                if let Some(message) = &entry.message {
                    ui.label(message);
                }
            }
            for warning in &report.warnings {
                ui.label(warning);
            }
        }
        if state.busy() {
            ui.spinner();
            if ui
                .button("Cancel protocol operation / wait for settlement")
                .clicked()
            {
                state.cancel.store(true, Ordering::Release);
            }
        }
        let available = !self.desktop.busy()
            && self.receiver.is_none()
            && !self.sharing.closing
            && self.probe.is_none()
            && self.desktop.preferences_ready(&self.profile_directory);
        let ownership = self
            .desktop
            .protocol
            .report
            .as_ref()
            .and_then(|r| r.entries.iter().find(|e| e.scheme == Scheme::Nxm))
            .map(|e| e.state.clone());
        let foreign = matches!(
            ownership,
            Some(Ownership::Foreign | Ownership::OtherModSync)
        );
        let mut action = None;
        ui.add_enabled_ui(available, |ui| {
            if foreign {
                ui.checkbox(
                    &mut self.desktop.protocol.replace_confirmed,
                    "Replace the displayed handler with this package (explicit confirmation)",
                );
            }
            if cfg!(target_os = "macos") {
                ui.checkbox(
                    &mut self.desktop.protocol.bundle_confirmed,
                    "Unregister this whole app bundle, including modsync:// and nxm://",
                );
            }
            ui.horizontal(|ui| {
                if ui.button("Disable startup preference only").clicked() {
                    action = Some(Action::DisablePreference);
                }
                if ui.button("Refresh protocol status").clicked() {
                    action = Some(Action::Status);
                }
                if ui
                    .add_enabled(
                        ownership.is_some()
                            && ownership != Some(Ownership::Unsupported)
                            && (!foreign || self.desktop.protocol.replace_confirmed),
                        egui::Button::new("Register NXM / enable preference"),
                    )
                    .clicked()
                {
                    action = Some(Action::Register);
                }
                let can_remove = matches!(ownership, Some(Ownership::Owned | Ownership::Absent))
                    && (!cfg!(target_os = "macos")
                        || ownership == Some(Ownership::Absent)
                        || self.desktop.protocol.bundle_confirmed);
                if ui
                    .add_enabled(
                        can_remove,
                        egui::Button::new("Unregister owned handler / disable preference"),
                    )
                    .clicked()
                {
                    action = Some(Action::Unregister);
                }
            });
        });
        if let Some(action) = action {
            self.begin_protocol_action(action);
        }
    }
}
