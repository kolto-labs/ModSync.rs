//! Record ordinary authoring actions through the shared session and engine.
mod browser;
mod view;
use super::{App, EditOperation, Work};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use modsync::{
    Guide, Instruction, Settings, editor::easy::Session, engine::Prediction, paths::Roots,
};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Default)]
pub(super) struct State {
    pub session: Option<Session>,
    pub prediction: Option<Prediction>,
    filesystem: Option<modsync::editor::easy::disk::DiskHistory>,
    patcher_preview: Option<modsync::editor::easy::patcher::PatchPreview>,
    refresh_prediction: bool,
    enabled: bool,
    directory: String,
    component: usize,
    instructions: usize,
    style: usize,
    folder: PathBuf,
    roots: Option<Roots>,
    selected: BTreeSet<PathBuf>,
    search: String,
    action: usize,
    sources: String,
    destination: String,
    arguments: String,
    overwrite: bool,
    status: String,
    archive_source: String,
    pending_resume: Option<PathBuf>,
    saved_view: Option<Vec<u8>>,
}

pub(super) enum Action {
    ImportArchive(PathBuf),
    RecoverDisk,
    SeekDisk(usize),
    Apply {
        settings: Box<Settings>,
        component: usize,
        instruction: usize,
    },
    InspectPatcher {
        settings: Box<Settings>,
        component: usize,
        instructions: usize,
        source: String,
        namespace: Option<usize>,
    },
    Create(PathBuf),
    Open(PathBuf),
    Edit {
        label: String,
        operations: Vec<EditOperation>,
    },
    Seek(usize),
    Predict {
        settings: Box<Settings>,
        component: usize,
        instructions: usize,
    },
}

pub(super) struct Request {
    session: Option<Session>,
    action: Action,
    guide: Guide,
}

pub(in crate::ui) struct Outcome {
    filesystem: Option<modsync::editor::easy::disk::DiskHistory>,
    applied: bool,
    patcher_preview: Option<modsync::editor::easy::patcher::PatchPreview>,
    session: Option<Session>,
    prediction: Option<Prediction>,
    roots: Option<Roots>,
    changed: bool,
    opened: bool,
    error: Option<String>,
}

