//! Pure preparation after an adapter has made its own selection decisions.
//! Completion is only a candidate here: the engine verifies persisted evidence.
use crate::{Guide, fomod::ChoicesFile};
use anyhow::{Result, bail, ensure};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    All,
    Base,
    Remaining,
}

#[derive(Clone, Copy, Debug)]
pub enum CompletionContext<'a> {
    None,
    /// The guide already contains the exact effective successful definitions.
    Retained {
        ids: &'a [String],
    },
    /// Rebuild candidate definitions from current inputs, never from a receipt.
    Reconstruct,
}

#[derive(Debug)]
pub struct PreparedPhase {
    pub guide: Guide,
    pub completed: Vec<String>,
}

fn widescreen(component: &crate::Component) -> bool {
    component
        .metadata
        .get("WidescreenOnly")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

/// Prepare one explicitly requested phase without I/O or changing input selection.
/// External choices carry their entire payload in component metadata. Consequently
/// even suffix-only edits can invalidate reconstructed completed definitions.
pub fn prepare(
    selected: &Guide,
    phase: Phase,
    context: CompletionContext<'_>,
    choices: Option<&ChoicesFile>,
) -> Result<PreparedPhase> {
    let mut guide = selected.clone();
    let completed = match (phase, context) {
        (Phase::All | Phase::Base, CompletionContext::None) => Vec::new(),
        (Phase::Remaining, CompletionContext::Retained { ids }) => ids.to_vec(),
        (Phase::Remaining, CompletionContext::Reconstruct) => guide
            .ordered()?
            .iter()
            .filter(|c| !widescreen(c))
            .map(|c| c.guid.clone())
            .collect(),
        _ => bail!("Invalid phase/completion context"),
    };
    if phase == Phase::Base {
        for component in &mut guide.components {
            if widescreen(component) {
                component.is_selected = false;
            }
        }
    }
    let ordered = guide.ordered()?;
    if phase == Phase::Remaining {
        ensure!(
            !completed.is_empty(),
            "Remaining requires a nonempty completed prefix"
        );
        ensure!(
            completed.len() < ordered.len(),
            "No remaining components to install"
        );
        let ids: BTreeSet<_> = completed.iter().map(|id| id.to_ascii_lowercase()).collect();
        ensure!(
            ids.len() == completed.len(),
            "Duplicate completed component ID"
        );
        ensure!(
            completed
                .iter()
                .zip(&ordered)
                .all(|(id, c)| id.eq_ignore_ascii_case(&c.guid)),
            "Completed components must form the selected installation prefix in order"
        );
    }
    if let Some(choices) = choices {
        match context {
            CompletionContext::Retained { .. } => {
                choices.apply_to_remaining(&mut guide, &completed)?
            }
            CompletionContext::None | CompletionContext::Reconstruct => {
                choices.apply_to_guide(&mut guide)?
            }
        }
    }
    Ok(PreparedPhase { guide, completed })
}
