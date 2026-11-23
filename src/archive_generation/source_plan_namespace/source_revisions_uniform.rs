//! Current public Uniform 7z is a separate source revision from the original ZIP.
//! Only immutable source identities and typed effects are authored here. The
//! common factory reconstructs every package member and native configuration.
use super::original_plan_protocol::{Effect, Program};
use super::{Entry, ProgramEntry, ProgramSlot, file_pin, literal, sha};
use anyhow::{Context, Result, ensure};
use odypatcher::patcher::ini::{ConfigParser, OptionXform};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    sync::atomic::AtomicBool,
};

const DATA: &[u8] = include_bytes!("source_revisions_uniform.json");
pub(super) const DATA_SHA: &str =
    "688e1024b2608169f1e3999ce8249f002540e2cfc2ac8b9f2d01f17035e0d931";
pub(super) const OWNER: &str = "ed488679-bc04-892d-973b-107511862024";
const CURRENT: &str = "231ca858a5c354c3a76f8408b06256376930bcc999e8967491b1bc750547abe0";
const EXPANDED: u64 = 542155537;
const OLD_FILE: &str = "Heyorange_s_Sith Uniform_Reformation_Revised.zip";
const NEW_FILE: &str = "Heyorange_s_Sith Uniform_Reformation_Revised.7z";
fn authority() -> Result<Value> {
    ensure!(
        sha(DATA) == DATA_SHA,
        "Compiled Uniform source revision differs"
    );
    let data: Value = serde_json::from_slice(DATA)?;
    ensure!(
        data["schema"] == 1
            && data["policy"] == "closed-reviewed-source-revision-v1"
            && data["owner_guid"] == OWNER
            && data["component_index"] == 80
            && data["game"] == "k1"
            && data["original_guide_sha256"] == super::GUIDES[0].1
            && data["original_plan_sha256"] == super::GUIDES[0].3,
        "Uniform original source context differs"
    );
    Ok(data)
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("Uniform source {key} absent"))
}
fn revision_step(entry: &ProgramEntry, data: &Value) -> Value {
    json!({"SourceKind":"reviewed-publisher-source-revision", "SourceRevisionAuthoritySha256":DATA_SHA,
        "OriginalSourceProgram":entry.step,"SourceRevision":data})
}
pub(super) fn alternate_input_group(guid: &str, filenames: &[String]) -> Result<bool> {
    if guid != OWNER {
        return Ok(false);
    }
    authority()?;
    Ok(filenames.len() == 1 && filenames[0] == NEW_FILE)
}
pub(super) fn source_definition_matches(entry: &ProgramEntry, submitted: &Value) -> Result<bool> {
    if entry.original.guid != OWNER {
        return Ok(false);
    }
    let data = authority()?;
    Ok(entry.index == 80
        && entry.game == "k1"
        && entry.step == data["original_step"]
        && (submitted == &entry.step || submitted == &revision_step(entry, &data)))
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
pub(super) fn select(
    entry: &ProgramEntry,
    root: &Path,
    cancelled: &AtomicBool,
) -> Result<Option<ProgramEntry>> {
    if entry.original.guid != OWNER {
        return Ok(None);
    }
    let data = authority()?;
    ensure!(
        entry.index == 80
            && entry.game == "k1"
            && entry.step == data["original_step"]
            && entry.inputs
                == BTreeMap::from([(
                    OLD_FILE.into(),
                    text(&data["original_archive"], "sha256")?.into()
                )])
            && entry
                .original
                .resource_registry
                .contains_key(text(&data["publisher_download"], "source_url")?),
        "Uniform revision does not replace its complete original owner"
    );
    let root = super::super::resolved::local_root(root)?;
    let old = exists(&root.join(OLD_FILE))?;
    let new = exists(&root.join(NEW_FILE))?;
    ensure!(
        !(old && new),
        "Uniform source revision is ambiguous: both ZIP and 7z present"
    );
    ensure!(
        old || new,
        "Uniform original ZIP or reviewed current 7z is required"
    );
    let filename = if old { OLD_FILE } else { NEW_FILE };
    let pin = file_pin(
        &root.join(literal(filename)?),
        filename,
        512 * 1024 * 1024,
        cancelled,
    )?;
    if old {
        ensure!(
            pin.sha256 == text(&data["original_archive"], "sha256")?,
            "Uniform original ZIP body differs"
        );
        return Ok(None);
    }
    ensure!(
        pin.sha256 == CURRENT && pin.bytes == 139261662,
        "Uniform current 7z body differs"
    );
    let step_id = "source-revision-k1-uniform-231ca858";
    let slot = format!("{{extract}}/{step_id}/1");
    let program = Program {
        step_id: step_id.into(),
        source_heading:
            "Reviewed source revision: Sith Uniform Reformation Revised Community Patch option"
                .into(),
        effects: vec![
            Effect::Extract {
                ordinal: 0,
                source: format!("{{archives}}/{NEW_FILE}"),
                destination: slot.clone(),
                sha256: CURRENT.into(),
            },
            Effect::NativeRun {
                ordinal: 1,
                patcher: format!("{slot}/INSTALL.exe"),
                sha256: text(&data["installer"], "sha256")?.into(),
                cwd: slot,
                namespace: Some(1),
            },
        ],
    };
    Ok(Some(ProgramEntry {
        game: entry.game.clone(),
        index: entry.index,
        raw: entry.raw.clone(),
        original: entry.original.clone(),
        step: revision_step(entry, &data),
        inputs: BTreeMap::from([(NEW_FILE.into(), CURRENT.into())]),
        program: Ok(program),
    }))
}
/// Per-body measured expansion; no other archive or member ceiling is raised.
/// Parent callers have already verified this archive pin against a fresh body.
pub(super) fn expanded_limit(archive_sha: &str) -> Result<Option<u64>> {
    if archive_sha != CURRENT {
        return Ok(None);
    }
    let data = authority()?;
    ensure!(
        data["revision_archive"]["bytes"] == 139261662
            && data["bounded_inventory"]["source_specific_expansion_limit"] == EXPANDED,
        "Uniform measured source bounds differ"
    );
    Ok(Some(EXPANDED))
}
pub(super) fn validate_slot(
    entry: &ProgramEntry,
    slot: &ProgramSlot,
    cancelled: &AtomicBool,
) -> Result<()> {
    if entry.original.guid != OWNER || entry.step.get("SourceRevision").is_none() {
        return Ok(());
    }
    let data = authority()?;
    ensure!(
        entry.step
            == json!({"SourceKind":"reviewed-publisher-source-revision", "SourceRevisionAuthoritySha256":DATA_SHA,
            "OriginalSourceProgram":data["original_step"], "SourceRevision":data})
            && slot.archive.filename == NEW_FILE
            && slot.archive.sha256 == CURRENT
            && slot.archive.bytes == 139261662,
        "Uniform decoded revision context differs"
    );
    ensure!(
        slot.members.len() == 110
            && slot.members.values().filter(|m| !m.directory).count() == 109
            && slot.members.values().map(|m| m.bytes).sum::<u64>() == EXPANDED
            && slot.members.values().all(|m| m.bytes <= 128 * 1024 * 1024),
        "Uniform complete measured inventory differs"
    );
    for evidence in [
        "installer",
        "selected_ini",
        "initial_ini",
        "namespaces",
        "author_info",
    ] {
        let pin = &data[evidence];
        let member = slot
            .members
            .get(text(pin, "path")?)
            .context("Uniform evidence member absent")?;
        ensure!(
            !member.directory
                && member.sha256 == text(pin, "sha256")?
                && Some(member.bytes) == pin["bytes"].as_u64(),
            "Uniform selected source evidence differs"
        );
    }
    let (_, parser) = super::read_ini(&slot.tree.path().join("tslpatchdata/namespaces.ini"))?;
    ensure!(
        super::field(&parser, "Namespaces", "Namespace2") == Some("k1cp")
            && super::field(&parser, "k1cp", "IniName") == Some("k1cp.ini")
            && super::field(&parser, "k1cp", "DataPath") == Some("")
            && super::field(&parser, "k1cp", "Name")
                == Some("KOTOR Community Patch-Compatible Installation"),
        "Uniform current Community Patch source choice differs"
    );
    super::check(cancelled)
}

fn inactive_names(parser: &ConfigParser, selected: bool) -> Result<BTreeSet<String>> {
    let mut active: BTreeSet<String> = [
        "Settings",
        "TLKList",
        "InstallList",
        "2DAList",
        "GFFList",
        "CompileList",
        "SSFList",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    for list in ["TLKList", "2DAList", "CompileList", "SSFList"] {
        ensure!(
            parser.section(list).is_some_and(|s| s.is_empty()),
            "Uniform unexpected active patch list"
        );
    }
    let install = parser
        .section("InstallList")
        .context("Uniform InstallList absent")?;
    ensure!(install.len() == 2, "Uniform install group count differs");
    for (key, _) in install.items() {
        active.insert(key.clone());
    }
    let gff = parser
        .section("GFFList")
        .context("Uniform GFFList absent")?;
    ensure!(
        gff.len() == if selected { 29 } else { 35 },
        "Uniform active GFF count differs"
    );
    let mut pending = Vec::new();
    for (_, value) in gff.items() {
        let name = value.as_deref().context("Uniform dynamic GFF source")?;
        if active.insert(name.to_owned()) {
            pending.push(name.to_owned());
        }
    }
    while let Some(name) = pending.pop() {
        let section = parser
            .section(&name)
            .context("Uniform active GFF section absent")?;
        for (key, value) in section.items() {
            if key.starts_with("AddField") {
                let child = value
                    .as_deref()
                    .context("Uniform dynamic active AddField")?;
                if active.insert(child.to_owned()) {
                    ensure!(active.len() <= 116, "Uniform active graph exceeds bound");
                    pending.push(child.to_owned());
                }
            }
        }
    }
    let mut expected: BTreeSet<String> = [
        "gff_kor_calo_thug2_Equip_ItemList_0_0",
        "gff_kor_calo_thug2_EquippedRes_0",
        "gff_kor_calo_thug2_Dropable_0",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    if selected {
        expected.extend(
            [
                "kor35_sithguard.utc",
                "kor35_sithguard2.utc",
                "kor35_sithguard3.utc",
                "kor35_sithguard4.utc",
                "kor36_sithguard.utc",
                "kor36_sithguard1.utc",
            ]
            .into_iter()
            .map(String::from),
        );
    }
    let actual: BTreeSet<_> = parser
        .sections()
        .filter(|s| !active.contains(*s))
        .map(String::from)
        .collect();
    ensure!(
        actual == expected,
        "Uniform unclassified active/inactive source sections"
    );
    Ok(actual)
}
/// Graph resource analysis only: the exact raw/selected INI and runtime model
/// remain unchanged. Prove active reachability before excluding malformed but
/// unreachable authored AddField templates from the conservative graph walker.
pub(super) fn effective_graph_text(owner_guid: &str, raw: &str) -> Result<Option<String>> {
    if owner_guid != OWNER {
        return Ok(None);
    }
    let data = authority()?;
    let digest = sha(raw.as_bytes());
    let selected = digest == text(&data["selected_ini"], "sha256")?;
    if !selected && digest != text(&data["initial_ini"], "sha256")? {
        return Ok(None);
    }
    let parser = ConfigParser::read_string(raw, OptionXform::Identity)
        .map_err(|e| anyhow::anyhow!("Uniform source graph: {e}"))?;
    let inactive = inactive_names(&parser, selected)?;
    let mut include = true;
    let mut result = String::new();
    for line in raw.split_inclusive('\n') {
        let trimmed = line.trim();
        if let Some(section) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            include = !inactive.contains(section);
        }
        if include {
            result.push_str(line);
        }
    }
    Ok(Some(result))
}
pub(super) fn classify_inactive(
    entry: &Entry,
    parser: &ConfigParser,
    message: &str,
) -> Result<bool> {
    let row = &entry.row;
    if row.guid != OWNER
        || row.archive.sha256 != CURRENT
        || row.game != "k1"
        || row.ini.path != "tslpatchdata/k1cp.ini"
        || row.ini.sha256 != "51fd2182fa36dc08bbd2f0a9a1dea9ecbe6f52e5d1c61ac95d6e7eb7e7a2890b"
        || !row
            .namespace
            .as_ref()
            .is_some_and(|n| n.index == 1 && n.id == "k1cp" && n.data_path.is_empty())
    {
        return Ok(false);
    }
    let Some(names) =
        message.strip_prefix("There are some orphaned ini sections found in the changes:\n")
    else {
        return Ok(false);
    };
    let expected = inactive_names(parser, true)?;
    let logged: Vec<_> = names.lines().collect();
    ensure!(
        logged.len() == expected.len()
            && logged.iter().copied().collect::<BTreeSet<_>>()
                == expected.iter().map(String::as_str).collect(),
        "Uniform orphan note differs from proved unreachable sections"
    );
    Ok(true)
}
