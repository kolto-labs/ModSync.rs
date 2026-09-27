//! Persistent profiles shared by command-line and desktop callers.
use crate::{Guide, PatcherEngine, recovery};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

/// Same application directory name and platform base as the C# settings store.
pub fn settings_directory(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.to_owned());
    }
    dirs::config_dir()
        .map(|base| base.join("ModSync"))
        .context("Cannot locate user settings; supply --settings-dir")
}

fn setting_text(settings: &Value, key: &str) -> Result<Option<String>> {
    match settings
        .as_object()
        .unwrap()
        .iter()
        .rev()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v)
    {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        _ => anyhow::bail!("Settings {key} must be text or null"),
    }
}

/// `File.ReadAllText` strips a UTF-8 byte-order mark before the C# JSON readers see it.
fn without_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

/// Canonical GUID text as C# `Guid.ToString()` writes it (lowercase, hyphenated, no braces).
/// Values that are not GUIDs are returned unchanged.
pub fn normalize_guid(text: &str) -> String {
    let trimmed = text.trim();
    let inner = trimmed
        .strip_prefix('{')
        .and_then(|t| t.strip_suffix('}'))
        .or_else(|| trimmed.strip_prefix('(').and_then(|t| t.strip_suffix(')')))
        .unwrap_or(trimmed);
    let hex: String = inner.chars().filter(|c| *c != '-').collect();
    let hyphens_ok = !inner.contains('-')
        || (inner.len() == 36 && [8, 13, 18, 23].iter().all(|&i| inner.as_bytes()[i] == b'-'));
    if hex.len() != 32 || !hyphens_ok || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return text.to_owned();
    }
    let h = hex.to_ascii_lowercase();
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// The two fields `ModSyncSettings` reads: case-insensitive names, and a wrong type in
/// either resets both to defaults because the C# deserializer rejects the whole object.
fn managed_fields(settings: &Value) -> (bool, Option<String>) {
    let mut enabled = false;
    let mut active = None;
    for (key, value) in settings.as_object().into_iter().flatten() {
        if key.eq_ignore_ascii_case("managedDeploymentEnabled") {
            match value {
                Value::Bool(b) => enabled = *b,
                _ => return (false, None),
            }
        } else if key.eq_ignore_ascii_case("activeProfileName") {
            match value {
                Value::String(s) => active = Some(s.clone()),
                Value::Null => active = None,
                _ => return (false, None),
            }
        }
    }
    (enabled, active)
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
fn unset_date() -> String {
    "0001-01-01T00:00:00".into()
}
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Selection {
    pub is_selected: bool,
    pub selected_option_guids: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kotor_directory: Option<String>,
    #[serde(default)]
    pub mod_directory: Option<String>,
    #[serde(default)]
    pub instruction_file_path: Option<String>,
    #[serde(default)]
    pub component_selections: BTreeMap<String, Selection>,
    #[serde(default = "unset_date")]
    pub created_utc: String,
    #[serde(default = "unset_date")]
    pub last_used_utc: String,
}
impl Profile {
    pub fn apply(&self, guide: &mut Guide) {
        for c in &mut guide.components {
            if let Some((_, selection)) = self
                .component_selections
                .iter()
                .find(|(id, _)| normalize_guid(id).eq_ignore_ascii_case(&normalize_guid(&c.guid)))
            {
                c.is_selected = selection.is_selected;
                for option in &mut c.options {
                    option.is_selected = selection.selected_option_guids.iter().any(|id| {
                        normalize_guid(id).eq_ignore_ascii_case(&normalize_guid(&option.guid))
                    });
                }
            }
        }
    }
    pub fn cli_value(&self) -> Value {
        json!({"Name":self.name,"KotorDirectory":self.kotor_directory,"ModDirectory":self.mod_directory,"InstructionFilePath":self.instruction_file_path,
            "ComponentSelections":self.component_selections.iter().map(|(id,s)| (id.clone(),json!({"IsSelected":s.is_selected,"SelectedOptionGuids":s.selected_option_guids}))).collect::<serde_json::Map<_,_>>(),
            "CreatedUtc":self.created_utc,"LastUsedUtc":self.last_used_utc})
    }
}
pub fn sanitize(name: &str) -> Result<String> {
    ensure!(
        !name.trim().is_empty(),
        "Profile name cannot be null or whitespace"
    );
    let result: String = name
        .trim()
        .chars()
        .map(|c| {
            if c == '/'
                || c == '\\'
                || c == '\0'
                || (cfg!(windows) && (c < ' ' || "<>:\"|?*".contains(c)))
            {
                '_'
            } else {
                c
            }
        })
        .collect();
    ensure!(
        !result.chars().all(|c| c == '.' || c == '_'),
        "Profile name does not produce a usable filename"
    );
    Ok(result)
}
pub struct PatcherPreferences {
    pub engine: PatcherEngine,
    pub executable: Option<String>,
}

pub struct Store {
    root: PathBuf,
}
impl Store {
    pub fn new(root: &Path) -> Result<Self> {
        ensure!(
            !root.as_os_str().is_empty(),
            "Storage directory cannot be empty"
        );
        Ok(Self {
            root: std::path::absolute(root)?,
        })
    }
    pub fn directory(&self) -> PathBuf {
        self.root.join("profiles")
    }
    fn path(&self, name: &str) -> Result<PathBuf> {
        Ok(self.directory().join(format!("{}.json", sanitize(name)?)))
    }
    pub fn artifact_directory(&self, name: &str) -> Result<PathBuf> {
        Ok(self.directory().join(sanitize(name)?))
    }
    fn lock(&self) -> Result<File> {
        fs::create_dir_all(self.directory())?;
        let path = self.directory().join(".lock");
        if let Ok(meta) = fs::symlink_metadata(&path) {
            ensure!(!meta.file_type().is_symlink(), "Profile lock is a symlink");
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        fs2::FileExt::lock_exclusive(&file)?;
        Ok(file)
    }
    pub fn load(&self, name: &str) -> Result<Option<Profile>> {
        Ok(fs::read(self.path(name)?)
            .ok()
            .and_then(|bytes| serde_json::from_slice(without_bom(&bytes)).ok()))
    }
    pub fn require(&self, name: &str) -> Result<Profile> {
        self.load(name)?
            .with_context(|| format!("Profile '{name}' was not found"))
    }
    fn save_unlocked(&self, profile: &Profile) -> Result<()> {
        recovery::atomic_write(
            &self.path(&profile.name)?,
            &serde_json::to_vec_pretty(profile)?,
        )
    }
    pub fn save(&self, profile: &Profile) -> Result<()> {
        let _lock = self.lock()?;
        self.save_unlocked(profile)
    }
    pub fn list(&self) -> Result<Vec<Profile>> {
        let mut result = Vec::new();
        if !self.directory().exists() {
            return Ok(result);
        }
        for entry in fs::read_dir(self.directory())? {
            let path = entry?.path();
            if path.extension().is_some_and(|s| s == "json")
                && let Ok(bytes) = fs::read(path)
                && let Ok(profile) = serde_json::from_slice::<Profile>(without_bom(&bytes))
                && !profile.name.trim().is_empty()
            {
                result.push(profile);
            }
        }
        result.sort_by_key(|p| p.name.to_lowercase());
        Ok(result)
    }
    pub fn create(&self, name: &str) -> Result<Profile> {
        let _lock = self.lock()?;
        ensure!(
            !self.path(name)?.exists(),
            "A profile named '{name}' already exists"
        );
        let timestamp = now();
        let profile = Profile {
            name: name.trim().into(),
            kotor_directory: None,
            mod_directory: None,
            instruction_file_path: None,
            component_selections: BTreeMap::new(),
            created_utc: timestamp.clone(),
            last_used_utc: timestamp,
        };
        self.save_unlocked(&profile)?;
        Ok(profile)
    }
    pub fn clone_profile(&self, from: &str, to: &str) -> Result<Profile> {
        let _lock = self.lock()?;
        // The reference reports a missing source before a taken target name.
        let mut profile = self.require(from)?;
        ensure!(
            !self.path(to)?.exists(),
            "A profile named '{to}' already exists"
        );
        profile.name = to.trim().into();
        profile.created_utc = now();
        self.save_unlocked(&profile)?;
        Ok(profile)
    }
    pub fn rename(&self, from: &str, to: &str) -> Result<Profile> {
        let _lock = self.lock()?;
        let old = self.path(from)?;
        let new = self.path(to)?;
        ensure!(
            old == new || !new.exists(),
            "A profile named '{to}' already exists"
        );
        let mut profile = self.require(from)?;
        profile.name = to.trim().into();
        self.save_unlocked(&profile)?;
        if old != new {
            fs::remove_file(old)?;
        }
        Ok(profile)
    }
    pub fn delete(&self, name: &str) -> Result<bool> {
        let _lock = self.lock()?;
        let path = self.path(name)?;
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(path)?;
        Ok(true)
    }
    /// Desktop deletion also clears the active selection, preserving unrelated settings.
    pub fn delete_and_deactivate(&self, name: &str) -> Result<bool> {
        let _lock = self.lock()?;
        let path = self.path(name)?;
        if !path.exists() {
            return Ok(false);
        }
        let mut settings = self.settings()?;
        if managed_fields(&settings)
            .1
            .is_some_and(|active| active.eq_ignore_ascii_case(name))
        {
            for (key, value) in settings.as_object_mut().unwrap().iter_mut() {
                if key.eq_ignore_ascii_case("activeProfileName") {
                    *value = Value::Null;
                }
            }
            settings["activeProfileName"] = Value::Null;
            // Clear the setting first: a failed delete leaves a usable, inactive profile.
            recovery::atomic_write(
                &self.root.join("settings.json"),
                &serde_json::to_vec_pretty(&settings)?,
            )?;
        }
        fs::remove_file(path)?;
        Ok(true)
    }
    /// Settings as read for the two managed fields (activation, active name, deactivation).
    /// Like `ModSyncSettings.LoadFromDirectory`: a JSON `null` file reads as `{}`, and an
    /// unusable legacy file warns and yields defaults. Unusable current files are still
    /// refused so they are never replaced.
    fn settings(&self) -> Result<Value> {
        let current = self.root.join("settings.json");
        if current.exists() {
            let value: Value = serde_json::from_slice(without_bom(&fs::read(&current)?))?;
            return match value {
                Value::Null => Ok(json!({})),
                Value::Object(_) => Ok(value),
                _ => anyhow::bail!("Settings must be an object"),
            };
        }
        self.read_settings(true).or_else(|e| {
            eprintln!("Warning: failed to read legacy settings: {e:#}");
            Ok(json!({}))
        })
    }
    fn read_settings(&self, managed_only: bool) -> Result<Value> {
        let current = self.root.join("settings.json");
        let legacy = dirs::config_dir().map(|p| p.join("KOTORModSync/settings.json"));
        let path = if current.exists() {
            &current
        } else if let Some(path) = legacy.as_ref().filter(|p| p.is_file()) {
            path
        } else {
            return Ok(json!({}));
        };
        let mut value: Value = serde_json::from_slice(without_bom(&fs::read(path)?))?;
        ensure!(value.is_object(), "Settings must be an object");
        if path != &current && managed_only {
            // The C# profile CLI imports only its two managed-deployment fields.
            value.as_object_mut().unwrap().retain(|key, _| {
                key.eq_ignore_ascii_case("activeProfileName")
                    || key.eq_ignore_ascii_case("managedDeploymentEnabled")
            });
        }
        Ok(value)
    }
    pub fn directory_preferences(&self) -> Result<(Option<String>, Option<String>)> {
        let settings = self.read_settings(false)?;
        Ok((
            setting_text(&settings, "sourcePath")?,
            setting_text(&settings, "destinationPath")?,
        ))
    }
    pub fn nexus_api_key(&self) -> Result<Option<String>> {
        setting_text(&self.read_settings(false)?, "nexusModsApiKey")
    }
    pub fn save_nexus_api_key(&self, key: &str) -> Result<()> {
        let _lock = self.lock()?;
        let mut settings = self.read_settings(false)?;
        settings
            .as_object_mut()
            .unwrap()
            .retain(|name, _| !name.eq_ignore_ascii_case("nexusModsApiKey"));
        if !key.trim().is_empty() {
            settings["nexusModsApiKey"] = key.trim().into();
        }
        recovery::atomic_write(
            &self.root.join("settings.json"),
            &serde_json::to_vec_pretty(&settings)?,
        )
    }
    pub fn spoiler_free_preference(&self) -> Result<bool> {
        let settings = self.read_settings(false)?;
        match settings
            .as_object()
            .unwrap()
            .iter()
            .rev()
            .find(|(k, _)| k.eq_ignore_ascii_case("spoilerFreeMode"))
            .map(|(_, v)| v)
        {
            None => Ok(false),
            Some(Value::Bool(value)) => Ok(*value),
            _ => anyhow::bail!("Settings spoilerFreeMode must be a boolean"),
        }
    }
    pub fn save_spoiler_free_preference(&self, enabled: bool) -> Result<()> {
        let _lock = self.lock()?;
        let mut settings = self.read_settings(false)?;
        settings
            .as_object_mut()
            .unwrap()
            .retain(|k, _| !k.eq_ignore_ascii_case("spoilerFreeMode"));
        settings["spoilerFreeMode"] = enabled.into();
        recovery::atomic_write(
            &self.root.join("settings.json"),
            &serde_json::to_vec_pretty(&settings)?,
        )
    }
    pub fn patcher_preferences(&self) -> Result<PatcherPreferences> {
        let settings = self.read_settings(false)?;
        let engine = match setting_text(&settings, "patcherEngine")?
            .as_deref()
            .unwrap_or("Holopatcher")
            .to_ascii_lowercase()
            .as_str()
        {
            "holopatcher" => PatcherEngine::HoloPatcher,
            "kpatcher" => PatcherEngine::KPatcher,
            "odypatcher" => PatcherEngine::OdyPatcher,
            other => anyhow::bail!("Unsupported patcherEngine: {other}"),
        };
        let key = match engine {
            PatcherEngine::KPatcher => "kpatcherExecutablePath",
            PatcherEngine::OdyPatcher => "odyPatcherExecutablePath",
            PatcherEngine::HoloPatcher => "holopatcherExecutablePath",
        };
        Ok(PatcherPreferences {
            engine,
            executable: setting_text(&settings, key)?,
        })
    }
    pub fn save_patcher_preferences(&self, engine: PatcherEngine, executable: &str) -> Result<()> {
        let _lock = self.lock()?;
        let mut settings = self.read_settings(false)?;
        let (engine_name, key) = match engine {
            PatcherEngine::HoloPatcher => ("Holopatcher", "holopatcherExecutablePath"),
            PatcherEngine::KPatcher => ("KPatcher", "kpatcherExecutablePath"),
            PatcherEngine::OdyPatcher => ("OdyPatcher", "odyPatcherExecutablePath"),
        };
        settings.as_object_mut().unwrap().retain(|name, _| {
            !name.eq_ignore_ascii_case("patcherEngine") && !name.eq_ignore_ascii_case(key)
        });
        settings["patcherEngine"] = engine_name.into();
        if !executable.trim().is_empty() {
            settings[key] = executable.trim().into();
        }
        recovery::atomic_write(
            &self.root.join("settings.json"),
            &serde_json::to_vec_pretty(&settings)?,
        )
    }
    pub fn theme_preference(&self) -> Result<String> {
        Ok(setting_text(&self.read_settings(false)?, "theme")?
            .unwrap_or_else(|| "/Styles/LightStyle.axaml".into()))
    }
    pub fn save_theme_preference(&self, theme: &str) -> Result<()> {
        let _lock = self.lock()?;
        let mut settings = self.read_settings(false)?;
        settings
            .as_object_mut()
            .unwrap()
            .retain(|k, _| !k.eq_ignore_ascii_case("theme"));
        settings["theme"] = theme.into();
        recovery::atomic_write(
            &self.root.join("settings.json"),
            &serde_json::to_vec_pretty(&settings)?,
        )
    }

    pub fn save_directory_preferences(&self, source: &str, destination: &str) -> Result<()> {
        let _lock = self.lock()?;
        let mut settings = self.read_settings(false)?;
        settings.as_object_mut().unwrap().retain(|k, _| {
            !k.eq_ignore_ascii_case("sourcePath") && !k.eq_ignore_ascii_case("destinationPath")
        });
        settings["sourcePath"] = source.into();
        settings["destinationPath"] = destination.into();
        recovery::atomic_write(
            &self.root.join("settings.json"),
            &serde_json::to_vec_pretty(&settings)?,
        )
    }
    pub fn active_name(&self) -> Result<Option<String>> {
        Ok(managed_fields(&self.settings()?).1)
    }
    pub fn activate(&self, name: &str) -> Result<Profile> {
        let _lock = self.lock()?;
        let mut profile = self.require(name)?;
        // ModSyncSettings reads the typed fields from the current or legacy file, then
        // merges them into the current file only (a legacy file is never copied wholesale).
        let (enabled, _) = managed_fields(&self.settings()?);
        let mut settings = if self.root.join("settings.json").exists() {
            self.settings()?
        } else {
            json!({})
        };
        settings["managedDeploymentEnabled"] = enabled.into();
        settings["activeProfileName"] = profile.name.clone().into();
        profile.last_used_utc = now();
        self.save_unlocked(&profile)?;
        recovery::atomic_write(
            &self.root.join("settings.json"),
            &serde_json::to_vec_pretty(&settings)?,
        )?;
        Ok(profile)
    }
    pub fn capture(
        &self,
        name: &str,
        guide: &Guide,
        input: Option<&Path>,
        game: Option<&Path>,
        mods: Option<&Path>,
    ) -> Result<Profile> {
        let _lock = self.lock()?;
        let timestamp = now();
        let path_text = |p: Option<&Path>| -> Result<Option<String>> {
            p.map(|p| Ok(std::path::absolute(p)?.to_string_lossy().into_owned()))
                .transpose()
        };
        let mut profile = Profile {
            name: name.trim().into(),
            kotor_directory: path_text(game)?,
            mod_directory: path_text(mods)?,
            instruction_file_path: path_text(input)?,
            created_utc: self
                .load(name)?
                .map_or_else(|| timestamp.clone(), |p| p.created_utc),
            last_used_utc: timestamp,
            component_selections: BTreeMap::new(),
        };
        for c in &guide.components {
            profile.component_selections.insert(
                normalize_guid(&c.guid),
                Selection {
                    is_selected: c.is_selected,
                    selected_option_guids: c
                        .options
                        .iter()
                        .filter(|o| o.is_selected)
                        .map(|o| normalize_guid(&o.guid))
                        .collect(),
                },
            );
        }
        self.save_unlocked(&profile)?;
        Ok(profile)
    }
    pub fn apply(&self, name: &str, guide: &mut Guide) -> Result<Profile> {
        let _lock = self.lock()?;
        let mut profile = self.require(name)?;
        profile.apply(guide);
        profile.last_used_utc = now();
        self.save_unlocked(&profile)?;
        Ok(profile)
    }
}

/// Resolve explicit CLI paths over stored profile defaults without changing storage.
pub struct InstallPaths {
    pub input: PathBuf,
    pub game: PathBuf,
    pub source: PathBuf,
}
impl Profile {
    pub fn install_paths(
        &self,
        input: Option<&Path>,
        game: Option<&Path>,
        source: Option<&Path>,
    ) -> Result<InstallPaths> {
        fn resolve(explicit: Option<&Path>, stored: Option<&str>, flag: &str) -> Result<PathBuf> {
            explicit
                .map(Path::to_owned)
                .or_else(|| stored.filter(|s| !s.is_empty()).map(PathBuf::from))
                .with_context(|| format!("Profile has no {flag}; supply --{flag}"))
        }
        Ok(InstallPaths {
            input: resolve(input, self.instruction_file_path.as_deref(), "input")?,
            game: resolve(game, self.kotor_directory.as_deref(), "game-dir")?,
            source: resolve(source, self.mod_directory.as_deref(), "source-dir")?,
        })
    }
}
/// Keys whose values are masked in settings output unless secrets are revealed.
pub const SENSITIVE_KEYS: &[&str] = &["nexusModsApiKey"];

fn is_sensitive(key: &str) -> bool {
    SENSITIVE_KEYS.iter().any(|k| k.eq_ignore_ascii_case(key))
}

/// Clone settings for CLI/UI output without exposing stored credentials.
/// Every case variant is masked (the C# reference masks only the exact-case key and
/// prints a `NexusModsApiKey` value verbatim); a JSON null stays null, as in C#.
pub fn redacted(settings: &Value, reveal_secrets: bool) -> Value {
    let mut clone = settings.clone();
    if !reveal_secrets && let Some(root) = clone.as_object_mut() {
        for (key, value) in root.iter_mut() {
            if is_sensitive(key) && !value.is_null() {
                *value = "***".into();
            }
        }
    }
    clone
}

/// Whether `settings get` should mask this key's value.
pub fn is_sensitive_key(key: &str) -> bool {
    is_sensitive(key)
}

/// The `settings` verb's file, resolved like C# `SettingsFileStore`: a trimmed explicit
/// directory, otherwise the default directory. The legacy KOTORModSync file is never used,
/// because the reference passes the already-resolved directory to its path lookup.
pub struct SettingsFile {
    pub directory: PathBuf,
    pub path: PathBuf,
}

impl SettingsFile {
    pub fn resolve(explicit: Option<&Path>) -> Result<Self> {
        let explicit = explicit
            .map(|p| match p.to_str() {
                Some(text) => PathBuf::from(text.trim()),
                None => p.to_owned(),
            })
            .filter(|p| !p.as_os_str().is_empty());
        let directory = settings_directory(explicit.as_deref())?;
        Ok(Self {
            path: directory.join("settings.json"),
            directory,
        })
    }

    /// The stored object, `{}` when absent or JSON `null`. An unreadable, unparseable,
    /// empty or non-object file is an error carrying the reason.
    pub fn load(&self) -> Result<Value> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(json!({})),
            Err(e) => return Err(e.into()),
        };
        match serde_json::from_slice::<Value>(without_bom(&bytes))? {
            Value::Object(map) => Ok(Value::Object(map)),
            Value::Null => Ok(json!({})),
            _ => anyhow::bail!("The settings file must hold a JSON object"),
        }
    }

    /// Owner-only atomic replacement (0600 on Unix, like the reference `SaveRoot`).
    pub fn save(&self, root: &Value) -> Result<()> {
        ensure!(root.is_object(), "Settings must be an object");
        let path = std::path::absolute(&self.path)?;
        recovery::atomic_write(&path, &serde_json::to_vec_pretty(root)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}

/// `SettingsFileStore.SetValue`: empty or whitespace removes the exact-case key; anything
/// else is parsed like the reference CLI. Other keys keep their order.
pub fn set_cli_value(root: &mut Value, key: &str, raw: Option<&str>) -> Result<()> {
    let map = root.as_object_mut().context("Settings must be an object")?;
    match raw.filter(|r| !r.trim().is_empty()) {
        None => {
            map.shift_remove(key);
        }
        Some(raw) => {
            map.insert(key.to_owned(), parse_cli_value(raw)?);
        }
    }
    Ok(())
}

/// `SettingsFileStore.ParseCliValue`, matched against the pinned DLL on an en-US host:
/// booleans and null (any case), 64-bit integers, then .NET-style doubles (thousands
/// commas allowed, integral results written without a fraction), then JSON objects or
/// arrays, otherwise the trimmed text. NaN and out-of-range numbers fail like the
/// reference, which cannot serialize them.
pub fn parse_cli_value(raw: &str) -> Result<Value> {
    let t = raw.trim();
    if t.eq_ignore_ascii_case("true") {
        return Ok(true.into());
    }
    if t.eq_ignore_ascii_case("false") {
        return Ok(false.into());
    }
    if t.eq_ignore_ascii_case("null") {
        return Ok(Value::Null);
    }
    if let Ok(n) = t.parse::<i64>() {
        return Ok(n.into());
    }
    if t.eq_ignore_ascii_case("nan") || matches!(t, "\u{221e}" | "-\u{221e}" | "+\u{221e}") {
        anyhow::bail!("Non-finite numbers cannot be written as valid JSON");
    }
    if let Some(d) = dotnet_double(t) {
        ensure!(
            d.is_finite(),
            "Non-finite numbers cannot be written as valid JSON"
        );
        if d.fract() == 0.0 && d.abs() < 9_007_199_254_740_992.0 {
            return Ok((d as i64).into());
        }
        return Ok(serde_json::Number::from_f64(d)
            .context("Number is not representable")?
            .into());
    }
    if (t.starts_with('{') && t.ends_with('}')) || (t.starts_with('[') && t.ends_with(']')) {
        return serde_json::from_str(t).context("Invalid JSON value");
    }
    Ok(t.into())
}

/// `double.TryParse` with `NumberStyles.Float | AllowThousands` for an en-US culture.
fn dotnet_double(t: &str) -> Option<f64> {
    let body = t.strip_prefix(['+', '-']).unwrap_or(t);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let int_ok = int.is_empty()
        || (int.starts_with(|c: char| c.is_ascii_digit())
            && int.chars().all(|c| c.is_ascii_digit() || c == ','));
    let digits = int.chars().filter(char::is_ascii_digit).count() + frac.len();
    let exp_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && e.chars().all(|c| c.is_ascii_digit())
    });
    if !int_ok || digits == 0 || !frac.chars().all(|c| c.is_ascii_digit()) || !exp_ok {
        return None;
    }
    t.replace(',', "").parse().ok()
}

/// `FormatSettingsValueForText`: raw strings, lowercase booleans, empty for null,
/// otherwise compact JSON.
pub fn settings_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}
