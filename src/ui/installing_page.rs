//! Native Installing page. Every progress fact comes from the shared engine
//! observer; this module only keeps run-bound presentation state.
use super::App;
use eframe::egui::{self, RichText};
use modsync::install_progress::{Phase, Progress};
use modsync::paths::Roots;
use modsync::{Component, Guide};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

const LOG_LIMIT: usize = 2000;

#[derive(Default)]
enum Outcome {
    #[default]
    Idle,
    Running,
    /// The engine returned Ok after its Completed commit point.
    Succeeded,
    /// The engine committed, but the result was not applied to the visible project.
    NotApplied(String),
    Failed(String),
    /// Route-level skip of an empty widescreen phase; no engine run happened.
    Skipped,
}

#[derive(Default)]
pub(super) struct State {
    run: u64,
    stamp: String,
    outcome: Outcome,
    started: Option<Instant>,
    ended: Option<Duration>,
    progress: Option<Progress>,
    total: usize,
    completed: usize,
    /// Elapsed time at the latest ComponentCompleted fact.
    boundary: Duration,
    committed: bool,
    /// The captured roots held a pending snapshot at the terminal, so writes
    /// may have happened. False means a startup/preflight failure.
    wrote: bool,
    /// A restore of exactly `roots` succeeded after this failure.
    restored: bool,
    /// Roots and path text this run was dispatched with. Restore and Retry
    /// are bound to these, never to the editable paths.
    roots: Option<Roots>,
    folders: (String, String),
    directions: HashMap<Vec<String>, String>,
    log: VecDeque<String>,
}

impl State {
    /// Start a new run bound to the exact input stamp. Prior progress, metrics,
    /// terminal text and log are cleared; messages from older runs are ignored.
    pub fn begin(&mut self, stamp: &str) -> u64 {
        *self = Self {
            run: self.run.wrapping_add(1),
            stamp: stamp.to_owned(),
            outcome: Outcome::Running,
            started: Some(Instant::now()),
            ..Self::default()
        };
        self.run
    }

    /// Bind the folder text this run starts with, before anything can fail.
    pub fn bind_folders(&mut self, mods: &str, game: &str) {
        self.folders = (mods.to_owned(), game.to_owned());
    }

    /// Bind the roots resolved from those folders before dispatch.
    pub fn bind_roots(&mut self, roots: &Roots) {
        self.roots = Some(roots.clone());
    }

    /// The engine committed, but the result was not applied to this project.
    pub fn not_applied(&self) -> bool {
        matches!(self.outcome, Outcome::NotApplied(_))
    }

    /// Roots of a failed run whose snapshot is still pending.
    pub fn restore_target(&self) -> Option<&Roots> {
        self.roots
            .as_ref()
            .filter(|_| self.failed() && self.pending())
    }

    /// Record a verified restore. Only a restore of this run's own roots counts.
    pub fn record_restore(&mut self, roots: &Roots) -> bool {
        let same = self
            .roots
            .as_ref()
            .is_some_and(|own| own.game == roots.game && own.mods == roots.mods);
        if same && self.failed() {
            self.restored = true;
        }
        same
    }

    /// True when this run failed and its captured snapshot is not restored yet.
    pub fn pending(&self) -> bool {
        self.failed() && self.wrote && !self.restored
    }

    pub fn same_folders(&self, mods: &str, game: &str) -> bool {
        self.folders.0 == mods && self.folders.1 == game
    }

    /// Bind directions to the prepared guide actually handed to the engine.
    pub fn prepared(&mut self, guide: &Guide) {
        fn collect(map: &mut HashMap<Vec<String>, String>, path: &mut Vec<String>, c: &Component) {
            path.push(c.guid.clone());
            map.insert(path.clone(), c.directions.clone());
            for option in &c.options {
                collect(map, path, option);
            }
            path.pop();
        }
        let mut path = Vec::new();
        for component in &guide.components {
            collect(&mut self.directions, &mut path, component);
        }
    }

    pub fn skip(&mut self) {
        *self = Self {
            run: self.run.wrapping_add(1),
            outcome: Outcome::Skipped,
            ..Self::default()
        };
    }

