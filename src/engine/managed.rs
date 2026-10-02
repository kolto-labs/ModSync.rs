use super::*;
use crate::deployment::{HashIndex, Service};
use sha2::{Digest, Sha256};
use std::io::Read;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct ManagedRunReport {
    pub managed_mode_used: bool,
    pub manifests_written: usize,
    pub active_profile_name: Option<String>,
    pub patcher_component_names: Vec<String>,
    pub patcher_provenance_recorded: bool,
    pub untracked_live_edits: Vec<ManagedLiveEdit>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedLiveEdit {
    pub component_guid: String,
    pub action: String,
    pub path: String,
}

pub(super) struct Run {
    pub service: Service,
    pub pending: BTreeMap<String, PathBuf>,
    owner: Option<(String, String)>,
    pub report: ManagedRunReport,
    pub action: String,
    manifests: BTreeSet<String>,
}
impl Run {
    pub fn new(service: Service) -> Self {
        let report = ManagedRunReport {
            managed_mode_used: true,
            active_profile_name: service.active_profile_name().map(str::to_owned),
            ..ManagedRunReport::default()
        };
        Self {
            service,
            pending: BTreeMap::new(),
            owner: None,
            report,
            action: String::new(),
            manifests: BTreeSet::new(),
        }
    }
    pub fn begin(&mut self, component: &Component) -> Result<()> {
        ensure!(
            self.pending.is_empty(),
            "Prior managed component has unflushed staged writes"
        );
        self.service.prepare_reinstall(&component.guid)?;
        self.owner = Some((component.guid.clone(), component.name.clone()));
        Ok(())
    }
    fn owner(&self) -> Result<&(String, String)> {
        self.owner
            .as_ref()
            .context("Managed write has no owning top-level component")
    }
}

pub(super) struct Preimage {
    directory: tempfile::TempDir,
    before: HashIndex,
}

impl Engine {
    pub(super) fn managed_output(&self, logical: &Path) -> Result<PathBuf> {
        if let Some(managed) = &self.managed
            && let Ok(relative) = logical.strip_prefix(&self.settings.roots.game)
            && (self.stage_managed_writes
                || managed.pending.contains_key(&self.path_policy.key(logical)))
        {
            return managed
                .service
                .checkpoint_stage_target(&managed.owner()?.0, relative);
        }
        Ok(logical.to_owned())
    }

    pub(super) fn managed_delete(&mut self, logical: &Path) -> Result<()> {
        if let Some(managed) = &mut self.managed
            && let Ok(relative) = logical.strip_prefix(&self.settings.roots.game)
        {
            let retained = managed
                .service
                .checkpoint_stage_target(&managed.owner()?.0, relative)?;
            let pending = managed.pending.remove(&self.path_policy.key(logical));
            if fs::symlink_metadata(logical).is_ok_and(|metadata| metadata.is_file()) {
                let component_guid = managed.owner()?.0.clone();
                managed.report.untracked_live_edits.push(ManagedLiveEdit {
                    component_guid,
                    action: managed.action.clone(),
                    path: relative.to_string_lossy().into_owned(),
                });
            }
            // Delete the logical path from both materialized and pending layers.
            // Leaving an older live file behind would reintroduce it on rescan.
            match fs::remove_file(logical) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && pending.is_some() => {
                }
                Err(error) => return Err(error.into()),
            }
            match fs::remove_file(&retained) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && pending.is_none() => {
                }
                Err(error) => return Err(error.into()),
            }
            return Ok(());
        }
        fs::remove_file(logical)?;
        Ok(())
    }

    pub(super) fn flush_managed(&mut self) -> Result<()> {
        let Some(managed) = &mut self.managed else {
            return Ok(());
        };
        let (id, name) = managed.owner()?.clone();
        let staged = managed.service.checkpoint_stage_root(&id)?;
        let mut has_files = false;
        if staged.is_dir() {
            for item in walkdir::WalkDir::new(&staged).min_depth(1) {
                let item = item?;
                ensure!(
                    !item.file_type().is_symlink(),
                    "Managed staging contains a symlink"
                );
                if item.file_type().is_file() {
                    has_files = true;
                    break;
                }
                ensure!(
                    item.file_type().is_dir(),
                    "Managed staging contains a special file"
                );
            }
        }
        if has_files {
            managed.service.deploy_append(&id, Some(name))?;
            managed.manifests.insert(id.clone());
            managed.report.manifests_written = managed.manifests.len();
        }
        managed.pending.clear();
        self.scan()?;
        Ok(())
    }

    pub(super) fn managed_preimage(&self) -> Result<Preimage> {
        let managed = self.managed.as_ref().context("No managed service")?;
        let before = game_index(managed.service.game_directory(), &self.settings.cancelled)?;
        let directory = tempfile::Builder::new()
            .prefix("modsync-managed-preimage-")
            .tempdir_in(
                self.settings
                    .archive
                    .scratch_dir
                    .as_ref()
                    .context("Missing archive scratch directory")?,
            )?;
        for (relative, expected) in &before {
            self.check_cancelled()?;
            let source = self
                .settings
                .roots
                .game
                .join(crate::paths::relative(relative, false)?);
            self.settings.roots.check(&source)?;
            let destination = directory.path().join(relative);
            let input = crate::paths::open_regular_read(&source)?;
            let metadata = input.metadata()?;
            recovery::atomic_copy(&destination, input, &self.settings.cancelled)?;
            fs::OpenOptions::new()
                .write(true)
                .open(&destination)?
                .set_modified(metadata.modified()?)?;
            fs::set_permissions(&destination, metadata.permissions())?;
            ensure!(
                hash_cancelled(&destination, &self.settings.cancelled)? == *expected,
                "Managed preimage changed while copying"
            );
        }
        ensure!(
            game_index(managed.service.game_directory(), &self.settings.cancelled)? == before,
            "Game changed during managed process preimage capture"
        );
        Ok(Preimage { directory, before })
    }

    pub(super) fn record_managed_process(&mut self, preimage: &Preimage) -> Result<()> {
        let managed = self.managed.as_mut().context("No managed service")?;
        let (id, name) = managed.owner()?.clone();
        let after = game_index(managed.service.game_directory(), &self.settings.cancelled)?;
        let after_keys: BTreeSet<_> = after.keys().map(|path| path.to_lowercase()).collect();
        for relative in preimage
            .before
            .keys()
            .filter(|path| !after_keys.contains(&path.to_lowercase()))
        {
            let retained = managed
                .service
                .checkpoint_stage_target(&id, &crate::paths::relative(relative, false)?)?;
            match fs::remove_file(retained) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            managed.report.untracked_live_edits.push(ManagedLiveEdit {
                component_guid: id.clone(),
                action: managed.action.clone(),
                path: relative.clone(),
            });
        }
        let (_, report) = managed.service.record_live(
            &id,
            Some(name.clone()),
            &preimage.before,
            Some(preimage.directory.path()),
        )?;
        ensure!(
            report.skipped.is_empty(),
            "Managed process provenance incomplete: {}",
            report.skipped.join("; ")
        );
        managed.manifests.insert(id);
        managed.report.manifests_written = managed.manifests.len();
        if managed.action == "Patcher" {
            if !managed.report.patcher_component_names.contains(&name) {
                managed.report.patcher_component_names.push(name);
            }
            managed.report.patcher_provenance_recorded |=
                !report.recorded.is_empty() || !report.updated.is_empty();
        }
        Ok(())
    }
}

