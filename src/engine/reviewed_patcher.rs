//! Strict outcomes for the four compiled native Patcher translations only.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

type Manifest = BTreeMap<String, (u64, String)>;
pub(super) struct Before {
    files: Manifest,
}
fn check(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Reviewed Patcher verification cancelled"
    );
    Ok(())
}
fn file_identity(path: &Path, cancelled: &AtomicBool) -> Result<(u64, String)> {
    let mut file = crate::paths::open_regular_read(path)?;
    let size = file.metadata()?.len();
    let mut digest = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        check(cancelled)?;
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .context("Game file size overflow")?;
        ensure!(
            total <= size,
            "Game file changed during strict verification"
        );
        digest.update(&buffer[..read]);
    }
    ensure!(
        total == size,
        "Game file truncated during strict verification"
    );
    Ok((size, format!("{:x}", digest.finalize())))
}
fn manifest(root: &Path, cancelled: &AtomicBool) -> Result<Manifest> {
    let root_metadata = std::fs::symlink_metadata(root)?;
    ensure!(
        root_metadata.is_dir() && !root_metadata.file_type().is_symlink(),
        "Strict Patcher game root must be a real directory"
    );
    let root = root.canonicalize()?;
    let mut pending = vec![root.clone()];
    let mut files = BTreeMap::new();
    let mut names = BTreeSet::new();
    let mut bytes = 0u64;
    while let Some(directory) = pending.pop() {
        check(cancelled)?;
        for entry in std::fs::read_dir(directory)? {
            check(cancelled)?;
            let path = entry?.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            ensure!(
                !metadata.file_type().is_symlink() && (metadata.is_dir() || metadata.is_file()),
                "Strict Patcher game inventory rejects links/special files"
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "Strict Patcher game inventory rejects reparse points"
                );
            }
            let relative = path
                .strip_prefix(&root)?
                .to_str()
                .context("Non-UTF8 game path")?
                .replace('\\', "/");
            ensure!(
                names.len() < 500_000 && names.insert(crate::unicode_case::ordinal_key(&relative)),
                "Strict Patcher game inventory bound or case collision"
            );
            ensure!(
                path.canonicalize()?.starts_with(&root),
                "Strict Patcher game inventory escaped root"
            );
            if metadata.is_dir() {
                pending.push(path);
            } else {
                let identity = file_identity(&path, cancelled)?;
                bytes = bytes
                    .checked_add(identity.0)
                    .context("Game inventory size overflow")?;
                ensure!(
                    bytes <= 256 * 1024 * 1024 * 1024,
                    "Strict Patcher game inventory exceeds 256 GiB"
                );
                files.insert(relative, identity);
            }
        }
    }
    ensure!(!files.is_empty(), "Strict Patcher game inventory is empty");
    Ok(files)
}
fn digest(files: &Manifest) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(files)?)))
}
pub(super) fn before(
    base: &Path,
    game: &Path,
    load_logs: &[odypatcher::PatchLog],
    cancelled: &AtomicBool,
) -> Result<Before> {
    ensure!(
        load_logs
            .iter()
            .all(|log| log.log_type < odypatcher::LogType::Warning),
        "Reviewed native Patcher parser reported a warning or error"
    );
    for name in ["installlog.txt", "installlog.rtf"] {
        ensure!(
            matches!(std::fs::symlink_metadata(base.join(name)), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
            "Reviewed native Patcher requires fresh absent logs"
        );
    }
    Ok(Before {
        files: manifest(game, cancelled)?,
    })
}
pub(super) fn finish(
    before: Before,
    base: &Path,
    game: &Path,
    report: &odypatcher::RunReport,
    logs: &[odypatcher::PatchLog],
    cancelled: &AtomicBool,
) -> Result<Value> {
    check(cancelled)?;
    ensure!(
        report.errors == 0
            && report.warnings == 0
            && report.patches_failed == 0
            && logs
                .iter()
                .all(|log| log.log_type < odypatcher::LogType::Warning),
        "Reviewed native Patcher requires zero warnings, errors and failed operations"
    );
    let marker = logs
        .iter()
        .find(|log| {
            log.log_type == odypatcher::LogType::Note
                && log
                    .message
                    .starts_with("Successfully completed installation. Processed ")
        })
        .context("Reviewed native Patcher did not emit its native success marker")?;
    let log_path = base.join("installlog.txt");
    let file = crate::paths::open_regular_read(&log_path)?;
    ensure!(
        file.metadata()?.len() <= 8 * 1024 * 1024,
        "Reviewed native Patcher log exceeds 8 MiB"
    );
    let mut bytes = Vec::new();
    file.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 8 * 1024 * 1024,
        "Reviewed native Patcher log grew beyond bound"
    );
    let text = std::str::from_utf8(&bytes).context("Reviewed native Patcher log is not UTF-8")?;
    ensure!(
        text.contains(&marker.message),
        "Reviewed native Patcher real log lacks native success marker"
    );
    let after = manifest(game, cancelled)?;
    let added = after
        .keys()
        .filter(|key| !before.files.contains_key(*key))
        .count();
    let removed = before
        .files
        .keys()
        .filter(|key| !after.contains_key(*key))
        .count();
    let changed = after
        .iter()
        .filter(|(key, value)| before.files.get(*key).is_some_and(|old| old != *value))
        .count();
    ensure!(
        added + removed + changed > 0,
        "Reviewed native Patcher made no actual game file changes"
    );
    Ok(
        json!({"Contract":"native-zero-warnings-changed-game-v1", "BeforeManifestSha256":digest(&before.files)?, "AfterManifestSha256":digest(&after)?, "Added":added,"Removed":removed,"Changed":changed,"LogSha256":format!("{:x}",Sha256::digest(&bytes)),"LogBytes":bytes.len(),"Configured":report.patches_configured,"Completed":report.patches_completed,"Skipped":report.patches_skipped,"Failed":report.patches_failed,"Warnings":report.warnings,"Errors":report.errors}),
    )
}
