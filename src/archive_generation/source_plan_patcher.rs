//! Closed original-source native recipes. Archive observations never supply
//! authority: each boundary reconstructs the original owner and derives them anew.
use crate::{Component, Guide, Instruction, archive, draft};
use anyhow::{Context, Result, bail, ensure};
use odypatcher::patcher::{
    config::PatcherConfig,
    ini::{ConfigParser, OptionXform},
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

pub(crate) const GENERATOR: &str = "source-plan-native-patcher-v1";
pub(crate) const BINDING: &str = "SourcePlanPatcherBinding";
const POLICY: &str = "closed-normal-nine-action-empty-directions-v1";
const MAPPING: &[u8] = include_bytes!("source_plan_patcher_authority.json");
const MAPPING_SHA: &str = "88aac6db9661452a9a64e230ec4e795131117dd9a3d246262323214e2fa26700";
const K1_GUIDE: &[u8] = include_bytes!("source_plan_k1.md");
const K2_GUIDE: &[u8] = include_bytes!("source_plan_k2.md");
const K1_PLAN: &[u8] = include_bytes!("../../tools/modsync-ci/fixtures/ci/k1-manual-plan.json");
const K2_PLAN: &[u8] = include_bytes!("../../tools/modsync-ci/fixtures/ci/k2-manual-plan.json");
const K1_GUIDE_SHA: &str = "44067788c6cb7a83b38d468817f273ce5cc901cd5e000c0f9ff65f8af15fc6ed";
const K2_GUIDE_SHA: &str = "f442bdf574031623ed577609730d887069b9bc8e38e8e2f38ecc43a7e7487d5f";
const K1_PLAN_SHA: &str = "1b402ba085625c4863795069eaacaef0f53e8ea7179e56a5bd150d88eb9c0a74";
const K2_PLAN_SHA: &str = "0add8c1aeb7cdfad3df4b21761e135c150aa7832cf3fc0edabe01a91245a9258";
const INSTALLERS: [&str; 3] = [
    "1a267454b2efcd61232332be83f4da4ffe88bd6770c5a0df517880062b1a75e8",
    "013655cd871a71e0f5b50f80ed0d32308912916ba7e12e2a366f42d992a71b16",
    "638aaeea4e40ffb6c592f68787f2140318a0dec7f2cde85908806ce3b5bc5131",
];
const MAX_ARCHIVE: u64 = 512 * 1024 * 1024;
const MAX_EXPANDED: u64 = 512 * 1024 * 1024;
const MAX_MEMBER: u64 = 128 * 1024 * 1024;
const MAX_INI: u64 = 2 * 1024 * 1024;
const MAX_ENTRIES: usize = 10000;
const MANIFEST_CONVENTION: &str =
    "sorted-path-kind-bytes-sha256-explicit-and-implicit-directories-v1";

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SourcePins {
    guide_sha256: String,
    plan_sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Authority {
    schema: u32,
    policy: String,
    scope: String,
    sources: BTreeMap<String, SourcePins>,
    rows: Vec<Row>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Row {
    game: String,
    component_index: usize,
    guid: String,
    heading: String,
    heading_occurrence: usize,
    step_index: usize,
    step_id: String,
    disposition: String,
    semantic_basis: String,
    source_method: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    guide_sha256: String,
    archive_sha256: BTreeMap<String, String>,
    steps: Vec<Value>,
    supplemental_sections: Value,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Step {
    id: String,
    source_heading: String,
    operations: Vec<Operation>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    Extract {
        source: String,
        destination: String,
    },
    GuiPatcher {
        patcher: String,
        sha256: String,
        argv: Vec<String>,
        cwd: String,
        success_log: String,
        success_markers: Vec<String>,
        failure_regex: Vec<String>,
        expected_changes: String,
        actions: Vec<Value>,
        wine_registry: Vec<Value>,
    },
}
#[derive(Clone, Serialize)]
struct Pin {
    path: String,
    sha256: String,
    bytes: u64,
}
#[derive(Clone, Serialize)]
struct Member {
    path: String,
    directory: bool,
    bytes: u64,
    sha256: String,
}
#[derive(Clone)]
struct Entry {
    row: Row,
    raw: Value,
    original: Component,
    step: Step,
    archive: String,
    archive_sha: String,
    installer: String,
    installer_sha: String,
    package: String,
    ini: String,
    source_pins: SourcePins,
}
struct Compiled {
    entries: BTreeMap<String, Entry>,
    digest: String,
}
static COMPILED: OnceLock<std::result::Result<Compiled, String>> = OnceLock::new();

fn check(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Source-plan native derivation cancelled"
    );
    Ok(())
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn value_digest(value: &Value) -> Result<String> {
    fn sort(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                let mut entries: Vec<_> = std::mem::take(fields).into_iter().collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                for (key, mut child) in entries {
                    sort(&mut child);
                    fields.insert(key, child);
                }
            }
            Value::Array(items) => items.iter_mut().for_each(sort),
            _ => {}
        }
    }
    let mut value = value.clone();
    sort(&mut value);
    Ok(sha(&serde_json::to_vec(&value)?))
}
fn literal(value: &str) -> Result<PathBuf> {
    ensure!(
        !value.is_empty()
            && value.len() <= 4096
            && !value.contains(['\\', '*', '?', ':', '<', '>', '"', '{', '}'])
            && !value.chars().any(char::is_control)
            && value.split('/').all(|part| !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])),
        "Unsafe source-plan native literal path"
    );
    ensure!(
        value.split('/').count() <= 64,
        "Native path depth exceeds bound"
    );
    for part in value.split('/') {
        let device = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        ensure!(
            !matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                && !["COM", "LPT"]
                    .iter()
                    .any(
                        |prefix| device.strip_prefix(prefix).is_some_and(|suffix| matches!(
                            suffix,
                            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                        ))
                    ),
            "Native path contains a reserved device name"
        );
    }
    let path = crate::paths::literal_relative(value)?;
    ensure!(
        path.to_str() == Some(value),
        "Noncanonical source-plan native path"
    );
    Ok(path)
}
fn token(value: &str, root: &str) -> Result<String> {
    let name = value
        .strip_prefix(&format!("{{{root}}}/"))
        .context("Wrong source-plan native path root")?;
    literal(name)?;
    Ok(name.into())
}
fn portable(value: &Value, depth: usize) -> Result<()> {
    ensure!(depth <= 64, "Native binding metadata nesting exceeds bound");
    match value {
        Value::Null => bail!("Native source-plan binding cannot encode null metadata"),
        Value::Object(fields) => {
            for child in fields.values() {
                portable(child, depth + 1)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                portable(child, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn original_source(
    guide: &[u8],
    plan: &[u8],
    guide_sha: &str,
    plan_sha: &str,
    count: usize,
) -> Result<(Vec<Value>, Guide, Plan)> {
    ensure!(
        guide.len() <= 1024 * 1024
            && plan.len() <= 8 * 1024 * 1024
            && sha(guide) == guide_sha
            && sha(plan) == plan_sha,
        "Compiled original source-plan bytes differ"
    );
    let mut raw = Guide::parse(std::str::from_utf8(guide)?, "md")?;
    draft::generate(&mut raw);
    let original = raw
        .components
        .iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let canonical = Guide::parse_preserving_selection(&serde_json::to_string(&raw)?, "json")?;
    ensure!(
        canonical.components.len() == count,
        "Original source-plan component inventory differs"
    );
    let plan: Plan = serde_json::from_slice(plan)?;
    ensure!(
        plan.guide_sha256 == guide_sha
            && plan.steps.len() <= 1024
            && plan.supplemental_sections.is_array(),
        "Compiled original plan context differs"
    );
    Ok((original, canonical, plan))
}
fn protocol(
    row: &Row,
    plan: &Plan,
) -> Result<(Step, String, String, String, String, String, String)> {
    let step: Step = serde_json::from_value(
        plan.steps
            .get(row.step_index)
            .context("Missing original native step")?
            .clone(),
    )?;
    ensure!(
        step.id == row.step_id && step.source_heading == row.heading && step.operations.len() == 2,
        "Source-plan native original step mapping differs"
    );
    let Operation::Extract {
        source,
        destination,
    } = &step.operations[0]
    else {
        bail!("Source-plan native first operation must extract");
    };
    let archive = token(source, "archives")?;
    ensure!(
        literal(&archive)?.components().count() == 1,
        "Source-plan native archive must be flat"
    );
    let archive_sha = plan
        .archive_sha256
        .get(source)
        .context("Original archive hash absent")?
        .clone();
    ensure!(
        archive_sha.len() == 64
            && archive_sha
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "Invalid original archive hash"
    );
    let slot = token(destination, "extract")?;
    ensure!(
        slot == format!("{}/1", step.id),
        "Original extraction slot differs"
    );
    let Operation::GuiPatcher {
        patcher,
        sha256,
        argv,
        cwd,
        success_log,
        success_markers,
        failure_regex,
        expected_changes,
        actions,
        wine_registry,
    } = &step.operations[1]
    else {
        bail!("Source-plan native second operation must be the original GUI patcher");
    };
    let installer = token(patcher, "extract")?
        .strip_prefix(&format!("{slot}/"))
        .context("Original installer is outside its extraction")?
        .to_owned();
    literal(&installer)?;
    let package = Path::new(&installer)
        .parent()
        .context("Original installer parent absent")?
        .to_str()
        .context("Non UTF8 installer root")?
        .to_owned();
    let expected_cwd = if package.is_empty() {
        format!("{{extract}}/{slot}")
    } else {
        format!("{{extract}}/{slot}/{package}")
    };
    ensure!(
        INSTALLERS.contains(&sha256.as_str())
            && argv == &["/usr/bin/wine".to_owned(), patcher.clone()]
            && cwd == &expected_cwd
            && success_log == &format!("{expected_cwd}/installlog.rtf")
            && success_markers == &["Done. All changes have been applied.".to_owned()]
            && failure_regex
                == &[
                    "Error: ".to_owned(),
                    "Warning: ".to_owned(),
                    "Done\\. Some changes may have been applied".to_owned()
                ]
            && expected_changes == "changed_manifest"
            && actions.len() == 9,
        "Original native GUI protocol differs"
    );
    let title = actions[0]
        .get("wait_window")
        .and_then(Value::as_str)
        .context("Original window title absent")?;
    ensure!(
        title.len() <= 256 && title.starts_with("^(") && !title.chars().any(char::is_control),
        "Unsupported original installer window title"
    );
    ensure!(
        actions
            == &vec![
                json!({"wait_window":title,"timeout":120}),
                json!({"sleep":3}),
                json!({"click":[502,433],"window":title}),
                json!({"wait_window":"^Browse for Folder$","timeout":120}),
                json!({"sleep":2}),
                json!({"click":[62,69],"window":"^Browse for Folder$"}),
                json!({"sleep":1}),
                json!({"click":[157,274],"window":"^Browse for Folder$"}),
                json!({"wait_log":600})
            ]
            && wine_registry
                == &vec![
                    json!({"key":"HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\User Shell Folders","name":"Desktop","data":"{game_win}"}),
                    json!({"key":"HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Shell Folders","name":"Desktop","data":"{game_win}"})
                ],
        "Original native GUI actions or registry choices differ"
    );
    let ini = if package.is_empty() {
        "tslpatchdata/changes.ini".into()
    } else {
        format!("{package}/tslpatchdata/changes.ini")
    };
    Ok((
        step.clone(),
        archive,
        archive_sha,
        installer,
        sha256.clone(),
        package,
        ini,
    ))
}
// The source-plan generator still refuses every held row. These two installer
// choices have a separate compiled authority preserving their original Directions.
// This exception grants no admission through this factory or to any other book.
fn legacy_owner_is_disjoint_or_exact_held(row: &Row) -> Result<bool> {
    if !super::reviewed_manual::has_recipe(&row.guid)? {
        return Ok(true);
    }
    if row.disposition != "held"
        || !matches!(
            row.guid.as_str(),
            "40840198-65b3-8cbc-aaa1-db49a544e6da" | "4a1e3739-5396-84a6-96c7-5ff99267d498"
        )
    {
        return Ok(false);
    }
    let (filename, sha256) = super::reviewed_manual::verification_identity(&row.guid)?;
    Ok(filename == "reviewed_manual_installer_choice_k2.json"
        && sha256 == "e50408f7108f9c0a6b1829d9cfa70b1004721de6f9162437271aa46e59df9b9b")
}

fn load_compiled() -> Result<Compiled> {
    ensure!(
        MAPPING.len() <= 128 * 1024 && sha(MAPPING) == MAPPING_SHA,
        "Compiled source-plan mapping bytes differ"
    );
    let authority: Authority = serde_json::from_slice(MAPPING)?;
    ensure!(
        authority.schema == 1
            && authority.policy == POLICY
            && authority.scope
                == "semantic-mapping-only-fresh-archive-and-native-admission-required-no-runtime-proof"
            && authority.sources.len() == 2
            && authority.rows.len() == 46,
        "Compiled source-plan mapping scope differs"
    );
    let (raw1, guide1, plan1) = original_source(K1_GUIDE, K1_PLAN, K1_GUIDE_SHA, K1_PLAN_SHA, 197)?;
    let (raw2, guide2, plan2) = original_source(K2_GUIDE, K2_PLAN, K2_GUIDE_SHA, K2_PLAN_SHA, 151)?;
    let mut entries = BTreeMap::new();
    let mut positions = BTreeSet::new();
    let mut admitted = [0, 0];
    for row in authority.rows {
        let (raw, guide, plan, guide_sha, plan_sha, index) = match row.game.as_str() {
            "k1" => (&raw1, &guide1, &plan1, K1_GUIDE_SHA, K1_PLAN_SHA, 0),
            "k2" => (&raw2, &guide2, &plan2, K2_GUIDE_SHA, K2_PLAN_SHA, 1),
            _ => bail!("Unknown source-plan mapping game"),
        };
        let pins = authority
            .sources
            .get(&row.game)
            .context("Source authority pins absent")?;
        ensure!(
            pins.guide_sha256 == guide_sha && pins.plan_sha256 == plan_sha,
            "Source-plan authority source pins differ"
        );
        ensure!(
            positions.insert((row.game.clone(), row.component_index))
                && row.component_index == row.step_index
                && row.step_index < plan.steps.len()
                && row.heading_occurrence > 0
                && row.heading_occurrence <= 1024
                && legacy_owner_is_disjoint_or_exact_held(&row)?,
            "Duplicate, conflicting or legacy-owned source-plan mapping"
        );
        let original = guide
            .components
            .get(row.component_index)
            .context("Original source owner index absent")?
            .clone();
        let heading = original
            .metadata
            .get("Heading")
            .and_then(Value::as_str)
            .unwrap_or(&original.name);
        let occurrence = plan.steps[..=row.step_index]
            .iter()
            .filter(|s| {
                s.get("source_heading").and_then(Value::as_str) == Some(row.heading.as_str())
            })
            .count();
        let body = if index == 0 { K1_GUIDE } else { K2_GUIDE };
        ensure!(
            original.guid == row.guid
                && format!("### {heading}") == row.heading
                && occurrence == row.heading_occurrence
                && std::str::from_utf8(body)?
                    .lines()
                    .filter(|line| line.trim_end() == row.heading)
                    .count()
                    >= row.heading_occurrence
                && original
                    .metadata
                    .get("InstallationMethod")
                    .and_then(Value::as_str)
                    == Some(row.source_method.as_str())
                && matches!(row.source_method.as_str(), "TSLPatcher" | "TSLPatcher Mod"),
            "Original source owner heading, GUID, occurrence or method differs"
        );
        ensure!(
            !row.semantic_basis.is_empty()
                && row.semantic_basis.len() <= 1024
                && matches!(row.disposition.as_str(), "admitted" | "held"),
            "Invalid source semantic disposition"
        );
        if row.disposition == "admitted" {
            ensure!(
                original.directions.trim().is_empty()
                    && original.options.is_empty()
                    && original
                        .metadata
                        .get("UnresolvedMasters")
                        .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty)),
                "Source-plan semantic row exceeds empty-directions/options/masters scope"
            );
            admitted[index] += 1;
        }
        let (step, archive, archive_sha, installer, installer_sha, package, ini) =
            protocol(&row, plan)?;
        let entry = Entry {
            raw: raw[row.component_index].clone(),
            original,
            step,
            archive,
            archive_sha,
            installer,
            installer_sha,
            package,
            ini,
            source_pins: pins.clone(),
            row: row.clone(),
        };
        ensure!(
            entries.insert(row.guid, entry).is_none(),
            "Duplicate source-plan owner GUID"
        );
    }
    ensure!(
        admitted == [33, 7],
        "Initial reviewed source-plan semantic cohort differs"
    );
    let digest = value_digest(
        &json!({"Schema":1,"MappingSha256":MAPPING_SHA,"Policy":POLICY,
        "K1GuideSha256":K1_GUIDE_SHA,"K2GuideSha256":K2_GUIDE_SHA,"K1PlanSha256":K1_PLAN_SHA,"K2PlanSha256":K2_PLAN_SHA,
        "ManifestConvention":MANIFEST_CONVENTION,"MaxArchive":MAX_ARCHIVE,"MaxExpanded":MAX_EXPANDED,"MaxMember":MAX_MEMBER,"MaxIni":MAX_INI,"MaxEntries":MAX_ENTRIES,"MaxPathDepth":64,
        "IniPolicy":"strict-UTF8-BOM-aware-empty-CompileList-no-unclassified-sections-v1",
        "PayloadPolicy":"declared-native-sources-and-named-documentation-only-v1","StandardInstallers":INSTALLERS,
        "IniGraphPolicy":"static-AddField-exact-prefix-case-insensitive-sections-no-cycles-sum-all-weighted-node-expansions-v2",
        "MaxIniSections":MAX_ENTRIES,"MaxIniFields":MAX_ENTRIES,"MaxIniGraphExpansion":MAX_ENTRIES,"MaxIniGraphDepth":64,
        "IniGraphCostPolicy":"sum-node-one-plus-all-fields-and-expanded-reference-costs-v1",
        "IniDynamicReferencePolicy":"reject-exact-ConfigReader-requires-runtime-gff-resolution-v1"}),
    )?;
    Ok(Compiled { entries, digest })
}
fn compiled() -> Result<&'static Compiled> {
    COMPILED
        .get_or_init(|| load_compiled().map_err(|error| format!("{error:#}")))
        .as_ref()
        .map_err(|error| anyhow::anyhow!("{error}"))
}
fn entry(guid: &str) -> Result<&'static Entry> {
    compiled()?
        .entries
        .get(guid)
        .context("Owner is outside compiled source-plan mapping")
}
pub(crate) fn has_owner(guid: &str) -> Result<bool> {
    Ok(compiled()?.entries.contains_key(guid))
}
pub(crate) fn canonical_context(component: &Component) -> Result<Component> {
    let entry = entry(&component.guid)?;
    ensure!(
        entry.row.disposition == "admitted",
        "Mapped source-plan owner is held: {}",
        entry.row.semantic_basis
    );
    let mut actual = component.clone();
    actual.is_selected = entry.original.is_selected;
    ensure!(
        serde_json::to_value(&actual)? == serde_json::to_value(&entry.original)?,
        "Source-plan owner differs from its complete original context"
    );
    let mut original = entry.original.clone();
    original.is_selected = component.is_selected;
    Ok(original)
}
fn own_claim(component: &Component) -> bool {
    component.metadata.contains_key(BINDING)
        || component
            .metadata
            .get("DraftReview")
            .and_then(|r| r.get("Generator"))
            .and_then(Value::as_str)
            == Some(GENERATOR)
}
pub(crate) fn contains_native(component: &Component) -> bool {
    fn visit(c: &Component, depth: usize) -> bool {
        depth > 64 || own_claim(c) || c.options.iter().any(|child| visit(child, depth + 1))
    }
    visit(component, 0)
}

fn ordinary(metadata: &fs::Metadata, directory: bool) -> Result<()> {
    ensure!(
        !metadata.file_type().is_symlink()
            && (if directory {
                metadata.is_dir()
            } else {
                metadata.is_file()
            }),
        "Source-plan payload rejects links/special files"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "Source-plan payload rejects reparse points"
        );
    }
    Ok(())
}
fn scratch(options: &archive::Options, root: &Path) -> Result<PathBuf> {
    let path = options
        .scratch_dir
        .clone()
        .unwrap_or_else(std::env::temp_dir);
    ordinary(&fs::symlink_metadata(&path)?, true)?;
    let path = path.canonicalize()?;
    ensure!(
        !path.starts_with(root),
        "Source-plan scratch must be outside mod archives"
    );
    Ok(path)
}
fn file_pin(path: &Path, name: &str, limit: u64, cancelled: &AtomicBool) -> Result<Pin> {
    let mut file = crate::paths::open_regular_read(path)?;
    let size = file.metadata()?.len();
    ensure!(size <= limit, "Source-plan member exceeds resource bound");
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        check(cancelled)?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .context("Source member size overflow")?;
        ensure!(total <= size, "Source-plan member grew");
        hash.update(&buffer[..count]);
    }
    ensure!(total == size, "Source-plan member changed during read");
    Ok(Pin {
        path: name.into(),
        sha256: format!("{:x}", hash.finalize()),
        bytes: total,
    })
}
fn snapshot(
    entry: &Entry,
    root: &Path,
    scratch: &Path,
    cancelled: &AtomicBool,
) -> Result<tempfile::NamedTempFile> {
    let mut input = crate::paths::open_regular_read(&root.join(literal(&entry.archive)?))?;
    let expected = input.metadata()?.len();
    ensure!(
        expected > 0 && expected <= MAX_ARCHIVE,
        "Source-plan archive exceeds bound"
    );
    let mut output = tempfile::Builder::new()
        .prefix("modsync-source-plan-")
        .tempfile_in(scratch)?;
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        check(cancelled)?;
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .context("Source archive size overflow")?;
        ensure!(total <= expected, "Source-plan archive grew");
        hash.update(&buffer[..count]);
        output.write_all(&buffer[..count])?;
    }
    ensure!(
        total == expected && format!("{:x}", hash.finalize()) == entry.archive_sha,
        "Original source-plan archive hash differs"
    );
    output.flush()?;
    Ok(output)
}
fn documentation(path: &str) -> bool {
    let path = Path::new(path);
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(ext.as_str(), "txt" | "md" | "rtf" | "pdf" | "html" | "htm")
        && [
            "readme",
            "read me",
            "info",
            "license",
            "licence",
            "changelog",
            "change log",
            "credits",
            "instructions",
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}
fn payload(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "2da"
            | "utc"
            | "uti"
            | "utp"
            | "utd"
            | "ute"
            | "utm"
            | "uts"
            | "utt"
            | "utw"
            | "are"
            | "git"
            | "ifo"
            | "dlg"
            | "jrl"
            | "gff"
            | "fac"
            | "bic"
            | "lip"
            | "lyt"
            | "vis"
            | "wok"
            | "pwk"
            | "dwk"
            | "mdl"
            | "mdx"
            | "tga"
            | "tpc"
            | "dds"
            | "txi"
            | "wav"
            | "mp3"
            | "ncs"
            | "tlk"
            | "ssf"
            | "mod"
            | "rim"
            | "erf"
    )
}
fn configured(value: &str, allow_dot: bool) -> Result<PathBuf> {
    if allow_dot && value == "." {
        return Ok(PathBuf::new());
    }
    let normalized = value.replace('\\', "/");
    literal(&normalized)
}
fn consume_base(
    base: &ModBase,
    package: &Path,
    consumed: &mut BTreeSet<String>,
    files: &BTreeMap<String, Member>,
) -> Result<()> {
    let folder = configured(&base.sourcefolder, true)?;
    let source = folder.join(configured(&base.sourcefile, false)?);
    configured(&base.saveas, false)?;
    configured(&base.destination, true)?;
    ensure!(
        payload(&base.sourcefile),
        "Native configured source type requires separate admission"
    );
    let relative = package
        .join("tslpatchdata")
        .join(source)
        .to_str()
        .context("Non UTF8 configured source")?
        .replace('\\', "/");
    let key = crate::unicode_case::ordinal_key(&relative);
    let actual = files
        .values()
        .find(|member| !member.directory && crate::unicode_case::ordinal_key(&member.path) == key)
        .context("Native configured source is absent from authenticated package")?;
    consumed.insert(actual.path.clone());
    Ok(())
}
/// Match ConfigReader's static AddField references before it recursively unrolls
/// them. Case-only duplicate sections and dynamic/unknown references are outside
/// this initial closed cohort; cycles and expansion budgets fail before loading.
fn validate_addfield_graph(parser: &ConfigParser) -> Result<()> {
    struct Node {
        weight: usize,
        references: Vec<String>,
    }
    let mut graph = BTreeMap::<String, Node>::new();
    let mut field_count = 0usize;
    for name in parser.sections() {
        ensure!(
            graph.len() < MAX_ENTRIES && name.len() <= 4096,
            "Native INI section inventory exceeds bound"
        );
        let fields = parser
            .section(name)
            .context("Native INI graph section absent")?;
        field_count = field_count
            .checked_add(fields.len())
            .context("Native INI field count overflow")?;
        ensure!(
            field_count <= MAX_ENTRIES,
            "Native INI field inventory exceeds bound"
        );
        let mut references = Vec::new();
        for (key, value) in fields.items() {
            // This is the exact case-sensitive prefix used by parse_gff_operation
            // and add_field_gff; section lookup itself is case-insensitive.
            if key.starts_with("AddField") {
                let target = value
                    .as_deref()
                    .context("Native AddField reference has no static section")?;
                ensure!(
                    !target.is_empty() && target.len() <= 4096,
                    "Native AddField section reference exceeds bound"
                );
                ensure!(
                    !odypatcher::patcher::reader::ConfigReader::requires_runtime_gff_resolution(
                        target
                    ),
                    "Native dynamic AddField tokens require separate admission even when a literal section exists"
                );
                references.push(target.to_lowercase());
            }
        }
        ensure!(
            graph
                .insert(
                    name.to_lowercase(),
                    Node {
                        weight: fields
                            .len()
                            .checked_add(1)
                            .context("Native INI section weight overflow")?,
                        references,
                    }
                )
                .is_none(),
            "Native INI has case-ambiguous section definitions"
        );
    }
    fn cost(
        node: &str,
        graph: &BTreeMap<String, Node>,
        active: &mut BTreeSet<String>,
        memo: &mut BTreeMap<String, (usize, usize)>,
    ) -> Result<(usize, usize)> {
        if let Some(value) = memo.get(node) {
            return Ok(*value);
        }
        ensure!(
            active.len() < 64 && active.insert(node.to_owned()),
            "Native AddField graph is cyclic or exceeds depth bound"
        );
        let definition = graph
            .get(node)
            .context("Native AddField reference is dynamic or names an unknown section")?;
        // Every visit may create operations for non-reference fields too (for
        // example 2DAMEMORYn=!FieldPath). Charge them on every unrolled visit,
        // rather than treating a field-heavy section as one cheap graph node.
        let mut count = definition.weight;
        ensure!(
            count <= MAX_ENTRIES,
            "Native AddField weighted section exceeds bound"
        );
        let mut height = 1usize;
        for child in &definition.references {
            let (child_count, child_height) = cost(child, graph, active, memo)?;
            count = count
                .checked_add(child_count)
                .context("Native AddField expansion count overflow")?;
            height = height.max(child_height + 1);
            ensure!(
                count <= MAX_ENTRIES && height <= 64,
                "Native AddField unrolled expansion exceeds resource bounds"
            );
        }
        active.remove(node);
        memo.insert(node.into(), (count, height));
        Ok((count, height))
    }
    let mut memo = BTreeMap::new();
    let mut total = 0usize;
    for node in graph.keys() {
        let (count, _) = cost(node, &graph, &mut BTreeSet::new(), &mut memo)?;
        total = total
            .checked_add(count)
            .context("Native AddField aggregate count overflow")?;
        ensure!(
            total <= MAX_ENTRIES,
            "Native AddField aggregate expansion exceeds bound"
        );
    }
    Ok(())
}