fn hash_cancelled(path: &Path, cancelled: &AtomicBool) -> Result<String> {
    let source = crate::paths::open_regular_read(path)?;
    let expected = source.metadata()?.len();
    let mut source = source.take(expected.saturating_add(1));
    let mut seen = 0u64;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        ensure!(
            !cancelled.load(Ordering::Relaxed),
            "Managed preimage cancelled"
        );
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        seen += count as u64;
        hash.update(&buffer[..count]);
    }
    ensure!(seen == expected, "Managed preimage file length changed");
    Ok(format!("{:x}", hash.finalize()))
}

fn game_index(root: &Path, cancelled: &AtomicBool) -> Result<HashIndex> {
    let mut index = HashIndex::new();
    for (count, item) in walkdir::WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .enumerate()
    {
        ensure!(
            count < 100_000,
            "Managed preimage exceeds checkpoint entry bound"
        );
        ensure!(
            !cancelled.load(Ordering::Relaxed),
            "Managed preimage cancelled"
        );
        let item = item?;
        ensure!(
            !item.file_type().is_symlink()
                && (item.file_type().is_file() || item.file_type().is_dir()),
            "Unsafe managed preimage entry"
        );
        if item.file_type().is_file() {
            let relative = item
                .path()
                .strip_prefix(root)?
                .to_str()
                .context("Managed preimage path needs UTF8")?
                .replace('\\', "/");
            index.insert(relative, hash_cancelled(item.path(), cancelled)?);
        }
    }
    Ok(index)
}
