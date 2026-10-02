use super::{
    history::{component_identity, definition_hash},
    model::*,
    read::*,
    store::{Store, ensure_directory, namespace_boundary, publish_json, replace_json},
};
use crate::recovery;
use anyhow::{Context, Result, ensure};
use std::{fs, sync::atomic::AtomicBool};

impl Store {
    /// Valid pending records remain visible after a crash, including HEAD publication
    /// followed by failed journal cleanup. Readers never repair or erase them.
    pub fn pending_operations(&self) -> Result<Vec<OperationIntent>> {
        self.validate_location()?;
        let directory = self.directory.join("operations");
        if safe_path(&directory)?.is_none() {
            return self.with_cleanup_heads(Vec::new());
        }
        let mut operations = Vec::new();
        let mut count = 0;
        for entry in fs::read_dir(&directory)? {
            count += 1;
            ensure!(
                count <= 10_000,
                "Checkpoint operation inventory exceeds bound"
            );
            let entry = entry?;
            ensure!(
                safe_path(&entry.path())?.is_some_and(|m| m.is_file()),
                "Operation entry is not a regular file"
            );
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-UTF8 operation name"))?;
            if name.starts_with(".tmp-") {
                continue;
            }
            let operation_id = name
                .strip_suffix(".json")
                .context("Unknown checkpoint operation artifact")?;
            id(operation_id)?;
            let operation: OperationIntent = json(&entry.path())?;
            ensure!(
                operation.schema_version == SCHEMA_VERSION
                    && operation.operation_id == operation_id
                    && matches!(
                        operation.kind.as_str(),
                        "InstallComponent" | "Finalize" | "Restore" | "Resume" | "SourceRebase"
                    )
                    && operation.roots == self.binding
                    && matches!(
                        operation.phase.as_str(),
                        "Intent" | "Materializing" | "Verified"
                    ),
                "Invalid or foreign checkpoint operation"
            );
            id(&operation.session_id)?;
            id(&operation.from_checkpoint_id)?;
            digest(&operation.component_sha256)?;
            if operation.transition.is_some()
                || matches!(
                    operation.kind.as_str(),
                    "Restore" | "Resume" | "SourceRebase"
                )
            {
                self.validate_transition_intent(&operation)?;
                operations.push(operation);
                ensure!(
                    operations.len() <= 1,
                    "Multiple unresolved operations share the same root binding"
                );
                continue;
            }
            ensure!(
                matches!(operation.phase.as_str(), "Intent" | "Materializing"),
                "Invalid component/finalize phase"
            );
            let (session, head, history) = self.checked_history(&operation.session_id)?;
            ensure!(
                session.install_scope == "install",
                "Operation is not bound to an install session"
            );
            let parent = history
                .iter()
                .find(|checkpoint| checkpoint.checkpoint_id == operation.from_checkpoint_id)
                .context("Operation parent is not committed ancestry")?;
            ensure!(
                operation.expected_generation
                    == parent
                        .sequence
                        .checked_add(1)
                        .context("Operation generation overflow")?,
                "Operation generation does not match parent"
            );
            if operation.kind == "Finalize" {
                self.validate_finalize_intent(&operation, &session, &head, parent)?;
            } else {
                ensure!(
                    operation.finalization_receipt.is_none()
                        && operation.component_sha256
                            == definition_hash(std::slice::from_ref(
                                &operation.component_definition
                            ))?,
                    "Component intent receipt or definition differs"
                );
                ensure!(
                    session
                        .ordered_components
                        .get(parent.completed_components.len())
                        == Some(&operation.component_definition),
                    "Operation is not the next immutable component"
                );
                let uncommitted = head.generation == operation.expected_generation
                    && head.checkpoint_id == operation.from_checkpoint_id;
                let published = operation.phase == "Materializing"
                    && head.generation
                        == operation
                            .expected_generation
                            .checked_add(1)
                            .context("Operation generation overflow")?
                    && head.operation_id == operation.operation_id
                    && history.last().is_some_and(|checkpoint| {
                        checkpoint.parent_id.as_deref()
                            == Some(operation.from_checkpoint_id.as_str())
                    });
                ensure!(
                    uncommitted || published,
                    "Stale or unrelated operation/HEAD binding"
                );
            }
            operations.push(operation);
            ensure!(
                operations.len() <= 1,
                "Multiple unresolved operations share the same root binding"
            );
        }
        self.with_cleanup_heads(operations)
    }

