//! Observed external-backend input identity, never native recipe authority.
//!
//! These receipts bind bytes handed to a real process. They do not establish
//! original-guide provenance, predict effects, or authorize checkpoint reuse.
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::Path,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Member {
    directory: bool,
    bytes: u64,
    sha256: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PackageIdentity {
    convention: &'static str,
    manifest_sha256: String,
    members: BTreeMap<String, Member>,
}

impl PackageIdentity {
    pub(super) fn capture(root: &Path, cancelled: &AtomicBool) -> Result<Self> {
        ensure!(
            std::fs::symlink_metadata(root)?.is_dir(),
            "Backend input root must be an ordinary directory"
        );
        let mut members = BTreeMap::new();
        let mut total = 0u64;
        for entry in walkdir::WalkDir::new(root).follow_links(false).min_depth(1) {
            ensure!(
                !cancelled.load(Ordering::Acquire),
                "Backend identity cancelled"
            );
            ensure!(
                members.len() < 100_000,
                "Backend identity exceeds entry bound"
            );
            let entry = entry?;
            let metadata = std::fs::symlink_metadata(entry.path())?;
            ensure!(
                metadata.is_dir() || metadata.is_file(),
                "Backend identity refuses links and special files"
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "Backend identity refuses reparse points"
                );
            }
            let relative = entry.path().strip_prefix(root)?;
            let name = relative
                .to_str()
                .context("Backend input path is not UTF-8")?
                .replace(std::path::MAIN_SEPARATOR, "/");
            let mut bytes = 0u64;
            let sha256 = if metadata.is_file() {
                let mut input = crate::paths::open_regular_read(entry.path())?;
                let mut digest = Sha256::new();
                let mut buffer = [0u8; 65536];
                loop {
                    ensure!(
                        !cancelled.load(Ordering::Acquire),
                        "Backend identity cancelled"
                    );
                    let count = input.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    total = total
                        .checked_add(count as u64)
                        .context("Backend identity size overflow")?;
                    ensure!(
                        total <= 64 * 1024 * 1024 * 1024,
                        "Backend identity exceeds 64 GiB"
                    );
                    bytes += count as u64;
                    digest.update(&buffer[..count]);
                }
                ensure!(
                    bytes == metadata.len(),
                    "Backend input changed during hashing"
                );
                Some(format!("{:x}", digest.finalize()))
            } else {
                None
            };
            ensure!(
                members
                    .insert(
                        name,
                        Member {
                            directory: metadata.is_dir(),
                            bytes,
                            sha256
                        }
                    )
                    .is_none(),
                "Duplicate backend input path"
            );
        }
        let manifest_sha256 = format!("{:x}", Sha256::digest(serde_json::to_vec(&members)?));
        Ok(Self {
            convention: "sorted-relative-path-directory-bytes-sha256-json-v1",
            manifest_sha256,
            members,
        })
    }
}

/// Record only explicit environment changes, not inherited secrets. OsString
/// serialization is refused if lossy conversion would hide the executed argv.
pub(super) fn launch_identity(command: &Command) -> Result<serde_json::Value> {
    let text = |value: &std::ffi::OsStr| -> Result<String> {
        Ok(value
            .to_str()
            .context("Backend launch contains non-UTF8 text")?
            .to_owned())
    };
    let environment = command
        .get_envs()
        .map(|(key, value)| Ok((text(key)?, value.map(text).transpose()?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    Ok(serde_json::json!({
        "program": text(command.get_program())?,
        "arguments": command.get_args().map(text).collect::<Result<Vec<_>>>()?,
        "workingDirectory": command.get_current_dir(),
        "environmentOverrides": environment,
        "inheritedEnvironmentCaptured": false
    }))
}

pub(super) fn file_identity(path: &Path, cancelled: &AtomicBool) -> Result<serde_json::Value> {
    let mut file = crate::paths::open_regular_read(path)?;
    let mut digest = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "Backend identity cancelled"
        );
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        ensure!(
            bytes <= 512 * 1024 * 1024,
            "Backend executable identity exceeds 512 MiB"
        );
        digest.update(&buffer[..count]);
    }
    Ok(
        serde_json::json!({"path": path, "bytes": bytes, "sha256": format!("{:x}", digest.finalize())}),
    )
}

pub(super) fn invocation_identity(
    invocation: &crate::patcher::Invocation,
    cancelled: &AtomicBool,
) -> Result<serde_json::Value> {
    if let crate::patcher::Invocation::Python {
        interpreter,
        entrypoint,
        module_roots,
    } = invocation
    {
        let modules = module_roots.iter().map(|root| -> Result<serde_json::Value> {
            Ok(serde_json::json!({"root": root, "package": PackageIdentity::capture(root, cancelled)?}))
        }).collect::<Result<Vec<_>>>()?;
        let interpreter_target = std::fs::canonicalize(interpreter)?;
        Ok(
            serde_json::json!({"kind": "python", "interpreter": file_identity(&interpreter_target, cancelled)?,
            "interpreterInvocationPath": interpreter,
            "entrypoint": file_identity(entrypoint, cancelled)?, "modules": modules,
            "systemDependencyClosureVerified": false}),
        )
    } else {
        Ok(
            serde_json::json!({"kind": "executable", "program": file_identity(invocation.native_path().context("Backend executable missing")?, cancelled)?,
            "systemDependencyClosureVerified": false}),
        )
    }
}

impl PackageIdentity {
    /// Retain every new or changed regular file, not only recognized install logs.
    /// Called after writer quiescence, before temporary staging is discarded.
    pub(super) fn retain_changes(
        &self,
        root: &Path,
        destination: &Path,
    ) -> Result<serde_json::Value> {
        // Evidence retention must survive an application cancellation request.
        let cancelled = AtomicBool::new(false);
        let after = Self::capture(root, &cancelled)?;
        std::fs::create_dir(destination)?;
        let mut retained = Vec::new();
        for (name, member) in &after.members {
            if member.directory || self.members.get(name) == Some(member) {
                continue;
            }
            let relative = crate::paths::literal_relative(name)?;
            let target = destination.join(&relative);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            crate::recovery::atomic_copy(
                &target,
                crate::paths::open_regular_read(&root.join(&relative))?,
                &cancelled,
            )?;
            let copied = file_identity(&target, &cancelled)?;
            ensure!(
                copied["sha256"].as_str() == member.sha256.as_deref()
                    && copied["bytes"].as_u64() == Some(member.bytes),
                "Retained backend artifact differs from settled output"
            );
            retained.push(name);
        }
        Ok(
            serde_json::json!({"directory": destination, "retainedFiles": retained, "settledPackage": after}),
        )
    }
}