pub(super) fn run(
    request: Request,
    cancelled: &Arc<AtomicBool>,
    mut progress: impl FnMut(String),
) -> Outcome {
    progress(
        match &request.action {
            Action::ImportArchive(_) => "Importing the archive into this build…",
            Action::Predict { .. } => "Predicting the selected build point…",
            Action::InspectPatcher { .. } => "Reading patcher choices…",
            Action::Apply { .. } => "Preparing the next recorded disk action…",
            Action::SeekDisk(_) => "Restoring installed files from a saved checkpoint…",
            Action::RecoverDisk => "Recovering the pending recorded action…",
            Action::Create(_) => "Creating the authoring session…",
            Action::Open(_) => "Opening the saved authoring session…",
            Action::Edit { .. } => "Saving the accepted guide revision…",
            Action::Seek(_) => "Opening the saved guide revision…",
        }
        .into(),
    );
    let mut session = request.session;
    let mut prediction = None;
    let mut roots = None;
    let mut changed = false;
    let mut patcher_preview = None;
    let mut applied = false;
    let mut opened_session = false;
    let mut result = (|| -> Result<()> {
        match request.action {
            Action::ImportArchive(source) => {
                let session = session
                    .as_mut()
                    .context("Create or resume an authoring session first")?;
                ensure!(
                    modsync::updates::guide_identity(session.guide())?
                        == modsync::updates::guide_identity(&request.guide)?,
                    "Guide changed outside this session"
                );
                session.import_archive(&source, 8 * 1024 * 1024 * 1024, cancelled)?;
                changed = true;
            }
            Action::RecoverDisk => {
                progress("Recovering the recorded action's checkpoint".into());
                session
                    .as_mut()
                    .context("No authoring session")?
                    .recover_disk_action(cancelled)?;
                applied = true;
            }
            Action::SeekDisk(cursor) => {
                session
                    .as_mut()
                    .context("No authoring session")?
                    .seek_disk(cursor, cancelled, |update| progress(update.message))?;
                applied = true;
            }
            Action::Apply {
                mut settings,
                component,
                instruction,
            } => {
                settings.cancelled = cancelled.clone();
                roots = Some(settings.roots.clone());
                let session = session.as_mut().context("No authoring session")?;
                ensure!(
                    modsync::updates::guide_identity(session.guide())?
                        == modsync::updates::guide_identity(&request.guide)?,
                    "Guide changed outside this session"
                );
                session.apply_to_disk(
                    *settings,
                    component,
                    instruction,
                    |_| {},
                    |update| progress(update.message),
                )?;
                applied = true;
            }
            Action::InspectPatcher {
                mut settings,
                component,
                instructions,
                source,
                namespace,
            } => {
                settings.cancelled = cancelled.clone();
                let session = session.as_ref().context("No authoring session")?;
                ensure!(
                    modsync::updates::guide_identity(session.guide())?
                        == modsync::updates::guide_identity(&request.guide)?,
                    "Guide changed outside this session"
                );
                patcher_preview = Some(session.inspect_patcher(
                    *settings,
                    component,
                    instructions,
                    &source,
                    namespace,
                )?);
            }
            Action::Create(directory) => {
                modsync::editor::validate_guide(&request.guide)?;
                session = Some(Session::create(&directory, &request.guide)?);
                changed = true;
            }
            Action::Open(directory) => {
                let opened = Session::open(&directory)?;
                modsync::editor::validate_guide(opened.guide())?;
                session = Some(opened);
                changed = true;
                // The opened session owns its saved UI context.
                opened_session = true;
            }
            Action::Edit { label, operations } => {
                let session = session
                    .as_mut()
                    .context("Create or resume an authoring session first")?;
                ensure!(
                    modsync::updates::guide_identity(session.guide())?
                        == modsync::updates::guide_identity(&request.guide)?,
                    "Guide changed outside this session; resume it or start a new session"
                );
                session.edit(&label, &operations)?;
                changed = true;
            }
            Action::Seek(cursor) => {
                session
                    .as_mut()
                    .context("No authoring session")?
                    .seek(cursor)?;
                changed = true;
            }
            Action::Predict {
                mut settings,
                component,
                instructions,
            } => {
                settings.cancelled = cancelled.clone();
                roots = Some(settings.roots.clone());
                let session = session.as_ref().context("No authoring session")?;
                ensure!(
                    modsync::updates::guide_identity(session.guide())?
                        == modsync::updates::guide_identity(&request.guide)?,
                    "Guide changed outside this session"
                );
                prediction = Some(session.predict(*settings, component, instructions)?);
            }
        }
        Ok(())
    })();
    let filesystem = match session.as_ref().map(Session::disk_history).transpose() {
        Ok(history) => history.flatten(),
        Err(error) => {
            result = Err(error);
            None
        }
    };
    Outcome {
        filesystem,
        applied,
        patcher_preview,
        session,
        prediction,
        roots,
        changed,
        opened: opened_session,
        error: result.err().map(|error| format!("{error:#}")),
    }
}

fn archive_settings(mut settings: Settings, scratch: &str, budget: &str) -> Result<Settings> {
    settings.archive.scratch_dir = (!scratch.is_empty()).then(|| PathBuf::from(scratch));
    settings.archive.max_expanded_bytes = if budget.trim().is_empty() {
        None
    } else {
        Some(
            budget
                .trim()
                .parse()
                .context("Expanded archive budget must be an integer byte count")?,
        )
    };
    Ok(settings)
}

impl App {
    pub(in crate::ui) fn easy_progress(&mut self, run: u64, context: &str, text: String) {
        if self.editor.run == run && self.input_stamp == context && !self.editor.terminal_received {
            self.editor.status = text.clone();
            self.editor.easy.status = text;
        }
    }
    pub(in crate::ui) fn poll_easy_prediction(&mut self, ctx: &egui::Context) {
        if !self.editor.easy.enabled
            || !self.editor.easy.refresh_prediction
            || self.editor.easy.session.is_none()
            || !self.editor.settled()
            || self.receiver.is_some()
            || self.desktop.busy()
            || self.sharing_continuation.is_some()
            || self.sharing.closing
            || self.fomod_pending()
            || !self.sharing.ready()
        {
            return;
        }
        self.editor.easy.refresh_prediction = false;
        if self
            .guide
            .as_ref()
            .is_none_or(|guide| guide.components.is_empty())
        {
            return;
        }
        let result = Settings::new(
            std::path::Path::new(&self.mods),
            std::path::Path::new(&self.game),
        )
        .and_then(|settings| archive_settings(settings, &self.scratch, &self.expanded_budget))
        .and_then(|settings| {
            self.easy_work(
                Action::Predict {
                    settings: Box::new(settings),
                    component: self.editor.easy.component,
                    instructions: self.editor.easy.instructions,
                },
                ctx,
            )
        });
        if let Err(error) = result {
            self.editor.easy.status = format!("Prediction stopped: {error:#}");
        }
    }

