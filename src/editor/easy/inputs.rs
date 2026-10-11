//! Revision-owned archive inputs, stored privately rather than in install roots.
use super::Session;
use crate::{Engine, Settings, editor::EditOperation};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const KEY: &str = "ModSyncAuthoringInputs";
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    sha256: String,
    bytes: u64,
}

fn checked_name(name: &str) -> Result<()> {
    let path = crate::paths::relative(name, false)?;
    ensure!(
        path.components().count() == 1 && path.to_str() == Some(name),
        "Archive input needs an ordinary filename"
    );
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    ensure!(
        matches!(
            extension.as_str(),
            "zip" | "7z" | "rar" | "tar" | "gz" | "tgz" | "exe"
        ),
        "Unsupported archive input filename"
    );
    Ok(())
}

fn verify_object(object: &Path, input: &Input, cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        crate::checkpoint::safe_path(object)?.is_some_and(|metadata| metadata.is_file()),
        "Archive input object is missing or unsafe"
    );
    let mut reader = crate::paths::open_regular_read(object)?;
    let before = reader.metadata()?;
    ensure!(before.len() == input.bytes, "Archive input size changed");
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        ensure!(
            !cancelled.load(Ordering::Relaxed),
            "Archive verification cancelled"
        );
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(count as u64)
            .context("Archive size overflow")?;
        ensure!(bytes <= input.bytes, "Archive input grew");
        hash.update(&buffer[..count]);
    }
    let after = reader.metadata()?;
    ensure!(
        bytes == input.bytes
            && after.len() == before.len()
            && after.modified()? == before.modified()?
            && format!("{:x}", hash.finalize()) == input.sha256,
        "Archive input bytes differ from the saved revision"
    );
    Ok(())
}

impl Session {
    fn archive_inputs(&self) -> Result<BTreeMap<String, Input>> {
        let inputs: BTreeMap<String, Input> = self
            .guide
            .metadata
            .get(KEY)
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()?
            .unwrap_or_default();
        ensure!(inputs.len() <= 4096, "Too many authoring inputs");
        let mut aliases = std::collections::BTreeSet::new();
        for (name, input) in &inputs {
            checked_name(name)?;
            ensure!(
                aliases.insert(crate::unicode_case::ordinal_key(name)),
                "Case-colliding authoring inputs"
            );
            ensure!(
                input.sha256.len() == 64
                    && input
                        .sha256
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
                "Invalid archive input digest"
            );
        }
        Ok(inputs)
    }

    pub(super) fn verify_applied_inputs(&self, history: &super::disk::DiskHistory) -> Result<()> {
        let current = self.archive_inputs()?;
        for applied in history.actions.iter().take(history.cursor) {
            let original = super::load_revision(&self.directory, &applied.revision)
                .context("Applied action's saved input revision is unavailable")?;
            let previous: BTreeMap<String, Input> = original
                .metadata
                .get(KEY)
                .map(|value| serde_json::from_value(value.clone()))
                .transpose()?
                .unwrap_or_default();
            for (name, input) in previous {
                ensure!(
                    current.get(&name) == Some(&input),
                    "Previously applied archive input changed or was removed: {name}; restore its filesystem boundary before applying another action"
                );
            }
        }
        Ok(())
    }

