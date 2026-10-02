use super::{model::*, read::*, store::Store};
use crate::{paths::Roots, recovery};
use anyhow::{Context, Result, ensure};
use std::{fs, path::Path};

/// Read-only game-scoped fence for callers that do not possess source roots.
/// Bound session roots are validated rather than guessing a source directory.
pub(crate) fn ensure_game_checkpoint_clean(game: &Path) -> Result<()> {
    crate::maintenance::ensure_clear_game(game)?;
    let canonical = fs::canonicalize(game)?;
    ensure!(
        canonical == game && safe_path(game)?.is_some_and(|m| m.is_dir()),
        "Checkpoint game fence requires a canonical safe game directory"
    );
    super::ensure_external_process_clear(game)?;
    let provisional = Roots {
        mods: canonical.clone(),
        game: canonical.clone(),
    };
    let directory = recovery::state_dir(&provisional).join("checkpoints-v1");
    if safe_path(&directory)?.is_none() {
        return Ok(());
    }
    let mut binding: Option<RootBinding> = None;
    let sessions = directory.join("sessions");
    if let Some(metadata) = safe_path(&sessions)? {
        ensure!(
            metadata.is_dir(),
            "Checkpoint sessions path is not a directory"
        );
        for (count, entry) in fs::read_dir(&sessions)?.enumerate() {
            ensure!(count < 10_000, "Checkpoint session inventory exceeds bound");
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-UTF8 session ID"))?;
            id(&name)?;
            ensure!(
                safe_path(&entry.path())?.is_some_and(|m| m.is_dir()),
                "Unsafe session directory"
            );
            if safe_path(&entry.path().join("session.json"))?.is_some() {
                let session: Session = json(&entry.path().join("session.json"))?;
                ensure!(
                    session.roots.game == game.to_str().context("Non-UTF8 game path")?,
                    "Checkpoint game binding differs"
                );
                if let Some(existing) = &binding {
                    ensure!(
                        existing == &session.roots,
                        "Checkpoint sessions disagree about source roots"
                    );
                } else {
                    binding = Some(session.roots);
                }
            }
        }
    }
    let store = if let Some(binding) = binding {
        let roots = Roots::new(Path::new(&binding.mods), Path::new(&binding.game))?;
        Store::open(&roots)?
    } else {
        // Only incomplete pre-session artifacts may exist without a root binding.
        // Normal store validation/readers still reject foreign paths or orphan HEADs.
        Store {
            roots: provisional,
            directory,
            binding: RootBinding {
                mods: game.to_str().context("Non-UTF8 game path")?.into(),
                game: game.to_str().context("Non-UTF8 game path")?.into(),
            },
        }
    };
    store.validate_location()?;
    ensure!(
        store.pending_operations()?.is_empty(),
        "Unfinished checkpoint operation fences standalone deployment"
    );
    store.ensure_no_unfinished_install_sessions()
}
