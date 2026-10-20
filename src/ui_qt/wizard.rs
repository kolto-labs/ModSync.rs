#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
    impl cxx_qt::Threading for WizardBackend {}
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, busy)]
        #[qproperty(QString, snapshot)]
        #[qproperty(QString, activity)]
        #[qproperty(QString, message)]
        #[qproperty(bool, easy)]
        #[qproperty(QString, desktop)]
        type WizardBackend = super::WizardBackendRust;

        #[qinvokable]
        fn bootstrap(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "loadGuide"]
        fn load_guide(self: Pin<&mut Self>, path: &QString);
        #[qinvokable]
        #[cxx_name = "setMods"]
        fn set_mods(self: Pin<&mut Self>, path: &QString);
        #[qinvokable]
        #[cxx_name = "setGame"]
        fn set_game(self: Pin<&mut Self>, path: &QString);
        #[qinvokable]
        #[cxx_name = "setSurface"]
        fn set_surface(self: Pin<&mut Self>, surface: &QString);
        #[qinvokable]
        #[cxx_name = "setSearch"]
        fn set_search(self: Pin<&mut Self>, search: &QString);
        #[qinvokable]
        #[cxx_name = "setCategoryFilter"]
        fn set_category_filter(self: Pin<&mut Self>, category: &QString);
        #[qinvokable]
        #[cxx_name = "setComponent"]
        fn set_component(self: Pin<&mut Self>, index: i32, selected: bool);
        #[qinvokable]
        #[cxx_name = "selectAll"]
        fn select_all(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "selectCategory"]
        fn select_category(self: Pin<&mut Self>, category: &QString);
        #[qinvokable]
        #[cxx_name = "deselectAll"]
        fn deselect_all(self: Pin<&mut Self>);
        #[qinvokable]
        fn next(self: Pin<&mut Self>);
        #[qinvokable]
        fn back(self: Pin<&mut Self>);
        #[qinvokable]
        fn restart(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "confirmInstall"]
        fn confirm_install(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "cancelConfirmation"]
        fn cancel_confirmation(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "runValidation"]
        fn run_validation(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "analyzeConflicts"]
        fn analyze_conflicts(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "openInstallReview"]
        fn open_install_review(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "requestStop"]
        fn request_stop(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "dismissStop"]
        fn dismiss_stop(self: Pin<&mut Self>);
        #[qinvokable]
        fn stop(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "restoreSnapshot"]
        fn restore_snapshot(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "revalidatePhase"]
        fn revalidate_phase(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "setTheme"]
        fn set_theme(self: Pin<&mut Self>, theme: &QString);
        #[qinvokable]
        #[cxx_name = "answerConsent"]
        fn answer_consent(self: Pin<&mut Self>, allow: bool, edit: &QString);
        #[qinvokable]
        #[cxx_name = "saveSettings"]
        fn save_settings(self: Pin<&mut Self>, edit: &QString);
        #[qinvokable]
        #[cxx_name = "saveTelemetry"]
        fn save_telemetry(self: Pin<&mut Self>, edit: &QString);
        #[qinvokable]
        #[cxx_name = "saveNexusKey"]
        fn save_nexus_key(self: Pin<&mut Self>, key: &QString);
        #[qinvokable]
        #[cxx_name = "profileAction"]
        fn profile_action(self: Pin<&mut Self>, action: &QString, name: &QString, target: &QString);
        #[qinvokable]
        #[cxx_name = "peerChange"]
        fn peer_change(self: Pin<&mut Self>, key: &QString, value: &QString);
        #[qinvokable]
        #[cxx_name = "pauseSharing"]
        fn pause_sharing(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "stopSharing"]
        fn stop_sharing(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "resumeSharing"]
        fn resume_sharing(self: Pin<&mut Self>);
    }
}

use super::desktop::{
    Desktop, Event, SharingAction, key_work, load_work, sharing_action_work, sharing_start_work,
    telemetry_save_work,
};
use super::flow::{Flow, JobSpec, Surface, Terminal, Update, run_job};
use anyhow::Result;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use std::pin::Pin;

