//! Read-only measurement for the exact TOMB duplicate-script contract.
//! The best-effort observation grants no authority. The separate typed verifier
//! fails on unequal bytes; neither path changes the logger or filesystem.
use anyhow::{Result, anyhow, ensure};
use odypatcher::formats::erf::{ArchiveKind, ErfFile, extension_to_res_type};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const SCRIPT: &str = "k_oei_userdef";
const SOURCE_PATH: &str = "tslpatchdata/k_oei_userdef.ncs";
const OVERRIDE_PATH: &str = "Override/k_oei_userdef.ncs";
const MODULE_PATH: &str = "Modules/711kor.mod";
const RESOURCE_PATH: &str = "Modules/711kor.mod#k_oei_userdef.ncs";
const EXPECTED_BYTES: usize = 40_430;
const EXPECTED_SHA256: &str = "1391e63a11af0669fc86146644be76e934e34ea296df48a280a6798b4d354113";
const SCRIPT_LIMIT: usize = 1024 * 1024;
const MODULE_LIMIT: usize = 64 * 1024 * 1024;
const DIRECTORY_LIMIT: usize = 100_000;

// Constructed only from fresh bounded snapshots; never deserialized from receipts.
struct Measurement {
    source: FileIdentity,
    loose: FileIdentity,
    resource: FileIdentity,
    module: FileIdentity,
    source_equals_override: bool,
    source_equals_module: bool,
    override_equals_module: bool,
}

#[derive(serde::Serialize)]
struct FileIdentity {
    path: &'static str,
    bytes: usize,
    sha256: String,
}

impl Measurement {
    fn all_equal(&self) -> bool {
        self.source_equals_override && self.source_equals_module && self.override_equals_module
    }

    fn receipt(&self, observation_only: bool) -> Value {
        json!({
            "schema_version": 1,
            "observation_only": observation_only,
            "expected_source": {"path": SOURCE_PATH, "bytes": EXPECTED_BYTES, "sha256": EXPECTED_SHA256},
            "staged_source": self.source,
            "override_script": self.loose,
            "module_resource": self.resource,
            "module_file": self.module,
            "source_equals_override": self.source_equals_override,
            "source_equals_module": self.source_equals_module,
            "override_equals_module": self.override_equals_module,
            "all_equal": self.all_equal(),
        })
    }
}

/// Successful mandatory verification, with no public constructor or Deserialize.
pub(super) struct VerifiedShadow {
    measured: Measurement,
}

impl VerifiedShadow {
    pub(super) fn receipt(&self) -> Value {
        self.measured.receipt(false)
    }
}

pub(super) fn verify_source(base: &Path, cancel: &AtomicBool) -> Result<()> {
    source_snapshot(base, cancel)?;
    Ok(())
}

fn source_snapshot(base: &Path, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let source = read_snapshot(base, SOURCE_PATH, SCRIPT_LIMIT, cancel)?;
    ensure!(
        source.len() == EXPECTED_BYTES && sha256(&source, cancel)? == EXPECTED_SHA256,
        "TOMB staged source differs from the exact original public script member"
    );
    Ok(source)
}

pub(super) fn observe(base: &Path, game: &Path, cancel: &AtomicBool) -> Result<Value> {
    Ok(measure(base, game, cancel)?.receipt(true))
}

/// Mandatory postcondition. Failure must propagate before any success receipt.
pub(super) fn verify(base: &Path, game: &Path, cancel: &AtomicBool) -> Result<VerifiedShadow> {
    let measured = measure(base, game, cancel)?;
    ensure!(
        measured.all_equal(),
        "TOMB original source, Override and module script bytes differ"
    );
    check_cancelled(cancel)?;
    Ok(VerifiedShadow { measured })
}

