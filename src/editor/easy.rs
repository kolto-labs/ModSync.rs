//! Durable authoring history, independent of the desktop and installation state.
//! Revisions are immutable; only the small head document is replaced atomically.
use super::{EditOperation, apply_operations, read_document};
use crate::{Engine, Guide, Settings, engine::Prediction};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}};

const FORMAT: u32 = 1;
const LIMIT: usize = 4096;

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
        ensure!(!directory.exists(), "Authoring session already exists");
        fs::create_dir(directory)?;
        let lock = lock(directory)?;
        let revision = Revision { id: uuid::Uuid::new_v4().to_string(), label: "Starting guide".into() };
        let head = Head { format: FORMAT, revisions: vec![revision.clone()], cursor: 0 };
        save_revision(directory, &revision.id, guide)?;
        save_head(directory, &head)?;
        Ok(Self { directory: directory.canonicalize()?, head, guide: guide.clone(), _lock: lock })
    }

    pub fn open(directory: &Path) -> Result<Self> {
        let lock = lock(directory)?;
        let head: Head = serde_json::from_str(&read_document(&directory.join("head.json"))?)?;
        ensure!(head.format == FORMAT && !head.revisions.is_empty() && head.revisions.len() <= LIMIT
            && head.cursor < head.revisions.len(), "Invalid authoring history");
        for revision in &head.revisions { revision_path(directory, &revision.id)?; }
        let guide = load_revision(directory, &head.revisions[head.cursor].id)?;
        Ok(Self { directory: directory.canonicalize()?, head, guide, _lock: lock })
    }

    pub fn guide(&self) -> &Guide { &self.guide }
    pub fn directory(&self) -> &Path { &self.directory }
    pub fn revisions(&self) -> &[Revision] { &self.head.revisions }
    pub fn cursor(&self) -> usize { self.head.cursor }
    pub fn can_undo(&self) -> bool { self.head.cursor > 0 }
    pub fn can_redo(&self) -> bool { self.head.cursor + 1 < self.head.revisions.len() }

    /// Persist the entire accepted revision before advancing either memory or
    /// the durable head. Abandoned redo revisions remain on disk for recovery.
    pub fn edit(&mut self, label: &str, operations: &[EditOperation]) -> Result<()> {
        ensure!(label.len() <= 4096, "Revision label is too long");
        ensure!(self.head.cursor + 1 < LIMIT, "Authoring history is full; export a new session");
        let guide = apply_operations(&self.guide, operations)?;
        let revision = Revision { id: uuid::Uuid::new_v4().to_string(), label: label.into() };
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

    pub fn seek(&mut self, cursor: usize) -> Result<()> {
        ensure!(cursor < self.head.revisions.len(), "Revision is out of range");
        let guide = load_revision(&self.directory, &self.head.revisions[cursor].id)?;
        let mut head = self.head.clone();
        head.cursor = cursor;
        save_head(&self.directory, &head)?;
        self.head = head;
        self.guide = guide;
        Ok(())
    }

    pub fn predict(&self, settings: Settings, component: usize, instructions: usize) -> Result<Prediction> {
        ensure!(!self.directory.starts_with(&settings.roots.game)
            && !self.directory.starts_with(&settings.roots.mods), "Authoring history overlaps an install root");
        let mut guide = self.guide.clone();
        ensure!(component < guide.components.len(), "Component is out of range");
        ensure!(instructions <= guide.components[component].instructions.len(), "Instruction is out of range");
        guide.components[component].instructions.truncate(instructions);
        // Retain identities for dependencies/restrictions while excluding future
        // effects. Do not reorder the guide or invent selections.
        for future in guide.components.iter_mut().skip(component + 1) { future.is_selected = false; }
        Engine::new(settings)?.predict(&guide)
    }

    pub fn export(&self, output: &Path) -> Result<()> { super::save_validated(&self.guide, output) }
}

fn lock(directory: &Path) -> Result<fs::File> {
    let metadata = fs::symlink_metadata(directory)?;
    ensure!(metadata.is_dir() && !metadata.file_type().is_symlink(), "Authoring directory must be ordinary");
    let path = directory.join("session.lock");
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        ensure!(metadata.is_file() && !metadata.file_type().is_symlink(), "Invalid authoring lock");
    }
    let file = fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(path)?;
    file.try_lock_exclusive().context("Authoring session is open in another process")?;
    Ok(file)
}

fn revision_path(directory: &Path, id: &str) -> Result<PathBuf> {
    let parsed = uuid::Uuid::parse_str(id).context("Invalid revision identity")?;
    ensure!(parsed.to_string() == id, "Noncanonical revision identity");
    Ok(directory.join(format!("revision-{id}.json")))
}

fn save_revision(directory: &Path, id: &str, guide: &Guide) -> Result<()> {
    let text = super::render_preview(guide)?;
    crate::recovery::atomic_write(&revision_path(directory, id)?, text.as_bytes())
}
fn load_revision(directory: &Path, id: &str) -> Result<Guide> {
    Ok(serde_json::from_str(&read_document(&revision_path(directory, id)?)?)?)
}
fn save_head(directory: &Path, head: &Head) -> Result<()> {
    crate::recovery::atomic_write(&directory.join("head.json"), &serde_json::to_vec_pretty(head)?)
}
