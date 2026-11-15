//! Shared prediction/publication of the separately journaled pre-ModList phase.
use super::*;
use crate::k2_preparation as recipe;
use serde_json::Value;
use std::io::{Read, Write};

impl Engine {
    fn k2_input(&self, name: &str) -> Result<Option<Vec<u8>>> {
        let path = self.actual(&self.settings.roots.game.join(name));
        self.settings.roots.check(&path)?;
        ensure!(
            !self.directories.contains_key(&self.path_policy.key(&path)),
            "K2 preparation path is a directory"
        );
        self.files
            .get(&self.path_policy.key(&path))
            .map(|(_, content)| {
                let mut file = crate::paths::open_regular_read(content.path()?)?;
                ensure!(
                    file.metadata()?.len() == recipe::SIZE as u64,
                    "K2 preparation file has unexpected size"
                );
                let mut bytes = Vec::with_capacity(recipe::SIZE);
                let mut buffer = [0u8; 65536];
                loop {
                    self.check_cancelled()?;
                    let n = file.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    ensure!(bytes.len() + n <= recipe::SIZE, "K2 preparation input grew");
                    bytes.extend_from_slice(&buffer[..n]);
                }
                Ok(bytes)
            })
            .transpose()
    }

    fn verify_k2_prepared_bytes(&self) -> Result<()> {
        let exe = self
            .k2_input("swkotor2.exe")?
            .context("Missing prepared K2 executable")?;
        let backup = self
            .k2_input("swkotor2.exe.bak")?
            .context("Missing original K2 backup")?;
        ensure!(
            recipe::sha(&exe) == recipe::OUTPUT_SHA && recipe::sha(&backup) == recipe::INPUT_SHA,
            "K2 prepared executable or backup differs"
        );
        Ok(())
    }

    pub(super) fn verify_k2_preparation_master(&self) -> Result<()> {
        ensure!(
            self.k2_prepared.is_some(),
            "Lightsaber prerequisite requires a typed K2 preparation capability"
        );
        if self.install {
            return self.verify_k2_prepared_bytes();
        }
        if self.verify_k2_prepared_bytes().is_ok() {
            return Ok(());
        }
        // Read-only preflight may rescan the original disk after predicting the
        // complete install. This proves only applicability, never installed state.
        let bytes = self
            .k2_input("swkotor2.exe")?
            .context("Missing K2 preparation input")?;
        ensure!(
            recipe::sha(&bytes) == recipe::INPUT_SHA
                && self.k2_input("swkotor2.exe.bak")?.is_none(),
            "K2 provisional preparation baseline differs"
        );
        Ok(())
    }

