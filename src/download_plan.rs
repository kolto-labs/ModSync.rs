//! Bounded read-only filename selection matching the reference download cache service.
use crate::{Component, reference_paths::pattern_matches};
use anyhow::Result;
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const MAX_ENTRIES: usize = 100_000;
const MAX_NAMES: usize = 100_000;
const MAX_DEPTH: usize = 128;
const MAX_NAME_BYTES: usize = 4096;
const MAX_RETAINED_BYTES: usize = 16 * 1024 * 1024;
const MAX_WORK: usize = 2_000_000;
const MAX_WORK_BYTES: usize = 128 * 1024 * 1024;

/// A local rejection must not authorize an unnamed provider fallback.
#[derive(Debug)]
pub(crate) struct QualificationError(&'static str);
impl std::fmt::Display for QualificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Download filename selection refused: {}", self.0)
    }
}
impl std::error::Error for QualificationError {}
fn rejected(reason: &'static str) -> anyhow::Error {
    QualificationError(reason).into()
}

#[derive(Default)]
struct Work {
    steps: usize,
    bytes: usize,
    retained: usize,
    names: usize,
}
impl Work {
    fn visit(&mut self, bytes: usize, cancelled: &AtomicBool) -> Result<()> {
        if cancelled.load(Ordering::Acquire) {
            return Err(rejected("cancelled"));
        }
        self.steps = self
            .steps
            .checked_add(1)
            .ok_or_else(|| rejected("work overflow"))?;
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or_else(|| rejected("work overflow"))?;
        if self.steps > MAX_WORK || self.bytes > MAX_WORK_BYTES {
            return Err(rejected("cumulative work bound exceeded"));
        }
        Ok(())
    }
    fn name(&mut self, name: &str, cancelled: &AtomicBool) -> Result<()> {
        self.visit(name.len(), cancelled)?;
        if name.len() > MAX_NAME_BYTES {
            return Err(rejected("filename or Extract pattern exceeds 4096 bytes"));
        }
        Ok(())
    }
    fn retain(&mut self, bytes: usize) -> Result<()> {
        self.retained = self
            .retained
            .checked_add(bytes)
            .ok_or_else(|| rejected("retained text overflow"))?;
        if self.retained > MAX_RETAINED_BYTES {
            return Err(rejected("retained text exceeds 16 MiB"));
        }
        Ok(())
    }
    fn key(&mut self, name: &str, cancelled: &AtomicBool) -> Result<String> {
        self.name(name, cancelled)?;
        // One mapped scalar; qualify its bounded temporary allocation first.
        self.visit(name.len().saturating_mul(4), cancelled)?;
        Ok(crate::unicode_case::ordinal_key(name))
    }
    fn insert(
        &mut self,
        names: &mut BTreeMap<String, String>,
        name: &str,
        cancelled: &AtomicBool,
    ) -> Result<()> {
        let key = self.key(name, cancelled)?;
        if let std::collections::btree_map::Entry::Vacant(entry) = names.entry(key) {
            self.names += 1;
            if self.names > MAX_NAMES {
                return Err(rejected("unique filename bound exceeded"));
            }
            self.retain(entry.key().len() + name.len())?;
            entry.insert(name.to_owned());
        }
        Ok(())
    }
    fn matches(&mut self, name: &str, source: &str, cancelled: &AtomicBool) -> Result<bool> {
        self.visit((name.len() + source.len()).saturating_mul(4), cancelled)?;
        let result = matches(name, source);
        self.visit(0, cancelled)?;
        Ok(result)
    }
}

fn files<'a>(component: &'a Component, url: &str) -> Option<&'a Map<String, Value>> {
    component
        .resource_registry
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(url))
        .and_then(|(_, metadata)| metadata.get("Files"))
        .and_then(Value::as_object)
}
pub fn url_enabled(component: &Component, url: &str) -> bool {
    if url.trim().is_empty() {
        return true;
    }
    files(component, url).is_none_or(|files| {
        files.is_empty() || files.values().any(|flag| flag != &Value::Bool(false))
    })
}
fn filename(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
}
fn matches(name: &str, source: &str) -> bool {
    let pattern = source
        .replace("<<modDirectory>>\\", "")
        .replace("<<modDirectory>>/", "");
    let pattern = pattern.trim_start_matches(['\\', '/']);
    pattern_matches(&format!("<<modDirectory>>\\{name}"), source)
        || pattern_matches(name, filename(pattern))
}

