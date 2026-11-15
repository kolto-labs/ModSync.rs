//! Extras: validate instructions, restore backups, force lowercase filenames.

use std::path::Path;

use crate::compat::fsutil;
use crate::patcher::logger::{LogType, PatchLogger};
use crate::patcher::uninstall::{ModUninstaller, UninstallDialogs};
use crate::patcher::ModInstaller;
use crate::session::Session;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Something validation noticed about a mod.
pub struct Finding {
    /// True when this would stop a run rather than just warn.
    pub blocking: bool,
    /// What was noticed, in words.
    pub message: String,
}

impl Finding {
    fn error(message: impl Into<String>) -> Self {
        Self {
            blocking: true,
            message: message.into(),
        }
    }

    fn warning(message: impl Into<String>) -> Self {
        Self {
            blocking: false,
            message: message.into(),
        }
    }
}

/// Check the mod's instructions the way a run would read them, without
/// changing anything.
///
/// The reader's own verdict decides: a failure to load the instruction file
/// is blocking, as are the lines it logged as errors; its warnings are
/// reported. With a game folder the check goes through
/// [`ModInstaller::config`] (so `Required*` files are checked too) and then
/// the installer's path validation; without one the file is read on its own.
pub fn validate(session: &Session, game_folder: Option<&Path>) -> Vec<Finding> {
    let mut findings = Vec::new();

    if let Some(error) = &session.namespace_error {
        findings.push(Finding::error(format!(
            "Cannot read {}: {error}",
            crate::session::NAMESPACES_FILE
        )));
    }

    if !session.has_instructions() {
        findings.push(Finding::error(format!(
            "Cannot find the instruction file \"{}\" in {}.",
            session.ini_name(),
            session.tslpatchdata_path.display()
        )));
        return findings;
    }

    match game_folder {
        Some(game_folder) => {
            let mut installer = match ModInstaller::new(
                &session.base_path,
                game_folder,
                &session.ini_path(),
                Some(PatchLogger::new()),
            ) {
                Ok(installer) => installer,
                Err(err) => {
                    findings.push(Finding::error(err.simplified()));
                    return findings;
                }
            };

            let loaded = installer.config().map(|_| ());
            let checked = loaded.and_then(|_| installer.validate_configured_paths());
            collect_log_findings(&installer.log.all_logs, &mut findings);
            if let Err(err) = checked {
                findings.push(Finding::error(err.simplified()));
            }
        }
        None => {
            collect_log_findings(&session.load_logs, &mut findings);
            if let Some(error) = &session.config_error {
                findings.push(Finding::error(error.clone()));
            }
        }
    }

    if findings.is_empty() {
        findings.push(Finding::warning("No problems found."));
    }

    findings
}

fn collect_log_findings(logs: &[crate::patcher::logger::PatchLog], findings: &mut Vec<Finding>) {
    for log in logs {
        match log.log_type {
            LogType::Error => findings.push(Finding::error(log.message.clone())),
            LogType::Warning => findings.push(Finding::warning(log.message.clone())),
            LogType::Verbose | LogType::Note => {}
        }
    }
}

/// What a restore did.
#[derive(Debug, Default)]
pub struct RestoreReport {
    /// True when a backup was restored.
    pub restored: bool,
    /// Messages the uninstaller would have shown in dialogs, as
    /// `(title, message)`.
    pub messages: Vec<(String, String)>,
    /// Every line the uninstaller logged.
    pub logs: Vec<crate::patcher::logger::PatchLog>,
}

/// Restore the most recent backup under `mod_folder/backup` into the game.
///
/// Questions are answered as an unattended run would: yes to restoring, no
/// to deleting the backup afterwards. Anything the uninstaller would have
/// shown in a dialog is collected in the report instead.
pub fn restore_backup(mod_folder: &Path, game_folder: &Path) -> RestoreReport {
    let messages = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

    let (m1, m2, m3) = (messages.clone(), messages.clone(), messages.clone());
    let dialogs = UninstallDialogs {
        askyesno: Box::new(move |title, message| {
            m1.borrow_mut()
                .push((title.to_string(), message.to_string()));
            // Keep the backup once it has been restored.
            title != "Uninstall completed"
        }),
        showerror: Box::new(move |title, message| {
            m2.borrow_mut()
                .push((title.to_string(), message.to_string()));
        }),
        showwarning: Box::new(move |title, message| {
            m3.borrow_mut()
                .push((title.to_string(), message.to_string()));
        }),
    };

    let mut uninstaller = ModUninstaller::new(
        &mod_folder.join("backup"),
        game_folder,
        Some(PatchLogger::new()),
    )
    .with_dialogs(dialogs);

    let restored = uninstaller.uninstall_selected_mod().unwrap_or(false);
    let logs = uninstaller.logger.all_logs.clone();
    drop(uninstaller);

    RestoreReport {
        restored,
        messages: messages.take(),
        logs,
    }
}