    pub(super) fn prepare_k2(
        &mut self,
        guide: &Guide,
        committed: Option<&Value>,
        emit: &mut impl FnMut(Event),
    ) -> Result<()> {
        let Some(binding) = recipe::verify(guide)? else {
            return Ok(());
        };
        ensure!(
            self.path_policy.case_insensitive,
            "K2 preparation requires collision-checked Windows path semantics"
        );
        let inherited = if !self.install && committed.is_none() {
            if let Some(request) = &self.checkpoint_resume {
                let store = crate::checkpoint::Store::open(&self.settings.roots)?;
                let verified = store.verify(
                    &request.expected.session_id,
                    Some(&request.expected.checkpoint_id),
                    &self.settings.cancelled,
                )?;
                ensure!(
                    verified.generation == request.expected.generation,
                    "Preparation resume HEAD changed"
                );
                let inspected = store.inspect(
                    &request.expected.session_id,
                    Some(&request.expected.checkpoint_id),
                )?;
                ensure!(
                    inspected.head_checkpoint_id == request.expected.checkpoint_id,
                    "Preparation requires current committed HEAD"
                );
                inspected.checkpoint.preparation
            } else {
                None
            }
        } else {
            None
        };
        let committed = committed.or(inherited.as_ref());
        if let Some(committed) = committed {
            ensure!(
                *committed == binding,
                "K2 checkpoint preparation binding differs"
            );
            self.verify_k2_prepared_bytes()?;
            self.k2_prepared = Some(binding);
            return Ok(());
        }
        // A prior prediction on this same VFS is immutable private state, not a disk receipt.
        if !self.install
            && self.k2_prepared.as_ref() == Some(&binding)
            && self.verify_k2_prepared_bytes().is_ok()
        {
            return Ok(());
        }
        let before = self
            .k2_input("swkotor2.exe")?
            .context("Missing K2 Steam executable")?;
        ensure!(
            self.k2_input("swkotor2.exe.bak")?.is_none(),
            "K2 preparation refuses an existing backup without a committed preparation checkpoint"
        );
        let output = recipe::transform(&before, &self.settings.cancelled)?;
        if self.install {
            ensure!(
                self.component_checkpoints
                    && self
                        .checkpoint_process
                        .as_ref()
                        .is_some_and(|(_, op)| op.kind == "InstallPreparation"),
                "K2 preparation requires its owned preparation journal and root lock"
            );
            ensure!(
                crate::paths::open_regular_read(
                    &self.actual(&self.settings.roots.game.join("swkotor2.exe"))
                )?
                .metadata()?
                .len()
                    == recipe::SIZE as u64,
                "K2 executable changed before publication"
            );
            let mut current = Vec::new();
            crate::paths::open_regular_read(
                &self.actual(&self.settings.roots.game.join("swkotor2.exe")),
            )?
            .take(recipe::SIZE as u64 + 1)
            .read_to_end(&mut current)?;
            ensure!(
                current == before,
                "K2 executable changed before publication"
            );
            ensure!(
                matches!(fs::symlink_metadata(self.actual(&self.settings.roots.game.join("swkotor2.exe.bak"))),Err(e) if e.kind()==std::io::ErrorKind::NotFound),
                "K2 backup appeared before publication"
            );
        }
        let scratch = self
            .settings
            .archive
            .scratch_dir
            .as_ref()
            .context("Missing preparation scratch")?;
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            let path = self.actual(&self.settings.roots.game.join("swkotor2.exe"));
            let (_, content) = self
                .files
                .get(&self.path_policy.key(&path))
                .context("Missing executable content")?;
            Some(
                crate::paths::open_regular_read(content.path()?)?
                    .metadata()?
                    .permissions()
                    .mode(),
            )
        };
        #[cfg(not(unix))]
        let mode = None;
        let mut pending = Vec::new();
        for (name, bytes, overwrite) in [
            ("swkotor2.exe.bak", before, false),
            ("swkotor2.exe", output, true),
        ] {
            let mut file = tempfile::NamedTempFile::new_in(scratch)?;
            file.write_all(&bytes)?;
            file.flush()?;
            pending.push((
                self.settings.roots.game.join(name),
                Content::Scratch(Arc::new(file), mode, None),
                overwrite,
            ));
        }
        for (path, content, overwrite) in pending {
            if self.install && !overwrite {
                self.check_cancelled()?;
                self.settings.roots.check(&path)?;
                self.path_policy.check_spelling(
                    &self.settings.roots.game,
                    &path,
                    &self.settings.cancelled,
                )?;
                let mut temp = tempfile::NamedTempFile::new_in(
                    path.parent().context("Backup parent absent")?,
                )?;
                std::io::copy(&mut content.open()?, temp.as_file_mut())?;
                #[cfg(unix)]
                if let Some(mode) = mode {
                    use std::os::unix::fs::PermissionsExt;
                    temp.as_file()
                        .set_permissions(fs::Permissions::from_mode(mode))?;
                }
                temp.as_file().sync_all()?;
                temp.persist_noclobber(&path)
                    .map_err(|e| e.error)
                    .context("K2 backup appeared before exclusive publication")?;
                crate::checkpoint::sync_recovery_directory(
                    path.parent().context("Backup parent absent")?,
                )?;
                self.files.insert(
                    self.path_policy.key(&path),
                    (path.clone(), Content::Disk(path)),
                );
            } else {
                self.put(&path, content, overwrite)?;
            }
        }
        self.verify_k2_prepared_bytes()?;
        self.k2_prepared = Some(binding.clone());
        emit(Event {
            component: String::new(),
            step: 0,
            message: format!(
                "K2 pre-ModList source-derived preparation: {binding}; original GUI equivalence is not claimed"
            ),
        });
        Ok(())
    }
}