    /// Invalidate any in-flight run without starting a new one.
    pub fn reset(&mut self) {
        *self = Self {
            run: self.run.wrapping_add(1),
            ..Self::default()
        };
    }

    pub fn active(&self) -> bool {
        matches!(self.outcome, Outcome::Running)
    }

    pub fn failed(&self) -> bool {
        matches!(self.outcome, Outcome::Failed(_))
    }

    pub fn accepts(&self, run: u64, stamp: &str) -> bool {
        self.active() && self.run == run && self.stamp == stamp
    }

    fn elapsed(&self) -> Duration {
        self.ended
            .or_else(|| self.started.map(|s| s.elapsed()))
            .unwrap_or_default()
    }

    pub fn progress(&mut self, progress: Progress) {
        let elapsed = self.elapsed();
        self.total = progress.component_total;
        match progress.phase {
            Phase::ComponentCompleted => {
                self.completed = progress.component_index;
                self.boundary = elapsed;
            }
            Phase::Completed => {
                self.committed = true;
                self.completed = self.total;
                self.boundary = elapsed;
            }
            _ => {}
        }
        self.push_log(format!("[{}] {}", clock(elapsed), step_text(&progress)));
        self.progress = Some(progress);
    }

    /// Record the worker terminal. `engine_ok` is the engine's own result and is
    /// authoritative; a later cancellation request cannot change it.
    pub fn finish(
        &mut self,
        run: u64,
        stamp: &str,
        engine_ok: bool,
        result: &anyhow::Result<()>,
        pending: bool,
    ) {
        if self.run != run || !self.active() {
            return;
        }
        self.ended = Some(self.elapsed());
        self.wrote = pending;
        let stale = self.stamp != stamp;
        self.outcome = match (engine_ok, result) {
            (true, Ok(())) if !stale => Outcome::Succeeded,
            (true, Ok(())) => Outcome::NotApplied(
                "The install finished for the captured folders, but the guide or folders changed. Use Restart wizard below to start again with the current guide.".into(),
            ),
            (true, Err(error)) => Outcome::NotApplied(format!("{error:#}")),
            (false, Err(error)) => Outcome::Failed(format!("{error:#}")),
            (false, Ok(())) => Outcome::Failed("The install did not report a result.".into()),
        };
        if engine_ok {
            self.committed = true;
            self.completed = self.total;
        }
        if let Some(progress) = &mut self.progress {
            progress.snapshot_pending = pending;
        }
        if let Some(line) = match &self.outcome {
            Outcome::Failed(error) => Some(format!("Stopped: {error}")),
            Outcome::NotApplied(error) => Some(format!("Not applied: {error}")),
            _ => None,
        } {
            self.push_log(line);
        }
    }

    /// Keep the newest lines; the oldest drop first.
    fn push_log(&mut self, line: String) {
        if self.log.len() == LOG_LIMIT {
            self.log.pop_front();
        }
        self.log.push_back(line);
    }