    pub(super) fn easy_work(&mut self, action: Action, ctx: &egui::Context) -> Result<()> {
        ensure!(
            self.editor.settled()
                && self.receiver.is_none()
                && self.sharing_continuation.is_none()
                && !self.sharing.closing
                && !self.fomod_pending()
                && !self.desktop.busy(),
            "An editor operation is still running"
        );
        let guide = match self.guide.clone() {
            Some(guide) => guide,
            None if matches!(&action, Action::Open(_)) => Guide::default(),
            None => anyhow::bail!("Load a guide first"),
        };
        self.persist_easy_view();
        let inspect_patcher = matches!(action, Action::InspectPatcher { .. });
        let request = Request {
            session: self.editor.easy.session.take(),
            action,
            guide,
        };
        if !inspect_patcher {
            self.editor.easy.prediction = None;
        }
        self.editor.easy.patcher_preview = None;
        self.editor.easy.selected.clear();
        self.editor_work(Work::Easy(Box::new(request)), ctx);
        Ok(())
    }

    pub(super) fn easy_terminal(&mut self, outcome: Outcome, context: &str) {
        self.editor.easy.filesystem = outcome.filesystem;
        self.editor.easy.session = outcome.session;
        self.refresh_input_stamp();
        if context != self.input_stamp {
            self.editor.easy.status = "Inputs changed. The saved session is retained; resume it to recover the accepted revision.".into();
            self.editor.easy.prediction = None;
            return;
        }
        if outcome.applied {
            self.invalidate_validation();
            self.editor.easy.refresh_prediction = true;
        }
        if outcome.changed {
            self.editor.easy.patcher_preview = None;
            self.editor.easy.refresh_prediction = true;
            if let Some(session) = &self.editor.easy.session {
                let guide = session.guide().clone();
                self.editor_accept_guide(guide);
                if let Some(component) = self
                    .guide
                    .as_ref()
                    .and_then(|guide| guide.components.get(self.editor.easy.component))
                {
                    self.editor.easy.instructions = component.instructions.len();
                }
            }
        }
        if outcome.opened {
            self.restore_easy_view();
        }
        if let Some(roots) = outcome.roots {
            if !self.editor.easy.folder.starts_with(&roots.game)
                && !self.editor.easy.folder.starts_with(&roots.mods)
            {
                self.editor.easy.folder = roots.game.clone();
            }
            self.editor.easy.roots = Some(roots);
        }
        if outcome.changed || outcome.prediction.is_some() {
            self.editor.easy.prediction = outcome.prediction;
        }
        let patcher_ready = outcome.patcher_preview.is_some();
        if let Some(preview) = outcome.patcher_preview {
            self.editor.easy.arguments = preview
                .selected
                .map_or_else(String::new, |index| index.to_string());
            self.editor.easy.patcher_preview = Some(preview);
        }
        self.editor.easy.status = outcome.error.unwrap_or_else(|| {
            if outcome.applied { "Action applied to disk and checkpointed. The saved guide is retained.".into() }
            else if patcher_ready { "Patcher options and summary ready; no mod was installed.".into() }
            else if outcome.changed { "Saved automatically. Undo/redo history is ready.".into() }
            else { "Predicted files are ready. Patcher edits with unknown content are marked explicitly.".into() }
        });
        self.persist_easy_view();
    }

