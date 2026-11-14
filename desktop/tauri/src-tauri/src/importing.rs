//! "Import folder" and dropped archives: install mod archives that are not part
//! of a build, as the original backend's library import did. Each archive is
//! placed in the mod folder and turned into a one-mod guide (`modsync::import`),
//! then checked and installed into the game install's library like a build.
use crate::api::Request;
use crate::pipeline::{Tally, Target, check_and_install};
use crate::state::{Reply, emit, fail, fail_any, game_of, log, state};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::AppHandle;

/// The game install and mod folder an import goes into.
struct Destination {
    mods: PathBuf,
    game: PathBuf,
    profile: String,
}

fn destination(app: &AppHandle, r: &Request) -> Result<Destination, Value> {
    let st = state(app);
    if crate::pipeline::pending_roots(app).is_some() {
        return Err(fail(409, "restore_pending"));
    }
    if st.lock().running {
        return Err(fail(409, "Install already running"));
    }
    let game = r.b("game");
    let store = st.prefs.store().map_err(fail_any)?;
    let requested = r.b("profile");
    let mut profile = if requested.trim().is_empty() {
        crate::builds::target_profile(&st, &game).map_err(fail_any)?
    } else {
        store.require(requested.trim()).map_err(fail_any)?
    };
    let override_path = r.b("game_path");
    if !override_path.trim().is_empty() {
        profile.kotor_directory = Some(override_path.trim().to_owned());
        store.save(&profile).map_err(fail_any)?;
    }
    let path = profile.kotor_directory.clone().unwrap_or_default();
    if path.trim().is_empty() || !Path::new(&path).is_dir() {
        let mut e = fail(400, "game_path_required");
        e["game"] = game.into();
        return Err(e);
    }
    if game_of(&path).is_none() {
        let mut e = fail(400, "game_path_invalid");
        e["game"] = game.into();
        e["path"] = path.into();
        return Err(e);
    }
    let mods = crate::pipeline::mods_folder(&st);
    std::fs::create_dir_all(&mods).map_err(|e| fail(500, e.to_string()))?;
    Ok(Destination {
        mods: PathBuf::from(mods),
        game: PathBuf::from(path),
        profile: profile.name,
    })
}

/// Claim the install slot, so imports and installs never run at once. Stop and
/// Pause act on an import as on an install.
fn begin(app: &AppHandle) -> Result<(Arc<AtomicBool>, Arc<AtomicBool>), Value> {
    let st = state(app);
    let mut inner = st.lock();
    if inner.running {
        return Err(fail(409, "Install already running"));
    }
    let flags = (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    inner.running = true;
    inner.cancelled = flags.0.clone();
    inner.paused = flags.1.clone();
    Ok(flags)
}

fn finish(app: &AppHandle) {
    state(app).lock().running = false;
    emit(app, json!({"type": "library", "event": "changed"}));
}

fn method_name(method: modsync::import::Method) -> &'static str {
    match method {
        modsync::import::Method::Tslpatcher => "TSLPATCHER",
        modsync::import::Method::DirectCopy => "DIRECT_COPY",
    }
}

/// Check and install one prepared archive. Returns true when it was installed.
fn install(
    app: &AppHandle,
    prepared: &modsync::import::Prepared,
    to: &Destination,
    flags: &(Arc<AtomicBool>, Arc<AtomicBool>),
) -> bool {
    let component = &prepared.guide.components[0];
    let name = component.name.clone();
    log(
        app,
        format!("Importing: {name} ({})", method_name(prepared.method)),
        "info",
    );
    let target = Target {
        mods: to.mods.clone(),
        game: to.game.clone(),
        case_insensitive: state(app).lock().case_insensitive,
        cancelled: flags.0.clone(),
        paused: flags.1.clone(),
    };
    let mut tally = Tally::default();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        check_and_install(app, &prepared.guide, &target, false, &mut tally)
    }))
    .unwrap_or_else(|_| Err(anyhow::anyhow!("The import stopped unexpectedly.")));
    match result {
        Ok(()) if !flags.0.load(Ordering::Acquire) => {
            let st = state(app);
            st.lock().ui.imported_mods.insert(
                component.guid.to_ascii_lowercase(),
                prepared.archive.display().to_string(),
            );
            st.save_ui();
            log(app, format!("Imported '{name}'."), "success");
            emit(
                app,
                json!({"type": "library", "event": "imported", "profile": to.profile,
                    "mod": {"id": component.guid, "name": name}}),
            );
            true
        }
        Ok(()) => {
            log(app, format!("Import of '{name}' stopped."), "warning");
            false
        }
        Err(error) => {
            log(
                app,
                format!("Import failed for '{name}': {error:#}"),
                "error",
            );
            emit(
                app,
                json!({"type": "library", "event": "import_failed", "error": format!("{error:#}")}),
            );
            false
        }
    }
}

