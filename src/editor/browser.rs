//! Read-only resource browsing. No extraction, external processes or guide edits.
use crate::{Component, archive, paths::Roots};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceItem {
    pub url: String,
    pub filename: String,
    pub source: String,
    pub enabled: bool,
    /// Archive names use the shared inventory's lowercase normalized spelling.
    /// They are relative to an extraction destination, not to the archive file.
    pub archive_entries: Vec<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceInventory {
    pub component_guid: String,
    pub resources: Vec<ResourceItem>,
}

/// Project an existing or proposed path under an explicit root. Reject links in
/// existing ancestors and traversal even when the final destination is absent.
pub fn variable_path(roots: &Roots, path: &Path) -> Result<String> {
    let path = std::path::absolute(path)?;
    roots.check(&path)?;
    let (prefix, root) = if path.starts_with(&roots.mods) {
        ("<<modDirectory>>", &roots.mods)
    } else {
        ("<<kotorDirectory>>", &roots.game)
    };
    let relative = path
        .strip_prefix(root)?
        .to_str()
        .context("Resource path is not UTF-8")?
        .replace('\\', "/");
    crate::paths::relative(&relative, false)?;
    Ok(if relative.is_empty() {
        prefix.into()
    } else {
        format!("{prefix}/{relative}")
    })
}

/// Exact registered filenames only: no broad filesystem search or inferred
/// basename match. Per-resource failures remain visible; cancellation is fatal.
pub fn resource_inventory(
    component: &Component,
    roots: &Roots,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<ResourceInventory> {
    let mut resources = Vec::new();
    let mut seen = BTreeSet::new();
    for (url, metadata) in &component.resource_registry {
        let files = metadata
            .get("Files")
            .and_then(serde_json::Value::as_object)
            .context("Resource registration needs a Files object")?;
        for (filename, enabled) in files {
            ensure!(
                !cancelled.load(Ordering::Relaxed),
                "Resource browsing cancelled"
            );
            ensure!(
                resources.len() < 10_000,
                "Resource inventory exceeds 10000 files"
            );
            let enabled = enabled
                .as_bool()
                .context("Resource file selection must be boolean")?;
            let mut item = ResourceItem {
                url: url.clone(),
                filename: filename.clone(),
                source: String::new(),
                enabled,
                archive_entries: Vec::new(),
                error: None,
            };
            let inspect = || -> Result<(String, Vec<String>)> {
                let relative = crate::paths::relative(filename, false)?;
                ensure!(!relative.as_os_str().is_empty(), "Empty resource filename");
                let path = roots.mods.join(relative);
                let source = variable_path(roots, &path)?;
                // The shared opener also rejects devices and directories.
                let _file = crate::paths::open_regular_read(&path)?;
                let extension = path
                    .extension()
                    .and_then(|part| part.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let entries = if ["zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "xz", "exe"]
                    .contains(&extension.as_str())
                {
                    archive::list(&path, &path, options, cancelled)?
                        .into_iter()
                        .collect()
                } else {
                    Vec::new()
                };
                Ok((source, entries))
            };
            match inspect() {
                Ok((source, entries)) => {
                    item.source = source;
                    item.archive_entries = entries;
                }
                Err(error) => item.error = Some(format!("{error:#}")),
            }
            ensure!(
                !cancelled.load(Ordering::Relaxed),
                "Resource browsing cancelled"
            );
            ensure!(
                seen.insert((url.clone(), filename.clone())),
                "Duplicate resource registration"
            );
            resources.push(item);
        }
    }
    Ok(ResourceInventory {
        component_guid: component.guid.clone(),
        resources,
    })
}
