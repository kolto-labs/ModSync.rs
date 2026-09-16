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
    pub required: bool,
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
