//! Patcher configuration previews from a future filesystem state.
use super::*;
use std::io::Read;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthoringActionReport {
    pub before: CheckpointRunReport,
    pub after: CheckpointRunReport,
}

impl Engine {
    pub(crate) fn with_authoring_inputs(mut self, files: Vec<(PathBuf, PathBuf)>) -> Result<Self> {
        self.authoring_inputs = files;
        self.seed_authoring_inputs()?;
        Ok(self)
    }

    pub(super) fn seed_authoring_inputs(&mut self) -> Result<()> {
        for (logical, object) in &self.authoring_inputs {
            ensure!(
                logical.parent() == Some(self.settings.roots.mods.as_path()),
                "Archive input is outside the mod root"
            );
            let key = self.path_policy.key(logical);
            ensure!(
                !self.directories.contains_key(&key),
                "Archive input conflicts with a directory"
            );
            if let Some((_, content)) = self.files.get(&key) {
                // Import never silently replaces a live or checkpoint archive.
                let mut left = content.open()?;
                let mut right = crate::paths::open_regular_read(object)?;
                let before = left.metadata()?;
                let expected = right.metadata()?.len();
                ensure!(
                    before.len() == expected,
                    "Archive input conflicts with existing bytes"
                );
                let mut bytes = 0u64;
                let mut a = [0u8; 65536];
                let mut b = [0u8; 65536];
                loop {
                    self.check_cancelled()?;
                    let n = left.read(&mut a)?;
                    bytes = bytes
                        .checked_add(n as u64)
                        .context("Archive size overflow")?;
                    right.read_exact(&mut b[..n])?;
                    ensure!(
                        a[..n] == b[..n],
                        "Archive input conflicts with existing bytes"
                    );
                    if n == 0 {
                        break;
                    }
                }
                let after = left.metadata()?;
                ensure!(
                    bytes == expected
                        && after.len() == before.len()
                        && after.modified()? == before.modified()?,
                    "Existing archive changed while comparing imported bytes"
                );
                // Keep the scanned spelling and physical backing of a real
                // file (or checkpoint entry); imports only fill absent paths.
                continue;
            }
            self.files
                .insert(key, (logical.clone(), Content::Disk(object.clone())));
        }
        Ok(())
    }

    pub(super) fn consume_authoring_input(&mut self, path: &Path) {
        if self.authoring_inputs.is_empty()
            || path.parent() != Some(self.settings.roots.mods.as_path())
        {
            return;
        }
        let key = self.path_policy.key(path);
        self.authoring_inputs
            .retain(|(logical, _)| self.path_policy.key(logical) != key);
    }

    pub(super) fn private_authoring_input(&self, path: &Path) -> bool {
        if self.authoring_inputs.is_empty()
            || path.parent() != Some(self.settings.roots.mods.as_path())
        {
            return false;
        }
        let key = self.path_policy.key(path);
        self.authoring_inputs.iter().any(|(logical, object)| {
            self.path_policy.key(logical) == key
                && self.files.get(&key).is_some_and(
                    |(_, content)| matches!(content, Content::Disk(backing) if backing == object),
                )
        })
    }

    pub(crate) fn with_authoring_boundary(
        mut self,
        boundary: &CheckpointRunReport,
        definition: &Component,
    ) -> Result<Self> {
        ensure!(!self.install, "Authoring prediction cannot write files");
        let entries = crate::checkpoint::Store::open(&self.settings.roots)?.authoring_inventory(
            &boundary.session_id,
            &boundary.checkpoint_id,
            &serde_json::to_value(definition)?,
            &self.settings.cancelled,
        )?;
        self.files.clear();
        self.directories.clear();
        for entry in entries {
            let key = self.path_policy.key(&entry.path);
            if let Some(content) = entry.content {
                ensure!(
                    self.files
                        .insert(key, (entry.path, Content::Disk(content)))
                        .is_none(),
                    "Authoring checkpoint has colliding file paths"
                );
            } else {
                ensure!(
                    self.directories.insert(key, entry.path).is_none(),
                    "Authoring checkpoint has colliding directory paths"
                );
            }
        }
        Ok(self)
    }

