use super::{
    model::*,
    read::*,
    store::{
        Store, canonical_bytes, namespace_boundary, publish_json, replace_bytes, replace_json,
    },
};
use crate::recovery;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Write, sync::atomic::AtomicBool};

pub(super) fn game_files(tree: &Tree) -> BTreeMap<String, String> {
    tree.entries
        .iter()
        .filter_map(|(path, entry)| {
            if let Entry::File { sha256, .. } = entry {
                Some((
                    path.replace('/', std::path::MAIN_SEPARATOR_STR),
                    sha256.clone(),
                ))
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn expected_receipt(
    roots: &crate::paths::Roots,
    session: &Session,
    checkpoint: &Checkpoint,
    report: Option<&serde_json::Value>,
) -> Result<(Vec<u8>, ReceiptBinding)> {
    let mut value = recovery::completion_value_from_manifest(
        roots,
        &session.ordered_components,
        &game_files(&checkpoint.game),
    )?;
    if let Some(report) = report {
        let typed: crate::engine::InstallReport =
            serde_json::from_value(report.clone()).context("Malformed stored install report")?;
        let identity = typed
            .checkpoint
            .as_ref()
            .context("Checkpoint install report has no checkpoint identity")?;
        ensure!(
            identity.session_id == session.session_id
                && identity.checkpoint_id == checkpoint.checkpoint_id
                && Some(identity.generation) == checkpoint.sequence.checked_add(1),
            "Install report checkpoint identity differs"
        );
        ensure!(
            typed.managed.is_some() != session.participants.is_empty(),
            "Managed install report differs from participant context"
        );
        value
            .as_object_mut()
            .context("Completion receipt is not an object")?
            .insert("installReport".into(), report.clone());
    }
    let bytes = canonical_bytes(&value)?;
    let receipt = ReceiptBinding {
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        size: bytes.len() as u64,
        install_report: report.cloned(),
    };
    Ok((bytes, receipt))
}

pub(super) fn finalize_intent(
    session: &Session,
    head: &Head,
    receipt: ReceiptBinding,
) -> Result<OperationIntent> {
    Ok(OperationIntent {
        schema_version: SCHEMA_VERSION,
        operation_id: head.operation_id.clone(),
        kind: "Finalize".into(),
        session_id: session.session_id.clone(),
        expected_generation: head.generation,
        from_checkpoint_id: head.checkpoint_id.clone(),
        roots: session.roots.clone(),
        component_definition: serde_json::Value::Null,
        component_sha256: super::history::definition_hash(&session.ordered_components)?,
        phase: "Materializing".into(),
        finalization_receipt: Some(receipt),
        transition: None,
    })
}

impl Store {
    /// Idle component boundaries are still an unfinished install. Legacy whole-run
    /// restore cannot safely restore only roots and leave auxiliary state behind.
    pub fn ensure_no_unfinished_install_sessions(&self) -> Result<()> {
        for summary in self.list()?.sessions {
            if summary.checkpoint_id.is_none() {
                continue;
            }
            let (session, head, _) = self.checked_history(&summary.session_id)?;
            ensure!(
                session.install_scope != "install"
                    || matches!(head.status.as_str(), "Complete" | "Detached"),
                "Unfinished component checkpoint session {}; use checkpoint recovery before legacy mutation",
                session.session_id
            );
        }
        Ok(())
    }

    pub(super) fn validate_finalize_intent(
        &self,
        operation: &OperationIntent,
        session: &Session,
        head: &Head,
        checkpoint: &Checkpoint,
    ) -> Result<()> {
        let receipt = operation
            .finalization_receipt
            .as_ref()
            .context("Finalize intent has no receipt")?;
        digest(&receipt.sha256)?;
        ensure!(
            *receipt
                == expected_receipt(
                    &self.roots,
                    session,
                    checkpoint,
                    receipt.install_report.as_ref()
                )?
                .1,
            "Finalize receipt does not match immutable checkpoint"
        );
        ensure!(
            receipt.size > 0
                && receipt.size <= MAX_DOCUMENT_BYTES
                && operation.component_definition.is_null()
                && operation.component_sha256
                    == super::history::definition_hash(&session.ordered_components)?
                && checkpoint.completed_components == session.ordered_components
                && operation.from_checkpoint_id == head.checkpoint_id
                && operation.expected_generation == head.generation,
            "Invalid Finalize receipt, definition prefix or HEAD binding"
        );
        if head.status == "Ready" {
            ensure!(
                head.finalization_receipt.is_none(),
                "Uncommitted Finalize has an unexpected receipt binding"
            );
        } else {
            ensure!(
                matches!(head.status.as_str(), "FinalizeCleanupRequired" | "Complete")
                    && head.operation_id == operation.operation_id
                    && head.finalization_receipt.as_ref() == Some(receipt),
                "Finalize operation is stale or unrelated to HEAD"
            );
        }
        Ok(())
    }

    /// Finalize the exact full prefix and reconcile legacy completion state under
    /// one durable intent. Status changes do not invent a component or sequence.
    pub fn finalize_session(
        &self,
        lock: &recovery::Lock,
        expected: &ExpectedHead,
        flag: &AtomicBool,
    ) -> Result<FinalizeReport> {
        self.finalize_session_inner(lock, expected, flag, None)
    }

    pub fn finalize_session_with_report(
        &self,
        lock: &recovery::Lock,
        expected: &ExpectedHead,
        flag: &AtomicBool,
        report: &crate::engine::InstallReport,
    ) -> Result<FinalizeReport> {
        self.finalize_session_inner(lock, expected, flag, Some(serde_json::to_value(report)?))
    }

    fn finalize_session_inner(
        &self,
        lock: &recovery::Lock,
        expected: &ExpectedHead,
        flag: &AtomicBool,
        report: Option<serde_json::Value>,
    ) -> Result<FinalizeReport> {
        self.require_lock(lock)?;
        cancelled(flag)?;
        self.ensure_no_pending()?;
        let (session, old_head, history) = self.checked_history(&expected.session_id)?;
        ensure!(
            session.install_scope == "install"
                && old_head.status == "Ready"
                && old_head.generation == expected.generation
                && old_head.checkpoint_id == expected.checkpoint_id,
            "Stale or non-ready finalization HEAD"
        );
        let checkpoint = history.last().context("Finalization checkpoint missing")?;
        ensure!(
            checkpoint.completed_components == session.ordered_components,
            "Cannot finalize an incomplete component prefix"
        );
        for (sha256, size) in super::auxiliary::objects(checkpoint) {
            ensure!(
                hash_file(&self.object_path(&sha256)?, flag)? == (sha256, size),
                "Finalization object closure differs"
            );
        }
        ensure!(
            self.scan(&self.roots.mods, flag)? == checkpoint.mods
                && self.scan(&self.roots.game, flag)? == checkpoint.game
                && self.scan_auxiliary(&session.participants, &session.ancestor_shells, flag)?
                    == (
                        checkpoint.auxiliary_snapshots.clone(),
                        checkpoint.ancestor_shells.clone()
                    ),
            "Live roots or auxiliary state differ from final committed checkpoint"
        );
        let baseline = history.first().context("Session baseline missing")?;
        recovery::verify_checkpoint_legacy_snapshot(
            &self.roots,
            &game_files(&baseline.mods),
            &game_files(&baseline.game),
        )?;
        let state = recovery::state_dir(&self.roots);
        let pending = state.join("pending");
        ensure!(
            safe_path(&pending)?.is_some_and(|metadata| metadata.is_file()),
            "Finalization requires the matching legacy pending snapshot"
        );
        let (bytes, receipt) =
            expected_receipt(&self.roots, &session, checkpoint, report.as_ref())?;
        // Reuse the streamed no-clobber CAS writer; this scratch file is store-owned.
        let mut temporary = tempfile::Builder::new()
            .prefix(".tmp-")
            .tempfile_in(&self.directory)?;
        temporary.write_all(&bytes)?;
        super::persistence::flush_file(temporary.as_file())?;
        self.publish_object(temporary.path(), &receipt.sha256, receipt.size, flag)?;
        let head = Head {
            status: "FinalizeCleanupRequired".into(),
            operation_id: uuid::Uuid::new_v4().to_string(),
            finalization_receipt: Some(receipt.clone()),
            ..old_head.clone()
        };
        let operation = finalize_intent(&session, &head, receipt.clone())?;
        super::store::ensure_directory(&self.directory.join("operations"))?;
        ensure!(
            self.read_head(&expected.session_id)? == old_head,
            "HEAD changed before Finalize intent"
        );
        cancelled(flag)?;
        publish_json(&self.operation_path(&operation)?, &operation)?;
        ensure!(
            self.pending_operations()? == [operation.clone()],
            "Finalize intent changed after publication"
        );
        // Once receipt reconciliation starts, finish the durable transition even
        // if cancellation arrives; failures retain the intent and legacy fence.
        replace_bytes(&state.join("completion.json"), &bytes)?;
        ensure!(
            hash_file(&state.join("completion.json"), &AtomicBool::new(false))?
                == (receipt.sha256.clone(), receipt.size),
            "Published completion receipt differs"
        );
        fs::remove_file(&pending)?;
        namespace_boundary(&state)?;
        ensure!(
            self.read_head(&expected.session_id)? == old_head,
            "HEAD changed during Finalize reconciliation"
        );
        replace_json(
            &self
                .session_directory(&expected.session_id)?
                .join("HEAD.json"),
            &head,
        )?;
        ensure!(
            self.pending_operations()? == [operation.clone()],
            "Finalize intent changed before cleanup"
        );
        fs::remove_file(self.operation_path(&operation)?)?;
        namespace_boundary(&self.directory.join("operations"))?;
        ensure!(
            self.read_head(&expected.session_id)? == head,
            "Finalize HEAD changed before completion"
        );
        let complete = Head {
            status: "Complete".into(),
            ..head
        };
        replace_json(
            &self
                .session_directory(&expected.session_id)?
                .join("HEAD.json"),
            &complete,
        )?;
        ensure!(
            self.read_head(&expected.session_id)? == complete,
            "HEAD changed after finalization"
        );
        Ok(FinalizeReport {
            schema_version: SCHEMA_VERSION,
            operation: "finalizeSession",
            session_id: expected.session_id.clone(),
            checkpoint_id: expected.checkpoint_id.clone(),
            generation: expected.generation,
            operation_id: complete.operation_id,
            receipt,
        })
    }
}

impl Store {
    pub(super) fn recover_finalize(
        &self,
        lock: &recovery::Lock,
        operation: &OperationIntent,
        flag: &AtomicBool,
    ) -> Result<()> {
        self.require_lock(lock)?;
        let (session, old_head, history) = self.checked_history(&operation.session_id)?;
        let checkpoint = history
            .last()
            .context("Finalize recovery checkpoint missing")?;
        self.validate_finalize_intent(operation, &session, &old_head, checkpoint)?;
        self.verify_target(&session, checkpoint, flag)?;
        ensure!(
            self.live_matches(&session, checkpoint, flag)?,
            "Finalize recovery live state differs"
        );
        let receipt = operation
            .finalization_receipt
            .clone()
            .context("Finalize receipt missing")?;
        let (bytes, expected_receipt) = expected_receipt(
            &self.roots,
            &session,
            checkpoint,
            receipt.install_report.as_ref(),
        )?;
        ensure!(
            receipt == expected_receipt
                && hash_file(&self.object_path(&receipt.sha256)?, flag)?
                    == (receipt.sha256.clone(), receipt.size),
            "Finalize receipt CAS differs"
        );
        cancelled(flag)?;
        let mut running = operation.clone();
        running.phase = "Materializing".into();
        replace_json(&self.operation_path(operation)?, &running)?;
        let state = recovery::state_dir(&self.roots);
        replace_bytes(&state.join("completion.json"), &bytes)?;
        ensure!(
            hash_file(&state.join("completion.json"), &AtomicBool::new(false))?
                == (receipt.sha256.clone(), receipt.size),
            "Recovered receipt bytes differ"
        );
        let pending = state.join("pending");
        if let Some(metadata) = safe_path(&pending)? {
            ensure!(metadata.is_file(), "Legacy pending marker is unsafe");
            fs::remove_file(&pending)?;
        }
        namespace_boundary(&state)?;
        let cleanup = Head {
            status: "FinalizeCleanupRequired".into(),
            operation_id: running.operation_id.clone(),
            finalization_receipt: Some(receipt),
            transition_operation: None,
            ..old_head
        };
        replace_json(
            &self
                .session_directory(&session.session_id)?
                .join("HEAD.json"),
            &cleanup,
        )?;
        ensure!(
            self.pending_operations()? == [running.clone()],
            "Finalize recovery fence changed"
        );
        fs::remove_file(self.operation_path(&running)?)?;
        namespace_boundary(&self.directory.join("operations"))?;
        let complete = Head {
            status: "Complete".into(),
            ..cleanup
        };
        replace_json(
            &self
                .session_directory(&session.session_id)?
                .join("HEAD.json"),
            &complete,
        )
    }
}
