//! The backend routes the interface calls, served in-process on top of the
//! ModSync engine. Paths, request bodies and replies keep the shape of the
//! kotor-mod-manager backend so the interface works unchanged.
use crate::state::{AppState, Reply, fail, fail_any, game_of, log, open_path, state};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
use tauri::AppHandle;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// One request: the path (with query), the method and an optional JSON body.
pub struct Request {
    pub method: String,
    pub segments: Vec<String>,
    pub query: BTreeMap<String, String>,
    pub body: Value,
}

impl Request {
    pub fn q(&self, key: &str) -> String {
        self.query.get(key).cloned().unwrap_or_default()
    }
    pub fn b(&self, key: &str) -> String {
        self.body
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    }
}

#[tauri::command]
pub async fn api_request(
    app: AppHandle,
    path: String,
    method: String,
    body: Option<Value>,
) -> Reply {
    let url =
        tauri::Url::parse(&format!("http://app{path}")).map_err(|_| fail(400, "Bad request"))?;
    let request = Request {
        method: method.to_ascii_uppercase(),
        segments: url
            .path_segments()
            .map(|parts| parts.filter(|p| !p.is_empty()).map(str::to_owned).collect())
            .unwrap_or_default(),
        query: url.query_pairs().into_owned().collect(),
        body: body.unwrap_or(Value::Null),
    };
    tauri::async_runtime::spawn_blocking(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| route(&app, &request)))
            .unwrap_or_else(|_| Err(fail(500, "The request stopped unexpectedly.")))
    })
    .await
    .map_err(|error| fail(500, error.to_string()))?
}

