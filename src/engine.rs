mod checkpoint_coordinator;
mod managed;
pub use managed::{ManagedLiveEdit, ManagedRunReport};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallReport {
    pub managed: Option<ManagedRunReport>,
    pub checkpoint: Option<CheckpointRunReport>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckpointRunReport {
    pub session_id: String,
    pub checkpoint_id: String,
    pub generation: u64,
}
mod patcher_prepare;
mod patcher_simulation;
pub(crate) mod process_arguments;

use crate::{
    archive,
    install_progress::{OwnerFrame, Phase as InstallPhase, Progress as InstallProgress},
    model::{Component, Guide, Instruction},
    paths::{Roots, relative},
    recovery,
};
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet, btree_map},
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PatcherEngine {
    OdyPatcher,
    KPatcher,
    #[default]
    HoloPatcher,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub roots: Roots,
    pub allow_processes: bool,
    pub patcher: Option<PathBuf>,
    pub patcher_engine: PatcherEngine,
    pub archive: archive::Options,
    pub cancelled: Arc<AtomicBool>,
}
impl Settings {
    pub fn new(mods: &Path, game: &Path) -> Result<Self> {
        let game = crate::game::resolve_install_root(game);
        Ok(Self {
            roots: Roots::new(mods, &game)?,
            allow_processes: false,
            patcher: None,
            patcher_engine: PatcherEngine::default(),
            archive: archive::Options::default(),
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub component: String,
    pub step: usize,
    pub message: String,
}

/// Run-local observer state. Nothing is installed on Engine itself, so silent
/// validation/preflight and ordinary Event consumers cannot observe these facts.
struct InstallObserver<'a> {
    callback: &'a mut dyn FnMut(InstallProgress),
    current: InstallProgress,
}

impl<'a> InstallObserver<'a> {
    fn new(total: usize, callback: &'a mut dyn FnMut(InstallProgress)) -> Self {
        Self {
            callback,
            current: InstallProgress {
                phase: InstallPhase::Preparing,
                top_level_component_name: None,
                top_level_component_guid: None,
                component_index: 0,
                component_total: total,
                owner_name: None,
                owner_guid: None,
                owner_path: Vec::new(),
                instruction_index: None,
                instruction_total: None,
                operation: None,
                message: String::new(),
                snapshot_pending: false,
            },
        }
    }

    fn emit(&mut self, phase: InstallPhase, message: impl Into<String>) {
        self.current.phase = phase;
        self.current.message = message.into();
        (self.callback)(self.current.clone());
    }

    fn component(&mut self, component: &Component, index: usize) {
        self.current.top_level_component_name = Some(component.name.clone());
        self.current.top_level_component_guid = Some(component.guid.clone());
        self.current.component_index = index;
        self.current.owner_name = Some(component.name.clone());
        self.current.owner_guid = Some(component.guid.clone());
        self.current.owner_path = vec![OwnerFrame {
            name: component.name.clone(),
            guid: component.guid.clone(),
            reached_from_instruction: None,
        }];
        self.current.instruction_index = None;
        self.current.instruction_total = Some(component.instructions.len());
        self.current.operation = None;
    }

    fn instruction(&mut self, owner: &Component, index: usize, message: String) {
        self.current.owner_name = Some(owner.name.clone());
        self.current.owner_guid = Some(owner.guid.clone());
        self.current.instruction_index = Some(index);
        self.current.instruction_total = Some(owner.instructions.len());
        self.current.operation = Some(owner.instructions[index - 1].action.clone());
        self.emit(InstallPhase::RunningInstruction, message);
    }

    fn clear_component(&mut self) {
        self.current.top_level_component_name = None;
        self.current.top_level_component_guid = None;
        self.current.component_index = 0;
        self.current.owner_name = None;
        self.current.owner_guid = None;
        self.current.owner_path.clear();
        self.current.instruction_index = None;
        self.current.instruction_total = None;
        self.current.operation = None;
    }
}

#[derive(Clone)]
enum Content {
    /// Presence inferred from InstallList; never publish or interpret as empty bytes.
    Unknown,
    Disk(PathBuf),
    Scratch(
        Arc<tempfile::NamedTempFile>,
        Option<u32>,
        Option<std::time::SystemTime>,
    ),
}
impl Content {
    fn path(&self) -> Result<&Path> {
        match self {
            Self::Unknown => bail!("Patcher InstallList file content is unknown during simulation"),
            Self::Disk(path) => Ok(path),
            Self::Scratch(file, _, _) => Ok(file.path()),
        }
    }
    fn open(&self) -> Result<fs::File> {
        Ok(fs::File::open(self.path()?)?)
    }

    fn modified(&self) -> Result<Option<std::time::SystemTime>> {
        match self {
            Self::Unknown => Ok(None),
            Self::Disk(path) => Ok(Some(fs::metadata(path)?.modified()?)),
            Self::Scratch(_, _, modified) => Ok(*modified),
        }
    }
}

/// The glob half of Extract/Copy source matching; a source also matches every file
/// below it as a folder (see `Engine::source_matches`).
pub(crate) fn source_glob_matches(pattern: &glob::Pattern, candidate: &str) -> bool {
    pattern.matches_with(
        candidate,
        glob::MatchOptions {
            case_sensitive: false,
            require_literal_separator: true,
            require_literal_leading_dot: false,
        },
    )
}

/// File names the FOMOD gate treats as archives; also the folder-source heuristic
/// for run-scoped FOMOD choices.
pub(crate) fn is_archive(name: &str) -> bool {
    matches!(
        Path::new(name)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("zip" | "7z" | "rar" | "exe" | "tar" | "gz")
    )
}

/// A resolver miss remains an error for strict validation and installation.
#[derive(Debug)]
struct MissingSource(String);
impl std::fmt::Display for MissingSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Source matches no files: {}", self.0)
    }
}
impl std::error::Error for MissingSource {}

/// Only the Copy/Move/Extract source-resolution boundary produces this cause.
/// In particular, Rename and configured FOMOD member failures stay hard errors.
#[derive(Debug)]
struct MissingSourceFiles(anyhow::Error);
impl std::fmt::Display for MissingSourceFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for MissingSourceFiles {}

pub struct Engine {
    pub settings: Settings,
    path_policy: crate::paths::PathPolicy,
    patcher_invocation: Option<crate::patcher::Invocation>,
    patcher_resolution_error: Option<String>,
    files: BTreeMap<String, (PathBuf, Content)>,
    directories: BTreeMap<String, PathBuf>,
    install: bool,
    component_checkpoints: bool,
    checkpoint_report: Option<CheckpointRunReport>,
    checkpoint_resume: Option<crate::checkpoint::ResumeRequest>,
    #[cfg(windows)]
    checkpoint_process: Option<(
        std::sync::Arc<recovery::Lock>,
        crate::checkpoint::OperationIntent,
    )>,
    managed: Option<managed::Run>,
    stage_managed_writes: bool,
    selected: BTreeSet<String>,
    completed: BTreeSet<String>,
    selected_names: BTreeSet<String>,
    conflicts: Option<crate::conflicts::Tracker>,
    validation_target: Option<(String, String, Option<usize>)>,
    validation_warnings: Vec<crate::validation::DryRunIssue>,
}

impl Engine {
    /// Opt in without adding required fields to the existing Settings contract.
    pub fn with_component_checkpoints(mut self, enabled: bool) -> Self {
        self.component_checkpoints = enabled;
        self
    }

    /// Resume only a validated checkpoint boundary; the shared store builds the
    /// immutable continuation plan while the engine holds the root lock.
    pub fn with_checkpoint_resume(
        mut self,
        request: crate::checkpoint::ResumeRequest,
    ) -> Result<Self> {
        ensure!(
            self.component_checkpoints,
            "Checkpoint resume requires component checkpoints"
        );
        ensure!(
            self.path_policy.case_insensitive,
            "Checkpoint resume requires case-insensitive path matching"
        );
        self.checkpoint_resume = Some(request);
        Ok(self)
    }

    pub fn with_managed_deployment(mut self, service: crate::deployment::Service) -> Result<Self> {
        ensure!(
            self.path_policy.case_insensitive,
            "Managed deployment requires case-insensitive path matching; exact policy is not encoded in managed manifests"
        );
        ensure!(
            self.component_checkpoints,
            "Managed installation requires component checkpoints"
        );
        ensure!(
            service.game_directory() == self.settings.roots.game,
            "Managed game root differs from engine game root"
        );
        for path in [service.staging_root(), service.manifest_root()] {
            ensure!(
                !path.starts_with(&self.settings.roots.mods)
                    && !self.settings.roots.mods.starts_with(path),
                "Managed participant overlaps mod root"
            );
        }
        service.validate_readonly()?;
        self.managed = Some(managed::Run::new(service));
        Ok(self)
    }

    pub fn new(settings: Settings) -> Result<Self> {
        Self::new_with_case_insensitive_pathing(settings, true)
    }

