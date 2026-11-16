//! Import a mod archive that is not part of a build.
//!
//! The archive is placed in the mod folder (never overwriting a different file
//! of the same name), its contents are inspected with the archive generator's
//! read-only inference (`archive_generation::generate_from_archive`), and the
//! result is a one-mod guide that the normal engine validates and installs. A
//! folder import lists the archives directly inside it, each imported on its own.
use crate::{Component, Guide, archive};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

/// Archive types an import accepts, as the archive reader opens them.
pub const ARCHIVE_EXTENSIONS: [&str; 3] = ["zip", "7z", "rar"];
/// Archives listed from one folder.
const MAX_FOLDER_ARCHIVES: usize = 10_000;

/// How an imported mod installs, from its inferred instructions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Method {
    /// A TSLPatcher-style installer (`tslpatchdata`), applied by the patcher.
    Tslpatcher,
    /// Game files copied into the game folder.
    DirectCopy,
}

/// One archive ready to install.
#[derive(Debug)]
pub struct Prepared {
    /// A guide with one selected mod that installs the archive.
    pub guide: Guide,
    /// The archive in the mod folder that the guide extracts.
    pub archive: PathBuf,
    /// True when the archive was copied into the mod folder by this import.
    pub copied: bool,
    pub sha256: String,
    pub method: Method,
}

fn cancelled_error(cancelled: &AtomicBool) -> Result<()> {
    ensure!(!cancelled.load(Ordering::Acquire), "Import cancelled");
    Ok(())
}

/// True for a file name with an accepted archive extension.
pub fn is_archive(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ARCHIVE_EXTENSIONS.iter().any(|a| e.eq_ignore_ascii_case(a)))
}

/// The archives directly inside `folder`, by name. Subfolders are not searched.
pub fn archives_in(folder: &Path, cancelled: &AtomicBool) -> Result<Vec<PathBuf>> {
    ensure!(folder.is_dir(), "Not a folder: {}", folder.display());
    let mut found = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        cancelled_error(cancelled)?;
        let entry = entry?;
        if entry.file_type()?.is_file() && is_archive(&entry.path()) {
            ensure!(
                found.len() < MAX_FOLDER_ARCHIVES,
                "The folder holds more than {MAX_FOLDER_ARCHIVES} archives"
            );
            found.push(entry.path());
        }
    }
    found.sort_by_key(|path| path.file_name().map(|n| n.to_ascii_lowercase()));
    Ok(found)
}

