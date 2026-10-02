//! Shared FOMOD selection, configuration and archive ownership rules.
mod ordinal;
mod resolved_choices;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

/// Case key for FOMOD and shared selection/authoring names, using the explicit
/// Unicode 15.0 simple-uppercase dataset and .NET 9 ICU ordinal exceptions.
/// This never expands, normalizes or applies culture-specific string casing.
/// Scalars without a mapping in that dataset retain their exact identity.
pub fn fold_case(value: &str) -> String {
    ordinal::key(value)
}

/// Equality under the declared Unicode 15.0 ordinal name-key contract.
pub fn eq_ignore_case(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right) || fold_case(left) == fold_case(right)
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum GroupType {
    SelectAny,
    SelectExactlyOne,
    SelectAtMostOne,
    SelectAtLeastOne,
    SelectAll,
}

/// FOMOD's authored plugin type descriptor. It controls defaults only for
/// Required and Recommended; retain the other values for inspection and
/// future dependency-type support instead of silently converting them.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum PluginType {
    Optional,
    Required,
    Recommended,
    NotUsable,
    CouldBeUsable,
}

impl PluginType {
    pub(crate) fn from_xml(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "required" => Self::Required,
            "recommended" => Self::Recommended,
            "notusable" => Self::NotUsable,
            "couldbeusable" => Self::CouldBeUsable,
            _ => Self::Optional,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PluginSelection {
    pub name: String,
    pub description: String,
    /// Archive-relative authored path only; not a trusted filesystem path.
    pub image_path: String,
    pub files: Vec<FileInstall>,
    pub plugin_type: PluginType,
    pub required: bool,
    pub flags: Vec<(String, String)>,
    /// Seed with Required/Recommended descriptor defaults before creating the group.
    pub selected: bool,
}

#[derive(Clone, Debug)]
pub struct GroupSelection {
    pub name: String,
    pub kind: GroupType,
    plugins: Vec<PluginSelection>,
}

impl GroupSelection {
    pub fn new(name: String, kind: GroupType, mut plugins: Vec<PluginSelection>) -> Self {
        for plugin in &mut plugins {
            plugin.selected |= plugin.required || kind == GroupType::SelectAll;
        }
        if matches!(
            kind,
            GroupType::SelectExactlyOne | GroupType::SelectAtLeastOne
        ) && !plugins.iter().any(|p| p.selected)
            && let Some(first) = plugins.first_mut()
        {
            first.selected = true;
        }
        Self {
            name,
            kind,
            plugins,
        }
    }

    pub fn plugins(&self) -> &[PluginSelection] {
        &self.plugins
    }

    /// Rejected changes leave every selection intact.
    pub fn set_selected(&mut self, index: usize, selected: bool) -> Result<()> {
        let plugin = self
            .plugins
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("FOMOD plugin index {index} is out of range"))?;
        ensure!(
            selected || !plugin.required,
            "Cannot deselect required plugin '{}' in group '{}'.",
            plugin.name,
            self.name
        );
        let exclusive = matches!(
            self.kind,
            GroupType::SelectExactlyOne | GroupType::SelectAtMostOne
        );
        if selected && exclusive {
            // The reference clears required siblings here. Preserve required choices instead.
            ensure!(
                !self
                    .plugins
                    .iter()
                    .enumerate()
                    .any(|(i, p)| i != index && p.required),
                "Cannot replace a required plugin in group '{}'.",
                self.name
            );
            for (i, plugin) in self.plugins.iter_mut().enumerate() {
                plugin.selected = i == index;
            }
        } else {
            self.plugins[index].selected = selected;
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        let count = self.plugins.iter().filter(|p| p.selected).count();
        match self.kind {
            GroupType::SelectExactlyOne => {
                ensure!(count == 1, "Select exactly one option in '{}'.", self.name)
            }
            GroupType::SelectAtMostOne => {
                ensure!(count <= 1, "Select at most one option in '{}'.", self.name)
            }
            GroupType::SelectAtLeastOne => {
                ensure!(count >= 1, "Select at least one option in '{}'.", self.name)
            }
            GroupType::SelectAll => ensure!(
                count == self.plugins.len(),
                "All options in '{}' must be selected.",
                self.name
            ),
            GroupType::SelectAny => {}
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoicesFile {
    #[serde(default = "choices_version")]
    pub version: u32,
    #[serde(default)]
    pub archives: Vec<ArchiveChoices>,
}
fn choices_version() -> u32 {
    1
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveChoices {
    pub archive_file_name: String,
    #[serde(default)]
    pub selections: Vec<NamedSelection>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedSelection {
    pub step_name: String,
    pub group_name: String,
    #[serde(default)]
    pub plugins: Vec<String>,
}

impl ChoicesFile {
    /// Read a bounded ordinary UTF-8 sidecar without following a symlink or
    /// blocking on a FIFO. Desktop preparation calls this on its worker.
    pub fn load(path: &std::path::Path) -> Result<Self> {
        use std::io::Read;
        let mut bytes = Vec::new();
        crate::paths::open_regular_read(path)?
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 8 * 1024 * 1024,
            "FOMOD choices file exceeds 8 MiB"
        );
        Self::parse(std::str::from_utf8(&bytes)?)
    }
    pub fn parse(json: &str) -> Result<Self> {
        let choices: Self = serde_json::from_str(json)?;
        ensure!(
            choices.version == 1,
            "Unsupported FOMOD choices version {}",
            choices.version
        );
        let mut archives = std::collections::BTreeSet::new();
        for archive in &choices.archives {
            ensure!(
                !archive.archive_file_name.trim().is_empty(),
                "FOMOD archive filename is empty"
            );
            ensure!(
                archives.insert(fold_case(&archive.archive_file_name)),
                "Duplicate FOMOD archive '{}'",
                archive.archive_file_name
            );
            let mut groups = std::collections::BTreeSet::new();
            for selection in &archive.selections {
                ensure!(
                    groups.insert((
                        fold_case(&selection.step_name),
                        fold_case(&selection.group_name)
                    )),
                    "Duplicate FOMOD selection for '{}' / '{}'",
                    selection.step_name,
                    selection.group_name
                );
            }
        }
        Ok(choices)
    }

    pub fn archive(&self, filename: &str) -> Option<&ArchiveChoices> {
        self.archives
            .iter()
            .find(|a| eq_ignore_case(&a.archive_file_name, filename))
    }
}

impl GroupSelection {
    /// Apply an explicit choices-file group atomically; omitted plugins are deselected.
    pub fn apply_names(&mut self, names: &[String]) -> Result<()> {
        let mut selected = std::collections::BTreeSet::new();
        for name in names {
            let matches: Vec<_> = self
                .plugins
                .iter()
                .enumerate()
                .filter(|(_, p)| eq_ignore_case(&p.name, name))
                .map(|(i, _)| i)
                .collect();
            ensure!(
                matches.len() == 1,
                "FOMOD plugin '{}' in group '{}' is missing or ambiguous",
                name,
                self.name
            );
            selected.insert(matches[0]);
        }
        let mut candidate = self.clone();
        for (i, plugin) in candidate.plugins.iter_mut().enumerate() {
            ensure!(
                !plugin.required || selected.contains(&i),
                "Cannot deselect required plugin '{}' in group '{}'.",
                plugin.name,
                self.name
            );
            plugin.selected = selected.contains(&i);
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct StepSelection {
    pub name: String,
    pub groups: Vec<GroupSelection>,
    pub visible: Option<FlagCondition>,
}

impl ArchiveChoices {
    /// Apply choices and validate the resulting visible steps.
    /// Unknown or ambiguous names and invalid groups leave the entire set unchanged.
    pub fn apply_to(&self, steps: &mut [StepSelection]) -> Result<()> {
        let mut candidate = steps.to_vec();
        let mut seen = std::collections::BTreeSet::new();
        for selection in &self.selections {
            ensure!(
                seen.insert((
                    fold_case(&selection.step_name),
                    fold_case(&selection.group_name)
                )),
                "Duplicate FOMOD selection for '{}' / '{}'",
                selection.step_name,
                selection.group_name
            );
            let mut matches = candidate
                .iter_mut()
                .filter(|s| eq_ignore_case(&s.name, &selection.step_name));
            let step = matches.next().ok_or_else(|| {
                anyhow::anyhow!("FOMOD step '{}' was not found", selection.step_name)
            })?;
            ensure!(
                matches.next().is_none(),
                "FOMOD step '{}' is ambiguous",
                selection.step_name
            );
            let mut matches = step
                .groups
                .iter_mut()
                .filter(|g| eq_ignore_case(&g.name, &selection.group_name));
            let group = matches.next().ok_or_else(|| {
                anyhow::anyhow!(
                    "FOMOD group '{}' was not found in step '{}'",
                    selection.group_name,
                    step.name
                )
            })?;
            ensure!(
                matches.next().is_none(),
                "FOMOD group '{}' is ambiguous in step '{}'",
                selection.group_name,
                step.name
            );
            group.apply_names(&selection.plugins)?;
        }
        for index in visible_steps(&candidate) {
            for group in &candidate[index].groups {
                group.validate()?;
            }
        }
        steps.clone_from_slice(&candidate);
        Ok(())
    }
}

/// Flag-only dependency expressions. File/game dependencies need a separate evaluator.
#[derive(Clone, Debug)]
pub enum FlagCondition {
    Flag {
        name: String,
        value: String,
    },
    /// C# does not model FOMOD file/game dependency state and treats these
    /// leaves as satisfied. Keep their effect explicit rather than rejecting
    /// otherwise usable packages. The label names the authored dependency
    /// (C# wording, e.g. `fileDependency 'x.esp'`) for the fallback warning.
    Always(String),
    All(Vec<FlagCondition>),
    Any(Vec<FlagCondition>),
}

impl FlagCondition {
    fn matches(&self, flags: &std::collections::BTreeMap<String, String>) -> bool {
        match self {
            Self::Flag { name, value } => flags.get(&fold_case(name)) == Some(value),
            Self::Always(_) => true,
            Self::All(children) => children.iter().all(|c| c.matches(flags)),
            // C# treats every empty composite as true, including Or.
            Self::Any(children) => children.is_empty() || children.iter().any(|c| c.matches(flags)),
        }
    }
}

/// Reference order: all selected plugins contribute, including those in hidden steps.
/// Later flag assignments win; names ignore case (C# OrdinalIgnoreCase) and values are
/// case-sensitive.
pub fn visible_steps(steps: &[StepSelection]) -> Vec<usize> {
    let mut flags = std::collections::BTreeMap::new();
    for step in steps {
        for group in &step.groups {
            for plugin in group.plugins().iter().filter(|p| p.selected) {
                for (name, value) in &plugin.flags {
                    flags.insert(fold_case(name), value.clone());
                }
            }
        }
    }
    steps
        .iter()
        .enumerate()
        .filter_map(|(i, step)| {
            step.visible
                .as_ref()
                .is_none_or(|c| c.matches(&flags))
                .then_some(i)
        })
        .collect()
}

mod metadata;
mod xml;
pub use xml::{
    parse_info_xml, parse_info_xml_with_cancel, parse_module_xml, parse_module_xml_with_cancel,
    parse_selection_xml,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileInstall {
    pub source: String,
    pub destination: String,
    pub priority: i32,
    pub is_folder: bool,
}

#[derive(Clone, Debug)]
pub struct ConditionalFiles {
    pub condition: FlagCondition,
    pub files: Vec<FileInstall>,
}

#[derive(Clone, Debug)]
pub struct ModuleConfig {
    pub name: String,
    pub required_files: Vec<FileInstall>,
    pub steps: Vec<StepSelection>,
    pub conditional_files: Vec<ConditionalFiles>,
}

/// How the reference mapper places one conditional file pattern.
enum PatternPlacement {
    /// No flag dependency: installed after every plugin's files.
    Trailing,
    /// Attached to the first plugin (global index) that sets every flag.
    Plugin(usize),
    /// Or/nested composites the reference cannot attach (C# skips with a warning).
    Unsupported,
    /// No single plugin sets every flag (C# skips with a warning).
    Unattached,
}

impl ModuleConfig {
    fn plugins(&self) -> Vec<(usize, &PluginSelection)> {
        self.steps
            .iter()
            .enumerate()
            .flat_map(|(i, step)| {
                step.groups
                    .iter()
                    .flat_map(move |group| group.plugins().iter().map(move |p| (i, p)))
            })
            .collect()
    }

    /// Reference `ApplyConditionalInstalls` classification, with the labels of
    /// file/game dependencies it treats as satisfied.
    fn place_pattern(
        pattern: &ConditionalFiles,
        plugins: &[(usize, &PluginSelection)],
    ) -> (PatternPlacement, Vec<String>) {
        let children: Vec<_> = match &pattern.condition {
            FlagCondition::Flag { .. } | FlagCondition::Always(_) => vec![&pattern.condition],
            FlagCondition::All(children) => children.iter().collect(),
            FlagCondition::Any(children) if children.len() <= 1 => children.iter().collect(),
            _ => return (PatternPlacement::Unsupported, Vec::new()),
        };
        let mut flags = Vec::new();
        let mut ignored = Vec::new();
        for condition in children {
            match condition {
                FlagCondition::Flag { name, value } => flags.push((name, value)),
                FlagCondition::Always(label) => ignored.push(label.clone()),
                _ => return (PatternPlacement::Unsupported, ignored),
            }
        }
        if flags.is_empty() {
            return (PatternPlacement::Trailing, ignored);
        }
        let index = plugins.iter().position(|(_, plugin)| {
            flags.iter().all(|(name, value)| {
                plugin
                    .flags
                    .iter()
                    .any(|(n, v)| eq_ignore_case(n, name) && v == *value)
            })
        });
        (
            index.map_or(PatternPlacement::Unattached, PatternPlacement::Plugin),
            ignored,
        )
    }

    /// Build the reference mapper's file order without resolving paths or writing files.
    /// Conditional patterns that the reference cannot attach to one plugin are skipped;
    /// [`ModuleConfig::fallback_warnings`] reports them.
    pub fn selected_files(&self) -> Result<Vec<&FileInstall>> {
        let visible = visible_steps(&self.steps);
        let plugins = self.plugins();
        let mut attached: std::collections::BTreeMap<usize, Vec<&FileInstall>> =
            std::collections::BTreeMap::new();
        let mut trailing = Vec::new();
        for pattern in &self.conditional_files {
            match Self::place_pattern(pattern, &plugins).0 {
                PatternPlacement::Trailing => trailing.extend(&pattern.files),
                PatternPlacement::Plugin(index) => {
                    attached.entry(index).or_default().extend(&pattern.files)
                }
                PatternPlacement::Unsupported | PatternPlacement::Unattached => {}
            }
        }
        for &index in &visible {
            for group in &self.steps[index].groups {
                group.validate()?;
            }
        }
        let mut files: Vec<_> = self.required_files.iter().collect();
        for (index, (step, plugin)) in plugins.iter().enumerate() {
            if visible.contains(step) && plugin.selected {
                files.extend(&plugin.files);
                if let Some(extra) = attached.get(&index) {
                    files.extend(extra);
                }
            }
        }
        files.extend(trailing);
        Ok(files)
    }

    /// Visible diagnostics for the fallbacks where the reference mapper
    /// (`FomodToComponentMapper`) logs a warning: file/game dependencies treated as
    /// satisfied, skipped conditional patterns and dropped destination renames. They
    /// describe the installer, not a selection, so every authored entry is covered.
    pub fn fallback_warnings(&self) -> Vec<String> {
        fn unevaluated(condition: &FlagCondition, warnings: &mut WarningCollector) {
            match condition {
                FlagCondition::Always(label) => warnings.push(format!(
                    "{} is not evaluated; treating as satisfied.",
                    warning_text(label)
                )),
                FlagCondition::All(children) | FlagCondition::Any(children) => {
                    for child in children {
                        unevaluated(child, warnings);
                    }
                }
                FlagCondition::Flag { .. } => {}
            }
        }
        fn rename(file: &FileInstall, warnings: &mut WarningCollector) {
            if let Some((source, destination)) = dropped_rename(file) {
                warnings.push(format!("FOMOD wants to install '{}' as '{}'; renames are not supported by the Copy action, the source file name is kept.", warning_text(&source), warning_text(&destination)));
            }
        }
        let mut warnings = WarningCollector::default();
        for step in &self.steps {
            if let Some(condition) = &step.visible {
                unevaluated(condition, &mut warnings);
            }
        }
        let plugins = self.plugins();
        for file in &self.required_files {
            rename(file, &mut warnings);
        }
        for (_, plugin) in &plugins {
            for file in &plugin.files {
                rename(file, &mut warnings);
            }
        }
        for pattern in &self.conditional_files {
            let (placement, _) = Self::place_pattern(pattern, &plugins);
            match placement {
                PatternPlacement::Unsupported => warnings.push("Skipping a conditionalFileInstalls pattern: only a flat And of flagDependency elements is supported in this slice.".into()),
                PatternPlacement::Unattached => warnings.push("Skipping a conditionalFileInstalls pattern: its flag dependencies are not all set by a single plugin.".into()),
                PatternPlacement::Trailing | PatternPlacement::Plugin(_) => {
                    unevaluated(&pattern.condition, &mut warnings);
                    for file in &pattern.files { rename(file, &mut warnings); }
                }
            }
        }
        warnings.finish()
    }
}

/// Bound authored diagnostic text before formatting or UI publication.
pub(crate) fn warning_text(value: &str) -> String {
    bounded_warning_text(value, 256)
}
fn bounded_warning_text(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let mut text: String = chars
        .by_ref()
        .take(limit)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if chars.next().is_some() {
        text.push('…');
    }
    text
}
/// Shared bounded warning collection. Counts beyond the cap are occurrences,
/// since retaining every omitted identity would itself be unbounded.
#[derive(Default)]
pub(crate) struct WarningCollector {
    warnings: Vec<String>,
    omitted: usize,
}
impl WarningCollector {
    pub(crate) fn push(&mut self, warning: String) {
        let warning = bounded_warning_text(&warning, 1024);
        if self.warnings.contains(&warning) {
            return;
        }
        if self.warnings.len() < 64 {
            self.warnings.push(warning);
        } else {
            self.omitted = self.omitted.saturating_add(1);
        }
    }
    pub(crate) fn finish(mut self) -> Vec<String> {
        if self.omitted > 0 {
            self.warnings.push(format!("{} additional FOMOD warning occurrences omitted; review the installer before proceeding.", self.omitted));
        }
        self.warnings
    }
}

/// Log an installer's mapper fallbacks as warnings with C# `FomodToComponentMapper`'s
/// warning semantics. Authored text and count are bounded; hosts also expose
/// these diagnostics in structured validation and the configuration editor.
pub fn log_fallback_warnings(config: &ModuleConfig) {
    for warning in config.fallback_warnings() {
        crate::logging::warning(format!("[FomodToComponentMapper] {warning}"));
    }
}

/// Source and destination names when the mapper keeps the source name although the
/// authored destination names a different file (C# `BuildDestination`).
fn dropped_rename(file: &FileInstall) -> Option<(String, String)> {
    if file.is_folder {
        return None;
    }
    let name = |path: &str| {
        path.trim()
            .replace('\\', "/")
            .trim_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_owned()
    };
    let (source, destination) = (name(&file.source), name(&file.destination));
    (destination.contains('.') && !eq_ignore_case(&destination, &source))
        .then_some((source, destination))
}

impl ModuleConfig {
    /// Map configured file copies from an already-extracted, mod-relative directory.
    /// This does not bypass archive configuration gates or perform extraction.
    pub fn copy_instructions(&self, extracted_directory: &str) -> Result<Vec<crate::Instruction>> {
        let root = crate::paths::relative(extracted_directory, false)?;
        ensure!(
            !root.as_os_str().is_empty(),
            "FOMOD extraction directory is empty"
        );
        let mut instructions = Vec::new();
        for file in self.selected_files()? {
            let source = crate::paths::relative(file.source.trim(), false)?;
            let mut destination = crate::paths::relative(file.destination.trim(), false)?;
            if !file.is_folder {
                let filename = source
                    .file_name()
                    .ok_or_else(|| anyhow::anyhow!("FOMOD file source is empty"))?
                    .to_string_lossy();
                if let Some(name) = destination.file_name() {
                    let name = name.to_string_lossy();
                    if name.contains('.') || eq_ignore_case(&name, &filename) {
                        // The shared Copy action preserves the source name. The C# mapper
                        // therefore drops an authored destination filename, including a
                        // requested rename, and keeps the destination directory.
                        destination.pop();
                    }
                }
            }
            let mut source = root.join(source).to_string_lossy().replace('\\', "/");
            if file.is_folder {
                // Verified C# mapper behavior: immediate files only, not recursive copy.
                source.push_str("/*");
            }
            let destination = destination.to_string_lossy().replace('\\', "/");
            instructions.push(crate::Instruction {
                action: "Copy".into(),
                source: vec![format!("<<modDirectory>>/{source}")],
                destination: if destination.is_empty() {
                    "<<gameDirectory>>".into()
                } else {
                    format!("<<gameDirectory>>/{destination}")
                },
                overwrite: true,
                guid: String::new(),
                parent: String::new(),
                arguments: String::new(),
                dependencies: vec![],
                restrictions: vec![],
            });
        }
        Ok(instructions)
    }
}

/// Optional authored info.xml metadata. Website is text, not a trusted URL.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FomodInfo {
    pub name: String,
    pub author: String,
    pub version: String,
    pub website: String,
    pub description: String,
}

/// Detect the terminal directory/file pair only; this does not validate path safety.
pub fn is_module_config_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let mut segments = normalized.trim_matches('/').rsplit('/');
    matches!((segments.next(), segments.next()), (Some(file), Some(directory))
        if file.eq_ignore_ascii_case("ModuleConfig.xml") && directory.eq_ignore_ascii_case("fomod"))
}

/// Parsed installer metadata from a verified archive; no installation is authorized.
pub struct ArchiveModule {
    pub module_path: std::path::PathBuf,
    pub content_root: std::path::PathBuf,
    pub config: ModuleConfig,
    pub info: Option<FomodInfo>,
}

/// Inspect in temporary scratch storage, retaining no extracted files.
/// ponytail: spools the whole archive through the existing verified reader;
/// selective metadata extraction can replace this if profiling warrants it.
pub fn inspect_archive(
    path: &std::path::Path,
    options: &crate::archive::Options,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<Option<ArchiveModule>> {
    let entries = crate::archive::unpack(path, path, options, cancelled)?;
    inspect_entries_with_cancel(&entries, cancelled)
}

/// Like [`inspect_archive`], but a verified listing decides first: only an archive
/// that lists a `fomod/ModuleConfig.xml` is spooled to scratch storage and parsed.
pub(crate) fn inspect_listed(
    path: &std::path::Path,
    name: &std::path::Path,
    options: &crate::archive::Options,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<Option<ArchiveModule>> {
    let names = crate::archive::list(path, name, options, cancelled)?;
    if !names.iter().any(|name| is_module_config_path(name)) {
        return Ok(None);
    }
    inspect_entries_with_cancel(
        &crate::archive::unpack(path, name, options, cancelled)?,
        cancelled,
    )
}

pub(crate) fn inspect_entries_with_cancel(
    entries: &[crate::archive::Entry],
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<Option<ArchiveModule>> {
    metadata::check(cancelled)?;
    let mut module = None;
    for entry in entries {
        metadata::check(cancelled)?;
        if is_module_config_path(&entry.path.to_string_lossy()) {
            ensure!(
                module.is_none(),
                "Archive contains multiple FOMOD installers"
            );
            module = Some(entry);
        }
    }
    let Some(module) = module else {
        metadata::check(cancelled)?;
        return Ok(None);
    };
    let directory = module.path.parent().unwrap();
    let info_path = directory
        .join("info.xml")
        .to_string_lossy()
        .to_ascii_lowercase();
    let mut info_entry = None;
    for entry in entries {
        metadata::check(cancelled)?;
        if entry.path.to_string_lossy().to_ascii_lowercase() == info_path {
            info_entry = Some(entry);
            break;
        }
    }
    let info = info_entry
        .map(|entry| -> Result<FomodInfo> {
            parse_info_xml_with_cancel(&metadata::read(entry.file.path(), cancelled)?, cancelled)
        })
        .transpose()?;
    Ok(Some(ArchiveModule {
        module_path: module.path.clone(),
        content_root: directory.parent().unwrap().to_path_buf(),
        config: parse_module_xml_with_cancel(
            &metadata::read(module.file.path(), cancelled)?,
            cancelled,
        )?,
        info,
    }))
}

impl ArchiveModule {
    /// Resolve mapped sources against both the extraction directory and archive wrapper.
    pub fn copy_instructions(&self, extracted_directory: &str) -> Result<Vec<crate::Instruction>> {
        let root = crate::paths::relative(extracted_directory, false)?;
        ensure!(
            !root.as_os_str().is_empty(),
            "FOMOD extraction directory is empty"
        );
        self.config.copy_instructions(
            &root
                .join(&self.content_root)
                .to_string_lossy()
                .replace('\\', "/"),
        )
    }
}

/// C# stores this state in `resourceRegistry.*.handlerMetadata.fomodPromptStatus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptStatus {
    Dismissed,
    Configured,
    Warned,
}
impl PromptStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Dismissed => "dismissed",
            Self::Configured => "configured",
            Self::Warned => "warned",
        }
    }
}

fn archive_name(name: &str) -> String {
    name.trim()
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn resource_matches(value: &serde_json::Value, filename: &str) -> bool {
    let Some(files) = value
        .get("files")
        .or_else(|| value.get("Files"))
        .and_then(serde_json::Value::as_object)
    else {
        return false;
    };
    files.keys().any(|name| {
        eq_ignore_case(&archive_name(name), filename) || eq_ignore_case(name.trim(), filename)
    })
}

/// Whether this component registers the archive under a resource-registry Files key.
pub fn resource_matches_archive(component: &crate::Component, archive: &str) -> bool {
    let filename = archive_name(archive);
    component
        .resource_registry
        .values()
        .any(|resource| resource_matches(resource, &filename))
}

/// Whether an Extract source could resolve to the archive under the engine's source
/// matching: a glob (`*`, `?`, `[...]`) over the file name, or a folder source that
/// contains the archive at any depth. The archive's folder is unknown here, so any
/// source whose last component is not an archive file name may be that folder, and
/// an unparseable glob is kept too: over-inclusion only applies a named entry.
fn source_may_extract(source: &str, filename: &str) -> bool {
    let source = source.trim().replace('\\', "/");
    let last = source
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("");
    !crate::engine::is_archive(last)
        || glob::Pattern::new(&fold_case(last)).map_or(true, |pattern| {
            crate::engine::source_glob_matches(&pattern, &fold_case(filename))
        })
}

/// Whether this component registers the archive or may extract it through one of its
/// own Extract sources (see [`source_may_extract`]). Options are separate components.
fn references_archive(component: &crate::Component, archive: &str) -> bool {
    let filename = archive_name(archive);
    resource_matches_archive(component, archive)
        || component
            .instructions
            .iter()
            .filter(|instruction| instruction.action == "Extract")
            .flat_map(|instruction| &instruction.source)
            .any(|source| source_may_extract(source, &filename))
}

/// Read the C#-compatible per-archive prompt status from the matching resource entry.
pub fn prompt_status(component: &crate::Component, archive: &str) -> Option<String> {
    let filename = archive_name(archive);
    let resource = component
        .resource_registry
        .values()
        .find(|resource| resource_matches(resource, &filename))?;
    let metadata = resource
        .get("handlerMetadata")
        .or_else(|| resource.get("HandlerMetadata"))?
        .as_object()?;
    let statuses = metadata.get("fomodPromptStatus")?.as_object()?;
    statuses
        .iter()
        .find(|(name, _)| eq_ignore_case(&archive_name(name), &filename))
        .and_then(|(_, status)| status.as_str())
        .map(str::to_owned)
}

/// Match C# prompt policy: dismissed prompts again; configured and warned do not.
pub fn should_prompt(component: &crate::Component, archive: &str) -> bool {
    !matches!(
        prompt_status(component, archive).as_deref(),
        Some("configured" | "warned")
    )
}

/// Persist a C#-compatible prompt state when a resource registry entry contains the archive.
/// Returns false when no entry matches or the matching entry's metadata is malformed.
pub fn set_prompt_status(
    component: &mut crate::Component,
    archive: &str,
    status: PromptStatus,
) -> bool {
    try_set_prompt_status(component, archive, status).unwrap_or(false)
}

/// Like [`set_prompt_status`], but malformed matching metadata is an error:
/// `Ok(false)` means no resource registry entry contains the archive.
pub fn try_set_prompt_status(
    component: &mut crate::Component,
    archive: &str,
    status: PromptStatus,
) -> Result<bool> {
    let filename = archive_name(archive);
    let Some(resource) = component
        .resource_registry
        .values_mut()
        .find(|resource| resource_matches(resource, &filename))
    else {
        return Ok(false);
    };
    write_prompt_status(resource, &filename, status)?;
    Ok(true)
}

/// Write one archive's prompt status into a resource registry entry, preserving the
/// entry's existing handler-metadata key spelling. Malformed shapes are errors.
fn write_prompt_status(
    resource: &mut serde_json::Value,
    filename: &str,
    status: PromptStatus,
) -> Result<()> {
    let object = resource
        .as_object_mut()
        .context("FOMOD resource entry must be an object")?;
    let key = if object.contains_key("handlerMetadata") {
        "handlerMetadata"
    } else if object.contains_key("HandlerMetadata") {
        "HandlerMetadata"
    } else {
        "handlerMetadata"
    };
    // Like C# FomodDownloadPromptState: null metadata becomes a fresh map, and a
    // missing or non-map status value is replaced rather than rejected.
    let metadata = object.entry(key).or_insert(serde_json::Value::Null);
    if metadata.is_null() {
        *metadata = serde_json::json!({});
    }
    let metadata = metadata
        .as_object_mut()
        .context("FOMOD handler metadata must be an object")?;
    let statuses = metadata
        .entry("fomodPromptStatus")
        .or_insert(serde_json::Value::Null);
    if !statuses.is_object() {
        *statuses = serde_json::json!({});
    }
    let statuses = statuses
        .as_object_mut()
        .context("FOMOD prompt status must be an object")?;
    // Like the C# OrdinalIgnoreCase status map, an existing key for this archive
    // (any case) is updated in place instead of gaining a differently cased twin.
    let value = serde_json::Value::String(status.as_str().into());
    let mut updated = false;
    for (name, current) in statuses.iter_mut() {
        if eq_ignore_case(&archive_name(name), filename) {
            *current = value.clone();
            updated = true;
        }
    }
    if !updated {
        statuses.insert(filename.to_owned(), value);
    }
    Ok(())
}

impl ChoicesFile {
    /// Override embedded choices per archive, recursively; the caller owns persistence
    /// of the guide (the CLI never persists it).
    pub fn apply_to_guide(&self, guide: &mut crate::Guide) -> Result<()> {
        self.apply_to_remaining(guide, &[])
    }

    /// Apply external choices without changing completed component definitions or
    /// their nested options. The engine still verifies the completed prefix and
    /// its receipt before any remaining-phase operation.
    ///
    /// Rust extension (C# validate/install take no choices file): an entry replaces
    /// that archive's embedded choices; other archives keep theirs. Unparseable
    /// embedded choices are an error naming the repair route, never discarded.
    pub fn apply_to_remaining(&self, guide: &mut crate::Guide, completed: &[String]) -> Result<()> {
        self.update_guide(guide, completed)
    }
}

impl ChoicesFile {
    /// Capture all selected plugins, including hidden steps that may supply flags.
    /// Reject selections that cannot be unambiguously reapplied by name.
    pub fn capture(archive_file_name: &str, steps: &[StepSelection]) -> Result<Self> {
        let archive = ArchiveChoices {
            archive_file_name: archive_file_name.into(),
            selections: steps
                .iter()
                .flat_map(|step| {
                    step.groups.iter().map(|group| NamedSelection {
                        step_name: step.name.clone(),
                        group_name: group.name.clone(),
                        plugins: group
                            .plugins()
                            .iter()
                            .filter(|p| p.selected)
                            .map(|p| p.name.clone())
                            .collect(),
                    })
                })
                .collect(),
        };
        archive.apply_to(&mut steps.to_vec())?;
        Self::parse(&serde_json::to_string(&Self {
            version: 1,
            archives: vec![archive],
        })?)
    }
}

/// Edit selections using numbered groups. Prompts go to the supplied writer,
/// leaving CLI stdout available for JSON. Cancellation leaves the session unchanged.
pub fn prompt_choices(
    steps: &mut [StepSelection],
    input: &mut impl std::io::BufRead,
    output: &mut impl std::io::Write,
) -> Result<()> {
    match prompt_choices_with(
        steps,
        &mut || {
            let mut line = String::new();
            Ok(if input.read_line(&mut line)? == 0 {
                PromptLine::Eof
            } else {
                PromptLine::Line(line)
            })
        },
        output,
    )? {
        PromptOutcome::Configured => Ok(()),
        PromptOutcome::Eof => anyhow::bail!("FOMOD selection cancelled at end of input"),
        _ => anyhow::bail!("FOMOD selection cancelled"),
    }
}

/// Typed input boundary shared by the standalone wizard and cancellable Fetch host.
pub enum PromptLine {
    Line(String),
    Eof,
    SignalCancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptOutcome {
    Configured,
    Refused,
    Eof,
    UserQuit,
    SignalCancelled,
}

/// A completed candidate replaces the session only after all visible groups validate.
pub fn prompt_choices_with(
    steps: &mut [StepSelection],
    input: &mut impl FnMut() -> Result<PromptLine>,
    output: &mut impl std::io::Write,
) -> Result<PromptOutcome> {
    let mut candidate = steps.to_vec();
    let mut cursor = 0;
    'steps: while cursor < candidate.len() {
        if !visible_steps(&candidate).contains(&cursor) {
            cursor += 1;
            continue;
        }
        writeln!(output, "Step: {}", candidate[cursor].name)?;
        for group_index in 0..candidate[cursor].groups.len() {
            loop {
                let group = &mut candidate[cursor].groups[group_index];
                writeln!(output, "{} ({:?})", group.name, group.kind)?;
                for (index, plugin) in group.plugins().iter().enumerate() {
                    writeln!(
                        output,
                        "{}. [{}] {}{}\n   {}",
                        index + 1,
                        if plugin.selected { "x" } else { " " },
                        plugin.name,
                        if plugin.required { " (required)" } else { "" },
                        plugin.description
                    )?;
                }
                write!(
                    output,
                    "Numbers separated by commas; Enter keeps selection, 0 clears, back revisits, q cancels: "
                )?;
                output.flush()?;
                let line = match input()? {
                    PromptLine::Line(line) => line,
                    PromptLine::Eof => return Ok(PromptOutcome::Eof),
                    PromptLine::SignalCancelled => return Ok(PromptOutcome::SignalCancelled),
                };
                let line = line.trim();
                if line.eq_ignore_ascii_case("q") {
                    return Ok(PromptOutcome::UserQuit);
                }
                if line.eq_ignore_ascii_case("back") {
                    cursor = visible_steps(&candidate)
                        .into_iter()
                        .rfind(|index| *index < cursor)
                        .unwrap_or(cursor);
                    continue 'steps;
                }
                let result = if line.is_empty() {
                    group.validate()
                } else {
                    let names = if line == "0" {
                        Ok(Vec::new())
                    } else {
                        line.split(',')
                            .map(|item| {
                                let index = item.trim().parse::<usize>()?;
                                let plugin = index
                                    .checked_sub(1)
                                    .and_then(|i| group.plugins().get(i))
                                    .ok_or_else(|| {
                                        anyhow::anyhow!("Invalid option number: {index}")
                                    })?;
                                Ok(plugin.name.clone())
                            })
                            .collect::<Result<Vec<_>>>()
                    };
                    names.and_then(|names| group.apply_names(&names))
                };
                match result {
                    Ok(()) => break,
                    Err(error) => writeln!(output, "Invalid selection: {error}")?,
                }
            }
        }
        cursor += 1;
    }
    for index in visible_steps(&candidate) {
        for group in &candidate[index].groups {
            group.validate()?;
        }
    }
    steps.clone_from_slice(&candidate);
    Ok(PromptOutcome::Configured)
}

impl ChoicesFile {
    /// Merge edited archives while preserving choices for other packages atomically.
    pub fn merge_into_guide(&self, guide: &mut crate::Guide) -> Result<()> {
        self.merge_into_remaining(guide, &[])
    }

    /// Merge editor choices only into unfinished components, preserving the exact
    /// completed definitions needed by recovery receipt verification.
    pub fn merge_into_remaining(
        &self,
        guide: &mut crate::Guide,
        completed: &[String],
    ) -> Result<()> {
        self.update_guide(guide, completed)
    }

    /// Per-archive merge shared by run-scoped application and editor merges. Each
    /// component (or option) receives only the entries for archives it registers or
    /// extracts itself; other components, including ones with unparseable saved
    /// choices, are left untouched.
    pub fn apply_to_remaining_with_roots(
        &self,
        guide: &mut crate::Guide,
        completed: &[String],
        roots: &crate::paths::Roots,
        case_insensitive: bool,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<()> {
        ensure!(
            !cancelled.load(std::sync::atomic::Ordering::Acquire),
            "Cancelled while resolving FOMOD choices"
        );
        // A completed component protects its entire subtree, so no descendant
        // can make an otherwise ineligible top-level component relevant here.
        let eligible = guide.components.iter().any(|component| {
            !completed
                .iter()
                .any(|id| id.eq_ignore_ascii_case(&component.guid))
        });
        if self.archives.is_empty() || !eligible {
            return Ok(());
        }
        let inventory = resolved_choices::Inventory::read(roots, case_insensitive, cancelled)?;
        self.update_guide_resolved(guide, completed, Some(&inventory))
    }

    fn update_guide(&self, guide: &mut crate::Guide, completed: &[String]) -> Result<()> {
        self.update_guide_resolved(guide, completed, None)
    }

    fn update_guide_resolved(
        &self,
        guide: &mut crate::Guide,
        completed: &[String],
        inventory: Option<&resolved_choices::Inventory<'_>>,
    ) -> Result<()> {
        fn update(
            components: &mut [crate::Component],
            edits: &ChoicesFile,
            completed: &[String],
            inventory: Option<&resolved_choices::Inventory<'_>>,
        ) -> Result<()> {
            for component in components {
                if completed
                    .iter()
                    .any(|id| id.eq_ignore_ascii_case(&component.guid))
                {
                    continue;
                }
                let mut relevant = Vec::new();
                for archive in &edits.archives {
                    let matches = match inventory {
                        Some(inventory) => {
                            inventory.references(component, &archive.archive_file_name)?
                        }
                        None => references_archive(component, &archive.archive_file_name),
                    };
                    if matches {
                        relevant.push(archive.clone());
                    }
                }
                if !relevant.is_empty() {
                    merge_relevant(component, &relevant)?;
                }
                update(&mut component.options, edits, completed, inventory)?;
            }
            Ok(())
        }
        fn merge_relevant(
            component: &mut crate::Component,
            edits: &[ArchiveChoices],
        ) -> Result<()> {
            let mut choices = saved_choices_file(component)?.unwrap_or(ChoicesFile {
                version: 1,
                archives: Vec::new(),
            });
            choices.archives.retain(|old| {
                !edits
                    .iter()
                    .any(|new| eq_ignore_case(&new.archive_file_name, &old.archive_file_name))
            });
            choices.archives.extend(edits.iter().cloned());
            component
                .metadata
                .insert("FomodChoices".into(), serde_json::to_value(choices)?);
            for archive in edits {
                try_set_prompt_status(
                    component,
                    &archive.archive_file_name,
                    PromptStatus::Configured,
                )
                .with_context(|| format!("Recording FOMOD prompt status for {}", component.guid))?;
            }
            Ok(())
        }
        let mut candidate = guide.clone();
        update(&mut candidate.components, self, completed, inventory)?;
        if let Some(inventory) = inventory {
            inventory.check_cancelled()?;
        }
        *guide = candidate;
        Ok(())
    }
}

/// Supported routes for missing, stale or C#-only FOMOD configuration.
pub const CHOICES_RECOVERY_HINT: &str = "Capture choices with `modsync fomod-inspect <archive> --interactive --save-choices <file>`, then persist them with `modsync fetch-downloads --record --fomod-choices <file>`.";

/// Route for saved `FomodChoices` that cannot be parsed. Fetch never overwrites them
/// (that could discard other archives' entries), so the guide must be edited first.
/// Repair comes first: removal drops every archive's entry for the component.
pub const CHOICES_REPAIR_HINT: &str = "Repair this component's FomodChoices in the guide (removing it discards every archive's saved choices for this component); `fetch-downloads --fomod-choices` does not overwrite unparseable saved choices. Afterwards, capture any missing choices with `modsync fomod-inspect <archive> --interactive --save-choices <file>` and record them with `modsync fetch-downloads --record --fomod-choices <file>`.";

/// The component's saved choices for one archive, or an actionable error.
/// Prompt status, including a C# `configured` marker, is never executable choices:
/// C# persists generated instructions and a status, not a choices representation.
pub fn required_saved_choices(
    component: &crate::Component,
    filename: &str,
) -> Result<ArchiveChoices> {
    let missing = |what: &str| match prompt_status(component, filename).as_deref() {
        Some("configured") => anyhow::anyhow!(
            "FOMOD archive '{filename}' is marked configured in handlerMetadata.fomodPromptStatus but has {what}. Prompt status is not executable choices; configuration recorded by the C# app (generated instructions plus status) is not imported. {CHOICES_RECOVERY_HINT}"
        ),
        Some(status) => anyhow::anyhow!(
            "FOMOD archive '{filename}' (prompt status '{status}') has {what}. {CHOICES_RECOVERY_HINT}"
        ),
        None => anyhow::anyhow!("FOMOD archive '{filename}' has {what}. {CHOICES_RECOVERY_HINT}"),
    };
    let file = saved_choices_file(component)?
        .ok_or_else(|| missing("no explicit FomodChoices configuration"))?;
    file.archive(filename)
        .cloned()
        .ok_or_else(|| missing("no saved FomodChoices entry for this archive"))
}

/// The component's saved `FomodChoices`, if any. Unparseable saved choices are an
/// error naming the repair route, with one wording for Fetch, the gate and install.
pub fn saved_choices_file(component: &crate::Component) -> Result<Option<ChoicesFile>> {
    let Some(raw) = component.metadata.get("FomodChoices") else {
        return Ok(None);
    };
    ChoicesFile::parse(&serde_json::to_string(raw)?)
        .with_context(|| {
            format!(
                "Saved FomodChoices of component {} cannot be parsed. {CHOICES_REPAIR_HINT}",
                component.guid
            )
        })
        .map(Some)
}

/// Context for choices that no longer fit the archive now on disk. Choices carry
/// only the archive filename (as in C#), so identity is the current installer
/// accepting every saved step/group/plugin name and mapping safely.
pub fn stale_choices_context(filename: &str) -> String {
    format!(
        "Saved FOMOD choices for '{filename}' do not apply to the current archive, which may have changed since they were saved (to recover: {})",
        CHOICES_RECOVERY_HINT.trim_end_matches('.')
    )
}

/// Apply choices to the current archive's installer and map them without writing.
/// Returns the normalized capture (authored names, every group) for comparison.
pub fn current_archive_choices(
    module: &ArchiveModule,
    choices: &ArchiveChoices,
    filename: &str,
) -> Result<ArchiveChoices> {
    let mut steps = module.config.steps.clone();
    choices.apply_to(&mut steps)?;
    let mapped = ArchiveModule {
        module_path: module.module_path.clone(),
        content_root: module.content_root.clone(),
        config: ModuleConfig {
            steps,
            ..module.config.clone()
        },
        info: None,
    };
    mapped.copy_instructions("fomod-extracted")?;
    Ok(ChoicesFile::capture(filename, &mapped.config.steps)?
        .archives
        .remove(0))
}

/// Find a saved archive session; refuse conflicting component configurations.
pub fn saved_archive_choices(
    guide: &crate::Guide,
    filename: &str,
) -> Result<Option<ArchiveChoices>> {
    saved_archive_choices_remaining(guide, filename, &[])
}

pub fn saved_archive_choices_remaining(
    guide: &crate::Guide,
    filename: &str,
    completed: &[String],
) -> Result<Option<ArchiveChoices>> {
    fn visit(
        components: &[crate::Component],
        filename: &str,
        found: &mut Option<ArchiveChoices>,
        completed: &[String],
    ) -> Result<()> {
        for component in components {
            if completed
                .iter()
                .any(|id| id.eq_ignore_ascii_case(&component.guid))
            {
                continue;
            }
            if let Some(value) = component.metadata.get("FomodChoices") {
                let file = ChoicesFile::parse(&serde_json::to_string(value)?)?;
                if let Some(choices) = file.archive(filename) {
                    if let Some(previous) = found {
                        ensure!(
                            serde_json::to_value(&previous.selections)?
                                == serde_json::to_value(&choices.selections)?,
                            "Conflicting saved FOMOD choices for {filename}"
                        );
                    } else {
                        *found = Some(choices.clone());
                    }
                }
            }
            visit(&component.options, filename, found, completed)?;
        }
        Ok(())
    }
    let mut found = None;
    visit(&guide.components, filename, &mut found, completed)?;
    Ok(found)
}

/// Record editor dismissal/configuration without altering completed definitions.
pub fn set_prompt_status_remaining(
    guide: &mut crate::Guide,
    filename: &str,
    status: PromptStatus,
    completed: &[String],
) {
    fn visit(
        components: &mut [crate::Component],
        filename: &str,
        status: PromptStatus,
        completed: &[String],
    ) {
        for component in components {
            if completed
                .iter()
                .any(|id| id.eq_ignore_ascii_case(&component.guid))
            {
                continue;
            }
            set_prompt_status(component, filename, status);
            visit(&mut component.options, filename, status, completed);
        }
    }
    visit(&mut guide.components, filename, status, completed);
}

/// Apply one validated archive session to exact captured acquisition consumers.
/// The caller owns GUID/path authorization; unlike the legacy broad merge this
/// never traverses options or enrolls another component by archive name. Completed
/// GUIDs are rejected. Cloning makes a multi-consumer edit all-or-nothing.
pub fn apply_bound_archive(
    guide: &mut crate::Guide,
    guids: &[String],
    filename: &str,
    choices: Option<&ArchiveChoices>,
    status: PromptStatus,
    completed: &[String],
) -> Result<()> {
    let mut candidate = guide.clone();
    for guid in guids {
        ensure!(
            !completed.iter().any(|id| id.eq_ignore_ascii_case(guid)),
            "FOMOD consumer {guid} is completed"
        );
        ensure!(
            candidate
                .components
                .iter()
                .filter(|c| c.guid == *guid)
                .count()
                == 1,
            "FOMOD consumer {guid} is missing or ambiguous"
        );
        let component = candidate
            .components
            .iter_mut()
            .find(|c| c.guid == *guid)
            .unwrap();
        if let Some(edit) = choices {
            ensure!(
                eq_ignore_case(&edit.archive_file_name, filename),
                "FOMOD choices archive does not match captured binding"
            );
            let mut saved = match component.metadata.get("FomodChoices") {
                Some(value) => ChoicesFile::parse(&serde_json::to_string(value)?)?,
                None => ChoicesFile {
                    version: 1,
                    archives: Vec::new(),
                },
            };
            saved
                .archives
                .retain(|old| !eq_ignore_case(&old.archive_file_name, filename));
            saved.archives.push(edit.clone());
            component
                .metadata
                .insert("FomodChoices".into(), serde_json::to_value(saved)?);
        }
        let filename = archive_name(filename);
        for resource in component
            .resource_registry
            .values_mut()
            .filter(|value| resource_matches(value, &filename))
        {
            write_prompt_status(resource, &filename, status)
                .with_context(|| format!("Recording FOMOD prompt status for {guid}"))?;
        }
    }
    *guide = candidate;
    Ok(())
}