fn route(app: &AppHandle, r: &Request) -> Reply {
    let s: Vec<&str> = r.segments.iter().map(String::as_str).collect();
    let post = r.method == "POST";
    match (s.as_slice(), r.method.as_str()) {
        (["api", "health"], _) => {
            Ok(json!({"ok": true, "version": VERSION, "platform": platform()}))
        }
        (["api", "status"], _) => status(app),
        (["api", "settings"], "GET") => get_settings(app),
        (["api", "settings"], "POST") => set_settings(app, r),
        (["api", "credentials"], _) => {
            Ok(json!({"username": state(app).lock().ui.deadlystream_username}))
        }
        (["api", "logout"], _) => logout(app),
        (["api", "source-site"], "GET") => {
            let st = state(app);
            let ui = &st.lock().ui;
            Ok(
                json!({"url": ui.source_site_url, "username": ui.source_site_username, "has_password": false}),
            )
        }
        (["api", "source-site"], "POST") => {
            let st = state(app);
            {
                let mut inner = st.lock();
                inner.ui.source_site_url = r.b("url").trim().to_owned();
                inner.ui.source_site_username = r.b("username").trim().to_owned();
            }
            st.save_ui();
            Ok(json!({"ok": true, "has_password": false}))
        }
        (["api", "install", "confirm"], _) => Ok(json!({"ok": true})),
        (["api", "install", "start"], _) => crate::pipeline::start(app, r),
        (["api", "install", "check"], _) => crate::pipeline::check(app, r),
        (["api", "recovery"], _) => crate::pipeline::recovery_state(app),
        (["api", "recovery", "restore"], _) if post => crate::pipeline::restore(app),
        (["api", "install", "state"], _) => {
            Ok(json!({"running": state(app).lock().running, "mods": []}))
        }
        (["api", "install", action], _) if post => crate::pipeline::control(app, action),
        (["api", "profiles"], "GET") => profiles(app),
        (["api", "profiles"], "POST") => create_profile(app, r),
        (["api", "profiles", "active"], _) if post => set_active(app, &r.b("id")),
        (["api", "profiles", id], "PATCH") => update_profile(app, id, r),
        (["api", "profiles", id], "DELETE") => delete_profile(app, id),
        (["api", "builds"], "GET") => crate::builds::list(app),
        (["api", "builds"], "POST") => crate::builds::add(app, r),
        (["api", "builds", "local"], "POST") => crate::builds::add_local(app, r),
        (["api", "builds", key], "DELETE") => crate::builds::delete(app, key),
        (["api", "builds", key, "load"], _) => crate::builds::load(app, key, r),
        (["api", "builds", key, "option"], _) if post => crate::builds::set_option(app, key, r),
        (["api", "library", ..], _) | (["api", "conflicts", ..], _) | (["api", "cache", ..], _) => {
            crate::library::route(app, r, &s)
        }
        (["api", "open-path"], _) => open_path(app, Path::new(&r.b("path"))),
        (["api", "mod", "open-download"], _) => {
            let mods = state(app).mods_dir();
            open_path(app, Path::new(&mods))
                .map(|_| json!({"ok": true, "path": mods, "fallback": true}))
        }
        (["api", "patcher", "status"], _) => patcher_status(app),
        (["api", "spoilers"], "GET") => {
            let on = state(app)
                .prefs
                .store()
                .and_then(|store| store.spoiler_free_preference())
                .unwrap_or(false);
            Ok(json!({"spoiler_free": on}))
        }
        (["api", "spoilers"], "POST") => {
            let on = r
                .body
                .get("spoiler_free")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            state(app)
                .prefs
                .store()
                .and_then(|store| store.save_spoiler_free_preference(on))
                .map_err(fail_any)?;
            Ok(json!({"ok": true}))
        }
        (["api", "patcher", "options"], "GET") => patcher_options(app),
        (["api", "patcher", "options"], "POST") => save_patcher_options(app, r),
        (["api", "extractor", "status"], _) => {
            // Archives are extracted by the engine itself; this shows the 7-Zip
            // the player chose, or the one installed on this computer.
            let chosen = state(app).lock().ui.seven_zip_path.clone();
            Ok(json!({"seven_zip_path": seven_zip(&chosen).map(|p| p.display().to_string())}))
        }
        (["api", "setup"], _) => setup_state(app),
        (["api", "setup", "done"], _) => {
            let st = state(app);
            {
                let mut inner = st.lock();
                inner.ui.setup_done = true;
                inner.ui.last_seen_version = VERSION.into();
            }
            st.save_ui();
            Ok(json!({"ok": true}))
        }
        (["api", "update", "open"], _) => crate::extras::open_url(app, &r.q("url"), true),
        (["api", "gamefront", "request"], _) => Ok(json!({"request": null})),
        (["api", "gamefront", "external"], _) => {
            Ok(json!({"ok": true, "destination": state(app).mods_dir()}))
        }
        (["api", "games", "detect"], _) => detect_games(app),
        (["api", "mod", "info"], _) => crate::extras::mod_info(app, r),
        (["api", "update", "check"], _) => crate::extras::update_check(),
        (["api", "update", "download"], _) => crate::extras::update_download(app),
        (["api", "whatsnew"], _) => crate::extras::whats_new(app),
        (["api", "whatsnew", "seen"], _) => {
            let st = state(app);
            st.lock().ui.last_seen_version = VERSION.into();
            st.save_ui();
            Ok(json!({"ok": true}))
        }
        (["api", "login"], _) => crate::extras::login(app, r),
        (["api", "nexus", "validate"], _) => nexus_validate(app),
        _ => Err(fail(404, "This part of the app is not available yet.")),
    }
}

