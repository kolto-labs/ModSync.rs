//! Durable checkpoint history, component transactions, restore, and ordered resume.
//! Immutable snapshots bind completed prefixes to verified trees and receipts.
//! Read operations never create store paths; mutations require the recovery lock.
mod auxiliary;
mod capability;
pub use capability::CapabilityReport;
pub(crate) use capability::prepare_existing;
mod finalize;
mod guard;
mod history;
mod journal;
mod model;
pub(crate) mod persistence;
mod read;
mod restore;
mod resume;
mod store;
mod transition;
pub use model::*;
pub use store::Store;
pub use transition::{ExternalProcessStatus, ProcessQuiescenceReport, ProcessRecoveryReport};

/// Presence, including malformed or foreign records, fences ordinary mutations.
/// Explicit process recovery uses capability checks without waiving this guard.
pub(crate) fn ensure_external_process_clear(game: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(windows)]
    crate::patcher::checkpoint_job::ensure_clear_game(game)?;
    #[cfg(not(windows))]
    let _ = game;
    Ok(())
}

pub(crate) use guard::ensure_game_checkpoint_clean;

pub(crate) use persistence::namespace_boundary as sync_recovery_directory;

mod browser;
pub use browser::{HistoryEntry, HistoryReport};

mod lifecycle;
pub use lifecycle::{DeleteReport, GcReport, RepairIssue, RepairReport};
