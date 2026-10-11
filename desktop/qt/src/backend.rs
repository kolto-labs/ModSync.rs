//! Qt presentation bridge; the existing engine remains authoritative.
#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
        include!("cxx-qt-lib/qqmlapplicationengine.h");
        type QQmlApplicationEngine = cxx_qt_lib::QQmlApplicationEngine;
        include!("development_reload.h");
        fn install_development_reload(engine: Pin<&mut QQmlApplicationEngine>, source: &QUrl);
        include!("QtQuickControls2/QQuickStyle");
        type QQuickStyle;
        #[Self = "QQuickStyle"]
        #[rust_name = "use_shared_style"]
        fn setStyle(style: &QString);
    }
    impl cxx_qt::Threading for AuthoringBackend {}
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, busy)]
        #[qproperty(bool, ready)]
        #[qproperty(QString, status)]
        #[qproperty(QString, snapshot)]
        type AuthoringBackend = super::AuthoringBackendRust;

        #[qinvokable]
        #[cxx_name = "openSession"]
        fn open_session(self: Pin<&mut Self>, directory: &QString, mods: &QString, game: &QString);
        #[qinvokable]
        #[cxx_name = "configureProvidedRunner"]
        fn configure_provided_runner(
            self: Pin<&mut Self>,
            configuration: &QString,
            revision: &QString,
        );
        #[qinvokable]
        #[cxx_name = "createSession"]
        fn create_session(
            self: Pin<&mut Self>,
            guide: &QString,
            directory: &QString,
            mods: &QString,
            game: &QString,
        );
        #[qinvokable]
        #[cxx_name = "addGuideSetup"]
        fn add_guide_setup(self: Pin<&mut Self>, guide: &QString, revision: &QString);
        #[qinvokable]
        #[cxx_name = "reconcileGuideMasters"]
        fn reconcile_guide_masters(self: Pin<&mut Self>, revision: &QString);
        #[qinvokable]
        fn predict(self: Pin<&mut Self>, component: i32, instructions: i32);
        #[qinvokable]
        #[cxx_name = "inspectPatcher"]
        fn inspect_patcher(
            self: Pin<&mut Self>,
            component: i32,
            instructions: i32,
            source: &QString,
            option_index: i32,
            revision: &QString,
        );
        #[qinvokable]
        #[cxx_name = "applyDiskAction"]
        fn apply_disk_action(
            self: Pin<&mut Self>,
            component: &QString,
            instruction: i32,
            expected_guid: &QString,
            revision: &QString,
            expected_mods: &QString,
            expected_game: &QString,
        );
        #[qinvokable]
        #[cxx_name = "seekDisk"]
        fn seek_disk(
            self: Pin<&mut Self>,
            cursor: i32,
            expected_cursor: i32,
            revision: &QString,
            expected_mods: &QString,
            expected_game: &QString,
        );
        #[qinvokable]
        #[cxx_name = "recoverDisk"]
        fn recover_disk(
            self: Pin<&mut Self>,
            expected_cursor: i32,
            revision: &QString,
            expected_mods: &QString,
            expected_game: &QString,
        );
        #[qinvokable]
        fn cancel(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "seekRevision"]
        fn seek_revision(self: Pin<&mut Self>, offset: i32);
        #[qinvokable]
        #[cxx_name = "importArchive"]
        fn import_archive(self: Pin<&mut Self>, source: &QUrl);
        #[qinvokable]
        #[cxx_name = "recordAction"]
        fn record_action(
            self: Pin<&mut Self>,
            component: &QString,
            position: i32,
            definition: &QString,
            revision: &QString,
            execution_roots: &QString,
        );
        #[qinvokable]
        #[cxx_name = "replaceAction"]
        fn replace_action(
            self: Pin<&mut Self>,
            component: &QString,
            position: i32,
            definition: &QString,
            expected_guid: &QString,
            revision: &QString,
        );
        #[qinvokable]
        #[cxx_name = "removeAction"]
        fn remove_action(
            self: Pin<&mut Self>,
            component: &QString,
            position: i32,
            expected_guid: &QString,
            revision: &QString,
        );
        #[qinvokable]
        #[cxx_name = "setIncluded"]
        fn set_included(self: Pin<&mut Self>, component: &QString, included: bool);
    }
}

use anyhow::{Context, Result, ensure};
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QUrl, QUuid};
use modsync::{Settings, editor::easy::Session};
use std::{
    path::Path,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Default)]
struct WorkerState {
    session: Option<Session>,
    settings: Option<Settings>,
}

pub struct AuthoringBackendRust {
    busy: bool,
    ready: bool,
    status: QString,
    snapshot: QString,
    worker: Arc<Mutex<WorkerState>>,
    cancelled: Arc<AtomicBool>,
}

