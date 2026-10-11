//! Reusable immutable snapshots for authoring undo/redo.
use super::{
    model::*,
    read::*,
    store::{Store, ensure_directory, namespace_boundary, publish_json, replace_json},
};
use anyhow::{Context, Result, ensure};
use std::{fs, sync::atomic::AtomicBool};

pub(crate) struct AuthoringEntry {
    pub path: std::path::PathBuf,
    /// None denotes a directory, including an empty captured directory.
    pub content: Option<std::path::PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImportRepair {
    schema_version: u32,
    source_session: String,
    source_checkpoint: String,
    source_session_sha256: String,
    source_checkpoint_sha256: String,
    orphan_session: String,
    session_bytes: String,
    head_bytes: String,
    checkpoint_bytes: String,
}
fn repair_bytes(path: &std::path::Path) -> Result<String> {
    use std::io::Read;
    let file = open_file(path)?;
    ensure!(
        file.metadata()?.len() <= MAX_DOCUMENT_BYTES,
        "Repair metadata exceeds bound"
    );
    let mut bytes = Vec::new();
    file.take(MAX_DOCUMENT_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_DOCUMENT_BYTES,
        "Repair metadata grew beyond bound"
    );
    Ok(String::from_utf8(bytes)?)
}

impl Store {
    /// Quarantine only an exact orphan of the previous authoring importer.
    /// Caller must already own a durable same-target disk restore journal.
    pub(crate) fn repair_authoring_import(
        &self,
        lock: &crate::recovery::Lock,
        source_id: &str,
        checkpoint_id: &str,
        flag: &AtomicBool,
    ) -> Result<()> {
        ensure!(
            lock.guards(&self.roots),
            "Import repair needs matching root locks"
        );
        super::persistence::writer_origin()?;
        super::external_writer::ensure_clear(&self.roots.game)?;
        #[cfg(windows)]
        crate::patcher::checkpoint_job::ensure_clear_game(&self.roots.game)?;
        crate::maintenance::ensure_clear_game(&self.roots.game)?;
        crate::maintenance::ensure_clear_mods(&self.roots.mods)?;
        self.validate_location()?;
        let state = crate::recovery::state_dir(&self.roots);
        ensure!(
            safe_path(&state.join("pending"))?.is_none(),
            "Legacy pending recovery prevents import repair"
        );
        if safe_path(&self.directory.join("operations"))?.is_some() {
            ensure!(
                fs::read_dir(self.directory.join("operations"))?
                    .next()
                    .is_none(),
                "Checkpoint operation prevents import repair"
            );
        }
        let (source, _, ancestry) = self.checked_history(source_id)?;
        let selected = ancestry
            .iter()
            .find(|c| c.checkpoint_id == checkpoint_id)
            .context("Repair target is not committed source ancestry")?;
        ensure!(
            source.participants.is_empty(),
            "Managed authoring import repair is unsupported"
        );
        self.verify_target(&source, selected, flag)?;
        let source_dir = self.session_directory(source_id)?;
        let source_session_sha256 = hash_file(&source_dir.join("session.json"), flag)?.0;
        let source_checkpoint_sha256 = hash_file(
            &source_dir
                .join("checkpoints")
                .join(format!("{checkpoint_id}.json")),
            flag,
        )?
        .0;
        let intent_path = state.join("authoring-import-repair.json");
        let existing: Option<ImportRepair> = if safe_path(&intent_path)?.is_some() {
            Some(json(&intent_path)?)
        } else {
            None
        };
        let quarantine = state.join("authoring-import-quarantine");
        let mut candidate = None;
        if let Some(saved) = &existing {
            ensure!(
                saved.schema_version == SCHEMA_VERSION
                    && saved.source_session == source_id
                    && saved.source_checkpoint == checkpoint_id
                    && saved.source_session_sha256 == source_session_sha256
                    && saved.source_checkpoint_sha256 == source_checkpoint_sha256,
                "Repair intent belongs to a different immutable source boundary"
            );
            id(&saved.orphan_session)?;
            candidate = Some(saved.clone());
        } else {
            for (index, entry) in fs::read_dir(self.directory.join("sessions"))?.enumerate() {
                cancelled(flag)?;
                ensure!(index < 10_000, "Repair session inventory exceeds bound");
                let entry = entry?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("Non-UTF8 repair session"))?;
                id(&name)?;
                if name == source_id {
                    continue;
                }
                let raw_session = repair_bytes(&entry.path().join("session.json"))?;
                let session: Session = serde_json::from_str(&raw_session)?;
                let head: Head = json(&entry.path().join("HEAD.json"))?;
                if head.status != "Ready"
                    || head.generation != 1
                    || !matches!(session.install_scope.as_str(), "baseline" | "install")
                    || session.initial_completed_components != selected.completed_components
                {
                    self.checked_history(&name)?;
                    continue;
                }
                let record = ImportRepair {
                    schema_version: SCHEMA_VERSION,
                    source_session: source_id.into(),
                    source_checkpoint: checkpoint_id.into(),
                    source_session_sha256: source_session_sha256.clone(),
                    source_checkpoint_sha256: source_checkpoint_sha256.clone(),
                    orphan_session: name,
                    session_bytes: raw_session,
                    head_bytes: repair_bytes(&entry.path().join("HEAD.json"))?,
                    checkpoint_bytes: repair_bytes(
                        &entry
                            .path()
                            .join("checkpoints")
                            .join(format!("{}.json", head.checkpoint_id)),
                    )?,
                };
                self.validate_import_repair(&source, selected, &record)?;
                ensure!(
                    candidate.replace(record).is_none(),
                    "Multiple import clones require explicit recovery"
                );
            }
        }
        let Some(record) = candidate else {
            return Ok(());
        };
        self.validate_import_repair(&source, selected, &record)?;
        // Every other session must remain strictly readable and cannot reference
        // the clone. Never quarantine live lineage, active selectors or operations.
        for (index, entry) in fs::read_dir(self.directory.join("sessions"))?.enumerate() {
            ensure!(index < 10_000, "Repair reference inventory exceeds bound");
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-UTF8 session"))?;
            if name == record.orphan_session {
                continue;
            }
            let (other, head, _) = self.checked_history(&name)?;
            ensure!(
                other.parent_session_id.as_deref() != Some(record.orphan_session.as_str())
                    && other.safety_session_id.as_deref() != Some(record.orphan_session.as_str()),
                "Import clone has child references"
            );
            ensure!(
                other.install_scope != "install"
                    || matches!(head.status.as_str(), "Complete" | "Detached"),
                "Another unfinished install prevents import repair"
            );
        }
        if safe_path(&self.directory.join("active.json"))?.is_some() {
            let active: serde_json::Value = json(&self.directory.join("active.json"))?;
            ensure!(
                active.get("sessionId").and_then(|v| v.as_str())
                    != Some(record.orphan_session.as_str()),
                "Import clone is active"
            );
        }
        let original = self.session_directory(&record.orphan_session)?;
        let retained = quarantine.join(&record.orphan_session);
        ensure!(
            safe_path(&original)?.is_some() != safe_path(&retained)?.is_some(),
            "Repair clone location is ambiguous"
        );
        let present = if safe_path(&original)?.is_some() {
            &original
        } else {
            &retained
        };
        self.verify_import_raw(present, &record)?;
        cancelled(flag)?;
        if existing.is_none() {
            publish_json(&intent_path, &record)?;
        }
        // From the durable intent onward, complete or leave replayable ownership.
        ensure_directory(&quarantine)?;
        if present == &original {
            fs::rename(&original, &retained)?;
        }
        namespace_boundary(&self.directory.join("sessions"))?;
        namespace_boundary(&quarantine)?;
        self.verify_import_raw(&retained, &record)?;
        let receipt = quarantine.join(format!("{}.repair.json", record.orphan_session));
        if safe_path(&receipt)?.is_none() {
            publish_json(&receipt, &record)?;
        }
        ensure!(
            json::<ImportRepair>(&receipt)? == record,
            "Retained repair receipt differs"
        );
        fs::remove_file(&intent_path)?;
        namespace_boundary(&state)?;
        Ok(())
    }

