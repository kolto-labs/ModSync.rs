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
//! Nothing here approves a draft by itself, and nothing on disk changes. A
//! draft can be wrong, so the mod's details show its steps and only the
//! player's approval there (`approve`) lets Check use one the replay could not
//! confirm.
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

/// One drafted step in plain words, with the folders named the way players
/// know them.
fn step_text(step: &modsync::Instruction, options: &[Component]) -> String {
    fn folders(path: &str) -> String {
        path.replace("<<modDirectory>>", "mods folder")
            .replace("<<kotorDirectory>>", "game folder")
            .replace("<<gameDirectory>>", "game folder")
    }
    fn option_name(options: &[Component], id: &str) -> Option<String> {
        options.iter().find_map(|o| {
            if o.guid.eq_ignore_ascii_case(id) {
                Some(o.name.clone())
            } else {
                option_name(&o.options, id)
            }
        })
    }
    let sources: Vec<String> = step
        .source
        .iter()
        .map(|s| option_name(options, s).unwrap_or_else(|| folders(s)))
        .collect();
    let mut text = format!("{}: {}", step.action, sources.join(", "));
    if !step.destination.trim().is_empty() {
        text.push_str(&format!(" to {}", folders(&step.destination)));
    }
    if !step.arguments.trim().is_empty() {
        text.push_str(&format!(" ({})", step.arguments.trim()));
    }
    text
}

/// The drafted steps of a mod and its options, in order.
fn steps(component: &Component, options: &[Component], out: &mut Vec<String>, prefix: &str) {
    for step in &component.instructions {
        out.push(format!("{prefix}{}", step_text(step, options)));
    }
    for option in &component.options {
        steps(option, options, out, &format!("{prefix}{}: ", option.name));
    }
}

/// What the details of a mod say about steps the app drafted for it, or `None`
/// when the mod's steps were written by the guide author.
/// `checked` is whether a check has already tried to work the steps out from
/// the mod's download.
pub(crate) fn describe(component: &Component, checked: bool) -> Option<Value> {
    let needs_review = draft::requires_review(component);
    let record = component.metadata.get("DraftReview");
    if record.is_none() && !needs_review {
        return None;
    }
    let result = record.and_then(|r| r.get("Result"));
    let list = |key: &str| -> Vec<String> {
        result
            .and_then(|r| r.get(key))
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    // The engine leaves this gap on a mod whose steps depend on what is in its
    // download. Check works those out from the downloaded archive.
    let mut waiting = false;
    let mut open: Vec<String> = list("UnparsedGaps")
        .into_iter()
        .filter(|gap| {
            let method = gap.contains("requires observed archive contents");
            waiting |= method;
            !method
        })
        .collect();
    open.extend(list("ConditionalDrafts"));
    let rejected = result
        .and_then(|r| r.get("RejectedDrafts"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if waiting && checked {
        open.insert(
            0,
            "The app could not work out exact steps from the download.".to_owned(),
        );
    }
    let waiting = waiting && !checked;
    let mut drafted = Vec::new();
    steps(component, &component.options, &mut drafted, "");
    Some(json!({
        "needs_review": needs_review,
        // Steps the app worked out from the download and confirmed by working
        // them out again from the same file (`review_verified_drafts`).
        "confirmed": !needs_review && record.is_some_and(|r| r.get("Verification").is_some()),
        "waiting_for_download": needs_review && waiting,
        "steps": drafted,
        "unclear": open,
        "rejected": rejected,
        "can_approve": needs_review && !waiting && record.is_some() && open.is_empty() && rejected == 0,
    }))
}

/// The drafted steps of one mod in the open build.
pub fn show(app: &AppHandle, key: &str, guid: &str) -> crate::state::Reply {
    let st = state(app);
    let inner = st.lock();
    let loaded = inner
        .loaded
        .as_ref()
        .filter(|l| l.key == key)
        .ok_or_else(|| crate::state::fail(400, "Load the mod list first"))?;
    let component = loaded
        .guide
        .components
        .iter()
        .find(|c| c.guid.eq_ignore_ascii_case(guid))
        .ok_or_else(|| crate::state::fail(404, "Unknown mod"))?;
    let checked = loaded
        .checked
        .contains(&component.guid.to_ascii_lowercase());
    Ok(json!({"draft": describe(component, checked)}))
}

/// Approve the drafted steps of one mod, as ModSync's "Mark these
/// instructions reviewed" does (`draft::mark_reviewed`). The approval is kept
/// for this build, tied to exactly these steps.
pub fn approve(app: &AppHandle, key: &str, guid: &str) -> crate::state::Reply {
    let st = state(app);
    let (name, draft) = {
        let mut inner = st.lock();
        let loaded = inner
            .loaded
            .as_mut()
            .filter(|l| l.key == key)
            .ok_or_else(|| crate::state::fail(400, "Load the mod list first"))?;
        let component = loaded
            .guide
            .components
            .iter_mut()
            .find(|c| c.guid.eq_ignore_ascii_case(guid))
            .ok_or_else(|| crate::state::fail(404, "Unknown mod"))?;
        let identity = draft::review::identity(component)
            .and_then(|identity| draft::mark_reviewed(component).map(|()| identity))
            .map_err(|e| crate::state::fail(422, format!("{e:#}")))?;
        let name = component.name.clone();
        let draft = describe(component, true);
        inner
            .ui
            .reviewed_drafts
            .entry(key.to_owned())
            .or_default()
            .insert(guid.to_ascii_lowercase(), identity);
        (name, draft)
    };
    st.save_ui();
    log(
        app,
        format!("You approved the drafted install steps of {name}."),
        "info",
    );
    Ok(json!({"ok": true, "draft": draft}))
}

/// Bring back the approvals the player gave for this build, for drafts that
/// came out exactly the same as the steps they approved.
pub(crate) fn apply_saved(app: &AppHandle, key: &str, guide: &mut Guide) {
    let saved = state(app)
        .lock()
        .ui
        .reviewed_drafts
        .get(key)
        .cloned()
        .unwrap_or_default();
    for component in &mut guide.components {
        let Some(approved) = saved.get(&component.guid.to_ascii_lowercase()) else {
            continue;
        };
        if draft::requires_review(component)
            && draft::review::identity(component).is_ok_and(|now| &now == approved)
        {
            let _ = draft::mark_reviewed(component);
        }
    }
}

/// Give the open build the steps this run worked out from downloads, so the
/// mod's details show them, and remember which mods the run tried.
pub(crate) fn keep_for_review(app: &AppHandle, key: &str, before: &Guide, after: &Guide) {
    let st = state(app);
    let mut inner = st.lock();
    let Some(loaded) = inner.loaded.as_mut().filter(|l| l.key == key) else {
        return;
    };
    for (old, new) in before.components.iter().zip(&after.components) {
        if !new.is_selected || old.guid != new.guid {
            continue;
        }
        loaded.checked.insert(new.guid.to_ascii_lowercase());
        let changed = serde_json::to_value(old).ok() != serde_json::to_value(new).ok();
        if let Some(slot) = changed
            .then(|| {
                loaded
                    .guide
                    .components
                    .iter_mut()
                    .find(|c| c.guid == new.guid)
            })
            .flatten()
        {
            let selected = slot.is_selected;
            *slot = new.clone();
            slot.is_selected = selected;
        }
    }
}
