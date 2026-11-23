//! Closed authored bindings and independent whole-component replay.
//! The engine alone owns game writes and installed-success receipts.
use crate::{Component, Guide, Instruction, archive, draft, galaxy_map, k1_widescreen};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub(crate) const GENERATOR: &str = "resolved-k1-widescreen";
const BINDING: &str = "K1WidescreenBinding";
const PLAN: &[u8] = include_bytes!("../tools/modsync-ci/fixtures/ci/k1-manual-plan.json");
const GUIDE_SHA: &str = "44067788c6cb7a83b38d468817f273ce5cc901cd5e000c0f9ff65f8af15fc6ed";
const PLAN_SHA: &str = "1b402ba085625c4863795069eaacaef0f53e8ea7179e56a5bd150d88eb9c0a74";
const REVISION: &str = "67dc5fd8da6abde324c51d32eee3d04687070dd6";
const HRMENUS: &str = "6c3c7165-297e-80eb-b9f8-d47506dab2d5";
const GALAXY: &str = "7c11497d-de47-8eee-b80a-967b984a2822";
const FOURGB: &str = "267b10d2-bab0-8e3a-a258-f9b6d6f9d71b";
const INPUT_LIMIT: u64 = 8 * 1024 * 1024;
const EXPANDED_LIMIT: u64 = 256 * 1024 * 1024;
const GAPS: &[&str] = &[];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    Hrmenus,
    Galaxy,
    Fourgb,
}
impl Role {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Hrmenus => "hrmenus",
            Self::Galaxy => "galaxy",
            Self::Fourgb => "fourgb",
        }
    }
}
fn role(guid: &str) -> Result<Role> {
    match guid {
        HRMENUS => Ok(Role::Hrmenus),
        GALAXY => Ok(Role::Galaxy),
        FOURGB => Ok(Role::Fourgb),
        _ => bail!("Unknown K1 widescreen owner"),
    }
}
fn check(cancelled: &AtomicBool) -> Result<()> {
    ensure!(!cancelled.load(Ordering::Acquire), "K1 recipe cancelled");
    Ok(())
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn bounded_read(path: &Path, limit: u64, cancelled: &AtomicBool) -> Result<Vec<u8>> {
    let mut file = crate::paths::open_regular_read(path)?;
    ensure!(
        file.metadata()?.len() <= limit,
        "K1 private input exceeds bound"
    );
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 65536];
    loop {
        check(cancelled)?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        ensure!(
            bytes.len() as u64 + count as u64 <= limit,
            "K1 private input grew beyond bound"
        );
        bytes.extend_from_slice(&buffer[..count]);
    }
    Ok(bytes)
}
struct Source {
    raw: Vec<Value>,
    canonical: Guide,
}
fn source(document: &str, cancelled: &AtomicBool) -> Result<Source> {
    check(cancelled)?;
    ensure!(
        document.len() as u64 <= INPUT_LIMIT
            && sha(document.as_bytes()) == GUIDE_SHA
            && sha(PLAN) == PLAN_SHA,
        "K1 original guide/plan byte pin differs"
    );
    // Markdown drafting precedes JSON loader canonicalization, matching convert.
    let mut original = Guide::parse(document, "md")?;
    draft::generate(&mut original);
    let raw = original
        .components
        .iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let canonical = Guide::parse_preserving_selection(&serde_json::to_string(&original)?, "json")?;
    check(cancelled)?;
    Ok(Source { raw, canonical })
}
fn original_owner<'a>(source: &'a Source, component: &Component) -> Result<&'a Component> {
    let owner = source
        .canonical
        .components
        .iter()
        .find(|owner| owner.guid == component.guid)
        .context("K1 source owner missing")?;
    ensure!(
        serde_json::to_value(component)? == serde_json::to_value(owner)?,
        "K1 complete original source component differs"
    );
    Ok(owner)
}
fn ensure_portable_metadata(value: &Value, depth: usize) -> Result<()> {
    ensure!(
        depth <= 64,
        "K1 metadata nesting exceeds the portable bound"
    );
    match value {
        Value::Null => bail!("K1 metadata must represent absence explicitly without null"),
        Value::Object(fields) => {
            for value in fields.values() {
                ensure_portable_metadata(value, depth + 1)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                ensure_portable_metadata(value, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .context("Missing K1 compiled string")
}
fn list<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .context("Missing K1 compiled array")
}

struct Acquired {
    role: String,
    filename: String,
    bytes: Vec<u8>,
    members: Vec<(String, Vec<u8>)>,
}
fn acquire(
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Vec<Acquired>> {
    let metadata = std::fs::symlink_metadata(root)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "K1 archive root must be a real directory"
    );
    let root = root.canonicalize()?;
    let spec = k1_widescreen::input_spec(cancelled)?;
    let mut acquired = Vec::new();
    let mut bounded = options.clone();
    bounded.max_expanded_bytes = Some(
        options
            .max_expanded_bytes
            .unwrap_or(EXPANDED_LIMIT)
            .min(EXPANDED_LIMIT),
    );
    for pin in list(&spec, "archives")? {
        check(cancelled)?;
        let filename = text(pin, "filename")?;
        let role = text(pin, "role")?;
        let bytes = bounded_read(&root.join(filename), INPUT_LIMIT, cancelled)?;
        ensure!(
            bytes.len() as u64 == pin["bytes"].as_u64().context("K1 archive size absent")?
                && sha(&bytes) == text(pin, "sha256")?,
            "K1 acquired archive differs"
        );
        let selected: Vec<_> = list(&spec, "assets")?
            .iter()
            .filter(|asset| asset["archive"] == role)
            .collect();
        let mut members = Vec::new();
        if !selected.is_empty() {
            // Decode a verified immutable snapshot so concurrent source changes
            // cannot substitute bytes between hashing and archive decoding.
            let mut builder = tempfile::Builder::new();
            builder.prefix("modsync-k1-private-input-");
            let mut snapshot = match &options.scratch_dir {
                Some(dir) => builder.tempfile_in(dir)?,
                None => builder.tempfile()?,
            };
            snapshot.write_all(&bytes)?;
            snapshot.flush()?;
            let entries =
                archive::unpack(snapshot.path(), Path::new(filename), &bounded, cancelled)?;
            ensure!(
                entries.len() <= 10000,
                "K1 archive member count exceeds bound"
            );
            for asset in selected {
                let path = text(asset, "path")?;
                let entry = entries
                    .iter()
                    .find(|entry| entry.path.to_str() == Some(path))
                    .context("K1 exact authored selected archive member missing")?;
                let size = asset["bytes"]
                    .as_u64()
                    .context("K1 selected member size absent")?;
                let data = bounded_read(entry.file.path(), size, cancelled)?;
                ensure!(
                    data.len() as u64 == size && sha(&data) == text(asset, "sha256")?,
                    "K1 decoded selected member differs"
                );
                members.push((path.to_owned(), data));
            }
        }
        acquired.push(Acquired {
            role: role.to_owned(),
            filename: filename.to_owned(),
            bytes,
            members,
        });
    }
    Ok(acquired)
}
fn admit_materializer(
    document: &str,
    source: &Source,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<k1_widescreen::CandidateContract> {
    let acquired = acquire(root, options, cancelled)?;
    let members: Vec<Vec<_>> = acquired
        .iter()
        .map(|a| {
            a.members
                .iter()
                .map(|(path, bytes)| k1_widescreen::MemberInput { path, bytes })
                .collect()
        })
        .collect();
    let inputs: Vec<_> = acquired
        .iter()
        .zip(&members)
        .map(|(a, members)| k1_widescreen::ArchiveInput {
            role: &a.role,
            filename: &a.filename,
            bytes: &a.bytes,
            members,
        })
        .collect();
    k1_widescreen::admit(
        k1_widescreen::PROFILE_ID,
        k1_widescreen::SourceContext {
            guide_revision: REVISION,
            guide_body: document.as_bytes(),
            source_plan: PLAN,
            raw_components: &source.raw,
            canonical_components: &source.canonical.components,
        },
        &inputs,
        cancelled,
    )
}
fn instruction(
    component: &Component,
    index: usize,
    action: &str,
    source: Vec<String>,
    destination: &str,
    arguments: &str,
) -> Instruction {
    Instruction {
        guid: crate::markdown::generated_id(&format!("{}:k1-widescreen", component.guid), index),
        parent: component.guid.clone(),
        action: action.into(),
        source,
        destination: destination.into(),
        arguments: arguments.into(),
        overwrite: true,
        dependencies: vec![],
        restrictions: vec![],
    }
}
fn expected(
    original: &Component,
    document: &str,
    choice: Option<&str>,
    cancelled: &AtomicBool,
) -> Result<Component> {
    let role = role(&original.guid)?;
    ensure!(
        if role == Role::Galaxy {
            choice == Some("Vanilla")
        } else {
            choice.is_none()
        },
        "K1 explicit Galaxy Vanilla choice differs"
    );
    let mut bound = original.clone();
    bound.instructions = vec![instruction(
        original,
        0,
        "K1Widescreen",
        vec![],
        "<<kotorDirectory>>",
        role.as_str(),
    )];
    let spec = k1_widescreen::input_spec(cancelled)?;
    // Observation catalog only. Actual Galaxy input authority is still issued
    // exclusively by galaxy_map::admit_vanilla on the original component.
    let galaxy_catalog = include_str!("galaxy_map_vanilla.json");
    ensure!(
        sha(galaxy_catalog.as_bytes())
            == "76794533372b440b9b804a3afac5a3909e1e323da3479bab165ffdfe664167b7",
        "Galaxy observation catalog changed"
    );
    let galaxy_catalog: Value = serde_json::from_str(galaxy_catalog)?;
    let dependency_context = json!({"OriginalOwners":spec["source_components"],"HrmenusGuid":HRMENUS,"GalaxyGuid":GALAXY,"FourgbGuid":FOURGB,
        "GalaxyWholeBaseAfterAllOtherContent":true,"GalaxyOverlayAfterWholeBase":true,
        "FourgbLast":true,"ExpectedFourgbInput":"63698d8aba343621591646b03e2a8fafd4a170befbe9dce65fc4f873b5429bf0"});
    let mut binding = json!({"Schema":1,"Generator":GENERATOR,"Role":role.as_str(),
        "ProfileId":k1_widescreen::PROFILE_ID,"InputComponent":serde_json::to_value(original)?,
        "SourceGuideDocument":document,"SourceGuideSha256":GUIDE_SHA,"SourceGuideRevision":REVISION,"SourcePlanSha256":PLAN_SHA,
        "ArchiveObservations":spec["archives"],"MemberObservations":spec["assets"],
        "GalaxyChoiceApplicable":role == Role::Galaxy,"GalaxyBuiltinBackendRequired":role == Role::Galaxy,
        "DependencyContext":dependency_context,"VendorOutputEquivalenceProven":true,
        "VendorOutputEquivalenceScope":spec["vendor_output_equivalence_scope"],
        "UniwsVendorOutputEquivalenceProven":spec["uniws_vendor_output_equivalence_proven"],
        "UniwsVendorWitness":spec["uniws_vendor_witness"],"IniCreationPolicy":spec["ini_creation"],"ConsumerGaps":GAPS});
    if role == Role::Galaxy {
        let fields = binding
            .as_object_mut()
            .context("Invalid K1 binding object")?;
        fields.insert("GalaxyBaseObservations".into(),json!({"Archive":galaxy_catalog["archive"],"Members":galaxy_catalog["members"],"Namespace":"Vanilla","NamespaceIndex":0}));
        fields.insert("ExplicitGalaxyChoice".into(), json!("Vanilla"));
        fields.insert("GalaxyNamespaceIndex".into(), json!(0));
        fields.insert(
            "GalaxyNativeOutcomeContract".into(),
            json!("native-zero-warnings-changed-game-v1"),
        );
    }
    ensure_portable_metadata(&binding, 0)?;
    bound.metadata.insert(BINDING.into(), binding);
    bound.metadata.insert("DraftReview".into(),json!({"RequiresReview":true,"Generator":GENERATOR,
        "Derivations":spec["archives"],"Result":draft::DraftResult {component_guid:bound.guid.clone(),draft_instruction_count:bound.instructions.len(),
            unparsed_gaps:GAPS.iter().map(|s| (*s).into()).collect(),conditional_drafts:vec![],rejected_drafts:0}}));
    Ok(bound)
}

/// Binding changes only the caller's component, atomically after full admission.
/// Every generated instruction remains behind the existing pending draft fence.
pub(crate) fn bind(
    component: &mut Component,
    explicit_profile: &str,
    explicit_galaxy_choice: Option<&str>,
    source_guide: &Path,
    archives_root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<()> {
    ensure!(
        explicit_profile == k1_widescreen::PROFILE_ID,
        "K1 fixed explicit profile required"
    );
    let bytes = bounded_read(source_guide, INPUT_LIMIT, cancelled)?;
    let document = std::str::from_utf8(&bytes).context("K1 guide must retain exact UTF8 bytes")?;
    let source = source(document, cancelled)?;
    original_owner(&source, component)?;
    let _materials = admit_materializer(document, &source, archives_root, options, cancelled)?;
    if role(&component.guid)? == Role::Galaxy {
        let admission = galaxy_map::admit_vanilla(
            component,
            explicit_galaxy_choice.context("Explicit Galaxy choice required")?,
            archives_root,
            options,
            cancelled,
        )?;
        ensure!(
            admission.namespace_index() == 0 && admission.namespace_id() == "Vanilla",
            "K1 Galaxy namespace differs"
        );
    }
    let bound = expected(component, document, explicit_galaxy_choice, cancelled)?;
    check(cancelled)?;
    *component = bound;
    Ok(())
}

/// Pure inspection reconstructs source from its complete preserved body, then
/// compares every model field and the exact generated diagnostics. Only the owned
/// pending-to-verified review transition and outer selection may differ.
/// It grants no input, execution, or review-clearing authority.
pub(crate) fn inspect_role(component: &Component) -> Result<Option<Role>> {
    if !component.metadata.contains_key(BINDING) {
        ensure!(
            component
                .metadata
                .get("DraftReview")
                .and_then(|r| r.get("Generator"))
                .and_then(Value::as_str)
                != Some(GENERATOR),
            "K1 generator lost binding"
        );
        return Ok(None);
    }
    let cancelled = AtomicBool::new(false);
    let binding = &component.metadata[BINDING];
    let document = text(binding, "SourceGuideDocument")?;
    let source = source(document, &cancelled)?;
    let original: Component = serde_json::from_value(
        binding
            .get("InputComponent")
            .cloned()
            .context("K1 original owner absent")?,
    )?;
    original_owner(&source, &original)?;
    let choice = match binding.get("ExplicitGalaxyChoice") {
        None => None,
        Some(Value::String(choice)) => Some(choice.as_str()),
        _ => bail!("K1 explicit choice shape differs"),
    };
    let expected = expected(&original, document, choice, &cancelled)?;
    compare_replayed(component, &expected)?;
    Ok(Some(role(&component.guid)?))
}

/// Re-admit actual archives and original source on every replay. No snapshot of
/// serialized observations substitutes for this fresh runtime input check.
pub(crate) fn replay(
    component: &Component,
    archives_root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Component> {
    let _candidates = readmit_pending(component, archives_root, options, cancelled)?;
    let binding = &component.metadata[BINDING];
    let original: Component = serde_json::from_value(binding["InputComponent"].clone())?;
    expected(
        &original,
        text(binding, "SourceGuideDocument")?,
        binding["ExplicitGalaxyChoice"].as_str(),
        cancelled,
    )
}
fn readmit_pending(
    component: &Component,
    archives_root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<k1_widescreen::CandidateContract> {
    inspect_role(component)?.context("K1 binding absent")?;
    let binding = &component.metadata[BINDING];
    let document = text(binding, "SourceGuideDocument")?;
    let source = source(document, cancelled)?;
    let original: Component = serde_json::from_value(binding["InputComponent"].clone())?;
    let candidates = admit_materializer(document, &source, archives_root, options, cancelled)?;
    if role(&original.guid)? == Role::Galaxy {
        let choice = binding["ExplicitGalaxyChoice"]
            .as_str()
            .context("Explicit Galaxy Vanilla choice absent")?;
        galaxy_map::admit_vanilla(&original, choice, archives_root, options, cancelled)?;
    }
    check(cancelled)?;
    Ok(candidates)
}

/// Materials returned only after fresh whole-component input readmission. The
/// engine still owns the root lock, transaction and every game publication.
pub(crate) struct ReplayMaterials {
    role: Role,
    candidates: k1_widescreen::CandidateContract,
}
impl ReplayMaterials {
    pub(crate) fn role(&self) -> Role {
        self.role
    }
    pub(crate) fn hrmenus_candidate(
        &self,
        baseline: &[u8],
        baseline_ini: Option<&[u8]>,
        cancelled: &AtomicBool,
    ) -> Result<k1_widescreen::CandidateOutput> {
        ensure!(self.role == Role::Hrmenus, "Wrong K1 candidate role");
        self.candidates.hrmenus(baseline, baseline_ini, cancelled)
    }
    pub(crate) fn fourgb_candidate(
        &self,
        hrmenus: &[u8],
        cancelled: &AtomicBool,
    ) -> Result<k1_widescreen::CandidateOutput> {
        ensure!(self.role == Role::Fourgb, "Wrong K1 candidate role");
        self.candidates.fourgb(hrmenus, cancelled)
    }
    pub(crate) fn galaxy_overlay(
        &self,
        cancelled: &AtomicBool,
    ) -> Result<k1_widescreen::CandidateOutput> {
        ensure!(self.role == Role::Galaxy, "Wrong K1 candidate role");
        self.candidates.galaxy_overlay(cancelled)
    }
}

fn compare_replayed(actual: &Component, expected: &Component) -> Result<()> {
    let mut actual_value = serde_json::to_value(actual)?;
    let mut expected_value = serde_json::to_value(expected)?;
    actual_value
        .as_object_mut()
        .context("Invalid K1 model")?
        .remove("DraftReview");
    expected_value
        .as_object_mut()
        .context("Invalid K1 replay")?
        .remove("DraftReview");
    // Outer selection belongs to the phase/selection adapter, not authoring.
    actual_value.as_object_mut().unwrap().remove("IsSelected");
    expected_value.as_object_mut().unwrap().remove("IsSelected");
    ensure!(
        actual_value == expected_value,
        "K1 full source, binding or instruction differs from independent replay"
    );
    let review = actual
        .metadata
        .get("DraftReview")
        .context("Missing K1 draft record")?;
    let pending = &expected.metadata["DraftReview"];
    for key in ["Generator", "Derivations", "Result"] {
        ensure!(
            review.get(key) == pending.get(key),
            "K1 diagnostics differ from independent replay"
        );
    }
    Ok(())
}

pub(crate) fn replay_digest(component: &Component) -> Result<String> {
    // Stable across save/load object key ordering; never used as sole authority.
    fn sort(value: &mut Value) {
        match value {
            Value::Object(object) => {
                let mut entries: Vec<_> = std::mem::take(object).into_iter().collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                for (key, mut child) in entries {
                    sort(&mut child);
                    object.insert(key, child);
                }
            }
            Value::Array(items) => items.iter_mut().for_each(sort),
            _ => {}
        }
    }
    let mut value = serde_json::to_value(component)?;
    value
        .as_object_mut()
        .context("Invalid K1 replay")?
        .remove("IsSelected");
    sort(&mut value);
    Ok(sha(&serde_json::to_vec(&value)?))
}

pub(crate) fn verification(replayed: &Component) -> Result<Value> {
    Ok(
        json!({"Reviewer":draft::review::VERIFIER_NAME,"Kind":draft::review::VERIFICATION_KIND,
        "ToolVersion":concat!("ModSync.rs/",env!("CARGO_PKG_VERSION")),
        "ArchiveSha256":PLAN_SHA,"ArchiveFilename":"k1-manual-plan.json",
        "ReplayDigest":replay_digest(replayed)?,"IdentityVersion":2}),
    )
}

/// A public receipt hash is never authority. This regenerates all instructions,
/// full source owner context and actual archive/member bytes before each action.
pub(crate) fn verify_runtime(
    component: &Component,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Option<ReplayMaterials>> {
    let Some(role) = inspect_role(component)? else {
        return Ok(None);
    };
    let review = &component.metadata["DraftReview"];
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
            && review["RequiresReview"] == false,
        "K1 action needs dedicated independent review"
    );
    let candidates = readmit_pending(component, root, options, cancelled)?;
    let binding = &component.metadata[BINDING];
    let original: Component = serde_json::from_value(binding["InputComponent"].clone())?;
    let replayed = expected(
        &original,
        text(binding, "SourceGuideDocument")?,
        binding["ExplicitGalaxyChoice"].as_str(),
        cancelled,
    )?;
    ensure!(
        review.get("Verification") == Some(&verification(&replayed)?)
            && review["ReviewedIdentity"].as_str()
                == Some(draft::review::identity(component)?.as_str()),
        "K1 runtime review does not match fresh full-component replay"
    );
    Ok(Some(ReplayMaterials { role, candidates }))
}

pub(crate) fn galaxy_contract(
    component: &Component,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<galaxy_map::VanillaContract> {
    ensure!(
        inspect_role(component)? == Some(Role::Galaxy),
        "Wrong Galaxy owner"
    );
    let binding = &component.metadata[BINDING];
    let original = serde_json::from_value(binding["InputComponent"].clone())?;
    galaxy_map::admit_vanilla(&original, "Vanilla", root, options, cancelled)
}

const SCHEDULE: &str = "K1WidescreenSchedule";
const INACTIVE_CUTSCENES: &str = "7ca6f5d5-4ef3-81c1-a805-415c2c30b47c";
const INACTIVE_CUTSCENES_URL: &str = "https://www.nexusmods.com/kotor/mods/1306";

/// The Linux projection has TWO inactive guide rules, but exactly ONE of them
/// is a converted component. Quicker TSLPatching is guide-only/null GUID.
/// Therefore the only admitted converted inventories are original197 or exact196;
/// old195 that additionally omits4GB is never a full native widescreen schedule.
fn source_partition<'a>(source: &'a Source, guide: &Guide) -> Result<Vec<&'a Component>> {
    ensure!(
        source.canonical.components.len() == 197,
        "Original K1 source inventory differs"
    );
    let inactive = source
        .canonical
        .components
        .iter()
        .find(|c| c.guid == INACTIVE_CUTSCENES)
        .context("Missing pinned inactive cutscene owner")?;
    ensure!(
        inactive.metadata.get("Heading") == Some(&json!("Remastered Cutscenes"))
            && inactive.name == "KOTOR Remastered Cutscenes"
            && inactive.resource_registry.len() == 1
            && inactive
                .resource_registry
                .contains_key(INACTIVE_CUTSCENES_URL),
        "Inactive K1 partition heading, name or URL differs from pinned source"
    );
    let projected = guide.components.len() == 196;
    ensure!(
        projected || guide.components.len() == 197,
        "K1 accepts only full197 or closed196 converted partition"
    );
    let retained: Vec<_> = source
        .canonical
        .components
        .iter()
        .filter(|c| !projected || c.guid != INACTIVE_CUTSCENES)
        .collect();
    ensure!(
        guide
            .components
            .iter()
            .zip(&retained)
            .all(|(actual, original)| actual.guid == original.guid)
            && guide.components.len() == retained.len(),
        "K1 active source GUID inventory/order differs"
    );
    Ok(retained)
}

fn schedule_record(source: &Source, guide: &Guide) -> Result<Value> {
    source_partition(source, guide)?;
    let ids: Vec<_> = source
        .canonical
        .components
        .iter()
        .map(|c| c.guid.as_str())
        .collect();
    // Active order is already exhaustively committed by ModSyncSourceOrder.
    // Keep this policy invariant across the one documented inactive partition.
    let inactive: Vec<_> = source
        .canonical
        .components
        .iter()
        .enumerate()
        .filter(|(_, c)| c.guid == INACTIVE_CUTSCENES)
        .map(|(index, c)| {
            Ok(
                json!({"Guid":c.guid,"Heading":c.metadata["Heading"],"Url":INACTIVE_CUTSCENES_URL,
            "OriginalIndex":index,"OriginalComponentSha256":sha(&serde_json::to_vec(c)?)}),
            )
        })
        .collect::<Result<_>>()?;
    let record = json!({"Schema":1,"ProfileId":k1_widescreen::PROFILE_ID,"SourceGuideSha256":GUIDE_SHA,
        "SourcePlanSha256":PLAN_SHA,"OriginalSourceOrder":ids,
        "AllowedInactiveConvertedComponents":inactive,
        "GuideOnlyHelper":{"Rule":"linux_unsupported_helper","Section":"Optional Install Assistant - Quicker TSLPatching",
            "Heading":"Quicker TSLPatching","Url":"https://www.nexusmods.com/kotor/mods/1214?tab=description","Converted":false,"Scope":"guide-only-not-converted"},
        "ExecutionException":"all-selected-widescreen-content-in-source-order-then-whole-Vanilla-Galaxy-base-then-overlay-then-FourGB-last",
        "DeferredWholeComponents":[GALAXY,FOURGB],"GalaxyChoice":"Vanilla","GalaxyNamespaceIndex":0});
    ensure_portable_metadata(&record, 0)?;
    Ok(record)
}

/// One explicit fixed-profile authoring operation; no filesystem mutation of inputs.
/// Source GUID inventory and WidescreenOnly provenance are retained verbatim.
pub fn bind_guide(
    guide: &mut Guide,
    explicit_profile: &str,
    explicit_galaxy_choice: &str,
    source_guide: &Path,
    archives_root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<()> {
    ensure!(
        explicit_profile == k1_widescreen::PROFILE_ID && explicit_galaxy_choice == "Vanilla",
        "Explicit fixed K1 profile and Vanilla required"
    );
    let bytes = bounded_read(source_guide, INPUT_LIMIT, cancelled)?;
    let document = std::str::from_utf8(&bytes)?;
    let source = source(document, cancelled)?;
    source_partition(&source, guide)?;
    let mut bound = guide.clone();
    for component in &mut bound.components {
        if [HRMENUS, GALAXY, FOURGB].contains(&component.guid.as_str()) {
            let selected = component.is_selected;
            // Complete original model still required; selection is adapter-owned.
            let original = source
                .canonical
                .components
                .iter()
                .find(|c| c.guid == component.guid)
                .context("Missing K1 owner")?;
            let mut compare = component.clone();
            compare.is_selected = original.is_selected;
            original_owner(&source, &compare)?;
            let choice = (compare.guid == GALAXY).then_some(explicit_galaxy_choice);
            bind(
                &mut compare,
                explicit_profile,
                choice,
                source_guide,
                archives_root,
                options,
                cancelled,
            )?;
            compare.is_selected = selected;
            *component = compare;
        }
    }
    let schedule = schedule_record(&source, &bound)?;
    let active_inventory = json!(
        bound
            .components
            .iter()
            .map(|c| c.guid.as_str())
            .collect::<Vec<_>>()
    );
    bound.metadata.insert(SCHEDULE.into(), schedule);
    bound
        .metadata
        .insert("ModSyncSourceOrder".into(), active_inventory);
    execution_order(&bound)?.context("K1 closed schedule absent")?;
    *guide = bound;
    Ok(())
}

/// Model/phase hooks must call this before generic source-order handling. The
/// original exhaustive source order stays in metadata; only execution is deferred.
pub(crate) fn execution_order(guide: &Guide) -> Result<Option<Vec<usize>>> {
    let mut bound = false;
    for component in &guide.components {
        ensure!(
            !component.options.iter().any(contains_native),
            "K1 native operations cannot occur inside options"
        );
        if native_marker(component) {
            ensure!(
                [HRMENUS, GALAXY, FOURGB].contains(&component.guid.as_str())
                    && component.metadata.contains_key(BINDING),
                "K1 native operation requires its original top-level owner"
            );
            bound = true;
        }
    }
    let Some(record) = guide.metadata.get(SCHEDULE) else {
        ensure!(
            !bound,
            "K1 native bindings require their explicit closed schedule"
        );
        return Ok(None);
    };
    let owner = guide
        .components
        .iter()
        .find(|c| c.guid == HRMENUS)
        .context("Missing K1 HRMenus owner")?;
    let document = text(&owner.metadata[BINDING], "SourceGuideDocument")?;
    let source = source(document, &AtomicBool::new(false))?;
    let retained = source_partition(&source, guide)?;
    ensure!(
        record == &schedule_record(&source, guide)?,
        "K1 closed schedule differs"
    );
    let inventory: Vec<_> = retained.iter().map(|c| c.guid.as_str()).collect();
    ensure!(
        guide.metadata.get("ModSyncSourceOrder") == Some(&json!(inventory))
            && guide
                .components
                .iter()
                .zip(&retained)
                .all(|(actual, original)| actual.metadata.get("WidescreenOnly")
                    == original.metadata.get("WidescreenOnly")),
        "K1 active source order or WidescreenOnly provenance changed"
    );
    for guid in [HRMENUS, GALAXY, FOURGB] {
        let component = guide
            .components
            .iter()
            .find(|c| c.guid == guid)
            .context("Missing K1 scheduled owner")?;
        inspect_role(component)?.context("Unbound K1 scheduled owner")?;
    }
    let active = [HRMENUS, GALAXY, FOURGB].map(|guid| {
        guide
            .components
            .iter()
            .find(|c| c.guid == guid)
            .unwrap()
            .is_selected
    });
    ensure!(
        active.iter().all(|v| *v) || active.iter().all(|v| !*v),
        "K1 profile requires all three owners selected together"
    );
    let mut order: Vec<_> = (0..guide.components.len())
        .filter(|&i| {
            ![GALAXY, FOURGB].contains(&guide.components[i].guid.as_str())
                && !crate::phase::widescreen(&guide.components[i])
        })
        .collect();
    order.extend((0..guide.components.len()).filter(|&i| {
        ![GALAXY, FOURGB].contains(&guide.components[i].guid.as_str())
            && crate::phase::widescreen(&guide.components[i])
    }));
    for guid in [GALAXY, FOURGB] {
        order.push(
            guide
                .components
                .iter()
                .position(|c| c.guid == guid)
                .unwrap(),
        );
    }
    Ok(Some(order))
}

fn native_marker(component: &Component) -> bool {
    component.metadata.contains_key(BINDING)
        || component
            .metadata
            .get("DraftReview")
            .and_then(|record| record.get("Generator"))
            .and_then(Value::as_str)
            == Some(GENERATOR)
        || component
            .instructions
            .iter()
            .any(|instruction| instruction.action == "K1Widescreen")
}

pub(crate) fn contains_native(component: &Component) -> bool {
    native_marker(component) || component.options.iter().any(contains_native)
}

pub(crate) fn deferred(component: &Component) -> bool {
    component.metadata.contains_key(BINDING) && [GALAXY, FOURGB].contains(&component.guid.as_str())
}
