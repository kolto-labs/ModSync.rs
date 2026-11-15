mod authoring;
mod backend_contract;
pub use authoring::AuthoringActionReport;
mod checkpoint_coordinator;
mod cleanlist;
mod k1_widescreen;
mod k2_preparation;
mod managed;
mod source_resolution;
mod tomb_shadow_compatibility;
mod tomb_shadow_observer;
pub use managed::{ManagedLiveEdit, ManagedRunReport};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallReport {
    pub managed: Option<ManagedRunReport>,
    pub checkpoint: Option<CheckpointRunReport>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckpointRunReport {
    pub session_id: String,
    pub checkpoint_id: String,
    pub generation: u64,
}
mod companion_selector;
mod external_patcher;
mod patcher_prediction;
mod patcher_prepare;
mod provided_installer;
mod reviewed_patcher;
mod source_plan_resources;
mod strict_nss;
pub use provided_installer::{ProvidedInstallerRunner, WindowsProcessRunner};
mod patcher_simulation;
pub(crate) mod process_arguments;

use crate::{
    archive,
    install_progress::{OwnerFrame, Phase as InstallPhase, Progress as InstallProgress},
    model::{Component, Guide, Instruction},
    paths::{Roots, relative},
    recovery,
};
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet, btree_map},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PatcherEngine {
    #[default]
    OdyPatcher,
    KPatcher,
    HoloPatcher,
    /// The actual executable shipped in each authenticated mod package.
    ModPackaged,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub roots: Roots,
    pub allow_processes: bool,
    pub patcher: Option<PathBuf>,
    pub patcher_engine: PatcherEngine,
    pub provided_installer: Option<ProvidedInstallerRunner>,
    /// Explicit headless execution for Execute/Run, independent of the Patcher backend.
    pub execute_runner: Option<ProvidedInstallerRunner>,
    pub archive: archive::Options,
    pub cancelled: Arc<AtomicBool>,
}
impl Settings {
    pub fn new(mods: &Path, game: &Path) -> Result<Self> {
        let game = crate::game::resolve_install_root(game);
        Ok(Self {
            roots: Roots::new(mods, &game)?,
            allow_processes: false,
            patcher: None,
            patcher_engine: PatcherEngine::default(),
            provided_installer: None,
            execute_runner: None,
            archive: archive::Options::default(),
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub component: String,
    pub step: usize,
    pub message: String,
}

/// An entry in the predicted filesystem. Unknown patcher content must never be
/// presented as an empty file or as a verified binary patch result.
#[derive(Clone, Debug, Serialize)]
pub struct PredictedEntry {
    pub path: PathBuf,
    pub directory: bool,
    pub content_known: bool,
    pub bytes: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Prediction {
    pub entries: Vec<PredictedEntry>,
    pub events: Vec<Event>,
}

/// Run-local observer state. Nothing is installed on Engine itself, so silent
/// validation/preflight and ordinary Event consumers cannot observe these facts.
struct InstallObserver<'a> {
    callback: &'a mut dyn FnMut(InstallProgress),
    current: InstallProgress,
}

impl<'a> InstallObserver<'a> {
    fn new(total: usize, callback: &'a mut dyn FnMut(InstallProgress)) -> Self {
        Self {
            callback,
            current: InstallProgress {
                phase: InstallPhase::Preparing,
                top_level_component_name: None,
                top_level_component_guid: None,
                component_index: 0,
                component_total: total,
                owner_name: None,
                owner_guid: None,
                owner_path: Vec::new(),
                instruction_index: None,
                instruction_total: None,
                operation: None,
                message: String::new(),
                snapshot_pending: false,
            },
        }
    }

    fn emit(&mut self, phase: InstallPhase, message: impl Into<String>) {
        self.current.phase = phase;
        self.current.message = message.into();
        (self.callback)(self.current.clone());
    }

    fn component(&mut self, component: &Component, index: usize) {
        self.current.top_level_component_name = Some(component.name.clone());
        self.current.top_level_component_guid = Some(component.guid.clone());
        self.current.component_index = index;
        self.current.owner_name = Some(component.name.clone());
        self.current.owner_guid = Some(component.guid.clone());
        self.current.owner_path = vec![OwnerFrame {
            name: component.name.clone(),
            guid: component.guid.clone(),
            reached_from_instruction: None,
        }];
        self.current.instruction_index = None;
        self.current.instruction_total = Some(component.instructions.len());
        self.current.operation = None;
    }

    fn instruction(&mut self, owner: &Component, index: usize, message: String) {
        self.current.owner_name = Some(owner.name.clone());
        self.current.owner_guid = Some(owner.guid.clone());
        self.current.instruction_index = Some(index);
        self.current.instruction_total = Some(owner.instructions.len());
        self.current.operation = Some(owner.instructions[index - 1].action.clone());
        self.emit(InstallPhase::RunningInstruction, message);
    }

    fn clear_component(&mut self) {
        self.current.top_level_component_name = None;
        self.current.top_level_component_guid = None;
        self.current.component_index = 0;
        self.current.owner_name = None;
        self.current.owner_guid = None;
        self.current.owner_path.clear();
        self.current.instruction_index = None;
        self.current.instruction_total = None;
        self.current.operation = None;
    }
}

#[derive(Clone)]
enum Content {
    /// Presence inferred from InstallList; never publish or interpret as empty bytes.
    Unknown,
    Disk(PathBuf),
    Scratch(
        Arc<tempfile::NamedTempFile>,
        Option<u32>,
        Option<std::time::SystemTime>,
    ),
}
impl Content {
    fn path(&self) -> Result<&Path> {
        match self {
            Self::Unknown => bail!("Patcher InstallList file content is unknown during simulation"),
            Self::Disk(path) => Ok(path),
            Self::Scratch(file, _, _) => Ok(file.path()),
        }
    }
    fn open(&self) -> Result<fs::File> {
        Ok(fs::File::open(self.path()?)?)
    }

    fn modified(&self) -> Result<Option<std::time::SystemTime>> {
        match self {
            Self::Unknown => Ok(None),
            Self::Disk(path) => Ok(Some(fs::metadata(path)?.modified()?)),
            Self::Scratch(_, _, modified) => Ok(*modified),
        }
    }
}

fn same_regular_file(left: &fs::File, right: &fs::File) -> Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let (left, right) = (left.metadata()?, right.metadata()?);
        Ok(left.dev() == right.dev() && left.ino() == right.ino())
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let identity = |file: &fs::File| -> Result<(u32, u32, u32)> {
            let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
            ensure!(
                unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } != 0,
                "Could not verify CopyFile object identity: {}",
                std::io::Error::last_os_error()
            );
            Ok((
                info.dwVolumeSerialNumber,
                info.nFileIndexHigh,
                info.nFileIndexLow,
            ))
        };
        Ok(identity(left)? == identity(right)?)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (left, right);
        bail!("CopyFile object identity is unsupported on this platform")
    }
}

/// Whether an authored Source of this action also matches every file under it as
/// a folder. The reference binds file-action Sources with
/// `PathHelper.EnumerateFilesWithWildcards`, which returns files only, so a folder
/// Source is a missing source for them. Extract, Patcher and FOMOD scoping keep the
/// folder rule (FOMOD root-folder flows rely on it for Extract).
pub(crate) fn authored_descendants(action: &str) -> bool {
    !matches!(
        action,
        "Copy"
            | "CopyFile"
            | "ExtractTo"
            | "Move"
            | "Rename"
            | "Delete"
            | "CleanList"
            | "Execute"
            | "Run"
    )
}

/// How an action's missing Source is classified; see `Engine::classify_miss`.
#[derive(Clone, Copy)]
enum MissKind {
    LiteralTyped,
    WildcardTyped,
}

/// Whether a Patcher Destination is `<<gameDirectory>>` or `<<kotorDirectory>>`,
/// ignoring case (`OrdinalIgnoreCase`), as C# `ParseDestinationWithAction` accepts
/// it. C# then checks it as a path, so only the exact spelling resolves.
fn game_placeholder(raw: &str) -> bool {
    ["<<gameDirectory>>", "<<kotorDirectory>>"]
        .iter()
        .any(|root| crate::unicode_case::ordinal_eq(raw, root))
}

/// Whether an authored Destination has a part that C# `PathValidator.IsValidPath`
/// refuses and `paths::relative` lets through: a `.` part (which `relative` drops
/// while resolving), `..` inside a longer name (`a..b`; a bare `..` is refused by
/// `relative` as unsafe) or a control character (below U+0020). C# validation (and
/// so its install) refuses such a Copy, Move, CleanList or DelDuplicate Destination,
/// and its DelDuplicate and CleanList execution fail on one.
fn invalid_destination(raw: &str) -> bool {
    raw.split(['/', '\\']).any(|part| {
        part == "." || (part != ".." && part.contains("..")) || part.chars().any(|c| c < ' ')
    })
}

/// A Rename Destination as the new file name in the source's own folder. C#
/// `RenameFile` joins it to that folder unresolved (`Instruction.cs:976-979`, `:1005`),
/// so its install fails a placeholder path such as `<<kotorDirectory>>/Override/b.txt`
/// (there is no such folder) while its dry run passes it. Rust refuses a placeholder
/// where the step runs (its dry run, so plain validate, and before any install
/// write); a path of several parts is also refused by the archive/destination stage.
///
/// The name must be one file name that is valid on Windows, written as it is used: no
/// separator or `.` part (`./b.txt`, `b.txt/`), none of the parts `invalid_destination`
/// refuses (`..` inside the name, a control character) and none `relative` refuses (a
/// trailing dot or space, a device name such as `con.txt`, `:`, wildcards). C#
/// validation never checks a Rename Destination, so its Linux install renames to such a
/// name; Rust refuses it at the same points as a folder part. The drafter keeps a bare
/// new name only when this accepts it (`draft::is_file_name`).
pub(crate) fn rename_file_name(destination: &str) -> Result<PathBuf> {
    ensure!(
        !destination.starts_with("<<"),
        "Rename Destination must be a file name, not a placeholder path: {destination}"
    );
    let name = relative(destination, false)?;
    ensure!(
        name.components().count() == 1 && name.as_os_str() == std::ffi::OsStr::new(destination),
        "Rename Destination must be one file name: {destination}"
    );
    ensure!(
        !invalid_destination(destination),
        "Rename Destination is not a valid file name (`..` inside the name or a control character): {destination}"
    );
    Ok(name)
}

/// What a component's Extract steps would extract; see `Engine::archive_members`.
struct ArchiveMembers<'a> {
    members: BTreeSet<String>,
    has_archives: bool,
    /// An archive only an option that is not chosen extracts could not be listed.
    unknown: bool,
    missing: Vec<&'a Instruction>,
}

/// Whether an instruction is one of the component's own (not an option's).
fn is_top_level(component: &Component, instruction: &Instruction) -> bool {
    component
        .instructions
        .iter()
        .any(|own| std::ptr::eq(own, instruction))
}

/// Whether an instruction's Dependencies are all in `selected` and none of its
/// Restrictions is.
fn applies_with(selected: &BTreeSet<crate::guid::Identity>, instruction: &Instruction) -> bool {
    instruction
        .dependencies
        .iter()
        .all(|id| selected.contains(&crate::guid::key_ascii(id)))
        && !instruction
            .restrictions
            .iter()
            .any(|id| selected.contains(&crate::guid::key_ascii(id)))
}

/// Glob text for one authored Source path with the reference wildcard rules
/// (`PathHelper.WildcardPathMatch`): each path level is matched on its own, `*`
/// (and so `**`) matches any run of characters within one level, `?` matches one
/// character, and every other character, including `[` and `]`, is literal.
pub(crate) fn source_wildcard_glob(text: &str) -> String {
    let mut glob = String::with_capacity(text.len());
    let mut star = false;
    for character in text.chars() {
        match character {
            '*' if star => {}
            '*' => glob.push('*'),
            '?' => glob.push('?'),
            '[' => glob.push_str("[[]"),
            ']' => glob.push_str("[]]"),
            other => glob.push(other),
        }
        star = character == '*';
    }
    glob
}

/// The glob half of Extract/Copy source matching; a source also matches every file
/// below it as a folder (see `Engine::source_matches`).
pub(crate) fn source_glob_matches(pattern: &glob::Pattern, candidate: &str) -> bool {
    pattern.matches_with(
        candidate,
        glob::MatchOptions {
            case_sensitive: false,
            require_literal_separator: true,
            require_literal_leading_dot: false,
        },
    )
}

/// File names the FOMOD gate treats as archives; also the folder-source heuristic
/// for run-scoped FOMOD choices.
pub(crate) fn is_archive(name: &str) -> bool {
    matches!(
        Path::new(name)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("zip" | "7z" | "rar" | "exe" | "tar" | "gz")
    )
}

/// Where an Extract unpacks `source`: a folder named after the archive's stem, next
/// to the archive. A step's Destination is never used, as in C#: validation
/// (`ComponentValidation.ParseDestinationWithAction`, AttemptFixes on by default)
/// clears an Extract Destination before the dry run and the install.
fn extraction_destination(source: &Path) -> PathBuf {
    source.parent().unwrap().join(source.file_stem().unwrap())
}

/// A resolver miss remains an error for strict validation and installation.
#[derive(Debug)]
struct MissingSource(String);
impl std::fmt::Display for MissingSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Source matches no files: {}", self.0)
    }
}
impl std::error::Error for MissingSource {}

/// Only the Copy/Move/Extract/Patcher source-resolution boundary (including an
/// empty Copy/Move/Extract Source list), a strict (Overwrite=true) Delete miss
/// or empty Source, and the misses `Engine::classify_miss` types (Rename,
/// CleanList, Execute/Run) produce this cause. Configured FOMOD member failures
/// stay hard errors.
#[derive(Debug)]
struct MissingSourceFiles(anyhow::Error);
impl std::fmt::Display for MissingSourceFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for MissingSourceFiles {}

/// C# `ActionExitCode.OptionalInstallFailed`: any failure of a chosen option, a
/// missing source included, fails its component (`UserCancelledInstall`), so the
/// dry run reports it as an error rather than `DryRunSkipped`.
#[derive(Debug)]
struct OptionalInstallFailed(anyhow::Error);
impl OptionalInstallFailed {
    /// Hides the `MissingSourceFiles` cause; other errors already fail the dry run.
    fn wrap(error: anyhow::Error) -> anyhow::Error {
        if error.is::<MissingSourceFiles>() {
            anyhow::Error::new(Self(error))
        } else {
            error
        }
    }
}
impl std::fmt::Display for OptionalInstallFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for OptionalInstallFailed {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

/// The install-tree scan's refusal of two names that differ only in case. Typed so
/// the full report can show it as C#'s Environment stage does
/// ([`Engine::case_collision_report`]); its text is unchanged.
#[derive(Debug)]
struct CaseCollision {
    path: PathBuf,
    directory: bool,
}
impl std::fmt::Display for CaseCollision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = if self.directory {
            "directories"
        } else {
            "files"
        };
        write!(f, "Case-colliding {kind}: {}", self.path.display())
    }
}
impl std::error::Error for CaseCollision {}

/// The first name under the game folder that differs from another only in case,
/// with the scan's walk and keys. C# checks only this folder
/// (`PathHelper.FindCaseInsensitiveDuplicates(MainConfig.DestinationPath)`).
fn game_case_collision(game: &Path, policy: crate::paths::PathPolicy) -> Option<PathBuf> {
    let (mut files, mut directories) = (BTreeSet::new(), BTreeSet::new());
    for entry in walkdir::WalkDir::new(game) {
        let entry = entry.ok()?;
        let seen = if entry.file_type().is_file() {
            &mut files
        } else if entry.file_type().is_dir() {
            &mut directories
        } else {
            continue;
        };
        if !seen.insert(policy.key(entry.path())) {
            return Some(entry.into_path());
        }
    }
    None
}

#[derive(Debug)]
struct SelectionInspectionIncomplete;
impl std::fmt::Display for SelectionInspectionIncomplete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Archive selection inputs changed during inspection; retry the scan")
    }
}
impl std::error::Error for SelectionInspectionIncomplete {}

fn selection_io_uncertain(error: &std::io::Error) -> bool {
    !matches!(
        error.kind(),
        std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof
    )
}

fn selection_inspection_uncertain(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        if cause.is::<SelectionInspectionIncomplete>() {
            return true;
        }
        if let Some(error) = cause.downcast_ref::<std::io::Error>() {
            return selection_io_uncertain(error);
        }
        // These decoders do not expose their wrapped I/O errors through source().
        if let Some(error) = cause.downcast_ref::<sevenz_rust2::Error>() {
            return match error {
                sevenz_rust2::Error::Io(error, _)
                | sevenz_rust2::Error::FileOpen(error, _)
                | sevenz_rust2::Error::MaybeBadPassword(error) => selection_io_uncertain(error),
                sevenz_rust2::Error::MaxMemLimited { .. } | sevenz_rust2::Error::FileNotFound => {
                    true
                }
                _ => false,
            };
        }
        if let Some(error) = cause.downcast_ref::<unrar::error::UnrarError>() {
            return matches!(
                error.code,
                unrar::error::Code::EOpen
                    | unrar::error::Code::ERead
                    | unrar::error::Code::EClose
                    | unrar::error::Code::NoMemory
                    | unrar::error::Code::Unknown
            );
        }
        false
    })
}

fn selection_file_stamp(path: &Path) -> Result<String> {
    let metadata = fs::symlink_metadata(path)?;
    let base = format!(
        "{:?}/{}/{:?}/{:?}",
        metadata.file_type(),
        metadata.len(),
        metadata.modified()?,
        metadata.created().ok()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(format!(
            "{base}/{}/{}/{}/{}",
            metadata.dev(),
            metadata.ino(),
            metadata.ctime(),
            metadata.ctime_nsec()
        ))
    }
    #[cfg(not(unix))]
    {
        Ok(base)
    }
}

pub struct Engine {
    pub settings: Settings,
    path_policy: crate::paths::PathPolicy,
    patcher_invocation: Option<crate::patcher::Invocation>,
    patcher_resolution_error: Option<String>,
    files: BTreeMap<String, (PathBuf, Content)>,
    authoring_inputs: Vec<(PathBuf, PathBuf)>,
    exact_patcher_prediction: bool,
    directories: BTreeMap<String, PathBuf>,
    selection_directory_stamps: BTreeMap<PathBuf, String>,
    install: bool,
    component_checkpoints: bool,
    checkpoint_history_forbidden: bool,
    checkpoint_report: Option<CheckpointRunReport>,
    checkpoint_resume: Option<crate::checkpoint::ResumeRequest>,
    checkpoint_process: Option<(
        std::sync::Arc<recovery::Lock>,
        crate::checkpoint::OperationIntent,
    )>,
    managed: Option<managed::Run>,
    stage_managed_writes: bool,
    selected: BTreeSet<crate::guid::Identity>,
    completed: BTreeSet<crate::guid::Identity>,
    checkpoint_completed: BTreeSet<crate::guid::Identity>,
    completed_source_definitions: BTreeMap<crate::guid::Identity, Component>,
    supplemental_source_guide: Option<Guide>,
    k2_prepared: Option<serde_json::Value>,
    selected_names: BTreeSet<String>,
    conflicts: Option<crate::conflicts::Tracker>,
    validation_target: Option<(String, String, Option<usize>)>,
    validation_warnings: Vec<crate::validation::DryRunIssue>,
}

/// C#'s Environment stage (`InstallationService.ValidateInstallationEnvironmentAsync`)
/// fails when it cannot create a file in the game or mod folder
/// (`UtilityHelper.IsDirectoryWritable`), with the mod folder's message when both
/// fail. Rust asks the operating system instead of writing a probe file, so
/// validation changes nothing; Windows is not checked.
fn unwritable_root(roots: &Roots) -> Option<&'static str> {
    if !directory_writable(&roots.mods) {
        Some(
            "The Mod directory is not writable! Please ensure administrative privileges \
             or choose a new mod directory.",
        )
    } else if !directory_writable(&roots.game) {
        Some(
            "The Install directory is not writable! Please ensure administrative \
             privileges or reinstall KOTOR to a directory with write access.",
        )
    } else {
        None
    }
}

/// The errors C#'s Environment stage logs for each selected component
/// (`InstallationService.ValidateInstallationEnvironmentAsync`): a Restriction that
/// names a selected component or immediate option, then a Dependency that names one
/// not selected. As C# `FindComponentsFromGuidList`, a GUID that names neither is
/// skipped here (the later stages refuse it). `selected` is the Conflicts stage's set.
fn environment_relations(
    guide: &Guide,
    components: &[&Component],
    selected: &BTreeSet<String>,
) -> Vec<String> {
    use crate::guid::{eq_ascii, key_ascii};
    let selected: BTreeSet<_> = selected.iter().map(|id| key_ascii(id)).collect();
    let find = |id: &String| {
        guide.components.iter().find_map(|component| {
            if eq_ascii(&component.guid, id) {
                Some(component)
            } else {
                component
                    .options
                    .iter()
                    .find(|option| eq_ascii(&option.guid, id))
            }
        })
    };
    let mut errors = Vec::new();
    for component in components {
        for found in component.restrictions.iter().filter_map(&find) {
            if selected.contains(&key_ascii(&found.guid)) {
                errors.push(format!(
                    "Cannot install '{}' due to '{}' being selected for install.",
                    component.name, found.name
                ));
            }
        }
        for found in component.dependencies.iter().filter_map(&find) {
            if !selected.contains(&key_ascii(&found.guid)) {
                errors.push(format!(
                    "Cannot install '{}' due to '{}' not being selected for install.",
                    component.name, found.name
                ));
            }
        }
    }
    errors
}

#[cfg(unix)]
fn directory_writable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `path` is a NUL-terminated string that outlives the call.
    unsafe { libc::access(path.as_ptr(), libc::W_OK | libc::X_OK) == 0 }
}

#[cfg(not(unix))]
fn directory_writable(_: &Path) -> bool {
    true
}

fn selected_guid_keys(guide: &Guide) -> BTreeSet<crate::guid::Identity> {
    guide
        .selected_ids()
        .iter()
        .map(|id| crate::guid::key_ascii(id))
        .collect()
}

impl Engine {
    /// Opt in without adding required fields to the existing Settings contract.
    pub fn with_component_checkpoints(mut self, enabled: bool) -> Self {
        self.component_checkpoints = enabled;
        self
    }

    /// An explicit CLI refusal of retained history cannot waive writer ownership.
    /// The actual remaining execution scope decides whether that is a conflict.
    pub fn with_checkpoint_history_forbidden(mut self, forbidden: bool) -> Self {
        self.checkpoint_history_forbidden = forbidden;
        self
    }

    /// Resume only a validated checkpoint boundary; the shared store builds the
    /// immutable continuation plan while the engine holds the root lock.
    pub fn with_checkpoint_resume(
        mut self,
        request: crate::checkpoint::ResumeRequest,
    ) -> Result<Self> {
        ensure!(
            self.component_checkpoints,
            "Checkpoint resume requires component checkpoints"
        );
        ensure!(
            self.path_policy.case_insensitive,
            "Checkpoint resume requires case-insensitive path matching"
        );
        self.checkpoint_resume = Some(request);
        Ok(self)
    }

