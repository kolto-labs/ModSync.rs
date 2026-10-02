//! Basic CLI validation with the pinned C# pipeline's null-root context.
//!
//! Source contract: d31ccbd, `ComponentValidation.cs` (`Run`, archive collection,
//! source membership, destinations), `InstallationValidationPipeline.cs`
//! (`RunComponentValidationStage`, `RunFomodConfigurationStage`), and
//! `Utility/Utility.cs:ReplaceCustomVariables`.
//!
//! This is a read-only subset: no editor fallback, directory creation, automatic
//! instruction rewrites, external programs, environment checks, conflict/order
//! checks, or dry-run. Unlike C# AttemptFixes it never mutates a guide or disk.
//! It therefore does not claim parity for mutation or exception side effects.
//! The model loses the distinction between null and empty Source lists, so the
//! reference's null-Source warning cannot be reconstructed. Archive member names
//! are verified with the shared in-process readers and retain exact spelling.
//! Symlink traversal, device files, parent traversal, and exhausted budgets are
//! explicit unsupported diagnostics rather than successful validation.

use super::{ComponentOutcome, Report, Stage, StageResult};
use crate::{Component, Guide, Instruction};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component as PathComponent, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const MAX_PATH_BYTES: usize = 16_384;
const MAX_VISITS: usize = 10_000;
const MAX_ARCHIVES: usize = 64;
const MAX_COMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_TOTAL_EXPANDED_BYTES: u64 = 1024 * 1024 * 1024;
const RECOVERY_HINT: &str = "Run Fetch Downloads and complete the FOMOD installer wizard for this archive, or re-run CLI download with --fomod-choices / MODSYNC_FOMOD_CHOICES (interactive TTY configure also works).";

/// Validate selected base components in authored order, without initializing
/// installation roots, profiles, an engine, or settings. Root flags do not enter
/// this API. Exact-case custom variables are removed as C# replaces null roots
/// with empty strings, so authored literal paths still receive actual FS checks.
/// Cancellation is returned as an error, never a successful partial report.
pub fn validate(guide: &Guide, cancelled: &AtomicBool) -> Result<Report> {
    check_cancelled(cancelled)?;
    let selected: Vec<_> = guide.components.iter().filter(|c| c.is_selected).collect();
    let mut report = Report::new(selected.len());
    if selected.is_empty() {
        report.error_count = 1;
        report.stages.push(StageResult {
            stage: Stage::Environment,
            passed: false,
            has_warnings: false,
            summary: "No components selected for validation.".into(),
            messages: Vec::new(),
        });
        return Ok(report.finish());
    }
    let mut context = ContextState {
        guide,
        cancelled,
        visits: 0,
        archives: BTreeMap::new(),
        expanded_budget: MAX_TOTAL_EXPANDED_BYTES,
    };
    let mut outcomes = Vec::new();
    for component in selected {
        check_cancelled(cancelled)?;
        outcomes.push((component, context.component(component)?));
    }
    report.component_outcomes(&outcomes);
    check_cancelled(cancelled)?;
    // InstallationValidationPipeline.cs:439-450. This null-root basic context
    // cannot run the gate; it must report that missing prerequisite explicitly.
    report.error_count += 1;
    report.stages.push(StageResult {
        stage: Stage::FomodConfiguration,
        passed: false,
        has_warnings: false,
        summary: "Cannot check FOMOD configuration: mod directory is not set or does not exist."
            .into(),
        messages: vec![format!(
            "ERROR: FOMOD: Mod directory is not set or does not exist. {RECOVERY_HINT}"
        )],
    });
    Ok(report.finish())
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Basic validation cancelled"
    );
    Ok(())
}

fn formatted(raw: &str) -> String {
    let mut text = String::new();
    for c in raw.trim_start_matches('\\').chars() {
        let c = if c == '\\' { '/' } else { c };
        if c == '/' && text.ends_with('/') && !text.ends_with(":/") {
            continue;
        }
        text.push(c);
    }
    if text.len() > 1 {
        text.truncate(text.trim_end_matches('/').len());
    }
    text
}

fn replace_null_roots(raw: &str) -> String {
    raw.replace("<<modDirectory>>", "")
        .replace("<<kotorDirectory>>", "")
        .replace("<<gameDirectory>>", "")
}

fn instructions(component: &Component) -> impl Iterator<Item = &Instruction> {
    component.instructions.iter().chain(
        component
            .options
            .iter()
            .flat_map(|option| option.instructions.iter()),
    )
}

