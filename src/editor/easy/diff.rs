//! Read-only, byte-exact tree comparison for per-mod installation receipts.
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileIdentity {
    pub directory: bool,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeDiff {
    pub left: PathBuf,
    pub right: PathBuf,
    pub left_files: usize,
    pub right_files: usize,
    pub left_directories: usize,
    pub right_directories: usize,
    pub only_left: Vec<PathBuf>,
    pub only_right: Vec<PathBuf>,
    pub changed: Vec<PathBuf>,
    /// Exact relative exclusions used on both trees, included in the receipt.
    pub exclusions: Vec<PathBuf>,
    pub excluded_left: Vec<PathBuf>,
    pub excluded_right: Vec<PathBuf>,
    pub identical: bool,
}

type Inventory = BTreeMap<PathBuf, FileIdentity>;

pub fn compare(left: &Path, right: &Path, exclusions: &[String]) -> Result<TreeDiff> {
    let mut excluded = Vec::new();
    for path in exclusions {
        let path = crate::paths::relative(path, false)?;
        ensure!(
            !path.as_os_str().is_empty(),
            "Cannot exclude the entire comparison tree"
        );
        excluded.push(path);
    }
    let (left_root, left_files, excluded_left) = inventory(left, &excluded)?;
    let (right_root, right_files, excluded_right) = inventory(right, &excluded)?;
    ensure!(
        left_root != right_root,
        "Compare two distinct installation trees"
    );
    let only_left: Vec<_> = left_files
        .keys()
        .filter(|path| !right_files.contains_key(*path))
        .cloned()
        .collect();
    let only_right: Vec<_> = right_files
        .keys()
        .filter(|path| !left_files.contains_key(*path))
        .cloned()
        .collect();
    let changed: Vec<_> = left_files
        .iter()
        .filter_map(|(path, value)| {
            right_files
                .get(path)
                .filter(|other| {
                    value.directory != other.directory
                        || value.bytes != other.bytes
                        || value.sha256 != other.sha256
                })
                .map(|_| path.clone())
        })
        .collect();
    let identical = only_left.is_empty() && only_right.is_empty() && changed.is_empty();
    Ok(TreeDiff {
        left: left_root,
        right: right_root,
        left_files: left_files.values().filter(|entry| !entry.directory).count(),
        right_files: right_files
            .values()
            .filter(|entry| !entry.directory)
            .count(),
        left_directories: left_files.values().filter(|entry| entry.directory).count(),
        right_directories: right_files.values().filter(|entry| entry.directory).count(),
        only_left,
        only_right,
        changed,
        exclusions: excluded,
        excluded_left,
        excluded_right,
        identical,
    })
}

fn inventory(root: &Path, exclusions: &[PathBuf]) -> Result<(PathBuf, Inventory, Vec<PathBuf>)> {
    ensure!(
        fs::symlink_metadata(root)?.is_dir()
            && !fs::symlink_metadata(root)?.file_type().is_symlink(),
        "Comparison root must be an ordinary directory"
    );
    let root = root.canonicalize()?;
    let mut files = BTreeMap::new();
    let mut excluded = Vec::new();
    let mut entries = 0usize;
    for entry in walkdir::WalkDir::new(&root)
        .follow_links(false)
        .sort_by_file_name()
    {
        let entry = entry?;
        entries += 1;
        ensure!(
            entries <= 1_000_000,
            "Comparison tree exceeds one million entries"
        );
        let path = entry.path();
        let relative = path.strip_prefix(&root)?.to_path_buf();
        if relative.as_os_str().is_empty() {
            continue;
        }
        ensure!(
            !entry.file_type().is_symlink(),
            "Comparison refuses symlink {}",
            path.display()
        );
        ensure!(
            entry.file_type().is_file() || entry.file_type().is_dir(),
            "Comparison refuses special entry {}",
            path.display()
        );
        if exclusions.iter().any(|prefix| relative.starts_with(prefix)) {
            excluded.push(relative);
            continue;
        }
        if entry.file_type().is_dir() {
            files.insert(
                relative,
                FileIdentity {
                    directory: true,
                    bytes: 0,
                    sha256: String::new(),
                },
            );
            continue;
        }
        let mut file = crate::paths::open_regular_read(path)?;
        let before = file.metadata()?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut bytes = 0u64;
        loop {
            let count = file
                .read(&mut buffer)
                .with_context(|| format!("Hash {}", path.display()))?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
            bytes = bytes
                .checked_add(count as u64)
                .context("Comparison byte count overflow")?;
        }
        let after = file.metadata()?;
        ensure!(
            before.len() == bytes
                && after.len() == bytes
                && before.modified()? == after.modified()?,
            "File changed during comparison: {}",
            path.display()
        );
        files.insert(
            relative,
            FileIdentity {
                directory: false,
                bytes,
                sha256: format!("{:x}", hash.finalize()),
            },
        );
    }
    Ok((root, files, excluded))
}
