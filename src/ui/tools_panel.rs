//! Conflict and provider-result presentation over shared production services.
use super::{App, Job, Message};
use eframe::egui;
use modsync::updates::{StoredSummary, StoredSummaryState};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

#[derive(Default)]
pub(super) struct State {
    pub conflicts_open: bool,
    pub conflict_context: Option<String>,
    pub conflict_error: Option<String>,
    pub updates_open: bool,
    pub update_context: Option<String>,
    pub update_state: Option<StoredSummaryState>,
    pub update_note: String,
    summary_context: Option<String>,
}

pub(super) struct UpdateResult {
    pub guide: modsync::Guide,
    pub snapshot: StoredSummary,
    pub cancelled: bool,
    pub persistence_error: Option<String>,
}

impl App {
    pub(super) fn update_result(&mut self, result: UpdateResult) {
        self.refresh_input_stamp();
        let current = self.tools.update_context.as_ref() == Some(&self.input_stamp);
        let cancelled = result.cancelled;
        let eligible = current
            && !cancelled
            && self
                .guide
                .as_ref()
                .is_some_and(|guide| result.snapshot.may_apply_to(guide).unwrap_or(false));
        self.tools.update_note = if cancelled {
            "Check cancelled. Partial results remain in this window; guide metadata and the previous saved result were not replaced."
        } else if !eligible {
            "Inputs changed. Results are inspectable; guide metadata was not applied."
        } else {
            "Provider results applied to the matching guide."
        }
        .into();
        if let Some(error) = result.persistence_error {
            self.tools
                .update_note
                .push_str(&format!(" Snapshot persistence failed: {error}"));
        }
        if eligible {
            self.guide_generation = self.guide_generation.wrapping_add(1);
            self.guide = Some(result.guide);
            self.invalidate_validation();
            self.sharing.resume_after_mutation(self.guide.as_ref());
        }
        self.tools.update_state = Some(if eligible {
            StoredSummaryState::Valid {
                snapshot: result.snapshot,
            }
        } else {
            StoredSummaryState::IdentityMismatch {
                snapshot: result.snapshot,
            }
        });
        self.tools.updates_open = true;
        self.refresh_input_stamp();
        self.tools.summary_context = Some(self.input_stamp.clone());
    }

    /// Rehydrate once per settled input/settings identity, without provider I/O
    /// or guide metadata changes. Failed reads wait for an explicit retry/change.
    pub(super) fn rehydrate_update_summary(&mut self, ctx: &egui::Context) {
        if self.guide.is_none()
            || self.probe.is_some()
            || self.desktop.busy()
            || !self.desktop.preferences_ready(&self.profile_directory)
            || self.receiver.is_some()
            || self.sharing_continuation.is_some()
            || self.sharing.closing
            || self.fomod_pending()
        {
            return;
        }
        self.refresh_input_stamp();
        if self.tools.summary_context.as_ref() != Some(&self.input_stamp) {
            self.reload_update_summary(ctx);
        }
    }

