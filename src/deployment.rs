//! Managed deployment: staged per-component trees materialized into the game
//! directory by hardlink (copy fallback), recorded in C#-shaped manifests, and
//! undeployed by exactly what each manifest owns.
//!
//! Behavior follows `ModSync.Core.Services.Deployment.DeploymentService` with
//! these deliberate, fail-closed differences (see docs/parity/deployment-csharp-audit.md):
//! a component that already has a manifest is not redeployed over itself, the
//! staged tree is preflighted before the game directory changes, a mid-deploy
//! failure rolls back, symlinked targets are refused, existing case variants of
//! game paths are reused, out-of-order uninstall hands the original backup to the
//! later owner, and patcher provenance never records an overwritten file
//! without a backup of its prior bytes.

use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

/// Shown when managed deployment has no profile to resolve its roots from.
pub const MISSING_ACTIVE_PROFILE: &str = "Managed deployment is enabled but no active profile is loaded. Open Profiles, activate a profile, then try again.";

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
    #[serde(with = "net_utc")]
    pub deployed_utc: DateTime<Utc>,
    #[serde(default)]
    pub entries: Vec<Entry>,
}

/// Newtonsoft round-trip shape for a UTC `DateTime`: seven fractional digits
/// and a `Z` suffix. Reading also accepts RFC 3339 offsets and offset-less text.
mod net_utc {
    use super::*;
    pub fn serialize<S: Serializer>(
        value: &DateTime<Utc>,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!(
            "{}.{:07}Z",
            value.format("%Y-%m-%dT%H:%M:%S"),
            value.timestamp_subsec_nanos() / 100
        ))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<DateTime<Utc>, D::Error> {
        let text = String::deserialize(deserializer)?;
        if let Ok(value) = DateTime::parse_from_rfc3339(&text) {
            return Ok(value.with_timezone(&Utc));
        }
        NaiveDateTime::parse_from_str(&text, "%Y-%m-%dT%H:%M:%S%.f")
            .map(|value| value.and_utc())
            .map_err(serde::de::Error::custom)
    }
}

