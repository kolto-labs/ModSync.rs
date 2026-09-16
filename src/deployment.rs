use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Method {
    Hardlink,
    Copy,
}
impl Serialize for Method {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_u8(match self {
            Self::Hardlink => 0,
            Self::Copy => 1,
        })
    }
}
impl<'de> Deserialize<'de> for Method {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        match u8::deserialize(deserializer)? {
            0 => Ok(Self::Hardlink),
            1 => Ok(Self::Copy),
            other => Err(serde::de::Error::custom(format!(
                "Unknown deployment method {other}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Entry {
    pub relative_path: String,
    pub source_hash: String,
    pub size: u64,
    pub deployment_method: Method,
    pub overwrote_existing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_relative_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Manifest {
    pub component_guid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_name: Option<String>,
    pub deployed_utc: DateTime<Utc>,
    #[serde(default)]
    pub entries: Vec<Entry>,
}

pub struct Service {
    game: PathBuf,
    staging: PathBuf,
    root: PathBuf,
}

impl Service {
    pub fn new(game: &Path, staging: &Path, root: &Path) -> Result<Self> {
        let game = fs::canonicalize(game).context("Game directory is unavailable")?;
        let staging = fs::canonicalize(staging).context("Staging directory is unavailable")?;
        ensure!(
            game.is_dir() && staging.is_dir(),
            "Game and staging paths must be directories"
        );
        let root = std::path::absolute(root)?;
        fs::create_dir_all(root.join("manifests"))?;
        fs::create_dir_all(root.join("backups"))?;
        Ok(Self {
            game,
            staging,
            root,
        })
    }

    fn manifests(&self) -> PathBuf {
        self.root.join("manifests")
    }
    fn backup(&self, component: &str) -> PathBuf {
        self.root.join("backups").join(component)
    }
    fn manifest_path(&self, component: &str) -> Result<PathBuf> {
        valid_component(component)?;
        Ok(self.manifests().join(format!("{component}.json")))
    }
    fn confined(root: &Path, relative: &str) -> Result<PathBuf> {
        let path = crate::paths::relative(relative, false)?;
        ensure!(!path.as_os_str().is_empty(), "Deployment path is empty");
        Ok(root.join(path))
    }

    pub fn deployed(&self) -> Result<Vec<Manifest>> {
        let mut manifests = Vec::new();
        for entry in fs::read_dir(self.manifests())? {
            let entry = entry?;
            if entry.file_type()?.is_file() && entry.path().extension().is_some_and(|x| x == "json")
            {
                manifests.push(serde_json::from_slice::<Manifest>(&fs::read(
                    entry.path(),
                )?)?);
            }
        }
        manifests.sort_by_key(|manifest| manifest.deployed_utc);
        Ok(manifests)
    }
    pub fn manifest(&self, component: &str) -> Result<Option<Manifest>> {
        let path = self.manifest_path(component)?;
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
    }
    fn save(&self, manifest: &Manifest) -> Result<()> {
        let path = self.manifest_path(&manifest.component_guid)?;
        crate::recovery::atomic_write(&path, &serde_json::to_vec_pretty(manifest)?)
    }

    pub fn deploy(&self, component: &str, name: Option<String>) -> Result<Manifest> {
        let staged = self.staging.join(valid_component(component)?);
        ensure!(
            staged.is_dir(),
            "Staged component directory is unavailable: {}",
            staged.display()
        );
        let backup = self.backup(component);
        let mut manifest = Manifest {
            component_guid: component.into(),
            component_name: name,
            deployed_utc: Utc::now(),
            entries: Vec::new(),
        };
        for item in walkdir::WalkDir::new(&staged)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|item| item.file_type().is_file())
        {
            let relative = item
                .path()
                .strip_prefix(&staged)?
                .to_string_lossy()
                .replace('\\', "/");
            let destination = Self::confined(&self.game, &relative)?;
            let mut entry = Entry {
                relative_path: relative.clone(),
                source_hash: hash(item.path())?,
                size: item.metadata()?.len(),
                deployment_method: Method::Copy,
                overwrote_existing: false,
                backup_relative_path: None,
            };
            if destination.exists() {
                let backup_path = Self::confined(&backup, &relative)?;
                if let Some(parent) = backup_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                if backup_path.exists() {
                    fs::remove_file(&backup_path)?;
                }
                fs::rename(&destination, &backup_path)?;
                entry.overwrote_existing = true;
                entry.backup_relative_path = Some(relative.clone());
            }
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            if fs::hard_link(item.path(), &destination).is_ok() {
                entry.deployment_method = Method::Hardlink;
            } else {
                fs::copy(item.path(), &destination)?;
            }
            manifest.entries.push(entry);
        }
        self.save(&manifest)?;
        Ok(manifest)
    }

    pub fn uninstall(&self, component: &str) -> Result<bool> {
        let Some(manifest) = self.manifest(component)? else {
            return Ok(false);
        };
        let backup = self.backup(component);
        for entry in &manifest.entries {
            let destination = match Self::confined(&self.game, &entry.relative_path) {
                Ok(path) => path,
                Err(_) => continue,
            };
            let removed = if destination.exists() {
                if hash(&destination)? != entry.source_hash {
                    false
                } else {
                    fs::remove_file(&destination)?;
                    true
                }
            } else {
                true
            };
            if removed {
                if let Some(relative) = &entry.backup_relative_path
                    && let Ok(old) = Self::confined(&backup, relative)
                    && old.exists()
                {
                    if let Some(parent) = destination.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::rename(old, &destination)?;
                }
                prune(&self.game, destination.parent());
            }
        }
        fs::remove_file(self.manifest_path(component)?)?;
        prune(&self.root.join("backups"), Some(&backup));
        Ok(true)
    }

    pub fn purge(&self) -> Result<usize> {
        let mut manifests = self.deployed()?;
        manifests.sort_by_key(|manifest| std::cmp::Reverse(manifest.deployed_utc));
        let count = manifests.len();
        for manifest in manifests {
            self.uninstall(&manifest.component_guid)?;
        }
        Ok(count)
    }
}

fn valid_component(component: &str) -> Result<&str> {
    ensure!(
        !component.trim().is_empty()
            && component == component.trim()
            && !component.contains(['/', '\\', '.']),
        "Unsafe component ID"
    );
    Ok(component)
}
fn hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    std::io::copy(&mut file, &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}
fn prune(root: &Path, mut current: Option<&Path>) {
    while let Some(path) = current {
        if path == root
            || !path.starts_with(root)
            || fs::read_dir(path).map_or(true, |mut entries| entries.next().is_some())
        {
            return;
        }
        if fs::remove_dir(path).is_err() {
            return;
        }
        current = path.parent();
    }
}
