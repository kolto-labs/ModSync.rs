//! Base Installation Complete, Widescreen Support, Widescreen Installation
//! Complete and Finished pages. Install facts come from the shared completion
//! receipt read by the worker; this module only formats them.
use super::App;
use super::installing_page::{card, columns, faded};
use eframe::egui::{self, RichText};
use modsync::paths::Roots;
use modsync::recovery::{self, CompletionSummary};
use modsync::wizard::metadata_text;
use std::path::Path;
use std::time::Duration;

/// Plain-text replacement for the C# rollback promise. Rust keeps one snapshot
/// per run and offers restore only while that run's snapshot is pending.
const RECOVERY_TEXT: &str = "This run finished. Its snapshot was released, and the next install replaces it. Restore is offered only when an install stops partway.";

/// What the install worker read after the engine returned Ok and released its
/// recovery lock. Sent with the terminal message; never computed on the UI thread.
pub(super) struct Terminal {
    /// Completed component ids the engine was given for this phase.
    prefix: usize,
    /// Roots captured for this run, not the editable folder text.
    roots: Roots,
    summary: Result<Option<CompletionSummary>, String>,
}

impl Terminal {
    /// Read the saved receipt for `roots`. A read error never turns a committed
    /// success into a failure; it is shown as an unavailable summary.
    pub fn read(prefix: usize, roots: Roots) -> Self {
        let summary = std::panic::catch_unwind(|| recovery::completion_summary(&roots))
            .map_or_else(
                |_| Err("The summary reader stopped unexpectedly.".to_owned()),
                |result| result.map_err(|error| format!("{error:#}")),
            );
        Self {
            prefix,
            roots,
            summary,
        }
    }
}

/// One accepted phase terminal, copied out before the Installing page state is
/// replaced by the next phase.
struct PhaseReceipt {
    /// Receipt components minus the prefix the engine was given.
    installed_in_phase: Option<usize>,
    elapsed: Duration,
    summary: Result<Option<CompletionSummary>, String>,
    skipped: bool,
    log: Vec<String>,
    roots: Option<Roots>,
}

#[derive(Default)]
pub(super) struct State {
    base: Option<PhaseReceipt>,
    remaining: Option<PhaseReceipt>,
    feedback: Option<String>,
    recovery_open: bool,
}

impl State {
    /// Keep an accepted engine success for the Base or Remaining phase.
    pub fn record(&mut self, base: bool, terminal: Terminal, elapsed: Duration, log: Vec<String>) {
        let installed_in_phase = match &terminal.summary {
            Ok(Some(summary)) => Some(summary.components.len().saturating_sub(terminal.prefix)),
            _ => None,
        };
        let receipt = PhaseReceipt {
            installed_in_phase,
            elapsed,
            summary: terminal.summary,
            skipped: false,
            log,
            roots: Some(terminal.roots),
        };
        if base {
            self.base = Some(receipt);
        } else {
            self.remaining = Some(receipt);
        }
    }

    /// The empty widescreen phase: no engine run and no receipt change.
    pub fn skip_remaining(&mut self) {
        self.remaining = Some(PhaseReceipt {
            installed_in_phase: Some(0),
            elapsed: Duration::ZERO,
            summary: Ok(None),
            skipped: true,
            log: Vec::new(),
            roots: None,
        });
    }

    fn widescreen_skipped(&self) -> bool {
        self.remaining.as_ref().is_some_and(|r| r.skipped)
    }
}