impl Default for AuthoringBackendRust {
    fn default() -> Self {
        Self {
            busy: false,
            ready: false,
            status: QString::from("Open your saved authoring session to begin."),
            snapshot: QString::from("{}"),
            worker: Arc::new(Mutex::new(WorkerState::default())),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

fn summary(state: &WorkerState) -> Result<serde_json::Value> {
    let session = state
        .session
        .as_ref()
        .context("Open a saved session first")?;
    let settings = state
        .settings
        .as_ref()
        .context("Install roots are unavailable")?;
    let components: Vec<_> = session
        .guide()
        .components
        .iter()
        .enumerate()
        .map(|(index, c)| {
            let mut notes = [
                "DownloadInstructions",
                "InstallationWarning",
                "UsageWarning",
                "CompatibilityWarning",
                "KnownBugs",
                "SteamNotes",
                "GuideSectionContext",
            ]
            .into_iter()
            .filter_map(|key| {
                c.metadata
                    .get(key)
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| serde_json::json!({"field": key, "text": value}))
            })
            .collect::<Vec<_>>();
            if !c.dependencies.is_empty() {
                let required = c.dependencies.iter().map(|guid|
                    modsync::editor::find_component(session.guide(), guid)
                        .map_or_else(|| guid.clone(), |component| component.name.clone()))
                    .collect::<Vec<_>>().join(", ");
                notes.insert(0, serde_json::json!({"field": "Requires", "text": required}));
            }
            if let Some(masters) = c.metadata.get("UnresolvedMasters").and_then(serde_json::Value::as_array) {
                let text = masters.iter().filter_map(serde_json::Value::as_str).collect::<Vec<_>>().join("\n");
                if !text.is_empty() {
                    notes.push(serde_json::json!({"field": "Unresolved master requirements", "text": text}));
                }
            }
            serde_json::json!({"index": index, "name": c.name, "guid": c.guid,
            "directions": c.directions, "selected": c.is_selected, "actions": c.instructions.len(),
            "recipe": c.instructions, "description": c.description,
            "method": c.metadata.get("InstallationMethod"),
            "notes": notes})
        })
        .collect();
    let history = session.disk_history()?;
    let disk_cursor = history.as_ref().map_or(0, |history| history.cursor);
    let ordered = session.guide().ordered();
    let next_action = ordered.as_ref().ok().and_then(|ordered| {
        ordered
            .iter()
            .flat_map(|component| {
                component
                    .instructions
                    .iter()
                    .enumerate()
                    .map(move |(index, instruction)| {
                        serde_json::json!({"component": component.guid, "mod": component.name,
                    "instruction": index, "guid": instruction.guid, "requiresRunner": instruction.action == "Execute" || (instruction.action == "Patcher" && settings.provided_installer.is_some())})
                    })
            })
            .nth(disk_cursor)
    });
    let disk = serde_json::json!({
        "cursor": disk_cursor,
        "checkpoints": history.as_ref().map_or(0, |history| history.actions.len()),
        "pending": history.as_ref().is_some_and(|history| history.pending.is_some()),
        "restoring": history.as_ref().and_then(|history| history.restoring),
        "rootsMatch": history.as_ref().is_none_or(|history|
            history.mods == settings.roots.mods && history.game == settings.roots.game),
        "orderError": ordered.as_ref().err().map(|error| format!("{error:#}")),
        "nextAction": next_action
    });
    Ok(
        serde_json::json!({"components": components, "cursor": session.cursor(),
        "canUndo": session.can_undo(), "canRedo": session.can_redo(),
        "revisionCount": session.revisions().len(), "revisionId": session.revisions()[session.cursor()].id, "game": settings.roots.game,
        "mods": settings.roots.mods, "entries": [], "disk": disk,
        "allowProcesses": settings.allow_processes,
        "providedRunner": settings.provided_installer.as_ref().map(|runner| serde_json::json!({
            "requestDir": runner.request_dir, "xvfb": runner.xvfb,
            "windows": runner.windows.as_ref().map(|windows| serde_json::json!({
                "executable": windows.executable, "wineserver": windows.wineserver, "prefix": windows.prefix
            }))
        })),
        "guideNotes": {
            "introduction": session.guide().metadata.get("preambleContent"),
            "widescreen": session.guide().metadata.get("widescreenWarningContent"),
            "afterBuild": session.guide().metadata.get("epilogueContent")
        }}),
    )
}

fn verify_disk_request(
    state: &WorkerState,
    revision: &str,
    expected_cursor: i32,
    expected_mods: &str,
    expected_game: &str,
) -> Result<()> {
    ensure!(expected_cursor >= 0, "Invalid disk checkpoint");
    let session = state
        .session
        .as_ref()
        .context("Open a saved session first")?;
    ensure!(
        session.revisions()[session.cursor()].id == revision,
        "The recipe changed; reopen the disk action dialog"
    );
    let settings = state
        .settings
        .as_ref()
        .context("Install roots are unavailable")?;
    ensure!(
        settings.roots.mods == Path::new(expected_mods)
            && settings.roots.game == Path::new(expected_game),
        "The configured folders changed; reopen the disk action dialog"
    );
    let history = session
        .disk_history()?
        .context("No disk checkpoint history")?;
    ensure!(
        history.cursor == expected_cursor as usize,
        "The disk checkpoint changed; reopen the dialog"
    );
    ensure!(
        history.mods == settings.roots.mods && history.game == settings.roots.game,
        "This session's disk checkpoints belong to different roots"
    );
    Ok(())
}

impl Drop for AuthoringBackendRust {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl ffi::AuthoringBackend {
    pub fn configure_provided_runner(
        self: Pin<&mut Self>,
        configuration: &QString,
        revision: &QString,
    ) {
        let configuration = configuration.to_string();
        let revision = revision.to_string();
        self.start("Configuring headless installer runner…", move |state| {
            ensure!(
                configuration.len() <= 64 * 1024,
                "Runner configuration is too large"
            );
            let session = state
                .session
                .as_ref()
                .context("Open a saved session first")?;
            ensure!(
                session.revisions()[session.cursor()].id == revision,
                "The recipe changed; reopen runner settings"
            );
            let value: serde_json::Value =
                serde_json::from_str(&configuration).context("Read runner configuration")?;
            let settings = state
                .settings
                .as_mut()
                .context("Install roots are unavailable")?;
            if value.is_null() {
                settings.provided_installer = None;
                settings.allow_processes = false;
            } else {
                ensure!(
                    value["trusted"].as_bool() == Some(true),
                    "Explicit installer execution trust is required"
                );
                let path = |key: &str| -> Result<std::path::PathBuf> {
                    let raw = value[key]
                        .as_str()
                        .context("Runner paths must be strings")?;
                    ensure!(
                        Path::new(raw).is_absolute(),
                        "Runner paths must be absolute"
                    );
                    std::fs::canonicalize(raw).context("Runner paths must already exist")
                };
                let request_dir = path("requestDir")?;
                ensure!(
                    request_dir.is_dir()
                        && !request_dir.starts_with(&settings.roots.mods)
                        && !request_dir.starts_with(&settings.roots.game),
                    "Installer request storage must be outside game and source folders"
                );
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    ensure!(
                        request_dir.metadata()?.permissions().mode() & 0o077 == 0,
                        "Installer request storage must be private"
                    );
                }
                let xvfb = path("xvfb")?;
                ensure!(xvfb.is_file(), "Xvfb executable must be an ordinary file");
                let windows = if value["wine"]
                    .as_str()
                    .is_some_and(|wine| !wine.trim().is_empty())
                {
                    let executable = path("wine")?;
                    let wineserver = path("wineserver")?;
                    ensure!(
                        executable.is_file(),
                        "Wine executable must be an ordinary file"
                    );
                    ensure!(
                        wineserver.is_file(),
                        "Wineserver executable must be an ordinary file"
                    );
                    let prefix = path("prefix")?;
                    ensure!(
                        prefix.is_dir()
                            && prefix.starts_with(&request_dir)
                            && prefix != request_dir,
                        "Wine prefix must be dedicated storage inside the request directory"
                    );
                    Some(modsync::engine::WindowsProcessRunner {
                        executable,
                        wineserver,
                        prefix,
                    })
                } else {
                    None
                };
                settings.provided_installer = Some(modsync::engine::ProvidedInstallerRunner {
                    request_dir,
                    xvfb,
                    windows,
                });
                settings.allow_processes = true;
            }
            let mut snapshot = summary(state)?;
            snapshot["operationStatus"] = serde_json::json!(
                "Installer settings updated for this open session only • no files executed"
            );
            Ok(snapshot)
        });
    }
    pub fn reconcile_guide_masters(self: Pin<&mut Self>, revision: &QString) {
        let revision = revision.to_string();
        self.start("Reconciling known guide masters…", move |state| {
            let session = state
                .session
                .as_mut()
                .context("Open a saved session first")?;
            ensure!(
                session.revisions()[session.cursor()].id == revision,
                "The recipe changed; reopen the Session dialog"
            );
            let operations = modsync::editor::reconcile_authoring_masters(session.guide())?;
            if !operations.is_empty() {
                session.edit("Resolve known guide masters", &operations)?;
            }
            let mut value = summary(state)?;
            value["operationStatus"] = serde_json::json!(format!(
                "Reconciled {} mods • unknown/conditional masters retained • no disk actions run",
                operations.len()
            ));
            Ok(value)
        });
    }

    pub fn add_guide_setup(self: Pin<&mut Self>, guide: &QString, revision: &QString) {
        let guide = guide.to_string();
        let revision = revision.to_string();
        self.start("Saving missing guide mods…", move |state| {
            let imported = modsync::editor::read_authoring_guide(Path::new(&guide))?;
            let session = state
                .session
                .as_mut()
                .context("Open a saved session first")?;
            ensure!(
                session.revisions()[session.cursor()].id == revision,
                "The recipe changed; reopen the guide import dialog"
            );
            let outside = |component: &modsync::Component| {
                ["GuideSetup", "GuideTail"].iter().any(|key| {
                    component
                        .metadata
                        .get(*key)
                        .and_then(serde_json::Value::as_bool)
                        == Some(true)
                })
            };
            let body: Vec<_> = imported.components.iter().filter(|c| !outside(c)).collect();
            let existing_body: Vec<_> = session
                .guide()
                .components
                .iter()
                .filter(|c| !outside(c))
                .collect();
            ensure!(
                body.len() == existing_body.len()
                    && existing_body
                        .iter()
                        .zip(body)
                        .all(|(old, new)| old.guid.eq_ignore_ascii_case(&new.guid)),
                "Choose the original full guide; main-list identities or order differ"
            );
            for key in [
                "preambleContent",
                "epilogueContent",
                "widescreenWarningContent",
            ] {
                ensure!(
                    session.guide().metadata.get(key) == imported.metadata.get(key),
                    "The guide notes differ from this saved session"
                );
            }
            let identities: std::collections::BTreeSet<_> = session
                .guide()
                .components
                .iter()
                .map(|c| c.guid.to_ascii_lowercase())
                .collect();
            let retained: Vec<_> = imported
                .components
                .iter()
                .filter(|c| identities.contains(&c.guid.to_ascii_lowercase()))
                .collect();
            ensure!(
                retained.len() == session.guide().components.len()
                    && retained
                        .iter()
                        .zip(&session.guide().components)
                        .all(|(new, old)| new.guid.eq_ignore_ascii_case(&old.guid)),
                "Existing guide mods differ or are reordered; reconcile them before import"
            );
            let mut operations = Vec::new();
            for (index, component) in imported.components.into_iter().enumerate() {
                if identities.contains(&component.guid.to_ascii_lowercase()) {
                    continue;
                }
                ensure!(
                    outside(&component),
                    "A main-list mod cannot be inserted by this repair"
                );
                operations.push(modsync::editor::EditOperation::InsertComponent {
                    parent: None,
                    index,
                    component: Box::new(component),
                });
            }
            ensure!(
                !operations.is_empty(),
                "All named guide setup and tail mods are already included"
            );
            session.edit("Include named guide setup and tail mods", &operations)?;
            let mut value = summary(state)?;
            value["operationStatus"] =
                serde_json::json!("Missing guide mods saved in source order • no disk actions run");
            Ok(value)
        });
    }

    pub fn create_session(
        self: Pin<&mut Self>,
        guide: &QString,
        directory: &QString,
        mods: &QString,
        game: &QString,
    ) {
        let (guide, directory, mods, game) = (
            guide.to_string(),
            directory.to_string(),
            mods.to_string(),
            game.to_string(),
        );
        let cancelled = Arc::clone(&self.rust().cancelled);
        self.start("Creating authoring session from guide…", move |state| {
            ensure!(
                !directory.trim().is_empty(),
                "Choose a new session directory"
            );
            let target = Path::new(&directory);
            ensure!(
                target.is_absolute(),
                "Session directory must be an absolute path"
            );
            let name = target
                .file_name()
                .context("Choose a named session directory")?;
            let parent = target
                .parent()
                .context("Session directory needs a parent")?
                .canonicalize()
                .context("The session's parent directory must already exist")?;
            let target = parent.join(name);
            let mut settings = Settings::new(Path::new(&mods), Path::new(&game))?;
            ensure!(
                !target.starts_with(&settings.roots.mods)
                    && !target.starts_with(&settings.roots.game),
                "Keep the authoring session outside game and mod source folders"
            );
            let guide = modsync::editor::read_authoring_guide(Path::new(&guide))?;
            ensure!(!guide.components.is_empty(), "The guide contains no mods");
            ensure!(
                !cancelled.load(Ordering::Relaxed),
                "Session creation cancelled"
            );
            settings.cancelled = cancelled;
            let session = Session::create(&target, &guide)?;
            state.settings = Some(settings);
            state.session = Some(session);
            let mut value = summary(state)?;
            value["operationStatus"] =
                serde_json::json!("Guide saved as a new authoring session • virtual filesystem");
            Ok(value)
        });
    }

    pub fn set_included(self: Pin<&mut Self>, component: &QString, included: bool) {
        let guid = component.to_string();
        self.start("Saving mod selection…", move |state| {
            let session = state
                .session
                .as_mut()
                .context("Open a saved session first")?;
            let mut component = modsync::editor::find_component(session.guide(), &guid)
                .context("Mod is no longer in this session")?
                .clone();
            component.is_selected = included;
            session.edit(
                "Change mod selection",
                &[modsync::editor::EditOperation::ReplaceComponent {
                    guid,
                    component: Box::new(component),
                }],
            )?;
            summary(state)
        });
    }

    pub fn record_action(
        self: Pin<&mut Self>,
        component: &QString,
        position: i32,
        definition: &QString,
        revision: &QString,
        execution_roots: &QString,
    ) {
        self.save_action(
            component.to_string(),
            position,
            definition.to_string(),
            None,
            revision.to_string(),
            execution_roots.to_string(),
        );
    }

    pub fn replace_action(
        self: Pin<&mut Self>,
        component: &QString,
        position: i32,
        definition: &QString,
        expected_guid: &QString,
        revision: &QString,
    ) {
        self.save_action(
            component.to_string(),
            position,
            definition.to_string(),
            Some(expected_guid.to_string()),
            revision.to_string(),
            "null".into(),
        );
    }

    pub fn remove_action(
        self: Pin<&mut Self>,
        component: &QString,
        position: i32,
        expected_guid: &QString,
        revision: &QString,
    ) {
        let component = component.to_string();
        let guid = expected_guid.to_string();
        let revision = revision.to_string();
        self.start("Removing saved action…", move |state| {
            ensure!(position >= 0, "Invalid saved action position");
            let session = state
                .session
                .as_mut()
                .context("Open a saved session first")?;
            ensure!(
                session.revisions()[session.cursor()].id == revision,
                "The recipe changed; reopen the action"
            );
            let existing = modsync::editor::find_component(session.guide(), &component)
                .and_then(|c| c.instructions.get(position as usize))
                .context("Saved action no longer exists")?;
            ensure!(existing.guid == guid, "The action changed; reopen it");
            let label = format!("Remove {} action", existing.action);
            session.edit(
                &label,
                &[modsync::editor::EditOperation::RemoveInstruction {
                    component,
                    index: position as usize,
                }],
            )?;
            let mut snapshot = summary(state)?;
            snapshot["recordedPosition"] = serde_json::json!(position);
            Ok(snapshot)
        });
    }

    fn save_action(
        self: Pin<&mut Self>,
        component: String,
        position: i32,
        definition: String,
        replacement: Option<String>,
        revision: String,
        execution_roots: String,
    ) {
        let qt = self.qt_thread();
        self.start("Saving recorded action…", move |state| {
            ensure!(execution_roots.len() <= 64 * 1024, "Invalid execution folders");
            let immediate_roots: Option<(String, String)> = serde_json::from_str(&execution_roots)
                .context("Read immediate execution folders")?;
            ensure!(
                position >= 0 && definition.len() <= 64 * 1024,
                "Invalid recorded action"
            );
            let settings = state
                .settings
                .as_ref()
                .context("Install roots are unavailable")?
                .clone();
            if let Some((mods, game)) = &immediate_roots {
                ensure!(settings.roots.mods == Path::new(mods) && settings.roots.game == Path::new(game),
                    "Install folders changed; reopen the action form");
            }
            let session = state
                .session
                .as_mut()
                .context("Open a saved session first")?;
            ensure!(
                session.revisions()[session.cursor()].id == revision,
                "The recipe changed; reopen the action form"
            );
            let value: serde_json::Value =
                serde_json::from_str(&definition).context("Read recorded action")?;
            let fields = value
                .as_object()
                .context("Recorded action must be an object")?;
            ensure!(
                fields.contains_key("Action")
                    && fields.keys().all(|key| matches!(
                        key.as_str(),
                        "Action"
                            | "Source"
                            | "Destination"
                            | "Arguments"
                            | "Overwrite"
                            | "Dependencies"
                            | "Restrictions"
                    )),
                "Recorded action fields must use the canonical instruction format"
            );
            let mut instruction: modsync::Instruction =
                serde_json::from_value(value).context("Read recorded instruction")?;
            ensure!(
                matches!(
                    instruction.action.as_str(),
                    "Copy"
                        | "Move"
                        | "Rename"
                        | "Delete"
                        | "Extract"
                        | "Patcher"
                        | "Execute"
                        | "CreateDirectory"
                        | "DelDuplicate"
                        | "CleanList"
                        | "Choose"
                ),
                "Unsupported recorded action"
            );
            if immediate_roots.is_some() && instruction.action == "Execute" {
                ensure!(settings.allow_processes && settings.provided_installer.is_some(),
                    "Configure and explicitly trust the headless runner before executing recorded programs");
            }
            instruction.guid = if let Some(expected) = &replacement {
                let existing = modsync::editor::find_component(session.guide(), &component)
                    .and_then(|c| c.instructions.get(position as usize))
                    .context("Saved action no longer exists")?;
                ensure!(&existing.guid == expected, "The action changed; reopen it");
                existing.guid.clone()
            } else {
                QUuid::create_uuid()
                    .to_string()
                    .trim_matches(['{', '}'])
                    .to_owned()
            };
            instruction.parent = component.clone();
            for source in &mut instruction.source {
                if Path::new(source).is_absolute() {
                    *source = modsync::editor::variable_path(&settings.roots, Path::new(source))?;
                }
            }
            if Path::new(&instruction.destination).is_absolute() {
                instruction.destination = modsync::editor::variable_path(
                    &settings.roots,
                    Path::new(&instruction.destination),
                )?;
            }
            let action_guid = instruction.guid.clone();
            let owner_guid = component.clone();
            let operation = if replacement.is_some() {
                let existing = modsync::editor::find_component(session.guide(), &component)
                    .and_then(|c| c.instructions.get(position as usize))
                    .context("Saved action no longer exists")?;
                if serde_json::to_value(existing)? == serde_json::to_value(&instruction)? {
                    return summary(state);
                }
                modsync::editor::EditOperation::ReplaceInstruction {
                    component,
                    index: position as usize,
                    instruction,
                }
            } else {
                modsync::editor::EditOperation::InsertInstruction {
                    component,
                    index: position as usize,
                    instruction,
                }
            };
            let label = if replacement.is_some() {
                "Edit saved action"
            } else {
                "Record action"
            };
            if immediate_roots.is_some() {
                let history = session.disk_history()?;
                if let Some(history) = &history {
                    ensure!(history.pending.is_none() && history.restoring.is_none(),
                        "Recover the pending disk action before recording in immediate mode");
                    ensure!(history.mods == settings.roots.mods && history.game == settings.roots.game,
                        "Disk checkpoints belong to different install folders");
                    ensure!(history.cursor == history.actions.len(),
                        "Restore the latest disk checkpoint before recording in immediate mode");
                }
                let candidate = modsync::editor::apply_operations(session.guide(), std::slice::from_ref(&operation))?;
                let ordered = candidate.ordered()?;
                let next = ordered.iter().flat_map(|owner| &owner.instructions)
                    .nth(history.as_ref().map_or(0, |history| history.cursor));
                ensure!(next.is_some_and(|next| next.guid == action_guid),
                    "This is not the next ordered disk action. Apply earlier actions or use virtual mode");
            }
            session.edit(label, &[operation])?;
            let mut events = Vec::new();
            if immediate_roots.is_some() {
                let owner = session.guide().components.iter().position(|owner| owner.guid == owner_guid)
                    .context("Saved action owner is unavailable")?;
                let mut last = std::time::Instant::now();
                session.apply_to_disk(settings, owner, position as usize, |event| {
                    if events.len() < 10000 { events.push(event); }
                }, |progress| {
                    if last.elapsed() >= std::time::Duration::from_millis(100) {
                        last = std::time::Instant::now();
                        let message = progress.message;
                        let _ = qt.queue(move |mut object| object.as_mut().set_status(QString::from(message.as_str())));
                    }
                }).context("Recipe saved; immediate disk action failed. Inspect disk checkpoints before retrying")?;
            }
            let mut snapshot = summary(state)?;
            if immediate_roots.is_some() {
                snapshot["operationStatus"] = serde_json::json!("Action saved and applied with a disk checkpoint");
                snapshot["events"] = serde_json::to_value(events)?;
            }
            snapshot["recordedPosition"] = serde_json::json!(position + 1);
            Ok(snapshot)
        });
    }

    pub fn seek_revision(self: Pin<&mut Self>, offset: i32) {
        self.start("Loading saved authoring revision…", move |state| {
            ensure!(offset == -1 || offset == 1, "Invalid revision direction");
            let session = state
                .session
                .as_mut()
                .context("Open a saved session first")?;
            let cursor = session
                .cursor()
                .checked_add_signed(offset as isize)
                .context("Already at the first revision")?;
            session.seek(cursor)?;
            summary(state)
        });
    }

    pub fn apply_disk_action(
        self: Pin<&mut Self>,
        component: &QString,
        instruction: i32,
        expected_guid: &QString,
        revision: &QString,
        expected_mods: &QString,
        expected_game: &QString,
    ) {
        let component = component.to_string();
        let expected_guid = expected_guid.to_string();
        let revision = revision.to_string();
        let expected_mods = expected_mods.to_string();
        let expected_game = expected_game.to_string();
        let qt = self.qt_thread();
        self.start(
            "Applying the next recorded action with checkpoints…",
            move |state| {
                ensure!(instruction >= 0, "Invalid disk action");
                let settings = state
                    .settings
                    .as_ref()
                    .context("Install roots are unavailable")?
                    .clone();
                ensure!(
                    settings.roots.mods == Path::new(&expected_mods)
                        && settings.roots.game == Path::new(&expected_game),
                    "The configured folders changed; reopen the disk action dialog"
                );
                let session = state
                    .session
                    .as_mut()
                    .context("Open a saved session first")?;
                ensure!(
                    session.revisions()[session.cursor()].id == revision,
                    "The recipe changed; reopen the disk action dialog"
                );
                let index = session
                    .guide()
                    .components
                    .iter()
                    .position(|owner| owner.guid.eq_ignore_ascii_case(&component))
                    .context("Mod is no longer in this session")?;
                let action = session.guide().components[index]
                    .instructions
                    .get(instruction as usize)
                    .context("Action is no longer in this mod")?;
                ensure!(
                    action.guid.eq_ignore_ascii_case(&expected_guid),
                    "The action changed; reopen the dialog"
                );
                let mut events = Vec::new();
                let mut last = std::time::Instant::now();
                session.apply_to_disk(
                    settings,
                    index,
                    instruction as usize,
                    |event| {
                        if events.len() < 10000 {
                            events.push(event);
                        }
                    },
                    |progress| {
                        if last.elapsed() >= std::time::Duration::from_millis(100) {
                            last = std::time::Instant::now();
                            let message = progress.message;
                            let _ = qt.queue(move |mut object| {
                                object.as_mut().set_status(QString::from(message.as_str()))
                            });
                        }
                    },
                )?;
                let mut value = summary(state)?;
                value["events"] = serde_json::to_value(events)?;
                value["operationStatus"] = serde_json::json!(
                    "Action applied to disk and checkpointed. Virtual preview remains available."
                );
                Ok(value)
            },
        );
    }

    pub fn seek_disk(
        self: Pin<&mut Self>,
        cursor: i32,
        expected_cursor: i32,
        revision: &QString,
        expected_mods: &QString,
        expected_game: &QString,
    ) {
        let revision = revision.to_string();
        let expected_mods = expected_mods.to_string();
        let expected_game = expected_game.to_string();
        let cancelled = Arc::clone(&self.rust().cancelled);
        let qt = self.qt_thread();
        self.start("Restoring the recorded disk checkpoint…", move |state| {
            ensure!(cursor >= 0, "Invalid disk checkpoint");
            verify_disk_request(
                state,
                &revision,
                expected_cursor,
                &expected_mods,
                &expected_game,
            )?;
            let mut last = std::time::Instant::now();
            state
                .session
                .as_mut()
                .context("Open a saved session first")?
                .seek_disk(cursor as usize, &cancelled, |progress| {
                    if last.elapsed() >= std::time::Duration::from_millis(100) {
                        last = std::time::Instant::now();
                        let message = progress.message;
                        let _ = qt.queue(move |mut object| {
                            object.as_mut().set_status(QString::from(message.as_str()))
                        });
                    }
                })?;
            let mut value = summary(state)?;
            value["operationStatus"] =
                serde_json::json!("Disk checkpoint restored. Your saved recipe is retained.");
            Ok(value)
        });
    }

    pub fn recover_disk(
        self: Pin<&mut Self>,
        expected_cursor: i32,
        revision: &QString,
        expected_mods: &QString,
        expected_game: &QString,
    ) {
        let revision = revision.to_string();
        let expected_mods = expected_mods.to_string();
        let expected_game = expected_game.to_string();
        let cancelled = Arc::clone(&self.rust().cancelled);
        self.start("Recovering the pending disk action…", move |state| {
            verify_disk_request(
                state,
                &revision,
                expected_cursor,
                &expected_mods,
                &expected_game,
            )?;
            state
                .session
                .as_mut()
                .context("Open a saved session first")?
                .recover_disk_action(&cancelled)?;
            let mut value = summary(state)?;
            value["operationStatus"] = serde_json::json!(
                "Disk action recovery finished. Inspect its checkpoint before continuing."
            );
            Ok(value)
        });
    }

    pub fn import_archive(self: Pin<&mut Self>, source: &QUrl) {
        let source = source
            .to_local_file()
            .map(|path| path.to_string())
            .unwrap_or_default();
        let cancelled = Arc::clone(&self.rust().cancelled);
        self.start(
            "Importing your archive into the saved session…",
            move |state| {
                ensure!(!source.is_empty(), "Choose a local archive file");
                state
                    .session
                    .as_mut()
                    .context("Open a saved session first")?
                    .import_archive(Path::new(&source), 8 * 1024 * 1024 * 1024, &cancelled)?;
                summary(state)
            },
        );
    }

    pub fn cancel(mut self: Pin<&mut Self>) {
        self.rust().cancelled.store(true, Ordering::Relaxed);
        self.as_mut()
            .set_status(QString::from("Stopping prediction…"));
    }

    fn start(
        mut self: Pin<&mut Self>,
        message: &str,
        job: impl FnOnce(&mut WorkerState) -> Result<serde_json::Value> + Send + 'static,
    ) {
        if *self.busy() {
            return;
        }
        self.rust().cancelled.store(false, Ordering::Relaxed);
        self.as_mut().set_busy(true);
        let mut stale: serde_json::Value = serde_json::from_str(&self.snapshot().to_string())
            .unwrap_or_else(|_| serde_json::json!({}));
        stale["entries"] = serde_json::json!([]);
        stale["events"] = serde_json::json!([]);
        stale["patcher"] = serde_json::Value::Null;
        self.as_mut()
            .set_snapshot(QString::from(stale.to_string().as_str()));
        self.as_mut().set_status(QString::from(message));
        let worker = Arc::clone(&self.rust().worker);
        let qt = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("modsync-qt-authoring".into())
            .spawn(move || {
                let (result, fallback) = match worker.lock() {
                    Ok(mut state) => {
                        let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(&mut state))) {
                            Ok(result) => result,
                            Err(_) => {
                                state.session = None;
                                state.settings = None;
                                Err(anyhow::anyhow!("Authoring worker stopped unexpectedly. Reopen the saved session."))
                            }
                        };
                        let fallback = result.as_ref().err().and_then(|_| summary(&state).ok());
                        (result, fallback)
                    }
                    Err(_) => (Err(anyhow::anyhow!("Authoring worker failed")), None),
                };
                let _ = qt.queue(move |mut object| {
                    match result {
                        Ok(value) => {
                            object
                                .as_mut()
                                .set_snapshot(QString::from(value.to_string().as_str()));
                            object.as_mut().set_ready(true);
                            object.as_mut().set_status(QString::from(value.get("operationStatus")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("Saved session ready • virtual filesystem")));
                        }
                        Err(error) => {
                            object.as_mut().set_ready(fallback.is_some());
                            if let Some(value) = fallback {
                                object
                                    .as_mut()
                                    .set_snapshot(QString::from(value.to_string().as_str()));
                            } else {
                                object.as_mut().set_snapshot(QString::from("{}"));
                            }
                            object
                                .as_mut()
                                .set_status(QString::from(format!("{error:#}").as_str()));
                        }
                    }
                    object.as_mut().set_busy(false);
                });
            });
        if let Err(error) = spawned {
            self.as_mut().set_busy(false);
            self.as_mut().set_ready(false);
            self.as_mut().set_snapshot(QString::from("{}"));
            self.as_mut().set_status(QString::from(
                format!("Cannot start authoring worker: {error}").as_str(),
            ));
        }
    }