fn sha256_of(path: &Path, cancelled: &AtomicBool) -> Result<String> {
    let mut file = crate::paths::open_regular_read(path)?;
    let mut hash = Sha256::new();
    let mut block = vec![0u8; 64 * 1024];
    loop {
        cancelled_error(cancelled)?;
        let count = file.read(&mut block)?;
        if count == 0 {
            break;
        }
        hash.update(&block[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Put `archive` in the mod folder under its own name. An archive already there
/// is used as is. A file of the same name with other contents is never replaced.
/// The copy is staged beside its destination, checked against the source's
/// SHA-256, and published without overwriting. Returns (path, copied, sha256).
pub fn place_archive(
    archive: &Path,
    mods: &Path,
    cancelled: &AtomicBool,
) -> Result<(PathBuf, bool, String)> {
    cancelled_error(cancelled)?;
    ensure!(
        is_archive(archive),
        "{} is not a ZIP, 7z or RAR archive",
        archive.display()
    );
    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .context("The archive name is not valid text")?;
    let name = crate::download::one_component(name)?;
    ensure!(
        !name.contains(['*', '?']),
        "The archive name {name} contains * or ?; rename it first"
    );
    let source = std::fs::canonicalize(archive)
        .with_context(|| format!("The archive {} cannot be found", archive.display()))?;
    let sha256 = sha256_of(&source, cancelled)?;
    let lock = crate::recovery::ModRootLock::acquire(mods)?;
    let destination = lock.root().join(name);
    // As the caller named the folder (no verbatim prefix), for showing and guides.
    let shown = std::path::absolute(mods)?.join(name);
    lock.contains_destination(&destination)?;
    match std::fs::symlink_metadata(&destination) {
        Ok(meta) => {
            ensure!(
                meta.is_file(),
                "{} in the mod folder is not an ordinary file",
                name
            );
            if std::fs::canonicalize(&destination)? == source {
                return Ok((shown, false, sha256));
            }
            ensure!(
                sha256_of(&destination, cancelled)? == sha256,
                "A different file named {name} is already in the mod folder; rename one of them and import again"
            );
            return Ok((shown, false, sha256));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let mut reader = crate::paths::open_regular_read(&source)?;
    let mut staged = tempfile::NamedTempFile::new_in(lock.root())?;
    let mut hash = Sha256::new();
    let mut block = vec![0u8; 64 * 1024];
    loop {
        cancelled_error(cancelled)?;
        let count = reader.read(&mut block)?;
        if count == 0 {
            break;
        }
        hash.update(&block[..count]);
        staged.write_all(&block[..count])?;
    }
    staged.as_file().sync_all()?;
    ensure!(
        format!("{:x}", hash.finalize()) == sha256,
        "{name} changed while it was being copied; import it again"
    );
    cancelled_error(cancelled)?;
    lock.contains_destination(&destination)?;
    staged.persist_noclobber(&destination).map_err(|error| {
        anyhow::Error::new(error.error).context(format!(
            "{name} appeared in the mod folder during the import; import it again"
        ))
    })?;
    Ok((shown, true, sha256))
}

fn method_of(component: &Component) -> Method {
    let patcher = component
        .instructions
        .iter()
        .chain(component.options.iter().flat_map(|o| &o.instructions))
        .any(|i| i.action == "Patcher");
    if patcher {
        Method::Tslpatcher
    } else {
        Method::DirectCopy
    }
}

/// Place `archive` in the mod folder and describe how to install it. Returns
/// None when the archive holds nothing this app can install by itself (no game
/// files and no TSLPatcher data): it needs a manual install. When the mod offers
/// several install options, the first is chosen, as the patcher would default.
pub fn prepare(
    archive: &Path,
    mods: &Path,
    name: Option<&str>,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Option<Prepared>> {
    let (placed, copied, sha256) = place_archive(archive, mods, cancelled)?;
    let stem = placed
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported mod")
        .to_owned();
    let name = name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map_or(stem, str::to_owned);
    let mut component = Component {
        guid: uuid::Uuid::new_v4().to_string(),
        name: name.clone(),
        author: String::new(),
        description: format!(
            "Imported from {}",
            std::path::absolute(archive)
                .unwrap_or_else(|_| archive.to_path_buf())
                .display()
        ),
        is_selected: true,
        ..Component::default()
    };
    let generated = crate::archive_generation::generate_from_archive(
        &mut component,
        &placed,
        options,
        cancelled,
    );
    let generated = match generated {
        Ok(generated) => generated,
        Err(error) => {
            if copied {
                let _ = std::fs::remove_file(&placed);
            }
            return Err(error.context(format!("{name} could not be read")));
        }
    };
    if !generated || component.instructions.is_empty() {
        if copied {
            let _ = std::fs::remove_file(&placed);
        }
        return Ok(None);
    }
    if !component.options.is_empty() && !component.options.iter().any(|o| o.is_selected) {
        component.options[0].is_selected = true;
    }
    let method = method_of(&component);
    // Read the guide back through the loader so it has exactly the shape of an
    // instruction file opened from disk.
    let draft = Guide {
        components: vec![component],
        ..Guide::default()
    };
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("import.toml");
    draft.save(&path)?;
    let guide = Guide::load_for_install_preserving_selection(&path)?;
    ensure!(
        guide.components.len() == 1 && guide.components[0].is_selected,
        "The imported mod could not be prepared"
    );
    Ok(Some(Prepared {
        guide,
        archive: placed,
        copied,
        sha256,
        method,
    }))
}
