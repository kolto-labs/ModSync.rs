use crate::{Component, Instruction};
use anyhow::{Result, bail};

mod browser;
pub mod easy;
mod import;
pub use browser::{ResourceInventory, ResourceItem, resource_inventory, variable_path};
mod regex_import;
mod transaction;
pub use import::{ImportMode, ImportPreview, apply_preview, preview_import};
pub use regex_import::{
    MatchTrace, RegexFlags, RegexImportPreview, RegexImportProfile, preview_regex_import,
};
pub use transaction::{EditError, EditOperation, apply_operations, validate_guide};

/// Read a bounded JSON operation list without following a final symlink or
/// blocking on a device/FIFO. This performs no edits or writes.
pub fn load_operations(path: &std::path::Path) -> Result<Vec<EditOperation>> {
    Ok(serde_json::from_str(&read_document(path)?)?)
}

pub fn read_document(path: &std::path::Path) -> Result<String> {
    use std::io::Read;
    const LIMIT: u64 = 8 * 1024 * 1024;
    let mut bytes = Vec::new();
    crate::paths::open_regular_read(path)?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() as u64 <= LIMIT, "Editor document exceeds 8 MiB");
    Ok(String::from_utf8(bytes)?)
}

/// Authored editor/merge input: bounded regular UTF-8 file and no implicit selection.
pub fn read_guide(path: &std::path::Path) -> Result<crate::Guide> {
    crate::Guide::parse_preserving_selection(&read_document(path)?, "auto")
}

/// Authoring includes named setup and tail mods omitted by reference import.
/// The reference importer and its main-list GUIDs remain unchanged.
pub fn read_authoring_guide(path: &std::path::Path) -> Result<crate::Guide> {
    let text = read_document(path)?;
    let mut guide = crate::Guide::parse_preserving_selection(&text, "auto")?;
    let intro = guide
        .metadata
        .get("preambleContent")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    // Reference import advances its epilogue past optional widescreen mods.
    // Recover that original Markdown section rather than inventing its contents.
    let mut offset = 0;
    let mut widescreen = None;
    let mut comment = false;
    let mut fence: Option<(char, usize)> = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if !comment
            && fence.is_none()
            && line
                .to_ascii_lowercase()
                .starts_with("## optional widescreen")
        {
            widescreen = Some(text[offset..].to_owned());
            break;
        }
        if !comment && let Some(ch @ ('`' | '~')) = trimmed.chars().next() {
            let count = trimmed.chars().take_while(|c| *c == ch).count();
            if let Some((open, width)) = fence {
                if ch == open && count >= width && trimmed[count..].trim().is_empty() {
                    fence = None;
                }
            } else if count >= 3 {
                fence = Some((ch, count));
            }
        }
        if fence.is_none() {
            if line.contains("<!--") {
                comment = true;
            }
            if line.contains("-->") {
                comment = false;
            }
        }
        offset += line.len();
    }
    let tail = widescreen.or_else(|| {
        guide
            .metadata
            .get("epilogueContent")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    });
    for (notes, marker, prepend) in [(intro, "GuideSetup", true), (tail, "GuideTail", false)] {
        let Some(notes) = notes.filter(|notes| notes.contains("**Name:")) else {
            continue;
        };
        let parsed = crate::Guide::parse_preserving_selection(&notes, "md")?;
        let context = notes.split("\n### ").next().unwrap_or("").trim();
        let mut components = Vec::new();
        for mut component in parsed.components {
            // Troubleshooting headings are notes, not mod recipes.
            if !component.metadata.contains_key("NameFieldContent") {
                continue;
            }
            if let Some(existing) = find_component(&guide, &component.guid) {
                anyhow::ensure!(
                    existing
                        .metadata
                        .get(marker)
                        .and_then(serde_json::Value::as_bool)
                        == Some(true),
                    "Outside-list mod identity duplicates a main-list mod"
                );
                continue;
            }
            component.metadata.insert(marker.into(), true.into());
            component
                .metadata
                .insert("GuideSectionContext".into(), context.into());
            if !prepend && context.to_ascii_lowercase().contains("widescreen") {
                component
                    .metadata
                    .insert("WidescreenOnly".into(), true.into());
            }
            components.push(component);
        }
        if prepend {
            components.append(&mut guide.components);
            guide.components = components;
        } else {
            guide.components.extend(components);
        }
    }
    let operations = reconcile_authoring_masters(&guide)?;
    Ok(apply_operations(&guide, &operations)?)
}

/// Resolve existing master text using the same lookup as reference Markdown import.
/// Only matched references change; conditions and unknown references remain explicit.
pub fn reconcile_authoring_masters(guide: &crate::Guide) -> Result<Vec<EditOperation>> {
    let resolver = crate::markdown::MasterResolver::new(guide);
    let mut operations = Vec::new();
    for original in &guide.components {
        let Some(masters) = original
            .metadata
            .get("UnresolvedMasters")
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        let mut component = original.clone();
        let mut unresolved = Vec::new();
        let mut changed = false;
        for master in masters {
            let name = master
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Master reference must be text"))?;
            if let Some(id) = resolver.resolve(name) {
                anyhow::ensure!(
                    !id.eq_ignore_ascii_case(&component.guid),
                    "A mod cannot require itself"
                );
                if !component
                    .dependencies
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(&id))
                {
                    component.dependencies.push(id);
                }
                let names = component
                    .metadata
                    .entry("DependencyNames".into())
                    .or_insert(serde_json::json!([]))
                    .as_array_mut()
                    .ok_or_else(|| anyhow::anyhow!("Dependency names must be a list"))?;
                if !names.contains(master) {
                    names.push(master.clone());
                }
                changed = true;
            } else {
                unresolved.push(master.clone());
            }
        }
        if changed {
            if unresolved.is_empty() {
                component.metadata.remove("UnresolvedMasters");
            } else {
                component
                    .metadata
                    .insert("UnresolvedMasters".into(), unresolved.into());
            }
            operations.push(EditOperation::ReplaceComponent {
                guid: original.guid.clone(),
                component: Box::new(component),
            });
        }
    }
    // Reject any newly formed cycle or invalid identity before a durable edit.
    apply_operations(guide, &operations)?;
    Ok(operations)
}