pub struct WizardBackendRust {
    busy: bool,
    snapshot: QString,
    activity: QString,
    message: QString,
    easy: bool,
    desktop: QString,
    flow: Flow,
    host: Desktop,
}

impl Default for WizardBackendRust {
    fn default() -> Self {
        let flow = Flow::default();
        Self {
            busy: false,
            snapshot: QString::from(flow.snapshot().to_string().as_str()),
            activity: QString::from(flow.activity().to_string().as_str()),
            message: QString::from(""),
            easy: false,
            desktop: QString::from(""),
            flow,
            host: Desktop::new(None, false),
        }
    }
}

impl ffi::WizardBackend {
    fn flow(self: Pin<&mut Self>) -> &mut Flow {
        &mut self.rust_mut().get_mut().flow
    }

    fn publish(mut self: Pin<&mut Self>) {
        let (snapshot, activity, busy) = {
            let flow = &self.rust().flow;
            (
                flow.snapshot().to_string(),
                flow.activity().to_string(),
                flow.busy(),
            )
        };
        self.as_mut().set_snapshot(QString::from(snapshot.as_str()));
        self.as_mut().set_activity(QString::from(activity.as_str()));
        self.as_mut().set_busy(busy);
        self.publish_desktop();
    }

    fn publish_desktop(mut self: Pin<&mut Self>) {
        let desktop = self.rust().host.json().to_string();
        self.as_mut().set_desktop(QString::from(desktop.as_str()));
    }

    fn guarded(self: Pin<&mut Self>) -> Result<()> {
        self.rust().host.gate()
    }

    fn background(self: Pin<&mut Self>, work: impl FnOnce() -> Event + Send + 'static) {
        let qt = self.qt_thread();
        let queue = qt.clone();
        let started = std::thread::Builder::new()
            .name("modsync-qt-desktop".into())
            .spawn(move || {
                let event = work();
                let _ = queue.queue(move |object| object.apply_event(event));
            });
        if let Err(error) = started {
            let message = format!("Could not start the desktop worker: {error}");
            let _ = qt.queue(move |object| object.settle(Err(anyhow::anyhow!(message))));
        }
    }

    fn start_sharing(mut self: Pin<&mut Self>) {
        if self.rust().host.probe {
            return;
        }
        let directory = self.rust().host.directory.clone();
        let generation = {
            let host = &mut self.as_mut().rust_mut().get_mut().host;
            host.generation = host.generation.wrapping_add(1);
            host.sharing_busy = true;
            host.sharing_ready = false;
            host.generation
        };
        self.background(move || sharing_start_work(directory, generation));
    }

    fn sharing_action(mut self: Pin<&mut Self>, action: SharingAction) {
        let Some(owner) = Desktop::owner() else {
            return;
        };
        if self.rust().host.sharing_busy {
            return;
        }
        let guide = self.rust().flow.guide().cloned();
        self.as_mut().rust_mut().get_mut().host.sharing_busy = true;
        self.as_mut().publish_desktop();
        self.background(move || sharing_action_work(owner, action, guide));
    }

