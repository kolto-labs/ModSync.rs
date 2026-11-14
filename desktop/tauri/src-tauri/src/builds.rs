//! Mod builds: the list, adding and removing build links, and loading a
//! build's instruction file into the engine (`Guide::load_for_install`).
use crate::api::Request;
use crate::state::{AppState, Loaded, Reply, fail, fail_any, log, state};
use modsync::{Component, Guide};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::PathBuf, sync::atomic::AtomicBool};
use tauri::AppHandle;

struct Build {
    key: String,
    label: String,
    game: String,
    url: String,
    custom: bool,
}

fn build_json(b: &Build) -> Value {
    json!({"key": b.key, "label": b.label, "game": b.game, "custom": b.custom, "url": b.url})
}

fn catalog(st: &AppState) -> anyhow::Result<modsync::builds::Catalog> {
    modsync::builds::Catalog::open(st.prefs.explicit())
}

fn all(st: &AppState) -> anyhow::Result<Vec<Build>> {
    let mut builds: Vec<Build> = catalog(st)?
        .list()?
        .into_iter()
        .map(|b| Build {
            key: b.key,
            label: b.label,
            game: b.game,
            url: b.url,
            custom: b.custom,
        })
        .collect();
    // Instruction files opened from this computer.
    builds.extend(st.lock().ui.custom_builds.iter().map(|b| Build {
        key: b.key.clone(),
        label: b.label.clone(),
        game: b.game.clone(),
        url: b.url.clone(),
        custom: true,
    }));
    Ok(builds)
}

fn is_local(key: &str) -> bool {
    key.starts_with("file_")
}

/// Add an instruction file from this computer (TOML, JSON, YAML, XML or
/// Markdown) to the build list. It is read again from disk every time.
pub fn add_local(app: &AppHandle, r: &Request) -> Reply {
    let st = state(app);
    let path = PathBuf::from(r.b("path").trim());
    let guide = Guide::load_for_install_preserving_selection(&path)
        .map_err(|e| fail(422, format!("{e:#}")))?;
    let label = modsync::wizard::metadata_text(&guide, "title", "Title")
        .map(str::to_owned)
        .filter(|t| !t.trim().is_empty())
        .or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "Instruction file".into());
    let game = st
        .prefs
        .store()
        .ok()
        .and_then(|store| store.load(&store.active_name().ok()??).ok()?)
        .map(|p| crate::api::profile_game(&st, &p))
        .unwrap_or_else(|| "KOTOR1".into());
    let url = path.display().to_string();
    let existing = st
        .lock()
        .ui
        .custom_builds
        .iter()
        .find(|b| b.url == url)
        .cloned();
    let build = match existing {
        Some(build) => build,
        None => {
            let mut n = st.lock().ui.custom_builds.len() + 1;
            while st
                .lock()
                .ui
                .custom_builds
                .iter()
                .any(|b| b.key == format!("file_{n}"))
            {
                n += 1;
            }
            let build = crate::prefs::CustomBuild {
                key: format!("file_{n}"),
                label,
                game,
                url,
            };
            st.lock().ui.custom_builds.push(build.clone());
            st.save_ui();
            build
        }
    };
    Ok(
        json!({"ok": true, "build": {"key": build.key, "label": build.label, "game": build.game, "custom": true, "url": build.url}}),
    )
}

pub fn add(app: &AppHandle, r: &Request) -> Reply {
    let st = state(app);
    let b = catalog(&st)
        .and_then(|c| c.add(&r.b("label"), &r.b("game"), &r.b("url")))
        .map_err(|e| fail(400, format!("{e:#}")))?;
    Ok(
        json!({"ok": true, "build": {"key": b.key, "label": b.label, "game": b.game, "custom": true, "url": b.url}}),
    )
}

