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

fn all(st: &AppState) -> anyhow::Result<Vec<Build>> {
    Ok(st
        .lock()
        .ui
        .custom_builds
        .iter()
        .map(|b| Build {
            key: b.key.clone(),
            label: b.label.clone(),
            game: b.game.clone(),
            url: b.url.clone(),
            custom: true,
        })
        .collect())
}

fn cache_path(st: &AppState, key: &str) -> anyhow::Result<PathBuf> {
    Ok(st
        .prefs
        .settings_dir()?
        .join("builds")
        .join("cache")
        .join(format!("{key}.md")))
}

pub fn add(app: &AppHandle, r: &Request) -> Reply {
    let st = state(app);
    let label = r.b("label").trim().to_owned();
    let url = r.b("url").trim().to_owned();
    if !url.starts_with("https://") {
        return Err(fail(400, "Build links must start with https://"));
    }
    let game = if r.b("game") == "KOTOR2" {
        "KOTOR2"
    } else {
        "KOTOR1"
    };
    let mut key: String = format!(
        "custom_{}",
        label
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>()
    );
    while st.lock().ui.custom_builds.iter().any(|b| b.key == key) {
        key.push('_');
    }
    let build = crate::prefs::CustomBuild {
        key: key.clone(),
        label: if label.is_empty() { url.clone() } else { label },
        game: game.into(),
        url,
    };
    st.lock().ui.custom_builds.push(build.clone());
    st.save_ui();
    Ok(
        json!({"ok": true, "build": {"key": build.key, "label": build.label, "game": build.game, "custom": true, "url": build.url}}),
    )
}

pub fn delete(app: &AppHandle, key: &str) -> Reply {
    let st = state(app);
    st.lock().ui.custom_builds.retain(|b| b.key != key);
    st.save_ui();
    if let Ok(path) = cache_path(&st, key) {
        let _ = std::fs::remove_file(path);
    }
    Ok(json!({"ok": true}))
}

/// The instruction file for a build: the saved copy, or a fresh download.
fn fetch(st: &AppState, build: &Build, refresh: bool) -> anyhow::Result<(PathBuf, bool)> {
    let cached = cache_path(st, &build.key)?;
    if !refresh && cached.is_file() {
        return Ok((cached, true));
    }
    let dir = cached.parent().expect("cache folder");
    std::fs::create_dir_all(dir)?;
    let partial = tempfile_dir(dir)?;
    let download = partial.join("build.md");
    let result = modsync::protocol::fetch_instruction_with_cancel(
        &build.url,
        &download,
        &AtomicBool::new(false),
    );
    let outcome = match result {
        Ok(()) => {
            std::fs::rename(&download, &cached)?;
            Ok((cached, false))
        }
        Err(_) if cached.is_file() => Ok((cached, true)),
        Err(error) => Err(error),
    };
    let _ = std::fs::remove_dir_all(&partial);
    outcome
}

fn tempfile_dir(dir: &std::path::Path) -> anyhow::Result<PathBuf> {
    for n in 0..100u32 {
        let path = dir.join(format!(".build-{}-{n}", std::process::id()));
        if std::fs::create_dir(&path).is_ok() {
            return Ok(path);
        }
    }
    anyhow::bail!("Could not create a temporary folder")
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

/// A component in the shape the interface's mod list expects.
pub fn mod_json(
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
        "name": c.name,
        "url": url,
        "game": game,
        "section": c.tier,
        "category": c.category.first().cloned().unwrap_or_default(),
        "note": "",
        "option_hint": "",
        "install_method_hint": text(c, "InstallationMethod"),
        "build_key": key,
        "instructions": c.directions,
        "warnings": warnings,
        "install_method": text(c, "InstallationMethod"),
        "description": c.description,
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
    let mods: Vec<Value> = guide
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| {
            mod_json(
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
