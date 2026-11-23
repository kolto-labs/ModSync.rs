//! Fixed K1 native operations. All publication goes through the owning engine's
//! VFS, root lock, recovery journal and component checkpoint transaction.
use super::*;
use crate::k1_widescreen_recipe::{self as recipe, Role};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

const LIMIT: u64 = 32 * 1024 * 1024;
const PROJECTION_LIMIT: u64 = 256 * 1024 * 1024 * 1024;
const RESERVE: u64 = 2 * 1024 * 1024 * 1024;

impl Engine {
    pub(super) fn execute_k1_widescreen(
        &mut self,
        component: &Component,
        instruction: &Instruction,
        emit: &mut impl FnMut(Event),
    ) -> Result<()> {
        self.check_cancelled()?;
        let materials = recipe::verify_runtime(
            component,
            &self.settings.roots.mods,
            &self.settings.archive,
            &self.settings.cancelled,
        )?
        .context("K1 native action has no independently replayed owner")?;
        ensure!(
            component.instructions.len() == 1
                && serde_json::to_value(&component.instructions[0])?
                    == serde_json::to_value(instruction)?,
            "K1 action must be the complete compiled component operation"
        );
        ensure!(
            instruction.action == "K1Widescreen"
                && instruction.arguments == materials.role().as_str()
                && instruction.destination == "<<kotorDirectory>>"
                && instruction.source.is_empty(),
            "K1 native action role or destination differs"
        );
        if self.install {
            ensure!(
                self.component_checkpoints && self.checkpoint_process.is_some(),
                "K1 native publication requires its owned running component checkpoint and root lock"
            );
        }
        match materials.role() {
            Role::Hrmenus => {
                let baseline = self
                    .k1_game_input("swkotor.exe")?
                    .context("Missing K1 protected baseline")?;
                let ini = self.k1_game_input("swkotor.ini")?;
                let candidate = materials.hrmenus_candidate(
                    &baseline,
                    ini.as_deref(),
                    &self.settings.cancelled,
                )?;
                // Recheck actual None under the owned root lock immediately before
                // publication. Existing Some(empty), aliases, links and folders fail.
                if self.install {
                    let exe = self.actual(&self.settings.roots.game.join("swkotor.exe"));
                    ensure!(
                        self.k1_read_file(&exe)? == baseline,
                        "K1 baseline changed before transactional replacement"
                    );
                    let ini = self.actual(&self.settings.roots.game.join("swkotor.ini"));
                    self.settings.roots.check(&ini)?;
                    ensure!(
                        matches!(fs::symlink_metadata(&ini), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
                        "K1 authenticated absent INI appeared before transactional create"
                    );
                }
                self.k1_publish(candidate, emit)?;
            }
            Role::Fourgb => {
                let before = self
                    .k1_game_input("swkotor.exe")?
                    .context("Missing K1 HRMenus input")?;
                let candidate = materials.fourgb_candidate(&before, &self.settings.cancelled)?;
                if self.install {
                    let path = self.actual(&self.settings.roots.game.join("swkotor.exe"));
                    ensure!(
                        self.k1_read_file(&path)? == before,
                        "K1 HRMenus executable changed before 4GB publication"
                    );
                }
                self.k1_publish(candidate, emit)?;
            }
            Role::Galaxy => {
                ensure!(
                    self.settings.patcher_engine == PatcherEngine::OdyPatcher
                        && self.settings.patcher.is_none()
                        && self.patcher_invocation.is_none()
                        && self.settings.provided_installer.is_none(),
                    "Galaxy Vanilla requires the linked built-in native backend"
                );
                let contract = recipe::galaxy_contract(
                    component,
                    &self.settings.roots.mods,
                    &self.settings.archive,
                    &self.settings.cancelled,
                )?;
                ensure!(
                    contract.namespace_index() == 0
                        && contract.namespace_id() == "Vanilla"
                        && contract.strict_outcome_contract()
                            == "native-zero-warnings-changed-game-v1",
                    "Galaxy explicit namespace contract differs"
                );
                let staged = contract.stage_private(
                    &self.settings.roots.mods,
                    &self.settings.archive,
                    &self.settings.cancelled,
                )?;
                let outcome = self.k1_galaxy_base(staged.path(), emit)?;
                // The whole base must have succeeded before these overlay bytes
                // are available to this consumer. Overlay alone cannot finish GUID.
                self.k1_publish(materials.galaxy_overlay(&self.settings.cancelled)?, emit)?;
                emit(Event {
                    component: component.name.clone(),
                    step: 0,
                    message: format!(
                        "K1 whole Galaxy Vanilla base then 1920 overlay: {}",
                        json!({"Input":contract.input_receipt()?,"NativeOutcome":outcome,"WholeComponent":true,"Installed":self.install,"NativeBaseRunsInPrivateProjection":true})
                    ),
                });
            }
        }
        Ok(())
    }

    fn k1_read_file(&self, path: &Path) -> Result<Vec<u8>> {
        let mut input = crate::paths::open_regular_read(path)?;
        ensure!(
            input.metadata()?.len() <= LIMIT,
            "K1 input exceeds fixed bound"
        );
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 65536];
        loop {
            self.check_cancelled()?;
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            ensure!(
                bytes.len() as u64 + count as u64 <= LIMIT,
                "K1 input grew beyond bound"
            );
            bytes.extend_from_slice(&buffer[..count]);
        }
        Ok(bytes)
    }

