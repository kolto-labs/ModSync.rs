//! Legacy shell actions reuse the application worker, sharing gate and shared guide APIs.
use super::{App, Job, Message, SharingContinuation};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use modsync::{Guide, editor};
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
    pub raw_open: bool,
    pub log_open: bool,
    about: bool,
    dropped: Option<PathBuf>,
    drop_shown: bool,
    drop_previous_focus: Option<egui::Id>,
    pub status: String,
    raw: String,
    generated_guid: String,
    format: usize,
    candidate: Option<Guide>,
    context: String,
    run: u64,
    log_text: String,
    auto_scroll: bool,
    structured: super::structured_log::State,
    worker: Option<ShellWorker>,
    terminal_received: bool,
}
struct ShellWorker {
    cancelled: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}
impl Drop for ShellWorker {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.cancelled.store(true, Ordering::Release);
            let _ = handle.join();
        }
    }
}
impl State {
    pub(super) fn settled(&self) -> bool {
        self.worker.is_none()
    }
}
#[derive(Clone)]
pub(super) enum Work {
    PickGuide,
    Load(PathBuf),
    SaveGuide,
    SaveSelection,
    SaveLog,
    SaveStructuredLog(String),
    RawRender(String),
    RawParse(String, String),
    RawApply(Guide),
    Close,
}
enum Output {
    Guide(Guide, Option<PathBuf>),
    Saved(PathBuf),
    SelectionSaved(Guide, PathBuf, Option<Vec<String>>),
    Raw(String),
    Candidate(Guide),
    Closed,
}
pub(super) struct Finished {
    run: u64,
    context: String,
    result: Result<Output>,
}
impl App {
    pub(super) fn poll_shell_worker(&mut self) {
        if self.shell.worker.as_ref().is_some_and(|worker| {
            worker
                .handle
                .as_ref()
                .is_some_and(std::thread::JoinHandle::is_finished)
        }) {
            let mut worker = self.shell.worker.take().expect("Finished shell worker");
            if let Some(handle) = worker.handle.take()
                && handle.join().is_err()
            {
                // A successful terminal can arrive after this frame's message
                // drain but before the worker panics in its final repaint call.
                // Joining makes this final drain stable; retain any save receipt.
                if !self.shell.terminal_received && self.job == Job::Editor {
                    let pending: Vec<_> = self
                        .receiver
                        .as_ref()
                        .map(|receiver| receiver.try_iter().collect())
                        .unwrap_or_default();
                    for message in pending {
                        self.handle_message(message);
                    }
                }
                if self.shell.terminal_received {
                    self.logs
                        .push("Window worker stopped after reporting its result.".into());
                } else {
                    self.shell.status = "Window worker stopped unexpectedly.".into();
                    self.logs.push(self.shell.status.clone());
                    if self.job == Job::Editor {
                        self.receiver = None;
                    }
                }
            }
        }
    }
    pub(super) fn shell_work(&mut self, work: Work, context: Option<String>, ctx: &egui::Context) {
        if !self.shell.settled()
            || self.receiver.is_some()
            || self.desktop.busy()
            || self.sharing_continuation.is_some()
            || self.sharing.closing
            || self.fomod_pending()
        {
            return;
        }
        self.refresh_input_stamp();
        if context
            .as_ref()
            .is_some_and(|context| *context != self.input_stamp)
        {
            self.shell.status = "Action discarded because inputs changed.".into();
            return;
        }
        if matches!(
            work,
            Work::PickGuide | Work::Load(_) | Work::RawApply(_) | Work::Close
        ) && !self.sharing.mutation()
        {
            self.sharing_continuation =
                Some(SharingContinuation::Shell(work, self.input_stamp.clone()));
            return;
        }
        let guide = self.guide.clone();
        let input = self.loaded_input.clone();
        let choices_path = self.fomod_choices.clone();
        let completed_base = self.completed_base.clone();
        let logs = self.logs.join("\n");
        let context = self.input_stamp.clone();
        self.shell.run = self.shell.run.wrapping_add(1);
        let run = self.shell.run;
        self.shell.status = "Working…".into();
        self.shell.terminal_received = false;
        self.cancelled = Arc::new(AtomicBool::new(false));
        let flag = self.cancelled.clone();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.job = Job::Editor;
        let ctx = ctx.clone();
        let worker_cancelled = flag.clone();
        let handle = std::thread::spawn(move || {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<Output> {
                    ensure!(!flag.load(Ordering::Acquire), "Window operation cancelled");
                    let output = match work {
                        Work::PickGuide | Work::Load(_) => {
                            let path = if let Work::Load(path) = work {
                                path
                            } else {
                                rfd::FileDialog::new()
                                    .add_filter(
                                        "Guide",
                                        &["toml", "json", "yaml", "yml", "xml", "md"],
                                    )
                                    .pick_file()
                                    .context("No guide selected")?
                            };
                            let guide = Guide::load_for_install(&path)?;
                            Output::Guide(guide, Some(path))
                        }
                        Work::SaveGuide => {
                            let guide = guide.context("Load a guide first")?;
                            let path = rfd::FileDialog::new()
                                .set_file_name("edited-guide.toml")
                                .save_file()
                                .context("No output selected")?;
                            ensure!(
                                !flag.load(Ordering::Acquire),
                                "Save cancelled before publication"
                            );
                            if let Some(input) = input {
                                editor::save_distinct(&guide, &input, &path)?;
                            } else {
                                editor::save_validated(&guide, &path)?;
                            }
                            return Ok(Output::Saved(path));
                        }
                        Work::SaveSelection => {
                            let mut guide = guide.context("Load a guide first")?;
                            if !choices_path.trim().is_empty() {
                                modsync::fomod::ChoicesFile::load(std::path::Path::new(
                                    &choices_path,
                                ))?
                                .apply_to_remaining(
                                    &mut guide,
                                    completed_base.as_deref().unwrap_or_default(),
                                )?;
                            }
                            let path = rfd::FileDialog::new()
                                .set_file_name("selection.toml")
                                .save_file()
                                .context("No output selected")?;
                            ensure!(
                                !flag.load(Ordering::Acquire),
                                "Save cancelled before publication"
                            );
                            match std::fs::symlink_metadata(&path) {
                                Ok(metadata) => ensure!(
                                    metadata.file_type().is_file(),
                                    "Selection output must be a regular file or absent"
                                ),
                                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                                Err(error) => return Err(error.into()),
                            }
                            guide.save(&path)?;
                            // A published file remains a success even if cancellation
                            // arrives while the completion message is in transit.
                            return Ok(Output::SelectionSaved(guide, path, completed_base));
                        }
                        Work::SaveLog | Work::SaveStructuredLog(_) => {
                            let logs = if let Work::SaveStructuredLog(text) = work {
                                text
                            } else {
                                logs
                            };
                            let path = rfd::FileDialog::new()
                                .set_file_name("modsync-report.txt")
                                .save_file()
                                .context("No output selected")?;
                            ensure!(
                                !flag.load(Ordering::Acquire),
                                "Save cancelled before publication"
                            );
                            if let Ok(meta) = std::fs::symlink_metadata(&path) {
                                ensure!(
                                    meta.file_type().is_file(),
                                    "Log output must be a regular file"
                                );
                            }
                            modsync::recovery::atomic_write(
                                &std::path::absolute(&path)?,
                                logs.as_bytes(),
                            )?;
                            return Ok(Output::Saved(path));
                        }
                        Work::RawRender(format) => {
                            Output::Raw(guide.context("Load a guide first")?.render(&format)?)
                        }
                        Work::RawParse(text, format) => {
                            ensure!(text.len() <= 8 * 1024 * 1024, "Raw document exceeds 8 MiB");
                            let guide = Guide::parse_preserving_selection(&text, &format)?;
                            editor::validate_guide(&guide)?;
                            Output::Candidate(guide)
                        }
                        Work::RawApply(guide) => {
                            editor::validate_guide(&guide)?;
                            Output::Guide(guide, input)
                        }
                        Work::Close => Output::Closed,
                    };
                    ensure!(!flag.load(Ordering::Acquire), "Window operation cancelled");
                    Ok(output)
                }))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Window worker stopped unexpectedly")));
            let _ = tx.send(Message::ShellTerminal(Box::new(Finished {
                run,
                context,
                result,
            })));
            ctx.request_repaint();
        });
        self.shell.worker = Some(ShellWorker {
            cancelled: worker_cancelled,
            handle: Some(handle),
        });
    }
    pub(super) fn shell_terminal(&mut self, finished: Finished) {
        if finished.run != self.shell.run {
            return;
        }
        self.shell.terminal_received = true;
        self.receiver = None;
        self.refresh_input_stamp();
        let current = finished.context == self.input_stamp && !self.sharing.closing;
        match finished.result {
            Ok(Output::Saved(path)) => self.shell.status = format!("Saved {}", path.display()),
            Ok(Output::SelectionSaved(guide, path, completed_base)) => {
                self.shell.status = format!("Saved selection to {}.", path.display());
                self.logs.push(self.shell.status.clone());
                if current && completed_base == self.completed_base {
                    self.downloads.unsaved_records = false;
                    self.sharing.promote(&guide, path);
                }
            }
            Ok(_) if !current || self.cancelled.load(Ordering::Acquire) => {
                self.shell.status = "Result discarded after cancellation or changed inputs.".into()
            }
            Ok(Output::Guide(guide, path)) => {
                self.reset_installation();
                self.wizard =
                    (!guide.components.is_empty()).then(|| modsync::wizard::Wizard::new(&guide));
                self.utility = false;
                self.guide = Some(guide);
                self.selected = 0;
                self.page = 0;
                self.downloads.unsaved_records = false;
                self.loaded_input = path.clone();
                if let Some(path) = path {
                    self.input = path.to_string_lossy().into_owned();
                }
                self.shell.candidate = None;
                self.shell.status =
                    "Guide loaded; validation is required before installation.".into();
                self.sharing.resume_after_mutation(self.guide.as_ref());
            }
            Ok(Output::Raw(text)) => {
                self.shell.raw = text;
                self.shell.context = finished.context;
                self.shell.candidate = None;
                self.shell.status = "Raw document rendered from the captured guide.".into();
            }
            Ok(Output::Candidate(guide)) => {
                self.shell.candidate = Some(guide);
                self.shell.context = finished.context;
                self.shell.status =
                    "Candidate parsed and validated; confirm replacement explicitly.".into();
            }
            Ok(Output::Closed) => {
                self.reset_installation();
                self.guide = None;
                self.loaded_input = None;
                self.input.clear();
                self.wizard = None;
                self.utility = true;
                self.page = 0;
                self.shell.status = "Guide closed.".into();
                self.sharing.resume_after_mutation(None);
            }
            Err(error) => self.shell.status = format!("Window action stopped: {error:#}"),
        }
    }
    pub(super) fn shell_ui(&mut self, ctx: &egui::Context) {
        let available = self.shell.settled()
            && self.receiver.is_none()
            && !self.desktop.busy()
            && self.sharing_continuation.is_none()
            && !self.sharing.closing
            && !self.fomod_pending();
        for file in ctx.input(|input| input.raw.dropped_files.clone()) {
            if let Some(path) = file.path {
                if available
                    && matches!(
                        path.extension()
                            .and_then(|v| v.to_str())
                            .map(str::to_ascii_lowercase)
                            .as_deref(),
                        Some("toml" | "json" | "yaml" | "yml" | "xml" | "md")
                    )
                {
                    if self.shell.dropped.is_none() {
                        self.shell.drop_previous_focus = ctx.memory(|memory| memory.focused());
                    }
                    self.shell.drop_shown = false;
                    self.shell.dropped = Some(path);
                } else {
                    self.shell.status =
                        "Drop rejected: wait for active work and use a supported guide format."
                            .into();
                }
            }
        }
        let mut work = None;
        egui::TopBottomPanel::top("legacy_menu").show(ctx,|ui|{
            ui.horizontal_wrapped(|ui|{
                ui.menu_button("File",|ui|{if ui.add_enabled(available,egui::Button::new("Open File…")).clicked(){work=Some(Work::PickGuide);ui.close();}
                if ui.add_enabled(available&&self.guide.is_some(),egui::Button::new("Save separate guide…")).clicked(){work=Some(Work::SaveGuide);ui.close();}
                if ui.add_enabled(available&&self.guide.is_some(),egui::Button::new("Close guide")).clicked(){self.shell.dropped=None;work=Some(Work::Close);ui.close();}
                if ui.button("Exit").clicked(){self.sharing.close_confirm=true;ui.close();}});
                ui.menu_button("Tools",|ui|{
                    if ui.add_enabled(available,egui::Button::new("Editor Mode")).clicked(){self.utility=true;self.page=3;ui.close();}
                    for(label,open)in[("Manage Checkpoints",&mut self.checkpoints.open),("Settings",&mut self.desktop.open),("Managed Deployment Status / Purge",&mut self.deployment.open),("Show Output Log",&mut self.shell.log_open),("Merge guides",&mut self.merge.open),("Conflict analysis",&mut self.tools.conflicts_open),("Provider updates",&mut self.tools.updates_open)]{if ui.button(label).clicked(){*open=true;ui.close();}}
                    if ui.add_enabled(available, egui::Button::new("Run standalone HoloPatcher")).clicked() { self.standalone.open = true; }
                    for label in ["Fix iOS case sensitivity", "Fix file/folder permissions"] {ui.add_enabled(false,egui::Button::new(label)).on_disabled_hover_text("No shared desktop action is available; see F24/F25 compatibility. Install patcher instructions through the validated guide instead.");}
                });
                ui.menu_button("Help", |ui| {
                    ui.hyperlink_to("Create GitHub Issue", "https://github.com/th3w1zard1/ModSync/issues/new");
                    ui.hyperlink_to("Sponsor on GitHub", "https://github.com/sponsors/th3w1zard1");
                    ui.separator();
                    link_menu(ui, "DeadlyStream", &[("Discord", "https://discord.gg/nDkHXfc36s"), ("Website", "https://deadlystream.com")]);
                    link_menu(ui, "KOTOR Community Portal", &[("Discord", "https://discord.com/invite/kotor"), ("Website", "https://kotor.neocities.org")]);
                    link_menu(ui, "PCGamingWiki", &[("KOTOR 1", "https://www.pcgamingwiki.com/wiki/Star_Wars:_Knights_of_the_Old_Republic"), ("KOTOR 2: TSL", "https://www.pcgamingwiki.com/wiki/Star_Wars:_Knights_of_the_Old_Republic_II_-_The_Sith_Lords")]);
                });
                ui.menu_button("About", |ui| {
                    if ui.button("About ModSync.rs").clicked() { self.shell.about = true; }
                    link_menu(ui, "The ModSync Project", &[("DeadlyStream", "https://deadlystream.com/files/file/2317-kotormodsync/"), ("GitHub", "https://github.com/th3w1zard1/ModSync")]);
                    holopatcher_links(ui);
                });
                ui.menu_button("More", |ui| {
                    ui.menu_button("Other Projects", |ui| {
                        ui.menu_button("PyKotor Library", |ui| {
                            // A parent menu cannot also open a link in egui; retain its destination as a named child.
                            ui.hyperlink_to("PyKotor Library", "https://github.com/NickHugi/PyKotor");
                            holopatcher_links(ui);
                            link_menu(ui, "Holocron Toolset", &[("GitHub", "https://github.com/NickHugi/PyKotor/blob/master/Tools/HolocronToolset"), ("DeadlyStream", "https://deadlystream.com/files/file/1982-holocron-toolset"), ("Discord", "https://discord.gg/hfAqtkVEzQ")]);
                            ui.hyperlink_to("Auto-Translate / Font Creator", "https://deadlystream.com/files/file/2375-kotor-autotranslate-tool");
                            ui.hyperlink_to("KotorDiff", "https://deadlystream.com/files/file/2364-kotordiff");
                        });
                        link_menu(ui, "LIP Composer / reone toolkit", &[("DeadlyStream", "https://deadlystream.com/files/file/1862-reone-toolkit"), ("GitHub", "https://github.com/seedhartha/reone/wiki/Tooling")]);
                        link_menu(ui, "Open-Source Odyssey/Aurora Engines", &[("KotOR.js", "https://github.com/KobaltBlu/KotOR.js"), ("NorthernLights", "https://github.com/lachjames/NorthernLights"), ("reone", "https://github.com/seedhartha/reone")]);
                    });
                    ui.hyperlink_to("Modding Tools", "https://deadlystream.github.io/ds-kotor-modding-wiki/en/#!pages/tools_overview.md");
                });
                if ui.add_enabled(available,egui::Button::new("Home")).clicked(){self.utility=true;self.page=0;}
                if ui.add_enabled(available&&self.guide.is_some(),egui::Button::new("Raw document")).clicked(){self.shell.raw_open=true;}
            });
            ui.horizontal_wrapped(|ui|{for(label,page)in[("Getting started",0),("Summary / selection",1),("Validation / install",2),("Editor",3)]{if ui.add_enabled(available,egui::Button::new(label).selected(self.utility&&self.page==page)).clicked(){self.utility=true;self.page=page;}}
                if self.wizard.is_some()&&ui.button("Wizard mode").clicked(){self.utility=false;}ui.label(&self.shell.status);});
        });
        if let Some(path) = self.shell.dropped.clone() {
            let mut load = false;
            let mut cancel = false;
            let response = egui::Modal::new(egui::Id::new("confirm_dropped_guide"))
                .show(ctx, |ui| {
                    ui.heading("Load dropped guide?");
                    ui.label(path.display().to_string());
                    ui.label("This replaces the loaded guide after sharing stops. It never installs automatically.");
                    load = ui.add_enabled(available, egui::Button::new("Load this guide")).clicked();
                    let button = ui.button("Cancel");
                    if !self.shell.drop_shown { button.request_focus(); }
                    cancel = button.clicked();
                });
            self.shell.drop_shown = true;
            // Cancellation wins if keyboard dismissal and activation coincide.
            let dismissed = cancel || response.should_close();
            if dismissed || load {
                self.shell.dropped = None;
                self.shell.drop_shown = false;
                if let Some(previous) = self.shell.drop_previous_focus.take() {
                    ctx.memory_mut(|memory| memory.request_focus(previous));
                }
                if !dismissed {
                    work = Some(Work::Load(path));
                }
            }
        }
        if self.shell.raw_open {
            let mut open = true;
            egui::Window::new("Raw guide document")
                .open(&mut open)
                .default_size([720.0, 560.0])
                .show(ctx, |ui| {
                    ui.add_enabled_ui(available, |ui| {
                        let formats = ["toml", "md", "yaml", "json"];
                        let before = self.shell.format;
                        egui::ComboBox::from_id_salt("raw_format")
                            .selected_text(formats[self.shell.format])
                            .show_ui(ui, |ui| {
                                for (index, format) in formats.iter().enumerate() {
                                    ui.selectable_value(&mut self.shell.format, index, *format);
                                }
                            });
                        if before != self.shell.format {
                            self.shell.candidate = None;
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Render current guide").clicked() {
                                work = Some(Work::RawRender(formats[self.shell.format].into()));
                            }
                            if ui.button("Copy raw text").clicked() {
                                ctx.copy_text(self.shell.raw.clone());
                            }
                            if ui.button("Parse replacement preview").clicked() {
                                work = Some(Work::RawParse(
                                    self.shell.raw.clone(),
                                    formats[self.shell.format].into(),
                                ));
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Generate GUID").clicked() {
                                self.shell.generated_guid = uuid::Uuid::new_v4().to_string();
                            }
                            let mut value = self.shell.generated_guid.as_str();
                            ui.add(egui::TextEdit::singleline(&mut value)
                                .hint_text("Generated GUID")
                                .desired_width(290.0));
                            if ui.add_enabled(!self.shell.generated_guid.is_empty(), egui::Button::new("Copy GUID")).clicked() {
                                ctx.copy_text(self.shell.generated_guid.clone());
                            }
                        });
                        ui.label("Existing GUIDs may be referenced elsewhere in the guide. Generating a GUID here does not replace any IDs; change existing IDs only after reviewing their references.");
                        egui::ScrollArea::vertical()
                            .max_height(350.0)
                            .show(ui, |ui| {
                                if ui
                                    .add(
                                        egui::TextEdit::multiline(&mut self.shell.raw)
                                            .desired_rows(18)
                                            .code_editor(),
                                    )
                                    .changed()
                                {
                                    self.shell.candidate = None;
                                }
                            });
                        if let Some(candidate) = &self.shell.candidate {
                            ui.label(format!(
                                "Validated replacement: {} components",
                                candidate.components.len()
                            ));
                            if ui.button("Confirm replacement of loaded guide").clicked() {
                                if self.shell.context == self.input_stamp {
                                    work = Some(Work::RawApply(candidate.clone()));
                                } else {
                                    self.shell.status =
                                        "Current guide changed; parse again.".into();
                                }
                            }
                        }
                    });
                });
            self.shell.raw_open = open;
        }
        if self.shell.about {
            egui::Window::new("About ModSync.rs").open(&mut self.shell.about).show(ctx,|ui|{ui.heading(format!("ModSync.rs {}",env!("CARGO_PKG_VERSION")));ui.label("Load a guide, choose your mods, and install in guide order. Desktop and command-line tools share installation and recovery operations.");});
        }
        if self.shell.log_open {
            let mut open = true;
            egui::Window::new("Output log")
                .open(&mut open)
                .default_size([760.0, 480.0])
                .show(ctx, |ui| {
                    self.log_content(ui, ctx, available);
                });
            self.shell.log_open = open;
        }
        if let Some(work) = work {
            self.shell_work(work, None, ctx);
        }
    }
    pub(super) fn log_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, available: bool) {
        let save = ui
            .collapsing("Application records — timestamp and severity", |ui| {
                self.shell.structured.show(ui, available)
            })
            .body_returned
            .flatten();
        if let Some(snapshot) = save {
            let count = snapshot.record_count;
            self.shell_work(Work::SaveStructuredLog(snapshot.text), None, ctx);
            self.shell.status = format!("Saving snapshot of {count} application records…");
        }
        ui.heading("Activity output");
        ui.horizontal(|ui| {
            ui.label(format!("{} entries", self.logs.len()));
            ui.checkbox(&mut self.shell.auto_scroll, "Auto-scroll");
            if ui.button("Copy all").clicked() {
                ctx.copy_text(self.logs.join("\n"));
            }
            if ui
                .add_enabled(available, egui::Button::new("Save report…"))
                .clicked()
            {
                self.shell_work(Work::SaveLog, None, ctx);
            }
            if ui
                .add_enabled(available, egui::Button::new("Clear display"))
                .clicked()
            {
                self.logs.clear();
                self.desktop.logged = 0;
            }
        });
        self.shell.log_text = self.logs.join("\n");
        ui.label(
            "Focus the log to select text; Ctrl+A selects all and Ctrl+C copies the selection.",
        );
        egui::ScrollArea::vertical()
            .stick_to_bottom(self.shell.auto_scroll)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.shell.log_text.as_str())
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .desired_rows(5),
                );
            });
    }
}

fn link_menu(ui: &mut egui::Ui, label: &str, links: &[(&str, &str)]) {
    ui.menu_button(label, |ui| {
        for (label, url) in links {
            ui.hyperlink_to(*label, *url);
        }
    });
}
fn holopatcher_links(ui: &mut egui::Ui) {
    link_menu(
        ui,
        "HoloPatcher",
        &[
            (
                "DeadlyStream",
                "https://deadlystream.com/files/file/2243-holopatcher",
            ),
            ("GitHub", "https://github.com/NickHugi/PyKotor"),
        ],
    );
}
