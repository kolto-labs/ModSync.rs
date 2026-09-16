//! Persistent profiles shared by command-line and desktop callers.
use crate::{Guide, recovery};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

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
                .find(|(id, _)| id.eq_ignore_ascii_case(&c.guid))
            {
                c.is_selected = selection.is_selected;
                for option in &mut c.options {
                    option.is_selected = selection
                        .selected_option_guids
                        .iter()
                        .any(|id| id.eq_ignore_ascii_case(&option.guid));
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
            .and_then(|bytes| serde_json::from_slice(&bytes).ok()))
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
                && let Ok(profile) = serde_json::from_slice::<Profile>(&bytes)
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
        ensure!(
            !self.path(to)?.exists(),
            "A profile named '{to}' already exists"
        );
        let mut profile = self.require(from)?;
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
    fn settings(&self) -> Result<Value> {
        let path = self.root.join("settings.json");
        if !path.exists() {
            return Ok(json!({}));
        }
        let value: Value = serde_json::from_slice(&fs::read(path)?)?;
        ensure!(value.is_object(), "Settings must be an object");
        Ok(value)
    }
    pub fn active_name(&self) -> Result<Option<String>> {
        Ok(self
            .settings()?
            .get("activeProfileName")
            .and_then(Value::as_str)
            .map(str::to_owned))
    }
    pub fn activate(&self, name: &str) -> Result<Profile> {
        let _lock = self.lock()?;
        let mut profile = self.require(name)?;
        let mut settings = self.settings()?;
        settings["activeProfileName"] = profile.name.clone().into();
        settings
            .as_object_mut()
            .unwrap()
            .entry("managedDeploymentEnabled")
            .or_insert(false.into());
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
                c.guid.clone(),
                Selection {
                    is_selected: c.is_selected,
                    selected_option_guids: c
                        .options
                        .iter()
                        .filter(|o| o.is_selected)
                        .map(|o| o.guid.clone())
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
