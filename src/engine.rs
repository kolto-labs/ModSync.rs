use crate::{
    archive,
    model::{Component, Guide, Instruction},
    paths::{Roots, relative},
    recovery,
};
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufRead, BufReader, Read},
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

#[derive(Clone)]
enum Content {
    Disk(PathBuf),
    Scratch(Arc<tempfile::NamedTempFile>, Option<u32>),
}
impl Content {
    fn path(&self) -> &Path {
        match self {
            Self::Disk(path) => path,
            Self::Scratch(file, _) => file.path(),
        }
    }
    fn open(&self) -> Result<fs::File> {
        Ok(fs::File::open(self.path())?)
    }
}

fn key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
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
    files: BTreeMap<String, (PathBuf, Content)>,
    directories: BTreeMap<String, PathBuf>,
    install: bool,
    selected: BTreeSet<String>,
    completed: BTreeSet<String>,
    selected_names: BTreeSet<String>,
    conflicts: Option<crate::conflicts::Tracker>,
    validation_target: Option<(String, String, Option<usize>)>,
    validation_warnings: Vec<crate::validation::DryRunIssue>,
}

impl Engine {
    pub fn new(mut settings: Settings) -> Result<Self> {
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
        settings.patcher =
            crate::patcher::resolve(settings.patcher_engine, settings.patcher.as_deref())?;
        let mut engine = Self {
            settings,
            files: BTreeMap::new(),
            directories: BTreeMap::new(),
            install: false,
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
            for entry in walkdir::WalkDir::new(root) {
                let entry = entry?;
                ensure!(
                    !entry.file_type().is_symlink(),
                    "Symlinks are unsupported in install trees: {}",
                    entry.path().display()
                );
                let path = entry.path().to_path_buf();
                if entry.file_type().is_file() {
                    ensure!(
                        self.files
                            .insert(key(&path), (path.clone(), Content::Disk(path.clone())))
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
                        self.directories.insert(key(&path), path.clone()).is_none(),
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
        self.run(guide, &mut emit)
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
            let mut engine = Engine::new(settings.clone())?;
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
                return report.finish();
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
            .and_then(|engine| engine.validate_fomod_configuration(guide))
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
        report.finish()
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
                    !self.files.contains_key(&key(&dest)),
                    "File blocks extraction destination: {}",
                    dest.display()
                );
                let entries = archive::unpack(
                    self.files[&key(&source)].1.path(),
                    &source,
                    &self.settings.archive,
                    &self.settings.cancelled,
                )
                .with_context(|| format!("Cannot inspect archive {}", source.display()))?;
                for entry in entries {
                    let path = dest.join(entry.path);
                    self.settings.roots.check(&path)?;
                    members.insert(key(&path));
                }
            }
        }
        for instruction in instructions {
            self.check_cancelled()?;
            match instruction.action.as_str() {
                "Copy" | "Move" | "CleanList" | "DelDuplicate" => {
                    let dest = self.path(&instruction.destination, false)?;
                    ensure!(
                        self.directories.contains_key(&key(&dest)),
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
                let prefix = format!("{}/", key(&path));
                ensure!(
                    members
                        .iter()
                        .any(|member| Self::source_matches(&prefix, &pattern, member)),
                    "Failed to find '{source}' in any archives"
                );
            }
        }
        Ok(())
    }

    fn validate_fomod_configuration(&self, guide: &Guide) -> Result<()> {
        fn is_archive(name: &str) -> bool {
            matches!(
                std::path::Path::new(name)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| extension.to_ascii_lowercase())
                    .as_deref(),
                Some("zip" | "7z" | "rar" | "exe" | "tar" | "gz")
            )
        }
        fn configured_choices(component: &Component, filename: &str) -> Result<()> {
            let raw = component.metadata.get("FomodChoices").with_context(|| {
                format!("FOMOD archive '{filename}' has no explicit FomodChoices configuration")
            })?;
            let choices = crate::fomod::ChoicesFile::parse(&serde_json::to_string(raw)?)?;
            choices
                .archive(filename)
                .with_context(|| format!("No FOMOD choices for {filename}"))?;
            Ok(())
        }

        // C# gates registered download artifacts even when no Extract instruction names them.
        // A previously recorded prompt state makes a missing artifact fail closed.
        let selected = guide.selected_ids();
        for component in guide.ordered()? {
            if self
                .completed
                .contains(&component.guid.to_ascii_lowercase())
            {
                continue;
            }
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
                    if !is_archive(name) || !registered.insert(name.to_ascii_lowercase()) {
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
                        continue;
                    }
                    let entries = archive::unpack(
                        &path,
                        &relative_name,
                        &self.settings.archive,
                        &self.settings.cancelled,
                    )
                    .with_context(|| {
                        format!("Registered archive '{filename}' cannot be inspected")
                    })?;
                    if crate::fomod::inspect_entries(&entries)?.is_some() {
                        configured_choices(component, filename)?;
                    }
                }
            }
        }
        for component in guide.ordered()? {
            if self
                .completed
                .contains(&component.guid.to_ascii_lowercase())
            {
                continue;
            }
            for instruction in &component.instructions {
                if instruction.action != "Extract"
                    || !instruction
                        .dependencies
                        .iter()
                        .all(|id| selected.contains(&id.to_ascii_lowercase()))
                    || instruction
                        .restrictions
                        .iter()
                        .any(|id| selected.contains(&id.to_ascii_lowercase()))
                {
                    continue;
                }
                // Missing generic Extract inputs belong to archive validation and
                // dry-run availability, not the independent FOMOD state gate.
                for source in self.matches(&instruction.source, true)? {
                    let content = &self.files[&key(&source)].1;
                    let entries = archive::unpack(
                        content.path(),
                        &source,
                        &self.settings.archive,
                        &self.settings.cancelled,
                    )?;
                    if crate::fomod::inspect_entries(&entries)?.is_none() {
                        continue;
                    }
                    let filename = source
                        .file_name()
                        .and_then(|name| name.to_str())
                        .context("FOMOD archive filename is invalid")?;
                    configured_choices(component, filename)?;
                }
            }
        }
        Ok(())
    }

