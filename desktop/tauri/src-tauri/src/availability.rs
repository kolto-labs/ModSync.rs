//! Which mods can be selected, worked out by the engine's own selection graph
//! (#68): a mod whose archive is broken, or whose instructions can't be read,
//! can't be selected, and neither can anything that needs it, directly or
//! through other mods.
//!
//! The app downloads what a build needs at Check, so a missing archive only
//! counts once the app has tried to download it and the download failed. The
//! engine decides everything else: the failures it finds in the mods folder,
//! which mods depend on them, the reasons, and the game rules of Select All.

use crate::api::Request;
use crate::state::{Reply, fail, state};
use modsync::{
    Engine, Guide, Settings,
    selection::{self, Action, Availability, ExclusionCategory, Filter},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
use tauri::AppHandle;

/// Inspect the mods folder with the install engine's archive rules and keep the
/// failures a player can act on now. `failed` holds the mods (lowercase ids)
/// whose download failed, with the download's reason.
pub fn inspect(
    app: &AppHandle,
    guide: &Guide,
    game: &Path,
    mods: &Path,
    failed: &BTreeMap<String, String>,
) -> anyhow::Result<Availability> {
    let st = state(app);
    let mut settings = Settings::new(mods, game)?;
    settings.archive = st.archive_options();
    let case_insensitive = st.lock().case_insensitive;
    let engine = Engine::new_with_case_insensitive_pathing(settings, case_insensitive)?
        .with_archive_listing_cache(st.archive_listings.clone());
    let found = engine.selection_availability(guide)?;
    // Keep the engine's direct failures, then let the engine work out again which
    // mods need them. Its own dependency results are recomputed, not copied.
    let direct = found
        .unavailable
        .into_iter()
        .filter_map(|mut entry| match entry.category {
            ExclusionCategory::InvalidArchive | ExclusionCategory::InvalidInstruction => {
                Some(entry)
            }
            ExclusionCategory::MissingArchive => {
                let reason = failed.get(&entry.guid.to_ascii_lowercase())?;
                entry.reason = format!("The download failed: {reason}");
                Some(entry)
            }
            _ => None,
        })
        .collect();
    Ok(Availability::from_archive_failures(
        &guide.components,
        direct,
    ))
}

/// The game folder a build installs to, if one is set and exists.
fn game_folder(app: &AppHandle, game: &str, r: &Request) -> Option<String> {
    let chosen = r.b("game_path");
    let path = if chosen.trim().is_empty() {
        crate::builds::target_profile(&state(app), game)
            .ok()
            .and_then(|profile| profile.kotor_directory)
            .unwrap_or_default()
    } else {
        chosen.trim().to_owned()
    };
    (!path.trim().is_empty() && Path::new(&path).is_dir()).then_some(path)
}

/// One mod or option the player can't select, for the interface.
fn entries(availability: &Availability) -> Vec<Value> {
    availability
        .unavailable
        .iter()
        .map(|entry| json!({"file_id": entry.guid, "category": entry.category, "reason": entry.reason}))
        .collect()
}

/// Select All: every mod the build lists, with what each needs, minus mods that
/// can't be installed and everything that needs them. The same engine action as
/// the command line's `selection` with no selectors.
pub fn select_all(app: &AppHandle, key: &str, r: &Request) -> Reply {
    let st = state(app);
    let (guide, game) = {
        let inner = st.lock();
        if inner.running {
            return Err(fail(409, "Install already running"));
        }
        let Some(loaded) = inner.loaded.as_ref().filter(|l| l.key == key) else {
            return Err(fail(400, "Load the mod list first"));
        };
        (loaded.guide.clone(), loaded.game.clone())
    };
    let folder = game_folder(app, &game, r);
    let mods = crate::pipeline::mods_folder(&st);
    let detected = folder
        .as_deref()
        .map(|dir| modsync::game::detect(&modsync::game::resolve_install_root(Path::new(dir))));
    let mut components = guide.components.clone();
    let action = Action::SelectVisible(Filter::default());
    // Without a game folder or a mods folder there is nothing to inspect yet, so
    // only the selection rules apply. Check asks for the game folder.
    let (outcome, availability) = match folder.filter(|_| Path::new(&mods).is_dir()) {
        Some(folder) => {
            let failed = st
                .lock()
                .download_failures
                .get(key)
                .cloned()
                .unwrap_or_default();
            // An incomplete inspection fails here, before anything is changed.
            let availability = inspect(app, &guide, Path::new(&folder), Path::new(&mods), &failed)
                .map_err(|e| fail(409, format!("{e:#}")))?;
            let outcome =
                selection::apply_available(&mut components, detected, action, &availability)
                    .map_err(|e| fail(422, format!("{e:#}")))?;
            st.lock()
                .availability
                .insert(key.to_owned(), availability.clone());
            (outcome, Some(availability))
        }
        None => (
            selection::apply(&mut components, detected, action)
                .map_err(|e| fail(422, format!("{e:#}")))?,
            None,
        ),
    };
    let selected: Vec<&str> = components
        .iter()
        .filter(|c| c.is_selected)
        .map(|c| c.guid.as_str())
        .collect();
    Ok(json!({
        "selected": selected,
        "inspected": availability.is_some(),
        "unavailable": availability.as_ref().map(entries).unwrap_or_default(),
        "warnings": outcome.warnings,
    }))
}

/// Give failed downloads another try (Retry failed): forget them for the loaded
/// build, and work out again which mods the remaining failures block.
pub fn forget_failed_downloads(app: &AppHandle) {
    let st = state(app);
    let mut inner = st.lock();
    let Some(loaded) = inner.loaded.as_ref() else {
        return;
    };
    let (key, components) = (loaded.key.clone(), loaded.guide.components.clone());
    inner.download_failures.remove(&key);
    if let Some(availability) = inner.availability.get_mut(&key) {
        let direct = std::mem::take(&mut availability.unavailable)
            .into_iter()
            .filter(|entry| {
                matches!(
                    entry.category,
                    ExclusionCategory::InvalidArchive | ExclusionCategory::InvalidInstruction
                )
            })
            .collect();
        *availability = Availability::from_archive_failures(&components, direct);
    }
}

/// The last completed check of which mods can be selected, for one build.
pub fn last(app: &AppHandle, key: &str) -> Vec<Value> {
    state(app)
        .lock()
        .availability
        .get(key)
        .map(entries)
        .unwrap_or_default()
}
