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
    sync::atomic::{AtomicBool, Ordering},
};

pub(crate) const GENERATOR: &str = "reviewed-manual-file-effects";
pub(crate) const PATCHER_GENERATOR: &str = "reviewed-manual-native-patcher";
const BINDING: &str = "AuthoredRecipeBinding";
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
const BOOKS: [BookSpec; 6] = [
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

/// Admit exactly the existing two-operation shape or either closed three-step
/// source plan. No other action, deletion, slot, optional deletion or owner is
/// admitted by extending the Patcher operation index.
fn native_shape(recipe: &Recipe) -> Result<Option<&'static str>> {
    let translation = recipe
        .native_patcher
        .as_ref()
        .context("Missing native translation")?;
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
        })
    }
    pub(crate) fn component_guid(&self) -> &str {
        &self.component_guid
    }
    pub(crate) fn requires_exact_prediction(&self) -> bool {
        self.source_plan.is_some() || self.tomb_shadow_identity_observation()
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
        if let Some(admitted) = &self.source_plan {
            admitted.verify_staged(base, cancelled)?;
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
    Ok(find_recipe(guid)?.is_some())
}
/// Verification identity belongs to the exact compiled book admitting this GUID.
pub(crate) fn verification_identity(guid: &str) -> Result<(String, String)> {
    let (spec, _) = find_recipe(guid)?.context("Recipe identity not admitted")?;
    Ok((spec.filename.into(), spec.sha256.into()))
}
/// Filename admission exception for an exact compiled recipe/archive pair only.
/// This grants neither recipe approval nor permission to skip byte verification.
pub fn allows_literal_filename(guid: &str, filename: &str) -> Result<bool> {
    Ok(find_recipe(guid)?
        .is_some_and(|(_, recipe)| recipe.archives.iter().any(|pin| pin.filename == filename)))
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
    let mut file = crate::paths::open_regular_read(path)?;
    let expected = file.metadata()?.len();
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
        bytes == expected && bytes > 0,
        "Archive changed or is empty"
    );
    Ok((format!("{:x}", hash.finalize()), bytes))
}
fn canonical_context(recipe: &Recipe, component: &Component) -> Result<Component> {
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
    let (sha256, bytes) = hash(&path, cancelled)?;
    ensure!(
        sha256 == pin.sha256,
        "Reviewed archive hash mismatch: {filename}"
    );
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
        recipe.archives.len() == 1,
        "Native translation requires one archive and canonical sole INI"
    );
    translation.installer_parent()?;
    let packaged_log = native_shape(recipe)?;
    let mut packaged_log_found = false;
    let pin = &recipe.archives[0];
    let mut bounded = options.clone();
    bounded.max_expanded_bytes = Some(
        options
            .max_expanded_bytes
            .unwrap_or(64 * 1024 * 1024)
            .min(64 * 1024 * 1024),
    );
    let entries = archive::unpack(
        &root.join(&pin.filename),
        Path::new(&pin.filename),
        &bounded,
        cancelled,
    )?;
    let mut found = BTreeSet::new();
    let mut ini_count = 0;
    for entry in entries {
        check(cancelled)?;
        let name = entry
            .path
            .to_str()
            .context("Non-UTF8 native member")?
            .replace('\\', "/");
        let lower = name.to_lowercase();
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
    let packaged_log = native_shape(&recipe)?;
    ensure!(
        spec.generator == PATCHER_GENERATOR
            && component.instructions.len() == recipe.plan_step.operations.len(),
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
    let native = format!(
        "<<modDirectory>>/.modsync-reviewed-{}-slot0",
        component.guid
    );
    let expected_extract = instruction(
        component,
        0,
        "ExtractTo",
        format!("<<modDirectory>>/{}", recipe.archives[0].filename),
        native.clone(),
        false,
    );
    let mut expected_instructions = vec![expected_extract];
    if let Some(member) = packaged_log {
        expected_instructions.push(instruction(
            component,
            1,
            "Delete",
            format!("{native}/{member}"),
            String::new(),
            true,
        ));
    }
    let mut expected_patcher = instruction(
        component,
        translation.operation_index,
        "Patcher",
        format!("{native}/{}", translation.installer.path),
        translation.destination.clone(),
        false,
    );
    expected_patcher.arguments = translation.arguments.clone();
    expected_instructions.push(expected_patcher);
    ensure!(
        serde_json::to_value(&component.instructions)?
            == serde_json::to_value(expected_instructions)?,
        "Native Patcher complete instruction model differs from catalog"
    );
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
    for pin in &recipe.archives {
        inventories.insert(
            pin.filename.clone(),
            inspect(pin, &root, options, cancelled)?,
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
            Operation::GuiPatcher {
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
                        && recipe.archives.len() == 1
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
    let mut original: Component = serde_json::from_value(recipe.canonical_input_component)?;
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
    let mut original: Component = serde_json::from_value(
        binding
            .get("InputComponent")
            .cloned()
            .context("Missing recipe input context")?,
    )?;
    let (_, recipe) = find_recipe(&component.guid)?.context("Recipe identity not admitted")?;
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
