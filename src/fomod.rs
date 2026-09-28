//! Shared FOMOD selection rules. Archive configuration and installation are not yet wired.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

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
                archives.insert(archive.archive_file_name.to_ascii_lowercase()),
                "Duplicate FOMOD archive '{}'",
                archive.archive_file_name
            );
            let mut groups = std::collections::BTreeSet::new();
            for selection in &archive.selections {
                ensure!(
                    groups.insert((
                        selection.step_name.to_ascii_lowercase(),
                        selection.group_name.to_ascii_lowercase()
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
            .find(|a| a.archive_file_name.eq_ignore_ascii_case(filename))
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
                .filter(|(_, p)| p.name.eq_ignore_ascii_case(name))
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
                    selection.step_name.to_ascii_lowercase(),
                    selection.group_name.to_ascii_lowercase()
                )),
                "Duplicate FOMOD selection for '{}' / '{}'",
                selection.step_name,
                selection.group_name
            );
            let mut matches = candidate
                .iter_mut()
                .filter(|s| s.name.eq_ignore_ascii_case(&selection.step_name));
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
                .filter(|g| g.name.eq_ignore_ascii_case(&selection.group_name));
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
    /// otherwise usable packages.
    Always,
    All(Vec<FlagCondition>),
    Any(Vec<FlagCondition>),
}

impl FlagCondition {
    fn matches(&self, flags: &std::collections::BTreeMap<String, String>) -> bool {
        match self {
            Self::Flag { name, value } => flags.get(&name.to_ascii_lowercase()) == Some(value),
            Self::Always => true,
            Self::All(children) => children.iter().all(|c| c.matches(flags)),
            // C# treats every empty composite as true, including Or.
            Self::Any(children) => children.is_empty() || children.iter().any(|c| c.matches(flags)),
        }
    }
}