    fn validate_import_repair(
        &self,
        source: &Session,
        selected: &Checkpoint,
        record: &ImportRepair,
    ) -> Result<()> {
        let imported: Session = serde_json::from_str(&record.session_bytes)?;
        let snapshot: Checkpoint = serde_json::from_str(&record.checkpoint_bytes)?;
        let head: Head = serde_json::from_str(&record.head_bytes)?;
        id(&record.orphan_session)?;
        id(&snapshot.checkpoint_id)?;
        id(&head.operation_id)?;
        chrono::DateTime::parse_from_rfc3339(&imported.created_utc)?;
        let mut expected = source.clone();
        expected.session_id = record.orphan_session.clone();
        expected.created_utc = imported.created_utc.clone();
        ensure!(
            matches!(imported.install_scope.as_str(), "baseline" | "install"),
            "Unknown importer scope"
        );
        expected.install_scope = imported.install_scope.clone();
        expected.ordered_components = selected.completed_components.clone();
        expected.initial_completed_components = selected.completed_components.clone();
        expected.definitions_sha256 = Some(super::history::definition_hash(
            &expected.ordered_components,
        )?);
        expected.parent_session_id = None;
        expected.parent_checkpoint_id = None;
        expected.branch_kind = None;
        expected.safety_session_id = None;
        expected.origin = super::persistence::writer_origin()?.into();
        ensure!(
            imported == expected,
            "Session is not the exact authoring importer transform"
        );
        let mut expected_snapshot = selected.clone();
        expected_snapshot.session_id = record.orphan_session.clone();
        expected_snapshot.checkpoint_id = snapshot.checkpoint_id.clone();
        expected_snapshot.parent_id = None;
        expected_snapshot.sequence = 0;
        expected_snapshot.kind = "Baseline".into();
        expected_snapshot.component = None;
        expected_snapshot.origin = expected.origin;
        ensure!(
            snapshot == expected_snapshot,
            "Checkpoint is not the exact selected immutable clone"
        );
        ensure!(
            head == Head {
                schema_version: SCHEMA_VERSION,
                generation: 1,
                checkpoint_id: snapshot.checkpoint_id,
                status: "Ready".into(),
                operation_id: head.operation_id.clone(),
                finalization_receipt: None,
                transition_operation: None
            },
            "Import clone HEAD is not untouched Ready generation one"
        );
        Ok(())
    }

