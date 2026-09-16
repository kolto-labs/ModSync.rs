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
