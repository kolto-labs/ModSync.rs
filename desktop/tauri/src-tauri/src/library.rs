//! The installed mods library and the download cache, on ModSync managed
//! deployment (`modsync::deployment`): each game install keeps its mods in a
//! staging folder with a manifest of the files each one owns.
use crate::api::Request;
use crate::state::{AppState, Reply, emit, fail, fail_any, open_path, state};
use modsync::deployment::{ManagementAction, Manifest, Service};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
use tauri::AppHandle;

/// The deployment roots of a game install (the active one when `profile` is empty).
fn service(st: &AppState, profile: &str) -> anyhow::Result<(Service, String, String)> {
    let store = st.prefs.store()?;
    let name = if profile.trim().is_empty() {
        store
            .active_name()?
            .ok_or_else(|| anyhow::anyhow!("No game install is active"))?
    } else {
        profile.to_owned()
    };
    let p = store.require(&name)?;
    let game = p
        .kotor_directory
        .clone()
        .filter(|g| !g.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("Set the game folder for {name} first"))?;
    let svc = Service::resolve_readonly(
        Path::new(&game),
        None,
        None,
        st.prefs.explicit(),
        Some(&name),
    )?;
    Ok((svc, crate::api::profile_game(st, &p), name))
}

/// The active install's deployment roots for a game folder, for installing.
pub fn service_for(app: &AppHandle, game: &str) -> anyhow::Result<Service> {
    let st = state(app);
    Service::resolve(Path::new(game), None, None, st.prefs.explicit(), None)?;
    Service::resolve_readonly(Path::new(game), None, None, st.prefs.explicit(), None)
}

/// Lowercase component ids deployed for a game folder's active install.
pub fn installed_ids(st: &AppState, game: &str) -> BTreeSet<String> {
    Service::resolve_readonly(Path::new(game), None, None, st.prefs.explicit(), None)
        .and_then(|svc| svc.management_status())
        .map(|status| {
            status
                .manifests
                .iter()
                .map(|m| m.component_guid.to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default()
}

fn guide_info(st: &AppState) -> BTreeMap<String, (String, String, String)> {
    let inner = st.lock();
    inner
        .loaded
        .as_ref()
        .map(|l| {
            l.guide
                .components
                .iter()
                .map(|c| {
                    (
                        c.guid.to_ascii_lowercase(),
                        (
                            c.name.clone(),
                            c.category.first().cloned().unwrap_or_default(),
                            l.key.clone(),
                        ),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn mod_json(
    m: Option<&Manifest>,
    guid: &str,
    order: usize,
    game: &str,
    info: &BTreeMap<String, (String, String, String)>,
    conflicts: &BTreeMap<String, usize>,
    duplicate: bool,
) -> Value {
    let key = guid.to_ascii_lowercase();
    let (name, category, build) = info.get(&key).cloned().unwrap_or_default();
    let name = m
        .and_then(|m| m.component_name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or(if name.is_empty() {
            guid.to_owned()
        } else {
            name
        });
    let count = conflicts.get(&key).copied().unwrap_or(0);
    json!({
        "id": guid,
        "name": name,
        "game": game,
        "enabled": m.is_some(),
        "toggleable": true,
        "state": if m.is_some() { "installed" } else { "disabled" },
        "install_method": "",
        "deploy_kind": "",
        "load_order": order,
        "source_type": "build",
        "source_ref": guid,
        "source_slug": "",
        "build_key": if build.is_empty() { Value::Null } else { Value::from(build) },
        "category": category,
        "file_count": m.map_or(0, |m| m.entries.len()),
        "baked_count": 0,
        "install_ts": m.map_or(0, |m| m.deployed_utc.timestamp()),
        "has_conflict": count > 0,
        "conflict_count": count,
        "source_exists": true,
        "duplicate_unmergeable": duplicate,
    })
}

fn conflict_counts(_: &Service, _: &AppState) -> BTreeMap<String, usize> {
    BTreeMap::new()
}

fn duplicate_ids(_: &Service) -> BTreeSet<String> {
    BTreeSet::new()
}

fn listing(st: &AppState, profile: &str) -> anyhow::Result<Value> {
    let (svc, game, name) = service(st, profile)?;
    let status = svc.management_status()?;
    let info = guide_info(st);
    let conflicts = conflict_counts(&svc, st);
    let dupes = duplicate_ids(&svc);
    let mut mods: Vec<Value> = status
        .manifests
        .iter()
        .enumerate()
        .map(|(i, m)| {
            mod_json(
                Some(m),
                &m.component_guid,
                i,
                &game,
                &info,
                &conflicts,
                false,
            )
        })
        .collect();
    let staged = svc.staged_not_deployed().unwrap_or_default();
    for (i, guid) in staged.iter().enumerate() {
        mods.push(mod_json(
            None,
            guid,
            status.manifests.len() + i,
            &game,
            &info,
            &conflicts,
            false,
        ));
    }
    Ok(json!({"game": game, "profile": name, "mods": mods, "mergeable_duplicates": dupes.len()}))
}

fn identity(svc: &Service) -> anyhow::Result<String> {
    Ok(svc.management_status()?.identity)
}

fn manage(svc: &Service, action: ManagementAction) -> anyhow::Result<()> {
    let receipt = svc.manage(&identity(svc)?, action, &AtomicBool::new(false))?;
    if let Some(error) = receipt.error {
        anyhow::bail!(error);
    }
    Ok(())
}

fn set_enabled(st: &AppState, profile: &str, id: &str, enabled: bool) -> anyhow::Result<()> {
    let (svc, _, name) = service(st, profile)?;
    if enabled {
        let writable = Service::resolve(
            svc.game_directory(),
            None,
            None,
            st.prefs.explicit(),
            Some(&name),
        )?;
        let _lock = writable.lock()?;
        let label = guide_info(st)
            .get(&id.to_ascii_lowercase())
            .map(|i| i.0.clone());
        writable.deploy_report(id, label)?;
    } else {
        manage(&svc, ManagementAction::Uninstall(id.to_owned()))?;
    }
    Ok(())
}

fn changed(app: &AppHandle) {
    emit(app, json!({"type": "library", "event": "changed"}));
}

fn mod_reply(st: &AppState, profile: &str, id: &str) -> Reply {
    let list = listing(st, profile).map_err(fail_any)?;
    let m = list["mods"]
        .as_array()
        .and_then(|mods| {
            mods.iter()
                .find(|m| m["id"].as_str().is_some_and(|g| g.eq_ignore_ascii_case(id)))
        })
        .cloned()
        .unwrap_or(Value::Null);
    Ok(json!({"ok": true, "mod": m, "conflicts": []}))
}

fn cache_dir(st: &AppState) -> PathBuf {
    PathBuf::from(st.mods_dir())
}

fn cache_entries(dir: &Path) -> Vec<(String, u64)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, u64)> = entries
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let size = if meta.is_file() {
                meta.len()
            } else {
                walk_size(&e.path())
            };
            Some((e.file_name().to_string_lossy().into_owned(), size))
        })
        .collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.1));
    out
}

fn walk_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| match e.metadata() {
                    Ok(m) if m.is_file() => m.len(),
                    Ok(m) if m.is_dir() => walk_size(&e.path()),
                    _ => 0,
                })
                .sum()
        })
        .unwrap_or(0)
}