    /// Install exactly one saved action, retaining the original guide's selection
    /// context. The ordinary component journal fences all writes and failures.
    /// `record_baseline` must durably bind the authoring request before execution.
    pub fn install_recorded_action(
        self,
        guide: &Guide,
        component: usize,
        instruction: usize,
        mut record_baseline: impl FnMut(&CheckpointRunReport) -> Result<()>,
        emit: impl FnMut(Event),
        progress: impl FnMut(InstallProgress),
    ) -> Result<AuthoringActionReport> {
        self.install_recorded_action_with_input_plan(
            guide,
            component,
            instruction,
            |before, _| before.map_or(Ok(()), &mut record_baseline),
            emit,
            progress,
        )
    }

    pub(crate) fn install_recorded_action_with_input_plan(
        mut self,
        guide: &Guide,
        component: usize,
        instruction: usize,
        mut record: impl FnMut(Option<&CheckpointRunReport>, &[PathBuf]) -> Result<()>,
        mut emit: impl FnMut(Event),
        mut progress: impl FnMut(InstallProgress),
    ) -> Result<AuthoringActionReport> {
        ensure!(
            self.managed.is_none() && self.checkpoint_resume.is_none() && self.completed.is_empty(),
            "Authoring actions cannot combine with deployment or install resume"
        );
        ensure!(
            self.path_policy.case_insensitive,
            "Authoring needs ordinary path matching"
        );
        let mut owner = guide
            .components
            .get(component)
            .context("Mod is out of range")?
            .clone();
        ensure!(
            owner.is_selected,
            "Select this mod before applying its action"
        );
        let action = owner
            .instructions
            .get(instruction)
            .context("Action is out of range")?
            .clone();
        owner.instructions = vec![action];
        self.selected = selected_guid_keys(guide);
        self.selected_names = guide
            .components
            .iter()
            .filter(|c| c.is_selected)
            .map(|c| c.name.clone())
            .collect();
        self.check_component(&owner)?;
        let mut scope = Vec::new();
        self.collect_execution_components(&owner, &mut scope)?;
        for nested in scope {
            self.check_component(nested)?;
            for action in &nested.instructions {
                if !self.instruction_applies(action) {
                    continue;
                }
                if matches!(action.action.as_str(), "Execute" | "Run" | "Patcher") {
                    if self.requires_process_trust(&action.action) {
                        ensure!(
                            self.settings.allow_processes,
                            "This action requires explicit external-program trust"
                        );
                    }
                    ensure!(!action.source.is_empty(), "External action has no source");
                    if action.action == "Patcher" {
                        ensure!(action.source.len() == 1, "Patcher needs exactly one source");
                        if let Some(error) = &self.patcher_resolution_error {
                            bail!("Patcher backend unavailable: {error}");
                        }
                        self.check_patcher_backend()?;
                    } else {
                        process_arguments::expand(&action.arguments, &self.settings.roots)?;
                    }
                }
            }
        }
        let lock = Arc::new(recovery::Lock::acquire(&self.settings.roots)?);
        recovery::ensure_clean(&self.settings.roots)?;
        let single = Guide {
            components: vec![owner.clone()],
            metadata: guide.metadata.clone(),
            ordering: guide.ordering,
        };
        self.fomod_gate(&single, &mut emit)?;
        self.scan()?;
        let inputs = self.authoring_inputs.clone();
        self.component(&owner, &mut |_| {}, None)
            .context("Action preflight failed before any write")?;
        let consumed: Vec<_> = inputs
            .iter()
            .filter(|entry| !self.authoring_inputs.contains(entry))
            .map(|(logical, _)| logical.clone())
            .collect();
        let remaining_inputs = self.authoring_inputs.clone();
        // The journal must retain the predicted metadata transition before
        // either recovery system begins a writable transaction.
        record(None, &consumed)?;
        self.authoring_inputs = inputs;
        self.scan()?;
        self.check_cancelled()?;
        let mut observer = InstallObserver::new(1, &mut progress);
        observer.emit(
            InstallPhase::CreatingSnapshot,
            "Saving action recovery snapshot",
        );
        recovery::begin(&self.settings.roots)?;
        let store = crate::checkpoint::Store::open(&self.settings.roots)?;
        let baseline = store.begin_session(
            &lock,
            &[serde_json::to_value(&owner)?],
            &self.settings.cancelled,
            |update| {
                observer.emit(
                    InstallPhase::CreatingSnapshot,
                    format!(
                        "{}: {} files, {} bytes",
                        update.phase, update.files, update.bytes
                    ),
                );
            },
        )?;
        let before = CheckpointRunReport {
            session_id: baseline.session_id,
            checkpoint_id: baseline.checkpoint_id,
            generation: baseline.generation,
        };
        record(Some(&before), &consumed)?;
        let expected = crate::checkpoint::ExpectedHead {
            session_id: before.session_id.clone(),
            checkpoint_id: before.checkpoint_id.clone(),
            generation: before.generation,
        };
        let intent = store.begin_component(&lock, &expected, &self.settings.cancelled)?;
        let intent = store.mark_component_running(&lock, &intent, &self.settings.cancelled)?;
        self.component_checkpoints = true;
        self.install = true;
        observer.current.snapshot_pending = true;
        observer.component(&owner, 1);
        observer.emit(
            InstallPhase::InstallingComponent,
            "Applying recorded action",
        );
        #[cfg(windows)]
        {
            self.checkpoint_process = Some((Arc::clone(&lock), intent.clone()));
        }
        let execution = self.component(&owner, &mut emit, Some(&mut observer));
        #[cfg(windows)]
        {
            self.checkpoint_process = None;
        }
        execution.context("Action stopped; restore its checkpoint before retrying")?;
        ensure!(
            self.authoring_inputs == remaining_inputs,
            "Action input consumption differs from preflight; restore its checkpoint"
        );
        let committed =
            store.commit_component(&lock, &intent, &self.settings.cancelled, |update| {
                observer.emit(
                    InstallPhase::RecordingCompletion,
                    format!(
                        "{}: {} files, {} bytes",
                        update.phase, update.files, update.bytes
                    ),
                );
            })?;
        let after = CheckpointRunReport {
            session_id: before.session_id.clone(),
            checkpoint_id: committed.checkpoint_id,
            generation: committed.generation,
        };
        let expected = crate::checkpoint::ExpectedHead {
            session_id: after.session_id.clone(),
            checkpoint_id: after.checkpoint_id.clone(),
            generation: after.generation,
        };
        observer.emit(
            InstallPhase::RecordingCompletion,
            "Finalizing action checkpoint",
        );
        store.finalize_session_with_report(
            &lock,
            &expected,
            &self.settings.cancelled,
            &InstallReport {
                managed: None,
                checkpoint: Some(after.clone()),
            },
        )?;
        observer.current.snapshot_pending = false;
        observer.emit(
            InstallPhase::Completed,
            "Recorded action applied and checkpointed",
        );
        Ok(AuthoringActionReport { before, after })
    }
    /// Replay the authored prefix with native private-scratch prediction, then
    /// project the selected patcher documents for library inspection. Neither
    /// configured root is modified; unknown VFS bytes remain an error.
    pub fn inspect_predicted_patcher(
        mut self,
        guide: &Guide,
        source: &str,
        namespace: Option<usize>,
    ) -> Result<crate::editor::easy::patcher::PatchPreview> {
        self.exact_patcher_prediction = true;
        recovery::ensure_clean(&self.settings.roots)?;
        self.fomod_gate(guide, &mut |_| {})?;
        self.run(guide, &mut |_| {})?;
        let path = self.path(source, false)?;
        let key = self.path_policy.key(&path);
        let (folder, selected_file) = if let Some(directory) = self.directories.get(&key) {
            (directory.clone(), None)
        } else {
            let file = &self
                .files
                .get(&key)
                .context("Patcher source is missing at this point in the build")?
                .0;
            (
                file.parent()
                    .context("Patcher source has no parent")?
                    .to_path_buf(),
                Some(file),
            )
        };
        let in_data = folder
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("tslpatchdata"));
        let selected_ini = selected_file.filter(|file| {
            in_data
                && file
                    .extension()
                    .and_then(|part| part.to_str())
                    .is_some_and(|part| part.eq_ignore_ascii_case("ini"))
                && !file
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.eq_ignore_ascii_case("namespaces.ini"))
        });
        let base = if in_data {
            folder
                .parent()
                .context("Patcher data folder has no parent")?
                .to_path_buf()
        } else {
            folder
        };
        let temp = tempfile::tempdir()?;
        let mods = temp.path().join("mods");
        let game = temp.path().join("game");
        std::fs::create_dir(&mods)?;
        std::fs::create_dir(&game)?;
        let projected = Roots::new(&mods, &game)?;
        let (original_root, projected_root, prefix) = if base.starts_with(&self.settings.roots.mods)
        {
            (
                &self.settings.roots.mods,
                &projected.mods,
                "<<modDirectory>>",
            )
        } else {
            (
                &self.settings.roots.game,
                &projected.game,
                "<<kotorDirectory>>",
            )
        };
        let relative_base = base.strip_prefix(original_root)?;
        let projected_base = projected_root.join(relative_base);
        let data = self.actual(&base.join("tslpatchdata"));
        let projected_data = projected_base.join("tslpatchdata");
        std::fs::create_dir_all(&projected_data)?;
        let mut total = 0u64;
        let namespace_path = data.join("namespaces.ini");
        let has_namespaces = self.copy_predicted_document(
            &namespace_path,
            &projected_data.join("namespaces.ini"),
            &mut total,
        )?;
        let variants = if has_namespaces {
            odypatcher::patcher::reader::NamespaceReader::from_filepath(
                &projected_data.join("namespaces.ini"),
            )
            .map_err(|error| anyhow::anyhow!("Read predicted patcher options: {error}"))?
        } else {
            Vec::new()
        };
        ensure!(
            variants.len() <= 1024,
            "Patcher offers more than 1024 options"
        );
        let mut documents = vec![PathBuf::from("changes.ini"), PathBuf::from("info.rtf")];
        if let Some(file) = selected_ini {
            documents.push(PathBuf::from(
                file.file_name().context("Patcher INI has no filename")?,
            ));
        }
        for variant in variants {
            let folder = crate::paths::relative(
                variant
                    .data_folderpath
                    .to_str()
                    .context("Patcher option path is not UTF-8")?,
                false,
            )?;
            documents.push(folder.join(crate::paths::relative(&variant.ini_filename, false)?));
            documents.push(folder.join(crate::paths::relative(&variant.info_filename, false)?));
        }
        documents.sort();
        documents.dedup();
        for document in documents {
            self.copy_predicted_document(
                &data.join(&document),
                &projected_data.join(document),
                &mut total,
            )?;
        }
        let projected_relative = match selected_ini {
            Some(file) => relative_base
                .join("tslpatchdata")
                .join(file.file_name().context("Patcher INI has no filename")?),
            None => relative_base.to_path_buf(),
        };
        let projected_source = format!(
            "{prefix}/{}",
            projected_relative
                .to_str()
                .context("Patcher path is not UTF-8")?
                .replace('\\', "/")
        );
        let mut preview =
            crate::editor::easy::patcher::inspect(&projected, &projected_source, namespace)?;
        preview.source = source.into();
        Ok(preview)
    }

    fn copy_predicted_document(
        &self,
        source: &Path,
        output: &Path,
        total: &mut u64,
    ) -> Result<bool> {
        self.check_cancelled()?;
        let Some((_, content)) = self.files.get(&self.path_policy.key(source)) else {
            return Ok(false);
        };
        let mut file = content.open()?;
        ensure!(
            file.metadata()?.len() <= 8 * 1024 * 1024,
            "Patcher document exceeds 8 MiB"
        );
        std::fs::create_dir_all(output.parent().context("Patcher document has no parent")?)?;
        let mut target = std::fs::File::create(output)?;
        let bytes = std::io::copy(
            &mut std::io::Read::by_ref(&mut file).take(8 * 1024 * 1024 + 1),
            &mut target,
        )?;
        ensure!(
            bytes <= 8 * 1024 * 1024,
            "Patcher document grew beyond 8 MiB"
        );
        *total = total
            .checked_add(bytes)
            .context("Patcher document byte count overflow")?;
        ensure!(
            *total <= 256 * 1024 * 1024,
            "Patcher preview exceeds 256 MiB"
        );
        Ok(true)
    }
}