    /// Select path matching before the first scan so exact mode can retain
    /// distinct Unix filenames instead of silently folding their identities.
    pub fn new_with_case_insensitive_pathing(
        mut settings: Settings,
        case_insensitive: bool,
    ) -> Result<Self> {
        let path_policy = crate::paths::PathPolicy::new(case_insensitive)?;
        let scratch = fs::canonicalize(
            settings
                .archive
                .scratch_dir
                .clone()
                .unwrap_or_else(std::env::temp_dir),
        )?;
        ensure!(scratch.is_dir(), "Archive scratch path must be a directory");
        ensure!(
            !scratch.starts_with(&settings.roots.game)
                && !scratch.starts_with(&settings.roots.mods),
            "Archive scratch directory must be outside game and mod trees"
        );
        settings.archive.scratch_dir = Some(scratch);
        let (patcher_invocation, patcher_resolution_error) =
            match crate::patcher::resolve_invocation(
                settings.patcher_engine,
                settings.patcher.as_deref(),
            ) {
                Ok(invocation) => (invocation, None),
                Err(error) if settings.patcher.is_some() => return Err(error),
                Err(error) => (None, Some(format!("{error:#}"))),
            };
        settings.patcher = patcher_invocation
            .as_ref()
            .and_then(|invocation| invocation.native_path().map(Path::to_owned));
        let mut engine = Self {
            settings,
            path_policy,
            patcher_invocation,
            patcher_resolution_error,
            files: BTreeMap::new(),
            directories: BTreeMap::new(),
            install: false,
            component_checkpoints: false,
            checkpoint_report: None,
            checkpoint_resume: None,
            #[cfg(windows)]
            checkpoint_process: None,
            managed: None,
            stage_managed_writes: false,
            selected: BTreeSet::new(),
            completed: BTreeSet::new(),
            selected_names: BTreeSet::new(),
            conflicts: None,
            validation_target: None,
            validation_warnings: Vec::new(),
        };
        engine.scan()?;
        Ok(engine)
    }

    fn scan(&mut self) -> Result<()> {
        self.files.clear();
        self.directories.clear();
        for root in [&self.settings.roots.mods, &self.settings.roots.game] {
            let walk = walkdir::WalkDir::new(root)
                .into_iter()
                .filter_entry(|entry| {
                    root != &self.settings.roots.mods
                        || entry.depth() == 0
                        || !(entry.file_type().is_dir()
                            && crate::download_orchestrator::is_peer_staging(entry.file_name()))
                });
            for entry in walk {
                let entry = entry?;
                ensure!(
                    !entry.file_type().is_symlink(),
                    "Symlinks are unsupported in install trees: {}",
                    entry.path().display()
                );
                let path = entry.path().to_path_buf();
                ensure!(
                    self.path_policy.case_insensitive || path.to_str().is_some(),
                    "Exact path matching requires UTF-8 paths; refusing lossy filename identity"
                );
                if entry.file_type().is_file() {
                    ensure!(
                        self.files
                            .insert(
                                self.path_policy.key(&path),
                                (path.clone(), Content::Disk(path.clone()))
                            )
                            .is_none(),
                        "Case-colliding files: {}",
                        path.display()
                    );
                } else {
                    ensure!(
                        entry.file_type().is_dir(),
                        "Unsupported special file: {}",
                        path.display()
                    );
                    ensure!(
                        self.directories
                            .insert(self.path_policy.key(&path), path.clone())
                            .is_none(),
                        "Case-colliding directories: {}",
                        path.display()
                    );
                }
            }
        }
        Ok(())
    }

    pub fn validate(mut self, guide: &Guide, mut emit: impl FnMut(Event)) -> Result<()> {
        recovery::ensure_clean(&self.settings.roots)?;
        recovery::verify_completion(&self.settings.roots, guide, self.completed.len())?;
        // C# runs FomodConfigurationGate on every validate path, not only the report.
        self.fomod_gate(guide, &mut emit)?;
        self.run(guide, &mut emit)
    }

    /// The read-only FOMOD configuration gate for plain validate and install. Issues
    /// fail before any simulation or write; warnings are emitted as events.
    fn fomod_gate(&mut self, guide: &Guide, emit: &mut impl FnMut(Event)) -> Result<()> {
        let warnings = self
            .validate_fomod_configuration(guide)
            .context("FOMOD configuration check failed")?;
        for warning in warnings {
            emit(Event {
                component: String::new(),
                step: 0,
                message: format!("WARNING: {warning}"),
            });
        }
        Ok(())
    }

    pub fn validate_report(
        self,
        guide: &Guide,
        emit: impl FnMut(Event),
    ) -> crate::validation::Report {
        self.validate_report_with_dry_run(guide, emit, false)
    }

    pub fn validate_report_with_dry_run(
        self,
        guide: &Guide,
        emit: impl FnMut(Event),
        dry_run: bool,
    ) -> crate::validation::Report {
        self.validate_report_with_options(guide, emit, true, dry_run, false)
    }

    pub fn validate_report_with_options(
        self,
        guide: &Guide,
        emit: impl FnMut(Event),
        full_validation: bool,
        dry_run: bool,
        dry_run_only: bool,
    ) -> crate::validation::Report {
        self.validate_report_with_options_observed(
            guide,
            emit,
            full_validation,
            dry_run,
            dry_run_only,
            |_| {},
        )
    }

    /// Observe actual stage starts and completions without changing ordinary events.
    #[allow(clippy::too_many_arguments)]
    pub fn validate_report_with_options_observed(
        self,
        guide: &Guide,
        mut emit: impl FnMut(Event),
        full_validation: bool,
        dry_run: bool,
        dry_run_only: bool,
        mut progress: impl FnMut(crate::validation::StageProgress),
    ) -> crate::validation::Report {
        use crate::validation::{Stage, StageProgress};
        let started = std::time::Instant::now();
        let finish = |report: crate::validation::Report| {
            let report = report.finish();
            crate::telemetry::global().record_validation(
                "guide",
                report.success,
                report.error_count + report.warning_count,
                started.elapsed(),
            );
            report
        };
        let total = usize::from(full_validation) * 3
            + usize::from(!dry_run_only)
            + 1
            + usize::from(dry_run);
        let mut step = 0;
        let mut boundary = |stage, message: &str, complete: bool| {
            if !complete {
                step += 1;
            }
            progress(StageProgress {
                stage,
                step,
                total,
                message: if complete {
                    format!("Completed: {message}")
                } else {
                    message.into()
                },
            });
        };
        let settings = self.settings.clone();
        let fresh_engine = || -> Result<Engine> {
            let mut engine = Engine::new_with_case_insensitive_pathing(
                settings.clone(),
                self.path_policy.case_insensitive,
            )?;
            engine.completed = self.completed.clone();
            Ok(engine)
        };
        let validated_components: Vec<_> = guide
            .components
            .iter()
            .filter(|component| {
                component.is_selected
                    && !self
                        .completed
                        .contains(&component.guid.to_ascii_lowercase())
            })
            .collect();
        let mut report = crate::validation::Report::new(validated_components.len());
        if full_validation {
            boundary(
                Stage::Environment,
                "Validating installation environment",
                false,
            );
            let environment = (|| -> Result<()> {
                recovery::ensure_clean(&self.settings.roots)?;
                recovery::verify_completion(&self.settings.roots, guide, self.completed.len())
            })()
            .map_err(|error| format!("{error:#}"));
            let passed = report.push(crate::validation::Stage::Environment, environment);
            boundary(
                Stage::Environment,
                "Validating installation environment",
                true,
            );
            if !passed {
                return finish(report);
            }
            boundary(Stage::Conflicts, "Checking conflicts", false);
            let mut selected = guide.selected_ids();
            selected.extend(self.completed.iter().cloned());
            report.declared_conflicts(guide, &validated_components, &selected);
            boundary(Stage::Conflicts, "Checking conflicts", true);
            boundary(
                Stage::InstallOrder,
                "Validating mod installation order",
                false,
            );
            let order = guide
                .ordered()
                .map(|_| ())
                .map_err(|error| format!("{error:#}"));
            report.install_order(order);
            boundary(
                Stage::InstallOrder,
                "Validating mod installation order",
                true,
            );
        }
        if !dry_run_only {
            boundary(Stage::ComponentValidation, "Validating mod archives", false);
            let component = fresh_engine()
                .and_then(|engine| engine.validate_archives(guide, &mut emit))
                .map_err(|error| format!("{error:#}"));
            report.component_validation(&validated_components, component);
            boundary(Stage::ComponentValidation, "Validating mod archives", true);
        }
        boundary(
            Stage::FomodConfiguration,
            "Checking FOMOD configuration",
            false,
        );
        let fomod = fresh_engine()
            .and_then(|mut engine| engine.validate_fomod_configuration(guide))
            .map_err(|error| format!("{error:#}"));
        report.fomod_configuration(fomod);
        boundary(
            Stage::FomodConfiguration,
            "Checking FOMOD configuration",
            true,
        );
        if dry_run {
            boundary(
                Stage::DryRun,
                "Running instruction execution validation (dry-run)",
                false,
            );
            let issues = match fresh_engine() {
                Ok(engine) => engine.validate_dry_run(guide),
                Err(error) => vec![crate::validation::DryRunIssue::engine(
                    format!("{error:#}"),
                    None,
                )],
            };
            report.dry_run(issues);
            boundary(
                Stage::DryRun,
                "Running instruction execution validation (dry-run)",
                true,
            );
        }
        finish(report)
    }

