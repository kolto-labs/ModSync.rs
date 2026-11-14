//! Shared app state and the helpers the routes use: errors shaped like the
//! original backend's JSON errors, log lines, and live events.
use crate::prefs::{Prefs, UiPrefs};
use modsync::{Guide, paths::Roots};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{AppHandle, Emitter, Manager};

/// A route result: JSON on success, `{status, error, ...}` on failure.
pub type Reply = Result<Value, Value>;

pub fn fail(status: u16, error: impl Into<String>) -> Value {
    json!({"ok": false, "status": status, "error": error.into()})
}

pub fn fail_any(error: anyhow::Error) -> Value {
    fail(500, format!("{error:#}"))
}

/// A build opened from the build list.
pub struct Loaded {
    pub key: String,
    pub game: String,
    pub guide: Guide,
}

pub struct Inner {
    pub ui: UiPrefs,
    pub loaded: Option<Loaded>,
    pub running: bool,
    pub cancelled: Arc<AtomicBool>,
    /// The folders of an install that stopped and left a restore point behind.
    pub failed_roots: Option<Roots>,
    /// DeadlyStream sign-in checked during this session.
    pub signed_in: bool,
    pub case_insensitive: bool,
}

pub struct AppState {
    inner: Mutex<Inner>,
    pub prefs: Prefs,
}

impl AppState {
    pub fn load() -> Self {
        let prefs = Prefs::from_env();
        let ui = prefs.load_ui();
        let case_insensitive = prefs
            .store()
            .map(|store| store.desktop_settings().settings.case_insensitive_pathing)
            .unwrap_or(true);
        Self {
            inner: Mutex::new(Inner {
                signed_in: !ui.deadlystream_username.is_empty(),
                ui,
                loaded: None,
                running: false,
                cancelled: Arc::new(AtomicBool::new(false)),
                failed_roots: None,
                case_insensitive,
            }),
            prefs,
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    pub fn save_ui(&self) {
        let ui = self.lock().ui.clone();
        let _ = self.prefs.save_ui(&ui);
    }

    /// Ask a running install to stop. Returns true when one is running.
    pub fn request_stop(&self) -> bool {
        let inner = self.lock();
        if inner.running {
            inner.cancelled.store(true, Ordering::Release);
        }
        inner.running
    }

    /// The shared mod (download) folder.
    pub fn mods_dir(&self) -> String {
        self.prefs
            .store()
            .and_then(|store| store.directory_preferences())
            .ok()
            .and_then(|(source, _)| source)
            .unwrap_or_default()
    }

    pub fn nexus_key(&self) -> Option<String> {
        self.prefs
            .store()
            .and_then(|store| store.nexus_api_key())
            .ok()
            .flatten()
            .filter(|key| !key.trim().is_empty())
    }
}

pub fn state(app: &AppHandle) -> tauri::State<'_, AppState> {
    app.state::<AppState>()
}

/// Send one live event (the original backend's WebSocket message).
pub fn emit(app: &AppHandle, event: Value) {
    let _ = app.emit("ws-event", event);
}

pub fn log(app: &AppHandle, message: impl Into<String>, tag: &str) {
    emit(
        app,
        json!({"type": "log", "message": message.into(), "tag": tag}),
    );
}

/// `KOTOR1` or `KOTOR2` for a game folder, from `modsync::game::detect`.
pub fn game_of(path: &str) -> Option<&'static str> {
    use modsync::game::Detected;
    if path.trim().is_empty() {
        return None;
    }
    match modsync::game::detect(&modsync::game::resolve_install_root(Path::new(path.trim()))) {
        Detected::Kotor1 => Some("KOTOR1"),
        Detected::Kotor2Legacy | Detected::Kotor2Aspyr => Some("KOTOR2"),
        Detected::Unknown => None,
    }
}

pub fn open_path(app: &AppHandle, path: &Path) -> Reply {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(|error| fail(500, error.to_string()))?;
    Ok(json!({"ok": true, "path": path.display().to_string()}))
}
