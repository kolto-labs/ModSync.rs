use anyhow::Result;
use eframe::egui;
use modsync::{Component, Engine, Guide, Settings, download, recovery};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};

enum Message {
    Log(String),
    Done(Result<()>),
}
#[derive(Clone, Copy, PartialEq)]
enum Job {
    Validate,
    Install,
    Restore,
    Download,
}

pub struct App {
    input: String,
    game: String,
    mods: String,
    patcher: String,
    scratch: String,
    expanded_budget: String,
    allow_processes: bool,
    guide: Option<Guide>,
    page: usize,
    selected: usize,
    search: String,
    logs: Vec<String>,
    receiver: Option<Receiver<Message>>,
    cancelled: Arc<AtomicBool>,
    job: Job,
    validated: bool,
    confirm_install: bool,
    download_url: String,
    download_name: String,
    download_hash: String,
}

impl App {
    fn new(input: Option<PathBuf>, game: Option<PathBuf>, mods: Option<PathBuf>) -> Self {
        let string = |p: Option<PathBuf>| {
            p.map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let mut app = Self {
            input: string(input),
            game: string(game),
            mods: string(mods),
            patcher: String::new(),
            scratch: String::new(),
            expanded_budget: String::new(),
            allow_processes: false,
            guide: None,
            page: 0,
            selected: 0,
            search: String::new(),
            logs: Vec::new(),
            receiver: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            job: Job::Validate,
            validated: false,
            confirm_install: false,
            download_url: String::new(),
            download_name: String::new(),
            download_hash: String::new(),
        };
        if !app.input.is_empty() {
            app.load();
        }
        app
    }

    fn load(&mut self) {
        self.validated = false;
        match Guide::load(Path::new(&self.input)) {
            Ok(g) => {
                self.logs
                    .push(format!("Loaded {} components", g.components.len()));
                self.guide = Some(g);
                self.selected = 0;
            }
            Err(e) => {
                self.guide = None;
                self.logs.push(format!("Load failed: {e:#}"));
            }
        }
    }

    fn start(&mut self, job: Job, ctx: &egui::Context) {
        let settings = Settings::new(Path::new(&self.mods), Path::new(&self.game));
        let mut settings = match settings {
            Ok(s) => s,
            Err(e) => {
                self.logs.push(format!("Check directories: {e:#}"));
                return;
            }
        };
        settings.allow_processes = self.allow_processes;
        settings.patcher = if self.patcher.is_empty() {
            None
        } else {
            Some(PathBuf::from(&self.patcher))
        };
        settings.archive.scratch_dir = if self.scratch.is_empty() {
            None
        } else {
            Some(PathBuf::from(&self.scratch))
        };
        settings.archive.max_expanded_bytes = if self.expanded_budget.trim().is_empty() {
            None
        } else {
            match self.expanded_budget.trim().parse() {
                Ok(value) => Some(value),
                Err(_) => {
                    self.logs
                        .push("Expanded-byte budget must be a nonnegative integer.".into());
                    return;
                }
            }
        };
        self.cancelled = settings.cancelled.clone();
        let guide = self.guide.clone();
        let url = self.download_url.clone();
        let filename = self.download_name.clone();
        let hash = self.download_hash.clone();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.job = job;
        self.validated = false;
        self.confirm_install = false;
        self.logs.push(
            match job {
                Job::Validate => "Validating…",
                Job::Install => "Installing…",
                Job::Restore => "Restoring snapshot…",
                Job::Download => "Downloading…",
            }
            .into(),
        );
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
                    if job == Job::Restore {
                        return recovery::restore(&settings.roots);
                    }
                    if job == Job::Download {
                        let name = modsync::paths::relative(&filename, false)?;
                        anyhow::ensure!(
                            name.components().count() == 1,
                            "Enter one archive filename"
                        );
                        return download::fetch_with_options(
                            &url,
                            &settings.roots.mods.join(name),
                            if hash.is_empty() { None } else { Some(&hash) },
                            &settings.cancelled,
                            &settings.archive,
                        );
                    }
                    let guide = guide.ok_or_else(|| anyhow::anyhow!("Load a guide first"))?;
                    let emit = |e: modsync::Event| {
                        let _ = tx.send(Message::Log(format!(
                            "{} / {}: {}",
                            e.component, e.step, e.message
                        )));
                        ctx.request_repaint();
                    };
                    let engine = Engine::new(settings)?;
                    if job == Job::Install {
                        engine.install(&guide, emit)
                    } else {
                        engine.validate(&guide, emit)
                    }
                }))
                .unwrap_or_else(|_| {
                    Err(anyhow::anyhow!(
                        "Worker stopped unexpectedly. Inspect recovery state before retrying."
                    ))
                });
            let _ = tx.send(Message::Done(result));
            ctx.request_repaint();
        });
    }

    fn path_row(ui: &mut egui::Ui, label: &str, value: &mut String, folder: bool) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(label);
            changed |= ui
                .add(egui::TextEdit::singleline(value).desired_width(520.0))
                .changed();
            if ui.button("Browse…").clicked() {
                let dialog = rfd::FileDialog::new();
                let path = if folder {
                    dialog.pick_folder()
                } else {
                    dialog.pick_file()
                };
                if let Some(p) = path {
                    *value = p.to_string_lossy().into_owned();
                    changed = true;
                }
            }
        });
        changed
    }

    fn options(ui: &mut egui::Ui, component: &mut Component) -> bool {
        let mut changed = false;
        for option in &mut component.options {
            changed |= ui.checkbox(&mut option.is_selected, &option.name).changed();
            if !option.description.is_empty() {
                ui.label(&option.description);
            }
            if option.is_selected {
                changed |= Self::options(ui, option);
            }
        }
        changed
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let messages: Vec<_> = self
            .receiver
            .as_ref()
            .map(|r| r.try_iter().collect())
            .unwrap_or_default();
        for message in messages {
            match message {
                Message::Log(line) => self.logs.push(line),
                Message::Done(result) => {
                    self.receiver = None;
                    self.validated = result.is_ok() && self.job == Job::Validate;
                    self.logs.push(match result { Ok(()) => match self.job { Job::Validate => "Validation passed. External program effects, if any, remain unverified.", Job::Install => "Installation complete.", Job::Restore => "Snapshot restored and file hashes verified.", Job::Download => "Archive downloaded and integrity checked." }.into(), Err(e) => format!("Stopped: {e:#}") });
                }
            }
        }
        if self.logs.len() > 5000 {
            self.logs.drain(..self.logs.len() - 5000);
        }
        let busy = self.receiver.is_some();
        if busy {
            ctx.request_repaint_after(Duration::from_millis(100));
            if ctx.input(|i| i.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.cancelled.store(true, Ordering::Relaxed);
            }
        }
        egui::TopBottomPanel::top("title").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("ModSync.rs");
                ui.label("Install mods in guide order");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::widgets::global_theme_preference_buttons(ui);
                });
            });
            ui.add_enabled_ui(!busy, |ui| {
                ui.horizontal(|ui| {
                    for (n, title) in ["1  Load guide", "2  Choose mods", "3  Validate and install"]
                        .iter()
                        .enumerate()
                    {
                        if ui.selectable_label(self.page == n, *title).clicked() {
                            self.page = n;
                        }
                    }
                });
            });
        });
        egui::TopBottomPanel::bottom("activity")
            .resizable(true)
            .default_height(175.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Activity");
                    if busy {
                        ui.spinner();
                        if ui.button("Stop").clicked() {
                            self.cancelled.store(true, Ordering::Relaxed);
                        }
                    }
                    if ui.button("Copy report").clicked() {
                        ctx.copy_text(self.logs.join("\n"));
                    }
                    if ui.button("Save report…").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .set_file_name("modsync-report.txt")
                            .save_file()
                        && let Err(e) = std::fs::write(path, self.logs.join("\n"))
                    {
                        self.logs.push(e.to_string());
                    }
                });
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &self.logs {
                            ui.monospace(line);
                        }
                    });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_enabled_ui(!busy, |ui| match self.page {
                0 => {
                    ui.heading("Load an instruction file");
                    ui.label("Read ModSync TOML, JSON, YAML, XML, or Markdown. Installation requires explicit instructions.");
                    if Self::path_row(ui, "Guide", &mut self.input, false) { self.validated = false; }
                    if ui.button("Load guide").clicked() { self.load(); }
                    ui.separator();
                    if Self::path_row(ui, "Game directory", &mut self.game, true) | Self::path_row(ui, "Mod directory", &mut self.mods, true) { self.validated = false; }
                    ui.label("Use separate folders. Put downloaded archives in the mod directory.");
                    if let Some(g) = &self.guide { ui.label(format!("{} components loaded", g.components.len())); }
                    if ui.add_enabled(self.guide.is_some(), egui::Button::new("Choose mods →")).clicked() { self.page = 1; }
                }
                1 => {
                    let Some(g) = &mut self.guide else { ui.label("Load a guide first."); return; };
                    ui.horizontal(|ui| {
                        if ui.button("Select all").clicked() { for c in &mut g.components { c.is_selected = true; } self.validated = false; }
                        if ui.button("Deselect all").clicked() { for c in &mut g.components { c.is_selected = false; } self.validated = false; }
                        ui.label(format!("{} / {} selected", g.components.iter().filter(|c| c.is_selected).count(), g.components.len()));
                        ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Filter name, tier, category"));
                    });
                    ui.separator();
                    ui.columns(2, |columns| {
                        egui::ScrollArea::vertical().id_salt("mods").show(&mut columns[0], |ui| {
                            for (index, c) in g.components.iter_mut().enumerate() {
                                if !format!("{} {} {}", c.name, c.tier, c.category.join(" ")).to_lowercase().contains(&self.search.to_lowercase()) { continue; }
                                ui.horizontal(|ui| {
                                    if ui.checkbox(&mut c.is_selected, "").on_hover_text("Select component").changed() { self.validated = false; }
                                    if ui.selectable_label(self.selected == index, &c.name).clicked() { self.selected = index; }
                                });
                            }
                        });
                        egui::ScrollArea::vertical().id_salt("details").show(&mut columns[1], |ui| {
                            if let Some(c) = g.components.get_mut(self.selected) {
                                ui.heading(&c.name);
                                ui.label(format!("{}  ·  {}", c.author, c.tier));
                                ui.label(&c.description);
                                if !c.directions.is_empty() { ui.separator(); ui.label(&c.directions); }
                                if Self::options(ui, c) { self.validated = false; }
                                ui.separator();
                                ui.strong("Download links");
                                for url in c.resource_registry.keys() { ui.hyperlink(url); }
                                for field in ["ModLink", "DownloadLink"] {
                                    if let Some(value) = c.metadata.get(field) {
                                        if let Some(url) = value.as_str() { ui.hyperlink(url); }
                                        if let Some(urls) = value.as_array() { for url in urls.iter().filter_map(|v| v.as_str()) { ui.hyperlink(url); } }
                                    }
                                }
                                ui.label(format!("{} install steps", c.instructions.len()));
                            }
                        });
                    });
                }
                _ => {
                    ui.heading("Validate before installing");
                    ui.label("Validation simulates file changes. Installation keeps a recovery copy of both folders.");
                    ui.label(format!("Game: {}", self.game));
                    ui.label(format!("Mods: {}", self.mods));
                    if ui.checkbox(&mut self.allow_processes, "Trust this guide to run external programs").changed() { self.validated = false; }
                    ui.collapsing("Archive storage", |ui| {
                        if Self::path_row(ui, "Scratch folder (optional)", &mut self.scratch, true) { self.validated = false; }
                        ui.label("Use an existing folder outside the game and mod folders. Blank uses system temp.");
                        ui.horizontal(|ui| {
                            ui.label("Maximum expanded bytes per archive (optional)");
                            if ui.text_edit_singleline(&mut self.expanded_budget).changed() { self.validated = false; }
                        });
                    });
                    if self.allow_processes && Self::path_row(ui, "OdyPatcher", &mut self.patcher, false) { self.validated = false; }
                    ui.horizontal(|ui| {
                        if ui.button("Run validation").clicked() { self.start(Job::Validate, ctx); }
                        if ui.add_enabled(self.validated, egui::Button::new("Install selected mods…")).clicked() { self.confirm_install = true; }
                        if ui.button("Restore failed install").clicked() { self.start(Job::Restore, ctx); }
                        if ui.button("Save selection…").clicked()
                            && let (Some(g), Some(path)) = (&self.guide, rfd::FileDialog::new().set_file_name("selection.toml").save_file())
                                && let Err(e) = g.save(&path) { self.logs.push(format!("Save failed: {e:#}")); }
                    });
                    ui.separator();
                    ui.collapsing("Download a direct archive link", |ui| {
                        ui.label("Use browser downloads for Nexus, DeadlyStream pages, and MEGA. Direct HTTPS ZIP/7z links work here.");
                        ui.add(egui::TextEdit::singleline(&mut self.download_url).hint_text("https://…/mod.zip").desired_width(f32::INFINITY));
                        ui.add(egui::TextEdit::singleline(&mut self.download_name).hint_text("Archive filename"));
                        ui.add(egui::TextEdit::singleline(&mut self.download_hash).hint_text("Expected SHA-256 (optional)"));
                        if ui.button("Download archive").clicked() { self.start(Job::Download, ctx); }
                    });
                }
            });
        });
        if self.confirm_install {
            egui::Window::new("Install selected mods?").collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(format!("Modify {}", self.game));
                ui.label("The snapshot needs space for both folders. The install stops on the first error.");
                ui.horizontal(|ui| {
                    if ui.button("Create snapshot and install").clicked() { self.start(Job::Install, ctx); }
                    if ui.button("Cancel").clicked() { self.confirm_install = false; }
                });
            });
        }
    }
}

pub fn launch(input: Option<PathBuf>, game: Option<PathBuf>, mods: Option<PathBuf>) -> Result<()> {
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 780.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ModSync.rs",
        native,
        Box::new(move |_| Ok(Box::new(App::new(input, game, mods)))),
    )
    .map_err(|e| anyhow::anyhow!("Desktop startup failed: {e}"))
}