    fn validate_dry_run(mut self, guide: &Guide) -> Vec<crate::validation::DryRunIssue> {
        if let Err(error) = recovery::ensure_clean(&self.settings.roots).and_then(|_| {
            recovery::verify_completion(&self.settings.roots, guide, self.completed.len())
        }) {
            return vec![crate::validation::DryRunIssue::engine(
                format!("{error:#}"),
                None,
            )];
        }
        let ordered = match guide.ordered() {
            Ok(ordered) => ordered,
            Err(error) => {
                return vec![crate::validation::DryRunIssue::engine(
                    format!("{error:#}"),
                    None,
                )];
            }
        };
        self.selected = guide.selected_ids();
        self.selected_names = guide
            .components
            .iter()
            .filter(|component| component.is_selected)
            .map(|component| component.name.to_lowercase())
            .collect();
        let mut issues = Vec::new();
        for component in ordered {
            if self
                .completed
                .contains(&component.guid.to_ascii_lowercase())
            {
                continue;
            }
            self.validation_target = Some((component.name.clone(), component.guid.clone(), None));
            let component_issue_start = issues.len();
            if component.instructions.is_empty() {
                issues.push(crate::validation::DryRunIssue::engine(
                    format!(
                        "{} has no executable instructions; review the guide",
                        component.name
                    ),
                    self.validation_target.clone(),
                ));
                continue;
            }
            if let Err(error) = self.check_component(component) {
                issues.push(crate::validation::DryRunIssue::engine(
                    format!("{error:#}"),
                    self.validation_target.clone(),
                ));
                continue;
            }
            for (index, instruction) in component.instructions.iter().enumerate() {
                self.validation_target = Some((
                    component.name.clone(),
                    component.guid.clone(),
                    Some(index + 1),
                ));
                if !self.instruction_applies(instruction) {
                    continue;
                }
                let result = self
                    .check_cancelled()
                    .and_then(|_| self.execute(component, instruction, &mut |_| {}))
                    .with_context(|| {
                        format!(
                            "{} / step {} ({})",
                            component.name,
                            index + 1,
                            instruction.action
                        )
                    });
                issues.append(&mut self.validation_warnings);
                if let Err(error) = result {
                    if error.is::<MissingSourceFiles>() {
                        // Keep successful virtual effects, but replace this component's
                        // provisional warnings with its single component-level outcome.
                        issues.truncate(component_issue_start);
                        issues.push(crate::validation::DryRunIssue::skipped(component));
                        break;
                    }
                    let message = format!("{error:#}");
                    let mut issue = crate::validation::DryRunIssue::engine(
                        message.clone(),
                        self.validation_target.clone(),
                    );
                    if let Some(missing) = error.downcast_ref::<MissingSource>() {
                        issue.affected_path = Some(missing.0.clone());
                    }
                    issues.push(issue);
                    if self.settings.cancelled.load(Ordering::Relaxed) {
                        return issues;
                    }
                    break;
                }
            }
        }
        issues
    }

    /// Read-only archive/destination checks; never execute installation instructions.
    fn validate_archives(
        mut self,
        guide: &Guide,
        emit: &mut impl FnMut(Event),
    ) -> Result<Vec<Result<(), String>>> {
        recovery::ensure_clean(&self.settings.roots)?;
        recovery::verify_completion(&self.settings.roots, guide, self.completed.len())?;
        self.selected = guide.selected_ids();
        self.selected.extend(self.completed.iter().cloned());
        let mut outcomes = Vec::new();
        for component in guide.components.iter().filter(|component| {
            component.is_selected
                && !self
                    .completed
                    .contains(&component.guid.to_ascii_lowercase())
        }) {
            let outcome = self
                .validate_component_archives(component)
                .with_context(|| format!("{}: archive/destination validation", component.name))
                .map_err(|error| format!("{error:#}"));
            emit(Event {
                component: component.name.clone(),
                step: 0,
                message: "Archive and destination checks completed".into(),
            });
            outcomes.push(outcome);
        }
        Ok(outcomes)
    }

    fn validate_component_archives(&self, component: &Component) -> Result<()> {
        self.check_cancelled()?;
        // The reference archive stage includes immediate option instructions.
        let instructions: Vec<_> = component
            .instructions
            .iter()
            .chain(
                component
                    .options
                    .iter()
                    .flat_map(|option| &option.instructions),
            )
            .filter(|instruction| self.instruction_applies(instruction))
            .collect();
        let mut members = BTreeSet::new();
        let mut has_archives = false;
        for instruction in &instructions {
            self.check_cancelled()?;
            if instruction.action != "Extract" {
                continue;
            }
            for source in self.matches(&instruction.source, false)? {
                has_archives = true;
                let dest = self.extraction_destination(instruction, &source)?;
                self.settings.roots.check(&dest)?;
                ensure!(
                    !self.files.contains_key(&self.path_policy.key(&dest)),
                    "File blocks extraction destination: {}",
                    dest.display()
                );
                let entries = archive::unpack(
                    self.files[&self.path_policy.key(&source)].1.path()?,
                    &source,
                    &self.settings.archive,
                    &self.settings.cancelled,
                )
                .with_context(|| format!("Cannot inspect archive {}", source.display()))?;
                for entry in entries {
                    let path = dest.join(entry.path);
                    self.settings.roots.check(&path)?;
                    members.insert(self.path_policy.key(&path));
                }
            }
        }
        for instruction in instructions {
            self.check_cancelled()?;
            match instruction.action.as_str() {
                "Copy" | "Move" | "CleanList" | "DelDuplicate" => {
                    let dest = self.path(&instruction.destination, false)?;
                    ensure!(
                        self.directories.contains_key(&self.path_policy.key(&dest)),
                        "Destination cannot be found: {}",
                        instruction.destination
                    );
                }
                "Rename" if !instruction.destination.starts_with("<<") => {
                    ensure!(
                        relative(&instruction.destination, false)?
                            .components()
                            .count()
                            == 1,
                        "Rename needs one filename or a placeholder path"
                    );
                }
                "Rename" | "Patcher" => {
                    let dest = self.path(&instruction.destination, false)?;
                    if instruction.action == "Patcher" {
                        ensure!(
                            dest == self.settings.roots.game,
                            "Patcher destination must be the game directory"
                        );
                    }
                }
                "Extract" | "Choose" | "Delete" | "Execute" | "Run" => {}
                other => bail!("Unsupported action {other}; refusing to skip it"),
            }
            // Source existence is deliberately not a loose-file check here.
            // When Extract archives exist, check their actual safe member paths.
            if matches!(
                instruction.action.as_str(),
                "Choose" | "DelDuplicate" | "Extract"
            ) {
                continue;
            }
            for source in &instruction.source {
                let (path, pattern) = self.source_pattern(source)?;
                if !has_archives || path.starts_with(&self.settings.roots.game) {
                    continue;
                }
                let prefix = format!("{}/", self.path_policy.key(&path));
                ensure!(
                    members
                        .iter()
                        .any(|member| self.source_matches(&prefix, &pattern, member)),
                    "Failed to find '{source}' in any archives"
                );
            }
        }
        Ok(())
    }

    /// Returns warnings for configured registered archives that no selected Extract
    /// instruction installs: choices are valid, but configuration installs no files.
    /// Mapper fallbacks of each checked installer are logged once per archive, as C#
    /// logs them, outside the report. Reads archives into scratch only; never writes
    /// the game or mod trees.
    fn validate_fomod_configuration(&mut self, guide: &Guide) -> Result<Vec<String>> {
        fn configured_choices(
            component: &Component,
            filename: &str,
            module: &crate::fomod::ArchiveModule,
        ) -> Result<()> {
            let choices = crate::fomod::required_saved_choices(component, filename)?;
            crate::fomod::current_archive_choices(module, &choices, filename)
                .with_context(|| crate::fomod::stale_choices_context(filename))?;
            Ok(())
        }

        // C# gates registered download artifacts even when no Extract instruction names them.
        // A previously recorded prompt state makes a missing artifact fail closed.
        self.selected = guide.selected_ids();
        self.selected.extend(self.completed.iter().cloned());
        let mut scope = Vec::new();
        for component in guide.ordered()? {
            if self
                .completed
                .contains(&component.guid.to_ascii_lowercase())
            {
                continue;
            }
            self.collect_execution_components(component, &mut scope)?;
        }
        // Registered archives with valid choices, by path: filename and registrants.
        let mut configured = BTreeMap::<String, (String, Vec<String>)>::new();
        // (archive path, component) pairs already checked by the registered pass.
        let mut checked = BTreeSet::new();
        // Each archive path is inspected once per gate, whichever component names it.
        let mut inspected = BTreeMap::<String, Option<crate::fomod::ArchiveModule>>::new();
        let mut routes = BTreeSet::new();
        // Checked FOMOD archive paths whose mapper fallbacks were already logged.
        let mut logged = BTreeSet::new();
        let mut warnings = crate::fomod::WarningCollector::default();
        for component in &scope {
            let mut registered = BTreeSet::new();
            for resource in component.resource_registry.values() {
                let Some(files) = resource
                    .get("Files")
                    .or_else(|| resource.get("files"))
                    .and_then(serde_json::Value::as_object)
                else {
                    continue;
                };
                for name in files.keys() {
                    if !is_archive(name) || !registered.insert(crate::fomod::fold_case(name)) {
                        continue;
                    }
                    let relative_name = relative(name, false).with_context(|| {
                        format!("Registered FOMOD archive path is unsafe: {name}")
                    })?;
                    let path = self.settings.roots.mods.join(&relative_name);
                    let filename = relative_name
                        .file_name()
                        .and_then(|name| name.to_str())
                        .context("Registered FOMOD archive filename is invalid")?;
                    if !path.is_file() {
                        if crate::fomod::prompt_status(component, filename).is_some() {
                            bail!(
                                "Registered FOMOD archive '{filename}' is missing from the mod directory after configuration state was recorded"
                            );
                        }
                        warnings.push(format!(
                            "Registered archive '{}' is missing from the mod directory. Re-download it, then configure its FOMOD installer if present before installation.",
                            crate::fomod::warning_text(filename)
                        ));
                        continue;
                    }
                    checked.insert((self.path_policy.key(&path), component.guid.clone()));
                    let module = match inspected.entry(self.path_policy.key(&path)) {
                        btree_map::Entry::Occupied(entry) => entry.into_mut(),
                        btree_map::Entry::Vacant(entry) => entry.insert(
                            crate::fomod::inspect_listed(
                                &path,
                                &relative_name,
                                &self.settings.archive,
                                &self.settings.cancelled,
                            )
                            .with_context(|| {
                                format!("Registered archive '{filename}' cannot be inspected")
                            })?,
                        ),
                    };
                    if let Some(module) = module {
                        configured_choices(component, filename, module)?;
                        if logged.insert(self.path_policy.key(&path)) {
                            for warning in module.config.fallback_warnings() {
                                warnings.push(format!(
                                    "FOMOD archive '{}': {warning}",
                                    crate::fomod::warning_text(filename)
                                ));
                            }
                        }
                        configured
                            .entry(self.path_policy.key(&path))
                            .or_insert_with(|| (filename.to_owned(), Vec::new()))
                            .1
                            .push(component.name.clone());
                    }
                }
            }
        }
        for component in scope {
            for instruction in &component.instructions {
                if instruction.action != "Extract" || !self.instruction_applies(instruction) {
                    continue;
                }
                // Missing generic Extract inputs belong to archive validation and
                // dry-run availability, not the independent FOMOD state gate.
                for source in self.matches(&instruction.source, true)? {
                    // Any applying Extract of this path is an install route, FOMOD or not.
                    routes.insert(self.path_policy.key(&source));
                    if checked.contains(&(self.path_policy.key(&source), component.guid.clone())) {
                        continue;
                    }
                    let module = match inspected.entry(self.path_policy.key(&source)) {
                        btree_map::Entry::Occupied(entry) => entry.into_mut(),
                        btree_map::Entry::Vacant(entry) => {
                            entry.insert(crate::fomod::inspect_listed(
                                self.files[&self.path_policy.key(&source)].1.path()?,
                                &source,
                                &self.settings.archive,
                                &self.settings.cancelled,
                            )?)
                        }
                    };
                    let Some(module) = module else {
                        continue;
                    };
                    let filename = source
                        .file_name()
                        .and_then(|name| name.to_str())
                        .context("FOMOD archive filename is invalid")?;
                    configured_choices(component, filename, module)?;
                    if logged.insert(self.path_policy.key(&source)) {
                        for warning in module.config.fallback_warnings() {
                            warnings.push(format!(
                                "FOMOD archive '{}': {warning}",
                                crate::fomod::warning_text(filename)
                            ));
                        }
                    }
                }
            }
        }
        // C# FomodConfigurationGate soft-warns "configured" without archive-scoped
        // instructions. Rust keeps choices, so the analogue is a missing Extract route.
        // One warning per archive, naming every registering component.
        for warning in configured
            .into_iter()
            .filter(|(path, _)| !routes.contains(path))
            .map(|(_, (filename, names))| {
                let omitted = names.len().saturating_sub(8);
                let names = names
                    .iter()
                    .take(8)
                    .map(|name| format!("'{}'", crate::fomod::warning_text(name)))
                    .collect::<Vec<_>>()
                    .join(", ");
                let extra = if omitted > 0 { format!(" and {omitted} other components") } else { String::new() };
                let filename = crate::fomod::warning_text(&filename);
                format!(
                    "FOMOD archive '{filename}' has valid choices, but no selected Extract instruction installs it; configuration alone installs no files. Registered by {names}{extra}."
                )
            })
        { warnings.push(warning); }
        Ok(warnings.finish())
    }

