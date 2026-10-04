//! Pure, atomic guide edits shared by desktop and command-line adapters.
use crate::{Component, Guide, Instruction};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Positions refer to the guide after all preceding operations. `parent: None`
/// addresses the top-level list; a parent GUID addresses that component's options.
/// Replacement includes every authored field, including extension metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase", deny_unknown_fields)]
pub enum EditOperation {
    ResolveDraft {
        component: String,
        resolution: crate::draft::review::Resolution,
    },
    InsertComponent {
        parent: Option<String>,
        index: usize,
        component: Box<Component>,
    },
    ReplaceComponent {
        guid: String,
        component: Box<Component>,
    },
    RemoveComponent {
        guid: String,
    },
    MoveComponent {
        guid: String,
        index: usize,
    },
    InsertInstruction {
        component: String,
        index: usize,
        instruction: Instruction,
    },
    ReplaceInstruction {
        component: String,
        index: usize,
        instruction: Instruction,
    },
    RemoveInstruction {
        component: String,
        index: usize,
    },
    MoveInstruction {
        component: String,
        index: usize,
        destination: usize,
    },
    SetResource {
        component: String,
        url: String,
        value: serde_json::Value,
    },
    RemoveResource {
        component: String,
        url: String,
    },
    SetGuideMetadata {
        key: String,
        value: serde_json::Value,
    },
    RemoveGuideMetadata {
        key: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditError {
    /// Zero-based failing operation, or None for final whole-guide validation.
    pub operation_index: Option<usize>,
    pub message: String,
}
impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.operation_index {
            Some(index) => write!(f, "Edit operation {index}: {}", self.message),
            None => write!(f, "Edited guide is invalid: {}", self.message),
        }
    }
}
impl std::error::Error for EditError {}

/// Mutates only a private clone. Cross-references may be temporarily incomplete
/// within a batch (for example while renaming an identity and its references).
pub fn apply_operations(
    guide: &Guide,
    operations: &[EditOperation],
) -> std::result::Result<Guide, EditError> {
    identities(guide).map_err(|error| EditError {
        operation_index: None,
        message: format!("Input identities: {error:#}"),
    })?;
    let mut edited = guide.clone();
    for (index, operation) in operations.iter().enumerate() {
        let before = edited.clone();
        apply(&mut edited, operation).map_err(|error| EditError {
            operation_index: Some(index),
            message: format!("{error:#}"),
        })?;
        let explicitly_resolved = match operation {
            EditOperation::ResolveDraft { component, .. } => Some(component.as_str()),
            _ => None,
        };
        invalidate_reviews(&before, guide, &mut edited.components, explicitly_resolved).map_err(
            |error| EditError {
                operation_index: Some(index),
                message: format!("Draft review invalidation: {error:#}"),
            },
        )?;
        // Every following GUID lookup must remain unambiguous.
        identities(&edited).map_err(|error| EditError {
            operation_index: Some(index),
            message: format!("{error:#}"),
        })?;
    }
    validate_guide(&edited).map_err(|error| EditError {
        operation_index: None,
        message: format!("{error:#}"),
    })?;
    Ok(edited)
}

fn invalidate_reviews(
    before: &Guide,
    original: &Guide,
    items: &mut [Component],
    resolved: Option<&str>,
) -> Result<()> {
    for item in items {
        if !resolved.is_some_and(|guid| guid.eq_ignore_ascii_case(&item.guid))
            && let Some(old) = super::find_component(before, &item.guid)
                .filter(|old| old.metadata.contains_key("DraftReview"))
                .or_else(|| super::find_component(original, &item.guid))
        {
            crate::draft::review::invalidate_changed(old, item)?;
        }
        invalidate_reviews(before, original, &mut item.options, resolved)?;
    }
    Ok(())
}