    fn verify_import_raw(&self, directory: &std::path::Path, record: &ImportRepair) -> Result<()> {
        ensure!(
            safe_path(directory)?.is_some_and(|m| m.is_dir()),
            "Unsafe import clone directory"
        );
        let head: Head = serde_json::from_str(&record.head_bytes)?;
        let expected = ["session.json", "HEAD.json", "checkpoints"];
        let mut names = Vec::new();
        for entry in fs::read_dir(directory)? {
            names.push(entry?.file_name());
        }
        ensure!(
            names.len() == expected.len()
                && expected.iter().all(|name| names
                    .iter()
                    .any(|n| n.as_os_str() == std::ffi::OsStr::new(name))),
            "Import clone has extra artifacts"
        );
        let checkpoints = directory.join("checkpoints");
        ensure!(
            safe_path(&checkpoints)?.is_some_and(|m| m.is_dir()),
            "Unsafe import checkpoint directory"
        );
        let entries: Vec<_> = fs::read_dir(&checkpoints)?.collect::<std::io::Result<Vec<_>>>()?;
        ensure!(
            entries.len() == 1
                && entries[0].file_name()
                    == std::ffi::OsStr::new(&format!("{}.json", head.checkpoint_id)),
            "Import clone has extra checkpoint ancestry"
        );
        ensure!(
            repair_bytes(&directory.join("session.json"))? == record.session_bytes
                && repair_bytes(&directory.join("HEAD.json"))? == record.head_bytes
                && repair_bytes(&entries[0].path())? == record.checkpoint_bytes,
            "Import clone raw bytes changed"
        );
        Ok(())
    }

