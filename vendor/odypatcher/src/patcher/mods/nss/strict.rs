//! Explicit opt-in NSS compilation from caller-certified staged inputs.
//!
//! Loading creates an immutable byte snapshot; compilation never reopens package,
//! CWD, workspace, executable-adjacent or cached files. Certification/known game
//! header authority belongs to the caller, not to filenames or this loader.

use std::collections::BTreeMap;
use std::path::Path;

use super::{
    decode_script_bytes, Game, ModificationsNSS, PResult, PatchException, PatchLogger,
    PatcherMemory,
};
pub use bionsscomp::virtual_compile::{VirtualCompileOptions, VirtualLimits, POLICY_VERSION};

fn reject(message: impl Into<String>) -> PatchException {
    PatchException::new("StrictNssError", message)
}

fn valid_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".nss") else {
        return false;
    };
    !stem.is_empty()
        && stem.len() <= 64
        && stem
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Immutable raw and deterministically decoded inputs for one flat source folder.
#[derive(Debug, Clone)]
pub struct StrictNssBundle {
    raw: BTreeMap<String, Vec<u8>>,
    text: BTreeMap<String, String>,
}

impl StrictNssBundle {
    /// Load exactly a caller-certified flat closure from an absolute staged root.
    ///
    /// `expected` must include the authenticated correct-game `nwscript.nss` bytes
    /// and every main/include. Every disk byte must equal its expected bytes.
    /// Names are lowercase logical names; one unambiguous disk case spelling is
    /// allowed. Symlinks, special files, extra entries and non-Linux hosts reject.
    /// The root and every ancestor are opened with descriptor-relative NOFOLLOW.
    /// Raw and UTF-8/CP1252-decoded sizes both obey the compiler's hard ceilings.
    pub fn load(root: &Path, expected: &BTreeMap<String, Vec<u8>>) -> PResult<Self> {
        if !cfg!(target_os = "linux") {
            return Err(reject("strict NSS staging is supported only on Linux"));
        }
        let limits = VirtualLimits::default();
        if expected.is_empty()
            || expected.len() > limits.max_files
            || !expected.contains_key("nwscript.nss")
        {
            return Err(reject(
                "explicit bounded source closure and known header required",
            ));
        }
        let mut total = 0usize;
        for (name, bytes) in expected {
            if !valid_name(name) {
                return Err(reject("invalid certified NSS filename"));
            }
            let ceiling = if name == "nwscript.nss" {
                limits.max_header_bytes
            } else {
                limits.max_source_bytes
            };
            total = total
                .checked_add(bytes.len())
                .ok_or_else(|| reject("source size overflow"))?;
            if bytes.len() > ceiling || total > limits.max_total_source_bytes {
                return Err(reject("certified raw source budget exceeded"));
            }
        }
        let raw = load_snapshot(root, expected)?;
        let mut text = BTreeMap::new();
        let mut decoded_total = 0usize;
        for (name, bytes) in &raw {
            let source = decode_script_bytes(bytes)?;
            let ceiling = if name == "nwscript.nss" {
                limits.max_header_bytes
            } else {
                limits.max_source_bytes
            };
            decoded_total = decoded_total
                .checked_add(source.len())
                .ok_or_else(|| reject("decoded size overflow"))?;
            if source.len() > ceiling || decoded_total > limits.max_total_source_bytes {
                return Err(reject("decoded source budget exceeded"));
            }
            text.insert(name.clone(), source);
        }
        Ok(Self { raw, text })
    }
}