fn component_mut<'a>(components: &'a mut [Component], guid: &str) -> Result<&'a mut Component> {
    for component in components {
        if component.guid.eq_ignore_ascii_case(guid) {
            return Ok(component);
        }
        if let Ok(found) = component_mut(&mut component.options, guid) {
            return Ok(found);
        }
    }
    anyhow::bail!("Unknown component GUID: {guid}")
}
fn siblings_mut<'a>(
    components: &'a mut Vec<Component>,
    guid: &str,
) -> Result<(&'a mut Vec<Component>, usize)> {
    if let Some(index) = components
        .iter()
        .position(|c| c.guid.eq_ignore_ascii_case(guid))
    {
        return Ok((components, index));
    }
    for component in components {
        if let Ok(found) = siblings_mut(&mut component.options, guid) {
            return Ok(found);
        }
    }
    anyhow::bail!("Unknown component GUID: {guid}")
}
fn move_item<T>(items: &mut Vec<T>, index: usize, destination: usize) -> Result<()> {
    ensure!(
        index < items.len() && destination < items.len(),
        "Move is out of range"
    );
    let value = items.remove(index);
    items.insert(destination, value);
    Ok(())
}
fn apply(guide: &mut Guide, operation: &EditOperation) -> Result<()> {
    match operation {
        EditOperation::ResolveDraft {
            component,
            resolution,
        } => {
            crate::draft::review::resolve(
                component_mut(&mut guide.components, component)?,
                resolution,
            )?;
        }
        EditOperation::InsertComponent {
            parent,
            index,
            component,
        } => {
            let items = if let Some(parent) = parent {
                &mut component_mut(&mut guide.components, parent)?.options
            } else {
                &mut guide.components
            };
            ensure!(*index <= items.len(), "Component insertion is out of range");
            items.insert(*index, component.as_ref().clone());
        }
        EditOperation::ReplaceComponent { guid, component } => {
            let existing = component_mut(&mut guide.components, guid)?;
            ensure!(
                !existing.metadata.contains_key("DraftReview")
                    || existing.guid.eq_ignore_ascii_case(&component.guid),
                "Generated draft identity cannot be changed by replacement; retain its provenance"
            );
            fn preserve_generated_subtree(old: &Component, replacement: &Component) -> Result<()> {
                fn contains(root: &Component, guid: &str) -> bool {
                    root.guid.eq_ignore_ascii_case(guid)
                        || root.options.iter().any(|child| contains(child, guid))
                }
                if old.metadata.contains_key("DraftReview") {
                    ensure!(
                        contains(replacement, &old.guid),
                        "Replacement cannot discard a generated child identity; use an explicit remove operation"
                    );
                }
                for child in &old.options {
                    preserve_generated_subtree(child, replacement)?;
                }
                Ok(())
            }
            preserve_generated_subtree(existing, component)?;
            *existing = component.as_ref().clone();
        }
        EditOperation::RemoveComponent { guid } => {
            let (items, index) = siblings_mut(&mut guide.components, guid)?;
            items.remove(index);
        }
        EditOperation::MoveComponent { guid, index } => {
            let (items, old) = siblings_mut(&mut guide.components, guid)?;
            move_item(items, old, *index)?;
        }
        EditOperation::InsertInstruction {
            component,
            index,
            instruction,
        } => {
            let items = &mut component_mut(&mut guide.components, component)?.instructions;
            ensure!(
                *index <= items.len(),
                "Instruction insertion is out of range"
            );
            items.insert(*index, instruction.clone());
        }
        EditOperation::ReplaceInstruction {
            component,
            index,
            instruction,
        } => {
            let items = &mut component_mut(&mut guide.components, component)?.instructions;
            ensure!(
                *index < items.len(),
                "Instruction replacement is out of range"
            );
            items[*index] = instruction.clone();
        }
        EditOperation::RemoveInstruction { component, index } => {
            let items = &mut component_mut(&mut guide.components, component)?.instructions;
            ensure!(*index < items.len(), "Instruction removal is out of range");
            items.remove(*index);
        }
        EditOperation::MoveInstruction {
            component,
            index,
            destination,
        } => {
            move_item(
                &mut component_mut(&mut guide.components, component)?.instructions,
                *index,
                *destination,
            )?;
        }
        EditOperation::SetResource {
            component,
            url,
            value,
        } => {
            ensure!(!url.trim().is_empty(), "Resource URL cannot be empty");
            component_mut(&mut guide.components, component)?
                .resource_registry
                .insert(url.clone(), value.clone());
        }
        EditOperation::RemoveResource { component, url } => {
            ensure!(
                component_mut(&mut guide.components, component)?
                    .resource_registry
                    .remove(url)
                    .is_some(),
                "Unknown resource URL: {url}"
            );
        }
        EditOperation::SetGuideMetadata { key, value } => {
            ensure!(
                !["thismod", "components"].contains(&key.to_ascii_lowercase().as_str()),
                "Reserved guide field: {key}"
            );
            guide.metadata.insert(key.clone(), value.clone());
        }
        EditOperation::RemoveGuideMetadata { key } => {
            ensure!(
                guide.metadata.remove(key).is_some(),
                "Unknown guide metadata field: {key}"
            );
        }
    }
    Ok(())
}