    fn k1_game_input(&self, name: &str) -> Result<Option<Vec<u8>>> {
        let path = self.actual(
            &self
                .settings
                .roots
                .game
                .join(crate::paths::literal_relative(name)?),
        );
        self.settings.roots.check(&path)?;
        ensure!(
            !self.directories.contains_key(&self.path_policy.key(&path)),
            "K1 input is a directory"
        );
        self.files
            .get(&self.path_policy.key(&path))
            .map(|(_, content)| self.k1_read_file(content.path()?))
            .transpose()
    }

    fn k1_publish(
        &mut self,
        candidate: crate::k1_widescreen::CandidateOutput,
        emit: &mut impl FnMut(Event),
    ) -> Result<()> {
        let mut pending = Vec::new();
        for (relative, bytes) in candidate.files {
            self.check_cancelled()?;
            let path = self
                .settings
                .roots
                .game
                .join(crate::paths::literal_relative(&relative)?);
            self.settings.roots.check(&path)?;
            let mut file = tempfile::NamedTempFile::new_in(
                self.settings
                    .archive
                    .scratch_dir
                    .as_ref()
                    .context("Missing K1 scratch")?,
            )?;
            file.write_all(&bytes)?;
            file.flush()?;
            pending.push((
                path,
                Content::Scratch(Arc::new(file), None, None),
                relative != "swkotor.ini",
            ));
        }
        for (path, content, overwrite) in pending {
            if !overwrite {
                ensure!(
                    !self.files.contains_key(&self.path_policy.key(&path)),
                    "K1 deliberate INI create requires absent VFS input"
                );
            }
            self.put(&path, content, overwrite)?;
        }
        emit(Event {
            component: "K1Widescreen".into(),
            step: 0,
            message: format!(
                "K1 deterministic candidate publication: {}",
                candidate.receipt
            ),
        });
        Ok(())
    }

