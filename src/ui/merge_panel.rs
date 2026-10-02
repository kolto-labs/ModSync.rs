//! Shared immutable merge review; the active guide remains unchanged.
use super::{App, Job, Message, SharingContinuation};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use modsync::{Guide, editor, merge};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    input: String,
    output: String,
    options: merge::Options,
    preview: Option<merge::Preview>,
    rendered: String,
    context: String,
    controls: String,
    disk_identity: Option<String>,
    status: String,
    run: u64,
}
pub(super) struct Finished {
    run: u64,
    context: String,
    result: Result<Output>,
}
enum Output {
    Preview(Box<merge::Preview>, String, Option<String>),
    Input(PathBuf),
    SelectedOutputPath(PathBuf),
    Saved(PathBuf),
}
#[derive(Clone, Copy)]
enum Work {
    Preview,
    PickInput,
    PickOutput,
    Save,
}

fn read_guide(path: &Path) -> Result<Guide> {
    editor::read_guide(path)
}
fn controls(state: &State) -> String {
    format!(
        "{}\n{}",
        state.input,
        serde_json::to_string(&state.options).unwrap_or_default()
    )
}

impl App {
    fn merge_work(&mut self, work: Work, ctx: &egui::Context) {
        if self.receiver.is_some()
            || self.desktop.busy()
            || self.sharing_continuation.is_some()
            || self.sharing.closing
            || self.fomod_pending()
        {
            return;
        }
        self.refresh_input_stamp();
        if matches!(work, Work::Save) {
            if self.merge.context != self.input_stamp
                || self.merge.controls != controls(&self.merge)
            {
                self.merge.status = "Inputs changed; create a new preview.".into();
                self.merge.preview = None;
                return;
            }
            if !self.sharing.mutation() {
                self.sharing_continuation =
                    Some(SharingContinuation::MergeSave(self.input_stamp.clone()));
                return;
            }
        }
        let Some(existing) = self.guide.clone() else {
            self.merge.status = "Load the existing guide first.".into();
            return;
        };
        let context = self.input_stamp.clone();
        let input = PathBuf::from(&self.merge.input);
        let output = PathBuf::from(&self.merge.output);
        let loaded = self.loaded_input.clone();
        let options = self.merge.options.clone();
        let preview = self.merge.preview.clone();
        let disk_identity = self.merge.disk_identity.clone();
        self.merge.run = self.merge.run.wrapping_add(1);
        let run = self.merge.run;
        self.merge.status = "Merge operation running…".into();
        self.cancelled = Arc::new(AtomicBool::new(false));
        let flag = self.cancelled.clone();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.job = Job::Editor;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<Output> {
                    ensure!(!flag.load(Ordering::Acquire), "Merge cancelled");
                    match work {
                        Work::PickInput => Ok(Output::Input(
                            rfd::FileDialog::new()
                                .pick_file()
                                .context("No incoming guide selected")?,
                        )),
                        Work::PickOutput => Ok(Output::SelectedOutputPath(
                            rfd::FileDialog::new()
                                .set_file_name("merged-guide.toml")
                                .save_file()
                                .context("No output selected")?,
                        )),
                        Work::Preview => {
                            let disk_identity = loaded
                                .as_deref()
                                .map(|path| {
                                    read_guide(path)
                                        .and_then(|guide| modsync::updates::guide_identity(&guide))
                                })
                                .transpose()?;
                            let incoming = read_guide(&input)?;
                            ensure!(!flag.load(Ordering::Acquire), "Merge cancelled");
                            let preview = merge::preview(&existing, &incoming, &options)?;
                            let rendered = editor::render_preview(&preview)?;
                            ensure!(!flag.load(Ordering::Acquire), "Merge cancelled");
                            Ok(Output::Preview(Box::new(preview), rendered, disk_identity))
                        }
                        Work::Save => {
                            let preview = preview.context("Preview the merge first")?;
                            let incoming = read_guide(&input)?;
                            let merged = merge::apply_preview(&existing, &incoming, &preview)?;
                            // Both authored sources must survive the distinct-save boundary.
                            if let Some(loaded) = loaded {
                                ensure!(
                                    Some(modsync::updates::guide_identity(&read_guide(&loaded)?)?)
                                        == disk_identity,
                                    "Existing guide file changed since preview"
                                );
                                let absolute = std::path::absolute(&output)?;
                                let parent = absolute
                                    .parent()
                                    .context("Output needs a directory")?
                                    .canonicalize()?;
                                let target = parent
                                    .join(absolute.file_name().context("Output needs a filename")?);
                                ensure!(
                                    loaded.canonicalize()? != target,
                                    "Merge output must differ from the existing guide file"
                                );
                            }
                            ensure!(!flag.load(Ordering::Acquire), "Merge cancelled before save");
                            editor::save_distinct(&merged, &input, &output)?;
                            Ok(Output::Saved(output))
                        }
                    }
                }))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Merge worker stopped unexpectedly")));
            let _ = tx.send(Message::MergeTerminal(Box::new(Finished {
                run,
                context,
                result,
            })));
            ctx.request_repaint();
        });
    }
    pub(super) fn merge_save_after_barrier(&mut self, context: String, ctx: &egui::Context) {
        self.refresh_input_stamp();
        if context != self.input_stamp {
            self.merge.status =
                "Save discarded because inputs changed while sharing stopped.".into();
            return;
        }
        self.merge_work(Work::Save, ctx);
    }
    pub(super) fn merge_terminal(&mut self, finished: Finished) {
        if finished.run != self.merge.run {
            return;
        }
        self.receiver = None;
        self.refresh_input_stamp();
        let current = finished.context == self.input_stamp && !self.sharing.closing;
        match finished.result {
            Ok(Output::Saved(path)) => {
                self.merge.status = format!(
                    "Merged guide saved to {}. The loaded guide and selection are unchanged.",
                    path.display()
                );
                if current {
                    self.sharing.resume_after_mutation(self.guide.as_ref());
                }
            }
            Ok(_) if !current || self.cancelled.load(Ordering::Acquire) => {
                self.merge.preview = None;
                self.merge.status =
                    "Merge result discarded after cancellation or changed inputs.".into();
            }
            Ok(Output::Preview(preview, rendered, disk_identity)) => {
                self.merge.disk_identity = disk_identity;
                self.merge.preview = Some(*preview);
                self.merge.rendered = rendered;
                self.merge.context = finished.context;
                self.merge.controls = controls(&self.merge);
                self.merge.status =
                    "Preview ready. Review order, selections, fields and warnings before saving."
                        .into();
            }
            Ok(Output::Input(path)) => {
                self.merge.input = path.to_string_lossy().into_owned();
                self.merge.preview = None;
            }
            Ok(Output::SelectedOutputPath(path)) => {
                self.merge.output = path.to_string_lossy().into_owned()
            }
            Err(error) => {
                self.merge.status = format!("Merge stopped: {error:#}");
                self.merge.preview = None;
            }
        }
    }
    pub(super) fn merge_window(&mut self, ctx: &egui::Context) {
        if !self.merge.open {
            return;
        }
        self.refresh_input_stamp();
        if self.merge.preview.is_some()
            && (self.merge.context != self.input_stamp
                || self.merge.controls != controls(&self.merge))
        {
            self.merge.preview = None;
            self.merge.rendered.clear();
            self.merge.status = "Sources or options changed; preview again.".into();
        }
        let available = self.receiver.is_none()
            && !self.desktop.busy()
            && self.sharing_continuation.is_none()
            && !self.sharing.closing
            && !self.fomod_pending();
        let mut open = true;
        let mut work = None;
        egui::Window::new("Merge guide review").open(&mut open).default_size([760.0,650.0]).vscroll(true).show(ctx,|ui| {
            ui.label("The loaded guide is the existing source. The candidate stays separate; neither source nor the loaded selection is replaced.");
            ui.label(&self.merge.status);
            ui.add_enabled_ui(available,|ui| {
                ui.horizontal(|ui|{ui.label("Incoming guide");ui.text_edit_singleline(&mut self.merge.input);if ui.button("Browse incoming").clicked(){work=Some(Work::PickInput);}});
                ui.collapsing("Merge options",|ui| {
                    let o=&mut self.merge.options;
                    for (label,value) in [("Exclude existing-only components",&mut o.exclude_existing_only),("Exclude incoming-only components",&mut o.exclude_incoming_only),("Use existing order",&mut o.use_existing_order),("Prefer all existing fields",&mut o.prefer_existing_fields),("Prefer existing name",&mut o.prefer_existing_name),("Prefer existing author",&mut o.prefer_existing_author),("Prefer existing description",&mut o.prefer_existing_description),("Prefer existing directions",&mut o.prefer_existing_directions),("Prefer existing category",&mut o.prefer_existing_category),("Prefer existing tier",&mut o.prefer_existing_tier),("Prefer existing installation method",&mut o.prefer_existing_installation_method),("Prefer existing instructions",&mut o.prefer_existing_instructions),("Prefer existing options",&mut o.prefer_existing_options),("Prefer existing resources",&mut o.prefer_existing_resource_registry)] {ui.checkbox(value,label);}
                });
                ui.collapsing("Matching heuristics",|ui| {
                    let h=&mut self.merge.options.heuristics;ui.horizontal(|ui|{ui.label("Minimum similarity score");ui.add(egui::DragValue::new(&mut h.name_similarity).range(0.0..=10.0).speed(0.01));});
                    for(label,value) in [("Ignore punctuation",&mut h.ignore_punctuation),("Exact name",&mut h.use_name_exact),("Exact author",&mut h.use_author_exact),("Similar name",&mut h.use_name_similarity),("Similar author",&mut h.use_author_similarity),("Match URL domain",&mut h.match_by_domain),("Ignore case",&mut h.ignore_case),("Trim whitespace",&mut h.trim_whitespace),("Validate resource links",&mut h.validate_links),("Skip blank updates",&mut h.skip_blank_updates)] {ui.checkbox(value,label);}
                });
                if ui.button("Create merge preview").clicked(){work=Some(Work::Preview);}
            });
            if let Some(preview)=&self.merge.preview {
                ui.label(format!("Candidate: {} components, {} selected",preview.merged.components.len(),preview.merged.components.iter().filter(|c|c.is_selected).count()));
                for warning in &preview.warnings {ui.colored_label(egui::Color32::YELLOW,warning);}
                if let Some(error)=&preview.validation_error {ui.colored_label(egui::Color32::RED,error);}
                ui.label("Content-loss warnings and invalid candidates block saving. Correct sources or options and preview again.");
                ui.collapsing("Complete immutable preview",|ui|{if ui.button("Copy preview").clicked(){ctx.copy_text(self.merge.rendered.clone());}ui.add(egui::TextEdit::multiline(&mut self.merge.rendered).interactive(false).desired_rows(15));});
                let can_save=available && preview.warnings.is_empty() && preview.validation_error.is_none();
                ui.add_enabled_ui(can_save,|ui|{ui.horizontal(|ui|{ui.label("Separate output");ui.text_edit_singleline(&mut self.merge.output);if ui.button("Choose output").clicked(){work=Some(Work::PickOutput);}});if ui.button("Apply reviewed merge and save separate guide").clicked(){work=Some(Work::Save);}});
            }
        });
        self.merge.open = open;
        if let Some(work) = work {
            self.merge_work(work, ctx);
        }
    }
}
