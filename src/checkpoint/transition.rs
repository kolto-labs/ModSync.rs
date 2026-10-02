use super::{
    model::*,
    read::*,
    store::{
        Store, canonical_bytes, ensure_directory, namespace_boundary, publish_json, replace_bytes,
        replace_json,
    },
};
use crate::recovery;
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    sync::atomic::{AtomicBool, Ordering},
};

/// Read-only process fence state. A present record never implies quiescence.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalProcessStatus {
    pub present: bool,
    pub session_id: Option<String>,
    pub operation_id: Option<String>,
    pub launch_id: Option<String>,
    pub phase: Option<String>,
}
/// Process settlement is separate from the subsequent checkpoint recovery result.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessQuiescenceReport {
    pub had_record: bool,
    pub launch_id: Option<String>,
    pub absent_job: bool,
}
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRecoveryReport {
    pub schema_version: u32,
    pub operation: &'static str,
    pub quiescence: ProcessQuiescenceReport,
    pub complete: bool,
    pub recovery: Option<RecoveryReport>,
    pub recovery_error: Option<String>,
}

impl Store {
    /// Validate the process record and component binding without opening a job.
    pub fn external_process_status(&self) -> Result<ExternalProcessStatus> {
        #[cfg(windows)]
        {
            let status = crate::patcher::checkpoint_job::status(&self.roots)?;
            Ok(ExternalProcessStatus {
                present: status.present,
                session_id: status.session_id,
                operation_id: status.operation_id,
                launch_id: status.launch_id,
                phase: status.phase,
            })
        }
        #[cfg(not(windows))]
        {
            Ok(ExternalProcessStatus {
                present: false,
                session_id: None,
                operation_id: None,
                launch_id: None,
                phase: None,
            })
        }
    }

