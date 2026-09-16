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
    theme: String,
    input: String,
    loaded_input: Option<PathBuf>,
    profile_directory: String,
    profile_name: String,
    profile_names: Vec<String>,
    profile_target: String,
    profile_delete: Option<(String, String)>,
    game: String,
    mods: String,
    patcher: String,
    scratch: String,
    expanded_budget: String,
    allow_processes: bool,
    guide: Option<Guide>,
    page: usize,
    selected: usize,
    spoiler_free: bool,
    search: String,
    tier_filter: String,
    category_filter: String,
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
            theme: "/Styles/LightStyle.axaml".into(),
            input: string(input),
            loaded_input: None,
            profile_directory: modsync::profiles::settings_directory(None)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            profile_name: String::new(),
            profile_names: Vec::new(),
            profile_target: String::new(),
            profile_delete: None,
            game: string(game),
            mods: string(mods),
            patcher: String::new(),
            scratch: String::new(),
            expanded_budget: String::new(),
            allow_processes: false,
            guide: None,
            page: 0,
            selected: 0,
            spoiler_free: false,
            search: String::new(),
            tier_filter: String::new(),
            category_filter: String::new(),
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
        match Guide::load_for_install(Path::new(&self.input)) {
            Ok(g) => {
                self.logs
                    .push(format!("Loaded {} components", g.components.len()));
                self.loaded_input = Some(PathBuf::from(&self.input));
                self.guide = Some(g);
                self.selected = 0;
                self.tier_filter.clear();
                self.category_filter.clear();
            }
            Err(e) => {
                self.loaded_input = None;
                self.guide = None;
                self.logs.push(format!("Load failed: {e:#}"));
            }
        }
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        let path = self.theme.to_ascii_lowercase();
        let mut visuals = egui::Visuals::light();
        if !path.contains("lightstyle") && path.contains("kotor") {
            visuals = egui::Visuals::dark();
            let accent = if path.contains("kotor2") {
                egui::Color32::from_rgb(0x18, 0xae, 0x88)
            } else {
                egui::Color32::from_rgb(0x3a, 0xaa, 0xff)
            };
            visuals.hyperlink_color = accent;
            visuals.selection.stroke.color = accent;
            visuals.widgets.active.fg_stroke.color = accent;
            visuals.widgets.hovered.fg_stroke.color = accent;
        }
        ctx.set_visuals(visuals);
    }

    fn change_theme(&mut self, theme: &str, ctx: &egui::Context) -> Result<()> {
        anyhow::ensure!(
            !self.profile_directory.is_empty(),
            "Choose a settings directory"
        );
        modsync::profiles::Store::new(Path::new(&self.profile_directory))?
            .save_theme_preference(theme)?;
        self.theme = theme.into();
        self.apply_theme(ctx);
        Ok(())
    }

    fn restore_directory_preferences(&mut self) -> Result<()> {
        anyhow::ensure!(
            !self.profile_directory.is_empty(),
            "Settings directory unavailable"
        );
        let store = modsync::profiles::Store::new(Path::new(&self.profile_directory))?;
        let (source, destination) = store.directory_preferences()?;
        if self.mods.is_empty() {
            self.mods = source.unwrap_or_default();
        }
        if self.game.is_empty() {
            self.game = destination.unwrap_or_default();
        }
        self.validated = false;
        self.confirm_install = false;
        Ok(())
    }

    fn profile_action(&mut self, activate: bool) -> Result<()> {
        anyhow::ensure!(
            !self.profile_directory.trim().is_empty(),
            "Choose a profile settings directory"
        );
        let store = modsync::profiles::Store::new(Path::new(&self.profile_directory))?;
        let guide = self
            .guide
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Load a guide first"))?;
        if activate {
            // Persist activation before changing the displayed install state.
            let profile = store.activate(&self.profile_name)?;
            let mut guide = guide.clone();
            profile.apply(&mut guide);
            if let Some(game) = profile.kotor_directory.filter(|p| !p.is_empty()) {
                self.game = game;
            }
            if let Some(mods) = profile.mod_directory.filter(|p| !p.is_empty()) {
                self.mods = mods;
            }
            self.guide = Some(guide);
            self.validated = false;
            self.confirm_install = false;
        } else {
            store.capture(
                &self.profile_name,
                guide,
                self.loaded_input.as_deref(),
                (!self.game.is_empty()).then(|| Path::new(&self.game)),
                (!self.mods.is_empty()).then(|| Path::new(&self.mods)),
            )?;
        }
        Ok(())
    }

    fn manage_profile(&mut self, action: &str) -> Result<()> {
        anyhow::ensure!(
            !self.profile_directory.trim().is_empty(),
            "Choose a profile settings directory"
        );
        let store = modsync::profiles::Store::new(Path::new(&self.profile_directory))?;
        match action {
            "Clone" => {
                self.profile_name = store
                    .clone_profile(&self.profile_name, &self.profile_target)?
                    .name;
            }
            "Rename" => {
                self.profile_name = store.rename(&self.profile_name, &self.profile_target)?.name;
            }
            "Delete" => {
                anyhow::ensure!(
                    self.profile_delete.as_ref()
                        == Some(&(self.profile_directory.clone(), self.profile_name.clone())),
                    "Confirm this profile deletion first"
                );
                store.delete_and_deactivate(&self.profile_name)?;
                self.profile_delete = None;
                self.profile_name.clear();
            }
            "Refresh" => {}
            _ => anyhow::bail!("Unknown profile action"),
        }
        self.profile_names = store.list()?.into_iter().map(|p| p.name).collect();
        Ok(())
    }

    fn profiles(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Install profiles", |ui| {
            if Self::path_row(ui, "Settings directory", &mut self.profile_directory, true) {
                self.profile_names.clear();
                self.profile_delete = None;
            }
            ui.horizontal(|ui| {
                if ui.button("Refresh profiles").clicked() && let Err(e) = self.manage_profile("Refresh") {
                    self.logs.push(format!("Profile failed: {e:#}"));
                }
                egui::ComboBox::from_id_salt("stored_profiles").selected_text("Choose stored profile").show_ui(ui, |ui| {
                    for name in &self.profile_names { ui.selectable_value(&mut self.profile_name, name.clone(), name); }
                });
            });
            ui.horizontal(|ui| {
                ui.label("Profile name");
                ui.text_edit_singleline(&mut self.profile_name);
            });
            ui.horizontal(|ui| {
                ui.label("New name for clone or rename");
                ui.text_edit_singleline(&mut self.profile_target);
                for action in ["Clone", "Rename"] {
                    if ui.button(action).clicked() && let Err(e) = self.manage_profile(action) {
                        self.logs.push(format!("Profile failed: {e:#}"));
                    }
                }
                if ui.button("Delete profile…").clicked() {
                    self.profile_delete = Some((self.profile_directory.clone(), self.profile_name.clone()));
                }
            });
            if self.profile_delete.as_ref() == Some(&(self.profile_directory.clone(), self.profile_name.clone())) {
                ui.label(format!("Delete '{}'? This cannot be undone. Its active setting will be cleared; installed files are unchanged.", self.profile_name));
                ui.horizontal(|ui| {
                    if ui.button("Confirm delete").clicked() && let Err(e) = self.manage_profile("Delete") {
                        self.logs.push(format!("Profile failed: {e:#}"));
                    }
                    if ui.button("Cancel deletion").clicked() { self.profile_delete = None; }
                });
            }
            ui.label("Activation applies selections to the loaded guide and restores stored game/mod directories.");
            ui.horizontal(|ui| {
                for (label, activate) in [("Save current profile", false), ("Activate profile", true)] {
                    if ui.add_enabled(self.guide.is_some(), egui::Button::new(label)).clicked() {
                        let result = self.profile_action(activate);
                        self.logs.push(match result {
                            Ok(()) => format!("Profile '{}' {}.", self.profile_name, if activate { "activated" } else { "saved" }),
                            Err(e) => format!("Profile failed: {e:#}"),
                        });
                    }
                }
            });
        });
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

    fn options(ui: &mut egui::Ui, component: &mut Component, spoiler_free: bool) -> bool {
        let mut changed = false;
        for option in &mut component.options {
            let name = modsync::selection::display_name(option, spoiler_free);
            changed |= ui.checkbox(&mut option.is_selected, name).changed();
            if !option.description.is_empty() {
                ui.label(modsync::selection::display_description(
                    option,
                    spoiler_free,
                ))
                .on_hover_text(modsync::selection::tooltip_description(
                    option,
                    spoiler_free,
                ));
            }
            if option.is_selected {
                changed |= Self::options(ui, option, spoiler_free);
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
                    egui::ComboBox::from_id_salt("theme")
                        .selected_text("Theme")
                        .show_ui(ui, |ui| {
                            for (label, path) in [
                                ("Light", "/Styles/LightStyle.axaml"),
                                ("KOTOR", "/Styles/KotorStyle.axaml"),
                                ("KOTOR 2", "/Styles/Kotor2Style.axaml"),
                            ] {
                                if ui.selectable_label(self.theme == path, label).clicked()
                                    && let Err(e) = self.change_theme(path, ctx)
                                {
                                    self.logs.push(format!("Theme save failed: {e:#}"));
                                }
                            }
                        });
                });
            });
            ui.add_enabled_ui(!busy, |ui| {
                self.profiles(ui);
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
                    ui.label("Read ModSync TOML, JSON, YAML, XML, or Markdown. Review generated instructions before installing.");
                    if Self::path_row(ui, "Guide", &mut self.input, false) { self.validated = false; }
                    if ui.button("Load guide").clicked() { self.load(); }
                    if ui.add_enabled(self.guide.is_some(), egui::Button::new("Draft instructions from prose")).clicked()
                        && let Some(guide) = &mut self.guide {
                        let results = modsync::draft::generate(guide);
                        self.logs.push(format!("Drafted {} instructions; inspect each component before accepting its draft.", results.iter().map(|r| r.draft_instruction_count).sum::<usize>()));
                        self.validated = false;
                    }
                    ui.separator();
                    if Self::path_row(ui, "Game directory", &mut self.game, true) | Self::path_row(ui, "Mod directory", &mut self.mods, true) { self.validated = false; }
                    ui.label("Use separate folders. Put downloaded archives in the mod directory.");
                    if ui.button("Save directories for next launch").clicked() {
                        let result = (|| -> Result<()> {
                            anyhow::ensure!(!self.profile_directory.is_empty(), "Choose a settings directory");
                            modsync::profiles::Store::new(Path::new(&self.profile_directory))?
                                .save_directory_preferences(&self.mods, &self.game)
                        })();
                        self.logs.push(match result { Ok(()) => "Directories saved in the selected settings directory.".into(), Err(e) => format!("Settings save failed: {e:#}") });
                    }
                    if let Some(g) = &self.guide { ui.label(format!("{} components loaded", g.components.len())); }
                    if ui.add_enabled(self.guide.is_some(), egui::Button::new("Choose mods →")).clicked() { self.page = 1; }
                }
                1 => {
                    let Some(g) = &mut self.guide else { ui.label("Load a guide first."); return; };
                    ui.horizontal(|ui| {
                        if ui.button("Select all").clicked() { for c in &mut g.components { c.is_selected = true; } self.validated = false; }
                        if ui.button("Deselect all").clicked() { for c in &mut g.components { c.is_selected = false; } self.validated = false; }
                        ui.label(format!("{} / {} selected", g.components.iter().filter(|c| c.is_selected).count(), g.components.len()));
                        ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search displayed name or description"));
                    });
                    let mut spoiler_free = self.spoiler_free;
                    if ui.checkbox(&mut spoiler_free, "Spoiler-free names and descriptions").changed() {
                        let result = (|| -> Result<()> {
                            anyhow::ensure!(!self.profile_directory.is_empty(), "Choose a settings directory");
                            modsync::profiles::Store::new(Path::new(&self.profile_directory))?.save_spoiler_free_preference(spoiler_free)
                        })();
                        match result {
                            Ok(()) => self.spoiler_free = spoiler_free,
                            Err(e) => self.logs.push(format!("Spoiler preference save failed: {e:#}")),
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        let tiers: std::collections::BTreeSet<_> = g.components.iter().map(|c| c.tier.clone()).filter(|s| !s.is_empty()).collect();
                        let categories: std::collections::BTreeSet<_> = g.components.iter().flat_map(|c| c.category.clone()).collect();
                        egui::ComboBox::from_id_salt("tier_filter").selected_text(if self.tier_filter.is_empty() { "All tiers" } else { &self.tier_filter }).show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.tier_filter, String::new(), "All tiers");
                            for tier in tiers { ui.selectable_value(&mut self.tier_filter, tier.clone(), tier); }
                        });
                        if ui.add_enabled(!self.tier_filter.is_empty(), egui::Button::new("Select through tier")).clicked() {
                            modsync::selection::select_tier(&mut g.components, &self.tier_filter);
                            self.validated = false; self.confirm_install = false;
                        }
                        egui::ComboBox::from_id_salt("category_filter").selected_text(if self.category_filter.is_empty() { "All categories" } else { &self.category_filter }).show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.category_filter, String::new(), "All categories");
                            for category in categories { ui.selectable_value(&mut self.category_filter, category.clone(), category); }
                        });
                        if ui.add_enabled(!self.category_filter.is_empty(), egui::Button::new("Select category")).clicked() {
                            modsync::selection::select_categories(&mut g.components, std::slice::from_ref(&self.category_filter));
                            self.validated = false; self.confirm_install = false;
                        }
                    });
                    ui.separator();
                    ui.columns(2, |columns| {
                        egui::ScrollArea::vertical().id_salt("mods").show(&mut columns[0], |ui| {
                            for (index, c) in g.components.iter_mut().enumerate() {
                                if !modsync::selection::matches_search(c, &self.search, self.spoiler_free) { continue; }
                                if !self.tier_filter.is_empty() && !c.tier.eq_ignore_ascii_case(&self.tier_filter) { continue; }
                                if !self.category_filter.is_empty() && !c.category.iter().any(|cat| cat.eq_ignore_ascii_case(&self.category_filter)) { continue; }
                                ui.horizontal(|ui| {
                                    if ui.checkbox(&mut c.is_selected, "").on_hover_text("Select component").changed() { self.validated = false; }
                                    if ui.selectable_label(self.selected == index, modsync::selection::display_name(c, self.spoiler_free)).clicked() { self.selected = index; }
                                });
                            }
                        });
                        egui::ScrollArea::vertical().id_salt("details").show(&mut columns[1], |ui| {
                            if let Some(c) = g.components.get_mut(self.selected) {
                                ui.heading(modsync::selection::display_name(c, self.spoiler_free));
                                ui.label(format!("{}  ·  {}", c.author, c.tier));
                                ui.label(modsync::selection::display_description(c, self.spoiler_free)).on_hover_text(modsync::selection::tooltip_description(c, self.spoiler_free));
                                if !c.directions.is_empty() && !self.spoiler_free { ui.separator(); ui.label(&c.directions); }
                                if Self::options(ui, c, self.spoiler_free) { self.validated = false; }
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
                                if c.metadata.contains_key("DraftReview") {
                                    ui.collapsing("Draft instructions and review notes", |ui| {
                                        ui.monospace(serde_json::to_string_pretty(&c.instructions).unwrap_or_default());
                                        ui.monospace(serde_json::to_string_pretty(&c.metadata["DraftReview"]).unwrap_or_default());
                                    });
                                    if modsync::draft::requires_review(c) && ui.button("Mark these instructions reviewed").clicked() {
                                        if let Err(e) = modsync::draft::mark_reviewed(c) { self.logs.push(format!("Review remains open: {e:#}")); }
                                        self.validated = false;
                                    }
                                }
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

pub fn launch(
    input: Option<PathBuf>,
    game: Option<PathBuf>,
    mods: Option<PathBuf>,
    settings_dir: Option<PathBuf>,
) -> Result<()> {
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 780.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ModSync.rs",
        native,
        Box::new(move |creation| {
            let mut app = App::new(input, game, mods);
            if let Some(directory) = settings_dir {
                app.profile_directory = directory.to_string_lossy().into_owned();
            }
            if let Err(e) = app.restore_directory_preferences() {
                app.logs.push(format!("Settings load failed: {e:#}"));
            }
            match modsync::profiles::Store::new(Path::new(&app.profile_directory))
                .and_then(|store| store.theme_preference())
            {
                Ok(theme) => app.theme = theme,
                Err(e) => app.logs.push(format!("Theme load failed: {e:#}")),
            }
            match modsync::profiles::Store::new(Path::new(&app.profile_directory))
                .and_then(|store| store.spoiler_free_preference())
            {
                Ok(enabled) => app.spoiler_free = enabled,
                Err(e) => app
                    .logs
                    .push(format!("Spoiler preference load failed: {e:#}")),
            }
            app.apply_theme(&creation.egui_ctx);
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Desktop startup failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_profile_capture_and_activation_preserve_loaded_guide_and_reset_validation() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("guide.json");
        std::fs::write(
            &input,
            r#"[{"Guid":"11111111-1111-4111-8111-111111111111","Name":"Mod","IsSelected":true}]"#,
        )
        .unwrap();
        let mut app = App::new(
            Some(input.clone()),
            Some(dir.path().join("game")),
            Some(dir.path().join("mods")),
        );
        app.profile_directory = dir.path().join("settings").to_string_lossy().into_owned();
        app.profile_name = "Desktop".into();
        app.guide.as_mut().unwrap().components[0].is_selected = false;
        app.input = "edited-but-not-loaded.json".into();
        app.profile_action(false).unwrap();
        let store = modsync::profiles::Store::new(Path::new(&app.profile_directory)).unwrap();
        assert_eq!(
            store
                .require("Desktop")
                .unwrap()
                .instruction_file_path
                .as_deref(),
            input.to_str()
        );
        app.guide.as_mut().unwrap().components[0].is_selected = true;
        app.game = "different".into();
        app.mods = "different".into();
        app.validated = true;
        app.confirm_install = true;
        app.profile_action(true).unwrap();
        assert!(!app.guide.as_ref().unwrap().components[0].is_selected);
        assert_eq!(Path::new(&app.game), dir.path().join("game"));
        assert_eq!(Path::new(&app.mods), dir.path().join("mods"));
        assert!(!app.validated && !app.confirm_install);
        assert_eq!(store.active_name().unwrap().as_deref(), Some("Desktop"));
        std::fs::write(dir.path().join("settings/settings.json"), "broken").unwrap();
        app.guide.as_mut().unwrap().components[0].is_selected = true;
        app.game = "unchanged".into();
        assert!(app.profile_action(true).is_err());
        assert!(app.guide.as_ref().unwrap().components[0].is_selected);
        assert_eq!(app.game, "unchanged");
    }
    #[test]
    fn desktop_profile_management_checks_confirmation_and_refreshes_names() {
        let dir = tempfile::tempdir().unwrap();
        let store = modsync::profiles::Store::new(dir.path()).unwrap();
        store.create("Original").unwrap();
        let mut app = App::new(None, None, None);
        app.profile_directory = dir.path().to_string_lossy().into_owned();
        app.manage_profile("Refresh").unwrap();
        assert_eq!(app.profile_names, ["Original"]);
        app.profile_name = "Original".into();
        app.profile_target = "Copy".into();
        app.manage_profile("Clone").unwrap();
        assert_eq!(app.profile_name, "Copy");
        app.profile_target = "Renamed".into();
        app.manage_profile("Rename").unwrap();
        assert_eq!(app.profile_names, ["Original", "Renamed"]);
        store.activate("Renamed").unwrap();
        assert!(app.manage_profile("Delete").is_err());
        app.profile_delete = Some((app.profile_directory.clone(), "Original".into()));
        assert!(app.manage_profile("Delete").is_err());
        app.profile_delete = Some((app.profile_directory.clone(), app.profile_name.clone()));
        app.manage_profile("Delete").unwrap();
        assert_eq!(app.profile_names, ["Original"]);
        assert_eq!(store.active_name().unwrap(), None);
        assert!(app.profile_delete.is_none());
    }
    #[test]
    fn desktop_restores_saved_directories_but_preserves_explicit_launch_paths() {
        let dir = tempfile::tempdir().unwrap();
        let store = modsync::profiles::Store::new(dir.path()).unwrap();
        store
            .save_directory_preferences("saved-mods", "saved-game")
            .unwrap();
        let mut app = App::new(None, None, Some(PathBuf::from("explicit-mods")));
        app.profile_directory = dir.path().to_string_lossy().into_owned();
        app.restore_directory_preferences().unwrap();
        assert_eq!(app.mods, "explicit-mods");
        assert_eq!(app.game, "saved-game");
        assert!(!app.validated && !app.confirm_install);
        std::fs::write(
            dir.path().join("settings.json"),
            r#"{"sourcePath":"other","destinationPath":42}"#,
        )
        .unwrap();
        assert!(app.restore_directory_preferences().is_err());
        assert_eq!(app.mods, "explicit-mods");
        assert_eq!(app.game, "saved-game");
    }
    #[test]
    fn themes_persist_and_failed_save_keeps_displayed_theme() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::new(None, None, None);
        app.profile_directory = dir.path().to_string_lossy().into_owned();
        let ctx = egui::Context::default();
        for path in [
            "/Styles/KotorStyle.axaml",
            "/Styles/Kotor2Style.axaml",
            "/Styles/LightStyle.axaml",
        ] {
            app.change_theme(path, &ctx).unwrap();
            assert_eq!(ctx.style().visuals.dark_mode, !path.contains("Light"));
            assert_eq!(
                modsync::profiles::Store::new(dir.path())
                    .unwrap()
                    .theme_preference()
                    .unwrap(),
                path
            );
        }
        std::fs::write(dir.path().join("settings.json"), "broken").unwrap();
        assert!(app.change_theme("/Styles/KotorStyle.axaml", &ctx).is_err());
        assert!(!ctx.style().visuals.dark_mode);
        assert_eq!(app.theme, "/Styles/LightStyle.axaml");
    }
}