/// Reference order: all selected plugins contribute, including those in hidden steps.
/// Later flag assignments win; names ignore ASCII case and values are case-sensitive.
pub fn visible_steps(steps: &[StepSelection]) -> Vec<usize> {
    let mut flags = std::collections::BTreeMap::new();
    for step in steps {
        for group in &step.groups {
            for plugin in group.plugins().iter().filter(|p| p.selected) {
                for (name, value) in &plugin.flags {
                    flags.insert(name.to_ascii_lowercase(), value.clone());
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

mod xml;
pub use xml::{parse_info_xml, parse_module_xml, parse_selection_xml};

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

impl ModuleConfig {
    /// Build the reference mapper's file order without resolving paths or writing files.
    /// Conditional patterns that the reference cannot attach to one plugin are skipped.
    pub fn selected_files(&self) -> Result<Vec<&FileInstall>> {
        let visible = visible_steps(&self.steps);
        let plugins: Vec<_> = self
            .steps
            .iter()
            .enumerate()
            .flat_map(|(i, step)| {
                step.groups
                    .iter()
                    .flat_map(move |group| group.plugins().iter().map(move |p| (i, p)))
            })
            .collect();
        let mut attached: std::collections::BTreeMap<usize, Vec<&FileInstall>> =
            std::collections::BTreeMap::new();
        let mut trailing = Vec::new();
        for pattern in &self.conditional_files {
            let children: Vec<_> = match &pattern.condition {
                FlagCondition::Flag { .. } | FlagCondition::Always => vec![&pattern.condition],
                FlagCondition::All(children) => children.iter().collect(),
                FlagCondition::Any(children) if children.len() <= 1 => children.iter().collect(),
                _ => continue,
            };
            let mut flags = Vec::new();
            let mut supported = true;
            for condition in children {
                match condition {
                    FlagCondition::Flag { name, value } => flags.push((name, value)),
                    FlagCondition::Always => {}
                    _ => supported = false,
                }
            }
            if !supported {
                continue;
            }
            if flags.is_empty() {
                trailing.extend(&pattern.files);
            } else {
                let index = plugins.iter().position(|(_, plugin)| {
                    flags.iter().all(|(name, value)| {
                        plugin
                            .flags
                            .iter()
                            .any(|(n, v)| n.eq_ignore_ascii_case(name) && v == *value)
                    })
                });
                let Some(index) = index else {
                    continue;
                };
                attached.entry(index).or_default().extend(&pattern.files);
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
                    if name.contains('.') || name.eq_ignore_ascii_case(&filename) {
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
    inspect_entries(&entries)
}

pub(crate) fn inspect_entries(entries: &[crate::archive::Entry]) -> Result<Option<ArchiveModule>> {
    let mut modules = entries
        .iter()
        .filter(|e| is_module_config_path(&e.path.to_string_lossy()));
    let Some(module) = modules.next() else {
        return Ok(None);
    };
    ensure!(
        modules.next().is_none(),
        "Archive contains multiple FOMOD installers"
    );
    let directory = module.path.parent().unwrap();
    let info_path = directory
        .join("info.xml")
        .to_string_lossy()
        .to_ascii_lowercase();
    let info = entries
        .iter()
        .find(|e| e.path.to_string_lossy().to_ascii_lowercase() == info_path)
        .map(|e| -> Result<FomodInfo> { parse_info_xml(&std::fs::read_to_string(e.file.path())?) })
        .transpose()?;
    Ok(Some(ArchiveModule {
        module_path: module.path.clone(),
        content_root: directory.parent().unwrap().to_path_buf(),
        config: parse_module_xml(&std::fs::read_to_string(module.file.path())?)?,
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
    files
        .keys()
        .any(|name| archive_name(name) == filename || name.trim().eq_ignore_ascii_case(filename))
}

/// Whether this component registers the archive under a resource-registry Files key.
pub fn resource_matches_archive(component: &crate::Component, archive: &str) -> bool {
    let filename = archive_name(archive);
    component
        .resource_registry
        .values()
        .any(|resource| resource_matches(resource, &filename))
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
        .find(|(name, _)| archive_name(name) == filename)
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
    statuses
        .as_object_mut()
        .context("FOMOD prompt status must be an object")?
        .insert(
            filename.to_owned(),
            serde_json::Value::String(status.as_str().into()),
        );
    Ok(())
}

impl ChoicesFile {
    /// Override embedded choices recursively; the caller owns persistence of the guide.
    pub fn apply_to_guide(&self, guide: &mut crate::Guide) -> Result<()> {
        self.apply_to_remaining(guide, &[])
    }

    /// Apply external choices without changing completed component definitions or
    /// their nested options. The engine still verifies the completed prefix and
    /// its receipt before any remaining-phase operation.
    pub fn apply_to_remaining(&self, guide: &mut crate::Guide, completed: &[String]) -> Result<()> {
        self.update_guide(guide, completed, false)
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
        self.update_guide(guide, completed, true)
    }

    fn update_guide(
        &self,
        guide: &mut crate::Guide,
        completed: &[String],
        merge: bool,
    ) -> Result<()> {
        fn update(
            components: &mut [crate::Component],
            edits: &ChoicesFile,
            completed: &[String],
            merge: bool,
        ) -> Result<()> {
            for component in components {
                if completed
                    .iter()
                    .any(|id| id.eq_ignore_ascii_case(&component.guid))
                {
                    continue;
                }
                let mut choices = match component.metadata.get("FomodChoices").filter(|_| merge) {
                    Some(value) => ChoicesFile::parse(&serde_json::to_string(value)?)?,
                    None => ChoicesFile {
                        version: 1,
                        archives: Vec::new(),
                    },
                };
                choices
                    .archives
                    .retain(|old| edits.archive(&old.archive_file_name).is_none());
                choices.archives.extend(edits.archives.clone());
                component
                    .metadata
                    .insert("FomodChoices".into(), serde_json::to_value(choices)?);
                for archive in &edits.archives {
                    try_set_prompt_status(
                        component,
                        &archive.archive_file_name,
                        PromptStatus::Configured,
                    )
                    .with_context(|| {
                        format!("Recording FOMOD prompt status for {}", component.guid)
                    })?;
                }
                update(&mut component.options, edits, completed, merge)?;
            }
            Ok(())
        }
        let mut candidate = guide.clone();
        update(&mut candidate.components, self, completed, merge)?;
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
                edit.archive_file_name.eq_ignore_ascii_case(filename),
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
                .retain(|old| !old.archive_file_name.eq_ignore_ascii_case(filename));
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
