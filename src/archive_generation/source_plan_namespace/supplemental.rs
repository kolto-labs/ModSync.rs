//! Supplemental source choices, authored separately from the original ModList
//! plan. No supplemental owner is assigned a fictional original operation.
//!
//! Package hashes authenticate fresh bounded inventories in the parent factory;
//! this table contains source choices, never a second whole-member manifest.
use super::original_plan_protocol::{Effect, Program};
use super::{GUIDES, ProgramEntry, sha};
use crate::{Guide, draft};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const DATA: &[u8] = include_bytes!("supplemental_authority.json");
pub(super) const DATA_SHA: &str =
    "92fad6add3641d53416effbf8a1360c51fb9b64fc340273c2baa33b06672ee74";
const ENCLAVE: &str = "403daebc-dd7f-892c-ac37-823cf1ae6600";
const ENCLAVE_PROFILE: &str = "english-tslrcm-standalone-no-m4-78-v1";
const MAPS: &str = "600d2fa2-c148-846f-bf3b-a44f10daf103";
const POLICY: &str = "closed-supplemental-source-choices-v2";
pub(super) const PROFILE: &str = "fixed-1920x1080-v1";

/// A successful earlier source binding is insufficient: consumers must prove the
/// actual earlier component completed, with this exact profile/selected INI, in
/// the current prediction replay or installed checkpoint before any effect.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", deny_unknown_fields)]
pub(super) enum Requirement {
    #[serde(rename = "completed_original_program")]
    CompletedOriginalProgram { owner_guid: String },
    #[serde(rename = "tslrcm_standalone")]
    TslrcmStandalone { owner_guid: String, version: String },
    #[serde(rename = "k1_high_resolution_menus")]
    K1HighResolutionMenus {
        owner_guid: String,
        profile_id: String,
    },
    #[serde(rename = "cloaked_jedi_robes")]
    CloakedJediRobes {
        owner_guid: String,
        archive_sha256: String,
        namespace_index: usize,
        namespace_id: String,
        data_path: String,
        ini_path: String,
        ini_sha256: String,
        namespaces_sha256: String,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Authority {
    schema: u32,
    policy: String,
    source_guide_sha256: BTreeMap<String, String>,
    source_kind: String,
    original_manual_plan_steps: bool,
    rows: Vec<Row>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Row {
    game: String,
    index: usize,
    guid: String,
    name: String,
    source_kind: String,
    profile: String,
    raw_component: Value,
    canonical_component: Value,
    inputs: Vec<Input>,
    copies: Vec<Copy>,
    prerequisites: Vec<Requirement>,
    choice_evidence: String,
    gap: Option<String>,
    #[serde(default)]
    historical_original_step: Option<Value>,
    #[serde(default)]
    original_source_plan_sha256: Option<String>,
    #[serde(default)]
    native_runs: Vec<NativeRun>,
    #[serde(default)]
    related_sources: Vec<RelatedSource>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeRun {
    input: usize,
    patcher: String,
    installer_sha256: String,
    cwd: String,
    namespace: usize,
    ini_path: String,
    ini_sha256: String,
    namespaces_path: String,
    namespaces_sha256: String,
    readme_path: String,
    readme_sha256: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RelatedSource {
    index: usize,
    guid: String,
    relation: String,
    raw_component: Value,
    canonical_component: Value,
    original_step: Option<Value>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Input {
    filename: String,
    sha256: String,
    source_url: String,
    source_id: String,
    publisher_filename: String,
    active: bool,
    exact_bytes_observed: bool,
    #[serde(default)]
    layout_evidence: Option<Value>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Copy {
    input: usize,
    member: String,
    destination: String,
    skip_existing: bool,
}
fn canonical_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn flat_filename(name: &str) -> Result<()> {
    ensure!(
        super::literal(name)?.components().count() == 1,
        "Supplemental input filename is not flat"
    );
    Ok(())
}
// Only this authenticated original CI layout may stand in for local body
// observation. The runtime parent still snapshots, hashes and fully decodes it.
fn input_certified(row: &Row, pin: &Input) -> bool {
    if pin.exact_bytes_observed {
        return pin.layout_evidence.is_none();
    }
    row.guid == MAPS
        && pin.filename == "Mod files-1086-1-0-1590189064.7z"
        && pin.sha256 == "e3c49ad8480d30bd9f936ded4f9c26cbfd5de076ba4d2ec58e72b07799ab5e5a"
        && pin.layout_evidence.as_ref()
            == Some(&json!({
                "archive_sha256":pin.sha256,
                "receipt_sha256":"10751b2b201db53010d935f3d5677efd1db640f313110f48fe4bcd842d554e70",
                "listing_sha256":"6acbed884238cc7e0eee4a772fad54650cd31e6b08d9014dd26c74fe0ce1f561",
                "inventory_sha256":"4ef82cb534acc76fa499eb4c580df294e3e994912970e12732acb4551f161377",
                "artifact_id":14137273816_u64,"run_id":"43654798480",
                "head":"e48ea51738ab223814a8128b8026e2354cbe14b0",
                "selected_prefix":"Galloways Maps Upscale/override",
                "selected_extension":"tga","selected_regular_files":75,
                "inert_regular_files":["Galloways Maps Upscale/title.jpg","Galloways Maps Upscale/readme.txt"]
            }))
}

/// Called only with the freshly decoded, whole-hash-authenticated inventory.
/// This supplies a bounded prefix/extension choice, not a member manifest.
pub(super) fn validate_inventory(guid: &str, filename: &str, files: &[String]) -> Result<()> {
    if guid != MAPS {
        return Ok(());
    }
    ensure!(
        filename == "Mod files-1086-1-0-1590189064.7z",
        "Maps input filename differs"
    );
    let prefix = "Galloways Maps Upscale/override/";
    let mut selected = 0;
    let mut inert = BTreeSet::new();
    for file in files {
        if let Some(member) = file.strip_prefix(prefix) {
            flat_filename(member)?;
            ensure!(
                member.ends_with(".tga"),
                "Maps selected prefix contains a non-TGA member"
            );
            selected += 1;
        } else {
            ensure!(
                [
                    "Galloways Maps Upscale/title.jpg",
                    "Galloways Maps Upscale/readme.txt"
                ]
                .contains(&file.as_str()),
                "Maps archive contains an unauthored payload location"
            );
            ensure!(inert.insert(file), "Maps inert member repeated");
        }
    }
    ensure!(
        selected == 75 && inert.len() == 2 && files.len() == 77,
        "Maps original bounded layout differs"
    );
    Ok(())
}

fn validate_native_choice(row: &Row) -> Result<()> {
    if row.guid != ENCLAVE {
        ensure!(
            row.native_runs.is_empty()
                && row.historical_original_step.is_none()
                && row.original_source_plan_sha256.is_none()
                && row.related_sources.is_empty(),
            "Unexpected supplemental historical/native source choice"
        );
        return Ok(());
    }
    ensure!(
        row.game == "k2"
            && row.index == 94
            && row.profile == ENCLAVE_PROFILE
            && row.copies.is_empty()
            && row.native_runs.len() == 1
            && row.inputs.len() == 1,
        "Extended Enclave source profile differs"
    );
    let run = &row.native_runs[0];
    ensure!(
        row.inputs[0].active
            && row.inputs[0].exact_bytes_observed
            && row.inputs[0].sha256
                == "ee329c08a30fd9b0e66766e51d841127f477bc8d9acfdc5be94e10fa4933db70"
            && run.input == 0
            && run.namespace == 0
            && run.cwd == "ExtendedEnclave_English"
            && run.patcher == "ExtendedEnclave_English/Ext Enclave install.exe"
            && run.installer_sha256
                == "013655cd871a71e0f5b50f80ed0d32308912916ba7e12e2a366f42d992a71b16"
            && run.ini_path == "ExtendedEnclave_English/tslpatchdata/rcmchanges.ini"
            && run.ini_sha256 == "36766175e0c5cb99c8f9f5846a0eb4ebe6b7c76f9fd05fad8e2dcea672d786ee"
            && run.namespaces_path == "ExtendedEnclave_English/tslpatchdata/namespaces.ini"
            && run.namespaces_sha256
                == "fc395dcc452d3c8a724ab48366955c1d01b89550ac9933cbff34a7f0ec32357a"
            && run.readme_path == "ExtendedEnclave_English/Readme.txt"
            && run.readme_sha256
                == "7da8751c7e2410a554757a878c70d9bb6840f188c9a645484bfc5a96882e1f68",
        "Extended Enclave exact English standalone choice differs"
    );
    Ok(())
}

fn validate_original_context(row: &Row, raw: &Guide, canonical: &Guide) -> Result<()> {
    if row.guid != ENCLAVE {
        return Ok(());
    }
    let (_, _, plan, plan_sha, _) = GUIDES[1];
    let plan: Value = serde_json::from_slice(plan)?;
    ensure!(
        row.original_source_plan_sha256.as_deref() == Some(plan_sha)
            && row.historical_original_step.as_ref() == Some(&plan["steps"][94]),
        "Extended Enclave historical extraction-only source differs"
    );
    ensure!(
        row.related_sources.len() == 3,
        "Extended Enclave related source inventory differs"
    );
    for (source, (index, relation)) in
        row.related_sources
            .iter()
            .zip([(1, "predecessor"), (93, "predecessor"), (95, "successor")])
    {
        ensure!(
            source.index == index
                && source.relation == relation
                && source.guid == canonical.components[index].guid
                && source.raw_component == serde_json::to_value(&raw.components[index])?
                && source.canonical_component
                    == serde_json::to_value(&canonical.components[index])?
                && source.original_step.as_ref() == plan["steps"].get(index),
            "Extended Enclave predecessor/successor full original context differs"
        );
    }
    Ok(())
}

fn authority() -> Result<Authority> {
    ensure!(
        sha(DATA) == DATA_SHA,
        "Supplemental compiled source choices changed"
    );
    let data: Authority = serde_json::from_slice(DATA)?;
    ensure!(
        data.schema == 1
            && data.policy == POLICY
            && data.source_kind == "authored-supplemental"
            && !data.original_manual_plan_steps
            && data.rows.len() == 11,
        "Supplemental authority header differs"
    );
    let mut guids = BTreeSet::new();
    for row in &data.rows {
        ensure!(
            guids.insert(&row.guid)
                && row.source_kind == "authored-supplemental"
                && row.profile
                    == if row.guid == ENCLAVE {
                        ENCLAVE_PROFILE
                    } else {
                        PROFILE
                    }
                && !row.choice_evidence.is_empty(),
            "Supplemental source row differs"
        );
        let game = match row.game.as_str() {
            "k1" => 0,
            "k2" => 1,
            _ => bail!("Supplemental game differs"),
        };
        let (_, guide_sha, _, _, count) = GUIDES[game];
        ensure!(
            data.source_guide_sha256.get(&row.game).map(String::as_str) == Some(guide_sha)
                && row.index < count,
            "Supplemental original guide/index differs"
        );
        ensure!(
            row.raw_component["Guid"] == row.guid
                && row.canonical_component["Guid"] == row.guid
                && row.raw_component["Name"] == row.name,
            "Supplemental owner context differs"
        );
        ensure!(!row.inputs.is_empty(), "Supplemental source input absent");
        let mut filenames = BTreeSet::new();
        for pin in &row.inputs {
            flat_filename(&pin.filename)?;
            flat_filename(&pin.publisher_filename)?;
            ensure!(
                filenames.insert(&pin.filename)
                    && canonical_hash(&pin.sha256)
                    && canonical_hash(&pin.source_id),
                "Supplemental input pin differs"
            );
            ensure!(
                row.raw_component["ResourceRegistry"]
                    .get(&pin.source_url)
                    .is_some(),
                "Supplemental input is not associated with original owner source"
            );
        }
        for copy in &row.copies {
            ensure!(
                row.inputs
                    .get(copy.input)
                    .is_some_and(|p| p.active && input_certified(row, p)),
                "Supplemental choice lacks observed exact input"
            );
            super::literal(&copy.member)?;
            super::literal(&copy.destination)?;
            ensure!(
                copy.destination == "override" || copy.destination.starts_with("override/"),
                "Supplemental copy leaves global Override destination"
            );
        }
        if row.guid == MAPS {
            ensure!(
                row.inputs.len() == 1
                    && input_certified(row, &row.inputs[0])
                    && row.copies.len() == 1
                    && row.copies[0].input == 0
                    && row.copies[0].member == "Galloways Maps Upscale/override"
                    && row.copies[0].destination == "override"
                    && !row.copies[0].skip_existing,
                "Maps bounded selected prefix differs"
            );
        }
        validate_native_choice(row)?;
        ensure!(
            row.gap.is_some() || !row.copies.is_empty() || !row.native_runs.is_empty(),
            "Supplemental source has no selected install effect"
        );
    }
    Ok(data)
}

pub(super) fn requirements(guid: &str) -> Result<Vec<Requirement>> {
    let required = match guid {
        ENCLAVE => vec![
            Requirement::TslrcmStandalone {
                owner_guid: "bee5a72b-d748-8f83-8261-81758a9bfc56".into(),
                version: "1.8.6".into(),
            },
            Requirement::CompletedOriginalProgram {
                owner_guid: "0d645887-5b3a-82a1-925e-cdd7c68bf923".into(),
            },
        ],
        "0e6154b2-8591-879d-b410-62baa95cd654" | "6ff2988d-370a-8079-9162-093df5c6d60b" => {
            vec![Requirement::K1HighResolutionMenus {
                owner_guid: "6c3c7165-297e-80eb-b9f8-d47506dab2d5".into(),
                profile_id: crate::k1_widescreen::PROFILE_ID.into(),
            }]
        }
        "461350dd-6027-8124-a17e-ae6fd5536f37" => vec![Requirement::CloakedJediRobes {
            owner_guid: "02034096-4a59-8fbc-b2a0-beb89cbf2067".into(),
            archive_sha256: "e1ff8d6398cc1cc8de4789ddcdba711ceb5c98358ceef9623504d1c5bc4491e4"
                .into(),
            namespace_index: 4,
            namespace_id: "k_brb-a".into(),
            data_path: "k_brb-a".into(),
            ini_path: "tslpatchdata/k_brb-a/k_brb-a.ini".into(),
            ini_sha256: "fa0dd504265581920b6ffc58c91ec736fece3f729e616a5fa28f429ffa7c4f4e".into(),
            namespaces_sha256: "4cb41d172d1184fe1ef602aeea6ae47418a4ce1c14d2da35e10189225b0966b3"
                .into(),
        }],
        _ => Vec::new(),
    };
    Ok(required)
}

fn compile_choice(row: &Row) -> Result<Program> {
    if let Some(gap) = &row.gap {
        bail!("Supplemental source gap: {gap}");
    }
    let step_id = format!("supplemental-{}-{}-{}", row.game, row.index, row.profile);
    let mut effects = Vec::new();
    let mut slots = BTreeMap::new();
    for (input, pin) in row.inputs.iter().enumerate().filter(|(_, p)| p.active) {
        ensure!(
            input_certified(row, pin),
            "Supplemental input lacks observed bytes or authenticated original layout"
        );
        let destination = format!("{{extract}}/{step_id}/{}", input + 1);
        slots.insert(input, destination.clone());
        effects.push(Effect::Extract {
            ordinal: effects.len(),
            source: format!("{{archives}}/{}", pin.filename),
            destination,
            sha256: pin.sha256.clone(),
        });
    }
    for copy in &row.copies {
        let slot = slots
            .get(&copy.input)
            .context("Supplemental selected input is inactive")?;
        effects.push(Effect::Copy {
            ordinal: effects.len(),
            source: format!("{slot}/{}", copy.member),
            destination: format!("{{game}}/{}", copy.destination),
            skip_existing: copy.skip_existing,
        });
    }
    for run in &row.native_runs {
        let slot = slots
            .get(&run.input)
            .context("Supplemental native input is inactive")?;
        effects.push(Effect::NativeRun {
            ordinal: effects.len(),
            patcher: format!("{slot}/{}", run.patcher),
            sha256: run.installer_sha256.clone(),
            cwd: format!("{slot}/{}", run.cwd),
            namespace: Some(run.namespace),
        });
    }
    ensure!(
        !row.copies.is_empty() || !row.native_runs.is_empty(),
        "Supplemental choice has no install effects"
    );
    Ok(Program {
        step_id,
        source_heading: format!("Supplemental: {}", row.name),
        effects,
    })
}

/// This is a private source issuer for the parent verifier, never a public model
/// metadata decoder. Callers may use this only after independently proving EVERY
/// typed requirement from `requirements` in actual checkpoint/prediction state.
/// The engine checks completed predecessor definitions before any runtime effect.
pub(super) fn compiled_program(guid: &str) -> Result<Program> {
    let row = authority()?
        .rows
        .into_iter()
        .find(|r| r.guid == guid)
        .context("Unknown compiled supplemental source")?;
    compile_choice(&row)
}

pub(super) fn entries() -> Result<Vec<ProgramEntry>> {
    let data = authority()?;
    let mut sources = Vec::new();
    for (guide, guide_sha, plan, plan_sha, count) in GUIDES {
        ensure!(
            sha(guide) == guide_sha && sha(plan) == plan_sha,
            "Supplemental full original source pins changed"
        );
        let mut raw = Guide::parse(std::str::from_utf8(guide)?, "md")?;
        draft::generate(&mut raw);
        let canonical = Guide::parse_preserving_selection(&serde_json::to_string(&raw)?, "json")?;
        ensure!(
            raw.components.len() == count && canonical.components.len() == count,
            "Supplemental full original owner inventory changed"
        );
        sources.push((raw, canonical));
    }
    let mut entries = Vec::with_capacity(data.rows.len());
    for row in data.rows {
        let game = usize::from(row.game == "k2");
        let (raw, canonical) = &sources[game];
        let original = canonical
            .components
            .get(row.index)
            .context("Supplemental original owner index absent")?;
        ensure!(
            serde_json::to_value(&raw.components[row.index])? == row.raw_component
                && serde_json::to_value(original)? == row.canonical_component,
            "Supplemental full raw/canonical original context differs"
        );
        validate_original_context(&row, raw, canonical)?;
        let inputs = row
            .inputs
            .iter()
            .filter(|pin| pin.active)
            .map(|p| (p.filename.clone(), p.sha256.clone()))
            .collect();
        let step = json!({"source_kind":"authored-supplemental","policy":POLICY,"profile":row.profile,
            "source_guide_sha256":data.source_guide_sha256[&row.game],"original_manual_plan_step":null,
            "source_owner_index_zero_based":row.index,"source_owner_guid":row.guid,
            "source_inputs":row.inputs,"selected_copies":row.copies,"prerequisites":row.prerequisites,
            "choice_evidence":row.choice_evidence,"source_gap":row.gap,
            "historical_original_step":row.historical_original_step,
            "original_source_plan_sha256":row.original_source_plan_sha256,
            "native_runs":row.native_runs,"related_sources":row.related_sources});
        let prerequisites = requirements(&row.guid)?;
        ensure!(
            prerequisites == row.prerequisites,
            "Supplemental typed prerequisites changed"
        );
        let program = compiled_program(&row.guid).map_err(|e| e.to_string());
        entries.push(ProgramEntry {
            game: row.game,
            index: row.index,
            raw: row.raw_component,
            original: original.clone(),
            step,
            inputs,
            program,
        });
    }
    Ok(entries)
}