    /// Return true while easy mode owns the editor surface.
    pub(super) fn easy_content(&mut self, ui: &mut egui::Ui) -> bool {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.editor.easy.enabled, true, "Easy mode");
            ui.selectable_value(&mut self.editor.easy.enabled, false, "Advanced editor");
        });
        if !self.editor.easy.enabled {
            return false;
        }
        let Some(guide) = self.guide.as_ref() else {
            ui.label("Load a guide to begin recording your build.");
            return true;
        };
        let mut action = None;
        if self.editor.easy.directory.is_empty() {
            self.editor.easy.directory = PathBuf::from(&self.profile_directory)
                .join("authoring")
                .join(uuid::Uuid::new_v4().to_string())
                .to_string_lossy()
                .into_owned();
        }
        let session_label = ui.label("Session folder");
        ui.add(
            egui::TextEdit::singleline(&mut self.editor.easy.directory)
                .id_source("easy_session_folder")
                .desired_width(f32::INFINITY),
        )
        .labelled_by(session_label.id);
        ui.horizontal(|ui| {
            if ui.button("Start new session").clicked() {
                action = Some(Action::Create(PathBuf::from(&self.editor.easy.directory)));
            }
            if ui.button("Resume saved session").clicked() {
                action = Some(
                    if let Some(session) = &self.editor.easy.session
                        && std::path::Path::new(&self.editor.easy.directory) == session.directory()
                    {
                        Action::Seek(session.cursor())
                    } else {
                        Action::Open(PathBuf::from(&self.editor.easy.directory))
                    },
                );
            }
            if let Some(session) = &self.editor.easy.session {
                if ui
                    .add_enabled(session.can_undo(), egui::Button::new("Undo"))
                    .clicked()
                {
                    action = Some(Action::Seek(session.cursor() - 1));
                }
                if ui
                    .add_enabled(session.can_redo(), egui::Button::new("Redo"))
                    .clicked()
                {
                    action = Some(Action::Seek(session.cursor() + 1));
                }
                ui.label(format!(
                    "Saved · revision {} of {}",
                    session.cursor() + 1,
                    session.revisions().len()
                ));
            }
        });
        ui.label(&self.editor.easy.status);
        let can_import = self.editor.easy.session.is_some()
            && self
                .editor
                .easy
                .filesystem
                .as_ref()
                .is_none_or(|history| history.pending.is_none() && history.restoring.is_none());
        ui.add_enabled_ui(can_import, |ui| {
            ui.horizontal(|ui| {
                let label = ui.label("Archive to import");
                ui.add(egui::TextEdit::singleline(&mut self.editor.easy.archive_source).id_source("easy_archive_source").desired_width(360.0)).labelled_by(label.id);
                if ui.button("Choose archive…").clicked()
                    && let Some(path) = rfd::FileDialog::new().add_filter("Mod archives", &["zip", "7z", "rar", "tar", "gz", "tgz", "exe"]).pick_file()
                {
                    self.editor.easy.archive_source = path.to_string_lossy().into_owned();
                }
                if ui.add_enabled(!self.editor.easy.archive_source.trim().is_empty(), egui::Button::new("Import archive")).on_hover_text("Add this archive to the build without changing the game or mod folders. Undo removes it from predicted files.").clicked() {
                    action = Some(Action::ImportArchive(PathBuf::from(&self.editor.easy.archive_source)));
                }
            });
        });
        if let Some(history) = &self.editor.easy.filesystem {
            ui.horizontal(|ui| {
                let ready = history.pending.is_none() && history.restoring.is_none();
                if ui
                    .add_enabled(
                        ready && history.cursor > 0,
                        egui::Button::new("Undo installed action"),
                    )
                    .clicked()
                {
                    action = Some(Action::SeekDisk(history.cursor - 1));
                }
                if ui
                    .add_enabled(
                        ready && history.cursor < history.actions.len(),
                        egui::Button::new("Redo installed action"),
                    )
                    .clicked()
                {
                    action = Some(Action::SeekDisk(history.cursor + 1));
                }
                ui.label(format!(
                    "Disk · action {} of {}",
                    history.cursor,
                    history.actions.len()
                ));
                if let Some(target) = history.restoring
                    && ui.button("Resume filesystem restore").clicked()
                {
                    action = Some(Action::SeekDisk(target));
                }
            });
            if let Some(pending) = &history.pending {
                ui.label(format!(
                    "Action recovery required: {}",
                    pending.definition.name
                ));
                if ui.button("Recover recorded action").clicked() {
                    action = Some(Action::RecoverDisk);
                }
            }
        }
        if self.editor.easy.session.is_some() {
            ui.separator();
            let state = &mut self.editor.easy;
            state.component = state
                .component
                .min(guide.components.len().saturating_sub(1));
            let old_component = state.component;
            egui::ComboBox::from_id_salt("easy_component")
                .selected_text(
                    guide
                        .components
                        .get(state.component)
                        .map_or("Choose a mod", |component| &component.name),
                )
                .show_ui(ui, |ui| {
                    for (index, component) in guide.components.iter().enumerate() {
                        ui.selectable_value(&mut state.component, index, &component.name);
                    }
                });
            if let Some(component) = guide.components.get(state.component) {
                let mut included = component.is_selected;
                if ui.checkbox(&mut included, "Include this mod").changed() {
                    let mut updated = component.clone();
                    updated.is_selected = included;
                    action = Some(Action::Edit {
                        label: format!("{} · selection", component.name),
                        operations: vec![EditOperation::ReplaceComponent {
                            guid: component.guid.clone(),
                            component: Box::new(updated),
                        }],
                    });
                }
                if old_component != state.component {
                    state.patcher_preview = None;
                    state.instructions = component.instructions.len();
                    state.prediction = None;
                    state.selected.clear();
                    state.refresh_prediction = true;
                }
                egui::CollapsingHeader::new("Guide instructions")
                    .id_salt(("easy_directions", &component.guid))
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label(&component.directions);
                        ui.label(&component.description);
                    });
                state.instructions = state.instructions.min(component.instructions.len());
                let changed = ui
                    .add(
                        egui::Slider::new(
                            &mut state.instructions,
                            0..=component.instructions.len(),
                        )
                        .text("After action"),
                    )
                    .changed();
                if changed {
                    state.patcher_preview = None;
                    state.prediction = None;
                    state.selected.clear();
                    state.refresh_prediction = true;
                }
                if ui.button("Show predicted files here").on_hover_text("Simulates earlier selected mods and this mod through the chosen action without installing anything.").clicked() {
                    let result = Settings::new(std::path::Path::new(&self.mods), std::path::Path::new(&self.game))
                        .and_then(|settings| archive_settings(settings, &self.scratch, &self.expanded_budget));
                    match result {
                        Ok(settings) => action = Some(Action::Predict { settings: Box::new(settings), component: state.component, instructions: state.instructions }),
                        Err(error) => state.status = format!("Check game and mod folders: {error:#}"),
                    }
                }
                if ui.add_enabled(state.instructions > 0, egui::Button::new("Apply this action to disk"))
                    .on_hover_text("Applies only this recorded action to the configured folders, in guide order. Saves before/after checkpoints and stops on failure.").clicked() {
                    match Settings::new(std::path::Path::new(&self.mods), std::path::Path::new(&self.game))
                        .and_then(|settings| archive_settings(settings, &self.scratch, &self.expanded_budget)) {
                        Ok(mut settings) => {
                            settings.allow_processes = self.allow_processes;
                            settings.patcher_engine = self.patcher_engine;
                            settings.patcher = (!self.patcher.is_empty()).then(|| PathBuf::from(&self.patcher));
                            settings.archive.scratch_dir = (!self.scratch.is_empty()).then(|| PathBuf::from(&self.scratch));
                            action = Some(Action::Apply { settings: Box::new(settings), component: state.component, instruction: state.instructions - 1 });
                        }
                        Err(error) => state.status = format!("Check game and mod folders: {error:#}"),
                    }
                }
                browser::show(ui, state);
                let selected: Vec<_> = state
                    .selected
                    .iter()
                    .filter_map(|path| {
                        state
                            .roots
                            .as_ref()
                            .and_then(|roots| modsync::editor::variable_path(roots, path).ok())
                    })
                    .collect();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !selected.is_empty(),
                            egui::Button::new("Use selected files"),
                        )
                        .clicked()
                    {
                        state.sources = selected.join("\n");
                    }
                    if ui
                        .add_enabled(
                            state.roots.is_some(),
                            egui::Button::new("Use current folder as destination"),
                        )
                        .clicked()
                        && let Some(roots) = &state.roots
                    {
                        match modsync::editor::variable_path(roots, &state.folder) {
                            Ok(path) => state.destination = path,
                            Err(error) => state.status = format!("Destination rejected: {error:#}"),
                        }
                    }
                });
                ui.separator();
                ui.strong("Record an action");
                const ACTIONS: [&str; 9] = [
                    "Copy",
                    "Move",
                    "Rename",
                    "Delete",
                    "Extract",
                    "Patcher",
                    "DelDuplicate",
                    "CleanList",
                    "Choose",
                ];
                egui::ComboBox::from_id_salt("easy_action")
                    .selected_text(ACTIONS[state.action])
                    .show_ui(ui, |ui| {
                        for (index, label) in ACTIONS.iter().enumerate() {
                            ui.selectable_value(&mut state.action, index, *label);
                        }
                    });
                let source_label = ui.label("Source files or folders (one per line)");
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut state.sources)
                            .id_source("easy_sources")
                            .desired_width(f32::INFINITY),
                    )
                    .labelled_by(source_label.id)
                    .changed()
                {
                    state.patcher_preview = None;
                }
                let destination_label = ui.label("Destination folder or new name");
                ui.add(
                    egui::TextEdit::singleline(&mut state.destination)
                        .id_source("easy_destination")
                        .desired_width(f32::INFINITY),
                )
                .labelled_by(destination_label.id);
                if state.action == 5 {
                    let mut namespace = None;
                    let mut inspect = ui
                        .button("Inspect patcher at this point in the build")
                        .clicked();
                    if let Some(preview) = &state.patcher_preview {
                        ui.label(format!(
                            "{} {} · {}",
                            preview.backend, preview.version, preview.instructions
                        ));
                        if let Some(reason) = &preview.blocked_reason {
                            ui.label(reason);
                        }
                        if let Some(mut selected) = preview.selected {
                            let old = selected;
                            egui::ComboBox::from_id_salt("easy_patcher_namespace")
                                .selected_text(
                                    preview
                                        .options
                                        .get(selected)
                                        .map_or("Choose install option", |option| &option.name),
                                )
                                .show_ui(ui, |ui| {
                                    for option in &preview.options {
                                        ui.selectable_value(
                                            &mut selected,
                                            option.index,
                                            &option.name,
                                        )
                                        .on_hover_text(&option.description);
                                    }
                                });
                            if old != selected {
                                namespace = Some(selected);
                                inspect = true;
                            }
                        }
                        ui.collapsing("Patcher description", |ui| {
                            ui.label(&preview.description);
                        });
                        ui.collapsing("Planned patcher changes", |ui| {
                            ui.label(&preview.summary);
                        });
                    }
                    if inspect {
                        let source = state.sources.trim();
                        let result = Settings::new(
                            std::path::Path::new(&self.mods),
                            std::path::Path::new(&self.game),
                        )
                        .and_then(|settings| {
                            archive_settings(settings, &self.scratch, &self.expanded_budget)
                        });
                        if source.is_empty() || source.lines().count() != 1 {
                            state.status = "Select one patcher file or folder.".into();
                        } else {
                            match result {
                                Ok(settings) => {
                                    action = Some(Action::InspectPatcher {
                                        settings: Box::new(settings),
                                        component: state.component,
                                        instructions: state.instructions,
                                        source: source.into(),
                                        namespace,
                                    })
                                }
                                Err(error) => {
                                    state.status = format!("Check game and mod folders: {error:#}")
                                }
                            }
                        }
                    }
                    let options_label = ui.label("Patcher options");
                    if ui
                        .text_edit_singleline(&mut state.arguments)
                        .labelled_by(options_label.id)
                        .changed()
                    {
                        state.patcher_preview = None;
                    }
                    ui.label(
                        "Prediction shows declared patcher writes; patched bytes remain unknown.",
                    );
                }
                ui.checkbox(&mut state.overwrite, "Replace existing files");
                if ui.button("Record and save action").clicked() {
                    let instruction = Instruction {
                        guid: uuid::Uuid::new_v4().to_string(),
                        parent: component.guid.clone(),
                        action: ACTIONS[state.action].into(),
                        source: state
                            .sources
                            .lines()
                            .map(str::trim)
                            .filter(|line| !line.is_empty())
                            .map(str::to_owned)
                            .collect(),
                        destination: state.destination.clone(),
                        arguments: state.arguments.clone(),
                        overwrite: state.overwrite,
                        dependencies: Vec::new(),
                        restrictions: Vec::new(),
                    };
                    action = Some(Action::Edit {
                        label: format!("{} · {}", component.name, ACTIONS[state.action]),
                        operations: vec![EditOperation::InsertInstruction {
                            component: component.guid.clone(),
                            index: state.instructions,
                            instruction,
                        }],
                    });
                }
                ui.collapsing("Recorded actions", |ui| {
                    for (index, instruction) in component.instructions.iter().enumerate() {
                        ui.label(format!(
                            "{}. {}: {} → {}",
                            index + 1,
                            instruction.action,
                            instruction.source.join(", "),
                            instruction.destination
                        ));
                    }
                });
            } else {
                ui.label("This guide has no mods yet. Add a mod in the advanced editor.");
            }
            ui.collapsing("Saved history", |ui| {
                if let Some(session) = &self.editor.easy.session {
                    for (index, revision) in session.revisions().iter().enumerate() {
                        if ui
                            .selectable_label(index == session.cursor(), &revision.label)
                            .clicked()
                        {
                            action = Some(Action::Seek(index));
                        }
                    }
                }
            });
        }
        if let Some(action) = action
            && let Err(error) = self.easy_work(action, ui.ctx())
        {
            self.editor.easy.status = format!("Action stopped: {error:#}");
        }
        self.persist_easy_view();
        true
    }
}