pub fn delete(app: &AppHandle, key: &str) -> Reply {
    let st = state(app);
    if is_local(key) {
        st.lock().ui.custom_builds.retain(|b| b.key != key);
        st.save_ui();
        return Ok(json!({"ok": true}));
    }
    catalog(&st)
        .and_then(|c| c.remove(key))
        .map_err(|e| fail(400, format!("{e:#}")))?;
    Ok(json!({"ok": true}))
}

fn fetch(st: &AppState, build: &Build, refresh: bool) -> anyhow::Result<(PathBuf, bool)> {
    if is_local(&build.key) {
        return Ok((PathBuf::from(&build.url), false));
    }
    let fetched = catalog(st)?.fetch(&build.key, refresh, &AtomicBool::new(false))?;
    Ok((fetched.path, fetched.from_cache))
}

pub fn list(app: &AppHandle) -> Reply {
    let st = state(app);
    let builds: Vec<Value> = all(&st).map_err(fail_any)?.iter().map(build_json).collect();
    Ok(json!({"builds": builds}))
}

/// The game install for a build: the active install when it is for the same
/// game, otherwise the default install for that game.
pub fn target_profile(st: &AppState, game: &str) -> anyhow::Result<modsync::profiles::Profile> {
    crate::api::ensure_default_profiles(st)?;
    let store = st.prefs.store()?;
    if let Some(active) = store.active_name()?
        && let Some(profile) = store.load(&active)?
        && crate::api::profile_game(st, &profile) == game
    {
        return Ok(profile);
    }
    let name = crate::api::DEFAULTS
        .iter()
        .find(|(_, g)| *g == game)
        .map_or(crate::api::DEFAULTS[0].0, |(n, _)| *n);
    store.require(name)
}

fn host(url: &str) -> (&'static str, &'static str) {
    let lower = url.to_ascii_lowercase();
    if lower.contains("deadlystream.com") {
        ("deadlystream", "DeadlyStream")
    } else if lower.contains("nexusmods.com") {
        ("nexus", "Nexus Mods")
    } else if lower.contains("mega.nz") || lower.contains("mega.co.nz") {
        ("mega", "MEGA")
    } else if lower.contains("gamefront.com") {
        ("gamefront", "GameFront")
    } else {
        ("other", "Direct link")
    }
}

