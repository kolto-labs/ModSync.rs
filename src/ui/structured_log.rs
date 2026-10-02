//! Typed application logger view. Acquisition/install activity remains separate.
use eframe::egui;
use modsync::logging::{self, Level, Record};
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct State {
    selected: BTreeSet<u64>,
    highlighted: BTreeSet<u64>,
    raw_open: bool,
    auto_scroll: bool,
    cached: Vec<Record>,
    clear_pending: bool,
}

/// Captured bounded snapshot: the caller saves these exact bytes asynchronously.
pub(super) struct SaveSnapshot {
    pub text: String,
    pub record_count: usize,
}

fn severity(level: Level) -> &'static str {
    match level {
        Level::Verbose => "Verbose",
        Level::Info => "Info",
        Level::Warning => "Warning",
        Level::Error => "Error",
    }
}
fn snapshot(records: &[&Record]) -> SaveSnapshot {
    use std::fmt::Write;
    let truncated = records.iter().filter(|record| record.truncated).count();
    let mut text = format!(
        "Application log snapshot: {} retained records; {truncated} truncated messages.\nThis is the bounded recent history, not the complete persistent log file.\n",
        records.len()
    );
    for record in records {
        let _ = writeln!(
            text,
            "{} [{}] {}{}",
            record.timestamp.to_rfc3339(),
            severity(record.level),
            record.message,
            if record.truncated {
                "\n[Message truncated at 64 KiB]"
            } else {
                ""
            }
        );
    }
    SaveSnapshot {
        text,
        record_count: records.len(),
    }
}
impl State {
    /// Returns a save request; never opens a picker or writes a file on the UI thread.
    pub(super) fn show(&mut self, ui: &mut egui::Ui, available: bool) -> Option<SaveSnapshot> {
        if self.clear_pending && logging::try_clear_recent() {
            self.clear_pending = false;
            self.cached.clear();
            self.selected.clear();
            self.highlighted.clear();
        }
        let busy = if let Some(records) = logging::try_recent_records(logging::RECENT_RECORD_LIMIT)
        {
            self.cached = records;
            false
        } else {
            true
        };
        if busy || self.clear_pending {
            ui.label(if self.clear_pending {
                "Clear queued; waiting for the logger. The last snapshot remains visible."
            } else {
                "Logger busy; showing the last captured snapshot."
            });
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        let records = &self.cached;
        let present: BTreeSet<_> = records.iter().map(|record| record.id).collect();
        self.selected.retain(|id| present.contains(id));
        self.highlighted.retain(|id| present.contains(id));
        let mut save = None;
        ui.label(format!(
            "{} retained application records · {} selected",
            records.len(),
            self.selected.len()
        ));
        ui.label("History retains at most 4,096 records and 1 MiB of message text; configured retention can be lower. Messages over 64 KiB are marked truncated.");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Select all retained").clicked() {
                self.selected = present;
            }
            if ui
                .add_enabled(
                    !self.selected.is_empty(),
                    egui::Button::new("Toggle selected highlights"),
                )
                .clicked()
            {
                toggle_highlights(&self.selected, &mut self.highlighted);
            }
            if ui.button("Clear selection").clicked() {
                self.selected.clear();
            }
            if ui
                .add_enabled(
                    !self.selected.is_empty(),
                    egui::Button::new("Copy selected"),
                )
                .clicked()
            {
                let chosen: Vec<_> = records
                    .iter()
                    .filter(|record| self.selected.contains(&record.id))
                    .collect();
                ui.ctx().copy_text(snapshot(&chosen).text);
            }
            if ui.button("Copy all retained").clicked() {
                ui.ctx()
                    .copy_text(snapshot(&records.iter().collect::<Vec<_>>()).text);
            }
            if ui
                .add_enabled(available, egui::Button::new("Save all retained…"))
                .clicked()
            {
                save = Some(snapshot(&records.iter().collect::<Vec<_>>()));
            }
            ui.checkbox(&mut self.auto_scroll, "Auto-scroll");
            if ui.button("Clear recent history").clicked() {
                self.clear_pending = true;
                ui.ctx().request_repaint();
            }
            ui.checkbox(
                &mut self.raw_open,
                "Selectable full text of selected records",
            );
        });
        ui.label(
            "Time (local offset) · Severity · Message — select rows to inspect multiline text; H on a focused row toggles selected highlights",
        );
        let height = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
        egui::ScrollArea::vertical()
            .id_salt("structured_log_rows")
            .stick_to_bottom(self.auto_scroll)
            .max_height(340.0)
            .show_rows(ui, height, records.len(), |ui, range| {
                for index in range {
                    let record = &records[index];
                    let first = record.message.lines().next().unwrap_or("");
                    let preview: String = first.chars().take(160).collect();
                    let clipped = first.chars().count() > 160 || record.message.contains('\n');
                    let label = format!(
                        "{}  {:7}  {}{}{}",
                        record.timestamp.format("%H:%M:%S%.3f %:z"),
                        severity(record.level),
                        preview,
                        if clipped { " …" } else { "" },
                        if record.truncated { " [truncated]" } else { "" }
                    );
                    let response = ui.selectable_label(self.selected.contains(&record.id), label);
                    if response.clicked() {
                        response.request_focus();
                        if !self.selected.remove(&record.id) {
                            self.selected.insert(record.id);
                        }
                    }
                    if response.has_focus()
                        && ui.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::H)
                        })
                    {
                        toggle_highlights(&self.selected, &mut self.highlighted);
                    }
                    if self.highlighted.contains(&record.id) {
                        ui.painter().rect_stroke(
                            response.rect,
                            2.0,
                            egui::Stroke::new(1.5, ui.visuals().warn_fg_color),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
            });
        if self.raw_open {
            let chosen: Vec<_> = records
                .iter()
                .filter(|record| self.selected.contains(&record.id))
                .collect();
            let text = snapshot(&chosen).text;
            let mut read_only = text.as_str();
            egui::ScrollArea::both()
                .id_salt("structured_log_raw")
                .max_height(240.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut read_only)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    );
                });
        }
        save
    }
}

fn toggle_highlights(selected: &BTreeSet<u64>, highlighted: &mut BTreeSet<u64>) {
    for id in selected {
        if !highlighted.remove(id) {
            highlighted.insert(*id);
        }
    }
}
