//! Closed revisions of an original guide owner's source package. An updated
//! publisher download is a distinct source choice, never an alias for an old SHA.
//! The parent snapshots and decodes that exact archive through its common factory.
use super::original_plan_protocol::{Effect, Program};
use super::{ProgramEntry, ProgramSlot, file_pin, literal, sha};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, sync::atomic::AtomicBool};

const DATA: &[u8] = include_bytes!("source_revisions.json");
pub(super) const DATA_SHA: &str =
    "d474974049562b004021982f0c4499de07761d0e7289e7cabe862cadc23ee64f";
const OWNER: &str = "14d77038-49c0-882b-83a1-0a424efa0f60";
const REVISION: &str = "trask-without-tutorials-1.0.2-holopatcher-2.0";

fn authority() -> Result<Value> {
    ensure!(sha(DATA) == DATA_SHA, "Compiled source revision changed");
    let data: Value = serde_json::from_slice(DATA)?;
    ensure!(
        data["schema"] == 1
            && data["policy"] == "closed-reviewed-source-revision-v1"
            && data["revision_id"] == REVISION
            && data["owner_guid"] == OWNER
            && data["game"] == "k1"
            && data["component_index"] == 124
            && data["original_guide_sha256"] == super::GUIDES[0].1
            && data["original_plan_sha256"] == super::GUIDES[0].3,
        "Source revision original context differs"
    );
    Ok(data)
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("Source revision {key} absent"))
}

fn revision_step(entry: &ProgramEntry, data: &Value) -> Value {
    json!({"SourceKind":"reviewed-publisher-source-revision", "SourceRevisionAuthoritySha256":DATA_SHA,
        "OriginalSourceProgram":entry.step, "SourceRevision":data})
}

/// This recognizes a declared original definition, never source bytes or a
/// completed install. Execution still requires select and fresh derive_program.
pub(super) fn source_definition_matches(entry: &ProgramEntry, submitted: &Value) -> Result<bool> {
    if entry.original.guid == super::source_revisions_enlightenment::OWNER {
        return super::source_revisions_enlightenment::source_definition_matches(entry, submitted);
    }
    if entry.original.guid == super::source_revisions_uniform::OWNER {
        return super::source_revisions_uniform::source_definition_matches(entry, submitted);
    }
    if submitted == &entry.step {
        return Ok(true);
    }
    if entry.original.guid != OWNER {
        return Ok(false);
    }
    let data = authority()?;
    Ok(entry.game == "k1"
        && entry.index == 124
        && entry.step == data["original_step"]
        && submitted == &revision_step(entry, &data))
}

/// Select by bounded observed body SHA, then snapshot again against that SHA in
/// derive_program. A race can only cause rejection, never substitute new bytes.
/// The old body still follows the original compiler and original GUI provenance.
pub(super) fn select(
    entry: &ProgramEntry,
    root: &Path,
    cancelled: &AtomicBool,
) -> Result<Option<ProgramEntry>> {
    if entry.original.guid == super::source_revisions_enlightenment::OWNER {
        return super::source_revisions_enlightenment::select(entry, root, cancelled);
    }
    if entry.original.guid == super::source_revisions_uniform::OWNER {
        return super::source_revisions_uniform::select(entry, root, cancelled);
    }
    if entry.original.guid != OWNER {
        return Ok(None);
    }
    let data = authority()?;
    let original = &data["original_archive"];
    let revision = &data["revision_archive"];
    let filename = text(original, "filename")?;
    ensure!(
        entry.game == "k1"
            && entry.index == 124
            && entry.step == data["original_step"]
            && entry.inputs
                == BTreeMap::from([(filename.into(), text(original, "sha256")?.into())])
            && entry
                .original
                .resource_registry
                .contains_key(text(&data["publisher_download"], "source_url")?),
        "Source revision does not replace its complete original guide owner"
    );
    ensure!(
        text(revision, "filename")? == filename,
        "Source revision transport filename differs from publisher filename"
    );
    let root = super::super::resolved::local_root(root)?;
    let pin = file_pin(
        &root.join(literal(filename)?),
        filename,
        super::MAX_ARCHIVE,
        cancelled,
    )?;
    if pin.sha256 == text(original, "sha256")? {
        ensure!(
            Some(pin.bytes) == original["bytes"].as_u64(),
            "Original source size differs"
        );
        return Ok(None);
    }
    ensure!(
        pin.sha256 == text(revision, "sha256")? && Some(pin.bytes) == revision["bytes"].as_u64(),
        "Source body is neither the original Trask package nor its reviewed 1.0.2 revision"
    );
    let step_id = "source-revision-k1-trask-1.0.2";
    let slot = format!("{{extract}}/{step_id}/1");
    let installer = &data["installer"];
    let installer_path = text(installer, "path")?;
    let cwd = Path::new(installer_path)
        .parent()
        .context("Source revision installer parent absent")?
        .to_str()
        .context("Source revision installer path is not UTF8")?;
    let program = Program {
        step_id: step_id.into(),
        source_heading: "Reviewed source revision: Trask Without Tutorials 1.0.2".into(),
        effects: vec![
            Effect::Extract {
                ordinal: 0,
                source: format!("{{archives}}/{filename}"),
                destination: slot.clone(),
                sha256: pin.sha256.clone(),
            },
            Effect::NativeRun {
                ordinal: 1,
                patcher: format!("{slot}/{installer_path}"),
                sha256: text(installer, "sha256")?.into(),
                cwd: format!("{slot}/{cwd}"),
                namespace: None,
            },
        ],
    };
    Ok(Some(ProgramEntry {
        game: entry.game.clone(),
        index: entry.index,
        raw: entry.raw.clone(),
        original: entry.original.clone(),
        step: revision_step(entry, &data),
        inputs: BTreeMap::from([(filename.into(), pin.sha256)]),
        program: Ok(program),
    }))
}