    /// Read an immutable authoring boundary without materializing either root.
    /// Logical paths retain their install-root names; file bytes come only from
    /// verified content-addressed objects. No checkpoint HEAD is changed.
    pub(crate) fn authoring_inventory(
        &self,
        session_id: &str,
        checkpoint_id: &str,
        definition: &serde_json::Value,
        flag: &AtomicBool,
    ) -> Result<Vec<AuthoringEntry>> {
        let (session, _, history) = self.checked_history(session_id)?;
        ensure!(
            session.participants.is_empty() && session.ordered_components == [definition.clone()],
            "Authoring boundary belongs to a different action or participant scope"
        );
        let checkpoint = history
            .iter()
            .find(|checkpoint| checkpoint.checkpoint_id == checkpoint_id)
            .context("Authoring boundary is not committed checkpoint ancestry")?;
        self.verify_target(&session, checkpoint, flag)?;
        let mut inventory = Vec::new();
        for (root, tree) in [
            (&self.roots.mods, &checkpoint.mods),
            (&self.roots.game, &checkpoint.game),
        ] {
            inventory.push(AuthoringEntry {
                path: root.clone(),
                content: None,
            });
            for (relative, entry) in &tree.entries {
                cancelled(flag)?;
                inventory.push(AuthoringEntry {
                    path: root.join(relative),
                    content: match entry {
                        Entry::Directory { .. } => None,
                        Entry::File { sha256, .. } => Some(self.object_path(sha256)?),
                    },
                });
            }
        }
        Ok(inventory)
    }

