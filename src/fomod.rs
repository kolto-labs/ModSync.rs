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

#[derive(Clone, Debug)]
pub struct PluginSelection {
    pub name: String,
    pub description: String,
    /// Archive-relative authored path only; not a trusted filesystem path.
    pub image_path: String,
    pub files: Vec<FileInstall>,
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
    Flag { name: String, value: String },
    All(Vec<FlagCondition>),
    Any(Vec<FlagCondition>),
}

impl FlagCondition {
    fn matches(&self, flags: &std::collections::BTreeMap<String, String>) -> bool {
        match self {
            Self::Flag { name, value } => flags.get(&name.to_ascii_lowercase()) == Some(value),
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
pub use xml::{parse_module_xml, parse_selection_xml};

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
                FlagCondition::Flag { .. } => vec![&pattern.condition],
                FlagCondition::All(children) => children.iter().collect(),
                FlagCondition::Any(children) if children.len() <= 1 => children.iter().collect(),
                _ => anyhow::bail!("FOMOD file pattern requires unsupported OR mapping"),
            };
            let mut flags = Vec::new();
            for condition in children {
                let FlagCondition::Flag { name, value } = condition else {
                    anyhow::bail!(
                        "FOMOD file pattern requires unsupported nested dependency mapping"
                    );
                };
                flags.push((name, value));
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
