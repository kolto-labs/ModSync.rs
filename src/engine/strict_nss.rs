//! Native NSS installation is enabled only by a closed source-plan certificate.
use crate::archive_generation::{
    reviewed_manual::NativePatcherContract, source_plan_patcher::CertifiedNssInputs,
};
use anyhow::{Context, Result, ensure};
use odypatcher::patcher::{
    installer::ModInstaller,
    mods::nss::strict::{StrictNssBundle, StrictNssPolicy, VirtualCompileOptions},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

fn check(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Relaxed),
        "Certified NSS operation cancelled"
    );
    Ok(())
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

// Staged bytes are authenticated below, so an ancestor replacement cannot grant
// authority over different contents. Reject observed links before every open.
fn regular_bytes(path: &Path, limit: u64, cancelled: &AtomicBool) -> Result<Vec<u8>> {
    check(cancelled)?;
    ensure!(path.is_absolute(), "Certified NSS path must be absolute");
    let mut ancestor = PathBuf::new();
    for part in path.components() {
        ensure!(
            matches!(
                part,
                Component::RootDir | Component::Prefix(_) | Component::Normal(_)
            ),
            "Unsafe certified NSS path"
        );
        ancestor.push(part);
        let meta = fs::symlink_metadata(&ancestor)?;
        ensure!(
            !meta.file_type().is_symlink(),
            "Certified NSS ancestor is a symbolic link"
        );
    }
    let file = crate::paths::open_regular_read(path)?;
    ensure!(
        file.metadata()?.len() <= limit,
        "Certified NSS input exceeds bound"
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Certified NSS input grew past bound"
    );
    check(cancelled)?;
    Ok(bytes)
}

/// Install before configured-path validation. A missing certificate never enables
/// compilation; parent admission separately rejects every uncertified CompileList.
pub(super) fn prepare(
    contract: &NativePatcherContract,
    staged_package: &Path,
    installer: &mut ModInstaller,
    cancelled: Arc<AtomicBool>,
) -> Result<()> {
    if let Some(inputs) = contract.certified_program_nss() {
        ensure!(
            contract.certified_nss().is_none(),
            "Conflicting NSS authorities"
        );
        return prepare_program(inputs, staged_package, installer, cancelled);
    }
    let Some(inputs) = contract.certified_nss() else {
        return Ok(());
    };
    check(&cancelled)?;
    ensure!(
        installer.game == Some(inputs.game()),
        "Certified NSS game mismatch"
    );
    inputs.validate_config(
        installer
            .config()
            .map_err(|e| anyhow::anyhow!("Certified NSS configuration: {e}"))?,
    )?;
    let data = staged_package.join("tslpatchdata");
    ensure!(
        data == installer.patch_data_path,
        "Certified NSS staged source root mismatch"
    );
    let private = tempfile::Builder::new()
        .prefix("modsync-certified-nss-")
        .tempdir()?;
    let mut expected = BTreeMap::new();
    for (name, length, digest) in inputs.inputs() {
        let bytes = regular_bytes(&data.join(name), length, &cancelled)?;
        ensure!(
            bytes.len() as u64 == length && hash(&bytes) == digest,
            "Certified NSS original input changed: {name}"
        );
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(private.path().join(name))?;
        output.write_all(&bytes)?;
        expected.insert(name.to_owned(), bytes);
    }
    let bundle = StrictNssBundle::load(private.path(), &expected)
        .map_err(|e| anyhow::anyhow!("Certified NSS closure: {e}"))?;
    let options = VirtualCompileOptions {
        cancelled,
        deadline: Some(Instant::now() + Duration::from_secs(120)),
        ..Default::default()
    };
    let mut policy = StrictNssPolicy::new(inputs.game(), options)
        .map_err(|e| anyhow::anyhow!("Certified NSS policy: {e}"))?;
    policy
        .add_bundle(inputs.sourcefolder().to_owned(), bundle)
        .map_err(|e| anyhow::anyhow!("Certified NSS bundle: {e}"))?;
    installer.set_strict_nss_policy(policy);
    Ok(())
}