fn text(c: &Component, key: &str) -> String {
    c.metadata
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// A component's options (nested), for choosing them in the mod details.
fn options_json(options: &[Component]) -> Vec<Value> {
    options
        .iter()
        .map(|o| {
            json!({
                "guid": o.guid,
                "name": o.name,
                "description": o.description,
                "selected": o.is_selected,
                "options": options_json(&o.options),
            })
        })
        .collect()
}

fn names(all: &[Component], ids: &[String]) -> Vec<String> {
    ids.iter()
        .map(|id| {
            all.iter()
                .find(|c| c.guid.eq_ignore_ascii_case(id))
                .map_or_else(|| id.clone(), |c| c.name.clone())
        })
        .collect()
}

/// Turn one option of a mod on or off in the open build, with the engine's
/// selection rules (`selection::Action::SetOption`). Returns the mod's options.
pub fn set_option(app: &AppHandle, key: &str, r: &Request) -> Reply {
    let st = state(app);
    let mut inner = st.lock();
    let loaded = inner
        .loaded
        .as_mut()
        .filter(|l| l.key == key)
        .ok_or_else(|| fail(400, "Load the mod list first"))?;
    let guid = r.b("guid");
    let selected = r
        .body
        .get("selected")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    modsync::selection::apply(
        &mut loaded.guide.components,
        None,
        modsync::selection::Action::SetOption {
            guid: guid.clone(),
            selected,
        },
    )
    .map_err(fail_any)?;
    let owner = loaded
        .guide
        .components
        .iter()
        .find(|c| {
            fn has(options: &[Component], id: &str) -> bool {
                options
                    .iter()
                    .any(|o| o.guid.eq_ignore_ascii_case(id) || has(&o.options, id))
            }
            has(&c.options, &guid)
        })
        .ok_or_else(|| fail(404, "Unknown option"))?;
    Ok(json!({"ok": true, "options": options_json(&owner.options)}))
}

/// A component in the shape the interface's mod list expects.
#[allow(clippy::too_many_arguments)]
pub fn mod_json(
    all: &[Component],
    spoiler_free: bool,
    c: &Component,
    order: usize,
    game: &str,
    key: &str,
    installed: bool,
    nexus_key: bool,
) -> Value {
    let mut links: Vec<String> = c.resource_registry.keys().cloned().collect();
    for k in ["Link", "DownloadLink"] {
        if let Some(Value::String(url)) = c.metadata.get(k)
            && !links.contains(url)
        {
            links.push(url.clone());
        }
    }
    let url = links.first().cloned().unwrap_or_default();
    let (host_key, host_label) = host(&url);
    let warnings = [
        "UsageWarning",
        "InstallationWarning",
        "CompatibilityWarning",
        "KnownBugs",
    ]
    .iter()
    .map(|k| text(c, k))
    .filter(|t| !t.is_empty())
    .collect::<Vec<_>>()
    .join("\n\n");
    json!({
        "install_order": order,
        "file_id": c.guid,
        "slug": "",
        "name": if spoiler_free { modsync::selection::display_name(c, true) } else { c.name.clone() },
        "url": url,
        "game": game,
        "section": c.tier,
        "category": c.category.first().cloned().unwrap_or_default(),
        "note": "",
        "option_hint": "",
        "install_method_hint": text(c, "InstallationMethod"),
        "build_key": key,
        "instructions": if spoiler_free { String::new() } else { c.directions.clone() },
        "warnings": warnings,
        "install_method": text(c, "InstallationMethod"),
        "description": modsync::selection::tooltip_description(c, spoiler_free),
        "author": c.author,
        "directive_summary": "",
        "source_host": host_key,
        "source_label": host_label,
        "auto_downloadable": match host_key {
            "nexus" => nexus_key,
            "gamefront" => false,
            _ => true,
        },
        "patch_urls": links.iter().skip(1).collect::<Vec<_>>(),
        "installed": installed,
        "options": options_json(&c.options),
        "dependencies": names(all, &c.dependencies),
        "restrictions": names(all, &c.restrictions),
    })
}

/// Load a build into the engine and list its mods.
pub fn load(app: &AppHandle, key: &str, r: &Request) -> Reply {
    let st = state(app);
    let build = all(&st)
        .map_err(fail_any)?
        .into_iter()
        .find(|b| b.key == key)
        .ok_or_else(|| fail(404, format!("There is no build called {key}")))?;
    let refresh = r.q("refresh") == "true";
    let (path, from_cache) =
        fetch(&st, &build, refresh).map_err(|e| fail(502, format!("{e:#}")))?;
    let guide = Guide::load_for_install_preserving_selection(&path)
        .map_err(|e| fail(422, format!("{e:#}")))?;
    let installed: BTreeSet<String> = target_profile(&st, &build.game)
        .ok()
        .and_then(|profile| profile.kotor_directory)
        .map(|game| crate::library::installed_ids(&st, &game))
        .unwrap_or_default();
    let nexus = st.nexus_key().is_some();
    let spoiler_free = st
        .prefs
        .store()
        .and_then(|store| store.spoiler_free_preference())
        .unwrap_or(false);
    let mods: Vec<Value> = guide
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| {
            mod_json(
                &guide.components,
                spoiler_free,
                c,
                i + 1,
                &build.game,
                key,
                installed.contains(&c.guid.to_ascii_lowercase()),
                nexus,
            )
        })
        .collect();
    if !from_cache {
        log(
            app,
            format!("Downloaded the latest {}.", build.label),
            "muted",
        );
    }
    st.lock().loaded = Some(Loaded {
        key: key.to_owned(),
        game: build.game,
        guide,
    });
    Ok(json!({"ok": true, "build_key": key, "from_cache": from_cache, "mods": mods}))
}
