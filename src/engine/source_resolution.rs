//! Nonrecursive missing-source recovery against an owned, unpublished VFS overlay.
use super::*;
use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::atomic::Ordering,
};

const ENTRIES: usize = 100_000;
const NAME_BYTES: usize = 16 * 1024 * 1024;
const WORK: usize = 2_000_000;
const RECOVERY_BYTES: u64 = 8 * 1024 * 1024 * 1024;

struct Budget {
    cancelled: Arc<AtomicBool>,
    visits: usize,
    names: usize,
    members: usize,
    recovery_remaining: u64,
    digits: Option<BTreeSet<u16>>,
}
impl Budget {
    fn new(engine: &Engine) -> Self {
        Self {
            cancelled: engine.settings.cancelled.clone(),
            visits: 0,
            names: 0,
            members: 0,
            recovery_remaining: engine
                .settings
                .archive
                .max_expanded_bytes
                .unwrap_or(RECOVERY_BYTES),
            digits: None,
        }
    }
    fn visit(&mut self) -> Result<()> {
        ensure!(
            !self.cancelled.load(Ordering::Acquire),
            "Source resolution cancelled"
        );
        self.visits += 1;
        ensure!(
            self.visits <= WORK,
            "Source resolution exceeds 2 million visits"
        );
        Ok(())
    }
    fn name(&mut self, name: &str) -> Result<()> {
        self.visit()?;
        ensure!(
            name.len() <= 16 * 1024,
            "Source resolution path exceeds 16 KiB"
        );
        self.names = self
            .names
            .checked_add(name.len())
            .context("Source name budget overflow")?;
        ensure!(
            self.names <= NAME_BYTES,
            "Source resolution names exceed 16 MiB"
        );
        Ok(())
    }
    fn path(&mut self, path: &Path) -> Result<()> {
        let text = path
            .to_str()
            .context("Source resolution requires Unicode paths")?;
        self.name(text)?;
        ensure!(
            path.components().count() <= 128,
            "Source resolution path exceeds depth 128"
        );
        Ok(())
    }
    fn digit(&mut self, c: char) -> Result<bool> {
        if c.is_ascii_digit() {
            return Ok(true);
        }
        if c as u32 > 0xffff {
            return Ok(false);
        }
        if self.digits.is_none() {
            // Consume the retained primary dataset directly; do not regenerate
            // another casing/property table or depend on Rust's Unicode version.
            let mut digits = BTreeSet::new();
            for row in include_str!("../maintenance/casing/UnicodeData-15.0.0.txt").lines() {
                self.visit()?;
                let mut fields = row.split(';');
                let code = fields
                    .next()
                    .context("Retained Unicode data is malformed")?;
                let _name = fields.next();
                if fields.next() == Some("Nd") {
                    let code = u32::from_str_radix(code, 16)?;
                    if code <= 0xffff {
                        digits.insert(code as u16);
                    }
                }
            }
            self.digits = Some(digits);
        }
        Ok(self
            .digits
            .as_ref()
            .is_some_and(|digits| digits.contains(&(c as u16))))
    }
}

struct Write {
    path: PathBuf,
    content: Content,
    overwrite: bool,
}
pub(super) struct Resolution {
    pub(super) sources: Vec<PathBuf>,
    writes: Vec<Write>,
    pub(super) satisfied: bool,
}