    /// Analyze actual simulated writes without changing either installation tree.
    /// Failed instructions return an error rather than an incomplete success report.
    pub fn analyze_conflicts(mut self, guide: &Guide) -> Result<crate::conflicts::Report> {
        recovery::ensure_clean(&self.settings.roots)?;
        self.conflicts = Some(crate::conflicts::Tracker::default());
        self.run(guide, &mut |_| {})?;
        Ok(self.conflicts.take().unwrap().finish())
    }

    pub fn install(mut self, guide: &Guide, mut emit: impl FnMut(Event)) -> Result<()> {
        let _lock = recovery::Lock::acquire(&self.settings.roots)?;
        recovery::ensure_clean(&self.settings.roots)?;
        self.scan()?;
        recovery::verify_completion(&self.settings.roots, guide, self.completed.len())?;
        self.run(guide, &mut |_| {})?;
        self.scan()?;
        emit(Event {
            component: String::new(),
            step: 0,
            message: "Creating recovery snapshot of game and mod directories".into(),
        });
        self.check_cancelled()?;
        recovery::begin(&self.settings.roots)?;
        self.install = true;
        self.run(guide, &mut emit).with_context(|| {
            format!(
                "Install stopped. Restore before retrying; snapshot: {}",
                recovery::state_dir(&self.settings.roots).display()
            )
        })?;
        recovery::record_completion(&self.settings.roots, guide)?;
        recovery::finish(&self.settings.roots)?;
        Ok(())
    }