fn measure(base: &Path, game: &Path, cancel: &AtomicBool) -> Result<Measurement> {
    check_cancelled(cancel)?;
    let source = source_snapshot(base, cancel)?;
    let loose = read_snapshot(game, OVERRIDE_PATH, SCRIPT_LIMIT, cancel)?;
    let module_bytes = read_snapshot(game, MODULE_PATH, MODULE_LIMIT, cancel)?;
    preflight_module(&module_bytes, cancel)?;
    check_cancelled(cancel)?;
    // Parse the already bounded, no-follow snapshot. No Capsule disk reopen or
    // resource writer is involved. A logical name also keeps parser errors private.
    let module = ErfFile::parse(&module_bytes, MODULE_PATH)
        .map_err(|_| anyhow!("TOMB observer could not parse Modules/711kor.mod"))?;
    check_cancelled(cancel)?;
    ensure!(
        module.kind == ArchiveKind::Mod,
        "TOMB observer expected a MOD archive"
    );
    let ncs = extension_to_res_type("ncs");
    let mut matches = module.resources().iter().filter(|resource| {
        resource.res_type == ncs && resource.resref.eq_ignore_ascii_case(SCRIPT)
    });
    let resource = matches
        .next()
        .ok_or_else(|| anyhow!("TOMB module lacks k_oei_userdef.ncs"))?;
    ensure!(
        matches.next().is_none(),
        "TOMB module has ambiguous k_oei_userdef.ncs resources"
    );
    ensure!(
        resource.data.len() == EXPECTED_BYTES && resource.data.len() <= SCRIPT_LIMIT,
        "TOMB module script does not have the original full resource size"
    );
    check_cancelled(cancel)?;
    let source_equals_override = source == loose;
    let source_equals_module = source == resource.data;
    let override_equals_module = loose == resource.data;
    Ok(Measurement {
        source: identity(SOURCE_PATH, &source, cancel)?,
        loose: identity(OVERRIDE_PATH, &loose, cancel)?,
        resource: identity(RESOURCE_PATH, &resource.data, cancel)?,
        module: identity(MODULE_PATH, &module_bytes, cancel)?,
        source_equals_override,
        source_equals_module,
        override_equals_module,
    })
}

fn check_cancelled(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Acquire),
        "Cancelled during TOMB shadow observation"
    );
    Ok(())
}

fn identity(path: &'static str, bytes: &[u8], cancel: &AtomicBool) -> Result<FileIdentity> {
    Ok(FileIdentity {
        path,
        bytes: bytes.len(),
        sha256: sha256(bytes, cancel)?,
    })
}

fn sha256(bytes: &[u8], cancel: &AtomicBool) -> Result<String> {
    let mut hasher = Sha256::new();
    for chunk in bytes.chunks(65_536) {
        check_cancelled(cancel)?;
        hasher.update(chunk);
    }
    check_cancelled(cancel)?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn checked_metadata(path: &Path, logical: &str) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| anyhow!("Cannot inspect TOMB observer path {logical}"))?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "TOMB observer refuses a link at {logical}"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        ensure!(
            metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
            "TOMB observer refuses a reparse point at {logical}"
        );
    }
    Ok(metadata)
}

// Use the engine's OrdinalIgnoreCase path identity, rejecting every duplicate
// match rather than choosing the first directory entry. Caller owns the roots
// and holds the install lock; final-file opens additionally enforce no-follow.
fn lookup(root: &Path, logical: &str, cancel: &AtomicBool) -> Result<PathBuf> {
    let relative = crate::paths::literal_relative(logical)?;
    let policy = crate::paths::PathPolicy::new(true)?;
    let mut current = root.to_path_buf();
    ensure!(
        checked_metadata(&current, "install root")?.is_dir(),
        "TOMB observer install root is not a directory"
    );
    let parts: Vec<_> = relative.components().collect();
    for (part_index, part) in parts.iter().enumerate() {
        check_cancelled(cancel)?;
        let wanted = policy.key(Path::new(part.as_os_str()));
        let mut matched = None;
        let entries = fs::read_dir(&current)
            .map_err(|_| anyhow!("Cannot enumerate TOMB observer path {logical}"))?;
        for (index, entry) in entries.enumerate() {
            check_cancelled(cancel)?;
            ensure!(
                index < DIRECTORY_LIMIT,
                "TOMB observer directory inventory exceeds bound"
            );
            let entry =
                entry.map_err(|_| anyhow!("Cannot enumerate TOMB observer path {logical}"))?;
            if policy.key(Path::new(&entry.file_name())) == wanted {
                ensure!(
                    matched.is_none(),
                    "TOMB observer refuses case aliases at {logical}"
                );
                matched = Some(entry.path());
            }
        }
        current = matched.ok_or_else(|| anyhow!("TOMB observer path missing: {logical}"))?;
        let metadata = checked_metadata(&current, logical)?;
        if part_index + 1 < parts.len() {
            ensure!(
                metadata.is_dir(),
                "TOMB observer path parent is not a directory: {logical}"
            );
        } else {
            ensure!(
                metadata.is_file(),
                "TOMB observer path is not a regular file: {logical}"
            );
        }
    }
    Ok(current)
}