    fn apply_event(mut self: Pin<&mut Self>, event: Event) {
        match event {
            Event::Loaded(result) => match *result {
                Ok(loaded) => {
                    let warnings = self.as_mut().rust_mut().get_mut().host.apply_loaded(loaded);
                    let theme = self.rust().host.theme_name();
                    let case_insensitive = self.rust().host.settings.case_insensitive_pathing;
                    let (total, selected) = self.rust().flow.selected_count();
                    {
                        let object = self.as_mut().rust_mut().get_mut();
                        for warning in warnings {
                            object.flow.log(warning);
                        }
                        object.flow.set_theme(theme);
                        object.flow.set_case_insensitive(case_insensitive);
                        if let Err(error) = object.host.start_lifecycle(total, selected) {
                            object.host.status =
                                format!("Telemetry/log owner unavailable: {error:#}");
                        }
                    }
                    self.as_mut().start_sharing();
                    self.publish();
                }
                Err(error) => {
                    self.as_mut().rust_mut().get_mut().host.status =
                        format!("Could not load desktop preferences: {error:#}");
                    self.publish();
                }
            },
            Event::SharingStarted(result) => {
                let host = &mut self.as_mut().rust_mut().get_mut().host;
                host.sharing_busy = false;
                match result {
                    Ok(Some(owner)) => {
                        Desktop::set_owner(Some(owner));
                        host.sharing_available = true;
                        host.sharing_ready = false;
                        host.sharing_status = "Sharing is starting.".into();
                    }
                    Ok(None) => {
                        host.sharing_available = false;
                        host.sharing_ready = true;
                        host.sharing_status =
                            "Completed sharing is owned by another application instance.".into();
                    }
                    Err(error) => {
                        host.sharing_available = false;
                        host.sharing_ready = true;
                        host.sharing_status = format!("Sharing unavailable: {error:#}");
                    }
                }
                let available = self.rust().host.sharing_available;
                if available {
                    self.as_mut().sharing_action(SharingAction::ResumeMutation);
                }
                self.publish();
            }
            Event::SharingSettled { action, result } => {
                let host = &mut self.as_mut().rust_mut().get_mut().host;
                host.sharing_busy = false;
                match result {
                    Ok(()) => match action {
                        SharingAction::PauseForEdit | SharingAction::Stop => {
                            host.sharing_ready = true;
                            host.sharing_status =
                                "Sharing paused; owned worker cleanup completed.".into();
                        }
                        SharingAction::Resume => {
                            host.sharing_ready = false;
                            host.sharing_status =
                                "Sharing resumed with remaining budgets; each source is revalidated."
                                    .into();
                        }
                        SharingAction::ResumeMutation => {
                            host.sharing_ready = false;
                            host.sharing_status =
                                "Sharing context checked; user stops and settings pauses remain in effect."
                                    .into();
                        }
                    },
                    Err(error) => {
                        host.sharing_status = error.clone();
                        host.sharing_ready = false;
                    }
                }
                self.publish();
            }
            Event::TelemetrySaved(result) => {
                match *result {
                    Ok(saved) => {
                        let configured = {
                            let host = &mut self.as_mut().rust_mut().get_mut().host;
                            host.telemetry = saved.clone();
                            host.consent = None;
                            host.status = "Telemetry preference saved.".into();
                            host.configure_lifecycle(saved)
                        };
                        if let Err(error) = configured {
                            self.as_mut().rust_mut().get_mut().host.status =
                                format!("Telemetry configuration failed: {error:#}");
                        }
                    }
                    Err(error) => {
                        let host = &mut self.as_mut().rust_mut().get_mut().host;
                        if let Some(consent) = host.consent.as_mut() {
                            consent.saving = false;
                        }
                        host.status = format!("Telemetry preference not saved: {error:#}");
                    }
                }
                self.publish();
            }
            Event::KeySaved(result) => {
                let host = &mut self.as_mut().rust_mut().get_mut().host;
                host.key_busy = false;
                host.status = match result {
                    Ok(()) => "Nexus key validated and saved.".into(),
                    Err(error) => format!("Nexus key not saved: {error}"),
                };
                self.publish();
            }
        }
    }

    fn publish_activity(mut self: Pin<&mut Self>) {
        let activity = self.rust().flow.activity().to_string();
        self.as_mut().set_activity(QString::from(activity.as_str()));
    }

    fn settle(mut self: Pin<&mut Self>, result: Result<()>) {
        let message = match result {
            Ok(()) => String::new(),
            Err(error) => format!("{error:#}"),
        };
        self.as_mut().set_message(QString::from(message.as_str()));
        self.publish();
    }

    fn settle_job(mut self: Pin<&mut Self>, result: Result<Option<JobSpec>>) {
        match result {
            Ok(Some(spec)) => {
                self.as_mut().set_message(QString::from(""));
                self.as_mut().publish();
                self.spawn(spec);
            }
            Ok(None) => self.settle(Ok(())),
            Err(error) => self.settle(Err(error)),
        }
    }

