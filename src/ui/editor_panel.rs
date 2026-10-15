//! Transactional desktop authoring and background import/resource adapters.
mod easy_mode;
mod generation;
use super::{App, Job, Message};
use anyhow::{Context, Result};
use eframe::egui;
use modsync::{
    Component, Instruction,
    editor::{
        self, EditOperation, ImportMode, ImportPreview, RegexFlags, RegexImportProfile,
        ResourceInventory,
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

#[derive(Default)]
pub(super) struct State {
    easy: easy_mode::State,
    worker: Option<EditorWorker>,
    terminal_received: bool,
    sections: EditorSections,
    generation: generation::State,
    guid: String,
    draft: Option<Component>,
    identity: String,
    resources: String,
    metadata: String,
    guide_metadata: String,
    instruction: usize,
    status: String,
    input: String,
    format: String,
    location: String,
    source: usize,
    replace: bool,
    regex_mode: usize,
    pattern: String,
    field_patterns: String,
    flags: RegexFlags,
    preview: Option<ImportPreview>,
    import_stamp: String,
    preview_text: String,
    inventory: Option<ResourceInventory>,
    projected: Vec<String>,
    resource_trees: Vec<ResourceTree>,
    resource_selection: BTreeSet<(usize, Option<String>)>,
    browser_context: String,
    browser_extraction: String,
    projection_path: String,
    extraction_directory: String,
    save_path: String,
    run: u64,
    resolution: Option<modsync::draft::review::Resolution>,
    removal: Option<PendingRemoval>,
    restore_component_focus: bool,
}

struct EditorWorker {
    cancelled: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}
impl Drop for EditorWorker {
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

#[derive(Default)]
struct EditorSections {
    basic: bool,
    description: bool,
    dependencies: bool,
    instructions: bool,
    options: bool,
}
impl EditorSections {
    fn set_all(&mut self, expanded: bool) {
        self.basic = expanded;
        self.description = expanded;
        self.dependencies = expanded;
        self.instructions = expanded;
        self.options = expanded;
    }
}
fn section(ui: &mut egui::Ui, label: &str, expanded: &mut bool) -> bool {
    ui.separator();
    ui.toggle_value(expanded, label)
        .on_hover_text("Toggle this editor section");
    *expanded
}

struct PendingRemoval {
    guid: String,
    name: String,
    identity: String,
    context: String,
    shown: bool,
}

pub(super) enum Output {
    Easy(Box<easy_mode::Outcome>),
    Generated(Box<generation::Preview>),
    Preview(ImportPreview, String),
    Resources(ResourceInventory, Vec<ResourceTree>),
    Projected(Vec<String>),
    ImportPath(PathBuf),
    SavePath(PathBuf),
    Saved(PathBuf),
}
pub(super) struct Finished {
    pub run: u64,
    pub context: String,
    pub result: Result<Output>,
}
enum Work {
    Easy(Box<easy_mode::Request>),
    Generate(Box<generation::Request>),
    Import {
        input: String,
        location: String,
        source: usize,
        format: String,
        mode: ImportMode,
        regex: Option<RegexImportProfile>,
    },
    Resources(String),
    Project(Vec<PathBuf>),
    PickProjection(bool),
    PickImport,
    PickSave,
    Save(PathBuf),
}

const MAX_BROWSER_SELECTION: usize = 4096;
const MAX_BROWSER_ENTRIES: usize = 20_000;
const MAX_BROWSER_NODES: usize = 100_000;
const MAX_BROWSER_TEXT: usize = 4 * 1024 * 1024;

#[derive(Default)]
pub(super) struct ResourceTree {
    children: BTreeMap<String, ResourceTree>,
    member: Option<String>,
}
fn resource_trees(inventory: &ResourceInventory, flag: &AtomicBool) -> Result<Vec<ResourceTree>> {
    let mut entries = 0usize;
    let mut nodes = 0usize;
    let mut text = 0usize;
    let mut trees = Vec::new();
    for resource in &inventory.resources {
        let mut tree = ResourceTree::default();
        for member in &resource.archive_entries {
            anyhow::ensure!(!flag.load(Ordering::Acquire), "Resource tree cancelled");
            entries += 1;
            text = text
                .checked_add(member.len())
                .context("Resource tree text overflow")?;
            anyhow::ensure!(
                entries <= MAX_BROWSER_ENTRIES && text <= MAX_BROWSER_TEXT,
                "Resource browser inventory exceeds its 20000 member / 4 MiB limit"
            );
            let path = modsync::paths::literal_relative(member)?;
            anyhow::ensure!(
                !path.as_os_str().is_empty() && !member.contains('\\'),
                "Invalid archive member path"
            );
            let parts: Vec<_> = member.split('/').collect();
            anyhow::ensure!(
                parts.len() <= 64 && parts.iter().all(|part| !part.is_empty()),
                "Archive tree exceeds path depth limit"
            );
            let mut branch = &mut tree;
            for part in parts {
                if !branch.children.contains_key(part) {
                    nodes += 1;
                    anyhow::ensure!(
                        nodes <= MAX_BROWSER_NODES,
                        "Resource tree exceeds node limit"
                    );
                }
                branch = branch.children.entry(part.into()).or_default();
            }
            anyhow::ensure!(branch.member.is_none(), "Duplicate archive browser member");
            branch.member = Some(member.clone());
        }
        trees.push(tree);
    }
    Ok(trees)
}
fn resource_tree_ui(
    ui: &mut egui::Ui,
    tree: &ResourceTree,
    resource: usize,
    selected: &mut BTreeSet<(usize, Option<String>)>,
    status: &mut String,
) -> bool {
    let mut changed = false;
    for (name, branch) in &tree.children {
        if let Some(member) = &branch.member {
            let key = (resource, Some(member.clone()));
            let mut checked = selected.contains(&key);
            if ui.checkbox(&mut checked, name).changed() {
                changed = true;
                if checked && selected.len() < MAX_BROWSER_SELECTION {
                    selected.insert(key);
                } else if !checked {
                    selected.remove(&key);
                } else {
                    *status = "Select at most 4096 browser entries.".into();
                }
            }
        }
        if !branch.children.is_empty() {
            ui.collapsing(name, |ui| {
                changed |= resource_tree_ui(ui, branch, resource, selected, status);
            });
        }
    }
    changed
}

fn project_paths(
    roots: &modsync::paths::Roots,
    paths: Vec<PathBuf>,
    flag: &AtomicBool,
) -> Result<Vec<String>> {
    anyhow::ensure!(
        !paths.is_empty() && paths.len() <= MAX_BROWSER_SELECTION,
        "Choose between 1 and 4096 paths"
    );
    let mut projected = Vec::new();
    let mut seen = BTreeSet::new();
    let mut bytes = 0usize;
    for path in paths {
        anyhow::ensure!(!flag.load(Ordering::Acquire), "Path projection cancelled");
        anyhow::ensure!(path.is_absolute(), "Browser paths must be absolute");
        let value = editor::variable_path(roots, &path)?;
        bytes = bytes
            .checked_add(value.len())
            .context("Projected path size overflow")?;
        anyhow::ensure!(bytes <= MAX_BROWSER_TEXT, "Projected paths exceed 4 MiB");
        if seen.insert(value.clone()) {
            projected.push(value);
        }
    }
    Ok(projected)
}

type ComponentRow = (String, String, Option<String>, usize, usize);

fn rows(
    components: &[Component],
    parent: Option<&str>,
    depth: usize,
    result: &mut Vec<ComponentRow>,
) {
    for (index, component) in components.iter().enumerate() {
        result.push((
            component.guid.clone(),
            format!("{}{}", "  ".repeat(depth.min(32)), component.name),
            parent.map(str::to_owned),
            index,
            components.len(),
        ));
        rows(&component.options, Some(&component.guid), depth + 1, result);
    }
}

impl App {
    pub(super) fn editor_jump(&mut self, guid: &str, instruction: usize) {
        if self.receiver.is_some()
            || self.desktop.busy()
            || self.sharing_continuation.is_some()
            || self.sharing.closing
            || self.fomod_pending()
        {
            return;
        }
        match self.editor_select(guid) {
            Ok(()) => {
                self.editor.instruction = instruction;
                self.utility = true;
                self.page = 3;
                self.editor.status = "Instruction selected in a separate draft; Apply validates before changing the guide.".into();
            }
            Err(error) => self
                .logs
                .push(format!("Editor navigation failed: {error:#}")),
        }
    }

    fn editor_select(&mut self, guid: &str) -> Result<()> {
        let guide = self.guide.as_ref().context("Load a guide first")?;
        let component = editor::find_component(guide, guid)
            .context("Component no longer exists")?
            .clone();
        self.editor.identity = modsync::updates::guide_identity(guide)?;
        self.editor.resources = editor::render_preview(&component.resource_registry)?;
        self.editor.metadata = editor::render_preview(&component.metadata)?;
        self.editor.guid = guid.into();
        self.editor.generation.preview = None;
        self.editor.draft = Some(component);
        self.editor.instruction = 0;
        self.editor.inventory = None;
        self.editor.resource_trees.clear();
        self.editor.resource_selection.clear();
        self.editor.projected.clear();
        Ok(())
    }

    fn editor_apply(&mut self, operations: &[EditOperation], ctx: &egui::Context) -> Result<()> {
        let guide = self.guide.as_ref().context("Load a guide first")?;
        let edited = editor::apply_operations(guide, operations)?;
        if self.editor.easy.session.is_some() {
            self.easy_work(
                easy_mode::Action::Edit {
                    label: "Guide edit".into(),
                    operations: operations.to_vec(),
                },
                ctx,
            )?;
            return Ok(());
        }
        self.editor_accept_guide(edited);
        Ok(())
    }

    fn editor_accept_guide(&mut self, edited: modsync::Guide) {
        self.guide = Some(edited);
        self.guide_generation = self.guide_generation.wrapping_add(1);
        self.invalidate_validation();
        self.completion = Default::default();
        self.completed_base = None;
        self.pending_base.clear();
        self.finished = false;
        self.editor.preview = None;
        self.editor.removal = None;
        self.editor.generation.preview = None;
        self.editor.draft = None;
        self.editor.inventory = None;
        self.editor.resource_trees.clear();
        self.editor.resource_selection.clear();
        self.editor.projected.clear();
        self.editor.easy.prediction = None;
        self.editor.status = "Edits applied; validate before installing.".into();
    }

    pub(super) fn poll_editor_worker(&mut self) {
        if self.editor.worker.as_ref().is_some_and(|worker| {
            worker
                .handle
                .as_ref()
                .is_some_and(std::thread::JoinHandle::is_finished)
        }) {
            let mut worker = self.editor.worker.take().expect("Finished editor worker");
            if let Some(handle) = worker.handle.take()
                && handle.join().is_err()
            {
                if !self.editor.terminal_received && matches!(self.job, Job::Editor) {
                    // Joining guarantees the producer can send no more messages.
                    // A terminal may have arrived after the frame's regular drain.
                    let messages: Vec<_> = self
                        .receiver
                        .as_ref()
                        .map(|receiver| receiver.try_iter().collect())
                        .unwrap_or_default();
                    for message in messages {
                        self.handle_message(message);
                    }
                    if !self.editor.terminal_received {
                        self.receiver = None;
                        self.editor.status =
                            "Editor worker stopped unexpectedly before returning a result.".into();
                    }
                }
                self.logs.push(if self.editor.terminal_received {
                    "Editor worker stopped unexpectedly after returning its result; the reported outcome is retained.".into()
                } else {
                    "Editor worker stopped unexpectedly before returning a result.".into()
                });
            }
        }
    }

    fn editor_work(&mut self, work: Work, ctx: &egui::Context) {
        if !self.editor.settled()
            || self.receiver.is_some()
            || self.sharing_continuation.is_some()
            || self.fomod_pending()
            || self.sharing.closing
        {
            return;
        }
        let Some(guide) = self.guide.clone() else {
            return;
        };
        if matches!(
            work,
            Work::Project(_) | Work::PickProjection(_) | Work::Resources(_)
        ) {
            self.editor.projected.clear();
        }
        if matches!(work, Work::Resources(_)) {
            self.editor.inventory = None;
            self.editor.resource_trees.clear();
            self.editor.resource_selection.clear();
        }
        self.refresh_input_stamp();
        let context = self.input_stamp.clone();
        let input_path = self.loaded_input.clone();
        let mods = self.mods.clone();
        let game = self.game.clone();
        self.editor.run = self.editor.run.wrapping_add(1);
        let run = self.editor.run;
        self.editor.terminal_received = false;
        self.editor.status = if matches!(work, Work::Generate(_)) {
            "Generating an instruction preview… For local archive selection, cancelling or closing waits for the native file picker to be dismissed.".into()
        } else {
            "Working…".into()
        };
        self.cancelled = Arc::new(AtomicBool::new(false));
        let flag = self.cancelled.clone();
        let worker_cancelled = flag.clone();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.job = Job::Editor;
        let ctx = ctx.clone();
        let handle = std::thread::spawn(move || {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<Output> {
                    anyhow::ensure!(!flag.load(Ordering::Acquire), "Editor operation cancelled");
                    let output = match work {
                        Work::Easy(request) => {
                            return Ok(Output::Easy(Box::new(easy_mode::run(
                                *request,
                                &flag,
                                |message| {
                                    let _ = tx.send(Message::EditorProgress(
                                        run,
                                        context.clone(),
                                        message,
                                    ));
                                    ctx.request_repaint();
                                },
                            ))));
                        }
                        Work::Generate(request) => {
                            Output::Generated(Box::new(generation::run(*request, &flag)?))
                        }
                        Work::Import {
                            input,
                            location,
                            source,
                            format,
                            mode,
                            regex,
                        } => {
                            let text = match source {
                                0 => input,
                                1 => editor::read_document(std::path::Path::new(&location))?,
                                _ => {
                                    let temp = tempfile::tempdir()?;
                                    let path = temp.path().join("import.txt");
                                    modsync::protocol::fetch_instruction_with_cancel(
                                        &location, &path, &flag,
                                    )?;
                                    editor::read_document(&path)?
                                }
                            };
                            anyhow::ensure!(!flag.load(Ordering::Acquire), "Import cancelled");
                            if let Some(profile) = regex {
                                let result =
                                    editor::preview_regex_import(&guide, &text, &profile, mode)?;
                                let rendered = editor::render_preview(&result)?;
                                Output::Preview(result.preview, rendered)
                            } else {
                                let preview = editor::preview_import(
                                    &guide,
                                    &text,
                                    if format.trim().is_empty() {
                                        "auto"
                                    } else {
                                        &format
                                    },
                                    mode,
                                )?;
                                let rendered = editor::render_preview(&preview)?;
                                Output::Preview(preview, rendered)
                            }
                        }
                        Work::Resources(guid) => {
                            let roots = modsync::paths::Roots::new(
                                std::path::Path::new(&mods),
                                std::path::Path::new(&game),
                            )?;
                            let component = editor::find_component(&guide, &guid)
                                .context("Resource component disappeared")?;
                            let options = modsync::archive::Options {
                                max_expanded_bytes: Some(256 * 1024 * 1024),
                                ..Default::default()
                            };
                            let inventory =
                                editor::resource_inventory(component, &roots, &options, &flag)?;
                            let trees = resource_trees(&inventory, &flag)?;
                            Output::Resources(inventory, trees)
                        }
                        Work::PickImport => Output::ImportPath(
                            rfd::FileDialog::new()
                                .pick_file()
                                .context("File selection cancelled")?,
                        ),
                        Work::PickSave => Output::SavePath(
                            rfd::FileDialog::new()
                                .set_file_name("edited-guide.toml")
                                .save_file()
                                .context("Save path selection cancelled")?,
                        ),
                        Work::PickProjection(folder) => {
                            let dialog = rfd::FileDialog::new();
                            let paths = if folder {
                                vec![dialog.pick_folder().context("Path selection cancelled")?]
                            } else {
                                dialog.pick_files().context("Path selection cancelled")?
                            };
                            let roots = modsync::paths::Roots::new(
                                std::path::Path::new(&mods),
                                std::path::Path::new(&game),
                            )?;
                            Output::Projected(project_paths(&roots, paths, &flag)?)
                        }
                        Work::Project(paths) => {
                            let roots = modsync::paths::Roots::new(
                                std::path::Path::new(&mods),
                                std::path::Path::new(&game),
                            )?;
                            Output::Projected(project_paths(&roots, paths, &flag)?)
                        }
                        Work::Save(path) => {
                            editor::validate_guide(&guide)?;
                            anyhow::ensure!(
                                !flag.load(Ordering::Acquire),
                                "Save cancelled before publication"
                            );
                            if let Some(input) = &input_path {
                                editor::save_distinct(&guide, input, &path)?;
                            } else {
                                editor::save_validated(&guide, &path)?;
                            }
                            return Ok(Output::Saved(path));
                        }
                    };
                    anyhow::ensure!(!flag.load(Ordering::Acquire), "Editor operation cancelled");
                    Ok(output)
                }))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Editor worker stopped unexpectedly")));
            let _ = tx.send(Message::EditorTerminal(Box::new(Finished {
                run,
                context,
                result,
            })));
            ctx.request_repaint();
        });
        self.editor.worker = Some(EditorWorker {
            cancelled: worker_cancelled,
            handle: Some(handle),
        });
    }

    pub(super) fn editor_terminal(&mut self, result: Finished) {
        if result.run != self.editor.run {
            return;
        }
        self.editor.terminal_received = true;
        self.receiver = None;
        if let Ok(Output::Easy(outcome)) = result.result {
            self.easy_terminal(*outcome, &result.context);
            return;
        }
        self.refresh_input_stamp();
        if result.context != self.input_stamp {
            self.editor.status = match result.result {
                Ok(Output::Saved(path)) => format!(
                    "Captured guide saved to {}; current inputs changed.",
                    path.display()
                ),
                Err(error) => format!("Previous context operation stopped: {error:#}"),
                _ => "Inputs changed; editor result discarded.".into(),
            };
            return;
        }
        match result.result {
            Ok(Output::Easy(_)) => unreachable!("Easy result handled above"),
            Ok(Output::Generated(preview)) => {
                let current = self
                    .guide
                    .as_ref()
                    .context("Guide missing")
                    .and_then(|guide| {
                        generation::fingerprint(
                            &self.editor,
                            guide,
                            &self.input_stamp,
                            &self.mods,
                            self.acquisition_key().as_deref(),
                            self.probe.is_none(),
                        )
                    });
                if !self.cancelled.load(Ordering::Acquire)
                    && current
                        .as_ref()
                        .is_ok_and(|stamp| stamp == &preview.fingerprint)
                {
                    self.editor.generation.preview = Some(*preview);
                    self.editor.status =
                        "Instruction candidate ready. Review it before adopting into the draft."
                            .into();
                } else {
                    self.editor.generation.preview = None;
                    self.editor.status =
                        "Generation cancelled or draft changed; candidate discarded.".into();
                }
            }
            Ok(Output::Preview(preview, text)) => {
                self.editor.preview = Some(preview);
                self.editor.preview_text = text;
                self.editor.status =
                    "Preview ready. Inspect its operations and warnings before applying.".into();
            }
            Ok(Output::Resources(resources, trees)) => {
                self.editor.browser_context = self.input_stamp.clone();
                self.editor.resource_selection.clear();
                self.editor.projected.clear();
                self.editor.resource_trees = trees;
                self.editor.inventory = Some(resources);
                self.editor.status = "Resource inventory ready; no archive was extracted.".into();
            }
            Ok(Output::ImportPath(path)) => {
                self.editor.location = path.to_string_lossy().into_owned();
                self.editor.source = 1;
                self.editor.status = "File selected; preview before applying.".into();
            }
            Ok(Output::SavePath(path)) => {
                self.editor.save_path = path.to_string_lossy().into_owned();
                self.editor.status =
                    "Save path selected; save the validated guide explicitly.".into();
            }
            Ok(Output::Projected(paths)) => {
                self.editor.browser_context = self.input_stamp.clone();
                self.editor.projected = paths;
                self.editor.status =
                    "Variable path ready for explicit insertion into the draft.".into();
            }
            Ok(Output::Saved(path)) => {
                self.editor.status = format!("Validated guide saved to {}", path.display());
                self.downloads.unsaved_records = false;
                if let Some(guide) = &self.guide {
                    self.sharing.promote(guide, path);
                }
            }
            Err(error) => {
                self.editor.status = format!("Editor operation stopped: {error:#}");
                self.editor.preview = None;
                self.editor.generation.preview = None;
            }
        }
    }

    pub(super) fn editor_content(&mut self, ui: &mut egui::Ui) {
        ui.heading("Guide editor");
        ui.label("Draft changes stay separate until Apply validates the whole guide.");
        if !ui.is_enabled()
            || !self.editor.settled()
            || self.desktop.busy()
            || self.receiver.is_some()
            || self.sharing_continuation.is_some()
            || self.fomod_pending()
            || self.sharing.closing
        {
            ui.label(&self.editor.status);
            return;
        }
        if !self.sharing.edit_gate(ui) {
            return;
        }
        if self.easy_content(ui) {
            return;
        }
        let Some(guide) = &self.guide else {
            self.editor.removal = None;
            ui.label("Load a guide first.");
            return;
        };
        if let Some(mut pending) = self.editor.removal.take() {
            let valid = modsync::updates::guide_identity(guide)
                .is_ok_and(|identity| identity == pending.identity)
                && pending.context == self.input_stamp
                && editor::find_component(guide, &pending.guid).is_some();
            if !valid {
                self.editor.status =
                    "Removal cancelled because the guide or inputs changed.".into();
                self.editor.restore_component_focus = true;
                return;
            }
            let mut confirm = false;
            let mut cancel = false;
            let modal = egui::Modal::new(egui::Id::new("editor_remove_component"))
                .show(ui.ctx(), |ui| {
                    ui.heading("Remove component or option?");
                    ui.label(format!("Remove {} [{}] from the guide?", pending.name, pending.guid));
                    ui.label("Unsaved edits for this item will be discarded. The shared guide validator must accept the removal before it is applied.");
                    ui.horizontal(|ui| {
                        let response = ui.button("Cancel");
                        if !pending.shown { response.request_focus(); }
                        cancel = response.clicked();
                        confirm = ui.button("Delete").clicked();
                    });
                });
            pending.shown = true;
            if cancel || modal.should_close() {
                self.editor.status = "Removal cancelled.".into();
                self.editor.restore_component_focus = true;
            } else if confirm {
                // Modal Areas do not inherit a disabled parent Ui. Keep the
                // captured request pending until every mutation gate is ready.
                if !ui.is_enabled()
                    || self.desktop.busy()
                    || self.receiver.is_some()
                    || self.sharing_continuation.is_some()
                    || self.fomod_pending()
                    || self.sharing.closing
                    || !self.sharing.ready()
                {
                    self.editor.removal = Some(pending);
                    return;
                }
                // The modal holds no mutation authority. Recheck immediately
                // before entering the same transaction used by ordinary edits.
                let result = (|| -> Result<()> {
                    let guide = self.guide.as_ref().context("Guide missing")?;
                    anyhow::ensure!(
                        modsync::updates::guide_identity(guide)? == pending.identity
                            && self.input_stamp == pending.context,
                        "Guide or inputs changed; confirm removal again"
                    );
                    self.editor_apply(
                        &[EditOperation::RemoveComponent { guid: pending.guid }],
                        ui.ctx(),
                    )
                })();
                if let Err(error) = result {
                    self.editor.status = format!("Removal rejected: {error:#}");
                }
                self.editor.restore_component_focus = true;
            } else {
                self.editor.removal = Some(pending);
            }
            return;
        }
        let mut entries = Vec::new();
        rows(&guide.components, None, 0, &mut entries);
        let import_stamp = format!(
            "{:?}",
            (
                &self.editor.input,
                &self.editor.location,
                self.editor.source,
                &self.editor.format,
                self.editor.replace,
                self.editor.regex_mode,
                &self.editor.pattern,
                &self.editor.field_patterns,
                &self.editor.flags,
                &self.editor.guid
            )
        );
        if self.editor.import_stamp != import_stamp {
            self.editor.preview = None;
            self.editor.import_stamp = import_stamp.clone();
        }
        ui.horizontal(|ui| {
            if ui.button("Expand All").clicked() {
                self.editor.sections.set_all(true);
            }
            if ui.button("Collapse All").clicked() {
                self.editor.sections.set_all(false);
            }
        });
        let mut selected = None;
        let mut operations = None;
        let mut remove = None;
        let mut work = None;
        let mut commit = false;
        let mut apply_import = false;
        let keyboard = ui.button("Component list keyboard controls")
            .on_hover_text("Focus here: Up/Down selects a row; Ctrl+Up/Down reorders within its parent; Delete removes; Space toggles a top-level component through dependency selection.");
        if self.editor.restore_component_focus {
            keyboard.request_focus();
            self.editor.restore_component_focus = false;
        }
        if keyboard.has_focus() && !entries.is_empty() {
            let index = entries
                .iter()
                .position(|row| row.0 == self.editor.guid)
                .unwrap_or(0);
            let (guid, _, parent, position, count) = &entries[index];
            let control = ui.input(|input| input.modifiers.ctrl);
            if ui.input_mut(|input| {
                input.consume_key(
                    if control {
                        egui::Modifiers::CTRL
                    } else {
                        egui::Modifiers::NONE
                    },
                    egui::Key::ArrowUp,
                )
            }) {
                if control && *position > 0 {
                    operations = Some(vec![EditOperation::MoveComponent {
                        guid: guid.clone(),
                        index: position - 1,
                    }]);
                } else if !control && index > 0 {
                    selected = Some(entries[index - 1].0.clone());
                }
            }
            if ui.input_mut(|input| {
                input.consume_key(
                    if control {
                        egui::Modifiers::CTRL
                    } else {
                        egui::Modifiers::NONE
                    },
                    egui::Key::ArrowDown,
                )
            }) {
                if control && position + 1 < *count {
                    operations = Some(vec![EditOperation::MoveComponent {
                        guid: guid.clone(),
                        index: position + 1,
                    }]);
                } else if !control && index + 1 < entries.len() {
                    selected = Some(entries[index + 1].0.clone());
                }
            }
            if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)) {
                remove = Some(guid.clone());
            }
            if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Space)) {
                if parent.is_none() {
                    let mut components = guide.components.clone();
                    let selected = !components[*position].is_selected;
                    modsync::selection::set_component_selected(
                        &mut components,
                        *position,
                        selected,
                    );
                    operations = Some(
                        components
                            .into_iter()
                            .map(|component| EditOperation::ReplaceComponent {
                                guid: component.guid.clone(),
                                component: Box::new(component),
                            })
                            .collect(),
                    );
                } else {
                    self.editor.status =
                        "Use the component options controls to toggle nested options.".into();
                }
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Add component").clicked() {
                operations = Some(vec![EditOperation::InsertComponent {
                    parent: None,
                    index: guide.components.len(),
                    component: Box::new(Component {
                        guid: uuid::Uuid::new_v4().to_string(),
                        name: "New component".into(),
                        ..Default::default()
                    }),
                }]);
            }
            if ui.button("Choose save path…").clicked() {
                work = Some(Work::PickSave);
            }
            ui.label("Save to");
            ui.text_edit_singleline(&mut self.editor.save_path);
            if ui.button("Save validated guide").clicked() {
                work = Some(Work::Save(PathBuf::from(&self.editor.save_path)));
            }
        });
        egui::ComboBox::from_id_salt("transaction_component")
            .selected_text(if self.editor.guid.is_empty() {
                "Select component or option"
            } else {
                &self.editor.guid
            })
            .show_ui(ui, |ui| {
                for (guid, name, _, _, _) in &entries {
                    if ui
                        .selectable_label(*guid == self.editor.guid, name)
                        .clicked()
                    {
                        selected = Some(guid.clone());
                    }
                }
            });
        if let Some((guid, _, _, index, len)) = entries
            .iter()
            .find(|(guid, _, _, _, _)| *guid == self.editor.guid)
        {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(*index > 0, egui::Button::new("Move up"))
                    .clicked()
                {
                    operations = Some(vec![EditOperation::MoveComponent {
                        guid: guid.clone(),
                        index: index - 1,
                    }]);
                }
                if ui
                    .add_enabled(index + 1 < *len, egui::Button::new("Move down"))
                    .clicked()
                {
                    operations = Some(vec![EditOperation::MoveComponent {
                        guid: guid.clone(),
                        index: index + 1,
                    }]);
                }
                if ui.button("Remove selected").clicked() {
                    remove = Some(guid.clone());
                }
                if ui.button("Reload draft").clicked() {
                    selected = Some(guid.clone());
                }
            });
        }
        if section(ui, "Options", &mut self.editor.sections.options)
            && let Some(component) = editor::find_component(guide, &self.editor.guid)
        {
            let guid = &component.guid;
            if ui.button("Add option").clicked() {
                let count = editor::find_component(guide, guid)
                    .map_or(0, |component| component.options.len());
                operations = Some(vec![EditOperation::InsertComponent {
                    parent: Some(guid.clone()),
                    index: count,
                    component: Box::new(Component {
                        guid: uuid::Uuid::new_v4().to_string(),
                        name: "New option".into(),
                        ..Default::default()
                    }),
                }]);
            }
            for option in &component.options {
                if ui.button(format!("Edit option: {}", option.name)).clicked() {
                    selected = Some(option.guid.clone());
                }
            }
            if component.options.is_empty() {
                ui.label("No options for this component.");
            }
        }
        if let Some(component) = editor::find_component(guide, &self.editor.guid)
            && component.metadata.contains_key("DraftReview")
        {
            ui.collapsing("Resolve generated-instruction review", |ui| {
                ui.label("Apply instruction and option edits first. This review binds only the currently applied component, preserves its original diagnostics, and requires an explicit decision for every gap, condition and rejection.");
                match (modsync::draft::review::identity(component), modsync::draft::review::original_diagnostics(component)) {
                    (Ok(identity), Ok(original)) if original.unparsed_gaps.len().saturating_add(original.conditional_drafts.len()).saturating_add(original.rejected_drafts) <= 4096 => {
                        if self.editor.resolution.as_ref().is_none_or(|resolution| resolution.expected_identity != identity) {
                            self.editor.resolution = Some(modsync::draft::review::Resolution { expected_identity: identity, gap_decisions: vec![String::new(); original.unparsed_gaps.len()], condition_decisions: vec![String::new(); original.conditional_drafts.len()], rejected_decisions: vec![String::new(); original.rejected_drafts], review_note: String::new() });
                        }
                        let resolution = self.editor.resolution.as_mut().expect("initialized review");
                        for (label, diagnostics, decisions) in [("Gap", &original.unparsed_gaps, &mut resolution.gap_decisions), ("Condition", &original.conditional_drafts, &mut resolution.condition_decisions)] {
                            for (index, (diagnostic, decision)) in diagnostics.iter().zip(decisions).enumerate() { ui.push_id((label,index), |ui| { ui.label(format!("{label} {}: {diagnostic}", index + 1)); ui.add(egui::TextEdit::multiline(decision).char_limit(4096).hint_text("Explain the applied resolution")); }); }
                        }
                        for (index, decision) in resolution.rejected_decisions.iter_mut().enumerate() { ui.label(format!("Rejected instruction {} decision",index+1)); ui.add(egui::TextEdit::multiline(decision).char_limit(4096)); }
                        ui.label("Final review note"); ui.add(egui::TextEdit::multiline(&mut resolution.review_note).char_limit(4096));
                        if ui.button("Resolve review for this applied component").clicked() { operations=Some(vec![EditOperation::ResolveDraft { component: component.guid.clone(), resolution: resolution.clone() }]); }
                    }
                    (Err(error), _) | (_, Err(error)) => { ui.colored_label(egui::Color32::RED, format!("Review metadata invalid: {error:#}")); }
                    _ => { ui.colored_label(egui::Color32::RED, "Diagnostic count exceeds the review limit."); }
                }
            });
        }
        if let Some(draft) = &mut self.editor.draft {
            if section(ui, "Basic information", &mut self.editor.sections.basic) {
                egui::Grid::new("transaction_fields")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (label, value) in [
                            ("GUID", &mut draft.guid),
                            ("Name", &mut draft.name),
                            ("Author", &mut draft.author),
                            ("Tier", &mut draft.tier),
                        ] {
                            ui.label(label);
                            ui.text_edit_singleline(value);
                            ui.end_row();
                        }
                    });
                ui.checkbox(&mut draft.is_selected, "Selected");
            }
            if section(ui, "Description", &mut self.editor.sections.description) {
                ui.label("Description");
                ui.text_edit_multiline(&mut draft.description);
                ui.label("Directions");
                ui.text_edit_multiline(&mut draft.directions);
            }
            if section(ui, "Dependencies", &mut self.editor.sections.dependencies) {
                for (label, values) in [
                    ("Categories", &mut draft.category),
                    ("Dependencies", &mut draft.dependencies),
                    ("Restrictions", &mut draft.restrictions),
                    ("Install before", &mut draft.install_before),
                    ("Install after", &mut draft.install_after),
                ] {
                    Self::edit_lines(ui, label, values);
                }
            }
            ui.collapsing("Extension metadata (JSON)", |ui| {
                ui.text_edit_multiline(&mut self.editor.metadata);
            });
            ui.collapsing("Resource URLs and registrations (JSON)", |ui| {
                ui.text_edit_multiline(&mut self.editor.resources);
            });
            if section(ui, "Instructions", &mut self.editor.sections.instructions) {
                let count = draft.instructions.len();
                if count > 0 {
                    self.editor.instruction = self.editor.instruction.min(count - 1);
                    egui::ComboBox::from_id_salt("transaction_instruction")
                        .selected_text(format!("Step {}", self.editor.instruction + 1))
                        .show_ui(ui, |ui| {
                            for index in 0..count {
                                ui.selectable_value(
                                    &mut self.editor.instruction,
                                    index,
                                    format!("Step {}", index + 1),
                                );
                            }
                        });
                    let index = self.editor.instruction;
                    let instruction = &mut draft.instructions[index];
                    for (label, text) in [
                        ("Action", &mut instruction.action),
                        ("GUID", &mut instruction.guid),
                        ("Parent", &mut instruction.parent),
                        ("Destination", &mut instruction.destination),
                        ("Arguments", &mut instruction.arguments),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(label);
                            ui.text_edit_singleline(text);
                        });
                    }
                    ui.checkbox(&mut instruction.overwrite, "Overwrite");
                    Self::edit_lines(ui, "Sources", &mut instruction.source);
                    Self::edit_lines(ui, "Condition dependencies", &mut instruction.dependencies);
                    Self::edit_lines(ui, "Condition restrictions", &mut instruction.restrictions);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(index > 0, egui::Button::new("Step up"))
                            .clicked()
                        {
                            draft.instructions.swap(index, index - 1);
                            self.editor.instruction -= 1;
                        }
                        if ui
                            .add_enabled(index + 1 < count, egui::Button::new("Step down"))
                            .clicked()
                        {
                            draft.instructions.swap(index, index + 1);
                            self.editor.instruction += 1;
                        }
                        if ui.button("Remove step from draft").clicked() {
                            draft.instructions.remove(index);
                        }
                    });
                }
                if ui.button("Add instruction to draft").clicked() {
                    match serde_json::from_value::<Instruction>(
                        serde_json::json!({"Action":"Unset"}),
                    ) {
                        Ok(instruction) => draft.instructions.push(instruction),
                        Err(error) => self.editor.status = error.to_string(),
                    }
                }
            }
            if ui.button("Apply component/option draft").clicked() {
                commit = true;
            }
        }
        ui.collapsing("Guide metadata", |ui| {
            if ui.button("Load guide metadata").clicked() {
                match editor::render_preview(&guide.metadata) {
                    Ok(text) => self.editor.guide_metadata = text,
                    Err(error) => self.editor.status = error.to_string(),
                }
            }
            ui.text_edit_multiline(&mut self.editor.guide_metadata);
            if ui.button("Apply guide metadata").clicked() {
                match serde_json::from_str::<std::collections::BTreeMap<String, serde_json::Value>>(
                    &self.editor.guide_metadata,
                ) {
                    Ok(values) => {
                        let mut edits: Vec<_> = guide
                            .metadata
                            .keys()
                            .filter(|key| !values.contains_key(*key))
                            .map(|key| EditOperation::RemoveGuideMetadata { key: key.clone() })
                            .collect();
                        edits.extend(
                            values
                                .into_iter()
                                .map(|(key, value)| EditOperation::SetGuideMetadata { key, value }),
                        );
                        operations = Some(edits);
                    }
                    Err(error) => self.editor.status = format!("Invalid metadata: {error}"),
                }
            }
        });
        ui.collapsing("Import with preview", |ui| {
            ui.horizontal(|ui| { ui.selectable_value(&mut self.editor.source, 0, "Raw text"); ui.selectable_value(&mut self.editor.source, 1, "Local file"); ui.selectable_value(&mut self.editor.source, 2, "Confirmed URL"); });
            if self.editor.source == 0 { ui.text_edit_multiline(&mut self.editor.input); } else { ui.label("File path or HTTPS document URL"); ui.text_edit_singleline(&mut self.editor.location); }
            if ui.button("Choose local import file…").clicked() { work = Some(Work::PickImport); }
            ui.checkbox(&mut self.editor.replace, "Replace selected component (otherwise append top-level)");
            ui.horizontal(|ui| { ui.label("Format (auto, json, toml, yaml, md)"); ui.text_edit_singleline(&mut self.editor.format); });
            egui::ComboBox::from_id_salt("regex_mode").selected_text(["Structured document", "Default Markdown", "Raw regex", "Individual field regex"][self.editor.regex_mode]).show_ui(ui, |ui| { for (index, title) in ["Structured document", "Default Markdown", "Raw regex", "Individual field regex"].iter().enumerate() { ui.selectable_value(&mut self.editor.regex_mode, index, *title); } });
            if ui.button("Reset to default Markdown parser").clicked() { self.editor.regex_mode = 1; self.editor.pattern.clear(); self.editor.field_patterns.clear(); self.editor.flags = Default::default(); }
            if self.editor.regex_mode > 1 {
                ui.label("Rust regex syntax; unsupported .NET constructs are rejected. Named captures identify fields.");
                ui.label("Raw/section pattern"); ui.text_edit_multiline(&mut self.editor.pattern);
                if self.editor.regex_mode == 3 { ui.label("Field-pattern map (JSON)"); ui.text_edit_multiline(&mut self.editor.field_patterns); }
                ui.horizontal(|ui| { ui.checkbox(&mut self.editor.flags.multiline, "Multiline"); ui.checkbox(&mut self.editor.flags.singleline, "Dot matches newline"); ui.checkbox(&mut self.editor.flags.ignore_case, "Ignore case"); ui.checkbox(&mut self.editor.flags.global, "All matches"); });
            }
            if ui.button(if self.editor.source == 2 { "Fetch URL and preview import" } else { "Preview import" }).clicked() {
                let profile = match self.editor.regex_mode {
                    0 => Ok(None), 1 => Ok(Some(RegexImportProfile::DefaultMarkdown)),
                    2 => Ok(Some(RegexImportProfile::Raw { pattern: self.editor.pattern.clone(), flags: self.editor.flags.clone() })),
                    _ => serde_json::from_str(&self.editor.field_patterns).map(|fields| Some(RegexImportProfile::Individual { section_pattern: self.editor.pattern.clone(), fields, flags: self.editor.flags.clone() })),
                };
                match profile {
                    Ok(regex) => {
                        let mode = if self.editor.replace { ImportMode::Replace { guid: self.editor.guid.clone() } } else { ImportMode::Append { parent: None, index: guide.components.len() } };
                        work = Some(Work::Import { input: self.editor.input.clone(), location: self.editor.location.clone(), source: self.editor.source, format: self.editor.format.clone(), mode, regex });
                    }
                    Err(error) => self.editor.status = format!("Invalid field patterns: {error}"),
                }
            }
            if self.editor.preview.is_some() {
                ui.label("Proposed operations, warnings, regex capture offsets and unmatched lines:");
                ui.add(egui::TextEdit::multiline(&mut self.editor.preview_text).interactive(false).desired_rows(12));
                if ui.button("Copy import preview").clicked() { ui.ctx().copy_text(self.editor.preview_text.clone()); }
                if ui.button("Confirm and apply import").clicked() { apply_import = true; }
            }
        });
        ui.collapsing("Registered resources and variable paths", |ui| {
            if self.editor.browser_context != self.input_stamp {
                self.editor.inventory = None;
                self.editor.resource_trees.clear();
                self.editor.resource_selection.clear();
                self.editor.projected.clear();
            }
            if ui.add_enabled(!self.editor.guid.is_empty(), egui::Button::new("Read registered resources")).clicked() {
                work = Some(Work::Resources(self.editor.guid.clone()));
            }
            ui.horizontal(|ui| {
                ui.label("Actual extraction directory for archive members");
                ui.text_edit_singleline(&mut self.editor.extraction_directory);
            });
            if self.editor.browser_extraction != self.editor.extraction_directory {
                self.editor.projected.clear();
                self.editor.browser_extraction = self.editor.extraction_directory.clone();
            }
            if let Some(inventory) = &self.editor.inventory {
                for (index, resource) in inventory.resources.iter().enumerate() {
                    ui.push_id(index, |ui| ui.collapsing(format!("{} — {}", resource.filename, if resource.enabled { "enabled" } else { "disabled" }), |ui| {
                        ui.label(&resource.url);
                        if let Some(error) = &resource.error { ui.colored_label(egui::Color32::RED, error); }
                        ui.add_enabled_ui(resource.error.is_none() && !resource.source.is_empty(), |ui| {
                            let key = (index, None);
                            let mut checked = self.editor.resource_selection.contains(&key);
                            if ui.checkbox(&mut checked, "Select registered file").changed() {
                                self.editor.projected.clear();
                                if checked && self.editor.resource_selection.len() < MAX_BROWSER_SELECTION { self.editor.resource_selection.insert(key); }
                                else if !checked { self.editor.resource_selection.remove(&key); }
                                else { self.editor.status = "Select at most 4096 browser entries.".into(); }
                            }
                            if let Some(tree) = self.editor.resource_trees.get(index)
                                && resource_tree_ui(ui, tree, index, &mut self.editor.resource_selection, &mut self.editor.status) { self.editor.projected.clear(); }
                        });
                    }));
                }
                ui.label(format!("{} files selected", self.editor.resource_selection.len()));
                if ui.button("Clear resource selection").clicked() { self.editor.resource_selection.clear(); self.editor.projected.clear(); }
                if ui.add_enabled(!self.editor.resource_selection.is_empty(), egui::Button::new("Project selected files")).clicked() {
                    let result = (|| -> Result<Vec<PathBuf>> {
                        let mut paths = Vec::new();
                        for (resource, member) in &self.editor.resource_selection {
                            let resource = inventory.resources.get(*resource).context("Resource selection changed")?;
                            anyhow::ensure!(resource.error.is_none(), "Resource cannot be selected after an inventory error");
                            paths.push(if let Some(member) = member {
                                anyhow::ensure!(resource.archive_entries.contains(member), "Archive member selection changed");
                                let extraction = PathBuf::from(&self.editor.extraction_directory);
                                anyhow::ensure!(extraction.is_absolute(), "Enter the actual absolute extraction directory before projecting archive members");
                                extraction.join(modsync::paths::literal_relative(member)?)
                            } else {
                                std::path::absolute(&self.mods)?.join(modsync::paths::relative(&resource.filename, false)?)
                            });
                        }
                        Ok(paths)
                    })();
                    match result { Ok(paths) => work = Some(Work::Project(paths)), Err(error) => self.editor.status = format!("Selection rejected: {error:#}") }
                }
            }
            ui.horizontal(|ui| {
                if ui.button("Browse source files…").clicked() { work = Some(Work::PickProjection(false)); }
                if ui.button("Browse source/destination folder…").clicked() { work = Some(Work::PickProjection(true)); }
            });
            ui.label("Absolute source/destination path under the current roots");
            if ui.text_edit_singleline(&mut self.editor.projection_path).changed() { self.editor.projected.clear(); }
            if ui.button("Project supported variable").clicked() { work = Some(Work::Project(vec![PathBuf::from(&self.editor.projection_path)])); }
            ui.label("Review projected paths, then explicitly insert into the draft. Apply component/option draft validates the complete guide.");
            for path in &self.editor.projected { ui.monospace(path); }
            if let Some(draft) = &mut self.editor.draft && let Some(instruction) = draft.instructions.get_mut(self.editor.instruction) {
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.editor.projected.is_empty(), egui::Button::new("Confirm and append selected draft sources")).clicked() {
                        let mut candidate = instruction.source.clone();
                        for path in &self.editor.projected { if !candidate.contains(path) { candidate.push(path.clone()); } }
                        if candidate.len() <= MAX_BROWSER_SELECTION { instruction.source = candidate; }
                        else { self.editor.status = "Draft source list exceeds 4096 entries; no sources inserted.".into(); }
                    }
                    if ui.add_enabled(self.editor.projected.len() == 1, egui::Button::new("Confirm single draft destination")).clicked() {
                        instruction.destination = self.editor.projected[0].clone();
                    }
                });
            }
        });
        ui.label(&self.editor.status);
        if let Some(guid) = selected
            && let Err(error) = self.editor_select(&guid)
        {
            self.editor.status = format!("{error:#}");
        }
        if commit {
            let result = (|| -> Result<Vec<EditOperation>> {
                anyhow::ensure!(
                    self.editor.identity
                        == modsync::updates::guide_identity(
                            self.guide.as_ref().context("Guide missing")?
                        )?,
                    "Guide changed; reload the draft before applying"
                );
                let mut draft = self.editor.draft.clone().context("No component draft")?;
                draft.resource_registry = serde_json::from_str(&self.editor.resources)?;
                draft.metadata = serde_json::from_str(&self.editor.metadata)?;
                Ok(vec![EditOperation::ReplaceComponent {
                    guid: self.editor.guid.clone(),
                    component: Box::new(draft),
                }])
            })();
            match result {
                Ok(edits) => operations = Some(edits),
                Err(error) => self.editor.status = format!("Draft rejected: {error:#}"),
            }
        }
        let current_import_stamp = format!(
            "{:?}",
            (
                &self.editor.input,
                &self.editor.location,
                self.editor.source,
                &self.editor.format,
                self.editor.replace,
                self.editor.regex_mode,
                &self.editor.pattern,
                &self.editor.field_patterns,
                &self.editor.flags,
                &self.editor.guid
            )
        );
        if current_import_stamp != import_stamp {
            self.editor.preview = None;
            self.editor.import_stamp = current_import_stamp;
            apply_import = false;
        }
        if apply_import {
            let result = self
                .guide
                .as_ref()
                .context("Guide missing")
                .and_then(|guide| {
                    editor::apply_preview(
                        guide,
                        self.editor.preview.as_ref().context("No import preview")?,
                    )
                });
            match result {
                Ok(_) => {
                    operations = self
                        .editor
                        .preview
                        .as_ref()
                        .map(|preview| preview.operations.clone());
                }
                Err(error) => self.editor.status = format!("Import rejected: {error:#}"),
            }
        }
        if let Some(guid) = remove {
            let pending = (|| -> Result<PendingRemoval> {
                let guide = self.guide.as_ref().context("Guide missing")?;
                let component =
                    editor::find_component(guide, &guid).context("Component no longer exists")?;
                Ok(PendingRemoval {
                    name: component.name.clone(),
                    guid,
                    identity: modsync::updates::guide_identity(guide)?,
                    context: self.input_stamp.clone(),
                    shown: false,
                })
            })();
            match pending {
                Ok(pending) => self.editor.removal = Some(pending),
                Err(error) => self.editor.status = format!("Removal unavailable: {error:#}"),
            }
            return;
        }
        if let Some(operations) = operations
            && let Err(error) = self.editor_apply(&operations, ui.ctx())
        {
            self.editor.status = format!("Edit rejected: {error:#}");
        }
        if work.is_none() && self.guide.is_some() {
            self.refresh_input_stamp();
            // All same-frame authoring actions have settled before fingerprinting.
            let key = self.acquisition_key();
            if let Some(guide) = &self.guide {
                work = generation::show(
                    ui,
                    &mut self.editor,
                    guide,
                    &self.input_stamp,
                    &self.mods,
                    key,
                    self.probe.is_none(),
                );
            }
        }
        if let Some(work) = work {
            self.editor_work(work, ui.ctx());
        }
    }
}
