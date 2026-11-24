//! Closed source-master overlays over the unchanged original UCO patch recipes.
use crate::{Component, Guide, archive, draft};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::{OnceLock, atomic::AtomicBool},
};

pub(super) const GENERATOR: &str = "reviewed-uco-source-masters-v1";
const BINDING: &str = "ReviewedUcoMasterBinding";
const K1: &str = "4a452675-10a9-81d2-b825-43731624248a";
const K2: &str = "1acdc42b-0b9f-836b-8989-bae84915273c";
const PRIORS1: &[(&str, usize)] = &[
    ("f01daefc-b4f6-8294-bfa1-2a95b564a1f2", 3),
    ("6fb1971d-5775-85e5-9e77-ecc68a896e84", 5),
    ("7e99b1de-b8a1-8ee4-9318-bbacaa52311f", 15),
    ("b4bb7dc6-8cfb-8015-b439-5b74895ea4ed", 36),
    ("f966fc1f-6b47-88fc-9050-07b1371eb56a", 46),
    ("4188c324-caeb-8ff6-9512-93e481d8b0ae", 48),
    ("83226d49-f919-82a5-923e-c1d00f727c40", 144),
];
const PRIORS2: &[(&str, usize)] = &[
    ("bee5a72b-d748-8f83-8261-81758a9bfc56", 1),
    ("216eb8b6-3076-8a92-bd7d-012862b10b32", 8),
    ("5694d896-2945-86ac-9aba-34b7c922d4c1", 12),
    ("00ab0357-31d9-87f7-b435-5ac18ecf7141", 24),
    ("a2072211-a255-8f3a-9068-748319eef496", 25),
    ("c1bd1e7c-85be-8469-93d8-e3aba3777809", 35),
    ("97d39844-42a1-8664-98ef-c02a1f2c6b2a", 39),
    ("7b678426-adfb-8f47-a090-bf8e263dc37f", 41),
    ("39cd6164-573d-8d0d-875e-6ac5c63b4fa7", 49),
    ("d440d1d2-8dc0-8b02-8b6c-bf6c5f607de5", 56),
    ("9bc989ce-8165-8ad5-a056-ffac404ffd45", 64),
    ("6b1a2cc1-49d5-80ce-920c-8ab7eed947da", 132),
];
struct Spec {
    owner: &'static str,
    index: usize,
    book: &'static str,
    guide: &'static [u8],
    guide_sha: &'static str,
    plan: &'static [u8],
    plan_sha: &'static str,
    count: usize,
    priors: &'static [(&'static str, usize)],
}
const SPECS: [Spec; 2] = [
    Spec {
        owner: K1,
        index: 181,
        book: "38a1231f12632d0765b9741343db92ee5858f27405ab2bdb8667119ad71e8d6b",
        guide: include_bytes!("../source_plan_k1.md"),
        guide_sha: "44067788c6cb7a83b38d468817f273ce5cc901cd5e000c0f9ff65f8af15fc6ed",
        plan: include_bytes!("../../../tools/modsync-ci/fixtures/ci/k1-manual-plan.json"),
        plan_sha: "1b402ba085625c4863795069eaacaef0f53e8ea7179e56a5bd150d88eb9c0a74",
        count: 197,
        priors: PRIORS1,
    },
    Spec {
        owner: K2,
        index: 139,
        book: "f85240122267d088f7b52c1746c2edb2fdb8795846889f4158dcf59b4fd91d60",
        guide: include_bytes!("../source_plan_k2.md"),
        guide_sha: "f442bdf574031623ed577609730d887069b9bc8e38e8e2f38ecc43a7e7487d5f",
        plan: include_bytes!("../../../tools/modsync-ci/fixtures/ci/k2-manual-plan.json"),
        plan_sha: "0add8c1aeb7cdfad3df4b21761e135c150aa7832cf3fc0edabe01a91245a9258",
        count: 151,
        priors: PRIORS2,
    },
];
struct Authority {
    source: Guide,
    value: Value,
    digest: String,
}
static AUTHORITIES: OnceLock<std::result::Result<Vec<Authority>, String>> = OnceLock::new();
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn authorities() -> Result<&'static Vec<Authority>> {
    AUTHORITIES.get_or_init(||( || ->Result<Vec<Authority>> {
        let mut result=Vec::new();
        for spec in &SPECS {
            ensure!(hash(spec.guide)==spec.guide_sha && hash(spec.plan)==spec.plan_sha,"UCO original source hashes differ");
            let mut raw=Guide::parse(std::str::from_utf8(spec.guide)?,"md")?;
            draft::generate(&mut raw);
            let source=Guide::parse_preserving_selection(&serde_json::to_string(&raw)?,"json")?;
            ensure!(source.components.len()==spec.count && source.components[spec.index].guid==spec.owner,"UCO original source owner index differs");
            let owner=&source.components[spec.index];
            ensure!(owner.metadata.get("UnresolvedMasters")==Some(&json!(["Various Texture Mods"])) && owner.dependencies.len()==1,
                "UCO original collective master/base dependency changed");
            let mut priors=Vec::new();
            for &(guid,index) in spec.priors {
                let prior=source.components.get(index).context("UCO predecessor index absent")?;
                ensure!(prior.guid==guid && index<spec.index && prior.is_selected,"UCO compiled predecessor source differs");
                priors.push(json!({"Guid":guid,"Index":index,"Original":prior}));
            }
            ensure!(spec.priors.iter().any(|(guid,_)|*guid==owner.dependencies[0]),"UCO existing base must remain required");
            let value=json!({"Policy":"closed-original-uco-source-masters-v1","Owner":spec.owner,"OwnerIndex":spec.index,
                "SourceGuideSha256":spec.guide_sha,"SourcePlanSha256":spec.plan_sha,"LegacyBookSha256":spec.book,
                "OriginalOwner":owner,"RequiredSourceDefinitions":priors,"OriginalUnresolvedMasters":["Various Texture Mods"],
                "InstallationRequirement":"actual-runtime-replay-and-owned-committed-prefix-before-effects-v1"});
            let digest=hash(&serde_json::to_vec(&value)?);
            result.push(Authority {source,value,digest});
        }
        Ok(result)
    })().map_err(|e|format!("{e:#}"))).as_ref().map_err(|e|anyhow::anyhow!(e.clone()))
}
fn scope(guid: &str) -> Result<(&'static Spec, &'static Authority)> {
    let index = SPECS
        .iter()
        .position(|s| s.owner == guid)
        .context("Unknown UCO master owner")?;
    Ok((&SPECS[index], &authorities()?[index]))
}
pub(super) fn own(c: &Component) -> bool {
    c.metadata.contains_key(BINDING)
        || c.metadata
            .get("DraftReview")
            .and_then(|v| v.get("Generator"))
            .and_then(Value::as_str)
            == Some(GENERATOR)
}
fn expected_dependencies(spec: &Spec, authority: &Authority) -> Vec<String> {
    let mut result = authority.source.components[spec.index].dependencies.clone();
    for &(guid, _) in spec.priors {
        if !result.iter().any(|g| g == guid) {
            result.push(guid.into());
        }
    }
    result
}
fn binding(spec: &Spec, a: &Authority) -> Value {
    json!({"Schema":1,"AuthoritySha256":a.digest,"Authority":a.value,"LegacyBookSha256":spec.book,
        "OriginalUnresolvedMasters":["Various Texture Mods"],"OriginalDependencies":a.source.components[spec.index].dependencies,
        "RequiredPredecessors":spec.priors.iter().map(|(g,_)|g).collect::<Vec<_>>()})
}
fn claim(c: &Component) -> Result<()> {
    let (spec, a) = scope(&c.guid)?;
    ensure!(
        c.options.is_empty()
            && c.metadata.get(BINDING) == Some(&binding(spec, a))
            && c.metadata.get("UnresolvedMasters") == Some(&json!([]))
            && c.dependencies == expected_dependencies(spec, a),
        "UCO master authority, preserved originals or exact predecessor relation changed"
    );
    Ok(())
}
pub(super) fn required_predecessors(c: &Component) -> Result<Vec<String>> {
    claim(c)?;
    let (spec, _) = scope(&c.guid)?;
    Ok(spec.priors.iter().map(|(g, _)| g.to_string()).collect())
}
fn comparable(c: &Component) -> Result<Value> {
    let mut value = serde_json::to_value(c)?;
    let map = value
        .as_object_mut()
        .context("Invalid UCO source context")?;
    map.remove("IsSelected");
    map.remove("DraftReview");
    Ok(value)
}
fn source_definition(c: &Component) -> Result<Component> {
    // These declarations are definition comparisons only. Actual runtime must
    // replay each predecessor's archives and commit its owned guide-hash prefix.
    if let Some(origin) = super::super::reviewed_manual::community_patch_origin(c)? {
        return Ok(origin.original().clone());
    }
    if let Some(origin) = super::super::reviewed_manual::declared_original_definition(c)? {
        return Ok(origin);
    }
    if let Some(origin) = super::super::source_plan_namespace::declared_original_definition(c)? {
        return Ok(origin);
    }
    if let Some(origin) = super::super::source_plan_patcher::declared_original_definition(c)? {
        return Ok(origin);
    }
    if let Some(origin) = crate::vendor_installer::declared_original_definition(c)? {
        return Ok(origin);
    }
    if let Some(origin) = super::super::resolved::declared_original_definition(c)? {
        return Ok(origin);
    }
    Ok(c.clone())
}
pub(super) fn validate_guide(guide: &Guide) -> Result<()> {
    for owner in guide.components.iter().filter(|c| own(c)) {
        claim(owner)?;
        if !owner.is_selected {
            continue;
        }
        let (spec, a) = scope(&owner.guid)?;
        let owner_positions = guide
            .components
            .iter()
            .enumerate()
            .filter(|(_, c)| c.guid == spec.owner)
            .collect::<Vec<_>>();
        ensure!(
            owner_positions.len() == 1,
            "UCO master owner must occur exactly once"
        );
        let mut previous = None;
        let mut differing = Vec::new();
        for &(guid, index) in spec.priors {
            let positions = guide
                .components
                .iter()
                .enumerate()
                .filter(|(_, c)| c.guid == guid)
                .collect::<Vec<_>>();
            ensure!(
                positions.len() == 1
                    && positions[0].1.is_selected
                    && positions[0].0 < owner_positions[0].0,
                "UCO requires every exact selected source predecessor strictly earlier"
            );
            ensure!(
                previous.is_none_or(|p| p < positions[0].0),
                "UCO predecessor source order changed"
            );
            previous = Some(positions[0].0);
            let bound = comparable(&source_definition(positions[0].1)?)?;
            let original = comparable(&a.source.components[index])?;
            if bound != original {
                // Name the prerequisite and its differing top-level fields: twelve
                // predecessors share this check, and the values may hold guide text.
                let fields = original
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(key, _)| key)
                    .chain(bound.as_object().into_iter().flatten().map(|(key, _)| key))
                    .filter(|key| bound.get(key.as_str()) != original.get(key.as_str()))
                    .collect::<std::collections::BTreeSet<_>>();
                differing.push(format!("{guid} ({}) in {fields:?}", positions[0].1.name));
            }
        }
        // Every differing prerequisite is reported at once, not only the first.
        ensure!(
            differing.is_empty(),
            "UCO prerequisite differs from its complete compiled original definition: {}",
            differing.join("; ")
        );
        if let Some(order) = guide.metadata.get("ModSyncSourceOrder") {
            let order = order.as_array().context("Invalid UCO source order")?;
            let mut required = spec
                .priors
                .iter()
                .map(|(guid, _)| *guid)
                .collect::<Vec<_>>();
            required.push(spec.owner);
            let mut previous = None;
            for guid in required {
                let positions = order
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.as_str() == Some(guid))
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                ensure!(
                    positions.len() == 1 && previous.is_none_or(|p| p < positions[0]),
                    "Explicit source order violates UCO predecessor closure"
                );
                previous = Some(positions[0]);
            }
        }
    }
    Ok(())
}
fn overlay(mut c: Component) -> Result<Component> {
    let (spec, a) = scope(&c.guid)?;
    let original = &a.source.components[spec.index];
    ensure!(
        c.options.is_empty()
            && c.dependencies == original.dependencies
            && c.metadata.get("UnresolvedMasters") == original.metadata.get("UnresolvedMasters")
            && c.metadata["AuthoredRecipeBinding"]["DatabookSha256"] == spec.book
            && c.metadata["AuthoredRecipeBinding"]["InputComponent"]
                == serde_json::to_value(original)?,
        "UCO overlay requires the unchanged original compiled legacy recipe context"
    );
    c.dependencies = expected_dependencies(spec, a);
    c.metadata.insert("UnresolvedMasters".into(), json!([]));
    c.metadata.insert(BINDING.into(), binding(spec, a));
    c.metadata
        .get_mut("DraftReview")
        .and_then(Value::as_object_mut)
        .context("Missing UCO legacy pending review")?
        .insert("Generator".into(), GENERATOR.into());
    claim(&c)?;
    Ok(c)
}
pub(super) fn bind_in_guide(guide: &mut Guide) -> Result<()> {
    for c in &mut guide.components {
        if (c.guid == K1 || c.guid == K2)
            && !own(c)
            && c.metadata.contains_key("AuthoredRecipeBinding")
        {
            *c = overlay(c.clone())?;
        }
    }
    validate_guide(guide)
}
pub(super) fn replay(
    c: &Component,
    root: &Path,
    options: &archive::Options,
    cancel: &AtomicBool,
) -> Result<Component> {
    claim(c)?;
    let mut expected = overlay(super::super::reviewed_manual::regenerate_owner(
        &c.guid, root, options, cancel,
    )?)?;
    expected.is_selected = c.is_selected;
    let mut actual = serde_json::to_value(c)?;
    let mut value = serde_json::to_value(&expected)?;
    for item in [&mut actual, &mut value] {
        item.as_object_mut()
            .context("Invalid UCO replay model")?
            .remove("DraftReview");
    }
    ensure!(
        actual == value,
        "Complete UCO master model differs from fresh original archive/recipe replay"
    );
    for key in ["Generator", "Derivations", "Result"] {
        ensure!(
            c.metadata["DraftReview"].get(key) == expected.metadata["DraftReview"].get(key),
            "UCO pending diagnostics changed"
        );
    }
    Ok(expected)
}
pub(super) fn verification(c: &Component) -> Result<Value> {
    claim(c)?;
    let (_, a) = scope(&c.guid)?;
    let mut value = serde_json::to_value(c)?;
    value
        .as_object_mut()
        .context("Invalid UCO identity")?
        .remove("IsSelected");
    Ok(
        json!({"Reviewer":draft::review::VERIFIER_NAME,"Kind":draft::review::VERIFICATION_KIND,"ToolVersion":concat!("ModSync.rs/",env!("CARGO_PKG_VERSION")),
        "ArchiveSha256":a.digest,"ArchiveFilename":"reviewed-uco-original-source-masters-v1","ReplayDigest":super::super::reviewed_manual::value_digest(&value)?,"IdentityVersion":2}),
    )
}
pub(super) fn verify_runtime(
    c: &Component,
    root: &Path,
    options: &archive::Options,
    cancel: &AtomicBool,
) -> Result<()> {
    let pending = replay(c, root, options, cancel)?;
    let record = &c.metadata["DraftReview"];
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
            && record["ReviewedIdentity"].as_str() == Some(draft::review::identity(c)?.as_str()),
        "UCO runtime requires exact independent replay approval"
    );
    Ok(())
}
