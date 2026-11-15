use super::*;
use crate::checkpoint::{ExpectedHead, InstallContext, ResumePlan, Store};

impl Engine {
    /// Build checkpoint authority from current engine configuration and full
    /// authored definitions. Selection changes between Base and Remaining do
    /// not alter source authority, while instructions and recipe bindings do.
    pub(super) fn checkpoint_backend_binding(
        &self,
        guide: &Guide,
    ) -> Result<Option<crate::checkpoint::BackendBinding>> {
        use crate::checkpoint::{BackendBinding, BackendKind};
        let backend = match self.settings.patcher_engine {
            PatcherEngine::OdyPatcher if self.patcher_invocation.is_none() => return Ok(None),
            PatcherEngine::OdyPatcher => BackendKind::OdyPatcher,
            PatcherEngine::KPatcher => BackendKind::KPatcher,
            PatcherEngine::HoloPatcher => BackendKind::HoloPatcher,
            PatcherEngine::ModPackaged => BackendKind::ModPackaged,
        };
        let mut invocation = if backend == BackendKind::ModPackaged {
            serde_json::json!({"kind": "modPackaged", "programAuthority": "authenticatedOriginalComponentPackage"})
        } else {
            backend_contract::invocation_identity(
                self.patcher_invocation
                    .as_ref()
                    .context("Checkpoint backend invocation is unavailable")?,
                &self.settings.cancelled,
            )?
        };
        if backend == BackendKind::ModPackaged {
            let runner = self
                .settings
                .execute_runner
                .as_ref()
                .context("Mod-packaged checkpoint requires a configured installer runner")?;
            let identity = |path: &std::path::Path| {
                backend_contract::invocation_identity(
                    &crate::patcher::Invocation::Native {
                        program: path.to_path_buf(),
                    },
                    &self.settings.cancelled,
                )
            };
            let windows = runner
                .windows
                .as_ref()
                .map(|windows| -> Result<serde_json::Value> {
                    Ok(serde_json::json!({
                        "wine": identity(&windows.executable)?,
                        "wineserver": identity(&windows.wineserver)?,
                    }))
                })
                .transpose()?;
            invocation["runner"] = serde_json::json!({
                "xvfb": identity(&runner.xvfb)?, "windows": windows,
            });
        }
        fn remove_component_selection(component: &mut serde_json::Value) {
            if let Some(object) = component.as_object_mut() {
                object.remove("IsSelected");
                if let Some(serde_json::Value::Array(options)) = object.get_mut("Options") {
                    for option in options {
                        remove_component_selection(option);
                    }
                }
            }
        }
        let mut authority = serde_json::to_value(guide)?;
        if let Some(serde_json::Value::Array(components)) = authority.get_mut("thisMod") {
            for component in components {
                remove_component_selection(component);
            }
        }
        Ok(Some(BackendBinding::new(backend, &invocation, &authority)?))
    }

