//! Explicit, previewed maintenance transactions. Never an installation component.
//! See docs/parity/maintenance-contract.md for authority and platform limits.
mod fence;
mod journal;
mod plan;
mod platform;

use crate::{paths::Roots, recovery};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub use crate::checkpoint::TreeRoot;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    LowercaseNames,
    OwnerWritable,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    #[serde(with = "roots_serde")]
    pub roots: Roots,
    pub target: TreeRoot,
    pub subtree: String,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capability {
    pub supported: bool,
    pub platform: String,
    pub durability: String,
    pub restrictions: Vec<String>,
    /// Stable admission contract; absent in the original Linux intent format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Operation {
    Rename {
        from: String,
        to: String,
        temporary: String,
    },
    OwnerMode {
        path: String,
        before: u32,
        after: u32,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub digest: String,
    pub operations: Vec<Operation>,
    pub entry_count: usize,
    pub operation_count: usize,
    pub capability: Capability,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutcomeStatus {
    Applied,
    CancelledAndRolledBack,
    FailedAndRolledBack,
    RecoveryRequired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TreeDisposition {
    AppliedTree,
    RestoredOriginalTree,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub digest: String,
    pub completed_operations: usize,
    pub recovered: bool,
    pub status: OutcomeStatus,
    pub failure: Option<String>,
    pub disposition: Option<TreeDisposition>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub pending: bool,
    pub digest: Option<String>,
    pub operation_count: usize,
    pub phase: Option<String>,
}

/// Read-only preview; reads and hashes actual objects without acquiring a lock
/// or creating state. Unsupported platforms and collisions are errors.
pub fn preview(request: &Request, cancelled: &AtomicBool) -> Result<Preview> {
    check_cancelled(cancelled)?;
    plan::validate_roots(&request.roots)?;
    recovery::ensure_clean(&request.roots)?;
    let plan = plan::build(request, cancelled)?;
    plan.preview()
}

/// Recompute the reviewed digest under the shared root locks, durably record
/// intent, then apply. Every post-intent failure returns an explicit terminal
/// outcome; a retained intent always fences ordinary mutations.
pub fn apply(request: &Request, expected_digest: &str, cancelled: &AtomicBool) -> Result<Outcome> {
    check_cancelled(cancelled)?;
    platform::require_capability()?;
    plan::validate_roots(&request.roots)?;
    recovery::ensure_clean(&request.roots)?;
    let lock = recovery::Lock::acquire_maintenance(&request.roots)?;
    recovery::ensure_clean(&request.roots)?;
    let plan = plan::build(request, cancelled)?;
    ensure!(
        plan.digest()? == expected_digest,
        "Maintenance preview changed; review a new preview before applying"
    );
    journal::apply(plan, &lock, cancelled)
}

/// Read-only pending intent summary. Malformed or foreign records are errors,
/// never an idle status.
pub fn status(roots: &Roots) -> Result<Status> {
    plan::validate_roots(roots)?;
    journal::status(roots)
}

/// Recover the caller-selected durable operation. Nonterminal operations restore
/// original state; verified terminal operations finish metadata cleanup only.
pub fn recover(roots: &Roots, expected_digest: &str, cancelled: &AtomicBool) -> Result<Outcome> {
    check_cancelled(cancelled)?;
    platform::require_capability()?;
    plan::validate_roots(roots)?;
    recovery::ensure_checkpoint_clean(roots)?;
    ensure!(
        crate::checkpoint::safe_path(&recovery::state_dir(roots).join("pending"))?.is_none(),
        "Legacy install recovery must complete before maintenance recovery"
    );
    let lock = recovery::Lock::acquire_maintenance(roots)?;
    recovery::ensure_checkpoint_clean(roots)?;
    ensure!(
        crate::checkpoint::safe_path(&recovery::state_dir(roots).join("pending"))?.is_none(),
        "Legacy install recovery must complete before maintenance recovery"
    );
    journal::recover(roots, expected_digest, &lock, cancelled)
}

/// Presence, including incomplete or foreign metadata, fences all ordinary
/// checkpoint/install/deployment writes. This lookup needs only a game root.
pub(crate) fn ensure_clear_game(game: &Path) -> Result<()> {
    ensure!(
        game.is_absolute() && game.parent().is_some(),
        "Maintenance fence requires an absolute non-root game directory"
    );
    let provisional = Roots {
        mods: game.to_path_buf(),
        game: game.to_path_buf(),
    };
    let directory = recovery::state_dir(&provisional).join("maintenance-v1");
    if let Some(metadata) = crate::checkpoint::safe_path(&directory)? {
        ensure!(
            metadata.is_dir(),
            "Maintenance state is not an ordinary directory"
        );
        ensure!(
            std::fs::read_dir(&directory)?.next().is_none(),
            "Unfinished maintenance transaction fences mutations; inspect maintenance status and recover its operation identity"
        );
        // A namespace left empty by failed terminal cleanup must itself flush
        // successfully before a mutation guard may report it as idle.
        crate::checkpoint::persistence::namespace_boundary(&directory)?;
    }
    Ok(())
}

/// Acquisition guard using only the canonical mod root. Presence of any
/// maintenance fence, including malformed or foreign metadata, blocks writers.
pub(crate) fn ensure_clear_mods(root: &Path) -> Result<()> {
    fence::ensure_clear(root)
}

/// Finalize a terminal mod-side fence under its matching held root lock. This
/// path may only prove the recorded tree and remove owned metadata.
pub fn recover_mod_fence(
    root: &Path,
    expected_digest: &str,
    cancelled: &AtomicBool,
) -> Result<Outcome> {
    platform::require_capability()?;
    check_cancelled(cancelled)?;
    let lock = recovery::ModRootLock::acquire_maintenance(root)?;
    recover_mod_fence_locked(lock.root(), expected_digest, cancelled, &lock)
}

pub(crate) fn recover_mod_fence_locked(
    root: &Path,
    expected_digest: &str,
    cancelled: &AtomicBool,
    lock: &recovery::ModRootLock,
) -> Result<Outcome> {
    ensure!(
        lock.guards(root),
        "Mod fence recovery requires its held root lock"
    );
    check_cancelled(cancelled)?;
    fence::recover_only(root, expected_digest, cancelled)
}

fn check_cancelled(flag: &AtomicBool) -> Result<()> {
    ensure!(
        !flag.load(Ordering::Acquire),
        "Maintenance operation cancelled"
    );
    Ok(())
}

mod roots_serde {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Binding {
        mods: std::path::PathBuf,
        game: std::path::PathBuf,
    }
    pub fn serialize<S: serde::Serializer>(
        roots: &Roots,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        Binding {
            mods: roots.mods.clone(),
            game: roots.game.clone(),
        }
        .serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Roots, D::Error> {
        let roots = Binding::deserialize(deserializer)?;
        Ok(Roots {
            mods: roots.mods,
            game: roots.game,
        })
    }
}
