//! Library housekeeping on top of managed deployment: removing a turned-off
//! component's staged copy, and finding the same mod deployed more than once
//! under different component identities (for example after a guide update
//! changed its GUID).
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    /// The shared display name.
    pub name: String,
    /// The most recently deployed copy, which is kept.
    pub keep: String,
    /// Older copies of the same mod, oldest first.
    pub older: Vec<String>,
}

impl Service {
    /// Delete the staged copy of a component that is not deployed, so it leaves
    /// the library. A deployed component must be uninstalled first; the game
    /// directory is never touched here.
    pub fn remove_from_library(&self, component: &str) -> Result<bool> {
        let _lock = self.lock()?;
        self.checkpoint_mutation_gate()?;
        let id = component_id(component)?;
        ensure!(
            !self.manifest_path(&id).exists(),
            "Turn this mod off before removing it from the library"
        );
        let staged = self.staging.join(&id);
        match fs::symlink_metadata(&staged) {
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
            Ok(metadata) => ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "The staged copy is not an ordinary folder: {}",
                staged.display()
            ),
        }
        ensure!(
            staged.parent() == Some(self.staging.as_path()),
            "Staged copy is outside the staging folder"
        );
        fs::remove_dir_all(&staged)
            .with_context(|| format!("Could not remove {}", staged.display()))?;
        Ok(true)
    }

    /// Deployed components that share a display name with another deployed
    /// component. Read-only apart from the ordinary game lock file.
    pub fn duplicates(&self) -> Result<Vec<DuplicateGroup>> {
        let status = self.management_status()?;
        Ok(duplicate_groups(&status.manifests))
    }
}

/// Group `manifests` (oldest first) by case-insensitive display name. A
/// manifest without a name never matches another.
pub fn duplicate_groups(manifests: &[Manifest]) -> Vec<DuplicateGroup> {
    let mut groups: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    for manifest in manifests {
        let Some(name) = manifest
            .component_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
        else {
            continue;
        };
        let slot = groups
            .entry(crate::unicode_case::ordinal_key(name))
            .or_insert_with(|| (name.to_owned(), Vec::new()));
        if !slot
            .1
            .iter()
            .any(|guid| guid.eq_ignore_ascii_case(&manifest.component_guid))
        {
            slot.1.push(manifest.component_guid.clone());
        }
    }
    groups
        .into_values()
        .filter(|(_, guids)| guids.len() > 1)
        .map(|(name, mut guids)| {
            let keep = guids.pop().expect("two or more copies");
            DuplicateGroup {
                name,
                keep,
                older: guids,
            }
        })
        .collect()
}