/// One invocation owns a lazy disk snapshot and cumulative selection budgets.
/// This is filename evidence, not an integrity or publication receipt.
pub(crate) struct SelectionContext<'a> {
    source: Option<PathBuf>,
    cancelled: &'a AtomicBool,
    disk: Option<BTreeMap<String, String>>,
    work: Work,
}
impl<'a> SelectionContext<'a> {
    pub(crate) fn cancelled(&self) -> &'a AtomicBool {
        self.cancelled
    }
    pub(crate) fn new(source: Option<&Path>, cancelled: &'a AtomicBool) -> Result<Self> {
        let mut work = Work::default();
        work.visit(0, cancelled)?;
        if let Some(source) = source {
            if source.as_os_str().len() > 16 * 1024 {
                return Err(rejected("source directory path exceeds 16 KiB"));
            }
            work.retain(source.as_os_str().len())?;
        }
        Ok(Self {
            source: source.map(Path::to_owned),
            cancelled,
            disk: None,
            work,
        })
    }
    fn inventory(&mut self) -> Result<()> {
        if self.disk.is_some() {
            return Ok(());
        }
        let mut disk = BTreeMap::new();
        if let Some(authored) = &self.source {
            let source = std::path::absolute(authored)
                .map_err(|_| rejected("source directory cannot be qualified"))?;
            self.work.visit(source.as_os_str().len(), self.cancelled)?;
            let metadata = crate::checkpoint::safe_path(&source)
                .map_err(|_| rejected("source directory is unsafe or unreadable"))?;
            // Preserve source.is_dir() behavior for missing/non-directory roots.
            if metadata.is_some_and(|metadata| metadata.is_dir()) {
                let mut walk = walkdir::WalkDir::new(&source)
                    .follow_links(false)
                    .max_depth(MAX_DEPTH + 1)
                    .into_iter();
                let mut entries = 0;
                while let Some(entry) = walk.next() {
                    self.work.visit(0, self.cancelled)?;
                    let entry = entry.map_err(|_| rejected("source inventory is unreadable"))?;
                    entries += 1;
                    if entries > MAX_ENTRIES || entry.depth() > MAX_DEPTH {
                        return Err(rejected("source inventory entry or depth bound exceeded"));
                    }
                    // Shared qualification checks every ancestor and Windows reparse
                    // points. Charge that traversal before performing filesystem IO.
                    for ancestor in entry.path().ancestors() {
                        self.work
                            .visit(ancestor.as_os_str().len(), self.cancelled)?;
                    }
                    let metadata = crate::checkpoint::safe_path(entry.path())
                        .map_err(|_| {
                            rejected("source inventory contains unsafe or unreadable paths")
                        })?
                        .ok_or_else(|| rejected("source inventory changed while reading"))?;
                    if entry.depth() != 0
                        && metadata.is_dir()
                        && crate::download_orchestrator::is_peer_staging(entry.file_name())
                    {
                        walk.skip_current_dir();
                        continue;
                    }
                    let relative = entry
                        .path()
                        .strip_prefix(&source)
                        .map_err(|_| rejected("source inventory path escaped its root"))?
                        .to_str()
                        .ok_or_else(|| rejected("source inventory contains non-UTF-8 names"))?;
                    self.work.name(relative, self.cancelled)?;
                    if metadata.is_file() {
                        self.work.insert(&mut disk, relative, self.cancelled)?;
                        self.work
                            .insert(&mut disk, filename(relative), self.cancelled)?;
                    }
                }
            }
        }
        self.work.visit(0, self.cancelled)?;
        self.disk = Some(disk);
        Ok(())
    }
    pub(crate) fn select(
        &mut self,
        component: &Component,
        url: &str,
        candidates: &[String],
    ) -> Result<Vec<String>> {
        // Qualify before every return/candidate copy, including no-root/no-Extract.
        self.work.visit(url.len(), self.cancelled)?;
        let mut patterns = Vec::new();
        let mut has_extract = false;
        for instruction in &component.instructions {
            self.qualify_instruction(instruction, &mut has_extract, &mut patterns)?;
        }
        for option in &component.options {
            self.work.visit(0, self.cancelled)?;
            for instruction in &option.instructions {
                self.qualify_instruction(instruction, &mut has_extract, &mut patterns)?;
            }
        }
        for name in candidates {
            self.work.name(name, self.cancelled)?;
        }
        let mut available = BTreeMap::new();
        for (key, metadata) in &component.resource_registry {
            self.work.visit(key.len(), self.cancelled)?;
            if let Some(files) = metadata.get("Files").and_then(Value::as_object) {
                for name in files.keys() {
                    self.work.name(name, self.cancelled)?;
                    if !name.trim().is_empty() {
                        self.work.insert(&mut available, name, self.cancelled)?;
                        self.work
                            .insert(&mut available, filename(name), self.cancelled)?;
                    }
                }
            }
        }
        if url.trim().is_empty() || candidates.is_empty() {
            return Ok(Vec::new());
        }
        let explicit = files(component, url).filter(|files| !files.is_empty());
        if explicit.is_some() && self.source.is_none() {
            return Err(rejected("MainConfig.SourcePath not set"));
        }
        if self.source.is_none() {
            let mut result = Vec::new();
            for name in candidates {
                self.work.visit(name.len(), self.cancelled)?;
                self.work.retain(name.len())?;
                result.push(name.clone());
            }
            return Ok(result);
        }
        let mut unsatisfied = Vec::new();
        if has_extract {
            self.inventory()?;
            for pattern in patterns {
                let mut satisfied = false;
                for name in available.values().chain(
                    self.disk
                        .as_ref()
                        .into_iter()
                        .flat_map(|disk| disk.values()),
                ) {
                    if self.work.matches(name, &pattern, self.cancelled)? {
                        satisfied = true;
                        break;
                    }
                }
                if !satisfied {
                    unsatisfied.push(pattern);
                }
            }
        }
        let mut explicit_keys = BTreeMap::new();
        if let Some(files) = explicit {
            for (name, value) in files {
                let key = self.work.key(name, self.cancelled)?;
                if let std::collections::btree_map::Entry::Vacant(entry) = explicit_keys.entry(key)
                {
                    self.work.retain(entry.key().len())?;
                    entry.insert(value != &Value::Bool(false));
                }
            }
        }
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        for name in candidates {
            let key = self.work.key(name, self.cancelled)?;
            let known = explicit_keys.get(&key).copied();
            let mut allowed = known.unwrap_or(!has_extract);
            if known.is_none() && has_extract && !name.trim().is_empty() {
                for pattern in &unsatisfied {
                    if self.work.matches(name, pattern, self.cancelled)? {
                        allowed = true;
                        break;
                    }
                }
            }
            if allowed && (explicit.is_some() || !has_extract || !seen.contains(&key)) {
                // LINQ Where preserves explicit duplicates; inferred names use a
                // reference ordinal-ignore-case HashSet, retaining first spelling.
                self.work.retain(name.len())?;
                result.push(name.clone());
                if explicit.is_none() && has_extract {
                    self.work.retain(key.len())?;
                    seen.insert(key);
                }
            }
        }
        self.work.visit(0, self.cancelled)?;
        Ok(result)
    }
    fn qualify_instruction(
        &mut self,
        instruction: &crate::Instruction,
        has_extract: &mut bool,
        patterns: &mut Vec<String>,
    ) -> Result<()> {
        self.work.visit(0, self.cancelled)?;
        if instruction.action == "Extract" {
            *has_extract = true;
            for source in &instruction.source {
                self.work.name(source, self.cancelled)?;
                if !source.trim().is_empty() {
                    self.work.retain(source.len())?;
                    patterns.push(source.clone());
                }
            }
        }
        Ok(())
    }
}

pub fn select_filenames(
    component: &Component,
    url: &str,
    candidates: &[String],
    source: Option<&Path>,
) -> Result<Vec<String>> {
    select_filenames_with_cancel(component, url, candidates, source, &AtomicBool::new(false))
}
/// Bounded cancellable selection; the original entry point remains available.
pub fn select_filenames_with_cancel(
    component: &Component,
    url: &str,
    candidates: &[String],
    source: Option<&Path>,
    cancelled: &AtomicBool,
) -> Result<Vec<String>> {
    SelectionContext::new(source, cancelled)?.select(component, url, candidates)
}