    /// Stop-dialog text from the last shared snapshot fact of this run.
    pub fn stop_warning(&self) -> &'static str {
        match &self.progress {
            Some(p) if p.phase == Phase::Completed => {
                "The install has finished and its recovery snapshot is finalized. Nothing is left to stop."
            }
            Some(p) if p.snapshot_pending => {
                "A recovery snapshot is active. The install stops at the next safe point. Restore the snapshot before you retry."
            }
            Some(p) if p.phase == Phase::CreatingSnapshot => {
                "The recovery snapshot is being created. The install stops at the next safe point. Check Activity for recovery steps before you retry."
            }
            _ => {
                "No game or mod files have been changed yet. The install stops at the next safe point."
            }
        }
    }

    fn fraction(&self) -> f32 {
        if self.committed {
            1.0
        } else if self.total == 0 {
            0.0
        } else {
            // 100% belongs to the Completed commit point only.
            (self.completed as f32 / self.total as f32).min(0.99)
        }
    }

    fn percent(&self) -> String {
        match self.outcome {
            Outcome::Skipped => "0%".into(),
            _ if self.committed => "100%".into(),
            _ if self.total == 0 => "0%".into(),
            // .NET Math.Round rounds half to even; 100% waits for Completed.
            _ => format!(
                "{}%",
                ((self.completed * 100) as f64 / self.total as f64)
                    .round_ties_even()
                    .min(99.0)
            ),
        }
    }

    fn current_title(&self) -> String {
        match &self.outcome {
            Outcome::Idle => "Preparing installation…".into(),
            Outcome::Skipped => "Widescreen phase skipped".into(),
            Outcome::Succeeded => "✅ Installation complete".into(),
            Outcome::NotApplied(_) => "Installed, but not applied here".into(),
            Outcome::Failed(_) if self.wrote => "Install stopped".into(),
            Outcome::Failed(_) => "Install stopped before changing files".into(),
            Outcome::Running => match &self.progress {
                None => "Preparing installation…".into(),
                Some(p) => match p.phase {
                    Phase::Preparing => "Preparing installation…".into(),
                    Phase::CreatingSnapshot => "Creating recovery snapshot…".into(),
                    Phase::InstallingComponent | Phase::RunningInstruction => format!(
                        "Installing: {}",
                        p.top_level_component_name.as_deref().unwrap_or_default()
                    ),
                    Phase::ComponentCompleted => format!(
                        "Installed: {}",
                        p.top_level_component_name.as_deref().unwrap_or_default()
                    ),
                    Phase::RecordingCompletion => "Recording completion…".into(),
                    Phase::Completed => "✅ Installation complete".into(),
                },
            },
        }
    }

    fn directions(&self) -> String {
        match &self.outcome {
            Outcome::Skipped => {
                return "No widescreen mods were selected, so nothing was installed.".into();
            }
            Outcome::Failed(error) | Outcome::NotApplied(error) => return error.clone(),
            _ => {}
        }
        let Some(p) = &self.progress else {
            return String::new();
        };
        if p.component_index == 0 {
            return String::new();
        }
        // Identity is the GUID path from the prepared guide, never a display name.
        let path: Vec<String> = p.owner_path.iter().map(|f| f.guid.clone()).collect();
        let owner = self.directions.get(&path).filter(|d| !d.trim().is_empty());
        owner
            .or_else(|| {
                path.first()
                    .and_then(|top| self.directions.get(std::slice::from_ref(top)))
            })
            .map(|d| d.trim().to_owned())
            .unwrap_or_default()
    }

    fn operation(&self) -> String {
        match &self.outcome {
            Outcome::Idle | Outcome::Running => {}
            Outcome::Succeeded => return "Complete".into(),
            Outcome::NotApplied(_) => return "Not applied".into(),
            Outcome::Failed(_) => return "Stopped".into(),
            Outcome::Skipped => return "Skipped".into(),
        }
        match &self.progress {
            None => "Starting…".into(),
            Some(p) if p.phase == Phase::InstallingComponent => "Starting".into(),
            Some(p) => step_text(p),
        }
    }

    fn animating(&self) -> bool {
        self.active()
            && !self
                .progress
                .as_ref()
                .is_some_and(|p| p.phase == Phase::Completed)
    }

    fn snapshot_line(&self) -> &'static str {
        match &self.outcome {
            Outcome::Skipped => "No recovery snapshot needed",
            Outcome::Succeeded | Outcome::NotApplied(_) => "Recovery finalized",
            // Both come from this run's captured roots, not the editable paths.
            Outcome::Failed(_) if self.pending() => {
                "Recovery snapshot active. Restore it before you retry."
            }
            Outcome::Failed(_) if self.restored => "Recovery snapshot restored",
            Outcome::Failed(_) => "No recovery snapshot pending",
            Outcome::Idle | Outcome::Running => match &self.progress {
                Some(p) if p.phase == Phase::Completed => "Recovery finalized",
                Some(p) if p.snapshot_pending => "Recovery snapshot active",
                Some(p) if p.phase == Phase::CreatingSnapshot => "Preparing recovery snapshot…",
                _ => "Recovery snapshot not created yet",
            },
        }
    }

    fn snapshot_value(&self) -> &'static str {
        match &self.outcome {
            Outcome::Skipped => "None",
            Outcome::Succeeded | Outcome::NotApplied(_) => "Finalized",
            Outcome::Failed(_) if self.pending() => "Active",
            Outcome::Failed(_) if self.restored => "Restored",
            Outcome::Failed(_) => "None",
            Outcome::Idle | Outcome::Running => match &self.progress {
                Some(p) if p.phase == Phase::Completed => "Finalized",
                Some(p) if p.snapshot_pending => "Active",
                Some(p) if p.phase == Phase::CreatingSnapshot => "Creating…",
                _ => "Not yet",
            },
        }
    }

    fn eta(&self) -> String {
        if self.committed {
            return clock(Duration::ZERO);
        }
        if self.completed == 0 || !self.active() {
            return "--:--:--".into();
        }
        let average = self.boundary.as_secs_f64() / self.completed as f64;
        clock(Duration::from_secs_f64(
            average * self.total.saturating_sub(self.completed) as f64,
        ))
    }

    fn rate(&self) -> String {
        if self.completed == 0 {
            return "0.0 mods/min".into();
        }
        let minutes = (self.boundary.as_secs_f64() / 60.0).max(0.001);
        format!("{:.1} mods/min", self.completed as f64 / minutes)
    }
}