impl Engine {
    /// Binds an Extract/Copy/Move/Patcher source. A direct hit keeps the
    /// ordinary action path unchanged; only a typed miss enters recovery.
    pub(super) fn resolve_instruction(
        &mut self,
        component: &Component,
        instruction: &Instruction,
    ) -> Result<Resolution> {
        let patcher = instruction.action == "Patcher";
        let direct = if patcher {
            ensure!(
                instruction.source.len() == 1,
                "Patcher needs exactly one source to bind its configuration and logs"
            );
            let path = self.actual(&self.path(&instruction.source[0], false)?);
            let key = self.path_policy.key(&path);
            if self.files.contains_key(&key) || self.directories.contains_key(&key) {
                Ok(vec![path])
            } else {
                Err(MissingSource(instruction.source[0].clone()).into())
            }
        } else if instruction.source.is_empty() {
            // C# SetRealPaths finds no files and throws; nothing can be recovered,
            // so the action fails as a missing source (FileNotFoundPre).
            return Err(MissingSourceFiles(anyhow::anyhow!(
                "{} has no Source",
                instruction.action
            ))
            .into());
        } else {
            // A miss in one pattern must not hide an unsafe/malformed later pattern.
            for source in &instruction.source {
                self.source_pattern(source)?;
            }
            self.matches_scoped(
                &instruction.source,
                false,
                authored_descendants(&instruction.action),
            )
        };
        let sources = match direct {
            Ok(sources) => sources,
            Err(error) if error.is::<MissingSource>() => {
                // A Patcher miss stays typed: C# returns FileNotFoundPre, which the
                // dry run reports as DryRunSkipped, as for Copy/Move/Extract.
                return self.recover(component, instruction).map_err(|error| {
                    if patcher && error.is::<MissingSourceFiles>() {
                        MissingSourceFiles(anyhow::anyhow!(
                            "Missing external program/mod: {}",
                            instruction.source[0]
                        ))
                        .into()
                    } else {
                        error
                    }
                });
            }
            Err(error) => return Err(error),
        };
        if !patcher {
            for path in &sources {
                // Reading must be possible even when virtual copying needs no bytes.
                let content = &self.files[&self.path_policy.key(path)].1;
                if !matches!(content, Content::Unknown) || self.install {
                    content.open()?;
                }
            }
        }
        Ok(Resolution {
            sources,
            writes: Vec::new(),
            satisfied: false,
        })
    }

