//! Durable linkage between saved actions and the engine's filesystem checkpoints.
use super::Session;
use crate::{
    Component, Engine, Settings,
    engine::{AuthoringActionReport, CheckpointRunReport, Event},
    install_progress::Progress,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppliedAction {
    pub revision: String,
    pub definition: Component,
    pub report: AuthoringActionReport,
    #[serde(default)]
    pub consumed_inputs: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingAction {
    pub revision: String,
    pub definition: Component,
    pub before: Option<CheckpointRunReport>,
    #[serde(default)]
    pub consumed_inputs: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiskHistory {
    pub format: u32,
    pub mods: PathBuf,
    pub game: PathBuf,
    pub actions: Vec<AppliedAction>,
    #[serde(default)]
    pub cursor: usize,
    pub pending: Option<PendingAction>,
    #[serde(default)]
    pub restoring: Option<usize>,
}

impl Session {
    pub fn recover_disk_action(&mut self, cancelled: &std::sync::atomic::AtomicBool) -> Result<()> {
        let mut history = self.disk_history()?.context("No filesystem history")?;
        let pending = history
            .pending
            .as_ref()
            .context("No pending action")?
            .clone();
        let settings = Settings::new(&history.mods, &history.game)?;
        self.check_roots(&settings)?;
        let lock = crate::recovery::Lock::acquire(&settings.roots)?;
        let store = crate::checkpoint::Store::open(&settings.roots)?;
        let Some(before) = pending.before.as_ref() else {
            // Execution cannot start until the baseline identity is saved. An
            // interrupted baseline may still leave a legacy recovery fence or
            // an unfinished checkpoint session; neither may be waived here.
            crate::recovery::ensure_clean(&settings.roots).context(
                "The action stopped before saving its checkpoint identity; restore the configured roots' verified recovery boundary before reconciling this journal",
            )?;
            store.ensure_no_unfinished_install_sessions()?;
            history.pending = None;
            self.save_disk_history(&history)?;
            return Ok(());
        };
        let recovered = store.recover_authoring_action(
            &lock,
            before,
            &serde_json::to_value(&pending.definition)?,
            cancelled,
            |_| {},
        )?;
        if let Some(report) = recovered {
            history.actions.push(AppliedAction {
                revision: pending.revision,
                definition: pending.definition,
                report,
                consumed_inputs: pending.consumed_inputs,
            });
            history.cursor = history.actions.len();
        }
        history.pending = None;
        self.save_disk_history(&history)
    }
    pub fn disk_history(&self) -> Result<Option<DiskHistory>> {
        let path = self.directory.join("disk.json");
        if !path.try_exists()? {
            return Ok(None);
        }
        let history: DiskHistory = serde_json::from_str(&super::super::read_document(&path)?)?;
        ensure!(
            history.format == 1
                && history.actions.len() <= 10000
                && history.cursor <= history.actions.len()
                && history
                    .restoring
                    .is_none_or(|cursor| cursor <= history.actions.len()),
            "Invalid filesystem history"
        );
        Ok(Some(history))
    }

    /// Apply the next action in guide order. A durable pending record survives
    /// errors and process death; it cannot silently be retried or overwritten.
    pub fn apply_to_disk(
        &mut self,
        settings: Settings,
        component: usize,
        instruction: usize,
        emit: impl FnMut(Event),
        progress: impl FnMut(Progress),
    ) -> Result<AuthoringActionReport> {
        self.check_roots(&settings)?;
        let mut history = self.disk_history()?.unwrap_or_else(|| DiskHistory {
            format: 1,
            mods: settings.roots.mods.clone(),
            game: settings.roots.game.clone(),
            actions: Vec::new(),
            cursor: 0,
            pending: None,
            restoring: None,
        });
        ensure!(
            history.mods == settings.roots.mods && history.game == settings.roots.game,
            "This session's filesystem history belongs to different roots"
        );
        ensure!(
            history.pending.is_none() && history.restoring.is_none(),
            "A recorded action is pending; inspect and recover its checkpoint before continuing"
        );
        crate::recovery::ensure_clean(&settings.roots)?;
        crate::checkpoint::Store::open(&settings.roots)?.ensure_no_unfinished_install_sessions()?;
        self.verify_applied_inputs(&history)?;
        let consumed = self.consumed_archive_inputs(&history)?;
        let ordered = self.guide.ordered()?;
        let positions: Vec<_> = ordered
            .iter()
            .flat_map(|owner| {
                owner
                    .instructions
                    .iter()
                    .enumerate()
                    .map(move |(index, _)| (*owner, index))
            })
            .collect();
        for (applied, (owner, index)) in history.actions.iter().take(history.cursor).zip(&positions)
        {
            let saved = super::load_revision(&self.directory, &applied.revision)?;
            ensure!(
                saved.ordering == self.guide.ordering,
                "Previously applied ordering changed; restore its filesystem boundary before proceeding"
            );
            ensure!(
                serde_json::to_value(&applied.definition)?
                    == serde_json::to_value(action_definition(owner, *index)?)?,
                "Previously applied instructions changed; restore their filesystem boundary before proceeding"
            );
        }
        ensure!(history.cursor < 10000, "Filesystem history is full");
        let (owner, next) = positions
            .get(history.cursor)
            .context("No recorded action remains")?;
        let requested = self
            .guide
            .components
            .get(component)
            .context("Mod is out of range")?;
        ensure!(
            crate::guid::eq_ascii(&owner.guid, &requested.guid) && *next == instruction,
            "Apply actions in guide order; the next action is {} / {}",
            owner.name,
            next + 1
        );
        let definition = action_definition(requested, instruction)?;
        if history.cursor < history.actions.len() {
            crate::recovery::atomic_write(
                &self
                    .directory
                    .join(format!("abandoned-disk-{}.json", uuid::Uuid::new_v4())),
                &serde_json::to_vec_pretty(&history)?,
            )?;
            history.actions.truncate(history.cursor);
        }
        let revision = self.head.revisions[self.head.cursor].id.clone();
        history.pending = Some(PendingAction {
            revision: revision.clone(),
            definition: definition.clone(),
            before: None,
            consumed_inputs: None,
        });
        self.save_disk_history(&history)?;
        let result = (|| {
            self.attach_remaining_archive_inputs(
                Engine::new(settings.clone())?,
                &settings,
                &consumed,
            )?
            .install_recorded_action_with_input_plan(
                &self.guide,
                component,
                instruction,
                |before, newly_consumed| {
                    let mut plan = Vec::new();
                    for logical in newly_consumed {
                        let relative = logical.strip_prefix(&settings.roots.mods)?;
                        ensure!(
                            relative.components().count() == 1,
                            "Consumed archive is outside its declared root"
                        );
                        let name = relative.to_str().context("Archive filename is not UTF-8")?;
                        if !plan.iter().any(|existing| existing == name) {
                            plan.push(name.to_owned());
                        }
                    }
                    let pending = history.pending.as_mut().context("Missing action journal")?;
                    pending.consumed_inputs = Some(plan);
                    pending.before = before.cloned();
                    self.save_disk_history(&history)
                },
                emit,
                progress,
            )
        })();
        match result {
            Ok(report) => {
                history.actions.push(AppliedAction {
                    revision,
                    definition,
                    report: report.clone(),
                    consumed_inputs: history
                        .pending
                        .as_ref()
                        .and_then(|pending| pending.consumed_inputs.clone()),
                });
                history.pending = None;
                history.cursor = history.actions.len();
                self.save_disk_history(&history)?;
                Ok(report)
            }
            Err(error) => {
                // Preflight errors have no boundary to recover. Never clear an
                // uncertain snapshot or unfinished checkpoint session.
                if history
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.before.is_none())
                    && crate::recovery::ensure_clean(&settings.roots).is_ok()
                    && crate::checkpoint::Store::open(&settings.roots)?
                        .ensure_no_unfinished_install_sessions()
                        .is_ok()
                {
                    history.pending = None;
                    self.save_disk_history(&history)?;
                }
                Err(error)
            }
        }
    }

    /// Restore saved bytes rather than replaying an action. Redo checkpoints stay
    /// immutable, and the checkpoint service saves current files before restoring.
    pub fn seek_disk(
        &mut self,
        cursor: usize,
        cancelled: &std::sync::atomic::AtomicBool,
        mut progress: impl FnMut(Progress),
    ) -> Result<()> {
        let mut history = self.disk_history()?.context("No filesystem history")?;
        ensure!(
            history.pending.is_none(),
            "Recover the pending action before restoring history"
        );
        ensure!(
            cursor <= history.actions.len(),
            "Filesystem revision is out of range"
        );
        ensure!(
            history.restoring.is_none_or(|target| target == cursor),
            "Resume the unfinished filesystem restore first"
        );
        if history.cursor == cursor && history.restoring.is_none() {
            return Ok(());
        }
        let boundary = if cursor == 0 {
            &history
                .actions
                .first()
                .context("No applied action to undo")?
                .report
                .before
        } else {
            &history.actions[cursor - 1].report.after
        }
        .clone();
        let settings = Settings::new(&history.mods, &history.game)?;
        self.check_roots(&settings)?;
        let lock = crate::recovery::Lock::acquire(&settings.roots)?;
        if history.restoring == Some(cursor) {
            crate::checkpoint::Store::open(&settings.roots)?.repair_authoring_import(
                &lock,
                &boundary.session_id,
                &boundary.checkpoint_id,
                cancelled,
            )?;
        }
        crate::recovery::ensure_clean(&settings.roots)?;
        history.restoring = Some(cursor);
        self.save_disk_history(&history)?;
        let restore_progress = Progress {
            phase: crate::install_progress::Phase::CreatingSnapshot,
            top_level_component_name: None,
            top_level_component_guid: None,
            component_index: 0,
            component_total: 0,
            owner_name: None,
            owner_guid: None,
            owner_path: Vec::new(),
            instruction_index: None,
            instruction_total: None,
            operation: None,
            message: "Restoring saved filesystem boundary".into(),
            snapshot_pending: true,
        };
        progress(restore_progress.clone());
        let mut last_stage = "";
        let mut last_emit = std::time::Instant::now();
        let mut pending_progress = None;
        let restore_result = crate::checkpoint::Store::open(&settings.roots)?
            .restore_authoring_snapshot(
                &lock,
                &boundary.session_id,
                &boundary.checkpoint_id,
                cancelled,
                |checkpoint| {
                    let now = std::time::Instant::now();
                    let mut event = restore_progress.clone();
                    event.message = if checkpoint.phase == "restored" {
                        format!(
                            "Checkpoint file restoration completed: {} files, {} bytes processed; finalizing filesystem boundary",
                            checkpoint.files, checkpoint.bytes,
                        )
                    } else {
                        format!(
                            "Checkpoint {}: {} files, {} bytes processed",
                            checkpoint.phase, checkpoint.files, checkpoint.bytes,
                        )
                    };
                    // Stage transitions are facts, not a completion percentage.
                    // Coalesce per-file events without dropping the latest fact.
                    if checkpoint.phase != last_stage
                        || now.duration_since(last_emit) >= std::time::Duration::from_millis(250)
                    {
                        last_stage = checkpoint.phase;
                        last_emit = now;
                        pending_progress = None;
                        progress(event);
                    } else {
                        pending_progress = Some(event);
                    }
                },
            );
        if let Some(event) = pending_progress {
            progress(event);
        }
        // Only the returned restore/finalize result authorizes cursor advancement.
        restore_result?;
        history.cursor = cursor;
        history.restoring = None;
        self.save_disk_history(&history)?;
        Ok(())
    }

    fn save_disk_history(&self, history: &DiskHistory) -> Result<()> {
        crate::recovery::atomic_write(
            &self.directory.join("disk.json"),
            &serde_json::to_vec_pretty(history)?,
        )
    }
}

pub(super) fn action_definition(owner: &Component, index: usize) -> Result<Component> {
    let mut definition = owner.clone();
    definition.instructions = vec![
        owner
            .instructions
            .get(index)
            .context("Action is out of range")?
            .clone(),
    ];
    Ok(definition)
}