fn step_text(p: &Progress) -> String {
    match p.phase {
        Phase::RunningInstruction => {
            let owner = if p.owner_path.len() > 1 {
                format!("{} · ", p.owner_name.as_deref().unwrap_or_default())
            } else {
                String::new()
            };
            format!(
                // Emitted after the step succeeds, so it names finished work.
                "{owner}Finished step {} of {}: {}",
                p.instruction_index.unwrap_or_default(),
                p.instruction_total.unwrap_or_default(),
                p.message
            )
        }
        _ => p.message.clone(),
    }
}

fn clock(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

/// C# cards set padding and radius only: no fill, stroke or brush.
fn card(radius: u8, x: i8, y: i8) -> egui::Frame {
    egui::Frame::default()
        .corner_radius(radius)
        .inner_margin(egui::Margin::symmetric(x, y))
}

fn faded(ui: &egui::Ui, opacity: f32) -> egui::Color32 {
    ui.visuals().text_color().gamma_multiply(opacity)
}

fn bar(ui: &mut egui::Ui, height: f32, radius: f32, fraction: Option<f32>, hover: &str) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(1.0), height),
        egui::Sense::hover(),
    );
    let visuals = ui.visuals();
    let fill = visuals.selection.bg_fill;
    // Inactive widget fill stays visible on every theme's panel fill.
    ui.painter()
        .rect_filled(rect, radius, visuals.widgets.inactive.bg_fill);
    match fraction {
        Some(fraction) => {
            let mut filled = rect;
            filled.set_width(rect.width() * fraction.clamp(0.0, 1.0));
            if filled.width() > 0.0 {
                ui.painter().rect_filled(filled, radius, fill);
            }
        }
        None => {
            // Indeterminate: a sweeping segment, not a work estimate.
            let t = (ui.input(|i| i.time) * 0.6).fract() as f32;
            let width = rect.width() * 0.3;
            let left = rect.left() - width + t * (rect.width() + width);
            let segment = egui::Rect::from_min_max(
                egui::pos2(left.max(rect.left()), rect.top()),
                egui::pos2((left + width).min(rect.right()), rect.bottom()),
            );
            if segment.width() > 0.0 {
                ui.painter().rect_filled(segment, radius, fill);
            }
            // Share the page's 100 ms repaint budget instead of every frame.
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
    }
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::WidgetType::ProgressIndicator, true, hover);
        info.value = fraction.map(|f| f64::from(f.clamp(0.0, 1.0)) * 100.0);
        info
    });
    response.on_hover_text(hover);
}

/// Equal columns with a fixed gap, like a XAML `*,gap,*` grid row.
fn columns<const N: usize>(
    ui: &mut egui::Ui,
    gap: f32,
    align: egui::Align,
    mut cell: impl FnMut(usize, &mut egui::Ui),
) {
    let width = ((ui.available_width() - gap * (N as f32 - 1.0)) / N as f32).max(1.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for index in 0..N {
            ui.allocate_ui_with_layout(
                egui::vec2(width, 0.0),
                egui::Layout::top_down(align),
                |ui| {
                    ui.set_width(width);
                    cell(index, ui);
                },
            );
        }
    });
}

