//! Explicit resolution of generated instructions without discarding their provenance.
use super::DraftResult;
use crate::Component;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// One explanation per original diagnostic, in its original order. Conditions
/// require an explicit decision in the edited instructions/options; the notes
/// describe that decision, they do not execute or infer one.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resolution {
    pub expected_identity: String,
    pub gap_decisions: Vec<String>,
    pub condition_decisions: Vec<String>,
    pub rejected_decisions: Vec<String>,
    pub review_note: String,
}

/// Selection of the outer component is not an authoring change. Nested option
/// selections are included because they can embody a conditional decision.
pub fn identity(component: &Component) -> Result<String> {
    fn strip_receipts(value: &mut Value, depth: usize) -> Result<()> {
        ensure!(depth <= 64, "Draft option nesting exceeds 64 levels");
        let object = value
            .as_object_mut()
            .context("Component must be an object")?;
        object.remove("DraftReview");
        if let Some(options) = object.get_mut("Options").and_then(Value::as_array_mut) {
            for option in options {
                strip_receipts(option, depth + 1)?;
            }
        }
        Ok(())
    }
    let mut value = serde_json::to_value(component)?;
    strip_receipts(&mut value, 0)?;
    // Use the same field normalization as saved guide ingestion so harmless
    // canonicalization (tier aliases, source paths, resource defaults) survives.
    let canonical = crate::Guide::from_value(value)?;
    let mut value = serde_json::to_value(&canonical.components[0])?;
    value
        .as_object_mut()
        .context("Component must be an object")?
        .remove("IsSelected");
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(&value)?)))
}

pub fn original_diagnostics(component: &Component) -> Result<DraftResult> {
    let review = component
        .metadata
        .get("DraftReview")
        .context("No draft provenance")?;
    let result: DraftResult = serde_json::from_value(
        review
            .get("OriginalResult")
            .or_else(|| review.get("Result"))
            .cloned()
            .context("Missing draft diagnostics")?,
    )?;
    ensure!(
        result.component_guid.eq_ignore_ascii_case(&component.guid),
        "Draft provenance belongs to a different component"
    );
    Ok(result)
}

fn notes(notes: &[String], expected: usize, kind: &str) -> Result<()> {
    ensure!(
        notes.len() == expected,
        "Every original {kind} needs exactly one decision"
    );
    for note in notes {
        ensure!(
            !note.trim().is_empty() && note.len() <= 16 * 1024,
            "Each {kind} decision must contain 1 to 16384 bytes"
        );
    }
    Ok(())
}

/// This is an explicit authoring action, not automatic approval. The enclosing
/// editor transaction validates the entire resulting guide before publication.
pub fn resolve(component: &mut Component, resolution: &Resolution) -> Result<()> {
    ensure!(
        identity(component)? == resolution.expected_identity,
        "Draft changed since the resolution was prepared"
    );
    let original = original_diagnostics(component)?;
    ensure!(
        original
            .unparsed_gaps
            .len()
            .saturating_add(original.conditional_drafts.len())
            .saturating_add(original.rejected_drafts)
            <= 4096,
        "Draft diagnostic count exceeds the review bound"
    );
    notes(
        &resolution.gap_decisions,
        original.unparsed_gaps.len(),
        "gap",
    )?;
    notes(
        &resolution.condition_decisions,
        original.conditional_drafts.len(),
        "condition",
    )?;
    notes(
        &resolution.rejected_decisions,
        original.rejected_drafts,
        "rejected instruction",
    )?;
    ensure!(
        !resolution.review_note.trim().is_empty() && resolution.review_note.len() <= 16 * 1024,
        "A bounded final review note is required"
    );
    let review = component
        .metadata
        .get_mut("DraftReview")
        .context("No draft provenance")?;
    let object = review
        .as_object_mut()
        .context("Invalid draft review object")?;
    object.insert("OriginalResult".into(), serde_json::to_value(&original)?);
    object.insert(
        "Result".into(),
        json!({
            "ComponentGuid": component.guid,
            "DraftInstructionCount": component.instructions.len(),
            "UnparsedGaps": [], "ConditionalDrafts": [], "RejectedDrafts": 0
        }),
    );
    object.insert("Resolution".into(), serde_json::to_value(resolution)?);
    object.insert(
        "ReviewedIdentity".into(),
        resolution.expected_identity.clone().into(),
    );
    object.insert("RequiresReview".into(), false.into());
    Ok(())
}

pub(super) fn receipt_matches(component: &Component, review: &Value) -> bool {
    match review.get("ReviewedIdentity") {
        None => review.get("Resolution").is_none() && review.get("OriginalResult").is_none(),
        Some(Value::String(expected)) => {
            if !identity(component).is_ok_and(|actual| actual == *expected) {
                return false;
            }
            if let Some(value) = review.get("Resolution") {
                let Ok(resolution) = serde_json::from_value::<Resolution>(value.clone()) else {
                    return false;
                };
                let Ok(original) = original_diagnostics(component) else {
                    return false;
                };
                original
                    .unparsed_gaps
                    .len()
                    .saturating_add(original.conditional_drafts.len())
                    .saturating_add(original.rejected_drafts)
                    <= 4096
                    && resolution.expected_identity == *expected
                    && review.get("OriginalResult").is_some()
                    && notes(
                        &resolution.gap_decisions,
                        original.unparsed_gaps.len(),
                        "gap",
                    )
                    .is_ok()
                    && notes(
                        &resolution.condition_decisions,
                        original.conditional_drafts.len(),
                        "condition",
                    )
                    .is_ok()
                    && notes(
                        &resolution.rejected_decisions,
                        original.rejected_drafts,
                        "rejected instruction",
                    )
                    .is_ok()
                    && !resolution.review_note.trim().is_empty()
                    && resolution.review_note.len() <= 16 * 1024
            } else {
                review.get("OriginalResult").is_none()
            }
        }
        Some(_) => false,
    }
}

/// An editor replacement cannot erase generated provenance or carry an approval
/// over a changed definition. Call on every retained GUID after a normal edit.
pub(crate) fn invalidate_changed(before: &Component, after: &mut Component) -> Result<()> {
    if let Some(review) = before.metadata.get("DraftReview")
        && (identity(before)? != identity(after)?
            || after.metadata.get("DraftReview") != Some(review))
    {
        let mut retained = review.clone();
        retained
            .as_object_mut()
            .context("Invalid draft review object")?
            .insert("RequiresReview".into(), true.into());
        after.metadata.insert("DraftReview".into(), retained);
    }
    Ok(())
}
