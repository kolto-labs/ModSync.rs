//! Settings persistence. The mod folder, patcher choice, Nexus key and game
//! installs (profiles) go through the shared ModSync settings store, so they
//! match the other ModSync frontends. Interface-only values live in a small
//! file next to it that the engine never reads.
use anyhow::Result;
use modsync::profiles::Store;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Set this to keep all settings in an isolated directory (for trying the app
/// without touching the normal ModSync settings).
pub const SETTINGS_DIR_ENV: &str = "MODSYNC_SETTINGS_DIR";
const UI_FILE: &str = "kotor-mod-manager.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomBuild {
    pub key: String,
    pub label: String,
    pub game: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiPrefs {
    pub language: String,
    pub setup_done: bool,
    pub last_seen_version: String,
    pub seven_zip_path: String,
    pub source_site_url: String,
    pub source_site_username: String,
    /// Game (KOTOR1/KOTOR2) of each game install, by install name.
    pub install_games: BTreeMap<String, String>,
    /// Build links added before the engine had a build catalog.
    pub custom_builds: Vec<CustomBuild>,
    /// Conflict ids the player dismissed.
    pub dismissed_conflicts: Vec<String>,
    /// DeadlyStream account name of the saved sign-in.
    pub deadlystream_username: String,
    /// Let guides run patchers and installer programs (on unless turned off).
    pub block_programs: bool,
    /// Folder for unpacking archives during checks and installs; empty for the
    /// system temporary folder.
    pub scratch_dir: String,
    /// Largest unpacked size allowed per archive, in GB; empty for no limit.
    pub max_expanded_gb: String,
}

pub struct Prefs {
    explicit: Option<PathBuf>,
}

impl Prefs {
    /// ModSync's `filterDownloadsByResolution` setting (off unless turned on).
    pub fn filter_by_resolution(&self) -> bool {
        self.store()
            .map(|store| {
                store
                    .desktop_settings()
                    .settings
                    .filter_downloads_by_resolution
            })
            .unwrap_or(false)
    }

    /// Save `filterDownloadsByResolution` in ModSync's settings file.
    pub fn save_filter_by_resolution(&self, on: bool) -> Result<()> {
        modsync::profiles::SettingsFile::resolve(self.explicit())?.update(|root| {
            let map = root
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("Settings must be an object"))?;
            map.retain(|key, _| !key.eq_ignore_ascii_case("filterDownloadsByResolution"));
            map.insert("filterDownloadsByResolution".into(), on.into());
            Ok(())
        })?;
        Ok(())
    }
    pub fn from_env() -> Self {
        Self {
            explicit: std::env::var_os(SETTINGS_DIR_ENV)
                .map(PathBuf::from)
                .filter(|path| !path.as_os_str().is_empty()),
        }
    }

    pub fn explicit(&self) -> Option<&Path> {
        self.explicit.as_deref()
    }

    pub fn store(&self) -> Result<Store> {
        Store::from_settings_directory(self.explicit.as_deref())
    }

    pub fn settings_dir(&self) -> Result<PathBuf> {
        modsync::profiles::settings_directory(self.explicit.as_deref())
    }

    fn ui_path(&self) -> Result<PathBuf> {
        Ok(self.settings_dir()?.join(UI_FILE))
    }

    pub fn load_ui(&self) -> UiPrefs {
        self.ui_path()
            .ok()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save_ui(&self, ui: &UiPrefs) -> Result<()> {
        let path = self.ui_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        modsync::recovery::atomic_write(&path, &serde_json::to_vec_pretty(ui)?)
    }
}
