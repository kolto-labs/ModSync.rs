//! Compact history projection for CLI and desktop; no extra filesystem authority.
use super::{ComponentIdentity, SCHEMA_VERSION, Store};
use anyhow::Result;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub checkpoint_id: String,
    pub parent_id: Option<String>,
    pub sequence: u64,
    pub kind: String,
    pub component: Option<ComponentIdentity>,
    pub completed_components: usize,
    pub file_count: u64,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryReport {
    pub schema_version: u32,
    pub operation: &'static str,
    pub session_id: String,
    pub checkpoint_id: String,
    pub generation: u64,
    pub status: String,
    pub parent_session_id: Option<String>,
    pub parent_checkpoint_id: Option<String>,
    pub entries: Vec<HistoryEntry>,
}

impl Store {
    /// Walk validated committed ancestry once, retaining generation/stale-HEAD
    /// checks and the reader's aggregate bounds. Does not hash live roots.
    pub fn history(&self, session_id: &str) -> Result<HistoryReport> {
        let (session, head, history) = self.checked_history(session_id)?;
        Ok(HistoryReport {
            schema_version: SCHEMA_VERSION,
            operation: "history",
            session_id: session.session_id,
            checkpoint_id: head.checkpoint_id,
            generation: head.generation,
            status: head.status,
            parent_session_id: session.parent_session_id,
            parent_checkpoint_id: session.parent_checkpoint_id,
            entries: history
                .into_iter()
                .map(|checkpoint| HistoryEntry {
                    checkpoint_id: checkpoint.checkpoint_id,
                    parent_id: checkpoint.parent_id,
                    sequence: checkpoint.sequence,
                    kind: checkpoint.kind,
                    component: checkpoint.component,
                    completed_components: checkpoint.completed_components.len(),
                    file_count: checkpoint.file_count,
                    total_bytes: checkpoint.total_bytes,
                })
                .collect(),
        })
    }
}