fn stat_label(ui: &mut egui::Ui, text: &str) {
    let color = faded(ui, 0.6);
    ui.label(RichText::new(text).size(11.0).color(color));
}

impl App {
    pub(super) fn installing_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let same_folders = self.installing_page.same_folders(&self.mods, &self.game);
        let state = &self.installing_page;
        if state.animating() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        egui::Frame::default()
            .inner_margin(egui::Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                // Row 0: progress card.
                card(12, 24, 20).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 16.0;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(state.percent()).size(28.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let color = faded(ui, 0.8);
                            let count = if matches!(state.outcome, Outcome::Skipped) {
                                "No mods in this phase".to_owned()
                            } else {
                                format!("{}/{} mods installed", state.completed, state.total)
                            };
                            ui.label(RichText::new(count).size(16.0).color(color));
                        });
                    });
                    bar(ui, 12.0, 6.0, Some(state.fraction()), "Installed mods out of this phase's total");
                    let operation = state.operation();
                    let operation_color = faded(ui, 0.6);
                    let operation_galley = ui.painter().layout(
                        operation.clone(),
                        egui::FontId::proportional(13.0),
                        operation_color,
                        (ui.available_width() * 0.4).max(1.0),
                    );
                    let right = operation_galley.size().x;
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        let left = (ui.available_width() - right - 12.0).max(1.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(left, 0.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_width(left);
                                ui.spacing_mut().item_spacing.y = 6.0;
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(state.current_title()).size(16.0).strong(),
                                    )
                                    .wrap(),
                                );
                                let directions = state.directions();
                                if !directions.is_empty() {
                                    let color = faded(ui, 0.7);
                                    egui::ScrollArea::vertical()
                                        .id_salt(("install_directions", state.run))
                                        .max_height(60.0)
                                        .auto_shrink([false, true])
                                        .show(ui, |ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(directions)
                                                        .size(13.0)
                                                        .color(color),
                                                )
                                                .selectable(true)
                                                .wrap(),
                                            );
                                        });
                                }
                            },
                        );
                        ui.add(egui::Label::new(
                            RichText::new(operation).size(13.0).color(operation_color),
                        ).wrap());
                    });
                    bar(
                        ui,
                        6.0,
                        3.0,
                        if state.animating() { None } else { Some(0.0) },
                        "Current mod: no per-mod percentage is available",
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let color = faded(ui, 0.7);
                        ui.label(RichText::new("💾").size(12.0).color(color));
                        ui.label(
                            RichText::new(state.snapshot_line())
                                .size(12.0)
                                .color(color),
                        );
                    });
                });
                if matches!(state.outcome, Outcome::Failed(_) | Outcome::NotApplied(_)) {
                    ui.add_space(16.0);
                    card(10, 20, 16).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 8.0;
                        ui.label(RichText::new("What to do next").size(14.0).strong());
                        // Text only: the actions live once, in the footer.
                        let guidance = match &state.outcome {
                            Outcome::NotApplied(_) => {
                                "The engine finished, but this project did not record it. Use Restart wizard below, then check Activity before you install again."
                            }
                            _ if state.pending() => {
                                "Files may have changed. Use Restore snapshot below, then retry this phase."
                            }
                            _ if state.restored => "The recovery snapshot was restored. Use Retry phase below.",
                            _ => "The install stopped before any game or mod files changed. Fix the error above, then use Retry phase below.",
                        };
                        ui.label(RichText::new(guidance).size(13.0));
                        if matches!(state.outcome, Outcome::Failed(_)) {
                            if let Some(roots) = &state.roots {
                                let color = faded(ui, 0.7);
                                ui.label(
                                    RichText::new(format!(
                                        "Install folders: game {}, mods {}",
                                        roots.game.display(),
                                        roots.mods.display()
                                    ))
                                    .size(12.0)
                                    .color(color),
                                );
                            }
                            if !same_folders {
                                ui.label(RichText::new("The game or mod folder changed after this install. Restore uses the install's own folders. Set the folders back to retry.").size(13.0).strong());
                            }
                        }
                    });
                }
                ui.add_space(16.0);
                // Row 1: statistics and showcase, `*,16,*`; explanation spans both.
                columns::<2>(ui, 16.0, egui::Align::Min, |index, ui| {
                    if index == 0 {
                        card(10, 20, 16).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.spacing_mut().item_spacing.y = 0.0;
                            let elapsed = clock(state.elapsed());
                            let values = [
                                ("ELAPSED", elapsed),
                                ("REMAINING", state.eta()),
                                ("RATE", state.rate()),
                            ];
                            columns::<3>(ui, 0.0, egui::Align::Center, |i, ui| {
                                stat_label(ui, values[i].0);
                                ui.label(RichText::new(&values[i].1).size(18.0).strong());
                            });
                            ui.add_space(12.0);
                            ui.vertical_centered(|ui| {
                                stat_label(ui, "ISSUES");
                                let color = faded(ui, 0.7);
                                ui.label(RichText::new("Not counted").size(16.0).strong())
                                    .on_hover_text("Program output is not classified. Read warnings and errors in Activity.");
                                ui.label(
                                    RichText::new("See Activity for program output")
                                        .size(11.0)
                                        .color(color),
                                );
                            });
                            ui.add_space(12.0);
                            ui.vertical_centered(|ui| {
                                stat_label(ui, "RECOVERY SNAPSHOT");
                                ui.label(
                                    RichText::new(state.snapshot_value())
                                        .size(16.0)
                                        .strong(),
                                );
                            });
                        });
                    } else {
                        card(10, 20, 16).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.vertical_centered(|ui| {
                                ui.spacing_mut().item_spacing.y = 12.0;
                                ui.label(RichText::new("🎮").size(48.0));
                                let skipped = matches!(state.outcome, Outcome::Skipped);
                                let (title, body) = if skipped {
                                    ("Nothing to install", "No widescreen mods were selected")
                                } else {
                                    ("Installing Your Mods", "Sit back and relax while we handle everything")
                                };
                                ui.label(RichText::new(title).size(16.0).strong());
                                let color = faded(ui, 0.7);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(body)
                                            .size(13.0)
                                            .color(color),
                                    )
                                    .wrap(),
                                );
                            });
                        });
                    }
                });
                ui.add_space(16.0);
                card(10, 20, 16).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 8.0;
                    ui.label(RichText::new("⚙ What's Happening?").size(14.0).strong());
                    let items = [
                        "✔ Extracting archives",
                        "✔ Running TSLPatcher/HoloPatcher",
                        "✔ Moving and copying files",
                        "✔ Following guide order",
                        "✔ Creating one recovery snapshot",
                        "✔ Checking steps before writing",
                    ];
                    columns::<2>(ui, 0.0, egui::Align::Min, |column, ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        let color = faded(ui, 0.8);
                        for item in items.iter().skip(column).step_by(2) {
                            ui.label(RichText::new(*item).size(12.0).color(color));
                        }
                    });
                });
                if !state.log.is_empty() {
                    ui.add_space(16.0);
                    egui::CollapsingHeader::new(format!("Install steps ({})", state.log.len()))
                        .id_salt("install_steps")
                        .default_open(false)
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt(("install_steps_log", state.run))
                                .max_height(160.0)
                                .stick_to_bottom(true)
                                .show(ui, |ui| {
                                    for line in &state.log {
                                        ui.add(
                                            egui::Label::new(RichText::new(line).monospace().size(11.0))
                                                .wrap(),
                                        );
                                    }
                                });
                        });
                }
                // Row 2: footer warning while writes or recovery are pending.
                let footer = if state.active() {
                    Some("⚠ Do not close ModSync while the install runs")
                } else if state.pending() {
                    Some("⚠ Restore the recovery snapshot before you install again")
                } else {
                    None
                };
                if let Some(text) = footer {
                    ui.add_space(16.0);
                    card(8, 16, 12).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.vertical_centered(|ui| {
                            let color = faded(ui, 0.7);
                            ui.label(RichText::new(text).size(12.0).color(color));
                        });
                    });
                }
            });
    }
}