    /// Copy a real archive into private session storage and autosave its identity.
    /// Undo removes its availability; redo reuses the immutable stored bytes.
    pub fn import_archive(
        &mut self,
        source: &Path,
        max_bytes: u64,
        cancelled: &AtomicBool,
    ) -> Result<()> {
        ensure!(
            self.disk_history()?
                .is_none_or(|history| history.pending.is_none() && history.restoring.is_none()),
            "Recover the pending disk action before importing archives"
        );
        let name = source
            .file_name()
            .and_then(|value| value.to_str())
            .context("Archive filename is not UTF-8")?;
        checked_name(name)?;
        let mut inputs = self.archive_inputs()?;
        ensure!(
            !inputs
                .keys()
                .any(|existing| crate::unicode_case::ordinal_key(existing)
                    == crate::unicode_case::ordinal_key(name)),
            "Archive input already exists; undo its import before replacing it"
        );
        ensure!(inputs.len() < 4096, "Authoring input inventory is full");
        let mut reader = crate::paths::open_regular_read(source)?;
        let before = reader.metadata()?;
        ensure!(
            before.len() <= max_bytes,
            "Archive exceeds the configured import byte limit"
        );
        let directory = self.directory.join("inputs");
        if directory.exists() {
            ensure!(
                crate::checkpoint::safe_path(&directory)?.is_some_and(|metadata| metadata.is_dir()),
                "Unsafe authoring input directory"
            );
        } else {
            std::fs::create_dir(&directory)?;
        }
        ensure!(
            fs2::available_space(&directory)? > before.len().saturating_add(2 * 1024 * 1024 * 1024),
            "Not enough space to import this archive with a 2 GiB reserve"
        );
        let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
        let mut hash = Sha256::new();
        let mut bytes = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            ensure!(
                !cancelled.load(Ordering::Relaxed),
                "Archive import cancelled"
            );
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            bytes = bytes
                .checked_add(count as u64)
                .context("Archive size overflow")?;
            ensure!(
                bytes <= max_bytes && bytes <= before.len(),
                "Archive grew during import"
            );
            hash.update(&buffer[..count]);
            temporary.write_all(&buffer[..count])?;
        }
        let after = reader.metadata()?;
        ensure!(
            bytes == before.len()
                && after.len() == before.len()
                && after.modified()? == before.modified()?,
            "Archive changed during import"
        );
        let sha256 = format!("{:x}", hash.finalize());
        let input = Input { sha256, bytes };
        temporary.as_file().sync_all()?;
        let object = directory.join(&input.sha256);
        if !object.try_exists()? {
            temporary
                .persist_noclobber(&object)
                .map_err(|error| error.error)?;
            crate::checkpoint::sync_recovery_directory(&directory)?;
        } else {
            // A digest-shaped filename alone does not certify a reused object.
            // Refuse before recording another revision if its bytes changed.
            verify_object(&object, &input, cancelled)?;
        }
        ensure!(
            !cancelled.load(Ordering::Relaxed),
            "Archive import cancelled before autosave"
        );
        inputs.insert(name.to_owned(), input);
        self.edit(
            &format!("Import archive · {name}"),
            &[EditOperation::SetGuideMetadata {
                key: KEY.into(),
                value: serde_json::to_value(inputs)?,
            }],
        )?;
        Ok(())
    }

    pub(super) fn attach_archive_inputs(
        &self,
        engine: Engine,
        settings: &Settings,
    ) -> Result<Engine> {
        self.attach_remaining_archive_inputs(engine, settings, &[])
    }

    pub(super) fn consumed_archive_inputs(
        &self,
        history: &super::disk::DiskHistory,
    ) -> Result<Vec<String>> {
        let mut consumed = Vec::new();
        for action in history.actions.iter().take(history.cursor) {
            if let Some(plan) = &action.consumed_inputs {
                for name in plan {
                    ensure!(
                        !consumed.contains(name),
                        "Filesystem history consumes an archive input twice"
                    );
                    consumed.push(name.clone());
                }
            } else {
                let original = super::load_revision(&self.directory, &action.revision)?;
                let inputs: BTreeMap<String, Input> = original
                    .metadata
                    .get(KEY)
                    .map(|value| serde_json::from_value(value.clone()))
                    .transpose()?
                    .unwrap_or_default();
                ensure!(
                    inputs.is_empty(),
                    "Legacy filesystem history lacks an archive consumption plan; undo to its initial boundary before applying imported inputs"
                );
            }
        }
        Ok(consumed)
    }

    pub(super) fn attach_remaining_archive_inputs(
        &self,
        engine: Engine,
        settings: &Settings,
        consumed: &[String],
    ) -> Result<Engine> {
        let inputs = self.archive_inputs()?;
        let names: std::collections::BTreeSet<_> = consumed.iter().collect();
        ensure!(
            names.len() == consumed.len() && names.iter().all(|name| inputs.contains_key(*name)),
            "Filesystem history has invalid consumed archive names"
        );
        let mut files = Vec::new();
        for (name, input) in inputs {
            if names.contains(&name) {
                continue;
            }
            let object: PathBuf = self.directory.join("inputs").join(&input.sha256);
            verify_object(&object, &input, &settings.cancelled)?;
            files.push((settings.roots.mods.join(name), object));
        }
        engine.with_authoring_inputs(files)
    }
}
