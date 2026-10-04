use super::{
    model::*,
    read::*,
    store::{Store, canonical_bytes},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub(super) fn definition_hash(definitions: &[serde_json::Value]) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(canonical_bytes(&definitions)?)
    ))
}
pub(super) fn component_identity(definition: &serde_json::Value) -> Result<ComponentIdentity> {
    let guid = definition
        .get("Guid")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .context("Component definition needs a nonempty Guid")?;
    ensure!(
        !guid.chars().any(char::is_control),
        "Component definition Guid contains control characters"
    );
    Ok(ComponentIdentity {
        guid: guid.into(),
        name: definition
            .get("Name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .into(),
    })
}
pub(super) fn validate_definitions(definitions: &[serde_json::Value]) -> Result<()> {
    ensure!(
        definitions.len() <= 10_000,
        "Checkpoint session exceeds 10000 component definitions"
    );
    let mut guids = BTreeSet::new();
    for definition in definitions {
        ensure!(
            guids.insert(component_identity(definition)?.guid.to_lowercase()),
            "Duplicate component definition GUID"
        );
    }
    Ok(())
}

impl Store {
    /// Read immutable ancestry, not arbitrary orphan documents. Prefix equality uses
    /// canonical scalar-preserving JSON of full definitions, never GUIDs alone.
    pub(super) fn checked_history(
        &self,
        session_id: &str,
    ) -> Result<(Session, Head, Vec<Checkpoint>)> {
        self.checked_history_inner(session_id, &mut BTreeSet::new(), &mut 0)
    }

    fn checked_history_inner(
        &self,
        session_id: &str,
        seen_sessions: &mut BTreeSet<String>,
        total_bytes: &mut u64,
    ) -> Result<(Session, Head, Vec<Checkpoint>)> {
        ensure!(
            seen_sessions.len() < 128 && seen_sessions.insert(session_id.into()),
            "Checkpoint session lineage cycles or exceeds bound"
        );
        self.validate_location()?;
        let session = self.read_session(session_id)?;
        let head = self.read_head(session_id)?;
        let mut next = Some(head.checkpoint_id.clone());
        let mut seen = BTreeSet::new();
        let mut history = Vec::new();

        while let Some(checkpoint_id) = next {
            ensure!(
                seen.insert(checkpoint_id.clone()),
                "Checkpoint parent cycle"
            );
            ensure!(
                seen.len() <= 10_001,
                "Checkpoint ancestry exceeds 10001 boundaries"
            );
            let path = self
                .session_directory(session_id)?
                .join("checkpoints")
                .join(format!("{checkpoint_id}.json"));
            *total_bytes = total_bytes
                .checked_add(
                    safe_path(&path)?
                        .context("Checkpoint ancestor missing")?
                        .len(),
                )
                .context("Checkpoint history byte count overflow")?;
            ensure!(
                *total_bytes <= 256 * 1024 * 1024,
                "Checkpoint ancestry exceeds 256 MiB document budget"
            );
            let checkpoint = self.read_checkpoint(session_id, &checkpoint_id)?;
            let length = checkpoint.completed_components.len();
            ensure!(
                length <= session.ordered_components.len()
                    && canonical_bytes(&checkpoint.completed_components)?
                        == canonical_bytes(&session.ordered_components[..length])?,
                "Checkpoint completed definitions are not the immutable session prefix"
            );
            next = checkpoint.parent_id.clone();
            history.push(checkpoint);
        }
        history.reverse();
        ensure!(history.first().is_some_and(|checkpoint| checkpoint.kind == "Baseline" && checkpoint.sequence == 0), "Checkpoint chain does not terminate at baseline");
        for pair in history.windows(2) {
            ensure!(
                pair[1].sequence == pair[0].sequence + 1
                    && pair[1].parent_id.as_deref() == Some(pair[0].checkpoint_id.as_str())
                    && pair[1].completed_components.len() == pair[0].completed_components.len() + 1,
                "Checkpoint chain sequence/prefix growth is invalid"
            );
        }
        let current = history.last().context("Empty checkpoint history")?;
        ensure!(
            head.generation
                == current
                    .sequence
                    .checked_add(1)
                    .context("HEAD generation overflow")?,
            "HEAD generation differs from checkpoint sequence"
        );
        if head.status == "CleanupRequired" {
            ensure!(
                session.install_scope == "install"
                    && current.kind == "Component"
                    && current.parent_id.is_some()
                    && !current.completed_components.is_empty()
                    && head.generation > 1,
                "Cleanup HEAD is not bound to a committed install component"
            );
        }
        ensure!(
            history
                .first()
                .is_some_and(|baseline| baseline.completed_components
                    == session.initial_completed_components),
            "Baseline does not preserve the inherited completed prefix"
        );
        if matches!(head.status.as_str(), "Complete" | "FinalizeCleanupRequired") {
            ensure!(
                current.completed_components.len() == session.ordered_components.len()
                    && (head.status != "FinalizeCleanupRequired"
                        || session.install_scope == "install"),
                "Complete HEAD has an incomplete definition prefix"
            );
        }
        if session.install_scope == "install"
            && (matches!(head.status.as_str(), "Complete" | "FinalizeCleanupRequired")
                || head.finalization_receipt.is_some())
        {
            ensure!(
                head.finalization_receipt.as_ref()
                    == Some(
                        &super::finalize::expected_receipt(
                            &self.roots,
                            &session,
                            current,
                            head.finalization_receipt
                                .as_ref()
                                .and_then(|receipt| receipt.install_report.as_ref())
                        )?
                        .1
                    ),
                "Finalized HEAD receipt differs from immutable completed checkpoint"
            );
        }
        if let (Some(parent_session), Some(parent_checkpoint)) =
            (&session.parent_session_id, &session.parent_checkpoint_id)
        {
            let (parent, _, parent_history) =
                self.checked_history_inner(parent_session, seen_sessions, total_bytes)?;
            ensure!(
                parent.origin == session.origin,
                "Branch parent capability differs"
            );
            let target = parent_history
                .iter()
                .find(|checkpoint| &checkpoint.checkpoint_id == parent_checkpoint)
                .context("Branch parent is not committed ancestry")?;
            let baseline = history.first().context("Branch baseline missing")?;
            ensure!(
                session.initial_completed_components == target.completed_components
                    && session.participants == parent.participants
                    && session.ancestor_shells == parent.ancestor_shells
                    && baseline.game == target.game
                    && baseline.auxiliary_snapshots == target.auxiliary_snapshots
                    && baseline.ancestor_shells == target.ancestor_shells
                    && (session.branch_kind.as_deref() == Some("SourceRebase")
                        || baseline.mods == target.mods),
                "Branch baseline differs from authorized lineage boundary"
            );
        }
        ensure!(
            self.read_head(session_id)? == head,
            "Checkpoint HEAD changed during history read; retry"
        );
        Ok((session, head, history))
    }

    pub fn inspect(
        &self,
        session_id: &str,
        checkpoint_id: Option<&str>,
    ) -> Result<InspectionReport> {
        let (session, head, history) = self.checked_history(session_id)?;
        let selected = checkpoint_id.unwrap_or(&head.checkpoint_id);
        id(selected)?;
        let checkpoint = history
            .into_iter()
            .find(|checkpoint| checkpoint.checkpoint_id == selected)
            .context("Checkpoint is not in committed HEAD ancestry")?;
        let pending_operations = self
            .pending_operations()?
            .into_iter()
            .filter(|operation| operation.session_id == session_id)
            .collect();
        ensure!(
            self.read_head(session_id)? == head,
            "Checkpoint HEAD changed while inspecting; retry"
        );
        Ok(InspectionReport {
            schema_version: SCHEMA_VERSION,
            operation: "inspect",
            session_id: session_id.into(),
            checkpoint_id: selected.into(),
            generation: head.generation,
            head_checkpoint_id: head.checkpoint_id,
            session,
            checkpoint,
            pending_operations,
            finalization_receipt: head.finalization_receipt,
        })
    }

    pub fn diff(&self, session_id: &str, from: &str, to: &str) -> Result<DiffReport> {
        id(from)?;
        id(to)?;
        self.pending_operations()?;
        let (_, head, history) = self.checked_history(session_id)?;
        let from_document = history
            .iter()
            .find(|checkpoint| checkpoint.checkpoint_id == from)
            .context("Diff source is not in committed ancestry")?;
        let to_document = history
            .iter()
            .find(|checkpoint| checkpoint.checkpoint_id == to)
            .context("Diff destination is not in committed ancestry")?;
        let mut changes = Vec::new();
        for (tree, before, after) in [
            (TreeRoot::Mods, &from_document.mods, &to_document.mods),
            (TreeRoot::Game, &from_document.game, &to_document.game),
        ] {
            let paths: BTreeSet<_> = before.entries.keys().chain(after.entries.keys()).collect();
            for path in paths {
                let old = before.entries.get(path);
                let new = after.entries.get(path);
                if old != new {
                    changes.push(PathChange {
                        tree,
                        path: path.clone(),
                        change: change_kind(old, new),
                        before: old.cloned(),
                        after: new.cloned(),
                    });
                }
            }
        }
        ensure!(
            self.read_head(session_id)? == head,
            "Checkpoint HEAD changed while diffing; retry"
        );
        Ok(DiffReport {
            schema_version: SCHEMA_VERSION,
            operation: "diff",
            session_id: session_id.into(),
            generation: head.generation,
            from_checkpoint_id: from.into(),
            to_checkpoint_id: to.into(),
            changes,
            mods_root_mode_changed: from_document.mods.root_mode != to_document.mods.root_mode,
            game_root_mode_changed: from_document.game.root_mode != to_document.game.root_mode,
            auxiliary_changes: from_document
                .auxiliary_snapshots
                .iter()
                .filter_map(|(participant, before)| {
                    let after = &to_document.auxiliary_snapshots[participant];
                    (before != after).then(|| AuxiliaryChange {
                        participant: participant.clone(),
                        before: before.clone(),
                        after: after.clone(),
                    })
                })
                .collect(),
            ancestor_shell_changes: from_document
                .ancestor_shells
                .iter()
                .filter(|(path, before)| to_document.ancestor_shells.get(*path) != Some(*before))
                .map(|(path, _)| path.clone())
                .collect(),
        })
    }

    pub fn provenance(
        &self,
        session_id: &str,
        checkpoint_id: Option<&str>,
        tree: TreeRoot,
        path: &str,
    ) -> Result<ProvenanceReport> {
        self.pending_operations()?;
        let relative = crate::paths::stored_relative(path, true)?;
        ensure!(
            !path.is_empty() && !path.contains('\\') && relative.to_str() == Some(path),
            "Provenance needs a portable relative path"
        );
        let (_, head, history) = self.checked_history(session_id)?;
        let selected = checkpoint_id.unwrap_or(&head.checkpoint_id);
        id(selected)?;
        let end = history
            .iter()
            .position(|checkpoint| checkpoint.checkpoint_id == selected)
            .context("Provenance checkpoint is not in committed ancestry")?;
        let mut previous = None;
        let mut events = Vec::new();
        for checkpoint in &history[..=end] {
            let entries = match tree {
                TreeRoot::Mods => &checkpoint.mods.entries,
                TreeRoot::Game => &checkpoint.game.entries,
            };
            let entry = entries.get(path);
            if previous.as_ref() != entry {
                events.push(ProvenanceEvent {
                    checkpoint_id: checkpoint.checkpoint_id.clone(),
                    sequence: checkpoint.sequence,
                    component: checkpoint.component.clone(),
                    change: change_kind(previous.as_ref(), entry),
                    entry: entry.cloned(),
                });
                previous = entry.cloned();
            }
        }
        ensure!(
            self.read_head(session_id)? == head,
            "Checkpoint HEAD changed while reading provenance; retry"
        );
        Ok(ProvenanceReport {
            schema_version: SCHEMA_VERSION,
            operation: "provenance",
            session_id: session_id.into(),
            checkpoint_id: selected.into(),
            generation: head.generation,
            tree,
            path: path.into(),
            history: events,
        })
    }
}
fn change_kind(before: Option<&Entry>, after: Option<&Entry>) -> &'static str {
    match (before, after) {
        (None, Some(_)) => "added",
        (Some(_), None) => "deleted",
        _ => "changed",
    }
}