fn validate_ini(
    entry: &Entry,
    tree: &Path,
    members: &BTreeMap<String, Member>,
    cancelled: &AtomicBool,
) -> Result<BTreeSet<String>> {
    check(cancelled)?;
    let ini = tree.join(literal(&entry.ini)?);
    let pin = file_pin(&ini, &entry.ini, MAX_INI, cancelled)?;
    let mut bytes = Vec::new();
    crate::paths::open_regular_read(&ini)?
        .take(MAX_INI + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == pin.bytes && sha(&bytes) == pin.sha256,
        "Native INI changed while decoding"
    );
    let text =
        odypatcher::patcher::encoding::decode_bytes_with_fallbacks(&bytes).map_err(|error| {
            anyhow::anyhow!("Native source INI requires strict UTF8 admission: {error}")
        })?;
    let parser =
        ConfigParser::read_string(text.trim_start_matches('\u{feff}'), OptionXform::Identity)
            .map_err(|e| anyhow::anyhow!("Parse native source INI: {e}"))?;
    validate_addfield_graph(&parser)?;
    for section in parser.sections() {
        let fields = parser
            .section(section)
            .context("Native parsed INI section absent")?;
        if section.eq_ignore_ascii_case("CompileList") {
            ensure!(
                fields.is_empty(),
                "Source-plan native CompileList must be empty"
            );
        }
        for key in fields.keys() {
            let lower = key.to_ascii_lowercase();
            if let Some(offset) = lower.find("(lang") {
                ensure!(
                    &key[offset..offset + 5] == "(lang",
                    "Native localized selector casing requires separate compatibility admission"
                );
                let selector = lower[offset + 5..].strip_suffix(')').context(
                    "Native localized selector requires separate compatibility admission",
                )?;
                ensure!(
                    !selector.is_empty()
                        && selector.bytes().all(|b| b.is_ascii_digit())
                        && selector.parse::<i32>().is_ok(),
                    "Native localized selector requires separate compatibility admission"
                );
            }
        }
    }
    let package = literal_package(&entry.package)?;
    let data = tree.join(&package).join("tslpatchdata");
    let mut config = PatcherConfig::new();
    let mut logger = odypatcher::PatchLogger::new();
    config
        .load(&text, &data, &mut logger, Some(&data))
        .map_err(|e| anyhow::anyhow!("Read native source INI configuration: {e}"))?;
    ensure!(
        logger
            .all_logs
            .iter()
            .all(|log| log.log_type < odypatcher::LogType::Warning
                && !log
                    .message
                    .starts_with("There are some orphaned ini sections")),
        "Native source INI has warnings/errors or unclassified sections"
    );
    let game = if entry.row.game == "k1" { 1 } else { 2 };
    ensure!(
        config.game_number.is_none_or(|number| number == game)
            && config.patches_nss.is_empty()
            && !config.ignore_file_extensions,
        "Native source INI has compiler, game or extension semantics outside this policy"
    );
    let mut consumed = BTreeSet::new();
    for base in config
        .install_list
        .iter()
        .map(|p| &p.base)
        .chain(config.patches_2da.iter().map(|p| &p.base))
        .chain(config.patches_gff.iter().map(|p| &p.base))
        .chain(config.patches_ncs.iter().map(|p| &p.base))
        .chain(config.patches_ssf.iter().map(|p| &p.base))
    {
        consume_base(base, &package, &mut consumed, members)?;
    }
    if !config.patches_tlk.modifiers.is_empty() {
        let base = &config.patches_tlk.base;
        let folder = configured(&base.sourcefolder, true)?;
        configured(&base.sourcefile, false)?;
        configured(&base.saveas, false)?;
        configured(&base.destination, true)?;
        for modifier in &config.patches_tlk.modifiers {
            if let Some(path) = modifier.tlk_filepath() {
                let relative = path
                    .strip_prefix(&data)
                    .context("Native TLK source escaped its authenticated data root")?;
                let relative = relative
                    .to_str()
                    .context("Non UTF8 native TLK source")?
                    .replace('\\', "/");
                let source = package
                    .join("tslpatchdata")
                    .join(configured(&relative, false)?);
                let key = crate::unicode_case::ordinal_key(
                    source.to_str().context("Non UTF8 native TLK source")?,
                );
                let actual = members
                    .values()
                    .find(|member| {
                        !member.directory && crate::unicode_case::ordinal_key(&member.path) == key
                    })
                    .context("Native TLK source is absent from authenticated package")?;
                ensure!(
                    Path::new(&actual.path)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("tlk")),
                    "Native TLK source has unsupported type"
                );
                consumed.insert(actual.path.clone());
            }
        }
        let paired = package
            .join("tslpatchdata")
            .join(folder)
            .join(configured(&config.patches_tlk.sourcefile_f, false)?);
        let key = crate::unicode_case::ordinal_key(
            paired.to_str().context("Non UTF8 native paired TLK path")?,
        );
        if let Some(actual) = members.values().find(|member| {
            !member.directory && crate::unicode_case::ordinal_key(&member.path) == key
        }) {
            ensure!(
                Path::new(&actual.path)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("tlk")),
                "Native paired TLK source has unsupported type"
            );
            consumed.insert(actual.path.clone());
        }
    }
    // Installer operations are file-level. Do not recursively flatten GFF
    // fields merely to establish that the original configuration has work.
    let file_operations = config.install_list.len()
        + config.patches_2da.len()
        + config.patches_gff.len()
        + config.patches_ncs.len()
        + config.patches_ssf.len()
        + usize::from(!config.patches_tlk.modifiers.is_empty());
    ensure!(
        file_operations > 0 && file_operations <= MAX_ENTRIES,
        "Native source INI has no supported operations or exceeds its file-operation bound"
    );
    Ok(consumed)
}
fn literal_package(value: &str) -> Result<PathBuf> {
    if value.is_empty() {
        Ok(PathBuf::new())
    } else {
        literal(value)
    }
}

