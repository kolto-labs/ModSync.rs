//! Immutable merge review shared by desktop and command-line adapters.
use super::{Options, components_reporting};
use crate::Guide;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub existing_identity: String,
    pub incoming_identity: String,
    pub options: Options,
    pub merged: Guide,
    pub warnings: Vec<String>,
    pub validation_error: Option<String>,
}

pub fn preview(existing: &Guide, incoming: &Guide, options: &Options) -> Result<Preview> {
    ensure!(
        options.heuristics.name_similarity.is_finite() && options.heuristics.name_similarity >= 0.0,
        "Merge similarity threshold must be finite and nonnegative"
    );
    let (components, warnings) =
        components_reporting(&existing.components, &incoming.components, options);
    let mut merged = incoming.clone();
    merged.components = components;
    // The new guide owns document-level values; preserve old fields for which
    // the new document has no value, including unrecognized extensions.
    for (key, value) in &existing.metadata {
        if !merged
            .metadata
            .keys()
            .any(|known| known.eq_ignore_ascii_case(key))
        {
            merged.metadata.insert(key.clone(), value.clone());
        }
    }
    let validation_error = crate::editor::validate_guide(&merged)
        .err()
        .map(|error| format!("{error:#}"));
    Ok(Preview {
        existing_identity: crate::updates::guide_identity(existing)?,
        incoming_identity: crate::updates::guide_identity(incoming)?,
        options: options.clone(),
        merged,
        warnings,
        validation_error,
    })
}

/// Recompute from both current sources; never trust serialized candidate bytes.
/// Loss warnings are a review stop, so applying cannot silently drop a conflict.
pub fn apply_preview(existing: &Guide, incoming: &Guide, candidate: &Preview) -> Result<Guide> {
    ensure!(
        crate::updates::guide_identity(existing)? == candidate.existing_identity,
        "Existing guide changed since merge preview"
    );
    ensure!(
        crate::updates::guide_identity(incoming)? == candidate.incoming_identity,
        "Incoming guide changed since merge preview"
    );
    let fresh = preview(existing, incoming, &candidate.options)?;
    ensure!(
        crate::updates::guide_identity(&fresh.merged)?
            == crate::updates::guide_identity(&candidate.merged)?,
        "Merge preview content differs from its source and options"
    );
    ensure!(
        fresh.warnings.is_empty(),
        "Merge has unresolved content-loss warnings: {}",
        fresh.warnings.join("; ")
    );
    crate::editor::validate_guide(&fresh.merged)?;
    Ok(fresh.merged)
}