#[derive(Serialize)]
pub(super) struct CompiledIdentity {
    source_name: String,
    raw_source_sha256: String,
    transformed_source_sha256: String,
    ncs_sha256: String,
    ncs_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    destination: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    saveas: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    included_files: Option<Vec<String>>,
}

fn child(parent: &Path, name: &str, cancelled: &AtomicBool) -> Result<PathBuf> {
    let mut found = None;
    for (i, entry) in fs::read_dir(parent)?.enumerate() {
        check(cancelled)?;
        ensure!(
            i < 100_000,
            "Certified NSS output directory exceeds entry bound"
        );
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|n| n.eq_ignore_ascii_case(name))
        {
            ensure!(found.is_none(), "Certified NSS output has case aliases");
            found = Some(entry.path());
        }
    }
    found.context("Certified NSS output missing")
}

/// Call only after installer success; count/order checks reject skipped scripts,
/// and exact hashes bind installed bytes to the successful compiler result.
pub(super) fn finish(
    contract: &NativePatcherContract,
    installer: &ModInstaller,
    cancelled: &AtomicBool,
) -> Result<Vec<CompiledIdentity>> {
    if let Some(inputs) = contract.certified_program_nss() {
        ensure!(
            contract.certified_nss().is_none(),
            "Conflicting NSS authorities"
        );
        return finish_program(inputs, installer, cancelled);
    }
    let Some(inputs) = contract.certified_nss() else {
        ensure!(
            installer.strict_nss_outcomes().is_empty(),
            "Uncertified native compiler outcomes"
        );
        return Ok(Vec::new());
    };
    finish_certified(inputs, installer, cancelled)
}
fn finish_certified(
    inputs: &CertifiedNssInputs,
    installer: &ModInstaller,
    cancelled: &AtomicBool,
) -> Result<Vec<CompiledIdentity>> {
    let outcomes = installer.strict_nss_outcomes();
    ensure!(
        outcomes.len() == inputs.mains().len(),
        "Certified NSS compilation count mismatch (including skipped scripts)"
    );
    let override_dir = child(&installer.game_path, "Override", cancelled)?;
    let pins: BTreeMap<_, _> = inputs
        .inputs()
        .into_iter()
        .map(|(n, _, h)| (n, h))
        .collect();
    let mut identities = Vec::new();
    for (outcome, name) in outcomes.iter().zip(inputs.mains()) {
        check(cancelled)?;
        ensure!(
            outcome.source_name == *name && outcome.error.is_none() && outcome.warnings.is_empty(),
            "Certified NSS compilation order, warning or failure mismatch"
        );
        let mut included = vec!["nwscript.nss", *name];
        if *name == "a_forcepointheal.nss" {
            included.push("k_inc_debug.nss");
        }
        ensure!(
            outcome.included_files == included,
            "Certified NSS actual include closure changed"
        );
        ensure!(
            outcome.raw_source_sha256.as_deref() == Some(pins[name]),
            "Certified NSS original main hash mismatch"
        );
        let transformed = outcome
            .transformed_source_sha256
            .as_ref()
            .context("Missing transformed NSS identity")?;
        ensure!(
            transformed.len() == 64 && transformed.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid transformed NSS identity"
        );
        let ncs_hash = outcome
            .ncs_sha256
            .as_ref()
            .context("Missing compiled NSS identity")?;
        let output = child(&override_dir, &name.replace(".nss", ".ncs"), cancelled)?;
        let bytes = regular_bytes(&output, 2 * 1024 * 1024, cancelled)?;
        ensure!(
            bytes.len() >= 13 && bytes.starts_with(b"NCS V1.0") && hash(&bytes) == *ncs_hash,
            "Installed NCS differs from compiled output"
        );
        identities.push(CompiledIdentity {
            source_name: (*name).to_owned(),
            raw_source_sha256: pins[name].to_owned(),
            transformed_source_sha256: transformed.clone(),
            ncs_sha256: ncs_hash.clone(),
            ncs_bytes: bytes.len() as u64,
            destination: None,
            saveas: None,
            included_files: None,
        });
    }
    Ok(identities)
}