/// Match the input reader's limit before publishing a saved preview document.
pub fn render_preview<T: serde::Serialize>(preview: &T) -> Result<String> {
    let text = serde_json::to_string_pretty(preview)?;
    anyhow::ensure!(
        text.len() < 8 * 1024 * 1024,
        "Editor preview exceeds the 8 MiB apply-document limit"
    );
    Ok(text)
}

pub fn find_component<'a>(guide: &'a crate::Guide, guid: &str) -> Option<&'a Component> {
    fn find<'a>(items: &'a [Component], guid: &str) -> Option<&'a Component> {
        items.iter().find_map(|component| {
            if component.guid.eq_ignore_ascii_case(guid) {
                Some(component)
            } else {
                find(&component.options, guid)
            }
        })
    }
    find(&guide.components, guid)
}

/// Validated save for a newly authored guide without a source file.
pub fn save_validated(guide: &crate::Guide, output: &std::path::Path) -> Result<()> {
    use anyhow::ensure;
    validate_guide(guide)?;
    match std::fs::symlink_metadata(output) {
        Ok(metadata) => ensure!(
            metadata.file_type().is_file(),
            "Editor output must be a regular file or absent"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    guide.save(output)
}

/// Explicit save boundary shared by scriptable editing/import adapters.
pub fn save_distinct(
    guide: &crate::Guide,
    input: &std::path::Path,
    output: &std::path::Path,
) -> Result<()> {
    validate_guide(guide)?;
    let output = distinct_output_path(input, output)?;
    guide.save(&output)
}

/// Resolve a separate ordinary output while retaining the source file boundary.
pub fn distinct_output_path(
    input: &std::path::Path,
    output: &std::path::Path,
) -> Result<std::path::PathBuf> {
    use anyhow::{Context, ensure};
    let input = input.canonicalize().context("Resolve editor input")?;
    let output = std::path::absolute(output)?;
    let parent = output
        .parent()
        .context("Editor output needs a parent directory")?
        .canonicalize()?;
    let output = parent.join(
        output
            .file_name()
            .context("Editor output needs a filename")?,
    );
    ensure!(
        input != output,
        "Guide edits require a distinct output path"
    );
    match std::fs::symlink_metadata(&output) {
        Ok(metadata) => {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "Editor output must be a regular file or absent"
            );
            ensure!(
                output.canonicalize()? != input,
                "Guide edits cannot overwrite the input through an alias"
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    Ok(output)
}

pub fn add_component(components: &mut Vec<Component>) -> usize {
    let mut number = components.len() + 1;
    let guid = loop {
        let candidate = format!("component-{number}");
        if components
            .iter()
            .all(|component| !component.guid.eq_ignore_ascii_case(&candidate))
        {
            break candidate;
        }
        number += 1;
    };
    components.push(Component {
        guid: guid.clone(),
        name: guid,
        ..Default::default()
    });
    components.len() - 1
}

pub fn move_component(components: &mut [Component], guid: &str, offset: isize) -> Result<()> {
    let index = components
        .iter()
        .position(|component| component.guid.eq_ignore_ascii_case(guid))
        .ok_or_else(|| anyhow::anyhow!("Unknown component GUID: {guid}"))?;
    let destination = index
        .checked_add_signed(offset)
        .filter(|index| *index < components.len())
        .ok_or_else(|| anyhow::anyhow!("Component move is out of range"))?;
    components.swap(index, destination);
    Ok(())
}

pub fn remove_component(components: &mut Vec<Component>, guid: &str) -> Result<Component> {
    let index = components
        .iter()
        .position(|component| component.guid.eq_ignore_ascii_case(guid))
        .ok_or_else(|| anyhow::anyhow!("Unknown component GUID: {guid}"))?;
    if components.iter().any(|component| {
        component
            .dependencies
            .iter()
            .any(|dependency| dependency.eq_ignore_ascii_case(guid))
    }) {
        bail!("Cannot remove a component required by another component");
    }
    Ok(components.remove(index))
}

pub fn move_instruction(component: &mut Component, index: usize, offset: isize) -> Result<()> {
    let destination = index
        .checked_add_signed(offset)
        .filter(|index| *index < component.instructions.len())
        .ok_or_else(|| anyhow::anyhow!("Instruction move is out of range"))?;
    component.instructions.swap(index, destination);
    Ok(())
}

pub fn add_instruction(component: &mut Component, index: usize) -> Result<()> {
    if index > component.instructions.len() {
        bail!("Instruction insert is out of range");
    }
    component.instructions.insert(
        index,
        Instruction {
            guid: String::new(),
            parent: String::new(),
            action: "Unset".into(),
            source: vec![],
            destination: String::new(),
            arguments: String::new(),
            overwrite: true,
            dependencies: vec![],
            restrictions: vec![],
        },
    );
    Ok(())
}

pub fn remove_instruction(component: &mut Component, index: usize) -> Result<Instruction> {
    if index >= component.instructions.len() {
        bail!("Instruction removal is out of range");
    }
    Ok(component.instructions.remove(index))
}
