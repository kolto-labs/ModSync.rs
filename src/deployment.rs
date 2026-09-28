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
    /// Paths this component already owned whose patcher output replaced the recorded bytes.
    pub updated: Vec<String>,
    pub backed_up: Vec<String>,
    pub skipped: Vec<String>,
    pub warnings: Vec<String>,
}

pub struct Service {
    game: PathBuf,
    staging: PathBuf,
    root: PathBuf,
    /// Other profiles' manifest roots (name, root) that must not be live on this game.
    peers: Vec<(String, PathBuf)>,
}

/// Exclusive hold on the per-game ModSync lock file. Released on drop.
pub struct GameLock {
    _file: fs::File,
}

/// Records which game directory a manifest root deploys into.
const GAME_MARKER: &str = "game-directory";

/// Path of the game-scoped owner record: which manifest root currently owns
/// this game directory, regardless of whether that root was reached through
/// a profile (peers, checked in `ensure_exclusive_game`) or an explicit
/// `--manifest-dir`/`--staging-dir` pair (no peers to check). Lives next to
/// the per-game `.modsync-lock-<hash>` file, same hash scheme as
/// `Service::lock` and `recovery::Lock::acquire`.
fn owner_record_path(game: &Path) -> Result<PathBuf> {
    let key = format!("{:x}", Sha256::digest(game.to_string_lossy().as_bytes()));
    Ok(game
        .parent()
        .context("Game directory needs a parent")?
        .join(format!(".modsync-owner-{}", &key[..16])))
}