/// C# `BaseInstallCompletePage`: `{minutes}:{ss}` from one minute, else `{s}s`.
fn elapsed_text(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    if seconds >= 60 {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

/// Stroke and radius for the notice and epilogue boxes, from the active theme.
fn bordered(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::default()
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(8)
        .inner_margin(egui::Margin::same(12))
}

/// Space left in the visible page below the cursor, never under `min`.
fn visible_below(ui: &egui::Ui, reserve: f32, min: f32) -> f32 {
    (ui.clip_rect().bottom() - ui.cursor().top() - reserve).max(min)
}

fn wrapped(ui: &mut egui::Ui, text: RichText) -> egui::Response {
    ui.add(egui::Label::new(text).wrap())
}

/// A card no wider than `max`, centered in a centered stack.
fn capped(ui: &mut egui::Ui, max: f32, frame: egui::Frame, body: impl FnOnce(&mut egui::Ui)) {
    let width = ui.available_width().min(max);
    ui.allocate_ui_with_layout(
        egui::vec2(width, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(width);
            frame.show(ui, |ui| {
                ui.set_width(ui.available_width());
                body(ui);
            });
        },
    );
}

enum Action {
    CopyLog,
    Recovery,
    OpenGame,
    OpenMods,
}

impl App {
    pub(super) fn base_install_complete_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let receipt = self.completion.base.as_ref();
        let mods = receipt
            .and_then(|r| r.installed_in_phase)
            .map_or_else(|| "N/A".to_owned(), |n| n.to_string());
        let time = receipt.map_or_else(|| "N/A".to_owned(), |r| elapsed_text(r.elapsed));
        let files = match receipt.map(|r| &r.summary) {
            Some(Ok(Some(summary))) => summary
                .game_delta_since_run_start
                .as_ref()
                .map(|d| d.added + d.changed + d.removed)
                .filter(|n| *n > 0)
                .map_or_else(|| "N/A".to_owned(), |n| n.to_string()),
            _ => "N/A".to_owned(),
        };
        let unavailable = match receipt.map(|r| &r.summary) {
            None => {
                Some("Summary unavailable: this install's result was not recorded here.".to_owned())
            }
            Some(Err(error)) => Some(format!("Summary unavailable: {error}")),
            Some(Ok(None)) => {
                Some("Summary unavailable: no completion receipt was found.".to_owned())
            }
            Some(Ok(Some(_))) => None,
        };
        let stats = [
            ("MODS INSTALLED", mods),
            ("RECOVERY SNAPSHOT", "Released".to_owned()),
            ("TIME ELAPSED", time),
            ("GAME FILES CHANGED", files),
        ];
        let mut action = None;
        egui::Frame::default()
            .inner_margin(egui::Margin::symmetric(24, 16))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                // Row 0: success header.
                card(16, 32, 24).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.vertical_centered(|ui| {
                        ui.spacing_mut().item_spacing.y = 16.0;
                        ui.label(RichText::new("✅").size(64.0));
                        wrapped(ui, RichText::new("Base Installation Complete!").size(32.0).strong());
                        let color = faded(ui, 0.8);
                        wrapped(
                            ui,
                            RichText::new("Your base KOTOR mods have been successfully installed")
                                .size(16.0)
                                .color(color),
                        );
                    });
                });
                ui.add_space(24.0);
                // Row 1: statistics, `*,16,*,16,*,16,*`. Narrow windows wrap the
                // same cards to two or one per row instead of clipping values.
                let needed = stats
                    .iter()
                    .flat_map(|(label, value)| {
                        [
                            egui::WidgetText::from(RichText::new(*label).size(12.0)),
                            egui::WidgetText::from(RichText::new(value).size(36.0).strong()),
                        ]
                    })
                    .map(|text| {
                        text.into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body)
                            .size()
                            .x
                    })
                    .fold(0.0_f32, f32::max)
                    + 48.0;
                let width = ui.available_width();
                let fits = |n: f32| (width - 16.0 * (n - 1.0)) / n >= needed;
                let stat = |index: usize, ui: &mut egui::Ui| {
                    let (label, value) = &stats[index];
                    card(12, 24, 20).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.vertical_centered(|ui| {
                            ui.spacing_mut().item_spacing.y = 8.0;
                            let color = faded(ui, 0.6);
                            ui.label(RichText::new(*label).size(12.0).color(color));
                            let response = ui.label(RichText::new(value).size(36.0).strong());
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Label,
                                    true,
                                    format!("{label}: {value}"),
                                )
                            });
                        });
                    });
                };
                if fits(4.0) {
                    columns::<4>(ui, 16.0, egui::Align::Min, |i, ui| stat(i, ui));
                } else if fits(2.0) {
                    columns::<2>(ui, 16.0, egui::Align::Min, |i, ui| stat(i, ui));
                    ui.add_space(16.0);
                    columns::<2>(ui, 16.0, egui::Align::Min, |i, ui| stat(i + 2, ui));
                } else {
                    for index in 0..stats.len() {
                        if index > 0 {
                            ui.add_space(16.0);
                        }
                        stat(index, ui);
                    }
                }
                if let Some(text) = &unavailable {
                    ui.add_space(8.0);
                    let color = faded(ui, 0.7);
                    wrapped(ui, RichText::new(text).size(12.0).color(color));
                }
                ui.add_space(24.0);
                // Row 2: What's Next and Important Notes, `*,16,*`.
                columns::<2>(ui, 16.0, egui::Align::Min, |index, ui| {
                    card(12, 24, 20).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 16.0;
                        if index == 0 {
                            wrapped(ui, RichText::new("📋 What's Next?").size(20.0).strong());
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 12.0;
                                for (title, body) in [
                                    (
                                        "1. Widescreen Support",
                                        "If you're playing on a widescreen monitor, continue to install widescreen-specific mods",
                                    ),
                                    (
                                        "2. Test Your Game",
                                        "Launch KOTOR and verify everything is working correctly before starting a new playthrough",
                                    ),
                                    ("3. Recovery Snapshot", RECOVERY_TEXT),
                                ] {
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 6.0;
                                        wrapped(ui, RichText::new(title).size(14.0).strong());
                                        let color = faded(ui, 0.8);
                                        wrapped(ui, RichText::new(body).size(13.0).color(color));
                                    });
                                }
                            });
                        } else {
                            wrapped(ui, RichText::new("⚠️ Important Notes").size(20.0).strong());
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 12.0;
                                let color = faded(ui, 0.8);
                                for note in [
                                    "• Start a NEW GAME - Don't load old save files",
                                    "• Check the log files if you encounter any issues",
                                    "• Join our community for support and discussion",
                                    "• Remember to credit mod authors if you share content",
                                ] {
                                    wrapped(ui, RichText::new(note).size(13.0).color(color));
                                }
                            });
                        }
                    });
                });
                ui.add_space(16.0);
                // Quick Actions span both columns, 2x2.
                card(12, 24, 20).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 12.0;
                    wrapped(ui, RichText::new("🚀 Quick Actions").size(18.0).strong());
                    let buttons = [
                        ("📄 Copy Log", "Copy this install's step log", Action::CopyLog),
                        ("🔄 Recovery", "What recovery is available now", Action::Recovery),
                        ("📁 Open Game Folder", "Open the game folder this install used", Action::OpenGame),
                        ("📦 Open Mod Folder", "Open the mod workspace this install used", Action::OpenMods),
                    ];
                    let mut buttons = buttons.into_iter();
                    // Rows 8 apart, cells 8 apart, like the C# 2x2 margins.
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 8.0;
                        for _ in 0..2 {
                            let pair: Vec<_> = buttons.by_ref().take(2).collect();
                            let mut picked = None;
                            columns::<2>(ui, 8.0, egui::Align::Min, |index, ui| {
                                ui.spacing_mut().button_padding = egui::vec2(16.0, 12.0);
                                let (text, hover, _) = &pair[index];
                                let button = egui::Button::new(*text)
                                    .min_size(egui::vec2(ui.available_width(), 0.0));
                                if ui.add(button).on_hover_text(*hover).clicked() {
                                    picked = Some(index);
                                }
                            });
                            if let Some(index) = picked {
                                action = pair.into_iter().nth(index).map(|(_, _, a)| a);
                            }
                        }
                    });
                    if let Some(text) = &self.completion.feedback {
                        let color = faded(ui, 0.8);
                        wrapped(ui, RichText::new(text).size(13.0).color(color));
                    }
                });
                ui.add_space(24.0);
                // Row 3: footer. Finish is not shown on this page, so the C#
                // "or 'Finish' to exit" clause is dropped.
                card(8, 16, 12).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.vertical_centered(|ui| {
                        let color = faded(ui, 0.7);
                        wrapped(
                            ui,
                            RichText::new(
                                "Click 'Next' to continue with widescreen-specific mods (if applicable)",
                            )
                            .size(13.0)
                            .color(color),
                        );
                    });
                });
            });
        if let Some(action) = action {
            self.completion_action(action, ctx);
        }
        self.recovery_modal(ctx);
    }

    fn completion_action(&mut self, action: Action, ctx: &egui::Context) {
        let roots = self.completion.base.as_ref().and_then(|r| r.roots.clone());
        let feedback = match (action, roots) {
            (Action::CopyLog, _) => {
                let lines = self
                    .completion
                    .base
                    .as_ref()
                    .map_or(&[][..], |r| &r.log[..]);
                if lines.is_empty() {
                    "This install has no step log to copy.".to_owned()
                } else {
                    ctx.copy_text(lines.join("\n"));
                    format!("Copied {} log lines from this install.", lines.len())
                }
            }
            (Action::Recovery, _) => {
                self.completion.recovery_open = true;
                return;
            }
            (Action::OpenGame, Some(roots)) => open_dir(ctx, "game folder", &roots.game),
            (Action::OpenMods, Some(roots)) => open_dir(ctx, "mod folder", &roots.mods),
            (Action::OpenGame | Action::OpenMods, None) => {
                "The folders for this install were not recorded.".to_owned()
            }
        };
        self.logs.push(feedback.clone());
        self.completion.feedback = Some(feedback);
    }

    fn recovery_modal(&mut self, ctx: &egui::Context) {
        if !self.completion.recovery_open {
            return;
        }
        let mut open_folder = false;
        let mut close = false;
        let response =
            egui::Modal::new(egui::Id::new("completion_recovery_modal")).show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.heading("Recovery");
                ui.add(egui::Label::new(RECOVERY_TEXT).wrap());
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    open_folder = ui.button("Open recovery folder").clicked();
                    close = ui.button("Close").clicked();
                });
            });
        if open_folder {
            let feedback = match self.completion.base.as_ref().and_then(|r| r.roots.as_ref()) {
                Some(roots) => open_dir(ctx, "recovery folder", &recovery::state_dir(roots)),
                None => "The folders for this install were not recorded.".to_owned(),
            };
            self.logs.push(feedback.clone());
            self.completion.feedback = Some(feedback);
        }
        if close || response.should_close() {
            self.completion.recovery_open = false;
        }
    }

    pub(super) fn widescreen_notice_content(&mut self, ui: &mut egui::Ui) {
        let content = self
            .guide
            .as_ref()
            .and_then(|g| metadata_text(g, "widescreenWarningContent", "WidescreenWarningContent"))
            .unwrap_or_default()
            .to_owned();
        egui::Frame::default()
            .inner_margin(egui::Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label("🖥️ Widescreen Mods");
                ui.add_space(12.0);
                // Raw text, as in C#: Markdown is not rendered here.
                let height = visible_below(ui, 12.0 + 24.0 + 12.0, 100.0);
                bordered(ui).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(100.0 - 24.0);
                    egui::ScrollArea::vertical()
                        .id_salt("widescreen_notice_text")
                        .max_height(height - 24.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(content).wrap());
                        });
                });
            });
    }

    pub(super) fn widescreen_complete_content(&mut self, ui: &mut egui::Ui) {
        let body = if self.completion.widescreen_skipped() {
            "No widescreen mods were selected, so nothing was installed."
        } else {
            "All widescreen mods have been successfully installed."
        };
        egui::Frame::default()
            .inner_margin(egui::Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.spacing_mut().item_spacing.y = 12.0;
                    wrapped(ui, RichText::new("✅ Widescreen Installation Complete!"));
                    wrapped(ui, RichText::new(body));
                });
            });
    }

    pub(super) fn finished_content(&mut self, ui: &mut egui::Ui) {
        let body = if self.completion.widescreen_skipped() {
            "Base mods were installed. No widescreen mods were selected."
        } else {
            "All mods have been successfully installed."
        };
        let epilogue = self
            .guide
            .as_ref()
            .and_then(|g| metadata_text(g, "epilogueContent", "EpilogueContent"))
            .filter(|text| !text.trim().is_empty())
            .map(str::to_owned);
        egui::Frame::default()
            .inner_margin(egui::Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.spacing_mut().item_spacing.y = 14.0;
                    ui.label("🎉 Installation Complete!");
                    wrapped(ui, RichText::new(body));
                    capped(ui, 600.0, card(8, 16, 14), |ui| {
                        ui.spacing_mut().item_spacing.y = 8.0;
                        ui.vertical_centered(|ui| ui.label("🎮 Next Steps:"));
                        for step in [
                            "• Launch the game and enjoy your modded experience!",
                            "• This run's snapshot was released. Restore is offered only when an install stops partway.",
                            "• Copy the activity log for installation details.",
                        ] {
                            wrapped(ui, RichText::new(step));
                        }
                    });
                    capped(ui, 500.0, card(8, 14, 12), |ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        ui.vertical_centered(|ui| {
                            ui.spacing_mut().item_spacing.y = 6.0;
                            wrapped(ui, RichText::new("💝 Support the Project"));
                            wrapped(
                                ui,
                                RichText::new(
                                    "If you found ModSync helpful, consider supporting the project!",
                                ),
                            );
                            let color = faded(ui, 0.8);
                            wrapped(
                                ui,
                                RichText::new("Visit our sponsor page to learn more.").color(color),
                            );
                        });
                    });
                    // Added content: C# parses the epilogue but never shows it.
                    if let Some(text) = epilogue {
                        let width = ui.available_width().min(600.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(width, 0.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_width(width);
                                ui.label(RichText::new("📝 Guide notes").strong());
                                ui.add_space(6.0);
                                bordered(ui).show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    egui::ScrollArea::vertical()
                                        .id_salt("finished_epilogue_text")
                                        .max_height(240.0)
                                        .auto_shrink([false, true])
                                        .show(ui, |ui| {
                                            ui.add(egui::Label::new(text).wrap());
                                        });
                                });
                            },
                        );
                    }
                    let color = faded(ui, 0.7);
                    wrapped(
                        ui,
                        RichText::new("Click 'Finish' to close the wizard.").color(color),
                    );
                });
            });
    }
}

/// Ask the desktop to open a folder. The request is fire-and-forget, so success
/// is reported as a request; a missing folder is reported on the page.
fn open_dir(ctx: &egui::Context, label: &str, path: &Path) -> String {
    match std::fs::canonicalize(path) {
        Ok(path) if path.is_dir() => {
            ctx.open_url(egui::OpenUrl::new_tab(format!("file://{}", path.display())));
            format!("Asked the system to open the {label}: {}", path.display())
        }
        Ok(path) => format!(
            "Could not open the {label}: {} is not a folder.",
            path.display()
        ),
        Err(error) => format!("Could not open the {label} {}: {error}", path.display()),
    }
}