fn same_guid(left: &str, right: &str) -> bool {
    crate::guid::eq_ascii(left, right)
}

/// Only archive collection consults these dependency/restriction conditions.
fn should_run(instruction: &Instruction, guide: &Guide) -> bool {
    let find = |id: &str| {
        guide.components.iter().find_map(|component| {
            if same_guid(&component.guid, id) {
                Some(component)
            } else {
                component
                    .options
                    .iter()
                    .find(|option| same_guid(&option.guid, id))
            }
        })
    };
    instruction
        .dependencies
        .iter()
        .all(|id| find(id).is_some_and(|c| c.is_selected))
        && instruction
            .restrictions
            .iter()
            .all(|id| !find(id).is_some_and(|c| c.is_selected))
}

/// C# WildcardPathMatch: exact case, equal path depth, only * and ? are special.
fn wildcard_path(input: &str, pattern: &str) -> bool {
    fn level(input: &str, pattern: &str) -> bool {
        let input: Vec<_> = input.chars().collect();
        let pattern: Vec<_> = pattern.chars().collect();
        let (mut i, mut p, mut star, mut retry) = (0, 0, None, 0);
        while i < input.len() {
            if p < pattern.len() && (pattern[p] == '?' || pattern[p] == input[i]) {
                i += 1;
                p += 1;
            } else if p < pattern.len() && pattern[p] == '*' {
                star = Some(p);
                p += 1;
                retry = i;
            } else if let Some(start) = star {
                retry += 1;
                i = retry;
                p = start + 1;
            } else {
                return false;
            }
        }
        pattern[p..].iter().all(|c| *c == '*')
    }
    let input = formatted(input);
    let pattern = formatted(pattern);
    let inputs: Vec<_> = input.split('/').collect();
    let patterns: Vec<_> = pattern.split('/').collect();
    inputs.len() == patterns.len()
        && inputs
            .iter()
            .zip(patterns)
            .all(|(input, pattern)| level(input, pattern))
}

struct ContextState<'a> {
    guide: &'a Guide,
    cancelled: &'a AtomicBool,
    visits: usize,
    archives: BTreeMap<PathBuf, std::result::Result<Vec<crate::archive::OriginalEntry>, String>>,
    expanded_budget: u64,
}