fn identities(guide: &Guide) -> Result<BTreeMap<String, &Component>> {
    fn collect<'a>(
        items: &'a [Component],
        result: &mut BTreeMap<String, &'a Component>,
        depth: usize,
    ) -> Result<()> {
        ensure!(depth <= 64, "Option nesting exceeds 64 levels");
        for component in items {
            ensure!(
                !component.guid.trim().is_empty(),
                "Every component and option needs a Guid"
            );
            ensure!(
                result
                    .insert(component.guid.to_ascii_lowercase(), component)
                    .is_none(),
                "Duplicate component GUID: {}",
                component.guid
            );
            collect(&component.options, result, depth + 1)?;
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    collect(&guide.components, &mut result, 0)?;
    Ok(result)
}

/// Authoring validation is independent of the current selection. Unselected
/// definitions may not hide dangling references or dependency/order cycles.
/// Unset instructions are allowed while authoring; install preflight remains
/// responsible for executable safety and draft-review requirements.
pub fn validate_guide(guide: &Guide) -> Result<()> {
    ensure!(!guide.components.is_empty(), "The guide has no components");
    crate::draft::validate_review_records(&serde_json::to_value(guide)?)?;
    let identities = identities(guide)?;
    let mut predecessors: BTreeMap<String, BTreeSet<String>> = identities
        .keys()
        .map(|id| (id.clone(), BTreeSet::new()))
        .collect();
    for (id, component) in &identities {
        ensure!(
            !component.name.trim().is_empty(),
            "Component {} needs a Name",
            component.guid
        );
        for key in component.metadata.keys() {
            ensure!(
                ![
                    "guid",
                    "name",
                    "author",
                    "description",
                    "directions",
                    "tier",
                    "category",
                    "isselected",
                    "dependencies",
                    "restrictions",
                    "installbefore",
                    "installafter",
                    "instructions",
                    "options",
                    "resourceregistry"
                ]
                .contains(&key.to_ascii_lowercase().as_str()),
                "Extension metadata shadows component field {key}"
            );
        }
        for reference in component
            .dependencies
            .iter()
            .chain(&component.restrictions)
            .chain(&component.install_before)
            .chain(&component.install_after)
        {
            ensure!(
                identities.contains_key(&reference.to_ascii_lowercase()),
                "{} references missing component {reference}",
                component.guid
            );
            ensure!(
                !id.eq_ignore_ascii_case(reference),
                "{} references itself",
                component.guid
            );
        }
        for reference in component
            .dependencies
            .iter()
            .chain(&component.install_after)
        {
            predecessors
                .get_mut(id)
                .unwrap()
                .insert(reference.to_ascii_lowercase());
        }
        for reference in &component.install_before {
            predecessors
                .get_mut(&reference.to_ascii_lowercase())
                .unwrap()
                .insert(id.clone());
        }
        for instruction in &component.instructions {
            for reference in instruction
                .dependencies
                .iter()
                .chain(&instruction.restrictions)
            {
                ensure!(
                    identities.contains_key(&reference.to_ascii_lowercase()),
                    "Instruction in {} references missing component {reference}",
                    component.guid
                );
            }
            if instruction.action.eq_ignore_ascii_case("Choose") {
                for reference in &instruction.source {
                    ensure!(
                        component
                            .options
                            .iter()
                            .any(|option| crate::guid::names_option(&option.guid, reference)),
                        "Choose in {} references missing direct option {reference}",
                        component.guid
                    );
                }
            }
        }
    }
    let mut done = BTreeSet::new();
    while done.len() < predecessors.len() {
        let next = predecessors
            .iter()
            .find(|(id, required)| !done.contains(*id) && required.is_subset(&done))
            .map(|(id, _)| id.clone());
        let Some(next) = next else {
            anyhow::bail!("Dependency/order cycle in guide definitions");
        };
        done.insert(next);
    }
    Ok(())
}
