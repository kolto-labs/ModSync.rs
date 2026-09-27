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
}
impl Drop for Lock {
    fn drop(&mut self) {
        for file in &self.files {
            let _ = FileExt::unlock(file);
        }
    }
}
impl Lock {
    pub fn acquire(roots: &Roots) -> Result<Self> {
        let dir = state_dir(roots);
        ensure!(
            !dir.starts_with(&roots.mods) && !roots.mods.starts_with(&dir),
            "Recovery directory overlaps mod directory"
        );
        fs::create_dir_all(&dir)?;
        ensure!(
            !fs::symlink_metadata(&dir)?.file_type().is_symlink(),
            "Recovery directory is a symlink"
        );
        let mut lock = Self { files: Vec::new() };
        let mut roots = [&roots.game, &roots.mods];
        roots.sort();
        for root in roots {
            let hash = format!("{:x}", Sha256::digest(root.to_string_lossy().as_bytes()));
            let path = root
                .parent()
                .unwrap()
                .join(format!(".modsync-lock-{}", &hash[..16]));
            if let Ok(meta) = fs::symlink_metadata(&path) {
                ensure!(!meta.file_type().is_symlink(), "Lock file is a symlink");
            }
            let file = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(path)?;
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

pub fn ensure_clean(roots: &Roots) -> Result<()> {
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

pub fn finish(roots: &Roots) -> Result<()> {
    fs::remove_file(state_dir(roots).join("pending"))?;
    Ok(())
}

pub fn restore(roots: &Roots) -> Result<()> {
    let _lock = Lock::acquire(roots)?;
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
