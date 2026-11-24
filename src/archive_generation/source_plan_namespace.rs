//! Closed source-plan choices, reconstructed from original sources and fresh bytes.
//! A namespace index is never authority by itself. The compiled source choice,
//! complete package and selected INI must agree before a private witness exists.
use crate::{Component, Guide, Instruction, archive, draft};
use anyhow::{Context, Result, ensure};
use odypatcher::patcher::{
    config::PatcherConfig,
    ini::{CaseInsensitiveDict, ConfigParser, OptionXform},
    mods::ModBase,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

mod certified_nss;
mod inactive_sections;
pub(crate) use certified_nss::CertifiedProgramNss;
pub(super) mod original_plan_protocol;
mod source_revisions;
mod source_revisions_enlightenment;
mod source_revisions_uniform;
mod supplemental;

pub(crate) const GENERATOR: &str = "source-plan-selected-namespace-v1";
pub(crate) const PROGRAM_GENERATOR: &str = "original-pinned-source-program-v1";
pub(crate) const BINDING: &str = "SourcePlanNamespaceBinding";
const POLICY: &str = "closed-original-source-selected-namespace-v1";
const DATA: &[u8] = include_bytes!("source_plan_namespace_authority.json");
const DATA_SHA: &str = "ebe33970bcd62beedc641b7a814cd1544377ecc66f42f5adbcc8d02aa293100d";
const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;
const MAX_MEMBER: u64 = 128 * 1024 * 1024;
const MAX_EXPANDED: u64 = 512 * 1024 * 1024;
const MAX_INI: u64 = 2 * 1024 * 1024;
const MAX_ENTRIES: usize = 10000;
type PinnedGuide = (
    &'static [u8],
    &'static str,
    &'static [u8],
    &'static str,
    usize,
);
const GUIDES: [PinnedGuide; 2] = [
    (
        include_bytes!("source_plan_k1.md"),
        "44067788c6cb7a83b38d468817f273ce5cc901cd5e000c0f9ff65f8af15fc6ed",
        include_bytes!("../../tools/modsync-ci/fixtures/ci/k1-manual-plan.json"),
        "1b402ba085625c4863795069eaacaef0f53e8ea7179e56a5bd150d88eb9c0a74",
        197,
    ),
    (
        include_bytes!("source_plan_k2.md"),
        "f442bdf574031623ed577609730d887069b9bc8e38e8e2f38ecc43a7e7487d5f",
        include_bytes!("../../tools/modsync-ci/fixtures/ci/k2-manual-plan.json"),
        "0add8c1aeb7cdfad3df4b21761e135c150aa7832cf3fc0edabe01a91245a9258",
        151,
    ),
];

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    path: String,
    sha256: String,
    bytes: u64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ArchivePin {
    filename: String,
    sha256: String,
    bytes: u64,
}
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Member {
    path: String,
    directory: bool,
    bytes: u64,
    sha256: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Namespace {
    index: usize,
    key: String,
    id: String,
    map: Pin,
    ini_name: String,
    data_path: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Row {
    game: String,
    component_index: usize,
    guid: String,
    source_heading: String,
    step_index: usize,
    step_id: String,
    source_choice: String,
    archive: ArchivePin,
    installer: Pin,
    ini: Pin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    namespace: Option<Namespace>,
    members: Vec<Member>,
    raw_component: Value,
    canonical_component: Value,
    original_step: Value,
    disabled_lookup_game_hint: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Authority {
    schema: u32,
    policy: String,
    source_guides: BTreeMap<String, String>,
    source_plans: BTreeMap<String, String>,
    rows: Vec<Row>,
}
struct Entry {
    row: Row,
    original: Component,
    patch_index: usize,
    deleted: BTreeSet<String>,
}
struct Compiled {
    entries: BTreeMap<String, Entry>,
}
static COMPILED: OnceLock<std::result::Result<Compiled, String>> = OnceLock::new();

fn check(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Source namespace admission cancelled"
    );
    Ok(())
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn value_digest(value: &Value) -> Result<String> {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(fields) => {
                let mut keys: Vec<_> = fields.keys().collect();
                keys.sort();
                Value::Object(
                    keys.into_iter()
                        .map(|key| (key.clone(), sorted(&fields[key])))
                        .collect(),
                )
            }
            Value::Array(values) => Value::Array(values.iter().map(sorted).collect()),
            _ => value.clone(),
        }
    }
    Ok(sha(&serde_json::to_vec(&sorted(value))?))
}
fn literal(value: &str) -> Result<PathBuf> {
    ensure!(
        !value.is_empty()
            && value.len() <= 4096
            && !value.contains(['\\', '*', '?', ':'])
            && !value.chars().any(char::is_control),
        "Unsafe namespace literal path"
    );
    let path = crate::paths::literal_relative(value)?;
    ensure!(
        !path.as_os_str().is_empty() && path.to_string_lossy().replace('\\', "/") == value,
        "Noncanonical namespace literal path"
    );
    Ok(path)
}
fn folder(value: &str) -> Result<PathBuf> {
    if value.is_empty() || value == "." {
        Ok(PathBuf::new())
    } else {
        literal(&value.replace('\\', "/"))
    }
}
fn token(value: &str, root: &str) -> Result<String> {
    let value = value
        .strip_prefix(&format!("{{{root}}}/"))
        .context("Wrong original source-plan path root")?;
    literal(value)?;
    Ok(value.into())
}
fn ordinary(metadata: &fs::Metadata, directory: bool) -> Result<()> {
    ensure!(
        !metadata.file_type().is_symlink()
            && if directory {
                metadata.is_dir()
            } else {
                metadata.is_file()
            },
        "Namespace admission rejects links and special files"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "Namespace admission rejects reparse points"
        );
    }
    Ok(())
}
fn file_pin(path: &Path, name: &str, limit: u64, cancelled: &AtomicBool) -> Result<Pin> {
    ordinary(&fs::symlink_metadata(path)?, false)?;
    let mut file = crate::paths::open_regular_read(path)?;
    let before = file.metadata()?;
    ensure!(before.len() <= limit, "Namespace source exceeds byte bound");
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0; 65536];
    loop {
        check(cancelled)?;
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes = bytes
            .checked_add(n as u64)
            .context("Namespace file size overflow")?;
        ensure!(
            bytes <= before.len(),
            "Namespace source changed while reading"
        );
        hash.update(&buffer[..n]);
    }
    ensure!(
        bytes == before.len() && file.metadata()?.len() == bytes,
        "Namespace source truncated while reading"
    );
    Ok(Pin {
        path: name.into(),
        sha256: format!("{:x}", hash.finalize()),
        bytes,
    })
}
fn same_pin(actual: &Pin, expected: &Pin) -> Result<()> {
    ensure!(
        actual.path == expected.path
            && actual.sha256 == expected.sha256
            && actual.bytes == expected.bytes,
        "Namespace member identity differs from compiled source choice"
    );
    Ok(())
}

fn load_compiled() -> Result<Compiled> {
    ensure!(
        DATA.len() <= 512 * 1024 && sha(DATA) == DATA_SHA,
        "Compiled namespace authority changed"
    );
    let authority: Authority = serde_json::from_slice(DATA)?;
    ensure!(
        authority.schema == 1
            && authority.policy == POLICY
            && authority.rows.len() == 10
            && authority.source_guides.len() == 2
            && authority.source_plans.len() == 2,
        "Compiled namespace authority scope differs"
    );
    let mut sources = Vec::new();
    for (index, (guide, guide_sha, plan, plan_sha, count)) in GUIDES.iter().enumerate() {
        let game = if index == 0 { "k1" } else { "k2" };
        ensure!(
            sha(guide) == *guide_sha
                && sha(plan) == *plan_sha
                && authority.source_guides.get(game).map(String::as_str) == Some(*guide_sha)
                && authority.source_plans.get(game).map(String::as_str) == Some(*plan_sha),
            "Original namespace sources changed"
        );
        let mut raw = Guide::parse(std::str::from_utf8(guide)?, "md")?;
        draft::generate(&mut raw);
        let raw_values: Vec<Value> = raw
            .components
            .iter()
            .map(serde_json::to_value)
            .collect::<std::result::Result<_, _>>()?;
        let canonical = Guide::parse_preserving_selection(&serde_json::to_string(&raw)?, "json")?;
        ensure!(
            raw_values.len() == *count && canonical.components.len() == *count,
            "Original namespace owner inventory differs"
        );
        let plan: Value = serde_json::from_slice(plan)?;
        ensure!(
            plan["guide_sha256"] == *guide_sha,
            "Original namespace plan guide differs"
        );
        sources.push((raw_values, canonical, plan));
    }
    let mut entries = BTreeMap::new();
    for row in authority.rows {
        let game = match row.game.as_str() {
            "k1" => 0,
            "k2" => 1,
            _ => anyhow::bail!("Unknown namespace source game"),
        };
        let (raw, canonical, plan) = &sources[game];
        let original = canonical
            .components
            .get(row.component_index)
            .context("Namespace source owner index out of range")?;
        ensure!(
            original.guid == row.guid
                && raw[row.component_index] == row.raw_component
                && serde_json::to_value(original)? == row.canonical_component
                && plan["steps"][row.step_index] == row.original_step
                && row.original_step["id"] == row.step_id
                && row.original_step["source_heading"] == row.source_heading
                && !row.source_choice.is_empty(),
            "Namespace full raw/canonical source context differs"
        );
        ensure!(
            row.archive.bytes > 0
                && row.archive.bytes <= MAX_ARCHIVE
                && literal(&row.archive.filename)?.components().count() == 1
                && plan["archive_sha256"][format!("{{archives}}/{}", row.archive.filename)]
                    == row.archive.sha256,
            "Namespace original archive pin differs"
        );
        let operations = row.original_step["operations"]
            .as_array()
            .context("Namespace original operations missing")?;
        ensure!(
            (2..=3).contains(&operations.len())
                && operations[0]["type"] == "extract"
                && operations[0]["source"] == format!("{{archives}}/{}", row.archive.filename),
            "Namespace original operation protocol differs"
        );
        let slot = token(
            operations[0]["destination"]
                .as_str()
                .context("Original extraction slot absent")?,
            "extract",
        )?;
        ensure!(
            slot == format!("{}/1", row.step_id),
            "Namespace original extraction slot differs"
        );
        let patch_index = operations.len() - 1;
        let operation = &operations[patch_index];
        ensure!(
            token(
                operation["patcher"]
                    .as_str()
                    .context("Original installer absent")?,
                "extract"
            )? == format!("{slot}/{}", row.installer.path)
                && operation["sha256"] == row.installer.sha256,
            "Namespace original installer identity differs"
        );
        let argv = operation["argv"]
            .as_array()
            .context("Namespace source argv missing")?;
        if let Some(namespace) = &row.namespace {
            ensure!(
                operation["type"] == "patcher"
                    && argv.len() == 9
                    && argv[0] == "/usr/bin/wine"
                    && argv[1] == operation["patcher"]
                    && argv[2] == "--game-dir"
                    && argv[3] == "{game}"
                    && argv[4] == "--tslpatchdata"
                    && argv[5] == operation["cwd"]
                    && argv[6] == "--namespace-option-index"
                    && argv[7]
                        .as_str()
                        .and_then(|value| value.parse::<usize>().ok())
                        == Some(namespace.index)
                    && argv[8] == "--install",
                "Namespace positional source selection differs"
            );
        } else {
            let title = operation["actions"][0]["wait_window"]
                .as_str()
                .context("Original GUI title absent")?;
            let actions = json!([{"wait_window":title,"timeout":120},{"sleep":3},{"click":[502,433],"window":title},
                {"wait_window":"^Browse for Folder$","timeout":120},{"sleep":2},{"click":[62,69],"window":"^Browse for Folder$"},
                {"sleep":1},{"click":[157,274],"window":"^Browse for Folder$"},{"wait_log":600}]);
            ensure!(
                operation["type"] == "gui_patcher"
                    && operation["actions"] == actions
                    && argv == &vec![json!("/usr/bin/wine"), operation["patcher"].clone()],
                "Namespace original normal GUI protocol differs"
            );
        }
        let mut deleted = BTreeSet::new();
        for operation in &operations[1..patch_index] {
            ensure!(
                operation["type"] == "delete" && operation.get("missing_ok").is_none(),
                "Unadmitted namespace interleaved operation"
            );
            let path = token(
                operation["path"]
                    .as_str()
                    .context("Original deleted path absent")?,
                "extract",
            )?;
            let relative = path
                .strip_prefix(&format!("{slot}/"))
                .context("Namespace deletion escaped original slot")?;
            let deleted_path = Path::new(relative);
            ensure!(
                deleted_path.parent() == Path::new(&row.installer.path).parent()
                    && deleted_path
                        .file_name()
                        .is_some_and(|name| name == "installlog.txt" || name == "installlog.rtf")
                    && row
                        .members
                        .iter()
                        .any(|member| member.path == relative && !member.directory),
                "Unadmitted namespace log deletion"
            );
            deleted.insert(relative.into());
        }
        let mut names = BTreeSet::new();
        let mut total = 0u64;
        for member in &row.members {
            literal(&member.path)?;
            ensure!(
                names.insert(crate::unicode_case::ordinal_key(&member.path))
                    && member.bytes <= MAX_MEMBER
                    && (!member.directory || (member.bytes == 0 && member.sha256.is_empty())),
                "Namespace compiled manifest aliases or exceeds bound"
            );
            total = total
                .checked_add(member.bytes)
                .context("Namespace manifest size overflow")?;
        }
        ensure!(
            !row.members.is_empty() && row.members.len() <= MAX_ENTRIES && total <= MAX_EXPANDED,
            "Namespace compiled package exceeds bounds"
        );
        let item = Entry {
            row: row.clone(),
            original: original.clone(),
            patch_index,
            deleted,
        };
        ensure!(
            entries.insert(row.guid.clone(), item).is_none(),
            "Duplicate namespace source owner"
        );
    }
    Ok(Compiled { entries })
}
fn compiled() -> Result<&'static Compiled> {
    COMPILED
        .get_or_init(|| load_compiled().map_err(|e| format!("{e:#}")))
        .as_ref()
        .map_err(|e| anyhow::anyhow!("{e}"))
}
pub(crate) fn has_owner(guid: &str) -> Result<bool> {
    Ok(compiled()?.entries.contains_key(guid)
        || (programs()?.contains_key(guid)
            && !super::source_plan_patcher::has_admitted_owner(guid)?))
}
/// Definition comparison only; not an archive, execution or completion witness.
pub(crate) fn declared_original_definition(component: &Component) -> Result<Option<Component>> {
    let Some(binding) = component.metadata.get(BINDING) else {
        return Ok(None);
    };
    if binding["Schema"] == 2 {
        let entry = programs()?
            .get(&component.guid)
            .context("Unknown original program declaration")?;
        ensure!(
            binding["AuthoritySha256"] == program_authority()?
                && binding["OriginalSourceChoice"]["OriginalComponent"]
                    == serde_json::to_value(&entry.original)?
                && binding["OriginalSourceChoice"]["RawOriginalComponent"] == entry.raw
                && source_revisions::source_definition_matches(
                    entry,
                    &binding["OriginalSourceChoice"]["SourceProgram"]
                )?,
            "Original program definition declaration changed"
        );
        return Ok(Some(entry.original.clone()));
    }
    let entry = compiled()?
        .entries
        .get(&component.guid)
        .context("Unknown namespace source declaration")?;
    ensure!(
        binding["AuthoritySha256"] == DATA_SHA
            && binding["OriginalSourceChoice"] == serde_json::to_value(&entry.row)?,
        "Namespace original definition declaration changed"
    );
    Ok(Some(entry.original.clone()))
}

