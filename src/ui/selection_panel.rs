//! Owned read-only archive inspection for the installer's shared selection graph.
use super::App;
use anyhow::{Context, Result};
use modsync::{
    Engine, Guide, Settings,
    selection::{Action, Availability},
};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

fn selection_messages(
    components: &[modsync::Component],
    outcome: modsync::selection::Outcome,
) -> Vec<String> {
    let mut messages: Vec<_> = outcome
        .warnings
        .into_iter()
        .map(|warning| format!("Selection warning: {warning}"))
        .collect();
    for guid in outcome.auto_selected {
        messages.push(format!(
            "Selection: auto-selected {}",
            modsync::selection::describe(components, &guid)
        ));
    }
    for guid in outcome.auto_deselected {
        messages.push(format!(
            "Selection: auto-deselected {}",
            modsync::selection::describe(components, &guid)
        ));
    }
    messages
}

struct Finished {
    stamp: String,
    result: Result<(Guide, Availability, Vec<String>)>,
}

#[derive(Default)]
pub(super) struct State {
    worker: Option<JoinHandle<Finished>>,
    cancelled: Arc<AtomicBool>,
    attempted: String,
    pub stamp: String,
    pub availability: Availability,
    pub status: String,
}
impl State {
    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl App {
    pub(super) fn start_selection(&mut self, action: Option<Action>) {
        if self.selection_panel.busy() || self.receiver.is_some() || self.sharing.closing {
            return;
        }
        if let Some(action) = &action
            && matches!(
                action,
                Action::DeselectBase
                    | Action::SetComponent {
                        selected: false,
                        ..
                    }
                    | Action::SetOption {
                        selected: false,
                        ..
                    }
                    | Action::SetWidescreen {
                        selected: false,
                        ..
                    }
                    | Action::SetGroup {
                        selected: false,
                        ..
                    }
            )
        {
            if let Some(guide) = &mut self.guide {
                match modsync::selection::apply(&mut guide.components, None, action.clone()) {
                    Ok(outcome) => self
                        .logs
                        .extend(selection_messages(&guide.components, outcome)),
                    Err(error) => self.logs.push(format!("Selection failed: {error:#}")),
                }
                self.invalidate_validation();
            }
            return;
        }
        self.refresh_input_stamp();
        let Some(mut guide) = self.guide.clone() else {
            return;
        };
        let stamp = self.input_stamp.clone();
        let mods = self.mods.clone();
        let game = self.game.clone();
        let scratch = self.scratch.clone();
        let budget = self.expanded_budget.clone();
        let case_insensitive = self.desktop.settings.case_insensitive_pathing;
        self.selection_panel.attempted = stamp.clone();
        self.selection_panel.cancelled = Arc::new(AtomicBool::new(false));
        let cancelled = self.selection_panel.cancelled.clone();
        self.selection_panel.status = "Checking local archives and required dependencies…".into();
        self.selection_panel.worker = Some(std::thread::spawn(move || {
            let result = std::panic::catch_unwind(|| -> Result<_> {
                let mut settings = Settings::new(Path::new(&mods), Path::new(&game))?;
                settings.cancelled = cancelled;
                if !scratch.is_empty() {
                    settings.archive.scratch_dir = Some(scratch.into());
                }
                if !budget.trim().is_empty() {
                    settings.archive.max_expanded_bytes = Some(
                        budget
                            .trim()
                            .parse()
                            .context("Invalid expanded-byte budget")?,
                    );
                }
                let detected = Some(modsync::game::detect(&settings.roots.game));
                let engine = Engine::new_with_case_insensitive_pathing(settings, case_insensitive)?;
                let (outcome, availability) = engine.select_available(
                    &mut guide.components,
                    detected,
                    action.unwrap_or(Action::EnforceGame),
                )?;
                let warnings = selection_messages(&guide.components, outcome);
                Ok((guide, availability, warnings))
            })
            .unwrap_or_else(|_| {
                Err(anyhow::anyhow!(
                    "Archive selection inspection stopped unexpectedly"
                ))
            });
            Finished { stamp, result }
        }));
    }

    pub(super) fn poll_selection(&mut self) {
        self.refresh_input_stamp();
        if self.sharing.closing || self.selection_panel.attempted != self.input_stamp {
            self.selection_panel
                .cancelled
                .store(true, Ordering::Release);
        }
        if self
            .selection_panel
            .worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
        {
            let worker = self
                .selection_panel
                .worker
                .take()
                .expect("finished selection worker");
            match worker.join() {
                Ok(finished) if finished.stamp == self.input_stamp && !self.sharing.closing => {
                    match finished.result {
                        Ok((guide, availability, warnings)) => {
                            self.guide = Some(guide);
                            self.invalidate_validation();
                            self.refresh_input_stamp();
                            self.logs.extend(warnings);
                            self.selection_panel.status = format!(
                                "{} unavailable mod/option(s). Unavailable selections were cleared; refresh after changing archives.",
                                availability.unavailable.len()
                            );
                            self.selection_panel.availability = availability;
                            self.selection_panel.stamp = self.input_stamp.clone();
                            self.selection_panel.attempted = self.input_stamp.clone();
                        }
                        Err(error) => {
                            self.invalidate_validation();
                            self.selection_panel.stamp.clear();
                            self.selection_panel.status =
                                format!("Archive selection checks unavailable: {error:#}");
                        }
                    }
                }
                Ok(_) => {
                    self.selection_panel.status =
                        "Inputs changed; archive selection results discarded.".into()
                }
                Err(_) => self.selection_panel.status = "Archive selection worker failed.".into(),
            }
        }
        let selecting = self.wizard.as_ref().map_or(self.page == 1, |wizard| {
            matches!(
                wizard.current(),
                modsync::wizard::Page::ModSelection | modsync::wizard::Page::WidescreenModSelection
            )
        });
        if selecting
            && !self.selection_panel.busy()
            && self.guide.is_some()
            && !self.mods.trim().is_empty()
            && !self.game.trim().is_empty()
            && self.selection_panel.attempted != self.input_stamp
        {
            self.start_selection(None);
        }
    }
}
