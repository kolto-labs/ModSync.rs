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
    }
}

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
    flow: Flow,
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
            flow,
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
        let result = self.as_mut().flow().load_guide(&path.to_string());
        self.settle(result);
    }

    pub fn set_mods(mut self: Pin<&mut Self>, path: &QString) {
        let result = self.as_mut().flow().set_mods(&path.to_string());
        self.settle(result);
    }

    pub fn set_game(mut self: Pin<&mut Self>, path: &QString) {
        let result = self.as_mut().flow().set_game(&path.to_string());
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
            .and_then(|index| self.as_mut().flow().set_component(index, selected));
        self.settle(result);
    }

    pub fn select_all(mut self: Pin<&mut Self>) {
        let result = self.as_mut().flow().select_all();
        self.settle(result);
    }

    pub fn select_category(mut self: Pin<&mut Self>, category: &QString) {
        let result = self.as_mut().flow().select_category(&category.to_string());
        self.settle(result);
    }

    pub fn deselect_all(mut self: Pin<&mut Self>) {
        let result = self.as_mut().flow().deselect_all();
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
        self.as_mut().flow().set_theme(&theme.to_string());
        self.publish();
    }
}
