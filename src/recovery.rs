use crate::paths::Roots;
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Serialize, Deserialize)]
struct Snapshot {
    mods: PathBuf,
    game: PathBuf,
    files: BTreeMap<String, String>,
    #[serde(default)]
    completion: Option<serde_json::Value>,
}

pub fn state_dir(roots: &Roots) -> PathBuf {
    let key = format!(
        "{:x}",
        Sha256::digest(roots.game.to_string_lossy().as_bytes())
    );
    roots
        .game
        .parent()
        .unwrap()
        .join(format!(".modsync-{}", &key[..16]))
}

pub struct Lock {
    files: Vec<File>,
    roots: Roots,
    checkpoint_context: Option<Vec<u8>>,
}
impl Drop for Lock {
    fn drop(&mut self) {
        for file in &self.files {
            let _ = FileExt::unlock(file);
        }
    }
}
impl Lock {
    pub(crate) fn guards(&self, roots: &Roots) -> bool {
        self.roots.mods == roots.mods && self.roots.game == roots.game && self.files.len() == 2
    }
    #[cfg(windows)]
    pub(crate) fn guards_checkpoint_context(&self, proof: &[u8]) -> bool {
        self.checkpoint_context.as_deref() == Some(proof)
    }
    pub(crate) fn acquire_checkpoint_context(roots: &Roots, proof: Vec<u8>) -> Result<Self> {
        let mut lock = Self::acquire(roots)?;
        lock.checkpoint_context = Some(proof);
        Ok(lock)
    }
    pub fn acquire(roots: &Roots) -> Result<Self> {
        crate::maintenance::ensure_clear_game(&roots.game)?;
        let lock = Self::acquire_impl(roots, false)?;
        crate::maintenance::ensure_clear_game(&roots.game)?;
        Ok(lock)
    }
    pub(crate) fn acquire_maintenance(roots: &Roots) -> Result<Self> {
        ensure_checkpoint_clean(roots)?;
        Self::acquire_impl(roots, true)
    }
    fn acquire_impl(roots: &Roots, maintenance: bool) -> Result<Self> {
        if maintenance {
            crate::checkpoint::prepare_existing_for_maintenance(roots)?;
        } else {
            crate::checkpoint::prepare_existing(roots)?;
        }
        let dir = state_dir(roots);
        ensure!(
            !dir.starts_with(&roots.mods) && !roots.mods.starts_with(&dir),
            "Recovery directory overlaps mod directory"
        );
        fs::create_dir_all(&dir)?;
        let directory_metadata = fs::symlink_metadata(&dir)?;
        ensure!(
            directory_metadata.is_dir() && !directory_metadata.file_type().is_symlink(),
            "Recovery directory is not an ordinary directory"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                directory_metadata.file_attributes()
                    & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                    == 0,
                "Recovery directory is a reparse point"
            );
        }
        let mut lock = Self {
            files: Vec::new(),
            roots: roots.clone(),
            checkpoint_context: None,
        };
        let mut roots = [&roots.game, &roots.mods];
        roots.sort();
        for root in roots {
            let hash = format!("{:x}", Sha256::digest(root.to_string_lossy().as_bytes()));
            let path = root
                .parent()
                .unwrap()
                .join(format!(".modsync-lock-{}", &hash[..16]));
            match fs::symlink_metadata(&path) {
                Ok(meta) => ensure!(
                    meta.is_file() && !meta.file_type().is_symlink(),
                    "Lock path is not a regular file"
                ),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            let mut options = OpenOptions::new();
            options.create(true).truncate(false).read(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.custom_flags(
                    windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
                );
            }
            let file = options.open(path)?;
            let metadata = file.metadata()?;
            ensure!(metadata.is_file(), "Lock path is not a regular file");
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes()
                        & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                        == 0,
                    "Recovery lock handle is a reparse point"
                );
            }
            file.try_lock_exclusive()
                .context("Another ModSync process is using the game or mod directory")?;
            lock.files.push(file);
        }
        Ok(lock)
    }
}