    pub fn open_session(self: Pin<&mut Self>, directory: &QString, mods: &QString, game: &QString) {
        let (directory, mods, game) = (directory.to_string(), mods.to_string(), game.to_string());
        let cancelled = Arc::clone(&self.rust().cancelled);
        self.start("Opening saved authoring session…", move |state| {
            state.session = None;
            state.settings = None;
            let mut settings = Settings::new(Path::new(&mods), Path::new(&game))?;
            settings.cancelled = cancelled;
            let session = Session::open(Path::new(&directory))?;
            state.settings = Some(settings);
            state.session = Some(session);
            summary(state)
        });
    }

    pub fn inspect_patcher(
        self: Pin<&mut Self>,
        component: i32,
        instructions: i32,
        source: &QString,
        option_index: i32,
        revision: &QString,
    ) {
        let source = source.to_string();
        let revision = revision.to_string();
        self.start("Reading built-in patcher options…", move |state| {
            ensure!(
                component >= 0
                    && instructions >= 0
                    && option_index >= -1
                    && !source.trim().is_empty()
                    && source.len() <= 32 * 1024,
                "Choose one patcher file or folder and a valid build point"
            );
            let session = state
                .session
                .as_ref()
                .context("Open a saved session first")?;
            ensure!(
                session.revisions()[session.cursor()].id == revision,
                "The recipe changed; reopen the patcher inspector"
            );
            let settings = state
                .settings
                .as_ref()
                .context("Install roots are unavailable")?;
            let source = if Path::new(source.trim()).is_absolute() {
                modsync::editor::variable_path(&settings.roots, Path::new(source.trim()))?
            } else {
                source.trim().to_owned()
            };
            let preview = session.inspect_patcher(
                settings.clone(),
                component as usize,
                instructions as usize,
                &source,
                (option_index >= 0).then_some(option_index as usize),
            )?;
            let mut value = summary(state)?;
            value["patcher"] = serde_json::to_value(preview)?;
            Ok(value)
        });
    }

    pub fn predict(self: Pin<&mut Self>, component: i32, instructions: i32) {
        self.start(
            "Predicting files without changing your game…",
            move |state| {
                ensure!(
                    component >= 0 && instructions >= 0,
                    "Invalid authoring point"
                );
                let session = state
                    .session
                    .as_ref()
                    .context("Open a saved session first")?;
                let settings = state
                    .settings
                    .as_ref()
                    .context("Install roots are unavailable")?;
                let prediction =
                    session.predict(settings.clone(), component as usize, instructions as usize)?;
                let mut value = summary(state)?;
                value["entries"] = serde_json::to_value(prediction.entries)?;
                value["events"] = serde_json::to_value(prediction.events)?;
                Ok(value)
            },
        );
    }
}