/// Relative game path (forward slashes, on-disk case) to lowercase SHA-256.
pub type HashIndex = BTreeMap<String, String>;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallReport {
    pub component: String,
    pub removed: bool,
    /// Deployed files deleted from the game directory.
    pub deleted: Vec<String>,
    /// Displaced originals moved back into the game directory.
    pub restored: Vec<String>,
    /// Files left in place because their bytes no longer match the manifest.
    pub skipped: Vec<String>,
    /// Paths a later deployment now owns; this component's backup moved to it.
    pub transferred: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceReport {
    pub recorded: Vec<String>,
    pub backed_up: Vec<String>,
    pub skipped: Vec<String>,
    pub warnings: Vec<String>,
}

pub struct Service {
    game: PathBuf,
    staging: PathBuf,
    root: PathBuf,
}

struct Planned {
    source: PathBuf,
    destination: PathBuf,
    relative: String,
    backup: Option<PathBuf>,
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

    /// Roots owned by one profile: `<settings>/profiles/<profile>/{staging,deployment}`,
    /// matching C# `ManagedInstallSession.TryCreate`.
    pub fn for_profile(settings: &Path, profile: &str, game: &Path) -> Result<Self> {
        let store = crate::profiles::Store::new(settings)?;
        ensure!(
            store.load(profile)?.is_some(),
            "Managed deployment requires profile '{}', but it was not found on disk.",
            profile.trim()
        );
        let artifacts = store.artifact_directory(profile.trim())?;
        let staging = artifacts.join("staging");
        let manifests = artifacts.join("deployment");
        fs::create_dir_all(&staging)?;
        fs::create_dir_all(&manifests)?;
        Self::new(game, &staging, &manifests)
    }

    /// Explicit staging and manifest roots win; otherwise the named or active profile supplies them.
    pub fn resolve(
        game: &Path,
        staging: Option<&Path>,
        manifests: Option<&Path>,
        settings: Option<&Path>,
        profile: Option<&str>,
    ) -> Result<Self> {
        match (staging, manifests) {
            (Some(staging), Some(manifests)) => {
                ensure!(
                    profile.is_none(),
                    "Use either --profile or --staging-dir with --manifest-dir, not both"
                );
                Self::new(game, staging, manifests)
            }
            (None, None) => {
                let settings = crate::profiles::settings_directory(settings)?;
                let profile = match profile.map(str::trim).filter(|name| !name.is_empty()) {
                    Some(name) => name.to_owned(),
                    None => crate::profiles::Store::new(&settings)?
                        .active_name()?
                        .filter(|name| !name.trim().is_empty())
                        .with_context(|| {
                            format!(
                                "{MISSING_ACTIVE_PROFILE} Pass --profile <name> (or activate a profile in Settings)."
                            )
                        })?,
                };
                Self::for_profile(&settings, &profile, game)
            }
            _ => bail!("--staging-dir and --manifest-dir must be given together"),
        }
    }

    pub fn game_directory(&self) -> &Path {
        &self.game
    }
    pub fn staging_root(&self) -> &Path {
        &self.staging
    }
    pub fn manifest_root(&self) -> &Path {
        &self.root
    }

    fn manifests(&self) -> PathBuf {
        self.root.join("manifests")
    }
    /// Earlier builds named files after the ID as typed, so an uppercase GUID
    /// file is still found after IDs were normalized to lowercase.
    fn backup(&self, id: &str) -> PathBuf {
        existing_ignoring_case(&self.root.join("backups"), id)
    }
    fn manifest_path(&self, id: &str) -> PathBuf {
        existing_ignoring_case(&self.manifests(), &format!("{id}.json"))
    }

    /// Manifests oldest first. Unreadable manifests are skipped, as in C#.
    pub fn deployed(&self) -> Result<Vec<Manifest>> {
        Ok(self.deployed_with_warnings()?.0)
    }

    /// Manifests oldest first, plus one warning per manifest that could not be read.
    pub fn deployed_with_warnings(&self) -> Result<(Vec<Manifest>, Vec<String>)> {
        let mut manifests = Vec::new();
        let mut warnings = Vec::new();
        for entry in fs::read_dir(self.manifests())? {
            let entry = entry?;
            if !entry.file_type()?.is_file() || entry.path().extension().is_none_or(|x| x != "json")
            {
                continue;
            }
            match fs::read(entry.path())
                .map_err(anyhow::Error::from)
                .and_then(|bytes| Ok(serde_json::from_slice::<Manifest>(&bytes)?))
            {
                Ok(manifest) => manifests.push(manifest),
                Err(error) => warnings.push(format!(
                    "Failed to read deployment manifest '{}': {error}",
                    entry.path().display()
                )),
            }
        }
        manifests.sort_by_key(|manifest| manifest.deployed_utc);
        Ok((manifests, warnings))
    }

    pub fn manifest(&self, component: &str) -> Result<Option<Manifest>> {
        let path = self.manifest_path(&component_id(component)?);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
    }
    fn save(&self, manifest: &Manifest) -> Result<()> {
        let path = self.manifest_path(&component_id(&manifest.component_guid)?);
        crate::recovery::atomic_write(&path, &serde_json::to_vec_pretty(manifest)?)
    }

    fn staged_directory(&self, component: &str, id: &str) -> Result<PathBuf> {
        let normalized = self.staging.join(id);
        if normalized.is_dir() {
            return Ok(normalized);
        }
        let raw = self.staging.join(component.trim());
        ensure!(
            valid_component(component.trim()).is_ok() && raw.is_dir(),
            "Staged component directory is unavailable: {}",
            normalized.display()
        );
        Ok(raw)
    }

    /// Lowercase relative path -> owning component name, for every manifest except `id`.
    fn foreign_index(&self, id: &str) -> Result<BTreeMap<String, String>> {
        let mut index = BTreeMap::new();
        for manifest in self.deployed()? {
            if manifest.component_guid.eq_ignore_ascii_case(id) {
                continue;
            }
            let owner = manifest
                .component_name
                .clone()
                .unwrap_or_else(|| manifest.component_guid.clone());
            for entry in &manifest.entries {
                index
                    .entry(entry.relative_path.to_lowercase())
                    .or_insert_with(|| owner.clone());
            }
        }
        Ok(index)
    }

    pub fn deploy(&self, component: &str, name: Option<String>) -> Result<Manifest> {
        Ok(self.deploy_report(component, name)?.0)
    }

    /// Deploy `staging/<component>`; returns the saved manifest and ownership warnings.
    pub fn deploy_report(
        &self,
        component: &str,
        name: Option<String>,
    ) -> Result<(Manifest, Vec<String>)> {
        let id = component_id(component)?;
        let staged = self.staged_directory(component, &id)?;
        ensure!(
            !self.manifest_path(&id).exists(),
            "Component {id} is already deployed. Uninstall it first so the files it displaced stay backed up."
        );
        let backup = self.backup(&id);
        let foreign = self.foreign_index(&id)?;
        let label = name.clone().unwrap_or_else(|| id.clone());
        let mut warnings = Vec::new();

        // Preflight the whole staged tree before the game directory changes.
        let mut plans = Vec::new();
        let mut seen = BTreeSet::new();
        for item in walkdir::WalkDir::new(&staged)
            .min_depth(1)
            .sort_by_file_name()
        {
            let item = item?;
            let kind = item.file_type();
            if kind.is_dir() {
                continue;
            }
            ensure!(
                !kind.is_symlink(),
                "Symlinks are unsupported in staged trees: {}",
                item.path().display()
            );
            ensure!(
                kind.is_file(),
                "Unsupported special file in staged tree: {}",
                item.path().display()
            );
            let raw = item
                .path()
                .strip_prefix(&staged)?
                .to_string_lossy()
                .replace('\\', "/");
            let (destination, relative) = resolve_case(&self.game, &relative_parts(&raw)?)?;
            ensure!(
                seen.insert(relative.to_lowercase()),
                "Staged files differ only by case and would both deploy to {relative}"
            );
            check_ancestors(&self.game, &relative)?;
            let backup_path = match fs::symlink_metadata(&destination) {
                Ok(meta) if meta.is_file() => {
                    let path = backup.join(relative_parts(&relative)?);
                    ensure!(
                        fs::symlink_metadata(&path).is_err(),
                        "An earlier backup for this component is still present at {}. Restore or remove it before deploying again.",
                        path.display()
                    );
                    Some(path)
                }
                Ok(_) => bail!(
                    "Deployment target is not a regular file: {}",
                    destination.display()
                ),
                Err(error) if error.kind() == ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            };
            if let Some(owner) = foreign.get(&relative.to_lowercase()) {
                warnings.push(format!(
                    "Conflict: '{relative}' from component '{label}' is already deployed by component '{owner}'. The new file will overwrite it."
                ));
            }
            plans.push(Planned {
                source: item.path().to_path_buf(),
                destination,
                relative,
                backup: backup_path,
            });
        }

        let mut manifest = Manifest {
            component_guid: id.clone(),
            component_name: name,
            deployed_utc: Utc::now(),
            entries: Vec::new(),
        };
        let mut done: Vec<&Planned> = Vec::new();
        for plan in &plans {
            match self.apply(plan) {
                Ok(entry) => {
                    manifest.entries.push(entry);
                    done.push(plan);
                }
                Err(error) => {
                    self.roll_back(&done, &backup);
                    return Err(error.context(format!(
                        "Deploy of {label} failed at {}; earlier files were rolled back",
                        plan.relative
                    )));
                }
            }
        }
        if let Err(error) = self.save(&manifest) {
            self.roll_back(&done, &backup);
            return Err(error.context("Deployment manifest could not be saved; rolled back"));
        }
        Ok((manifest, warnings))
    }

    fn apply(&self, plan: &Planned) -> Result<Entry> {
        let mut entry = Entry {
            relative_path: plan.relative.clone(),
            source_hash: hash(&plan.source)?,
            size: fs::metadata(&plan.source)?.len(),
            deployment_method: Method::Copy,
            overwrote_existing: false,
            backup_relative_path: None,
        };
        if let Some(backup) = &plan.backup {
            fs::create_dir_all(backup.parent().context("Backup path needs a parent")?)?;
            fs::rename(&plan.destination, backup)?;
            entry.overwrote_existing = true;
            entry.backup_relative_path = Some(plan.relative.clone());
        }
        let placed = (|| -> Result<Method> {
            fs::create_dir_all(
                plan.destination
                    .parent()
                    .context("Deployment path needs a parent")?,
            )?;
            if fs::hard_link(&plan.source, &plan.destination).is_ok() {
                return Ok(Method::Hardlink);
            }
            let mut input = fs::File::open(&plan.source)?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&plan.destination)?;
            std::io::copy(&mut input, &mut output)?;
            output.sync_all()?;
            fs::set_permissions(&plan.destination, fs::metadata(&plan.source)?.permissions())?;
            Ok(Method::Copy)
        })();
        match placed {
            Ok(method) => {
                entry.deployment_method = method;
                Ok(entry)
            }
            Err(error) => {
                let _ = fs::remove_file(&plan.destination);
                if let Some(backup) = &plan.backup {
                    let _ = fs::rename(backup, &plan.destination);
                }
                Err(error)
            }
        }
    }

    fn roll_back(&self, done: &[&Planned], backup: &Path) {
        for plan in done.iter().rev() {
            let _ = fs::remove_file(&plan.destination);
            if let Some(saved) = &plan.backup {
                let _ = fs::rename(saved, &plan.destination);
            }
            prune(&self.game, plan.destination.parent());
        }
        remove_empty_tree(backup);
    }

    /// Deploy what is staged so far for a component that may already have a
    /// manifest, without finishing it: the barrier before a Patcher, Execute or
    /// Run step, so the process sees the staged bytes in the live game tree.
    ///
    /// - With no manifest this is `deploy_report`.
    /// - A staged file the manifest does not own is deployed as in `deploy_report`
    ///   (a displaced game file is backed up).
    /// - An owned file whose staged bytes still equal the manifest hash is skipped.
    /// - An owned file whose staged bytes changed replaces the live file only when
    ///   the live bytes still equal the manifest hash; the entry keeps its original
    ///   backup. Anything else fails closed before the game tree changes.
    /// - Owned entries with no staged file stay untouched.
    ///
    /// The whole batch rolls back on failure and the manifest is saved once, with
    /// `DeployedUtc` moved to now so purge still unwinds it after earlier components.
    pub fn deploy_append(
        &self,
        component: &str,
        name: Option<String>,
    ) -> Result<(Manifest, Vec<String>)> {
        let id = component_id(component)?;
        let Some(mut manifest) = self.manifest(&id)? else {
            return self.deploy_report(component, name);
        };
        let staged = self.staged_directory(component, &id)?;
        let backup = self.backup(&id);
        let foreign = self.foreign_index(&id)?;
        let label = name
            .clone()
            .or_else(|| manifest.component_name.clone())
            .unwrap_or_else(|| id.clone());
        let owned: BTreeMap<String, usize> = manifest
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.relative_path.to_lowercase(), index))
            .collect();
        let mut warnings = Vec::new();

        // Preflight: (plan, owned entry index when it replaces a live owned file).
        let mut plans: Vec<(Planned, Option<usize>)> = Vec::new();
        let mut seen = BTreeSet::new();
        for item in walkdir::WalkDir::new(&staged)
            .min_depth(1)
            .sort_by_file_name()
        {
            let item = item?;
            let kind = item.file_type();
            if kind.is_dir() {
                continue;
            }
            ensure!(
                !kind.is_symlink(),
                "Symlinks are unsupported in staged trees: {}",
                item.path().display()
            );
            ensure!(
                kind.is_file(),
                "Unsupported special file in staged tree: {}",
                item.path().display()
            );
            let raw = item
                .path()
                .strip_prefix(&staged)?
                .to_string_lossy()
                .replace('\\', "/");
            let (destination, relative) = resolve_case(&self.game, &relative_parts(&raw)?)?;
            ensure!(
                seen.insert(relative.to_lowercase()),
                "Staged files differ only by case and would both deploy to {relative}"
            );
            check_ancestors(&self.game, &relative)?;
            if let Some(&index) = owned.get(&relative.to_lowercase()) {
                let entry = &manifest.entries[index];
                if hash(item.path())?.eq_ignore_ascii_case(&entry.source_hash) {
                    continue;
                }
                let live_matches = fs::symlink_metadata(&destination)
                    .is_ok_and(|meta| meta.is_file())
                    && hash(&destination)?.eq_ignore_ascii_case(&entry.source_hash);
                ensure!(
                    live_matches,
                    "'{relative}' is owned by {label} but the live file changed since it was deployed; refusing to replace it"
                );
                plans.push((
                    Planned {
                        source: item.path().to_path_buf(),
                        destination,
                        relative,
                        backup: None,
                    },
                    Some(index),
                ));
                continue;
            }
            let backup_path = match fs::symlink_metadata(&destination) {
                Ok(meta) if meta.is_file() => {
                    let path = backup.join(relative_parts(&relative)?);
                    ensure!(
                        fs::symlink_metadata(&path).is_err(),
                        "An earlier backup for this component is still present at {}. Restore or remove it before deploying again.",
                        path.display()
                    );
                    Some(path)
                }
                Ok(_) => bail!(
                    "Deployment target is not a regular file: {}",
                    destination.display()
                ),
                Err(error) if error.kind() == ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            };
            if let Some(owner) = foreign.get(&relative.to_lowercase()) {
                warnings.push(format!(
                    "Conflict: '{relative}' from component '{label}' is already deployed by component '{owner}'. The new file will overwrite it."
                ));
            }
            plans.push((
                Planned {
                    source: item.path().to_path_buf(),
                    destination,
                    relative,
                    backup: backup_path,
                },
                None,
            ));
        }

        // Replaced owned files are parked here so a failed batch can put them back.
        let parking = self.root.join("append-pending").join(&id);
        ensure!(
            fs::symlink_metadata(&parking).is_err(),
            "An interrupted append left files in {}. Move them back or remove them first.",
            parking.display()
        );
        let mut done: Vec<&Planned> = Vec::new();
        let mut parked: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut updates: Vec<(Option<usize>, Entry)> = Vec::new();
        let mut failure = None;
        for (plan, replaces) in &plans {
            if replaces.is_some() {
                let park = parking.join(relative_parts(&plan.relative)?);
                let moved = park
                    .parent()
                    .context("Parking path needs a parent")
                    .and_then(|parent| Ok(fs::create_dir_all(parent)?))
                    .and_then(|()| Ok(fs::rename(&plan.destination, &park)?));
                if let Err(error) = moved {
                    failure = Some((plan.relative.clone(), error));
                    break;
                }
                parked.push((park, plan.destination.clone()));
            }
            match self.apply(plan) {
                Ok(entry) => {
                    done.push(plan);
                    updates.push((*replaces, entry));
                }
                Err(error) => {
                    failure = Some((plan.relative.clone(), error));
                    break;
                }
            }
        }
        let undo = |parked: &[(PathBuf, PathBuf)]| {
            self.roll_back(&done, &backup);
            for (park, destination) in parked.iter().rev() {
                let _ = fs::remove_file(destination);
                if let Some(parent) = destination.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::rename(park, destination);
            }
            remove_empty_tree(&self.root.join("append-pending"));
        };
        if let Some((relative, error)) = failure {
            undo(&parked);
            return Err(error.context(format!(
                "Append deploy of {label} failed at {relative}; this batch was rolled back"
            )));
        }
        for (replaces, entry) in updates {
            match replaces {
                Some(index) => {
                    let old = &mut manifest.entries[index];
                    old.source_hash = entry.source_hash;
                    old.size = entry.size;
                    old.deployment_method = entry.deployment_method;
                }
                None => manifest.entries.push(entry),
            }
        }
        if name.as_deref().is_some_and(|name| !name.trim().is_empty()) {
            manifest.component_name = name;
        }
        manifest.deployed_utc = Utc::now();
        if let Err(error) = self.save(&manifest) {
            undo(&parked);
            return Err(
                error.context("Deployment manifest could not be saved; this batch was rolled back")
            );
        }
        let _ = fs::remove_dir_all(&parking);
        remove_empty_tree(&self.root.join("append-pending"));
        Ok((manifest, warnings))
    }

    /// Owned paths that would stop a clean uninstall: unsafe or non-file game
    /// paths, and changed files no later deployment has backed up. Read-only.
    pub fn reinstall_blockers(&self, component: &str) -> Result<Vec<String>> {
        let id = component_id(component)?;
        let Some(manifest) = self.manifest(&id)? else {
            return Ok(Vec::new());
        };
        let (mut others, _) = self.deployed_with_warnings()?;
        others.retain(|other| !other.component_guid.eq_ignore_ascii_case(&id));
        let mut blockers = Vec::new();
        for entry in &manifest.entries {
            let resolved = relative_parts(&entry.relative_path)
                .and_then(|parts| resolve_case(&self.game, &parts))
                .and_then(|(path, relative)| {
                    check_ancestors(&self.game, &relative)?;
                    Ok((path, relative))
                });
            let Ok((destination, relative)) = resolved else {
                blockers.push(format!("unsafe path '{}'", entry.relative_path));
                continue;
            };
            match fs::symlink_metadata(&destination) {
                Ok(meta) if meta.is_file() => {
                    if !hash(&destination)?.eq_ignore_ascii_case(&entry.source_hash)
                        && self
                            .later_owner(
                                &others,
                                manifest.deployed_utc,
                                &relative,
                                &entry.source_hash,
                            )
                            .is_none()
                    {
                        blockers.push(format!("'{relative}' was modified after deployment"));
                    }
                }
                Ok(_) => blockers.push(format!("'{relative}' is no longer a regular file")),
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(blockers)
    }

    /// Clear a component for a managed reinstall: refuse if any owned file would
    /// be skipped, otherwise uninstall it and empty its staging tree so the next
    /// install stages from scratch. With no manifest only the staging tree is
    /// cleared. The returned report is `removed == false` in that case.
    pub fn prepare_reinstall(&self, component: &str) -> Result<UninstallReport> {
        let id = component_id(component)?;
        let blockers = self.reinstall_blockers(&id)?;
        ensure!(
            blockers.is_empty(),
            "Cannot reinstall {id}: {}. Resolve these files, then retry; nothing was changed.",
            blockers.join("; ")
        );
        let report = if self.manifest(&id)?.is_some() {
            let report = self.uninstall_report(&id)?;
            ensure!(
                report.skipped.is_empty(),
                "Reinstall of {id} stopped: the game tree changed during uninstall and {} file(s) were left in place ({}). The manifest is gone; their originals remain under the component backup.",
                report.skipped.len(),
                report.skipped.join(", ")
            );
            report
        } else {
            UninstallReport {
                component: id.clone(),
                ..UninstallReport::default()
            }
        };
        if let Ok(staged) = self.staged_directory(component, &id)
            && fs::symlink_metadata(&staged).is_ok_and(|meta| meta.is_dir())
        {
            fs::remove_dir_all(&staged)?;
        }
        Ok(report)
    }

    pub fn uninstall(&self, component: &str) -> Result<bool> {
        Ok(self.uninstall_report(component)?.removed)
    }

    /// Remove exactly what the manifest owns, restoring displaced files. Files
    /// whose bytes changed stay; when a later deployment overwrote them, this
    /// component's backup moves to that deployment so its uninstall restores it.
    pub fn uninstall_report(&self, component: &str) -> Result<UninstallReport> {
        let id = component_id(component)?;
        let mut report = UninstallReport {
            component: id.clone(),
            ..UninstallReport::default()
        };
        let Some(manifest) = self.manifest(&id)? else {
            report.warnings.push(format!(
                "No deployment manifest found for component {id}; nothing to uninstall."
            ));
            return Ok(report);
        };
        let backup = self.backup(&id);
        let (mut others, _) = self.deployed_with_warnings()?;
        others.retain(|other| !other.component_guid.eq_ignore_ascii_case(&id));
        let mut changed_owners = BTreeSet::new();

        for entry in &manifest.entries {
            let resolved = relative_parts(&entry.relative_path)
                .and_then(|parts| resolve_case(&self.game, &parts))
                .and_then(|(path, relative)| {
                    check_ancestors(&self.game, &relative)?;
                    Ok((path, relative))
                });
            let Ok((destination, relative)) = resolved else {
                report.warnings.push(format!(
                    "Skipping unsafe relative path '{}' during uninstall of {id}.",
                    entry.relative_path
                ));
                continue;
            };
            let removed = match fs::symlink_metadata(&destination) {
                Ok(meta) if meta.is_file() => {
                    if hash(&destination)?.eq_ignore_ascii_case(&entry.source_hash) {
                        fs::remove_file(&destination)?;
                        report.deleted.push(relative.clone());
                        true
                    } else {
                        false
                    }
                }
                Ok(_) => {
                    report.warnings.push(format!(
                        "Skipping '{relative}': the game path is no longer a regular file."
                    ));
                    false
                }
                Err(error) if error.kind() == ErrorKind::NotFound => true,
                Err(error) => return Err(error.into()),
            };
            let saved = entry
                .overwrote_existing
                .then_some(entry.backup_relative_path.as_deref())
                .flatten()
                .and_then(|path| relative_parts(path).ok())
                .map(|path| backup.join(path))
                .filter(|path| fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file()));
            if removed {
                if let Some(saved) = saved {
                    fs::create_dir_all(destination.parent().context("Game path needs a parent")?)?;
                    fs::rename(&saved, &destination)?;
                    report.restored.push(relative);
                }
                prune(&self.game, destination.parent());
                continue;
            }
            if let Some((owner, index)) = self.later_owner(
                &others,
                manifest.deployed_utc,
                &relative,
                &entry.source_hash,
            ) {
                let owner_id = component_id(&others[owner].component_guid)?;
                let owner_entry = &mut others[owner].entries[index];
                let owner_backup = self.backup(&owner_id).join(relative_parts(
                    owner_entry
                        .backup_relative_path
                        .as_deref()
                        .unwrap_or_default(),
                )?);
                match saved {
                    Some(saved) => {
                        fs::remove_file(&owner_backup)?;
                        fs::rename(&saved, &owner_backup)?;
                    }
                    None => {
                        fs::remove_file(&owner_backup)?;
                        owner_entry.overwrote_existing = false;
                        owner_entry.backup_relative_path = None;
                        remove_empty_tree(&self.backup(&owner_id));
                    }
                }
                changed_owners.insert(owner);
                report.transferred.push(relative);
                continue;
            }
            report.skipped.push(relative.clone());
            report.warnings.push(format!(
                "Skipping '{relative}' during uninstall of {id}: file was modified after deployment (hash mismatch)."
            ));
            if let Some(saved) = saved {
                report.warnings.push(format!(
                    "Backup for '{relative}' was not restored because the deployed file was skipped; the backup remains at '{}'.",
                    saved.display()
                ));
            }
        }
        for owner in changed_owners {
            self.save(&others[owner])?;
        }
        fs::remove_file(self.manifest_path(&id))?;
        remove_empty_tree(&backup);
        report.removed = true;
        Ok(report)
    }

    /// The newest later deployment whose backup of `relative` holds exactly `hash`.
    fn later_owner(
        &self,
        others: &[Manifest],
        deployed: DateTime<Utc>,
        relative: &str,
        hash_value: &str,
    ) -> Option<(usize, usize)> {
        others
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, manifest)| manifest.deployed_utc > deployed)
            .find_map(|(owner, manifest)| {
                let index = manifest.entries.iter().position(|entry| {
                    entry.overwrote_existing && entry.relative_path.eq_ignore_ascii_case(relative)
                })?;
                let id = component_id(&manifest.component_guid).ok()?;
                let path = self.backup(&id).join(
                    relative_parts(manifest.entries[index].backup_relative_path.as_deref()?)
                        .ok()?,
                );
                (fs::symlink_metadata(&path).ok()?.is_file()
                    && hash(&path).ok()?.eq_ignore_ascii_case(hash_value))
                .then_some((owner, index))
            })
    }

    pub fn purge(&self) -> Result<usize> {
        Ok(self.purge_report()?.len())
    }

    /// MO2-style purge: uninstall every manifest, newest deployment first.
    pub fn purge_report(&self) -> Result<Vec<UninstallReport>> {
        let mut manifests = self.deployed()?;
        manifests.sort_by_key(|manifest| std::cmp::Reverse(manifest.deployed_utc));
        manifests
            .iter()
            .map(|manifest| self.uninstall_report(&manifest.component_guid))
            .collect()
    }

    /// Staged component directories that have no manifest.
    pub fn staged_not_deployed(&self) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in fs::read_dir(&self.staging)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type()?.is_dir()
                && component_id(&name).is_ok_and(|id| !self.manifest_path(&id).exists())
            {
                names.push(name);
            }
        }
        names.sort();
        Ok(names)
    }

    /// Hash every regular file under the game directory (C# `CaptureGameFileHashIndexAsync`).
    pub fn capture_index(&self) -> Result<HashIndex> {
        let mut index = HashIndex::new();
        for item in walkdir::WalkDir::new(&self.game).min_depth(1) {
            let item = item?;
            if item.file_type().is_file() {
                let relative = item
                    .path()
                    .strip_prefix(&self.game)?
                    .to_string_lossy()
                    .replace('\\', "/");
                index.insert(relative, hash(item.path())?);
            }
        }
        Ok(index)
    }

    /// Paths added or changed since `before`, compared without regard to case.
    pub fn diff_index(&self, before: &HashIndex) -> Result<Vec<String>> {
        let before: BTreeMap<String, &String> = before
            .iter()
            .map(|(path, hash)| (path.to_lowercase(), hash))
            .collect();
        Ok(self
            .capture_index()?
            .into_iter()
            .filter(|(path, hash)| {
                before
                    .get(&path.to_lowercase())
                    .is_none_or(|old| !old.eq_ignore_ascii_case(hash))
            })
            .map(|(path, _)| path)
            .collect())
    }

    /// Record live game files written outside staging (patcher output) into the
    /// component manifest. New files are recorded as-is. A file that existed in
    /// `before` is recorded only when `backup_source` (a tree mirroring the game
    /// directory before the patcher ran) holds bytes matching its old hash; those
    /// bytes become the entry's backup so uninstall restores them.
    pub fn record_live(
        &self,
        component: &str,
        name: Option<String>,
        before: &HashIndex,
        backup_source: Option<&Path>,
    ) -> Result<(Manifest, ProvenanceReport)> {
        let id = component_id(component)?;
        let mut report = ProvenanceReport::default();
        let existing = self.manifest(&id)?;
        let had_manifest = existing.is_some();
        let mut manifest = existing.unwrap_or_else(|| Manifest {
            component_guid: id.clone(),
            component_name: None,
            deployed_utc: Utc::now(),
            entries: Vec::new(),
        });
        if name.as_deref().is_some_and(|name| !name.trim().is_empty()) {
            manifest.component_name = name;
        }
        manifest.deployed_utc = Utc::now();
        let mut owned: BTreeSet<String> = manifest
            .entries
            .iter()
            .map(|entry| entry.relative_path.to_lowercase())
            .collect();
        let old: BTreeMap<String, &String> = before
            .iter()
            .map(|(path, hash)| (path.to_lowercase(), hash))
            .collect();
        for relative in self.diff_index(before)? {
            if !owned.insert(relative.to_lowercase()) {
                continue;
            }
            let parts = relative_parts(&relative)?;
            let path = self.game.join(&parts);
            if check_ancestors(&self.game, &relative).is_err()
                || !fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file())
            {
                continue;
            }
            let mut entry = Entry {
                relative_path: relative.clone(),
                source_hash: hash(&path)?,
                size: fs::metadata(&path)?.len(),
                deployment_method: Method::Copy,
                overwrote_existing: false,
                backup_relative_path: None,
            };
            if let Some(old_hash) = old.get(&relative.to_lowercase()) {
                let prior = backup_source.map(|root| root.join(&parts)).filter(|prior| {
                    fs::symlink_metadata(prior).is_ok_and(|meta| meta.is_file())
                        && hash(prior).is_ok_and(|bytes| bytes.eq_ignore_ascii_case(old_hash))
                });
                let Some(prior) = prior else {
                    report.skipped.push(relative.clone());
                    report.warnings.push(format!(
                        "'{relative}' existed before the patcher changed it and no matching prior copy was supplied; it is not recorded, so uninstall will leave it."
                    ));
                    continue;
                };
                let saved = self.backup(&id).join(&parts);
                ensure!(
                    fs::symlink_metadata(&saved).is_err(),
                    "A backup is already present at {}",
                    saved.display()
                );
                fs::create_dir_all(saved.parent().context("Backup path needs a parent")?)?;
                fs::copy(&prior, &saved)?;
                entry.overwrote_existing = true;
                entry.backup_relative_path = Some(relative.clone());
                report.backed_up.push(relative.clone());
            }
            report.recorded.push(relative);
            manifest.entries.push(entry);
        }
        if !report.recorded.is_empty() || !had_manifest {
            self.save(&manifest)?;
        }
        Ok((manifest, report))
    }
}