    /// Settle only the single-action session named by the durable authoring
    /// journal. A verified committed boundary is adopted; a failed action is
    /// restored and abandoned before the caller may retry it.
    pub fn recover_authoring_action(
        &self,
        lock: &crate::recovery::Lock,
        before: &crate::engine::CheckpointRunReport,
        definition: &serde_json::Value,
        flag: &AtomicBool,
        mut observe: impl FnMut(Progress),
    ) -> Result<Option<crate::engine::AuthoringActionReport>> {
        self.require_lock(lock)?;
        let (source, _, history) = self.checked_history(&before.session_id)?;
        ensure!(
            source.participants.is_empty()
                && source.ordered_components == [definition.clone()]
                && history
                    .first()
                    .is_some_and(|baseline| baseline.checkpoint_id == before.checkpoint_id
                        && before.generation == 1),
            "Pending action does not own this checkpoint session"
        );
        let recovery = self.recover_pending(lock, Some(&before.session_id), flag, &mut observe)?;
        let session_id = recovery
            .restored
            .as_ref()
            .map(|report| report.session_id.as_str())
            .or(recovery.session_id.as_deref())
            .unwrap_or(&before.session_id);
        let (session, head, history) = self.checked_history(session_id)?;
        let current = history
            .last()
            .context("Recovered action boundary is missing")?;
        ensure!(
            session.ordered_components == [definition.clone()]
                && self.live_matches(&session, current, flag)?,
            "Recovered action files or definition differ"
        );
        if current.completed_components.len() == 1 {
            let after = crate::engine::CheckpointRunReport {
                session_id: session.session_id.clone(),
                checkpoint_id: head.checkpoint_id.clone(),
                generation: head.generation,
            };
            if head.status != "Complete" {
                ensure!(
                    matches!(head.status.as_str(), "Ready" | "Restored"),
                    "Action checkpoint is not ready to finalize"
                );
                replace_json(
                    &self
                        .session_directory(&session.session_id)?
                        .join("HEAD.json"),
                    &Head {
                        status: "Ready".into(),
                        ..head
                    },
                )?;
                self.finalize_session_with_report(
                    lock,
                    &ExpectedHead {
                        session_id: after.session_id.clone(),
                        checkpoint_id: after.checkpoint_id.clone(),
                        generation: after.generation,
                    },
                    &AtomicBool::new(false),
                    &crate::engine::InstallReport {
                        managed: None,
                        checkpoint: Some(after.clone()),
                    },
                )?;
            }
            Ok(Some(crate::engine::AuthoringActionReport {
                before: before.clone(),
                after,
            }))
        } else {
            ensure!(
                current.completed_components.is_empty() && head.status == "Restored",
                "Action recovery did not restore its baseline"
            );
            crate::recovery::verify_checkpoint_legacy_snapshot(
                &self.roots,
                &super::finalize::game_files(&current.mods),
                &super::finalize::game_files(&current.game),
            )?;
            replace_json(
                &self
                    .session_directory(&session.session_id)?
                    .join("HEAD.json"),
                &Head {
                    status: "Detached".into(),
                    ..head
                },
            )?;
            crate::recovery::finish(&self.roots)?;
            Ok(None)
        }
    }
    /// Import a verified historical boundary as an independent snapshot before
    /// using the normal fenced restore. Original HEADs and their redo ancestry
    /// remain untouched; stale-HEAD protections on normal restore are unchanged.
    pub fn restore_authoring_snapshot(
        &self,
        lock: &crate::recovery::Lock,
        session_id: &str,
        checkpoint_id: &str,
        flag: &AtomicBool,
        observe: impl FnMut(Progress),
    ) -> Result<RestoreReport> {
        self.require_lock(lock)?;
        cancelled(flag)?;
        self.ensure_no_pending()?;
        self.ensure_no_unfinished_install_sessions()?;
        let (source, _, history) = self.checked_history(session_id)?;
        ensure!(
            source.participants.is_empty(),
            "Authoring snapshots cannot restore managed participants"
        );
        let selected = history
            .iter()
            .find(|checkpoint| checkpoint.checkpoint_id == checkpoint_id)
            .context("Authoring checkpoint is not committed ancestry")?;
        self.verify_target(&source, selected, flag)?;
        self.preflight_materialization(&source, selected, flag)?;
        let imported_id = uuid::Uuid::new_v4().to_string();
        let imported_checkpoint = uuid::Uuid::new_v4().to_string();
        let mut snapshot = selected.clone();
        snapshot.session_id = imported_id.clone();
        snapshot.checkpoint_id = imported_checkpoint.clone();
        snapshot.parent_id = None;
        snapshot.sequence = 0;
        snapshot.kind = "Baseline".into();
        snapshot.component = None;
        let mut imported = source;
        imported.session_id = imported_id.clone();
        imported.created_utc =
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        imported.install_scope = "install".into();
        imported.ordered_components = snapshot.completed_components.clone();
        imported.initial_completed_components = snapshot.completed_components.clone();
        imported.definitions_sha256 = Some(super::history::definition_hash(
            &imported.ordered_components,
        )?);
        imported.parent_session_id = None;
        imported.parent_checkpoint_id = None;
        imported.branch_kind = None;
        imported.safety_session_id = None;
        imported.origin = super::persistence::writer_origin()?.into();
        snapshot.origin = imported.origin.clone();
        let directory = self.session_directory(&imported_id)?;
        ensure!(
            safe_path(&directory)?.is_none(),
            "Authoring snapshot UUID collision"
        );
        fs::create_dir(&directory)?;
        namespace_boundary(&self.directory.join("sessions"))?;
        ensure_directory(&directory.join("checkpoints"))?;
        publish_json(&directory.join("session.json"), &imported)?;
        publish_json(
            &directory
                .join("checkpoints")
                .join(format!("{imported_checkpoint}.json")),
            &snapshot,
        )?;
        let head = Head {
            schema_version: SCHEMA_VERSION,
            generation: 1,
            checkpoint_id: imported_checkpoint.clone(),
            status: "Ready".into(),
            operation_id: uuid::Uuid::new_v4().to_string(),
            finalization_receipt: None,
            transition_operation: None,
        };
        cancelled(flag)?;
        publish_json(&directory.join("HEAD.json"), &head)?;
        let report = self.restore_checkpoint(
            lock,
            &ExpectedHead {
                session_id: imported_id,
                checkpoint_id: imported_checkpoint.clone(),
                generation: 1,
            },
            &imported_checkpoint,
            flag,
            observe,
        )?;
        // Restore has materialized and verified the complete imported prefix.
        // Finalize its receipt so another authoring action can start normally.
        let restored = self.read_head(&report.session_id)?;
        ensure!(
            restored.status == "Restored"
                && restored.checkpoint_id == report.checkpoint_id
                && restored.generation == report.generation,
            "Restored authoring HEAD differs"
        );
        replace_json(
            &self
                .session_directory(&report.session_id)?
                .join("HEAD.json"),
            &Head {
                status: "Ready".into(),
                ..restored
            },
        )?;
        self.finalize_session(
            lock,
            &ExpectedHead {
                session_id: report.session_id.clone(),
                checkpoint_id: report.checkpoint_id.clone(),
                generation: report.generation,
            },
            &AtomicBool::new(false),
        )?;
        Ok(report)
    }
}
