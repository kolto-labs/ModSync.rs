//! Closed destination-resource preflight shared by prediction and publication.
//! Authority is an independently admitted contract, never serialized metadata.
use crate::archive_generation::{reviewed_manual::NativePatcherContract, source_plan_patcher};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const MAX_FILE: u64 = 128 * 1024 * 1024;
const MAX_ENTRIES: usize = 10000;

fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Destination resource preflight cancelled"
    );
    Ok(())
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn existing(root: &Path, relative: &str, cancel: &AtomicBool) -> Result<Option<PathBuf>> {
    let relative = crate::paths::relative(&relative.replace('\\', "/"), false)?;
    let root_metadata = fs::symlink_metadata(root)?;
    ensure!(
        root_metadata.is_dir() && !root_metadata.file_type().is_symlink(),
        "Resource root must be a real directory"
    );
    let mut path = root.to_path_buf();
    for (position, part) in relative.components().enumerate() {
        check(cancel)?;
        let wanted = crate::unicode_case::ordinal_key(
            part.as_os_str()
                .to_str()
                .context("Non UTF8 resource path")?,
        );
        let mut found = None;
        for (index, entry) in fs::read_dir(&path)?.enumerate() {
            check(cancel)?;
            ensure!(
                index < MAX_ENTRIES,
                "Resource directory exceeds entry bound"
            );
            let entry = entry?;
            let name = entry.file_name();
            if crate::unicode_case::ordinal_key(
                name.to_str().context("Non UTF8 resource directory entry")?,
            ) == wanted
            {
                ensure!(
                    found.is_none(),
                    "Destination resource path has case aliases"
                );
                found = Some(entry.path());
            }
        }
        let Some(next) = found else { return Ok(None) };
        let metadata = fs::symlink_metadata(&next)?;
        ensure!(
            !metadata.file_type().is_symlink() && (metadata.is_file() || metadata.is_dir()),
            "Destination resource path has a link or special file"
        );
        if position + 1 < relative.components().count() {
            ensure!(
                metadata.is_dir(),
                "Destination resource ancestor is not a directory"
            );
        }
        path = next;
    }
    Ok(Some(path))
}

fn snapshot(path: &Path, cancel: &AtomicBool) -> Result<Vec<u8>> {
    check(cancel)?;
    let mut file = crate::paths::open_regular_read(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.len() > 0 && metadata.len() <= MAX_FILE,
        "Destination resource source exceeds file bound"
    );
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 65536];
    loop {
        check(cancel)?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        ensure!(
            bytes.len() as u64 + count as u64 <= metadata.len(),
            "Destination resource source grew during read"
        );
        bytes.extend_from_slice(&buffer[..count]);
    }
    let after = file.metadata()?;
    ensure!(
        bytes.len() as u64 == metadata.len()
            && after.len() == metadata.len()
            && after.modified().ok() == metadata.modified().ok(),
        "Destination resource source changed during read"
    );
    Ok(bytes)
}

pub(super) struct PreparedResources {
    evidence: Value,
    required: usize,
}
impl PreparedResources {
    pub(super) fn finish(&self, report: &odypatcher::RunReport) -> Result<Value> {
        ensure!(
            report.errors == 0
                && report.warnings == 0
                && report.patches_failed == 0
                && report.patches_completed >= self.required,
            "Destination resource native outcome is failed, warning-bearing or incomplete"
        );
        // InstallList FileN skips for already present modules are original semantics;
        // the source preflight selected that current module rather than its seed.
        Ok(self.evidence.clone())
    }
}

/// Must run directly after original staged verification and before normalization
/// or any installer effect. Missing actual predecessor sources are fatal.
pub(super) fn prepare(
    contract: Option<&NativePatcherContract>,
    base: &Path,
    game: &Path,
    cancel: &AtomicBool,
) -> Result<Option<PreparedResources>> {
    let Some(contract) = contract else {
        return Ok(None);
    };
    let Some(requirements) = contract.destination_resources() else {
        return Ok(None);
    };
    if requirements.is_empty() {
        return Ok(None);
    }
    ensure!(
        contract.requires_exact_prediction() && requirements.len() <= MAX_ENTRIES,
        "Destination resources require the owned exact-prediction contract"
    );
    // Recheck the original complete package before reading any of its seed bytes.
    contract.verify_staged(base, cancel)?;
    let mut snapshots: BTreeMap<PathBuf, Vec<u8>> = BTreeMap::new();
    let mut total = 0u64;
    let mut records = Vec::new();
    for requirement in requirements {
        check(cancel)?;
        let destination = existing(game, &requirement.destination, cancel)?;
        let (path, origin, resource_name) = if let Some(seed) = &requirement.seed {
            if !seed.replace_file && destination.is_some() {
                (
                    destination.context("Current module absent")?,
                    "current-predecessor-module",
                    Some(requirement.saveas.as_str()),
                )
            } else {
                (
                    existing(base, &seed.package_path, cancel)?
                        .context("Authenticated packaged module seed absent")?,
                    "preceding-packaged-module",
                    Some(requirement.saveas.as_str()),
                )
            }
        } else if requirement
            .destination
            .to_ascii_lowercase()
            .ends_with(".mod")
        {
            (destination.context("Required predecessor module absent; no packaged seed or KEY/BIF fallback is admitted")?,
                "current-predecessor-module", Some(requirement.saveas.as_str()))
        } else {
            let relative = format!("{}/{}", requirement.destination, requirement.saveas);
            (
                existing(game, &relative, cancel)?.context(
                    "Required predecessor loose resource absent; no KEY/BIF fallback is admitted",
                )?,
                "current-predecessor-loose",
                None,
            )
        };
        if let std::collections::btree_map::Entry::Vacant(slot) = snapshots.entry(path.clone()) {
            let bytes = snapshot(&path, cancel)?;
            total = total
                .checked_add(bytes.len() as u64)
                .context("Resource snapshot total overflow")?;
            ensure!(
                total <= 512 * 1024 * 1024,
                "Destination resource snapshots exceed total byte bound"
            );
            slot.insert(bytes);
        }
        let bytes = &snapshots[&path];
        if origin == "preceding-packaged-module" {
            let seed = requirement
                .seed
                .as_ref()
                .context("Missing certified module seed")?;
            ensure!(
                bytes.len() as u64 == seed.bytes && hash(bytes) == seed.sha256,
                "Packaged module seed differs from authenticated original"
            );
        }
        let resource = if let Some(name) = resource_name {
            source_plan_patcher::module_resource(bytes, name, cancel)?
        } else {
            bytes.as_slice()
        };
        if origin == "preceding-packaged-module" {
            let seed = requirement
                .seed
                .as_ref()
                .context("Missing certified module seed")?;
            ensure!(
                resource.len() as u64 == seed.resource_bytes
                    && hash(resource) == seed.resource_sha256,
                "Packaged module resource differs from certified original"
            );
        }
        records.push(json!({"SourceFile":requirement.sourcefile,"Destination":requirement.destination,
            "SaveAs":requirement.saveas,"Origin":origin,"ContainerSha256":hash(bytes),"ContainerBytes":bytes.len(),
            "ResourceSha256":hash(resource),"ResourceBytes":resource.len()}));
    }
    Ok(Some(PreparedResources {
        required: requirements.len(),
        evidence: json!({"Schema":1,
        "Policy":"closed-current-destination-saveas-sources-v1","ComponentGuid":contract.component_guid(),
        "RequiredSources":records,"KeyBifFallback":false}),
    }))
}