    pub(super) fn ensure_other_sessions_settled(&self, source: &str) -> Result<()> {
        for summary in self.list()?.sessions {
            if summary.session_id == source || summary.checkpoint_id.is_none() {
                continue;
            }
            let (session, head, _) = self.checked_history(&summary.session_id)?;
            ensure!(
                session.install_scope != "install"
                    || matches!(head.status.as_str(), "Complete" | "Detached"),
                "Another unfinished checkpoint session requires explicit recovery: {}",
                session.session_id
            );
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn publish_branch(
        &self,
        source: &Session,
        target: &Checkpoint,
        context: &InstallContext,
        kind: &str,
        mut baseline: Checkpoint,
        safety_session_id: Option<String>,
        flag: &AtomicBool,
    ) -> Result<(Session, Checkpoint)> {
        cancelled(flag)?;
        let session_id = uuid::Uuid::new_v4().to_string();
        let checkpoint_id = uuid::Uuid::new_v4().to_string();
        let session = Session {
            schema_version: SCHEMA_VERSION,
            session_id: session_id.clone(),
            created_utc: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            roots: self.binding.clone(),
            install_scope: "install".into(),
            ordered_components: context.ordered_components.clone(),
            initial_completed_components: target.completed_components.clone(),
            participants: source.participants.clone(),
            ancestor_shells: source.ancestor_shells.clone(),
            definitions_sha256: Some(super::history::definition_hash(
                &context.ordered_components,
            )?),
            parent_session_id: Some(source.session_id.clone()),
            parent_checkpoint_id: Some(target.checkpoint_id.clone()),
            branch_kind: Some(kind.into()),
            safety_session_id,
            origin: super::persistence::writer_origin()?.into(),
        };
        baseline.session_id = session_id.clone();
        baseline.checkpoint_id = checkpoint_id;
        baseline.parent_id = None;
        baseline.sequence = 0;
        baseline.kind = "Baseline".into();
        baseline.component = None;
        baseline.completed_components = target.completed_components.clone();
        baseline.completed_sha256 = Some(super::history::definition_hash(
            &target.completed_components,
        )?);
        let directory = self.session_directory(&session_id)?;
        ensure!(safe_path(&directory)?.is_none(), "Branch UUID collision");
        fs::create_dir(&directory)?;
        namespace_boundary(&self.directory.join("sessions"))?;
        ensure_directory(&directory.join("checkpoints"))?;
        publish_json(&directory.join("session.json"), &session)?;
        publish_json(
            &directory
                .join("checkpoints")
                .join(format!("{}.json", baseline.checkpoint_id)),
            &baseline,
        )?;
        ensure!(
            self.read_session(&session_id)? == session
                && self.read_checkpoint(&session_id, &baseline.checkpoint_id)? == baseline,
            "Branch metadata changed after publication"
        );
        self.verify_target(&session, &baseline, flag)?;
        Ok((session, baseline))
    }

    pub(super) fn validate_transition_intent(
        &self,
        operation: &OperationIntent,
    ) -> Result<(Session, Head, Session, Checkpoint)> {
        ensure!(
            operation.schema_version == SCHEMA_VERSION && operation.roots == self.binding,
            "Transition schema or roots differ"
        );
        id(&operation.operation_id)?;
        id(&operation.session_id)?;
        id(&operation.from_checkpoint_id)?;
        digest(&operation.component_sha256)?;
        ensure!(
            matches!(
                operation.kind.as_str(),
                "Restore" | "Resume" | "SourceRebase"
            ) && matches!(operation.phase.as_str(), "Materializing" | "Verified")
                && operation.component_definition.is_null()
                && operation.finalization_receipt.is_none(),
            "Invalid transition operation shape"
        );
        let binding = operation
            .transition
            .as_ref()
            .context("Transition has no binding")?;
        ensure!(
            matches!(
                binding.source_status.as_str(),
                "Ready" | "Complete" | "Restored" | "CleanupRequired" | "FinalizeCleanupRequired"
            ),
            "Invalid transition source status"
        );
        for value in [
            &binding.target_checkpoint_id,
            &binding.branch_session_id,
            &binding.branch_checkpoint_id,
            &binding.source_operation_id,
        ] {
            id(value)?;
        }
        if let Some(safety) = &binding.safety_session_id {
            id(safety)?;
        }
        ensure!(
            binding.branch_session_id != operation.session_id
                && binding.branch_kind == operation.kind
                && matches!(
                    binding.action.as_str(),
                    "restoredSelectedBoundary"
                        | "restoredFailedBoundary"
                        | "restoredPreviousBoundary"
                        | "restoredFinalBoundary"
                        | "resumed"
                        | "sourceRebased"
                ),
            "Invalid transition identity or outcome"
        );
        let (source, head, history) = self.checked_history(&operation.session_id)?;
        ensure!(
            head.generation == operation.expected_generation
                && head.checkpoint_id == operation.from_checkpoint_id,
            "Transition source HEAD advanced"
        );
        ensure!(
            (head.status == binding.source_status
                && head.operation_id == binding.source_operation_id)
                || (head.status == "Detached" && head.operation_id == operation.operation_id),
            "Transition source HEAD identity differs"
        );
        let target = history
            .iter()
            .find(|checkpoint| checkpoint.checkpoint_id == binding.target_checkpoint_id)
            .context("Transition target is not committed ancestry")?;
        let branch = self.read_session(&binding.branch_session_id)?;
        let baseline =
            self.read_checkpoint(&binding.branch_session_id, &binding.branch_checkpoint_id)?;
        ensure!(
            branch.safety_session_id == binding.safety_session_id
                && branch.parent_session_id.as_deref() == Some(operation.session_id.as_str())
                && branch.parent_checkpoint_id.as_deref()
                    == Some(binding.target_checkpoint_id.as_str())
                && branch.branch_kind.as_deref() == Some(binding.branch_kind.as_str())
                && branch.initial_completed_components == target.completed_components
                && branch.participants == source.participants
                && branch.ancestor_shells == source.ancestor_shells
                && baseline.kind == "Baseline"
                && baseline.sequence == 0
                && baseline.game == target.game
                && baseline.auxiliary_snapshots == target.auxiliary_snapshots
                && baseline.ancestor_shells == target.ancestor_shells
                && (binding.branch_kind == "SourceRebase" || baseline.mods == target.mods)
                && operation.component_sha256
                    == super::history::definition_hash(&branch.ordered_components)?,
            "Transition branch differs from immutable target binding"
        );
        ensure!(
            binding.branch_kind != "Restore"
                || branch.ordered_components == source.ordered_components,
            "Restore branch changed the ordered component definitions"
        );
        if let Some(metadata) = safe_path(
            &self
                .session_directory(&branch.session_id)?
                .join("HEAD.json"),
        )? {
            ensure!(metadata.is_file(), "Branch HEAD is not a file");
            let branch_head = self.read_head(&branch.session_id)?;
            ensure!(
                branch_head.checkpoint_id == baseline.checkpoint_id
                    && branch_head.generation == 1
                    && branch_head.operation_id == operation.operation_id
                    && matches!(
                        branch_head.status.as_str(),
                        "TransitionCleanupRequired" | "Ready" | "Restored"
                    ),
                "Transition branch HEAD differs"
            );
            if branch_head.status == "TransitionCleanupRequired" {
                let mut verified = operation.clone();
                verified.phase = "Verified".into();
                ensure!(
                    branch_head.transition_operation.as_ref() == Some(&verified),
                    "Branch cleanup fence differs from operation"
                );
            }
        }
        Ok((source, head, branch, baseline))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn publish_transition(
        &self,
        source: &Session,
        source_head: &Head,
        target: &Checkpoint,
        branch: &Session,
        baseline: &Checkpoint,
        kind: &str,
        action: &str,
        safety: Option<String>,
        replaces: Option<&OperationIntent>,
    ) -> Result<OperationIntent> {
        let operation = OperationIntent {
            schema_version: SCHEMA_VERSION,
            operation_id: replaces
                .map(|operation| operation.operation_id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            kind: kind.into(),
            session_id: source.session_id.clone(),
            expected_generation: source_head.generation,
            from_checkpoint_id: source_head.checkpoint_id.clone(),
            roots: self.binding.clone(),
            component_definition: serde_json::Value::Null,
            component_sha256: super::history::definition_hash(&branch.ordered_components)?,
            phase: "Materializing".into(),
            finalization_receipt: None,
            transition: Some(TransitionBinding {
                target_checkpoint_id: target.checkpoint_id.clone(),
                branch_session_id: branch.session_id.clone(),
                branch_checkpoint_id: baseline.checkpoint_id.clone(),
                source_operation_id: source_head.operation_id.clone(),
                source_status: source_head.status.clone(),
                safety_session_id: safety,
                action: action.into(),
                branch_kind: kind.into(),
            }),
        };
        ensure!(
            self.read_head(&source.session_id)? == *source_head,
            "Source HEAD changed before transition intent"
        );
        ensure_directory(&self.directory.join("operations"))?;
        if let Some(previous) = replaces {
            ensure!(
                self.pending_operations()? == [previous.clone()],
                "Superseded operation changed"
            );
            replace_json(&self.operation_path(previous)?, &operation)?;
        } else {
            self.ensure_no_pending()?;
            publish_json(&self.operation_path(&operation)?, &operation)?;
        }
        ensure!(
            self.pending_operations()? == [operation.clone()],
            "Transition intent changed after publication"
        );
        Ok(operation)
    }

    pub(super) fn finish_transition(
        &self,
        lock: &recovery::Lock,
        operation: &OperationIntent,
        flag: &AtomicBool,
        observe: &mut impl FnMut(Progress),
    ) -> Result<RestoreReport> {
        self.require_lock(lock)?;
        ensure!(
            self.pending_operations()? == [operation.clone()],
            "Transition capability differs from durable operation"
        );
        let (source, source_head, branch, baseline) = self.validate_transition_intent(operation)?;
        let binding = operation
            .transition
            .as_ref()
            .context("Missing transition binding")?;
        if operation.kind == "Restore" || !self.live_matches(&branch, &baseline, flag)? {
            self.materialize(&branch, &baseline, &operation.operation_id, flag, observe)?;
        }
        cancelled(flag)?;
        // The same fixed target is now verified. Receipt and legacy compatibility
        // snapshot reconciliation are idempotent under the surviving intent.
        self.reconcile_prefix(&baseline)?;
        recovery::rebase_checkpoint_snapshot(&self.roots, lock, flag)?;
        recovery::verify_checkpoint_legacy_snapshot(
            &self.roots,
            &super::finalize::game_files(&baseline.mods),
            &super::finalize::game_files(&baseline.game),
        )?;
        ensure!(
            self.live_matches(&branch, &baseline, &AtomicBool::new(false))?,
            "Restored state changed during receipt reconciliation"
        );
        ensure!(
            self.read_head(&source.session_id)? == source_head,
            "Source HEAD changed during transition"
        );
        cancelled(flag)?;
        let mut verified = operation.clone();
        verified.phase = "Verified".into();
        replace_json(&self.operation_path(operation)?, &verified)?;
        let cleanup = Head {
            schema_version: SCHEMA_VERSION,
            generation: 1,
            checkpoint_id: baseline.checkpoint_id.clone(),
            status: "TransitionCleanupRequired".into(),
            operation_id: operation.operation_id.clone(),
            finalization_receipt: None,
            transition_operation: Some(verified.clone()),
        };
        replace_json(
            &self
                .session_directory(&branch.session_id)?
                .join("HEAD.json"),
            &cleanup,
        )?;
        let detached = Head {
            status: "Detached".into(),
            operation_id: operation.operation_id.clone(),
            transition_operation: None,
            ..source_head
        };
        replace_json(
            &self
                .session_directory(&source.session_id)?
                .join("HEAD.json"),
            &detached,
        )?;
        ensure!(
            self.pending_operations()? == [verified.clone()],
            "Transition fence changed before cleanup"
        );
        match fs::remove_file(self.operation_path(operation)?) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        namespace_boundary(&self.directory.join("operations"))?;
        let ready = Head {
            status: if operation.kind == "Restore" {
                "Restored"
            } else {
                "Ready"
            }
            .into(),
            transition_operation: None,
            ..cleanup
        };
        replace_json(
            &self
                .session_directory(&branch.session_id)?
                .join("HEAD.json"),
            &ready,
        )?;
        replace_json(
            &self.directory.join("active.json"),
            &serde_json::json!({"sessionId": branch.session_id}),
        )?;
        ensure!(
            self.read_head(&branch.session_id)? == ready,
            "Branch HEAD changed after transition"
        );
        Ok(RestoreReport {
            schema_version: SCHEMA_VERSION,
            operation: "restoreCheckpoint",
            action: binding.action.clone(),
            source_session_id: source.session_id,
            source_checkpoint_id: operation.from_checkpoint_id.clone(),
            target_checkpoint_id: binding.target_checkpoint_id.clone(),
            session_id: branch.session_id,
            checkpoint_id: baseline.checkpoint_id,
            generation: 1,
            completed_components: baseline.completed_components.len(),
            safety_session_id: binding.safety_session_id.clone(),
        })
    }

    fn reconcile_prefix(&self, checkpoint: &Checkpoint) -> Result<()> {
        let path = recovery::state_dir(&self.roots).join("completion.json");
        if checkpoint.completed_components.is_empty() {
            if let Some(metadata) = safe_path(&path)? {
                ensure!(metadata.is_file(), "Completion receipt is not a file");
                fs::remove_file(&path)?;
                namespace_boundary(path.parent().context("Receipt has no parent")?)?;
            }
        } else {
            let bytes = canonical_bytes(&recovery::completion_value_from_manifest(
                &self.roots,
                &checkpoint.completed_components,
                &super::finalize::game_files(&checkpoint.game),
            )?)?;
            replace_bytes(&path, &bytes)?;
            recovery::verify_completion_manifest(
                &self.roots,
                &checkpoint.completed_components,
                &super::finalize::game_files(&checkpoint.game),
            )?;
        }
        Ok(())
    }

    pub fn restore_checkpoint(
        &self,
        lock: &recovery::Lock,
        expected: &ExpectedHead,
        target_checkpoint: &str,
        flag: &AtomicBool,
        mut observe: impl FnMut(Progress),
    ) -> Result<RestoreReport> {
        self.require_lock(lock)?;
        cancelled(flag)?;
        self.ensure_no_pending()?;
        self.restore_boundary(
            lock,
            expected,
            target_checkpoint,
            "restoredSelectedBoundary",
            None,
            flag,
            &mut observe,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn restore_boundary(
        &self,
        lock: &recovery::Lock,
        expected: &ExpectedHead,
        target_checkpoint: &str,
        action: &str,
        replaces: Option<&OperationIntent>,
        flag: &AtomicBool,
        observe: &mut impl FnMut(Progress),
    ) -> Result<RestoreReport> {
        let (source, head, history) = self.checked_history(&expected.session_id)?;
        ensure!(
            head.generation == expected.generation
                && head.checkpoint_id == expected.checkpoint_id
                && head.status != "Detached",
            "Restore source HEAD is stale or detached"
        );
        self.ensure_other_sessions_settled(&source.session_id)?;
        let target = history
            .iter()
            .find(|checkpoint| checkpoint.checkpoint_id == target_checkpoint)
            .context("Restore target is not in committed ancestry")?;
        self.verify_target(&source, target, flag)?;
        self.preflight_materialization(&source, target, flag)?;
        let safety = if replaces.is_none() {
            let report = self.capture_locked(
                lock,
                &InstallContext {
                    participants: source.participants.clone(),
                    ..InstallContext::default()
                },
                "safety",
                Some(&source.ancestor_shells),
                flag,
                &mut *observe,
            )?;
            Some(report.session_id)
        } else {
            None
        };
        let context = InstallContext {
            ordered_components: source.ordered_components.clone(),
            initial_completed_components: target.completed_components.clone(),
            participants: source.participants.clone(),
        };
        let (branch, baseline) = self.publish_branch(
            &source,
            target,
            &context,
            "Restore",
            target.clone(),
            safety.clone(),
            flag,
        )?;
        cancelled(flag)?;
        let operation = self.publish_transition(
            &source, &head, target, &branch, &baseline, "Restore", action, safety, replaces,
        )?;
        self.finish_transition(lock, &operation, flag, observe)
    }

    /// Explicit restart recovery: prove the selected operation's ownership before
    /// signaling a named job, then invoke the ordinary fenced recovery protocol.
    pub fn recover_with_process_quiescence(
        &self,
        lock: &recovery::Lock,
        session: Option<&str>,
        flag: &AtomicBool,
        observe: impl FnMut(Progress),
    ) -> Result<ProcessRecoveryReport> {
        ensure!(
            lock.guards(&self.roots),
            "Process recovery requires matching held root locks"
        );
        self.prepare_write()?;
        cancelled(flag)?;
        if let Some(selected) = session {
            id(selected)?;
            self.read_session(selected)?;
        }
        let pending = self.pending_operations()?;
        ensure!(
            pending.len() <= 1,
            "Multiple pending operations cannot authorize process recovery"
        );
        if let Some(operation) = pending.first() {
            ensure!(
                session.is_none_or(|selected| selected == operation.session_id
                    || operation
                        .transition
                        .as_ref()
                        .is_some_and(|binding| selected == binding.branch_session_id)),
                "Selected session does not own pending operation"
            );
        }
        let external = self.external_process_status()?;
        if external.present {
            let operation = pending
                .first()
                .context("External process has no authoritative pending operation")?;
            ensure!(
                external.session_id.as_deref() == Some(operation.session_id.as_str())
                    && external.operation_id.as_deref() == Some(operation.operation_id.as_str())
                    && session
                        .is_none_or(|selected| Some(selected) == external.session_id.as_deref()),
                "Selected session does not own the external process record"
            );
        }
        #[cfg(windows)]
        let quiescence = {
            let report = crate::patcher::checkpoint_job::recover(lock, &self.roots, flag)?;
            ProcessQuiescenceReport {
                had_record: report.had_record,
                launch_id: report.launch_id,
                absent_job: report.absent_job,
            }
        };
        #[cfg(not(windows))]
        let quiescence = ProcessQuiescenceReport {
            had_record: false,
            launch_id: None,
            absent_job: false,
        };
        let result = self.recover_pending(lock, session, flag, observe);
        let (recovery, recovery_error) = match result {
            Ok(report) => (Some(report), None),
            Err(error) => (None, Some(format!("{error:#}"))),
        };
        Ok(ProcessRecoveryReport {
            schema_version: SCHEMA_VERSION,
            operation: "recoverCheckpointWithProcessQuiescence",
            quiescence,
            complete: recovery.is_some(),
            recovery,
            recovery_error,
        })
    }

    pub fn recover_pending(
        &self,
        lock: &recovery::Lock,
        session: Option<&str>,
        flag: &AtomicBool,
        mut observe: impl FnMut(Progress),
    ) -> Result<RecoveryReport> {
        self.require_lock(lock)?;
        cancelled(flag)?;
        if let Some(selected) = session {
            id(selected)?;
            self.read_session(selected)?;
        }
        let pending = self.pending_operations()?;
        if let Some(operation) = pending.first() {
            ensure!(
                session.is_none_or(|session| session == operation.session_id
                    || operation
                        .transition
                        .as_ref()
                        .is_some_and(|binding| session == binding.branch_session_id)),
                "Selected session does not own pending operation"
            );
            if operation.transition.is_some() {
                let restored = self.finish_transition(lock, operation, flag, &mut observe)?;
                return Ok(recovery_report(restored));
            }
            let (source, head, history) = self.checked_history(&operation.session_id)?;
            self.ensure_other_sessions_settled(&source.session_id)?;
            let current = history.last().context("Recovery HEAD missing")?;
            let verified = self
                .verify_target(&source, current, flag)
                .and_then(|_| self.live_matches(&source, current, flag));
            if flag.load(Ordering::Relaxed) {
                cancelled(flag)?;
            }
            if operation.kind == "Finalize" && matches!(verified, Ok(true)) {
                self.recover_finalize(lock, operation, flag)?;
                return Ok(RecoveryReport {
                    schema_version: SCHEMA_VERSION,
                    operation: "recoverCheckpoint",
                    action: "finalized".into(),
                    restored: None,
                    session_id: Some(source.session_id),
                    checkpoint_id: Some(head.checkpoint_id),
                });
            }
            if operation.kind == "InstallComponent"
                && head.checkpoint_id != operation.from_checkpoint_id
                && matches!(verified, Ok(true))
            {
                self.recover_component_cleanup(operation, &head)?;
                return Ok(RecoveryReport {
                    schema_version: SCHEMA_VERSION,
                    operation: "recoverCheckpoint",
                    action: "committedBoundaryVerified".into(),
                    restored: None,
                    session_id: Some(source.session_id),
                    checkpoint_id: Some(head.checkpoint_id),
                });
            }
            let (target, action) = if operation.kind == "Finalize" {
                (head.checkpoint_id.as_str(), "restoredFinalBoundary")
            } else if head.checkpoint_id != operation.from_checkpoint_id {
                (
                    operation.from_checkpoint_id.as_str(),
                    "restoredPreviousBoundary",
                )
            } else {
                (
                    operation.from_checkpoint_id.as_str(),
                    "restoredFailedBoundary",
                )
            };
            let restored = self.restore_boundary(
                lock,
                &ExpectedHead {
                    session_id: source.session_id,
                    checkpoint_id: head.checkpoint_id.clone(),
                    generation: head.generation,
                },
                target,
                action,
                Some(operation),
                flag,
                &mut observe,
            )?;
            return Ok(recovery_report(restored));
        }
        let mut candidates = Vec::new();
        for summary in self.list()?.sessions {
            if summary.checkpoint_id.is_none() || session.is_some_and(|id| id != summary.session_id)
            {
                continue;
            }
            let (saved, head, _) = self.checked_history(&summary.session_id)?;
            if saved.install_scope == "install"
                && !matches!(head.status.as_str(), "Complete" | "Detached")
            {
                candidates.push((saved, head));
            }
        }
        ensure!(
            candidates.len() <= 1,
            "Multiple unfinished sessions; choose an explicit session for recovery"
        );
        if let Some((source, head)) = candidates.pop() {
            let restored = self.restore_boundary(
                lock,
                &ExpectedHead {
                    session_id: source.session_id,
                    checkpoint_id: head.checkpoint_id.clone(),
                    generation: head.generation,
                },
                &head.checkpoint_id,
                "restoredSelectedBoundary",
                None,
                flag,
                &mut observe,
            )?;
            return Ok(recovery_report(restored));
        }
        Ok(RecoveryReport {
            schema_version: SCHEMA_VERSION,
            operation: "recoverCheckpoint",
            action: "nothingPending".into(),
            restored: None,
            session_id: session.map(str::to_owned),
            checkpoint_id: None,
        })
    }

    fn recover_component_cleanup(&self, operation: &OperationIntent, head: &Head) -> Result<()> {
        let cleanup = Head {
            status: "CleanupRequired".into(),
            ..head.clone()
        };
        replace_json(
            &self
                .session_directory(&operation.session_id)?
                .join("HEAD.json"),
            &cleanup,
        )?;
        match fs::remove_file(self.operation_path(operation)?) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        namespace_boundary(&self.directory.join("operations"))?;
        let ready = Head {
            status: "Ready".into(),
            ..cleanup
        };
        replace_json(
            &self
                .session_directory(&operation.session_id)?
                .join("HEAD.json"),
            &ready,
        )
    }
}
fn recovery_report(restored: RestoreReport) -> RecoveryReport {
    RecoveryReport {
        schema_version: SCHEMA_VERSION,
        operation: "recoverCheckpoint",
        action: restored.action.clone(),
        session_id: Some(restored.session_id.clone()),
        checkpoint_id: Some(restored.checkpoint_id.clone()),
        restored: Some(restored),
    }
}
