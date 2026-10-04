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

pub(crate) fn widescreen(component: &crate::Component) -> bool {
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
    prepare_impl(selected, phase, context, choices, None)
}

/// Read-only preparation with filesystem-resolved external-choice ownership.
pub fn prepare_with_roots(
    selected: &Guide,
    phase: Phase,
    context: CompletionContext<'_>,
    choices: Option<&ChoicesFile>,
    roots: &crate::paths::Roots,
    case_insensitive: bool,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<PreparedPhase> {
    prepare_impl(
        selected,
        phase,
        context,
        choices,
        Some((roots, case_insensitive, cancelled)),
    )
}

fn prepare_impl(
    selected: &Guide,
    phase: Phase,
    context: CompletionContext<'_>,
    choices: Option<&ChoicesFile>,
    resolved: Option<(&crate::paths::Roots, bool, &std::sync::atomic::AtomicBool)>,
) -> Result<PreparedPhase> {
    let mut guide = selected.clone();
    // The phase split needs the base build first in the install order.
    guide.ordering.widescreen_last = phase != Phase::All;
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
        // Refuse before any write when a selected base component must install after a
        // selected widescreen-only one: Remaining could never continue that order.
        if let Ok(full) = guide.ordered()
            && let Some(first) = full.iter().position(|c| widescreen(c))
            && full[first..].iter().any(|c| !widescreen(c))
        {
            bail!(
                "The base build cannot be installed first: a base component must install after widescreen-only {} or a later widescreen-only component. Install with --phase all.",
                full[first].name
            );
        }
        for component in &mut guide.components {
            if widescreen(component) {
                component.is_selected = false;
            }
        }
        guide.ordered()?;
    }
    // All preserves ordinary engine validation, including pipeline Report errors.
    // Only phased operations need eager selection/prefix checks here.
    if phase == Phase::Remaining {
        let ordered = guide.ordered()?;
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
        if let Some((roots, case_insensitive, cancelled)) = resolved {
            let immutable = match context {
                CompletionContext::Retained { .. } => completed.as_slice(),
                _ => &[],
            };
            choices.apply_to_remaining_with_roots(
                &mut guide,
                immutable,
                roots,
                case_insensitive,
                cancelled,
            )?;
        } else {
            match context {
                CompletionContext::Retained { .. } => {
                    choices.apply_to_remaining(&mut guide, &completed)?
                }
                CompletionContext::None | CompletionContext::Reconstruct => {
                    choices.apply_to_guide(&mut guide)?
                }
            }
        }
    }
    Ok(PreparedPhase { guide, completed })
}