    pub fn with_managed_deployment(mut self, service: crate::deployment::Service) -> Result<Self> {
        ensure!(
            self.path_policy.case_insensitive,
            "Managed deployment requires case-insensitive path matching; exact policy is not encoded in managed manifests"
        );
        ensure!(
            self.component_checkpoints,
            "Managed installation requires component checkpoints"
        );
        ensure!(
            service.game_directory() == self.settings.roots.game,
            "Managed game root differs from engine game root"
        );
        for path in [service.staging_root(), service.manifest_root()] {
            ensure!(
                !path.starts_with(&self.settings.roots.mods)
                    && !self.settings.roots.mods.starts_with(path),
                "Managed participant overlaps mod root"
            );
        }
        service.validate_readonly()?;
        self.managed = Some(managed::Run::new(service));
        Ok(self)
    }

    pub fn new(settings: Settings) -> Result<Self> {
        Self::new_with_case_insensitive_pathing(settings, true)
    }

    /// Select path matching before the first scan so exact mode can retain
    /// distinct Unix filenames instead of silently folding their identities.
    pub fn new_with_case_insensitive_pathing(
        mut settings: Settings,
        case_insensitive: bool,
    ) -> Result<Self> {
        let path_policy = crate::paths::PathPolicy::new(case_insensitive)?;
        let scratch = fs::canonicalize(
            settings
                .archive
                .scratch_dir
                .clone()
                .unwrap_or_else(std::env::temp_dir),
        )?;
        ensure!(scratch.is_dir(), "Archive scratch path must be a directory");
        ensure!(
            !scratch.starts_with(&settings.roots.game)
                && !scratch.starts_with(&settings.roots.mods),
            "Archive scratch directory must be outside game and mod trees"
        );
        settings.archive.scratch_dir = Some(scratch);
        if settings.patcher_engine == PatcherEngine::ModPackaged {
            ensure!(
                settings.patcher.is_none(),
                "Mod-packaged backend selects each original executable; do not configure --patcher"
            );
            ensure!(
                settings.provided_installer.is_none(),
                "Mod-packaged source authority cannot be combined with an unbound supplied installer"
            );
        }
        let (patcher_invocation, patcher_resolution_error) = if settings.patcher_engine
            == PatcherEngine::ModPackaged
            || settings.provided_installer.is_some()
            || (settings.patcher_engine == PatcherEngine::OdyPatcher && settings.patcher.is_none())
        {
            (None, None)
        } else {
            match crate::patcher::resolve_invocation(
                settings.patcher_engine,
                settings.patcher.as_deref(),
            ) {
                Ok(invocation) => (invocation, None),
                Err(error) if settings.patcher.is_some() => return Err(error),
                Err(error) => (None, Some(format!("{error:#}"))),
            }
        };
        if let Some(program) = patcher_invocation
            .as_ref()
            .and_then(crate::patcher::Invocation::native_path)
        {
            settings.patcher = Some(program.to_owned());
        }
        let mut engine = Self {
            settings,
            path_policy,
            patcher_invocation,
            patcher_resolution_error,
            files: BTreeMap::new(),
            authoring_inputs: Vec::new(),
            exact_patcher_prediction: false,
            directories: BTreeMap::new(),
            selection_directory_stamps: BTreeMap::new(),
            install: false,
            component_checkpoints: false,
            checkpoint_history_forbidden: false,
            checkpoint_report: None,
            checkpoint_resume: None,
            checkpoint_process: None,
            managed: None,
            stage_managed_writes: false,
            selected: BTreeSet::new(),
            completed: BTreeSet::new(),
            checkpoint_completed: BTreeSet::new(),
            completed_source_definitions: BTreeMap::new(),
            supplemental_source_guide: None,
            k2_prepared: None,
            selected_names: BTreeSet::new(),
            conflicts: None,
            validation_target: None,
            validation_warnings: Vec::new(),
        };
        engine.scan()?;
        Ok(engine)
    }

    fn scan(&mut self) -> Result<()> {
        self.files.clear();
        self.directories.clear();
        self.selection_directory_stamps.clear();
        for root in [&self.settings.roots.mods, &self.settings.roots.game] {
            let walk = walkdir::WalkDir::new(root)
                .into_iter()
                .filter_entry(|entry| {
                    root != &self.settings.roots.mods
                        || entry.depth() == 0
                        || !(entry.file_type().is_dir()
                            && crate::download_orchestrator::is_peer_staging(entry.file_name()))
                });
            for entry in walk {
                let entry = entry?;
                ensure!(
                    !entry.file_type().is_symlink(),
                    "Symlinks are unsupported in install trees: {}",
                    entry.path().display()
                );
                let path = entry.path().to_path_buf();
                ensure!(
                    self.path_policy.case_insensitive || path.to_str().is_some(),
                    "Exact path matching requires UTF-8 paths; refusing lossy filename identity"
                );
                if entry.file_type().is_file() {
                    if self
                        .files
                        .insert(
                            self.path_policy.key(&path),
                            (path.clone(), Content::Disk(path.clone())),
                        )
                        .is_some()
                    {
                        return Err(CaseCollision {
                            path,
                            directory: false,
                        }
                        .into());
                    }
                } else {
                    ensure!(
                        entry.file_type().is_dir(),
                        "Unsupported special file: {}",
                        path.display()
                    );
                    self.selection_directory_stamps
                        .insert(path.clone(), selection_file_stamp(&path)?);
                    if self
                        .directories
                        .insert(self.path_policy.key(&path), path.clone())
                        .is_some()
                    {
                        return Err(CaseCollision {
                            path,
                            directory: true,
                        }
                        .into());
                    }
                }
            }
        }
        self.seed_authoring_inputs()?;
        Ok(())
    }

    fn verify_selection_directories(&self) -> Result<()> {
        for (path, before) in &self.selection_directory_stamps {
            self.check_cancelled()?;
            if selection_file_stamp(path)? != *before {
                return Err(SelectionInspectionIncomplete.into());
            }
        }
        Ok(())
    }

    /// Execute a native selection action against its candidate archive inputs.
    pub fn select_available(
        &self,
        components: &mut [Component],
        game: Option<crate::game::Detected>,
        action: crate::selection::Action,
    ) -> Result<(crate::selection::Outcome, crate::selection::Availability)> {
        let mut candidate = Guide {
            components: components.to_vec(),
            ..Guide::default()
        };
        let initial = self.selection_availability(&candidate)?;
        crate::selection::apply_available(
            &mut candidate.components,
            game,
            action.clone(),
            &initial,
        )?;
        let availability = self.selection_availability(&candidate)?;
        let outcome = crate::selection::apply_available(components, game, action, &availability)?;
        Ok((outcome, availability))
    }