/// Outcome of a [`force_lowercase`] walk.
#[derive(Debug, Default)]
pub struct LowercaseReport {
    /// Entries renamed to their canonical lowercase form; no collision.
    pub renamed: usize,
    /// Case-only duplicate pairs found byte-identical. The duplicate was
    /// removed and the canonical lowercase entry kept.
    pub merged: usize,
    /// Case-only duplicate pairs left untouched because deleting either
    /// side could lose real content: their contents differ, or one is a
    /// file where the other is a directory. Both paths remain on disk;
    /// each pair is reported here so a human or caller can resolve it
    /// deliberately instead of it being silently dropped or silently left
    /// duplicated.
    pub conflicts: Vec<(std::path::PathBuf, std::path::PathBuf)>,
    /// Entries the walk meant to fold but could not, with the reason. A
    /// write-protected parent is the usual cause. These matter because a
    /// caller that folds its lookups expects the lowercase name to be the
    /// one on disk; where the rename did not happen, it is not.
    pub failed: Vec<(std::path::PathBuf, String)>,
    /// True when the walk did not run at all, because the filesystem does not
    /// distinguish case or could not be probed. Distinct from a clean fold:
    /// nothing was folded, so a caller must not treat the tree as folded.
    pub skipped: bool,
}

impl LowercaseReport {
    /// True when nothing was left for a human to resolve.
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty() && self.failed.is_empty()
    }
}

/// True when the filesystem holding `dir` tells `A` and `a` apart.
///
/// Linux ext4 and btrfs do. macOS APFS, Windows NTFS, and an exFAT game
/// drive do not. Where it does not, folding a name is not a rename at all:
/// `Spells.2da` and `spells.2da` are the same file, so the merge path below
/// would compare that file with itself, find it identical, and delete it.
/// Probing first is what stops that.
///
/// A directory that cannot be written to answers `false`, which skips the
/// walk. That is the right answer for a read-only game folder: nothing to
/// gain, and no way to rename anyway.
pub fn filesystem_distinguishes_case(dir: &std::path::Path) -> bool {
    // Prefer asking about a name that is already there. A game folder is a
    // place the player owns, and writing a scratch file into it to answer a
    // question about it is a poor trade -- especially since a crash between
    // the write and the delete leaves the scratch file behind for good.
    if let Ok(entries) = std::fs::read_dir(dir) {
        // Bounded: on a filesystem that does not distinguish case every
        // flipped name resolves, so without a cap this would stat a whole
        // `override` folder before falling through to the write probe below.
        for entry in entries.flatten().take(32) {
            // A name that cannot be read faithfully cannot answer. Converting
            // it lossily would invent a spelling that exists under neither
            // case, which reads as proof of case-sensitivity -- the answer
            // that folds, and folding is what deletes.
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let flipped = flip_ascii_case(&name);
            if flipped == name {
                // No ASCII letters to flip; this entry cannot answer.
                continue;
            }

            // `symlink_metadata`, not `exists`. `exists` follows links and
            // reports false for any error at all, so a broken symlink or a
            // directory that can be listed but not searched would both look
            // like proof of case-sensitivity. Ask whether each *name*
            // resolves, and demand a positive answer for the one that is
            // really there before believing the negative for the other.
            if std::fs::symlink_metadata(dir.join(&name)).is_ok()
                && std::fs::symlink_metadata(dir.join(&flipped)).is_err()
            {
                return true;
            }

            // The flipped spelling resolves too. That is what a
            // case-insensitive filesystem looks like -- and also what a
            // genuine case-only duplicate pair looks like, which is the very
            // thing this walk exists to clean up. Not conclusive, so keep
            // looking, then ask the filesystem directly.
        }
    }

    // Nothing here could settle it. Writing is the only way left to ask.
    // A folder full of case-duplicate pairs lands here, which is why the
    // read-only check above is a shortcut and not the whole answer.
    let name = format!(
        "OdyPatcherCaseProbe-{}-{:?}.tmp",
        std::process::id(),
        std::thread::current().id()
    );
    let probe = dir.join(&name);
    if std::fs::write(&probe, b"x").is_err() {
        return false;
    }
    let collided = dir.join(name.to_ascii_lowercase()).exists();
    let _ = std::fs::remove_file(&probe);
    !collided
}