    /// Run the real native base in a bounded private projection in both modes.
    /// Validation publishes only retained scratch bytes into the VFS. Installation
    /// applies the resulting difference with engine put/delete under its checkpoint.
    fn k1_galaxy_base(&mut self, base: &Path, emit: &mut impl FnMut(Event)) -> Result<Value> {
        let scratch = self
            .settings
            .archive
            .scratch_dir
            .as_ref()
            .context("Missing Galaxy private scratch")?
            .clone();
        let mut required = 0u64;
        let mut count = 0usize;
        for (_, content) in self
            .files
            .values()
            .filter(|(path, _)| path.starts_with(&self.settings.roots.game))
        {
            self.check_cancelled()?;
            count += 1;
            ensure!(
                count <= 500_000,
                "Galaxy projection inventory exceeds bound"
            );
            required = required
                .checked_add(
                    crate::paths::open_regular_read(content.path()?)?
                        .metadata()?
                        .len(),
                )
                .context("Galaxy projection size overflow")?;
            ensure!(
                required <= PROJECTION_LIMIT,
                "Galaxy projection byte bound exceeded"
            );
        }
        ensure!(
            fs2::available_space(&scratch)? > required.saturating_mul(2).saturating_add(RESERVE),
            "Insufficient private scratch space for full Galaxy prediction and reserve"
        );
        let projection = tempfile::Builder::new()
            .prefix("modsync-k1-galaxy-game-")
            .tempdir_in(&scratch)?;
        let game = projection.path().join("game");
        fs::create_dir(&game)?;
        for directory in self
            .directories
            .values()
            .filter(|path| path.starts_with(&self.settings.roots.game))
        {
            self.check_cancelled()?;
            count += 1;
            ensure!(
                count <= 500_000,
                "Galaxy projected directory and file inventory exceeds bound"
            );
            fs::create_dir_all(game.join(directory.strip_prefix(&self.settings.roots.game)?))?;
        }
        let mut before = BTreeMap::new();
        let mut copied = 0u64;
        for (logical, content) in self
            .files
            .values()
            .filter(|(path, _)| path.starts_with(&self.settings.roots.game))
        {
            self.check_cancelled()?;
            let relative = logical.strip_prefix(&self.settings.roots.game)?;
            let target = game.join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let hash = self.k1_copy(content, &target, &mut copied)?;
            before.insert(self.path_policy.key(logical), hash);
        }
        super::patcher_prepare::normalize(base, &self.settings.cancelled)?;
        let mut session = odypatcher::Session::discover(
            base.to_str().context("Non UTF8 Galaxy staging")?,
            "changes.ini",
            "info.rtf",
        );
        ensure!(
            session.namespaces.len() == 2,
            "Galaxy expected exactly the two authenticated namespaces"
        );
        session.select_namespace(0);
        ensure!(
            session.blocked_reason().is_none(),
            "Galaxy Vanilla parser blocked: {}",
            session.blocked_reason().unwrap_or_default()
        );
        let strict = super::reviewed_patcher::before(
            base,
            &game,
            &session.load_logs,
            &self.settings.cancelled,
        )?;
        let mut installer = odypatcher::ModInstaller::new(
            &session.base_path,
            &game,
            &session.ini_path(),
            Some(odypatcher::PatchLogger::new()),
        )
        .map_err(|error| anyhow::anyhow!("Prepare Galaxy Vanilla native patcher: {error}"))?;
        installer
            .validate_configured_paths()
            .map_err(|error| anyhow::anyhow!("Unsafe Galaxy native configuration: {error}"))?;
        super::patcher_prepare::disable_external_compilers(&mut installer)?;
        let result = installer.install(
            Some(&|| self.settings.cancelled.load(Ordering::Acquire)),
            None,
        );
        let report = odypatcher::RunReport::from_logger(&installer.log);
        let log = session.write_install_log(&installer.log.all_logs);
        if let Err(error) = result {
            super::emit_patcher_failure_log(&installer.log.all_logs, emit);
            bail!("Galaxy Vanilla native failure: {error}");
        }
        ensure!(
            log.is_some(),
            "Galaxy native base did not write a fresh install log"
        );
        let outcome = super::reviewed_patcher::finish(
            strict,
            base,
            &game,
            &report,
            &installer.log.all_logs,
            &self.settings.cancelled,
        )?;
        let mut after = BTreeSet::new();
        let mut pending = Vec::new();
        let mut retained = 0u64;
        for (index, entry) in walkdir::WalkDir::new(&game)
            .min_depth(1)
            .into_iter()
            .enumerate()
        {
            self.check_cancelled()?;
            ensure!(index < 500_000, "Galaxy output inventory exceeds bound");
            let entry = entry?;
            ensure!(
                !entry.file_type().is_symlink(),
                "Galaxy native output contains a link"
            );
            let relative = entry.path().strip_prefix(&game)?;
            let relative = crate::paths::literal_relative(
                relative.to_str().context("Non UTF8 Galaxy output")?,
            )?;
            let logical = self.settings.roots.game.join(relative);
            if entry.file_type().is_dir() {
                continue;
            }
            ensure!(
                entry.file_type().is_file(),
                "Galaxy output contains special files"
            );
            let key = self.path_policy.key(&logical);
            ensure!(after.insert(key.clone()), "Galaxy output contains aliases");
            let source = Content::Disk(entry.path().into());
            let mut target = tempfile::NamedTempFile::new_in(&scratch)?;
            let hash = self.k1_copy(&source, target.path(), &mut retained)?;
            if before.get(&key) != Some(&hash) {
                target.flush()?;
                pending.push((logical, Content::Scratch(Arc::new(target), None, None)));
            }
        }
        let removed: Vec<_> = self
            .files
            .iter()
            .filter(|(key, (path, _))| {
                path.starts_with(&self.settings.roots.game) && !after.contains(*key)
            })
            .map(|(_, (path, _))| path.clone())
            .collect();
        for path in removed {
            self.delete(&path)?;
        }
        for (path, content) in pending {
            self.put(&path, content, true)?;
        }
        Ok(outcome)
    }

    fn k1_copy(&self, content: &Content, output: &Path, total: &mut u64) -> Result<[u8; 32]> {
        let mut input = crate::paths::open_regular_read(content.path()?)?;
        let mut writer = fs::File::create(output)?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            self.check_cancelled()?;
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            *total = total
                .checked_add(count as u64)
                .context("Galaxy projection size overflow")?;
            ensure!(
                *total <= PROJECTION_LIMIT
                    && fs2::available_space(
                        output.parent().context("Galaxy output parent absent")?
                    )? > RESERVE,
                "Galaxy private projection exhausted its byte bound or scratch reserve"
            );
            writer.write_all(&buffer[..count])?;
            hash.update(&buffer[..count]);
        }
        writer.flush()?;
        Ok(hash.finalize().into())
    }
}