// Resolve every game/package path component case-unambiguously before opening.
fn optional_path(root: &Path, relative: &str, cancel: &AtomicBool) -> Result<Option<PathBuf>> {
    let relative = crate::paths::relative(&relative.replace('\\', "/"), false)?;
    let mut path = root.to_owned();
    let root_meta = fs::symlink_metadata(root)?;
    ensure!(
        root_meta.is_dir() && !root_meta.file_type().is_symlink(),
        "Certified NSS root is not a real directory"
    );
    for (position, part) in relative.components().enumerate() {
        check(cancel)?;
        let mut found = None;
        for (index, entry) in fs::read_dir(&path)?.enumerate() {
            check(cancel)?;
            ensure!(index < 10000, "Certified NSS path directory exceeds bound");
            let entry = entry?;
            if crate::unicode_case::ordinal_key(
                entry.file_name().to_str().context("Non UTF8 NSS path")?,
            ) == crate::unicode_case::ordinal_key(
                part.as_os_str()
                    .to_str()
                    .context("Non UTF8 NSS component")?,
            ) {
                ensure!(found.is_none(), "Certified NSS path has case aliases");
                found = Some(entry.path());
            }
        }
        let Some(found) = found else {
            return Ok(None);
        };
        let meta = fs::symlink_metadata(&found)?;
        ensure!(
            !meta.file_type().is_symlink() && (meta.is_dir() || meta.is_file()),
            "Certified NSS path is linked or special"
        );
        ensure!(
            position + 1 == relative.components().count() || meta.is_dir(),
            "Certified NSS ancestor is not a directory"
        );
        path = found;
    }
    Ok(Some(path))
}

