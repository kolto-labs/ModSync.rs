//! Typed desktop settings. Failed decoding resets the whole typed projection;
//! persistence always preserves unknown keys and refuses unreadable input.
use super::{PatcherPreferences, Store, storage, without_bom};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    sync::atomic::{AtomicBool, Ordering},
};
const LIMIT: usize = 16 * 1024 * 1024;
const KEYS: &[&str] = &[
    "theme",
    "sourcePath",
    "destinationPath",
    "debugLogging",
    "attemptFixes",
    "noAdmin",
    "caseInsensitivePathing",
    "archiveDeepCheck",
    "useMultiThreadedIO",
    "useCopyForMoveActions",
    "lastOutputDirectory",
    "validateAndReplaceInvalidArchives",
    "filterDownloadsByResolution",
    "nexusModsApiKey",
    "fileEncoding",
    "selectedHolopatcherVersion",
    "patcherEngine",
    "kpatcherExecutablePath",
    "enableFileWatcher",
    "spoilerFreeMode",
    "registerNxmProtocolHandler",
    "fomodPostDownloadMode",
    "managedDeploymentEnabled",
    "activeProfileName",
];
const BOOL_KEYS: &[&str] = &[
    "debugLogging",
    "attemptFixes",
    "noAdmin",
    "caseInsensitivePathing",
    "archiveDeepCheck",
    "useMultiThreadedIO",
    "useCopyForMoveActions",
    "validateAndReplaceInvalidArchives",
    "filterDownloadsByResolution",
    "enableFileWatcher",
    "spoilerFreeMode",
    "registerNxmProtocolHandler",
    "managedDeploymentEnabled",
];

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct DesktopSettings {
    pub theme: Option<String>,
    pub source_path: Option<String>,
    pub destination_path: Option<String>,
    pub debug_logging: bool,
    pub attempt_fixes: bool,
    pub no_admin: bool,
    pub case_insensitive_pathing: bool,
    pub archive_deep_check: bool,
    #[serde(rename = "useMultiThreadedIO")]
    pub use_multi_threaded_io: bool,
    pub use_copy_for_move_actions: bool,
    pub last_output_directory: Option<String>,
    pub validate_and_replace_invalid_archives: bool,
    pub filter_downloads_by_resolution: bool,
    pub nexus_mods_api_key: Option<String>,
    pub file_encoding: Option<String>,
    pub selected_holopatcher_version: Option<String>,
    pub patcher_engine: Option<String>,
    pub kpatcher_executable_path: Option<String>,
    pub enable_file_watcher: bool,
    pub spoiler_free_mode: bool,
    pub register_nxm_protocol_handler: bool,
    pub fomod_post_download_mode: Option<String>,
    pub managed_deployment_enabled: bool,
    pub active_profile_name: Option<String>,
}
impl Default for DesktopSettings {
    fn default() -> Self {
        Self {
            theme: Some("/Styles/LightStyle.axaml".into()),
            source_path: None,
            destination_path: None,
            debug_logging: true,
            attempt_fixes: true,
            no_admin: false,
            case_insensitive_pathing: true,
            archive_deep_check: false,
            use_multi_threaded_io: false,
            use_copy_for_move_actions: false,
            last_output_directory: None,
            validate_and_replace_invalid_archives: true,
            filter_downloads_by_resolution: false,
            nexus_mods_api_key: None,
            file_encoding: Some("utf-8".into()),
            selected_holopatcher_version: None,
            patcher_engine: Some("Holopatcher".into()),
            kpatcher_executable_path: None,
            enable_file_watcher: true,
            spoiler_free_mode: false,
            register_nxm_protocol_handler: false,
            fomod_post_download_mode: Some("warn-continue".into()),
            managed_deployment_enabled: false,
            active_profile_name: None,
        }
    }
}
impl DesktopSettings {
    pub fn decode(value: &Value) -> Result<Self> {
        let object = value.as_object().context("Settings must be an object")?;
        let mut canonical = serde_json::Map::new();
        for (key, value) in object {
            let Some(name) = KEYS.iter().find(|name| name.eq_ignore_ascii_case(key)) else {
                continue;
            };
            // Validate before collapsing case aliases: a bad earlier known field
            // invalidates the object just as the reference streaming decoder does.
            ensure!(
                if BOOL_KEYS.contains(name) {
                    value.is_boolean()
                } else {
                    value.is_string() || value.is_null()
                },
                "Settings {name} has an invalid type"
            );
            canonical.insert((*name).into(), value.clone());
        }
        serde_json::from_value(Value::Object(canonical))
            .map_err(|_| anyhow::anyhow!("Settings typed projection failed"))
    }
    pub fn effective_theme(&self) -> &str {
        self.theme.as_deref().unwrap_or("/Styles/LightStyle.axaml")
    }
    pub fn effective_file_encoding(&self) -> &str {
        self.file_encoding.as_deref().unwrap_or("utf-8")
    }
    pub fn effective_fomod_mode(&self) -> &str {
        self.fomod_post_download_mode
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("warn-continue")
    }
    /// This is a theme preference, not detection of an installed game build.
    pub fn target_game(&self) -> Option<&'static str> {
        let theme = self.effective_theme().to_ascii_lowercase();
        if theme.contains("lightstyle") || theme.is_empty() {
            None
        } else if theme.contains("kotor2") {
            Some("TSL")
        } else if theme.contains("kotor") {
            Some("K1")
        } else {
            None
        }
    }
    pub fn output_value(&self, reveal_secrets: bool) -> Result<Value> {
        Ok(super::redacted(
            &serde_json::to_value(self)?,
            reveal_secrets,
        ))
    }
}
pub struct DesktopSettingsReport {
    pub settings: DesktopSettings,
    pub used_defaults: bool,
    pub warnings: Vec<String>,
}
impl Store {
    fn desktop_root(&self) -> Result<Value> {
        storage::directory(&self.root, false)?;
        let current = self.root.join("settings.json");
        let path = match fs::symlink_metadata(&current) {
            Ok(_) => current,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !self.allow_legacy_fallback {
                    return Ok(serde_json::json!({}));
                }
                let Some(path) =
                    dirs::config_dir().map(|root| root.join("KOTORModSync/settings.json"))
                else {
                    return Ok(serde_json::json!({}));
                };
                match fs::symlink_metadata(&path) {
                    Ok(_) => path,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        return Ok(serde_json::json!({}));
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) => return Err(error.into()),
        };
        let bytes = storage::read(&path)?.context("Settings disappeared while reading")?;
        // Parsing diagnostics intentionally exclude authored values and secrets.
        let value: Value = serde_json::from_slice(without_bom(&bytes))
            .map_err(|_| anyhow::anyhow!("Settings contain invalid JSON"))?;
        ensure!(value.is_object(), "Settings must be an object");
        Ok(value)
    }
    pub fn desktop_settings(&self) -> DesktopSettingsReport {
        self.desktop_settings_with_patcher().0
    }
    /// Decode both projections from one raw snapshot; any invalid projection resets both.
    pub fn desktop_settings_with_patcher(&self) -> (DesktopSettingsReport, PatcherPreferences) {
        let decoded = self.desktop_root().and_then(|root| {
            Ok((
                DesktopSettings::decode(&root)?,
                PatcherPreferences::decode(&root)?,
            ))
        });
        match decoded {
            Ok((settings, patcher)) => (
                DesktopSettingsReport {
                    settings,
                    used_defaults: false,
                    warnings: Vec::new(),
                },
                patcher,
            ),
            Err(error) => (
                DesktopSettingsReport {
                    settings: DesktopSettings::default(),
                    used_defaults: true,
                    warnings: vec![format!(
                        "Could not load desktop settings; defaults are in use: {error:#}"
                    )],
                },
                PatcherPreferences {
                    engine: crate::PatcherEngine::HoloPatcher,
                    executable: None,
                },
            ),
        }
    }
    /// Merge only the confirmed protocol preference, retaining implicit legacy
    /// settings and concurrent unrelated fields under the shared settings lock.
    pub fn save_nxm_registration_preference(&self, enabled: bool) -> Result<()> {
        let _lock = self.lock()?;
        let mut root = self.desktop_root()?;
        let object = root.as_object_mut().context("Settings must be an object")?;
        object.retain(|key, _| !key.eq_ignore_ascii_case("registerNxmProtocolHandler"));
        object.insert("registerNxmProtocolHandler".into(), Value::Bool(enabled));
        self.write_desktop_root(&root)
    }
    /// Explicit whole-settings save: recognized fields are replaced; unknown
    /// application/extensions fields survive. Corrupt input is never overwritten.
    pub fn save_desktop_settings(&self, settings: &DesktopSettings) -> Result<()> {
        self.save_desktop_settings_inner(settings, false)
            .map(|_| ())
    }
    /// General preferences cannot replace a key concurrently saved by validation.
    /// Preserve the current stored key under the same settings write lock.
    pub fn save_desktop_settings_preserving_key(&self, settings: &DesktopSettings) -> Result<()> {
        self.save_desktop_settings_inner(settings, true).map(|_| ())
    }
    /// Return the exact persisted settings and engine-specific path under the same lock.
    pub fn save_desktop_settings_preserving_key_with_patcher(
        &self,
        settings: &DesktopSettings,
    ) -> Result<(DesktopSettings, PatcherPreferences)> {
        self.save_desktop_settings_inner(settings, true)
    }
    fn save_desktop_settings_inner(
        &self,
        settings: &DesktopSettings,
        preserve_key: bool,
    ) -> Result<(DesktopSettings, PatcherPreferences)> {
        let _lock = self.lock()?;
        let mut root = self.desktop_root()?;
        let values = serde_json::to_value(settings)?;
        let object = root.as_object_mut().context("Settings must be an object")?;
        object.retain(|key, _| {
            (preserve_key && key.eq_ignore_ascii_case("nexusModsApiKey"))
                || !KEYS.iter().any(|known| known.eq_ignore_ascii_case(key))
        });
        object.extend(
            values
                .as_object()
                .context("Typed settings must be an object")?
                .iter()
                .filter(|(key, _)| !preserve_key || !key.eq_ignore_ascii_case("nexusModsApiKey"))
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        let settings = DesktopSettings::decode(&root)?;
        let patcher = PatcherPreferences::decode(&root)?;
        self.write_desktop_root(&root)?;
        Ok((settings, patcher))
    }
    fn write_desktop_root(&self, root: &Value) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(root)?;
        ensure!(
            bytes.len() <= LIMIT,
            "Serialized settings exceed 16 MiB limit"
        );
        storage::write(&self.root.join("settings.json"), &bytes)
    }
    /// The validation request owns cancellation. A changed stored key while it
    /// runs is a stale operation and cannot be silently overwritten.
    pub fn save_validated_nexus_api_key(
        &self,
        key: &str,
        cancelled: &AtomicBool,
    ) -> Result<crate::updates::NexusValidateResult> {
        let key = key.trim();
        ensure!(!key.is_empty(), "Nexus API key cannot be blank");
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "Nexus key validation cancelled"
        );
        let before = key_values(&self.desktop_root()?);
        let mut client = crate::updates::NexusClient::new(Some(key.into()))?;
        let validation = client.validate_with_cancel(cancelled)?;
        ensure!(validation.is_valid, "Nexus API key validation failed");
        let _lock = self.lock()?;
        let mut root = self.desktop_root()?;
        ensure!(
            key_values(&root) == before,
            "Stored Nexus key changed during validation; refresh before saving"
        );
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "Nexus key validation cancelled"
        );
        let object = root.as_object_mut().context("Settings must be an object")?;
        object.retain(|name, _| !name.eq_ignore_ascii_case("nexusModsApiKey"));
        object.insert("nexusModsApiKey".into(), key.into());
        let bytes = serde_json::to_vec_pretty(&root)?;
        ensure!(
            bytes.len() <= LIMIT,
            "Serialized settings exceed 16 MiB limit"
        );
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "Nexus key validation cancelled"
        );
        storage::write(&self.root.join("settings.json"), &bytes)?;
        Ok(validation)
    }
}
fn key_values(root: &Value) -> Vec<(String, Value)> {
    root.as_object()
        .into_iter()
        .flatten()
        .filter(|(key, _)| key.eq_ignore_ascii_case("nexusModsApiKey"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}
