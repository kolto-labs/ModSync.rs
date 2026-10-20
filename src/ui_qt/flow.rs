use anyhow::{Context, Result, ensure};
use modsync::{
    Component, Engine, Guide, Settings, game,
    install_progress::Progress,
    phase::{CompletionContext, Phase},
    recovery, selection,
    validation::{Report, StageProgress},
    wizard::{Gate, Page, Wizard, metadata_text},
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Wizard,
    Selection,
    Validation,
}

impl Surface {
    pub fn parse(value: &str) -> Self {
        match value {
            "selection" => Self::Selection,
            "validation" => Self::Validation,
            _ => Self::Wizard,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Wizard => "wizard",
            Self::Selection => "selection",
            Self::Validation => "validation",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    Validate,
    Install,
    Restore,
    Conflicts,
}

pub struct JobSpec {
    pub kind: JobKind,
    pub guide: Guide,
    pub settings: Settings,
    pub retained: Option<Vec<String>>,
    pub case_insensitive: bool,
}

pub enum Update {
    Log(String),
    Validation(StageProgress),
    Install(Progress),
    Prepared(Vec<Component>),
}

pub enum Terminal {
    Validated(Box<Report>),
    ValidationFailed(String),
    Installed { pending: bool },
    InstallFailed { error: String, pending: bool },
    Restored(std::result::Result<(), String>),
    Conflicts(std::result::Result<Box<modsync::conflicts::Presentation>, String>),
}

#[derive(Default)]
struct ValidationView {
    success: bool,
    errors: usize,
    warnings: usize,
    passed: usize,
    components: usize,
    stages: Vec<Value>,
    issues: Vec<Value>,
    failure: Option<String>,
}

#[derive(Default)]
struct InstallView {
    active: bool,
    progress: Option<Value>,
    error: Option<String>,
    pending: bool,
    restored: bool,
    roots: Option<(PathBuf, PathBuf)>,
}

pub struct Flow {
    guide: Option<Guide>,
    input: String,
    mods: String,
    game: String,
    wizard: Option<Wizard>,
    surface: Surface,
    search: String,
    category: String,
    validation: Option<ValidationView>,
    validation_stamp: Option<String>,
    validation_progress: Option<(usize, usize, String)>,
    conflicts: Option<Value>,
    install: InstallView,
    completed_base: Option<Vec<String>>,
    pending_base: Vec<Component>,
    finished: bool,
    phase_failed: bool,
    confirm_install: bool,
    stop_prompt: bool,
    job: Option<JobKind>,
    logs: Vec<String>,
    cancelled: Arc<AtomicBool>,
    theme: String,
    case_insensitive: bool,
}

fn widescreen(component: &Component) -> bool {
    component
        .metadata
        .get("WidescreenOnly")
        .and_then(Value::as_bool)
        == Some(true)
}

impl Default for Flow {
    fn default() -> Self {
        Self {
            guide: None,
            input: String::new(),
            mods: String::new(),
            game: String::new(),
            wizard: None,
            surface: Surface::Wizard,
            search: String::new(),
            category: String::new(),
            validation: None,
            validation_stamp: None,
            validation_progress: None,
            conflicts: None,
            install: InstallView::default(),
            completed_base: None,
            pending_base: Vec::new(),
            finished: false,
            phase_failed: false,
            confirm_install: false,
            stop_prompt: false,
            job: None,
            logs: Vec::new(),
            cancelled: Arc::new(AtomicBool::new(false)),
            theme: "dark".into(),
            case_insensitive: false,
        }
    }
}

impl Flow {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    pub fn guide(&self) -> Option<&Guide> {
        self.guide.as_ref()
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn mods(&self) -> &str {
        &self.mods
    }

    pub fn game(&self) -> &str {
        &self.game
    }

    pub fn selected_count(&self) -> (usize, usize) {
        self.guide.as_ref().map_or((0, 0), |guide| {
            (
                guide.components.len(),
                guide.components.iter().filter(|c| c.is_selected).count(),
            )
        })
    }

    pub fn set_case_insensitive(&mut self, enabled: bool) {
        self.case_insensitive = enabled;
    }

    pub fn apply_profile(&mut self, profile: &modsync::profiles::Profile) -> Result<()> {
        ensure!(!self.busy(), "Wait for the current operation to finish.");
        let guide = self.guide.as_mut().context("Load a guide first.")?;
        profile.apply(guide);
        if let Some(game) = profile.kotor_directory.as_ref().filter(|p| !p.is_empty()) {
            self.game = game.clone();
        }
        if let Some(mods) = profile.mod_directory.as_ref().filter(|p| !p.is_empty()) {
            self.mods = mods.clone();
        }
        self.invalidate();
        self.apply_target();
        Ok(())
    }

    pub fn set_theme(&mut self, theme: &str) {
        if matches!(theme, "dark" | "light" | "k1" | "k2") {
            self.theme = theme.into();
        }
    }

    pub fn set_surface(&mut self, surface: Surface) {
        self.surface = surface;
    }

    pub fn set_search(&mut self, search: &str) {
        self.search = search.chars().take(256).collect();
    }

    pub fn set_category(&mut self, category: &str) {
        self.category = category.chars().take(256).collect();
    }

    pub fn log(&mut self, line: impl Into<String>) {
        self.logs.push(line.into());
        let excess = self.logs.len().saturating_sub(400);
        if excess > 0 {
            self.logs.drain(..excess);
        }
    }

    fn stamp(&self) -> String {
        let guide = self
            .guide
            .as_ref()
            .and_then(|guide| serde_json::to_string(guide).ok())
            .unwrap_or_default();
        format!("{guide}\u{0}{}\u{0}{}", self.mods, self.game)
    }

    fn detected(&self) -> game::Detected {
        if self.game.trim().is_empty() {
            return game::Detected::Unknown;
        }
        game::detect(&game::resolve_install_root(Path::new(&self.game)))
    }

    fn invalidate(&mut self) {
        self.validation = None;
        self.validation_stamp = None;
        self.validation_progress = None;
        self.conflicts = None;
        self.confirm_install = false;
    }

    fn selected_base(&self) -> usize {
        self.guide.as_ref().map_or(0, |guide| {
            guide
                .components
                .iter()
                .filter(|c| c.is_selected && !widescreen(c))
                .count()
        })
    }

    fn selected_widescreen(&self) -> usize {
        self.guide.as_ref().map_or(0, |guide| {
            guide
                .components
                .iter()
                .filter(|c| c.is_selected && widescreen(c))
                .count()
        })
    }

    fn validation_current(&self) -> bool {
        self.validation.as_ref().is_some_and(|view| view.success)
            && self.validation_stamp.as_deref() == Some(self.stamp().as_str())
    }

    pub fn load_guide(&mut self, path: &str) -> Result<()> {
        ensure!(!self.busy(), "Wait for the current operation to finish.");
        let path = path.trim();
        ensure!(!path.is_empty(), "Choose a guide file first.");
        let guide = Guide::load_for_install(Path::new(path)).context("Load guide")?;
        ensure!(!guide.components.is_empty(), "The guide has no mods.");
        self.reset_installation();
        self.input = path.to_owned();
        self.wizard = Some(Wizard::new(&guide));
        self.log(format!("Loaded {} components", guide.components.len()));
        self.guide = Some(guide);
        self.apply_target();
        self.surface = Surface::Wizard;
        Ok(())
    }

    fn reset_installation(&mut self) {
        self.invalidate();
        self.install = InstallView::default();
        self.completed_base = None;
        self.pending_base.clear();
        self.finished = false;
        self.phase_failed = false;
        self.stop_prompt = false;
        self.cancelled = Arc::new(AtomicBool::new(false));
    }

    fn apply_target(&mut self) {
        let fallback = match self.detected() {
            game::Detected::Kotor1 => Some("KOTOR1"),
            game::Detected::Kotor2Legacy | game::Detected::Kotor2Aspyr => Some("KOTOR2"),
            game::Detected::Unknown => None,
        };
        if let (Some(wizard), Some(guide)) = (self.wizard.as_mut(), self.guide.as_ref()) {
            wizard.apply_target_context(guide, fallback);
        }
    }

    pub fn set_mods(&mut self, path: &str) -> Result<()> {
        ensure!(!self.busy(), "Wait for the current operation to finish.");
        if self.mods != path {
            self.mods = path.trim().to_owned();
            self.invalidate();
        }
        Ok(())
    }

    pub fn set_game(&mut self, path: &str) -> Result<()> {
        ensure!(!self.busy(), "Wait for the current operation to finish.");
        if self.game != path {
            self.game = path.trim().to_owned();
            self.invalidate();
            self.apply_target();
            let detected = self.detected();
            if let Some(guide) = self.guide.as_mut() {
                game::enforce_selectable(&mut guide.components, detected);
            }
        }
        Ok(())
    }

    pub fn set_component(&mut self, index: usize, selected: bool) -> Result<()> {
        ensure!(!self.busy(), "Wait for the current operation to finish.");
        let detected = self.detected();
        let guide = self.guide.as_mut().context("Load a guide first.")?;
        ensure!(index < guide.components.len(), "Unknown mod.");
        selection::set_component_selected(&mut guide.components, index, selected);
        game::enforce_selectable(&mut guide.components, detected);
        self.invalidate();
        Ok(())
    }

    pub fn select_all(&mut self) -> Result<()> {
        ensure!(!self.busy(), "Wait for the current operation to finish.");
        let detected = self.detected();
        let (search, category) = (self.search.clone(), self.category.clone());
        let guide = self.guide.as_mut().context("Load a guide first.")?;
        selection::select_visible(&mut guide.components, &search, "", &category, false);
        selection::resolve_selected(&mut guide.components);
        game::enforce_selectable(&mut guide.components, detected);
        self.invalidate();
        Ok(())
    }

    pub fn select_category(&mut self, category: &str) -> Result<()> {
        ensure!(!self.busy(), "Wait for the current operation to finish.");
        let detected = self.detected();
        let guide = self.guide.as_mut().context("Load a guide first.")?;
        let categories = [category.to_owned()];
        selection::select_categories(&mut guide.components, &categories);
        selection::resolve_selected(&mut guide.components);
        game::enforce_selectable(&mut guide.components, detected);
        self.invalidate();
        Ok(())
    }

    pub fn select_categories(&mut self, categories: &[String]) -> Result<()> {
        ensure!(!self.busy(), "Wait for the current operation to finish.");
        ensure!(!categories.is_empty(), "Choose at least one category.");
        let detected = self.detected();
        let guide = self.guide.as_mut().context("Load a guide first.")?;
        selection::select_categories(&mut guide.components, categories);
        selection::resolve_selected(&mut guide.components);
        game::enforce_selectable(&mut guide.components, detected);
        self.invalidate();
        Ok(())
    }

    pub fn deselect_all(&mut self) -> Result<()> {
        ensure!(!self.busy(), "Wait for the current operation to finish.");
        let guide = self.guide.as_mut().context("Load a guide first.")?;
        selection::deselect_base(&mut guide.components);
        self.invalidate();
        Ok(())
    }

    fn gate(&self) -> Gate {
        let Some(wizard) = &self.wizard else {
            return Gate::default();
        };
        let reading = !self.busy();
        let passed = match wizard.current() {
            Page::ModDirectory => !self.mods.is_empty() && Path::new(&self.mods).is_dir(),
            Page::GameDirectory => self.detected() != game::Detected::Unknown,
            Page::ModSelection => self.selected_base() > 0,
            Page::Validate => self.validation_current(),
            Page::InstallStart => {
                self.validation_current()
                    && self.selected_base() > 0
                    && Path::new(&self.mods).is_dir()
                    && !self.busy()
            }
            Page::Installing => self.completed_base.is_some() && !self.phase_failed,
            Page::WidescreenModSelection => {
                !self.phase_failed
                    && !self.busy()
                    && (self.selected_widescreen() == 0 || self.validation_current())
            }
            Page::WidescreenInstalling => self.finished && !self.phase_failed,
            Page::Finished => false,
            _ => true,
        };
        Gate {
            can_navigate_forward: reading,
            validation_passed: passed,
        }
    }

    pub fn request_next(&mut self) -> Result<Option<JobSpec>> {
        ensure!(self.guide.is_some(), "Load a guide first.");
        let gate = self.gate();
        let wizard = self.wizard.as_ref().context("Load a guide first.")?;
        ensure!(
            wizard.can_next(gate),
            "Complete this page before continuing."
        );
        if wizard.current() == Page::InstallStart && !self.confirm_install {
            self.confirm_install = true;
            return Ok(None);
        }
        self.advance()
    }

    pub fn confirm_install(&mut self) -> Result<Option<JobSpec>> {
        ensure!(
            self.confirm_install,
            "No installation is waiting for confirmation."
        );
        self.confirm_install = false;
        self.advance()
    }

    pub fn cancel_confirmation(&mut self) {
        self.confirm_install = false;
    }

    fn advance(&mut self) -> Result<Option<JobSpec>> {
        let gate = self.gate();
        let wizard = self.wizard.as_mut().context("Load a guide first.")?;
        ensure!(wizard.next(gate), "Complete this page before continuing.");
        match wizard.current() {
            Page::Installing => self.begin(JobKind::Install).map(Some),
            Page::WidescreenModSelection => {
                self.invalidate();
                Ok(None)
            }
            Page::WidescreenInstalling if self.selected_widescreen() == 0 => {
                self.finished = true;
                self.log("Widescreen phase skipped: no widescreen mods selected.");
                Ok(None)
            }
            Page::WidescreenInstalling => self.begin(JobKind::Install).map(Some),
            _ => Ok(None),
        }
    }

    pub fn back(&mut self) {
        if let Some(wizard) = self.wizard.as_mut() {
            wizard.back();
            self.confirm_install = false;
        }
    }

    pub fn restart(&mut self) -> Result<()> {
        ensure!(!self.busy(), "Wait for the current operation to finish.");
        let guide = self.guide.as_ref().context("Load a guide first.")?;
        self.wizard = Some(Wizard::new(guide));
        self.reset_installation();
        self.apply_target();
        self.surface = Surface::Wizard;
        Ok(())
    }

    pub fn open_install_review(&mut self) -> Result<()> {
        ensure!(self.guide.is_some(), "Load a guide first.");
        ensure!(
            !self.busy()
                && self.completed_base.is_none()
                && !self.phase_failed
                && self.selected_base() > 0
                && self.validation_current(),
            "Installation review blocked: run successful validation for the current guide and folders."
        );
        let guide = self.guide.as_ref().context("Load a guide first.")?;
        let mut wizard = self.wizard.take().unwrap_or_else(|| Wizard::new(guide));
        while wizard.current() != Page::Validate && wizard.current() != Page::InstallStart {
            let moved = wizard.next(Gate {
                can_navigate_forward: true,
                validation_passed: true,
            });
            if !moved {
                self.wizard = Some(wizard);
                anyhow::bail!("Installation review page is unavailable.");
            }
        }
        if wizard.current() == Page::Validate {
            wizard.next(Gate {
                can_navigate_forward: true,
                validation_passed: true,
            });
        }
        self.wizard = Some(wizard);
        self.surface = Surface::Wizard;
        Ok(())
    }

    pub fn begin_validation(&mut self) -> Result<JobSpec> {
        self.begin(JobKind::Validate)
    }

    pub fn begin_conflicts(&mut self) -> Result<JobSpec> {
        self.begin(JobKind::Conflicts)
    }

    pub fn begin_restore(&mut self) -> Result<JobSpec> {
        self.begin(JobKind::Restore)
    }

    fn begin(&mut self, kind: JobKind) -> Result<JobSpec> {
        ensure!(!self.busy(), "Wait for the current operation to finish.");
        let guide = self.guide.clone().context("Load a guide first.")?;
        if kind == JobKind::Install {
            ensure!(
                self.validation_current(),
                "Installation blocked: run successful validation for the current guide and folders first."
            );
        }
        let mut settings = match &self.install.roots {
            Some((mods, game)) if kind == JobKind::Restore => Settings::new(mods, game)?,
            _ => Settings::new(Path::new(&self.mods), Path::new(&self.game))
                .context("Check directories")?,
        };
        self.cancelled = settings.cancelled.clone();
        if kind != JobKind::Restore {
            self.install.roots = Some((settings.roots.mods.clone(), settings.roots.game.clone()));
        }
        settings.allow_processes = false;
        self.confirm_install = false;
        self.stop_prompt = false;
        self.job = Some(kind);
        match kind {
            JobKind::Validate => {
                self.validation = None;
                self.validation_stamp = None;
                let total = guide.components.iter().filter(|c| c.is_selected).count();
                self.validation_progress = Some((0, total, "Starting validation".into()));
                self.log("Validating…");
            }
            JobKind::Install => {
                self.install = InstallView {
                    active: true,
                    roots: self.install.roots.take(),
                    ..InstallView::default()
                };
                self.phase_failed = false;
                self.log("Installing…");
            }
            JobKind::Restore => self.log("Restoring snapshot…"),
            JobKind::Conflicts => {
                self.conflicts = None;
                self.log("Analyzing file conflicts…");
            }
        }
        Ok(JobSpec {
            kind,
            guide,
            settings,
            retained: self.completed_base.clone(),
            case_insensitive: self.case_insensitive,
        })
    }

    pub fn request_stop(&mut self) {
        if self.busy() {
            self.stop_prompt = true;
        }
    }

    pub fn dismiss_stop(&mut self) {
        self.stop_prompt = false;
    }

    pub fn stop(&mut self) {
        self.stop_prompt = false;
        if self.busy() {
            self.cancelled.store(true, Ordering::Relaxed);
            self.log("Stopping after the current step…");
        }
    }

    pub fn apply_update(&mut self, update: Update) {
        match update {
            Update::Log(line) => self.log(line),
            Update::Validation(progress) => {
                self.validation_progress = Some((progress.step, progress.total, progress.message));
            }
            Update::Install(progress) => {
                self.install.progress = Some(json!({
                    "phase": serde_json::to_value(progress.phase).unwrap_or(Value::Null),
                    "component": progress.top_level_component_name,
                    "componentIndex": progress.component_index,
                    "componentTotal": progress.component_total,
                    "instructionIndex": progress.instruction_index,
                    "instructionTotal": progress.instruction_total,
                    "operation": progress.operation,
                    "message": progress.message,
                    "snapshotPending": progress.snapshot_pending,
                }));
                if progress.snapshot_pending {
                    self.install.pending = true;
                }
            }
            Update::Prepared(ordered) => self.pending_base = ordered,
        }
    }

    pub fn apply_terminal(&mut self, terminal: Terminal) {
        let kind = self.job.take();
        self.cancelled = Arc::new(AtomicBool::new(false));
        match terminal {
            Terminal::Validated(report) => {
                self.validation_progress = None;
                let view = ValidationView {
                    success: report.success,
                    errors: report.error_count,
                    warnings: report.warning_count,
                    passed: report.passed_count,
                    components: report.component_count,
                    stages: report
                        .stages
                        .iter()
                        .map(|stage| {
                            json!({"name": format!("{:?}", stage.stage), "passed": stage.passed,
                                "warnings": stage.has_warnings, "summary": stage.summary,
                                "messages": stage.messages})
                        })
                        .collect(),
                    issues: report
                        .dry_run
                        .as_ref()
                        .map(|dry| {
                            dry.issues
                                .iter()
                                .take(200)
                                .map(|issue| {
                                    json!({"severity": issue.severity, "message": issue.message,
                                        "component": issue.component_name})
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    failure: None,
                };
                let success = view.success;
                self.validation = Some(view);
                if success {
                    self.validation_stamp = Some(self.stamp());
                    self.log(
                        "Validation passed. External program effects, if any, remain unverified.",
                    );
                } else {
                    self.validation_stamp = None;
                    self.log("Validation found problems. Fix them and run validation again.");
                }
            }
            Terminal::ValidationFailed(error) => {
                self.validation_progress = None;
                self.validation = Some(ValidationView {
                    failure: Some(error.clone()),
                    ..ValidationView::default()
                });
                self.validation_stamp = None;
                self.log(format!("Validation stopped: {error}"));
            }
            Terminal::Installed { pending } => {
                self.install.active = false;
                self.install.pending = pending;
                self.apply_installed();
            }
            Terminal::InstallFailed { error, pending } => {
                self.install.active = false;
                self.install.error = Some(error.clone());
                self.install.pending = pending;
                self.phase_failed = true;
                self.pending_base.clear();
                self.log(format!("Stopped: {error}"));
            }
            Terminal::Conflicts(result) => match result {
                Ok(presentation) => {
                    self.conflicts = serde_json::to_value(&*presentation).ok();
                    self.log("Conflict analysis complete. Review the report and its limitations.");
                }
                Err(error) => {
                    self.conflicts = Some(json!({"failure": error}));
                    self.log(format!("Stopped: {error}"));
                }
            },
            Terminal::Restored(result) => {
                debug_assert_eq!(kind, Some(JobKind::Restore));
                match result {
                    Ok(()) => {
                        self.install.pending = false;
                        self.install.restored = true;
                        self.log("Snapshot restored and file hashes verified.");
                    }
                    Err(error) => self.log(format!("Restore failed: {error}")),
                }
            }
        }
    }

    fn apply_installed(&mut self) {
        if self.completed_base.is_none() {
            let pending = std::mem::take(&mut self.pending_base);
            if let Some(guide) = self.guide.as_mut() {
                for effective in &pending {
                    if let Some(component) = guide
                        .components
                        .iter_mut()
                        .find(|c| c.guid == effective.guid)
                    {
                        *component = effective.clone();
                    }
                }
            }
            self.completed_base = Some(pending.iter().map(|c| c.guid.clone()).collect());
            self.log("Installation complete.");
        } else {
            self.finished = true;
            self.log("Widescreen installation complete.");
        }
    }

    pub fn revalidate_phase(&mut self) -> Result<()> {
        ensure!(!self.busy(), "Wait for the current operation to finish.");
        ensure!(
            self.phase_failed && !self.install.pending,
            "Restore the snapshot before validating again."
        );
        let guide = self.guide.as_ref().context("Load a guide first.")?;
        let target = if self.completed_base.is_some() {
            Page::WidescreenModSelection
        } else {
            Page::Validate
        };
        let mut wizard = Wizard::new(guide);
        while wizard.current() != target {
            ensure!(
                wizard.next(Gate {
                    can_navigate_forward: true,
                    validation_passed: true,
                }),
                "Retry validation page is unavailable."
            );
        }
        self.install = InstallView::default();
        self.phase_failed = false;
        self.invalidate();
        self.wizard = Some(wizard);
        self.surface = Surface::Wizard;
        self.log("Run validation again before reviewing and retrying this phase.");
        Ok(())
    }

    pub fn snapshot(&self) -> Value {
        let wizard = self.wizard.as_ref();
        let gate = self.gate();
        let page = wizard.map(Wizard::current);
        let guide = self.guide.as_ref();
        let detected = self.detected();
        let pages: Vec<Value> = wizard
            .map(|wizard| {
                wizard
                    .pages()
                    .iter()
                    .map(|page| json!({"id": format!("{page:?}"), "title": page.title()}))
                    .collect()
            })
            .unwrap_or_default();
        let metadata = |camel: &str, pascal: &str| {
            guide
                .and_then(|guide| metadata_text(guide, camel, pascal))
                .unwrap_or_default()
                .to_owned()
        };
        let components: Vec<Value> = guide
            .map(|guide| {
                guide
                    .components
                    .iter()
                    .enumerate()
                    .map(|(index, c)| {
                        json!({"index": index, "guid": c.guid,
                            "name": selection::display_name(c, false),
                            "description": selection::display_description(c, false),
                            "author": c.author, "tier": c.tier, "categories": c.category,
                            "selected": c.is_selected, "widescreen": widescreen(c),
                            "requires": c.dependencies.len(),
                            "visible": selection::matches_filters(c, &self.search, "", &self.category, false)})
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut categories: Vec<String> = guide
            .map(|guide| {
                guide
                    .components
                    .iter()
                    .filter(|c| !widescreen(c))
                    .flat_map(|c| c.category.iter().cloned())
                    .collect()
            })
            .unwrap_or_default();
        categories.sort_by_key(|name| name.to_lowercase());
        categories.dedup();
        let (required, optional) = guide
            .map(|guide| selection::selection_badges(&guide.components))
            .unwrap_or((0, 0));
        let summary = guide
            .map(|guide| selection::selection_summary(&guide.components))
            .unwrap_or_default();
        let conflicts = self.conflicts.clone();
        let validation = self.validation.as_ref().map(|view| {
            json!({"success": view.success, "errors": view.errors, "warnings": view.warnings,
                "passed": view.passed, "components": view.components, "stages": view.stages,
                "issues": view.issues, "failure": view.failure,
                "current": self.validation_stamp.is_some() && view.success})
        });
        let install_review = self.completed_base.is_none()
            && !self.phase_failed
            && !self.busy()
            && self.selected_base() > 0
            && self.validation_current();
        let mut out = json!({
            "loaded": guide.is_some(),
            "surface": self.surface.name(),
            "theme": self.theme,
            "input": self.input, "mods": self.mods, "game": self.game,
            "gameDetected": match detected {
                game::Detected::Unknown => Value::Null,
                game::Detected::Kotor1 => json!("Knights of the Old Republic"),
                game::Detected::Kotor2Legacy => json!("The Sith Lords"),
                game::Detected::Kotor2Aspyr => json!("The Sith Lords (Aspyr)"),
            },
            "modsValid": !self.mods.is_empty() && Path::new(&self.mods).is_dir(),
        });
        let route = json!({
            "page": page.map(|page| format!("{page:?}")),
            "pageTitle": page.map(Page::title),
            "pageSubtitle": page.map(Page::subtitle),
            "pageIndex": wizard.map_or(0, Wizard::current_index),
            "pages": pages,
            "canNext": wizard.is_some_and(|wizard| wizard.can_next(gate)),
            "canBack": wizard.is_some_and(Wizard::can_back) && !self.busy(),
            "hint": "Complete this page before continuing.",
        });
        let texts = json!({
            "preamble": metadata("preambleContent", "PreambleContent"),
            "aspyr": metadata("aspyrExclusiveWarningContent", "AspyrExclusiveWarningContent"),
            "widescreenWarning": metadata("widescreenWarningContent", "WidescreenWarningContent"),
            "epilogue": metadata("epilogueContent", "EpilogueContent"),
            "guideName": guide
                .and_then(|guide| {
                    guide
                        .metadata
                        .get("title")
                        .or_else(|| guide.metadata.get("name"))
                        .and_then(Value::as_str)
                })
                .unwrap_or_default(),
        });
        let selection_state = json!({
            "components": components,
            "categories": categories,
            "search": self.search, "category": self.category,
            "summary": summary,
            "required": required, "optional": optional,
            "selectedBase": self.selected_base(), "selectedWidescreen": self.selected_widescreen(),
        });
        let job_state = json!({
            "validation": validation,
            "conflicts": conflicts,
            "canInstallReview": install_review,
            "confirmInstall": self.confirm_install,
            "stopPrompt": self.stop_prompt,
            "busy": self.job.map(|job| match job {
                JobKind::Validate => "validate",
                JobKind::Install => "install",
                JobKind::Restore => "restore",
                JobKind::Conflicts => "conflicts",
            }),
            "install": {
                "active": self.install.active,
                "error": self.install.error,
                "pending": self.install.pending,
                "restored": self.install.restored,
                "failed": self.phase_failed,
                "baseComplete": self.completed_base.is_some(),
                "finished": self.finished,
            },
        });
        if let Some(object) = out.as_object_mut() {
            for part in [route, texts, selection_state, job_state] {
                if let Value::Object(fields) = part {
                    object.extend(fields);
                }
            }
        }
        out
    }

    pub fn activity(&self) -> Value {
        json!({
            "validationProgress": self.validation_progress.as_ref().map(|(step, total, message)|
                json!({"step": step, "total": total, "message": message})),
            "install": self.install.progress,
            "logs": self.logs.iter().rev().take(60).rev().collect::<Vec<_>>(),
        })
    }
}

pub fn run_job(spec: JobSpec, mut observe: impl FnMut(Update) + Send) -> Terminal {
    let kind = spec.kind;
    let roots = spec.settings.roots.clone();
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| execute(spec, &mut observe)));
    let panicked = || {
        let error =
            "Worker stopped unexpectedly. Inspect recovery state before retrying.".to_owned();
        match kind {
            JobKind::Validate => Terminal::ValidationFailed(error),
            JobKind::Install => Terminal::InstallFailed {
                error,
                pending: true,
            },
            JobKind::Restore => Terminal::Restored(Err(error)),
            JobKind::Conflicts => Terminal::Conflicts(Err(error)),
        }
    };
    let mut terminal = result.unwrap_or_else(|_| panicked());
    if let Terminal::Installed { pending } | Terminal::InstallFailed { pending, .. } = &mut terminal
    {
        *pending = recovery::state_dir(&roots).join("pending").exists()
            || modsync::checkpoint::Store::open(&roots)
                .and_then(|store| {
                    Ok(!store.pending_operations()?.is_empty()
                        || store.external_process_status()?.present)
                })
                .unwrap_or(true);
    }
    terminal
}

fn execute(spec: JobSpec, observe: &mut (impl FnMut(Update) + Send)) -> Terminal {
    let JobSpec {
        kind,
        guide,
        settings,
        retained,
        case_insensitive,
    } = spec;
    if kind == JobKind::Conflicts {
        return conflict_analysis(guide, settings, retained, case_insensitive);
    }
    if kind == JobKind::Restore {
        return Terminal::Restored(
            recovery::restore(&settings.roots).map_err(|e| format!("{e:#}")),
        );
    }
    let fail = |error: String| match kind {
        JobKind::Install => Terminal::InstallFailed {
            error,
            pending: false,
        },
        _ => Terminal::ValidationFailed(error),
    };
    let phase = match (&retained, kind) {
        (Some(_), _) => Phase::Remaining,
        (None, JobKind::Validate) => Phase::All,
        (None, _) => Phase::Base,
    };
    let context = match retained.as_deref() {
        Some(ids) => CompletionContext::Retained { ids },
        None => CompletionContext::None,
    };
    let prepared = match modsync::phase::prepare_with_roots(
        &guide,
        phase,
        context,
        None,
        &settings.roots,
        case_insensitive,
        &settings.cancelled,
    ) {
        Ok(prepared) => prepared,
        Err(error) => return fail(format!("{error:#}")),
    };
    let completed = retained.as_ref().map(|_| prepared.completed.clone());
    let prepared_guide = prepared.guide;
    if kind == JobKind::Install {
        let ordered = prepared_guide
            .ordered()
            .map(|list| list.iter().map(|c| (**c).clone()).collect())
            .unwrap_or_default();
        observe(Update::Prepared(ordered));
    }
    let report_settings = settings.clone();
    let engine = match Engine::new_with_case_insensitive_pathing(settings, case_insensitive) {
        Ok(engine) => engine,
        Err(error) => {
            if kind == JobKind::Validate
                && let Some(report) = Engine::case_collision_report(
                    &report_settings,
                    case_insensitive,
                    &error,
                    &prepared_guide,
                    completed.as_deref().unwrap_or_default(),
                )
            {
                return Terminal::Validated(Box::new(report));
            }
            return fail(format!("{error:#}"));
        }
    };
    let cell = std::cell::RefCell::new(observe);
    let emit = |event: modsync::Event| {
        (cell.borrow_mut())(Update::Log(format!(
            "{} / {}: {}",
            event.component, event.step, event.message
        )));
    };
    match kind {
        JobKind::Validate => {
            let progress = |progress| (cell.borrow_mut())(Update::Validation(progress));
            let report = match completed {
                Some(ids) => {
                    match engine.validate_report_remaining_observed(
                        &prepared_guide,
                        &ids,
                        emit,
                        progress,
                    ) {
                        Ok(report) => report,
                        Err(error) => return fail(format!("{error:#}")),
                    }
                }
                None => engine.validate_report_with_options_observed(
                    &prepared_guide,
                    emit,
                    true,
                    true,
                    false,
                    progress,
                ),
            };
            Terminal::Validated(Box::new(report))
        }
        JobKind::Install => {
            let progress = |progress| (cell.borrow_mut())(Update::Install(progress));
            let outcome = match completed {
                Some(ids) => engine
                    .install_remaining_observed_report(&prepared_guide, &ids, emit, progress)
                    .map(|_| ()),
                None => engine
                    .install_observed_report(&prepared_guide, emit, progress)
                    .map(|_| ()),
            };
            match outcome {
                Ok(()) => Terminal::Installed { pending: false },
                Err(error) => Terminal::InstallFailed {
                    error: format!("{error:#}"),
                    pending: false,
                },
            }
        }
        JobKind::Restore | JobKind::Conflicts => {
            unreachable!("restore and conflict analysis return before this point")
        }
    }
}

fn conflict_analysis(
    guide: Guide,
    settings: Settings,
    retained: Option<Vec<String>>,
    case_insensitive: bool,
) -> Terminal {
    let outcome = (|| -> Result<modsync::conflicts::Presentation> {
        let phase = if retained.is_some() {
            Phase::Remaining
        } else {
            Phase::Base
        };
        let context = match retained.as_deref() {
            Some(ids) => CompletionContext::Retained { ids },
            None => CompletionContext::None,
        };
        let prepared = modsync::phase::prepare_with_roots(
            &guide,
            phase,
            context,
            None,
            &settings.roots,
            case_insensitive,
            &settings.cancelled,
        )?;
        let completed = retained.as_ref().map(|_| prepared.completed.clone());
        let engine = Engine::new_with_case_insensitive_pathing(settings, case_insensitive)?;
        let report = match completed {
            Some(ids) => engine.analyze_conflicts_remaining(&prepared.guide, &ids)?,
            None => engine.analyze_conflicts(&prepared.guide)?,
        };
        Ok(modsync::conflicts::Presentation::from_report(&report)?)
    })();
    Terminal::Conflicts(outcome.map(Box::new).map_err(|error| format!("{error:#}")))
}