    fn spawn(self: Pin<&mut Self>, spec: JobSpec) {
        let qt = self.qt_thread();
        let worker = qt.clone();
        let started = std::thread::Builder::new()
            .name("modsync-qt-engine".into())
            .spawn(move || {
                let updates = worker.clone();
                let terminal = run_job(spec, move |update| {
                    let _ = updates.queue(move |object| object.apply_update(update));
                });
                let _ = worker.queue(move |object| object.finish(terminal));
            });
        if let Err(error) = started {
            let message = format!("Could not start the worker: {error}");
            let _ = qt.queue(move |object| object.finish(Terminal::ValidationFailed(message)));
        }
    }

    fn apply_update(mut self: Pin<&mut Self>, update: Update) {
        let structural = matches!(update, Update::Prepared(_));
        self.as_mut().flow().apply_update(update);
        if structural {
            self.publish();
        } else {
            self.publish_activity();
        }
    }

    fn finish(mut self: Pin<&mut Self>, terminal: Terminal) {
        self.as_mut().flow().apply_terminal(terminal);
        self.publish();
    }

    pub fn bootstrap(mut self: Pin<&mut Self>) {
        let Some(launch) = super::take_launch() else {
            self.publish();
            return;
        };
        let easy = launch.easy_session.is_some();
        self.as_mut().rust_mut().get_mut().host =
            Desktop::new(launch.settings_dir.clone(), launch.probe);
        let explicit = launch.settings_dir.clone();
        self.as_mut().background(move || load_work(explicit));
        let result = (|| -> Result<()> {
            let flow = self.as_mut().flow();
            if let Some(mods) = &launch.mods {
                flow.set_mods(&mods.to_string_lossy())?;
            }
            if let Some(game) = &launch.game {
                flow.set_game(&game.to_string_lossy())?;
            }
            if let Some(input) = &launch.input {
                flow.load_guide(&input.to_string_lossy())?;
            }
            Ok(())
        })();
        self.as_mut().set_easy(easy);
        self.settle(result);
    }

    pub fn load_guide(mut self: Pin<&mut Self>, path: &QString) {
        let result = self
            .as_mut()
            .guarded()
            .and_then(|()| self.as_mut().flow().load_guide(&path.to_string()));
        self.settle(result);
    }

    pub fn set_mods(mut self: Pin<&mut Self>, path: &QString) {
        let result = self
            .as_mut()
            .guarded()
            .and_then(|()| self.as_mut().flow().set_mods(&path.to_string()));
        self.settle(result);
    }

    pub fn set_game(mut self: Pin<&mut Self>, path: &QString) {
        let result = self
            .as_mut()
            .guarded()
            .and_then(|()| self.as_mut().flow().set_game(&path.to_string()));
        self.settle(result);
    }

    pub fn set_surface(mut self: Pin<&mut Self>, surface: &QString) {
        self.as_mut()
            .flow()
            .set_surface(Surface::parse(&surface.to_string()));
        self.publish();
    }

    pub fn set_search(mut self: Pin<&mut Self>, search: &QString) {
        self.as_mut().flow().set_search(&search.to_string());
        self.publish();
    }

    pub fn set_category_filter(mut self: Pin<&mut Self>, category: &QString) {
        self.as_mut().flow().set_category(&category.to_string());
        self.publish();
    }

    pub fn set_component(mut self: Pin<&mut Self>, index: i32, selected: bool) {
        let result = usize::try_from(index)
            .map_err(|_| anyhow::anyhow!("Unknown mod."))
            .and_then(|index| {
                self.as_mut().guarded()?;
                self.as_mut().flow().set_component(index, selected)
            });
        self.settle(result);
    }

    pub fn select_all(mut self: Pin<&mut Self>) {
        let result = self
            .as_mut()
            .guarded()
            .and_then(|()| self.as_mut().flow().select_all());
        self.settle(result);
    }