    /// Analyze actual simulated writes without changing either installation tree.
    /// Failed instructions return an error rather than an incomplete success report.
    pub fn analyze_conflicts(mut self, guide: &Guide) -> Result<crate::conflicts::Report> {
        ensure!(
            self.path_policy.case_insensitive,
            "Conflict reports currently require case-insensitive path matching"
        );
        recovery::ensure_clean(&self.settings.roots)?;
        self.conflicts = Some(crate::conflicts::Tracker::default());
        self.run(guide, &mut |_| {})?;
        Ok(self.conflicts.take().unwrap().finish())
    }

    pub fn install(self, guide: &Guide, emit: impl FnMut(Event)) -> Result<()> {
        self.install_observed(guide, emit, |_| {})
    }

    /// Observe real installation boundaries, excluding all silent preflight work.
    /// The observer is synchronous and non-fallible; it must not panic. Adapters
    /// may ignore failed channel delivery. Completed and Ok win over late cancel.
    pub fn install_observed(
        self,
        guide: &Guide,
        emit: impl FnMut(Event),
        progress: impl FnMut(InstallProgress),
    ) -> Result<()> {
        self.install_observed_report(guide, emit, progress)
            .map(|_| ())
    }

    pub fn install_observed_report(
        mut self,
        guide: &Guide,
        mut emit: impl FnMut(Event),
        mut progress: impl FnMut(InstallProgress),
    ) -> Result<InstallReport> {
        ensure!(
            self.path_policy.case_insensitive
                || (!self.component_checkpoints
                    && self.checkpoint_resume.is_none()
                    && self.managed.is_none()),
            "Exact path matching cannot be combined with managed deployment or checkpoint history"
        );
        #[cfg(windows)]
        let lock = if self.component_checkpoints {
            let participants = self
                .managed
                .as_ref()
                .map(|managed| managed.service.checkpoint_participants())
                .transpose()?
                .unwrap_or_default();
            let context = crate::checkpoint::InstallContext {
                participants,
                ..crate::checkpoint::InstallContext::default()
            };
            crate::checkpoint::Store::open(&self.settings.roots)?
                .acquire_lock_with_context(&context)?
        } else {
            recovery::Lock::acquire(&self.settings.roots)?
        };
        #[cfg(not(windows))]
        let lock = recovery::Lock::acquire(&self.settings.roots)?;
        let lock = std::sync::Arc::new(lock);
        ensure!(
            self.managed.is_none() || self.component_checkpoints,
            "Managed installation requires component checkpoints"
        );
        ensure!(
            self.checkpoint_resume.is_none() || self.component_checkpoints,
            "Checkpoint resume requires component checkpoints"
        );
        let resume_plan = if let Some(request) = &self.checkpoint_resume {
            let definitions = guide
                .ordered()?
                .iter()
                .map(serde_json::to_value)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let participants = self
                .managed
                .as_ref()
                .map(|managed| managed.service.checkpoint_participants())
                .transpose()?
                .unwrap_or_default();
            let store = crate::checkpoint::Store::open(&self.settings.roots)?;
            let plan = store.resume_plan(
                request,
                &definitions,
                &participants,
                &self.settings.cancelled,
            )?;
            let completed = plan
                .completed_components
                .iter()
                .map(|definition| {
                    definition
                        .get("Guid")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_ascii_lowercase)
                        .context("Resume prefix has no component Guid")
                })
                .collect::<Result<BTreeSet<_>>>()?;
            ensure!(
                self.completed.is_subset(&completed),
                "Requested phase prefix advances beyond the checkpoint resume boundary"
            );
            self.completed = completed;
            if !plan.source_changes.is_empty() || plan.source_root_mode_changed {
                emit(Event {
                    component: String::new(),
                    step: 0,
                    message: format!(
                        "Accepted source changes for checkpoint branch: {}",
                        serde_json::to_string(
                            &serde_json::json!({"paths": &plan.source_changes, "rootModeChanged": plan.source_root_mode_changed})
                        )?
                    ),
                });
            }
            Some(plan)
        } else {
            recovery::ensure_clean(&self.settings.roots)?;
            None
        };
        if let Some(managed) = &self.managed {
            managed.service.validate_readonly()?;
        }
        self.scan()?;
        // A component HEAD can be newer than the legacy receipt until Finalize.
        // ResumePlan already proves the exact checkpoint prefix and live inventory;
        // its branch transaction reconciles the legacy receipt before execution.
        if resume_plan.is_none() {
            recovery::verify_completion(&self.settings.roots, guide, self.completed.len())?;
        }
        self.selected = guide.selected_ids();
        self.selected_names = guide
            .components
            .iter()
            .filter(|component| component.is_selected)
            .map(|component| component.name.to_lowercase())
            .collect();
        // C# InstallationService blocks installation on FomodConfigurationGate issues.
        self.fomod_gate(guide, &mut emit)
            .context("Installation blocked before any write")?;
        // Managed reinstall restores originals before each component. Simulating
        // against the old deployed tree would validate the wrong inputs. Its
        // source preflight runs inside the journaled boundary after reinstall.
        if self.managed.is_none() {
            self.run(guide, &mut |_| {})?;
        } else {
            for component in self.remaining_ordered(guide)? {
                self.check_component(component)?;
            }
        }
        self.scan()?;
        let ordered = self.remaining_ordered(guide)?;
        // Pure validation needs no backend or process permission. Installation still
        // rejects missing authority/backend before taking its recovery snapshot.
        for component in &ordered {
            let mut scope = Vec::new();
            self.collect_execution_components(component, &mut scope)?;
            for owner in scope {
                self.check_component(owner)?;
                for instruction in &owner.instructions {
                    if matches!(instruction.action.as_str(), "Execute" | "Run")
                        && self.instruction_applies(instruction)
                    {
                        ensure!(
                            self.settings.allow_processes,
                            "{} needs explicit trust: enable external programs (--allow-processes)",
                            instruction.action
                        );
                        ensure!(
                            !instruction.source.is_empty(),
                            "External program instruction has no Source"
                        );
                        process_arguments::expand(&instruction.arguments, &self.settings.roots)?;
                    }
                    if instruction.action == "Patcher" && self.instruction_applies(instruction) {
                        ensure!(
                            instruction.source.len() == 1,
                            "Patcher needs exactly one source"
                        );
                        ensure!(
                            self.settings.allow_processes,
                            "Patcher needs explicit trust: enable external programs (--allow-processes)"
                        );
                        if let Some(error) = &self.patcher_resolution_error {
                            bail!("Patcher backend unavailable: {error}");
                        }
                        let invocation = self.patcher_invocation.as_ref().context(
                            "Configure an executable with --patcher or install complete packaged patcher resources",
                        )?;
                        // Prepare and validate the argv prefix without launching a
                        // process or requiring a backend during pure simulation.
                        let _ = invocation.command()?;
                    }
                }
            }
        }
        let mut observer = InstallObserver::new(ordered.len(), &mut progress);
        observer.emit(InstallPhase::Preparing, "Installation preflight completed");
        emit(Event {
            component: String::new(),
            step: 0,
            message: "Creating recovery snapshot of game and mod directories".into(),
        });
        self.check_cancelled()?;
        observer.emit(
            InstallPhase::CreatingSnapshot,
            "Creating recovery snapshot of game and mod directories",
        );
        if resume_plan.is_none() {
            recovery::begin(&self.settings.roots)?;
        }
        observer.current.snapshot_pending = true;
        observer.emit(
            InstallPhase::CreatingSnapshot,
            if resume_plan.is_some() {
                "Preparing verified checkpoint continuation branch"
            } else {
                "Recovery snapshot active"
            },
        );
        self.install = true;
        if self.component_checkpoints {
            self.run_checkpointed(
                guide,
                &ordered,
                &lock,
                resume_plan.as_ref(),
                &mut emit,
                &mut observer,
            )?;
        } else {
            self.run_ordered(guide, &ordered, &mut emit, Some(&mut observer))
                .with_context(|| {
                    format!(
                        "Install stopped. Restore before retrying; snapshot: {}",
                        recovery::state_dir(&self.settings.roots).display()
                    )
                })?;
            observer.clear_component();
            observer.emit(
                InstallPhase::RecordingCompletion,
                "Recording installation completion",
            );
            recovery::record_completion(&self.settings.roots, guide)?;
            recovery::finish(&self.settings.roots)?;
        }
        observer.current.snapshot_pending = false;
        observer.emit(InstallPhase::Completed, "Installation complete");
        Ok(InstallReport {
            managed: self.managed.map(|managed| managed.report),
            checkpoint: self.checkpoint_report,
        })
    }

    /// Execute the remaining suffix while retaining selected dependency context.
    /// A saved receipt must match roots, prefix definitions and current game bytes.
    pub fn install_remaining(
        self,
        guide: &Guide,
        completed: &[String],
        emit: impl FnMut(Event),
    ) -> Result<()> {
        self.install_remaining_observed(guide, completed, emit, |_| {})
    }

    /// Observe the same verified suffix execution as install_remaining.
    pub fn install_remaining_observed(
        mut self,
        guide: &Guide,
        completed: &[String],
        emit: impl FnMut(Event),
        progress: impl FnMut(InstallProgress),
    ) -> Result<()> {
        self.set_completed_prefix(guide, completed)?;
        self.install_observed(guide, emit, progress)
    }

    pub fn install_remaining_observed_report(
        mut self,
        guide: &Guide,
        completed: &[String],
        emit: impl FnMut(Event),
        progress: impl FnMut(InstallProgress),
    ) -> Result<InstallReport> {
        self.set_completed_prefix(guide, completed)?;
        self.install_observed_report(guide, emit, progress)
    }

    /// Simulate only the suffix, with the same saved completion contract
    /// as install_remaining. Does not write install trees or run external programs.
    pub fn validate_remaining(
        mut self,
        guide: &Guide,
        completed: &[String],
        emit: impl FnMut(Event),
    ) -> Result<()> {
        self.set_completed_prefix(guide, completed)?;
        self.validate(guide, emit)
    }

    /// Run the shared wizard report on the uninstalled suffix while retaining
    /// completed components as dependency context and verifying their receipt.
    pub fn validate_report_remaining(
        self,
        guide: &Guide,
        completed: &[String],
        emit: impl FnMut(Event),
    ) -> Result<crate::validation::Report> {
        self.validate_report_remaining_observed(guide, completed, emit, |_| {})
    }

    pub fn validate_report_remaining_observed(
        mut self,
        guide: &Guide,
        completed: &[String],
        emit: impl FnMut(Event),
        progress: impl FnMut(crate::validation::StageProgress),
    ) -> Result<crate::validation::Report> {
        self.set_completed_prefix(guide, completed)?;
        Ok(self.validate_report_with_options_observed(guide, emit, true, true, false, progress))
    }

    /// Analyze only the uninstalled suffix under the same verified completion
    /// contract as validate_remaining. Neither install tree is changed.
    pub fn analyze_conflicts_remaining(
        mut self,
        guide: &Guide,
        completed: &[String],
    ) -> Result<crate::conflicts::Report> {
        self.set_completed_prefix(guide, completed)?;
        recovery::verify_completion(&self.settings.roots, guide, self.completed.len())?;
        self.analyze_conflicts(guide)
    }

    fn set_completed_prefix(&mut self, guide: &Guide, completed: &[String]) -> Result<()> {
        let ordered = guide.ordered()?;
        let ids: BTreeSet<_> = completed.iter().map(|id| id.to_ascii_lowercase()).collect();
        ensure!(
            ids.len() == completed.len(),
            "Duplicate completed component ID"
        );
        ensure!(
            ids.len() < ordered.len(),
            "No remaining components to install"
        );
        let prefix: BTreeSet<_> = ordered
            .iter()
            .take(ids.len())
            .map(|c| c.guid.to_ascii_lowercase())
            .collect();
        ensure!(
            ids == prefix,
            "Completed components must form the selected installation prefix"
        );
        self.completed = ids;
        Ok(())
    }

    fn check_cancelled(&self) -> Result<()> {
        ensure!(
            !self.settings.cancelled.load(Ordering::Relaxed),
            "Cancelled; no later instructions ran"
        );
        Ok(())
    }

    fn remaining_ordered<'a>(&self, guide: &'a Guide) -> Result<Vec<&'a Component>> {
        Ok(guide
            .ordered()?
            .into_iter()
            .filter(|c| !self.completed.contains(&c.guid.to_ascii_lowercase()))
            .collect())
    }

    fn run(&mut self, guide: &Guide, emit: &mut impl FnMut(Event)) -> Result<()> {
        let ordered = self.remaining_ordered(guide)?;
        self.run_ordered(guide, &ordered, emit, None)
    }

    fn run_ordered(
        &mut self,
        guide: &Guide,
        ordered: &[&Component],
        emit: &mut impl FnMut(Event),
        mut progress: Option<&mut InstallObserver<'_>>,
    ) -> Result<()> {
        self.selected = guide.selected_ids();
        self.selected_names = guide
            .components
            .iter()
            .filter(|c| c.is_selected)
            .map(|c| c.name.to_lowercase())
            .collect();
        for (index, c) in ordered.iter().enumerate() {
            ensure!(
                !c.instructions.is_empty(),
                "{} has no executable instructions; review the guide",
                c.name
            );
            if let Some(tracker) = &mut self.conflicts {
                tracker.begin(c);
            }
            if let Some(observer) = progress.as_deref_mut() {
                observer.component(c, index + 1);
                observer.emit(
                    InstallPhase::InstallingComponent,
                    format!("Installing: {}", c.name),
                );
            }
            let started = std::time::Instant::now();
            let result = self.component(c, emit, progress.as_deref_mut());
            if self.install {
                let error = result.as_ref().err().map(|error| format!("{error:#}"));
                crate::telemetry::global().record_mod_installation(
                    &c.name,
                    result.is_ok(),
                    started.elapsed(),
                    error.as_deref(),
                );
            }
            result?;
            if let Some(observer) = progress.as_deref_mut() {
                observer.component(c, index + 1);
                observer.emit(
                    InstallPhase::ComponentCompleted,
                    format!("Installed: {}", c.name),
                );
            }
        }
        Ok(())
    }

    fn component(
        &mut self,
        c: &Component,
        emit: &mut impl FnMut(Event),
        mut progress: Option<&mut InstallObserver<'_>>,
    ) -> Result<()> {
        let started = std::time::Instant::now();
        let actual_install = self.install;
        let result: Result<()> = (|| {
            self.validation_target = Some((c.name.clone(), c.guid.clone(), None));
            self.check_component(c)?;
            for (n, instruction) in c.instructions.iter().enumerate() {
                self.validation_target = Some((c.name.clone(), c.guid.clone(), Some(n + 1)));
                self.check_cancelled()?;
                if !self.instruction_applies(instruction) {
                    emit(Event {
                        component: c.name.clone(),
                        step: n + 1,
                        message: "Skipped by instruction conditions".into(),
                    });
                    if let Some(observer) = progress.as_deref_mut() {
                        observer.instruction(c, n + 1, "Skipped by instruction conditions".into());
                    }
                    continue;
                }
                let message = if instruction.action == "Patcher" && !self.install {
                    "Patcher: InstallList presence simulated; binary patch effects remain unmodeled"
                        .into()
                } else if matches!(instruction.action.as_str(), "Execute" | "Run") && !self.install
                {
                    format!(
                        "{}: inputs checked; external effects cannot be simulated",
                        instruction.action
                    )
                } else {
                    instruction.action.clone()
                };
                if let Some(tracker) = &mut self.conflicts {
                    if let Some(writer) = &mut tracker.current
                        && writer.component_guid.eq_ignore_ascii_case(&c.guid)
                    {
                        writer.instruction_index = n;
                    }
                    tracker.external_effects |=
                        matches!(instruction.action.as_str(), "Patcher" | "Execute" | "Run");
                }
                if let Some(observer) = progress.as_deref_mut() {
                    // This is context for reached Choose frames, not an emitted success.
                    observer.current.instruction_index = Some(n + 1);
                }
                self.stage_managed_writes = self.install
                    && matches!(
                        instruction.action.as_str(),
                        "Extract" | "Copy" | "Move" | "Rename"
                    );
                if let Some(managed) = &mut self.managed {
                    managed.action = instruction.action.clone();
                }
                let instruction_result =
                    self.execute_observed(c, instruction, emit, progress.as_deref_mut());
                self.stage_managed_writes = false;
                instruction_result.with_context(|| {
                    format!("{} / step {} ({})", c.name, n + 1, instruction.action)
                })?;
                emit(Event {
                    component: c.name.clone(),
                    step: n + 1,
                    message: message.clone(),
                });
                if let Some(observer) = progress.as_deref_mut() {
                    observer.instruction(c, n + 1, message);
                }
            }
            Ok(())
        })();
        if actual_install {
            let error = result.as_ref().err().map(|error| format!("{error:#}"));
            crate::telemetry::global().record_component_execution(
                &c.name,
                result.is_ok(),
                c.instructions.len(),
                started.elapsed(),
                error.as_deref(),
            );
        }
        result
    }

    fn check_component(&self, c: &Component) -> Result<()> {
        self.check_cancelled()?;
        ensure!(
            !crate::draft::requires_review(c),
            "{} has unreviewed draft instructions; review them before installation",
            c.name
        );
        ensure!(
            c.metadata
                .get("UnresolvedMasters")
                .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty)),
            "{} has unresolved Markdown masters; review its dependencies",
            c.name
        );
        for dependency in &c.dependencies {
            ensure!(
                self.selected.contains(&dependency.to_ascii_lowercase()),
                "{} requires {dependency}",
                c.name
            );
        }
        for restriction in &c.restrictions {
            ensure!(
                !self.selected.contains(&restriction.to_ascii_lowercase()),
                "{} conflicts with {restriction}",
                c.name
            );
        }
        Ok(())
    }

    fn instruction_applies(&self, instruction: &Instruction) -> bool {
        instruction
            .dependencies
            .iter()
            .all(|id| self.selected.contains(&id.to_ascii_lowercase()))
            && !instruction
                .restrictions
                .iter()
                .any(|id| self.selected.contains(&id.to_ascii_lowercase()))
    }

    /// The same immediate-option selection used by execution and read-only gates.
    fn chosen_options<'a>(
        component: &'a Component,
        instruction: &Instruction,
    ) -> Result<Vec<&'a Component>> {
        for id in &instruction.source {
            ensure!(
                component
                    .options
                    .iter()
                    .any(|option| option.guid.eq_ignore_ascii_case(id)),
                "Unknown option {id}"
            );
        }
        Ok(component
            .options
            .iter()
            .filter(|option| {
                option.is_selected
                    && instruction
                        .source
                        .iter()
                        .any(|id| option.guid.eq_ignore_ascii_case(id))
            })
            .collect())
    }

    fn collect_execution_components<'a>(
        &self,
        component: &'a Component,
        scope: &mut Vec<&'a Component>,
    ) -> Result<()> {
        self.check_component(component)?;
        scope.push(component);
        for instruction in &component.instructions {
            if instruction.action == "Choose" && self.instruction_applies(instruction) {
                for option in Self::chosen_options(component, instruction)? {
                    self.collect_execution_components(option, scope)?;
                }
            }
        }
        Ok(())
    }

    fn validation_warning(&mut self, category: &str, message: String, path: &Path) {
        if !self.install {
            self.validation_warnings
                .push(crate::validation::DryRunIssue::warning(
                    category.into(),
                    message,
                    path.to_string_lossy().into_owned(),
                    self.validation_target.clone(),
                ));
        }
    }

    fn path(&self, raw: &str, wildcard: bool) -> Result<PathBuf> {
        let path = self.settings.roots.resolve(raw, wildcard)?;
        if !wildcard {
            let root = [&self.settings.roots.mods, &self.settings.roots.game]
                .into_iter()
                .find(|root| path.starts_with(root))
                .context("Path outside install roots")?;
            self.path_policy
                .check_spelling(root, &path, &self.settings.cancelled)?;
        }
        Ok(path)
    }

    fn matches(&self, sources: &[String], missing_ok: bool) -> Result<Vec<PathBuf>> {
        ensure!(!sources.is_empty(), "Instruction has no Source");
        let mut found = BTreeMap::new();
        for source in sources {
            let (path, pattern) = self.source_pattern(source)?;
            let prefix = format!("{}/", self.path_policy.key(&path));
            let matches: Vec<_> = self
                .files
                .iter()
                .filter(|(k, _)| self.source_matches(&prefix, &pattern, k))
                .collect();
            if !missing_ok && matches.is_empty() {
                return Err(MissingSource(source.clone()).into());
            }
            for (k, (p, _)) in matches {
                self.settings.roots.check(p)?;
                found.insert(k.clone(), p.clone());
            }
        }
        Ok(found.into_values().collect())
    }

    fn source_pattern(&self, source: &str) -> Result<(PathBuf, glob::Pattern)> {
        let path = self.path(source, true)?;
        let root = [&self.settings.roots.mods, &self.settings.roots.game]
            .into_iter()
            .find(|root| path.starts_with(root))
            .unwrap();
        let pattern = glob::Pattern::new(&format!(
            "{}/{}",
            glob::Pattern::escape(&self.path_policy.key(root)),
            self.path_policy.key(path.strip_prefix(root)?)
        ))?;
        Ok((path, pattern))
    }

    fn source_matches(&self, prefix: &str, pattern: &glob::Pattern, candidate: &str) -> bool {
        self.path_policy.glob_matches(pattern, candidate) || candidate.starts_with(prefix)
    }

    fn instruction_sources(&self, sources: &[String]) -> Result<Vec<PathBuf>> {
        // A miss in one pattern must not hide an unsafe/malformed later pattern.
        for source in sources {
            self.source_pattern(source)?;
        }
        let paths = self.matches(sources, false).map_err(|error| {
            if error.is::<MissingSource>() {
                anyhow::Error::new(MissingSourceFiles(error))
            } else {
                error
            }
        })?;
        for path in &paths {
            // Reading must be possible even when virtual copying needs no bytes.
            let content = &self.files[&self.path_policy.key(path)].1;
            if !matches!(content, Content::Unknown) || self.install {
                content.open()?;
            }
        }
        Ok(paths)
    }

    fn extraction_destination(&self, instruction: &Instruction, source: &Path) -> Result<PathBuf> {
        let default = source.parent().unwrap().join(source.file_stem().unwrap());
        if instruction.destination.is_empty() {
            Ok(default)
        } else {
            let path = self.path(&instruction.destination, false)?;
            Ok(
                if self.path_policy.key(&path) == self.path_policy.key(source.parent().unwrap()) {
                    default
                } else {
                    path
                },
            )
        }
    }

    fn actual(&self, path: &Path) -> PathBuf {
        if let Some((p, _)) = self.files.get(&self.path_policy.key(path)) {
            return p.clone();
        }
        if let Some(p) = self.directories.get(&self.path_policy.key(path)) {
            return p.clone();
        }
        if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
            let actual = self.actual(parent);
            return actual.join(name);
        }
        path.to_path_buf()
    }

    fn put(&mut self, path: &Path, content: Content, overwrite: bool) -> Result<()> {
        let started = std::time::Instant::now();
        let actual_install = self.install;
        let mut attempted = false;
        let result: Result<()> = (|| {
            let path = self.actual(path);
            self.settings.roots.check(&path)?;
            let root = [&self.settings.roots.mods, &self.settings.roots.game]
                .into_iter()
                .find(|root| path.starts_with(root))
                .context("Output outside install roots")?;
            self.path_policy
                .check_spelling(root, &path, &self.settings.cancelled)?;
            ensure!(
                !self.directories.contains_key(&self.path_policy.key(&path)),
                "Cannot overwrite directory with file: {}",
                path.display()
            );
            if self.files.contains_key(&self.path_policy.key(&path)) && !overwrite {
                return Ok(());
            }
            let mut parent = path.parent();
            while let Some(dir) = parent {
                ensure!(
                    !self.files.contains_key(&self.path_policy.key(dir)),
                    "File blocks destination directory: {}",
                    dir.display()
                );
                self.directories
                    .insert(self.path_policy.key(dir), dir.to_path_buf());
                parent = dir.parent();
            }
            let content = if self.install {
                attempted = true;
                let modified = content.modified()?;
                let permissions = match &content {
                    Content::Unknown => bail!("Cannot publish unknown simulated patcher content"),
                    Content::Disk(source) => Some(fs::metadata(source)?.permissions()),
                    Content::Scratch(_, mode, _) => {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            mode.map(|mode| fs::Permissions::from_mode(mode & 0o777))
                        }
                        #[cfg(not(unix))]
                        {
                            let _ = mode;
                            None
                        }
                    }
                };
                let physical = self.managed_output(&path)?;
                recovery::atomic_copy(&physical, content.open()?, &self.settings.cancelled)?;
                if let Some(modified) = modified {
                    // Set time before readonly permissions, on the actual managed or
                    // ordinary output. Metadata failure retains the install journal.
                    fs::OpenOptions::new()
                        .write(true)
                        .open(&physical)?
                        .set_modified(modified)?;
                }
                if let Some(permissions) = permissions {
                    fs::set_permissions(&physical, permissions)?;
                }
                if physical != path
                    && let Some(managed) = &mut self.managed
                {
                    managed
                        .pending
                        .insert(self.path_policy.key(&path), physical.clone());
                }
                Content::Disk(physical)
            } else {
                content
            };
            if let Some(tracker) = &mut self.conflicts
                && let Ok(relative) = path.strip_prefix(&self.settings.roots.game)
            {
                tracker.write(&relative.to_string_lossy());
            }
            self.files
                .insert(self.path_policy.key(&path), (path, content));
            Ok(())
        })();
        if actual_install && attempted {
            let error = result.as_ref().err().map(|error| format!("{error:#}"));
            crate::telemetry::global().record_file_operation(
                "write",
                result.is_ok(),
                1,
                started.elapsed(),
                error.as_deref(),
            );
        }
        result
    }

    fn delete(&mut self, path: &Path) -> Result<()> {
        let started = std::time::Instant::now();
        let actual_install = self.install;
        let mut attempted = false;
        let result: Result<()> = (|| {
            self.settings.roots.check(path)?;
            if self.install {
                attempted = self.files.contains_key(&self.path_policy.key(path));
                self.managed_delete(path)?;
            }
            self.files.remove(&self.path_policy.key(path));
            Ok(())
        })();
        if actual_install && attempted {
            let error = result.as_ref().err().map(|error| format!("{error:#}"));
            crate::telemetry::global().record_file_operation(
                "delete",
                result.is_ok(),
                1,
                started.elapsed(),
                error.as_deref(),
            );
        }
        result
    }

    fn execute(
        &mut self,
        component: &Component,
        i: &Instruction,
        emit: &mut impl FnMut(Event),
    ) -> Result<()> {
        self.execute_observed(component, i, emit, None)
    }

    fn execute_observed(
        &mut self,
        component: &Component,
        i: &Instruction,
        emit: &mut impl FnMut(Event),
        mut progress: Option<&mut InstallObserver<'_>>,
    ) -> Result<()> {
        match i.action.as_str() {
            "Choose" => {
                let reached_from_instruction = progress
                    .as_ref()
                    .and_then(|observer| observer.current.instruction_index);
                for option in Self::chosen_options(component, i)? {
                    if let Some(observer) = progress.as_deref_mut() {
                        observer.current.owner_path.push(OwnerFrame {
                            name: option.name.clone(),
                            guid: option.guid.clone(),
                            reached_from_instruction,
                        });
                    }
                    let result = self.component(option, emit, progress.as_deref_mut());
                    if let Some(observer) = progress.as_deref_mut() {
                        observer.current.owner_path.pop();
                    }
                    result?;
                }
            }
            "Extract" => {
                if !i.destination.is_empty() {
                    self.path(&i.destination, false)?;
                }
                for source in self.instruction_sources(&i.source)? {
                    let content = self.files[&self.path_policy.key(&source)].1.clone();
                    let dest = self.extraction_destination(i, &source)?;
                    let entries = archive::unpack(
                        content.path()?,
                        &source,
                        &self.settings.archive,
                        &self.settings.cancelled,
                    )?;
                    let configured = if let Some(mut module) =
                        crate::fomod::inspect_entries(&entries)?
                    {
                        let filename = source
                            .file_name()
                            .and_then(|n| n.to_str())
                            .context("FOMOD archive filename is invalid")?;
                        let choices = crate::fomod::required_saved_choices(component, filename)?;
                        let relative = dest.strip_prefix(&self.settings.roots.mods).context(
                            "Configured FOMOD extraction must stay in the mod directory",
                        )?;
                        // Applying and mapping both judge the saved choices against
                        // the current archive, so both carry the recovery route.
                        choices
                            .apply_to(&mut module.config.steps)
                            .and_then(|()| module.copy_instructions(&relative.to_string_lossy()))
                            .with_context(|| crate::fomod::stale_choices_context(filename))?
                    } else {
                        Vec::new()
                    };
                    let archive_paths: BTreeSet<_> = entries
                        .iter()
                        .map(|entry| self.path_policy.key(&dest.join(&entry.path)))
                        .collect();
                    if !configured.is_empty() && !i.overwrite {
                        ensure!(
                            archive_paths
                                .iter()
                                .all(|path| !self.files.contains_key(path)),
                            "Configured FOMOD extraction cannot retain existing files with Overwrite=false"
                        );
                    }
                    for entry in entries {
                        self.check_cancelled()?;
                        self.put(
                            &dest.join(entry.path),
                            Content::Scratch(entry.file, entry.mode, entry.modified),
                            i.overwrite,
                        )?;
                    }
                    for copy in configured {
                        for source in self.matches(&copy.source, false)? {
                            ensure!(
                                archive_paths.contains(&self.path_policy.key(&source)),
                                "FOMOD source is not supplied by the current archive: {}",
                                source.display()
                            );
                        }
                        self.execute(component, &copy, emit)?;
                    }
                }
            }
            "Copy" | "Move" | "Rename" => {
                let dest = if i.action == "Rename" && !i.destination.starts_with("<<") {
                    let path = relative(&i.destination, false)?;
                    ensure!(
                        path.components().count() == 1,
                        "Rename needs one filename or a placeholder path"
                    );
                    path
                } else {
                    self.path(&i.destination, false)?
                };
                let sources = if i.action == "Rename" {
                    self.matches(&i.source, false)?
                } else {
                    self.instruction_sources(&i.source)?
                };
                for source in sources {
                    self.check_cancelled()?;
                    let target = if i.action == "Rename" {
                        if dest.is_absolute() {
                            dest.clone()
                        } else {
                            source.parent().unwrap().join(&dest)
                        }
                    } else {
                        dest.join(source.file_name().unwrap())
                    };
                    ensure!(
                        self.path_policy.key(&source) != self.path_policy.key(&target),
                        "Source equals destination: {}",
                        source.display()
                    );
                    let exists = self.files.contains_key(&self.path_policy.key(&target));
                    if exists && !i.overwrite {
                        self.validation_warning(
                            &format!("{}File", i.action),
                            format!(
                                "Destination file already exists and overwrite is false: {}",
                                target.display()
                            ),
                            &target,
                        );
                        continue;
                    }
                    let data = self.files[&self.path_policy.key(&source)].1.clone();
                    self.put(&target, data, i.overwrite)?;
                    if i.action != "Copy" {
                        self.delete(&source)?;
                    }
                }
            }
            "Delete" => {
                let sources = self.matches(&i.source, true)?;
                if sources.is_empty() {
                    for source in &i.source {
                        let path = self.path(source, true)?;
                        self.validation_warning(
                            "DeleteFile",
                            format!("Attempting to delete non-existent file: {}", path.display()),
                            &path,
                        );
                    }
                }
                for source in sources {
                    self.delete(&source)?;
                }
            }
            "DelDuplicate" => {
                let dir = self.path(&i.destination, false)?;
                let extensions = if i.source.is_empty() {
                    vec![".dds".to_string(), ".tpc".to_string(), ".tga".to_string()]
                } else {
                    i.source.iter().map(|s| s.to_lowercase()).collect()
                };
                let mut groups: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
                for (p, _) in self.files.values().filter(|(p, _)| {
                    p.parent()
                        .is_some_and(|d| self.path_policy.key(d) == self.path_policy.key(&dir))
                }) {
                    let ext = format!(".{}", p.extension().unwrap_or_default().to_string_lossy())
                        .to_lowercase();
                    if extensions.contains(&ext) {
                        groups
                            .entry(self.path_policy.key(Path::new(p.file_stem().unwrap())))
                            .or_default()
                            .push(p.clone());
                    }
                }
                ensure!(
                    !i.arguments.is_empty(),
                    "DelDuplicate Arguments must name the extension to remove"
                );
                for group in groups.values().filter(|g| g.len() > 1) {
                    for p in group {
                        if format!(".{}", p.extension().unwrap_or_default().to_string_lossy())
                            .eq_ignore_ascii_case(&i.arguments)
                        {
                            self.delete(p)?;
                        }
                    }
                }
            }
            "CleanList" => {
                let dir = self.path(&i.destination, false)?;
                let sources = self.matches(&i.source, false)?;
                let source = sources.first().context("CleanList needs a source file")?;
                let data = self.files[&self.path_policy.key(source)].1.open()?;
                for line in BufReader::new(data).lines() {
                    for line in line?.split('\r') {
                        self.check_cancelled()?;
                        let Some((name, files)) = line.split_once(',') else {
                            continue;
                        };
                        if cleanlist_selected(name.trim(), &self.selected_names) {
                            for raw in files.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                                let p = self.actual(&dir.join(relative(raw, false)?));
                                if self.files.contains_key(&self.path_policy.key(&p)) {
                                    self.delete(&p)?;
                                }
                            }
                        }
                    }
                }
            }
            "Execute" | "Run" | "Patcher" => self.process(i, emit)?,
            other => bail!("Unsupported action {other}; refusing to skip it"),
        }
        Ok(())
    }

    fn process(&mut self, i: &Instruction, emit: &mut impl FnMut(Event)) -> Result<()> {
        if i.action == "Patcher" {
            ensure!(
                i.source.len() == 1,
                "Patcher needs exactly one source to bind its configuration and logs"
            );
            let path = self.actual(&self.path(&i.source[0], false)?);
            return self.process_path(i, &path, &[], emit);
        }
        ensure!(
            self.settings.allow_processes,
            "{} needs explicit trust: enable external programs (--allow-processes)",
            i.action
        );
        let arguments = process_arguments::expand(&i.arguments, &self.settings.roots)?;
        ensure!(
            !i.source.is_empty(),
            "External program instruction has no Source"
        );
        // Resolve every authored source before any child starts. Preserve source
        // order, stable wildcard order, and first occurrence for overlapping globs.
        let mut paths = Vec::new();
        let mut seen = BTreeSet::new();
        for source in &i.source {
            for path in self.instruction_sources(std::slice::from_ref(source))? {
                if seen.insert(self.path_policy.key(&path)) {
                    paths.push(path);
                }
            }
        }
        for path in paths {
            self.check_cancelled()?;
            self.process_path(i, &path, &arguments, emit)?;
        }
        Ok(())
    }

    fn process_path(
        &mut self,
        i: &Instruction,
        path: &Path,
        arguments: &[std::ffi::OsString],
        emit: &mut impl FnMut(Event),
    ) -> Result<()> {
        if self.install || i.action != "Patcher" {
            ensure!(
                self.settings.allow_processes,
                "{} needs explicit trust: enable external programs (--allow-processes)",
                i.action
            );
        }
        self.settings.roots.check(path)?;
        ensure!(
            self.files.contains_key(&self.path_policy.key(path))
                || self.directories.contains_key(&self.path_policy.key(path)),
            "Missing external program/mod: {}",
            path.display()
        );
        let preimage = if self.install && self.managed.is_some() {
            self.flush_managed()?;
            Some(self.managed_preimage()?)
        } else {
            None
        };
        let mut staged_patcher = None;
        let mut patcher_data = None;
        let mut command = if i.action == "Patcher" {
            let dest = if i.destination.is_empty() {
                self.settings.roots.game.clone()
            } else {
                self.actual(&self.path(&i.destination, false)?)
            };
            let base = if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            {
                path.parent().unwrap()
            } else {
                path
            };
            if !self.install {
                self.simulate_patcher(base, &dest, &i.arguments, emit)?;
                return Ok(());
            }
            let patcher = self
                .patcher_invocation
                .as_ref()
                .context("Configure an executable with --patcher or install complete packaged patcher resources")?;
            let (stage, staged_base) = patcher_prepare::stage(base, &self.settings.cancelled)?;
            patcher_data = Some(staged_base.clone());
            staged_patcher = Some(stage);
            let mut cmd = self.patcher_command(patcher, &dest, &staged_base)?;
            if !i.arguments.trim().is_empty() {
                if let Ok(index) = i.arguments.trim().parse::<usize>() {
                    cmd.arg("--namespace-option-index").arg(index.to_string());
                } else {
                    let name = relative(i.arguments.trim(), false)?;
                    ensure!(
                        name.components().count() == 1
                            && name
                                .extension()
                                .is_some_and(|e| e.eq_ignore_ascii_case("ini")),
                        "Patcher Arguments must be an INI filename or namespace index"
                    );
                    let data = if base
                        .file_name()
                        .is_some_and(|s| s.eq_ignore_ascii_case("tslpatchdata"))
                    {
                        base.to_path_buf()
                    } else {
                        base.join("tslpatchdata")
                    };
                    let ini = self.actual(&data.join(name));
                    ensure!(
                        self.files.contains_key(&self.path_policy.key(&ini)),
                        "Missing patcher INI: {}",
                        ini.display()
                    );
                    let actual_data = self.actual(&data);
                    let target = staged_base.join(actual_data.strip_prefix(base)?);
                    recovery::atomic_copy(
                        &target.join("changes.ini"),
                        crate::paths::open_regular_read(
                            &staged_base.join(ini.strip_prefix(base)?),
                        )?,
                        &self.settings.cancelled,
                    )?;
                    let mut namespace_roots = vec![staged_base.clone()];
                    if target != staged_base {
                        namespace_roots.push(target);
                    }
                    for directory in namespace_roots {
                        for entry in fs::read_dir(directory)? {
                            let entry = entry?;
                            if entry.file_name().eq_ignore_ascii_case("namespaces.ini") {
                                fs::remove_file(entry.path())?;
                            }
                        }
                    }
                }
            }
            cmd
        } else {
            let mut cmd = Command::new(path);
            cmd.args(arguments);
            cmd
        };
        if self.install {
            if let Some(data) = &patcher_data {
                patcher_prepare::normalize(data, &self.settings.cancelled)?;
                clear_patcher_logs(data)?;
            }
            command.current_dir(
                patcher_data
                    .as_deref()
                    .unwrap_or_else(|| path.parent().unwrap()),
            );
            let receipt = self.run_external_command(command)?;
            // Read and publish diagnostics before checking process success or dropping
            // named-INI staging. The event owns its text after the directory is gone.
            let patcher_result = patcher_data.as_ref().map(|data| {
                let result = patcher_install_log(data);
                match &result {
                    Ok((log, content)) => emit(Event {
                        component: i.action.clone(),
                        step: 0,
                        message: format!("Patcher install log {}:\n{content}", log.display()),
                    }),
                    Err(error) => emit(Event {
                        component: i.action.clone(),
                        step: 0,
                        message: format!("Patcher install log verification failed: {error:#}"),
                    }),
                }
                result
            });
            for output in [receipt.stdout, receipt.stderr] {
                if !output.is_empty() {
                    emit(Event {
                        component: i.action.clone(),
                        step: 0,
                        message: output,
                    });
                }
            }
            ensure!(
                !receipt.cancelled,
                "External program cancelled; restore before retrying"
            );
            if let Some(failure) = receipt.failure {
                bail!("External program failed: {failure}; restore before retrying");
            }
            ensure!(
                receipt.exit_code == Some(0),
                "External program failed with exit code {:?}",
                receipt.exit_code
            );
            if let Some(result) = patcher_result {
                let (_, content) = result?;
                let errors = patcher_log_error_lines(&content);
                ensure!(
                    errors.is_empty(),
                    "Patcher install log reported errors: {}",
                    errors.join(" | ")
                );
            }
            if let Some(preimage) = &preimage {
                self.record_managed_process(preimage)?;
            }
            self.scan()?;
        }
        drop(staged_patcher);
        Ok(())
    }

    fn patcher_command(
        &self,
        patcher: &crate::patcher::Invocation,
        destination: &Path,
        data: &Path,
    ) -> Result<Command> {
        let mut command = patcher.command()?;
        if self.settings.patcher_engine == PatcherEngine::KPatcher && !cfg!(windows) {
            command.arg("--console");
        }
        command
            .arg("--install")
            .arg("--game-dir")
            .arg(destination)
            .arg("--tslpatchdata")
            .arg(data);
        if self.settings.patcher_engine == PatcherEngine::OdyPatcher {
            command.arg("--cli").arg("-y");
        }
        Ok(command)
    }
}