struct Planned {
    source: PathBuf,
    destination: PathBuf,
    relative: String,
    /// Hashed at preflight, not at apply time, so the work between the
    /// mutation-time `check_ancestors` re-check and the actual write in
    /// `place` stays as small as possible (narrows the TOCTOU window).
    source_hash: String,
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
            peers: Vec::new(),
        })
    }

    /// Take the same `.modsync-lock-<hash>` file `recovery::Lock` takes for this
    /// game directory, so a deployment command cannot run beside another
    /// ModSync install or deployment on the same game tree. The engine already
    /// holds this lock during an install and must not call this.
    pub fn lock(&self) -> Result<GameLock> {
        let key = format!(
            "{:x}",
            Sha256::digest(self.game.to_string_lossy().as_bytes())
        );
        let path = self
            .game
            .parent()
            .context("Game directory needs a parent")?
            .join(format!(".modsync-lock-{}", &key[..16]));
        if let Ok(meta) = fs::symlink_metadata(&path) {
            ensure!(!meta.file_type().is_symlink(), "Lock file is a symlink");
        }
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)?;
        fs2::FileExt::try_lock_exclusive(&file)
            .context("Another ModSync process is using the game directory")?;
        Ok(GameLock { _file: file })
    }

    /// Refuse to add deployments while another profile still has manifests in
    /// this game directory (or in a root that does not say which game it
    /// targets). Two live roots cannot share ownership of one game tree.
    pub fn ensure_exclusive_game(&self) -> Result<()> {
        self.ensure_own_game()?;
        for (name, root) in &self.peers {
            if !has_manifests(root) {
                continue;
            }
            let same = marker_game(root).is_none_or(|game| game == self.game);
            ensure!(
                !same,
                "Profile '{name}' still has managed deployments in this game directory. Purge them before deploying from another profile; two live profiles cannot share one game tree."
            );
        }
        self.ensure_unowned_or_self()?;
        Ok(())
    }

    /// Refuse when a *different* manifest root (reached without going through
    /// `for_profile`, e.g. an explicit `--manifest-dir`) still owns this game
    /// directory. `peers` above only sees sibling profile roots under one
    /// settings directory; this check is root-agnostic, so two explicit roots
    /// pointed at the same game tree cannot both go live.
    fn ensure_unowned_or_self(&self) -> Result<()> {
        let path = owner_record_path(&self.game)?;
        if let Ok(meta) = fs::symlink_metadata(&path) {
            ensure!(!meta.file_type().is_symlink(), "Owner record is a symlink");
        }
        let Ok(recorded) = fs::read_to_string(&path) else {
            return Ok(());
        };
        let recorded = recorded.trim();
        if recorded.is_empty() {
            return Ok(());
        }
        let owner = PathBuf::from(recorded);
        let root = fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone());
        if owner != root && has_manifests(&owner) {
            ensure!(
                marker_game(&owner).is_none_or(|game| game != self.game),
                "Manifest root {} still has managed deployments in this game directory. Purge them there first; two live manifest roots cannot share one game tree.",
                owner.display()
            );
        }
        Ok(())
    }

    /// Record this root as the current owner of its game directory, so a
    /// later, unrelated manifest root pointed at the same game tree is
    /// refused by `ensure_unowned_or_self` even with no shared peer list.
    fn claim_owner_record(&self) -> Result<()> {
        let path = owner_record_path(&self.game)?;
        let root = fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone());
        let text = root.to_string_lossy().into_owned();
        if fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            crate::recovery::atomic_write(&path, text.as_bytes())?;
        }
        Ok(())
    }

    /// Release this root's claim on the owner record once it has no
    /// manifests left, so another root may deploy into this game directory.
    fn release_owner_record_if_empty(&self) -> Result<()> {
        if has_manifests(&self.root) {
            return Ok(());
        }
        let path = owner_record_path(&self.game)?;
        let Ok(recorded) = fs::read_to_string(&path) else {
            return Ok(());
        };
        let owner = PathBuf::from(recorded.trim());
        let root = fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone());
        if owner == root {
            let _ = fs::remove_file(&path);
        }
        Ok(())
    }

    /// Refuse to touch this root's manifests against a game directory other
    /// than the one they were deployed into. A root with no marker predates it
    /// and is accepted only when every owned entry still hash-matches a live
    /// file here; the next save records this game.
    ///
    /// Also opportunistically claims the game-directory owner record for a
    /// root that already has manifests and confirmed it belongs here, when
    /// that record is currently absent or free. A root deployed by an older
    /// build (or before another root released its own manifests) may have no
    /// record at all; without this, `ensure_unowned_or_self` would treat
    /// "absent" as free and let a second explicit root deploy over it. This
    /// runs under the caller's game lock, same as every mutating command.
    pub fn ensure_own_game(&self) -> Result<()> {
        if !has_manifests(&self.root) {
            return Ok(());
        }
        match marker_game(&self.root) {
            Some(game) => ensure!(
                game == self.game,
                "These deployments belong to game directory {}, not {}. Use that game directory, or purge them there first.",
                game.display(),
                self.game.display()
            ),
            // No marker: this root predates it (see `write_marker`). Blindly
            // accepting it let a marker-less root move originals into the
            // wrong game tree, so every owned entry (across every manifest)
            // must still hash-match a live file in *this* game directory;
            // a missing or mismatched entry, or no entries at all, refuses.
            None => {
                let manifests = self.deployed()?;
                let mut total = 0usize;
                let mut mismatch: Option<String> = None;
                'entries: for manifest in &manifests {
                    for entry in &manifest.entries {
                        total += 1;
                        let matches = self
                            .locate(&entry.relative_path)
                            .ok()
                            .flatten()
                            .is_some_and(|(destination, _)| {
                                hash(&destination).is_ok_and(|bytes| {
                                    bytes.eq_ignore_ascii_case(&entry.source_hash)
                                })
                            });
                        if !matches {
                            mismatch = Some(entry.relative_path.clone());
                            break 'entries;
                        }
                    }
                }
                ensure!(
                    total > 0 && mismatch.is_none(),
                    "This manifest root has no game-directory marker and {}; it likely belongs to a different game directory. Use the correct game directory, or purge these manifests there first.",
                    match mismatch {
                        Some(path) => format!(
                            "its entry '{path}' does not match a file in {}",
                            self.game.display()
                        ),
                        None => format!(
                            "has no owned entries to confirm it belongs to {}",
                            self.game.display()
                        ),
                    }
                );
            }
        }
        self.adopt_owner_record_if_unowned()?;
        Ok(())
    }

    /// Claim the game-directory owner record for this root when it is
    /// currently unclaimed (no record, the record already names this root, or
    /// the recorded owner has no manifests left). Called only after
    /// `ensure_own_game` has confirmed this root belongs to `self.game`.
    fn adopt_owner_record_if_unowned(&self) -> Result<()> {
        let path = owner_record_path(&self.game)?;
        if let Ok(meta) = fs::symlink_metadata(&path) {
            ensure!(!meta.file_type().is_symlink(), "Owner record is a symlink");
        }
        let recorded = fs::read_to_string(&path).ok();
        let recorded = recorded.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let root = fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone());
        let unowned = match recorded {
            None => true,
            Some(text) => {
                let owner = PathBuf::from(text);
                owner == root || !has_manifests(&owner)
            }
        };
        if unowned {
            self.claim_owner_record()?;
        }
        Ok(())
    }

    fn write_marker(&self) -> Result<()> {
        let path = self.root.join(GAME_MARKER);
        let game = self.game.to_string_lossy();
        if fs::read_to_string(&path).ok().as_deref() != Some(game.as_ref()) {
            crate::recovery::atomic_write(&path, game.as_bytes())?;
        }
        self.claim_owner_record()?;
        Ok(())
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
        let mut service = Self::new(game, &staging, &manifests)?;
        for entry in fs::read_dir(store.directory())? {
            let entry = entry?;
            if entry.file_type()?.is_dir() && entry.path() != artifacts {
                service.peers.push((
                    entry.file_name().to_string_lossy().into_owned(),
                    entry.path().join("deployment"),
                ));
            }
        }
        Ok(service)
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
        self.write_marker()?;
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

    /// Lowercase relative path -> (newest owning component name, its `DeployedUtc`),
    /// for every manifest except `id`.
    fn foreign_index(&self, id: &str) -> Result<BTreeMap<String, (String, DateTime<Utc>)>> {
        let mut index = BTreeMap::new();
        // Oldest first, so a newer owner replaces an older one.
        for manifest in self.deployed()? {
            if manifest.component_guid.eq_ignore_ascii_case(id) {
                continue;
            }
            let owner = manifest
                .component_name
                .clone()
                .unwrap_or_else(|| manifest.component_guid.clone());
            for entry in &manifest.entries {
                index.insert(
                    entry.relative_path.to_lowercase(),
                    (owner.clone(), manifest.deployed_utc),
                );
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
        // Read before `ensure_exclusive_game` may adopt it (see
        // `adopt_owner_record_if_unowned`) or before the first successful
        // save claims it below, so the migration-limit warning below can
        // tell "this call is the one establishing ownership" from "ownership
        // was already recorded". See docs/parity/deployment-csharp-audit.md
        // for why this stays a warning rather than a refusal.
        let record_existed = owner_record_path(&self.game)
            .is_ok_and(|path| fs::read_to_string(path).is_ok_and(|text| !text.trim().is_empty()));
        self.ensure_exclusive_game()?;
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
            if let Some((owner, _)) = foreign.get(&relative.to_lowercase()) {
                warnings.push(format!(
                    "Conflict: '{relative}' from component '{label}' is already deployed by component '{owner}'. The new file will overwrite it."
                ));
            }
            let source_hash = hash(item.path())?;
            plans.push(Planned {
                source: item.path().to_path_buf(),
                destination,
                relative,
                source_hash,
                backup: backup_path,
            });
        }

        if !record_existed {
            let overwritten = plans.iter().filter(|plan| plan.backup.is_some()).count();
            if overwritten > 0 {
                warnings.push(format!(
                    "Claiming ownership of {} for the first time while deploying {label}, and {overwritten} existing file(s) in its target paths will be backed up and overwritten; if another manager already tracks these files this could not be verified.",
                    self.game.display()
                ));
            }
        }

        let mut manifest = Manifest {
            component_guid: id.clone(),
            component_name: name,
            deployed_utc: Utc::now(),
            entries: Vec::new(),
        };
        let mut done: Vec<(&Planned, Entry)> = Vec::new();
        for plan in &plans {
            match self.apply(plan) {
                Ok(entry) => done.push((plan, entry)),
                Err(error) => {
                    let mut left = self.roll_back(&done, &backup);
                    left.extend(stranded(plan));
                    let empty = Manifest {
                        entries: Vec::new(),
                        ..manifest.clone()
                    };
                    return Err(self.abort(
                        empty,
                        left,
                        error,
                        &format!("Deploy of {label} failed at {}", plan.relative),
                    ));
                }
            }
        }
        manifest.entries = done.iter().map(|(_, entry)| entry.clone()).collect();
        if let Err(error) = self.save(&manifest) {
            let left = self.roll_back(&done, &backup);
            let empty = Manifest {
                entries: Vec::new(),
                ..manifest.clone()
            };
            return Err(self.abort(empty, left, error, "Deployment manifest could not be saved"));
        }
        Ok((manifest, warnings))
    }

    /// Finish a failed batch: report a clean rollback, or record what could not
    /// be returned in `base` so a later uninstall can finish the job.
    fn abort(
        &self,
        mut base: Manifest,
        left: Vec<Entry>,
        error: anyhow::Error,
        what: &str,
    ) -> anyhow::Error {
        if left.is_empty() {
            return error.context(format!("{what}; this batch was rolled back"));
        }
        let paths: Vec<String> = left
            .iter()
            .map(|entry| entry.relative_path.clone())
            .collect();
        base.entries.extend(left);
        let recorded = match self.save(&base) {
            Ok(()) => format!(
                "they are recorded in {} so uninstall can finish",
                self.manifest_path(&component_id(&base.component_guid).unwrap_or_default())
                    .display()
            ),
            Err(save) => format!("recording them also failed ({save})"),
        };
        error.context(format!(
            "{what}; rollback was incomplete for {} file(s) ({}); {recorded}",
            paths.len(),
            paths.join(", ")
        ))
    }

    fn apply(&self, plan: &Planned) -> Result<Entry> {
        // Re-check at mutation time; the preflight result may be stale. The
        // source hash was already taken at preflight (see `Planned`) so the
        // gap between this check and the write in `place` stays small.
        check_ancestors(&self.game, &plan.relative)?;
        let mut entry = Entry {
            relative_path: plan.relative.clone(),
            source_hash: plan.source_hash.clone(),
            size: fs::metadata(&plan.source)?.len(),
            deployment_method: Method::Copy,
            overwrote_existing: false,
            backup_relative_path: None,
        };
        if let Some(backup) = &plan.backup {
            move_no_clobber(&plan.destination, backup)?;
            entry.overwrote_existing = true;
            entry.backup_relative_path = Some(plan.relative.clone());
        }
        match self.place(
            &plan.source,
            &plan.destination,
            &plan.relative,
            &entry.source_hash,
        ) {
            Ok(method) => {
                entry.deployment_method = method;
                Ok(entry)
            }
            Err(error) => {
                if let Some(backup) = &plan.backup
                    && let Err(restore) = move_no_clobber(backup, &plan.destination)
                {
                    return Err(error.context(format!(
                        "the displaced original could not be put back ({restore}); it remains at {}",
                        backup.display()
                    )));
                }
                Err(error)
            }
        }
    }

    /// Materialize `source` at `destination` without ever replacing an existing
    /// file: a hardlink, or a copy published with no-clobber semantics. Nothing
    /// is left at `destination` on failure. A write that lands outside the game
    /// tree (an ancestor swapped for a symlink after the check) is undone,
    /// including any directories `create_dir_all` made on the way there.
    fn place(
        &self,
        source: &Path,
        destination: &Path,
        relative: &str,
        source_hash: &str,
    ) -> Result<Method> {
        let parent = destination
            .parent()
            .context("Deployment path needs a parent")?;
        // Re-check right before create_dir_all: this is the last read before
        // the write below, narrowing the window in which an ancestor could
        // be swapped for a symlink between the caller's check and this write.
        check_ancestors(&self.game, relative)?;
        let existing_ancestor = parent
            .ancestors()
            .find(|candidate| fs::symlink_metadata(candidate).is_ok())
            .map(Path::to_path_buf);
        fs::create_dir_all(parent)?;
        let method = match fs::hard_link(source, destination) {
            Ok(()) => Method::Hardlink,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => bail!(
                "{} appeared after the preflight; it was left untouched",
                destination.display()
            ),
            Err(_) => {
                let mut input = fs::File::open(source)?;
                let mut temp = tempfile::NamedTempFile::new_in(parent)?;
                std::io::copy(&mut input, temp.as_file_mut())?;
                temp.as_file().sync_all()?;
                fs::set_permissions(temp.path(), fs::metadata(source)?.permissions())?;
                temp.persist_noclobber(destination).map_err(|e| e.error)?;
                Method::Copy
            }
        };
        let inside = fs::canonicalize(parent).is_ok_and(|real| real.starts_with(&self.game));
        if !inside {
            if hash(destination).is_ok_and(|bytes| bytes.eq_ignore_ascii_case(source_hash)) {
                let _ = fs::remove_file(destination);
            }
            remove_created_dirs(parent, existing_ancestor.as_deref());
            bail!(
                "{} resolved outside the game directory during deployment",
                destination.display()
            );
        }
        Ok(method)
    }

    /// Undo one applied file: delete it only while it still holds the bytes this
    /// run deployed, then move the displaced original back without clobbering.
    /// Returns false when either step could not be completed.
    fn undo_one(&self, plan: &Planned, entry: &Entry) -> bool {
        let cleared = match fs::symlink_metadata(&plan.destination) {
            Ok(meta) if meta.is_file() => {
                hash(&plan.destination)
                    .is_ok_and(|bytes| bytes.eq_ignore_ascii_case(&entry.source_hash))
                    && fs::remove_file(&plan.destination).is_ok()
            }
            Ok(_) => false,
            Err(error) => error.kind() == ErrorKind::NotFound,
        };
        let restored = match &plan.backup {
            Some(saved) if cleared => move_no_clobber(saved, &plan.destination).is_ok(),
            Some(_) => false,
            None => true,
        };
        if cleared {
            prune(&self.game, plan.destination.parent());
        }
        cleared && restored
    }

    /// Undo a batch newest first. Returns the entries that could not be undone.
    fn roll_back(&self, done: &[(&Planned, Entry)], backup: &Path) -> Vec<Entry> {
        let mut left = Vec::new();
        for (plan, entry) in done.iter().rev() {
            if !self.undo_one(plan, entry) {
                left.push(entry.clone());
            }
        }
        remove_empty_tree(backup);
        left
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
    /// The whole batch rolls back on failure and the manifest is saved once.
    /// `DeployedUtc` is left unchanged: it is ownership and purge order.
    pub fn deploy_append(
        &self,
        component: &str,
        name: Option<String>,
    ) -> Result<(Manifest, Vec<String>)> {
        let id = component_id(component)?;
        let Some(manifest) = self.manifest(&id)? else {
            return self.deploy_report(component, name);
        };
        self.ensure_exclusive_game()?;
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
                let staged_hash = hash(item.path())?;
                if staged_hash.eq_ignore_ascii_case(&entry.source_hash) {
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
                        source_hash: staged_hash,
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
            if let Some((owner, owner_utc)) = foreign.get(&relative.to_lowercase()) {
                // This manifest keeps its older DeployedUtc, so taking a path a
                // newer deployment owns would invert the backup chain: that
                // owner's uninstall could no longer hand its original back.
                ensure!(
                    *owner_utc <= manifest.deployed_utc,
                    "'{relative}' is owned by component '{owner}', deployed after {label}. Appending it would break that component's backup chain; uninstall '{owner}' first."
                );
                warnings.push(format!(
                    "Conflict: '{relative}' from component '{label}' is already deployed by component '{owner}'. The new file will overwrite it."
                ));
            }
            let source_hash = hash(item.path())?;
            plans.push((
                Planned {
                    source: item.path().to_path_buf(),
                    destination,
                    relative,
                    source_hash,
                    backup: backup_path,
                },
                None,
            ));
        }

        // Replaced owned files are parked here so a failed batch can put them back.
        let pending = self.root.join("append-pending");
        let parking = pending.join(&id);
        ensure!(
            fs::symlink_metadata(&parking).is_err(),
            "An interrupted append left files in {}. Move them back or remove them first.",
            parking.display()
        );
        let mut done: Vec<(&Planned, Entry, Option<usize>)> = Vec::new();
        let mut parked: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut failure = None;
        for (plan, replaces) in &plans {
            if replaces.is_some() {
                let park = parking.join(relative_parts(&plan.relative)?);
                if let Err(error) = check_ancestors(&self.game, &plan.relative)
                    .and_then(|()| move_no_clobber(&plan.destination, &park))
                {
                    failure = Some((plan, error));
                    break;
                }
                parked.push((park, plan.destination.clone()));
            }
            match self.apply(plan) {
                Ok(entry) => done.push((plan, entry, *replaces)),
                Err(error) => {
                    failure = Some((plan, error));
                    break;
                }
            }
        }
        // Undo newest first; returns stranded new entries and notes on replaced files.
        let undo = |failed: Option<&Planned>| -> (Vec<Entry>, Vec<String>) {
            let mut left = Vec::new();
            let mut notes = Vec::new();
            if let Some(plan) = failed {
                left.extend(stranded(plan));
            }
            for (plan, entry, replaces) in done.iter().rev() {
                if !self.undo_one(plan, entry) && replaces.is_none() {
                    left.push(entry.clone());
                }
            }
            for (park, destination) in parked.iter().rev() {
                if let Err(error) = move_no_clobber(park, destination) {
                    notes.push(format!(
                        "the earlier deployed copy of {} stays at {} ({error})",
                        destination.display(),
                        park.display()
                    ));
                }
            }
            remove_empty_tree(&backup);
            remove_empty_tree(&pending);
            (left, notes)
        };
        let finish = |error: anyhow::Error, what: String, failed: Option<&Planned>| {
            let (left, notes) = undo(failed);
            let error = self.abort(manifest.clone(), left, error, &what);
            if notes.is_empty() {
                error
            } else {
                error.context(notes.join("; "))
            }
        };
        if let Some((plan, error)) = failure {
            return Err(finish(
                error,
                format!("Append deploy of {label} failed at {}", plan.relative),
                Some(plan),
            ));
        }
        let mut updated = manifest.clone();
        for (_, entry, replaces) in &done {
            match replaces {
                Some(index) => {
                    let old = &mut updated.entries[*index];
                    old.source_hash = entry.source_hash.clone();
                    old.size = entry.size;
                    old.deployment_method = entry.deployment_method.clone();
                }
                None => updated.entries.push(entry.clone()),
            }
        }
        if name.as_deref().is_some_and(|name| !name.trim().is_empty()) {
            updated.component_name = name;
        }
        // DeployedUtc is ownership order; appending keeps the original value.
        if let Err(error) = self.save(&updated) {
            return Err(finish(
                error,
                "Deployment manifest could not be saved".into(),
                None,
            ));
        }
        let _ = fs::remove_dir_all(&parking);
        remove_empty_tree(&pending);
        Ok((updated, warnings))
    }

    /// Owned paths that would stop a clean uninstall: unsafe or non-file game
    /// paths, and changed files no later deployment has backed up. Read-only.
    pub fn reinstall_blockers(&self, component: &str) -> Result<Vec<String>> {
        let id = component_id(component)?;
        self.ensure_own_game()?;
        let Some(manifest) = self.manifest(&id)? else {
            return Ok(Vec::new());
        };
        let (mut others, _) = self.deployed_with_warnings()?;
        others.retain(|other| !other.component_guid.eq_ignore_ascii_case(&id));
        let mut blockers = Vec::new();
        for entry in &manifest.entries {
            let Ok(Some((destination, relative))) = self.locate(&entry.relative_path) else {
                blockers.push(format!(
                    "unsafe or ambiguous path '{}'",
                    entry.relative_path
                ));
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
        self.ensure_own_game()?;
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
        // Entries whose original could not be put back; they keep the manifest alive.
        let mut retained = Vec::new();

        for entry in &manifest.entries {
            let (destination, relative) = match self.locate(&entry.relative_path) {
                Ok(Some(found)) => found,
                Ok(None) => {
                    // Keep the entry: dropping it would orphan its backup and
                    // leave the deployed bytes live with no record.
                    report.skipped.push(entry.relative_path.clone());
                    report.warnings.push(format!(
                        "'{}' cannot be resolved safely (case-colliding entries, or a symlink or file on the way); it stays in the manifest of {id}.",
                        entry.relative_path
                    ));
                    retained.push(entry.clone());
                    continue;
                }
                Err(_) => {
                    report.warnings.push(format!(
                        "Skipping unsafe relative path '{}' during uninstall of {id}.",
                        entry.relative_path
                    ));
                    continue;
                }
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
                    match move_no_clobber(&saved, &destination) {
                        Ok(()) => report.restored.push(relative),
                        Err(error) => {
                            report.skipped.push(relative.clone());
                            report.warnings.push(format!(
                                "The original of '{relative}' could not be put back ({error}); it stays at '{}' and this entry stays in the manifest.",
                                saved.display()
                            ));
                            retained.push(entry.clone());
                            continue;
                        }
                    }
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
                let owner_backup = self.backup(&owner_id).join(relative_parts(
                    others[owner].entries[index]
                        .backup_relative_path
                        .as_deref()
                        .unwrap_or_default(),
                )?);
                match saved {
                    // One rename replaces the later owner's backup slot atomically
                    // (both live under this manifest root), so the slot is never empty.
                    Some(saved) => fs::rename(&saved, &owner_backup).with_context(|| {
                        format!(
                            "Could not hand the original of '{relative}' to the later owner's backup at {}; nothing was changed for this entry and the manifest of {id} is kept",
                            owner_backup.display()
                        )
                    })?,
                    // Nothing was displaced: record that first, then drop the backup.
                    None => {
                        let owner_entry = &mut others[owner].entries[index];
                        owner_entry.overwrote_existing = false;
                        owner_entry.backup_relative_path = None;
                        self.save(&others[owner])?;
                        fs::remove_file(&owner_backup)?;
                        remove_empty_tree(&self.backup(&owner_id));
                    }
                }
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
        if retained.is_empty() {
            fs::remove_file(self.manifest_path(&id))?;
            remove_empty_tree(&backup);
            self.release_owner_record_if_empty()?;
        } else {
            self.save(&Manifest {
                entries: retained,
                ..manifest
            })?;
        }
        report.removed = true;
        Ok(report)
    }

    /// Find the live path for a recorded entry. The recorded spelling wins when
    /// it exists, so a case variant created later cannot hide a deployed file.
    /// `Err`: the recorded path is invalid. `Ok(None)`: it cannot be resolved
    /// safely right now (case collision, symlink or file ancestor).
    fn locate(&self, recorded: &str) -> Result<Option<(PathBuf, String)>> {
        let parts = relative_parts(recorded)?;
        let exact = self.game.join(&parts);
        let spelled = parts
            .iter()
            .map(|part| part.to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if fs::symlink_metadata(&exact).is_ok() {
            return Ok(check_ancestors(&self.game, &spelled)
                .ok()
                .map(|()| (exact, spelled)));
        }
        Ok(resolve_case(&self.game, &parts)
            .and_then(|(path, relative)| {
                check_ancestors(&self.game, &relative)?;
                Ok((path, relative))
            })
            .ok())
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
        self.ensure_own_game()?;
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
        self.ensure_exclusive_game()?;
        let mut report = ProvenanceReport::default();
        let existing = self.manifest(&id)?;
        let had_manifest = existing.is_some();
        // DeployedUtc is ownership order; only a new manifest gets a timestamp.
        let mut manifest = existing.unwrap_or_else(|| Manifest {
            component_guid: id.clone(),
            component_name: None,
            deployed_utc: Utc::now(),
            entries: Vec::new(),
        });
        if name.as_deref().is_some_and(|name| !name.trim().is_empty()) {
            manifest.component_name = name;
        }
        let owned: BTreeMap<String, usize> = manifest
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.relative_path.to_lowercase(), index))
            .collect();
        let old: BTreeMap<String, &String> = before
            .iter()
            .map(|(path, hash)| (path.to_lowercase(), hash))
            .collect();
        let foreign = self.foreign_index(&id)?;
        let changed = self.diff_index(before)?;
        // Decide refusals in a read-only pre-pass, before anything below can
        // mutate the game tree or the staged tree (`sync_staged` rewrites
        // staging with no undo). A newer component owning the path, or this
        // component's own path having changed before the patcher ran, must
        // fail the whole call rather than silently skip one entry.
        let mut refusals: Vec<String> = Vec::new();
        for relative in &changed {
            let lower = relative.to_lowercase();
            let parts = relative_parts(relative)?;
            let path = self.game.join(&parts);
            if check_ancestors(&self.game, relative).is_err()
                || !fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file())
            {
                continue;
            }
            if let Some(&index) = owned.get(&lower) {
                let ours = old
                    .get(&lower)
                    .is_some_and(|h| h.eq_ignore_ascii_case(&manifest.entries[index].source_hash));
                if !ours {
                    refusals.push(format!(
                        "'{relative}' is owned by this component but changed before the patcher ran"
                    ));
                }
                continue;
            }
            if let Some((owner, owner_utc)) = foreign.get(&lower)
                && *owner_utc > manifest.deployed_utc
            {
                refusals.push(format!(
                    "'{relative}' is owned by component '{owner}', deployed after this component"
                ));
            }
        }
        ensure!(
            refusals.is_empty(),
            "Refusing to record provenance: {}. Nothing was recorded or changed; resolve ownership first.",
            refusals.join("; ")
        );
        let mut created: Vec<PathBuf> = Vec::new();
        let outcome = (|| -> Result<()> {
            let mut seen = BTreeSet::new();
            for relative in changed {
                let lower = relative.to_lowercase();
                ensure!(
                    seen.insert(lower.clone()),
                    "Case-colliding game paths for {relative}"
                );
                let parts = relative_parts(&relative)?;
                let path = self.game.join(&parts);
                if check_ancestors(&self.game, &relative).is_err()
                    || !fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file())
                {
                    continue;
                }
                let current = hash(&path)?;
                if let Some(&index) = owned.get(&lower) {
                    // The patcher changed a file this component deployed. Adopt the
                    // new bytes only if the pre-patcher bytes were this component's.
                    let ours = old.get(&lower).is_some_and(|h| {
                        h.eq_ignore_ascii_case(&manifest.entries[index].source_hash)
                    });
                    if !ours {
                        report.skipped.push(relative.clone());
                        report.warnings.push(format!(
                            "'{relative}' is owned by this component but changed before the patcher ran; its record is left as it was."
                        ));
                        continue;
                    }
                    self.sync_staged(&id, &parts, &path, &current)?;
                    let entry = &mut manifest.entries[index];
                    entry.source_hash = current;
                    entry.size = fs::metadata(&path)?.len();
                    entry.deployment_method = Method::Copy;
                    report.updated.push(relative);
                    continue;
                }
                if let Some((owner, owner_utc)) = foreign.get(&lower)
                    && *owner_utc > manifest.deployed_utc
                {
                    // Recording it would chain this older manifest after a newer
                    // owner and leave that owner's original unrecoverable.
                    report.skipped.push(relative.clone());
                    report.warnings.push(format!(
                        "'{relative}' is owned by component '{owner}', deployed after this component; it is not recorded here."
                    ));
                    continue;
                }
                let mut entry = Entry {
                    relative_path: relative.clone(),
                    source_hash: current,
                    size: fs::metadata(&path)?.len(),
                    deployment_method: Method::Copy,
                    overwrote_existing: false,
                    backup_relative_path: None,
                };
                if let Some(old_hash) = old.get(&lower) {
                    let prior = backup_source.map(|root| root.join(&parts)).filter(|prior| {
                        fs::symlink_metadata(prior).is_ok_and(|meta| meta.is_file())
                    });
                    let Some(prior) = prior else {
                        report.skipped.push(relative.clone());
                        report.warnings.push(format!(
                            "'{relative}' existed before the patcher changed it and no prior copy was supplied; it is not recorded, so uninstall will leave it."
                        ));
                        continue;
                    };
                    let saved = self.backup(&id).join(&parts);
                    ensure!(
                        fs::symlink_metadata(&saved).is_err(),
                        "A backup is already present at {}",
                        saved.display()
                    );
                    let parent = saved.parent().context("Backup path needs a parent")?;
                    fs::create_dir_all(parent)?;
                    // Copy to a temporary file, prove those exact bytes are the
                    // snapshot, then publish without clobbering.
                    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
                    std::io::copy(&mut fs::File::open(&prior)?, temp.as_file_mut())?;
                    temp.as_file().sync_all()?;
                    if !hash(temp.path())?.eq_ignore_ascii_case(old_hash) {
                        report.skipped.push(relative.clone());
                        report.warnings.push(format!(
                            "'{relative}' existed before the patcher changed it but the supplied prior copy does not match the snapshot; it is not recorded, so uninstall will leave it."
                        ));
                        continue;
                    }
                    temp.persist_noclobber(&saved).map_err(|e| e.error)?;
                    created.push(saved);
                    entry.overwrote_existing = true;
                    entry.backup_relative_path = Some(relative.clone());
                    report.backed_up.push(relative.clone());
                }
                report.recorded.push(relative);
                manifest.entries.push(entry);
            }
            if !report.recorded.is_empty() || !report.updated.is_empty() || !had_manifest {
                self.save(&manifest)?;
            }
            Ok(())
        })();
        if let Err(error) = outcome {
            let mut kept = Vec::new();
            for saved in &created {
                if fs::remove_file(saved).is_err() {
                    kept.push(saved.display().to_string());
                }
            }
            remove_empty_tree(&self.backup(&id));
            return Err(error.context(if kept.is_empty() {
                "Provenance was not recorded; backups made by this call were removed".to_owned()
            } else {
                format!(
                    "Provenance was not recorded; these backups made by this call could not be removed: {}",
                    kept.join(", ")
                )
            }));
        }
        remove_empty_tree(&self.backup(&id));
        Ok((manifest, report))
    }

    /// Keep the component's staged copy equal to its deployed bytes, so a later
    /// `deploy_append` does not treat stale staged bytes as a new version.
    fn sync_staged(&self, id: &str, parts: &Path, live: &Path, live_hash: &str) -> Result<()> {
        let Ok(staged_root) = self.staged_directory(id, id) else {
            return Ok(());
        };
        let Ok((staged, _)) = resolve_case(&staged_root, parts) else {
            return Ok(());
        };
        if !fs::symlink_metadata(&staged).is_ok_and(|meta| meta.is_file())
            || hash(&staged)?.eq_ignore_ascii_case(live_hash)
        {
            return Ok(());
        }
        let parent = staged.parent().context("Staged path needs a parent")?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        std::io::copy(&mut fs::File::open(live)?, temp.as_file_mut())?;
        temp.as_file().sync_all()?;
        temp.persist(&staged).map_err(|e| e.error)?;
        Ok(())
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
        // Always scan, so an exact spelling cannot hide a case-colliding sibling.
        if searching {
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

/// True when a manifest root holds at least one manifest file.
fn has_manifests(root: &Path) -> bool {
    fs::read_dir(root.join("manifests")).is_ok_and(|entries| {
        entries
            .filter_map(Result::ok)
            .any(|entry| entry.path().extension().is_some_and(|x| x == "json"))
    })
}

/// The game directory a manifest root records, canonical when it still exists.
/// `None` when the root has no marker (it predates the marker).
fn marker_game(root: &Path) -> Option<PathBuf> {
    let marker = fs::read_to_string(root.join(GAME_MARKER)).ok()?;
    let marker = marker.trim();
    (!marker.is_empty()).then(|| fs::canonicalize(marker).unwrap_or_else(|_| PathBuf::from(marker)))
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
/// The entry to keep when a failed apply displaced an original it could not
/// put back: the original stays recorded as this plan's backup.
fn stranded(plan: &Planned) -> Option<Entry> {
    let backup = plan.backup.as_ref()?;
    if !fs::symlink_metadata(backup).is_ok_and(|meta| meta.is_file()) {
        return None;
    }
    Some(Entry {
        relative_path: plan.relative.clone(),
        source_hash: plan.source_hash.clone(),
        size: fs::metadata(&plan.source).map_or(0, |meta| meta.len()),
        deployment_method: Method::Copy,
        overwrote_existing: true,
        backup_relative_path: Some(plan.relative.clone()),
    })
}

/// Move a file without ever replacing what is at `to`: a hardlink then unlink,
/// or across filesystems a synced copy published with no-clobber semantics.
fn move_no_clobber(from: &Path, to: &Path) -> Result<()> {
    let parent = to.parent().context("Destination needs a parent")?;
    fs::create_dir_all(parent)?;
    match fs::hard_link(from, to) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            bail!("{} already exists; it was not overwritten", to.display())
        }
        Err(_) => {
            let mut temp = tempfile::NamedTempFile::new_in(parent)?;
            std::io::copy(&mut fs::File::open(from)?, temp.as_file_mut())?;
            temp.as_file().sync_all()?;
            fs::set_permissions(temp.path(), fs::metadata(from)?.permissions())?;
            temp.persist_noclobber(to).map_err(|e| e.error)?;
        }
    }
    fs::remove_file(from)?;
    Ok(())
}

fn hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    std::io::copy(&mut file, &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}
/// Best-effort cleanup of directories `create_dir_all` created on the way to
/// `parent`, walking upward from `parent` (inclusive) to (but not including)
/// `stop`, the nearest ancestor that already existed. `fs::remove_dir` only
/// removes empty directories, so anything else that landed inside is left
/// alone rather than force-removed.
fn remove_created_dirs(parent: &Path, stop: Option<&Path>) {
    let mut current = parent.to_path_buf();
    loop {
        if stop.is_some_and(|stop| current == stop) {
            return;
        }
        if fs::remove_dir(&current).is_err() {
            return;
        }
        match current.parent() {
            Some(next) => current = next.to_path_buf(),
            None => return,
        }
    }
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