/// Swap the case of every ASCII letter, leaving everything else alone.
fn flip_ascii_case(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_uppercase() {
                c.to_ascii_lowercase()
            } else if c.is_ascii_lowercase() {
                c.to_ascii_uppercase()
            } else {
                c
            }
        })
        .collect()
}

/// True when two paths name one and the same file on disk.
///
/// The guard of last resort. Where case is not distinguished, `Spells.2da`
/// and `spells.2da` are one file, and the merge below would compare it with
/// itself, find it identical, and delete it. Every other check here tries to
/// keep the walk away from such a filesystem; this one makes being wrong
/// about that survivable.
///
/// `canonicalize` answers on Windows, which corrects case; it does not on
/// macOS, which returns the spelling it was given. Identity numbers answer
/// everywhere that has them.
fn is_same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(left), Ok(right)) => left.dev() == right.dev() && left.ino() == right.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(left), Ok(right)) => left == right,
            _ => false,
        }
    }
}

/// True when the path is a directory in its own right rather than a link to
/// one. A walk that renames and deletes must not follow a link out of the
/// tree its caller checked.
fn is_real_dir(path: &std::path::Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir())
}

fn note_failure(report: &mut LowercaseReport, path: &std::path::Path, err: &std::io::Error) {
    report.failed.push((path.to_path_buf(), err.to_string()));
}

/// Rename files and folders below a path to lowercase.
///
/// Useful where the filesystem tells `Appearance.2da` and `appearance.2da`
/// apart. When the lowercase name already exists, this merges instead of
/// skipping: byte-identical duplicates collapse into the canonical entry,
/// and genuinely differing content is left in place and reported via
/// [`LowercaseReport::conflicts`] rather than being silently dropped or
/// silently left duplicated on disk.
/// On a filesystem that does not distinguish case this does nothing at all
/// and reports nothing: there is no rename to make, and attempting one would
/// destroy data rather than fold it.
/// Each root is probed in its own right. Sharing one answer across folders
/// would be cheaper, but a case-insensitive mount below a case-sensitive game
/// folder would then be folded on the parent's answer -- and there the merge
/// branch compares a file with itself and deletes it.
pub fn force_lowercase(root: &str) -> LowercaseReport {
    let mut report = LowercaseReport::default();
    if !filesystem_distinguishes_case(&fsutil::native_path(root)) {
        report.skipped = true;
        return report;
    }
    force_lowercase_into(root, &mut report);
    report
}

fn force_lowercase_into(root: &str, report: &mut LowercaseReport) {
    let target = fsutil::native_path(root);

    let entries = match std::fs::read_dir(&target) {
        Ok(entries) => entries,
        Err(err) => {
            // Reported, not swallowed. An empty report reads as a clean fold,
            // and a caller that then trusts this tree would conclude the
            // player's files are absent and install fresh copies over them
            // instead of patching them.
            note_failure(report, &target, &err);
            return;
        }
    };

    // Collect first: renaming while reading the directory is not dependable.
    let paths: Vec<std::path::PathBuf> = entries.flatten().map(|e| e.path()).collect();

    for path in paths {
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };

        // ASCII, not Unicode: every matcher that looks a folded name back up
        // compares with `eq_ignore_ascii_case`, so folding wider than ASCII
        // produces names those matchers cannot find.
        let lowered = name.to_ascii_lowercase();
        let mut current = path.clone();

        if lowered != name {
            let destination = path.with_file_name(&lowered);

            if !destination.exists() {
                match std::fs::rename(&path, &destination) {
                    Ok(()) => {
                        report.renamed += 1;
                        current = destination;
                    }
                    Err(err) => note_failure(report, &path, &err),
                }
            } else if is_real_dir(&path) && is_real_dir(&destination) {
                merge_directory(&path, &destination, report);
                // Only succeeds once every entry underneath has been moved
                // or merged; a non-empty remainder means unresolved
                // conflicts are still inside it, so leave it in place.
                if std::fs::remove_dir(&path).is_ok() {
                    current = destination;
                }
            } else if path.is_file() && destination.is_file() {
                if is_same_file(&path, &destination) {
                    // One file wearing two spellings. There is nothing to
                    // merge and nothing that may be deleted.
                    report.conflicts.push((path.clone(), destination));
                } else if files_are_identical(&path, &destination) {
                    match std::fs::remove_file(&path) {
                        Ok(()) => {
                            report.merged += 1;
                            current = destination;
                        }
                        Err(err) => note_failure(report, &path, &err),
                    }
                } else {
                    report.conflicts.push((path.clone(), destination));
                }
            } else {
                report.conflicts.push((path.clone(), destination));
            }
        }

        // `symlink_metadata`, not `is_dir`: a symlinked subdirectory would
        // walk the rename (and the merge branch's delete) outside the tree
        // whose root was confinement-checked.
        if is_real_dir(&current) {
            force_lowercase_into(&current.to_string_lossy(), report);
        }
    }
}