// Only these paths at the resolved invocation root can prove this run's log
// postcondition. RTF precedes TXT, matching the reference verifier.
const PATCHER_LOG_NAMES: [&str; 2] = ["installlog.rtf", "installlog.txt"];

fn clear_patcher_logs(data: &Path) -> Result<()> {
    for name in PATCHER_LOG_NAMES {
        let log = data.join(name);
        match fs::remove_file(&log) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "Cannot remove previous patcher install log {}",
                        log.display()
                    )
                });
            }
        }
    }
    Ok(())
}

fn patcher_install_log(data: &Path) -> Result<(PathBuf, String)> {
    for name in PATCHER_LOG_NAMES {
        let log = data.join(name);
        let metadata = match fs::symlink_metadata(&log) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("Cannot inspect patcher install log {}", log.display())
                });
            }
        };
        ensure!(
            metadata.is_file(),
            "Patcher install log must be a regular file: {}",
            log.display()
        );
        // Failure diagnostics remain available after cancellation. This bounded
        // read deliberately does not consult the action's cancellation flag.
        use std::io::Read;
        const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;
        let file = crate::paths::open_regular_read(&log)
            .with_context(|| format!("Cannot open patcher install log {}", log.display()))?;
        ensure!(
            file.metadata()?.len() <= MAX_LOG_BYTES,
            "Patcher install log exceeds the 8 MiB limit: {}",
            log.display()
        );
        let mut bytes = Vec::new();
        file.take(MAX_LOG_BYTES + 1)
            .read_to_end(&mut bytes)
            .with_context(|| format!("Cannot read patcher install log {}", log.display()))?;
        ensure!(
            bytes.len() as u64 <= MAX_LOG_BYTES,
            "Patcher install log exceeds the 8 MiB limit: {}",
            log.display()
        );
        let content = String::from_utf8(bytes)
            .with_context(|| format!("Patcher install log is not UTF-8: {}", log.display()))?;
        return Ok((log, content));
    }
    bail!(
        "Patcher install log not found at invocation root {} (expected installlog.rtf or installlog.txt)",
        data.display()
    )
}

