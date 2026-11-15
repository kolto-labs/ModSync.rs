//! Exact checked-in file-effect recipes, independently replayed from pinned archives.
//! This is not a prose interpreter or a general manual-plan execution facility.
use crate::{Component, Guide, Instruction, archive, draft};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

pub(crate) const GENERATOR: &str = "reviewed-manual-file-effects";
pub(crate) const PATCHER_GENERATOR: &str = "reviewed-manual-native-patcher";
const BINDING: &str = "AuthoredRecipeBinding";
const CP1: &str = "6fb1971d-5775-85e5-9e77-ecc68a896e84";
const CP2: &str = "216eb8b6-3076-8a92-bd7d-012862b10b32";
const CP1_BOOK: &str = "923c0a5ebe52615247717bd3bab6ce48bad98cc664825a840465025b2b7e1fbc";
const CP2_BOOK: &str = "c8f3877165f3da1adc8d5d76cc86ef8e803cc5d2a012aa22ef1f1fb25723db74";
const CP_EXPANDED: u64 = 384 * 1024 * 1024;
const CP_MANIFEST_POLICY: &str = "cp-raw-staged-package-v1";
fn community_guid(guid: &str) -> bool {
    matches!(guid, CP1 | CP2)
}
struct BookSpec {
    filename: &'static str,
    content: &'static str,
    sha256: &'static str,
    guide_sha256: &'static str,
    plan_sha256: &'static str,
    count: usize,
    kind: &'static str,
    generator: &'static str,
}
const BOOKS: [BookSpec; 9] = [
    BookSpec {
        filename: "reviewed_manual_raw_inputs_k1.json",
        content: include_str!("reviewed_manual_raw_inputs_k1.json"),
        sha256: "f3dadc5ad04f7c501753ca38bf50492e067c54b8ed1df401910780d3ca836d93",
        guide_sha256: "44067788c6cb7a83b38d468817f273ce5cc901cd5e000c0f9ff65f8af15fc6ed",
        plan_sha256: "1b402ba085625c4863795069eaacaef0f53e8ea7179e56a5bd150d88eb9c0a74",
        count: 2,
        kind: "ReviewedManualK1LooseInputs",
        generator: GENERATOR,
    },
    BookSpec {
        filename: "reviewed_manual_k2.json",
        content: include_str!("reviewed_manual_k2.json"),
        sha256: "f85240122267d088f7b52c1746c2edb2fdb8795846889f4158dcf59b4fd91d60",
        guide_sha256: "f442bdf574031623ed577609730d887069b9bc8e38e8e2f38ecc43a7e7487d5f",
        plan_sha256: "0add8c1aeb7cdfad3df4b21761e135c150aa7832cf3fc0edabe01a91245a9258",
        count: 57,
        kind: "ReviewedManualK2FileEffects",
        generator: GENERATOR,
    },
    BookSpec {
        filename: "reviewed_manual_k1.json",
        content: include_str!("reviewed_manual_k1.json"),
        sha256: "b04b7331b5001f81698feb5571460ea9e2e486d032956abb22967d704254c338",
        guide_sha256: "44067788c6cb7a83b38d468817f273ce5cc901cd5e000c0f9ff65f8af15fc6ed",
        plan_sha256: "1b402ba085625c4863795069eaacaef0f53e8ea7179e56a5bd150d88eb9c0a74",
        count: 66,
        kind: "ReviewedManualK1FileEffects",
        generator: GENERATOR,
    },
    BookSpec {
        filename: "reviewed_manual_single_ini_k2.json",
        content: include_str!("reviewed_manual_single_ini_k2.json"),
        sha256: "2eee9ca0321722919306743903b18a872c1be1954c547963b3a737a270679fbe",
        guide_sha256: "f442bdf574031623ed577609730d887069b9bc8e38e8e2f38ecc43a7e7487d5f",
        plan_sha256: "0add8c1aeb7cdfad3df4b21761e135c150aa7832cf3fc0edabe01a91245a9258",
        count: 4,
        kind: "ReviewedManualK2NativePatcher",
        generator: PATCHER_GENERATOR,
    },
    BookSpec {
        filename: "reviewed_manual_baodur_k2.json",
        content: include_str!("reviewed_manual_baodur_k2.json"),
        sha256: "f71e9fbf5c3e0bcf300d604c453b44fcdb9f98c8ff82c6f5f568a06a07fad577",
        guide_sha256: "f442bdf574031623ed577609730d887069b9bc8e38e8e2f38ecc43a7e7487d5f",
        plan_sha256: "0add8c1aeb7cdfad3df4b21761e135c150aa7832cf3fc0edabe01a91245a9258",
        count: 1,
        kind: "ReviewedManualK2NativePatcher",
        generator: PATCHER_GENERATOR,
    },
    BookSpec {
        filename: "reviewed_manual_log_delete_k2.json",
        content: include_str!("reviewed_manual_log_delete_k2.json"),
        sha256: "b4850e572859cf4c22616be584a0ca02411fb2ddcd3ae4f23fcb05d7a97af13a",
        guide_sha256: "f442bdf574031623ed577609730d887069b9bc8e38e8e2f38ecc43a7e7487d5f",
        plan_sha256: "0add8c1aeb7cdfad3df4b21761e135c150aa7832cf3fc0edabe01a91245a9258",
        count: 2,
        kind: "ReviewedManualK2NativePatcher",
        generator: PATCHER_GENERATOR,
    },
    BookSpec {
        filename: "reviewed_manual_installer_choice_k2.json",
        content: include_str!("reviewed_manual_installer_choice_k2.json"),
        sha256: "e50408f7108f9c0a6b1829d9cfa70b1004721de6f9162437271aa46e59df9b9b",
        guide_sha256: "f442bdf574031623ed577609730d887069b9bc8e38e8e2f38ecc43a7e7487d5f",
        plan_sha256: "0add8c1aeb7cdfad3df4b21761e135c150aa7832cf3fc0edabe01a91245a9258",
        count: 2,
        kind: "ReviewedManualK2NativePatcher",
        generator: PATCHER_GENERATOR,
    },
    BookSpec {
        filename: "reviewed_manual_community_patch_k1.json",
        content: include_str!("reviewed_manual_community_patch_k1.json"),
        sha256: CP1_BOOK,
        guide_sha256: "44067788c6cb7a83b38d468817f273ce5cc901cd5e000c0f9ff65f8af15fc6ed",
        plan_sha256: "1b402ba085625c4863795069eaacaef0f53e8ea7179e56a5bd150d88eb9c0a74",
        count: 1,
        kind: "ReviewedManualK1NativePatcher",
        generator: PATCHER_GENERATOR,
    },
    BookSpec {
        filename: "reviewed_manual_community_patch_k2.json",
        content: include_str!("reviewed_manual_community_patch_k2.json"),
        sha256: CP2_BOOK,
        guide_sha256: "f442bdf574031623ed577609730d887069b9bc8e38e8e2f38ecc43a7e7487d5f",
        plan_sha256: "0add8c1aeb7cdfad3df4b21761e135c150aa7832cf3fc0edabe01a91245a9258",
        count: 1,
        kind: "ReviewedManualK2NativePatcher",
        generator: PATCHER_GENERATOR,
    },
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Book {
    schema: u32,
    source_guide_sha256: String,
    source_plan_sha256: String,
    recipes: Vec<Recipe>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Recipe {
    component_guid: String,
    source_heading: String,
    source_component_index: usize,
    plan_step_index: usize,
    raw_input_component: Value,
    canonical_input_component: Value,
    canonicalization: Canonicalization,
    plan_step: Step,
    archives: Vec<Pin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    native_patcher: Option<NativeTranslation>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MemberPin {
    path: String,
    sha256: String,
    bytes: u64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeTranslation {
    operation_index: usize,
    extract_operation_index: usize,
    installer: MemberPin,
    changes_ini: MemberPin,
    inventory_sha256: String,
    certificate_sha256: String,
    arguments: String,
    destination: String,
    success_contract: String,
}
impl NativeTranslation {
    /// Archive-relative pins remain exact even when the installer has a wrapper
    /// directory. The engine stages that directory, not the extraction root.
    fn installer_parent(&self) -> Result<std::path::PathBuf> {
        let installer = literal(&self.installer.path)?;
        let parent = Path::new(&installer)
            .parent()
            .context("Native installer parent absent")?;
        ensure!(
            Path::new(&literal(&self.changes_ini.path)?) == parent.join("tslpatchdata/changes.ini"),
            "Native translation requires the installer's canonical sole INI"
        );
        Ok(parent.to_path_buf())
    }
}
/// Only these two compiled owners may delete their packaged stale log before
/// their native patch. This map is not recipe admission; find_recipe still
/// requires a hash-pinned compiled book and the full original owner/plan.
fn packaged_native_log(guid: &str) -> Option<&'static str> {
    match guid {
        "2de86e49-571f-8600-931e-5bc2ad5acebe" => Some("Rescaled Trandoshans/installlog.rtf"),
        "3eb56efe-013e-8095-80cf-a2584ba2cf94" => Some("Install/installlog.rtf"),
        _ => None,
    }
}

/// Admit the existing two-operation shape, either closed log-delete plan, or
/// the two exact compiled CP plans. K1 CLI/two-archive admission is confined to
/// its complete reconstructed four-operation source, never an index-only rule.
fn native_shape(recipe: &Recipe) -> Result<Option<&'static str>> {
    let translation = recipe
        .native_patcher
        .as_ref()
        .context("Missing native translation")?;
    if community_guid(&recipe.component_guid) {
        community_source(recipe)?;
        let k1 = recipe.component_guid == CP1;
        ensure!(
            translation.extract_operation_index == 0
                && translation.operation_index == if k1 { 2 } else { 1 }
                && recipe.archives.len() == if k1 { 2 } else { 1 }
                && recipe.plan_step.operations.len() == if k1 { 4 } else { 2 }
                && translation.installer.path == "INSTALL.exe"
                && translation.changes_ini.path == "tslpatchdata/changes.ini"
                && translation.arguments == "changes.ini"
                && translation.destination == "<<kotorDirectory>>"
                && translation.success_contract == "native-zero-warnings-changed-game-v1"
                && matches!(
                    recipe.plan_step.operations.first(),
                    Some(Operation::Extract { .. })
                )
                && if k1 {
                    matches!(recipe.plan_step.operations[2], Operation::Patcher { .. })
                } else {
                    matches!(recipe.plan_step.operations[1], Operation::GuiPatcher { .. })
                },
            "Community Patch closed original operation/member shape differs"
        );
        translation.installer_parent()?;
        return Ok(None);
    }
    ensure!(
        translation.extract_operation_index == 0 && recipe.archives.len() == 1,
        "Native translation requires one extraction and one archive"
    );
    let Some(Operation::Extract {
        source,
        destination,
    }) = recipe.plan_step.operations.first()
    else {
        anyhow::bail!("Native translation must begin with its exact extraction");
    };
    ensure!(
        token(source, "archives")? == recipe.archives[0].filename,
        "Native extraction differs from its sole compiled archive"
    );
    let slot = token(destination, "extract")?;
    let log = packaged_native_log(&recipe.component_guid);
    if let Some(member) = log {
        let step = match recipe.component_guid.as_str() {
            "2de86e49-571f-8600-931e-5bc2ad5acebe" => "step-122",
            "3eb56efe-013e-8095-80cf-a2584ba2cf94" => "step-118",
            _ => anyhow::bail!("Unknown native packaged-log owner"),
        };
        ensure!(
            recipe.plan_step.id == step
                && slot == format!("{step}/1")
                && translation.operation_index == 2
                && recipe.plan_step.operations.len() == 3
                && translation.installer_parent()?.join("installlog.rtf") == Path::new(member),
            "Native packaged-log translation differs from the closed owner slot/root/operation shape"
        );
        let Operation::Delete { path, missing_ok } = &recipe.plan_step.operations[1] else {
            anyhow::bail!("Native packaged-log translation requires its original deletion");
        };
        ensure!(
            missing_ok.is_none() && token(path, "extract")? == format!("{slot}/{member}"),
            "Native packaged-log deletion must be the exact original strict slot-local installlog.rtf"
        );
    } else {
        ensure!(
            translation.operation_index == 1 && recipe.plan_step.operations.len() == 2,
            "Native translation supports only the original extract/Patcher shape for this owner"
        );
    }
    ensure!(
        matches!(
            recipe.plan_step.operations.get(translation.operation_index),
            Some(Operation::GuiPatcher { .. })
        ),
        "Native translation must end at its original GUI operation"
    );
    Ok(log)
}

/// Cannot be constructed from authored JSON. Issued only for a known, cleared
/// compiled recipe and its exact native instruction.
pub(crate) struct NativePatcherContract {
    component_guid: String,
    translation: NativeTranslation,
    source_plan: Option<Box<super::source_plan_patcher::AdmittedSourcePlan>>,
    source_namespace: Option<Box<super::source_plan_namespace::AdmittedNamespace>>,
}
impl NativePatcherContract {
    /// Only the independently readmitted factory can construct this witness.
    /// Serialized pins or public hashes cannot select this admission path.
    pub(crate) fn from_source_plan(
        admitted: super::source_plan_patcher::AdmittedSourcePlan,
    ) -> Result<Self> {
        ensure!(
            super::source_plan_patcher::has_owner(admitted.component_guid())?,
            "Source-plan native contract lost its compiled owner"
        );
        let (path, sha256, bytes) = admitted.installer_identity();
        let installer = MemberPin {
            path: path.into(),
            sha256: sha256.into(),
            bytes,
        };
        let (path, sha256, bytes) = admitted.ini_identity();
        let changes_ini = MemberPin {
            path: path.into(),
            sha256: sha256.into(),
            bytes,
        };
        let translation = NativeTranslation {
            operation_index: 1,
            extract_operation_index: 0,
            installer,
            changes_ini,
            inventory_sha256: admitted.inventory_digest().into(),
            certificate_sha256: admitted.authority_digest().into(),
            arguments: "changes.ini".into(),
            destination: "<<kotorDirectory>>".into(),
            success_contract: "native-zero-warnings-changed-game-v1".into(),
        };
        translation.installer_parent()?;
        Ok(Self {
            component_guid: admitted.component_guid().into(),
            translation,
            source_plan: Some(Box::new(admitted)),
            source_namespace: None,
        })
    }
    /// Issued only by the independent original-source/whole-package choice factory.
    pub(crate) fn from_source_namespace(
        admitted: super::source_plan_namespace::AdmittedNamespace,
    ) -> Result<Self> {
        ensure!(
            super::source_plan_namespace::has_owner(admitted.component_guid())?
                || super::source_plan_namespace::eligible_for_fallback(admitted.component_guid())?,
            "Selected namespace contract lost its compiled source owner"
        );
        let (path, sha256, bytes) = admitted.installer_identity();
        let installer = MemberPin {
            path: path.into(),
            sha256: sha256.into(),
            bytes,
        };
        let (path, sha256, bytes) = admitted.ini_identity();
        let changes_ini = MemberPin {
            path: path.into(),
            sha256: sha256.into(),
            bytes,
        };
        let translation = NativeTranslation {
            operation_index: admitted.patch_index(),
            extract_operation_index: 0,
            installer,
            changes_ini,
            inventory_sha256: admitted.inventory_digest()?,
            certificate_sha256: admitted.authority_digest().into(),
            arguments: admitted
                .namespace()
                .map(|index| index.to_string())
                .unwrap_or_else(|| "changes.ini".into()),
            destination: "<<kotorDirectory>>".into(),
            success_contract: "native-zero-warnings-changed-game-v1".into(),
        };
        Ok(Self {
            component_guid: admitted.component_guid().into(),
            translation,
            source_plan: None,
            source_namespace: Some(Box::new(admitted)),
        })
    }
    /// Present only for a run issued by the complete original ordered program.
    pub(crate) fn original_program_run(&self) -> Option<&Value> {
        self.source_namespace
            .as_ref()
            .and_then(|run| run.original_program_run())
    }
    pub(crate) fn component_guid(&self) -> &str {
        &self.component_guid
    }
    pub(crate) fn expected_game(&self) -> Option<odypatcher::Game> {
        self.source_namespace
            .as_ref()
            .map(|admitted| admitted.expected_game())
            .or_else(|| {
                self.source_plan
                    .as_ref()
                    .map(|admitted| admitted.expected_game())
            })
    }
    pub(crate) fn destination_resources(
        &self,
    ) -> Option<&[super::source_plan_patcher::DestinationResourceRequirement]> {
        self.source_namespace
            .as_ref()
            .map(|admitted| admitted.destination_resources())
            .or_else(|| {
                self.source_plan
                    .as_ref()
                    .map(|admitted| admitted.destination_resources())
            })
    }
    pub(crate) fn certified_nss(&self) -> Option<&super::source_plan_patcher::CertifiedNssInputs> {
        self.source_plan
            .as_ref()
            .and_then(|admitted| admitted.certified_nss())
    }
    pub(crate) fn requires_exact_prediction(&self) -> bool {
        self.source_namespace.is_some()
            || self.source_plan.is_some()
            || self.tomb_shadow_identity_observation()
            || (self.source_plan.is_none() && community_guid(&self.component_guid))
    }
    /// The original CP protocols here admit only their reviewed English lane.
    /// Other source-language instructions require a separate reviewed contract.
    pub(crate) fn requires_english_game(&self) -> bool {
        self.source_plan.is_none()
            && self.translation.installer.path == "INSTALL.exe"
            && self.translation.changes_ini.path == "tslpatchdata/changes.ini"
            && matches!(
                (
                    self.component_guid.as_str(),
                    self.translation.installer.sha256.as_str(),
                    self.translation.changes_ini.sha256.as_str()
                ),
                (
                    CP1,
                    "5cc9fc7aeafefb8b18cd584de0ad530f5c7a057e4b0a1308ed7bc6a9e4bc3fbe",
                    "5ca192565cd425db03667364ab871273bacbd32da6a5760bd220729662bf3202"
                ) | (
                    CP2,
                    "1a267454b2efcd61232332be83f4da4ffe88bd6770c5a0df517880062b1a75e8",
                    "caa72cb34e8180a27e0464cf6e4fae29d9ce8e68417f1efc713b563293629b1d"
                )
            )
    }
    pub(crate) fn companion_selector_compatibility(&self) -> bool {
        self.component_guid == "f27d4b13-b349-8c56-b9b7-620cdc133579"
            && self.translation.installer.sha256
                == "638aaeea4e40ffb6c592f68787f2140318a0dec7f2cde85908806ce3b5bc5131"
            && self.translation.changes_ini.sha256
                == "5a2eed7f0ecd47a24f20540f9f231146c1730dc7810df6ed23c77d309c5d72a2"
    }
    pub(crate) fn tomb_shadow_identity_observation(&self) -> bool {
        self.source_plan.is_none()
            && self.component_guid == "ee2b251b-720d-8f69-9d38-5a56310f0bb4"
            && self.translation.installer.sha256
                == "1a267454b2efcd61232332be83f4da4ffe88bd6770c5a0df517880062b1a75e8"
            && self.translation.changes_ini.sha256
                == "82e88f244110ee54333e7253915f7d4c7eb1f381d213713827fc8de68f53a2fb"
    }
    pub(crate) fn verify_staged(&self, base: &Path, cancelled: &AtomicBool) -> Result<()> {
        if let Some(admitted) = &self.source_namespace {
            return admitted.verify_staged(base, cancelled);
        }
        if let Some(admitted) = &self.source_plan {
            admitted.verify_staged(base, cancelled)?;
        }
        if self.source_plan.is_none() && community_guid(&self.component_guid) {
            verify_community_staged(base, &self.translation, cancelled)?;
        }
        let parent = self.translation.installer_parent()?;
        if packaged_native_log(&self.component_guid).is_some() {
            ensure!(
                matches!(std::fs::symlink_metadata(base.join("installlog.rtf")),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound),
                "Native packaged stale log was not deleted before private staging"
            );
        }
        ensure!(
            !base.join("tslpatchdata/namespaces.ini").exists(),
            "Reviewed Patcher forbids namespaces"
        );
        for member in [&self.translation.installer, &self.translation.changes_ini] {
            ensure!(
                hash(
                    &base.join(Path::new(&literal(&member.path)?).strip_prefix(&parent)?),
                    cancelled,
                )? == (member.sha256.clone(), member.bytes),
                "Reviewed Patcher staged bytes differ"
            );
        }
        Ok(())
    }
    pub(crate) fn selected_namespace(&self) -> Option<usize> {
        self.source_namespace
            .as_ref()
            .and_then(|admitted| admitted.namespace())
    }
    pub(crate) fn certified_program_nss(
        &self,
    ) -> Option<&super::source_plan_namespace::CertifiedProgramNss> {
        self.source_namespace
            .as_ref()
            .and_then(|admitted| admitted.certified_program_nss())
    }
    pub(crate) fn selected_ini_relative_path(&self) -> Option<&str> {
        self.source_namespace
            .as_ref()
            .and_then(|admitted| admitted.selected_ini_relative_path())
    }
    pub(crate) fn selected_ini_name(&self) -> Option<&str> {
        self.source_namespace
            .as_ref()
            .and_then(|admitted| admitted.selected_ini_name())
    }
    /// Keep legacy/CP canonical-INI strictness; only a fresh opaque choice witness
    /// can bind a positional namespace and a different selected INI/data subtree.
    pub(crate) fn verify_selected_configuration(
        &self,
        base: &Path,
        namespace: Option<usize>,
        actual_ini: &Path,
        cancelled: &AtomicBool,
    ) -> Result<()> {
        ensure!(
            namespace == self.selected_namespace(),
            "Reviewed native namespace index differs from its private witness"
        );
        if let Some(admitted) = &self.source_namespace {
            return admitted.verify_selected_configuration(base, namespace, actual_ini, cancelled);
        }
        self.verify_selected_path(base, namespace, actual_ini, cancelled)?;
        ensure!(
            hash(actual_ini, cancelled)?
                == (
                    self.translation.changes_ini.sha256.clone(),
                    self.translation.changes_ini.bytes
                ),
            "Reviewed native Patcher selected INI bytes differ"
        );
        Ok(())
    }
    /// Selection-only guard after trusted private normalization; original raw
    /// INI bytes must have passed verify_selected_configuration beforehand.
    pub(crate) fn verify_selected_path(
        &self,
        base: &Path,
        namespace: Option<usize>,
        actual_ini: &Path,
        cancelled: &AtomicBool,
    ) -> Result<()> {
        if let Some(admitted) = &self.source_namespace {
            return admitted.verify_selected_path(base, namespace, actual_ini, cancelled);
        }
        check(cancelled)?;
        ensure!(
            namespace.is_none(),
            "Reviewed native Patcher forbids unbound namespace selection"
        );
        let parent = self.translation.installer_parent()?;
        let expected = base
            .join(Path::new(&literal(&self.translation.changes_ini.path)?).strip_prefix(parent)?);
        ensure!(
            expected.canonicalize()? == actual_ini.canonicalize()?,
            "Reviewed native Patcher selected a different INI"
        );
        Ok(())
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Canonicalization {
    method: String,
    raw_component_sha256: String,
    canonical_component_sha256: String,
    cli_binary_sha256: String,
    raw_export_sha256: String,
    canonical_export_sha256: String,
    source_separator_changes: usize,
    root_trailing_separator_changes: usize,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Step {
    id: String,
    source_heading: String,
    operations: Vec<Operation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout_seconds: Option<u64>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    filename: String,
    sha256: String,
}
#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    // Exact original HoloPatcher CLI provenance. Only CP1's compiled mixed
    // recipe can translate it; this is never an external command dispatcher.
    Patcher {
        patcher: String,
        sha256: String,
        argv: Vec<String>,
        cwd: String,
        success_log: String,
        success_markers: Vec<String>,
        failure_regex: Vec<String>,
        expected_changes: String,
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
    Extract {
        source: String,
        destination: String,
    },
    Copy {
        source: String,
        destination: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        skip_existing: Option<bool>,
    },
    Delete {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        missing_ok: Option<bool>,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct Inventory {
    filename: String,
    sha256: String,
    bytes: u64,
    expanded_bytes: u64,
    files: BTreeSet<String>,
    directories: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_kind: Option<String>,
}
struct Slot {
    native: String,
    files: BTreeSet<String>,
    directories: BTreeSet<String>,
}
fn check(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Recipe binding cancelled"
    );
    Ok(())
}
fn load_book(spec: &BookSpec) -> Result<Book> {
    ensure!(
        spec.content.len() <= 16 * 1024 * 1024,
        "Recipe book exceeds bound"
    );
    ensure!(
        format!("{:x}", Sha256::digest(spec.content.as_bytes())) == spec.sha256,
        "Compiled recipe book differs from reviewed source proof"
    );
    let book: Book = serde_json::from_str(spec.content)?;
    ensure!(
        book.schema == 1
            && book.source_guide_sha256 == spec.guide_sha256
            && book.source_plan_sha256 == spec.plan_sha256
            && book.recipes.len() == spec.count,
        "Recipe book identity or scope changed"
    );
    Ok(book)
}
/// Validate both compiled books before resolving an identity. Duplicate identities
/// are fatal even when the caller requested a different component.
fn find_recipe(guid: &str) -> Result<Option<(&'static BookSpec, Recipe)>> {
    let mut identities = BTreeSet::new();
    let mut selected = None;
    for spec in &BOOKS {
        for recipe in load_book(spec)?.recipes {
            ensure!(
                identities.insert(recipe.component_guid.clone()),
                "Duplicate recipe identity across compiled books"
            );
            if recipe.component_guid == guid {
                selected = Some((spec, recipe));
            }
        }
    }
    Ok(selected)
}
pub(crate) fn has_recipe(guid: &str) -> Result<bool> {
    // The books are immutable compiled inputs. Validate their scope once while
    // routing the complete source profile, rather than reparsing every book for
    // each candidate owner. Runtime archive verification remains uncached.
    static IDENTITIES: OnceLock<std::result::Result<BTreeSet<String>, String>> = OnceLock::new();
    let identities = IDENTITIES
        .get_or_init(|| {
            (|| -> Result<BTreeSet<String>> {
                let mut identities = BTreeSet::new();
                for spec in &BOOKS {
                    for recipe in load_book(spec)?.recipes {
                        ensure!(
                            identities.insert(recipe.component_guid),
                            "Duplicate recipe identity across compiled books"
                        );
                    }
                }
                Ok(identities)
            })()
            .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!(e.clone()))?;
    Ok(identities.contains(guid))
}
/// Verification identity belongs to the exact compiled book admitting this GUID.
/// Dependency definition only, never proof of execution or installed state.
/// Dependent effects separately require runtime replay and owned checkpoints.
pub(crate) fn declared_original_definition(component: &Component) -> Result<Option<Component>> {
    let Some(binding) = component.metadata.get(BINDING) else {
        return Ok(None);
    };
    let (spec, recipe) =
        find_recipe(&component.guid)?.context("Unknown recipe origin declaration")?;
    ensure!(
        binding["Schema"] == 1
            && binding["DatabookSha256"] == spec.sha256
            && binding.get("Recipe") == Some(&serde_json::to_value(&recipe)?)
            && binding.get("InputComponent") == Some(&recipe.canonical_input_component),
        "Original recipe definition declaration changed"
    );
    Ok(Some(serde_json::from_value(
        recipe.canonical_input_component.clone(),
    )?))
}

pub(crate) fn verification_identity(guid: &str) -> Result<(String, String)> {
    let (spec, _) = find_recipe(guid)?.context("Recipe identity not admitted")?;
    Ok((spec.filename.into(), spec.sha256.into()))
}

struct CommunitySource {
    raw: Value,
    canonical: Component,
    step: Value,
    archives: Value,
}
static CP1_SOURCE: OnceLock<std::result::Result<CommunitySource, String>> = OnceLock::new();
static CP2_SOURCE: OnceLock<std::result::Result<CommunitySource, String>> = OnceLock::new();

// Reconstruct original authority without ordering, binding, master resolution or
// archive access. In particular this cannot recurse through the NPC alias guard.
fn community_source(recipe: &Recipe) -> Result<&'static CommunitySource> {
    let (cache, guide, plan, index, count, guide_sha, plan_sha) =
        match recipe.component_guid.as_str() {
            CP1 => (
                &CP1_SOURCE,
                include_bytes!("source_plan_k1.md").as_slice(),
                include_bytes!("../../tools/modsync-ci/fixtures/ci/k1-manual-plan.json").as_slice(),
                5,
                197,
                "44067788c6cb7a83b38d468817f273ce5cc901cd5e000c0f9ff65f8af15fc6ed",
                "1b402ba085625c4863795069eaacaef0f53e8ea7179e56a5bd150d88eb9c0a74",
            ),
            CP2 => (
                &CP2_SOURCE,
                include_bytes!("source_plan_k2.md").as_slice(),
                include_bytes!("../../tools/modsync-ci/fixtures/ci/k2-manual-plan.json").as_slice(),
                8,
                151,
                "f442bdf574031623ed577609730d887069b9bc8e38e8e2f38ecc43a7e7487d5f",
                "0add8c1aeb7cdfad3df4b21761e135c150aa7832cf3fc0edabe01a91245a9258",
            ),
            _ => anyhow::bail!("Not a compiled Community Patch owner"),
        };
    let source = cache.get_or_init(|| (|| -> Result<CommunitySource> {
        ensure!(guide.len() <= 1024 * 1024 && plan.len() <= 8 * 1024 * 1024
            && format!("{:x}", Sha256::digest(guide)) == guide_sha
            && format!("{:x}", Sha256::digest(plan)) == plan_sha,
            "Compiled Community Patch original source bytes differ");
        let mut guide = Guide::parse(std::str::from_utf8(guide)?, "md")?;
        draft::generate(&mut guide);
        ensure!(guide.components.len() == count, "Community Patch original source inventory differs");
        let raw = serde_json::to_value(&guide.components[index])?;
        let canonical = Guide::parse_preserving_selection(&serde_json::to_string(&guide)?, "json")?;
        let canonical = canonical.components[index].clone();
        let plan: Value = serde_json::from_slice(plan)?;
        ensure!(plan["guide_sha256"] == guide_sha, "Community Patch original plan guide differs");
        let step = plan["steps"][index].clone();
        let operations = step["operations"].as_array().context("Community Patch source operations absent")?;
        let mut archives = Vec::new();
        for op in operations.iter().filter(|op| op["type"] == "extract") {
            let path = op["source"].as_str().context("Community Patch extract source absent")?;
            archives.push(json!({"filename":token(path,"archives")?,"sha256":plan["archive_sha256"][path]}));
        }
        Ok(CommunitySource { raw, canonical, step, archives: json!(archives) })
    })().map_err(|error| format!("{error:#}")))
        .as_ref().map_err(|error| anyhow::anyhow!("{error}"))?;
    ensure!(
        recipe.source_component_index == index
            && recipe.plan_step_index == index
            && recipe.component_guid == source.canonical.guid
            && recipe.raw_input_component == source.raw
            && recipe.canonical_input_component == serde_json::to_value(&source.canonical)?
            && serde_json::to_value(&recipe.plan_step)? == source.step
            && source.step["source_heading"].as_str() == Some(recipe.source_heading.as_str())
            && serde_json::to_value(&recipe.archives)? == source.archives,
        "Community Patch recipe differs from freshly reconstructed complete original source"
    );
    Ok(source)
}

fn community_manifest(
    files: &BTreeMap<String, (u64, String)>,
    directories: &BTreeSet<String>,
) -> Result<String> {
    value_digest(&json!({"Policy":CP_MANIFEST_POLICY,"Files":files,"Directories":directories}))
}
fn add_parents(path: &str, directories: &mut BTreeSet<String>) {
    for (position, _) in path.match_indices('/') {
        directories.insert(path[..position].into());
    }
}
fn community_metadata(path: &Path) -> Result<std::fs::Metadata> {
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        !meta.file_type().is_symlink() && (meta.is_dir() || meta.is_file()),
        "Community Patch package contains a link or special file"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            meta.file_attributes() & 0x400 == 0,
            "Community Patch package contains a reparse point"
        );
    }
    Ok(meta)
}
fn verify_community_staged(
    base: &Path,
    translation: &NativeTranslation,
    cancelled: &AtomicBool,
) -> Result<()> {
    ensure!(
        community_metadata(base)?.is_dir(),
        "Community Patch staged root is not a directory"
    );
    let mut pending = vec![base.to_path_buf()];
    let mut files = BTreeMap::new();
    let mut directories = BTreeSet::new();
    let mut aliases = BTreeSet::new();
    let mut total = 0u64;
    while let Some(parent) = pending.pop() {
        for entry in std::fs::read_dir(parent)? {
            check(cancelled)?;
            let path = entry?.path();
            let relative = literal(
                path.strip_prefix(base)?
                    .to_str()
                    .context("Non-UTF8 Community Patch package")?
                    .replace('\\', "/")
                    .as_str(),
            )?;
            ensure!(
                aliases.len() < 10000
                    && aliases.insert(crate::unicode_case::ordinal_key(&relative)),
                "Community Patch package exceeds bound or contains aliases"
            );
            let meta = community_metadata(&path)?;
            if meta.is_dir() {
                directories.insert(relative);
                pending.push(path);
            } else {
                ensure!(
                    meta.len() <= 64 * 1024 * 1024,
                    "Community Patch staged member exceeds bound"
                );
                total = total
                    .checked_add(meta.len())
                    .context("Community Patch size overflow")?;
                ensure!(
                    total <= CP_EXPANDED,
                    "Community Patch staged package exceeds bound"
                );
                let (sha256, bytes) = community_member_hash(&path, cancelled)?;
                files.insert(relative, (bytes, sha256));
            }
        }
    }
    ensure!(
        community_manifest(&files, &directories)? == translation.inventory_sha256,
        "Community Patch complete raw staged package differs from compiled authority"
    );
    check(cancelled)
}
/// Filename admission exception for an exact compiled recipe/archive pair only.
/// This grants neither recipe approval nor permission to skip byte verification.
pub fn allows_literal_filename(guid: &str, filename: &str) -> Result<bool> {
    Ok(find_recipe(guid)?
        .is_some_and(|(_, recipe)| recipe.archives.iter().any(|pin| pin.filename == filename))
        || super::source_plan_patcher::allows_literal_filename(guid, filename)?
        || super::source_plan_namespace::allows_literal_filename(guid, filename)?)
}

/// Only the two compiled whole-file source pins may cross the archive-only CLI
/// transport. This does not approve them or bypass the subsequent byte replay.
pub fn allows_raw_input(guid: &str, filename: &str) -> Result<bool> {
    Ok(find_recipe(guid)?.is_some_and(|(_, recipe)| {
        recipe
            .archives
            .iter()
            .any(|pin| pin.filename == filename && reviewed_raw_pin(pin))
    }))
}
/// The observed source-plan step042 installs exactly the three TGA members.
/// Its actual 909-byte readme (SHA256 fa90aca6fddf221388ce8608a2c639e899da3205677e87a6e4cff0eb365267ca)
/// only directs extraction to Override. Do not generalize its unusual name to
/// unreviewed packages, or classify arbitrary TXT files as documentation.
pub(crate) fn is_reviewed_documentation(guid: &str, archive_sha256: &str, member: &str) -> bool {
    guid == "7b678426-adfb-8f47-a090-bf8e263dc37f"
        && archive_sha256 == "8b38c3bb158395af5718db3a02f74f4995f06ebb1e93975f90355b330e0e89f8"
        && member == "Mandalorian Worn-Out Armour Reskin/Readme First.txt"
}
fn literal(value: &str) -> Result<String> {
    ensure!(
        !value.is_empty()
            && value.len() <= 4096
            && !value.contains(['\\', '*', '?', ':', '<', '>', '{', '}'])
            && !value.chars().any(char::is_control),
        "Unsafe recipe literal path"
    );
    let path = crate::paths::literal_relative(value)?;
    let normalized = path.to_str().context("Non-UTF8 recipe path")?;
    ensure!(
        normalized == value && !value.ends_with('/'),
        "Noncanonical recipe path"
    );
    Ok(value.to_owned())
}
fn token(value: &str, root: &str) -> Result<String> {
    literal(
        value
            .strip_prefix(&format!("{{{root}}}/"))
            .context("Wrong recipe root")?,
    )
}
fn hash(path: &Path, cancelled: &AtomicBool) -> Result<(String, u64)> {
    hash_impl(path, cancelled, false)
}
// A pinned full-package inventory may contain legitimate empty source members.
// Archive/installer/INI hashing retains its original nonempty contract.
fn community_member_hash(path: &Path, cancelled: &AtomicBool) -> Result<(String, u64)> {
    hash_impl(path, cancelled, true)
}
fn hash_impl(
    path: &Path,
    cancelled: &AtomicBool,
    allow_empty_member: bool,
) -> Result<(String, u64)> {
    let mut file = crate::paths::open_regular_read(path)?;
    let expected = file.metadata()?.len();
    ensure!(
        !allow_empty_member || expected <= 64 * 1024 * 1024,
        "Community Patch member exceeds its hash-read bound"
    );
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        check(cancelled)?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(count as u64)
            .context("Archive size overflow")?;
        ensure!(bytes <= expected, "Archive changed during hashing");
        hash.update(&buffer[..count]);
    }
    ensure!(
        bytes == expected && (bytes > 0 || allow_empty_member),
        "Archive changed or is empty"
    );
    Ok((format!("{:x}", hash.finalize()), bytes))
}
fn canonical_context(recipe: &Recipe, component: &Component) -> Result<Component> {
    if community_guid(&recipe.component_guid) {
        community_source(recipe)?;
    }
    let guide = Guide::parse_preserving_selection(
        &serde_json::to_string(&recipe.raw_input_component)?,
        "json",
    )?;
    ensure!(
        guide.components.len() == 1,
        "Recipe context is not one component"
    );
    ensure!(
        recipe.canonicalization.method == "production-guide-loader"
            && recipe.canonicalization.raw_component_sha256
                == value_digest(&recipe.raw_input_component)?
            && recipe.canonicalization.canonical_component_sha256
                == value_digest(&recipe.canonical_input_component)?,
        "Databook canonicalization identity mismatch"
    );
    let canonical = &guide.components[0];
    let value = serde_json::to_value(canonical)?;
    ensure!(
        value == recipe.canonical_input_component,
        "Databook canonical context differs from native loader"
    );
    // The stored raw and canonical contexts must differ only at the known
    // loader normalizations. Never discard other metadata or recipe choices.
    fn normalize_source(source: &str) -> String {
        let normalized = source.replace('\\', "/");
        if normalized.trim_end_matches('/') == "<<modDirectory>>" {
            "<<modDirectory>>".into()
        } else {
            normalized
        }
    }
    fn normalize(raw: &mut Value, canonical: &Value) -> Result<()> {
        let raw_object = raw.as_object_mut().context("Invalid recipe component")?;
        if let Some(instructions) = raw_object
            .get_mut("Instructions")
            .and_then(Value::as_array_mut)
        {
            for instruction in instructions {
                if let Some(sources) = instruction.get_mut("Source") {
                    match sources {
                        Value::String(source) => *source = normalize_source(source),
                        Value::Array(sources) => {
                            for source in sources {
                                let text = source.as_str().context("Invalid instruction source")?;
                                *source = Value::String(normalize_source(text));
                            }
                        }
                        _ => anyhow::bail!("Invalid instruction Source shape"),
                    }
                }
            }
        }
        if let Some(options) = raw_object.get_mut("Options").and_then(Value::as_array_mut) {
            let canonical_options = canonical
                .get("Options")
                .and_then(Value::as_array)
                .context("Missing canonical options")?;
            ensure!(
                options.len() == canonical_options.len(),
                "Option count changed"
            );
            for (option, expected) in options.iter_mut().zip(canonical_options) {
                normalize(option, expected)?;
            }
        }
        Ok(())
    }
    let mut raw = recipe.raw_input_component.clone();
    normalize(&mut raw, &value)?;
    // This exact witnessed original owner also undergoes the loader's comma
    // splitting of Language. Never discard or normalize arbitrary source fields.
    if recipe.component_guid == CP1 {
        community_source(recipe)?;
        ensure!(
            raw["Language"]
                == json!([
                    "YES for Russian and French only, follow the instructions on the mod page to use"
                ])
                && value["Language"]
                    == json!([
                        "YES for Russian and French only",
                        "follow the instructions on the mod page to use"
                    ]),
            "Community Patch original language normalization differs"
        );
        raw["Language"] = value["Language"].clone();
    }
    ensure!(raw == value, "Recipe has unsupported loader normalization");
    let mut actual = serde_json::to_value(component)?;
    // Accept only the complete stored raw context or its complete loader form.
    if actual == recipe.raw_input_component {
        actual = value.clone();
    }
    ensure!(
        actual == value && canonical.guid == recipe.component_guid,
        "Component differs from exact reviewed recipe context"
    );
    ensure!(
        canonical
            .options
            .iter()
            .all(|option| option.instructions.is_empty()
                && option.options.is_empty()
                && !option.metadata.contains_key("DraftReview")),
        "Recipe options must be exact inert reviewed leaf context"
    );
    Ok(canonical.clone())
}
fn reviewed_raw_pin(pin: &Pin) -> bool {
    matches!(
        (pin.filename.as_str(), pin.sha256.as_str()),
        (
            "N_DarthMalak01.tga",
            "04160ed9e2eef2c88a3ed644ec2be24e5cd765f67a7cca35106be062e20358cd"
        ) | (
            "tar02_duelorg021.dlg",
            "6ebb0b909c4ac7496971e3cc1320ce14b04fc67aec167f0feeb0e332181aff57"
        )
    )
}

fn inspect(
    pin: &Pin,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Inventory> {
    let filename = literal(&pin.filename)?;
    ensure!(
        !filename.contains('/')
            && pin.sha256.len() == 64
            && pin
                .sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "Invalid archive pin"
    );
    let path = root.join(&filename);
    let raw_input = reviewed_raw_pin(pin);
    if raw_input {
        ensure!(
            crate::paths::open_regular_read(&path)?.metadata()?.len() <= 128 * 1024 * 1024,
            "Reviewed loose input exceeds bound"
        );
    }
    let (sha256, bytes) = hash(&path, cancelled)?;
    ensure!(
        sha256 == pin.sha256,
        "Reviewed archive hash mismatch: {filename}"
    );
    if raw_input {
        // These original guide inputs are already individual game resources.
        // Their whole-file hash is the payload certificate; no archive decoder
        // or inferred extraction tree participates in this source role.
        return Ok(Inventory {
            filename,
            sha256,
            bytes,
            expanded_bytes: 0,
            files: BTreeSet::new(),
            directories: BTreeSet::new(),
            source_kind: Some("reviewed-loose-file-v1".into()),
        });
    }
    let (entries, expanded_bytes) =
        archive::list_original_with_size(&path, &path, options, cancelled)?;
    let mut files = BTreeSet::new();
    let mut directories = BTreeSet::new();
    let mut authored_names = BTreeSet::new();
    for entry in entries {
        let path = literal(&entry.path)?;
        ensure!(
            authored_names.insert(path.to_lowercase()),
            "Duplicate authored archive entry"
        );
        for (position, _) in path.match_indices('/') {
            directories.insert(path[..position].to_owned());
        }
        if entry.is_directory {
            directories.insert(path);
        } else {
            ensure!(files.insert(path), "Duplicate archive file");
        }
    }
    ensure!(
        files.iter().all(|path| !path
            .to_ascii_lowercase()
            .ends_with("fomod/moduleconfig.xml")),
        "FOMOD choices are not admitted by reviewed file-effect recipes"
    );
    let mut unique = BTreeSet::new();
    for path in files.iter().chain(directories.iter()) {
        ensure!(
            unique.insert(path.to_lowercase()),
            "Archive file/directory case collision"
        );
    }
    ensure!(
        hash(&path, cancelled)? == (sha256.clone(), bytes),
        "Archive changed during inspection"
    );
    Ok(Inventory {
        filename,
        sha256,
        bytes,
        expanded_bytes,
        files,
        directories,
        source_kind: None,
    })
}
fn extraction_source<'a>(
    source: &str,
    slots: &'a BTreeMap<String, Slot>,
) -> Result<(&'a Slot, String)> {
    let relative = token(source, "extract")?;
    for (name, slot) in slots {
        if relative == *name {
            return Ok((slot, String::new()));
        }
        if let Some(member) = relative.strip_prefix(&format!("{name}/")) {
            return Ok((slot, member.to_owned()));
        }
    }
    anyhow::bail!("Recipe source has no prior extraction")
}
fn instruction(
    component: &Component,
    index: usize,
    action: &str,
    source: String,
    destination: String,
    overwrite: bool,
) -> Instruction {
    Instruction {
        guid: crate::markdown::generated_id(
            &format!("{}:reviewed-manual-file-effects", component.guid),
            index,
        ),
        parent: component.guid.clone(),
        action: action.into(),
        source: vec![source],
        destination,
        arguments: String::new(),
        overwrite,
        dependencies: vec![],
        restrictions: vec![],
    }
}

fn expected_native_instructions(
    component: &Component,
    recipe: &Recipe,
) -> Result<Vec<Instruction>> {
    let translation = recipe
        .native_patcher
        .as_ref()
        .context("Missing native translation")?;
    let packaged_log = native_shape(recipe)?;
    let slot = format!(
        "<<modDirectory>>/.modsync-reviewed-{}-slot0",
        component.guid
    );
    let mut result = vec![instruction(
        component,
        0,
        "ExtractTo",
        format!("<<modDirectory>>/{}", recipe.archives[0].filename),
        slot.clone(),
        false,
    )];
    if recipe.component_guid == CP1 {
        let patch_slot = format!(
            "<<modDirectory>>/.modsync-reviewed-{}-slot1",
            component.guid
        );
        result.push(instruction(
            component,
            1,
            "ExtractTo",
            format!("<<modDirectory>>/{}", recipe.archives[1].filename),
            patch_slot,
            false,
        ));
    } else if let Some(member) = packaged_log {
        result.push(instruction(
            component,
            1,
            "Delete",
            format!("{slot}/{member}"),
            String::new(),
            true,
        ));
    }
    let mut patcher = instruction(
        component,
        translation.operation_index,
        "Patcher",
        format!("{slot}/{}", translation.installer.path),
        translation.destination.clone(),
        false,
    );
    patcher.arguments = translation.arguments.clone();
    result.push(patcher);
    if recipe.component_guid == CP1 {
        for (index, leaf) in ["n_fatcomm.mdl", "n_fatcomm.mdx"].iter().enumerate() {
            result.push(instruction(
                component,
                index + 3,
                "CopyFile",
                format!(
                    "<<modDirectory>>/.modsync-reviewed-{}-slot1/K1CP Patch/{leaf}",
                    component.guid
                ),
                format!("<<kotorDirectory>>/override/{leaf}"),
                true,
            ));
        }
    }
    Ok(result)
}

fn community_expected_pending(
    component: &Component,
    spec: &BookSpec,
    recipe: &Recipe,
) -> Result<Component> {
    let source = community_source(recipe)?;
    let inventories_value = component
        .metadata
        .get(BINDING)
        .and_then(|v| v.get("Archives"))
        .context("Community Patch inventories absent")?;
    let expected_inventory = if recipe.component_guid == CP1 {
        ensure!(spec.sha256 == CP1_BOOK, "Community Patch K1 book differs");
        "263cee781263cf889f390cc38d7c6db1105832553f36f71eff0b921792980398"
    } else {
        ensure!(spec.sha256 == CP2_BOOK, "Community Patch K2 book differs");
        "67e76dabee35e95dcf17e1ec6727441ba2916e95bc983994eb885e6c34b973af"
    };
    ensure!(
        value_digest(inventories_value)? == expected_inventory,
        "Community Patch complete bound resource inventory differs from compiled authority"
    );
    let inventories: Vec<Inventory> = serde_json::from_value(inventories_value.clone())?;
    let mut expected = source.canonical.clone();
    expected.is_selected = component.is_selected;
    expected.instructions = expected_native_instructions(&expected, recipe)?;
    let binding = json!({"Schema":1,"Kind":spec.kind,"DatabookSha256":spec.sha256,
        "SourceGuideSha256":spec.guide_sha256,"SourcePlanSha256":spec.plan_sha256,"Recipe":recipe,
        "InputComponent":serde_json::to_value(&source.canonical)?,"Archives":inventories_value});
    let derivations: Vec<_> = inventories
        .iter()
        .map(|item| json!({"Filename":item.filename,"Sha256":item.sha256,"Bytes":item.bytes}))
        .collect();
    expected.metadata.insert(BINDING.into(), binding);
    expected.metadata.insert("DraftReview".into(), json!({"RequiresReview":true,"Generator":PATCHER_GENERATOR,
        "Derivations":derivations,"Result":draft::DraftResult {component_guid:expected.guid.clone(),
        draft_instruction_count:expected.instructions.len(),unparsed_gaps:vec![],conditional_drafts:vec![],rejected_drafts:0}}));
    let mut actual_value = serde_json::to_value(component)?;
    let mut expected_value = serde_json::to_value(&expected)?;
    actual_value
        .as_object_mut()
        .context("Invalid Community Patch component")?
        .remove("DraftReview");
    expected_value
        .as_object_mut()
        .context("Invalid Community Patch model")?
        .remove("DraftReview");
    ensure!(
        actual_value == expected_value,
        "Community Patch complete effective context, instructions or resource binding differs"
    );
    Ok(expected)
}

/// Pure source-origin evidence only, with private fields and no deserialization.
/// Archive/runtime approval and an owned successful checkpoint remain separate.
pub(crate) struct CommunityPatchOrigin {
    original: Component,
}
impl CommunityPatchOrigin {
    pub(crate) fn original(&self) -> &Component {
        &self.original
    }
}
fn community_review(component: &Component, expected: &Component, spec: &BookSpec) -> Result<()> {
    let actual = component
        .metadata
        .get("DraftReview")
        .context("Community Patch origin review absent")?;
    if actual == &expected.metadata["DraftReview"] {
        return Ok(());
    }
    let mut approved = expected.metadata["DraftReview"].clone();
    approved["RequiresReview"] = json!(false);
    approved["Verification"] = json!({"Reviewer":draft::review::VERIFIER_NAME,"Kind":draft::review::VERIFICATION_KIND,
        "ToolVersion":concat!("ModSync.rs/",env!("CARGO_PKG_VERSION")),"ArchiveSha256":spec.sha256,
        "ArchiveFilename":spec.filename,"ReplayDigest":replay_digest(expected)?,"IdentityVersion":2});
    approved["ReviewedIdentity"] = json!(draft::review::identity(component)?);
    ensure!(
        actual == &approved,
        "Community Patch origin requires the exact pending or approved review transition"
    );
    Ok(())
}
pub(crate) fn community_patch_origin(
    component: &Component,
) -> Result<Option<CommunityPatchOrigin>> {
    if component.guid != CP1 {
        return Ok(None);
    }
    let record = component.metadata.get("DraftReview");
    let claimed = component.metadata.contains_key(BINDING)
        || record
            .and_then(|v| v.get("Generator"))
            .and_then(Value::as_str)
            == Some(PATCHER_GENERATOR);
    if !claimed {
        return Ok(None);
    }
    let (spec, recipe) =
        find_recipe(CP1)?.context("Compiled Community Patch source owner absent")?;
    let expected = community_expected_pending(component, spec, &recipe)?;
    community_review(component, &expected, spec)?;
    Ok(Some(CommunityPatchOrigin {
        original: community_source(&recipe)?.canonical.clone(),
    }))
}

fn verify_native_members(
    recipe: &Recipe,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<()> {
    let translation = recipe
        .native_patcher
        .as_ref()
        .context("Missing native translation")?;
    ensure!(
        recipe.archives.len() == 1 || (recipe.component_guid == CP1 && recipe.archives.len() == 2),
        "Native translation requires one archive and canonical sole INI"
    );
    translation.installer_parent()?;
    let packaged_log = native_shape(recipe)?;
    let mut packaged_log_found = false;
    let pin = &recipe.archives[0];
    let mut bounded = options.clone();
    let community = community_guid(&recipe.component_guid);
    let limit = if community {
        CP_EXPANDED
    } else {
        64 * 1024 * 1024
    };
    bounded.max_expanded_bytes = Some(options.max_expanded_bytes.unwrap_or(limit).min(limit));
    let entries = archive::unpack(
        &root.join(&pin.filename),
        Path::new(&pin.filename),
        &bounded,
        cancelled,
    )?;
    let mut found = BTreeSet::new();
    let mut ini_count = 0;
    let mut package_files = BTreeMap::new();
    let mut package_directories = BTreeSet::new();
    let mut package_bytes = 0u64;
    for entry in entries {
        check(cancelled)?;
        let name = entry
            .path
            .to_str()
            .context("Non-UTF8 native member")?
            .replace('\\', "/");
        let lower = name.to_lowercase();
        if community {
            literal(&name)?;
            ensure!(
                package_files.len() < 10000,
                "Community Patch raw member count exceeds bound"
            );
            let (sha256, bytes) = community_member_hash(entry.file.path(), cancelled)?;
            package_bytes = package_bytes
                .checked_add(bytes)
                .context("Community Patch raw size overflow")?;
            ensure!(
                bytes <= 64 * 1024 * 1024 && package_bytes <= CP_EXPANDED,
                "Community Patch raw package exceeds bound"
            );
            ensure!(
                package_files
                    .insert(name.clone(), (bytes, sha256))
                    .is_none(),
                "Duplicate Community Patch raw member"
            );
            add_parents(&name, &mut package_directories);
        }
        if packaged_log == Some(name.as_str()) {
            ensure!(!packaged_log_found, "Duplicate native packaged log member");
            packaged_log_found = true;
        }
        ensure!(
            !lower.ends_with("/namespaces.ini") && lower != "namespaces.ini",
            "Native translation forbids namespaces"
        );
        if lower.ends_with("/changes.ini") || lower == "changes.ini" {
            ini_count += 1;
            if community {
                ensure!(
                    entry.file.as_file().metadata()?.len() <= 2 * 1024 * 1024,
                    "Community Patch INI exceeds bound"
                );
                let body = std::fs::read(entry.file.path())?;
                let text = std::str::from_utf8(&body)?.trim_start_matches('\u{feff}');
                let mut in_compile = false;
                let mut compile_sections = 0;
                for line in text.lines().map(str::trim) {
                    if line.is_empty() || line.starts_with(';') {
                        continue;
                    }
                    if line.starts_with('[') && line.ends_with(']') {
                        in_compile = line.eq_ignore_ascii_case("[CompileList]");
                        compile_sections += usize::from(in_compile);
                    } else {
                        ensure!(
                            !in_compile,
                            "Community Patch requires its original empty CompileList"
                        );
                    }
                }
                ensure!(
                    compile_sections == 1,
                    "Community Patch CompileList identity differs"
                );
            }
        }
        for member in [&translation.installer, &translation.changes_ini] {
            if name == member.path {
                ensure!(
                    found.insert(name.clone())
                        && hash(entry.file.path(), cancelled)?
                            == (member.sha256.clone(), member.bytes),
                    "Native translation member bytes differ"
                );
            }
        }
    }
    ensure!(
        ini_count == 1 && found.len() == 2 && (packaged_log.is_none() || packaged_log_found),
        "Native translation requires exact installer, sole INI and any declared packaged log"
    );
    if community {
        ensure!(
            community_manifest(&package_files, &package_directories)?
                == translation.inventory_sha256,
            "Community Patch complete raw archive package differs from compiled manifest"
        );
    }
    ensure!(
        hash(&root.join(&pin.filename), cancelled)?.0 == pin.sha256,
        "Archive changed during native translation inspection"
    );
    Ok(())
}

/// Admission for runtime strictness; this is not an authored-JSON switch.
/// The engine also calls verify_runtime before any recipe instruction.
pub(crate) fn native_patcher_contract(
    component: &Component,
    current: &Instruction,
) -> Result<Option<NativePatcherContract>> {
    let has_binding = component.metadata.contains_key(BINDING);
    let native_generator = component
        .metadata
        .get("DraftReview")
        .and_then(|record| record.get("Generator"))
        .and_then(Value::as_str)
        == Some(PATCHER_GENERATOR);
    // A shared catalog GUID does not turn a legacy authored instruction into
    // a reviewed translation. Either half of new provenance must still be checked.
    if !has_binding && !native_generator {
        return Ok(None);
    }
    let (spec, recipe) = find_recipe(&component.guid)?
        .context("Native Patcher provenance has no compiled recipe")?;
    let Some(translation) = recipe.native_patcher.as_ref() else {
        ensure!(
            !native_generator,
            "Native Patcher generator does not match compiled recipe"
        );
        return Ok(None);
    };
    native_shape(&recipe)?;
    ensure!(
        spec.generator == PATCHER_GENERATOR,
        "Native Patcher recipe instruction shape changed"
    );
    let binding = component
        .metadata
        .get(BINDING)
        .context("Native Patcher binding absent")?;
    let record = component
        .metadata
        .get("DraftReview")
        .context("Native Patcher review absent")?;
    ensure!(
        record.get("RequiresReview") == Some(&json!(false))
            && record.get("Generator") == Some(&json!(PATCHER_GENERATOR))
            && record.get("ReviewedIdentity").and_then(Value::as_str)
                == Some(draft::review::identity(component)?.as_str())
            && binding.get("DatabookSha256") == Some(&json!(spec.sha256)),
        "Native Patcher requires unchanged dedicated approval"
    );
    ensure!(
        binding.get("Recipe") == Some(&serde_json::to_value(&recipe)?)
            && binding.get("InputComponent") == Some(&recipe.canonical_input_component),
        "Native Patcher catalog provenance differs"
    );
    let expected_instructions = expected_native_instructions(component, &recipe)?;
    ensure!(
        serde_json::to_value(&component.instructions)?
            == serde_json::to_value(expected_instructions)?,
        "Native Patcher complete instruction model differs from catalog"
    );
    if community_guid(&component.guid) {
        let expected = community_expected_pending(component, spec, &recipe)?;
        community_review(component, &expected, spec)?;
    }
    let expected = &component.instructions[translation.operation_index];
    ensure!(
        expected.action == "Patcher"
            && expected.arguments == translation.arguments
            && expected.destination == translation.destination,
        "Native Patcher instruction changed"
    );
    if serde_json::to_value(current)? != serde_json::to_value(expected)? {
        return Ok(None);
    }
    Ok(Some(NativePatcherContract {
        component_guid: recipe.component_guid.clone(),
        translation: translation.clone(),
        source_plan: None,
        source_namespace: None,
    }))
}

/// Atomic binding: no caller-visible mutation occurs until the entire recipe is verified.
pub(crate) fn bind(
    component: &mut Component,
    filenames: &[String],
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<()> {
    check(cancelled)?;
    let (spec, recipe) = find_recipe(&component.guid)?
        .context("No reviewed file-effect recipe for this component")?;
    let mut bound = canonical_context(&recipe, component)?;
    ensure!(
        bound.guid.len() == 36
            && bound
                .guid
                .bytes()
                .enumerate()
                .all(|(i, b)| if [8, 13, 18, 23].contains(&i) {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit() && !b.is_ascii_uppercase()
                }),
        "Recipe GUID is not a canonical extraction namespace"
    );
    ensure!(
        recipe.plan_step.source_heading == recipe.source_heading
            && !recipe.plan_step.operations.is_empty()
            && recipe.plan_step.operations.len() <= 10000,
        "Recipe step identity or operation bound invalid"
    );
    let root = super::resolved::local_root(root)?;
    let expected: BTreeSet<_> = recipe
        .archives
        .iter()
        .map(|pin| pin.filename.clone())
        .collect();
    ensure!(
        !expected.is_empty()
            && expected.len() == recipe.archives.len()
            && filenames.len() == expected.len()
            && filenames.iter().cloned().collect::<BTreeSet<_>>() == expected,
        "Resolved archive set differs from complete reviewed recipe"
    );
    let mut inventories = BTreeMap::new();
    let mut inspection_options = options.clone();
    if community_guid(&recipe.component_guid) {
        inspection_options.max_expanded_bytes = Some(
            options
                .max_expanded_bytes
                .unwrap_or(CP_EXPANDED)
                .min(CP_EXPANDED),
        );
    }
    for pin in &recipe.archives {
        inventories.insert(
            pin.filename.clone(),
            inspect(pin, &root, &inspection_options, cancelled)?,
        );
    }
    let packaged_log = if recipe.native_patcher.is_some() {
        native_shape(&recipe)?
    } else {
        None
    };
    let mut slots: BTreeMap<String, Slot> = BTreeMap::new();
    let mut used = BTreeSet::new();
    let mut instructions = Vec::new();
    let mut writes = 0usize;
    for (operation_index, operation) in recipe.plan_step.operations.iter().enumerate() {
        check(cancelled)?;
        match operation {
            Operation::Patcher {
                patcher,
                sha256,
                cwd,
                expected_changes,
                ..
            }
            | Operation::GuiPatcher {
                patcher,
                sha256,
                cwd,
                expected_changes,
                ..
            } => {
                let translation = recipe
                    .native_patcher
                    .as_ref()
                    .context("GUI operation is not a native translation")?;
                ensure!(
                    spec.generator == PATCHER_GENERATOR
                        && operation_index == translation.operation_index
                        && translation.extract_operation_index == 0
                        && (recipe.archives.len() == 1 || recipe.component_guid == CP1)
                        && translation.arguments == "changes.ini"
                        && translation.destination == "<<kotorDirectory>>"
                        && translation.success_contract == "native-zero-warnings-changed-game-v1"
                        && expected_changes == "changed_manifest",
                    "Unsupported native Patcher translation"
                );
                let (slot, member) = extraction_source(patcher, &slots)?;
                let original_patcher = token(patcher, "extract")?;
                let original_cwd = token(cwd, "extract")?;
                ensure!(
                    member == translation.installer.path
                        && sha256 == &translation.installer.sha256
                        && Path::new(&original_patcher).parent() == Some(Path::new(&original_cwd))
                        && slot.files.contains(&member),
                    "Native Patcher source differs from original recipe"
                );
                if let Some(log) = packaged_log {
                    ensure!(
                        !slot.files.contains(log),
                        "Native packaged log remains in the expected slot model before Patcher"
                    );
                }
                verify_native_members(&recipe, &root, options, cancelled)?;
                let mut generated = instruction(
                    &bound,
                    instructions.len(),
                    "Patcher",
                    format!("{}/{}", slot.native, member),
                    translation.destination.clone(),
                    false,
                );
                generated.arguments = translation.arguments.clone();
                instructions.push(generated);
                writes += 1;
            }
            Operation::Extract {
                source,
                destination,
            } => {
                let filename = token(source, "archives")?;
                let inventory = inventories
                    .get(&filename)
                    .context("Unpinned recipe archive")?;
                used.insert(filename.clone());
                let destination = token(destination, "extract")?;
                ensure!(
                    slots.keys().all(|name| name != &destination
                        && !name.starts_with(&format!("{destination}/"))
                        && !destination.starts_with(&format!("{name}/"))),
                    "Overlapping extraction slots"
                );
                ensure!(slots.len() < 4096, "Too many extraction slots");
                let native = format!(
                    "<<modDirectory>>/.modsync-reviewed-{}-slot{}",
                    bound.guid,
                    slots.len()
                );
                instructions.push(instruction(
                    &bound,
                    instructions.len(),
                    "ExtractTo",
                    format!("<<modDirectory>>/{filename}"),
                    native.clone(),
                    false,
                ));
                slots.insert(
                    destination,
                    Slot {
                        native,
                        files: inventory.files.clone(),
                        directories: inventory.directories.clone(),
                    },
                );
            }
            Operation::Copy {
                source,
                destination,
                skip_existing,
            } => {
                let destination = token(destination, "game")?;
                let mut copies = Vec::new();
                if source.starts_with("{archives}/") {
                    let filename = token(source, "archives")?;
                    ensure!(
                        inventories.contains_key(&filename),
                        "Unpinned raw file copy"
                    );
                    used.insert(filename.clone());
                    copies.push((format!("<<modDirectory>>/{filename}"), destination));
                } else {
                    let (slot, member) = extraction_source(source, &slots)?;
                    if slot.files.contains(&member) {
                        copies.push((format!("{}/{}", slot.native, member), destination));
                    } else {
                        ensure!(
                            member.is_empty() || slot.directories.contains(&member),
                            "Copy source absent from verified inventory"
                        );
                        let prefix = if member.is_empty() {
                            String::new()
                        } else {
                            format!("{member}/")
                        };
                        // Empty directory creation has no equivalent in CopyFile.
                        for directory in slot
                            .directories
                            .iter()
                            .filter(|name| name.starts_with(&prefix))
                        {
                            ensure!(
                                slot.files
                                    .iter()
                                    .any(|file| file.starts_with(&format!("{directory}/"))),
                                "Directory overlay includes an unsupported empty directory"
                            );
                        }
                        for file in &slot.files {
                            if let Some(relative) = file.strip_prefix(&prefix) {
                                copies.push((
                                    format!("{}/{}", slot.native, file),
                                    format!("{destination}/{relative}"),
                                ));
                            }
                        }
                        ensure!(!copies.is_empty(), "Directory copy has no verified files");
                    }
                }
                for (source, destination) in copies {
                    instructions.push(instruction(
                        &bound,
                        instructions.len(),
                        "CopyFile",
                        source,
                        format!("<<kotorDirectory>>/{destination}"),
                        !skip_existing.unwrap_or(false),
                    ));
                    writes += 1;
                }
            }
            Operation::Delete { path, missing_ok } => {
                let source = if path.starts_with("{game}/") {
                    format!("<<kotorDirectory>>/{}", token(path, "game")?)
                } else {
                    let (slot, member) = extraction_source(path, &slots)?;
                    ensure!(
                        !member.is_empty() && !slot.directories.contains(&member),
                        "Recursive recipe deletion unsupported"
                    );
                    ensure!(
                        slot.files.contains(&member) || missing_ok == &Some(true),
                        "Strict delete source absent"
                    );
                    let source = format!("{}/{}", slot.native, member);
                    let relative = token(path, "extract")?;
                    for (name, slot) in &mut slots {
                        if let Some(member) = relative.strip_prefix(&format!("{name}/")) {
                            slot.files.remove(member);
                        }
                    }
                    source
                };
                instructions.push(instruction(
                    &bound,
                    instructions.len(),
                    "Delete",
                    source,
                    String::new(),
                    !missing_ok.unwrap_or(false),
                ));
            }
        }
        ensure!(
            instructions.len() <= 10000,
            "Recipe instruction bound exceeded"
        );
    }
    ensure!(
        used == expected && writes > 0,
        "Recipe has unused inputs or no game writes"
    );
    let derivations: Vec<_> = inventories
        .values()
        .map(|item| json!({"Filename":item.filename,"Sha256":item.sha256,"Bytes":item.bytes}))
        .collect();
    bound.instructions = instructions;
    let binding = json!({"Schema":1,"Kind":spec.kind,"DatabookSha256":spec.sha256,
        "SourceGuideSha256":spec.guide_sha256,"SourcePlanSha256":spec.plan_sha256,"Recipe":recipe,
        "InputComponent":recipe.canonical_input_component,"Archives":inventories.values().collect::<Vec<_>>()});
    ensure!(
        serde_json::to_vec(&binding)?.len() <= 16 * 1024 * 1024,
        "Recipe binding exceeds bound"
    );
    bound.metadata.insert(BINDING.into(), binding);
    bound.metadata.insert("DraftReview".into(), json!({"RequiresReview":true,"Generator":spec.generator,
        "Derivations":derivations,"Result":draft::DraftResult { component_guid:bound.guid.clone(),
            draft_instruction_count:bound.instructions.len(),unparsed_gaps:vec![],conditional_drafts:vec![],rejected_drafts:0 }}));
    if community_guid(&bound.guid) {
        community_expected_pending(&bound, spec, &recipe)?;
    }
    check(cancelled)?;
    *component = bound;
    Ok(())
}

/// Regenerate only from the compiled original, never submitted binding metadata.
pub(crate) fn regenerate_owner(
    guid: &str,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Component> {
    let (_, recipe) = find_recipe(guid)?.context("Recipe identity not admitted")?;
    let mut original: Component = if community_guid(guid) {
        community_source(&recipe)?.canonical.clone()
    } else {
        serde_json::from_value(recipe.canonical_input_component)?
    };
    let filenames: Vec<_> = recipe
        .archives
        .into_iter()
        .map(|pin| pin.filename)
        .collect();
    bind(&mut original, &filenames, root, options, cancelled)?;
    Ok(original)
}

/// Regenerate from the compiled reviewed source, real archives and original context.
/// Approved and pending DraftReview differ only in receipt fields; all other data
/// and the exact pending diagnostics must be equal before the caller clears review.
pub(crate) fn replay(
    component: &Component,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Component> {
    let binding = component
        .metadata
        .get(BINDING)
        .context("Missing authored recipe binding")?;
    let (_, recipe) = find_recipe(&component.guid)?.context("Recipe identity not admitted")?;
    let mut original: Component = if community_guid(&component.guid) {
        community_source(&recipe)?.canonical.clone()
    } else {
        serde_json::from_value(
            binding
                .get("InputComponent")
                .cloned()
                .context("Missing recipe input context")?,
        )?
    };
    let filenames: Vec<_> = recipe
        .archives
        .iter()
        .map(|pin| pin.filename.clone())
        .collect();
    bind(&mut original, &filenames, root, options, cancelled)?;
    let mut actual = serde_json::to_value(component)?;
    let mut expected = serde_json::to_value(&original)?;
    actual
        .as_object_mut()
        .context("Invalid component")?
        .remove("DraftReview");
    expected
        .as_object_mut()
        .context("Invalid replay")?
        .remove("DraftReview");
    ensure!(
        actual == expected,
        "Authored recipe context, instructions or inventories differ from replay"
    );
    let record = component
        .metadata
        .get("DraftReview")
        .context("Missing recipe review")?;
    let expected = &original.metadata["DraftReview"];
    for key in ["Generator", "Derivations", "Result"] {
        ensure!(
            record.get(key) == expected.get(key),
            "Recipe diagnostics differ from replay"
        );
    }
    Ok(original)
}
pub(crate) fn replay_digest(component: &Component) -> Result<String> {
    value_digest(&serde_json::to_value(component)?)
}
pub(crate) fn value_digest(value: &Value) -> Result<String> {
    fn sort(value: &mut Value) {
        match value {
            Value::Object(object) => {
                let mut entries: Vec<_> = std::mem::take(object).into_iter().collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                for (key, mut value) in entries {
                    sort(&mut value);
                    object.insert(key, value);
                }
            }
            Value::Array(values) => values.iter_mut().for_each(sort),
            _ => {}
        }
    }
    let mut value = value.clone();
    sort(&mut value);
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(&value)?)))
}

/// Check the approved model and its immutable archive bytes before consumption.
/// Public receipt hashes are identities, not authorization. Runtime independently
/// reconstructs the complete model from compiled authority and pinned archives.
/// Archive decoding uses bounded private scratch, never game/mod writes or programs.
pub(crate) fn verify_runtime(
    component: &Component,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<()> {
    if crate::k2_preparation::contains_master(component) {
        return crate::k2_preparation::verify_master_runtime(component, root, options, cancelled);
    }
    if super::source_plan_namespace::contains_native(component) {
        super::source_plan_namespace::verify_runtime(component, root, options, cancelled)?;
        return Ok(());
    }
    if super::duplicate_cleanup::contains(component) {
        return super::duplicate_cleanup::verify_runtime(component, cancelled);
    }
    if super::reviewed_master::contains_resolved(component) {
        return super::reviewed_master::verify_runtime(component, root, options, cancelled);
    }
    let Some(binding) = component.metadata.get(BINDING) else {
        ensure!(
            component
                .metadata
                .get("DraftReview")
                .and_then(|v| v.get("Generator"))
                .and_then(Value::as_str)
                .is_none_or(|generator| generator != GENERATOR && generator != PATCHER_GENERATOR),
            "Reviewed recipe lost its binding"
        );
        return Ok(());
    };
    check(cancelled)?;
    let (spec, recipe) =
        find_recipe(&component.guid)?.context("Runtime recipe is not in compiled databook")?;
    let original: Component = serde_json::from_value(
        binding
            .get("InputComponent")
            .cloned()
            .context("Missing recipe context")?,
    )?;
    canonical_context(&recipe, &original)?;
    let object = binding.as_object().context("Invalid recipe binding")?;
    const BINDING_KEYS: [&str; 7] = [
        "Schema",
        "Kind",
        "DatabookSha256",
        "SourceGuideSha256",
        "SourcePlanSha256",
        "Recipe",
        "InputComponent",
    ];
    ensure!(
        object.len() == BINDING_KEYS.len() + 1
            && object
                .keys()
                .all(|key| BINDING_KEYS.contains(&key.as_str()) || key == "Archives")
            && binding.get("Schema") == Some(&json!(1))
            && binding.get("Kind") == Some(&json!(spec.kind))
            && binding.get("DatabookSha256") == Some(&json!(spec.sha256))
            && binding.get("SourceGuideSha256") == Some(&json!(spec.guide_sha256))
            && binding.get("SourcePlanSha256") == Some(&json!(spec.plan_sha256))
            && binding.get("Recipe") == Some(&serde_json::to_value(&recipe)?)
            && binding.get("InputComponent") == Some(&recipe.canonical_input_component),
        "Runtime recipe provenance changed"
    );
    let inventories: Vec<Inventory> = serde_json::from_value(
        binding
            .get("Archives")
            .cloned()
            .context("Missing bound inventories")?,
    )?;
    ensure!(
        inventories.len() == recipe.archives.len(),
        "Runtime recipe archive count changed"
    );
    let expected: BTreeMap<_, _> = recipe
        .archives
        .iter()
        .map(|pin| (pin.filename.as_str(), pin.sha256.as_str()))
        .collect();
    let mut seen = BTreeSet::new();
    let root = super::resolved::local_root(root)?;
    for inventory in &inventories {
        ensure!(
            seen.insert(&inventory.filename)
                && expected.get(inventory.filename.as_str()) == Some(&inventory.sha256.as_str()),
            "Runtime recipe archive identity changed"
        );
        let filename = literal(&inventory.filename)?;
        ensure!(
            !filename.contains('/')
                && hash(&root.join(filename), cancelled)?
                    == (inventory.sha256.clone(), inventory.bytes),
            "Runtime recipe archive bytes changed"
        );
    }
    let record = component
        .metadata
        .get("DraftReview")
        .context("Missing recipe approval")?;
    const REVIEW_KEYS: [&str; 6] = [
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
            .is_some_and(|record| record.len() == REVIEW_KEYS.len()
                && record.keys().all(|key| REVIEW_KEYS.contains(&key.as_str())))
            && record.get("RequiresReview").and_then(Value::as_bool) == Some(false)
            && record.get("Generator").and_then(Value::as_str) == Some(spec.generator)
            && record.get("ReviewedIdentity").and_then(Value::as_str)
                == Some(draft::review::identity(component)?.as_str()),
        "Runtime recipe is not a cleared unchanged review"
    );
    let derivations: Vec<_> = inventories
        .iter()
        .map(|item| json!({"Filename":item.filename,"Sha256":item.sha256,"Bytes":item.bytes}))
        .collect();
    let pending = json!({"RequiresReview":true,"Generator":spec.generator,"Derivations":derivations,
        "Result":draft::DraftResult {component_guid:component.guid.clone(),draft_instruction_count:component.instructions.len(),
            unparsed_gaps:vec![],conditional_drafts:vec![],rejected_drafts:0}});
    ensure!(
        record.get("Derivations") == pending.get("Derivations")
            && record.get("Result") == pending.get("Result"),
        "Runtime recipe diagnostics changed"
    );
    // A caller can edit instructions and recompute both public receipt hashes.
    // Only regeneration from the compiled recipe can authorize those instructions.
    let replayed = replay(component, &root, options, cancelled)?;
    let verification = record
        .get("Verification")
        .context("Missing verified replay receipt")?;
    ensure!(
        verification
            == &json!({"Reviewer":draft::review::VERIFIER_NAME,"Kind":draft::review::VERIFICATION_KIND,
        "ToolVersion":concat!("ModSync.rs/",env!("CARGO_PKG_VERSION")),"ArchiveSha256":spec.sha256,
        "ArchiveFilename":spec.filename,"ReplayDigest":replay_digest(&replayed)?,"IdentityVersion":2}),
        "Runtime recipe verification does not match unchanged replayed model"
    );
    check(cancelled)
}