/// `/api/library/import`: one archive. It is placed and inspected before the
/// reply, so a mod that needs a manual install is reported straight away; the
/// install itself runs in the background.
pub fn import_archive(app: &AppHandle, r: &Request) -> Reply {
    let to = destination(app, r)?;
    let path = PathBuf::from(r.b("path"));
    if !path.exists() {
        return Err(fail(400, "path_not_found"));
    }
    if !modsync::import::is_archive(&path) || !path.is_file() {
        return Err(fail(400, "Choose a ZIP, 7z or RAR archive to import."));
    }
    let flags = begin(app)?;
    let name = r.b("name");
    let options = state(app).archive_options();
    let prepared =
        match modsync::import::prepare(&path, &to.mods, Some(name.as_str()), &options, &flags.0) {
            Ok(Some(prepared)) => prepared,
            Ok(None) => {
                finish(app);
                return Ok(json!({"ok": false, "error": "manual", "readme": "",
                "mod_root": path.display().to_string()}));
            }
            Err(error) => {
                finish(app);
                return Err(fail(400, format!("detect_failed: {error:#}")));
            }
        };
    let method = method_name(prepared.method);
    let worker = app.clone();
    std::thread::spawn(move || {
        install(&worker, &prepared, &to, &flags);
        finish(&worker);
    });
    Ok(json!({"ok": true, "detected_method": method}))
}

/// `/api/library/import-folder`: every archive directly inside a folder, one
/// after another. A failed archive is reported and the next one still imports.
pub fn import_folder(app: &AppHandle, r: &Request) -> Reply {
    let to = destination(app, r)?;
    let folder = PathBuf::from(r.b("path"));
    if !folder.is_dir() {
        return Err(fail(400, "not_a_folder"));
    }
    let archives = modsync::import::archives_in(&folder, &AtomicBool::new(false))
        .map_err(|e| fail(400, format!("{e:#}")))?;
    if archives.is_empty() {
        return Err(fail(400, "no_archives_found"));
    }
    let flags = begin(app)?;
    let count = archives.len();
    let worker = app.clone();
    std::thread::spawn(move || {
        let label = folder.file_name().map_or_else(
            || folder.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        log(
            &worker,
            format!("Importing {count} mod(s) from {label}…"),
            "info",
        );
        let options = state(&worker).archive_options();
        for archive in &archives {
            if flags.0.load(Ordering::Acquire) {
                break;
            }
            let shown = archive
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            match modsync::import::prepare(archive, &to.mods, None, &options, &flags.0) {
                Ok(Some(prepared)) => {
                    install(&worker, &prepared, &to, &flags);
                }
                Ok(None) => log(
                    &worker,
                    format!("Skipped '{shown}': needs manual installation."),
                    "warning",
                ),
                Err(error) => log(
                    &worker,
                    format!("Import failed for '{shown}': {error:#}"),
                    "error",
                ),
            }
        }
        emit(
            &worker,
            json!({"type": "library", "event": "import_folder_done", "profile": to.profile, "count": count}),
        );
        log(
            &worker,
            format!("Folder import complete ({count} archive(s))."),
            "success",
        );
        finish(&worker);
    });
    Ok(json!({"ok": true, "count": count}))
}
