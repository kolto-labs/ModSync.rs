//! Closed translation of the two original guide duplicate-cleanup directions.
//! Source scripts remain provenance; no downloaded script is executed.
use crate::{Component, Guide, Instruction, draft};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};

pub(crate) const GENERATOR: &str = "reviewed-guide-duplicate-cleanup-v1";
const BINDING: &str = "GuideDuplicateCleanupBinding";
const POLICY: &str = "delete-tpc-with-same-directory-casefold-stem-tga-or-dds-v1";
const K1: &str = "bd712c74-49d6-8d80-88b5-8a3aab28e916";
const K2: &str = "50750eba-472b-862d-bb38-08d573c2c481";
static K1_SOURCE: OnceLock<std::result::Result<Component, String>> = OnceLock::new();
static K2_SOURCE: OnceLock<std::result::Result<Component, String>> = OnceLock::new();

fn source(game: usize) -> Result<&'static Component> {
    let (cache, bytes, expected, index, count) = if game == 1 {
        (
            &K1_SOURCE,
            include_bytes!("source_plan_k1.md").as_slice(),
            "44067788c6cb7a83b38d468817f273ce5cc901cd5e000c0f9ff65f8af15fc6ed",
            180,
            197,
        )
    } else {
        (
            &K2_SOURCE,
            include_bytes!("source_plan_k2.md").as_slice(),
            "f442bdf574031623ed577609730d887069b9bc8e38e8e2f38ecc43a7e7487d5f",
            142,
            151,
        )
    };
    cache
        .get_or_init(|| {
            (|| -> Result<Component> {
                ensure!(
                    format!("{:x}", Sha256::digest(bytes)) == expected,
                    "Duplicate cleanup compiled guide bytes differ"
                );
                let mut guide = Guide::parse(std::str::from_utf8(bytes)?, "md")?;
                draft::generate(&mut guide);
                ensure!(
                    guide.components.len() == count,
                    "Duplicate cleanup source inventory differs"
                );
                let canonical =
                    Guide::parse_preserving_selection(&serde_json::to_string(&guide)?, "json")?;
                let owner = canonical.components[index].clone();
                ensure!(
                    owner.name == "Remove Duplicate TGA/TPC"
                        && owner.options.is_empty()
                        && owner.is_selected
                        && owner.directions.contains("TPC should be deleted")
                        && owner
                            .directions
                            .contains("matching TGA file and a matching DDS file"),
                    "Duplicate cleanup original directions or options differ"
                );
                ensure!(
                    owner.guid == if game == 1 { K1 } else { K2 },
                    "Duplicate cleanup source identity differs"
                );
                Ok(owner)
            })()
            .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!(e.clone()))
}

fn selected_source(guid: &str, selected: bool) -> Result<Option<Component>> {
    let game = match guid {
        K1 => 1,
        K2 => 2,
        _ => return Ok(None),
    };
    let mut original = source(game)?.clone();
    original.is_selected = selected;
    Ok(Some(original))
}

pub(crate) fn contains(component: &Component) -> bool {
    component.metadata.contains_key(BINDING)
        || component
            .metadata
            .get("DraftReview")
            .and_then(|v| v.get("Generator"))
            .and_then(Value::as_str)
            == Some(GENERATOR)
        || component.options.iter().any(contains)
}

fn pending(original: Component) -> Result<Component> {
    let mut result = original.clone();
    let mut provenance = original;
    provenance.is_selected = true;
    result.instructions = vec![Instruction {
        action: "DelDuplicate".into(),
        // Empty Source selects the engine's exact DDS/TPC/TGA default set.
        // Nonempty Sources are serialized file paths by the guide loader.
        source: Vec::new(),
        destination: "<<kotorDirectory>>/Override".into(),
        arguments: ".tpc".into(),
        guid: String::new(),
        parent: String::new(),
        overwrite: true,
        dependencies: Vec::new(),
        restrictions: Vec::new(),
    }];
    result.metadata.insert(
        BINDING.into(),
        json!({"Schema":1,"Policy":POLICY,
        "OriginalComponent":provenance,"SourceKind":"compiled-original-guide-directions",
        "ScriptSourceRepository":"kolto-labs/mod-builds","ScriptSourceRevision":"67dc5fd8da6abde324c51d32eee3d04687070dd6",
        "OriginalScriptSha256":if result.guid == K1 {"45f64de636afb66da699d706a0885f070f80d4690d87f3198192f3f4508027b5"} else {"2ad21e47a0b193ca5b8b0dde8f5ddc23799d6459be7ab2aae644e78abff8fc52"},
        "Semantics":"guide-authorized deletion; recursive same-directory matches; Windows case-insensitive resource names; recovery through component checkpoint",
        "Execution":"in-process-VFS-and-checkpoint-delete-only-no-external-script"}),
    );
    result.metadata.remove("InstallationWarning");
    result.metadata.insert("DraftReview".into(), json!({"RequiresReview":true,"Generator":GENERATOR,"Derivations":[],
        "Result":{"ComponentGuid":result.guid,"DraftInstructionCount":1,"UnparsedGaps":[],"ConditionalDrafts":[],"RejectedDrafts":0}}));
    Ok(result)
}