fn read_snapshot(root: &Path, logical: &str, limit: usize, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let path = lookup(root, logical, cancel)?;
    let mut file = crate::paths::open_regular_read(&path)
        .map_err(|_| anyhow!("Cannot open regular TOMB observer file {logical}"))?;
    let before = file
        .metadata()
        .map_err(|_| anyhow!("Cannot inspect open TOMB observer file {logical}"))?;
    ensure!(
        before.len() <= limit as u64,
        "TOMB observer file exceeds bound: {logical}"
    );
    let mut bytes = Vec::with_capacity(before.len() as usize);
    let mut chunk = [0u8; 65_536];
    loop {
        check_cancelled(cancel)?;
        let count = file
            .read(&mut chunk)
            .map_err(|_| anyhow!("Cannot read TOMB observer file {logical}"))?;
        if count == 0 {
            break;
        }
        ensure!(
            bytes.len().saturating_add(count) <= limit,
            "TOMB observer file grew beyond bound: {logical}"
        );
        bytes.extend_from_slice(&chunk[..count]);
    }
    let after = file
        .metadata()
        .map_err(|_| anyhow!("Cannot inspect read TOMB observer file {logical}"))?;
    ensure!(
        before.len() == bytes.len() as u64
            && before.len() == after.len()
            && before.modified().ok() == after.modified().ok(),
        "TOMB observer file changed during observation: {logical}"
    );
    Ok(bytes)
}

// The linked parser allocates every resource. Bound table arithmetic, local
// strings, and the aggregate copied payload before handing it a snapshot.
// Its reader indexes the resource list by key position, so require canonical
// resource IDs too; an indexed mismatch must never identify the wrong script.
fn preflight_module(bytes: &[u8], cancel: &AtomicBool) -> Result<()> {
    ensure!(
        bytes.len() >= 160 && &bytes[..8] == b"MOD V1.0",
        "TOMB observer expected a complete MOD V1.0 header"
    );
    let word = |at: usize| -> Result<usize> {
        let raw = bytes
            .get(at..at.saturating_add(4))
            .ok_or_else(|| anyhow!("TOMB module table is truncated"))?;
        Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize)
    };
    let strings = word(8)?;
    let count = word(16)?;
    let mut cursor = word(20)?;
    let keys = word(24)?;
    let resources = word(28)?;
    ensure!(
        count <= DIRECTORY_LIMIT && strings <= DIRECTORY_LIMIT,
        "TOMB module table exceeds entry bound"
    );
    ensure!(
        keys >= 160
            && resources >= 160
            && keys.saturating_add(count.saturating_mul(24)) <= bytes.len()
            && resources.saturating_add(count.saturating_mul(8)) <= bytes.len(),
        "TOMB module resource tables are out of bounds"
    );
    for _ in 0..strings {
        check_cancelled(cancel)?;
        let size = word(cursor.saturating_add(4))?;
        cursor = cursor.saturating_add(8).saturating_add(size);
        ensure!(
            cursor <= bytes.len(),
            "TOMB module localized strings exceed bounds"
        );
    }
    let mut total = 0usize;
    for index in 0..count {
        check_cancelled(cancel)?;
        ensure!(
            word(keys + index * 24 + 16)? == index,
            "TOMB module resource IDs differ from indexed parser semantics"
        );
        let at = resources + index * 8;
        let start = word(at)?;
        let size = word(at + 4)?;
        total = total.saturating_add(size);
        ensure!(
            start.saturating_add(size) <= bytes.len() && total <= MODULE_LIMIT,
            "TOMB module resource payload exceeds bounds"
        );
    }
    Ok(())
}