    /// Inspect archive inputs using the install engine's path and archive rules.
    /// No extraction, external process, selection mutation, or game write occurs.
    pub fn selection_availability(&self, guide: &Guide) -> Result<crate::selection::Availability> {
        self.verify_selection_directories()?;
        fn owners<'a>(components: &'a [Component], out: &mut Vec<&'a Component>) {
            for component in components {
                out.push(component);
                owners(&component.options, out);
            }
        }
        fn contains(component: &Component, guid: &str) -> bool {
            crate::guid::eq_ascii(&component.guid, guid)
                || component
                    .options
                    .iter()
                    .any(|option| contains(option, guid))
        }
        fn instructions<'a>(
            component: &'a Component,
            selected: &BTreeSet<crate::guid::Identity>,
            out: &mut Vec<&'a Instruction>,
        ) -> Result<()> {
            for instruction in &component.instructions {
                if !applies_with(selected, instruction) {
                    continue;
                }
                out.push(instruction);
                if instruction.action == "Choose" {
                    for option in Engine::chosen_options(component, instruction)? {
                        instructions(option, selected, out)?;
                    }
                }
            }
            Ok(())
        }
        let mut components = Vec::new();
        owners(&guide.components, &mut components);
        let mut failures = Vec::new();
        let mut inspected_stamps = BTreeMap::<PathBuf, String>::new();
        let mut archives =
            BTreeMap::<(String, String), std::result::Result<BTreeSet<String>, String>>::new();
        for component in components {
            self.check_cancelled()?;
            let mut candidate = guide.clone();
            let action = if guide
                .components
                .iter()
                .any(|top| std::ptr::eq(top, component))
            {
                crate::selection::Action::SetComponent {
                    guid: component.guid.clone(),
                    selected: true,
                }
            } else {
                crate::selection::Action::SetOption {
                    guid: component.guid.clone(),
                    selected: true,
                }
            };
            if !component.is_selected {
                crate::selection::apply(&mut candidate.components, None, action)?;
            }
            let selected = selected_guid_keys(&candidate);
            let candidate_root = candidate
                .components
                .iter()
                .find(|owner| contains(owner, &component.guid))
                .context("Selection archive owner disappeared")?;
            let mut category = crate::selection::ExclusionCategory::InvalidInstruction;
            let checked = (|| -> Result<()> {
                let mut checked_instructions = Vec::new();
                instructions(candidate_root, &selected, &mut checked_instructions)?;
                let mut projected = BTreeSet::<String>::new();
                for instruction in checked_instructions {
                    if !matches!(instruction.action.as_str(), "Extract" | "ExtractTo") {
                        continue;
                    }
                    category = crate::selection::ExclusionCategory::InvalidInstruction;
                    ensure!(
                        !instruction.source.is_empty(),
                        "Extract has no required archive source"
                    );
                    for source in &instruction.source {
                        category = crate::selection::ExclusionCategory::InvalidInstruction;
                        let matches = self.extraction_sources(
                            instruction,
                            std::slice::from_ref(source),
                            true,
                        )?;
                        if matches.is_empty() {
                            let (_, pattern) = self.source_pattern(source)?;
                            // Nested archives produced by an earlier Extract are validated
                            // by the actual engine when their bytes become available.
                            category = crate::selection::ExclusionCategory::MissingArchive;
                            ensure!(
                                projected.iter().any(|member| pattern.matches(member)),
                                "Missing required archive: {source}"
                            );
                        }
                        for archive_path in matches {
                            self.check_cancelled()?;
                            let key = self.path_policy.key(&archive_path);
                            let destination = self.extraction_target(&archive_path, instruction)?;
                            let cache_key = (key.clone(), self.path_policy.key(&destination));
                            if !archives.contains_key(&cache_key) {
                                let path = self.files[&key].1.path()?;
                                let before = selection_file_stamp(path)?;
                                // Distinguish access failures before a decoder can flatten them.
                                drop(fs::File::open(path)?);
                                let inspected = (|| -> Result<BTreeSet<String>> {
                                    self.settings.roots.check(&destination)?;
                                    let (entries, _) = archive::list_original_with_size(
                                        self.files[&key].1.path()?,
                                        &archive_path,
                                        &self.settings.archive,
                                        &self.settings.cancelled,
                                    )
                                    .with_context(|| {
                                        format!("Invalid archive {}", archive_path.display())
                                    })?;
                                    let mut members = BTreeSet::new();
                                    for entry in
                                        entries.into_iter().filter(|entry| !entry.is_directory)
                                    {
                                        let path = destination.join(entry.path);
                                        self.settings.roots.check(&path)?;
                                        members.insert(self.path_policy.key(&path));
                                    }
                                    Ok(members)
                                })();
                                if selection_file_stamp(path)? != before {
                                    return Err(SelectionInspectionIncomplete.into());
                                }
                                let inspected = match inspected {
                                    Err(error) if selection_inspection_uncertain(&error) => {
                                        return Err(error.context("Archive inspection is incomplete; selections were not changed"));
                                    }
                                    other => other,
                                };
                                inspected_stamps.insert(path.to_owned(), before);
                                archives.insert(
                                    cache_key.clone(),
                                    inspected.map_err(|error| format!("{error:#}")),
                                );
                            }
                            category = crate::selection::ExclusionCategory::InvalidArchive;
                            let members = archives[&cache_key]
                                .as_ref()
                                .map_err(|error| anyhow::anyhow!(error.clone()))?;
                            projected.extend(members.iter().cloned());
                        }
                    }
                }
                Ok(())
            })();
            if let Err(error) = checked {
                self.check_cancelled()?;
                if selection_inspection_uncertain(&error) {
                    return Err(error
                        .context("Archive inspection is incomplete; selections were not changed"));
                }
                failures.push(crate::selection::Unavailable {
                    guid: component.guid.clone(),
                    category,
                    reason: format!("{error:#}"),
                });
            }
        }
        self.verify_selection_directories()?;
        for (path, before) in inspected_stamps {
            self.check_cancelled()?;
            if selection_file_stamp(&path)? != before {
                return Err(SelectionInspectionIncomplete.into());
            }
        }
        Ok(crate::selection::Availability::from_archive_failures(
            &guide.components,
            failures,
        ))
    }

    /// Explicit, read-only proof export. Hash the mod inventory on both sides of
    /// a fresh inspection; never return a receipt for changing or uncertain inputs.
    pub fn selection_receipt(&self, guide: &Guide) -> Result<serde_json::Value> {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        let inventory = || -> Result<Vec<serde_json::Value>> {
            self.verify_selection_directories()?;
            let mut entries = Vec::new();
            let mut inspected = Vec::new();
            for (path, content) in self.files.values() {
                let Ok(relative) = path.strip_prefix(&self.settings.roots.mods) else {
                    continue;
                };
                self.check_cancelled()?;
                let disk = content.path()?;
                let before = selection_file_stamp(disk)?;
                let mut file = fs::File::open(disk)?;
                let mut hash = Sha256::new();
                let mut buffer = [0u8; 65536];
                let mut bytes = 0u64;
                loop {
                    self.check_cancelled()?;
                    let count = file.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    bytes += count as u64;
                    hash.update(&buffer[..count]);
                }
                ensure!(
                    before == selection_file_stamp(disk)?,
                    "Mod inventory changed while preparing receipt"
                );
                inspected.push((disk.to_owned(), before));
                let relative = relative
                    .iter()
                    .map(|part| part.to_str().context("Receipt requires UTF-8 mod paths"))
                    .collect::<Result<Vec<_>>>()?
                    .join("/");
                entries.push(serde_json::json!({"path":relative,"bytes":bytes,"sha256":format!("{:x}",hash.finalize())}));
            }
            entries.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
            // Earlier files can change while later files are being hashed.
            // Validate the entire pass before publishing its content identities.
            for (path, stamp) in inspected {
                self.check_cancelled()?;
                ensure!(
                    stamp == selection_file_stamp(&path)?,
                    "Mod inventory changed while preparing receipt"
                );
            }
            self.verify_selection_directories()?;
            Ok(entries)
        };
        let before = inventory()?;
        let availability = self.selection_availability(guide)?;
        let after = inventory()?;
        ensure!(
            before == after,
            "Mod inventory changed while preparing receipt"
        );
        let selected = crate::selection::selected_guids(&guide.components);
        ensure!(
            selected.iter().all(|id| availability.reason(id).is_none()),
            "Selection changed availability; refresh selection before preparing a receipt"
        );
        let mut definition = guide.clone();
        fn clear_choices(components: &mut [Component]) {
            for component in components {
                component.is_selected = false;
                clear_choices(&mut component.options);
            }
        }
        clear_choices(&mut definition.components);
        // Value serializes maps in canonical key order; guide/option order is retained.
        let definition = serde_json::to_vec(&serde_json::to_value(definition)?)?;
        let inventory_bytes = serde_json::to_vec(&before)?;
        Ok(serde_json::json!({
            "schemaVersion": 1,
            "guideDefinitionSha256": format!("{:x}", Sha256::digest(&definition)),
            "modInventorySha256": format!("{:x}", Sha256::digest(&inventory_bytes)),
            "modInventory": before,
            "detectedGame": format!("{:?}",crate::game::detect(&self.settings.roots.game)),
            "caseInsensitivePathing": self.path_policy.case_insensitive,
            "maxExpandedBytes": self.settings.archive.max_expanded_bytes,
            "selected": selected,
            "excluded": crate::selection::exclusion_receipt(&guide.components, &availability),
            "k2Preparation": crate::k2_preparation::verify(guide)?,
        }))
    }

    pub fn validate(mut self, guide: &Guide, mut emit: impl FnMut(Event)) -> Result<()> {
        recovery::ensure_clean(&self.settings.roots)?;
        self.verify_completion_for_backend(guide, self.completed.len())?;
        // C# runs FomodConfigurationGate on every validate path, not only the report.
        self.fomod_gate(guide, &mut emit)?;
        self.destination_gate(guide)?;
        self.archive_source_gate(guide)?;
        self.run(guide, &mut emit)
    }

    /// Use exactly the install engine's read-only simulation and retain its
    /// resulting inventory for authoring. No external patcher is launched.
    pub fn predict(mut self, guide: &Guide) -> Result<Prediction> {
        self.exact_patcher_prediction = true;
        recovery::ensure_clean(&self.settings.roots)?;
        let mut events = Vec::new();
        self.fomod_gate(guide, &mut |event| events.push(event))?;
        self.run(guide, &mut |event| events.push(event))?;
        let mut entries = Vec::with_capacity(self.files.len() + self.directories.len());
        for path in self.directories.values() {
            entries.push(PredictedEntry {
                path: path.clone(),
                directory: true,
                content_known: true,
                bytes: None,
            });
        }
        for (path, content) in self.files.values() {
            let known = !matches!(content, Content::Unknown);
            entries.push(PredictedEntry {
                path: path.clone(),
                directory: false,
                content_known: known,
                bytes: if known {
                    Some(content.open()?.metadata()?.len())
                } else {
                    None
                },
            });
        }
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Prediction { entries, events })
    }

    /// C# validates every Copy, Move, CleanList and DelDuplicate Destination of the
    /// selected components and their immediate options before installing
    /// (`ComponentValidation.ParseDestinationWithAction`) and refuses one with a part
    /// `PathValidator.IsValidPath` refuses (`invalid_destination`), which the archive
    /// stage reports. Plain validate and install refuse it here, before any
    /// simulation or write, for the same steps that stage checks (instruction
    /// conditions met). A Destination that does not resolve is left to the
    /// simulation, which reports it as before.
    fn destination_gate(&self, guide: &Guide) -> Result<()> {
        let mut selected = selected_guid_keys(guide);
        selected.extend(self.completed.iter().cloned());
        for component in guide.components.iter().filter(|component| {
            component.is_selected
                && !self
                    .completed
                    .contains(&crate::guid::key_ascii(&component.guid))
        }) {
            let owners = std::iter::once((None, component)).chain(
                component
                    .options
                    .iter()
                    .map(|option| (Some(&option.name), option)),
            );
            for (option, owner) in owners {
                for (n, instruction) in owner.instructions.iter().enumerate() {
                    if matches!(
                        instruction.action.as_str(),
                        "Copy" | "CopyFile" | "ExtractTo" | "Move" | "CleanList" | "DelDuplicate"
                    ) && invalid_destination(&instruction.destination)
                        && applies_with(&selected, instruction)
                        && self.path(&instruction.destination, false).is_ok()
                    {
                        let owner = match option {
                            Some(option) => format!("{} / {option}", component.name),
                            None => component.name.clone(),
                        };
                        bail!(
                            "{owner} / step {} ({}): Destination is not a valid path (a `.` part, `..` inside a name or a control character): {}",
                            n + 1,
                            instruction.action,
                            instruction.destination
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// C# validation, run by the CLI `install` and the desktop Validate step before
    /// installing, refuses a step whose Source in the mod folder none of its
    /// component's archives holds (`ComponentValidation.VerifyExtractPaths`), once
    /// one of them exists; the archive stage reports it. Plain validate and install
    /// refuse it here, before any simulation or write, for the same steps
    /// (`check_archive_sources`). The archives are those of every Extract of the
    /// component and its options, chosen or not, that exist, in step order, as C#
    /// lists them; each is listed only when a Source needs it, at most once, and the
    /// search stops at the first that holds the Source (C# `IsSourcePathInArchives`).
    /// A Source no listed archive holds passes when some archive could not be listed
    /// (`listed_members`); if a step that runs extracts that archive, its Extract
    /// fails as before. A component whose missing archive is extracted by a step
    /// that runs (its own, or a chosen option's) is left to the simulation, which
    /// reports it as before; a missing archive in an option that is not chosen is
    /// left out, as C# leaves it out. A component with no Extract is not checked.
    fn archive_source_gate(&self, guide: &Guide) -> Result<()> {
        let mut selected = selected_guid_keys(guide);
        selected.extend(self.completed.iter().cloned());
        let mut listed: BTreeMap<(String, String), Option<BTreeSet<String>>> = BTreeMap::new();
        for component in guide.components.iter().filter(|component| {
            component.is_selected
                && !self
                    .completed
                    .contains(&crate::guid::key_ascii(&component.guid))
        }) {
            let owners: Vec<_> = std::iter::once((None, component))
                .chain(
                    component
                        .options
                        .iter()
                        .map(|option| (Some(&option.name), option)),
                )
                .collect();
            let mut archives: Vec<(PathBuf, PathBuf)> = Vec::new();
            let mut runs_missing = false;
            for (instruction, runs) in Self::archive_check_instructions(component, &selected) {
                self.check_cancelled()?;
                if !matches!(instruction.action.as_str(), "Extract" | "ExtractTo")
                    || instruction.source.is_empty()
                {
                    continue;
                }
                for source in &instruction.source {
                    if runs
                        && self
                            .extraction_sources(instruction, std::slice::from_ref(source), true)?
                            .is_empty()
                    {
                        runs_missing = true;
                    }
                }
                for archive in self.extraction_sources(instruction, &instruction.source, true)? {
                    let destination = self.extraction_target(&archive, instruction)?;
                    let projection = (archive, destination);
                    if !archives.contains(&projection) {
                        archives.push(projection);
                    }
                }
            }
            if archives.is_empty() || runs_missing {
                continue;
            }
            for (option, owner) in &owners {
                for (n, instruction) in owner.instructions.iter().enumerate() {
                    if !applies_with(&selected, instruction) {
                        continue;
                    }
                    self.check_archive_sources(component, instruction, |prefix, pattern| {
                        let mut unknown = false;
                        for (archive, destination) in &archives {
                            let key = (
                                self.path_policy.key(archive),
                                self.path_policy.key(destination),
                            );
                            if !listed.contains_key(&key) {
                                let members = self.listed_members(archive, destination)?;
                                listed.insert(key.clone(), members);
                            }
                            match &listed[&key] {
                                Some(members) => {
                                    if members
                                        .iter()
                                        .any(|member| self.source_matches(prefix, pattern, member))
                                    {
                                        return Ok(Some(true));
                                    }
                                }
                                None => unknown = true,
                            }
                        }
                        Ok((!unknown).then_some(false))
                    })
                    .with_context(|| {
                        let owner = match option {
                            Some(option) => format!("{} / {option}", component.name),
                            None => component.name.clone(),
                        };
                        format!("{owner} / step {} ({})", n + 1, instruction.action)
                    })?;
                }
            }
        }
        Ok(())
    }

    /// The read-only FOMOD configuration gate for plain validate and install. Issues
    /// fail before any simulation or write; warnings are emitted as events.
    fn fomod_gate(&mut self, guide: &Guide, emit: &mut impl FnMut(Event)) -> Result<()> {
        // Warnings are emitted even when the gate then fails.
        let (warnings, issues) = self.validate_fomod_configuration(guide);
        for warning in warnings {
            emit(Event {
                component: String::new(),
                step: 0,
                message: format!("WARNING: {warning}"),
            });
        }
        // One issue keeps its own error chain. Several are numbered in gate order in
        // one message: "<n> issues: [1] <first>; [2] <second>; ...".
        let mut issues = issues.into_iter();
        let Some(first) = issues.next() else {
            return Ok(());
        };
        if issues.len() == 0 {
            return Err(first).context("FOMOD configuration check failed");
        }
        let list = std::iter::once(first)
            .chain(issues)
            .enumerate()
            .map(|(index, issue)| format!("[{}] {issue:#}", index + 1))
            .collect::<Vec<_>>();
        Err(anyhow::anyhow!(
            "{} issues: {}",
            list.len(),
            list.join("; ")
        ))
        .context("FOMOD configuration check failed")
    }

    pub fn validate_report(
        self,
        guide: &Guide,
        emit: impl FnMut(Event),
    ) -> crate::validation::Report {
        self.validate_report_with_dry_run(guide, emit, false)
    }

    pub fn validate_report_with_dry_run(
        self,
        guide: &Guide,
        emit: impl FnMut(Event),
        dry_run: bool,
    ) -> crate::validation::Report {
        self.validate_report_with_options(guide, emit, true, dry_run, false)
    }

    pub fn validate_report_with_options(
        self,
        guide: &Guide,
        emit: impl FnMut(Event),
        full_validation: bool,
        dry_run: bool,
        dry_run_only: bool,
    ) -> crate::validation::Report {
        self.validate_report_with_options_observed(
            guide,
            emit,
            full_validation,
            dry_run,
            dry_run_only,
            |_| {},
        )
    }

    /// Observe actual stage starts and completions without changing ordinary events.
    #[allow(clippy::too_many_arguments)]
    pub fn validate_report_with_options_observed(
        self,
        guide: &Guide,
        mut emit: impl FnMut(Event),
        full_validation: bool,
        dry_run: bool,
        dry_run_only: bool,
        mut progress: impl FnMut(crate::validation::StageProgress),
    ) -> crate::validation::Report {
        use crate::validation::{Stage, StageProgress};
        let started = std::time::Instant::now();
        let finish = |report: crate::validation::Report| {
            let report = report.finish();
            crate::telemetry::global().record_validation(
                "guide",
                report.success,
                report.error_count + report.warning_count,
                started.elapsed(),
            );
            report
        };
        let total = usize::from(full_validation) * 3
            + usize::from(!dry_run_only)
            + 1
            + usize::from(dry_run);
        let mut step = 0;
        let mut boundary = |stage, message: &str, complete: bool| {
            if !complete {
                step += 1;
            }
            progress(StageProgress {
                stage,
                step,
                total,
                message: if complete {
                    format!("Completed: {message}")
                } else {
                    message.into()
                },
            });
        };
        let settings = self.settings.clone();
        let fresh_engine = || -> Result<Engine> {
            let mut engine = Engine::new_with_case_insensitive_pathing(
                settings.clone(),
                self.path_policy.case_insensitive,
            )?;
            engine.completed = self.completed.clone();
            engine.checkpoint_resume = self.checkpoint_resume.clone();
            Ok(engine)
        };
        let validated_components: Vec<_> = guide
            .components
            .iter()
            .filter(|component| {
                component.is_selected
                    && !self
                        .completed
                        .contains(&crate::guid::key_ascii(&component.guid))
            })
            .collect();
        let mut report = crate::validation::Report::new(validated_components.len());
        if full_validation {
            boundary(
                Stage::Environment,
                "Validating installation environment",
                false,
            );
            let mut selected = guide.selected_ids();
            selected.extend(
                guide
                    .components
                    .iter()
                    .filter(|component| {
                        self.completed
                            .contains(&crate::guid::key_ascii(&component.guid))
                    })
                    .map(|component| component.guid.clone()),
            );
            let environment = (|| -> Result<()> {
                recovery::ensure_clean(&self.settings.roots)?;
                self.verify_completion_for_backend(guide, self.completed.len())
            })()
            .map_err(|error| format!("{error:#}"));
            let passed = match environment {
                // C#'s Environment stage checks each selected component's
                // Restrictions and Dependencies, then runs its checks (with
                // AttemptFixes), and fails on any error. The full report runs those
                // checks in its ComponentValidation stage, which --dry-run-only
                // skips, so run them here instead in that mode.
                Ok(()) => {
                    let relations = environment_relations(guide, &validated_components, &selected);
                    let outcomes = if dry_run_only {
                        fresh_engine()
                            .and_then(|engine| engine.validate_archives(guide, &mut emit, true))
                    } else {
                        Ok(Vec::new())
                    };
                    match outcomes {
                        Ok(outcomes) => report.environment_components(
                            relations,
                            outcomes,
                            unwritable_root(&self.settings.roots),
                        ),
                        Err(error) => report.push(Stage::Environment, Err(format!("{error:#}"))),
                    }
                }
                environment => report.push(Stage::Environment, environment),
            };
            boundary(
                Stage::Environment,
                "Validating installation environment",
                true,
            );
            if !passed {
                return finish(report);
            }
            boundary(Stage::Conflicts, "Checking conflicts", false);
            report.declared_conflicts(guide, &validated_components, &selected);
            boundary(Stage::Conflicts, "Checking conflicts", true);
            boundary(
                Stage::InstallOrder,
                "Validating mod installation order",
                false,
            );
            let order = guide
                .ordered()
                .map(|_| ())
                .map_err(|error| format!("{error:#}"));
            report.install_order(order);
            boundary(
                Stage::InstallOrder,
                "Validating mod installation order",
                true,
            );
        }
        if !dry_run_only {
            boundary(Stage::ComponentValidation, "Validating mod archives", false);
            let component = fresh_engine()
                .and_then(|engine| engine.validate_archives(guide, &mut emit, full_validation))
                .map_err(|error| format!("{error:#}"));
            report.component_validation(&validated_components, component);
            boundary(Stage::ComponentValidation, "Validating mod archives", true);
        }
        boundary(
            Stage::FomodConfiguration,
            "Checking FOMOD configuration",
            false,
        );
        let (warnings, issues) = match fresh_engine() {
            Ok(mut engine) => engine.validate_fomod_configuration(guide),
            Err(error) => (Vec::new(), vec![error]),
        };
        report.fomod_configuration(
            warnings,
            issues.iter().map(|issue| format!("{issue:#}")).collect(),
        );
        boundary(
            Stage::FomodConfiguration,
            "Checking FOMOD configuration",
            true,
        );
        // Without a dry run the stages above run no step, while plain `validate`
        // simulates every step and stops at the first failure. Once every stage has
        // passed, run that simulation too, so this report never passes a guide plain
        // `validate` refuses.
        if !dry_run && report.error_count == 0 {
            let outcome = fresh_engine()
                .map_err(|error| (None, error))
                .and_then(|engine| engine.simulate_steps(guide));
            if let Err((failed, error)) = outcome {
                let name = failed.and_then(|failed| {
                    validated_components
                        .iter()
                        .find(|component| std::ptr::eq(**component, failed))
                        .map(|component| component.name.as_str())
                });
                report.simulation_failure(name, format!("{error:#}"));
            }
        }
        if dry_run {
            boundary(
                Stage::DryRun,
                "Running instruction execution validation (dry-run)",
                false,
            );
            let issues = match fresh_engine() {
                Ok(engine) => engine.validate_dry_run(guide),
                Err(error) => vec![crate::validation::DryRunIssue::engine(
                    format!("{error:#}"),
                    None,
                )],
            };
            report.dry_run(issues);
            boundary(
                Stage::DryRun,
                "Running instruction execution validation (dry-run)",
                true,
            );
        }
        finish(report)
    }

    /// Plain `validate`'s step simulation for the report without a dry run: the
    /// remaining components in install order, stopping at the first failure, which
    /// names the component it stopped in (none for an ordering error).
    fn simulate_steps(
        mut self,
        guide: &Guide,
    ) -> std::result::Result<(), (Option<&Component>, anyhow::Error)> {
        let ordered = self
            .remaining_ordered(guide)
            .map_err(|error| (None, error))?;
        self.load_selection(guide);
        for component in ordered {
            self.run_selected(&[component], &mut |_| {}, None)
                .map_err(|error| (Some(component), error))?;
        }
        Ok(())
    }

    fn validate_dry_run(mut self, guide: &Guide) -> Vec<crate::validation::DryRunIssue> {
        if let Err(error) = recovery::ensure_clean(&self.settings.roots)
            .and_then(|_| self.verify_completion_for_backend(guide, self.completed.len()))
        {
            return vec![crate::validation::DryRunIssue::engine(
                format!("{error:#}"),
                None,
            )];
        }
        let ordered = match guide.ordered() {
            Ok(ordered) => ordered,
            Err(error) => {
                return vec![crate::validation::DryRunIssue::engine(
                    format!("{error:#}"),
                    None,
                )];
            }
        };
        self.selected = selected_guid_keys(guide);
        self.selected_names = guide
            .components
            .iter()
            .filter(|component| component.is_selected)
            .map(|component| component.name.clone())
            .collect();
        let mut issues = Vec::new();
        if let Err(error) = self.prepare_k2(guide, None, &mut |_| {}) {
            return vec![crate::validation::DryRunIssue::engine(
                format!("{error:#}"),
                None,
            )];
        }
        for component in ordered {
            if self
                .completed
                .contains(&crate::guid::key_ascii(&component.guid))
            {
                continue;
            }
            self.validation_target = Some((component.name.clone(), component.guid.clone(), None));
            let component_issue_start = issues.len();
            if component.instructions.is_empty() {
                issues.push(crate::validation::DryRunIssue::engine(
                    format!(
                        "{} has no executable instructions; review the guide",
                        component.name
                    ),
                    self.validation_target.clone(),
                ));
                continue;
            }
            if let Err(error) = self.check_component(component).and_then(|_| {
                crate::archive_generation::reviewed_manual::verify_runtime(
                    component,
                    &self.settings.roots.mods,
                    &self.settings.archive,
                    &self.settings.cancelled,
                )
            }) {
                issues.push(crate::validation::DryRunIssue::engine(
                    format!("{error:#}"),
                    self.validation_target.clone(),
                ));
                continue;
            }
            for (index, instruction) in component.instructions.iter().enumerate() {
                self.validation_target = Some((
                    component.name.clone(),
                    component.guid.clone(),
                    Some(index + 1),
                ));
                if !self.instruction_applies(instruction) {
                    continue;
                }
                let result = self
                    .check_cancelled()
                    .and_then(|_| self.execute_indexed(component, instruction, index, &mut |_| {}))
                    .with_context(|| {
                        format!(
                            "{} / step {} ({})",
                            component.name,
                            index + 1,
                            instruction.action
                        )
                    });
                issues.append(&mut self.validation_warnings);
                if let Err(error) = result {
                    if error.is::<MissingSourceFiles>() {
                        // Keep successful virtual effects, but replace this component's
                        // provisional warnings with its single component-level outcome.
                        issues.truncate(component_issue_start);
                        issues.push(crate::validation::DryRunIssue::skipped(component));
                        break;
                    }
                    // A chosen option's steps retarget validation at the option; C# reports
                    // the failure against the component and its step.
                    self.validation_target = Some((
                        component.name.clone(),
                        component.guid.clone(),
                        Some(index + 1),
                    ));
                    let message = format!("{error:#}");
                    let mut issue = crate::validation::DryRunIssue::engine(
                        message.clone(),
                        self.validation_target.clone(),
                    );
                    if let Some(missing) = error.downcast_ref::<MissingSource>() {
                        issue.affected_path = Some(missing.0.clone());
                    }
                    issues.push(issue);
                    if self.settings.cancelled.load(Ordering::Relaxed) {
                        return issues;
                    }
                    break;
                }
            }
        }
        issues
    }

    /// Read-only archive/destination checks; never execute installation instructions.
    /// `full_validation` is the full pipeline, whose Environment stage runs first.
    fn validate_archives(
        mut self,
        guide: &Guide,
        emit: &mut impl FnMut(Event),
        full_validation: bool,
    ) -> Result<Vec<crate::validation::ArchiveOutcome>> {
        recovery::ensure_clean(&self.settings.roots)?;
        self.verify_completion_for_backend(guide, self.completed.len())?;
        self.prepare_k2(guide, None, emit)?;
        self.selected = selected_guid_keys(guide);
        self.selected.extend(self.completed.iter().cloned());
        let mut outcomes = Vec::new();
        for component in guide.ordered()?.into_iter().filter(|component| {
            !self
                .completed
                .contains(&crate::guid::key_ascii(&component.guid))
        }) {
            // Missing archives and Destination errors are one error per step, as
            // the reference lists them; any other failure stops this component's
            // checks with one error. Warnings show only when there is no error.
            let outcome = match self.validate_component_archives(component, full_validation) {
                Ok((errors, warnings)) if errors.is_empty() => Ok(warnings),
                Ok((errors, _)) => Err(errors
                    .into_iter()
                    .map(|error| format!("{}: {error}", component.name))
                    .collect()),
                Err(error) => Err(vec![format!(
                    "{:#}",
                    error.context(format!(
                        "{}: archive/destination validation",
                        component.name
                    ))
                )]),
            };
            emit(Event {
                component: component.name.clone(),
                step: 0,
                message: "Archive and destination checks completed".into(),
            });
            outcomes.push(outcome);
        }
        Ok(outcomes)
    }

    /// Returns the reference's per-step errors, if any: `Missing Required Archives`,
    /// or else `Destination cannot be found!`, `'Destination' key cannot be used
    /// with this action` and the Patcher Destination error; and its warnings (an
    /// empty Patcher Destination). Other failures are errors.
    ///
    /// In the full pipeline C#'s Environment stage has already run these checks with
    /// AttemptFixes and created each missing Destination folder it could, so this
    /// stage finds them. Rust creates nothing while validating; it leaves out a
    /// missing Copy, Move or CleanList folder that could be created, since executing
    /// the step creates it (or, for CleanList, finds nothing to delete).
    fn validate_component_archives(
        &self,
        component: &Component,
        full_validation: bool,
    ) -> Result<(Vec<String>, Vec<String>)> {
        self.check_cancelled()?;
        let instructions = Self::archive_check_instructions(component, &self.selected);
        let ArchiveMembers {
            members,
            has_archives,
            unknown,
            mut missing,
        } = self.archive_members(&instructions)?;
        // C# reports an own Extract with no Source (`Missing Required Archives: []`)
        // only when the component has no archive; otherwise execution refuses it as
        // a missing source, as C#'s install does.
        missing.retain(|instruction| {
            !instruction.source.is_empty()
                || (!has_archives && is_top_level(component, instruction))
        });
        if !missing.is_empty() {
            // C# `ComponentValidation.VerifyExtractPaths`: when none of the component's
            // archives exists, each top-level Extract is one error naming its Sources,
            // and the other checks are skipped. Rust also refuses an option-only or a
            // partly missing Extract there (fail closed), where C# reports nothing.
            if !has_archives
                && missing
                    .iter()
                    .any(|instruction| is_top_level(component, instruction))
            {
                missing.retain(|instruction| is_top_level(component, instruction));
            }
            let errors = missing
                .into_iter()
                .map(|instruction| {
                    format!(
                        "Missing Required Archives: [{}]",
                        instruction.source.join(",")
                    )
                })
                .collect();
            return Ok((errors, Vec::new()));
        }
        // C# `ParseDestinationWithAction` reports each step whose Destination folder is
        // missing, or that has a Destination its action cannot use, and goes on; any
        // other failure still ends this component's checks.
        let mut step_errors = Vec::new();
        let mut warnings = Vec::new();
        for (instruction, _) in instructions {
            self.check_cancelled()?;
            match instruction.action.as_str() {
                "ExtractTo" => {
                    let source = self.extension_source_path(instruction)?;
                    self.extraction_target(&source, instruction)?;
                }
                "CopyFile" => {
                    self.copy_file_destination(instruction)?;
                }
                "K1Widescreen" => {
                    crate::k1_widescreen_recipe::inspect_role(component)?
                        .context("Unbound K1 native action")?;
                    ensure!(
                        instruction.destination == "<<kotorDirectory>>",
                        "K1 action requires the compiled game destination"
                    );
                    self.path(&instruction.destination, false)?;
                }
                "Copy" | "Move" | "CleanList" | "DelDuplicate"
                    if instruction.destination.is_empty() =>
                {
                    step_errors.push("Destination cannot be found! Got ''".to_owned());
                }
                "Copy" | "Move" | "CleanList" | "DelDuplicate" => {
                    let dest = self.path(&instruction.destination, false)?;
                    let missing = !self.directories.contains_key(&self.path_policy.key(&dest));
                    // The full pipeline's C# Environment stage has created this folder.
                    let created = full_validation
                        && instruction.action != "DelDuplicate"
                        && self.folder_creatable(&dest);
                    // C# PathValidator.IsValidPath refuses these parts.
                    if invalid_destination(&instruction.destination) || missing && !created {
                        step_errors.push(format!(
                            "Destination cannot be found! Got '{}'",
                            self.shown_destination(&dest)
                        ));
                    }
                }
                "Rename" if !instruction.destination.starts_with("<<") => {
                    rename_file_name(&instruction.destination)?;
                }
                // This stage also checks unselected options' steps, which C# never
                // runs; a placeholder Rename Destination fails where its step runs
                // (`rename_file_name`), as the C# install does.
                "Rename" => {
                    self.path(&instruction.destination, false)?;
                }
                // C# warns about an empty Patcher Destination and reports any other
                // than the game folder's placeholder, then sets it (AttemptFixes). In
                // the full pipeline its Environment stage has already set it, so
                // only the basic report shows them. Rust never uses it (the patcher
                // runs on the game directory); the step refuses an unsafe one. The
                // placeholder in another case is not found, in both (fail closed).
                "Patcher" if instruction.destination.is_empty() => {
                    if !full_validation {
                        warnings.push(
                            "Destination must be <<gameDirectory>> with 'Patcher' action, \
                             setting it now automatically."
                                .to_owned(),
                        );
                    }
                }
                "Patcher" if game_placeholder(&instruction.destination) => {
                    self.path(&instruction.destination, false)?;
                }
                "Patcher" if full_validation => {}
                "Patcher" => step_errors.push(format!(
                    "'Destination' key must be either null or string literal \
                     '<<gameDirectory>>' (legacy: '<<kotorDirectory>>') for this action. \
                     Got '{}'",
                    instruction.destination
                )),
                // C# reports the Destination, then clears it (AttemptFixes). In the
                // full pipeline its Environment stage has already cleared it, so only
                // the basic report fails. Rust never uses it (`extraction_destination`).
                "Extract" | "Choose" | "Delete"
                    if !full_validation && !instruction.destination.is_empty() =>
                {
                    step_errors.push(format!(
                        "'Destination' key cannot be used with this action. Got '{}'",
                        instruction.destination
                    ));
                }
                "Extract" | "Choose" | "Delete" | "Execute" | "Run" => {}
                other => bail!("Unsupported action {other}; refusing to skip it"),
            }
            if has_archives {
                self.check_archive_sources(component, instruction, |prefix, pattern| {
                    let found = members
                        .iter()
                        .any(|member| self.source_matches(prefix, pattern, member));
                    Ok((found || !unknown).then_some(found))
                })?;
            }
        }
        Ok((step_errors, warnings))
    }

    /// The steps C# `ComponentValidation` checks against the component's archives:
    /// the component's and every immediate option's, selected or not, whose
    /// instruction conditions are met, each with whether it runs (the component's
    /// own, or a chosen option's; every step when a Choose names an unknown option,
    /// which execution refuses).
    fn archive_check_instructions<'a>(
        component: &'a Component,
        selected: &BTreeSet<crate::guid::Identity>,
    ) -> Vec<(&'a Instruction, bool)> {
        let mut chosen = Some(Vec::new());
        for instruction in &component.instructions {
            if instruction.action == "Choose" && applies_with(selected, instruction) {
                match (Self::chosen_options(component, instruction), &mut chosen) {
                    (Ok(options), Some(chosen)) => chosen.extend(options),
                    _ => chosen = None,
                }
            }
        }
        let runs = |option: &Component| {
            chosen
                .as_ref()
                .is_none_or(|chosen| chosen.iter().any(|c| std::ptr::eq(*c, option)))
        };
        component
            .instructions
            .iter()
            .map(|instruction| (instruction, true))
            .chain(component.options.iter().flat_map(|option| {
                let runs = runs(option);
                option
                    .instructions
                    .iter()
                    .map(move |instruction| (instruction, runs))
            }))
            .filter(|(instruction, _)| applies_with(selected, instruction))
            .collect()
    }

    /// Every file the given Extract steps would extract (keys of their paths in the
    /// folder named after each archive), whether any of their archives exists, and
    /// the Extract steps with no Source or with a Source that names no file. Lists
    /// the archives without extracting them. An archive of a step that runs must be
    /// listable (its Extract fails otherwise); one only an option that is not chosen
    /// extracts is skipped when it cannot be listed (`unknown`), as C# opens it only
    /// when a Source needs it.
    fn archive_members<'a>(
        &self,
        instructions: &[(&'a Instruction, bool)],
    ) -> Result<ArchiveMembers<'a>> {
        let mut found = ArchiveMembers {
            members: BTreeSet::new(),
            has_archives: false,
            unknown: false,
            missing: Vec::new(),
        };
        for &(instruction, runs) in instructions {
            self.check_cancelled()?;
            if !matches!(instruction.action.as_str(), "Extract" | "ExtractTo") {
                continue;
            }
            // An Extract with no Source names no archive to inspect.
            if instruction.source.is_empty() {
                found.missing.push(instruction);
                continue;
            }
            for source in &instruction.source {
                if self
                    .extraction_sources(instruction, std::slice::from_ref(source), true)?
                    .is_empty()
                {
                    found.missing.push(instruction);
                    break;
                }
            }
            for source in self.extraction_sources(instruction, &instruction.source, true)? {
                found.has_archives = true;
                let dest = self.extraction_target(&source, instruction)?;
                if !runs {
                    match self.listed_members(&source, &dest)? {
                        Some(members) => found.members.extend(members),
                        None => found.unknown = true,
                    }
                    continue;
                }
                self.settings.roots.check(&dest)?;
                ensure!(
                    !self.files.contains_key(&self.path_policy.key(&dest)),
                    "File blocks extraction destination: {}",
                    dest.display()
                );
                let (entries, _) = archive::list_original_with_size(
                    self.files[&self.path_policy.key(&source)].1.path()?,
                    &source,
                    &self.settings.archive,
                    &self.settings.cancelled,
                )
                .with_context(|| format!("Cannot inspect archive {}", source.display()))?;
                for entry in entries.into_iter().filter(|entry| !entry.is_directory) {
                    let path = dest.join(entry.path);
                    self.settings.roots.check(&path)?;
                    found.members.insert(self.path_policy.key(&path));
                }
            }
        }
        Ok(found)
    }

    /// C# `ComponentValidation.VerifyExtractPaths`, once the component has an archive:
    /// each Source in the mod folder must name a file (or a folder holding one) that
    /// the component's archives extract, or fail with `Failed to find '<Source>' in
    /// any archives`. Game Sources are not checked; a Choose or Extract has no such
    /// Source and a DelDuplicate's Sources are extensions. C# skips the check for a
    /// component named "Improved AI" (`ComponentValidation.cs:461-464`).
    /// `found(prefix, pattern)` says whether the archives hold a matching member;
    /// `None` is not known (an archive could not be listed), which passes.
    fn check_archive_sources(
        &self,
        component: &Component,
        instruction: &Instruction,
        mut found: impl FnMut(Option<&str>, &glob::Pattern) -> Result<Option<bool>>,
    ) -> Result<()> {
        if matches!(
            instruction.action.as_str(),
            "Choose" | "DelDuplicate" | "Extract" | "ExtractTo"
        ) || crate::unicode_case::ordinal_eq(&component.name, "Improved AI")
        {
            return Ok(());
        }
        for source in &instruction.source {
            let (path, pattern) = self.source_pattern(source)?;
            if path.starts_with(&self.settings.roots.game) {
                continue;
            }
            let prefix = self.source_prefix(&path);
            ensure!(
                found(prefix.as_deref(), &pattern)? != Some(false),
                "Failed to find '{source}' in any archives"
            );
        }
        Ok(())
    }

    /// The members one archive extracts (keys of their paths in the folder named
    /// after it), or `None` when Rust cannot list it (a corrupt archive, or one its
    /// admission rules refuse, such as a link or a case-duplicate entry, which C#
    /// opens). Only cancellation is an error.
    fn listed_members(&self, archive: &Path, dest: &Path) -> Result<Option<BTreeSet<String>>> {
        let listed = (|| -> Result<BTreeSet<String>> {
            let (entries, _) = archive::list_original_with_size(
                self.files[&self.path_policy.key(archive)].1.path()?,
                archive,
                &self.settings.archive,
                &self.settings.cancelled,
            )?;
            let mut members = BTreeSet::new();
            for entry in entries.into_iter().filter(|entry| !entry.is_directory) {
                let path = dest.join(entry.path);
                self.settings.roots.check(&path)?;
                members.insert(self.path_policy.key(&path));
            }
            Ok(members)
        })();
        self.check_cancelled()?;
        Ok(listed.ok())
    }

    /// Whether `Directory.CreateDirectory` could create this missing folder: no file
    /// stands at it or at any folder above it.
    fn folder_creatable(&self, dest: &Path) -> bool {
        dest.ancestors()
            .all(|folder| !self.files.contains_key(&self.path_policy.key(folder)))
    }

    /// The Destination folder as C# prints it: the full path, with the folders that
    /// exist in their on-disk spelling (C# `GetCaseSensitivePath`).
    fn shown_destination(&self, dest: &Path) -> String {
        dest.ancestors()
            .find_map(|folder| {
                let on_disk = self.directories.get(&self.path_policy.key(folder))?;
                let rest = dest.strip_prefix(folder).ok()?;
                // `join("")` would add a trailing separator, which C# trims.
                Some(if rest.as_os_str().is_empty() {
                    on_disk.clone()
                } else {
                    on_disk.join(rest)
                })
            })
            .unwrap_or_else(|| dest.to_path_buf())
            .display()
            .to_string()
    }

    /// The gate's soft warnings and its issues; no issue means the gate passed. Like
    /// C# FomodConfigurationGate, every in-gate FOMOD issue is collected in gate order
    /// with the warnings; an abort (cancellation, an unsafe path, a guide error) ends
    /// the list. Issues with identical text (one archive shared by components) are kept once.
    fn validate_fomod_configuration(&mut self, guide: &Guide) -> (Vec<String>, Vec<anyhow::Error>) {
        let mut warnings = crate::fomod::WarningCollector::default();
        let mut issues = Vec::new();
        if let Err(error) = self.check_fomod_configuration(guide, &mut warnings, &mut issues) {
            issues.push(error);
        }
        let mut seen = BTreeSet::new();
        issues.retain(|issue| seen.insert(format!("{issue:#}")));
        (warnings.finish(), issues)
    }

    /// Gates the selected components and, as C# does, their hard dependencies.
    /// Collects warnings for configured registered archives that no selected Extract
    /// instruction installs, or whose installer maps no files for any choice (C#'s
    /// "configured without archive-scoped instructions"): configuration installs nothing.
    /// Mapper fallbacks of each checked installer are logged once per archive, as C#
    /// logs them, outside the report. Reads archives into scratch only; never writes
    /// the game or mod trees.
    fn check_fomod_configuration(
        &mut self,
        guide: &Guide,
        warnings: &mut crate::fomod::WarningCollector,
        issues: &mut Vec<anyhow::Error>,
    ) -> Result<()> {
        self.prepare_k2(guide, None, &mut |_| {})?;
        fn configured_choices(
            component: &Component,
            filename: &str,
            module: &crate::fomod::ArchiveModule,
        ) -> Result<()> {
            let choices = crate::fomod::required_saved_choices(component, filename)?;
            crate::fomod::current_archive_choices(module, &choices, filename)
                .with_context(|| crate::fomod::stale_choices_context(filename))?;
            Ok(())
        }

        // C# gates registered download artifacts even when no Extract instruction names them.
        // A previously recorded prompt state makes a missing artifact fail closed.
        self.selected = selected_guid_keys(guide);
        self.selected.extend(self.completed.iter().cloned());
        // FOMOD findings come first, as C# FomodConfigurationGate reports them
        // whatever the selection. Rust's stricter selection, restriction and order
        // checks still fail the gate afterwards (C# only warns for a dependency).
        let ordered = guide.ordered();
        let seeds: Vec<&Component> = match &ordered {
            Ok(ordered) => ordered.clone(),
            Err(_) => guide.components.iter().filter(|c| c.is_selected).collect(),
        };
        let mut scope = Vec::new();
        let mut visited = BTreeSet::new();
        let mut queue = std::collections::VecDeque::new();
        for component in seeds {
            let key = crate::guid::key_ascii(&component.guid);
            if self.completed.contains(&key) {
                continue;
            }
            visited.insert(key);
            queue.push_back(component);
            self.collect_components(component, &mut scope, false)?;
        }
        // C# ExpandWithHardDependencies: hard dependencies, selected or not, are in
        // scope transitively. An unselected one only has its registered archives
        // gated (the C# gate checks nothing else); unknown GUIDs are ignored.
        let mut dependencies = Vec::new();
        while let Some(component) = queue.pop_front() {
            for required in &component.dependencies {
                let key = crate::guid::key_ascii(required);
                if self.completed.contains(&key) || visited.contains(&key) {
                    continue;
                }
                if let Some(dependency) = guide
                    .components
                    .iter()
                    .find(|c| crate::guid::key_ascii(&c.guid) == key)
                {
                    visited.insert(key);
                    dependencies.push(dependency);
                    queue.push_back(dependency);
                }
            }
        }
        // Registered archives with valid choices, by path: filename and registrants.
        let mut configured = BTreeMap::<String, (String, Vec<String>)>::new();
        // (archive path, component) pairs already checked by the registered pass.
        let mut checked = BTreeSet::new();
        // Each archive path is inspected once per gate, whichever component names it.
        let mut inspected = BTreeMap::<String, Option<crate::fomod::ArchiveModule>>::new();
        let mut routes = BTreeSet::new();
        // Checked FOMOD archive paths whose mapper fallbacks were already logged.
        let mut logged = BTreeSet::new();
        for component in scope.iter().chain(&dependencies) {
            let mut registered = BTreeSet::new();
            for resource in component.resource_registry.values() {
                let Some(files) = resource
                    .get("Files")
                    .or_else(|| resource.get("files"))
                    .and_then(serde_json::Value::as_object)
                else {
                    continue;
                };
                for name in files.keys() {
                    if !is_archive(name) || !registered.insert(crate::fomod::fold_case(name)) {
                        continue;
                    }
                    let relative_name = relative(name, false).with_context(|| {
                        format!("Registered FOMOD archive path is unsafe: {name}")
                    })?;
                    let path = self.settings.roots.mods.join(&relative_name);
                    let filename = relative_name
                        .file_name()
                        .and_then(|name| name.to_str())
                        .context("Registered FOMOD archive filename is invalid")?;
                    let archive_key = self.path_policy.key(&path);
                    if !self.files.contains_key(&archive_key) {
                        if crate::fomod::prompt_status(component, filename).is_some() {
                            issues.push(anyhow::anyhow!(
                                "Registered FOMOD archive '{filename}' is missing from the mod directory after configuration state was recorded"
                            ));
                            continue;
                        }
                        // C#'s soft warning, attributed per component like its report.
                        warnings.push(format!(
                            "{}: Registered archive '{}' is not on disk under the mod directory. Re-download the archive (Fetch Downloads), then configure the FOMOD installer before validate/install.",
                            crate::fomod::warning_text(&component.name),
                            crate::fomod::warning_text(filename)
                        ));
                        continue;
                    }
                    checked.insert((self.path_policy.key(&path), component.guid.clone()));
                    let module = match inspected.entry(self.path_policy.key(&path)) {
                        btree_map::Entry::Occupied(entry) => entry.into_mut(),
                        btree_map::Entry::Vacant(entry) => match crate::fomod::inspect_listed(
                            &path,
                            &relative_name,
                            &self.settings.archive,
                            &self.settings.cancelled,
                        )
                        .with_context(|| {
                            format!("Registered archive '{filename}' cannot be inspected")
                        }) {
                            Ok(module) => entry.insert(module),
                            // C# GateIssue ArchiveUnreadable; cancellation still aborts.
                            Err(error) => {
                                self.gate_issue(issues, error)?;
                                continue;
                            }
                        },
                    };
                    if let Some(module) = module {
                        if let Err(error) = configured_choices(component, filename, module) {
                            issues.push(error);
                            continue;
                        }
                        // C# FomodConfigurationGate R1, per component: a configured
                        // archive whose installer maps no archive-scoped instruction.
                        if crate::fomod::prompt_status(component, filename).as_deref()
                            == Some("configured")
                            && !module.config.maps_any_file()
                        {
                            warnings.push(format!(
                                "{}: FOMOD archive '{}' is marked configured but has no matching archive-scoped instructions. Its installer maps no files for any choice, so this configuration installs nothing from the archive.",
                                crate::fomod::warning_text(&component.name),
                                crate::fomod::warning_text(filename)
                            ));
                        }
                        if logged.insert(self.path_policy.key(&path)) {
                            for warning in module.config.fallback_warnings() {
                                warnings.push(format!(
                                    "FOMOD archive '{}': {warning}",
                                    crate::fomod::warning_text(filename)
                                ));
                            }
                        }
                        configured
                            .entry(self.path_policy.key(&path))
                            .or_insert_with(|| (filename.to_owned(), Vec::new()))
                            .1
                            .push(component.name.clone());
                    }
                }
            }
        }
        for component in &scope {
            for instruction in &component.instructions {
                if !matches!(instruction.action.as_str(), "Extract" | "ExtractTo")
                    || !self.instruction_applies(instruction)
                {
                    continue;
                }
                // Missing generic Extract inputs belong to archive validation and
                // dry-run availability, not the independent FOMOD state gate.
                if instruction.source.is_empty() {
                    continue;
                }
                for source in self.extraction_sources(instruction, &instruction.source, true)? {
                    // Any applying Extract of this path is an install route, FOMOD or not.
                    routes.insert(self.path_policy.key(&source));
                    if checked.contains(&(self.path_policy.key(&source), component.guid.clone())) {
                        continue;
                    }
                    let module = match inspected.entry(self.path_policy.key(&source)) {
                        btree_map::Entry::Occupied(entry) => entry.into_mut(),
                        btree_map::Entry::Vacant(entry) => match crate::fomod::inspect_listed(
                            self.files[&self.path_policy.key(&source)].1.path()?,
                            &source,
                            &self.settings.archive,
                            &self.settings.cancelled,
                        ) {
                            Ok(module) => entry.insert(module),
                            Err(error) => {
                                self.gate_issue(issues, error)?;
                                continue;
                            }
                        },
                    };
                    let Some(module) = module else {
                        continue;
                    };
                    let filename = source
                        .file_name()
                        .and_then(|name| name.to_str())
                        .context("FOMOD archive filename is invalid")?;
                    if let Err(error) = configured_choices(component, filename, module) {
                        issues.push(error);
                        continue;
                    }
                    if logged.insert(self.path_policy.key(&source)) {
                        for warning in module.config.fallback_warnings() {
                            warnings.push(format!(
                                "FOMOD archive '{}': {warning}",
                                crate::fomod::warning_text(filename)
                            ));
                        }
                    }
                }
            }
        }
        // Rust keeps choices rather than generated instructions, so it also warns when
        // no Extract route installs a configured archive (no C# counterpart).
        // One warning per archive, naming every registering component.
        for warning in configured
            .into_iter()
            .filter(|(path, _)| !routes.contains(path))
            .map(|(_, (filename, names))| {
                let omitted = names.len().saturating_sub(8);
                let names = names
                    .iter()
                    .take(8)
                    .map(|name| format!("'{}'", crate::fomod::warning_text(name)))
                    .collect::<Vec<_>>()
                    .join(", ");
                let extra = if omitted > 0 { format!(" and {omitted} other components") } else { String::new() };
                let filename = crate::fomod::warning_text(&filename);
                format!(
                    "FOMOD archive '{filename}' has valid choices, but no selected Extract instruction installs it; configuration alone installs no files. Registered by {names}{extra}."
                )
            })
        { warnings.push(warning); }
        // FOMOD issues fail the gate before Rust's stricter checks, as before.
        if !issues.is_empty() {
            return Ok(());
        }
        ordered?;
        for component in scope {
            self.check_component(component)?;
        }
        Ok(())
    }

    /// Analyze actual simulated writes without changing either installation tree.
    /// Failed instructions return an error rather than an incomplete success report.
    pub fn analyze_conflicts(mut self, guide: &Guide) -> Result<crate::conflicts::Report> {
        ensure!(
            self.path_policy.case_insensitive,
            "Conflict reports currently require case-insensitive path matching"
        );
        recovery::ensure_clean(&self.settings.roots)?;
        self.conflicts = Some(crate::conflicts::Tracker::default());
        self.run(guide, &mut |_| {})?;
        Ok(self.conflicts.take().unwrap().finish())
    }

    pub fn install(self, guide: &Guide, emit: impl FnMut(Event)) -> Result<()> {
        self.install_observed(guide, emit, |_| {})
    }

    /// Observe real installation boundaries, excluding all silent preflight work.
    /// The observer is synchronous and non-fallible; it must not panic. Adapters
    /// may ignore failed channel delivery. Completed and Ok win over late cancel.
    pub fn install_observed(
        self,
        guide: &Guide,
        emit: impl FnMut(Event),
        progress: impl FnMut(InstallProgress),
    ) -> Result<()> {
        self.install_observed_report(guide, emit, progress)
            .map(|_| ())
    }

    pub fn install_observed_report(
        mut self,
        guide: &Guide,
        mut emit: impl FnMut(Event),
        mut progress: impl FnMut(InstallProgress),
    ) -> Result<InstallReport> {
        let runner_checkpoint = self.requires_external_writer_checkpoint(guide)?;
        let continuation_checkpoint = self.requires_checkpoint_continuation()?;
        ensure!(
            !self.checkpoint_history_forbidden
                || !(self.component_checkpoints || runner_checkpoint || continuation_checkpoint),
            "--no-checkpoint conflicts with required component checkpoints for native K1/source-plan operations, resolved source masters, supplied installers or checkpointed phase continuation"
        );
        if (runner_checkpoint || continuation_checkpoint) && !self.component_checkpoints {
            self.component_checkpoints = true;
            emit(Event {
                component: String::new(),
                step: 0,
                message: "Native K1/source-plan operations, resolved source masters, supplied installer execution or checkpointed phase continuation require retained component checkpoints for all installation roots".into(),
            });
        }
        ensure!(
            self.path_policy.case_insensitive
                || (!self.component_checkpoints
                    && self.checkpoint_resume.is_none()
                    && self.managed.is_none()),
            "Exact path matching cannot be combined with managed deployment or checkpoint history"
        );
        #[cfg(windows)]
        let lock = if self.component_checkpoints {
            let participants = self
                .managed
                .as_ref()
                .map(|managed| managed.service.checkpoint_participants())
                .transpose()?
                .unwrap_or_default();
            let context = crate::checkpoint::InstallContext {
                participants,
                ..crate::checkpoint::InstallContext::default()
            };
            crate::checkpoint::Store::open(&self.settings.roots)?
                .acquire_lock_with_context(&context)?
        } else {
            recovery::Lock::acquire(&self.settings.roots)?
        };
        #[cfg(not(windows))]
        let lock = recovery::Lock::acquire(&self.settings.roots)?;
        let lock = std::sync::Arc::new(lock);
        ensure!(
            !self.requires_checkpoint_continuation()? || self.component_checkpoints,
            "Checkpointed phase receipt changed while acquiring root locks; retry with component checkpoints"
        );
        ensure!(
            self.managed.is_none() || self.component_checkpoints,
            "Managed installation requires component checkpoints"
        );
        ensure!(
            self.checkpoint_resume.is_none() || self.component_checkpoints,
            "Checkpoint resume requires component checkpoints"
        );
        let resume_plan = if let Some(request) = &self.checkpoint_resume {
            let definitions = guide
                .ordered()?
                .iter()
                .map(serde_json::to_value)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let participants = self
                .managed
                .as_ref()
                .map(|managed| managed.service.checkpoint_participants())
                .transpose()?
                .unwrap_or_default();
            let store = crate::checkpoint::Store::open(&self.settings.roots)?;
            let backend_binding = self.checkpoint_backend_binding(guide)?;
            let plan = store.resume_plan_with_backend(
                request,
                &definitions,
                &participants,
                backend_binding.as_ref(),
                &self.settings.cancelled,
            )?;
            let completed = plan
                .completed_components
                .iter()
                .map(|definition| {
                    definition
                        .get("Guid")
                        .and_then(serde_json::Value::as_str)
                        .map(crate::guid::key_ascii)
                        .context("Resume prefix has no component Guid")
                })
                .collect::<Result<BTreeSet<_>>>()?;
            ensure!(
                self.completed.is_subset(&completed),
                "Requested phase prefix advances beyond the checkpoint resume boundary"
            );
            self.completed = completed;
            if !plan.source_changes.is_empty() || plan.source_root_mode_changed {
                emit(Event {
                    component: String::new(),
                    step: 0,
                    message: format!(
                        "Accepted source changes for checkpoint branch: {}",
                        serde_json::to_string(
                            &serde_json::json!({"paths": &plan.source_changes, "rootModeChanged": plan.source_root_mode_changed})
                        )?
                    ),
                });
            }
            Some(plan)
        } else {
            recovery::ensure_clean(&self.settings.roots)?;
            None
        };
        // C#'s install runs the full validation pipeline first, whose Environment
        // stage refuses a game or mod folder it cannot write to.
        if let Some(message) = unwritable_root(&self.settings.roots) {
            bail!("Installation blocked before any write: {message}");
        }
        if let Some(managed) = &self.managed {
            managed.service.validate_readonly()?;
        }
        self.scan()?;
        // A component HEAD can be newer than the legacy receipt until Finalize.
        // ResumePlan already proves the exact checkpoint prefix and live inventory;
        // its branch transaction reconciles the legacy receipt before execution.
        if resume_plan.is_none() {
            self.verify_completion_for_backend(guide, self.completed.len())?;
        }
        self.selected = selected_guid_keys(guide);
        self.selected_names = guide
            .components
            .iter()
            .filter(|component| component.is_selected)
            .map(|component| component.name.clone())
            .collect();
        // C# InstallationService blocks installation on FomodConfigurationGate issues.
        self.fomod_gate(guide, &mut emit)
            .context("Installation blocked before any write")?;
        self.destination_gate(guide)
            .context("Installation blocked before any write")?;
        self.archive_source_gate(guide)
            .context("Installation blocked before any write")?;
        // Managed reinstall restores originals before each component. Simulating
        // against the old deployed tree would validate the wrong inputs. Its
        // source preflight runs inside the journaled boundary after reinstall.
        if self.managed.is_none() {
            self.run(guide, &mut |_| {})?;
        } else {
            for component in self.remaining_ordered(guide)? {
                self.check_component(component)?;
            }
        }
        self.scan()?;
        let ordered = self.remaining_ordered(guide)?;
        // Pure validation needs no backend or process permission. Installation still
        // rejects missing authority/backend before taking its recovery snapshot.
        for component in &ordered {
            let mut scope = Vec::new();
            self.collect_execution_components(component, &mut scope)?;
            for owner in scope {
                self.check_component(owner)?;
                for instruction in &owner.instructions {
                    if matches!(instruction.action.as_str(), "Execute" | "Run")
                        && self.instruction_applies(instruction)
                    {
                        ensure!(
                            self.settings.allow_processes,
                            "{} needs explicit trust: enable external programs (--allow-processes)",
                            instruction.action
                        );
                        ensure!(
                            !instruction.source.is_empty(),
                            "External program instruction has no Source"
                        );
                        process_arguments::expand(&instruction.arguments, &self.settings.roots)?;
                    }
                    if instruction.action == "Patcher" && self.instruction_applies(instruction) {
                        ensure!(
                            instruction.source.len() == 1,
                            "Patcher needs exactly one source"
                        );
                        if self.requires_process_trust("Patcher") {
                            ensure!(
                                self.settings.allow_processes,
                                "Patcher needs explicit trust: enable external programs (--allow-processes)"
                            );
                        }
                        if let Some(error) = &self.patcher_resolution_error {
                            bail!("Patcher backend unavailable: {error}");
                        }
                        self.check_patcher_backend()?;
                    }
                }
            }
        }
        let mut observer = InstallObserver::new(ordered.len(), &mut progress);
        observer.emit(InstallPhase::Preparing, "Installation preflight completed");
        emit(Event {
            component: String::new(),
            step: 0,
            message: "Creating recovery snapshot of game and mod directories".into(),
        });
        self.check_cancelled()?;
        observer.emit(
            InstallPhase::CreatingSnapshot,
            "Creating recovery snapshot of game and mod directories",
        );
        if resume_plan.is_none() {
            recovery::begin(&self.settings.roots)?;
        }
        observer.current.snapshot_pending = true;
        observer.emit(
            InstallPhase::CreatingSnapshot,
            if resume_plan.is_some() {
                "Preparing verified checkpoint continuation branch"
            } else {
                "Recovery snapshot active"
            },
        );
        self.install = true;
        if self.component_checkpoints {
            self.run_checkpointed(
                guide,
                &ordered,
                &lock,
                resume_plan.as_ref(),
                &mut emit,
                &mut observer,
            )?;
        } else {
            self.run_ordered(guide, &ordered, &mut emit, Some(&mut observer))
                .with_context(|| {
                    format!(
                        "Install stopped. Restore before retrying; snapshot: {}",
                        recovery::state_dir(&self.settings.roots).display()
                    )
                })?;
            observer.clear_component();
            observer.emit(
                InstallPhase::RecordingCompletion,
                "Recording installation completion",
            );
            recovery::record_completion(&self.settings.roots, guide)?;
            recovery::finish(&self.settings.roots)?;
        }
        observer.current.snapshot_pending = false;
        observer.emit(InstallPhase::Completed, "Installation complete");
        Ok(InstallReport {
            managed: self.managed.map(|managed| managed.report),
            checkpoint: self.checkpoint_report,
        })
    }

    /// Execute the remaining suffix while retaining selected dependency context.
    /// A saved receipt must match roots, prefix definitions and current game bytes.
    pub fn install_remaining(
        self,
        guide: &Guide,
        completed: &[String],
        emit: impl FnMut(Event),
    ) -> Result<()> {
        self.install_remaining_observed(guide, completed, emit, |_| {})
    }

    /// Observe the same verified suffix execution as install_remaining.
    pub fn install_remaining_observed(
        mut self,
        guide: &Guide,
        completed: &[String],
        emit: impl FnMut(Event),
        progress: impl FnMut(InstallProgress),
    ) -> Result<()> {
        self.set_completed_prefix(guide, completed)?;
        self.install_observed(guide, emit, progress)
    }

    pub fn install_remaining_observed_report(
        mut self,
        guide: &Guide,
        completed: &[String],
        emit: impl FnMut(Event),
        progress: impl FnMut(InstallProgress),
    ) -> Result<InstallReport> {
        self.set_completed_prefix(guide, completed)?;
        self.install_observed_report(guide, emit, progress)
    }

    /// Simulate only the suffix, with the same saved completion contract
    /// as install_remaining. Does not write install trees or run external programs.
    pub fn validate_remaining(
        mut self,
        guide: &Guide,
        completed: &[String],
        emit: impl FnMut(Event),
    ) -> Result<()> {
        self.set_completed_prefix(guide, completed)?;
        self.validate(guide, emit)
    }

    /// The full report for a game folder the engine refused for names that differ
    /// only in case. C#'s Environment stage lists such names
    /// (`InstallationService.ValidateInstallationEnvironmentAsync`) and fails with
    /// its duplicate message, which it sets last, so it replaces every other
    /// Environment message; the report then stops. `error` is the refusal of
    /// [`Engine::new`]; `None` keeps it (another error, or a collision only in the
    /// mod folder, which C# does not check). Rust's recovery-state checks still
    /// come first, as in [`Engine::validate_report_with_options_observed`].
    pub fn case_collision_report(
        settings: &Settings,
        case_insensitive: bool,
        error: &anyhow::Error,
        guide: &Guide,
        completed: &[String],
    ) -> Option<crate::validation::Report> {
        error.downcast_ref::<CaseCollision>()?;
        let policy = crate::paths::PathPolicy::new(case_insensitive).ok()?;
        let roots = &settings.roots;
        // Walk the game folder alone even when the scan stopped inside it: the
        // scan's first copy of a name may sit in the mod folder when the two
        // roots differ only in case.
        let path = game_case_collision(&roots.game, policy)?;
        let completed_keys: BTreeSet<_> = completed
            .iter()
            .map(|id| crate::guid::key_ascii(id))
            .collect();
        let count = guide
            .components
            .iter()
            .filter(|c| c.is_selected)
            .filter(|c| !completed_keys.contains(&crate::guid::key_ascii(&c.guid)))
            .count();
        let mut report = crate::validation::Report::new(count);
        let environment = recovery::ensure_clean(roots)
            .and_then(|()| recovery::verify_completion(roots, guide, completed.len()))
            .map_err(|error| format!("{error:#}"));
        match environment {
            Ok(()) => report.environment_case_duplicates(&path),
            environment => report.push(crate::validation::Stage::Environment, environment),
        };
        Some(report.finish())
    }

    /// Run the shared wizard report on the uninstalled suffix while retaining
    /// completed components as dependency context and verifying their receipt.
    pub fn validate_report_remaining(
        self,
        guide: &Guide,
        completed: &[String],
        emit: impl FnMut(Event),
    ) -> Result<crate::validation::Report> {
        self.validate_report_remaining_observed(guide, completed, emit, |_| {})
    }

    pub fn validate_report_remaining_observed(
        mut self,
        guide: &Guide,
        completed: &[String],
        emit: impl FnMut(Event),
        progress: impl FnMut(crate::validation::StageProgress),
    ) -> Result<crate::validation::Report> {
        self.set_completed_prefix(guide, completed)?;
        Ok(self.validate_report_with_options_observed(guide, emit, true, true, false, progress))
    }

    /// Analyze only the uninstalled suffix under the same verified completion
    /// contract as validate_remaining. Neither install tree is changed.
    pub fn analyze_conflicts_remaining(
        mut self,
        guide: &Guide,
        completed: &[String],
    ) -> Result<crate::conflicts::Report> {
        self.set_completed_prefix(guide, completed)?;
        self.verify_completion_for_backend(guide, self.completed.len())?;
        self.analyze_conflicts(guide)
    }

    fn verify_completion_for_backend(&self, guide: &Guide, completed: usize) -> Result<()> {
        recovery::verify_completion(&self.settings.roots, guide, completed)?;
        if completed > 0 {
            let binding = self.checkpoint_backend_binding(guide)?;
            crate::checkpoint::Store::open(&self.settings.roots)?
                .verify_completion_backend(binding.as_ref())?;
        }
        Ok(())
    }

    fn set_completed_prefix(&mut self, guide: &Guide, completed: &[String]) -> Result<()> {
        let ordered = guide.ordered()?;
        let ids: BTreeSet<_> = completed
            .iter()
            .map(|id| crate::guid::key_ascii(id))
            .collect();
        ensure!(
            ids.len() == completed.len(),
            "Duplicate completed component ID"
        );
        ensure!(
            ids.len() < ordered.len(),
            "No remaining components to install"
        );
        let prefix: BTreeSet<_> = ordered
            .iter()
            .take(ids.len())
            .map(|c| crate::guid::key_ascii(&c.guid))
            .collect();
        ensure!(
            ids == prefix,
            "Completed components must form the selected installation prefix"
        );
        self.completed = ids;
        Ok(())
    }

    /// Collect a per-archive inspection failure as a gate issue, unless the run was
    /// cancelled: cancellation aborts the gate with that error.
    fn gate_issue(&self, issues: &mut Vec<anyhow::Error>, error: anyhow::Error) -> Result<()> {
        if self.settings.cancelled.load(Ordering::Relaxed) {
            return Err(error);
        }
        issues.push(error);
        Ok(())
    }

    fn check_cancelled(&self) -> Result<()> {
        ensure!(
            !self.settings.cancelled.load(Ordering::Relaxed),
            "Cancelled; no later instructions ran"
        );
        Ok(())
    }

    fn remaining_ordered<'a>(&self, guide: &'a Guide) -> Result<Vec<&'a Component>> {
        Ok(guide
            .ordered()?
            .into_iter()
            .filter(|c| !self.completed.contains(&crate::guid::key_ascii(&c.guid)))
            .collect())
    }

    /// A completed checkpointed phase cannot silently downgrade its continuation
    /// when a fresh GUI engine has the optional history preference disabled. This
    /// marker only enables stricter mode; the existing locked prefix/game checks
    /// still verify the receipt before any snapshot or installation writes.
    fn requires_checkpoint_continuation(&self) -> Result<bool> {
        use std::io::Read;
        if self.completed.is_empty() {
            return Ok(false);
        }
        let path = recovery::state_dir(&self.settings.roots).join("completion.json");
        match fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        }
        let file = crate::paths::open_regular_read(&path)?;
        const LIMIT: u64 = 64 * 1024 * 1024;
        ensure!(
            file.metadata()?.len() <= LIMIT,
            "Completion receipt exceeds bound"
        );
        let mut bytes = Vec::new();
        file.take(LIMIT + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= LIMIT,
            "Completion receipt exceeds bound"
        );
        let receipt: serde_json::Value = serde_json::from_slice(&bytes)?;
        let Some(report) = receipt.get("installReport") else {
            return Ok(false);
        };
        let report: InstallReport = serde_json::from_value(report.clone())?;
        ensure!(
            report.checkpoint.is_some(),
            "Completion install report has no checkpoint identity"
        );
        Ok(true)
    }

    /// Inspect only applying actions and reached options. Runner configuration by
    /// itself does not change an installation that never launches such a runner.
    fn requires_external_writer_checkpoint(&self, guide: &Guide) -> Result<bool> {
        if crate::k2_preparation::verify(guide)?.is_some() {
            return Ok(true);
        }
        let provided = self.settings.provided_installer.is_some()
            || matches!(
                self.settings.patcher_engine,
                PatcherEngine::HoloPatcher | PatcherEngine::ModPackaged
            );
        let execute = self.settings.execute_runner.is_some() || provided;
        fn visit(
            engine: &Engine,
            component: &Component,
            selected: &BTreeSet<crate::guid::Identity>,
            provided: bool,
            execute: bool,
            depth: usize,
        ) -> Result<bool> {
            engine.check_cancelled()?;
            ensure!(
                depth <= 64,
                "Supplied installer option nesting exceeds bound"
            );
            if crate::archive_generation::reviewed_master::contains_resolved(component) {
                return Ok(true);
            }
            for (operation_index, instruction) in component.instructions.iter().enumerate() {
                if !applies_with(selected, instruction) {
                    continue;
                }
                match instruction.action.as_str() {
                    "K1Widescreen" => return Ok(true),
                    "Execute" | "Run" if execute => return Ok(true),
                    "Patcher"
                        if provided
                            || crate::archive_generation::source_plan_patcher::contains_native(
                                component,
                            )
                            || engine
                                .native_patcher_contract(component, instruction, operation_index)?
                                .is_some_and(|contract| contract.requires_exact_prediction()) =>
                    {
                        return Ok(true);
                    }
                    "Choose" => {
                        for option in Engine::chosen_options(component, instruction)? {
                            if visit(engine, option, selected, provided, execute, depth + 1)? {
                                return Ok(true);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(false)
        }
        let mut selected = selected_guid_keys(guide);
        selected.extend(self.completed.iter().cloned());
        for component in self.remaining_ordered(guide)? {
            if visit(self, component, &selected, provided, execute, 0)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn run(&mut self, guide: &Guide, emit: &mut impl FnMut(Event)) -> Result<()> {
        let ordered = self.remaining_ordered(guide)?;
        self.run_ordered(guide, &ordered, emit, None)
    }

    fn run_ordered(
        &mut self,
        guide: &Guide,
        ordered: &[&Component],
        emit: &mut impl FnMut(Event),
        progress: Option<&mut InstallObserver<'_>>,
    ) -> Result<()> {
        self.prepare_k2(guide, None, emit)?;
        self.load_selection(guide);
        self.run_selected(ordered, emit, progress)
    }

    fn load_selection(&mut self, guide: &Guide) {
        self.completed_source_definitions.clear();
        self.supplemental_source_guide = Some(guide.clone());
        self.selected = selected_guid_keys(guide);
        self.selected_names = guide
            .components
            .iter()
            .filter(|c| c.is_selected)
            .map(|c| c.name.clone())
            .collect();
    }

    /// `run_ordered` after `load_selection`.
    fn run_selected(
        &mut self,
        ordered: &[&Component],
        emit: &mut impl FnMut(Event),
        mut progress: Option<&mut InstallObserver<'_>>,
    ) -> Result<()> {
        for (index, c) in ordered.iter().enumerate() {
            if self.exact_patcher_prediction && c.instructions.is_empty() {
                self.check_component(c)?;
                continue;
            }
            ensure!(
                !c.instructions.is_empty(),
                "{} has no executable instructions; review the guide",
                c.name
            );
            if let Some(tracker) = &mut self.conflicts {
                tracker.begin(c);
            }
            if let Some(observer) = progress.as_deref_mut() {
                observer.component(c, index + 1);
                observer.emit(
                    InstallPhase::InstallingComponent,
                    format!("Installing: {}", c.name),
                );
            }
            let started = std::time::Instant::now();
            let result = self.component(c, emit, progress.as_deref_mut());
            if self.install {
                let error = result.as_ref().err().map(|error| format!("{error:#}"));
                crate::telemetry::global().record_mod_installation(
                    &c.name,
                    result.is_ok(),
                    started.elapsed(),
                    error.as_deref(),
                );
            }
            result?;
            if let Some(observer) = progress.as_deref_mut() {
                observer.component(c, index + 1);
                observer.emit(
                    InstallPhase::ComponentCompleted,
                    format!("Installed: {}", c.name),
                );
            }
        }
        Ok(())
    }

    fn component(
        &mut self,
        c: &Component,
        emit: &mut impl FnMut(Event),
        mut progress: Option<&mut InstallObserver<'_>>,
    ) -> Result<()> {
        let started = std::time::Instant::now();
        let actual_install = self.install;
        let result: Result<()> = (|| {
            self.validation_target = Some((c.name.clone(), c.guid.clone(), None));
            self.check_component(c)?;
            let external_owner = self.verify_original_external_component(c)?;
            if crate::archive_generation::source_plan_namespace::requires_standalone_k2_profile(c) {
                crate::archive_generation::source_plan_namespace::verify_standalone_k2_guide(
                    self.supplemental_source_guide
                        .as_ref()
                        .context("Missing owned standalone guide context")?,
                )?;
                // M4-78 DRO areas are incompatible with the original standalone
                // profile. Check actual VFS inputs too, including resumed installs.
                for (path, _) in self.files.values() {
                    if let Ok(relative) = path.strip_prefix(&self.settings.roots.game) {
                        let relative = relative
                            .to_string_lossy()
                            .replace('\\', "/")
                            .to_ascii_lowercase();
                        ensure!(
                            !(relative.starts_with("modules/")
                                && (relative.ends_with("dro.mod")
                                    || relative.ends_with("dro.rim")
                                    || relative.ends_with("dro_s.rim"))),
                            "Enclave standalone profile rejects installed M4-78 DRO modules"
                        );
                    }
                }
            }
            for predecessor in
                crate::archive_generation::source_plan_namespace::required_predecessors(c)?
            {
                let key = crate::guid::key_ascii(&predecessor);
                ensure!(
                    actual_install || self.exact_patcher_prediction || external_owner,
                    "Supplemental prerequisites require exact native prediction or authenticated external presence validation before source effects"
                );
                if actual_install {
                    ensure!(
                        self.component_checkpoints
                            && self.checkpoint_process.is_some()
                            && self.checkpoint_completed.contains(&key),
                        "{} requires an owned completed predecessor checkpoint {predecessor}",
                        c.name
                    );
                }
                let definition = self.completed_source_definitions.get(&key)
                    .context("Supplemental predecessor has not completed in this prediction or verified checkpoint prefix")?;
                crate::archive_generation::source_plan_namespace::verify_predecessor(
                    c,
                    definition,
                    &self.settings.roots.mods,
                    &self.settings.archive,
                    &self.settings.cancelled,
                )?;
            }
            if actual_install {
                let predecessors =
                    crate::archive_generation::reviewed_master::required_predecessors(c)?;
                if !predecessors.is_empty() {
                    ensure!(
                        self.component_checkpoints && self.checkpoint_process.is_some(),
                        "Resolved source masters require an owned running component checkpoint"
                    );
                    for predecessor in predecessors {
                        ensure!(
                            self.checkpoint_completed
                                .contains(&crate::guid::key_ascii(&predecessor)),
                            "{} requires a successfully checkpointed predecessor {predecessor}",
                            c.name
                        );
                    }
                }
            }
            if !external_owner {
                crate::archive_generation::reviewed_manual::verify_runtime(
                    c,
                    &self.settings.roots.mods,
                    &self.settings.archive,
                    &self.settings.cancelled,
                )?;
            }
            for instruction in c
                .instructions
                .iter()
                .enumerate()
                .filter(|(_, instruction)| instruction.action == "Patcher")
            {
                if !external_owner
                    && self
                        .native_patcher_contract(c, instruction.1, instruction.0)?
                        .is_some()
                {
                    if matches!(
                        self.settings.patcher_engine,
                        PatcherEngine::HoloPatcher | PatcherEngine::ModPackaged
                    ) {
                        ensure!(
                            self.settings.provided_installer.is_none(),
                            "Original external backend cannot use an unbound supplied installer"
                        );
                        crate::archive_generation::external_patcher_source::resolve(
                            c, instruction.1, instruction.0, &self.settings.roots.mods,
                            &self.settings.archive, &self.settings.cancelled,
                        )?.context("Reviewed external patcher requires independently authenticated original source authority")?;
                    } else {
                        ensure!(
                            self.settings.patcher_engine == PatcherEngine::OdyPatcher
                                && self.settings.patcher.is_none()
                                && self.patcher_invocation.is_none()
                                && self.settings.provided_installer.is_none(),
                            "Reviewed single-INI recipes require the built-in OdyPatcher backend"
                        );
                    }
                }
            }
            let vendor_proof = crate::vendor_installer::before(
                c,
                &self.settings.roots,
                self.install,
                &self.settings.cancelled,
            )?;
            let mut unobserved_external_game_effects = false;
            for (n, instruction) in c.instructions.iter().enumerate() {
                self.validation_target = Some((c.name.clone(), c.guid.clone(), Some(n + 1)));
                self.check_cancelled()?;
                if !self.instruction_applies(instruction) {
                    emit(Event {
                        component: c.name.clone(),
                        step: n + 1,
                        message: "Skipped by instruction conditions".into(),
                    });
                    if let Some(observer) = progress.as_deref_mut() {
                        observer.instruction(c, n + 1, "Skipped by instruction conditions".into());
                    }
                    continue;
                }
                if unobserved_external_game_effects
                    && self.defer_external_game_delete(instruction)?
                {
                    emit(Event {
                        component: c.name.clone(),
                        step: n + 1,
                        message: "Delete: game file presence deferred until the original external backend completes".into(),
                    });
                    continue;
                }
                let message = if instruction.action == "Patcher" && !self.install {
                    if self.exact_patcher_prediction {
                        "Patcher: projecting native patch bytes into private scratch".into()
                    } else {
                        "Patcher: InstallList presence simulated; binary patch effects remain unmodeled".into()
                    }
                } else if matches!(instruction.action.as_str(), "Execute" | "Run") && !self.install
                {
                    format!(
                        "{}: inputs checked; external effects cannot be simulated",
                        instruction.action
                    )
                } else {
                    instruction.action.clone()
                };
                if let Some(tracker) = &mut self.conflicts {
                    if let Some(writer) = &mut tracker.current
                        && crate::guid::eq_ascii(&writer.component_guid, &c.guid)
                    {
                        writer.instruction_index = n;
                    }
                    tracker.external_effects |=
                        matches!(instruction.action.as_str(), "Patcher" | "Execute" | "Run");
                }
                if let Some(observer) = progress.as_deref_mut() {
                    // This is context for reached Choose frames, not an emitted success.
                    observer.current.instruction_index = Some(n + 1);
                }
                self.stage_managed_writes = self.install
                    && matches!(
                        instruction.action.as_str(),
                        "Extract"
                            | "ExtractTo"
                            | "Copy"
                            | "CopyFile"
                            | "Move"
                            | "Rename"
                            | "K1Widescreen"
                    );
                if let Some(managed) = &mut self.managed {
                    managed.action = instruction.action.clone();
                }
                let instruction_result =
                    self.execute_observed(c, instruction, Some(n), emit, progress.as_deref_mut());
                self.stage_managed_writes = false;
                instruction_result.with_context(|| {
                    format!("{} / step {} ({})", c.name, n + 1, instruction.action)
                })?;
                unobserved_external_game_effects |=
                    !self.install && external_owner && instruction.action == "Patcher";
                emit(Event {
                    component: c.name.clone(),
                    step: n + 1,
                    message: message.clone(),
                });
                if let Some(observer) = progress.as_deref_mut() {
                    observer.instruction(c, n + 1, message);
                }
            }
            if let Some(proof) = vendor_proof {
                crate::vendor_installer::after(
                    proof,
                    &self.settings.roots,
                    &self.settings.cancelled,
                )?;
            }
            Ok(())
        })();
        if actual_install {
            let error = result.as_ref().err().map(|error| format!("{error:#}"));
            crate::telemetry::global().record_component_execution(
                &c.name,
                result.is_ok(),
                c.instructions.len(),
                started.elapsed(),
                error.as_deref(),
            );
        }
        // Prediction records source applicability only. Actual publication also
        // requires the independently committed checkpoint prefix above, and
        // native prediction/runtime must still resolve all required game inputs.
        if result.is_ok() {
            self.completed_source_definitions
                .insert(crate::guid::key_ascii(&c.guid), c.clone());
        }
        result
    }

    /// A freshly authenticated original program may delete an external patcher's
    /// output. Its absence during presence-only validation says nothing about
    /// the actual post-process game. Real execution still resolves every Delete
    /// after the backend has completed and the game inventory has been rescanned.
    fn defer_external_game_delete(&self, instruction: &Instruction) -> Result<bool> {
        if self.install || instruction.action != "Delete" || instruction.source.len() != 1 {
            return Ok(false);
        }
        let source = &instruction.source[0];
        if source.contains(['*', '?']) {
            return Ok(false);
        }
        let (path, _) = self.source_pattern(source)?;
        let key = self.path_policy.key(&path);
        Ok(path.starts_with(&self.settings.roots.game)
            && !self.settings.roots.is_root(&path)
            && !self.files.contains_key(&key)
            && !self.directories.contains_key(&key))
    }

    fn native_patcher_contract(
        &self,
        component: &Component,
        instruction: &Instruction,
        operation_index: usize,
    ) -> Result<Option<crate::archive_generation::reviewed_manual::NativePatcherContract>> {
        let legacy = crate::archive_generation::reviewed_manual::native_patcher_contract(
            component,
            instruction,
        )?;
        let derived = crate::archive_generation::source_plan_patcher::native_patcher_contract(
            component,
            instruction,
            &self.settings.roots.mods,
            &self.settings.archive,
            &self.settings.cancelled,
        )?;
        let namespace = crate::archive_generation::source_plan_namespace::native_patcher_contract(
            component,
            instruction,
            operation_index,
            &self.settings.roots.mods,
            &self.settings.archive,
            &self.settings.cancelled,
        )?;
        ensure!(
            usize::from(legacy.is_some())
                + usize::from(derived.is_some())
                + usize::from(namespace.is_some())
                <= 1,
            "Conflicting native Patcher authorities"
        );
        Ok(namespace.or(derived).or(legacy))
    }

    fn verify_original_external_component(&self, component: &Component) -> Result<bool> {
        if !matches!(
            self.settings.patcher_engine,
            PatcherEngine::HoloPatcher | PatcherEngine::ModPackaged
        ) {
            return Ok(false);
        }
        crate::archive_generation::external_patcher_source::verify_component_original(
            component,
            &self.settings.roots.mods,
            &self.settings.archive,
            &self.settings.cancelled,
        )
    }

    fn check_component(&self, c: &Component) -> Result<()> {
        self.check_cancelled()?;
        ensure!(
            !crate::draft::requires_review(c),
            "{} has unreviewed draft instructions; review them before installation",
            c.name
        );
        crate::k1_widescreen_recipe::verify_runtime(
            c,
            &self.settings.roots.mods,
            &self.settings.archive,
            &self.settings.cancelled,
        )?;
        if !self.verify_original_external_component(c)? {
            crate::archive_generation::source_plan_patcher::verify_runtime(
                c,
                &self.settings.roots.mods,
                &self.settings.archive,
                &self.settings.cancelled,
            )?;
            crate::archive_generation::source_plan_namespace::verify_runtime(
                c,
                &self.settings.roots.mods,
                &self.settings.archive,
                &self.settings.cancelled,
            )?;
        }
        ensure!(
            c.metadata
                .get("UnresolvedMasters")
                .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty)),
            "{} has unresolved Markdown masters; review its dependencies",
            c.name
        );
        if crate::k2_preparation::contains_master(c) {
            crate::k2_preparation::verify_master_runtime(
                c,
                &self.settings.roots.mods,
                &self.settings.archive,
                &self.settings.cancelled,
            )?;
            self.verify_k2_preparation_master()?;
        }
        for dependency in &c.dependencies {
            ensure!(
                self.selected.contains(&crate::guid::key_ascii(dependency)),
                "{} requires {dependency}",
                c.name
            );
        }
        for restriction in &c.restrictions {
            ensure!(
                !self.selected.contains(&crate::guid::key_ascii(restriction)),
                "{} conflicts with {restriction}",
                c.name
            );
        }
        Ok(())
    }

    fn instruction_applies(&self, instruction: &Instruction) -> bool {
        applies_with(&self.selected, instruction)
    }

    /// The same immediate-option selection used by execution and read-only gates.
    /// An entry that names no option is refused (fail closed), where C# ignores it.
    fn chosen_options<'a>(
        component: &'a Component,
        instruction: &Instruction,
    ) -> Result<Vec<&'a Component>> {
        for id in &instruction.source {
            if component
                .options
                .iter()
                .any(|option| crate::guid::names_option(&option.guid, id))
            {
                continue;
            }
            match crate::guid::try_parse(id) {
                Some(value)
                    if component
                        .options
                        .iter()
                        .any(|option| crate::guid::try_parse(&option.guid) == Some(value)) =>
                {
                    bail!("Unknown option {id}: a Choose entry names an option only as {value}")
                }
                _ => bail!("Unknown option {id}"),
            }
        }
        Ok(component
            .options
            .iter()
            .filter(|option| {
                option.is_selected
                    && instruction
                        .source
                        .iter()
                        .any(|id| crate::guid::names_option(&option.guid, id))
            })
            .collect())
    }

    fn collect_execution_components<'a>(
        &self,
        component: &'a Component,
        scope: &mut Vec<&'a Component>,
    ) -> Result<()> {
        self.collect_components(component, scope, true)
    }

    /// A component and its chosen options; `check` applies the readiness checks
    /// before each one, which the FOMOD gate defers until after its findings.
    fn collect_components<'a>(
        &self,
        component: &'a Component,
        scope: &mut Vec<&'a Component>,
        check: bool,
    ) -> Result<()> {
        if check {
            self.check_component(component)?;
        } else {
            self.check_cancelled()?;
        }
        scope.push(component);
        for instruction in &component.instructions {
            if instruction.action == "Choose" && self.instruction_applies(instruction) {
                for option in Self::chosen_options(component, instruction)? {
                    self.collect_components(option, scope, check)?;
                }
            }
        }
        Ok(())
    }

    fn validation_warning(&mut self, category: &str, message: String, path: &Path) {
        if !self.install {
            self.validation_warnings
                .push(crate::validation::DryRunIssue::warning(
                    category.into(),
                    message,
                    path.to_string_lossy().into_owned(),
                    self.validation_target.clone(),
                ));
        }
    }

    fn path(&self, raw: &str, wildcard: bool) -> Result<PathBuf> {
        let path = self.settings.roots.resolve(raw, wildcard)?;
        if !wildcard {
            let root = [&self.settings.roots.mods, &self.settings.roots.game]
                .into_iter()
                .find(|root| path.starts_with(root))
                .context("Path outside install roots")?;
            self.path_policy
                .check_spelling(root, &path, &self.settings.cancelled)?;
        }
        Ok(path)
    }

    /// A Source that binds one literal file or folder, as a Patcher does: `[` and `]`
    /// are ordinary characters, `*` and `?` are refused.
    pub(super) fn literal_source_path(&self, raw: &str) -> Result<PathBuf> {
        let path = self.settings.roots.resolve_literal(raw)?;
        let root = [&self.settings.roots.mods, &self.settings.roots.game]
            .into_iter()
            .find(|root| path.starts_with(root))
            .context("Path outside install roots")?;
        self.path_policy
            .check_spelling(root, &path, &self.settings.cancelled)?;
        Ok(path)
    }

    /// Explicit extensions bind one literal file; they never enter heuristic
    /// source recovery, folder expansion, or implicit archive extraction.
    fn extension_source_path(&self, instruction: &Instruction) -> Result<PathBuf> {
        ensure!(
            instruction.source.len() == 1 && instruction.arguments.is_empty(),
            "{} requires one literal Source and empty Arguments",
            instruction.action
        );
        let path = self.actual(&self.literal_source_path(&instruction.source[0])?);
        self.settings.roots.check(&path)?;
        Ok(path)
    }

    fn extraction_sources(
        &self,
        instruction: &Instruction,
        sources: &[String],
        missing_ok: bool,
    ) -> Result<Vec<PathBuf>> {
        if instruction.action == "ExtractTo" {
            let path = self.extension_source_path(instruction)?;
            ensure!(
                path.starts_with(&self.settings.roots.mods),
                "ExtractTo Source must be in the mod directory"
            );
            return self.matches_scoped(sources, missing_ok, false);
        }
        self.matches(sources, missing_ok)
    }

    /// Read-only projection: freshness belongs to actual execution, not archive
    /// membership checks that may already project a future slot's members.
    fn extraction_target(&self, source: &Path, instruction: &Instruction) -> Result<PathBuf> {
        if instruction.action != "ExtractTo" {
            return Ok(extraction_destination(source));
        }
        self.extension_source_path(instruction)?;
        let path = self
            .settings
            .roots
            .resolve_literal(&instruction.destination)?;
        self.settings.roots.check(&path)?;
        ensure!(
            path.parent() == Some(self.settings.roots.mods.as_path()),
            "ExtractTo requires one direct child of the mod directory"
        );
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("ExtractTo requires a named slot")?;
        let (owner, slot) = name
            .strip_prefix(".modsync-reviewed-")
            .and_then(|name| name.rsplit_once("-slot"))
            .context("ExtractTo requires .modsync-reviewed-<component GUID>-slot<n>")?;
        let guid = crate::guid::try_parse(owner)
            .context("ExtractTo slot has an invalid component GUID")?;
        let number: usize = slot.parse().context("ExtractTo slot number is invalid")?;
        ensure!(
            owner == guid.to_string() && number < 4096 && slot == number.to_string(),
            "ExtractTo slot must have a canonical GUID and number in 0..4096"
        );
        Ok(path)
    }

    fn copy_file_destination(&self, instruction: &Instruction) -> Result<PathBuf> {
        self.extension_source_path(instruction)?;
        let requested = self
            .settings
            .roots
            .resolve_literal(&instruction.destination)?;
        let path = self.actual(&requested);
        self.settings.roots.check(&path)?;
        ensure!(
            !self.settings.roots.is_root(&path) && path.file_name().is_some(),
            "CopyFile requires an exact destination filename"
        );
        ensure!(
            !invalid_destination(&instruction.destination),
            "CopyFile Destination is invalid"
        );
        ensure!(
            !self.directories.contains_key(&self.path_policy.key(&path)),
            "CopyFile Destination is a directory"
        );
        ensure!(
            path.parent()
                .is_some_and(|parent| self.folder_creatable(parent)),
            "File blocks CopyFile Destination parent"
        );
        Ok(path)
    }

    fn matches(&self, sources: &[String], missing_ok: bool) -> Result<Vec<PathBuf>> {
        self.matches_scoped(sources, missing_ok, true)
    }

    /// The files the Sources name, in Source order as C# `SetRealPaths` lists them
    /// (so a later Source's file wins a collision), each Source's own matches in key
    /// order, and a file two Sources name only at its first place. With
    /// `descendants` false, a Source names only the files its literal or wildcard
    /// path matches; a directory Source matches nothing beneath it.
    fn matches_scoped(
        &self,
        sources: &[String],
        missing_ok: bool,
        descendants: bool,
    ) -> Result<Vec<PathBuf>> {
        ensure!(!sources.is_empty(), "Instruction has no Source");
        let mut seen = BTreeSet::new();
        let mut found = Vec::new();
        for source in sources {
            let (path, pattern) = self.source_pattern(source)?;
            let prefix = self.source_prefix(&path);
            let matches: Vec<_> = self
                .files
                .iter()
                .filter(|(k, _)| {
                    if descendants {
                        self.source_matches(prefix.as_deref(), &pattern, k)
                    } else {
                        self.path_policy.glob_matches(&pattern, k)
                    }
                })
                .collect();
            if !missing_ok && matches.is_empty() {
                return Err(MissingSource(source.clone()).into());
            }
            for (k, (p, _)) in matches {
                self.settings.roots.check(p)?;
                if seen.insert(k) {
                    found.push(p.clone());
                }
            }
        }
        Ok(found)
    }

    /// Copy, Move and Rename: C# `SetRealPaths` gathers the files of every Source, so
    /// a wildcard Source that matches nothing is dropped when another Source of the
    /// step names a file (`[m/*.tga, m/*.tpc]` with only TGAs). A literal Source that
    /// names no file still fails the step (Copy and Move: C# drops it, Rust stays
    /// strict, documented; Rename: C# fails it too), as does a step whose Sources
    /// all matched nothing. Order and duplicates are as in [`Self::matches_scoped`];
    /// a miss names the first literal Source that matched nothing, else the first
    /// Source.
    fn gathered_matches(&self, sources: &[String], descendants: bool) -> Result<Vec<PathBuf>> {
        ensure!(!sources.is_empty(), "Instruction has no Source");
        let mut seen = BTreeSet::new();
        let mut found = Vec::new();
        let mut first_miss = None;
        let mut literal_miss = None;
        for source in sources {
            let matches = self.matches_scoped(std::slice::from_ref(source), true, descendants)?;
            if matches.is_empty() {
                first_miss.get_or_insert(source);
                if !source.contains(['*', '?']) {
                    literal_miss.get_or_insert(source);
                }
            }
            for path in matches {
                if seen.insert(self.path_policy.key(&path)) {
                    found.push(path);
                }
            }
        }
        match (literal_miss, first_miss) {
            (Some(source), _) => Err(MissingSource(source.clone()).into()),
            (None, Some(source)) if found.is_empty() => Err(MissingSource(source.clone()).into()),
            _ => Ok(found),
        }
    }

    fn source_pattern(&self, source: &str) -> Result<(PathBuf, glob::Pattern)> {
        let path = self.path(source, true)?;
        let root = [&self.settings.roots.mods, &self.settings.roots.game]
            .into_iter()
            .find(|root| path.starts_with(root))
            .unwrap();
        let pattern = glob::Pattern::new(&format!(
            "{}/{}",
            glob::Pattern::escape(&self.path_policy.key(root)),
            source_wildcard_glob(&self.path_policy.key(path.strip_prefix(root)?))
        ))?;
        Ok((path, pattern))
    }

    /// The folder prefix whose files a literal Source reaches. A bare root Source
    /// has none: C# keeps it as one literal path, which is not a file.
    fn source_prefix(&self, path: &Path) -> Option<String> {
        (!self.settings.roots.is_root(path)).then(|| format!("{}/", self.path_policy.key(path)))
    }

    fn source_matches(
        &self,
        prefix: Option<&str>,
        pattern: &glob::Pattern,
        candidate: &str,
    ) -> bool {
        self.path_policy.glob_matches(pattern, candidate)
            || prefix.is_some_and(|prefix| candidate.starts_with(prefix))
    }

    /// Whether any literal Source (no `*` or `?`; `[` and `]` are literal) of an
    /// instruction names no file. C# `SetRealPaths(skipExistenceCheck: true)` keeps
    /// every literal Source, so its expansion is empty only when every Source is a
    /// wildcard and none matched.
    fn any_literal_source_missing(&self, sources: &[String]) -> Result<bool> {
        for source in sources {
            if !source.contains(['*', '?']) {
                let path = self.path(source, true)?;
                if !self.files.contains_key(&self.path_policy.key(&path)) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Types a `MissingSource` miss as `MissingSourceFiles` (the dry run's
    /// `DryRunSkipped`) where the reference does, else keeps it an untyped error:
    /// - `LiteralTyped` (Rename, CleanList): typed when a literal Source is missing;
    ///   an all-wildcard empty expansion throws `ArgumentException` (UnknownError).
    /// - `WildcardTyped` (Execute, Run): typed when every Source is a wildcard; a
    ///   missing literal program is `UnknownError`.
    fn classify_miss<T>(&self, result: Result<T>, sources: &[String], kind: MissKind) -> Result<T> {
        match result {
            Err(error) if error.is::<MissingSource>() => {
                let literal_missing = self.any_literal_source_missing(sources)?;
                let typed = match kind {
                    MissKind::LiteralTyped => literal_missing,
                    MissKind::WildcardTyped => {
                        !literal_missing && sources.iter().all(|s| s.contains(['*', '?']))
                    }
                };
                Err(if typed {
                    anyhow::Error::new(MissingSourceFiles(error))
                } else {
                    error
                })
            }
            result => result,
        }
    }

    fn instruction_sources(&self, sources: &[String]) -> Result<Vec<PathBuf>> {
        // A miss in one pattern must not hide an unsafe/malformed later pattern.
        for source in sources {
            self.source_pattern(source)?;
        }
        // Execute/Run only: a folder Source is a miss, never every program in it.
        // The caller classifies the miss (see `classify_miss`).
        let paths = self.matches_scoped(sources, false, false)?;
        for path in &paths {
            // Reading must be possible even when virtual copying needs no bytes.
            let content = &self.files[&self.path_policy.key(path)].1;
            if !matches!(content, Content::Unknown) || self.install {
                content.open()?;
            }
        }
        Ok(paths)
    }

    fn actual(&self, path: &Path) -> PathBuf {
        if let Some((p, _)) = self.files.get(&self.path_policy.key(path)) {
            return p.clone();
        }
        if let Some(p) = self.directories.get(&self.path_policy.key(path)) {
            return p.clone();
        }
        if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
            let actual = self.actual(parent);
            return actual.join(name);
        }
        path.to_path_buf()
    }

    fn put(&mut self, path: &Path, content: Content, overwrite: bool) -> Result<()> {
        let started = std::time::Instant::now();
        let actual_install = self.install;
        let mut attempted = false;
        let result: Result<()> = (|| {
            let path = self.actual(path);
            self.settings.roots.check(&path)?;
            let root = [&self.settings.roots.mods, &self.settings.roots.game]
                .into_iter()
                .find(|root| path.starts_with(root))
                .context("Output outside install roots")?;
            self.path_policy
                .check_spelling(root, &path, &self.settings.cancelled)?;
            ensure!(
                !self.directories.contains_key(&self.path_policy.key(&path)),
                "Cannot overwrite directory with file: {}",
                path.display()
            );
            if self.files.contains_key(&self.path_policy.key(&path)) && !overwrite {
                return Ok(());
            }
            let mut parent = path.parent();
            while let Some(dir) = parent {
                ensure!(
                    !self.files.contains_key(&self.path_policy.key(dir)),
                    "File blocks destination directory: {}",
                    dir.display()
                );
                self.directories
                    .insert(self.path_policy.key(dir), dir.to_path_buf());
                parent = dir.parent();
            }
            let content = if self.install {
                attempted = true;
                let modified = content.modified()?;
                let permissions = match &content {
                    Content::Unknown => bail!("Cannot publish unknown simulated patcher content"),
                    Content::Disk(source) => Some(
                        fs::metadata(source)
                            .with_context(|| format!("Cannot read {}", source.display()))?
                            .permissions(),
                    ),
                    Content::Scratch(_, mode, _) => {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            mode.map(|mode| {
                                fs::Permissions::from_mode(crate::archive::extracted_file_mode(
                                    mode,
                                ))
                            })
                        }
                        #[cfg(not(unix))]
                        {
                            let _ = mode;
                            None
                        }
                    }
                };
                let physical = self.managed_output(&path)?;
                let input = content.open().with_context(|| match content.path() {
                    Ok(source) => format!("Cannot read {}", source.display()),
                    Err(_) => "Cannot read source content".to_owned(),
                })?;
                recovery::atomic_copy(&physical, input, &self.settings.cancelled)
                    .with_context(|| format!("Cannot write {}", physical.display()))?;
                if let Some(modified) = modified {
                    // Set time before readonly permissions, on the actual managed or
                    // ordinary output. Metadata failure retains the install journal.
                    fs::OpenOptions::new()
                        .write(true)
                        .open(&physical)?
                        .set_modified(modified)?;
                }
                if let Some(permissions) = permissions {
                    fs::set_permissions(&physical, permissions)?;
                }
                if physical != path
                    && let Some(managed) = &mut self.managed
                {
                    managed
                        .pending
                        .insert(self.path_policy.key(&path), physical.clone());
                }
                Content::Disk(physical)
            } else {
                content
            };
            if let Some(tracker) = &mut self.conflicts
                && let Ok(relative) = path.strip_prefix(&self.settings.roots.game)
            {
                tracker.write(&relative.to_string_lossy());
            }
            self.files
                .insert(self.path_policy.key(&path), (path.clone(), content));
            self.consume_authoring_input(&path);
            Ok(())
        })();
        if actual_install && attempted {
            let error = result.as_ref().err().map(|error| format!("{error:#}"));
            crate::telemetry::global().record_file_operation(
                "write",
                result.is_ok(),
                1,
                started.elapsed(),
                error.as_deref(),
            );
        }
        result
    }

    fn delete(&mut self, path: &Path) -> Result<()> {
        let started = std::time::Instant::now();
        let actual_install = self.install;
        let mut attempted = false;
        let result: Result<()> = (|| {
            self.settings.roots.check(path)?;
            if self.install {
                attempted = self.files.contains_key(&self.path_policy.key(path));
                let private_only = self.private_authoring_input(path)
                    && match fs::symlink_metadata(path) {
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
                        Ok(_) => false,
                        Err(error) => return Err(error.into()),
                    };
                if !private_only {
                    self.managed_delete(path)?;
                }
            }
            self.files.remove(&self.path_policy.key(path));
            self.consume_authoring_input(path);
            Ok(())
        })();
        if actual_install && attempted {
            let error = result.as_ref().err().map(|error| format!("{error:#}"));
            crate::telemetry::global().record_file_operation(
                "delete",
                result.is_ok(),
                1,
                started.elapsed(),
                error.as_deref(),
            );
        }
        result
    }

    fn execute(
        &mut self,
        component: &Component,
        i: &Instruction,
        emit: &mut impl FnMut(Event),
    ) -> Result<()> {
        self.execute_observed(component, i, None, emit, None)
    }
    fn execute_indexed(
        &mut self,
        component: &Component,
        i: &Instruction,
        operation_index: usize,
        emit: &mut impl FnMut(Event),
    ) -> Result<()> {
        self.execute_observed(component, i, Some(operation_index), emit, None)
    }

    // First-class authoring extension; managed file manifests cannot own empty dirs.
    fn directory_destination(&self, i: &Instruction, require_parent: bool) -> Result<PathBuf> {
        ensure!(
            i.source.is_empty() && i.arguments.is_empty(),
            "CreateDirectory requires empty Source and Arguments"
        );
        ensure!(
            self.managed.is_none(),
            "CreateDirectory is unsupported in managed deployment: empty directory ownership is not implemented"
        );
        let requested = self.path(&i.destination, false)?;
        let path = self.actual(&requested);
        self.settings.roots.check(&path)?;
        ensure!(
            path != self.settings.roots.mods && path != self.settings.roots.game,
            "CreateDirectory requires a child destination"
        );
        let leaf = path
            .file_name()
            .context("CreateDirectory requires one child name")?;
        ensure!(
            !leaf.to_string_lossy().contains(['*', '?', '[', ']', '\0']),
            "CreateDirectory name cannot contain wildcard or NUL characters"
        );
        let parent = path
            .parent()
            .context("CreateDirectory requires an existing parent")?;
        ensure!(
            !self.files.contains_key(&self.path_policy.key(&path)),
            "CreateDirectory destination is a file"
        );
        let mut ancestor = Some(parent);
        while let Some(dir) = ancestor {
            ensure!(
                !self.files.contains_key(&self.path_policy.key(dir)),
                "File blocks CreateDirectory ancestor"
            );
            if dir == self.settings.roots.mods || dir == self.settings.roots.game {
                break;
            }
            ancestor = dir.parent();
        }
        if require_parent {
            ensure!(
                self.directories.contains_key(&self.path_policy.key(parent)),
                "CreateDirectory parent does not exist in the current filesystem"
            );
        }
        Ok(path)
    }

    fn execute_observed(
        &mut self,
        component: &Component,
        i: &Instruction,
        operation_index: Option<usize>,
        emit: &mut impl FnMut(Event),
        mut progress: Option<&mut InstallObserver<'_>>,
    ) -> Result<()> {
        match i.action.as_str() {
            "CreateDirectory" => {
                let path = self.directory_destination(i, true)?;
                let key = self.path_policy.key(&path);
                if !self.directories.contains_key(&key) {
                    self.check_cancelled()?;
                    if self.install {
                        // Recheck physical ancestry before the single-leaf mutation.
                        self.settings.roots.check(&path)?;
                        let parent = path.parent().context("Directory parent disappeared")?;
                        ensure!(
                            fs::symlink_metadata(parent)?.is_dir(),
                            "Directory parent changed type"
                        );
                        match fs::create_dir(&path) {
                            Ok(()) => {}
                            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                                let metadata = fs::symlink_metadata(&path)?;
                                ensure!(
                                    metadata.is_dir() && !metadata.file_type().is_symlink(),
                                    "CreateDirectory destination changed type"
                                );
                            }
                            Err(error) => return Err(error.into()),
                        }
                        self.settings.roots.check(&path)?;
                    }
                    self.directories.insert(key, path);
                } else if self.install {
                    self.settings.roots.check(&path)?;
                    let metadata = fs::symlink_metadata(&path)?;
                    ensure!(
                        metadata.is_dir() && !metadata.file_type().is_symlink(),
                        "CreateDirectory destination changed type"
                    );
                }
            }
            "Choose" => {
                let reached_from_instruction = progress
                    .as_ref()
                    .and_then(|observer| observer.current.instruction_index);
                for option in Self::chosen_options(component, i)? {
                    if let Some(observer) = progress.as_deref_mut() {
                        observer.current.owner_path.push(OwnerFrame {
                            name: option.name.clone(),
                            guid: option.guid.clone(),
                            reached_from_instruction,
                        });
                    }
                    let result = self.component(option, emit, progress.as_deref_mut());
                    if let Some(observer) = progress.as_deref_mut() {
                        observer.current.owner_path.pop();
                    }
                    result.map_err(OptionalInstallFailed::wrap)?;
                }
            }
            "Extract" | "ExtractTo" => {
                if i.action == "Extract" && !i.destination.is_empty() {
                    // Still refused when unsafe (fail closed), though never used.
                    self.path(&i.destination, false)?;
                    emit(Event {
                        component: component.name.clone(),
                        step: 0,
                        message: format!(
                            "Extract Destination '{}' is not used; each archive extracts \
                             to a folder named after it, next to the archive",
                            i.destination
                        ),
                    });
                }
                let mut resolution = self.resolve_instruction(component, i)?;
                self.publish_resolution(&mut resolution)?;
                for source in resolution.sources {
                    let content = self.files[&self.path_policy.key(&source)].1.clone();
                    let dest = self.extraction_target(&source, i)?;
                    if i.action == "ExtractTo" {
                        let owner = crate::guid::try_parse(&component.guid)
                            .context("ExtractTo owner has an invalid GUID")?;
                        let prefix = format!(".modsync-reviewed-{owner}-slot");
                        ensure!(
                            dest.file_name()
                                .and_then(|name| name.to_str())
                                .is_some_and(|name| name.starts_with(&prefix)),
                            "ExtractTo slot belongs to another component"
                        );
                        ensure!(
                            !self.files.contains_key(&self.path_policy.key(&dest))
                                && !self.directories.contains_key(&self.path_policy.key(&dest)),
                            "ExtractTo slot is not fresh in the current filesystem"
                        );
                        match fs::symlink_metadata(&dest) {
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                            Err(error) => return Err(error.into()),
                            Ok(_) => bail!("ExtractTo requires a physically fresh owned slot"),
                        }
                    }
                    let entries = archive::unpack(
                        content.path()?,
                        &source,
                        &self.settings.archive,
                        &self.settings.cancelled,
                    )?;
                    if i.action == "ExtractTo" {
                        ensure!(
                            !entries.is_empty(),
                            "ExtractTo cannot publish an empty directory tree"
                        );
                        self.check_cancelled()?;
                        if self.install {
                            // Exclusive creation after full archive admission. Never
                            // merge with, clear, or reuse any pre-existing directory.
                            self.settings.roots.check(&dest)?;
                            fs::create_dir(&dest)?;
                            self.settings.roots.check(&dest)?;
                        }
                        self.directories
                            .insert(self.path_policy.key(&dest), dest.clone());
                    }
                    // ExtractTo is raw extraction only. The legacy action retains
                    // its automatic configured FOMOD copies without alteration.
                    let configured = if i.action == "Extract"
                        && let Some(mut module) = crate::fomod::inspect_entries_with_cancel(
                            &entries,
                            &self.settings.cancelled,
                        )? {
                        let filename = source
                            .file_name()
                            .and_then(|n| n.to_str())
                            .context("FOMOD archive filename is invalid")?;
                        let choices = crate::fomod::required_saved_choices(component, filename)?;
                        let relative = dest.strip_prefix(&self.settings.roots.mods).context(
                            "Configured FOMOD extraction must stay in the mod directory",
                        )?;
                        // Applying and mapping both judge the saved choices against
                        // the current archive, so both carry the recovery route.
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
                    if !configured.is_empty() && !i.overwrite {
                        ensure!(
                            archive_paths
                                .iter()
                                .all(|path| !self.files.contains_key(path)),
                            "Configured FOMOD extraction cannot retain existing files with Overwrite=false"
                        );
                    }
                    // C# extraction writes every entry with FileMode.Create, so
                    // Overwrite=false never keeps an existing file. Configured FOMOD
                    // extraction with Overwrite=false was refused above instead.
                    for entry in entries {
                        self.check_cancelled()?;
                        self.put(
                            &dest.join(entry.path),
                            Content::Scratch(entry.file, entry.mode, entry.modified),
                            true,
                        )?;
                    }
                    for copy in configured {
                        for source in self.matches(&copy.source, false)? {
                            ensure!(
                                archive_paths.contains(&self.path_policy.key(&source)),
                                "FOMOD source is not supplied by the current archive: {}",
                                source.display()
                            );
                        }
                        self.execute(component, &copy, emit)?;
                    }
                }
            }
            "CopyFile" => {
                let resolution = self.resolve_instruction(component, i)?;
                let source = resolution
                    .sources
                    .first()
                    .context("CopyFile has no source")?;
                let target = self.copy_file_destination(i)?;
                ensure!(
                    self.path_policy.key(source) != self.path_policy.key(&target),
                    "CopyFile Source equals Destination"
                );
                let content = self.files[&self.path_policy.key(source)].1.clone();
                let input = crate::paths::open_regular_read(content.path()?)?;
                match fs::symlink_metadata(&target) {
                    Ok(metadata) => {
                        ensure!(
                            metadata.is_file() && !metadata.file_type().is_symlink(),
                            "CopyFile Destination is not a regular file"
                        );
                        let existing = crate::paths::open_regular_read(&target)?;
                        ensure!(
                            !same_regular_file(&input, &existing)?,
                            "CopyFile Source and Destination identify the same file"
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
                self.put(&target, content, i.overwrite)?;
            }
            "Copy" | "Move" | "Rename" => {
                let dest = if i.action == "Rename" {
                    rename_file_name(&i.destination)?
                } else {
                    self.path(&i.destination, false)?
                };
                let sources = if i.action == "Rename" {
                    // RenameFile: FileNotFoundPost for a missing literal, an
                    // ArgumentException on an all-wildcard empty expansion.
                    let sources = self.gathered_matches(&i.source, authored_descendants(&i.action));
                    self.classify_miss(sources, &i.source, MissKind::LiteralTyped)?
                } else {
                    let mut resolution = self.resolve_instruction(component, i)?;
                    if resolution.satisfied {
                        emit(Event {
                            component: component.name.clone(),
                            step: 0,
                            message: "Move already satisfied at its qualified game targets".into(),
                        });
                        return Ok(());
                    }
                    self.publish_resolution(&mut resolution)?;
                    resolution.sources
                };
                let mut blocked_rename = None;
                for source in sources {
                    self.check_cancelled()?;
                    let target = if i.action == "Rename" {
                        source.parent().unwrap().join(&dest)
                    } else {
                        dest.join(source.file_name().unwrap())
                    };
                    // C# Copy/Move find the target present and, with Overwrite=false,
                    // warn and skip it; with Overwrite=true they delete the only copy
                    // and fail. Rust skips the first and refuses the second unchanged.
                    ensure!(
                        self.path_policy.key(&source) != self.path_policy.key(&target)
                            || (i.action != "Rename" && !i.overwrite),
                        "Source equals destination: {}",
                        source.display()
                    );
                    let exists = self.files.contains_key(&self.path_policy.key(&target));
                    if exists && !i.overwrite && i.action == "Rename" {
                        // C# RenameFile skips this source, renames the rest, then
                        // returns RenameTargetAlreadyExists, which fails the component.
                        blocked_rename.get_or_insert(target);
                        continue;
                    }
                    if exists && !i.overwrite {
                        let message = format!(
                            "Destination file already exists and overwrite is false: {}",
                            target.display()
                        );
                        // C# CopyFile/MoveFile log a warning for each skipped file
                        // during install too; validation records it as an issue.
                        if self.install {
                            emit(Event {
                                component: component.name.clone(),
                                step: self
                                    .validation_target
                                    .as_ref()
                                    .and_then(|target| target.2)
                                    .unwrap_or(0),
                                message: format!("WARNING: {message}"),
                            });
                        }
                        self.validation_warning(&format!("{}File", i.action), message, &target);
                        continue;
                    }
                    let data = self.files[&self.path_policy.key(&source)].1.clone();
                    self.put(&target, data, i.overwrite)?;
                    if i.action != "Copy" {
                        self.delete(&source)
                            .with_context(|| format!("Cannot remove {}", source.display()))?;
                    }
                }
                if let Some(target) = blocked_rename {
                    bail!(
                        "Rename target already exists and overwrite is false: {}",
                        target.display()
                    );
                }
            }
            "Delete" => {
                if i.source.is_empty() {
                    // C# DeleteFile with no source paths logs and returns Success,
                    // or FileNotFoundPost (MissingSourceFiles) when Overwrite=true.
                    if i.overwrite {
                        return Err(MissingSourceFiles(anyhow::anyhow!(
                            "Delete has no Source and Overwrite is true"
                        ))
                        .into());
                    }
                    emit(Event {
                        component: component.name.clone(),
                        step: 0,
                        message: "Delete has no Source; nothing to delete".into(),
                    });
                    return Ok(());
                }
                // C# DeleteFile deletes only paths for which File.Exists holds, so
                // a directory Source deletes nothing beneath it (a miss).
                let sources =
                    self.matches_scoped(&i.source, true, authored_descendants(&i.action))?;
                // A strict miss fails the step below with no warning, as C# DeleteFile
                // reports it without touching the virtual file system.
                if sources.is_empty() && !i.overwrite {
                    for source in &i.source {
                        let path = self.path(source, true)?;
                        let message = if self.directories.contains_key(&self.path_policy.key(&path))
                        {
                            format!(
                                "Delete Source is a directory; Delete removes files only: {}",
                                path.display()
                            )
                        } else {
                            format!("Attempting to delete non-existent file: {}", path.display())
                        };
                        self.validation_warning("DeleteFile", message, &path);
                    }
                }
                // C# DeleteFile with Overwrite=true returns FileNotFoundPost for a
                // missing literal, or when nothing resolves at all; that maps to
                // MissingSourceFiles after the present matches are deleted.
                let strict_miss = if !i.overwrite {
                    None
                } else if sources.is_empty() {
                    i.source.first()
                } else {
                    let mut missing = None;
                    for source in &i.source {
                        if !source.contains(['*', '?']) {
                            let path = self.path(source, true)?;
                            if !self.files.contains_key(&self.path_policy.key(&path)) {
                                missing = missing.or(Some(source));
                            }
                        }
                    }
                    missing
                };
                for source in sources {
                    self.delete(&source)?;
                }
                if let Some(source) = strict_miss {
                    return Err(MissingSourceFiles(MissingSource(source.clone()).into()).into());
                }
            }
            "DelDuplicate" => {
                let certified_cleanup =
                    crate::archive_generation::duplicate_cleanup::contains(component);
                if certified_cleanup {
                    crate::archive_generation::duplicate_cleanup::verify_runtime(
                        component,
                        &self.settings.cancelled,
                    )?;
                    if self.install {
                        ensure!(
                            self.component_checkpoints && self.checkpoint_process.is_some(),
                            "Compiled duplicate cleanup requires an owned running component checkpoint"
                        );
                    }
                }
                // C# DeleteDuplicateFile throws "Invalid directory path" for a
                // Destination `PathValidator.IsValidPath` refuses (UnknownError).
                ensure!(
                    !invalid_destination(&i.destination),
                    "DelDuplicate destination cannot be found: {}",
                    i.destination
                );
                let dir = self.path(&i.destination, false)?;
                ensure!(
                    self.directories.contains_key(&self.path_policy.key(&dir)),
                    "DelDuplicate destination cannot be found"
                );
                let cancelled = self.settings.cancelled.clone();
                let mut budget = cleanlist::Budget::new(&cancelled);
                let mut extensions = BTreeSet::new();
                for extension in &i.source {
                    budget.text(extension)?;
                    if !cleanlist::blank(extension) {
                        extensions.insert(crate::unicode_case::ordinal_key(extension));
                    }
                }
                if extensions.is_empty() {
                    extensions.extend([".DDS", ".TPC", ".TGA"].map(str::to_owned));
                }
                budget.text(&i.arguments)?;
                let preferred = crate::unicode_case::ordinal_key(&i.arguments);
                budget.text(&dir.to_string_lossy())?;
                let directory_key = self.path_policy.key(&dir);
                let descendant_prefix = format!("{directory_key}/");
                let mut candidates = Vec::new();
                let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
                let mut alternatives = BTreeSet::new();
                for (path, _) in self.files.values() {
                    budget.charge(1)?;
                    budget.text(&path.to_string_lossy())?;
                    let parent = path.parent().context("DelDuplicate file has no parent")?;
                    let parent_key = self.path_policy.key(parent);
                    // Descendants participate, but duplicate identity never crosses
                    // directory boundaries. Key prefixes obey the configured path policy.
                    if parent_key != directory_key && !parent_key.starts_with(&descendant_prefix) {
                        continue;
                    }
                    self.settings.roots.check(path)?;
                    ensure!(
                        candidates.len() < 100_000,
                        "DelDuplicate exceeds 100,000 candidates"
                    );
                    let name = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .context("DelDuplicate filename is not Unicode")?;
                    budget.text(name)?;
                    let (stem, extension) = cleanlist::name_parts(name);
                    let stem = crate::unicode_case::ordinal_key(stem);
                    let extension = crate::unicode_case::ordinal_key(extension);
                    let group = (parent_key, stem.clone());
                    if certified_cleanup && matches!(extension.as_str(), ".TGA" | ".DDS") {
                        alternatives.insert(group.clone());
                    }
                    if extensions.contains(&extension) {
                        *counts.entry(group.clone()).or_default() += 1;
                    }
                    candidates.push((path.clone(), group, extension));
                }
                let mut planned = Vec::new();
                for (path, group, extension) in candidates {
                    budget.charge(1)?;
                    // Empty Arguments selects extensionless candidates as in the
                    // reference; preferred extensions need not be compatible.
                    if !group.1.is_empty()
                        && (if certified_cleanup {
                            alternatives.contains(&group)
                        } else {
                            counts.get(&group).copied().unwrap_or(0) > 1
                        })
                        && extension == preferred
                    {
                        self.settings.roots.check(&path)?;
                        planned.push(path);
                    }
                }
                for path in planned {
                    budget.check()?;
                    self.delete(&path)?;
                }
            }
            "CleanList" => {
                // C# fails a CleanList into a Destination `PathValidator.IsValidPath`
                // refuses (UnknownError).
                ensure!(
                    !invalid_destination(&i.destination),
                    "CleanList destination cannot be found: {}",
                    i.destination
                );
                let dir = self.path(&i.destination, false)?;
                // CleanList: MissingSourceFiles for a missing literal list, an
                // ArgumentException (UnknownError) on an all-wildcard empty expansion.
                let descendants = authored_descendants(&i.action);
                let sources = self.matches_scoped(&i.source, false, descendants);
                self.classify_miss(sources, &i.source, MissKind::LiteralTyped)?;
                // C# reads RealSourcePaths[0]: the first authored Source's file, not
                // the first in sorted-key order. Every Source matched above, so the
                // first one does; within one wildcard the sorted first match is used.
                let first = self.matches_scoped(&i.source[..1], false, descendants)?;
                let source = first.first().context("CleanList needs a source file")?;
                let cancelled = self.settings.cancelled.clone();
                let mut budget = cleanlist::Budget::new(&cancelled);
                let matcher = cleanlist::Matcher::new(&self.selected_names, &mut budget)?;
                let data = self.files[&self.path_policy.key(source)].1.open()?;
                let rows = cleanlist::read_rows(data, &mut budget)?;
                let mut planned = Vec::new();
                let mut seen = BTreeSet::new();
                for line in rows {
                    budget.charge(1)?;
                    if cleanlist::blank(&line) {
                        continue;
                    }
                    let Some((name, files)) = line.split_once(',') else {
                        self.validation_warning(
                            "CleanList",
                            "Invalid CleanList row: no files specified".into(),
                            source,
                        );
                        if self.install {
                            emit(Event {
                                component: component.name.clone(),
                                step: 0,
                                message: "Warning: invalid CleanList row: no files specified"
                                    .into(),
                            });
                        }
                        continue;
                    };
                    if matcher.matches(cleanlist::trim(name), &mut budget)? {
                        for raw in files
                            .split(',')
                            .map(cleanlist::trim)
                            .filter(|s| !s.is_empty())
                        {
                            budget.text(raw)?;
                            // C# deletes Path.Combine(target, name) when FileExists:
                            // a name no file can have is skipped as missing, and
                            // `[`/`]` are literal. Otherwise-parsed `..`, absolute,
                            // trailing-dot and device names stay refused.
                            if cleanlist::unnameable(raw) {
                                continue;
                            }
                            let path = self.actual(&dir.join(relative(raw, true)?));
                            self.settings.roots.check(&path)?;
                            let key = self.path_policy.key(&path);
                            if self.files.contains_key(&key) && seen.insert(key) {
                                ensure!(
                                    planned.len() < 100_000,
                                    "CleanList exceeds 100,000 deletions"
                                );
                                planned.push(path);
                            }
                        }
                    }
                }
                // No deletion occurs until decoding, work admission, and every
                // selected path have been qualified. Errors retain recovery.
                for path in planned {
                    budget.check()?;
                    self.delete(&path)?;
                }
            }
            "Patcher" => {
                if !i.destination.is_empty() {
                    // Still refused when unsafe (fail closed), though never used.
                    let dest = self.path(&i.destination, false)?;
                    if self.path_policy.key(&dest)
                        != self.path_policy.key(&self.settings.roots.game)
                    {
                        emit(Event {
                            component: component.name.clone(),
                            step: 0,
                            message: format!(
                                "Patcher Destination '{}' is not used; the patcher runs on \
                                 the game directory",
                                i.destination
                            ),
                        });
                    }
                }
                let mut resolution = self.resolve_instruction(component, i)?;
                self.publish_resolution(&mut resolution)?;
                let path = resolution
                    .sources
                    .first()
                    .context("Patcher has no resolved binding")?;
                ensure!(
                    operation_index.is_some()
                        || !crate::archive_generation::source_plan_namespace::contains_native(
                            component
                        ),
                    "An original source program requires its explicit current operation ordinal"
                );
                let external = if matches!(
                    self.settings.patcher_engine,
                    PatcherEngine::HoloPatcher | PatcherEngine::ModPackaged
                ) {
                    crate::archive_generation::external_patcher_source::resolve(
                        component,
                        i,
                        operation_index.context("External source requires operation ordinal")?,
                        &self.settings.roots.mods,
                        &self.settings.archive,
                        &self.settings.cancelled,
                    )?
                } else {
                    None
                };
                ensure!(
                    self.settings.patcher_engine != PatcherEngine::ModPackaged
                        || external.is_some(),
                    "No authenticated original external patcher source for this instruction"
                );
                let contract = if external.is_some() {
                    None
                } else {
                    self.native_patcher_contract(
                        component,
                        i,
                        operation_index.unwrap_or(usize::MAX),
                    )?
                };
                if self.install
                    && contract
                        .as_ref()
                        .is_some_and(|contract| contract.requires_exact_prediction())
                {
                    ensure!(
                        self.component_checkpoints && self.checkpoint_process.is_some(),
                        "Reviewed native publication requires its owned running component checkpoint and root lock"
                    );
                }
                if let Some(external) = external {
                    ensure!(
                        !self.exact_patcher_prediction,
                        "External backend effects require a matching verified real-disk checkpoint; native prediction cannot stand in for this backend"
                    );
                    if self.install {
                        self.process_original_external(i, &external, emit)?;
                    } else {
                        // Presence-only validation reads the authored patch-data VFS.
                        // It neither executes a backend nor claims known patch bytes.
                        let base = if self.files.contains_key(&self.path_policy.key(path)) {
                            path.parent().context("Patcher source has no parent")?
                        } else {
                            path
                        };
                        self.simulate_patcher(
                            base,
                            &self.settings.roots.game.clone(),
                            &i.arguments,
                            None,
                            emit,
                        )?;
                    }
                } else {
                    self.process_path(i, path, &[], contract.as_ref(), emit)?;
                }
            }
            "K1Widescreen" => self.execute_k1_widescreen(component, i, emit)?,
            "Execute" | "Run" => self.process(i, emit)?,
            other => bail!("Unsupported action {other}; refusing to skip it"),
        }
        Ok(())
    }

    /// Linked native patching never starts a child, including NSS compilation.
    /// A configured executable and Execute/Run still need process authority.
    fn requires_process_trust(&self, action: &str) -> bool {
        action != "Patcher"
            || self.patcher_invocation.is_some()
            || self.settings.provided_installer.is_some()
            || self.settings.patcher_engine == PatcherEngine::ModPackaged
    }

    /// Execute/Run only; Patcher binds through `resolve_instruction`.
    fn process(&mut self, i: &Instruction, emit: &mut impl FnMut(Event)) -> Result<()> {
        ensure!(
            !self.exact_patcher_prediction,
            "{} has unobserved external installer effects; apply it in real-disk mode and use its verified checkpoint before predicting beyond it",
            i.action
        );
        ensure!(
            self.settings.allow_processes,
            "{} needs explicit trust: enable external programs (--allow-processes)",
            i.action
        );
        let arguments = process_arguments::expand(&i.arguments, &self.settings.roots)?;
        ensure!(
            !i.source.is_empty(),
            "External program instruction has no Source"
        );
        // Resolve every authored source before any child starts. Preserve source
        // order, stable wildcard order, and first occurrence for overlapping globs.
        let mut paths = Vec::new();
        let mut seen = BTreeSet::new();
        for source in &i.source {
            let resolved = self.instruction_sources(std::slice::from_ref(source));
            // ExecuteProgram: UnknownError for a missing literal program; an
            // all-wildcard empty expansion checks FileExists(pattern) instead,
            // FileNotFoundPost (MissingSourceFiles).
            for path in self.classify_miss(resolved, &i.source, MissKind::WildcardTyped)? {
                if seen.insert(self.path_policy.key(&path)) {
                    paths.push(path);
                }
            }
        }
        for path in paths {
            self.check_cancelled()?;
            self.process_path(i, &path, &arguments, None, emit)?;
        }
        Ok(())
    }

    fn process_path(
        &mut self,
        i: &Instruction,
        path: &Path,
        arguments: &[std::ffi::OsString],
        native_contract: Option<&crate::archive_generation::reviewed_manual::NativePatcherContract>,
        emit: &mut impl FnMut(Event),
    ) -> Result<()> {
        if self.requires_process_trust(&i.action) && (self.install || i.action != "Patcher") {
            ensure!(
                self.settings.allow_processes,
                "{} needs explicit trust: enable external programs (--allow-processes)",
                i.action
            );
        }
        self.settings.roots.check(path)?;
        ensure!(
            self.files.contains_key(&self.path_policy.key(path))
                || self.directories.contains_key(&self.path_policy.key(path)),
            "Missing external program/mod: {}",
            path.display()
        );
        let preimage = if self.install && self.managed.is_some() {
            self.flush_managed()?;
            Some(self.managed_preimage()?)
        } else {
            None
        };
        if i.action == "Patcher" && self.settings.provided_installer.is_some() {
            self.process_supplied_installer(i, path, emit)?;
            if self.install {
                if let Some(preimage) = &preimage {
                    self.record_managed_process(preimage)?;
                }
                self.scan()?;
            }
            return Ok(());
        }
        let mut runner_session = None;
        let mut staged_patcher = None;
        let mut patcher_data = None;
        let mut native_patcher = None;
        let mut native_resource_sources = None;
        let mut command = if i.action == "Patcher" {
            // C# runs every patcher on the game folder (`ExecuteTSLPatcherAsync`
            // passes `MainConfig.DestinationPath`, and the VFS simulation uses it
            // too); its validation rewrites any other Destination to
            // `<<gameDirectory>>`. The Patcher arm has refused an unsafe one.
            let dest = self.settings.roots.game.clone();
            let base = if self.files.contains_key(&self.path_policy.key(path)) {
                path.parent().unwrap()
            } else {
                path
            };
            if !self.install {
                ensure!(
                    !self.exact_patcher_prediction || self.patcher_invocation.is_none(),
                    "Configured external patcher effects require a matching verified real-disk checkpoint; native OdyPatcher prediction cannot stand in for this backend"
                );
                self.simulate_patcher(base, &dest, &i.arguments, native_contract, emit)?;
                return Ok(());
            }
            self.check_patcher_backend()?;
            let (stage, staged_base) = patcher_prepare::stage(base, &self.settings.cancelled)?;
            if let Some(contract) = native_contract {
                contract.verify_staged(&staged_base, &self.settings.cancelled)?;
            }
            native_resource_sources = source_plan_resources::prepare(
                native_contract,
                &staged_base,
                &dest,
                &self.settings.cancelled,
            )?;
            patcher_data = Some(staged_base.clone());
            staged_patcher = Some(stage);
            let mut cmd = self
                .patcher_invocation
                .as_ref()
                .map(|patcher| self.patcher_command(patcher, &dest, &staged_base))
                .transpose()?;
            let mut namespace = None;
            if !i.arguments.trim().is_empty() {
                if let Ok(index) = i.arguments.trim().parse::<usize>() {
                    namespace = Some(index);
                    if let Some(cmd) = &mut cmd {
                        cmd.arg("--namespace-option-index").arg(index.to_string());
                    }
                } else {
                    let name = relative(i.arguments.trim(), false)?;
                    ensure!(
                        name.components().count() == 1
                            && name
                                .extension()
                                .is_some_and(|e| e.eq_ignore_ascii_case("ini")),
                        "Patcher Arguments must be an INI filename or namespace index"
                    );
                    let data = if base
                        .file_name()
                        .is_some_and(|s| s.eq_ignore_ascii_case("tslpatchdata"))
                    {
                        base.to_path_buf()
                    } else {
                        base.join("tslpatchdata")
                    };
                    let ini = self.actual(&data.join(name));
                    ensure!(
                        self.files.contains_key(&self.path_policy.key(&ini)),
                        "Missing patcher INI: {}",
                        ini.display()
                    );
                    let actual_data = self.actual(&data);
                    let target = staged_base.join(actual_data.strip_prefix(base)?);
                    recovery::atomic_copy(
                        &target.join("changes.ini"),
                        crate::paths::open_regular_read(
                            &staged_base.join(ini.strip_prefix(base)?),
                        )?,
                        &self.settings.cancelled,
                    )?;
                    let mut namespace_roots = vec![staged_base.clone()];
                    if target != staged_base {
                        namespace_roots.push(target);
                    }
                    for directory in namespace_roots {
                        for entry in fs::read_dir(directory)? {
                            let entry = entry?;
                            if entry.file_name().eq_ignore_ascii_case("namespaces.ini") {
                                fs::remove_file(entry.path())?;
                            }
                        }
                    }
                }
            }
            if cmd.is_none() {
                native_patcher = Some((dest, namespace));
                if staged_base
                    .file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case("tslpatchdata"))
                {
                    patcher_data = Some(
                        staged_base
                            .parent()
                            .context("Staged patcher data has no parent")?
                            .to_path_buf(),
                    );
                }
            }
            if let Some(contract) = native_contract {
                let raw_base = patcher_data
                    .as_deref()
                    .context("Missing verified private patcher package")?;
                ensure!(
                    namespace == contract.selected_namespace(),
                    "Native namespace argument differs from its compiled selection"
                );
                let raw_ini = if let Some(relative) = contract.selected_ini_relative_path() {
                    raw_base.join(crate::paths::relative(relative, false)?)
                } else {
                    odypatcher::Session::discover(
                        raw_base
                            .to_str()
                            .context("Non UTF8 private patcher package")?,
                        "changes.ini",
                        "info.rtf",
                    )
                    .ini_path()
                };
                contract.verify_selected_configuration(
                    raw_base,
                    namespace,
                    &raw_ini,
                    &self.settings.cancelled,
                )?;
                if let Some(receipt) = patcher_prepare::prepare_native_ini(
                    Some(contract),
                    raw_base,
                    &raw_ini,
                    &self.settings.cancelled,
                )? {
                    emit(Event {
                        component: "Patcher".into(),
                        step: 0,
                        message: format!("Source-pinned native INI preparation: {receipt}"),
                    });
                }
            }
            cmd
        } else {
            // An imported installer is logically present but its immutable object
            // is outside the source tree. Materialize only its logical name for
            // real execution; dry-run records the same availability transition.
            if self.private_authoring_input(path) {
                if self.install {
                    match fs::symlink_metadata(path) {
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error.into()),
                        Ok(_) => bail!(
                            "Imported program path appeared after preflight; restore before retrying"
                        ),
                    }
                }
                let content = self.files[&self.path_policy.key(path)].1.clone();
                self.put(path, content, true)?;
            }
            let execute_runner = self
                .settings
                .execute_runner
                .as_ref()
                .or(self.settings.provided_installer.as_ref());
            let mut cmd = if self.install && execute_runner.is_some() {
                let runner = execute_runner.context("Missing headless runner")?;
                let requests = fs::canonicalize(&runner.request_dir)?;
                ensure!(
                    !requests.starts_with(&self.settings.roots.mods)
                        && !requests.starts_with(&self.settings.roots.game),
                    "Runner request storage must be outside mod/game trees"
                );
                let (lock, operation) = self.checkpoint_process.as_ref().context(
                    "Supplied installer execution requires its owned Materializing checkpoint operation",
                )?;
                let session = provided_installer::RunnerSession::start(
                    runner,
                    path,
                    &self.settings.roots,
                    lock,
                    operation,
                )?;
                let command = runner.command(path, &session.headless.display)?;
                runner_session = Some(session);
                command
            } else {
                ensure!(
                    !self.install
                        || !cfg!(target_os = "linux")
                        || !path
                            .extension()
                            .and_then(|part| part.to_str())
                            .is_some_and(|part| part.eq_ignore_ascii_case("exe")),
                    "Windows Execute requires explicitly configured headless Wine runner"
                );
                Command::new(path)
            };
            cmd.args(arguments);
            Some(cmd)
        };
        if self.install {
            if let Some(data) = &patcher_data {
                if let Some(contract) =
                    native_contract.filter(|contract| contract.original_program_run().is_some())
                {
                    let selected = data.join(
                        contract
                            .selected_ini_relative_path()
                            .context("Original program lost selected INI path")?,
                    );
                    patcher_prepare::normalize_selected(data, &selected, &self.settings.cancelled)?;
                } else {
                    patcher_prepare::normalize(data, &self.settings.cancelled)?;
                }
                clear_patcher_logs(data)?;
            }
            let receipt = if let Some(mut command) = command.take() {
                command.current_dir(
                    patcher_data
                        .as_deref()
                        .unwrap_or_else(|| path.parent().unwrap()),
                );
                if let Some(session) = &mut runner_session {
                    session.mark_submitted();
                }
                let result = self.run_external_command(command);
                if let Some(session) = &mut runner_session {
                    session.finish(result.as_ref().is_ok_and(|receipt| {
                        receipt.no_child_started || receipt.process_group_quiescent
                    }))?;
                }
                Some(result?)
            } else {
                let (destination, namespace) = native_patcher
                    .as_ref()
                    .context("Missing native patcher destination")?;
                self.run_builtin_patcher(
                    patcher_data.as_deref().context("Missing patcher staging")?,
                    destination,
                    *namespace,
                    native_contract,
                    native_resource_sources.as_ref(),
                    emit,
                )?;
                None
            };
            // Read and publish diagnostics before checking process success or dropping
            // named-INI staging. The event owns its text after the directory is gone.
            let patcher_result = patcher_data.as_ref().map(|data| {
                let result = patcher_install_log(data);
                match &result {
                    Ok((log, content)) => emit(Event {
                        component: i.action.clone(),
                        step: 0,
                        message: format!("Patcher install log {}:\n{content}", log.display()),
                    }),
                    Err(error) => emit(Event {
                        component: i.action.clone(),
                        step: 0,
                        message: format!("Patcher install log verification failed: {error:#}"),
                    }),
                }
                result
            });
            if let Some(receipt) = receipt {
                for output in [receipt.stdout, receipt.stderr] {
                    if !output.is_empty() {
                        emit(Event {
                            component: i.action.clone(),
                            step: 0,
                            message: output,
                        });
                    }
                }
                ensure!(
                    !receipt.cancelled,
                    "External program cancelled; restore before retrying"
                );
                if let Some(failure) = receipt.failure {
                    bail!("External program failed: {failure}; restore before retrying");
                }
                ensure!(
                    receipt.exit_code == Some(0),
                    "External program failed with exit code {:?}",
                    receipt.exit_code
                );
            }
            if let Some(result) = patcher_result {
                let (_, content) = result?;
                let errors = patcher_log_error_lines(&content);
                ensure!(
                    errors.is_empty(),
                    "Patcher install log reported errors: {}",
                    errors.join(" | ")
                );
            }
            if let Some(preimage) = &preimage {
                self.record_managed_process(preimage)?;
            }
            self.scan()?;
        }
        drop(staged_patcher);
        drop(runner_session);
        Ok(())
    }

    fn patcher_command(
        &self,
        patcher: &crate::patcher::Invocation,
        destination: &Path,
        data: &Path,
    ) -> Result<Command> {
        let mut command = patcher.command()?;
        if self.settings.patcher_engine == PatcherEngine::KPatcher && !cfg!(windows) {
            command.arg("--console");
        }
        command
            .arg("--install")
            .arg("--game-dir")
            .arg(destination)
            .arg("--tslpatchdata")
            .arg(data);
        if self.settings.patcher_engine == PatcherEngine::OdyPatcher {
            command.arg("--cli").arg("-y");
        }
        Ok(command)
    }

    fn check_patcher_backend(&self) -> Result<()> {
        if self.settings.patcher_engine == PatcherEngine::ModPackaged {
            ensure!(
                self.settings.execute_runner.is_some(),
                "Mod-packaged backend requires explicit headless runner configuration"
            );
            return Ok(());
        }
        if self.settings.provided_installer.is_some() {
            return Ok(());
        }
        if let Some(error) = &self.patcher_resolution_error {
            bail!("Patcher backend unavailable: {error}");
        }
        if let Some(invocation) = &self.patcher_invocation {
            invocation.command()?;
        } else {
            ensure!(
                self.settings.patcher_engine == PatcherEngine::OdyPatcher,
                "Configure an executable with --patcher or install complete packaged patcher resources"
            );
        }
        Ok(())
    }

    fn run_builtin_patcher(
        &self,
        base: &Path,
        destination: &Path,
        namespace: Option<usize>,
        contract: Option<&crate::archive_generation::reviewed_manual::NativePatcherContract>,
        resource_sources: Option<&source_plan_resources::PreparedResources>,
        emit: &mut impl FnMut(Event),
    ) -> Result<()> {
        patcher_prepare::verify_required_game_language(
            contract,
            destination,
            &self.settings.cancelled,
        )?;
        // Discovery expects these canonical document names. Adjust only the
        // private payload; the source mod's spelling and bytes stay intact.
        canonicalize_patcher_layout(base)?;
        let mut session = odypatcher::Session::discover(
            base.to_str().context("Patcher staging path is not UTF-8")?,
            contract
                .and_then(|contract| contract.selected_ini_name())
                .unwrap_or("changes.ini"),
            "info.rtf",
        );
        if let Some(index) = namespace {
            ensure!(
                index < session.namespaces.len().max(1),
                "Patcher namespace is out of range"
            );
            if !session.namespaces.is_empty() {
                session.select_namespace(index);
            }
        }
        ensure!(
            session.blocked_reason().is_none(),
            "Patcher configuration blocked: {}",
            session.blocked_reason().unwrap_or_default()
        );
        self.check_cancelled()?;
        let strict_before = if contract.is_some() {
            contract
                .context("Missing reviewed selection contract")?
                .verify_selected_path(
                    base,
                    namespace,
                    &session.ini_path(),
                    &self.settings.cancelled,
                )?;
            Some(reviewed_patcher::before(
                base,
                destination,
                &session.load_logs,
                &self.settings.cancelled,
            )?)
        } else {
            None
        };
        let mut installer = odypatcher::ModInstaller::new(
            &session.base_path,
            destination,
            &session.ini_path(),
            Some(odypatcher::PatchLogger::new()),
        )
        .map_err(|error| anyhow::anyhow!("Prepare built-in OdyPatcher: {error}"))?;
        if let Some(expected) = contract.and_then(|contract| contract.expected_game()) {
            ensure!(
                installer.game == Some(expected),
                "Source-plan native recipe game differs from the actual detected destination game"
            );
        }
        companion_selector::adapt(contract, &mut installer)?;
        let tomb_compatibility = tomb_shadow_compatibility::adapt(
            contract,
            &mut installer,
            base,
            &self.settings.cancelled,
        )?;
        if let Some(contract) = contract {
            strict_nss::prepare(
                contract,
                base,
                &mut installer,
                self.settings.cancelled.clone(),
            )?;
        }
        installer
            .validate_configured_paths()
            .map_err(|error| anyhow::anyhow!("Unsafe native patch configuration: {error}"))?;
        patcher_prepare::disable_external_compilers(&mut installer)?;
        let result = installer.install(
            Some(&|| self.settings.cancelled.load(Ordering::Acquire)),
            None,
        );
        let report = odypatcher::RunReport::from_logger(&installer.log);
        let log_path = session.write_install_log(&installer.log.all_logs);
        emit(Event {
            component: "Patcher".into(),
            step: 0,
            message: format!(
                "Built-in OdyPatcher: {} configured, {} completed, {} skipped, {} failed; {} warnings, {} errors",
                report.patches_configured,
                report.patches_completed,
                report.patches_skipped,
                report.patches_failed,
                report.warnings,
                report.errors
            ),
        });
        if contract.is_some_and(|contract| contract.tomb_shadow_identity_observation()) {
            // Read-only evidence precedes the strict gate, which remains fatal.
            // Never turn a failed observation into a warning waiver.
            let observation = match tomb_shadow_observer::observe(
                base,
                destination,
                &self.settings.cancelled,
            ) {
                Ok(value) => value,
                Err(_) => {
                    serde_json::json!({"Status":"observation-unavailable","StrictOutcomeRequired":true})
                }
            };
            emit(Event {
                component: "Patcher".into(),
                step: 0,
                message: format!("TOMB script-shadow identity observation: {observation}"),
            });
        }
        let outcome = result
            .map_err(|error| anyhow::anyhow!("Built-in OdyPatcher failed: {error}"))
            .and_then(|()| self.check_cancelled())
            .and_then(|()| {
                ensure!(
                    !report.completed_with_errors(),
                    "Built-in OdyPatcher reported errors; restore before retrying"
                );
                ensure!(
                    log_path.is_some(),
                    "Built-in OdyPatcher did not write its install log"
                );
                let compatibility = tomb_compatibility
                    .map(|prepared| {
                        prepared.finish(base, destination, &report, &self.settings.cancelled)
                    })
                    .transpose()?;
                let compiled_nss = contract
                    .map(|contract| {
                        strict_nss::finish(contract, &installer, &self.settings.cancelled)
                    })
                    .transpose()?
                    .unwrap_or_default();
                let resource_evidence = resource_sources
                    .map(|prepared| prepared.finish(&report))
                    .transpose()?;
                if let Some(before) = strict_before {
                    let mut receipt = reviewed_patcher::finish(
                        before,
                        base,
                        destination,
                        &report,
                        &installer.log.all_logs,
                        &self.settings.cancelled,
                    )?;
                    if let Some(compatibility) = compatibility {
                        receipt["Compatibility"] = compatibility;
                    }
                    if let Some(resources) = resource_evidence {
                        receipt["DestinationResourceSources"] = resources;
                    }
                    if let Some(run) =
                        contract.and_then(|authority| authority.original_program_run())
                    {
                        receipt["OriginalProgramRun"] = run.clone();
                    }
                    if !compiled_nss.is_empty() {
                        receipt["CertifiedNssCompilation"] = serde_json::to_value(compiled_nss)?;
                    }
                    receipt["ComponentGuid"] = serde_json::json!(
                        contract
                            .context("Reviewed Patcher outcome lost its authority")?
                            .component_guid()
                    );
                    emit(Event {
                        component: "Patcher".into(),
                        step: 0,
                        message: format!("Reviewed native Patcher outcome: {receipt}"),
                    });
                }
                Ok(())
            });
        if let Err(error) = outcome {
            // The staged install log is deleted with its staging directory, so the
            // diagnosis must be published here, before the error leaves.
            emit_patcher_failure_log(&installer.log.all_logs, emit);
            return Err(anyhow::anyhow!(
                "{error}{}",
                patcher_failure_summary(&installer.log.all_logs)
            ));
        }
        Ok(())
    }
}

const PATCHER_FAILURE_TAIL_LINES: usize = 200;
const PATCHER_FAILURE_SUMMARY_ERRORS: usize = 3;
const PATCHER_FAILURE_LINE_CHARS: usize = 400;

fn bounded_patcher_line(line: &str) -> String {
    match line.char_indices().nth(PATCHER_FAILURE_LINE_CHARS) {
        Some((end, _)) => format!("{}...", &line[..end]),
        None => line.to_owned(),
    }
}

fn patcher_log_lines(log: &odypatcher::PatchLog) -> Vec<String> {
    log.formatted_message()
        .lines()
        .map(bounded_patcher_line)
        .collect()
}

fn emit_patcher_failure_log(logs: &[odypatcher::PatchLog], emit: &mut impl FnMut(Event)) {
    let mut flagged = Vec::new();
    let mut flagged_total = 0usize;
    for log in logs.iter().filter(|log| {
        matches!(
            log.log_type,
            odypatcher::LogType::Error | odypatcher::LogType::Warning
        )
    }) {
        for line in patcher_log_lines(log) {
            flagged_total += 1;
            if flagged.len() < PATCHER_FAILURE_TAIL_LINES {
                flagged.push(line);
            }
        }
    }
    if flagged.is_empty() {
        emit(Event {
            component: "Patcher".into(),
            step: 0,
            message: "Built-in OdyPatcher logged no error or warning lines".into(),
        });
    } else {
        emit(Event {
            component: "Patcher".into(),
            step: 0,
            message: format!(
                "Built-in OdyPatcher error and warning lines ({} of {flagged_total}):\n{}",
                flagged.len(),
                flagged.join("\n")
            ),
        });
    }
    let mut tail = std::collections::VecDeque::with_capacity(PATCHER_FAILURE_TAIL_LINES);
    let mut total = 0usize;
    for line in logs.iter().flat_map(patcher_log_lines) {
        total += 1;
        if tail.len() == PATCHER_FAILURE_TAIL_LINES {
            tail.pop_front();
        }
        tail.push_back(line);
    }
    let tail = tail.into_iter().collect::<Vec<_>>();
    emit(Event {
        component: "Patcher".into(),
        step: 0,
        message: format!(
            "Built-in OdyPatcher install log tail (last {} of {total} lines):\n{}",
            tail.len(),
            tail.join("\n")
        ),
    });
}

fn patcher_failure_summary(logs: &[odypatcher::PatchLog]) -> String {
    let errors = logs
        .iter()
        .filter(|log| log.log_type == odypatcher::LogType::Error)
        .filter_map(|log| {
            log.message
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(bounded_patcher_line)
        })
        .take(PATCHER_FAILURE_SUMMARY_ERRORS)
        .collect::<Vec<_>>();
    if errors.is_empty() {
        String::new()
    } else {
        format!(
            "; first {} error line{} from the install log: {}",
            errors.len(),
            if errors.len() == 1 { "" } else { "s" },
            errors.join(" | ")
        )
    }
}

fn canonicalize_patcher_layout(base: &Path) -> Result<()> {
    canonicalize_patcher_child(base, "tslpatchdata")?;
    let data = base.join("tslpatchdata");
    if data.is_dir() {
        for name in ["changes.ini", "info.rtf", "namespaces.ini"] {
            canonicalize_patcher_child(&data, name)?;
        }
    }
    Ok(())
}

fn canonicalize_patcher_child(directory: &Path, name: &str) -> Result<()> {
    let mut found = None;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_name().eq_ignore_ascii_case(name) {
            ensure!(found.is_none(), "Case-colliding patcher documents");
            found = Some(entry.path());
        }
    }
    if let Some(path) = found {
        let target = directory.join(name);
        if path != target {
            fs::rename(path, target)?;
        }
    }
    Ok(())
}

// Only these paths at the resolved invocation root can prove this run's log
// postcondition. RTF precedes TXT, matching the reference verifier.
const PATCHER_LOG_NAMES: [&str; 2] = ["installlog.rtf", "installlog.txt"];

fn clear_patcher_logs(data: &Path) -> Result<()> {
    for name in PATCHER_LOG_NAMES {
        let log = data.join(name);
        match fs::remove_file(&log) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "Cannot remove previous patcher install log {}",
                        log.display()
                    )
                });
            }
        }
    }
    Ok(())
}