    /// Execute the remaining suffix while retaining selected dependency context.
    /// A saved receipt must match roots, prefix definitions and current game bytes.
    /// This is not exposed by CLI/GUI yet.
    pub fn install_remaining(
        mut self,
        guide: &Guide,
        completed: &[String],
        emit: impl FnMut(Event),
    ) -> Result<()> {
        self.set_completed_prefix(guide, completed)?;
        self.install(guide, emit)
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

    fn run(&mut self, guide: &Guide, emit: &mut impl FnMut(Event)) -> Result<()> {
        self.selected = guide.selected_ids();
        self.selected_names = guide
            .components
            .iter()
            .filter(|c| c.is_selected)
            .map(|c| c.name.to_lowercase())
            .collect();
        for c in guide.ordered()? {
            if self.completed.contains(&c.guid.to_ascii_lowercase()) {
                continue;
            }
            ensure!(
                !c.instructions.is_empty(),
                "{} has no executable instructions; review the guide",
                c.name
            );
            if let Some(tracker) = &mut self.conflicts {
                tracker.begin(c);
            }
            self.component(c, emit)?;
        }
        Ok(())
    }

    fn component(&mut self, c: &Component, emit: &mut impl FnMut(Event)) -> Result<()> {
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
                continue;
            }
            let message = if matches!(instruction.action.as_str(), "Patcher" | "Execute" | "Run")
                && !self.install
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
            self.execute(c, instruction, emit)
                .with_context(|| format!("{} / step {} ({})", c.name, n + 1, instruction.action))?;
            emit(Event {
                component: c.name.clone(),
                step: n + 1,
                message,
            });
        }
        Ok(())
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
        self.settings.roots.resolve(raw, wildcard)
    }

    fn matches(&self, sources: &[String], missing_ok: bool) -> Result<Vec<PathBuf>> {
        ensure!(!sources.is_empty(), "Instruction has no Source");
        let mut found = BTreeMap::new();
        for source in sources {
            let (path, pattern) = self.source_pattern(source)?;
            let prefix = format!("{}/", key(&path));
            let matches: Vec<_> = self
                .files
                .iter()
                .filter(|(k, _)| Self::source_matches(&prefix, &pattern, k))
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
            glob::Pattern::escape(&key(root)),
            key(path.strip_prefix(root)?)
        ))?;
        Ok((path, pattern))
    }

    fn source_matches(prefix: &str, pattern: &glob::Pattern, candidate: &str) -> bool {
        pattern.matches_with(
            candidate,
            glob::MatchOptions {
                case_sensitive: false,
                require_literal_separator: true,
                require_literal_leading_dot: false,
            },
        ) || candidate.starts_with(prefix)
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
            self.files[&key(path)].1.open()?;
        }
        Ok(paths)
    }

    fn extraction_destination(&self, instruction: &Instruction, source: &Path) -> Result<PathBuf> {
        let default = source.parent().unwrap().join(source.file_stem().unwrap());
        if instruction.destination.is_empty() {
            Ok(default)
        } else {
            let path = self.path(&instruction.destination, false)?;
            Ok(if key(&path) == key(source.parent().unwrap()) {
                default
            } else {
                path
            })
        }
    }

    fn actual(&self, path: &Path) -> PathBuf {
        if let Some((p, _)) = self.files.get(&key(path)) {
            return p.clone();
        }
        if let Some(p) = self.directories.get(&key(path)) {
            return p.clone();
        }
        if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
            let actual = self.actual(parent);
            return actual.join(name);
        }
        path.to_path_buf()
    }

    fn put(&mut self, path: &Path, content: Content, overwrite: bool) -> Result<()> {
        let path = self.actual(path);
        self.settings.roots.check(&path)?;
        ensure!(
            !self.directories.contains_key(&key(&path)),
            "Cannot overwrite directory with file: {}",
            path.display()
        );
        if self.files.contains_key(&key(&path)) && !overwrite {
            return Ok(());
        }
        let mut parent = path.parent();
        while let Some(dir) = parent {
            ensure!(
                !self.files.contains_key(&key(dir)),
                "File blocks destination directory: {}",
                dir.display()
            );
            self.directories.insert(key(dir), dir.to_path_buf());
            parent = dir.parent();
        }
        let content = if self.install {
            let permissions = match &content {
                Content::Disk(source) => Some(fs::metadata(source)?.permissions()),
                Content::Scratch(_, mode) => {
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
            recovery::atomic_copy(&path, content.open()?, &self.settings.cancelled)?;
            if let Some(permissions) = permissions {
                fs::set_permissions(&path, permissions)?;
            }
            Content::Disk(path.clone())
        } else {
            content
        };
        if let Some(tracker) = &mut self.conflicts
            && let Ok(relative) = path.strip_prefix(&self.settings.roots.game)
        {
            tracker.write(&relative.to_string_lossy());
        }
        self.files.insert(key(&path), (path, content));
        Ok(())
    }

    fn delete(&mut self, path: &Path) -> Result<()> {
        self.settings.roots.check(path)?;
        if self.install {
            fs::remove_file(path)?;
        }
        self.files.remove(&key(path));
        Ok(())
    }

    fn execute(
        &mut self,
        component: &Component,
        i: &Instruction,
        emit: &mut impl FnMut(Event),
    ) -> Result<()> {
        match i.action.as_str() {
            "Choose" => {
                for id in &i.source {
                    ensure!(
                        component
                            .options
                            .iter()
                            .any(|o| o.guid.eq_ignore_ascii_case(id)),
                        "Unknown option {id}"
                    );
                }
                for option in &component.options {
                    if option.is_selected
                        && i.source
                            .iter()
                            .any(|id| option.guid.eq_ignore_ascii_case(id))
                    {
                        self.component(option, emit)?;
                    }
                }
            }
            "Extract" => {
                if !i.destination.is_empty() {
                    self.path(&i.destination, false)?;
                }
                for source in self.instruction_sources(&i.source)? {
                    let content = self.files[&key(&source)].1.clone();
                    let dest = self.extraction_destination(i, &source)?;
                    let entries = archive::unpack(
                        content.path(),
                        &source,
                        &self.settings.archive,
                        &self.settings.cancelled,
                    )?;
                    let configured = if let Some(mut module) =
                        crate::fomod::inspect_entries(&entries)?
                    {
                        let raw = component.metadata.get("FomodChoices")
                            .context("FOMOD choices are unsupported without explicit FomodChoices configuration")?;
                        let choices =
                            crate::fomod::ChoicesFile::parse(&serde_json::to_string(raw)?)?;
                        let filename = source
                            .file_name()
                            .and_then(|n| n.to_str())
                            .context("FOMOD archive filename is invalid")?;
                        choices
                            .archive(filename)
                            .with_context(|| format!("No FOMOD choices for {filename}"))?
                            .apply_to(&mut module.config.steps)?;
                        let relative = dest.strip_prefix(&self.settings.roots.mods).context(
                            "Configured FOMOD extraction must stay in the mod directory",
                        )?;
                        module.copy_instructions(&relative.to_string_lossy())?
                    } else {
                        Vec::new()
                    };
                    let archive_paths: BTreeSet<_> = entries
                        .iter()
                        .map(|entry| key(&dest.join(&entry.path)))
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
                            Content::Scratch(entry.file, entry.mode),
                            i.overwrite,
                        )?;
                    }
                    for copy in configured {
                        for source in self.matches(&copy.source, false)? {
                            ensure!(
                                archive_paths.contains(&key(&source)),
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
                        key(&source) != key(&target),
                        "Source equals destination: {}",
                        source.display()
                    );
                    let exists = self.files.contains_key(&key(&target));
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
                    let data = self.files[&key(&source)].1.clone();
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
                    vec![".tga".to_string(), ".tpc".to_string()]
                } else {
                    i.source.iter().map(|s| s.to_lowercase()).collect()
                };
                let mut groups: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
                for (p, _) in self
                    .files
                    .values()
                    .filter(|(p, _)| p.parent().is_some_and(|d| key(d) == key(&dir)))
                {
                    let ext = format!(".{}", p.extension().unwrap_or_default().to_string_lossy())
                        .to_lowercase();
                    if extensions.contains(&ext) {
                        groups
                            .entry(p.file_stem().unwrap().to_string_lossy().to_lowercase())
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
                let data = self.files[&key(source)].1.open()?;
                for line in BufReader::new(data).lines() {
                    for line in line?.split('\r') {
                        self.check_cancelled()?;
                        let Some((name, files)) = line.split_once(',') else {
                            continue;
                        };
                        if cleanlist_selected(name.trim(), &self.selected_names) {
                            for raw in files.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                                let p = self.actual(&dir.join(relative(raw, false)?));
                                if self.files.contains_key(&key(&p)) {
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
        ensure!(
            self.settings.allow_processes,
            "{} needs explicit trust: enable external programs (--allow-processes)",
            i.action
        );
        ensure!(
            i.source.len() == 1,
            "External programs need exactly one source"
        );
        let path = self.actual(&self.path(&i.source[0], false)?);
        ensure!(
            self.files.contains_key(&key(&path)) || self.directories.contains_key(&key(&path)),
            "Missing external program/mod: {}",
            path.display()
        );
        let mut staged_patcher = None;
        let mut patcher_data = None;
        let mut command = if i.action == "Patcher" {
            let patcher = self
                .settings
                .patcher
                .as_ref()
                .context("Configure a patcher executable with --patcher")?;
            ensure!(patcher.is_file(), "Patcher executable is missing");
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
                &path
            };
            patcher_data = Some(base.to_path_buf());
            let mut cmd = self.patcher_command(patcher, &dest, base);
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
                        self.files.contains_key(&key(&ini)),
                        "Missing patcher INI: {}",
                        ini.display()
                    );
                    if self.install {
                        let stage = tempfile::tempdir()?;
                        let target = stage.path().join("tslpatchdata");
                        recovery::copy_tree(&self.actual(&data), &target)?;
                        recovery::atomic_copy(
                            &target.join("changes.ini"),
                            self.files[&key(&ini)].1.open()?,
                            &self.settings.cancelled,
                        )?;
                        for entry in fs::read_dir(&target)? {
                            let entry = entry?;
                            if entry.file_name().eq_ignore_ascii_case("namespaces.ini") {
                                fs::remove_file(entry.path())?;
                            }
                        }
                        let destination = if i.destination.is_empty() {
                            self.settings.roots.game.clone()
                        } else {
                            self.actual(&self.path(&i.destination, false)?)
                        };
                        cmd = self.patcher_command(patcher, &destination, stage.path());
                        patcher_data = Some(stage.path().to_path_buf());
                        staged_patcher = Some(stage);
                    }
                }
            }
            cmd
        } else {
            let mut cmd = Command::new(&path);
            cmd.args(shlex::split(&i.arguments).context("Unclosed quote in Arguments")?);
            cmd
        };
        if self.install {
            if let Some(data) = &patcher_data {
                for name in ["installlog.rtf", "installlog.txt"] {
                    let _ = fs::remove_file(data.join(name));
                }
            }
            let stdout = tempfile::NamedTempFile::new()?;
            let stderr = tempfile::NamedTempFile::new()?;
            let mut child = command
                .current_dir(path.parent().unwrap())
                .stdout(stdout.reopen()?)
                .stderr(stderr.reopen()?)
                .spawn()
                .context("Start external program")?;
            let status = loop {
                if self.settings.cancelled.load(Ordering::Relaxed) {
                    let _ = child.kill();
                    let _ = child.wait();
                    bail!("External program cancelled; restore before retrying");
                }
                if let Some(status) = child.try_wait()? {
                    break status;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            };
            for file in [stdout, stderr] {
                let mut output = String::new();
                file.reopen()?
                    .take(1024 * 1024)
                    .read_to_string(&mut output)?;
                if !output.is_empty() {
                    emit(Event {
                        component: i.action.clone(),
                        step: 0,
                        message: output,
                    });
                }
            }
            ensure!(status.success(), "External program failed with {status}");
            if let Some(data) = &patcher_data {
                let errors = patcher_log_errors(data)?;
                ensure!(
                    errors.is_empty(),
                    "Patcher install log reported errors: {}",
                    errors.join(" | ")
                );
            }
            self.scan()?;
        }
        drop(staged_patcher);
        Ok(())
    }

    fn patcher_command(&self, patcher: &Path, destination: &Path, data: &Path) -> Command {
        let mut command = Command::new(patcher);
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
        command
    }
}

fn patcher_log_errors(data: &Path) -> Result<Vec<String>> {
    let log = ["installlog.rtf", "installlog.txt"]
        .into_iter()
        .map(|name| data.join(name))
        .find(|path| path.is_file());
    let Some(log) = log else {
        return Ok(Vec::new());
    };
    Ok(fs::read_to_string(log)?
        .lines()
        .filter(|line| {
            !line.trim().is_empty() && (line.contains("Error: ") || line.contains("[Error]"))
        })
        .map(str::to_owned)
        .collect())
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