    // HEAD is the durable cleanup fence. Journal absence alone cannot grant
    // authority after a committed component's unlink or directory fsync fails.
    fn with_cleanup_heads(
        &self,
        mut operations: Vec<OperationIntent>,
    ) -> Result<Vec<OperationIntent>> {
        let sessions = self.directory.join("sessions");
        let Some(metadata) = safe_path(&sessions)? else {
            return Ok(operations);
        };
        ensure!(
            metadata.is_dir(),
            "Checkpoint sessions path is not a directory"
        );
        for (count, entry) in fs::read_dir(sessions)?.enumerate() {
            ensure!(count < 10_000, "Checkpoint session inventory exceeds bound");
            let entry = entry?;
            let session_id = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-UTF8 session ID"))?;
            id(&session_id)?;
            ensure!(
                safe_path(&entry.path())?.is_some_and(|m| m.is_dir()),
                "Session entry is not a directory"
            );
            // An interrupted baseline capture without HEAD has no committed
            // authority and cannot carry a component cleanup fence.
            if safe_path(&entry.path().join("HEAD.json"))?.is_none() {
                continue;
            }
            let head = self.read_head(&session_id)?;
            if head.status == "TransitionCleanupRequired" {
                let operation = head
                    .transition_operation
                    .clone()
                    .context("Missing transition HEAD fence")?;
                let binding = operation
                    .transition
                    .as_ref()
                    .context("Transition HEAD has no binding")?;
                ensure!(
                    binding.branch_session_id == session_id
                        && binding.branch_checkpoint_id == head.checkpoint_id
                        && head.operation_id == operation.operation_id,
                    "Transition HEAD points to a different branch"
                );
                self.validate_transition_intent(&operation)?;
                if let Some(existing) = operations
                    .iter()
                    .find(|existing| existing.operation_id == operation.operation_id)
                {
                    ensure!(existing == &operation, "Transition journal and HEAD differ");
                } else {
                    operations.push(operation);
                }
                ensure!(
                    operations.len() <= 1,
                    "Multiple unresolved operations share the same root binding"
                );
                continue;
            }
            if !matches!(
                head.status.as_str(),
                "CleanupRequired" | "FinalizeCleanupRequired"
            ) {
                continue;
            }
            // A durable Restore intent may supersede the failed component/finalize
            // operation while retaining this source HEAD until the branch commits.
            if operations.iter().any(|operation| {
                operation.transition.is_some() && operation.session_id == session_id
            }) {
                continue;
            }
            let (session, checked_head, history) = self.checked_history(&session_id)?;
            ensure!(head == checked_head, "Cleanup HEAD changed while reading");
            let checkpoint = history.last().context("Missing cleanup checkpoint")?;
            let operation = if head.status == "FinalizeCleanupRequired" {
                super::finalize::finalize_intent(
                    &session,
                    &head,
                    head.finalization_receipt
                        .clone()
                        .context("Missing finalization receipt")?,
                )?
            } else {
                let definition = checkpoint
                    .completed_components
                    .last()
                    .context("Cleanup checkpoint has no completed component")?
                    .clone();
                OperationIntent {
                    schema_version: SCHEMA_VERSION,
                    operation_id: head.operation_id.clone(),
                    kind: "InstallComponent".into(),
                    session_id: session_id.clone(),
                    expected_generation: head
                        .generation
                        .checked_sub(1)
                        .context("Invalid cleanup generation")?,
                    from_checkpoint_id: checkpoint
                        .parent_id
                        .clone()
                        .context("Cleanup checkpoint has no parent")?,
                    roots: self.binding.clone(),
                    component_sha256: definition_hash(std::slice::from_ref(&definition))?,
                    component_definition: definition,
                    phase: "Materializing".into(),
                    finalization_receipt: None,
                    transition: None,
                }
            };
            if let Some(existing) = operations
                .iter()
                .find(|existing| existing.operation_id == operation.operation_id)
            {
                ensure!(
                    existing == &operation,
                    "Cleanup HEAD and operation disagree"
                );
            } else {
                operations.push(operation);
            }
            ensure!(
                operations.len() <= 1,
                "Multiple unresolved operations share the same root binding"
            );
            ensure!(
                self.read_head(&session_id)? == head,
                "Cleanup HEAD changed while reading"
            );
        }
        Ok(operations)
    }

