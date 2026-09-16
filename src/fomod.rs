//! Shared FOMOD selection rules. Archive configuration and installation are not yet wired.
use anyhow::{Result, ensure};
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
    /// Unsupported patterns fail instead of reproducing the reference's silent skips.
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
                _ => anyhow::bail!("FOMOD file pattern requires unsupported OR mapping"),
            };
            let mut flags = Vec::new();
            for condition in children {
                match condition {
                    FlagCondition::Flag { name, value } => flags.push((name, value)),
                    FlagCondition::Always => {}
                    _ => anyhow::bail!(
                        "FOMOD file pattern requires unsupported nested dependency mapping"
                    ),
                }
            }
            if flags.is_empty() {
                trailing.extend(&pattern.files);
            } else {
                let index = plugins
                    .iter()
                    .position(|(_, plugin)| {
                        flags.iter().all(|(name, value)| {
                            plugin
                                .flags
                                .iter()
                                .any(|(n, v)| n.eq_ignore_ascii_case(name) && v == *value)
                        })
                    })
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "FOMOD conditional flags are not supplied by a single plugin"
                        )
                    })?;
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
                        ensure!(
                            name.eq_ignore_ascii_case(&filename),
                            "FOMOD file rename mapping is not implemented: {} -> {}",
                            file.source,
                            file.destination
                        );
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

impl ChoicesFile {
    /// Override embedded choices recursively; the caller owns persistence of the guide.
    pub fn apply_to_guide(&self, guide: &mut crate::Guide) -> Result<()> {
        let value = serde_json::to_value(self)?;
        fn attach(components: &mut [crate::Component], value: &serde_json::Value) {
            for component in components {
                component
                    .metadata
                    .insert("FomodChoices".into(), value.clone());
                attach(&mut component.options, value);
            }
        }
        attach(&mut guide.components, &value);
        Ok(())
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
                let mut line = String::new();
                ensure!(
                    input.read_line(&mut line)? != 0,
                    "FOMOD selection cancelled at end of input"
                );
                let line = line.trim();
                ensure!(!line.eq_ignore_ascii_case("q"), "FOMOD selection cancelled");
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
    Ok(())
}

impl ChoicesFile {
    /// Merge edited archives while preserving choices for other packages atomically.
    pub fn merge_into_guide(&self, guide: &mut crate::Guide) -> Result<()> {
        fn merge(components: &mut [crate::Component], edits: &ChoicesFile) -> Result<()> {
            for component in components {
                let mut choices = match component.metadata.get("FomodChoices") {
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
                merge(&mut component.options, edits)?;
            }
            Ok(())
        }
        let mut candidate = guide.clone();
        merge(&mut candidate.components, self)?;
        *guide = candidate;
        Ok(())
    }
}

/// Find a saved archive session; refuse conflicting component configurations.
pub fn saved_archive_choices(
    guide: &crate::Guide,
    filename: &str,
) -> Result<Option<ArchiveChoices>> {
    fn visit(
        components: &[crate::Component],
        filename: &str,
        found: &mut Option<ArchiveChoices>,
    ) -> Result<()> {
        for component in components {
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
            visit(&component.options, filename, found)?;
        }
        Ok(())
    }
    let mut found = None;
    visit(&guide.components, filename, &mut found)?;
    Ok(found)
}