fn patcher_install_log(data: &Path) -> Result<(PathBuf, String)> {
    for name in PATCHER_LOG_NAMES {
        let log = data.join(name);
        let metadata = match fs::symlink_metadata(&log) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("Cannot inspect patcher install log {}", log.display())
                });
            }
        };
        ensure!(
            metadata.is_file(),
            "Patcher install log must be a regular file: {}",
            log.display()
        );
        // Failure diagnostics remain available after cancellation. This bounded
        // read deliberately does not consult the action's cancellation flag.
        use std::io::Read;
        const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;
        let file = crate::paths::open_regular_read(&log)
            .with_context(|| format!("Cannot open patcher install log {}", log.display()))?;
        ensure!(
            file.metadata()?.len() <= MAX_LOG_BYTES,
            "Patcher install log exceeds the 8 MiB limit: {}",
            log.display()
        );
        let mut bytes = Vec::new();
        file.take(MAX_LOG_BYTES + 1)
            .read_to_end(&mut bytes)
            .with_context(|| format!("Cannot read patcher install log {}", log.display()))?;
        ensure!(
            bytes.len() as u64 <= MAX_LOG_BYTES,
            "Patcher install log exceeds the 8 MiB limit: {}",
            log.display()
        );
        let content = String::from_utf8(bytes)
            .with_context(|| format!("Patcher install log is not UTF-8: {}", log.display()))?;
        return Ok((log, content));
    }
    bail!(
        "Patcher install log not found at invocation root {} (expected installlog.rtf or installlog.txt)",
        data.display()
    )
}

fn patcher_log_error_lines(content: &str) -> Vec<String> {
    content
        .split(['\r', '\n'])
        .filter(|line| {
            !line.trim().is_empty() && (line.contains("Error:") || line.contains("[Error]"))
        })
        .map(str::to_owned)
        .collect()
}