    pub fn select_category(mut self: Pin<&mut Self>, category: &QString) {
        let result = self
            .as_mut()
            .guarded()
            .and_then(|()| self.as_mut().flow().select_category(&category.to_string()));
        self.settle(result);
    }

    pub fn deselect_all(mut self: Pin<&mut Self>) {
        let result = self
            .as_mut()
            .guarded()
            .and_then(|()| self.as_mut().flow().deselect_all());
        self.settle(result);
    }

    pub fn next(mut self: Pin<&mut Self>) {
        let result = self.as_mut().flow().request_next();
        self.settle_job(result);
    }

    pub fn back(mut self: Pin<&mut Self>) {
        self.as_mut().flow().back();
        self.settle(Ok(()));
    }

    pub fn restart(mut self: Pin<&mut Self>) {
        let result = self.as_mut().flow().restart();
        self.settle(result);
    }

    pub fn confirm_install(mut self: Pin<&mut Self>) {
        let result = self.as_mut().flow().confirm_install();
        self.settle_job(result);
    }

    pub fn cancel_confirmation(mut self: Pin<&mut Self>) {
        self.as_mut().flow().cancel_confirmation();
        self.publish();
    }

    pub fn run_validation(mut self: Pin<&mut Self>) {
        let result = self.as_mut().flow().begin_validation().map(Some);
        self.settle_job(result);
    }

    pub fn analyze_conflicts(mut self: Pin<&mut Self>) {
        let result = self.as_mut().flow().begin_conflicts().map(Some);
        self.settle_job(result);
    }

    pub fn open_install_review(mut self: Pin<&mut Self>) {
        let result = self.as_mut().flow().open_install_review();
        self.settle(result);
    }

    pub fn request_stop(mut self: Pin<&mut Self>) {
        self.as_mut().flow().request_stop();
        self.publish();
    }

    pub fn dismiss_stop(mut self: Pin<&mut Self>) {
        self.as_mut().flow().dismiss_stop();
        self.publish();
    }

    pub fn stop(mut self: Pin<&mut Self>) {
        self.as_mut().flow().stop();
        self.publish();
    }

    pub fn restore_snapshot(mut self: Pin<&mut Self>) {
        let result = self.as_mut().flow().begin_restore().map(Some);
        self.settle_job(result);
    }

    pub fn revalidate_phase(mut self: Pin<&mut Self>) {
        let result = self.as_mut().flow().revalidate_phase();
        self.settle(result);
    }

    pub fn set_theme(mut self: Pin<&mut Self>, theme: &QString) {
        let theme = theme.to_string();
        self.as_mut().flow().set_theme(&theme);
        let saved = self.as_mut().rust_mut().get_mut().host.save_theme(&theme);
        match saved {
            Ok(()) => self.publish(),
            Err(error) => self.settle(Err(error)),
        }
    }

    pub fn answer_consent(mut self: Pin<&mut Self>, allow: bool, edit: &QString) {
        let edit: serde_json::Value = serde_json::from_str(&edit.to_string()).unwrap_or_default();
        let request = {
            let host = &mut self.as_mut().rust_mut().get_mut().host;
            let Some(consent) = host.consent.as_mut() else {
                return;
            };
            if consent.saving {
                return;
            }
            let flag = |name: &str, current: bool| {
                edit.get(name)
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(current)
            };
            consent.draft.collect_usage_data = flag("usage", consent.draft.collect_usage_data);
            consent.draft.collect_performance_metrics =
                flag("performance", consent.draft.collect_performance_metrics);
            consent.draft.collect_crash_reports =
                flag("crash", consent.draft.collect_crash_reports);
            let mut config = consent.draft.clone();
            config.set_consent(allow);
            config.is_first_run = false;
            consent.saving = true;
            host.status = "Saving telemetry preference.".into();
            (host.directory.clone(), host.explicit.clone(), config)
        };
        self.as_mut().publish_desktop();
        self.background(move || telemetry_save_work(request.0, request.1, request.2));
    }

