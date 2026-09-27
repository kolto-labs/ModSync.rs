//! Native validation page. Diagnostics and report text belong to the shared library.
use super::{App, Job};
use eframe::egui::{self, Color32, RichText};
use modsync::validation::{Report, StageProgress, presentation};
use std::time::{Duration, Instant};

pub(super) struct State {
    pub run: u64,
    active: bool,
    started: Option<Instant>,
    selected: usize,
    pub progress: Option<StageProgress>,
    view: presentation::Presentation,
    log: Vec<String>,
    visible: bool,
    expanded: bool,
    completed: bool,
    log_height: f32,
    log_offset: f32,
    result_offset: f32,
    first_card: Option<usize>,
    card_offsets: Vec<f32>,
    pending_issue: bool,
    splitter_preview: Option<f32>,
    copied_until: Option<Instant>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            run: 0,
            active: false,
            started: None,
            selected: 0,
            progress: None,
            view: Default::default(),
            log: Vec::new(),
            visible: false,
            expanded: false,
            completed: false,
            log_height: 150.0,
            log_offset: 0.0,
            result_offset: 0.0,
            first_card: None,
            card_offsets: Vec::new(),
            pending_issue: false,
            splitter_preview: None,
            copied_until: None,
        }
    }
}
impl State {
    pub fn begin(&mut self, selected: usize) {
        let run = self.run.wrapping_add(1);
        let height = self.log_height;
        *self = Self {
            run,
            active: true,
            started: Some(Instant::now()),
            selected,
            visible: true,
            expanded: true,
            log_height: height,
            log: vec!["Starting validation...".into()],
            ..Self::default()
        };
        self.view.status = "Running validation...".into();
    }
    pub fn accepts(&self, run: u64) -> bool {
        self.run == run && self.active
    }
    pub fn invalidate(&mut self) {
        let run = self.run;
        *self = Self {
            run,
            ..Self::default()
        };
    }
    pub fn fail(&mut self, message: &str) {
        self.active = false;
        self.completed = false;
        self.visible = true;
        self.expanded = true;
        self.progress = None;
        self.view = presentation::Presentation {
            status: "Validation stopped".into(),
            summary: "Validation did not complete".into(),
            details: message.into(),
            ..Default::default()
        };
        self.log.push(format!("ERROR: {message}"));
    }
    pub fn complete(&mut self, report: &Report) {
        self.active = false;
        self.completed = true;
        self.view = presentation::project(report, self.selected);
        self.log = vec!["Starting validation...".into()];
        self.log.extend(self.view.log_lines.iter().cloned());
        let elapsed = self.started.map_or(0.0, |s| s.elapsed().as_secs_f64());
        self.log
            .push(format!("Validation completed in {elapsed:.1} seconds"));
        self.log.push(
            if report.success {
                "✅ Validation passed"
            } else {
                "❌ Validation failed"
            }
            .into(),
        );
        self.expanded = report.error_count > 0 || report.warning_count > 0;
        if self.expanded {
            self.first_card = self.view.first_issue_card(report.warning_count);
            self.log_offset =
                presentation::first_issue_log(&self.log.join("\n"), report.warning_count)
                    .map_or(0.0, |line| (line as f32 * 15.0 - 30.0).max(0.0));
            self.pending_issue = true;
        }
        if let Some(progress) = &mut self.progress {
            progress.step = progress.total;
            progress.message.clear();
        }
    }
    fn progress_text(&self) -> String {
        let Some(p) = &self.progress else {
            return String::new();
        };
        // Match the oracle's current-stage fraction using actual observed boundaries.
        let done = p.step;
        let percent = if p.total == 0 {
            0.0
        } else {
            done as f64 * 100.0 / p.total as f64
        };
        let mut text = format!("{percent:.0}%");
        if self.active && !p.message.starts_with("Completed: ") {
            text.push_str(" • ");
            text.push_str(&p.message);
        }
        if self.active && done > 0 && done < p.total {
            let remaining = self.started.map_or(0.0, |s| s.elapsed().as_secs_f64())
                * (p.total - done) as f64
                / done as f64;
            text.push_str(&if remaining < 60.0 {
                format!(" • ETA: {remaining:.0}s")
            } else {
                format!(" • ETA: {:.1}m", remaining / 60.0)
            });
        }
        text
    }
}

