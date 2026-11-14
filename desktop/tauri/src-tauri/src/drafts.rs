//! Install steps for mods the build only describes in words, worked out from
//! the archives that were just downloaded, with the engine's own checks:
//!
//! 1. `archive_generation::resolved::generate` drafts steps from each mod's
//!    downloaded archive (what `convert --resolved-archives` does).
//! 2. `draft::review::review_verified_drafts` clears a draft only when a fresh
//!    replay of that one archive reproduces it exactly (`convert
//!    --review-verified-drafts`). Every other draft stays fenced, so Check
//!    still refuses it.
//! 3. `archive_generation::generate_local_with_cancel` fills mods that have no
//!    steps and no draft at all from their own archives (`convert
//!    --auto-generate-local` with an archive map, so a mod only ever uses the
//!    files downloaded for it).
//!
//! Nothing here approves a draft by itself, and nothing on disk changes.
use crate::state::{log, state};
use modsync::{
    Component, Guide,
    archive_generation::{self, ArchiveMap, resolved},
    draft,
};
use serde_json::{Map, Value, json};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use tauri::AppHandle;

/// Downloaded archive names by link.
type Links = Vec<(String, Vec<String>)>;

/// Archive names the downloader recorded for `component`, by link, that are in
/// the mods folder. Only ZIP, 7z and RAR archives, as `convert` accepts.
fn downloaded(component: &Component, mods: &Path) -> Links {
    component
        .resource_registry
        .iter()
        .filter_map(|(link, entry)| {
            let names: Vec<String> = entry
                .get("Files")
                .and_then(Value::as_object)?
                .iter()
                .filter(|(_, used)| **used == Value::Bool(true))
                .map(|(name, _)| name.clone())
                .filter(|name| {
                    matches!(
                        Path::new(name)
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(str::to_ascii_lowercase)
                            .as_deref(),
                        Some("zip" | "7z" | "rar")
                    ) && mods.join(name).is_file()
                })
                .collect();
            (!names.is_empty()).then(|| (link.clone(), names))
        })
        .collect()
}

/// Steps still missing for the selected mods, worked out from their downloads.
/// Problems are logged; the check that follows reports whatever is still
/// missing.
pub(crate) fn complete(app: &AppHandle, guide: &mut Guide, mods: &Path, cancelled: &AtomicBool) {
    let options = state(app).archive_options();
    let wanted: Vec<(String, Links)> = guide
        .components
        .iter()
        .filter(|c| c.is_selected && c.instructions.is_empty() && c.options.is_empty())
        .map(|c| (c.guid.clone(), downloaded(c, mods)))
        .filter(|(_, files)| !files.is_empty())
        .collect();
    if wanted.is_empty() {
        return;
    }
    log(
        app,
        format!(
            "Working out install steps for {} mod(s) from their downloads.",
            wanted.len()
        ),
        "info",
    );
    // 1. Drafts from each mod's own archives, one mod at a time so one
    // unreadable archive does not hold back the rest.
    for (guid, files) in &wanted {
        if cancelled.load(Ordering::Acquire) {
            return;
        }
        let mut names: Vec<String> = Vec::new();
        for name in files.iter().flat_map(|(_, n)| n) {
            if !names.iter().any(|known| known.eq_ignore_ascii_case(name)) {
                names.push(name.clone());
            }
        }
        let group = resolved::ResolvedFiles {
            component_guid: guid.clone(),
            filenames: names,
            sha256: None,
            size_bytes: None,
        };
        if let Err(error) = resolved::generate(guide, &[group], Some(mods), &options, cancelled) {
            log(app, format!("{}: {error:#}", name_of(guide, guid)), "muted");
        }
    }
    // 2. Clear only the drafts a replay of their archive reproduces exactly.
    match draft::review::review_verified_drafts(&mut guide.components, mods, &options, cancelled) {
        Ok(report) => {
            if !report.cleared.is_empty() {
                log(
                    app,
                    format!(
                        "Confirmed the install steps of {} mod(s) against their downloads.",
                        report.cleared.len()
                    ),
                    "info",
                );
            }
        }
        Err(error) => log(app, format!("{error:#}"), "warning"),
    }
    // 3. Mods with neither steps nor a draft: steps from their own archives.
    let mut map = Map::new();
    let mut local = Guide {
        components: Vec::new(),
        metadata: guide.metadata.clone(),
        ordering: guide.ordering,
    };
    for (guid, files) in &wanted {
        let Some(component) = guide.components.iter().find(|c| &c.guid == guid) else {
            continue;
        };
        if !component.instructions.is_empty() || component.metadata.contains_key("DraftReview") {
            continue;
        }
        for (link, names) in files {
            map.insert(link.clone(), json!(names));
        }
        local.components.push(component.clone());
    }
    if local.components.is_empty() || cancelled.load(Ordering::Acquire) {
        return;
    }
    let generated = ArchiveMap::parse(&Value::Object(map).to_string()).and_then(|map| {
        archive_generation::generate_local_with_cancel(&mut local, mods, Some(&map), cancelled)
    });
    match generated {
        Ok(results) => {
            for filled in local.components {
                if !results
                    .iter()
                    .any(|r| r.success && r.component_guid == filled.guid)
                {
                    continue;
                }
                if let Some(slot) = guide.components.iter_mut().find(|c| c.guid == filled.guid) {
                    *slot = filled;
                }
            }
        }
        Err(error) => log(app, format!("{error:#}"), "warning"),
    }
}

fn name_of(guide: &Guide, guid: &str) -> String {
    guide
        .components
        .iter()
        .find(|c| c.guid == guid)
        .map_or_else(|| guid.to_owned(), |c| c.name.clone())
}