impl ContextState<'_> {
    fn visit(&mut self) -> Result<()> {
        check_cancelled(self.cancelled)?;
        self.visits += 1;
        ensure!(
            self.visits <= MAX_VISITS,
            "Basic validation filesystem budget exceeded"
        );
        Ok(())
    }

    /// Inspect only the authored path and its wildcard subtree. Never follow a
    /// symlink; every ancestor is checked before a regular-file reader is used.
    fn checked_path(&mut self, raw: &str) -> Result<PathBuf> {
        ensure!(
            raw.len() <= MAX_PATH_BYTES,
            "Basic validation path budget exceeded"
        );
        ensure!(!raw.contains('\0'), "Invalid path contains NUL");
        let path = std::path::absolute(Path::new(raw))?;
        let mut prefix = PathBuf::new();
        for part in path.components() {
            self.visit()?;
            ensure!(
                !matches!(part, PathComponent::ParentDir),
                "Parent traversal is unsupported"
            );
            prefix.push(part);
            if part.as_os_str().to_string_lossy().contains(['*', '?']) {
                break;
            }
            match fs::symlink_metadata(&prefix) {
                Ok(metadata) => ensure!(
                    !metadata.file_type().is_symlink(),
                    "Symlink traversal is unsupported: {}",
                    prefix.display()
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(path)
    }

    fn expand(&mut self, raw: &str) -> Result<Vec<PathBuf>> {
        let raw = formatted(&replace_null_roots(raw));
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        let path = self.checked_path(&raw)?;
        if !raw.contains(['*', '?']) {
            let path = self.case_path(&path)?;
            self.checked_path(&path.to_string_lossy())?;
            return match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_file() => Ok(vec![path]),
                Ok(metadata) if metadata.is_dir() => Ok(Vec::new()),
                Ok(_) => anyhow::bail!("Special-file source is unsupported: {}", path.display()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
                Err(error) => Err(error.into()),
            };
        }
        let mut root = path.clone();
        while root.to_string_lossy().contains(['*', '?']) {
            root = root
                .parent()
                .context("Wildcard has no containing directory")?
                .to_path_buf();
        }
        self.checked_path(&root.to_string_lossy())?;
        if !root.is_dir() {
            return Ok(Vec::new());
        }
        let mut stack = vec![(root.clone(), 0usize)];
        let mut matches = Vec::new();
        while let Some((directory, depth)) = stack.pop() {
            ensure!(
                depth <= 64,
                "Basic validation directory depth budget exceeded"
            );
            for entry in fs::read_dir(directory)? {
                self.visit()?;
                let entry = entry?;
                let kind = entry.file_type()?;
                ensure!(
                    !kind.is_symlink(),
                    "Symlink traversal is unsupported: {}",
                    entry.path().display()
                );
                if kind.is_dir() {
                    stack.push((entry.path(), depth + 1));
                } else if kind.is_file()
                    && wildcard_path(&entry.path().to_string_lossy(), &path.to_string_lossy())
                {
                    matches.push(entry.path());
                }
            }
        }
        matches.sort();
        Ok(matches)
    }

    /// Default C# Unix pathing retries a missing literal with actual filesystem
    /// spelling. Keep this retry bounded and reject ambiguous aliases rather
    /// than choosing an arbitrary file or traversing a symlink.
    fn case_path(&mut self, path: &Path) -> Result<PathBuf> {
        let mut resolved = PathBuf::new();
        for part in path.components() {
            self.visit()?;
            let candidate = resolved.join(part);
            match fs::symlink_metadata(&candidate) {
                Ok(metadata) => {
                    ensure!(
                        !metadata.file_type().is_symlink(),
                        "Symlink traversal is unsupported: {}",
                        candidate.display()
                    );
                    resolved = candidate;
                    continue;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            if !resolved.is_dir() || !matches!(part, PathComponent::Normal(_)) {
                resolved = candidate;
                continue;
            }
            let mut alias = None;
            for entry in fs::read_dir(&resolved)? {
                self.visit()?;
                let entry = entry?;
                if crate::fomod::eq_ignore_case(
                    &entry.file_name().to_string_lossy(),
                    &part.as_os_str().to_string_lossy(),
                ) {
                    ensure!(
                        alias.is_none(),
                        "Ambiguous case-insensitive path is unsupported: {}",
                        candidate.display()
                    );
                    ensure!(
                        !entry.file_type()?.is_symlink(),
                        "Symlink traversal is unsupported: {}",
                        entry.path().display()
                    );
                    alias = Some(entry.path());
                }
            }
            resolved = alias.unwrap_or(candidate);
        }
        Ok(resolved)
    }

    fn component(&mut self, component: &Component) -> Result<ComponentOutcome> {
        let mut outcome = ComponentOutcome {
            is_valid: true,
            ..Default::default()
        };
        let mut archives = Vec::new();
        for instruction in instructions(component) {
            check_cancelled(self.cancelled)?;
            if instruction.action != "Extract" || !should_run(instruction, self.guide) {
                continue;
            }
            let mut unique_sources = BTreeSet::new();
            for source in &instruction.source {
                if !unique_sources.insert(source) {
                    continue;
                }
                match self.expand(source) {
                    Ok(paths) => archives.extend(paths),
                    Err(error) => {
                        check_cancelled(self.cancelled)?;
                        outcome.errors.push(format!(
                            "Unsupported read-only archive lookup '{source}': {error}"
                        ));
                    }
                }
            }
        }
        if archives.is_empty() {
            // Intentionally only base Extracts: unmatched option-only Extracts
            // produce no Missing Required Archives diagnostic in the reference.
            for instruction in &component.instructions {
                if instruction.action == "Extract" {
                    outcome.errors.push(format!(
                        "Missing Required Archives: [{}]",
                        instruction.source.join(",")
                    ));
                    outcome.is_valid = false;
                }
            }
        } else {
            for instruction in instructions(component) {
                check_cancelled(self.cancelled)?;
                if instruction.action == "Unset" || instruction.action.is_empty() {
                    outcome.errors.push("Action cannot be null".into());
                    outcome.is_valid = false;
                    continue;
                }
                if matches!(instruction.action.as_str(), "Extract" | "Choose") {
                    continue;
                }
                let mut archive_name_found = true;
                for raw in &instruction.source {
                    if raw.len() > MAX_PATH_BYTES {
                        outcome.errors.push(
                            "Unsupported read-only source membership: path budget exceeded".into(),
                        );
                        continue;
                    }
                    let source = formatted(raw);
                    let lower = source.to_ascii_lowercase();
                    if lower.starts_with("<<gamedirectory>>")
                        || lower.starts_with("<<kotordirectory>>")
                    {
                        continue;
                    }
                    if lower.ends_with("tslpatcher.exe") && instruction.action != "Patcher" {
                        outcome.warnings.push("'tslpatcher.exe' used in Source path without the ActionType 'Patcher', was this intentional?".into());
                    }
                    let (found, _) =
                        self.membership(&source, &archives, instruction, &mut outcome)?;
                    if !found {
                        // Mirror the default AttemptFixes duplicate-folder probe
                        // without changing the authored instruction. The first
                        // lookup's diagnostics remain collected even if this
                        // alternate source matches.
                        let parts: Vec<_> = source.split('/').collect();
                        if let Some(folder) = parts.get(1) {
                            let alternate = std::iter::once(parts[0])
                                .chain([*folder, *folder])
                                .chain(parts.iter().skip(2).copied())
                                .collect::<Vec<_>>()
                                .join("/");
                            let (found, named) =
                                self.membership(&alternate, &archives, instruction, &mut outcome)?;
                            outcome.is_valid &= found;
                            archive_name_found &= named;
                        } else {
                            // C# indexes parts[1] and catches the resulting
                            // exception, returning false without a new error.
                            outcome.is_valid = false;
                            return Ok(outcome);
                        }
                    }
                }
                if !archive_name_found {
                    outcome.warnings.push("'Source' path does not include the archive's name as part of the extraction folder, possible FileNotFound exception.".into());
                }
            }
        }
        // Run() is VerifyExtractPaths() && ParseDestinationWithAction(). Source
        // membership errors survive successful read-only alternate-path probes;
        // unsuccessful probes stop destination validation through this bool.
        if outcome.is_valid {
            self.destinations(component, &mut outcome)?;
        }
        Ok(outcome)
    }

    fn members(
        &mut self,
        path: &Path,
    ) -> Result<std::result::Result<Vec<crate::archive::OriginalEntry>, String>> {
        if let Some(cached) = self.archives.get(path) {
            return Ok(cached.clone());
        }
        ensure!(
            self.archives.len() < MAX_ARCHIVES,
            "Basic validation archive-count budget exceeded"
        );
        let result = (|| -> Result<Vec<crate::archive::OriginalEntry>> {
            self.checked_path(&path.to_string_lossy())?;
            let file = crate::paths::open_regular_read(path)?;
            ensure!(
                file.metadata()?.len() <= MAX_COMPRESSED_BYTES,
                "Compressed archive budget exceeded"
            );
            let options = crate::archive::Options {
                scratch_dir: None,
                max_expanded_bytes: Some(MAX_EXPANDED_BYTES.min(self.expanded_budget)),
            };
            // Verified listing spools nothing and launches no external programs.
            let budget = options.max_expanded_bytes.unwrap();
            // Reserve the entire allowance even on failure: failed decoders can
            // already have consumed it, so repeated failures cannot reset it.
            self.expanded_budget -= budget;
            let (names, expanded) =
                crate::archive::list_original_with_size(path, path, &options, self.cancelled)?;
            self.expanded_budget += budget - expanded;
            Ok(names)
        })()
        .map_err(|error| error.to_string());
        check_cancelled(self.cancelled)?;
        self.archives.insert(path.to_path_buf(), result.clone());
        Ok(result)
    }

    fn membership(
        &mut self,
        source: &str,
        archives: &[PathBuf],
        instruction: &Instruction,
        outcome: &mut ComponentOutcome,
    ) -> Result<(bool, bool)> {
        let source = source
            .replace("<<modDirectory>>/", "")
            .replace("<<gameDirectory>>/", "");
        let mut found = false;
        let mut reasons = Vec::new();
        let mut archive_name_found = false;
        for archive in archives {
            check_cancelled(self.cancelled)?;
            let root = archive.file_stem().unwrap_or_default().to_string_lossy();
            archive_name_found = wildcard_path(&root, source.split('/').next().unwrap_or(""));
            if !crate::engine::is_archive(&archive.to_string_lossy()) {
                reasons.push("Not an archive".into());
                continue;
            }
            // MatchArchivePath treats an .exe as a successful sentinel; it is
            // neither opened as a process nor launched by basic validation.
            if archive
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
            {
                found = true;
                break;
            }
            let members = match self.members(archive) {
                Ok(Ok(members)) => members,
                Ok(Err(error)) => {
                    reasons.push(format!(
                        "Could not open archive (read-only inspection: {error})"
                    ));
                    continue;
                }
                Err(error) => {
                    check_cancelled(self.cancelled)?;
                    reasons.push(format!("Unsupported read-only archive inspection: {error}"));
                    continue;
                }
            };
            let mut matched = wildcard_path(&root, &source);
            for member in members {
                check_cancelled(self.cancelled)?;
                let joined = format!("{root}/{}", member.path);
                if wildcard_path(&joined, &source)
                    // C# uses an explicit directory's own path as its folder;
                    // only file entries contribute their immediate parent.
                    || !member.is_directory && joined
                        .rsplit_once('/')
                        .is_some_and(|(parent, _)| wildcard_path(parent, &source))
                {
                    matched = true;
                    break;
                }
            }
            if matched {
                found = true;
                break;
            }
        }
        // A prior unreadable archive still contributes an error if a later
        // archive matches. Plain NotFound results only contribute the final miss.
        if !reasons.is_empty() {
            outcome.errors.push(format!(
                "Invalid source path '{source}'. Reason: {}\n",
                reasons.join("\n")
            ));
            return Ok((false, archive_name_found));
        } else if !found && should_run(instruction, self.guide) {
            outcome
                .errors
                .push(format!("Failed to find '{source}' in any archives!"));
            return Ok((false, archive_name_found));
        }
        Ok((true, true))
    }

    fn destinations(
        &mut self,
        component: &Component,
        outcome: &mut ComponentOutcome,
    ) -> Result<()> {
        for instruction in instructions(component) {
            check_cancelled(self.cancelled)?;
            let destination = &instruction.destination;
            let error = match instruction.action.as_str() {
                "Unset" => None,
                "Patcher" if destination.is_empty() => {
                    outcome.warnings.push("Destination must be <<gameDirectory>> with 'Patcher' action, setting it now automatically.".into());
                    // Reference wording retained; this read-only API performs
                    // no rewrite. The warning describes the C# automatic fix.
                    None
                }
                "Patcher"
                    if destination.eq_ignore_ascii_case("<<gameDirectory>>")
                        || destination.eq_ignore_ascii_case("<<kotorDirectory>>") =>
                {
                    None
                }
                "Patcher" => Some(format!(
                    "'Destination' key must be either null or string literal '<<gameDirectory>>' (legacy: '<<kotorDirectory>>') for this action. Got '{destination}'"
                )),
                "Choose" | "Extract" | "Delete" if destination.is_empty() => None,
                "Choose" | "Extract" | "Delete" => Some(format!(
                    "'Destination' key cannot be used with this action. Got '{destination}'"
                )),
                "Rename" if destination.eq_ignore_ascii_case("<<gameDirectory>>/Override") => {
                    Some(format!(
                        "Incorrect 'Destination' format. Got '{destination}', expected a filename."
                    ))
                }
                "Rename" | "Run" | "Execute" => None,
                "Move" | "Copy" | "DelDuplicate" | "CleanList" => {
                    let path = formatted(&replace_null_roots(destination));
                    if path.trim().is_empty() || path.contains(['\0', '<', '>', '|', '"']) {
                        Some(format!("Destination cannot be found! Got '{path}'"))
                    } else {
                        match self.checked_path(&path) {
                            Ok(path_on_disk) if path_on_disk.is_dir() => None,
                            Ok(path_on_disk) => match self.case_path(&path_on_disk) {
                                Ok(alias) if alias.is_dir() => None,
                                Ok(_) => Some(format!("Destination cannot be found! Got '{path}'")),
                                Err(error) => {
                                    check_cancelled(self.cancelled)?;
                                    Some(format!(
                                        "Unsupported read-only destination check '{path}': {error}"
                                    ))
                                }
                            },
                            Err(error) => {
                                check_cancelled(self.cancelled)?;
                                Some(format!(
                                    "Unsupported read-only destination check '{path}': {error}"
                                ))
                            }
                        }
                    }
                }
                action => Some(format!("Unsupported basic validation action '{action}'")),
            };
            if let Some(error) = error {
                outcome.errors.push(error);
                outcome.is_valid = false;
            }
        }
        Ok(())
    }
}