    pub(super) fn ensure_no_pending(&self) -> Result<()> {
        ensure!(
            self.pending_operations()?.is_empty(),
            "Checkpoint operation is pending; recovery is required before another write"
        );
        Ok(())
    }

    pub fn begin_component(
        &self,
        lock: &recovery::Lock,
        expected: &ExpectedHead,
        flag: &AtomicBool,
    ) -> Result<OperationIntent> {
        self.require_lock(lock)?;
        cancelled(flag)?;
        self.ensure_no_pending()?;
        let (session, head, history) = self.checked_history(&expected.session_id)?;
        ensure!(
            head.generation == expected.generation
                && head.checkpoint_id == expected.checkpoint_id
                && head.status == "Ready",
            "Stale or non-ready expected HEAD"
        );
        ensure!(
            session.install_scope == "install",
            "Baseline-only sessions cannot record component completion"
        );
        let parent = history.last().context("Missing current checkpoint")?;
        let definition = session
            .ordered_components
            .get(parent.completed_components.len())
            .context("Session has no remaining component")?
            .clone();
        self.verify(&expected.session_id, Some(&expected.checkpoint_id), flag)?;
        ensure!(
            self.scan(&self.roots.mods, flag)? == parent.mods
                && self.scan(&self.roots.game, flag)? == parent.game
                && self.scan_auxiliary(&session.participants, &session.ancestor_shells, flag)?
                    == (
                        parent.auxiliary_snapshots.clone(),
                        parent.ancestor_shells.clone()
                    ),
            "Live roots differ from the committed boundary"
        );
        ensure!(
            self.read_head(&expected.session_id)? == head,
            "HEAD changed before operation intent"
        );
        cancelled(flag)?;
        let operation = OperationIntent {
            schema_version: SCHEMA_VERSION,
            operation_id: uuid::Uuid::new_v4().to_string(),
            kind: "InstallComponent".into(),
            session_id: expected.session_id.clone(),
            expected_generation: expected.generation,
            from_checkpoint_id: expected.checkpoint_id.clone(),
            roots: self.binding.clone(),
            component_sha256: definition_hash(std::slice::from_ref(&definition))?,
            component_definition: definition,
            phase: "Intent".into(),
            finalization_receipt: None,
            transition: None,
        };
        ensure_directory(&self.directory.join("operations"))?;
        publish_json(&self.operation_path(&operation)?, &operation)?;
        ensure!(
            self.pending_operations()? == [operation.clone()],
            "Operation intent changed after publication"
        );
        Ok(operation)
    }

    /// Must precede any component mutation. The updated record is the capability
    /// passed to commit; a caller cannot silently skip this durable phase.
    pub fn mark_component_running(
        &self,
        lock: &recovery::Lock,
        operation: &OperationIntent,
        flag: &AtomicBool,
    ) -> Result<OperationIntent> {
        self.require_lock(lock)?;
        cancelled(flag)?;
        self.current_operation(operation)?;
        ensure!(
            operation.kind == "InstallComponent" && operation.phase == "Intent",
            "Component operation is already materializing"
        );
        let mut running = operation.clone();
        running.phase = "Materializing".into();
        replace_json(&self.operation_path(operation)?, &running)?;
        ensure!(
            self.pending_operations()? == [running.clone()],
            "Materializing intent changed after publication"
        );
        Ok(running)
    }