fn prepare_program(
    inputs: &crate::archive_generation::source_plan_namespace::CertifiedProgramNss,
    staged: &Path,
    installer: &mut ModInstaller,
    cancelled: Arc<AtomicBool>,
) -> Result<()> {
    check(&cancelled)?;
    ensure!(
        cfg!(target_os = "linux"),
        "Strict original-program NSS is currently supported only on Linux"
    );
    ensure!(
        installer.game == Some(inputs.game()),
        "Original program NSS actual game mismatch"
    );
    inputs.validate_config(
        installer
            .config()
            .map_err(|e| anyhow::anyhow!("Original program NSS config: {e}"))?,
    )?;
    let data = staged.join(crate::paths::relative(
        inputs.selected_data_relative(),
        false,
    )?);
    ensure!(
        data == installer.patch_data_path,
        "Original program selected NSS source root mismatch"
    );
    let options = VirtualCompileOptions {
        cancelled: cancelled.clone(),
        deadline: Some(Instant::now() + Duration::from_secs(120)),
        ..Default::default()
    };
    let mut policy = StrictNssPolicy::new(inputs.game(), options)
        .map_err(|e| anyhow::anyhow!("Original NSS policy: {e}"))?;
    for bundle in inputs.bundles() {
        check(&cancelled)?;
        let private = tempfile::Builder::new()
            .prefix("modsync-original-nss-")
            .tempdir()?;
        let mut expected = BTreeMap::new();
        for (name, input) in bundle.inputs() {
            let raw = if input.is_prepared_header() {
                ensure!(
                    installer.game == Some(odypatcher::Game::K2)
                        && inputs.game() == odypatcher::Game::K2
                        && name == "nwscript.nss"
                        && input.sha256()
                            == "afb8adb2c2806b9c5ef75360ec75ecc62b74bcfe9d42cc4f4e1d305b26a7805d"
                        && input.raw().len() == 266225,
                    "Original prepared header does not match actual typed game"
                );
                input.raw().to_vec()
            } else {
                let path = optional_path(staged, input.package_path(), &cancelled)?
                    .context("Original NSS staged input absent")?;
                regular_bytes(&path, input.raw().len() as u64, &cancelled)?
            };
            ensure!(
                raw == input.raw() && hash(&raw) == input.sha256(),
                "Original NSS staged input changed"
            );
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(private.path().join(name))?;
            output.write_all(&raw)?;
            expected.insert(name.clone(), raw);
        }
        let snapshot = StrictNssBundle::load(private.path(), &expected)
            .map_err(|e| anyhow::anyhow!("Original NSS closure: {e}"))?;
        policy
            .add_bundle(bundle.sourcefolder().to_owned(), snapshot)
            .map_err(|e| anyhow::anyhow!("Original NSS bundle: {e}"))?;
    }
    // Resolve all module predecessors before any installer operation. A normal
    // FileN seed prefers an existing current module; ReplaceN selects its seed.
    let mut snapshots = BTreeMap::<PathBuf, Vec<u8>>::new();
    let mut total = 0usize;
    for main in inputs.mains() {
        check(&cancelled)?;
        let base = main.base();
        if base.destination.eq_ignore_ascii_case("override") {
            if !base.replace_file && base.skip_if_not_replace {
                ensure!(
                    optional_path(
                        &installer.game_path,
                        &format!("{}/{}", base.destination, base.saveas),
                        &cancelled
                    )?
                    .is_none(),
                    "Required original NSS would skip an existing loose output"
                );
            }
            continue;
        }
        let current = optional_path(&installer.game_path, &base.destination, &cancelled)?;
        let (path, origin) = if let Some(seed) = main.seed() {
            if !seed.replace_file && current.is_some() {
                (
                    current.context("Missing current NSS module")?,
                    "current-predecessor-module",
                )
            } else {
                (
                    optional_path(staged, &seed.package_path, &cancelled)?
                        .context("Original NSS module seed absent")?,
                    "preceding-packaged-module",
                )
            }
        } else {
            (
                current.context("Required NSS predecessor MOD absent; no KEY/BIF fallback")?,
                "current-predecessor-module",
            )
        };
        if let std::collections::btree_map::Entry::Vacant(slot) = snapshots.entry(path.clone()) {
            let bytes = regular_bytes(&path, 128 * 1024 * 1024, &cancelled)?;
            total = total
                .checked_add(bytes.len())
                .context("NSS module snapshot overflow")?;
            ensure!(
                total <= 512 * 1024 * 1024,
                "NSS module snapshots exceed aggregate bound"
            );
            slot.insert(bytes);
        }
        let bytes = &snapshots[&path];
        if origin == "preceding-packaged-module" {
            let seed = main
                .seed()
                .context("Original module seed identity absent")?;
            ensure!(
                bytes.len() as u64 == seed.bytes && hash(bytes) == seed.sha256,
                "Original NSS module seed changed"
            );
        }
        let existing = crate::archive_generation::source_plan_patcher::module_resource_optional(
            bytes,
            &base.saveas,
            &cancelled,
        )?;
        ensure!(
            base.replace_file || !base.skip_if_not_replace || existing.is_none(),
            "Required original NSS would skip an existing module resource"
        );
        installer.log.add_note(format!("Original NSS destination preflight: {}",serde_json::json!({"Destination":base.destination,"SaveAs":base.saveas,"Origin":origin,"ContainerSha256":hash(bytes),"ContainerBytes":bytes.len(),"ExistingResourceSha256":existing.map(hash)})));
    }
    installer.set_strict_nss_policy(policy);
    Ok(())
}