fn hash(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    io::copy(&mut file, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

pub(crate) fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in WalkDir::new(from).min_depth(1) {
        let entry = entry?;
        ensure!(
            !entry.file_type().is_symlink(),
            "Snapshot refuses symlinks: {}",
            entry.path().display()
        );
        let target = to.join(entry.path().strip_prefix(from)?);
        if entry.file_type().is_dir() {
            fs::create_dir_all(target)?;
        } else {
            ensure!(
                entry.file_type().is_file(),
                "Snapshot refuses special files"
            );
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn tree_manifest(dir: &Path) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for entry in WalkDir::new(dir).min_depth(1) {
        let entry = entry?;
        ensure!(!entry.file_type().is_symlink(), "Manifest refuses symlinks");
        if entry.file_type().is_file() {
            result.insert(
                entry
                    .path()
                    .strip_prefix(dir)?
                    .to_string_lossy()
                    .into_owned(),
                hash(entry.path())?,
            );
        } else {
            ensure!(entry.file_type().is_dir(), "Manifest refuses special files");
        }
    }
    Ok(result)
}

fn manifest(dir: &Path) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for root in ["mods", "game"] {
        for (path, hash) in tree_manifest(&dir.join(root))? {
            result.insert(
                Path::new(root).join(path).to_string_lossy().into_owned(),
                hash,
            );
        }
    }
    Ok(result)
}

#[derive(Serialize, Deserialize)]
struct Completion {
    mods: PathBuf,
    game: PathBuf,
    components: Vec<serde_json::Value>,
    files: BTreeMap<String, String>,
}

fn read_completion(roots: &Roots) -> Result<Option<serde_json::Value>> {
    let path = state_dir(roots).join("completion.json");
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
}

/// Serialize the existing completion schema from an already verified checkpoint.
/// This does not write a receipt or establish authority by itself.
pub(crate) fn completion_value_from_manifest(
    roots: &Roots,
    components: &[serde_json::Value],
    files: &BTreeMap<String, String>,
) -> Result<serde_json::Value> {
    Ok(serde_json::to_value(Completion {
        mods: roots.mods.clone(),
        game: roots.game.clone(),
        components: components.to_vec(),
        files: files.clone(),
    })?)
}

pub(crate) fn verify_completion_manifest(
    roots: &Roots,
    components: &[serde_json::Value],
    files: &BTreeMap<String, String>,
) -> Result<()> {
    use std::io::Read;
    let file = crate::paths::open_regular_read(&state_dir(roots).join("completion.json"))?;
    ensure!(
        file.metadata()?.len() <= 64 * 1024 * 1024,
        "Completion receipt exceeds bound"
    );
    let mut bytes = Vec::new();
    file.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 64 * 1024 * 1024,
        "Completion receipt exceeds bound"
    );
    let receipt: Completion = serde_json::from_slice(&bytes)?;
    ensure!(
        receipt.mods == roots.mods
            && receipt.game == roots.game
            && receipt.components == components
            && receipt.files == *files,
        "Inherited checkpoint prefix differs from verified legacy completion receipt"
    );
    Ok(())
}

pub(crate) fn verify_checkpoint_legacy_snapshot(
    roots: &Roots,
    mods: &BTreeMap<String, String>,
    game: &BTreeMap<String, String>,
) -> Result<()> {
    use std::io::Read;
    let file = crate::paths::open_regular_read(&state_dir(roots).join("snapshot.json"))?;
    ensure!(
        file.metadata()?.len() <= 64 * 1024 * 1024,
        "Legacy snapshot metadata exceeds bound"
    );
    let mut bytes = Vec::new();
    file.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 64 * 1024 * 1024,
        "Legacy snapshot metadata exceeds bound"
    );
    let snapshot: Snapshot = serde_json::from_slice(&bytes)?;
    let mut expected = BTreeMap::new();
    for (root, files) in [("mods", mods), ("game", game)] {
        for (path, hash) in files {
            expected.insert(
                Path::new(root).join(path).to_string_lossy().into_owned(),
                hash.clone(),
            );
        }
    }
    ensure!(
        snapshot.mods == roots.mods && snapshot.game == roots.game && snapshot.files == expected,
        "Legacy recovery snapshot differs from checkpoint session baseline"
    );
    Ok(())
}

pub(crate) fn record_completion(roots: &Roots, guide: &crate::Guide) -> Result<()> {
    let receipt = Completion {
        mods: roots.mods.clone(),
        game: roots.game.clone(),
        components: guide
            .ordered()?
            .into_iter()
            .map(serde_json::to_value)
            .collect::<Result<_, _>>()?,
        files: tree_manifest(&roots.game)?,
    };
    atomic_write(
        &state_dir(roots).join("completion.json"),
        &serde_json::to_vec_pretty(&receipt)?,
    )
}