    pub fn save_telemetry(mut self: Pin<&mut Self>, edit: &QString) {
        let edit: serde_json::Value = serde_json::from_str(&edit.to_string()).unwrap_or_default();
        let request = {
            let host = &self.rust().host;
            (
                host.directory.clone(),
                host.explicit.clone(),
                host.telemetry_edit(&edit),
            )
        };
        self.as_mut().rust_mut().get_mut().host.status = "Saving telemetry preference.".into();
        self.as_mut().publish_desktop();
        self.background(move || telemetry_save_work(request.0, request.1, request.2));
    }

    pub fn save_settings(mut self: Pin<&mut Self>, edit: &QString) {
        let edit: serde_json::Value = serde_json::from_str(&edit.to_string()).unwrap_or_default();
        let result = self.as_mut().rust_mut().get_mut().host.save_settings(&edit);
        if result.is_ok() {
            let case_insensitive = self.rust().host.settings.case_insensitive_pathing;
            self.as_mut().flow().set_case_insensitive(case_insensitive);
        }
        self.settle(result);
    }

    pub fn save_nexus_key(mut self: Pin<&mut Self>, key: &QString) {
        let key = key.to_string();
        let explicit = {
            let host = &mut self.as_mut().rust_mut().get_mut().host;
            if host.key_busy {
                return;
            }
            host.key_busy = true;
            host.status = "Validating Nexus key before saving.".into();
            host.explicit.clone()
        };
        self.as_mut().publish_desktop();
        self.background(move || key_work(explicit, key));
    }

    pub fn profile_action(
        mut self: Pin<&mut Self>,
        action: &QString,
        name: &QString,
        target: &QString,
    ) {
        let (action, name, target) = (action.to_string(), name.to_string(), target.to_string());
        let result = (|| -> Result<()> {
            match action.as_str() {
                "save" => {
                    let (input, game, mods, guide) = {
                        let flow = &self.rust().flow;
                        (
                            flow.input().to_owned(),
                            flow.game().to_owned(),
                            flow.mods().to_owned(),
                            flow.guide().cloned(),
                        )
                    };
                    let guide = guide.ok_or_else(|| anyhow::anyhow!("Load a guide first."))?;
                    self.as_mut()
                        .rust_mut()
                        .get_mut()
                        .host
                        .profile_save(&name, &guide, &input, &game, &mods)
                }
                "activate" => {
                    let profile = self
                        .as_mut()
                        .rust_mut()
                        .get_mut()
                        .host
                        .profile_activate(&name)?;
                    self.as_mut().flow().apply_profile(&profile)
                }
                "clone" => self
                    .as_mut()
                    .rust_mut()
                    .get_mut()
                    .host
                    .profile_clone(&name, &target)
                    .map(|_| ()),
                "rename" => self
                    .as_mut()
                    .rust_mut()
                    .get_mut()
                    .host
                    .profile_rename(&name, &target)
                    .map(|_| ()),
                "delete" => self
                    .as_mut()
                    .rust_mut()
                    .get_mut()
                    .host
                    .profile_delete(&name),
                "refresh" => self.as_mut().rust_mut().get_mut().host.refresh_profiles(),
                _ => Err(anyhow::anyhow!("Unknown profile action")),
            }
        })();
        self.settle(result);
    }

    pub fn peer_change(mut self: Pin<&mut Self>, key: &QString, value: &QString) {
        let value: serde_json::Value = serde_json::from_str(&value.to_string()).unwrap_or_default();
        let result = self
            .as_mut()
            .rust_mut()
            .get_mut()
            .host
            .peer_change(&key.to_string(), &value);
        self.settle(result);
    }

    pub fn pause_sharing(self: Pin<&mut Self>) {
        self.sharing_action(SharingAction::PauseForEdit);
    }

    pub fn stop_sharing(self: Pin<&mut Self>) {
        self.sharing_action(SharingAction::Stop);
    }

    pub fn resume_sharing(self: Pin<&mut Self>) {
        self.sharing_action(SharingAction::Resume);
    }
}
