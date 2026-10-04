//! Read-only archive ownership for run-scoped external choices.
use crate::paths::{PathPolicy, Roots};
use anyhow::{Context, Result, ensure};
use std::{
    cell::Cell,
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

pub(super) struct Inventory<'a> {
    roots: &'a Roots,
    policy: PathPolicy,
    cancelled: &'a AtomicBool,
    entries: BTreeMap<String, (PathBuf, bool)>,
    work: Cell<usize>,
}
impl<'a> Inventory<'a> {
    pub(super) fn read(
        roots: &'a Roots,
        case_insensitive: bool,
        cancelled: &'a AtomicBool,
    ) -> Result<Self> {
        let canonical = Roots::new(&roots.mods, &roots.game)?;
        ensure!(
            canonical.mods == roots.mods && canonical.game == roots.game,
            "FOMOD choice roots must be canonical"
        );
        let mut inventory = Self {
            roots,
            policy: PathPolicy::new(case_insensitive)?,
            cancelled,
            entries: BTreeMap::new(),
            work: Cell::new(0),
        };
        let mut bytes = 0usize;
        for root in [&roots.mods, &roots.game] {
            let walk = walkdir::WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                .filter_entry(|entry| {
                    root != &roots.mods
                        || entry.depth() == 0
                        || !(entry.file_type().is_dir()
                            && crate::download_orchestrator::is_peer_staging(entry.file_name()))
                });
            for entry in walk {
                inventory.check_cancelled()?;
                let entry = entry?;
                ensure!(
                    inventory.entries.len() < 100_000 && entry.depth() <= 64,
                    "FOMOD choice inventory exceeds entry/depth limits"
                );
                let path = entry.path();
                let text = path
                    .to_str()
                    .context("FOMOD choice paths require UTF-8 identity")?;
                bytes = bytes
                    .checked_add(text.len())
                    .context("FOMOD choice inventory overflow")?;
                ensure!(
                    text.len() <= 16_384 && bytes <= 32 * 1024 * 1024,
                    "FOMOD choice inventory exceeds path limits"
                );
                roots.check(path)?;
                let meta = std::fs::symlink_metadata(path)?;
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    ensure!(
                        meta.file_attributes()
                            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                            == 0,
                        "FOMOD choice inventory rejects reparse points"
                    );
                }
                ensure!(
                    meta.is_file() || meta.is_dir(),
                    "FOMOD choice inventory rejects symlinks and special files"
                );
                ensure!(
                    inventory
                        .entries
                        .insert(
                            inventory.policy.key(path),
                            (path.to_path_buf(), meta.is_dir())
                        )
                        .is_none(),
                    "FOMOD choice inventory contains ambiguous path aliases"
                );
            }
        }
        Ok(inventory)
    }

    pub(super) fn check_cancelled(&self) -> Result<()> {
        ensure!(
            !self.cancelled.load(Ordering::Acquire),
            "Cancelled while resolving FOMOD choices"
        );
        Ok(())
    }

    pub(super) fn references(&self, component: &crate::Component, archive: &str) -> Result<bool> {
        self.check_cancelled()?;
        let filename = super::archive_name(archive);
        let mut relevant = super::resource_matches_archive(component, archive);
        let mut matched_path: Option<&str> = None;
        for source in component
            .instructions
            .iter()
            .filter(|i| i.action == "Extract")
            .flat_map(|i| &i.source)
        {
            self.check_cancelled()?;
            ensure!(
                source.len() <= 16_384,
                "FOMOD Extract source exceeds path limit"
            );
            let path = self.roots.resolve(source, true)?;
            let root = [&self.roots.mods, &self.roots.game]
                .into_iter()
                .find(|root| path.starts_with(root))
                .context("FOMOD source is outside roots")?;
            self.policy.check_spelling(root, &path, self.cancelled)?;
            // Preserve the rootless overlay's ordinary-folder/glob ownership:
            // earlier instructions may create matching archives in the virtual tree.
            let last = source
                .trim()
                .trim_end_matches(['/', '\\'])
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or("");
            if !crate::engine::is_archive(last) && super::source_may_extract(source, &filename) {
                relevant = true;
            }
            let key = self.policy.key(&path);
            if !self
                .entries
                .get(&key)
                .is_some_and(|(_, directory)| *directory)
                && super::source_may_extract(source, &filename)
            {
                relevant = true;
            }
            let prefix = format!("{key}/");
            let pattern = glob::Pattern::new(&format!(
                "{}/{}",
                glob::Pattern::escape(&self.policy.key(root)),
                crate::engine::source_wildcard_glob(&self.policy.key(path.strip_prefix(root)?))
            ))?;
            // Preserve explicit literal archive ownership when the input is missing.
            // A verified directory, even with an archive suffix, takes only its real members.
            // `[` and `]` are literal Source characters (C# WildcardPathMatch).
            if !source.contains(['*', '?'])
                && !self
                    .entries
                    .get(&key)
                    .is_some_and(|(_, directory)| *directory)
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| super::eq_ignore_case(name, &filename))
            {
                relevant = true;
            }
            for (candidate, (physical, directory)) in &self.entries {
                self.check_cancelled()?;
                let work = self.work.get() + 1;
                ensure!(
                    work <= 20_000_000,
                    "FOMOD choice matching exceeds work limit"
                );
                self.work.set(work);
                if *directory
                    || !physical
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| super::eq_ignore_case(name, &filename))
                {
                    continue;
                }
                // Identical to Engine::source_matches: a glob or a literal folder prefix.
                if self.policy.glob_matches(&pattern, candidate) || candidate.starts_with(&prefix) {
                    ensure!(
                        matched_path.is_none_or(|previous| previous == candidate),
                        "Multiple physical archives share a FOMOD choices filename in one component's Extract scope"
                    );
                    self.roots.check(physical)?;
                    ensure!(
                        std::fs::symlink_metadata(physical)?.is_file(),
                        "FOMOD choice archive changed type during resolution"
                    );
                    matched_path = Some(candidate);
                    relevant = true;
                }
            }
        }
        Ok(relevant)
    }
}