pub(crate) fn verify_completion(roots: &Roots, guide: &crate::Guide, count: usize) -> Result<()> {
    if count == 0 {
        return Ok(());
    }
    let receipt: Completion = serde_json::from_value(
        read_completion(roots)?.context("No saved completion receipt for the completed prefix")?,
    )?;
    ensure!(
        receipt.mods == roots.mods && receipt.game == roots.game,
        "Completion roots differ"
    );
    let expected: Vec<_> = guide
        .ordered()?
        .into_iter()
        .take(count)
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()?;
    ensure!(
        receipt.components == expected,
        "Completed component definitions differ from receipt"
    );
    ensure!(
        receipt.files == tree_manifest(&roots.game)?,
        "Completed game files differ from receipt"
    );
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletedComponent {
    pub guid: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameFileDelta {
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionSummary {
    /// Every component in completion.json, in recorded order (base prefix + suffix).
    pub components: Vec<CompletedComponent>,
    /// Game files in the receipt manifest.
    pub game_files: usize,
    /// Receipt game manifest vs the retained snapshot.json game/ manifest of the
    /// run that wrote this receipt. None when the snapshot is absent or from other
    /// roots, when pending recovery exists, or when snapshot.completion equals the
    /// current receipt (the receipt predates that snapshot's run, e.g. after restore).
    pub game_delta_since_run_start: Option<GameFileDelta>,
}

/// Read-only summary of the saved completion receipt. The lock file is the only
/// write; nothing is hashed, so current game bytes are not checked here.
pub fn completion_summary(roots: &Roots) -> Result<Option<CompletionSummary>> {
    // Lock::acquire creates state and lock files; a query without a receipt must not.
    if !state_dir(roots).join("completion.json").exists() {
        return Ok(None);
    }
    let _lock = Lock::acquire(roots)?;
    let Some(value) = read_completion(roots)? else {
        return Ok(None);
    };
    let receipt: Completion = serde_json::from_value(value.clone())?;
    ensure!(
        receipt.mods == roots.mods && receipt.game == roots.game,
        "Completion roots differ"
    );
    let components = receipt
        .components
        .iter()
        .map(|component| {
            let field = |key: &str| {
                component
                    .get(key)
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .with_context(|| format!("Receipt component has no {key}"))
            };
            Ok(CompletedComponent {
                guid: field("Guid")?,
                name: field("Name")?,
            })
        })
        .collect::<Result<_>>()?;
    let state = state_dir(roots);
    let snapshot_path = state.join("snapshot.json");
    let game_delta_since_run_start = if state.join("pending").exists() || !snapshot_path.exists() {
        None
    } else {
        let saved: Snapshot = serde_json::from_slice(&fs::read(snapshot_path)?)?;
        if saved.mods != roots.mods
            || saved.game != roots.game
            || saved.completion.as_ref() == Some(&value)
        {
            None
        } else {
            let before: BTreeMap<String, &String> = saved
                .files
                .iter()
                .filter_map(|(file, hash)| {
                    Path::new(file)
                        .strip_prefix("game")
                        .ok()
                        .map(|rel| (rel.to_string_lossy().into_owned(), hash))
                })
                .collect();
            let mut delta = GameFileDelta {
                added: 0,
                changed: 0,
                removed: 0,
            };
            for (file, hash) in &receipt.files {
                match before.get(file) {
                    None => delta.added += 1,
                    Some(old) if *old != hash => delta.changed += 1,
                    Some(_) => {}
                }
            }
            delta.removed = before
                .keys()
                .filter(|file| !receipt.files.contains_key(*file))
                .count();
            Some(delta)
        }
    };
    Ok(Some(CompletionSummary {
        components,
        game_files: receipt.files.len(),
        game_delta_since_run_start,
    }))
}

pub(crate) fn ensure_checkpoint_clean(roots: &Roots) -> Result<()> {
    crate::checkpoint::ensure_external_process_clear(&roots.game)?;
    match fs::symlink_metadata(state_dir(roots).join("checkpoints-v1")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let store = crate::checkpoint::Store::open(roots)?;
    ensure!(
        store.pending_operations()?.is_empty(),
        "An unfinished checkpoint operation exists; legacy install/restore is fenced until checkpoint recovery"
    );
    store.ensure_no_unfinished_install_sessions()?;
    Ok(())
}

pub fn ensure_clean(roots: &Roots) -> Result<()> {
    crate::maintenance::ensure_clear_game(&roots.game)?;
    ensure_checkpoint_clean(roots)?;
    ensure!(
        !state_dir(roots).join("pending").exists(),
        "An unfinished install exists. Run restore before retrying. Recovery: {}",
        state_dir(roots).display()
    );
    Ok(())
}

pub fn begin(roots: &Roots) -> Result<()> {
    ensure_clean(roots)?;
    let state = state_dir(roots);
    let snapshot = state.join("snapshot");
    if snapshot.exists() {
        fs::remove_dir_all(&snapshot)?;
    }
    // ponytail: full copies cost two tree sizes; use reflink snapshots when install size warrants it.
    copy_tree(&roots.mods, &snapshot.join("mods"))?;
    copy_tree(&roots.game, &snapshot.join("game"))?;
    let data = Snapshot {
        mods: roots.mods.clone(),
        game: roots.game.clone(),
        files: manifest(&snapshot)?,
        completion: read_completion(roots)?,
    };
    atomic_write(
        &state.join("snapshot.json"),
        &serde_json::to_vec_pretty(&data)?,
    )?;
    atomic_write(
        &state.join("pending"),
        b"Restore the verified snapshot before retrying this install.\n",
    )?;
    Ok(())
}

/// The checkpoint transition intent is the recovery fence for rebuilding the
/// legacy compatibility snapshot. Standalone callers cannot waive that intent.
pub(crate) fn rebase_checkpoint_snapshot(
    roots: &Roots,
    lock: &Lock,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<()> {
    use std::io::Read;
    let check_cancelled = || -> Result<()> {
        ensure!(
            !cancelled.load(std::sync::atomic::Ordering::Relaxed),
            "Checkpoint snapshot rebase cancelled"
        );
        Ok(())
    };
    check_cancelled()?;
    ensure!(
        lock.guards(roots),
        "Checkpoint snapshot rebase needs matching root locks"
    );
    let store = crate::checkpoint::Store::open(roots)?;
    store.prepare_write()?;
    let operations = store.pending_operations()?;
    ensure!(
        operations.len() == 1
            && matches!(
                operations[0].kind.as_str(),
                "Restore" | "Resume" | "SourceRebase"
            ),
        "Checkpoint snapshot rebase requires a durable transition intent"
    );
    let state = state_dir(roots);
    let snapshot = state.join("snapshot");
    let snapshot_exists = match fs::symlink_metadata(&snapshot) {
        Ok(metadata) => {
            ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "Legacy snapshot root is unsafe"
            );
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    if snapshot_exists {
        let mut old_paths = Vec::new();
        for entry in WalkDir::new(&snapshot).follow_links(false) {
            check_cancelled()?;
            ensure!(
                old_paths.len() <= 200_002,
                "Legacy snapshot inventory exceeds bound"
            );
            let entry = entry?;
            crate::checkpoint::persistence::qualify(entry.path())?;
            ensure!(
                !entry.file_type().is_symlink()
                    && (entry.file_type().is_file() || entry.file_type().is_dir()),
                "Unsafe legacy snapshot artifact"
            );
            #[cfg(unix)]
            if entry.file_type().is_dir() {
                use std::os::unix::fs::PermissionsExt;
                let metadata = fs::metadata(entry.path())?;
                fs::set_permissions(
                    entry.path(),
                    fs::Permissions::from_mode(metadata.permissions().mode() | 0o700),
                )?;
            }
            old_paths.push(entry.into_path());
        }
        old_paths.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        for path in old_paths {
            check_cancelled()?;
            if fs::symlink_metadata(&path)?.is_dir() {
                fs::remove_dir(path)?;
            } else {
                crate::checkpoint::persistence::writable_file(&path)?;
                fs::remove_file(path)?;
            }
        }
    }
    for (root, name) in [(&roots.mods, "mods"), (&roots.game, "game")] {
        let destination = snapshot.join(name);
        fs::create_dir_all(&destination)?;
        for (count, entry) in WalkDir::new(root)
            .min_depth(1)
            .follow_links(false)
            .into_iter()
            .enumerate()
        {
            check_cancelled()?;
            ensure!(count < 100_000, "Checkpoint snapshot source exceeds bound");
            let entry = entry?;
            crate::checkpoint::persistence::qualify(entry.path())?;
            ensure!(
                !entry.file_type().is_symlink()
                    && (entry.file_type().is_file() || entry.file_type().is_dir()),
                "Unsafe checkpoint snapshot source"
            );
            let target = destination.join(entry.path().strip_prefix(root)?);
            if entry.file_type().is_dir() {
                fs::create_dir_all(&target)?;
            } else {
                let file = crate::paths::open_regular_read(entry.path())?;
                let metadata = file.metadata()?;
                checkpoint_atomic_copy(
                    &target,
                    file.take(metadata.len().saturating_add(1)),
                    cancelled,
                )?;
                ensure!(
                    fs::metadata(&target)?.len() == metadata.len(),
                    "Snapshot source changed while copying"
                );
                fs::set_permissions(&target, metadata.permissions())?;
            }
        }
    }
    let mut directories = Vec::new();
    for entry in WalkDir::new(&snapshot).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_dir() {
            directories.push(entry.into_path());
        } else {
            #[cfg(unix)]
            crate::paths::open_regular_read(entry.path())?.sync_all()?;
        }
    }
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        crate::checkpoint::sync_recovery_directory(&directory)?;
    }
    let data = Snapshot {
        mods: roots.mods.clone(),
        game: roots.game.clone(),
        files: manifest(&snapshot)?,
        completion: read_completion(roots)?,
    };
    checkpoint_atomic_write(
        &state.join("snapshot.json"),
        &serde_json::to_vec_pretty(&data)?,
    )?;
    checkpoint_atomic_write(
        &state.join("pending"),
        b"Checkpoint session active; use checkpoint recover before retrying.\n",
    )?;
    crate::checkpoint::sync_recovery_directory(&state)
}

fn checkpoint_atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    checkpoint_atomic_copy(path, data, &std::sync::atomic::AtomicBool::new(false))
}
fn checkpoint_atomic_copy(
    path: &Path,
    mut reader: impl io::Read,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<()> {
    use io::Write;
    let parent = path.parent().context("Checkpoint copy requires parent")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    let mut buffer = [0u8; 65536];
    loop {
        ensure!(
            !cancelled.load(std::sync::atomic::Ordering::Relaxed),
            "Checkpoint copy cancelled"
        );
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        temporary.write_all(&buffer[..n])?;
    }
    crate::checkpoint::persistence::publish_temp(temporary, path, true)?;
    crate::checkpoint::sync_recovery_directory(parent)
}

pub fn finish(roots: &Roots) -> Result<()> {
    fs::remove_file(state_dir(roots).join("pending"))?;
    Ok(())
}

pub fn restore(roots: &Roots) -> Result<()> {
    let _lock = Lock::acquire(roots)?;
    ensure_checkpoint_clean(roots)?;
    let state = state_dir(roots);
    ensure!(
        state.join("pending").exists(),
        "There is no unfinished install to restore"
    );
    let saved: Snapshot = serde_json::from_slice(&fs::read(state.join("snapshot.json"))?)?;
    ensure!(
        saved.mods == roots.mods && saved.game == roots.game,
        "Recovery roots do not match this install"
    );
    let snapshot = state.join("snapshot");
    ensure!(
        manifest(&snapshot)? == saved.files,
        "Snapshot hashes differ; refusing to restore"
    );
    for (name, target) in [("mods", &roots.mods), ("game", &roots.game)] {
        // Keep the root directory inode so another process cannot acquire a different lock.
        for entry in fs::read_dir(target)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                fs::remove_dir_all(entry.path())?;
            } else {
                fs::remove_file(entry.path())?;
            }
        }
        copy_tree(&snapshot.join(name), target)?;
        for (file, expected) in &saved.files {
            if let Ok(rel) = Path::new(file).strip_prefix(name) {
                ensure!(
                    hash(&target.join(rel))? == *expected,
                    "Restored hash mismatch: {file}"
                );
            }
        }
    }
    let completion_path = state.join("completion.json");
    if let Some(completion) = saved.completion {
        atomic_write(&completion_path, &serde_json::to_vec_pretty(&completion)?)?;
    } else if completion_path.exists() {
        fs::remove_file(completion_path)?;
    }
    finish(roots)
}

pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    use std::io::Write;
    fs::create_dir_all(path.parent().context("Path needs a parent")?)?;
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    temp.write_all(data)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

pub fn atomic_copy(
    path: &Path,
    mut reader: impl std::io::Read,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<()> {
    use std::io::Write;
    fs::create_dir_all(path.parent().context("Path needs a parent")?)?;
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        ensure!(
            !cancelled.load(std::sync::atomic::Ordering::Relaxed),
            "Copy cancelled"
        );
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        temp.write_all(&buffer[..n])?;
    }
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