    pub(super) fn reload_update_summary(&mut self, ctx: &egui::Context) {
        if self.receiver.is_some()
            || self.sharing_continuation.is_some()
            || self.sharing.closing
            || self.fomod_pending()
        {
            return;
        }
        let Some(guide) = self.guide.clone() else {
            self.tools.update_note = "Load a guide first.".into();
            return;
        };
        self.refresh_input_stamp();
        let context = self.input_stamp.clone();
        self.tools.summary_context = Some(context.clone());
        let directory = PathBuf::from(&self.profile_directory);
        self.cancelled = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.job = Job::UpdateSummary;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let state =
                std::panic::catch_unwind(|| modsync::updates::load_snapshot(&directory, &guide))
                    .unwrap_or_else(|_| StoredSummaryState::Corrupt {
                        message: "Summary reader stopped unexpectedly.".into(),
                    });
            let _ = tx.send(Message::UpdateSummary(context, state));
            ctx.request_repaint();
        });
    }

    pub(super) fn tools_windows(&mut self, ctx: &egui::Context) {
        if self.tools.conflicts_open {
            let mut open = true;
            egui::Window::new("File conflicts")
                .open(&mut open)
                .default_size([700.0, 560.0])
                .vscroll(true)
                .show(ctx, |ui| {
                    if self.receiver.is_some() && self.job == Job::Conflicts {
                        ui.spinner();
                        ui.label("Analyzing ordered writes…");
                    }
                    if let Some(error) = &self.tools.conflict_error {
                        ui.colored_label(egui::Color32::RED, error);
                    }
                    if let Some(report) = &self.conflicts {
                        match modsync::conflicts::Presentation::from_report(report) {
                            Ok(view) => {
                                ui.heading(&view.summary);
                                if ui.button("Copy conflict JSON").clicked() {
                                    ctx.copy_text(view.report_json);
                                }
                                for limitation in &view.limitations {
                                    ui.colored_label(egui::Color32::YELLOW, limitation);
                                }
                                ui.collapsing("Component conflict counts", |ui| {
                                    for component in &view.components {
                                        ui.label(format!(
                                            "{} [{}]: {} paths",
                                            component.component_name,
                                            component.component_guid,
                                            component.conflicting_paths
                                        ));
                                    }
                                });
                                for row in &view.rows {
                                    ui.collapsing(&row.destination_path, |ui| {
                                        for writer in &row.writers {
                                            let text = format!(
                                                "{} [{}] — instruction {}{}",
                                                writer.component_name,
                                                writer.component_guid,
                                                writer.instruction_index + 1,
                                                if writer.is_winner {
                                                    " — winning component"
                                                } else {
                                                    ""
                                                }
                                            );
                                            if writer.is_winner {
                                                ui.strong(text);
                                            } else {
                                                ui.label(text);
                                            }
                                        }
                                    });
                                }
                            }
                            Err(error) => {
                                ui.colored_label(egui::Color32::RED, error.to_string());
                            }
                        }
                    } else if self.receiver.is_none() {
                        ui.label(
                            "No current conflict result. Analyze the current guide and folders.",
                        );
                    }
                });
            self.tools.conflicts_open = open;
        }
        if self.tools.updates_open {
            let mut open = true;
            let mut retry = false;
            let mut reload = false;
            let available = !self.desktop.busy()
                && self.receiver.is_none()
                && self.sharing_continuation.is_none()
                && !self.sharing.closing
                && !self.fomod_pending();
            egui::Window::new("Provider updates").open(&mut open).default_size([700.0, 560.0]).vscroll(true).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    retry = ui.add_enabled(available, egui::Button::new("Check now / retry")).clicked();
                    reload = ui.add_enabled(available, egui::Button::new("Reload last result")).clicked();
                    if self.receiver.is_some() && self.job == Job::Update && ui.button("Cancel check").clicked() { self.cancelled.store(true, Ordering::Release); }
                });
                if self.receiver.is_some() && matches!(self.job, Job::Update | Job::UpdateSummary) { ui.spinner(); ui.label("Reading provider results…"); }
                ui.label(&self.tools.update_note);
                if let Some(state) = &self.tools.update_state {
                    if ui.button("Copy update result").clicked() { match modsync::editor::render_preview(state) { Ok(text) => ctx.copy_text(text), Err(error) => self.logs.push(error.to_string()) } }
                    match state {
                        StoredSummaryState::Absent => { ui.label("No saved result."); }
                        StoredSummaryState::UnsupportedVersion { version } => { ui.label(format!("Unsupported result schema {version}.")); }
                        StoredSummaryState::Corrupt { message } => { ui.colored_label(egui::Color32::RED, message); }
                        StoredSummaryState::Valid { snapshot } | StoredSummaryState::IdentityMismatch { snapshot } => {
                            if matches!(state, StoredSummaryState::IdentityMismatch { .. }) { ui.colored_label(egui::Color32::YELLOW, "Historical or cancelled result; it is not authority to modify this guide."); }
                            ui.label(format!("Checked {}", snapshot.checked_at));
                            let summary = &snapshot.summary;
                            ui.label(format!("Checked: {} — skipped: {} — updates: {} — rate limit reached: {}", summary.checked_count, summary.skipped_count, summary.updates_found.len(), summary.rate_limit_reached));
                            for update in &summary.updates_found { ui.group(|ui| { ui.strong(&update.component_name); ui.label(format!("{} → {}", update.installed_version, update.latest_version)); ui.hyperlink(&update.url); }); }
                            for error in &summary.errors { ui.colored_label(egui::Color32::RED, error); }
                        }
                    }
                }
            });
            self.tools.updates_open = open;
            if retry {
                self.start_update_check(ctx);
            }
            if reload {
                self.reload_update_summary(ctx);
            }
        }
    }
}