/// Additional exact selected-source evidence on the freshly decoded whole body.
/// No member book is trusted: the common factory reconstructs every member and
/// complete native configuration, including module merge and InstallList skips.
pub(super) fn validate_slot(
    entry: &ProgramEntry,
    slot: &ProgramSlot,
    cancelled: &AtomicBool,
) -> Result<()> {
    if entry.original.guid == super::source_revisions_enlightenment::OWNER {
        return super::source_revisions_enlightenment::validate_slot(entry, slot, cancelled);
    }
    if entry.original.guid == super::source_revisions_uniform::OWNER {
        return super::source_revisions_uniform::validate_slot(entry, slot, cancelled);
    }
    if entry.step.get("SourceRevision").is_none() {
        return Ok(());
    }
    let data = authority()?;
    ensure!(
        entry.original.guid == OWNER
            && entry.step["SourceRevision"] == data
            && entry.step["SourceRevisionAuthoritySha256"] == DATA_SHA
            && slot.archive.sha256 == text(&data["revision_archive"], "sha256")?
            && Some(slot.archive.bytes) == data["revision_archive"]["bytes"].as_u64(),
        "Decoded source revision context differs"
    );
    for evidence in ["installer", "selected_ini", "author_readme"] {
        let expected = &data[evidence];
        let path = text(expected, "path")?;
        let actual = slot
            .members
            .get(path)
            .context("Source revision evidence member absent")?;
        ensure!(
            !actual.directory
                && actual.sha256 == text(expected, "sha256")?
                && expected
                    .get("bytes")
                    .is_none_or(|bytes| bytes.as_u64() == Some(actual.bytes)),
            "Source revision selected installer/INI/author evidence differs"
        );
    }
    ensure!(
        !slot
            .members
            .keys()
            .any(|name| name.to_ascii_lowercase().ends_with("/namespaces.ini")),
        "Source revision unexpectedly has namespace choices"
    );
    let (_, parser) = super::read_ini(
        &slot
            .tree
            .path()
            .join(literal(text(&data["selected_ini"], "path")?)?),
    )?;
    ensure!(
        super::field(&parser, "Settings", "LookupGameFolder") == Some("0")
            && super::field(&parser, "Settings", "LookupGameNumber") == Some("1")
            && parser
                .section("CompileList")
                .is_some_and(|section| section.is_empty()),
        "Source revision game/compiler semantics differ"
    );
    super::check(cancelled)
}

/// A closed alternate transport group is not source execution authority. Body
/// selection, fresh snapshot and full native derivation still run afterwards.
pub(super) fn alternate_input_group(guid: &str, filenames: &[String]) -> Result<bool> {
    if guid == super::source_revisions_enlightenment::OWNER {
        return super::source_revisions_enlightenment::alternate_input_group(guid, filenames);
    }
    super::source_revisions_uniform::alternate_input_group(guid, filenames)
}

pub(super) fn expanded_limit(archive_sha: &str) -> Result<Option<u64>> {
    super::source_revisions_uniform::expanded_limit(archive_sha)
}
