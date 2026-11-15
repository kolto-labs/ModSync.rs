//! One compiled source-master alias layered over unchanged legacy archive recipes.
//! Selection/order admits a dependency definition, never installed success. The
//! engine must require owned committed predecessor checkpoints before effects.
use crate::{Component, Guide, archive, draft};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::atomic::AtomicBool};

pub(crate) const BINDING: &str = "ReviewedMasterBinding";
pub(crate) const GENERATOR: &str = "reviewed-source-master-v1";
const DATA: &str = include_str!("reviewed_master_k1.json");
const DATA_SHA: &str = "766236adcae5c0b720bacd8701274286d19d2a3753dbecbd4b2daa6faedf0ac9";
const OWNER: &str = "f966fc1f-6b47-88fc-9050-07b1371eb56a";
const PRIOR: &str = "6fb1971d-5775-85e5-9e77-ecc68a896e84";
const BOOK_SHA: &str = "b04b7331b5001f81698feb5571460ea9e2e486d032956abb22967d704254c338";

fn authority() -> Result<Value> {
    ensure!(
        format!("{:x}", Sha256::digest(DATA.as_bytes())) == DATA_SHA,
        "Compiled master authority changed"
    );
    let value: Value = serde_json::from_str(DATA)?;
    ensure!(
        value["schema"] == 1
            && value["owner_guid"] == OWNER
            && value["predecessor_guid"] == PRIOR
            && value["legacy_book_sha256"] == BOOK_SHA
            && value["owner_source_index"] == 46
            && value["predecessor_source_index"] == 5
            && value["original_unresolved_masters"] == json!(["K1 Community Patch"]),
        "Compiled master authority scope differs"
    );
    Ok(value)
}
fn own(component: &Component) -> bool {
    component.metadata.contains_key(BINDING)
        || component
            .metadata
            .get("DraftReview")
            .and_then(|v| v.get("Generator"))
            .and_then(Value::as_str)
            == Some(GENERATOR)
}
pub(crate) fn contains_resolved(component: &Component) -> bool {
    fn visit(component: &Component, depth: usize) -> bool {
        depth > 64 || own(component) || component.options.iter().any(|c| visit(c, depth + 1))
    }
    visit(component, 0)
}
fn binding(authority: &Value) -> Value {
    json!({"Schema":1,"AuthoritySha256":DATA_SHA,"LegacyBookSha256":BOOK_SHA,
        "Authority":authority,"OriginalUnresolvedMasters":authority["original_unresolved_masters"],
        "OriginalDependencies":authority["original_dependencies"],"RequiredPredecessors":[PRIOR],
        "InstallationRequirement":"owned-committed-predecessor-checkpoint-before-effects-v1"})
}
fn check_claim(component: &Component) -> Result<()> {
    let source = authority()?;
    ensure!(
        component.guid == OWNER
            && component.options.is_empty()
            && component.metadata.get(BINDING) == Some(&binding(&source))
            && component.dependencies == [PRIOR]
            && component.metadata.get("UnresolvedMasters") == Some(&json!([])),
        "Master resolution identity, relation or preserved source authority differs"
    );
    Ok(())
}
/// The returned identities are prerequisites, not proof that they ran. Actual
/// execution must check the engine's owned committed checkpoint set.
pub(crate) fn required_predecessors(component: &Component) -> Result<Vec<String>> {
    if !own(component) {
        ensure!(
            !component.options.iter().any(contains_resolved),
            "Nested master resolution forbidden"
        );
        return Ok(Vec::new());
    }
    check_claim(component)?;
    Ok(vec![PRIOR.into()])
}
fn predecessor_context(component: &Component, expected: &Value) -> Result<()> {
    let mut actual = serde_json::to_value(component)?;
    let mut expected = expected.clone();
    let original_review = expected.get("DraftReview").cloned();
    // A source-identical predecessor may itself pass normal explicit review.
    // No instruction, resource, option, source prose or other metadata may drift.
    if actual.get("DraftReview") != original_review.as_ref() {
        ensure!(
            !draft::requires_review(component),
            "Predecessor review transition is not valid"
        );
    }
    for value in [&mut actual, &mut expected] {
        let object = value
            .as_object_mut()
            .context("Invalid predecessor context")?;
        object.remove("IsSelected");
        object.remove("DraftReview");
    }
    ensure!(
        actual == expected,
        "Master prerequisite differs from complete original source context"
    );
    Ok(())
}
pub(crate) fn validate_guide(guide: &Guide) -> Result<()> {
    ensure!(
        !guide
            .components
            .iter()
            .any(|c| c.options.iter().any(contains_resolved)),
        "Master resolution cannot occur in options"
    );
    for owner in guide.components.iter().filter(|c| own(c)) {
        check_claim(owner)?;
        if !owner.is_selected {
            continue;
        }
        let source = authority()?;
        let prior: Vec<_> = guide
            .components
            .iter()
            .enumerate()
            .filter(|(_, c)| c.guid == PRIOR)
            .collect();
        let owners: Vec<_> = guide
            .components
            .iter()
            .enumerate()
            .filter(|(_, c)| c.guid == OWNER)
            .collect();
        ensure!(
            prior.len() == 1 && owners.len() == 1 && prior[0].1.is_selected,
            "Master resolution requires one selected original Community Patch predecessor"
        );
        predecessor_context(prior[0].1, &source["predecessor_original"])?;
        ensure!(
            prior[0].0 < owners[0].0,
            "Master prerequisite must precede its owner"
        );
        if let Some(order) = guide.metadata.get("ModSyncSourceOrder") {
            let order = order
                .as_array()
                .context("Invalid explicit master source order")?;
            let before: Vec<_> = order
                .iter()
                .enumerate()
                .filter(|(_, v)| v.as_str() == Some(PRIOR))
                .collect();
            let after: Vec<_> = order
                .iter()
                .enumerate()
                .filter(|(_, v)| v.as_str() == Some(OWNER))
                .collect();
            ensure!(
                before.len() == 1 && after.len() == 1 && before[0].0 < after[0].0,
                "Explicit source order violates master prerequisite"
            );
        }
    }
    Ok(())
}
fn overlay(mut component: Component) -> Result<Component> {
    let source = authority()?;
    ensure!(
        component.guid == OWNER
            && component.options.is_empty()
            && component.metadata.get("UnresolvedMasters")
                == Some(&source["original_unresolved_masters"])
            && serde_json::to_value(&component.dependencies)? == source["original_dependencies"]
            && component.metadata["AuthoredRecipeBinding"]["DatabookSha256"] == BOOK_SHA
            && component.metadata["AuthoredRecipeBinding"]["InputComponent"]
                == source["owner_original"],
        "Master overlay does not match exact original legacy owner"
    );
    component.dependencies.push(PRIOR.into());
    component
        .metadata
        .insert("UnresolvedMasters".into(), json!([]));
    component.metadata.insert(BINDING.into(), binding(&source));
    component
        .metadata
        .get_mut("DraftReview")
        .and_then(Value::as_object_mut)
        .context("Missing legacy pending review")?
        .insert("Generator".into(), GENERATOR.into());
    check_claim(&component)?;
    Ok(component)
}
/// Called after legacy archive binding; publication is atomic across the guide.
pub(crate) fn bind_in_guide(guide: &mut Guide) -> Result<()> {
    let mut candidate = guide.clone();
    for component in &mut candidate.components {
        if component.guid == OWNER
            && !own(component)
            && component.metadata.contains_key("AuthoredRecipeBinding")
        {
            *component = overlay(component.clone())?;
        }
    }
    validate_guide(&candidate)?;
    *guide = candidate;
    Ok(())
}
pub(crate) fn replay(
    component: &Component,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Component> {
    check_claim(component)?;
    let mut expected = overlay(super::reviewed_manual::regenerate_owner(
        OWNER, root, options, cancelled,
    )?)?;
    expected.is_selected = component.is_selected;
    let mut actual = serde_json::to_value(component)?;
    let mut value = serde_json::to_value(&expected)?;
    for item in [&mut actual, &mut value] {
        item.as_object_mut()
            .context("Invalid master replay model")?
            .remove("DraftReview");
    }
    ensure!(
        actual == value,
        "Complete master owner differs from independent archive/authority replay"
    );
    let record = &component.metadata["DraftReview"];
    for key in ["Generator", "Derivations", "Result"] {
        ensure!(
            record.get(key) == expected.metadata["DraftReview"].get(key),
            "Master pending diagnostics changed"
        );
    }
    Ok(expected)
}
pub(crate) fn verification(pending: &Component) -> Result<Value> {
    check_claim(pending)?;
    let mut value = serde_json::to_value(pending)?;
    value
        .as_object_mut()
        .context("Invalid master verification model")?
        .remove("IsSelected");
    Ok(
        json!({"Reviewer":draft::review::VERIFIER_NAME,"Kind":draft::review::VERIFICATION_KIND,
        "ToolVersion":concat!("ModSync.rs/",env!("CARGO_PKG_VERSION")),"ArchiveSha256":DATA_SHA,
        "ArchiveFilename":"reviewed-source-master-k1-v1","ReplayDigest":super::reviewed_manual::value_digest(&value)?,"IdentityVersion":2}),
    )
}
pub(crate) fn verify_runtime(
    component: &Component,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<()> {
    let pending = replay(component, root, options, cancelled)?;
    let record = &component.metadata["DraftReview"];
    const KEYS: [&str; 6] = [
        "RequiresReview",
        "Generator",
        "Derivations",
        "Result",
        "Verification",
        "ReviewedIdentity",
    ];
    ensure!(
        record
            .as_object()
            .is_some_and(|o| o.len() == KEYS.len() && o.keys().all(|k| KEYS.contains(&k.as_str())))
            && record["RequiresReview"] == false
            && record.get("Verification") == Some(&verification(&pending)?)
            && record["ReviewedIdentity"].as_str()
                == Some(draft::review::identity(component)?.as_str()),
        "Master runtime requires unchanged dedicated independent replay approval"
    );
    Ok(())
}