fn patcher_log_error_lines(content: &str) -> Vec<String> {
    content
        .split(['\r', '\n'])
        .filter(|line| {
            !line.trim().is_empty() && (line.contains("Error:") || line.contains("[Error]"))
        })
        .map(str::to_owned)
        .collect()
}

// Match ModComponent.ExecuteSingleInstructionAsync's CleanList selection rules.
fn cleanlist_selected(name: &str, selected: &BTreeSet<String>) -> bool {
    let name = name.to_lowercase();
    if name.trim().is_empty() {
        return false;
    }
    if name.starts_with("mandatory deletions") {
        return true;
    }
    fn tokens(name: &str) -> BTreeSet<String> {
        const STOP: &[&str] = &[
            "by",
            "for",
            "the",
            "and",
            "of",
            "mod",
            "pack",
            "k1",
            "k2",
            "hd",
            "kotor",
            "kotorn",
            "version",
            "recolored",
            "reskin",
            "retexture",
            "fix",
            "fixes",
        ];
        name.split(|c: char| !c.is_ascii_lowercase() && !c.is_ascii_digit())
            .filter(|word| !word.is_empty() && !STOP.contains(word))
            .map(|word| {
                if word.len() > 3 {
                    word.strip_suffix('s').unwrap_or(word)
                } else {
                    word
                }
            })
            .map(str::to_owned)
            .collect()
    }
    let target = tokens(&name);
    if target.is_empty() {
        return false;
    }
    selected.iter().any(|candidate| {
        if name.contains(candidate) || candidate.contains(&name) {
            return true;
        }
        let other = tokens(candidate);
        let shared = target.intersection(&other).count();
        shared >= 2 && shared * 5 >= target.len().min(other.len()) * 3
    })
}
