//! Native patch bytes projected in private scratch, never in configured roots.
use super::{Content, Engine, Event};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
    sync::{Arc, atomic::Ordering},
};

const RESERVE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;

impl Engine {
    pub(super) fn predict_native_patcher(
        &mut self,
        base: &Path,
        destination: &Path,
        arguments: &str,
        emit: &mut impl FnMut(Event),
    ) -> Result<()> {
        ensure!(!self.install, "Native prediction cannot run on live roots");
        self.settings.roots.check(base)?;
        ensure!(
            self.path_policy.key(destination) == self.path_policy.key(&self.settings.roots.game),
            "Exact patch prediction requires the configured game root"
        );
        let configuration = self
            .simulation_configuration(base, arguments)?
            .context("Exact patch prediction needs a selected changes INI")?;
        self.check_simulation_ini(&configuration)?;
        let package = if base
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("tslpatchdata"))
        {
            base.parent().context("Patch data has no package folder")?
        } else {
            base
        };
        // Archive expansion limits apply to extracted archives, not the entire
        // game projection. Bound this private workspace independently.
        let limit = 16 * 1024 * 1024 * 1024;
        let scratch = self
            .settings
            .archive
            .scratch_dir
            .as_ref()
            .context("No prediction scratch folder")?;
        let mut required = 0u64;
        let mut count = 0usize;
        for (logical, content) in self.files.values() {
            if logical.starts_with(&self.settings.roots.game) || logical.starts_with(package) {
                self.check_cancelled()?;
                count += 1;
                ensure!(count <= MAX_ENTRIES, "Native prediction has too many files");
                let copies = u64::from(logical.starts_with(&self.settings.roots.game))
                    + u64::from(logical.starts_with(package));
                let bytes = content
                    .open()?
                    .metadata()?
                    .len()
                    .checked_mul(copies)
                    .context("Projection size overflow")?;
                required = required
                    .checked_add(bytes)
                    .context("Projection size overflow")?;
                ensure!(
                    required <= limit,
                    "Native prediction exceeds the configured scratch byte limit"
                );
            }
        }
        ensure!(
            fs2::available_space(scratch)? > required.saturating_mul(2).saturating_add(RESERVE),
            "Not enough private scratch space for native patch prediction and its reserve"
        );
        let directory_count = self
            .directories
            .values()
            .filter(|logical| {
                logical.starts_with(&self.settings.roots.game) || logical.starts_with(package)
            })
            .count();
        ensure!(
            count.saturating_add(directory_count) <= MAX_ENTRIES,
            "Native prediction has too many filesystem entries"
        );
        let projection = tempfile::tempdir_in(scratch)?;
        let game = projection.path().join("game");
        let mods = projection.path().join("mod");
        fs::create_dir(&game)?;
        fs::create_dir(&mods)?;
        for logical in self.directories.values() {
            self.check_cancelled()?;
            if let Ok(relative) = logical.strip_prefix(&self.settings.roots.game) {
                fs::create_dir_all(game.join(relative))?;
            }
            if let Ok(relative) = logical.strip_prefix(package) {
                fs::create_dir_all(mods.join(relative))?;
            }
        }
        let mut original = BTreeMap::new();
        let mut total = 0u64;
        for (logical, content) in self.files.values() {
            if let Ok(relative) = logical.strip_prefix(&self.settings.roots.game) {
                let hash =
                    self.project_patch_content(content, &game.join(relative), &mut total, limit)?;
                original.insert(self.path_policy.key(logical), hash);
            }
            if let Ok(relative) = logical.strip_prefix(package) {
                self.project_patch_content(content, &mods.join(relative), &mut total, limit)?;
            }
        }
        let ini = mods.join(configuration.strip_prefix(package)?);
        super::patcher_prepare::normalize(&mods, &self.settings.cancelled)?;
        let mut installer =
            odypatcher::ModInstaller::new(&mods, &game, &ini, Some(odypatcher::PatchLogger::new()))
                .map_err(|error| anyhow::anyhow!("Prepare native prediction: {error}"))?;
        installer
            .validate_configured_paths()
            .map_err(|error| anyhow::anyhow!("Unsafe native prediction configuration: {error}"))?;
        super::patcher_prepare::disable_external_compilers(&mut installer)?;
        let space_failed = Cell::new(false);
        let cancelled = || self.settings.cancelled.load(Ordering::Relaxed) || space_failed.get();
        let mut progress = || {
            if fs2::available_space(projection.path()).map_or(true, |bytes| bytes <= RESERVE) {
                space_failed.set(true);
            }
        };
        installer
            .install(Some(&cancelled), Some(&mut progress))
            .map_err(|error| anyhow::anyhow!("Native future patch failed: {error}"))?;
        self.check_cancelled()?;
        ensure!(
            !space_failed.get(),
            "Native prediction exhausted its scratch reserve"
        );
        let report = odypatcher::RunReport::from_logger(&installer.log);
        ensure!(
            !report.completed_with_errors(),
            "Native prediction reported {} errors and {} failed patches",
            report.errors,
            report.patches_failed
        );
        let mut files = BTreeMap::new();
        let mut directories = BTreeMap::new();
        directories.insert(
            self.path_policy.key(&self.settings.roots.game),
            self.settings.roots.game.clone(),
        );
        let mut output_bytes = 0u64;
        let mut retained_bytes = 0u64;
        for (index, entry) in walkdir::WalkDir::new(&game)
            .min_depth(1)
            .into_iter()
            .enumerate()
        {
            self.check_cancelled()?;
            ensure!(
                index < MAX_ENTRIES,
                "Native prediction has too many output entries"
            );
            let entry = entry?;
            ensure!(
                !entry.file_type().is_symlink(),
                "Native prediction produced a symlink"
            );
            let relative = entry.path().strip_prefix(&game)?;
            let relative = crate::paths::relative(
                relative
                    .to_str()
                    .context("Prediction output is not UTF-8")?,
                false,
            )?;
            let logical = self.settings.roots.game.join(relative);
            let key = self.path_policy.key(&logical);
            ensure!(
                !files.contains_key(&key) && !directories.contains_key(&key),
                "Case-colliding native prediction output"
            );
            if entry.file_type().is_dir() {
                directories.insert(key, logical);
                continue;
            }
            ensure!(
                entry.file_type().is_file(),
                "Native prediction produced a special file"
            );
            let metadata = entry.metadata()?;
            let source = Content::Disk(entry.path().into());
            let hash = self.transfer_patch_content(
                &source,
                &mut std::io::sink(),
                &mut output_bytes,
                limit,
            )?;
            let content = if original.get(&key) == Some(&hash) {
                self.files
                    .get(&key)
                    .context("Unchanged predicted file disappeared")?
                    .1
                    .clone()
            } else {
                let temporary = tempfile::NamedTempFile::new_in(scratch)?;
                let retained_hash = self.project_patch_content(
                    &source,
                    temporary.path(),
                    &mut retained_bytes,
                    limit,
                )?;
                ensure!(
                    retained_hash == hash,
                    "Native prediction output changed while retaining bytes"
                );
                Content::Scratch(Arc::new(temporary), None, metadata.modified().ok())
            };
            files.insert(key, (logical, content));
        }
        // Publish only after every native output has been validated and retained.
        self.files
            .retain(|_, (path, _)| !path.starts_with(&self.settings.roots.game));
        self.directories
            .retain(|_, path| !path.starts_with(&self.settings.roots.game));
        self.files.extend(files);
        self.directories.extend(directories);
        emit(Event {
            component: "Patcher".into(),
            step: 0,
            message: format!(
                "Native future state: {} completed patches, {} skipped, {} warnings; patched bytes retained in private scratch",
                report.patches_completed, report.patches_skipped, report.warnings
            ),
        });
        Ok(())
    }

    fn project_patch_content(
        &self,
        content: &Content,
        output: &Path,
        total: &mut u64,
        limit: u64,
    ) -> Result<[u8; 32]> {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut writer = fs::File::create(output)?;
        self.transfer_patch_content(content, &mut writer, total, limit)
    }

    fn transfer_patch_content(
        &self,
        content: &Content,
        writer: &mut impl Write,
        total: &mut u64,
        limit: u64,
    ) -> Result<[u8; 32]> {
        let mut reader = content.open()?;
        let before = reader.metadata()?;
        ensure!(before.is_file(), "Prediction input is not a regular file");
        let expected = total
            .checked_add(before.len())
            .context("Projection size overflow")?;
        ensure!(expected <= limit, "Prediction exceeds its byte limit");
        let mut hash = Sha256::new();
        let mut bytes = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            self.check_cancelled()?;
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            bytes = bytes
                .checked_add(count as u64)
                .context("Projection size overflow")?;
            ensure!(bytes <= before.len(), "Prediction input grew while reading");
            writer.write_all(&buffer[..count])?;
            hash.update(&buffer[..count]);
        }
        let after = reader.metadata()?;
        ensure!(
            bytes == before.len()
                && after.len() == before.len()
                && after.modified()? == before.modified()?,
            "Prediction input changed while reading"
        );
        *total = expected;
        Ok(hash.finalize().into())
    }
}
