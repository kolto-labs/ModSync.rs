//! Reviewed current Enlightenment source version, preserving original guide and
//! plan context. The author's installation mode is two loose compiled scripts;
//! reference NSS is inert and grants no compiler or installer authority.
use super::original_plan_protocol::{Effect, Program};
use super::{ProgramEntry, ProgramSlot, file_pin, sha};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path, sync::atomic::AtomicBool};

const DATA: &[u8] = include_bytes!("source_revisions_enlightenment.json");
pub(super) const DATA_SHA: &str =
    "fbce609fe692e64319ff4ba7c7e1dc6c4de064e191b33d60f1e0b6ba4719ad35";
pub(super) const OWNER: &str = "e015887a-be0f-86d1-830e-92d82c8fc46c";
const OLD_FILE: &str = "Improved Enlightenment and Corruption Feats.zip";
const NEW_FILE: &str = "Improved_Enlightenment_and_Corruption_Feats_v1_0_1.zip";
const CURRENT: &str = "b3730c98a05a12ee0a6610a57570be62ff14a7217abaef667ced9b4b541f7eed";
fn authority() -> Result<Value> {
    ensure!(
        sha(DATA) == DATA_SHA,
        "Compiled Enlightenment source revision differs"
    );
    let data: Value = serde_json::from_slice(DATA)?;
    ensure!(
        data["schema"] == 1
            && data["policy"] == "closed-reviewed-source-revision-v1"
            && data["owner_guid"] == OWNER
            && data["game"] == "k2"
            && data["component_index"] == 135
            && data["original_guide_sha256"] == super::GUIDES[1].1
            && data["original_plan_sha256"] == super::GUIDES[1].3,
        "Enlightenment original source context differs"
    );
    Ok(data)
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("Enlightenment {key} absent"))
}
fn revision_step(original: &Value, data: &Value) -> Value {
    json!({"SourceKind":"reviewed-publisher-source-revision","SourceRevisionAuthoritySha256":DATA_SHA,
        "OriginalSourceProgram":original,"SourceRevision":data})
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
    Ok(entry.index == 135
        && entry.game == "k2"
        && entry.step == data["original_step"]
        && (submitted == &entry.step || submitted == &revision_step(&entry.step, &data)))
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
        entry.index == 135
            && entry.game == "k2"
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
        "Enlightenment revision does not replace its complete original owner"
    );
    let root = super::super::resolved::local_root(root)?;
    let old = exists(&root.join(OLD_FILE))?;
    let new = exists(&root.join(NEW_FILE))?;
    ensure!(
        !(old && new),
        "Enlightenment source revision is ambiguous: both ZIP versions present"
    );
    ensure!(
        old || new,
        "Enlightenment original or reviewed current source ZIP is required"
    );
    let name = if old { OLD_FILE } else { NEW_FILE };
    let pin = file_pin(&root.join(name), name, 512 * 1024 * 1024, cancelled)?;
    if old {
        ensure!(
            pin.sha256 == text(&data["original_archive"], "sha256")?,
            "Enlightenment original ZIP differs"
        );
        return Ok(None);
    }
    ensure!(
        pin.sha256 == CURRENT && pin.bytes == 4370,
        "Enlightenment current v1_0_1 ZIP differs"
    );
    ensure!(
        data["configuration_contract"]["guide_tripling_equivalence_verified"] == true,
        "Current Enlightenment v1_0_1 source is held: supplied 1.5x influence behavior does not preserve the guide's required 3x effect; original 0c6db source or independently verified compatibility is required"
    );
    let step_id = "source-revision-k2-enlightenment-b373";
    let slot = format!("{{extract}}/{step_id}/1");
    let mut effects = vec![Effect::Extract {
        ordinal: 0,
        source: format!("{{archives}}/{NEW_FILE}"),
        destination: slot.clone(),
        sha256: CURRENT.into(),
    }];
    let payloads = data["selected_payloads"]
        .as_array()
        .context("Enlightenment selected payloads absent")?;
    ensure!(payloads.len() == 2, "Enlightenment payload count differs");
    for payload in payloads {
        let path = text(payload, "path")?;
        let filename = Path::new(path)
            .file_name()
            .and_then(|p| p.to_str())
            .context("Enlightenment payload name absent")?;
        ensure!(
            matches!(filename, "a_influence_dec.ncs" | "a_influence_inc.ncs"),
            "Enlightenment payload name differs"
        );
        effects.push(Effect::Copy {
            ordinal: effects.len(),
            source: format!("{slot}/{path}"),
            destination: format!("{{game}}/override/{filename}"),
            skip_existing: false,
        });
    }
    Ok(Some(ProgramEntry {
        game: entry.game.clone(),
        index: entry.index,
        raw: entry.raw.clone(),
        original: entry.original.clone(),
        step: revision_step(&entry.step, &data),
        inputs: BTreeMap::from([(NEW_FILE.into(), CURRENT.into())]),
        program: Ok(Program {
            step_id: step_id.into(),
            source_heading: "Reviewed source revision: Improved Enlightenment/Corruption v1_0_1"
                .into(),
            effects,
        }),
    }))
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
        entry.step == revision_step(&data["original_step"], &data)
            && slot.archive.filename == NEW_FILE
            && slot.archive.sha256 == CURRENT
            && slot.archive.bytes == 4370,
        "Enlightenment decoded source revision differs"
    );
    ensure!(
        slot.members.len() == 8
            && slot.members.values().filter(|m| !m.directory).count() == 5
            && slot.members.values().map(|m| m.bytes).sum::<u64>() == 5664,
        "Enlightenment complete package inventory differs"
    );
    let mut evidence = vec![&data["author_readme"]];
    evidence.extend(
        data["selected_payloads"]
            .as_array()
            .context("Enlightenment selected payload pins absent")?,
    );
    evidence.extend(
        data["inert_reference_sources"]
            .as_array()
            .context("Enlightenment reference source pins absent")?,
    );
    for pin in evidence {
        let member = slot
            .members
            .get(text(pin, "path")?)
            .context("Enlightenment source evidence absent")?;
        ensure!(
            !member.directory
                && member.sha256 == text(pin, "sha256")?
                && Some(member.bytes) == pin["bytes"].as_u64(),
            "Enlightenment source evidence differs"
        );
    }
    ensure!(
        slot.members
            .keys()
            .all(|path| !path.to_ascii_lowercase().ends_with(".exe")
                && !path.to_ascii_lowercase().ends_with(".ini")),
        "Enlightenment loose-file source unexpectedly contains installer configuration"
    );
    super::check(cancelled)
}
