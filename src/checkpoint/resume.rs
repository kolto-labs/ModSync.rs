use super::{
    model::*,
    read::*,
    store::{Store, canonical_bytes},
};
use crate::recovery;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::AtomicBool,
};

fn observed_digest(
    mods: &Tree,
    game: &Tree,
    auxiliary: &BTreeMap<String, AuxiliarySnapshot>,
    shells: &BTreeMap<String, Option<Mode>>,
) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(canonical_bytes(&(mods, game, auxiliary, shells))?)
    ))
}
impl Store {
    pub fn resume_plan(
        &self,
        request: &ResumeRequest,
        definitions: &[serde_json::Value],
        participants: &BTreeMap<String, ParticipantBinding>,
        flag: &AtomicBool,
    ) -> Result<ResumePlan> {
        cancelled(flag)?;
        self.ensure_no_pending()?;
        self.ensure_other_sessions_settled(&request.expected.session_id)?;
        let (session, head, history) = self.checked_history(&request.expected.session_id)?;
        ensure!(
            matches!(head.status.as_str(), "Ready" | "Restored" | "Complete")
                && head.generation == request.expected.generation
                && head.checkpoint_id == request.expected.checkpoint_id,
            "Resume HEAD is stale or unresolved"
        );
        let target = history.last().context("Resume checkpoint missing")?;
        super::history::validate_definitions(definitions)?;
        let prefix = &target.completed_components;
        ensure!(
            definitions.len() >= prefix.len()
                && canonical_bytes(&definitions[..prefix.len()])? == canonical_bytes(prefix)?,
            "Resume changes, deletes or reorders the committed full-definition prefix"
        );
        ensure!(
            participants == &session.participants,
            "Resume auxiliary/profile bindings differ"
        );
        self.verify_target(&session, target, flag)?;
        let mods = self.scan(&self.roots.mods, flag)?;
        let game = self.scan(&self.roots.game, flag)?;
        let (auxiliary, shells) =
            self.scan_auxiliary(&session.participants, &session.ancestor_shells, flag)?;
        ensure!(
            game == target.game
                && auxiliary == target.auxiliary_snapshots
                && shells == target.ancestor_shells,
            "Resume requires exact committed game and auxiliary state; recover first"
        );
        let changed = mods != target.mods;
        ensure!(
            !changed || request.accept_source_changes,
            "Source files changed; restore first or explicitly accept source changes for a new branch"
        );
        ensure!(
            !changed || head.status == "Restored",
            "Source rebase requires a successfully restored checkpoint first; run checkpoint restore/recover before accepting source changes"
        );
        let mut changes = Vec::new();
        let paths: BTreeSet<_> = mods
            .entries
            .keys()
            .chain(target.mods.entries.keys())
            .collect();
        for path in paths {
            let before = target.mods.entries.get(path);
            let after = mods.entries.get(path);
            if before != after {
                changes.push(PathChange {
                    tree: TreeRoot::Mods,
                    path: path.clone(),
                    change: match (before, after) {
                        (None, Some(_)) => "added",
                        (Some(_), None) => "deleted",
                        _ => "changed",
                    },
                    before: before.cloned(),
                    after: after.cloned(),
                });
            }
        }
        ensure!(
            self.read_head(&session.session_id)? == head,
            "HEAD changed while planning resume"
        );
        Ok(ResumePlan {
            expected: request.expected.clone(),
            completed_components: prefix.clone(),
            remaining_components: definitions[prefix.len()..].to_vec(),
            source_changes: changes,
            source_root_mode_changed: mods.root_mode != target.mods.root_mode,
            context: InstallContext {
                ordered_components: definitions.to_vec(),
                initial_completed_components: prefix.clone(),
                participants: participants.clone(),
            },
            observed_sha256: observed_digest(&mods, &game, &auxiliary, &shells)?,
            source_rebase: changed,
            validated_expected: request.expected.clone(),
            source_operation_id: head.operation_id,
            source_status: head.status,
        })
    }

    pub fn branch_for_resume(
        &self,
        lock: &recovery::Lock,
        plan: &ResumePlan,
        flag: &AtomicBool,
        mut observe: impl FnMut(Progress),
    ) -> Result<CaptureReport> {
        self.require_lock(lock)?;
        cancelled(flag)?;
        self.ensure_no_pending()?;
        let expected = &plan.validated_expected;
        self.ensure_other_sessions_settled(&expected.session_id)?;
        let (session, head, history) = self.checked_history(&expected.session_id)?;
        ensure!(
            head.generation == expected.generation
                && head.checkpoint_id == expected.checkpoint_id
                && head.operation_id == plan.source_operation_id
                && head.status == plan.source_status,
            "Resume plan HEAD changed during preflight"
        );
        let target = history.last().context("Resume target missing")?;
        ensure!(
            session.participants == plan.context.participants
                && target.completed_components == plan.context.initial_completed_components,
            "Resume plan context changed"
        );
        let (mods, game, auxiliary, shells, files, bytes) = self.capture_all(
            &session.participants,
            &session.ancestor_shells,
            flag,
            &mut observe,
        )?;
        ensure!(
            observed_digest(&mods, &game, &auxiliary, &shells)? == plan.observed_sha256,
            "Roots or auxiliary state changed after resume preflight"
        );
        let mut baseline = target.clone();
        baseline.mods = mods;
        baseline.game = game;
        baseline.auxiliary_snapshots = auxiliary;
        baseline.ancestor_shells = shells;
        baseline.file_count = files;
        baseline.total_bytes = bytes;
        let kind = if plan.source_rebase {
            "SourceRebase"
        } else {
            "Resume"
        };
        let (branch, baseline) =
            self.publish_branch(&session, target, &plan.context, kind, baseline, None, flag)?;
        cancelled(flag)?;
        let operation = self.publish_transition(
            &session,
            &head,
            target,
            &branch,
            &baseline,
            kind,
            if plan.source_rebase {
                "sourceRebased"
            } else {
                "resumed"
            },
            None,
            None,
        )?;
        let restored = self.finish_transition(lock, &operation, flag, &mut observe)?;
        Ok(CaptureReport {
            schema_version: SCHEMA_VERSION,
            operation: "resumeSession",
            session_id: restored.session_id,
            checkpoint_id: restored.checkpoint_id,
            generation: restored.generation,
            file_count: files,
            total_bytes: bytes,
            durability: super::persistence::durability(super::persistence::writer_origin()?)?,
        })
    }
}