#[cfg(target_os = "linux")]
fn load_snapshot(
    root: &Path,
    expected: &BTreeMap<String, Vec<u8>>,
) -> PResult<BTreeMap<String, Vec<u8>>> {
    use rustix::fs::{open, openat, Dir, Mode, OFlags};
    use std::fs::File;
    use std::io::Read;
    use std::path::Component;
    fn io_error(e: impl std::fmt::Display) -> PatchException {
        reject(format!("strict staged source read failed: {e}"))
    }
    if !root.is_absolute() {
        return Err(reject("absolute staged NSS root required"));
    }
    let directory_flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut directory = open("/", directory_flags, Mode::empty()).map_err(io_error)?;
    for component in root.components() {
        match component {
            Component::RootDir => (),
            Component::Normal(part) => {
                directory =
                    openat(&directory, part, directory_flags, Mode::empty()).map_err(io_error)?;
            }
            _ => return Err(reject("noncanonical staged NSS root")),
        }
    }
    let mut names = BTreeMap::new();
    let mut entries = Dir::read_from(&directory).map_err(io_error)?;
    let mut count = 0usize;
    for entry in &mut entries {
        count += 1;
        if count > VirtualLimits::default().max_files + 2 {
            return Err(reject("staged directory entry budget exceeded"));
        }
        let entry = entry.map_err(io_error)?;
        let name = entry
            .file_name()
            .to_str()
            .map_err(|_| reject("non-UTF8 staged filename"))?;
        if matches!(name, "." | "..") {
            continue;
        }
        let logical = name.to_ascii_lowercase();
        if !name.is_ascii()
            || !valid_name(&logical)
            || !expected.contains_key(&logical)
            || names.insert(logical, name.to_owned()).is_some()
        {
            return Err(reject("extra, invalid or case-ambiguous staged NSS input"));
        }
    }
    if names.len() != expected.len() {
        return Err(reject("certified staged NSS input missing"));
    }
    let mut snapshot = BTreeMap::new();
    for (logical, name) in names {
        let fd = openat(
            &directory,
            name.as_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(io_error)?;
        let file = File::from(fd);
        let metadata = file.metadata().map_err(io_error)?;
        let certified = &expected[&logical];
        if !metadata.is_file() || metadata.len() != certified.len() as u64 {
            return Err(reject("staged NSS input type or length differs"));
        }
        let mut bytes = Vec::with_capacity(certified.len());
        file.take(certified.len() as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        if bytes != *certified {
            return Err(reject("staged NSS bytes differ from certificate"));
        }
        snapshot.insert(logical, bytes);
    }
    Ok(snapshot)
}

#[cfg(not(target_os = "linux"))]
fn load_snapshot(_: &Path, _: &BTreeMap<String, Vec<u8>>) -> PResult<BTreeMap<String, Vec<u8>>> {
    Err(reject("strict NSS staging is supported only on Linux"))
}

/// Explicit compiler selection; legacy installer behavior remains the default.
#[derive(Debug, Clone)]
pub struct StrictNssPolicy {
    game: Game,
    bundles: BTreeMap<String, StrictNssBundle>,
    options: VirtualCompileOptions,
}

impl StrictNssPolicy {
    /// Create policy for one known game and per-call resource/cancel/deadline limits.
    pub fn new(game: Game, options: VirtualCompileOptions) -> PResult<Self> {
        if !cfg!(target_os = "linux") {
            return Err(reject("strict NSS compilation is supported only on Linux"));
        }
        options.validate().map_err(|e| reject(e.to_string()))?;
        Ok(Self {
            game,
            bundles: BTreeMap::new(),
            options,
        })
    }

    /// Bind a certified bundle to an exact INI sourcefolder spelling (including `.`).
    /// The caller must certify the effective precedence-resolved closure for that folder.
    pub fn add_bundle(&mut self, sourcefolder: String, bundle: StrictNssBundle) -> PResult<()> {
        if self.bundles.contains_key(&sourcefolder) {
            return Err(reject("duplicate sourcefolder policy"));
        }
        self.bundles.insert(sourcefolder, bundle);
        Ok(())
    }

    /// Check that every compiler input is selected explicitly, without any file reads.
    pub fn validate_patch(&self, patch: &ModificationsNSS, game: Game) -> PResult<()> {
        self.options.validate().map_err(|e| reject(e.to_string()))?;
        if self.game != game || !patch.compiler_flags.trim().is_empty() {
            return Err(reject(
                "strict NSS game mismatch or unsupported compiler flags",
            ));
        }
        let name = patch.base.sourcefile.to_ascii_lowercase();
        if !valid_name(&name) || name == "nwscript.nss" {
            return Err(reject("strict main must be one flat NSS filename"));
        }
        let bundle = self
            .bundles
            .get(&patch.base.sourcefolder)
            .ok_or_else(|| reject("explicit sourcefolder closure missing"))?;
        if !bundle.raw.contains_key(&name) {
            return Err(reject("certified main source missing"));
        }
        let limits = &self.options.limits;
        let mut total = 0usize;
        if bundle.text.len() - 1 > limits.max_files {
            return Err(reject("certified closure exceeds requested file budget"));
        }
        for (input_name, source) in &bundle.text {
            let ceiling = if input_name == "nwscript.nss" {
                limits.max_header_bytes
            } else {
                limits.max_source_bytes
            };
            total = total
                .checked_add(source.len())
                .ok_or_else(|| reject("source byte overflow"))?;
            if source.len() > ceiling || total > limits.max_total_source_bytes {
                return Err(reject("certified closure exceeds requested source budget"));
            }
        }
        Ok(())
    }
}

/// Strict per-script evidence. A failed attempt never supplies compiled output.
#[derive(Debug, Clone)]
pub struct StrictNssOutcome {
    /// Lowercase logical main filename.
    pub source_name: String,
    /// SHA-256 of the certified original main bytes (lowercase hex).
    pub raw_source_sha256: Option<String>,
    /// SHA-256 of the UTF-8 main after memory-token substitution.
    pub transformed_source_sha256: Option<String>,
    /// SHA-256 of the exact successful compiler output returned for installation.
    pub ncs_sha256: Option<String>,
    /// Actual consumed logical inputs, including header and main; empty on failure.
    pub included_files: Vec<String>,
    /// Token-substitution and compiler warnings, also preserved in PatchLogger.
    pub warnings: Vec<String>,
    /// Explicit failure detail, when compilation did not complete.
    pub error: Option<String>,
}

impl ModificationsNSS {
    fn apply_strict_tokens(
        &self,
        source: &mut String,
        memory: &PatcherMemory,
        logger: &mut PatchLogger,
        options: &VirtualCompileOptions,
    ) -> PResult<u64> {
        use std::borrow::Cow;
        use std::sync::atomic::Ordering;
        let mut work = 0u64;
        let warning_start = logger.warnings().len();
        let ceiling = options
            .limits
            .max_source_bytes
            .min(VirtualLimits::default().max_source_bytes);
        for token_name in ["2DAMEMORY", "StrRef"] {
            let value = |id: i64| -> Option<Cow<'_, str>> {
                let id = i32::try_from(id).ok()?;
                if token_name == "2DAMEMORY" {
                    memory
                        .memory_2da
                        .get(&id)
                        .map(|v| Cow::Borrowed(v.as_str()))
                } else {
                    memory
                        .memory_str
                        .get(&id)
                        .map(|v| Cow::Owned(v.to_string()))
                }
            };
            let highest = if token_name == "2DAMEMORY" {
                memory.memory_2da.keys().copied().max().unwrap_or(0) as i64
            } else {
                0
            };
            let mut previous = -1i64;
            loop {
                work = work
                    .checked_add(source.len() as u64 + 1)
                    .ok_or_else(|| reject("token work overflow"))?;
                if work > options.limits.max_work
                    || options.cancelled.load(Ordering::Relaxed)
                    || options
                        .deadline
                        .is_some_and(|deadline| std::time::Instant::now() >= deadline)
                {
                    return Err(reject(
                        "strict token substitution budget, cancellation or deadline reached",
                    ));
                }
                let id = super::find_tokens(source, token_name)
                    .into_iter()
                    .filter(|(digits, id)| *digits == id.to_string())
                    .map(|(_, id)| id)
                    .filter(|id| *id > previous)
                    .filter(|id| value(*id).is_some() || (1..=highest).contains(id))
                    .min();
                let Some(id) = id else { break };
                previous = id;
                let token = format!("#{token_name}{id}#");
                let replacement = value(id).unwrap_or(Cow::Borrowed(""));
                let occurrences = source.matches(&token).count();
                let length = occurrences
                    .checked_mul(replacement.len())
                    .and_then(|added| {
                        source
                            .len()
                            .checked_sub(occurrences * token.len())?
                            .checked_add(added)
                    })
                    .ok_or_else(|| reject("token expansion overflow"))?;
                if length > ceiling {
                    return Err(reject("token-substituted source exceeds byte budget"));
                }
                logger.add_verbose(format!(
                    "{}: Replacing '{token}' with '{replacement}'",
                    self.base.sourcefile
                ));
                *source = source.replace(&token, &replacement);
            }
            let mut undefined: Vec<_> = super::find_tokens(source, token_name)
                .into_iter()
                .map(|(_, id)| id)
                .filter(|id| value(*id).is_none())
                .collect();
            undefined.sort_unstable();
            undefined.dedup();
            for id in undefined {
                if logger.warnings().len() - warning_start >= options.limits.max_diagnostics {
                    return Err(reject("token warning budget exceeded"));
                }
                logger.add_warning(format!(
                    "{token_name}{id} was not defined before use in '{}'; leaving token unchanged.",
                    self.base.sourcefile
                ));
            }
        }
        Ok(work)
    }

    /// Substitute actual current memory tokens and compile only certified snapshot inputs.
    /// No external executable, filesystem lookup, header cache, or processed-file write.
    /// Missing entrypoints fail rather than silently satisfying a required CompileList item.
    pub fn apply_strict(
        &self,
        memory: &PatcherMemory,
        logger: &mut PatchLogger,
        game: Game,
        policy: &StrictNssPolicy,
    ) -> (PResult<Vec<u8>>, StrictNssOutcome) {
        let name = self.base.sourcefile.to_ascii_lowercase();
        let warning_start = logger.warnings().len();
        let mut included_files = Vec::new();
        let mut raw_source_sha256 = None;
        let mut transformed_source_sha256 = None;
        let mut ncs_sha256 = None;
        let result = (|| {
            policy.validate_patch(self, game)?;
            let bundle = &policy.bundles[&self.base.sourcefolder];
            raw_source_sha256 =
                Some(super::sha256_hex_upper(&bundle.raw[&name]).to_ascii_lowercase());
            let mut source = bundle.text[&name].clone();
            let work = self.apply_strict_tokens(&mut source, memory, logger, &policy.options)?;
            transformed_source_sha256 =
                Some(super::sha256_hex_upper(source.as_bytes()).to_ascii_lowercase());
            let token_warnings = logger.warnings().len() - warning_start;
            let mut compiler_options = policy.options.clone();
            compiler_options.limits.max_work = compiler_options
                .limits
                .max_work
                .checked_sub(work)
                .filter(|remaining| *remaining > 0)
                .ok_or_else(|| {
                    reject("no compiler work budget remains after token substitution")
                })?;
            compiler_options.limits.max_diagnostics = compiler_options
                .limits
                .max_diagnostics
                .checked_sub(token_warnings)
                .filter(|remaining| *remaining > 0)
                .ok_or_else(|| {
                    reject("no compiler diagnostic budget remains after token substitution")
                })?;
            let mut files = bundle.text.clone();
            files.remove(&name);
            let game = match game {
                Game::K1 => bionsscomp::Game::K1,
                Game::K2 => bionsscomp::Game::K2,
            };
            match bionsscomp::virtual_compile::compile_virtual(
                game,
                &name,
                &source,
                &files,
                &compiler_options,
            ) {
                Ok(compilation) => {
                    for warning in &compilation.warnings {
                        logger.add_warning(warning.to_string());
                    }
                    ncs_sha256 =
                        Some(super::sha256_hex_upper(&compilation.ncs).to_ascii_lowercase());
                    included_files = compilation.included_files;
                    Ok(compilation.ncs)
                }
                Err(error) => {
                    for diagnostic in error.diagnostics() {
                        if !diagnostic.is_error() {
                            logger.add_warning(diagnostic.to_string());
                        }
                    }
                    Err(reject(error.to_string()))
                }
            }
        })();
        let warnings = logger
            .warnings()
            .into_iter()
            .skip(warning_start)
            .map(|w| w.message.clone())
            .collect();
        let error = result.as_ref().err().map(|e| e.message.clone());
        (
            result,
            StrictNssOutcome {
                source_name: name,
                raw_source_sha256,
                transformed_source_sha256,
                ncs_sha256,
                included_files,
                warnings,
                error,
            },
        )
    }
}