    pub(super) fn run_checkpointed(
        &mut self,
        guide: &Guide,
        ordered: &[&Component],
        lock: &std::sync::Arc<recovery::Lock>,
        resume: Option<&ResumePlan>,
        emit: &mut impl FnMut(Event),
        observer: &mut InstallObserver<'_>,
    ) -> Result<()> {
        // Legacy completion proves a definition/tree match, not a predecessor's
        // checkpointed execution. Only a verified resume may inherit that
        // authority for an uninstalled owner with resolved source masters.
        ensure!(
            self.completed.is_empty()
                || resume.is_some()
                || !ordered.iter().any(|component| {
                    crate::archive_generation::reviewed_master::contains_resolved(component)
                        || crate::archive_generation::source_plan_namespace::required_predecessors(
                            component,
                        )
                        .map_or(true, |required| !required.is_empty())
                }),
            "Resolved source masters with an inherited completed prefix require verified checkpoint resume; resume from the committed checkpoint"
        );
        let all = guide.ordered()?;
        let definitions = all
            .iter()
            .map(serde_json::to_value)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let context = InstallContext {
            backend_binding: self.checkpoint_backend_binding(guide)?,
            initial_completed_components: definitions[..self.completed.len()].to_vec(),
            ordered_components: definitions,
            participants: self
                .managed
                .as_ref()
                .map(|managed| managed.service.checkpoint_participants())
                .transpose()?
                .unwrap_or_default(),
        };
        let store = Store::open(&self.settings.roots)?;
        let baseline = match resume {
            Some(plan) => store.branch_for_resume(lock, plan, &self.settings.cancelled, |_| {})?,
            None => store.begin_session_with_context(
                lock,
                &context,
                &self.settings.cancelled,
                |_| {},
            )?,
        };
        // The store verified the current inventory and the exact completed prefix.
        // Extend this authority only after each component checkpoint commits.
        self.checkpoint_completed = self.completed.clone();
        self.completed_source_definitions.clear();
        self.supplemental_source_guide = Some(guide.clone());
        if resume.is_some() {
            for component in all.iter().take(self.completed.len()) {
                self.completed_source_definitions.insert(
                    crate::guid::key_ascii(&component.guid),
                    (*component).clone(),
                );
            }
        }
        observer.current.snapshot_pending = true;
        observer.emit(
            InstallPhase::CreatingSnapshot,
            "Component checkpoint baseline ready",
        );
        let mut expected = ExpectedHead {
            session_id: baseline.session_id,
            checkpoint_id: baseline.checkpoint_id,
            generation: baseline.generation,
        };
        emit(Event {
            component: String::new(),
            step: 0,
            message: format!(
                "Component checkpoint session {} at {}",
                expected.session_id, expected.checkpoint_id
            ),
        });
        if let Some(binding) = crate::k2_preparation::verify(guide)? {
            ensure!(
                self.managed.is_none(),
                "K2 preparation cannot use managed deployment"
            );
            let inspection = store.inspect(&expected.session_id, Some(&expected.checkpoint_id))?;
            if let Some(committed) = inspection.checkpoint.preparation.as_ref() {
                self.prepare_k2(guide, Some(committed), emit)?;
            } else {
                let intent =
                    store.begin_preparation(lock, &expected, &self.settings.cancelled, &binding)?;
                let intent =
                    store.mark_component_running(lock, &intent, &self.settings.cancelled)?;
                self.checkpoint_process = Some((std::sync::Arc::clone(lock), intent.clone()));
                let result = self.prepare_k2(guide, None, emit);
                self.checkpoint_process = None;
                result?;
                let committed =
                    store.commit_component(lock, &intent, &self.settings.cancelled, |_| {})?;
                expected.checkpoint_id = committed.checkpoint_id;
                expected.generation = committed.generation;
            }
        }
        self.selected = selected_guid_keys(guide);
        self.selected_names = guide
            .components
            .iter()
            .filter(|component| component.is_selected)
            .map(|component| component.name.clone())
            .collect();
        for (index, component) in ordered.iter().enumerate() {
            let started = std::time::Instant::now();
            let result: Result<()> = (|| {
                self.check_cancelled()?;
                ensure!(
                    !component.instructions.is_empty(),
                    "{} has no executable instructions",
                    component.name
                );
                self.check_component(component)?;
                let intent = store.begin_component(lock, &expected, &self.settings.cancelled)?;
                let intent =
                    store.mark_component_running(lock, &intent, &self.settings.cancelled)?;
                if let Some(managed) = &mut self.managed {
                    managed
                        .service
                        .initialize_checkpointed(lock, &self.settings.roots)?;
                    managed.begin(component)?;
                    self.scan()?;
                    self.install = false;
                    let simulated = self.component(component, &mut |_| {}, None);
                    self.install = true;
                    self.stage_managed_writes = false;
                    simulated.context("Managed component preflight failed after journaled reinstall preparation; restore checkpoint before retrying")?;
                    self.scan()?;
                }
                observer.component(component, index + 1);
                observer.emit(
                    InstallPhase::InstallingComponent,
                    format!("Installing: {}", component.name),
                );
                self.checkpoint_process = Some((std::sync::Arc::clone(lock), intent.clone()));
                let execution = self.component(component, emit, Some(&mut *observer));
                self.checkpoint_process = None;
                execution?;
                self.flush_managed()?;
                let committed =
                    store.commit_component(lock, &intent, &self.settings.cancelled, |_| {})?;
                expected.checkpoint_id = committed.checkpoint_id;
                expected.generation = committed.generation;
                self.checkpoint_completed
                    .insert(crate::guid::key_ascii(&component.guid));
                observer.component(component, index + 1);
                observer.emit(
                    InstallPhase::ComponentCompleted,
                    format!("Installed and checkpointed: {}", component.name),
                );
                Ok(())
            })();
            let error = result.as_ref().err().map(|error| format!("{error:#}"));
            crate::telemetry::global().record_mod_installation(
                &component.name,
                result.is_ok(),
                started.elapsed(),
                error.as_deref(),
            );
            result?;
        }
        observer.clear_component();
        observer.emit(
            InstallPhase::RecordingCompletion,
            "Finalizing checkpoint session and installation receipt",
        );
        let checkpoint = CheckpointRunReport {
            session_id: expected.session_id.clone(),
            checkpoint_id: expected.checkpoint_id.clone(),
            generation: expected.generation,
        };
        let report = InstallReport {
            managed: self.managed.as_ref().map(|managed| managed.report.clone()),
            checkpoint: Some(checkpoint.clone()),
        };
        store.finalize_session_with_report(lock, &expected, &self.settings.cancelled, &report)?;
        self.checkpoint_report = Some(checkpoint);
        Ok(())
    }
}

impl Engine {
    pub(super) fn run_external_command(
        &self,
        command: Command,
    ) -> Result<crate::patcher::standalone::CommandReceipt> {
        let timeout = std::time::Duration::from_secs(24 * 60 * 60);
        #[cfg(windows)]
        if self.component_checkpoints {
            let (lock, intent) = self.checkpoint_process.as_ref().context(
                "Checkpoint external execution requires the active Materializing operation",
            )?;
            eprintln!(
                "Windows checkpoint Job Object execution retains its explicit one MiB per-stream limit; complete live forwarding is unavailable on this route"
            );
            return crate::patcher::checkpoint_job::Lease::prepare(
                lock,
                &self.settings.roots,
                intent,
            )?
            .run(command, timeout, &self.settings.cancelled);
        }
        crate::patcher::standalone::run_command_streaming(
            command,
            timeout,
            &self.settings.cancelled,
        )
    }
}