/// The 7-Zip program: the chosen one, else `7z`, `7za`, `7zz` or `7zr` on
/// PATH, else the usual install folders (Program Files, both 32 and 64 bit,
/// and per-user installs), so a 7-Zip that is installed but not on PATH counts.
fn seven_zip(chosen: &str) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    let chosen = chosen.trim().trim_matches('"');
    if !chosen.is_empty() {
        return Path::new(chosen).is_file().then(|| PathBuf::from(chosen));
    }
    let suffixes: &[&str] = if cfg!(windows) { &[".exe", ""] } else { &[""] };
    let on_path = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    for name in ["7z", "7za", "7zz", "7zr"] {
        for folder in &on_path {
            for suffix in suffixes {
                let candidate = folder.join(format!("{name}{suffix}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    let mut folders: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        folders.push(PathBuf::from(r"C:\Program Files\7-Zip"));
        folders.push(PathBuf::from(r"C:\Program Files (x86)\7-Zip"));
        for variable in [
            "ProgramW6432",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "LOCALAPPDATA",
        ] {
            if let Some(base) = std::env::var_os(variable).map(PathBuf::from) {
                folders.push(base.join("7-Zip"));
                if variable == "LOCALAPPDATA" {
                    folders.push(base.join("Programs").join("7-Zip"));
                }
            }
        }
    }
    folders
        .into_iter()
        .map(|folder| folder.join("7z.exe"))
        .find(|candidate| candidate.is_file())
}

fn platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn status(app: &AppHandle) -> Reply {
    let st = state(app);
    let key = st.nexus_key().is_some();
    let inner = st.lock();
    Ok(json!({
        "version": VERSION,
        "logged_in": inner.signed_in,
        "pipeline_running": inner.running,
        "shim_available": false,
        "shim_path": null,
        "current_build": inner.loaded.as_ref().map(|l| l.key.clone()).unwrap_or_default(),
        "nexus_key_set": key,
    }))
}

fn logout(app: &AppHandle) -> Reply {
    let st = state(app);
    {
        let mut inner = st.lock();
        inner.signed_in = false;
        inner.ui.deadlystream_username.clear();
    }
    st.save_ui();
    Ok(json!({"ok": true}))
}

// ---------------------------------------------------------------------------
// Game installs: ModSync profiles (name, game folder). The two default
// installs "KOTOR 1" and "KOTOR 2" exist from the start, like the original.
// ---------------------------------------------------------------------------

pub const DEFAULTS: [(&str, &str); 2] = [("KOTOR 1", "KOTOR1"), ("KOTOR 2", "KOTOR2")];

pub fn ensure_default_profiles(st: &AppState) -> anyhow::Result<()> {
    let store = st.prefs.store()?;
    let names: Vec<String> = store.list()?.into_iter().map(|p| p.name).collect();
    for (name, _) in DEFAULTS {
        if !names
            .iter()
            .any(|n| modsync::profiles::names_equal(n, name))
        {
            store.create(name)?;
        }
    }
    if store.active_name()?.is_none() {
        store.activate(DEFAULTS[0].0)?;
    }
    Ok(())
}

pub fn profile_game(st: &AppState, profile: &modsync::profiles::Profile) -> String {
    if let Some((_, game)) = DEFAULTS
        .iter()
        .find(|(n, _)| modsync::profiles::names_equal(n, &profile.name))
    {
        return (*game).into();
    }
    st.lock()
        .ui
        .install_games
        .get(&profile.name)
        .cloned()
        .or_else(|| {
            game_of(profile.kotor_directory.as_deref().unwrap_or_default()).map(str::to_owned)
        })
        .unwrap_or_else(|| "KOTOR1".into())
}

/// What the game folder looks like (`modsync::game::detect`), for showing.
fn detected_label(path: &str) -> &'static str {
    use modsync::game::Detected;
    if path.trim().is_empty() || !Path::new(path.trim()).is_dir() {
        return "";
    }
    match modsync::game::detect(&modsync::game::resolve_install_root(Path::new(path.trim()))) {
        Detected::Kotor1 => "KOTOR 1",
        Detected::Kotor2Legacy => "KOTOR 2",
        Detected::Kotor2Aspyr => "KOTOR 2 (Aspyr update)",
        Detected::Unknown => "Not recognized",
    }
}

fn profile_json(st: &AppState, profile: &modsync::profiles::Profile) -> Value {
    json!({
        "detected": detected_label(profile.kotor_directory.as_deref().unwrap_or_default()),
        "id": profile.name,
        "name": profile.name,
        "game": profile_game(st, profile),
        "path": profile.kotor_directory.clone().unwrap_or_default(),
        "is_default": DEFAULTS.iter().any(|(n, _)| modsync::profiles::names_equal(n, &profile.name)),
    })
}

fn profiles(app: &AppHandle) -> Reply {
    let st = state(app);
    ensure_default_profiles(&st).map_err(fail_any)?;
    let store = st.prefs.store().map_err(fail_any)?;
    let list: Vec<Value> = store
        .list()
        .map_err(fail_any)?
        .iter()
        .map(|p| profile_json(&st, p))
        .collect();
    let active = store.active_name().map_err(fail_any)?.unwrap_or_default();
    Ok(json!({"profiles": list, "active": active}))
}

fn create_profile(app: &AppHandle, r: &Request) -> Reply {
    let st = state(app);
    let store = st.prefs.store().map_err(fail_any)?;
    let name = r.b("name").trim().to_owned();
    if name.is_empty() {
        return Err(fail(400, "Give this install a name."));
    }
    let mut profile = store.create(&name).map_err(fail_any)?;
    let path = r.b("path").trim().to_owned();
    profile.kotor_directory = (!path.is_empty()).then_some(path);
    let mods = st.mods_dir();
    profile.mod_directory = (!mods.is_empty()).then_some(mods);
    store.save(&profile).map_err(fail_any)?;
    st.lock().ui.install_games.insert(name, r.b("game"));
    st.save_ui();
    Ok(json!({"ok": true, "profile": profile_json(&st, &profile)}))
}

fn update_profile(app: &AppHandle, id: &str, r: &Request) -> Reply {
    let st = state(app);
    let store = st.prefs.store().map_err(fail_any)?;
    let mut profile = store.require(id).map_err(|e| fail(404, format!("{e:#}")))?;
    if let Some(name) = r.body.get("name").and_then(Value::as_str).map(str::trim)
        && !name.is_empty()
        && !modsync::profiles::names_equal(name, &profile.name)
    {
        let was_active = store
            .active_name()
            .map_err(fail_any)?
            .is_some_and(|a| modsync::profiles::names_equal(&a, id));
        profile = store.rename(id, name).map_err(fail_any)?;
        if was_active {
            store.activate(&profile.name).map_err(fail_any)?;
        }
        let mut inner = st.lock();
        if let Some(game) = inner.ui.install_games.remove(id) {
            inner.ui.install_games.insert(profile.name.clone(), game);
        }
    }
    if let Some(path) = r.body.get("path").and_then(Value::as_str) {
        let path = path.trim();
        profile.kotor_directory = (!path.is_empty()).then(|| path.to_owned());
        store.save(&profile).map_err(fail_any)?;
    }
    st.save_ui();
    Ok(json!({"ok": true, "profile": profile_json(&st, &profile)}))
}

fn delete_profile(app: &AppHandle, id: &str) -> Reply {
    let st = state(app);
    if DEFAULTS
        .iter()
        .any(|(n, _)| modsync::profiles::names_equal(n, id))
    {
        return Err(fail(400, "The default game installs cannot be deleted."));
    }
    let store = st.prefs.store().map_err(fail_any)?;
    store.delete_and_deactivate(id).map_err(fail_any)?;
    if store.active_name().map_err(fail_any)?.is_none() {
        let _ = store.activate(DEFAULTS[0].0);
    }
    st.lock().ui.install_games.remove(id);
    st.save_ui();
    Ok(json!({"ok": true}))
}

pub fn set_active(app: &AppHandle, id: &str) -> Reply {
    let st = state(app);
    let store = st.prefs.store().map_err(fail_any)?;
    let profile = store.activate(id).map_err(fail_any)?;
    if let Some(game) = &profile.kotor_directory {
        let mods = st.mods_dir();
        let _ = store.save_directory_preferences(&mods, game);
    }
    Ok(json!({"ok": true}))
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

fn default_path(st: &AppState, game: &str) -> String {
    st.prefs
        .store()
        .ok()
        .and_then(|store| {
            let name = DEFAULTS.iter().find(|(_, g)| *g == game)?.0;
            store.load(name).ok().flatten()
        })
        .and_then(|p| p.kotor_directory)
        .unwrap_or_default()
}

fn get_settings(app: &AppHandle) -> Reply {
    let st = state(app);
    let _ = ensure_default_profiles(&st);
    let patcher = st
        .prefs
        .store()
        .and_then(|store| store.patcher_preferences())
        .ok()
        .and_then(|p| p.executable)
        .unwrap_or_default();
    let key = st.nexus_key().unwrap_or_default();
    let ui = st.lock().ui.clone();
    Ok(json!({
        "kotor1_path": default_path(&st, "KOTOR1"),
        "kotor2_path": default_path(&st, "KOTOR2"),
        "download_dir": st.mods_dir(),
        "language": if ui.language.is_empty() { "en".to_owned() } else { ui.language },
        "custom_patcher_path": patcher,
        "seven_zip_path": ui.seven_zip_path,
        "nexus_api_key": key,
        "filter_downloads_by_resolution": st.prefs.filter_by_resolution(),
        "scratch_dir": ui.scratch_dir,
        "max_expanded_gb": ui.max_expanded_gb,
    }))
}

fn set_settings(app: &AppHandle, r: &Request) -> Reply {
    let st = state(app);
    let store = st.prefs.store().map_err(fail_any)?;
    for (game, key) in [("KOTOR1", "kotor1_path"), ("KOTOR2", "kotor2_path")] {
        let wanted = r.b(key);
        if wanted != default_path(&st, game)
            && let Some((name, _)) = DEFAULTS.iter().find(|(_, g)| *g == game)
            && let Some(mut profile) = store.load(name).map_err(fail_any)?
        {
            profile.kotor_directory = (!wanted.trim().is_empty()).then(|| wanted.trim().to_owned());
            store.save(&profile).map_err(fail_any)?;
        }
    }
    let mods = r.b("download_dir");
    if mods != st.mods_dir() {
        let (_, destination) = store.directory_preferences().map_err(fail_any)?;
        store
            .save_directory_preferences(mods.trim(), &destination.unwrap_or_default())
            .map_err(fail_any)?;
    }
    let patcher = r.b("custom_patcher_path");
    let current = store.patcher_preferences().map_err(fail_any)?;
    if patcher != current.executable.clone().unwrap_or_default() {
        store
            .save_patcher_preferences(current.engine, &patcher)
            .map_err(fail_any)?;
    }
    let key = r.b("nexus_api_key");
    if key.trim() != st.nexus_key().unwrap_or_default() {
        store.save_nexus_api_key(key.trim()).map_err(fail_any)?;
    }
    {
        let mut inner = st.lock();
        inner.ui.language = r.b("language");
        inner.ui.seven_zip_path = r.b("seven_zip_path");
        if let Some(on) = r
            .body
            .get("filter_downloads_by_resolution")
            .and_then(Value::as_bool)
        {
            st.prefs.save_filter_by_resolution(on).map_err(fail_any)?;
        }
        if r.body.get("scratch_dir").is_some() {
            inner.ui.scratch_dir = r.b("scratch_dir").trim().to_owned();
            inner.ui.max_expanded_gb = r.b("max_expanded_gb").trim().to_owned();
        }
    }
    st.save_ui();
    Ok(json!({"ok": true}))
}

fn patcher_status(app: &AppHandle) -> Reply {
    let st = state(app);
    let prefs = st
        .prefs
        .store()
        .and_then(|store| store.patcher_preferences())
        .map_err(fail_any)?;
    let custom = prefs.executable.unwrap_or_default();
    let found = Path::new(&custom).is_file();
    let engine = engine_name(prefs.engine);
    Ok(json!({
        "available": found,
        "path": if found { Value::from(custom.clone()) } else { Value::Null },
        "source": if found { "custom" } else { "none" },
        "custom_patcher_path": custom,
        "strategies": [engine],
    }))
}

fn engine_name(engine: modsync::PatcherEngine) -> &'static str {
    match engine {
        modsync::PatcherEngine::OdyPatcher => "OdyPatcher",
        modsync::PatcherEngine::KPatcher => "KPatcher",
        modsync::PatcherEngine::HoloPatcher => "HoloPatcher",
    }
}

/// Which patcher applies TSLPatcher-style mods, and whether guides may run
/// patchers and installer programs at all.
fn patcher_options(app: &AppHandle) -> Reply {
    let st = state(app);
    let prefs = st
        .prefs
        .store()
        .and_then(|store| store.patcher_preferences())
        .map_err(fail_any)?;
    let allow = !st.lock().ui.block_programs;
    Ok(json!({"engine": engine_name(prefs.engine), "allow_programs": allow}))
}

fn save_patcher_options(app: &AppHandle, r: &Request) -> Reply {
    let st = state(app);
    let store = st.prefs.store().map_err(fail_any)?;
    let current = store.patcher_preferences().map_err(fail_any)?;
    let engine = match r.b("engine").as_str() {
        "HoloPatcher" => modsync::PatcherEngine::HoloPatcher,
        "KPatcher" => modsync::PatcherEngine::KPatcher,
        _ => modsync::PatcherEngine::OdyPatcher,
    };
    if engine != current.engine {
        store
            .save_patcher_preferences(engine, &current.executable.unwrap_or_default())
            .map_err(fail_any)?;
    }
    let allow = r
        .body
        .get("allow_programs")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    st.lock().ui.block_programs = !allow;
    st.save_ui();
    Ok(json!({"ok": true}))
}

fn setup_state(app: &AppHandle) -> Reply {
    let st = state(app);
    let inner = st.lock();
    let existing = !inner.ui.last_seen_version.is_empty();
    Ok(json!({"show": !inner.ui.setup_done && !existing}))
}

fn detect_games(app: &AppHandle) -> Reply {
    let st = state(app);
    let known: Vec<String> = st
        .prefs
        .store()
        .and_then(|store| store.list())
        .map(|list| list.into_iter().filter_map(|p| p.kotor_directory).collect())
        .unwrap_or_default();
    let same = |a: &Path, b: &Path| match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    };
    let installs: Vec<Value> = modsync::game_locator::find_installs()
        .into_iter()
        .map(|found| {
            json!({
                "game": found.game,
                "path": found.path.display().to_string(),
                "source": found.source,
                "already_added": known.iter().any(|dir| same(Path::new(dir), &found.path)),
            })
        })
        .collect();
    Ok(json!({"installs": installs}))
}

fn nexus_validate(app: &AppHandle) -> Reply {
    let st = state(app);
    let Some(key) = st.nexus_key() else {
        return Ok(json!({"ok": false, "error": "No Nexus API key saved"}));
    };
    let result = modsync::updates::NexusClient::new(Some(key)).and_then(|mut client| {
        client.validate_with_cancel(&std::sync::atomic::AtomicBool::new(false))
    });
    Ok(match result {
        Ok(v) if v.is_valid => json!({"ok": true, "name": v.user_name, "is_premium": v.is_premium}),
        Ok(v) => json!({"ok": false, "error": v.message}),
        Err(error) => {
            log(app, format!("Nexus key check failed: {error:#}"), "warning");
            json!({"ok": false, "error": format!("{error:#}")})
        }
    })
}
