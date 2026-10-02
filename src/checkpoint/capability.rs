use super::{
    Store,
    model::*,
    persistence,
    read::{MAX_DOCUMENT_BYTES, json, safe_path},
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityReport {
    pub origin: String,
    pub durability: &'static str,
    pub writer_profile_matches_platform: bool,
}
impl Store {
    /// Read-only profile projection; does not create lock/store paths.
    pub fn capability(&self, session: &str) -> Result<CapabilityReport> {
        let session = self.read_session(session)?;
        Ok(CapabilityReport {
            durability: persistence::durability(&session.origin)?,
            writer_profile_matches_platform: persistence::writer_origin()
                .is_ok_and(|origin| origin == session.origin),
            origin: session.origin,
        })
    }
    /// Qualify every recorded location before lock acquisition. Repeat under lock.
    /// Includes incomplete sessions, orphan checkpoints and lifecycle trash.
    pub fn prepare_write(&self) -> Result<()> {
        let origin = persistence::writer_origin()?;
        self.validate_location()?;
        for path in [&self.roots.mods, &self.roots.game, &self.directory] {
            persistence::qualify(path)?;
        }
        let state = self
            .directory
            .parent()
            .context("Checkpoint store needs a parent")?;
        for leaf in [
            "snapshot",
            "snapshot/mods",
            "snapshot/game",
            "snapshot.json",
            "completion.json",
            "pending",
        ] {
            persistence::qualify(&state.join(leaf))?;
        }
        if safe_path(&self.directory)?.is_none() {
            return Ok(());
        }
        let mut bytes = 0u64;
        for (count, entry) in walkdir::WalkDir::new(&self.directory)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| entry.path() != self.directory.join("objects"))
            .enumerate()
        {
            ensure!(
                count < 200_000,
                "Checkpoint qualification inventory exceeds bound"
            );
            let entry = entry?;
            safe_path(entry.path())?.context("Checkpoint qualification artifact disappeared")?;
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry
                .file_name()
                .to_str()
                .context("Non-UTF8 checkpoint artifact")?;
            let is_session = name == "session.json";
            let is_checkpoint = entry
                .path()
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name == "checkpoints")
                && name.ends_with(".json")
                && !name.starts_with(".tmp-");
            if !is_session && !is_checkpoint {
                continue;
            }
            bytes = bytes
                .checked_add(fs::symlink_metadata(entry.path())?.len())
                .context("Qualification size overflow")?;
            ensure!(
                bytes <= 256 * 1024 * 1024,
                "Checkpoint qualification metadata exceeds bound"
            );
            if is_session {
                let session: Session = json(entry.path())?;
                ensure!(
                    session.schema_version == SCHEMA_VERSION
                        && session.roots == self.binding
                        && session.origin == origin,
                    "Checkpoint session capability/schema/roots do not match this writer"
                );
                self.qualify_participants(&session.participants, &session.ancestor_shells)?;
            } else {
                let checkpoint: Checkpoint = json(entry.path())?;
                ensure!(
                    checkpoint.schema_version == SCHEMA_VERSION
                        && checkpoint.roots == self.binding
                        && checkpoint.origin == origin,
                    "Checkpoint capability/schema/roots do not match this writer"
                );
            }
        }
        Ok(())
    }
    pub(super) fn qualify_participants(
        &self,
        participants: &BTreeMap<String, ParticipantBinding>,
        shells: &[String],
    ) -> Result<()> {
        self.validate_participants(participants, shells)?;
        for binding in participants.values() {
            persistence::qualify(Path::new(&binding.path))?;
        }
        for shell in shells {
            persistence::qualify(Path::new(shell))?;
        }
        Ok(())
    }
    /// Acquire root locks only after the new context's auxiliary paths qualify.
    /// Existing-context mutations can use prepare_write plus the ordinary lock API.
    pub fn acquire_lock_with_context(
        &self,
        context: &InstallContext,
    ) -> Result<crate::recovery::Lock> {
        self.prepare_write()?;
        let shells = self.participant_shells(&context.participants)?;
        self.qualify_participants(&context.participants, &shells)?;
        let proof = super::store::canonical_bytes(&context.participants)?;
        ensure!(
            proof.len() as u64 <= MAX_DOCUMENT_BYTES,
            "Participant proof exceeds bound"
        );
        let lock = crate::recovery::Lock::acquire_checkpoint_context(&self.roots, proof)?;
        self.prepare_write()?;
        self.qualify_participants(&context.participants, &shells)?;
        Ok(lock)
    }
}

/// Locks are also used by callers outside checkpoint adapters. Fence those before
/// Lock::acquire creates any state whenever checkpoint metadata already exists.
pub(crate) fn prepare_existing(roots: &crate::paths::Roots) -> Result<()> {
    let path = crate::recovery::state_dir(roots).join("checkpoints-v1");
    match fs::symlink_metadata(path) {
        Ok(_) => Store::open(roots)?.prepare_write(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
