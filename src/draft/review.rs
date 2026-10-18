//! Explicit resolution of generated instructions without discarding their provenance.
use super::DraftResult;
use crate::{Component, archive};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::atomic::AtomicBool};

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
///
/// An archive-replay `Verification` record is folded in (in traversal order)
/// only when one exists, so receipts without it keep their earlier identity
/// while any edit to a verification record re-fences its component.
pub fn identity(component: &Component) -> Result<String> {
    fn strip_receipts(value: &mut Value, depth: usize, folds: &mut Vec<Vec<u8>>) -> Result<()> {
        ensure!(depth <= 64, "Draft option nesting exceeds 64 levels");
        let object = value
            .as_object_mut()
            .context("Component must be an object")?;
        if let Some(verification) = object
            .get("DraftReview")
            .and_then(|review| review.get("Verification"))
        {
            folds.push(verification_digest(verification));
        }
        object.remove("DraftReview");
        if let Some(options) = object.get_mut("Options").and_then(Value::as_array_mut) {
            for option in options {
                strip_receipts(option, depth + 1, folds)?;
            }
        }
        Ok(())
    }
    let mut value = serde_json::to_value(component)?;
    let mut folds = Vec::new();
    strip_receipts(&mut value, 0, &mut folds)?;
    // Use the same field normalization as saved guide ingestion so harmless
    // canonicalization (tier aliases, source paths, resource defaults) survives.
    let canonical = crate::Guide::from_value(value)?;
    let mut value = serde_json::to_value(&canonical.components[0])?;
    value
        .as_object_mut()
        .context("Component must be an object")?
        .remove("IsSelected");
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(&value)?);
    for fold in folds {
        digest.update(fold);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn verification_digest(value: &Value) -> Vec<u8> {
    match serde_json::from_value::<Verification>(value.clone()) {
        Ok(record) => Sha256::digest(serde_json::to_vec(&record).unwrap_or_default()).to_vec(),
        Err(_) => Sha256::digest(b"invalid-verification").to_vec(),
    }
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
            if let Some(verification) = review.get("Verification")
                && !serde_json::from_value::<Verification>(verification.clone())
                    .is_ok_and(|record| record.kind == VERIFICATION_KIND)
            {
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

/// Hash-bound record of the single local archive a resolved-filename draft was
/// inferred from. It is evidence for a replay, never an approval by itself.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct Derivation {
    pub filename: String,
    pub sha256: String,
    pub bytes: u64,
}

/// Audit record left by a successful `verify_archive_derived`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct Verification {
    pub reviewer: String,
    pub kind: String,
    pub tool_version: String,
    pub archive_sha256: String,
    pub archive_filename: String,
    pub replay_digest: String,
}

pub const VERIFIER_NAME: &str = "modsync-ci-tool";
pub const VERIFICATION_KIND: &str = "ArchiveDerived";
const GENERATOR: &str = "resolved-filenames";

/// Instructions and options, normalized the way saved guides are ingested and
/// with generated identities replaced by option positions. Selection state of
/// the outer component is not part of the shape.
fn shape(component: &Component) -> Result<Value> {
    fn strip(value: &mut Value, depth: usize, review: bool) -> Result<()> {
        ensure!(depth <= 64, "Draft option nesting exceeds 64 levels");
        let object = value
            .as_object_mut()
            .context("Component must be an object")?;
        if review {
            object.remove("DraftReview");
        } else {
            object.remove("Guid");
        }
        if let Some(options) = object.get_mut("Options").and_then(Value::as_array_mut) {
            for option in options {
                strip(option, depth + 1, review)?;
            }
        }
        Ok(())
    }
    let skeleton = Component {
        guid: component.guid.clone(),
        name: component.name.clone(),
        instructions: component.instructions.clone(),
        options: component.options.clone(),
        ..Default::default()
    };
    let mut value = serde_json::to_value(&skeleton)?;
    strip(&mut value, 0, true)?;
    let canonical = crate::Guide::from_value(value)?;
    let component = &canonical.components[0];
    let mut instructions = Vec::new();
    for step in &component.instructions {
        let mut value = serde_json::to_value(step)?;
        let object = value
            .as_object_mut()
            .context("Instruction must be an object")?;
        object.remove("Guid");
        object.remove("Parent");
        if step.action == "Choose" {
            let chosen: Vec<Value> = step
                .source
                .iter()
                .map(|id| {
                    component
                        .options
                        .iter()
                        .position(|option| crate::guid::names_option(&option.guid, id))
                        .map_or_else(|| json!({"Unresolved": id}), |index| json!(index))
                })
                .collect();
            object.insert("Source".into(), Value::Array(chosen));
        }
        instructions.push(value);
    }
    let mut options = serde_json::to_value(&component.options)?;
    for option in options.as_array_mut().into_iter().flatten() {
        strip(option, 0, false)?;
    }
    Ok(json!({"Instructions": instructions, "Options": options}))
}

fn pending_masters(component: &Component, depth: usize) -> bool {
    depth > 64
        || component
            .metadata
            .get("UnresolvedMasters")
            .is_some_and(|value| !value.as_array().is_some_and(Vec::is_empty))
        || component
            .options
            .iter()
            .any(|option| pending_masters(option, depth + 1))
}

fn clear_subtree(component: &mut Component, verification: &Value, depth: usize) -> Result<()> {
    ensure!(depth <= 64, "Draft option nesting exceeds 64 levels");
    for option in &mut component.options {
        clear_subtree(option, verification, depth + 1)?;
    }
    if let Some(record) = component.metadata.get_mut("DraftReview") {
        let record = record
            .as_object_mut()
            .context("Invalid draft review object")?;
        record.insert("Verification".into(), verification.clone());
        record.insert("RequiresReview".into(), false.into());
        let reviewed = identity(component)?;
        component
            .metadata
            .get_mut("DraftReview")
            .and_then(Value::as_object_mut)
            .context("Invalid draft review object")?
            .insert("ReviewedIdentity".into(), reviewed.into());
    }
    Ok(())
}

/// Clear review only for a draft that a fresh replay reproduces from one
/// hash-bound local archive. Anything authored, edited, gapful, prose-drafted,
/// method-bound or not reproducible stays fenced and `component` is unchanged.
/// This is not a blanket acceptance: the proof is the archive replay.
pub fn verify_archive_derived(
    component: &mut Component,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<()> {
    let record = component
        .metadata
        .get("DraftReview")
        .context("No generated draft provenance")?;
    ensure!(
        record.get("RequiresReview").and_then(Value::as_bool) == Some(true),
        "Draft is not awaiting review"
    );
    ensure!(
        record.get("Generator").and_then(Value::as_str) == Some(GENERATOR),
        "Draft was not generated from resolved archive filenames"
    );
    ensure!(
        record.get("MethodContext").is_none(),
        "Method-bound drafts are not verifiable by archive replay"
    );
    ensure!(
        !component
            .metadata
            .get("InstallationWarning")
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains(super::REVIEW_FLAG)),
        "Prose-drafted instructions are not verifiable by archive replay"
    );
    let result: DraftResult = serde_json::from_value(
        record
            .get("Result")
            .cloned()
            .context("Missing draft diagnostics")?,
    )?;
    ensure!(
        result.component_guid.eq_ignore_ascii_case(&component.guid),
        "Draft diagnostics belong to a different component"
    );
    ensure!(
        result.unparsed_gaps.is_empty()
            && result.conditional_drafts.is_empty()
            && result.rejected_drafts == 0,
        "Draft has unresolved gaps, conditions, or rejected instructions"
    );
    ensure!(
        result.draft_instruction_count == component.instructions.len(),
        "Draft instruction count does not match the component"
    );
    fn name_keyed(component: &Component, depth: usize) -> bool {
        depth > 64
            || component
                .instructions
                .iter()
                .any(|step| matches!(step.action.as_str(), "DelDuplicate" | "Execute"))
            || component
                .options
                .iter()
                .any(|option| name_keyed(option, depth + 1))
    }
    ensure!(
        !name_keyed(component, 0),
        "Duplicate cleanup and external installer steps are not derived from archive contents"
    );
    ensure!(
        !pending_masters(component, 0),
        "Component has unresolved Markdown masters"
    );
    let derivation: Derivation = serde_json::from_value(
        record
            .get("Derivation")
            .cloned()
            .context("Draft records no archive derivation")?,
    )
    .context("Invalid archive derivation")?;
    let replayed = crate::archive_generation::resolved::replay_archive_derivation(
        component,
        &derivation,
        root,
        options,
        cancelled,
    )?;
    let expected = shape(&replayed)?;
    ensure!(
        shape(component)? == expected,
        "Saved instructions or options differ from the archive replay"
    );
    let verification = Verification {
        reviewer: VERIFIER_NAME.into(),
        kind: VERIFICATION_KIND.into(),
        tool_version: concat!("ModSync.rs/", env!("CARGO_PKG_VERSION")).into(),
        archive_sha256: derivation.sha256.to_ascii_lowercase(),
        archive_filename: derivation.filename.clone(),
        replay_digest: format!("{:x}", Sha256::digest(serde_json::to_vec(&expected)?)),
    };
    let mut cleared = component.clone();
    clear_subtree(&mut cleared, &serde_json::to_value(verification)?, 0)?;
    *component = cleared;
    Ok(())
}

/// One draft the bulk verifier left fenced, with the reason.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub guid: String,
    pub name: String,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedReport {
    pub cleared: Vec<String>,
    pub skipped: Vec<Skipped>,
}

/// Run `verify_archive_derived` over every pending draft, components first and
/// then their options. A cleared component clears its whole option subtree, so
/// its options are not visited again. Refusals are reported, not raised; only
/// an unusable root or cancellation is an error.
pub fn review_verified_drafts(
    components: &mut [Component],
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<VerifiedReport> {
    use std::sync::atomic::Ordering;
    fn walk(
        components: &mut [Component],
        root: &Path,
        options: &archive::Options,
        cancelled: &AtomicBool,
        report: &mut VerifiedReport,
        depth: usize,
    ) -> Result<()> {
        ensure!(depth <= 64, "Draft option nesting exceeds 64 levels");
        for component in components {
            ensure!(
                !cancelled.load(Ordering::Acquire),
                "Draft verification cancelled"
            );
            if super::requires_review(component) {
                match verify_archive_derived(component, root, options, cancelled) {
                    Ok(()) => {
                        report.cleared.push(component.guid.clone());
                        continue;
                    }
                    Err(error) => {
                        ensure!(
                            !cancelled.load(Ordering::Acquire),
                            "Draft verification cancelled"
                        );
                        report.skipped.push(Skipped {
                            guid: component.guid.clone(),
                            name: component.name.clone(),
                            reason: format!("{error:#}"),
                        });
                    }
                }
            }
            walk(
                &mut component.options,
                root,
                options,
                cancelled,
                report,
                depth + 1,
            )?;
        }
        Ok(())
    }
    let root = crate::archive_generation::resolved::local_root(root)?;
    let mut report = VerifiedReport::default();
    walk(components, &root, options, cancelled, &mut report, 0)?;
    Ok(report)
}