/// C# stores manifests as `Guid.ToString("D")`; GUID-shaped IDs are therefore
/// lowercased (and unbraced). Other IDs must be a single safe path segment.
fn component_id(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    let inner = trimmed
        .strip_prefix('{')
        .and_then(|text| text.strip_suffix('}'))
        .unwrap_or(trimmed);
    if is_guid(inner) {
        return Ok(inner.to_ascii_lowercase());
    }
    valid_component(raw)?;
    Ok(raw.to_owned())
}
fn is_guid(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(group, len)| group.len() == len && group.bytes().all(|b| b.is_ascii_hexdigit()))
}
fn valid_component(component: &str) -> Result<&str> {
    ensure!(
        !component.trim().is_empty()
            && component == component.trim()
            && !component.contains(['/', '\\', '.', '\0']),
        "Unsafe component ID"
    );
    Ok(component)
}

/// Confine a manifest or staged path: relative, no `..`, no NUL, no drive prefix.
/// Unlike instruction paths, filenames with brackets or trailing dots are allowed.
fn relative_parts(raw: &str) -> Result<PathBuf> {
    let text = raw.trim().replace('\\', "/");
    ensure!(!text.starts_with('/'), "Unsafe deployment path: {raw}");
    let mut path = PathBuf::new();
    for part in text.split('/').filter(|p| !p.is_empty() && *p != ".") {
        ensure!(
            part != ".." && !part.contains('\0') && !(cfg!(windows) && part.contains(':')),
            "Unsafe deployment path: {raw}"
        );
        path.push(part);
    }
    ensure!(!path.as_os_str().is_empty(), "Deployment path is empty");
    Ok(path)
}

