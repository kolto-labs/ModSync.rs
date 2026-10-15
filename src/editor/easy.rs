//! Durable authoring history, independent of the desktop and installation state.
//! Revisions are immutable; only the small head document is replaced atomically.
pub mod diff;
pub mod disk;
mod inputs;
pub mod patcher;
use super::{EditOperation, apply_operations, read_document};
use crate::{Engine, Guide, Settings, engine::Prediction};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const FORMAT: u32 = 1;
const REVISION_FORMAT: u32 = 1;
const LIMIT: usize = 4096;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuthoringRevision {
    authoring_revision_format: u32,
    ordering: crate::model::Ordering,
    guide: Guide,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Revision {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Head {
    format: u32,
    revisions: Vec<Revision>,
    cursor: usize,
}

/// The lock is held for the lifetime of the open authoring session. Sessions
/// belong outside game/mod trees so prediction cannot ingest its own history.
pub struct Session {
    directory: PathBuf,
    head: Head,
    guide: Guide,
    _lock: fs::File,
}

impl Session {
    pub fn create(directory: &Path, guide: &Guide) -> Result<Self> {
        super::validate_guide(guide)?;
        ensure!(!directory.exists(), "Authoring session already exists");
        if let Some(parent) = directory.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir(directory)?;
        let lock = lock(directory)?;
        let revision = Revision {
            id: uuid::Uuid::new_v4().to_string(),
            label: "Starting guide".into(),
        };
        let head = Head {
            format: FORMAT,
            revisions: vec![revision.clone()],
            cursor: 0,
        };
        save_revision(directory, &revision.id, guide)?;
        save_head(directory, &head)?;
        Ok(Self {
            directory: directory.canonicalize()?,
            head,
            guide: guide.clone(),
            _lock: lock,
        })
    }

    pub fn open(directory: &Path) -> Result<Self> {
        let lock = lock(directory)?;
        let head: Head = serde_json::from_str(&read_document(&directory.join("head.json"))?)?;
        ensure!(
            head.format == FORMAT
                && !head.revisions.is_empty()
                && head.revisions.len() <= LIMIT
                && head.cursor < head.revisions.len(),
            "Invalid authoring history"
        );
        for revision in &head.revisions {
            revision_path(directory, &revision.id)?;
        }
        let guide = load_revision(directory, &head.revisions[head.cursor].id)?;
        super::validate_guide(&guide)?;
        Ok(Self {
            directory: directory.canonicalize()?,
            head,
            guide,
            _lock: lock,
        })
    }

    pub fn guide(&self) -> &Guide {
        &self.guide
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn revisions(&self) -> &[Revision] {
        &self.head.revisions
    }
    pub fn cursor(&self) -> usize {
        self.head.cursor
    }
    pub fn can_undo(&self) -> bool {
        self.head.cursor > 0
    }
    pub fn can_redo(&self) -> bool {
        self.head.cursor + 1 < self.head.revisions.len()
    }

    /// Persist the entire accepted revision before advancing either memory or
    /// the durable head. Abandoned redo revisions remain on disk for recovery.
    pub fn edit(&mut self, label: &str, operations: &[EditOperation]) -> Result<()> {
        ensure!(label.len() <= 4096, "Revision label is too long");
        ensure!(
            self.head.cursor + 1 < LIMIT,
            "Authoring history is full; export a new session"
        );
        let guide = apply_operations(&self.guide, operations)?;
        let revision = Revision {
            id: uuid::Uuid::new_v4().to_string(),
            label: label.into(),
        };
        let mut head = self.head.clone();
        head.revisions.truncate(head.cursor + 1);
        head.revisions.push(revision.clone());
        head.cursor += 1;
        save_revision(&self.directory, &revision.id, &guide)?;
        save_head(&self.directory, &head)?;
        self.head = head;
        self.guide = guide;
        Ok(())
    }

    /// Move one authored instruction within its component while fencing the
    /// edit against the revision and instruction identity shown to the user.
    /// The move is stored as a normal immutable recipe revision, so undo/redo
    /// restores the exact prior ordering without regenerating instruction IDs.
    pub fn move_instruction(
        &mut self,
        component: &str,
        index: usize,
        expected_guid: &str,
        destination: usize,
        expected_revision: &str,
    ) -> Result<()> {
        ensure!(
            self.head.revisions[self.head.cursor].id == expected_revision,
            "The recipe changed; reopen the action"
        );
        let instruction = super::find_component(&self.guide, component)
            .and_then(|component| component.instructions.get(index))
            .context("Saved action no longer exists")?;
        ensure!(
            instruction.guid == expected_guid,
            "The action changed; reopen it"
        );
        ensure!(
            destination
                < super::find_component(&self.guide, component)
                    .context("Mod is no longer in this session")?
                    .instructions
                    .len(),
            "Action destination is out of range"
        );
        ensure!(index != destination, "Action is already at that position");
        self.edit(
            "Move saved action",
            &[EditOperation::MoveInstruction {
                component: component.to_owned(),
                index,
                destination,
            }],
        )
    }

    pub fn seek(&mut self, cursor: usize) -> Result<()> {
        ensure!(
            cursor < self.head.revisions.len(),
            "Revision is out of range"
        );
        let guide = load_revision(&self.directory, &self.head.revisions[cursor].id)?;
        let mut head = self.head.clone();
        head.cursor = cursor;
        save_head(&self.directory, &head)?;
        self.head = head;
        self.guide = guide;
        Ok(())
    }

    pub fn predict(
        &self,
        settings: Settings,
        component: usize,
        instructions: usize,
    ) -> Result<Prediction> {
        self.check_roots(&settings)?;
        let mut guide = self.guide_through(component, instructions)?;
        let (engine, observed) = self.prediction_engine(settings, &mut guide)?;
        let mut prediction = engine.predict(&guide)?;
        if observed > 0 {
            prediction.events.insert(0, crate::engine::Event {
                component: String::new(),
                step: 0,
                message: format!("Using verified checkpoint after {observed} recorded actions; predicting the remaining instructions"),
            });
        }
        Ok(prediction)
    }

    pub fn inspect_patcher(
        &self,
        settings: Settings,
        component: usize,
        instructions: usize,
        source: &str,
        namespace: Option<usize>,
    ) -> Result<patcher::PatchPreview> {
        self.check_roots(&settings)?;
        let mut guide = self.guide_through(component, instructions)?;
        let (engine, _) = self.prediction_engine(settings, &mut guide)?;
        engine.inspect_predicted_patcher(&guide, source, namespace)
    }

    fn prediction_engine(&self, settings: Settings, guide: &mut Guide) -> Result<(Engine, usize)> {
        let history = self.disk_history()?;
        if let Some(history) = &history {
            ensure!(
                history.mods == settings.roots.mods && history.game == settings.roots.game,
                "Authoring prediction belongs to different install roots"
            );
            ensure!(
                history.pending.is_none() && history.restoring.is_none(),
                "Recover the pending disk action before predicting its filesystem"
            );
        }
        crate::recovery::ensure_clean(&settings.roots)?;
        let engine = Engine::new(settings.clone())?;
        // Reuse only an executed prefix from precisely the same saved guide.
        // A changed selection, condition, input, or recipe invalidates reuse.
        let mut observed = 0;
        if let Some(mut history) = history.clone() {
            let positions: Vec<_> = guide
                .ordered()?
                .into_iter()
                .flat_map(|owner| {
                    owner
                        .instructions
                        .iter()
                        .enumerate()
                        .map(move |(index, _)| (owner, index))
                })
                .collect();
            let current = serde_json::to_value(&self.guide)?;
            let selection_matches = self
                .guide
                .components
                .iter()
                .filter(|c| c.is_selected)
                .map(|c| (&c.guid, &c.name))
                .eq(guide
                    .components
                    .iter()
                    .filter(|c| c.is_selected)
                    .map(|c| (&c.guid, &c.name)));
            let mut counts = std::collections::BTreeMap::<String, usize>::new();
            for (applied, (owner, index)) in
                history.actions.iter().take(history.cursor).zip(positions)
            {
                let saved = load_revision(&self.directory, &applied.revision)?;
                if !selection_matches
                    || saved.ordering != self.guide.ordering
                    || serde_json::to_value(saved)? != current
                    || serde_json::to_value(&applied.definition)?
                        != serde_json::to_value(disk::action_definition(owner, index)?)?
                {
                    break;
                }
                *counts.entry(owner.guid.clone()).or_default() += 1;
                observed += 1;
            }
            if observed > 0 {
                history.cursor = observed;
                self.verify_applied_inputs(&history)?;
                let consumed = self.consumed_archive_inputs(&history)?;
                let last = &history.actions[observed - 1];
                let engine =
                    engine.with_authoring_boundary(&last.report.after, &last.definition)?;
                let engine = self.attach_remaining_archive_inputs(engine, &settings, &consumed)?;
                // Preserve selected identities for conditions and dependencies.
                for component in &mut guide.components {
                    if let Some(count) = counts.get(&component.guid) {
                        component.instructions.drain(..*count);
                    }
                }
                return Ok((engine, observed));
            }
        }
        let engine =
            if let Some(first) = history.as_ref().and_then(|history| history.actions.first()) {
                engine.with_authoring_boundary(&first.report.before, &first.definition)?
            } else {
                engine
            };
        Ok((self.attach_archive_inputs(engine, &settings)?, observed))
    }

    fn check_roots(&self, settings: &Settings) -> Result<()> {
        ensure!(
            !self.directory.starts_with(&settings.roots.game)
                && !self.directory.starts_with(&settings.roots.mods)
                && !settings.roots.game.starts_with(&self.directory)
                && !settings.roots.mods.starts_with(&self.directory),
            "Authoring history overlaps an install root"
        );
        Ok(())
    }

    fn guide_through(&self, component: usize, instructions: usize) -> Result<Guide> {
        let mut guide = self.guide.clone();
        ensure!(
            component < guide.components.len(),
            "Component is out of range"
        );
        ensure!(
            instructions <= guide.components[component].instructions.len(),
            "Instruction is out of range"
        );
        ensure!(
            guide.components[component].is_selected,
            "Cannot preview an excluded component: {}",
            guide.components[component].name
        );
        let ordered = guide.ordered()?;
        let target = ordered
            .iter()
            .position(|owner| crate::guid::eq_ascii(&owner.guid, &guide.components[component].guid))
            .context("Preview target is not in the selected execution order")?;
        let future: std::collections::BTreeSet<_> = ordered
            .iter()
            .skip(target + 1)
            .map(|owner| crate::guid::key_ascii(&owner.guid))
            .collect();
        guide.components[component]
            .instructions
            .truncate(instructions);
        // Selection is recipe context, not the preview's execution boundary.
        // Keep it intact for dependencies, restrictions and CleanList names.
        // Explicit prediction accepts these empty effect lists while retaining
        // the complete selected ordering graph and checkpoint context.
        for owner in &mut guide.components {
            if future.contains(&crate::guid::key_ascii(&owner.guid)) {
                owner.instructions.clear();
            }
        }
        Ok(guide)
    }

    pub fn export(&self, output: &Path) -> Result<()> {
        super::save_validated(&self.guide, output)
    }
}

fn lock(directory: &Path) -> Result<fs::File> {
    let metadata = fs::symlink_metadata(directory)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "Authoring directory must be ordinary"
    );
    let path = directory.join("session.lock");
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "Invalid authoring lock"
        );
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    file.try_lock_exclusive()
        .context("Authoring session is open in another process")?;
    Ok(file)
}

fn revision_path(directory: &Path, id: &str) -> Result<PathBuf> {
    let parsed = uuid::Uuid::parse_str(id).context("Invalid revision identity")?;
    ensure!(parsed.to_string() == id, "Noncanonical revision identity");
    Ok(directory.join(format!("revision-{id}.json")))
}

fn save_revision(directory: &Path, id: &str, guide: &Guide) -> Result<()> {
    let text = super::render_preview(&AuthoringRevision {
        authoring_revision_format: REVISION_FORMAT,
        ordering: guide.ordering,
        guide: guide.clone(),
    })?;
    crate::recovery::atomic_write(&revision_path(directory, id)?, text.as_bytes())
}
fn load_revision(directory: &Path, id: &str) -> Result<Guide> {
    let text = read_document(&revision_path(directory, id)?)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    let guide = if value.get("authoringRevisionFormat").is_some()
        || (value.get("thisMod").is_none() && value.get("Components").is_none())
    {
        let revision: AuthoringRevision = serde_json::from_str(&text)?;
        ensure!(
            revision.authoring_revision_format == REVISION_FORMAT,
            "Unsupported authoring revision format: {}",
            revision.authoring_revision_format
        );
        let mut guide = revision.guide;
        guide.ordering = revision.ordering;
        guide
    } else {
        serde_json::from_str::<Guide>(&text)?
    };
    super::validate_guide(&guide)?;
    Ok(guide)
}
fn save_head(directory: &Path, head: &Head) -> Result<()> {
    crate::recovery::atomic_write(
        &directory.join("head.json"),
        &serde_json::to_vec_pretty(head)?,
    )
}
