//! Strict opt-in compilation from an explicit, immutable in-memory namespace.
//!
//! No filesystem resolver, current directory, executable directory or cached
//! header participates. Limits cover header and script together. Rejection is
//! preferable to silently accepting a larger language/resource envelope.
use crate::{CompileError, CompileOptions, Diagnostic, Game, NwScript, Span, Stage};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::Instant;

/// Version of the strict namespace and resource policy contract.
pub const POLICY_VERSION: u32 = 2;

/// Numeric policy. Values may be lowered from the hard ceilings in Default.
#[derive(Debug, Clone)]
pub struct VirtualLimits {
    /// Main/include bytes per source (UTF-8 representation).
    pub max_source_bytes: usize,
    /// Explicit nwscript.nss bytes (UTF-8 representation).
    pub max_header_bytes: usize,
    /// Bytes of main plus all supplied virtual files, including unused files.
    pub max_total_source_bytes: usize,
    /// Supplied files, including header and unused includes.
    pub max_files: usize,
    /// Transitive include depth.
    pub max_include_depth: usize,
    /// Expanded bytes per translation unit, including inserted newlines.
    pub max_expanded_bytes: usize,
    /// Total lexer tokens for header plus script (including EOF).
    pub max_tokens: usize,
    /// Conservative AST allocation bound of four nodes per token per unit.
    pub max_ast_nodes: usize,
    /// Delimiter nesting before any AST is constructed.
    pub max_syntax_depth: usize,
    /// Tokens between statement/block boundaries; bounds left-associated chains.
    pub max_expression_tokens: usize,
    /// Active guarded compiler calls, including parser precedence frames.
    pub max_recursion: usize,
    /// Cooperative stage operations across the entire call.
    pub max_work: u64,
    /// Total emitted diagnostics, including header warnings.
    pub max_diagnostics: usize,
    /// Complete NCS bytes, enforced before writer growth.
    pub max_output_bytes: usize,
}
impl Default for VirtualLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 128 * 1024,
            max_header_bytes: 512 * 1024,
            max_total_source_bytes: 1024 * 1024,
            max_files: 32,
            max_include_depth: 16,
            max_expanded_bytes: 2 * 1024 * 1024,
            max_tokens: 65536,
            max_ast_nodes: 262144,
            max_syntax_depth: 32,
            max_expression_tokens: 192,
            max_recursion: 256,
            max_work: 20_000_000,
            max_diagnostics: 128,
            max_output_bytes: 2 * 1024 * 1024,
        }
    }
}
impl VirtualLimits {
    /// Reject zero values and values above this API version's hard ceilings.
    pub fn validate(&self) -> Result<(), CompileError> {
        let ceiling = Self::default();
        macro_rules! bound { ($($name:ident),*) => { $(
            if self.$name == 0 || self.$name > ceiling.$name { return Err(failure(concat!("invalid limit: ", stringify!($name)))); }
        )* }; }
        bound!(
            max_source_bytes,
            max_header_bytes,
            max_total_source_bytes,
            max_files,
            max_include_depth,
            max_expanded_bytes,
            max_tokens,
            max_ast_nodes,
            max_syntax_depth,
            max_expression_tokens,
            max_recursion,
            max_work,
            max_diagnostics,
            max_output_bytes
        );
        Ok(())
    }
}
/// Per-invocation cancellation and resource policy, never process-global state.
#[derive(Debug, Clone)]
pub struct VirtualCompileOptions {
    /// Explicit numeric policy.
    pub limits: VirtualLimits,
    /// Set true from another thread to cancel; checked during all stages.
    pub cancelled: Arc<AtomicBool>,
    /// Optional monotonic deadline, checked with the cooperative work budget.
    pub deadline: Option<Instant>,
}
impl Default for VirtualCompileOptions {
    fn default() -> Self {
        Self {
            limits: VirtualLimits::default(),
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: None,
        }
    }
}
impl VirtualCompileOptions {
    /// Validate policy and cancellation state before a caller starts effects.
    pub fn validate(&self) -> Result<(), CompileError> {
        self.limits.validate()?;
        if !cfg!(panic = "unwind") {
            return Err(failure("virtual compilation requires panic=unwind"));
        }
        if self.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(failure("compilation cancelled"));
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(failure("compilation deadline exceeded"));
        }
        Ok(())
    }
}
/// Successful strict compilation. Caller owns hashing original source bytes.
#[derive(Debug, Clone)]
pub struct VirtualCompilation {
    /// Complete compiled script.
    pub ncs: Vec<u8>,
    /// All header/preprocessor/lexer/parser warnings.
    pub warnings: Vec<Diagnostic>,
    /// Canonical logical inputs actually consumed, header first, main next.
    pub included_files: Vec<String>,
}
pub(crate) fn failure(message: impl Into<String>) -> CompileError {
    CompileError::from_diagnostic(Diagnostic {
        severity: crate::Severity::Error,
        stage: Stage::Preprocessor,
        span: Span::default(),
        file: String::new(),
        message: message.into(),
    })
}
/// Canonical flat resource names avoid platform-dependent path interpretation.
pub(crate) fn valid_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".nss") else {
        return false;
    };
    !stem.is_empty()
        && stem.len() <= 64
        && stem
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
}
pub(crate) fn include_name(name: &str) -> Option<String> {
    let name = if name.ends_with(".nss") {
        name.to_owned()
    } else {
        format!("{name}.nss")
    };
    valid_name(&name).then_some(name)
}
/// Compile against exactly the supplied header and include map.
///
/// `files` must contain `nwscript.nss`. Keys and `source_name` must be canonical
/// lowercase flat ASCII *.nss names. Main must not also appear in the map.
/// Include spelling may omit .nss but cannot change case or use any path.
/// Source text decoding remains the caller's explicit responsibility.
///
/// A private 8 MiB stack isolates bounded recursive compiler work from caller
/// stack sizes. An internal unwind stops legacy-return-type loops without
/// installing a process-global panic hook; panic=abort builds reject this API.
/// No compiler panic or partial output is returned as successful compilation.
pub fn compile_virtual(
    game: Game,
    source_name: &str,
    source: &str,
    files: &BTreeMap<String, String>,
    options: &VirtualCompileOptions,
) -> Result<VirtualCompilation, CompileError> {
    if !cfg!(panic = "unwind") {
        return Err(failure("virtual compilation requires panic=unwind"));
    }
    options.validate()?;
    // All input checks precede copying, allocation proportional to input, and spawning.
    if !valid_name(source_name) || source_name == "nwscript.nss" || files.contains_key(source_name)
    {
        return Err(failure("invalid or aliased main source name"));
    }
    if source.len() > options.limits.max_source_bytes || files.len() > options.limits.max_files {
        return Err(failure("source or file-count limit exceeded"));
    }
    if !files.contains_key("nwscript.nss") {
        return Err(failure("explicit nwscript.nss is required"));
    }
    let mut total = source.len();
    for (name, text) in files {
        if !valid_name(name) {
            return Err(failure("invalid or case-aliased virtual filename"));
        }
        let limit = if name == "nwscript.nss" {
            options.limits.max_header_bytes
        } else {
            options.limits.max_source_bytes
        };
        total = total
            .checked_add(text.len())
            .ok_or_else(|| failure("source byte overflow"))?;
        if text.len() > limit || total > options.limits.max_total_source_bytes {
            return Err(failure("source closure byte limit exceeded"));
        }
    }
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("nss-virtual".into())
            .stack_size(8 * 1024 * 1024)
            .spawn_scoped(scope, || {
                crate::budget::install(options.clone());
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::budget::tick();
                    let resolver =
                        crate::preprocessor::IncludeResolver::virtual_files(files.clone());
                    let header = &files["nwscript.nss"];
                    let expansion =
                        crate::preprocessor::expand(header, Path::new("nwscript.nss"), &resolver)?;
                    let header_includes = expansion.included_files.clone();
                    let (table, mut warnings) =
                        NwScript::from_expansion(game, header.clone(), expansion)?;
                    let expansion =
                        crate::preprocessor::expand(source, Path::new(source_name), &resolver)?;
                    let result = crate::compile_expansion(
                        expansion,
                        &CompileOptions::new(&table).with_source_name(source_name),
                    )?;
                    warnings.extend(result.warnings);
                    let mut included_files =
                        vec!["nwscript.nss".to_string(), source_name.to_string()];
                    for path in header_includes.into_iter().chain(result.included_files) {
                        let name = path.to_string_lossy().into_owned();
                        if !included_files.contains(&name) {
                            included_files.push(name);
                        }
                    }
                    crate::budget::tick();
                    Ok(VirtualCompilation {
                        ncs: result.ncs,
                        warnings,
                        included_files,
                    })
                }))
                .unwrap_or_else(|panic| {
                    if let Some(stop) = panic.downcast_ref::<crate::budget::Stop>() {
                        Err(failure(stop.0))
                    } else {
                        Err(failure("internal compiler failure"))
                    }
                });
                match result {
                    Ok(compilation) => Ok(compilation),
                    Err(error) => {
                        let mut warnings = crate::budget::take_warnings();
                        let mut diagnostics = match error {
                            CompileError::Diagnostics(diagnostics) => diagnostics,
                            other => vec![Diagnostic {
                                severity: crate::Severity::Error,
                                stage: Stage::Preprocessor,
                                span: Span::default(),
                                file: String::new(),
                                message: other.to_string(),
                            }],
                        };
                        warnings.retain(|warning| !diagnostics.contains(warning));
                        warnings.append(&mut diagnostics);
                        Err(CompileError::Diagnostics(warnings))
                    }
                }
            })
            .map_err(|_| failure("unable to allocate compiler worker"))?;
        worker
            .join()
            .unwrap_or_else(|_| Err(failure("compiler worker failed")))
    })
}