/// Unforgeable admission witness: no Deserialize, Default or public fields.
/// A legacy contract constructor may consume only this freshly issued value.
pub(crate) struct AdmittedSourcePlan {
    expected: Component,
    installer: Pin,
    ini: Pin,
    inventory_digest: String,
    authority_digest: String,
    package_members: BTreeMap<String, Member>,
    archive_inventory: Vec<Member>,
    archive_bytes: u64,
}
impl AdmittedSourcePlan {
    pub(crate) fn component_guid(&self) -> &str {
        &self.expected.guid
    }
    pub(crate) fn installer_identity(&self) -> (&str, &str, u64) {
        (
            &self.installer.path,
            &self.installer.sha256,
            self.installer.bytes,
        )
    }
    pub(crate) fn ini_identity(&self) -> (&str, &str, u64) {
        (&self.ini.path, &self.ini.sha256, self.ini.bytes)
    }
    pub(crate) fn inventory_digest(&self) -> &str {
        &self.inventory_digest
    }
    pub(crate) fn authority_digest(&self) -> &str {
        &self.authority_digest
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
                let metadata = fs::symlink_metadata(&path)?;
                ordinary(&metadata, metadata.is_dir())?;
                ensure!(
                    path.canonicalize()?.starts_with(&root),
                    "Native staged package escaped its root"
                );
                let relative = path
                    .strip_prefix(&root)?
                    .to_str()
                    .context("Non UTF8 native staged path")?
                    .replace('\\', "/");
                literal(&relative)?;
                ensure!(
                    seen.len() < MAX_ENTRIES
                        && seen.insert(relative.clone())
                        && aliases.insert(crate::unicode_case::ordinal_key(&relative)),
                    "Native staged package aliases or exceeds bound"
                );
                let expected = self
                    .package_members
                    .get(&relative)
                    .context("Unclassified extra native staged package member")?;
                ensure!(
                    metadata.is_dir() == expected.directory,
                    "Native staged package member kind differs"
                );
                if expected.directory {
                    pending.push(path);
                } else {
                    let pin =
                        file_pin(&path, &relative, expected.bytes.min(MAX_MEMBER), cancelled)?;
                    ensure!(
                        pin.bytes == expected.bytes && pin.sha256 == expected.sha256,
                        "Native staged consumed payload bytes differ"
                    );
                }
            }
        }
        ensure!(
            seen.len() == self.package_members.len(),
            "Native staged package is incomplete"
        );
        check(cancelled)
    }
}
fn derive(
    entry: &Entry,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<AdmittedSourcePlan> {
    check(cancelled)?;
    ensure!(
        entry.row.disposition == "admitted",
        "Mapped source-plan owner is held: {}",
        entry.row.semantic_basis
    );
    ordinary(&fs::symlink_metadata(root)?, true)?;
    let root = root.canonicalize()?;
    let scratch = scratch(options, &root)?;
    let snapshot = snapshot(entry, &root, &scratch, cancelled)?;
    let archive_bytes = snapshot.as_file().metadata()?.len();
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
        Path::new(&entry.archive),
        &bounded,
        cancelled,
    )?;
    ensure!(
        listed.len() <= MAX_ENTRIES && expanded <= MAX_EXPANDED,
        "Native archive inventory exceeds bound"
    );
    let mut inventory = BTreeMap::new();
    let mut aliases = BTreeMap::new();
    let mut explicit = BTreeSet::new();
    for item in listed {
        check(cancelled)?;
        let name = item.path.strip_suffix('/').unwrap_or(&item.path).to_owned();
        literal(&name)?;
        ensure!(
            explicit.insert(crate::unicode_case::ordinal_key(&name)),
            "Duplicate native archive member"
        );
        let mut paths = Vec::new();
        for (position, _) in name.match_indices('/') {
            paths.push((name[..position].to_owned(), true));
        }
        paths.push((name, item.is_directory));
        for (name, directory) in paths {
            let key = crate::unicode_case::ordinal_key(&name);
            if let Some(prior) = aliases.get(&key) {
                ensure!(prior == &name, "Native archive path aliases");
            } else {
                aliases.insert(key, name.clone());
            }
            if let Some(prior) = inventory.get(&name) {
                let prior: &Member = prior;
                ensure!(
                    prior.directory && directory,
                    "Native archive file/directory collision"
                );
            } else {
                ensure!(
                    inventory.len() < MAX_ENTRIES,
                    "Native canonical inventory exceeds entry bound"
                );
                inventory.insert(
                    name.clone(),
                    Member {
                        path: name,
                        directory,
                        bytes: 0,
                        sha256: String::new(),
                    },
                );
            }
        }
    }
    let decoded = archive::unpack(
        snapshot.path(),
        Path::new(&entry.archive),
        &bounded,
        cancelled,
    )?;
    let private = tempfile::Builder::new()
        .prefix("modsync-source-native-tree-")
        .tempdir_in(&scratch)?;
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for item in inventory.values().filter(|m| m.directory) {
        fs::create_dir_all(private.path().join(literal(&item.path)?))?;
    }
    for item in decoded {
        check(cancelled)?;
        let name = item
            .path
            .to_str()
            .context("Non UTF8 decoded native member")?
            .replace('\\', "/");
        literal(&name)?;
        let expected = inventory
            .get_mut(&name)
            .context("Unexpected decoded native archive member")?;
        ensure!(
            !expected.directory && seen.insert(name.clone()),
            "Duplicate decoded native archive member"
        );
        let pin = file_pin(item.file.path(), &name, MAX_MEMBER, cancelled)?;
        total = total
            .checked_add(pin.bytes)
            .context("Native expanded size overflow")?;
        ensure!(
            total <= MAX_EXPANDED,
            "Native archive expanded size exceeds bound"
        );
        expected.bytes = pin.bytes;
        expected.sha256 = pin.sha256;
        let target = private.path().join(literal(&name)?);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut input = crate::paths::open_regular_read(item.file.path())?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        let mut copied = 0u64;
        let mut copied_hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            check(cancelled)?;
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            copied = copied
                .checked_add(count as u64)
                .context("Native private copy size overflow")?;
            ensure!(
                copied <= expected.bytes,
                "Native decoded member changed during private copy"
            );
            copied_hash.update(&buffer[..count]);
            output.write_all(&buffer[..count])?;
        }
        output.flush()?;
        ensure!(
            copied == expected.bytes && format!("{:x}", copied_hash.finalize()) == expected.sha256,
            "Native decoded member bytes changed during private copy"
        );
    }
    ensure!(
        total == expanded && seen.len() == inventory.values().filter(|m| !m.directory).count(),
        "Native decoded archive inventory differs"
    );
    let mut ini_count = 0;
    let package = literal_package(&entry.package)?;
    let mut package_members = BTreeMap::new();
    for member in inventory.values() {
        check(cancelled)?;
        let path = literal(&member.path)?;
        let lower = member.path.to_ascii_lowercase();
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        ensure!(
            !matches!(
                filename.as_str(),
                "namespaces.ini" | "installlog.rtf" | "installlog.txt" | "moduleconfig.xml"
            ) && !lower.split('/').any(|part| part == "fomod"),
            "Native archive has namespaces, stale logs or FOMOD choices"
        );
        if member.directory {
            ensure!(
                inventory
                    .keys()
                    .any(|name| name.starts_with(&format!("{}/", member.path))),
                "Native archive has unclassified empty directories"
            );
        } else {
            if filename.ends_with(".ini") {
                ini_count += 1;
                ensure!(
                    member.path == entry.ini,
                    "Native archive has alternate INIs"
                );
            }
            if filename.ends_with(".exe") {
                ensure!(
                    member.path == entry.installer,
                    "Native archive has alternate installers"
                );
            }
            if !path.starts_with(&package) {
                ensure!(
                    documentation(&member.path),
                    "Unclassified payload outside the native package"
                );
            }
        }
        if path.starts_with(&package) {
            let relative = path
                .strip_prefix(&package)?
                .to_str()
                .context("Non UTF8 native package path")?
                .replace('\\', "/");
            if !relative.is_empty() {
                package_members.insert(
                    relative.clone(),
                    Member {
                        path: relative,
                        ..member.clone()
                    },
                );
            }
        }
    }
    ensure!(
        ini_count == 1,
        "Native source archive needs exactly its canonical sole INI"
    );
    let installer = file_pin(
        &private.path().join(literal(&entry.installer)?),
        &entry.installer,
        MAX_MEMBER,
        cancelled,
    )?;
    ensure!(
        installer.sha256 == entry.installer_sha,
        "Original standard native installer whole hash differs"
    );
    let ini = file_pin(
        &private.path().join(literal(&entry.ini)?),
        &entry.ini,
        MAX_INI,
        cancelled,
    )?;
    let consumed = validate_ini(entry, private.path(), &inventory, cancelled)?;
    for member in inventory.values().filter(|m| !m.directory) {
        if member.path == entry.installer || member.path == entry.ini || documentation(&member.path)
        {
            continue;
        }
        ensure!(
            consumed.contains(&member.path),
            "Native archive contains unclassified or optional unused payload"
        );
    }
    let archive_inventory = inventory.into_values().collect::<Vec<_>>();
    let inventory_digest = value_digest(&serde_json::to_value(&archive_inventory)?)?;
    let authority_digest = compiled()?.digest.clone();
    let mut admitted = AdmittedSourcePlan {
        expected: entry.original.clone(),
        installer,
        ini,
        inventory_digest,
        authority_digest,
        package_members,
        archive_inventory,
        archive_bytes,
    };
    admitted.expected = materialize(entry, &admitted)?;
    check(cancelled)?;
    Ok(admitted)
}
fn instruction(
    owner: &Component,
    index: usize,
    action: &str,
    source: String,
    destination: String,
) -> Instruction {
    Instruction {
        guid: crate::markdown::generated_id(
            &format!("{}:source-plan-native-v1", owner.guid),
            index,
        ),
        parent: owner.guid.clone(),
        action: action.into(),
        source: vec![source],
        destination,
        arguments: if action == "Patcher" {
            "changes.ini".into()
        } else {
            String::new()
        },
        overwrite: false,
        dependencies: vec![],
        restrictions: vec![],
    }
}
fn materialize(entry: &Entry, admitted: &AdmittedSourcePlan) -> Result<Component> {
    let mut owner = entry.original.clone();
    let slot = format!("<<modDirectory>>/.modsync-reviewed-{}-slot0", owner.guid);
    owner.instructions = vec![
        instruction(
            &owner,
            0,
            "ExtractTo",
            format!("<<modDirectory>>/{}", entry.archive),
            slot.clone(),
        ),
        instruction(
            &owner,
            1,
            "Patcher",
            format!("{slot}/{}", entry.installer),
            "<<kotorDirectory>>".into(),
        ),
    ];
    let derivation =
        json!({"Filename":entry.archive,"Sha256":entry.archive_sha,"Bytes":admitted.archive_bytes});
    owner.metadata.insert(BINDING.into(),json!({"Schema":1,"Policy":POLICY,"Generator":GENERATOR,"AuthoritySha256":admitted.authority_digest,
        "MappingSha256":MAPPING_SHA,"SourcePins":entry.source_pins,"OwnerMapping":entry.row,"RawOriginalComponent":entry.raw,
        "OriginalComponent":entry.original,"OriginalStep":entry.step,"Archive":derivation,"ManifestConvention":MANIFEST_CONVENTION,
        "ArchiveInventory":admitted.archive_inventory,"InventorySha256":admitted.inventory_digest,"Installer":admitted.installer,"Ini":admitted.ini,
        "RawConsumedPackage":admitted.package_members.values().collect::<Vec<_>>(),"PackageRelativeRoot":entry.package,
        "NativeOutcomeContract":"native-zero-warnings-changed-game-v1","BuiltinBackendRequired":true,"ExactPredictionRequired":true}));
    owner.metadata.insert("DraftReview".into(),json!({"RequiresReview":true,"Generator":GENERATOR,"Derivations":[derivation],"Result":draft::DraftResult{
        component_guid:owner.guid.clone(),draft_instruction_count:2,unparsed_gaps:vec![],conditional_drafts:vec![],rejected_drafts:0}}));
    portable(&serde_json::to_value(&owner)?, 0)?;
    ensure!(
        serde_json::to_vec(&owner)?.len() <= 16 * 1024 * 1024,
        "Native source-plan binding exceeds model bound"
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
    let entry = entry(&component.guid)?;
    ensure!(
        filenames == [entry.archive.clone()],
        "Source-plan native requires its one exhaustive original archive group"
    );
    let mut admitted = derive(entry, root, options, cancelled)?;
    admitted.expected.is_selected = original.is_selected;
    *component = admitted.expected;
    Ok(())
}
fn compare(component: &Component, pending: &Component) -> Result<()> {
    ensure!(
        !component.options.iter().any(contains_native),
        "Nested source-plan native ownership is forbidden"
    );
    let mut actual = serde_json::to_value(component)?;
    let mut expected = serde_json::to_value(pending)?;
    for value in [&mut actual, &mut expected] {
        let object = value
            .as_object_mut()
            .context("Invalid native component model")?;
        object.remove("IsSelected");
        object.remove("DraftReview");
    }
    ensure!(
        actual == expected,
        "Source-plan native full component, raw payload or instruction differs from independent derivation"
    );
    let actual = component
        .metadata
        .get("DraftReview")
        .context("Missing source-plan native review")?;
    let expected = &pending.metadata["DraftReview"];
    for key in ["Generator", "Derivations", "Result"] {
        ensure!(
            actual.get(key) == expected.get(key),
            "Source-plan native diagnostics or derivations differ"
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
    ensure!(
        own_claim(component),
        "Missing source-plan native provenance"
    );
    let admitted = derive(entry(&component.guid)?, root, options, cancelled)?;
    compare(component, &admitted.expected)?;
    Ok(admitted.expected)
}
pub(crate) fn verification(pending: &Component) -> Result<Value> {
    entry(&pending.guid)?;
    let mut value = serde_json::to_value(pending)?;
    value
        .as_object_mut()
        .context("Invalid native replay model")?
        .remove("IsSelected");
    Ok(
        json!({"Reviewer":draft::review::VERIFIER_NAME,"Kind":draft::review::VERIFICATION_KIND,"ToolVersion":concat!("ModSync.rs/",env!("CARGO_PKG_VERSION")),
        "ArchiveSha256":compiled()?.digest,"ArchiveFilename":"source-plan-native-authority-v1","ReplayDigest":value_digest(&value)?,"IdentityVersion":2}),
    )
}
pub(crate) fn verify_runtime(
    component: &Component,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Option<AdmittedSourcePlan>> {
    if !own_claim(component) {
        ensure!(
            !component.options.iter().any(contains_native),
            "Nested source-plan native binding is forbidden"
        );
        return Ok(None);
    }
    let admitted = derive(entry(&component.guid)?, root, options, cancelled)?;
    compare(component, &admitted.expected)?;
    let review = component
        .metadata
        .get("DraftReview")
        .context("Missing source-plan native review")?;
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
        "Source-plan native requires unchanged dedicated independent replay approval"
    );
    check(cancelled)?;
    Ok(Some(admitted))
}
pub(crate) fn native_patcher_contract(
    component: &Component,
    current: &Instruction,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Option<super::reviewed_manual::NativePatcherContract>> {
    let Some(admitted) = verify_runtime(component, root, options, cancelled)? else {
        return Ok(None);
    };
    if serde_json::to_value(current)? != serde_json::to_value(&admitted.expected.instructions[1])? {
        return Ok(None);
    }
    Ok(Some(
        super::reviewed_manual::NativePatcherContract::from_source_plan(admitted)?,
    ))
}