/// Move every entry of `source` into `destination`, matching existing
/// entries by name without regard to case rather than assuming
/// `destination` is already fully normalized. Recurses into matching
/// subdirectories; resolves file collisions the same way
/// [`force_lowercase_into`] does (identical content merges, differing
/// content is reported as a conflict and left alone).
fn merge_directory(
    source: &std::path::Path,
    destination: &std::path::Path,
    report: &mut LowercaseReport,
) {
    let entries = match std::fs::read_dir(source) {
        Ok(entries) => entries,
        Err(err) => {
            // Not silent: a caller that trusts a folded tree needs to know
            // this subtree was never read.
            note_failure(report, source, &err);
            return;
        }
    };

    for entry in entries.flatten() {
        let src_path = entry.path();
        let Some(name) = src_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
        else {
            continue;
        };

        let existing = match find_case_insensitive(destination, &name) {
            Ok(existing) => existing,
            Err(err) => {
                note_failure(report, destination, &err);
                continue;
            }
        };

        match existing {
            None => {
                // Nothing in `destination` collides even ignoring case; move
                // it over under its own name. The outer walk lowercases it
                // on its next pass over `destination`.
                let dst_path = destination.join(&name);
                match std::fs::rename(&src_path, &dst_path) {
                    Ok(()) => report.renamed += 1,
                    Err(err) => note_failure(report, &src_path, &err),
                }
            }
            Some(existing_name) => {
                let dst_path = destination.join(&existing_name);
                if is_real_dir(&src_path) && is_real_dir(&dst_path) {
                    merge_directory(&src_path, &dst_path, report);
                    // Only empties once every entry moved. A failure here
                    // means something is still inside, so the fold is not
                    // clean and the caller must not treat it as one.
                    if let Err(err) = std::fs::remove_dir(&src_path) {
                        note_failure(report, &src_path, &err);
                    }
                } else if src_path.is_file() && dst_path.is_file() {
                    if is_same_file(&src_path, &dst_path) {
                        report.conflicts.push((src_path, dst_path));
                    } else if files_are_identical(&src_path, &dst_path) {
                        match std::fs::remove_file(&src_path) {
                            Ok(()) => report.merged += 1,
                            Err(err) => note_failure(report, &src_path, &err),
                        }
                    } else {
                        report.conflicts.push((src_path, dst_path));
                    }
                } else {
                    report.conflicts.push((src_path, dst_path));
                }
            }
        }
    }
}

/// Find an entry in `dir` whose name matches `wanted` ignoring case.
///
/// An error is not the same answer as "nothing collides". The caller moves a
/// file when nothing collides, and `rename` replaces the destination
/// silently, so a directory that could not be read must not look empty.
fn find_case_insensitive(dir: &std::path::Path, wanted: &str) -> std::io::Result<Option<String>> {
    for entry in std::fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.eq_ignore_ascii_case(wanted) {
            return Ok(Some(name));
        }
    }
    Ok(None)
}

/// True when two files have identical content, compared in chunks rather
/// than loading either fully into memory (game textures/audio can be large).
fn files_are_identical(a: &std::path::Path, b: &std::path::Path) -> bool {
    use std::io::Read;

    let (Ok(meta_a), Ok(meta_b)) = (std::fs::metadata(a), std::fs::metadata(b)) else {
        return false;
    };
    if meta_a.len() != meta_b.len() {
        return false;
    }

    let (Ok(file_a), Ok(file_b)) = (std::fs::File::open(a), std::fs::File::open(b)) else {
        return false;
    };

    let mut reader_a = std::io::BufReader::new(file_a);
    let mut reader_b = std::io::BufReader::new(file_b);
    let mut buf_a = [0u8; 8192];
    let mut buf_b = [0u8; 8192];

    loop {
        let Ok(read_a) = reader_a.read(&mut buf_a) else {
            return false;
        };
        let Ok(read_b) = reader_b.read(&mut buf_b) else {
            return false;
        };
        if read_a != read_b || buf_a[..read_a] != buf_b[..read_b] {
            return false;
        }
        if read_a == 0 {
            return true;
        }
    }
}
