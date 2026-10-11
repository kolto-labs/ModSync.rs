//! A root-bound writer fence survives lost runner settings and process interruption.
use super::{Store, read::safe_path};
use crate::{paths::Roots, recovery};
use anyhow::{Context, Result, ensure};
use std::io::Read;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn location(game: &Path) -> PathBuf {
    let provisional = Roots {
        mods: game.to_path_buf(),
        game: game.to_path_buf(),
    };
    recovery::state_dir(&provisional).join("external-writers-v1.json")
}
pub(crate) fn ensure_clear(game: &Path) -> Result<()> {
    ensure!(
        safe_path(&location(game))?.is_none(),
        "Unresolved external installer writers fence this game; prove process/prefix quiescence before checkpoint recovery, restore or mutation"
    );
    Ok(())
}
/// Registration only adds a fence; it cannot grant mutation or waive a journal.
pub(crate) struct Gate {
    path: PathBuf,
    bytes: Vec<u8>,
}
impl Gate {
    pub(crate) fn register(roots: &Roots, requests: &Path, prefix: Option<&Path>) -> Result<Self> {
        ensure_clear(&roots.game)?;
        let store = Store::open(roots)?;
        let operations = store.pending_operations()?;
        ensure!(
            operations.len() == 1,
            "External writer needs exactly one owned active checkpoint operation"
        );
        let requests = fs::canonicalize(requests)?;
        let prefix = prefix.map(fs::canonicalize).transpose()?;
        let bytes = serde_json::to_vec_pretty(&serde_json::json!({
            "schemaVersion": 1, "origin": super::persistence::writer_origin()?,
            "writerId": uuid::Uuid::new_v4().to_string(), "status": "mayHaveLiveWriters",
            "mods": roots.mods, "game": roots.game, "requestDirectory": requests,
            "winePrefix": prefix, "checkpointOperation": operations[0], "ownerPid": std::process::id()
        }))?;
        ensure!(
            bytes.len() <= 4 * 1024 * 1024,
            "External writer ownership record exceeds bound"
        );
        let path = location(&roots.game);
        ensure!(
            safe_path(path.parent().context("Writer fence has no parent")?)?
                .is_some_and(|meta| meta.is_dir()),
            "Writer fence requires existing safe recovery state"
        );
        recovery::atomic_write(&path, &bytes)?;
        fs::File::open(path.parent().context("Writer fence has no parent")?)?.sync_all()?;
        Ok(Self { path, bytes })
    }
    /// Caller must prove prefix/process quiescence first. Never infer it from PID
    /// disappearance, a lost settings object, or an interrupted request status.
    pub(crate) fn clear_after_quiescence(&mut self) -> Result<()> {
        if self.bytes.is_empty() {
            return Ok(());
        }
        ensure!(
            safe_path(&self.path)?.is_some_and(|meta| meta.is_file()),
            "External writer ownership fence changed"
        );
        let file = crate::paths::open_regular_read(&self.path)?;
        ensure!(
            file.metadata()?.len() == self.bytes.len() as u64,
            "External writer fence ownership size changed"
        );
        let mut observed = Vec::new();
        file.take(self.bytes.len() as u64 + 1)
            .read_to_end(&mut observed)?;
        ensure!(
            observed == self.bytes,
            "External writer fence ownership bytes changed"
        );
        fs::remove_file(&self.path)?;
        fs::File::open(self.path.parent().context("Writer fence has no parent")?)?.sync_all()?;
        self.bytes.clear();
        Ok(())
    }
}