    /// Recovery for a typed miss. Every authored argument is qualified first;
    /// all derived writes stay in a shadow overlay until the caller publishes.
    fn recover(&mut self, component: &Component, instruction: &Instruction) -> Result<Resolution> {
        let mut budget = Budget::new(self);
        ensure!(
            matches!(
                instruction.action.as_str(),
                "Extract" | "Copy" | "Move" | "Patcher"
            ),
            "Unsupported recovery action"
        );
        ensure!(
            !instruction.source.is_empty() && instruction.source.len() <= ENTRIES,
            "Source resolution needs 1..100,000 sources"
        );
        for raw in &instruction.source {
            budget.name(raw)?;
            self.source_pattern(raw)?;
        }
        if !instruction.destination.is_empty() {
            self.path(&instruction.destination, false)?;
        }
        budget.name(&instruction.arguments)?;
        if instruction.action == "Patcher" {
            ensure!(
                instruction.source.len() == 1,
                "Patcher needs exactly one source binding"
            );
            // `[`/`]` are literal Source characters, but a literal path still may
            // not contain them (`paths::relative`), so they stay refused here.
            ensure!(
                !instruction.source[0].contains(['*', '?', '[', ']']),
                "Patcher source must bind one literal file or directory"
            );
            if self.install {
                ensure!(
                    self.settings.allow_processes,
                    "Patcher needs explicit trust: enable external programs (--allow-processes)"
                );
                ensure!(
                    self.patcher_invocation.is_some(),
                    "Configure an executable with --patcher or install complete packaged patcher resources"
                );
            }
            let dest = if instruction.destination.is_empty() {
                self.settings.roots.game.clone()
            } else {
                self.path(&instruction.destination, false)?
            };
            ensure!(
                self.path_policy.key(&dest) == self.path_policy.key(&self.settings.roots.game),
                "Patcher destination must be the game directory"
            );
            let args = instruction.arguments.trim();
            if !args.is_empty() && args.parse::<usize>().is_err() {
                let name = relative(args, false)?;
                ensure!(
                    name.components().count() == 1
                        && name
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("ini")),
                    "Patcher Arguments must be an INI filename or namespace index"
                );
            }
        } else if matches!(instruction.action.as_str(), "Copy" | "Move") {
            self.path(&instruction.destination, false)?;
        }
        // Admit the entire snapshot before copying it, including directory names.
        ensure!(
            self.files.len().saturating_add(self.directories.len()) <= ENTRIES,
            "Source resolution inventory exceeds 100,000 entries"
        );
        for (path, _) in self.files.values() {
            budget.path(path)?;
        }
        for path in self.directories.values() {
            budget.path(path)?;
        }
        let files = self.files.clone();
        let directories = self.directories.clone();
        let original_files = std::mem::replace(&mut self.files, files);
        let original_directories = std::mem::replace(&mut self.directories, directories);
        // Read-only helpers inspect the shadow maps. Restore them even on unwind;
        // owned scratch entries drop without publishing into either live root.
        let result = catch_unwind(AssertUnwindSafe(|| {
            self.plan_resolution(component, instruction, &mut budget)
        }));
        self.files = original_files;
        self.directories = original_directories;
        match result {
            Ok(result) => result,
            Err(panic) => resume_unwind(panic),
        }
    }

    /// `descendants` false (authored Copy/Move, see [`authored_descendants`]): a
    /// Source names only the files its literal or wildcard path matches.
    fn recovery_matches(
        &self,
        sources: &[String],
        directory: bool,
        descendants: bool,
        budget: &mut Budget,
    ) -> Result<Vec<PathBuf>> {
        let mut result = BTreeMap::new();
        let mut missing = None;
        for raw in sources {
            budget.visit()?;
            let (path, pattern) = self.source_pattern(raw)?;
            let key = self.path_policy.key(&path);
            // A bare root Source names no file or folder (see `source_prefix`).
            if directory
                && !self.settings.roots.is_root(&path)
                && (self.files.contains_key(&key) || self.directories.contains_key(&key))
            {
                result.insert(key, self.actual(&path));
                continue;
            }
            let prefix = self.source_prefix(&path);
            let mut found = false;
            for (key, (path, content)) in &self.files {
                budget.visit()?;
                let matched = if descendants {
                    self.source_matches(prefix.as_deref(), &pattern, key)
                } else {
                    self.path_policy.glob_matches(&pattern, key)
                };
                if matched {
                    self.settings.roots.check(path)?;
                    if !matches!(content, Content::Unknown) || self.install {
                        content.open()?;
                    }
                    result.insert(key.clone(), path.clone());
                    found = true;
                }
            }
            if !found && missing.is_none() {
                missing = Some(raw.clone());
            }
        }
        if let Some(raw) = missing {
            return Err(MissingSource(raw).into());
        }
        Ok(result.into_values().collect())
    }

    fn plan_resolution(
        &mut self,
        component: &Component,
        instruction: &Instruction,
        budget: &mut Budget,
    ) -> Result<Resolution> {
        let mut writes = Vec::new();
        let mut produced = BTreeSet::new();
        let directory = instruction.action == "Patcher";
        let descendants = authored_descendants(&instruction.action);
        let direct = self.recovery_matches(&instruction.source, directory, descendants, budget);
        let sources = match direct {
            Ok(sources) => sources,
            Err(error) if error.is::<MissingSource>() => {
                if self.plan_auto_extract(
                    component,
                    instruction,
                    budget,
                    &mut writes,
                    &mut produced,
                )? {
                    match self.recovery_matches(&instruction.source, directory, descendants, budget)
                    {
                        Ok(sources) => sources,
                        Err(error) if error.is::<MissingSource>() => {
                            self.descendants(instruction, budget)?
                        }
                        Err(error) => return Err(error),
                    }
                } else {
                    let variant = self.move_variant(instruction, budget)?;
                    match self.recovery_matches(&variant, directory, descendants, budget) {
                        Ok(sources) => sources,
                        Err(error) if error.is::<MissingSource>() => {
                            if self.redundant_move(instruction, budget)? {
                                return Ok(Resolution {
                                    sources: Vec::new(),
                                    writes,
                                    satisfied: true,
                                });
                            }
                            return Err(MissingSourceFiles(error).into());
                        }
                        Err(error) => return Err(error),
                    }
                }
            }
            Err(error) => return Err(error),
        };
        // A recovered Extract binding is extracted by the ordinary Extract path
        // after publication, with its per-archive configured options.
        if instruction.action != "Extract" {
            self.qualify_action(instruction, &sources, budget)?;
        }
        Ok(Resolution {
            sources,
            writes,
            satisfied: false,
        })
    }

    fn stage_write(
        &mut self,
        path: PathBuf,
        content: Content,
        overwrite: bool,
        budget: &mut Budget,
        writes: &mut Vec<Write>,
        produced: &mut BTreeSet<String>,
    ) -> Result<()> {
        budget.path(&path)?;
        self.settings.roots.check(&path)?;
        let path = self.actual(&path);
        let key = self.path_policy.key(&path);
        ensure!(
            !self.directories.contains_key(&key),
            "Source recovery file collides with a directory"
        );
        ensure!(
            produced.insert(key.clone()),
            "Source recovery has colliding derived write targets; use explicit ordered instructions"
        );
        if self.files.contains_key(&key) && !overwrite {
            return Ok(());
        }
        let mut parent = path.parent();
        while let Some(directory) = parent {
            budget.visit()?;
            ensure!(
                !self.files.contains_key(&self.path_policy.key(directory)),
                "Source recovery file blocks a target directory"
            );
            self.directories
                .insert(self.path_policy.key(directory), directory.to_owned());
            if directory == self.settings.roots.mods || directory == self.settings.roots.game {
                break;
            }
            parent = directory.parent();
        }
        ensure!(
            self.files.len().saturating_add(self.directories.len()) < ENTRIES,
            "Source recovery overlay exceeds 100,000 entries"
        );
        self.files.insert(key, (path.clone(), content.clone()));
        writes.push(Write {
            path,
            content,
            overwrite,
        });
        Ok(())
    }

    fn plan_extract(
        &mut self,
        component: &Component,
        instruction: &Instruction,
        source: &Path,
        budget: &mut Budget,
        writes: &mut Vec<Write>,
        produced: &mut BTreeSet<String>,
    ) -> Result<()> {
        budget.path(source)?;
        let content = self
            .files
            .get(&self.path_policy.key(source))
            .context("Resolved archive vanished from VFS")?
            .1
            .clone();
        let dest = self.extraction_destination(instruction, source)?;
        self.settings.roots.check(&dest)?;
        ensure!(
            !self.files.contains_key(&self.path_policy.key(&dest)),
            "File blocks extraction destination"
        );
        // Implicit recovery shares one aggregate allowance: the configured
        // --max-expanded-bytes when present, otherwise 8 GiB.
        let mut options = self.settings.archive.clone();
        options.max_expanded_bytes = Some(budget.recovery_remaining);
        let entries = archive::unpack(content.path()?, source, &options, &budget.cancelled)?;
        let mut bytes = 0u64;
        for entry in &entries {
            budget.visit()?;
            bytes = bytes
                .checked_add(entry.file.as_file().metadata()?.len())
                .context("Aggregate archive size overflow")?;
        }
        budget.recovery_remaining = budget.recovery_remaining.checked_sub(bytes).context(
            "Implicit source recovery exceeds aggregate archive allowance; use explicit Extract or --max-expanded-bytes",
        )?;
        budget.members = budget
            .members
            .checked_add(entries.len())
            .context("Archive member count overflow")?;
        ensure!(
            budget.members <= ENTRIES,
            "Source recovery exceeds 100,000 archive members"
        );
        let configured = if let Some(mut module) =
            crate::fomod::inspect_entries_with_cancel(&entries, &budget.cancelled)?
        {
            let filename = source
                .file_name()
                .and_then(|name| name.to_str())
                .context("FOMOD archive filename is invalid")?;
            let choices = crate::fomod::required_saved_choices(component, filename)?;
            let relative = dest
                .strip_prefix(&self.settings.roots.mods)
                .context("Configured FOMOD extraction must stay in the mod directory")?;
            choices
                .apply_to(&mut module.config.steps)
                .and_then(|()| module.copy_instructions(&relative.to_string_lossy()))
                .with_context(|| crate::fomod::stale_choices_context(filename))?
        } else {
            Vec::new()
        };
        let archive_paths: BTreeSet<_> = entries
            .iter()
            .map(|entry| self.path_policy.key(&dest.join(&entry.path)))
            .collect();
        if !configured.is_empty() && !instruction.overwrite {
            ensure!(
                archive_paths
                    .iter()
                    .all(|path| !self.files.contains_key(path)),
                "Configured FOMOD extraction cannot retain existing files with Overwrite=false"
            );
        }
        for entry in entries {
            self.stage_write(
                dest.join(entry.path),
                Content::Scratch(entry.file, entry.mode, entry.modified),
                instruction.overwrite,
                budget,
                writes,
                produced,
            )?;
        }
        for copy in configured {
            ensure!(
                copy.action == "Copy",
                "Configured FOMOD produced an unsupported action"
            );
            for raw in &copy.source {
                self.source_pattern(raw)?;
            }
            // As in the explicit Extract path, which runs these through the Copy
            // arm: the mapper writes folders as `folder/*`, so files only.
            let sources =
                self.recovery_matches(&copy.source, false, authored_descendants("Copy"), budget)?;
            let target = self.path(&copy.destination, false)?;
            for source in sources {
                budget.visit()?;
                ensure!(
                    archive_paths.contains(&self.path_policy.key(&source)),
                    "FOMOD source is not supplied by the current archive"
                );
                let content = self.files[&self.path_policy.key(&source)].1.clone();
                self.stage_write(
                    target.join(source.file_name().context("FOMOD source has no filename")?),
                    content,
                    copy.overwrite,
                    budget,
                    writes,
                    produced,
                )?;
            }
        }
        Ok(())
    }

    fn plan_auto_extract(
        &mut self,
        component: &Component,
        instruction: &Instruction,
        budget: &mut Budget,
        writes: &mut Vec<Write>,
        produced: &mut BTreeSet<String>,
    ) -> Result<bool> {
        // Inspect every missing source before deciding: a later ambiguity,
        // unsafe registry identity or absent archive must still surface.
        let mut archives = BTreeMap::new();
        let mut unavailable = false;
        for raw in &instruction.source {
            budget.visit()?;
            match self.recovery_matches(
                std::slice::from_ref(raw),
                instruction.action == "Patcher",
                authored_descendants(&instruction.action),
                budget,
            ) {
                Ok(_) => continue,
                Err(error) if error.is::<MissingSource>() => {}
                Err(error) => return Err(error),
            }
            let path = self.path(raw, true)?;
            let missing = path
                .file_name()
                .and_then(|name| name.to_str())
                .context("Missing source has no filename")?;
            let mut owners = BTreeMap::new();
            for (resource, metadata) in &component.resource_registry {
                budget.name(resource)?;
                if let Some(files) = metadata.get("Files").and_then(serde_json::Value::as_object) {
                    for (entry, flag) in files {
                        budget.name(entry)?;
                        if flag == &serde_json::Value::Bool(true)
                            && registry_matches(entry, missing, budget)?
                        {
                            // Reference uses the registry KEY, not a URL basename
                            // nor any Files key, as the archive identity.
                            let relative = relative(resource, false).map_err(|_| anyhow::anyhow!("ResourceRegistry key is not a qualified local archive path; add explicit Extract"))?;
                            ensure!(
                                relative
                                    .to_str()
                                    .is_some_and(|name| !name.is_empty() && is_archive(name)),
                                "ResourceRegistry archive identity is unsupported; add explicit Extract"
                            );
                            let archive = self.settings.roots.mods.join(relative);
                            self.settings.roots.check(&archive)?;
                            owners.insert(crate::unicode_case::ordinal_key(resource), archive);
                        }
                    }
                }
            }
            if owners.is_empty() {
                unavailable = true;
                continue;
            }
            ensure!(
                owners.len() == 1,
                "Missing source is declared by multiple archives; add explicit Extract"
            );
            for (key, archive) in owners {
                ensure!(
                    self.files.contains_key(&self.path_policy.key(&archive)),
                    "Declared source archive is missing; acquire it or add explicit Extract"
                );
                archives.insert(key, archive);
            }
        }
        if unavailable || archives.is_empty() {
            return Ok(false);
        }
        let extract = Instruction {
            guid: String::new(),
            parent: String::new(),
            action: "Extract".into(),
            source: Vec::new(),
            destination: String::new(),
            arguments: String::new(),
            overwrite: true,
            dependencies: Vec::new(),
            restrictions: Vec::new(),
        };
        for source in archives.into_values() {
            self.plan_extract(component, &extract, &source, budget, writes, produced)?;
        }
        Ok(true)
    }

    fn descendants(&self, instruction: &Instruction, budget: &mut Budget) -> Result<Vec<PathBuf>> {
        let mut result = BTreeMap::new();
        for raw in &instruction.source {
            budget.visit()?;
            match self.recovery_matches(
                std::slice::from_ref(raw),
                instruction.action == "Patcher",
                authored_descendants(&instruction.action),
                budget,
            ) {
                Ok(paths) => {
                    for path in paths {
                        result.insert(self.path_policy.key(&path), path);
                    }
                    continue;
                }
                Err(error) if error.is::<MissingSource>() => {}
                Err(error) => return Err(error),
            }
            let authored = self.path(raw, true)?;
            let filename = authored
                .file_name()
                .and_then(|name| name.to_str())
                .context("Missing source has no basename")?;
            let wildcard = filename.contains(['*', '?']);
            let mut candidates = Vec::new();
            for (path, _) in self.files.values() {
                budget.visit()?;
                if !path.starts_with(&self.settings.roots.mods)
                    || (wildcard && path.parent() == Some(&self.settings.roots.mods))
                    || self.path_policy.key(path) == self.path_policy.key(&authored)
                {
                    continue;
                }
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .context("Descendant filename is not Unicode")?;
                if if wildcard {
                    ascii_wildcard(name, filename, budget)?
                } else {
                    crate::unicode_case::ordinal_eq(name, filename)
                } {
                    candidates.push(path.clone());
                }
            }
            if candidates.is_empty() {
                return Err(MissingSourceFiles(MissingSource(raw.clone()).into()).into());
            }
            ensure!(
                wildcard || candidates.len() == 1,
                "Missing source has multiple extracted descendants; add explicit paths"
            );
            for path in candidates {
                self.settings.roots.check(&path)?;
                result.insert(self.path_policy.key(&path), path);
            }
        }
        Ok(result.into_values().collect())
    }

    fn move_variant(&self, instruction: &Instruction, budget: &mut Budget) -> Result<Vec<String>> {
        if instruction.action != "Move" {
            return Ok(instruction.source.clone());
        }
        let mut result = Vec::new();
        for raw in &instruction.source {
            budget.name(raw)?;
            let Some(tail) = raw.strip_prefix("<<modDirectory>>") else {
                result.push(raw.clone());
                continue;
            };
            let tail = tail.trim_start_matches(['/', '\\']);
            let end = tail.find(['/', '\\']).unwrap_or(tail.len());
            let expected = &tail[..end];
            if expected.is_empty()
                || self.directories.contains_key(
                    &self
                        .path_policy
                        .key(&self.settings.roots.mods.join(expected)),
                )
            {
                result.push(raw.clone());
                continue;
            }
            let rest = &tail[end..];
            let expected_key = crate::unicode_case::ordinal_key(expected);
            let mut candidates = Vec::new();
            for directory in self.directories.values() {
                budget.visit()?;
                if directory.parent() != Some(&self.settings.roots.mods) {
                    continue;
                }
                let name = directory
                    .file_name()
                    .and_then(|name| name.to_str())
                    .context("Variant directory name is not Unicode")?;
                let key = crate::unicode_case::ordinal_key(name);
                let Some(suffix) = key.strip_prefix(&expected_key) else {
                    continue;
                };
                if let Some(c) = suffix.chars().next()
                    && c != '_'
                    && c != '-'
                    && !budget.digit(c)?
                {
                    continue;
                }
                let probe = rest
                    .trim_start_matches(['/', '\\'])
                    .split(['/', '\\', '*'])
                    .next()
                    .unwrap_or("");
                if !probe.is_empty()
                    && !self
                        .directories
                        .contains_key(&self.path_policy.key(&directory.join(probe)))
                {
                    continue;
                }
                self.settings.roots.check(directory)?;
                candidates.push((name.encode_utf16().count(), name.to_owned()));
            }
            candidates.sort();
            if let Some((length, name)) = candidates.first() {
                ensure!(
                    candidates
                        .get(1)
                        .is_none_or(|candidate| candidate.0 != *length),
                    "Move variant has ambiguous shortest directories; add an explicit source"
                );
                let rebuilt = format!("<<modDirectory>>/{name}{rest}");
                self.source_pattern(&rebuilt)?;
                result.push(rebuilt);
            } else {
                result.push(raw.clone());
            }
        }
        Ok(result)
    }

    fn redundant_move(&self, instruction: &Instruction, budget: &mut Budget) -> Result<bool> {
        if instruction.action != "Move"
            || !(instruction.destination.starts_with("<<gameDirectory>>")
                || instruction.destination.starts_with("<<kotorDirectory>>"))
        {
            return Ok(false);
        }
        let dest = self.path(&instruction.destination, false)?;
        if !dest.starts_with(&self.settings.roots.game)
            || !self.directories.contains_key(&self.path_policy.key(&dest))
        {
            return Ok(false);
        }
        for raw in &instruction.source {
            budget.name(raw)?;
            // A literal path may not contain `[`/`]` (`paths::relative`), so such
            // a Source is never treated as already satisfied.
            if raw.contains(['*', '?', '[', ']']) {
                return Ok(false);
            }
            let source = self.path(raw, false)?;
            let Some(name) = source.file_name() else {
                return Ok(false);
            };
            let target = dest.join(name);
            self.settings.roots.check(&target)?;
            if !self.files.contains_key(&self.path_policy.key(&target)) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn qualify_action(
        &self,
        instruction: &Instruction,
        sources: &[PathBuf],
        budget: &mut Budget,
    ) -> Result<()> {
        if instruction.action == "Patcher" {
            let source = sources.first().context("Patcher has no binding")?;
            let base = if self.files.contains_key(&self.path_policy.key(source)) {
                source.parent().context("Patcher file has no parent")?
            } else {
                source.as_path()
            };
            self.settings.roots.check(base)?;
            ensure!(
                self.directories.contains_key(&self.path_policy.key(base)),
                "Patcher base directory is missing"
            );
            ensure!(
                self.simulation_configuration(base, &instruction.arguments)?
                    .is_some(),
                "Patcher configuration is missing"
            );
        } else {
            let dest = self.path(&instruction.destination, false)?;
            ensure!(
                !self.files.contains_key(&self.path_policy.key(&dest)),
                "File blocks action destination directory"
            );
            let mut targets = BTreeSet::new();
            for source in sources {
                budget.path(source)?;
                let target = self.actual(
                    &dest.join(
                        source
                            .file_name()
                            .context("Action source has no filename")?,
                    ),
                );
                self.settings.roots.check(&target)?;
                if self.path_policy.key(source) == self.path_policy.key(&target) {
                    // As in the Copy/Move arm: with Overwrite=false the action skips
                    // this source with a warning and writes nothing for it.
                    ensure!(
                        instruction.action != "Rename" && !instruction.overwrite,
                        "Source equals destination"
                    );
                    continue;
                }
                ensure!(
                    !self
                        .directories
                        .contains_key(&self.path_policy.key(&target)),
                    "Action file collides with a directory"
                );
                ensure!(
                    targets.insert(self.path_policy.key(&target)),
                    "Action sources collide at one target; use explicit ordered instructions"
                );
                for parent in target.ancestors().skip(1) {
                    budget.visit()?;
                    ensure!(
                        !self.files.contains_key(&self.path_policy.key(parent)),
                        "File blocks action target parent"
                    );
                }
            }
        }
        Ok(())
    }

    pub(super) fn publish_resolution(&mut self, resolution: &mut Resolution) -> Result<()> {
        for write in resolution.writes.drain(..) {
            self.check_cancelled()?;
            self.put(&write.path, write.content, write.overwrite)?;
        }
        Ok(())
    }
}

fn registry_matches(entry: &str, missing: &str, budget: &mut Budget) -> Result<bool> {
    let entry = entry.replace('\\', "/");
    let missing = missing.replace('\\', "/");
    if missing.contains(['*', '?']) {
        ascii_wildcard(&entry, &missing, budget).and_then(|found| {
            if found {
                Ok(true)
            } else {
                ascii_wildcard(entry.rsplit('/').next().unwrap_or(""), &missing, budget)
            }
        })
    } else {
        let entry = crate::unicode_case::ordinal_key(&entry);
        let missing = crate::unicode_case::ordinal_key(&missing);
        Ok(entry == missing || entry.ends_with(&format!("/{missing}")))
    }
}
// Pinned reference uses Regex IgnoreCase under ambient culture, UTF-16 and
// newline-sensitive dot. Only ordinary ASCII without culture-sensitive I/i is
// admitted here; do not pretend ordinal scalar globbing implements that profile.
fn ascii_wildcard(candidate: &str, pattern: &str, budget: &mut Budget) -> Result<bool> {
    budget.name(candidate)?;
    budget.name(pattern)?;
    ensure!(
        candidate.is_ascii()
            && pattern.is_ascii()
            && !candidate.contains(['\r', '\n'])
            && !pattern.contains(['\r', '\n', 'I', 'i']),
        "Recovery wildcard requires an ordinary ASCII non-I profile; add explicit literal paths"
    );
    let candidate = candidate.to_ascii_uppercase();
    let pattern = pattern.to_ascii_uppercase();
    let (mut at, mut token, mut star, mut retry) = (0, 0, None, 0);
    let candidate = candidate.as_bytes();
    let pattern = pattern.as_bytes();
    while at < candidate.len() {
        budget.visit()?;
        if token < pattern.len() && (pattern[token] == b'?' || pattern[token] == candidate[at]) {
            at += 1;
            token += 1;
        } else if token < pattern.len() && pattern[token] == b'*' {
            star = Some(token);
            token += 1;
            retry = at;
        } else if let Some(index) = star {
            retry += 1;
            at = retry;
            token = index + 1;
        } else {
            return Ok(false);
        }
    }
    while token < pattern.len() && pattern[token] == b'*' {
        budget.visit()?;
        token += 1;
    }
    Ok(token == pattern.len())
}