pub(crate) fn requires_standalone_k2_profile(component: &Component) -> bool {
    own(component) && component.guid == "403daebc-dd7f-892c-ac37-823cf1ae6600"
}

/// Authenticate the original no-M4 guide inventory, rather than a profile label.
pub(crate) fn verify_standalone_k2_guide(guide: &Guide) -> Result<()> {
    let (document, digest, _, _, count) = GUIDES[1];
    ensure!(
        sha(document) == digest,
        "Original standalone K2 guide pin differs"
    );
    let original = Guide::parse(std::str::from_utf8(document)?, "md")?;
    ensure!(
        original.components.len() == count
            && guide.components.len() == count
            && guide
                .components
                .iter()
                .zip(&original.components)
                .all(|(actual, original)| actual.guid == original.guid),
        "Enclave requires the complete original K2 standalone no-M4 guide inventory/order"
    );
    for guid in [
        crate::vendor_installer::GUID,
        "0d645887-5b3a-82a1-925e-cdd7c68bf923",
    ] {
        ensure!(
            guide
                .components
                .iter()
                .any(|c| c.guid == guid && c.is_selected),
            "Enclave requires selected original TSLRCM and PartySwap predecessors"
        );
    }
    Ok(())
}

/// Identities from compiled Rust authority, never caller-supplied JSON.
pub(crate) fn required_predecessors(component: &Component) -> Result<Vec<String>> {
    if !own(component) {
        return Ok(Vec::new());
    }
    Ok(supplemental::requirements(&component.guid)?
        .into_iter()
        .map(|r| match r {
            supplemental::Requirement::K1HighResolutionMenus { owner_guid, .. }
            | supplemental::Requirement::CloakedJediRobes { owner_guid, .. }
            | supplemental::Requirement::CompletedOriginalProgram { owner_guid }
            | supplemental::Requirement::TslrcmStandalone { owner_guid, .. } => owner_guid,
        })
        .collect())
}

