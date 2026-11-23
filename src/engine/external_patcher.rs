//! Original-source external execution; never enters native preparation/prediction.
use super::*;
use crate::archive_generation::external_patcher_source::ExternalPatcherSource;
use sha2::{Digest, Sha256};

impl Engine {
    pub(super) fn process_original_external(
        &mut self,
        instruction: &Instruction,
        source: &ExternalPatcherSource,
        emit: &mut impl FnMut(Event),
    ) -> Result<()> {
        ensure!(
            self.install && !self.exact_patcher_prediction,
            "External observation requires explicit real-disk execution"
        );
        ensure!(
            self.settings.allow_processes,
            "Original external backend requires --allow-processes"
        );
        ensure!(
            self.managed.is_none(),
            "Original external backend requires an independent checkpointed game tree"
        );
        ensure!(
            self.component_checkpoints,
            "Original external backend requires component checkpoints"
        );
        let detected = match crate::game::detect(&self.settings.roots.game) {
            crate::game::Detected::Kotor1 => Some(odypatcher::Game::K1),
            crate::game::Detected::Kotor2Legacy | crate::game::Detected::Kotor2Aspyr => {
                Some(odypatcher::Game::K2)
            }
            crate::game::Detected::Unknown => None,
        };
        ensure!(
            detected == Some(source.expected_game()),
            "Original source game differs from destination game"
        );
        let runner = self.settings.execute_runner.as_ref().context(
            "Original external backend requires explicit headless runner and request storage",
        )?;
        let requests = fs::canonicalize(&runner.request_dir)?;
        ensure!(
            !requests.starts_with(&self.settings.roots.game)
                && !requests.starts_with(&self.settings.roots.mods),
            "Backend request storage must be outside mod and game trees"
        );
        let (_stage, staged_root) =
            patcher_prepare::stage(source.original_package_root(), &self.settings.cancelled)?;
        source.verify_staged_original(&staged_root, &self.settings.cancelled)?;
        let raw_package =
            backend_contract::PackageIdentity::capture(&staged_root, &self.settings.cancelled)?;
        let ini_interpretation =
            source.prepare_source_ini(&staged_root, &self.settings.cancelled)?;
        let executable = staged_root.join(source.executable_relative_path());
        let base = executable
            .parent()
            .context("Original executable has no package directory")?;
        let selected_ini = staged_root.join(source.selected_ini_relative_path());
        ensure!(selected_ini.is_file(), "Original selected INI is missing");
        let packaged = self.settings.patcher_engine == PatcherEngine::ModPackaged;
        ensure!(
            packaged || self.settings.patcher_engine == PatcherEngine::HoloPatcher,
            "Invalid original external backend"
        );
        let invocation = if packaged {
            None
        } else {
            Some(
                self.patcher_invocation
                    .as_ref()
                    .context("HoloPatcher executable is unavailable")?,
            )
        };
        // Runner ownership covers the actual backend executable, including Wine
        // when it is a Windows binary; the source executable is not run by Holo.
        let runner_program = match invocation {
            Some(crate::patcher::Invocation::Python { entrypoint, .. }) => entrypoint.as_path(),
            Some(value) => value.native_path().context("HoloPatcher program missing")?,
            None => executable.as_path(),
        };
        let backend_identity = match invocation {
            Some(value) => backend_contract::invocation_identity(value, &self.settings.cancelled)?,
            None => serde_json::json!({ "kind": "originalPackagedExecutable",
                "program": backend_contract::file_identity(&executable, &self.settings.cancelled)?,
                "systemDependencyClosureVerified": false }),
        };
        let (lock, operation) = self
            .checkpoint_process
            .as_ref()
            .context("Original backend requires its owned Materializing checkpoint")?;
        let mut session = provided_installer::RunnerSession::start(
            runner,
            runner_program,
            &self.settings.roots,
            lock,
            operation,
        )?;
        let windows = runner_program
            .extension()
            .is_some_and(|part| part.eq_ignore_ascii_case("exe"));
        let mut command = if packaged || windows {
            runner.command(runner_program, &session.headless.display)?
        } else {
            let mut command = invocation
                .context("HoloPatcher invocation missing")?
                .command()?;
            command
                .env("DISPLAY", &session.headless.display)
                .env_remove("WAYLAND_DISPLAY")
                .env_remove("DBUS_SESSION_BUS_ADDRESS")
                .env_remove("SESSION_MANAGER")
                .env_remove("XAUTHORITY");
            command
        };
        command.current_dir(base);
        if !packaged {
            command
                .arg("--install")
                .arg("--yes")
                .arg("--non-interactive")
                .arg("--game-dir")
                .arg(&self.settings.roots.game)
                .arg("--tslpatchdata")
                .arg(base);
            if let Some(index) = source.selected_namespace() {
                command
                    .arg("--namespace-option-index")
                    .arg(index.to_string());
            }
        }
        // Remove historical logs after any exact source-deletion interpretation.
        // The request retains both raw and submitted package identities.
        clear_patcher_logs(base)?;
        let submitted_package =
            backend_contract::PackageIdentity::capture(&staged_root, &self.settings.cancelled)?;
        let request_id = uuid::Uuid::new_v4().to_string();
        let request_path = requests.join(format!("{request_id}.json"));
        let mut request = serde_json::json!({
            "schemaVersion": 1, "requestId": request_id,
            "status": if packaged { "awaitingHeadlessInteraction" } else { "runningExternalBackend" },
            "backend": if packaged { "mod-packaged" } else { "holo-patcher" },
            "componentGuid": source.component_guid(), "operationIndex": source.operation_index(),
            "instruction": instruction, "originalSourceAuthority": source.provenance(),
            "sourceExecutable": source.original_package_root().join(source.executable_relative_path()),
            "stagedExecutable": executable, "selectedIni": selected_ini,
            "executableSha256": backend_contract::file_identity(&executable, &self.settings.cancelled)?["sha256"],
            "namespaceIndex": source.selected_namespace(), "gameTarget": self.settings.roots.game,
            "headlessDisplay": session.headless.display, "checkpointOperations": [operation],
            "rawPackage": raw_package, "submittedPackage": submitted_package,
            "sourceIniInterpretation": ini_interpretation,
            "backendIdentity": backend_identity,
            "launch": backend_contract::launch_identity(&command)?,
            "argumentsPassedToExecutable": if packaged { serde_json::json!([]) } else { serde_json::Value::Null },
            "checkpointCommitVerified": false, "laneParityVerified": false,
            "effectPredictionVerified": false
        });
        recovery::atomic_write(&request_path, &serde_json::to_vec_pretty(&request)?)?;
        fs::File::open(&requests)?.sync_all()?;
        ensure!(
            backend_contract::PackageIdentity::capture(&staged_root, &self.settings.cancelled)?
                == submitted_package,
            "Original backend input changed before submission"
        );
        if let Some(value) = invocation {
            ensure!(
                backend_contract::invocation_identity(value, &self.settings.cancelled)?
                    == backend_identity,
                "HoloPatcher backend changed before submission"
            );
        }
        emit(Event {
            component: source.component_guid().to_owned(),
            step: source.operation_index() + 1,
            message: format!(
                "Original external backend request: {}",
                request_path.display()
            ),
        });
        session.mark_submitted();
        let result = self.run_external_command(command);
        let quiescence = session.finish(
            result
                .as_ref()
                .is_ok_and(|receipt| receipt.no_child_started || receipt.process_group_quiescent),
        );
        request["writerQuiescenceVerified"] = quiescence.is_ok().into();
        if let Ok(receipt) = &result {
            request["processReceipt"] = serde_json::json!({ "exitCode": receipt.exit_code,
                "cancelled": receipt.cancelled, "failure": receipt.failure, "stdout": receipt.stdout,
                "stderr": receipt.stderr, "processGroupQuiescent": receipt.process_group_quiescent,
                "noChildStarted": receipt.no_child_started });
            for output in [&receipt.stdout, &receipt.stderr] {
                if !output.is_empty() {
                    emit(Event {
                        component: source.component_guid().to_owned(),
                        step: source.operation_index() + 1,
                        message: output.clone(),
                    });
                }
            }
        } else if let Err(error) = &result {
            request["processError"] = format!("{error:#}").into();
        }
        let log = if quiescence.is_ok() {
            let captured = patcher_install_log(base);
            match &captured {
                Ok((path, content)) => {
                    request["installLog"] = serde_json::json!({"path": path,
                    "sha256": format!("{:x}", Sha256::digest(content.as_bytes())), "text": content});
                    emit(Event {
                        component: source.component_guid().to_owned(),
                        step: source.operation_index() + 1,
                        message: format!("Original backend log {}:\n{content}", path.display()),
                    });
                }
                Err(error) => request["installLogError"] = format!("{error:#}").into(),
            }
            Some(captured)
        } else {
            None
        };
        let artifacts = if quiescence.is_ok() {
            let retained = submitted_package.retain_changes(
                &staged_root,
                &requests.join(format!("{request_id}-artifacts")),
            );
            match &retained {
                Ok(value) => request["outputArtifacts"] = value.clone(),
                Err(error) => request["outputArtifactsError"] = format!("{error:#}").into(),
            }
            Some(retained)
        } else {
            None
        };
        let verification = (|| -> Result<()> {
            quiescence?;
            artifacts.context("Backend artifacts require writer quiescence")??;
            let receipt = result?;
            ensure!(
                !receipt.cancelled && receipt.failure.is_none() && receipt.exit_code == Some(0),
                "Original external backend failed; restore its checkpoint before retrying"
            );
            let (_, content) = log.context("Original backend logs require writer quiescence")??;
            ensure!(
                patcher_log_error_lines(&content).is_empty(),
                "Original external backend log reports errors; restore before retrying"
            );
            Ok(())
        })();
        request["status"] = if verification.is_ok() {
            "processAndLogVerified".into()
        } else {
            "failedRestoreRequired".into()
        };
        if let Err(error) = &verification {
            request["error"] = format!("{error:#}").into();
        }
        recovery::atomic_write(&request_path, &serde_json::to_vec_pretty(&request)?)?;
        fs::File::open(&requests)?.sync_all()?;
        verification?;
        self.scan()?;
        Ok(())
    }
}
