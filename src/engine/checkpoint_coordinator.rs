use super::*;
use crate::checkpoint::{ExpectedHead, InstallContext, ResumePlan, Store};

impl Engine {
    pub(super) fn run_checkpointed(
        &mut self,
        guide: &Guide,
        ordered: &[&Component],
        lock: &std::sync::Arc<recovery::Lock>,
        resume: Option<&ResumePlan>,
        emit: &mut impl FnMut(Event),
        observer: &mut InstallObserver<'_>,
    ) -> Result<()> {
        let all = guide.ordered()?;
        let definitions = all
            .iter()
            .map(serde_json::to_value)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let context = InstallContext {
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
                #[cfg(windows)]
                {
                    self.checkpoint_process = Some((std::sync::Arc::clone(lock), intent.clone()));
                }
                let execution = self.component(component, emit, Some(&mut *observer));
                #[cfg(windows)]
                {
                    self.checkpoint_process = None;
                }
                execution?;
                self.flush_managed()?;
                let committed =
                    store.commit_component(lock, &intent, &self.settings.cancelled, |_| {})?;
                expected.checkpoint_id = committed.checkpoint_id;
                expected.generation = committed.generation;
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
            return crate::patcher::checkpoint_job::Lease::prepare(
                lock,
                &self.settings.roots,
                intent,
            )?
            .run(command, timeout, &self.settings.cancelled);
        }
        crate::patcher::standalone::run_command(command, timeout, &self.settings.cancelled)
    }
}