/// Completion is checked by the engine. Independently authenticate the completed
/// definition and exact source choice, including fresh archive/INI admission.
pub(crate) fn verify_predecessor(
    owner: &Component,
    predecessor: &Component,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<()> {
    ensure!(
        required_predecessors(owner)?
            .iter()
            .any(|guid| guid == &predecessor.guid),
        "Supplemental completed predecessor definition has a different exact identity"
    );
    for requirement in supplemental::requirements(&owner.guid)? {
        match requirement {
            supplemental::Requirement::CompletedOriginalProgram { owner_guid } => {
                if predecessor.guid != owner_guid {
                    continue;
                }
                let proof = verify_runtime(predecessor, root, options, cancelled)?.context(
                    "Supplemental requires the completed exact original predecessor program",
                )?;
                ensure!(
                    predecessor.guid == "0d645887-5b3a-82a1-925e-cdd7c68bf923"
                        && proof.runs.len() == 3
                        && proof
                            .runs
                            .values()
                            .map(|run| run.namespace())
                            .collect::<Vec<_>>()
                            == vec![Some(0), Some(2), None],
                    "Enclave requires exact whole PartySwap program namespaces0+2 and Kreia compatibility patch"
                );
            }
            supplemental::Requirement::TslrcmStandalone {
                owner_guid,
                version,
            } => {
                if predecessor.guid != owner_guid {
                    continue;
                }
                ensure!(
                    owner_guid == crate::vendor_installer::GUID && version == "1.8.6",
                    "Enclave TSLRCM prerequisite identity/version differs"
                );
                crate::vendor_installer::replay(predecessor, root, cancelled)?;
            }
            supplemental::Requirement::K1HighResolutionMenus {
                owner_guid,
                profile_id,
            } => {
                if predecessor.guid != owner_guid {
                    continue;
                }
                ensure!(
                    profile_id == crate::k1_widescreen::PROFILE_ID,
                    "Supplemental HRMenus identity/profile differs"
                );
                let proof = crate::k1_widescreen_recipe::verify_runtime(
                    predecessor,
                    root,
                    options,
                    cancelled,
                )?
                .context("Supplemental requires authenticated HRMenus source choice")?;
                ensure!(
                    proof.role() == crate::k1_widescreen_recipe::Role::Hrmenus,
                    "Supplemental requires completed HRMenus1080p profile"
                );
            }
            supplemental::Requirement::CloakedJediRobes {
                owner_guid,
                archive_sha256,
                namespace_index,
                namespace_id,
                data_path,
                ini_path,
                ini_sha256,
                namespaces_sha256,
            } => {
                if predecessor.guid != owner_guid {
                    continue;
                }
                let proof = verify_runtime(predecessor, root, options, cancelled)?
                    .context("Supplemental requires authenticated Cloaked source choice")?;
                ensure!(
                    proof.runs.len() == 1,
                    "Supplemental Cloaked run count differs"
                );
                let run = proof
                    .runs
                    .values()
                    .next()
                    .context("Cloaked native run absent")?;
                let namespace = run
                    .row
                    .namespace
                    .as_ref()
                    .context("Cloaked namespace absent")?;
                ensure!(
                    run.row.archive.sha256 == archive_sha256
                        && namespace.index == namespace_index
                        && namespace.id == namespace_id
                        && namespace.data_path == data_path
                        && namespace.map.sha256 == namespaces_sha256
                        && run.selected_ini_relative_path() == Some(ini_path.as_str())
                        && run.row.ini.sha256 == ini_sha256,
                    "Supplemental requires exact completed Cloaked namespace4 k_brb-a source choice"
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn eligible_for_fallback(guid: &str) -> Result<bool> {
    Ok(programs()?.contains_key(guid) && super::source_plan_patcher::has_admitted_owner(guid)?)
}
pub(crate) fn allows_literal_filename(guid: &str, filename: &str) -> Result<bool> {
    Ok(compiled()?
        .entries
        .get(guid)
        .is_some_and(|entry| entry.row.archive.filename == filename)
        || programs()?
            .get(guid)
            .is_some_and(|entry| entry.inputs.contains_key(filename)))
}

/// A byte-pinned legacy INI translation; this does not grant source authority.
pub(crate) fn effective_ini_text(guid: &str, raw: &str) -> Result<Option<String>> {
    inactive_sections::effective_swoop_ini(guid, raw)
}
/// Interpret the byte-pinned HQ INI after the caller authenticates its source program.
pub(crate) fn effective_hq_ini_text(raw: &str) -> Result<String> {
    inactive_sections::effective_hq_ini(raw)
}
/// Validate the entire original authored program and its freshly replayed
/// pre-run deletion before interpreting the one intentionally absent HQ source.
fn hq_native_ini(
    row: &Row,
    original: &Component,
    package: &Path,
    raw: &str,
    cancelled: &AtomicBool,
) -> Result<Option<(String, Value)>> {
    const GUID: &str = "97c5f09c-0521-82df-beec-0896382e83f9";
    if row.guid != GUID {
        return Ok(None);
    }
    let compiled = programs()?
        .get(GUID)
        .context("HQ original source program absent")?;
    ensure!(
        row.game == "k1"
            && row.archive.filename == "High Quality Blasters 1.1.zip"
            && row.archive.sha256
                == "7450288a232020e2b7ea7b408cb0d19e86783b364d57b1f3de4c1b5939671f63"
            && row.archive.bytes == 24860026
            && row.ini.path == "High Quality Blasters 1.1/tslpatchdata/changes.ini"
            && row.ini.sha256 == "17084e0e86b8de5d18b6118c8f33150dce9dd99fbfd4c7d659f3c4cd89ccaa43"
            && row.ini.bytes == 9650
            && row.namespace.is_none()
            && row.step_id == "step-137"
            && compiled.step["id"] == "step-137"
            && row.original_step == compiled.step
            && row.component_index == compiled.index
            && row.step_index == compiled.index
            && row.raw_component == compiled.raw
            && row.canonical_component == serde_json::to_value(&compiled.original)?
            && serde_json::to_value(original)? == serde_json::to_value(&compiled.original)?
            && compiled.step["operations"][1]
                == json!({"type":"delete","path":"{extract}/step-137/1/High Quality Blasters 1.1/tslpatchdata/keblastore.utm"})
            && compiled.step["operations"][2]["type"] == "gui_patcher",
        "HQ source omission lacks the entire exact original program authority"
    );
    let missing = "High Quality Blasters 1.1/tslpatchdata/keblastore.utm";
    ensure!(
        !row.members
            .iter()
            .any(|member| member.path.eq_ignore_ascii_case(missing)),
        "HQ source omission is absent from the replayed inventory"
    );
    check(cancelled)?;
    ordinary(&fs::symlink_metadata(package)?, true)?;
    let data = package.join("tslpatchdata");
    ordinary(&fs::symlink_metadata(&data)?, true)?;
    for member in fs::read_dir(&data)? {
        check(cancelled)?;
        ensure!(
            !member?.file_name().eq_ignore_ascii_case("keblastore.utm"),
            "HQ author-deleted source remains in the fresh native package"
        );
    }
    let effective = inactive_sections::effective_hq_ini(raw)?;
    let receipt = json!({"Policy":"exact-hq-author-deleted-store-source-v1",
        "OriginalDeleteOperation":1,"OriginalNativeOperation":2,
        "RawIniSha256":row.ini.sha256,"EffectiveIniSha256":sha(effective.as_bytes()),
        "OmittedGffListEntry":"File6=keblastore.utm","OmittedClosedSection":"keblastore.utm",
        "OriginalGuideExpectedMissingSourceErrors":1,"NativeEffectiveMissingSourceErrors":0,
        "OriginalSourceProgramSha256":value_digest(&row.original_step)?});
    Ok(Some((effective, receipt)))
}

pub(crate) fn canonical_context(component: &Component) -> Result<Component> {
    let original = if let Some(entry) = compiled()?.entries.get(&component.guid) {
        &entry.original
    } else {
        &programs()?
            .get(&component.guid)
            .context("Owner outside original program authority")?
            .original
    };
    let mut actual = component.clone();
    actual.is_selected = original.is_selected;
    ensure!(
        serde_json::to_value(actual)? == serde_json::to_value(original)?,
        "Namespace owner differs from complete original source"
    );
    let mut original = original.clone();
    original.is_selected = component.is_selected;
    Ok(original)
}
fn own(component: &Component) -> bool {
    component.metadata.contains_key(BINDING)
        || component
            .metadata
            .get("DraftReview")
            .and_then(|v| v.get("Generator"))
            .and_then(Value::as_str)
            .is_some_and(|g| g == GENERATOR || g == PROGRAM_GENERATOR)
}
pub(crate) fn contains_native(component: &Component) -> bool {
    fn visit(c: &Component, depth: usize) -> bool {
        depth > 64 || own(c) || c.options.iter().any(|o| visit(o, depth + 1))
    }
    visit(component, 0)
}

/// Preserve the Windows-1252 right apostrophe in the exact original Dodonna
/// namespace map and configurations. Every non-ASCII byte is authenticated.
pub(crate) fn decode_ini_bytes(bytes: &[u8]) -> Result<String> {
    const DODONNA: [&str; 3] = [
        "0d5512aff2bb525a920705e2f0572a39f9db2aa2f23d13dd7e8c1829f1b950ef",
        "2390f1a9bc7758759056f57b69f45ccb4fb57d9e2325730f68d2ef6a605235d2",
        "4f6e5e5d4fda6150835ae16042da91e12174972caae253606ff65b7a1965a935",
    ];
    if DODONNA.contains(&sha(bytes).as_str()) {
        ensure!(
            bytes.iter().all(|byte| byte.is_ascii() || *byte == 0x92),
            "Pinned legacy INI contains an unexpected encoding byte"
        );
        return Ok(bytes
            .iter()
            .map(|byte| {
                if *byte == 0x92 {
                    '\u{2019}'
                } else {
                    char::from(*byte)
                }
            })
            .collect());
    }
    odypatcher::patcher::encoding::decode_bytes_with_fallbacks(bytes)
        .map_err(|error| anyhow::anyhow!("Decode selected namespace INI: {error}"))
}

fn read_ini(path: &Path) -> Result<(String, ConfigParser)> {
    let file = crate::paths::open_regular_read(path)?;
    ensure!(
        file.metadata()?.len() <= MAX_INI,
        "Namespace INI exceeds bound"
    );
    let mut bytes = Vec::new();
    file.take(MAX_INI + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_INI, "Namespace INI exceeds bound");
    let text = decode_ini_bytes(&bytes)?;
    let parser =
        ConfigParser::read_string(text.trim_start_matches('\u{feff}'), OptionXform::Identity)
            .map_err(|e| anyhow::anyhow!("Parse selected namespace INI: {e}"))?;
    Ok((text, parser))
}
fn field<'a>(parser: &'a ConfigParser, section: &str, key: &str) -> Option<&'a str> {
    parser
        .sections()
        .find(|s| s.eq_ignore_ascii_case(section))
        .and_then(|s| parser.section(s))?
        .items()
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .and_then(|(_, v)| v.as_deref())
}
fn validate_graph(parser: &ConfigParser) -> Result<()> {
    let mut graph = BTreeMap::new();
    let mut fields_total = 0usize;
    for name in parser.sections() {
        let fields = parser
            .section(name)
            .context("Selected INI section absent")?;
        fields_total = fields_total
            .checked_add(fields.len())
            .context("INI graph size overflow")?;
        ensure!(
            fields_total <= MAX_ENTRIES && name.len() <= 4096,
            "Selected INI graph exceeds bound"
        );
        let mut edges = Vec::new();
        for (key, value) in fields.items() {
            if key.starts_with("AddField") {
                let target = value.as_deref().context("Dynamic namespace AddField")?;
                ensure!(!target.is_empty() && target.len()<=4096
                    && !odypatcher::patcher::reader::ConfigReader::requires_runtime_gff_resolution(target),"Dynamic namespace AddField requires separate admission");
                edges.push(target.to_lowercase());
            }
        }
        ensure!(
            graph
                .insert(name.to_lowercase(), (fields.len() + 1, edges))
                .is_none(),
            "Namespace INI has case-ambiguous sections"
        );
    }
    fn cost(
        name: &str,
        graph: &BTreeMap<String, (usize, Vec<String>)>,
        active: &mut BTreeSet<String>,
        memo: &mut BTreeMap<String, (usize, usize)>,
    ) -> Result<(usize, usize)> {
        if let Some(value) = memo.get(name) {
            return Ok(*value);
        }
        ensure!(
            active.len() < 64 && active.insert(name.into()),
            "Namespace AddField cycle/depth bound"
        );
        let (weight, children) = graph
            .get(name)
            .context("Unknown static namespace AddField")?;
        let mut total = *weight;
        let mut depth = 1;
        for child in children {
            let (count, height) = cost(child, graph, active, memo)?;
            total = total
                .checked_add(count)
                .context("Namespace AddField overflow")?;
            depth = depth.max(height + 1);
            ensure!(
                total <= MAX_ENTRIES && depth <= 64,
                "Namespace AddField expansion exceeds bound"
            );
        }
        active.remove(name);
        memo.insert(name.into(), (total, depth));
        Ok((total, depth))
    }
    let mut memo = BTreeMap::new();
    let referenced: BTreeSet<_> = graph.values().flat_map(|(_, edges)| edges.iter()).collect();
    // Validate every definition, including unreachable cycles and unknown edges.
    for name in graph.keys() {
        cost(name, &graph, &mut BTreeSet::new(), &mut memo)?;
    }
    let mut total = 0usize;
    // Each expansion root already includes its descendants. Counting every
    // descendant again would charge a single authored traversal repeatedly.
    for name in graph.keys().filter(|name| !referenced.contains(name)) {
        total = total
            .checked_add(memo[name].0)
            .context("Namespace graph aggregate overflow")?;
        ensure!(
            total <= MAX_ENTRIES,
            "Namespace AddField aggregate exceeds bound"
        );
    }
    Ok(())
}

fn validate_source_graph(guid: &str, text: &str, parser: &ConfigParser) -> Result<()> {
    if let Some(view) = source_revisions_uniform::effective_graph_text(guid, text)? {
        let view = ConfigParser::read_string(&view, OptionXform::Identity)
            .map_err(|error| anyhow::anyhow!("Parse authenticated active graph: {error}"))?;
        validate_graph(&view)
    } else if let Some(view) = inactive_sections::effective_korriban_graph(guid, text, parser)? {
        let view = ConfigParser::read_string(&view, OptionXform::Identity)
            .map_err(|error| anyhow::anyhow!("Parse authenticated Korriban graph: {error}"))?;
        validate_graph(&view)
    } else {
        validate_graph(parser)
    }
}
/// Resolve only names in the freshly authenticated inventory. Author spelling
/// remains in original steps and namespace IniName/DataPath; pins use true case.
fn inventory_member<'a>(
    members: impl Iterator<Item = &'a Member>,
    authored: &str,
) -> Result<Option<&'a Member>> {
    literal(authored)?;
    let key = crate::unicode_case::ordinal_key(authored);
    let mut matching =
        members.filter(|member| crate::unicode_case::ordinal_key(&member.path) == key);
    let member = matching.next();
    ensure!(
        matching.next().is_none(),
        "Selected source path has ambiguous inventory case aliases"
    );
    Ok(member)
}

fn selected_ini(entry: &Entry, tree: &Path, cancelled: &AtomicBool) -> Result<()> {
    let row = &entry.row;
    if let Some(selection) = &row.namespace {
        same_pin(
            &file_pin(
                &tree.join(literal(&selection.map.path)?),
                &selection.map.path,
                MAX_INI,
                cancelled,
            )?,
            &selection.map,
        )?;
        let (_, parser) = read_ini(&tree.join(literal(&selection.map.path)?))?;
        let namespace_section = parser
            .sections()
            .find(|s| s.eq_ignore_ascii_case("Namespaces"))
            .context("Namespace source map missing")?;
        let fields = parser
            .section(namespace_section)
            .context("Namespace source fields absent")?;
        let entries = CaseInsensitiveDict::from_section(fields);
        let (key, id) = entries
            .items()
            .nth(selection.index)
            .context("Namespace positional index out of range")?;
        ensure!(
            key == selection.key && id == selection.id,
            "Original positional namespace ID differs"
        );
        ensure!(
            field(&parser, &selection.id, "IniName")
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(odypatcher::patcher::namespaces::PatcherNamespace::DEFAULT_INI_FILENAME)
                == selection.ini_name
                && field(&parser, &selection.id, "DataPath").unwrap_or("") == selection.data_path,
            "Namespace selected DataPath/INI differs"
        );
        let expected = Path::new(&selection.map.path)
            .parent()
            .context("Namespace map parent absent")?
            .join(folder(&selection.data_path)?)
            .join(literal(&selection.ini_name)?);
        ensure!(
            inventory_member(
                row.members.iter(),
                &expected.to_string_lossy().replace('\\', "/")
            )?
            .is_some_and(|member| !member.directory && member.path == row.ini.path),
            "Selected namespace INI escaped compiled choice"
        );
    } else {
        let base = Path::new(&row.installer.path)
            .parent()
            .context("Installer parent absent")?;
        ensure!(
            inventory_member(
                row.members.iter(),
                &base
                    .join("tslpatchdata/changes.ini")
                    .to_string_lossy()
                    .replace('\\', "/")
            )?
            .is_some_and(|member| !member.directory && member.path == row.ini.path),
            "Selected GUI source subtree INI differs"
        );
    }
    Ok(())
}

/// Non-serializable witness. Its fields can only be issued by fresh derivation.
#[derive(Clone)]
pub(crate) struct AdmittedNamespace {
    expected: Component,
    row: Row,
    patch_index: usize,
    staged: BTreeMap<String, Member>,
    resources: Vec<super::source_plan_patcher::DestinationResourceRequirement>,
    authority: String,
    run_identity: Option<Value>,
    certified_nss: Option<CertifiedProgramNss>,
}
impl AdmittedNamespace {
    pub(crate) fn component_guid(&self) -> &str {
        &self.expected.guid
    }
    pub(crate) fn expected_game(&self) -> odypatcher::Game {
        if self.row.game == "k1" {
            odypatcher::Game::K1
        } else {
            odypatcher::Game::K2
        }
    }
    pub(crate) fn installer_identity(&self) -> (&str, &str, u64) {
        (
            &self.row.installer.path,
            &self.row.installer.sha256,
            self.row.installer.bytes,
        )
    }
    pub(crate) fn ini_identity(&self) -> (&str, &str, u64) {
        (&self.row.ini.path, &self.row.ini.sha256, self.row.ini.bytes)
    }
    pub(crate) fn namespace(&self) -> Option<usize> {
        self.row.namespace.as_ref().map(|n| n.index)
    }
    pub(crate) fn selected_ini_relative_path(&self) -> Option<&str> {
        let parent = Path::new(&self.row.installer.path).parent()?;
        Path::new(&self.row.ini.path)
            .strip_prefix(parent)
            .ok()?
            .to_str()
    }
    pub(crate) fn selected_ini_name(&self) -> Option<&str> {
        let selected = Path::new(self.selected_ini_relative_path()?);
        let name = selected.file_name()?.to_str()?;
        if selected
            .parent()?
            .to_str()?
            .eq_ignore_ascii_case("tslpatchdata")
        {
            for canonical in ["changes.ini", "info.rtf", "namespaces.ini"] {
                if name.eq_ignore_ascii_case(canonical) {
                    return Some(canonical);
                }
            }
        }
        Some(name)
    }
    pub(crate) fn patch_index(&self) -> usize {
        self.patch_index
    }
    pub(crate) fn inventory_digest(&self) -> Result<String> {
        value_digest(&serde_json::to_value(&self.row.members)?)
    }
    pub(crate) fn authority_digest(&self) -> &str {
        &self.authority
    }
    pub(crate) fn original_program_run(&self) -> Option<&Value> {
        self.run_identity.as_ref()
    }
    /// Available only through the freshly issued original-program witness.
    /// Call before parsing the private native staging INI. External installers
    /// retain their original INI and authored missing-source behavior.
    pub(crate) fn native_ini_translation(
        &self,
        base: &Path,
        raw: &str,
        cancelled: &AtomicBool,
    ) -> Result<Option<(String, Value)>> {
        if self.row.guid != "97c5f09c-0521-82df-beec-0896382e83f9" {
            return Ok(None);
        }
        let identity = self
            .run_identity
            .as_ref()
            .context("HQ native interpretation needs original-program issuance")?;
        ensure!(
            identity["OriginalOperation"] == 2 && identity["StepId"] == "step-137",
            "HQ native interpretation has a different original run ordinal"
        );
        self.verify_staged(base, cancelled)?;
        hq_native_ini(&self.row, &self.expected, base, raw, cancelled)
    }
    pub(crate) fn certified_program_nss(&self) -> Option<&CertifiedProgramNss> {
        self.certified_nss.as_ref()
    }
    pub(crate) fn destination_resources(
        &self,
    ) -> &[super::source_plan_patcher::DestinationResourceRequirement] {
        &self.resources
    }
    pub(crate) fn verify_staged(&self, base: &Path, cancelled: &AtomicBool) -> Result<()> {
        ordinary(&fs::symlink_metadata(base)?, true)?;
        let root = base.canonicalize()?;
        let mut pending = vec![root.clone()];
        let mut seen = BTreeSet::new();
        let mut aliases = BTreeSet::new();
        while let Some(directory) = pending.pop() {
            check(cancelled)?;
            for item in fs::read_dir(directory)? {
                check(cancelled)?;
                let path = item?.path();
                let meta = fs::symlink_metadata(&path)?;
                ordinary(&meta, meta.is_dir())?;
                ensure!(
                    path.canonicalize()?.starts_with(&root),
                    "Namespace staged path escaped root"
                );
                let relative = path
                    .strip_prefix(&root)?
                    .to_str()
                    .context("Non UTF8 staged path")?
                    .replace('\\', "/");
                literal(&relative)?;
                ensure!(
                    seen.len() < MAX_ENTRIES
                        && seen.insert(relative.clone())
                        && aliases.insert(crate::unicode_case::ordinal_key(&relative)),
                    "Namespace staged package aliases"
                );
                let expected = self
                    .staged
                    .get(&relative)
                    .context("Unexpected namespace staged member")?;
                ensure!(
                    meta.is_dir() == expected.directory,
                    "Namespace staged kind differs"
                );
                if expected.directory {
                    pending.push(path)
                } else {
                    same_pin(
                        &file_pin(&path, &relative, MAX_MEMBER, cancelled)?,
                        &Pin {
                            path: relative.clone(),
                            sha256: expected.sha256.clone(),
                            bytes: expected.bytes,
                        },
                    )?;
                }
            }
        }
        ensure!(
            seen.len() == self.staged.len(),
            "Namespace staged package incomplete"
        );
        Ok(())
    }
    pub(crate) fn verify_selected_configuration(
        &self,
        base: &Path,
        namespace: Option<usize>,
        actual_ini: &Path,
        cancelled: &AtomicBool,
    ) -> Result<()> {
        self.verify_selected_path(base, namespace, actual_ini, cancelled)?;
        let pin = file_pin(actual_ini, &self.row.ini.path, MAX_INI, cancelled)?;
        same_pin(&pin, &self.row.ini)
    }
    /// Used after the engine's private logging/discovery normalization. This
    /// checks selection, not an original-byte claim about the normalized INI.
    pub(crate) fn verify_selected_path(
        &self,
        base: &Path,
        namespace: Option<usize>,
        actual_ini: &Path,
        cancelled: &AtomicBool,
    ) -> Result<()> {
        check(cancelled)?;
        ensure!(
            namespace == self.namespace(),
            "Namespace execution index differs from compiled original choice"
        );
        let relative = self
            .selected_ini_relative_path()
            .context("Namespace selected INI path absent")?;
        // Engine discovery canonicalizes only the top-level patch-data directory
        // and its three document names in private staging. Both paths derive
        // from the certified inventory; never search an ambient filesystem.
        let mut normalized: Vec<_> = relative.split('/').map(str::to_owned).collect();
        if normalized
            .first()
            .is_some_and(|name| name.eq_ignore_ascii_case("tslpatchdata"))
        {
            normalized[0] = "tslpatchdata".into();
            if normalized.len() == 2 {
                for name in ["changes.ini", "info.rtf", "namespaces.ini"] {
                    if normalized[1].eq_ignore_ascii_case(name) {
                        normalized[1] = name.into();
                    }
                }
            }
        }
        let original = base.join(literal(relative)?);
        let normalized = base.join(literal(&normalized.join("/"))?);
        let expected = if actual_ini == normalized {
            normalized
        } else {
            original
        };
        ordinary(&fs::symlink_metadata(&expected)?, false)?;
        ensure!(
            expected.canonicalize()? == actual_ini.canonicalize()?,
            "Native session selected a different namespace INI"
        );
        ordinary(&fs::symlink_metadata(actual_ini)?, false)?;
        Ok(())
    }
}

fn derive(
    entry: &Entry,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<AdmittedNamespace> {
    check(cancelled)?;
    let root = super::resolved::local_root(root)?;
    let row = &entry.row;
    let scratch = options
        .scratch_dir
        .clone()
        .unwrap_or_else(std::env::temp_dir);
    ordinary(&fs::symlink_metadata(&scratch)?, true)?;
    let scratch = scratch.canonicalize()?;
    ensure!(
        !scratch.starts_with(&root),
        "Namespace scratch must be outside source tree"
    );
    let mut snapshot = tempfile::Builder::new()
        .prefix("modsync-source-choice-")
        .tempfile_in(&scratch)?;
    let mut input = crate::paths::open_regular_read(&root.join(literal(&row.archive.filename)?))?;
    ordinary(&input.metadata()?, false)?;
    ensure!(
        input.metadata()?.len() == row.archive.bytes,
        "Namespace archive size differs"
    );
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0; 65536];
    loop {
        check(cancelled)?;
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes = bytes
            .checked_add(n as u64)
            .context("Namespace archive size overflow")?;
        ensure!(
            bytes <= row.archive.bytes && bytes <= MAX_ARCHIVE,
            "Namespace archive grew during snapshot"
        );
        hash.update(&buffer[..n]);
        snapshot.write_all(&buffer[..n])?;
    }
    ensure!(
        bytes == row.archive.bytes && format!("{:x}", hash.finalize()) == row.archive.sha256,
        "Namespace archive hash differs from immutable source"
    );
    snapshot.flush()?;
    let mut bounded = options.clone();
    bounded.scratch_dir = Some(scratch.clone());
    bounded.max_expanded_bytes = Some(
        options
            .max_expanded_bytes
            .unwrap_or(MAX_EXPANDED)
            .min(MAX_EXPANDED),
    );
    let (listed, expanded) = archive::list_original_with_size(
        snapshot.path(),
        Path::new(&row.archive.filename),
        &bounded,
        cancelled,
    )?;
    ensure!(
        listed.len() <= MAX_ENTRIES && expanded <= MAX_EXPANDED,
        "Namespace archive inventory exceeds bound"
    );
    let mut inventory = BTreeMap::new();
    let mut aliases = BTreeMap::new();
    for item in listed {
        check(cancelled)?;
        let name = item.path.trim_end_matches('/').to_owned();
        literal(&name)?;
        let mut paths = Vec::new();
        for (at, _) in name.match_indices('/') {
            paths.push((name[..at].to_string(), true));
        }
        paths.push((name, item.is_directory));
        for (path, directory) in paths {
            let key = crate::unicode_case::ordinal_key(&path);
            if let Some(prior) = aliases.get(&key) {
                ensure!(prior == &path, "Namespace archive path aliases");
            } else {
                aliases.insert(key, path.clone());
            }
            if let Some(prior) = inventory.get(&path) {
                let prior: &Member = prior;
                ensure!(
                    prior.directory && directory,
                    "Namespace file/directory collision"
                );
            } else {
                inventory.insert(
                    path.clone(),
                    Member {
                        path,
                        directory,
                        bytes: 0,
                        sha256: String::new(),
                    },
                );
            }
        }
    }
    let expected: BTreeMap<_, _> = row
        .members
        .iter()
        .map(|m| (m.path.clone(), m.clone()))
        .collect();
    ensure!(
        inventory.len() == expected.len()
            && inventory
                .iter()
                .all(|(p, m)| expected.get(p).is_some_and(|e| e.directory == m.directory)),
        "Namespace complete raw member inventory differs"
    );
    let tree = tempfile::Builder::new()
        .prefix("modsync-source-choice-tree-")
        .tempdir_in(&scratch)?;
    for member in row.members.iter().filter(|m| m.directory) {
        fs::create_dir_all(tree.path().join(literal(&member.path)?))?;
    }
    let decoded = archive::unpack(
        snapshot.path(),
        Path::new(&row.archive.filename),
        &bounded,
        cancelled,
    )?;
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for item in decoded {
        check(cancelled)?;
        let name = item
            .path
            .to_str()
            .context("Non UTF8 decoded namespace member")?
            .replace('\\', "/");
        let expected = expected
            .get(&name)
            .context("Unexpected decoded namespace member")?;
        ensure!(
            !expected.directory && seen.insert(name.clone()),
            "Duplicate decoded namespace member"
        );
        same_pin(
            &file_pin(item.file.path(), &name, MAX_MEMBER, cancelled)?,
            &Pin {
                path: name.clone(),
                sha256: expected.sha256.clone(),
                bytes: expected.bytes,
            },
        )?;
        total = total
            .checked_add(expected.bytes)
            .context("Namespace expanded size overflow")?;
        ensure!(total <= MAX_EXPANDED, "Namespace expansion exceeds bound");
        let target = tree.path().join(literal(&name)?);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut incoming = crate::paths::open_regular_read(item.file.path())?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        let mut copied = 0u64;
        let mut copy_hash = Sha256::new();
        loop {
            check(cancelled)?;
            let n = incoming.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            copied += n as u64;
            ensure!(copied <= expected.bytes, "Namespace decoded source changed");
            copy_hash.update(&buffer[..n]);
            output.write_all(&buffer[..n])?;
        }
        ensure!(
            copied == expected.bytes && format!("{:x}", copy_hash.finalize()) == expected.sha256,
            "Namespace decoded source changed during private copy"
        );
    }
    ensure!(
        total == expanded && seen.len() == row.members.iter().filter(|m| !m.directory).count(),
        "Namespace decoded package incomplete"
    );
    // Session discovery currently reads namespace zero before switching to the
    // witnessed index. Bound every packaged INI graph before either private
    // prediction or live Session can encounter an unselected initial variant.
    let mut ini_fields = 0usize;
    for member in row
        .members
        .iter()
        .filter(|member| !member.directory && member.path.to_ascii_lowercase().ends_with(".ini"))
    {
        check(cancelled)?;
        let (_, parser) = read_ini(&tree.path().join(literal(&member.path)?))?;
        for section in parser.sections() {
            ini_fields = ini_fields
                .checked_add(
                    parser
                        .section(section)
                        .context("Packaged INI section absent")?
                        .len(),
                )
                .context("Namespace package INI field count overflow")?;
            ensure!(
                ini_fields <= MAX_ENTRIES,
                "Namespace package INI fields exceed aggregate bound"
            );
        }
        validate_graph(&parser)?;
    }
    same_pin(
        &file_pin(
            &tree.path().join(literal(&row.installer.path)?),
            &row.installer.path,
            MAX_MEMBER,
            cancelled,
        )?,
        &row.installer,
    )?;
    same_pin(
        &file_pin(
            &tree.path().join(literal(&row.ini.path)?),
            &row.ini.path,
            MAX_INI,
            cancelled,
        )?,
        &row.ini,
    )?;
    selected_ini(entry, tree.path(), cancelled)?;
    let (resources, certified_nss) =
        validate_configuration(entry, tree.path(), &root, cancelled, false)?;
    let parent = Path::new(&row.installer.path)
        .parent()
        .context("Namespace installer parent absent")?;
    let mut staged = BTreeMap::new();
    for member in &row.members {
        if entry.deleted.contains(&member.path) {
            continue;
        }
        let path = Path::new(&member.path);
        if let Ok(relative) = path.strip_prefix(parent) {
            if relative.as_os_str().is_empty() {
                continue;
            }
            let relative = relative
                .to_str()
                .context("Non UTF8 namespace staged member")?
                .replace('\\', "/");
            staged.insert(
                relative.clone(),
                Member {
                    path: relative,
                    ..member.clone()
                },
            );
        }
    }
    let mut admitted = AdmittedNamespace {
        expected: entry.original.clone(),
        row: row.clone(),
        patch_index: entry.patch_index,
        staged,
        resources,
        authority: DATA_SHA.into(),
        run_identity: None,
        certified_nss,
    };
    admitted.expected = materialize(entry, &admitted)?;
    Ok(admitted)
}

fn validate_configuration(
    entry: &Entry,
    tree: &Path,
    source_root: &Path,
    cancelled: &AtomicBool,
    allow_program_nss: bool,
) -> Result<(
    Vec<super::source_plan_patcher::DestinationResourceRequirement>,
    Option<CertifiedProgramNss>,
)> {
    let row = &entry.row;
    let (raw_text, raw_parser) = read_ini(&tree.join(literal(&row.ini.path)?))?;
    let package = tree.join(
        Path::new(&row.installer.path)
            .parent()
            .context("Native package root absent")?,
    );
    let hq = hq_native_ini(row, &entry.original, &package, &raw_text, cancelled)?;
    let effective = if let Some((text, _)) = hq {
        Some(text)
    } else {
        effective_ini_text(&row.guid, &raw_text)?
    };
    let (text, parser) = if let Some(text) = effective {
        let parser = ConfigParser::read_string(&text, OptionXform::Identity)
            .map_err(|error| anyhow::anyhow!("Parse translated legacy INI: {error}"))?;
        (text, parser)
    } else {
        (raw_text, raw_parser)
    };
    if original_plan_protocol::legacy_gui_installer(&row.installer.sha256) {
        inactive_sections::validate_legacy_lists(&text)?;
    }
    validate_source_graph(&row.guid, &text, &parser)?;
    for section in parser.sections() {
        if !allow_program_nss && section.eq_ignore_ascii_case("CompileList") {
            ensure!(
                parser
                    .section(section)
                    .context("CompileList absent")?
                    .is_empty(),
                "Selected namespace CompileList needs separate compiler authority"
            );
        }
    }
    let authored_data = Path::new(&row.installer.path)
        .parent()
        .context("Namespace package root absent")?
        .join("tslpatchdata")
        .to_string_lossy()
        .replace('\\', "/");
    let data_member = inventory_member(row.members.iter(), &authored_data)?
        .filter(|member| member.directory)
        .context("Selected patch data directory absent")?;
    let data = tree.join(literal(&data_member.path)?);
    let ini = tree.join(literal(&row.ini.path)?);
    let selected_data = ini.parent().context("Selected INI parent absent")?;
    let mut config = PatcherConfig::new();
    let mut logger = odypatcher::PatchLogger::new();
    config
        .load(&text, selected_data, &mut logger, Some(&data))
        .map_err(|e| anyhow::anyhow!("Read selected namespace config: {e}"))?;
    ensure!(
        logger
            .all_logs
            .iter()
            .all(|l| l.log_type < odypatcher::LogType::Warning),
        "Selected namespace parser reported a warning/error"
    );
    for log in &logger.all_logs {
        if log
            .message
            .starts_with("There are some orphaned ini sections")
        {
            // The patcher's note lists the orphaned sections; a reviewer needs that
            // exact set to classify them, so the refusal carries it.
            ensure!(
                inactive_sections::classify(entry, &parser, &log.message)?
                    || source_revisions_uniform::classify_inactive(entry, &parser, &log.message)?,
                "Selected namespace has unclassified inactive sections: {}",
                log.message.trim().replace('\n', ", ")
            );
        }
    }
    let certified_nss = if allow_program_nss {
        certified_nss::issue(entry, tree, &parser, &config, source_root, cancelled)?
    } else {
        None
    };
    let game = if row.game == "k1" { 1 } else { 2 };
    ensure!(
        (config.game_number.is_none_or(|n| n == game)
            || ((row.disabled_lookup_game_hint
                || (row.guid == "0c2079ec-5d23-8e69-8106-cbed77b3a58d"
                    && row.game == "k1"
                    && row.archive.sha256
                        == "f4f670d319c9e69f76c044ebf6b6a0130a7e725d186f1ee8c5683417262f1ed7"
                    && row.ini.sha256
                        == "2c436590a8d3a8652e41d57bf2d48464d86a1c88eba332bc1ab58721d3cef3e7"))
                && field(&parser, "Settings", "LookupGameFolder") == Some("0")
                && (config.patches_nss.is_empty() || certified_nss.is_some())))
            && (config.patches_nss.is_empty() || certified_nss.is_some())
            && !config.ignore_file_extensions,
        "Selected namespace game/compiler/extension semantics require separate authority"
    );
    let mut resources = Vec::new();
    let mut operations = 0usize;
    for base in config
        .install_list
        .iter()
        .map(|p| &p.base)
        .chain(config.patches_2da.iter().map(|p| &p.base))
        .chain(config.patches_gff.iter().map(|p| &p.base))
        .chain(config.patches_ncs.iter().map(|p| &p.base))
        .chain(config.patches_ssf.iter().map(|p| &p.base))
    {
        check(cancelled)?;
        operations += 1;
        validate_base_paths(base)?;
        let source = selected_data
            .join(folder(&base.sourcefolder)?)
            .join(literal(&base.sourcefile.replace('\\', "/"))?);
        let relative = source
            .strip_prefix(tree)?
            .to_str()
            .context("Non UTF8 configured source")?;
        let source = inventory_member(row.members.iter(), relative)?
            .filter(|member| !member.directory)
            .map(|member| tree.join(&member.path))
            .unwrap_or(source);
        if source.is_file() {
            ensure!(
                source.canonicalize()?.starts_with(tree),
                "Selected namespace source escaped package"
            );
        } else {
            let destination = literal(&base.destination.replace('\\', "/"))?;
            let module = destination.components().count() == 2
                && destination
                    .components()
                    .next()
                    .is_some_and(|part| part.as_os_str().eq_ignore_ascii_case("modules"))
                && destination
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("mod"));
            let loose = base.destination.eq_ignore_ascii_case("override");
            ensure!(
                !base.replace_file
                    && (module || loose)
                    && literal(&base.sourcefile.replace('\\', "/"))?
                        .components()
                        .count()
                        == 1
                    && literal(&base.saveas.replace('\\', "/"))?
                        .components()
                        .count()
                        == 1,
                "Selected namespace configured source needs a supported current loose/module resource: {}/{}, destination {}, replace {}",
                base.sourcefolder,
                base.sourcefile,
                base.destination,
                base.replace_file
            );
            let mut seed = None;
            if module {
                let candidates: Vec<_> = config
                    .install_list
                    .iter()
                    .filter(|install| {
                        crate::unicode_case::ordinal_key(&format!(
                            "{}/{}",
                            install.base.destination.replace('\\', "/"),
                            install.base.saveas
                        )) == crate::unicode_case::ordinal_key(&base.destination.replace('\\', "/"))
                    })
                    .collect();
                ensure!(candidates.len() <= 1, "Ambiguous preceding module seed");
                if let Some(install) = candidates.first() {
                    let source = selected_data
                        .join(folder(&install.base.sourcefolder)?)
                        .join(literal(&install.base.sourcefile.replace('\\', "/"))?);
                    ensure!(
                        source.canonicalize()?.starts_with(tree),
                        "Preceding module seed escaped package"
                    );
                    let package = tree.join(
                        Path::new(&row.installer.path)
                            .parent()
                            .context("Installer parent absent")?,
                    );
                    let relative = source
                        .strip_prefix(&package)?
                        .to_str()
                        .context("Non UTF8 module seed")?
                        .replace('\\', "/");
                    let pin = file_pin(&source, &relative, MAX_MEMBER, cancelled)?;
                    let mut bytes = Vec::new();
                    crate::paths::open_regular_read(&source)?
                        .take(MAX_MEMBER + 1)
                        .read_to_end(&mut bytes)?;
                    ensure!(
                        bytes.len() as u64 == pin.bytes && sha(&bytes) == pin.sha256,
                        "Preceding module seed changed"
                    );
                    let resource = super::source_plan_patcher::module_resource(
                        &bytes,
                        &base.saveas,
                        cancelled,
                    )?;
                    seed = Some(super::source_plan_patcher::DestinationResourceSeed {
                        package_path: relative,
                        sha256: pin.sha256,
                        bytes: pin.bytes,
                        replace_file: install.base.replace_file,
                        resource_sha256: sha(resource),
                        resource_bytes: resource.len() as u64,
                    });
                }
            }
            resources.push(super::source_plan_patcher::DestinationResourceRequirement {
                sourcefile: base.sourcefile.clone(),
                sourcefolder: base.sourcefolder.clone(),
                destination: base.destination.replace('\\', "/"),
                saveas: base.saveas.clone(),
                seed,
            });
        }
    }
    // NSS sources and destinations are checked by the private issuer, never
    // admitted through the current-resource fallback for other patch classes.
    operations += config.patches_nss.len();
    operations += usize::from(!config.patches_tlk.modifiers.is_empty());
    ensure!(
        operations > 0 && operations <= MAX_ENTRIES,
        "Selected namespace operation inventory exceeds bound"
    );
    for modifier in &config.patches_tlk.modifiers {
        if let Some(path) = modifier.tlk_filepath() {
            ensure!(
                path.is_file() && path.canonicalize()?.starts_with(tree),
                "Selected namespace TLK input is absent or escaped exact package"
            );
        }
    }
    Ok((resources, certified_nss))
}
fn validate_base_paths(base: &ModBase) -> Result<()> {
    folder(&base.sourcefolder)?;
    literal(&base.sourcefile.replace('\\', "/"))?;
    literal(&base.saveas.replace('\\', "/"))?;
    folder(&base.destination)?;
    Ok(())
}
fn instruction(
    owner: &Component,
    index: usize,
    action: &str,
    source: String,
    destination: String,
    arguments: String,
    overwrite: bool,
) -> Instruction {
    Instruction {
        guid: crate::markdown::generated_id(
            &format!("{}:source-selected-namespace-v1", owner.guid),
            index,
        ),
        parent: owner.guid.clone(),
        action: action.into(),
        source: vec![source],
        destination,
        arguments,
        overwrite,
        dependencies: vec![],
        restrictions: vec![],
    }
}
fn materialize(entry: &Entry, admitted: &AdmittedNamespace) -> Result<Component> {
    let mut owner = entry.original.clone();
    let slot = format!("<<modDirectory>>/.modsync-reviewed-{}-slot0", owner.guid);
    owner.instructions = vec![instruction(
        &owner,
        0,
        "ExtractTo",
        format!("<<modDirectory>>/{}", entry.row.archive.filename),
        slot.clone(),
        String::new(),
        false,
    )];
    for deleted in &entry.deleted {
        let index = owner.instructions.len();
        owner.instructions.push(instruction(
            &owner,
            index,
            "Delete",
            format!("{slot}/{deleted}"),
            String::new(),
            String::new(),
            true,
        ));
    }
    let arguments = entry
        .row
        .namespace
        .as_ref()
        .map(|n| n.index.to_string())
        .unwrap_or_else(|| "changes.ini".into());
    owner.instructions.push(instruction(
        &owner,
        entry.patch_index,
        "Patcher",
        format!("{slot}/{}", entry.row.installer.path),
        "<<kotorDirectory>>".into(),
        arguments,
        false,
    ));
    let resources:Vec<_>=admitted.resources.iter().map(|r|json!({"SourceFile":r.sourcefile,"SourceFolder":r.sourcefolder,"Destination":r.destination,"SaveAs":r.saveas,"SourceMode":"current-predecessor-module-no-key-bif-fallback"})).collect();
    let derivation = json!({"Filename":entry.row.archive.filename,"Sha256":entry.row.archive.sha256,"Bytes":entry.row.archive.bytes});
    owner.metadata.insert(BINDING.into(),json!({"Schema":1,"Policy":POLICY,"AuthoritySha256":DATA_SHA,"OriginalSourceChoice":entry.row,
        "InventorySha256":admitted.inventory_digest()?,"DestinationResourceSources":resources,"BuiltinBackendRequired":true,"ExactPredictionRequired":true,
        "NativeOutcomeContract":"native-zero-warnings-changed-game-v1"}));
    owner.metadata.insert("DraftReview".into(),json!({"RequiresReview":true,"Generator":GENERATOR,"Derivations":[derivation],"Result":draft::DraftResult{component_guid:owner.guid.clone(),draft_instruction_count:owner.instructions.len(),unparsed_gaps:vec![],conditional_drafts:vec![],rejected_drafts:0}}));
    ensure!(
        serde_json::to_vec(&owner)?.len() <= 4 * 1024 * 1024,
        "Namespace bound component exceeds model bound"
    );
    Ok(owner)
}
pub(crate) fn bind(
    component: &mut Component,
    filenames: &[String],
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<()> {
    let original = canonical_context(component)?;
    let expected_inputs: Vec<String> = if let Some(entry) = compiled()?.entries.get(&component.guid)
    {
        vec![entry.row.archive.filename.clone()]
    } else {
        programs()?
            .get(&component.guid)
            .context("Unknown original program")?
            .inputs
            .keys()
            .cloned()
            .collect()
    };
    let actual: BTreeSet<_> = filenames.iter().cloned().collect();
    ensure!(
        actual.len() == filenames.len()
            && (actual == expected_inputs.into_iter().collect()
                || source_revisions::alternate_input_group(&component.guid, filenames)?),
        "Original program needs its exhaustive exact input group"
    );
    let mut admitted = derive_program_or_legacy(&component.guid, root, options, cancelled)?;
    admitted.expected.is_selected = original.is_selected;
    *component = admitted.expected;
    Ok(())
}
fn compare(component: &Component, pending: &Component) -> Result<()> {
    ensure!(
        !component.options.iter().any(contains_native),
        "Nested namespace source ownership forbidden"
    );
    let mut actual = serde_json::to_value(component)?;
    let mut expected = serde_json::to_value(pending)?;
    for value in [&mut actual, &mut expected] {
        let object = value.as_object_mut().context("Invalid namespace owner")?;
        object.remove("IsSelected");
        object.remove("DraftReview");
    }
    ensure!(
        actual == expected,
        "Namespace whole component/source/selection/instructions differ from fresh derivation"
    );
    let review = component
        .metadata
        .get("DraftReview")
        .context("Namespace review missing")?;
    for key in ["Generator", "Derivations", "Result"] {
        ensure!(
            review.get(key) == pending.metadata["DraftReview"].get(key),
            "Namespace review diagnostics differ"
        );
    }
    Ok(())
}
pub(crate) fn replay(
    component: &Component,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Component> {
    ensure!(own(component), "Namespace source provenance missing");
    let admitted = derive_program_or_legacy(&component.guid, root, options, cancelled)?;
    compare(component, &admitted.expected)?;
    Ok(admitted.expected)
}
pub(crate) fn verification(pending: &Component) -> Result<Value> {
    ensure!(
        has_owner(&pending.guid)? || eligible_for_fallback(&pending.guid)?,
        "Unknown original program owner"
    );
    let authority = if compiled()?.entries.contains_key(&pending.guid) {
        DATA_SHA.into()
    } else {
        program_authority()?
    };
    let mut value = serde_json::to_value(pending)?;
    value
        .as_object_mut()
        .context("Invalid namespace owner")?
        .remove("IsSelected");
    Ok(
        json!({"Reviewer":draft::review::VERIFIER_NAME,"Kind":draft::review::VERIFICATION_KIND,"ToolVersion":concat!("ModSync.rs/",env!("CARGO_PKG_VERSION")),
        "ArchiveSha256":authority,"ArchiveFilename":"source-plan-selected-namespace-authority-v1","ReplayDigest":value_digest(&value)?,"IdentityVersion":2}),
    )
}
pub(crate) fn verify_runtime(
    component: &Component,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Option<AdmittedProgram>> {
    if !own(component) {
        ensure!(
            !component.options.iter().any(contains_native),
            "Nested namespace binding forbidden"
        );
        return Ok(None);
    }
    let admitted = derive_program_or_legacy(&component.guid, root, options, cancelled)?;
    compare(component, &admitted.expected)?;
    let review = component
        .metadata
        .get("DraftReview")
        .context("Namespace review absent")?;
    const KEYS: [&str; 6] = [
        "RequiresReview",
        "Generator",
        "Derivations",
        "Result",
        "Verification",
        "ReviewedIdentity",
    ];
    ensure!(
        review
            .as_object()
            .is_some_and(|o| o.len() == KEYS.len() && o.keys().all(|k| KEYS.contains(&k.as_str())))
            && review["RequiresReview"] == false
            && review.get("Verification") == Some(&verification(&admitted.expected)?)
            && review["ReviewedIdentity"].as_str()
                == Some(draft::review::identity(component)?.as_str()),
        "Namespace source requires unchanged dedicated independent replay approval"
    );
    Ok(Some(admitted))
}
pub(crate) fn native_patcher_contract(
    component: &Component,
    current: &Instruction,
    operation_index: usize,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Option<super::reviewed_manual::NativePatcherContract>> {
    let Some(mut program) = verify_runtime(component, root, options, cancelled)? else {
        return Ok(None);
    };
    let Some(admitted) = program.runs.remove(&operation_index) else {
        return Ok(None);
    };
    ensure!(
        serde_json::to_value(current)?
            == serde_json::to_value(
                program
                    .expected
                    .instructions
                    .get(operation_index)
                    .context("Native run index absent")?
            )?,
        "Native run instruction differs from its original ordinal"
    );
    Ok(Some(
        super::reviewed_manual::NativePatcherContract::from_source_namespace(admitted)?,
    ))
}

// The complete pinned source program, rather than caller-supplied metadata,
// authorizes this cohort. Inventories below are observations of authenticated
// bytes and are rebuilt at every public admission boundary.
struct ProgramEntry {
    game: String,
    index: usize,
    raw: Value,
    original: Component,
    step: Value,
    inputs: BTreeMap<String, String>,
    program: std::result::Result<original_plan_protocol::Program, String>,
}

/// Derived optional proof fields use absence for None, which survives TOML.
/// Never apply this to an original guide component or original plan document.
fn optional_proof_fields(mut value: Value) -> Value {
    fn visit(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                fields.retain(|_, value| !value.is_null());
                for value in fields.values_mut() {
                    visit(value);
                }
            }
            Value::Array(values) => {
                for value in values {
                    visit(value);
                }
            }
            _ => {}
        }
    }
    visit(&mut value);
    value
}
static PROGRAMS: OnceLock<std::result::Result<BTreeMap<String, ProgramEntry>, String>> =
    OnceLock::new();

fn program_authority() -> Result<String> {
    value_digest(&json!({"Policy":"original-pinned-ordered-program-v1",
        "Sources": GUIDES.iter().map(|(_,g,_,p,n)| json!([g,p,n])).collect::<Vec<_>>(),
        "FactorySha256":sha(include_bytes!("source_plan_namespace.rs")),
        "ProtocolSha256":sha(include_bytes!("source_plan_namespace/original_plan_protocol.rs")),
        "CompilerIssuerSha256":sha(include_bytes!("source_plan_namespace/certified_nss.rs")),
        "InactiveSectionsSha256":sha(include_bytes!("source_plan_namespace/inactive_sections.rs")),
        "CompilerPolicy":certified_nss::policy_json(),
        "SupplementalSourceSha256":sha(include_bytes!("source_plan_namespace/supplemental.rs")),
        "SupplementalAuthoritySha256":sha(include_bytes!("source_plan_namespace/supplemental_authority.json")),
        "SourceRevisionsSha256":sha(include_bytes!("source_plan_namespace/source_revisions.rs")),
        "SourceRevisionsAuthoritySha256":source_revisions::DATA_SHA,
        "UniformRevisionSha256":sha(include_bytes!("source_plan_namespace/source_revisions_uniform.rs")),
        "UniformRevisionAuthoritySha256":source_revisions_uniform::DATA_SHA,
        "EnlightenmentRevisionSha256":sha(include_bytes!("source_plan_namespace/source_revisions_enlightenment.rs")),
        "EnlightenmentRevisionAuthoritySha256":source_revisions_enlightenment::DATA_SHA,
        "MaxArchive":512_u64*1024*1024,"MaxExpanded":MAX_EXPANDED,"MaxEntries":MAX_ENTRIES,
        "MaxMember":MAX_MEMBER,"MaxIni":MAX_INI,"NativeContract":"native-zero-warnings-changed-game-v1"}))
}
fn programs() -> Result<&'static BTreeMap<String, ProgramEntry>> {
    PROGRAMS
        .get_or_init(|| load_programs().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| anyhow::anyhow!(e.clone()))
}
fn load_programs() -> Result<BTreeMap<String, ProgramEntry>> {
    let mut result = BTreeMap::new();
    for (game_index, (guide, guide_sha, plan, plan_sha, count)) in GUIDES.iter().enumerate() {
        ensure!(
            sha(guide) == *guide_sha && sha(plan) == *plan_sha,
            "Original program source bytes changed"
        );
        let mut raw = Guide::parse(std::str::from_utf8(guide)?, "md")?;
        draft::generate(&mut raw);
        let canonical = Guide::parse_preserving_selection(&serde_json::to_string(&raw)?, "json")?;
        ensure!(
            raw.components.len() == *count && canonical.components.len() == *count,
            "Original program owner inventory changed"
        );
        let plan: Value = serde_json::from_slice(plan)?;
        ensure!(
            plan["guide_sha256"] == *guide_sha,
            "Original program guide pin mismatch"
        );
        let hashes: BTreeMap<String, String> =
            serde_json::from_value(plan["archive_sha256"].clone())?;
        let steps = plan["steps"]
            .as_array()
            .context("Original program steps absent")?;
        ensure!(
            steps.len() <= *count,
            "Original program exceeds source owners"
        );
        for (index, step) in steps.iter().enumerate() {
            let original = &canonical.components[index];
            let Some(operations) = step["operations"].as_array() else {
                continue;
            };
            let native = operations
                .iter()
                .any(|o| matches!(o["type"].as_str(), Some("patcher" | "gui_patcher")));
            let file_owner = (game_index == 0 && [40, 149].contains(&index))
                || (game_index == 1 && [94, 135].contains(&index));
            if !(native || file_owner)
                || original.guid == crate::vendor_installer::GUID
                || compiled()?.entries.contains_key(&original.guid)
                || super::reviewed_manual::has_recipe(&original.guid)?
            {
                continue;
            }
            let heading = original
                .metadata
                .get("Heading")
                .and_then(Value::as_str)
                .unwrap_or(&original.name);
            ensure!(
                step["id"] == format!("step-{:03}", index + 1)
                    && step["source_heading"] == format!("### {heading}"),
                "Original program ordinal or heading differs"
            );
            let mut inputs = BTreeMap::new();
            for op in operations {
                if let Some(path) = op
                    .get("source")
                    .and_then(Value::as_str)
                    .filter(|p| p.starts_with("{archives}/"))
                {
                    let name = token(path, "archives")?;
                    ensure!(
                        literal(&name)?.components().count() == 1,
                        "Original input must be flat"
                    );
                    let hash = hashes
                        .get(path)
                        .context("Original program input lacks source pin")?;
                    inputs.insert(name, hash.clone());
                }
            }
            let program = original_plan_protocol::compile(step, &hashes).map_err(|e| e.to_string());
            ensure!(
                result
                    .insert(
                        original.guid.clone(),
                        ProgramEntry {
                            game: if game_index == 0 { "k1" } else { "k2" }.into(),
                            index,
                            raw: serde_json::to_value(&raw.components[index])?,
                            original: original.clone(),
                            step: step.clone(),
                            inputs,
                            program
                        }
                    )
                    .is_none(),
                "Duplicate original program GUID"
            );
        }
    }
    for mut entry in supplemental::entries()? {
        // This is our authored supplemental descriptor, not an original plan.
        entry.step = optional_proof_fields(entry.step);
        ensure!(
            !compiled()?.entries.contains_key(&entry.original.guid)
                && !super::source_plan_patcher::has_admitted_owner(&entry.original.guid)?
                && !super::reviewed_manual::has_recipe(&entry.original.guid)?,
            "Supplemental authority conflicts with existing source owner"
        );
        if let Some(existing) = result.get(&entry.original.guid) {
            ensure!(
                entry.game == "k2"
                    && entry.index == 94
                    && entry.original.guid == "403daebc-dd7f-892c-ac37-823cf1ae6600"
                    && existing.game == entry.game
                    && existing.index == entry.index
                    && existing.raw == entry.raw
                    && serde_json::to_value(&existing.original)?
                        == serde_json::to_value(&entry.original)?
                    && existing.step
                        == json!({
                            "id":"step-095", "source_heading":"### Extended Enclave",
                            "operations":[{"type":"extract",
                                "source":"{archives}/Extended Enclave.2.5.2.rar",
                                "destination":"{extract}/step-095/1"}]
                        }),
                "Supplemental replacement is not the exact original extraction-only Enclave owner"
            );
        }
        result.insert(entry.original.guid.clone(), entry);
    }

    Ok(result)
}

pub(crate) struct AdmittedProgram {
    expected: Component,
    runs: BTreeMap<usize, AdmittedNamespace>,
}
fn derive_program_or_legacy(
    guid: &str,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<AdmittedProgram> {
    if let Some(entry) = compiled()?.entries.get(guid) {
        let run = derive(entry, root, options, cancelled)?;
        return Ok(AdmittedProgram {
            expected: run.expected.clone(),
            runs: BTreeMap::from([(run.patch_index, run)]),
        });
    }
    let entry = programs()?
        .get(guid)
        .context("Unknown original program owner")?;
    let revision = source_revisions::select(entry, root, cancelled)?;
    derive_program(revision.as_ref().unwrap_or(entry), root, options, cancelled)
}
struct ProgramSlot {
    tree: tempfile::TempDir,
    members: BTreeMap<String, Member>,
    archive: ArchivePin,
    native: String,
}
fn snapshot_original(
    root: &Path,
    filename: &str,
    digest: &str,
    scratch: &Path,
    cancelled: &AtomicBool,
) -> Result<(tempfile::NamedTempFile, ArchivePin)> {
    ensure!(
        digest.len() == 64
            && digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "Invalid compiled original input hash"
    );
    let source = root.join(literal(filename)?);
    let mut input = crate::paths::open_regular_read(&source)?;
    ordinary(&input.metadata()?, false)?;
    let limit = input.metadata()?.len();
    ensure!(
        limit > 0 && limit <= 512 * 1024 * 1024,
        "Original input exceeds snapshot bound"
    );
    let mut snapshot = tempfile::Builder::new()
        .prefix("modsync-program-input-")
        .tempfile_in(scratch)?;
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0; 65536];
    loop {
        check(cancelled)?;
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        ensure!(bytes <= limit, "Original input grew during snapshot");
        hash.update(&buffer[..n]);
        snapshot.write_all(&buffer[..n])?;
    }
    ensure!(
        bytes == limit && format!("{:x}", hash.finalize()) == digest,
        "Original program input hash mismatch"
    );
    snapshot.flush()?;
    Ok((
        snapshot,
        ArchivePin {
            filename: filename.into(),
            sha256: digest.into(),
            bytes,
        },
    ))
}
fn decode_program_slot(
    snapshot: &Path,
    pin: ArchivePin,
    scratch: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
    native: String,
) -> Result<ProgramSlot> {
    let maximum = source_revisions::expanded_limit(&pin.sha256)?.unwrap_or(MAX_EXPANDED);
    let mut bounded = options.clone();
    bounded.scratch_dir = Some(scratch.to_owned());
    bounded.max_expanded_bytes = Some(options.max_expanded_bytes.unwrap_or(maximum).min(maximum));
    let (listed, total) =
        archive::list_original_with_size(snapshot, Path::new(&pin.filename), &bounded, cancelled)?;
    ensure!(
        listed.len() <= MAX_ENTRIES && total <= maximum,
        "Original program archive exceeds bounds"
    );
    let tree = tempfile::Builder::new()
        .prefix("modsync-program-tree-")
        .tempdir_in(scratch)?;
    let mut members: BTreeMap<String, Member> = BTreeMap::new();
    let mut aliases = BTreeMap::new();
    let mut explicit = BTreeSet::new();
    for item in listed {
        check(cancelled)?;
        let name = item.path.trim_end_matches('/').to_owned();
        literal(&name)?;
        ensure!(
            explicit.insert(crate::unicode_case::ordinal_key(&name)),
            "Duplicate original archive member"
        );
        let mut paths: Vec<_> = name
            .match_indices('/')
            .map(|(i, _)| (name[..i].to_owned(), true))
            .collect();
        paths.push((name, item.is_directory));
        for (path, directory) in paths {
            ensure!(
                Path::new(&path).components().count() <= 64,
                "Original archive path depth exceeds bound"
            );
            let key = crate::unicode_case::ordinal_key(&path);
            if let Some(old) = aliases.insert(key, path.clone()) {
                ensure!(old == path, "Original archive aliases");
            }
            if let Some(old) = members.get(&path) {
                ensure!(
                    old.directory && directory,
                    "Original archive file/directory collision"
                );
            } else {
                ensure!(
                    members.len() < MAX_ENTRIES,
                    "Original canonical inventory exceeds bound"
                );
                members.insert(
                    path.clone(),
                    Member {
                        path,
                        directory,
                        bytes: 0,
                        sha256: String::new(),
                    },
                );
            }
        }
    }
    for m in members.values().filter(|m| m.directory) {
        fs::create_dir_all(tree.path().join(literal(&m.path)?))?;
    }
    let mut seen = BTreeSet::new();
    let mut expanded = 0u64;
    for item in archive::unpack(snapshot, Path::new(&pin.filename), &bounded, cancelled)? {
        check(cancelled)?;
        let name = item
            .path
            .to_str()
            .context("Non UTF8 original archive member")?
            .replace('\\', "/");
        let member = members
            .get_mut(&name)
            .context("Decoded original archive member not listed")?;
        ensure!(
            !member.directory && seen.insert(name.clone()),
            "Duplicate original decoded member"
        );
        let mut input = crate::paths::open_regular_read(item.file.path())?;
        let before = input.metadata()?;
        ordinary(&before, false)?;
        ensure!(
            before.len() <= MAX_MEMBER,
            "Decoded original member exceeds bound"
        );
        let target = tree.path().join(literal(&name)?);
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        let mut copied = 0u64;
        let mut digest = Sha256::new();
        let mut buffer = [0; 65536];
        loop {
            check(cancelled)?;
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            copied += n as u64;
            ensure!(
                copied <= before.len(),
                "Decoded original member grew during snapshot"
            );
            digest.update(&buffer[..n]);
            output.write_all(&buffer[..n])?;
        }
        let after = input.metadata()?;
        ensure!(
            copied == before.len()
                && after.len() == before.len()
                && after.modified().ok() == before.modified().ok(),
            "Decoded original member changed during snapshot"
        );
        expanded = expanded.checked_add(copied).context("Expansion overflow")?;
        ensure!(expanded <= maximum, "Original expansion exceeds bound");
        member.bytes = copied;
        member.sha256 = format!("{:x}", digest.finalize());
    }
    ensure!(
        expanded == total && seen.len() == members.values().filter(|m| !m.directory).count(),
        "Original archive decoding incomplete"
    );
    Ok(ProgramSlot {
        tree,
        members,
        archive: pin,
        native,
    })
}
fn program_extract<'a>(
    value: &str,
    slots: &'a BTreeMap<String, ProgramSlot>,
) -> Result<(&'a str, &'a ProgramSlot, String)> {
    let relative = token(value, "extract")?;
    slots
        .iter()
        .filter_map(|(slot, data)| {
            if relative == *slot {
                Some((slot.as_str(), data, String::new()))
            } else {
                relative
                    .strip_prefix(&format!("{slot}/"))
                    .map(|tail| (slot.as_str(), data, tail.to_owned()))
            }
        })
        .max_by_key(|(s, _, _)| s.len())
        .context("Original operation references an unavailable extraction slot")
}
fn lower_path(value: &str, slots: &BTreeMap<String, ProgramSlot>) -> Result<String> {
    if value == "{game}" {
        return Ok("<<kotorDirectory>>".into());
    }
    if value.starts_with("{game}/") {
        return Ok(format!("<<kotorDirectory>>/{}", token(value, "game")?));
    }
    if value.starts_with("{archives}/") {
        return Ok(format!("<<modDirectory>>/{}", token(value, "archives")?));
    }
    let (_, slot, tail) = program_extract(value, slots)?;
    Ok(if tail.is_empty() {
        slot.native.clone()
    } else {
        format!("{}/{tail}", slot.native)
    })
}
fn program_namespace(
    slot: &ProgramSlot,
    installer: &str,
    selection: Option<usize>,
) -> Result<(Option<Namespace>, String)> {
    let parent = Path::new(installer)
        .parent()
        .context("Original installer has no parent")?;
    let map_path = parent
        .join("tslpatchdata/namespaces.ini")
        .to_string_lossy()
        .replace('\\', "/");
    let map = inventory_member(slot.members.values(), &map_path)?;
    if let Some(index) = selection {
        let map = map
            .filter(|member| !member.directory)
            .context("Original namespace map absent")?;
        let map_path = map.path.clone();
        let (_, parser) = read_ini(&slot.tree.path().join(literal(&map_path)?))?;
        validate_graph(&parser)?;
        let section = parser
            .sections()
            .find(|s| s.eq_ignore_ascii_case("Namespaces"))
            .context("Original namespace list absent")?;
        let section_fields = parser.section(section).context("Namespace list absent")?;
        let mut field_names = BTreeSet::new();
        ensure!(
            section_fields
                .keys()
                .all(|key| field_names.insert(crate::unicode_case::ordinal_key(key))),
            "Namespace list has case aliases"
        );
        let fields = CaseInsensitiveDict::from_section(section_fields);
        ensure!(
            fields.items().count() <= 256,
            "Namespace list exceeds bound"
        );
        let (key, id) = fields
            .items()
            .nth(index)
            .context("Original namespace selection out of range")?;
        let ini_name = field(&parser, id, "IniName")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(odypatcher::patcher::namespaces::PatcherNamespace::DEFAULT_INI_FILENAME)
            .to_owned();
        let data_path = field(&parser, id, "DataPath").unwrap_or("").to_owned();
        let ini = Path::new(&map_path)
            .parent()
            .context("Namespace map parent absent")?
            .join(folder(&data_path)?)
            .join(literal(&ini_name)?)
            .to_string_lossy()
            .replace('\\', "/");
        let ini = inventory_member(slot.members.values(), &ini)?
            .filter(|member| !member.directory)
            .context("Original selected INI absent")?
            .path
            .clone();
        Ok((
            Some(Namespace {
                index,
                key: key.into(),
                id: id.into(),
                map: Pin {
                    path: map_path,
                    sha256: map.sha256.clone(),
                    bytes: map.bytes,
                },
                ini_name,
                data_path,
            }),
            ini,
        ))
    } else {
        ensure!(
            map.is_none(),
            "Original default protocol has namespace choices but no explicit selected namespace"
        );
        let authored = parent
            .join("tslpatchdata/changes.ini")
            .to_string_lossy()
            .replace('\\', "/");
        let ini = inventory_member(slot.members.values(), &authored)?
            .filter(|member| !member.directory)
            .context("Original selected INI absent")?;
        Ok((None, ini.path.clone()))
    }
}

fn derive_program(
    entry: &ProgramEntry,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<AdmittedProgram> {
    use original_plan_protocol::Effect;
    check(cancelled)?;
    let program = entry
        .program
        .as_ref()
        .map_err(|e| anyhow::anyhow!("Original source program is held: {e}"))?;
    ensure!(
        entry
            .original
            .options
            .iter()
            .all(|o| o.instructions.is_empty() && o.options.is_empty()),
        "Original program has active option effects requiring an explicit compiled branch"
    );
    ensure!(
        entry
            .original
            .metadata
            .get("UnresolvedMasters")
            .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty)),
        "Original program has unresolved source prerequisites"
    );
    let root = super::resolved::local_root(root)?;
    let scratch = options
        .scratch_dir
        .clone()
        .unwrap_or_else(std::env::temp_dir);
    ordinary(&fs::symlink_metadata(&scratch)?, true)?;
    let scratch = scratch.canonicalize()?;
    ensure!(
        !scratch.starts_with(&root),
        "Original program scratch must be outside source root"
    );
    let authority = program_authority()?;
    let mut maximum_expanded = MAX_EXPANDED;
    for checksum in entry.inputs.values() {
        if let Some(measured) = source_revisions::expanded_limit(checksum)? {
            maximum_expanded = maximum_expanded.max(measured);
        }
    }
    let mut snapshots = BTreeMap::new();
    let mut input_bytes = 0u64;
    for (name, digest) in &entry.inputs {
        let (snapshot, pin) = snapshot_original(&root, name, digest, &scratch, cancelled)?;
        input_bytes = input_bytes
            .checked_add(pin.bytes)
            .context("Original input byte overflow")?;
        ensure!(
            input_bytes <= 1024 * 1024 * 1024,
            "Original program aggregate input bound exceeded"
        );
        snapshots.insert(name.clone(), (snapshot, pin));
    }
    let mut slots: BTreeMap<String, ProgramSlot> = BTreeMap::new();
    let mut instructions = Vec::new();
    let mut runs = BTreeMap::new();
    let mut inventories = BTreeMap::new();
    let mut effect_trace = Vec::new();
    let mut expanded = 0u64;
    let mut prior_native_logs = BTreeMap::new();
    for effect in &program.effects {
        check(cancelled)?;
        let start = instructions.len();
        match effect {
            Effect::Extract {
                source,
                destination,
                ..
            } => {
                let name = token(source, "archives")?;
                let slot_name = token(destination, "extract")?;
                ensure!(
                    !slots.contains_key(&slot_name),
                    "Original program overwrites an extraction slot"
                );
                let (snapshot, pin) = snapshots
                    .get(&name)
                    .context("Original extraction input is not pinned")?;
                let native = format!(
                    "<<modDirectory>>/.modsync-reviewed-{}-slot{}",
                    entry.original.guid,
                    slots.len()
                );
                let slot = decode_program_slot(
                    snapshot.path(),
                    pin.clone(),
                    &scratch,
                    options,
                    cancelled,
                    native.clone(),
                )?;
                source_revisions::validate_slot(entry, &slot, cancelled)?;
                supplemental::validate_inventory(
                    &entry.original.guid,
                    &pin.filename,
                    &slot
                        .members
                        .values()
                        .filter(|member| !member.directory)
                        .map(|member| member.path.clone())
                        .collect::<Vec<_>>(),
                )?;
                expanded = expanded
                    .checked_add(slot.members.values().map(|m| m.bytes).sum::<u64>())
                    .context("Program expanded size overflow")?;
                ensure!(
                    expanded <= maximum_expanded,
                    "Original program aggregate expansion bound exceeded"
                );
                inventories.insert(
                    slot_name.clone(),
                    json!({"Archive":pin,"Members":slot.members.values().collect::<Vec<_>>() }),
                );
                instructions.push(instruction(
                    &entry.original,
                    instructions.len(),
                    "ExtractTo",
                    lower_path(source, &slots)?,
                    native,
                    String::new(),
                    false,
                ));
                slots.insert(slot_name, slot);
            }
            Effect::Copy {
                source,
                destination,
                skip_existing,
                ..
            } => {
                let target = lower_path(destination, &slots)?;
                ensure!(
                    destination.starts_with("{game}/"),
                    "Original copy destination is not in game"
                );
                let mut copies = Vec::new();
                if source.starts_with("{archives}/") {
                    ensure!(
                        snapshots.contains_key(&token(source, "archives")?),
                        "Original loose input lacks authenticated snapshot"
                    );
                    copies.push((lower_path(source, &slots)?, target));
                } else {
                    let (_, slot, relative) = program_extract(source, &slots)?;
                    if slot.members.get(&relative).is_some_and(|m| !m.directory) {
                        copies.push((lower_path(source, &slots)?, target));
                    } else {
                        ensure!(
                            relative.is_empty()
                                || slot.members.get(&relative).is_some_and(|m| m.directory),
                            "Original copy source absent"
                        );
                        let prefix = if relative.is_empty() {
                            String::new()
                        } else {
                            format!("{relative}/")
                        };
                        for member in slot
                            .members
                            .values()
                            .filter(|m| m.path.starts_with(&prefix))
                        {
                            if member.directory {
                                ensure!(
                                    slot.members.values().any(|m| !m.directory
                                        && m.path.starts_with(&format!("{}/", member.path))),
                                    "Original copy contains unsupported empty directory effect"
                                );
                            } else {
                                copies.push((
                                    format!("{}/{}", slot.native, member.path),
                                    format!(
                                        "{target}/{}",
                                        member
                                            .path
                                            .strip_prefix(&prefix)
                                            .context("Copy prefix mismatch")?
                                    ),
                                ));
                            }
                        }
                        ensure!(!copies.is_empty(), "Original directory copy has no files");
                    }
                }
                for (source, target) in copies {
                    instructions.push(instruction(
                        &entry.original,
                        instructions.len(),
                        "CopyFile",
                        source,
                        target,
                        String::new(),
                        !*skip_existing,
                    ));
                }
            }
            Effect::Delete {
                path,
                missing_ok,
                if_any_exists,
                ordinal,
            } => {
                ensure!(
                    if_any_exists.is_empty(),
                    "Original conditional deletion requires the typed current-VFS predicate capability"
                );
                let source = lower_path(path, &slots)?;
                if path.starts_with("{extract}/") {
                    let (slot_key, slot, relative) = program_extract(path, &slots)?;
                    let slot_key = slot_key.to_owned();
                    ensure!(
                        !relative.is_empty()
                            && !slot.members.get(&relative).is_some_and(|m| m.directory),
                        "Original deletion is not a flat file"
                    );
                    let transient = prior_native_logs.get(path).copied();
                    if !slot.members.contains_key(&relative) && transient.is_some() {
                        effect_trace.push(json!({"OriginalOperation":ordinal,"NativeLogBoundaryAfter":transient,"Reason":"native backend log is private; no source or game file exists to delete","InstructionStart":start,"InstructionCount":0}));
                        continue;
                    }
                    ensure!(
                        slot.members.contains_key(&relative) || *missing_ok,
                        "Original strict deletion source absent"
                    );
                    let slot = slots
                        .get_mut(&slot_key)
                        .context("Extraction slot disappeared")?;
                    if slot.members.remove(&relative).is_some() {
                        fs::remove_file(slot.tree.path().join(literal(&relative)?))?;
                    }
                }
                instructions.push(instruction(
                    &entry.original,
                    instructions.len(),
                    "Delete",
                    source,
                    String::new(),
                    String::new(),
                    !*missing_ok,
                ));
            }
            Effect::Rename {
                source,
                destination,
                ..
            } => {
                ensure!(
                    source.starts_with("{game}/") && destination.starts_with("{game}/"),
                    "Original rename must remain in game"
                );
                let src = token(source, "game")?;
                let dst = token(destination, "game")?;
                ensure!(
                    Path::new(&src).parent() == Path::new(&dst).parent(),
                    "Original rename changes directories"
                );
                let name = Path::new(&dst)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .context("Original rename target absent")?;
                instructions.push(instruction(
                    &entry.original,
                    instructions.len(),
                    "Rename",
                    lower_path(source, &slots)?,
                    name.into(),
                    String::new(),
                    true,
                ));
            }
            Effect::NativeRun {
                patcher,
                sha256,
                namespace,
                ordinal,
                ..
            } => {
                let (slot_name, slot, installer_name) = program_extract(patcher, &slots)?;
                let installer = slot
                    .members
                    .get(&installer_name)
                    .filter(|m| !m.directory)
                    .context("Original native installer absent")?;
                ensure!(
                    installer.sha256 == *sha256,
                    "Original native installer hash differs"
                );
                let (selection, ini_name) = program_namespace(slot, &installer_name, *namespace)?;
                let ini = slot
                    .members
                    .get(&ini_name)
                    .filter(|m| !m.directory)
                    .context("Original selected INI absent")?;
                let mut ini_paths = BTreeSet::from([ini_name.clone()]);
                if namespace.is_some() {
                    ini_paths.insert(program_namespace(slot, &installer_name, Some(0))?.1);
                }
                for name in ini_paths {
                    let (text, parser) = read_ini(&slot.tree.path().join(literal(&name)?))?;
                    validate_source_graph(&entry.original.guid, &text, &parser)?;
                }
                let (_, parser) = read_ini(&slot.tree.path().join(literal(&ini_name)?))?;
                let row = Row {
                    game: entry.game.clone(),
                    component_index: entry.index,
                    guid: entry.original.guid.clone(),
                    source_heading: program.source_heading.clone(),
                    step_index: entry.index,
                    step_id: program.step_id.clone(),
                    source_choice: format!("original operation {ordinal}: {:?}", namespace),
                    archive: slot.archive.clone(),
                    installer: Pin {
                        path: installer_name.clone(),
                        sha256: installer.sha256.clone(),
                        bytes: installer.bytes,
                    },
                    ini: Pin {
                        path: ini_name,
                        sha256: ini.sha256.clone(),
                        bytes: ini.bytes,
                    },
                    namespace: selection,
                    members: slot.members.values().cloned().collect(),
                    raw_component: entry.raw.clone(),
                    canonical_component: serde_json::to_value(&entry.original)?,
                    original_step: entry.step.clone(),
                    disabled_lookup_game_hint: field(&parser, "Settings", "LookupGameFolder")
                        == Some("0"),
                };
                let run_entry = Entry {
                    row: row.clone(),
                    original: entry.original.clone(),
                    patch_index: instructions.len(),
                    deleted: BTreeSet::new(),
                };
                selected_ini(&run_entry, slot.tree.path(), cancelled)?;
                let (resources, certified_nss) =
                    validate_configuration(&run_entry, slot.tree.path(), &root, cancelled, true)?;
                let parent = Path::new(&installer_name)
                    .parent()
                    .context("Original installer parent absent")?;
                let mut staged = BTreeMap::new();
                for member in slot.members.values() {
                    if let Ok(relative) = Path::new(&member.path).strip_prefix(parent)
                        && !relative.as_os_str().is_empty()
                    {
                        let relative = relative
                            .to_str()
                            .context("Non UTF8 run source")?
                            .replace('\\', "/");
                        staged.insert(
                            relative.clone(),
                            Member {
                                path: relative,
                                ..member.clone()
                            },
                        );
                    }
                }
                let run_index = instructions.len();
                let run_identity = optional_proof_fields(
                    json!({"ProgramAuthoritySha256":authority,"OwnerGuid":entry.original.guid,"StepId":program.step_id,"OriginalOperation":ordinal,"InstructionIndex":run_index,"OriginalNamespaceIndex":namespace,"ExtractionSlot":slot_name,"RunOrdinal":runs.len()}),
                );
                let mut run_identity = run_identity;
                let (raw_ini, _) = read_ini(&slot.tree.path().join(literal(&row.ini.path)?))?;
                if let Some((_, interpretation)) = hq_native_ini(
                    &row,
                    &entry.original,
                    &slot.tree.path().join(parent),
                    &raw_ini,
                    cancelled,
                )? {
                    run_identity["NativeIniInterpretation"] = interpretation;
                }
                runs.insert(
                    run_index,
                    AdmittedNamespace {
                        expected: entry.original.clone(),
                        row,
                        patch_index: run_index,
                        staged,
                        resources,
                        authority: authority.clone(),
                        run_identity: Some(run_identity),
                        certified_nss,
                    },
                );
                instructions.push(instruction(
                    &entry.original,
                    run_index,
                    "Patcher",
                    lower_path(patcher, &slots)?,
                    "<<kotorDirectory>>".into(),
                    namespace
                        .map(|i| i.to_string())
                        .unwrap_or_else(|| "changes.ini".into()),
                    false,
                ));
                let original_parent = Path::new(patcher)
                    .parent()
                    .context("Original native path parent absent")?
                    .to_string_lossy();
                for name in ["installlog.rtf", "installlog.txt"] {
                    prior_native_logs.insert(format!("{original_parent}/{name}"), *ordinal);
                }
            }
        }
        ensure!(
            instructions.len() <= MAX_ENTRIES,
            "Original program instruction expansion exceeds bound"
        );
        effect_trace.push(json!({"OriginalOperation":effect.ordinal(),"InstructionStart":start,"InstructionCount":instructions.len()-start}));
    }
    ensure!(
        !instructions.is_empty(),
        "Original program has no executable effects"
    );
    let mut owner = entry.original.clone();
    owner.instructions = instructions;
    let run_records:Vec<_>=runs.values().map(|r|optional_proof_fields(json!({"Identity":r.run_identity,"Installer":r.row.installer,"Ini":r.row.ini,"Namespace":r.row.namespace,"StagedInventory":r.staged.values().collect::<Vec<_>>(),"DestinationResources":r.resources,"CertifiedNss":r.certified_nss.as_ref().map(|n|n.binding_json())}))).collect();
    let compiled_program = optional_proof_fields(serde_json::to_value(program)?);
    let inputs: Vec<_> = snapshots.values().map(|(_, p)| p).collect();
    owner.metadata.insert(BINDING.into(),json!({"Schema":2,"Policy":"original-pinned-ordered-program-v1","AuthoritySha256":authority,"OriginalSourceChoice":{"Game":entry.game,"ComponentIndex":entry.index,"RawOriginalComponent":entry.raw,"OriginalComponent":entry.original,"SourceProgram":entry.step},"Inputs":inputs,"ArchiveInventories":inventories,"CompiledProgram":compiled_program,"EffectTrace":effect_trace,"NativeRuns":run_records,"BuiltinBackendRequired":true,"ExactPredictionRequired":true,"NativeOutcomeContract":"native-zero-warnings-changed-game-v1"}));
    owner.metadata.insert("DraftReview".into(),json!({"RequiresReview":true,"Generator":PROGRAM_GENERATOR,"Derivations":inputs.iter().map(|p|json!({"Filename":p.filename,"Sha256":p.sha256,"Bytes":p.bytes})).collect::<Vec<_>>(),"Result":draft::DraftResult{component_guid:owner.guid.clone(),draft_instruction_count:owner.instructions.len(),unparsed_gaps:vec![],conditional_drafts:vec![],rejected_drafts:0}}));
    ensure!(
        serde_json::to_vec(&owner)?.len() <= 16 * 1024 * 1024,
        "Original program receipt exceeds model bound"
    );
    Ok(AdmittedProgram {
        expected: owner,
        runs,
    })
}