/// Archive names the open build's mods use.
fn in_use(st: &AppState) -> BTreeSet<String> {
    let inner = st.lock();
    let mut names = BTreeSet::new();
    if let Some(l) = &inner.loaded {
        for c in &l.guide.components {
            for resource in c.resource_registry.values() {
                if let Some(files) = resource
                    .get("Files")
                    .or_else(|| resource.get("files"))
                    .and_then(Value::as_object)
                {
                    names.extend(files.keys().map(|k| k.trim().to_lowercase()));
                }
            }
        }
    }
    names
}

pub fn route(app: &AppHandle, r: &Request, s: &[&str]) -> Reply {
    let st = state(app);
    let profile = r.q("profile");
    let post = r.method == "POST";
    match s {
        ["api", "library"] => listing(&st, &profile).map_err(fail_any),
        ["api", "library", "baseline"] => {
            let (svc, _, _) = service(&st, &profile).map_err(fail_any)?;
            let game = svc.game_directory().display().to_string();
            let pending = st.lock().failed_roots.is_some()
                || modsync::paths::Roots::new(Path::new(&st.mods_dir()), svc.game_directory())
                    .is_ok_and(|roots| {
                        modsync::recovery::state_dir(&roots)
                            .join("pending")
                            .exists()
                    });
            let deployed = svc
                .management_status()
                .is_ok_and(|s| !s.manifests.is_empty());
            Ok(json!({"has_baseline": pending || deployed, "game_path": game}))
        }
        ["api", "library", "baseline", "reset"] if post => {
            let (svc, _, _) = service(&st, &profile).map_err(fail_any)?;
            // Put back the restore point of an install that stopped, then
            // remove every library mod (putting back the files they replaced).
            let failed = st.lock().failed_roots.take();
            let roots = failed.or_else(|| {
                modsync::paths::Roots::new(Path::new(&st.mods_dir()), svc.game_directory()).ok()
            });
            if let Some(roots) = roots
                && modsync::recovery::state_dir(&roots)
                    .join("pending")
                    .exists()
            {
                modsync::recovery::restore(&roots).map_err(fail_any)?;
            }
            let before = svc
                .management_status()
                .map(|s| s.manifests.len())
                .unwrap_or(0);
            if before > 0 {
                manage(&svc, ManagementAction::Purge).map_err(fail_any)?;
            }
            changed(app);
            Ok(
                json!({"ok": true, "override_removed": before, "modules_removed": 0, "restored": []}),
            )
        }
        ["api", "library", "import-folder"] | ["api", "library", "import"] => Err(fail(
            400,
            "Importing mods from a folder is not supported. Install mods from a build instead.",
        )),
        ["api", "library", "bulk", "toggle"] if post => {
            let enable = r.q("action") == "enable";
            let ids: Vec<String> = r
                .body
                .get("mod_ids")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let (mut changed_ids, mut failed) = (Vec::new(), Vec::new());
            let total = ids.len();
            for (i, id) in ids.iter().enumerate() {
                emit(
                    app,
                    json!({"type": "library", "event": "bulk_progress", "action": r.q("action"), "current": i + 1, "total": total, "mod": id}),
                );
                match set_enabled(&st, &profile, id, enable) {
                    Ok(()) => changed_ids.push(id.clone()),
                    Err(e) => failed.push(json!({"mod": id, "reason": format!("{e:#}")})),
                }
            }
            emit(
                app,
                json!({"type": "library", "event": "bulk_progress", "action": r.q("action"), "current": total, "total": total, "mod": "", "done": true}),
            );
            changed(app);
            Ok(
                json!({"ok": true, "action": r.q("action"), "changed": changed_ids, "failed": failed}),
            )
        }
        ["api", "library", id, action @ ("enable" | "disable")] if post => {
            set_enabled(&st, &profile, id, *action == "enable").map_err(fail_any)?;
            changed(app);
            mod_reply(&st, &profile, id)
        }
        ["api", "library", id, "open-folder"] if post => {
            let (svc, _, _) = service(&st, &profile).map_err(fail_any)?;
            let staged = svc.staging_root().join(id.to_ascii_lowercase());
            let target = if staged.is_dir() {
                staged
            } else {
                svc.game_directory().to_path_buf()
            };
            open_path(app, &target)
        }
        ["api", "library", id] => {
            let (svc, _, _) = service(&st, &profile).map_err(fail_any)?;
            let manifest = svc.manifest(id).map_err(fail_any)?;
            let mut detail = mod_reply(&st, &profile, id)?["mod"].clone();
            detail["deployed_files"] = manifest
                .map(|m| m.entries.iter().map(|e| json!({
                    "rel_path": e.relative_path, "sha256": e.source_hash, "size": e.size, "overwrote": e.overwrote_existing,
                })).collect::<Vec<_>>())
                .unwrap_or_default()
                .into();
            detail["baked_files"] = json!([]);
            Ok(detail)
        }
        ["api", "cache"] => {
            let dir = cache_dir(&st);
            let used = in_use(&st);
            let entries = cache_entries(&dir);
            let total: u64 = entries.iter().map(|e| e.1).sum();
            let in_use_bytes: u64 = entries
                .iter()
                .filter(|e| used.contains(&e.0.to_lowercase()))
                .map(|e| e.1)
                .sum();
            Ok(json!({
                "path": dir.display().to_string(),
                "total_bytes": total,
                "count": entries.len(),
                "in_use_bytes": in_use_bytes,
                "entries": entries.iter().map(|(n, b)| json!({"name": n, "bytes": b, "in_use": used.contains(&n.to_lowercase()), "files": 1})).collect::<Vec<_>>(),
            }))
        }
        ["api", "cache", "open"] if post => {
            let dir = cache_dir(&st);
            let name = r.q("name");
            let target = if name.is_empty() || name.contains(['/', '\\']) {
                dir
            } else {
                dir.join(name)
            };
            open_path(app, &target)
        }
        ["api", "cache", "clear"] if post => {
            let dir = cache_dir(&st);
            if dir.as_os_str().is_empty() || !dir.is_dir() {
                return Ok(json!({"ok": true, "removed": [], "freed_bytes": 0, "failed": []}));
            }
            let keep = r
                .body
                .get("keep_installed")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let names: Vec<String> = r
                .body
                .get("names")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let used = in_use(&st);
            let (mut removed, mut failed, mut freed) = (Vec::new(), Vec::new(), 0u64);
            for (name, bytes) in cache_entries(&dir) {
                if (!names.is_empty() && !names.contains(&name))
                    || (keep && used.contains(&name.to_lowercase()))
                {
                    continue;
                }
                let path = dir.join(&name);
                let result = if path.is_dir() {
                    std::fs::remove_dir_all(&path)
                } else {
                    std::fs::remove_file(&path)
                };
                match result {
                    Ok(()) => {
                        removed.push(name);
                        freed += bytes;
                    }
                    Err(e) => failed.push(json!({"name": name, "reason": e.to_string()})),
                }
            }
            Ok(json!({"ok": true, "removed": removed, "freed_bytes": freed, "failed": failed}))
        }
        _ => Err(fail(404, "This part of the app is not available yet.")),
    }
}