fn finish_program(
    inputs: &crate::archive_generation::source_plan_namespace::CertifiedProgramNss,
    installer: &ModInstaller,
    cancelled: &AtomicBool,
) -> Result<Vec<CompiledIdentity>> {
    ensure!(
        installer.game == Some(inputs.game()),
        "Original NSS actual game changed"
    );
    let outcomes = installer.strict_nss_outcomes();
    ensure!(
        outcomes.len() == inputs.mains().len(),
        "Original NSS required compilation count mismatch, including skipped scripts"
    );
    let mut identities = Vec::new();
    let mut modules = BTreeMap::<PathBuf, Vec<u8>>::new();
    let mut total = 0usize;
    for (outcome, main) in outcomes.iter().zip(inputs.mains()) {
        check(cancelled)?;
        ensure!(
            outcome.source_name == main.source_name()
                && outcome.error.is_none()
                && outcome.warnings.is_empty(),
            "Original NSS ordered compilation failed, warned or changed source"
        );
        ensure!(
            outcome.raw_source_sha256.as_deref() == Some(main.raw_source_sha256()),
            "Original NSS raw main identity changed"
        );
        let bundle = &inputs.bundles()[main.bundle_index()];
        ensure!(
            outcome.included_files.len() >= 2
                && outcome.included_files[0] == "nwscript.nss"
                && outcome.included_files[1] == main.source_name(),
            "Original NSS evaluated closure lacks header/main prefix"
        );
        let mut seen = std::collections::BTreeSet::new();
        for name in &outcome.included_files {
            ensure!(
                bundle.inputs().contains_key(name) && seen.insert(name),
                "Original NSS consumed uncertified or repeated input"
            );
        }
        let transformed = outcome
            .transformed_source_sha256
            .as_ref()
            .context("Original NSS transformed identity absent")?;
        let compiled = outcome
            .ncs_sha256
            .as_ref()
            .context("Original NSS compiled identity absent")?;
        ensure!(
            [transformed, compiled]
                .iter()
                .all(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())),
            "Invalid original NSS outcome hash"
        );
        let base = main.base();
        let owned;
        let bytes = if base.destination.eq_ignore_ascii_case("override") {
            let path = optional_path(
                &installer.game_path,
                &format!("{}/{}", base.destination, base.saveas),
                cancelled,
            )?
            .context("Original NSS loose output absent")?;
            owned = regular_bytes(&path, 2 * 1024 * 1024, cancelled)?;
            owned.as_slice()
        } else {
            let path = optional_path(&installer.game_path, &base.destination, cancelled)?
                .context("Original NSS output MOD absent")?;
            if let std::collections::btree_map::Entry::Vacant(slot) = modules.entry(path.clone()) {
                let bytes = regular_bytes(&path, 128 * 1024 * 1024, cancelled)?;
                total = total
                    .checked_add(bytes.len())
                    .context("NSS output module overflow")?;
                ensure!(
                    total <= 512 * 1024 * 1024,
                    "NSS output modules exceed aggregate bound"
                );
                slot.insert(bytes);
            }
            crate::archive_generation::source_plan_patcher::module_resource(
                &modules[&path],
                &base.saveas,
                cancelled,
            )?
        };
        ensure!(
            bytes.len() >= 13
                && bytes.len() <= 2 * 1024 * 1024
                && bytes.starts_with(b"NCS V1.0")
                && hash(bytes) == *compiled,
            "Original NSS installed destination resource differs from compiled output"
        );
        identities.push(CompiledIdentity {
            source_name: main.source_name().to_owned(),
            raw_source_sha256: main.raw_source_sha256().to_owned(),
            transformed_source_sha256: transformed.clone(),
            ncs_sha256: compiled.clone(),
            ncs_bytes: bytes.len() as u64,
            destination: Some(base.destination.clone()),
            saveas: Some(base.saveas.clone()),
            included_files: Some(outcome.included_files.clone()),
        });
    }
    Ok(identities)
}