/// Map each segment onto the existing entry of any case, so `override/x` lands
/// in an existing `Override/` on case-sensitive filesystems.
fn resolve_case(root: &Path, relative: &Path) -> Result<(PathBuf, String)> {
    let mut current = root.to_path_buf();
    let mut parts = Vec::new();
    let mut searching = true;
    for part in relative.iter() {
        let mut chosen: OsString = part.to_owned();
        if searching && fs::symlink_metadata(current.join(part)).is_err() {
            let wanted = part.to_string_lossy().to_lowercase();
            let mut found: Vec<OsString> = match fs::read_dir(&current) {
                Ok(entries) => entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.file_name())
                    .filter(|name| name.to_string_lossy().to_lowercase() == wanted)
                    .collect(),
                Err(_) => Vec::new(),
            };
            ensure!(
                found.len() <= 1,
                "Case-colliding entries for {} in {}",
                part.to_string_lossy(),
                current.display()
            );
            match found.pop() {
                Some(name) => chosen = name,
                None => searching = false,
            }
        }
        current.push(&chosen);
        parts.push(chosen.to_string_lossy().into_owned());
    }
    Ok((current, parts.join("/")))
}

/// Refuse symlinks and non-directories on the way to a target inside `root`.
fn check_ancestors(root: &Path, relative: &str) -> Result<()> {
    let parts = relative_parts(relative)?;
    let mut current = root.to_path_buf();
    let mut components = parts.iter().peekable();
    while let Some(part) = components.next() {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                bail!(
                    "Refusing to deploy through a symlink: {}",
                    current.display()
                )
            }
            Ok(meta) if components.peek().is_some() && !meta.is_dir() => {
                bail!("File blocks deployment directory: {}", current.display())
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn existing_ignoring_case(directory: &Path, name: &str) -> PathBuf {
    let exact = directory.join(name);
    if fs::symlink_metadata(&exact).is_ok() {
        return exact;
    }
    fs::read_dir(directory)
        .ok()
        .and_then(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name())
                .filter(|found| found.to_string_lossy().eq_ignore_ascii_case(name))
                .min()
        })
        .map_or(exact, |found| directory.join(found))
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
/// C# `DeleteDirectoryIfEmptyRecursive`: remove empty subdirectories, then the
/// directory itself if nothing is left. Symlinks are never followed.
fn remove_empty_tree(directory: &Path) {
    let Ok(meta) = fs::symlink_metadata(directory) else {
        return;
    };
    if !meta.is_dir() {
        return;
    }
    if let Ok(entries) = fs::read_dir(directory) {
        for entry in entries.filter_map(Result::ok) {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                remove_empty_tree(&entry.path());
            }
        }
    }
    let _ = fs::remove_dir(directory);
}