/// Called only as part of explicit archive-aware conversion. Does not approve.
pub fn bind_guide(guide: &mut Guide, cancelled: &AtomicBool) -> Result<()> {
    let mut candidate = guide.clone();
    for owner in &mut candidate.components {
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "Duplicate cleanup binding cancelled"
        );
        ensure!(
            !owner.options.iter().any(contains),
            "Nested duplicate cleanup authority forbidden"
        );
        let Some(original) = selected_source(&owner.guid, owner.is_selected)? else {
            ensure!(
                !contains(owner),
                "Duplicate cleanup authority has no compiled source owner"
            );
            continue;
        };
        if contains(owner) {
            let expected = replay(owner, cancelled)?;
            if draft::requires_review(owner) {
                ensure!(
                    serde_json::to_value(&*owner)? == serde_json::to_value(expected)?,
                    "Duplicate cleanup pending model differs"
                );
            } else {
                verify_runtime(owner, cancelled)?;
            }
            continue;
        }
        let normalized = Guide::parse_preserving_selection(&serde_json::to_string(owner)?, "json")?;
        ensure!(
            normalized.components.len() == 1
                && serde_json::to_value(&normalized.components[0])?
                    == serde_json::to_value(&original)?,
            "Duplicate cleanup source definition or original choices differ"
        );
        *owner = pending(original)?;
    }
    *guide = candidate;
    Ok(())
}

pub(crate) fn replay(owner: &Component, cancelled: &AtomicBool) -> Result<Component> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Duplicate cleanup replay cancelled"
    );
    let original = selected_source(&owner.guid, owner.is_selected)?
        .context("Unknown duplicate cleanup source owner")?;
    let expected = pending(original)?;
    let mut actual = owner.clone();
    // Only the standard dedicated pending-to-approved transition may differ.
    let record = actual
        .metadata
        .get_mut("DraftReview")
        .and_then(Value::as_object_mut)
        .context("Missing duplicate cleanup review")?;
    if record.get("RequiresReview") == Some(&json!(false)) {
        ensure!(
            record.len() == 6
                && record.contains_key("Verification")
                && record.contains_key("ReviewedIdentity"),
            "Duplicate cleanup approval shape differs"
        );
        record.remove("Verification");
        record.remove("ReviewedIdentity");
        record.insert("RequiresReview".into(), true.into());
    }
    ensure!(
        serde_json::to_value(actual)? == serde_json::to_value(&expected)?,
        "Complete duplicate cleanup model differs from compiled source replay"
    );
    Ok(expected)
}

pub(crate) fn verification(pending: &Component) -> Result<Value> {
    let mut value = serde_json::to_value(pending)?;
    value
        .as_object_mut()
        .context("Invalid duplicate cleanup model")?
        .remove("IsSelected");
    let digest = super::reviewed_manual::value_digest(&value)?;
    Ok(
        json!({"Reviewer":draft::review::VERIFIER_NAME,"Kind":draft::review::VERIFICATION_KIND,
        "ToolVersion":concat!("ModSync.rs/",env!("CARGO_PKG_VERSION")),"ArchiveSha256":digest,
        "ArchiveFilename":"compiled-guide-duplicate-cleanup-source-v1","ReplayDigest":digest,"IdentityVersion":2}),
    )
}

pub(crate) fn verify_runtime(owner: &Component, cancelled: &AtomicBool) -> Result<()> {
    if !contains(owner) {
        return Ok(());
    }
    let expected = replay(owner, cancelled)?;
    let record = &owner.metadata["DraftReview"];
    ensure!(
        record["RequiresReview"] == false
            && record.get("Verification") == Some(&verification(&expected)?)
            && record["ReviewedIdentity"].as_str()
                == Some(draft::review::identity(owner)?.as_str()),
        "Duplicate cleanup requires unchanged dedicated independent source replay approval"
    );
    Ok(())
}
