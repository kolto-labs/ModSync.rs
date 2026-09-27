//! Synchronous installation facts, separate from the legacy event/log stream.
//!
//! Observers are non-fallible and must not panic. A channel adapter should ignore
//! failed delivery when its receiver is gone; delivery cannot change installation.
//! The engine's returned Result is authoritative for failure/cancellation. Only
//! Completed denotes committed success, including if cancellation arrives later.
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Preparing,
    CreatingSnapshot,
    InstallingComponent,
    RunningInstruction,
    ComponentCompleted,
    RecordingCompletion,
    Completed,
}

/// One GUID-bound owner in a reached top-level/Choose execution path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerFrame {
    pub name: String,
    pub guid: String,
    /// One-based authored Choose index in the parent; None for a top-level owner.
    pub reached_from_instruction: Option<usize>,
}

/// A fact at a real engine boundary, not an estimate of bytes, time or work.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: Phase,
    pub top_level_component_name: Option<String>,
    pub top_level_component_guid: Option<String>,
    /// One-based active top-level suffix index; zero outside component execution.
    pub component_index: usize,
    /// Ordered, selected, uncompleted top-level components only.
    pub component_total: usize,
    pub owner_name: Option<String>,
    pub owner_guid: Option<String>,
    pub owner_path: Vec<OwnerFrame>,
    /// One-based authored index in the current owner, including condition skips.
    pub instruction_index: Option<usize>,
    pub instruction_total: Option<usize>,
    pub operation: Option<String>,
    pub message: String,
    /// True only after snapshot creation succeeds, false after finalization.
    /// On failure callers must query recovery using the captured execution roots;
    /// a failed begin may already have left recovery artifacts.
    pub snapshot_pending: bool,
}