/// Clear both representations together so utility jobs and resets cannot leave stale cards.
pub(super) fn clear_report(report: &mut Option<Report>, state: &mut State) {
    *report = None;
    state.invalidate();
}

fn frame(radius: u8, x: i8, y: i8) -> egui::Frame {
    egui::Frame::default()
        .corner_radius(radius)
        .inner_margin(egui::Margin::symmetric(x, y))
}
impl App {
    pub(super) fn validation_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let total = self.guide.as_ref().map_or(0, |g| g.components.len());
        let selected = self
            .guide
            .as_ref()
            .map_or(0, |g| g.components.iter().filter(|c| c.is_selected).count());
        let busy = self.receiver.is_some();
        let is_light = self.theme.to_ascii_lowercase().contains("light");
        let k2 = self.theme.to_ascii_lowercase().contains("kotor2");
        let foreground = if is_light {
            Color32::BLACK
        } else if k2 {
            Color32::from_rgb(24, 174, 136)
        } else {
            Color32::from_rgb(58, 170, 255)
        };
        let border = if is_light {
            Color32::from_rgb(208, 208, 208)
        } else {
            foreground
        };
        let button_fill = if is_light {
            Color32::from_rgb(240, 240, 240)
        } else if k2 {
            Color32::from_rgb(2, 17, 13)
        } else {
            Color32::from_rgb(1, 1, 22)
        };
        let button_border = if is_light {
            Color32::from_rgb(173, 173, 173)
        } else if k2 {
            Color32::from_rgb(13, 89, 69)
        } else {
            Color32::from_rgb(6, 39, 102)
        };
        let hover_border = if is_light {
            Color32::from_rgb(0, 120, 212)
        } else if k2 {
            Color32::from_rgb(224, 242, 241)
        } else {
            Color32::from_rgb(168, 179, 72)
        };
        let hover_text = if is_light {
            foreground
        } else if k2 {
            Color32::from_rgb(242, 242, 242)
        } else {
            Color32::from_rgb(168, 179, 72)
        };
        let visuals = ui.visuals_mut();
        visuals.override_text_color = None;
        visuals.widgets.noninteractive.fg_stroke.color = foreground;
        visuals.widgets.inactive.fg_stroke.color = foreground;
        visuals.widgets.active.fg_stroke.color = foreground;
        visuals.widgets.hovered.fg_stroke.color = hover_text;
        visuals.widgets.inactive.weak_bg_fill = button_fill;
        visuals.widgets.inactive.bg_stroke =
            egui::Stroke::new(if is_light { 1.0 } else { 2.0 }, button_border);
        visuals.widgets.hovered.weak_bg_fill = if is_light {
            Color32::from_rgb(225, 225, 225)
        } else if k2 {
            button_fill
        } else {
            Color32::from_rgb(0, 1, 22)
        };
        visuals.widgets.hovered.bg_stroke =
            egui::Stroke::new(if is_light { 1.0 } else { 2.0 }, hover_border);
        visuals.widgets.active.weak_bg_fill = if is_light {
            Color32::from_rgb(208, 208, 208)
        } else {
            visuals.widgets.hovered.weak_bg_fill
        };
        visuals.widgets.active.bg_stroke = egui::Stroke::new(
            if is_light { 1.0 } else { 2.0 },
            if is_light {
                Color32::from_rgb(0, 90, 158)
            } else if k2 {
                Color32::from_rgb(63, 230, 192)
            } else {
                foreground
            },
        );
        ui.spacing_mut().item_spacing.y = 8.0;
        let width = ui.available_width();
        frame(10,20,16).show(ui, |ui| {
            ui.set_width((width - 40.0).max(1.0));
            ui.label(RichText::new("🔍 Pre-Installation Validation").size(20.0).strong());
            ui.label(RichText::new("We'll check for potential issues before starting the installation to ensure everything works smoothly.").size(14.0).color(foreground.gamma_multiply(0.85)));
        });
        ui.add_space(4.0);
        let mut run = false;
        frame(8, 16, 12).show(ui, |ui| {
            ui.set_width((width - 32.0).max(1.0));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(16.0, 10.0);
                run = ui
                    .add_enabled(
                        !busy && selected > 0,
                        egui::Button::new("🔍 Run Validation"),
                    )
                    .on_hover_text("Check for compatibility issues and missing files")
                    .clicked();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(&self.validation_page.view.status)
                            .size(14.0)
                            .strong(),
                    );
                    if self.validation_page.active {
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(200.0_f32.min(ui.available_width()), 8.0),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(rect, 4.0, border);
                        let phase = ((ctx.input(|i| i.time) * 1.5).sin() as f32 + 1.0) * 0.5;
                        let segment = egui::Rect::from_min_size(
                            rect.min + egui::vec2(phase * rect.width() * 0.75, 0.0),
                            egui::vec2(rect.width() * 0.25, 8.0),
                        );
                        ui.painter().rect_filled(segment, 4.0, foreground);
                        if ui.button("Stop").clicked() {
                            self.stop_confirmation = true;
                        }
                    }
                });
            });
        });
        if run {
            self.start(Job::Validate, ctx);
        }
        let state = &mut self.validation_page;
        if state.active {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        let report = self.validation_report.as_ref();
        let warnings = report.map_or(0, |r| r.warning_count);
        let errors = report.map_or(0, |r| r.error_count);
        let log_limit = (ui.available_height() - 260.0).max(56.0);
        state.log_height = state.log_height.clamp(56.0, log_limit);
        if state.visible {
            frame(0,12,12).show(ui, |ui| {
                ui.set_width((width - 24.0).max(1.0));
                ui.horizontal_wrapped(|ui| {
                    let arrow = if state.expanded { "▼" } else { "▶" };
                    if ui.button(RichText::new(format!("{arrow} 📋 Validation Log")).size(13.0).strong()).clicked() { state.expanded = !state.expanded; }
                    let progress_text = state.progress_text();
                    if !progress_text.is_empty() { ui.label(RichText::new(progress_text).size(12.0).color(foreground.gamma_multiply(0.7))); }
                    if state.completed && (errors > 0 || warnings > 0) && ui.button("Go to first issue").on_hover_text("Expand the log, flush pending lines, and scroll to the first error or warning in the log and results").clicked() {
                        state.expanded = true;
                        state.log_offset = presentation::first_issue_log(&state.log.join("\n"), warnings).map_or(0.0, |line| (line as f32 * 15.0 - 30.0).max(0.0));
                        state.first_card = state.view.first_issue_card(warnings);
                        if let Some(index) = state.first_card { state.result_offset = state.card_offsets.get(index).copied().unwrap_or(0.0); }
                    }
                    if state.completed && let Some(report) = report {
                        let copied = state.copied_until.is_some_and(|until| Instant::now() < until);
                        if copied { ctx.request_repaint_after(Duration::from_millis(100)); }
                        if ui.button(if copied { "Copied!" } else { "Copy report" }).on_hover_text("Copy validation results and log to the clipboard").clicked() {
                            ctx.copy_text(presentation::format_report(&state.view, report, &chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(), &state.log.join("\n")));
                            state.copied_until = Some(Instant::now() + Duration::from_millis(1500));
                        }
                    }
                    ui.label(RichText::new("Drag the splitter below to resize").size(11.0).color(foreground.gamma_multiply(0.6)));
                });
                if state.expanded {
                    frame(4,12,12).stroke(egui::Stroke::new(1.0,border)).show(ui, |ui| {
                        ui.set_width((width - 50.0).max(1.0));
                        let output = egui::ScrollArea::vertical().id_salt("validation_log")
                            .vertical_scroll_offset(state.log_offset).max_height(state.log_height.min(log_limit).max(56.0)).min_scrolled_height(56.0).auto_shrink([false,false]).show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                for line in &state.log { ui.add(egui::Label::new(RichText::new(line).monospace().size(11.0)).wrap()); }
                            });
                        state.log_offset = output.state.offset.y;
                    });
                }
            });
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(width, 6.0), egui::Sense::drag());
            ui.painter()
                .rect_filled(rect, 0.0, border.gamma_multiply(0.5));
            if response.drag_started() {
                state.splitter_preview = Some(state.log_height);
            }
            if response.dragged() {
                let preview = (state.splitter_preview.unwrap_or(state.log_height)
                    + response.drag_delta().y)
                    .clamp(56.0, log_limit);
                state.splitter_preview = Some(preview);
                let y = rect.center().y + preview - state.log_height;
                ui.painter()
                    .hline(rect.x_range(), y, egui::Stroke::new(2.0, border));
            }
            if response.drag_stopped()
                && let Some(height) = state.splitter_preview.take()
            {
                state.log_height = height;
            }
            response
                .on_hover_cursor(egui::CursorIcon::ResizeVertical)
                .on_hover_text("Drag to resize the validation log");
        }
        let results_height = (ui.available_height() - 134.0).max(56.0);
        frame(8, 12, 12).show(ui, |ui| {
            ui.set_width((width - 24.0).max(1.0));
            let output = egui::ScrollArea::vertical()
                .id_salt("validation_results")
                .vertical_scroll_offset(state.result_offset)
                .max_height(results_height)
                .min_scrolled_height(56.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let origin = ui.cursor().min.y;
                    state.card_offsets.clear();
                    for (index, card) in state.view.cards.iter().enumerate() {
                        state.card_offsets.push(ui.cursor().min.y - origin);
                        let stroke = if state.first_card == Some(index) {
                            egui::Stroke::new(
                                2.0,
                                if card.title.starts_with('❌') {
                                    Color32::from_rgb(229, 57, 53)
                                } else {
                                    Color32::from_rgb(251, 140, 0)
                                },
                            )
                        } else {
                            egui::Stroke::NONE
                        };
                        frame(8, 16, 12).stroke(stroke).show(ui, |ui| {
                            ui.set_width((width - 60.0).max(1.0));
                            ui.spacing_mut().item_spacing.y = 6.0;
                            ui.label(RichText::new(&card.title).size(14.0).strong());
                            ui.label(
                                RichText::new(&card.message)
                                    .size(13.0)
                                    .color(foreground.gamma_multiply(0.85)),
                            );
                        });
                        ui.add_space(10.0);
                    }
                });
            state.result_offset = output.state.offset.y;
            if state.pending_issue {
                if let Some(index) = state.first_card {
                    state.result_offset = state.card_offsets.get(index).copied().unwrap_or(0.0);
                }
                state.pending_issue = false;
                ctx.request_repaint();
            }
        });
        ui.add_space(4.0);
        frame(10,20,16).show(ui, |ui| {
            ui.set_width((width - 40.0).max(1.0));
            let summary = if state.view.summary.is_empty() { if selected == 0 { "Select mods before validating" } else { "Click 'Run Validation' to begin" } } else { &state.view.summary };
            let details = if state.view.details.is_empty() { if selected == 0 { "No mods are selected. Go back to Mod Selection and choose mods to install.".into() } else { format!("Ready to validate {selected} of {total} mod(s). Click Run Validation to check your setup.") } } else { state.view.details.clone() };
            ui.horizontal_wrapped(|ui| {
                ui.allocate_ui_with_layout(egui::vec2((width - 290.0).max(120.0),60.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_min_width((width - 290.0).max(120.0));
                    ui.label(RichText::new(summary).size(16.0).strong());
                    ui.label(RichText::new(details).size(13.0).color(foreground.gamma_multiply(0.7)));
                });
                ui.spacing_mut().item_spacing.x = 16.0;
                for (label,count) in [("ERRORS",errors),("WARNINGS",warnings),("PASSED",report.map_or(0,|r| r.passed_count))] {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(label).size(11.0).color(foreground.gamma_multiply(0.6)));
                        ui.label(RichText::new(count.to_string()).size(20.0).strong());
                    });
                }
            });
        });
    }
}