    /// Records a successfully executed component. This primitive does not execute
    /// the component or assert backend success; the engine owns that decision.
    pub fn commit_component(
        &self,
        lock: &recovery::Lock,
        operation: &OperationIntent,
        flag: &AtomicBool,
        mut observe: impl FnMut(Progress),
    ) -> Result<CommitReport> {
        self.require_lock(lock)?;
        cancelled(flag)?;
        self.current_operation(operation)?;
        ensure!(
            operation.kind == "InstallComponent" && operation.phase == "Materializing",
            "Component mutation phase was not durably entered"
        );
        let (session, old_head, history) = self.checked_history(&operation.session_id)?;
        let parent = history.last().context("Missing component parent")?;
        let (mods, game, auxiliary_snapshots, ancestor_shells, file_count, total_bytes) = self
            .capture_all(
                &session.participants,
                &session.ancestor_shells,
                flag,
                &mut observe,
            )?;
        let mut completed = parent.completed_components.clone();
        completed.push(operation.component_definition.clone());
        let checkpoint_id = uuid::Uuid::new_v4().to_string();
        let document = Checkpoint {
            schema_version: SCHEMA_VERSION,
            checkpoint_id: checkpoint_id.clone(),
            session_id: operation.session_id.clone(),
            parent_id: Some(parent.checkpoint_id.clone()),
            sequence: parent
                .sequence
                .checked_add(1)
                .context("Checkpoint sequence overflow")?,
            kind: "Component".into(),
            completed_sha256: Some(definition_hash(&completed)?),
            component: Some(component_identity(&operation.component_definition)?),
            completed_components: completed,
            roots: self.binding.clone(),
            mods,
            game,
            auxiliary: Default::default(),
            auxiliary_snapshots,
            ancestor_shells,
            file_count,
            total_bytes,
            creator: concat!("ModSync.rs/", env!("CARGO_PKG_VERSION")).into(),
            origin: super::persistence::writer_origin()?.into(),
        };
        publish_json(
            &self
                .session_directory(&operation.session_id)?
                .join("checkpoints")
                .join(format!("{checkpoint_id}.json")),
            &document,
        )?;
        let persisted = self.read_checkpoint(&operation.session_id, &checkpoint_id)?;
        ensure!(
            persisted == document,
            "Published component checkpoint differs"
        );
        for (sha256, size) in super::auxiliary::objects(&persisted) {
            ensure!(
                hash_file(&self.object_path(&sha256)?, flag)? == (sha256, size),
                "Component object closure changed before HEAD publication"
            );
        }
        self.current_operation(operation)?;
        ensure!(
            self.read_head(&operation.session_id)? == old_head,
            "HEAD changed before component commit"
        );
        cancelled(flag)?;
        let head = Head {
            schema_version: SCHEMA_VERSION,
            generation: old_head
                .generation
                .checked_add(1)
                .context("HEAD generation overflow")?,
            checkpoint_id: checkpoint_id.clone(),
            status: "CleanupRequired".into(),
            operation_id: operation.operation_id.clone(),
            finalization_receipt: None,
            transition_operation: None,
        };
        replace_json(
            &self
                .session_directory(&operation.session_id)?
                .join("HEAD.json"),
            &head,
        )?;
        // Once HEAD is committed, cancellation cannot report an uncommitted action.
        ensure!(
            self.read_head(&operation.session_id)? == head,
            "HEAD changed after component commit; operation retained"
        );
        ensure!(
            self.pending_operations()? == [operation.clone()],
            "Operation changed after HEAD commit; operation retained"
        );
        fs::remove_file(self.operation_path(operation)?)?;
        // CleanupRequired was durably published before unlink. Any failure
        // leaves that authoritative fence, even if no journal file is visible.
        namespace_boundary(&self.directory.join("operations"))?;
        ensure!(
            self.read_head(&operation.session_id)? == head,
            "Cleanup HEAD changed before readiness publication"
        );
        let ready = Head {
            status: "Ready".into(),
            ..head.clone()
        };
        replace_json(
            &self
                .session_directory(&operation.session_id)?
                .join("HEAD.json"),
            &ready,
        )?;
        ensure!(
            self.read_head(&operation.session_id)? == ready,
            "HEAD changed after readiness publication"
        );
        Ok(CommitReport {
            schema_version: SCHEMA_VERSION,
            operation: "commitComponent",
            session_id: operation.session_id.clone(),
            checkpoint_id,
            parent_checkpoint_id: parent.checkpoint_id.clone(),
            generation: head.generation,
            operation_id: operation.operation_id.clone(),
            completed_components: document.completed_components.len(),
        })
    }

    pub(super) fn operation_path(&self, operation: &OperationIntent) -> Result<std::path::PathBuf> {
        id(&operation.operation_id)?;
        Ok(self
            .directory
            .join("operations")
            .join(format!("{}.json", operation.operation_id)))
    }
    fn current_operation(&self, operation: &OperationIntent) -> Result<()> {
        ensure!(
            self.pending_operations()? == [operation.clone()],
            "Operation capability differs from durable intent"
        );
        let head = self.read_head(&operation.session_id)?;
        ensure!(
            head.generation == operation.expected_generation
                && head.checkpoint_id == operation.from_checkpoint_id,
            "Operation HEAD already committed or stale"
        );
        Ok(())
    }
}
