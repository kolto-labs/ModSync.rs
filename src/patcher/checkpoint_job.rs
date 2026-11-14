//! Preparatory Windows checkpoint process fence. Engine enablement remains separate.
//! The lease borrows the authoritative roots lock; restart cleanup never creates a job.
mod security;
use crate::{
    checkpoint::{OperationIntent, RootBinding, Store, persistence},
    paths::Roots,
    recovery,
};
use anyhow::{Context, Result, ensure};
use security::Identity;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::{Path, PathBuf},
    ptr,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, GetLastError, HANDLE, SetLastError},
    Security::SECURITY_ATTRIBUTES,
    System::JobObjects::{
        CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation, OpenJobObjectW,
        QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
    },
    System::SystemServices::{JOB_OBJECT_QUERY, JOB_OBJECT_TERMINATE},
};
const LIMIT: u64 = 16 * 1024;
const FILE_NAME: &str = "external-job-v1.json";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    version: u32,
    roots: RootBinding,
    session_id: String,
    operation_id: String,
    from_checkpoint_id: String,
    expected_generation: u64,
    component_sha256: String,
    launch_id: String,
    job_nonce: String,
    identity: Identity,
    phase: String,
}
impl Record {
    fn job_name(&self) -> String {
        format!("Local\\ModSync.Checkpoint.{}", self.job_nonce)
    }
    fn validate(&self, roots: &Roots, identity: &Identity) -> Result<()> {
        ensure!(
            self.version == 1 && matches!(self.phase.as_str(), "LaunchIntent" | "Drained"),
            "Unsupported external process record"
        );
        ensure!(
            &self.identity == identity,
            "External process belongs to another user, logon, or Windows session"
        );
        ensure!(
            self.roots == binding(roots)?,
            "External process roots differ"
        );
        for id in [
            &self.session_id,
            &self.operation_id,
            &self.from_checkpoint_id,
            &self.launch_id,
        ] {
            ensure!(
                uuid::Uuid::parse_str(id).is_ok_and(|value| value.to_string() == *id),
                "Invalid process-fence identity"
            );
        }
        ensure!(
            self.job_nonce.len() == 64
                && self
                    .job_nonce
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
            "Invalid job nonce"
        );
        ensure!(
            self.expected_generation > 0
                && self.component_sha256.len() == 64
                && self
                    .component_sha256
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
            "Invalid process-fence checkpoint binding"
        );
        Ok(())
    }
    fn matches(&self, operation: &OperationIntent) -> bool {
        operation.kind == "InstallComponent"
            && operation.phase == "Materializing"
            && operation.roots == self.roots
            && operation.session_id == self.session_id
            && operation.operation_id == self.operation_id
            && operation.from_checkpoint_id == self.from_checkpoint_id
            && operation.expected_generation == self.expected_generation
            && operation.component_sha256 == self.component_sha256
    }
}
fn binding(roots: &Roots) -> Result<RootBinding> {
    Ok(RootBinding {
        mods: roots.mods.to_str().context("Non-UTF8 mods root")?.into(),
        game: roots.game.to_str().context("Non-UTF8 game root")?.into(),
    })
}
fn safe_namespace(roots: &Roots, identity: &Identity) -> Result<()> {
    use std::os::windows::fs::MetadataExt;
    let state = recovery::state_dir(roots);
    for ancestor in state.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                ensure!(
                    metadata.is_dir() && metadata.file_attributes() & 0x400 == 0,
                    "Unsafe external process namespace"
                );
                if ancestor == state {
                    security::check_ancestor(ancestor, identity)?;
                } else {
                    security::check_outer_ancestor(ancestor, identity)?;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
/// Creates the game-scoped recovery directory privately for the current user, so
/// the process fence never depends on the game folder's inherited permissions.
pub(crate) fn create_state_dir(dir: &Path) -> Result<()> {
    security::create_private_directory(dir, &Identity::current()?)
}
fn path(roots: &Roots) -> PathBuf {
    recovery::state_dir(roots).join(FILE_NAME)
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn lock_binding(lock: &recovery::Lock, roots: &Roots) -> Result<()> {
    ensure!(
        lock.guards(roots),
        "External process requires matching held root locks"
    );
    let canonical = Roots::new(&roots.mods, &roots.game)?;
    ensure!(
        canonical.mods == roots.mods && canonical.game == roots.game,
        "External process roots changed"
    );
    Store::open(roots)?.prepare_write()?;
    Ok(())
}
fn current(record: &Record, roots: &Roots) -> Result<()> {
    let operations = Store::open(roots)?.pending_operations()?;
    ensure!(
        operations.len() == 1 && record.matches(&operations[0]),
        "External process is not bound to the authoritative Materializing component"
    );
    Ok(())
}
fn read(roots: &Roots, identity: &Identity) -> Result<Option<Record>> {
    safe_namespace(roots, identity)?;
    let path = path(roots);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    security::check_directory(
        path.parent().context("Missing process-fence parent")?,
        identity,
    )?;
    let file = crate::paths::open_regular_read(&path)?;
    security::check_file(&file, identity)?;
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= LIMIT,
        "External process record exceeds bound"
    );
    let record: Record = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("Malformed external process record"))?;
    record.validate(roots, identity)?;
    Ok(Some(record))
}
fn write(roots: &Roots, record: &Record, replace: bool) -> Result<()> {
    safe_namespace(roots, &record.identity)?;
    let destination = path(roots);
    let parent = destination
        .parent()
        .context("Missing process-fence parent")?;
    security::check_directory(parent, &record.identity)?;
    let bytes = serde_json::to_vec(record)?;
    ensure!(
        bytes.len() as u64 <= LIMIT,
        "External process record exceeds bound"
    );
    let mut temporary = tempfile::Builder::new()
        .prefix(".external-job-")
        .tempfile_in(parent)?;
    security::protect_new_file(temporary.path(), &record.identity)?;
    temporary.write_all(&bytes)?;
    persistence::publish_temp(temporary, &destination, replace)?;
    persistence::namespace_boundary(parent)?;
    ensure!(
        read(roots, &record.identity)?.as_ref() == Some(record),
        "External process publication differs"
    );
    Ok(())
}
fn finish(roots: &Roots, record: &Record) -> Result<()> {
    ensure!(
        read(roots, &record.identity)?.as_ref() == Some(record),
        "External process record changed before cleanup"
    );
    let mut drained = record.clone();
    drained.phase = "Drained".into();
    write(roots, &drained, true)?;
    ensure!(
        read(roots, &record.identity)?.as_ref() == Some(&drained),
        "External process record changed during cleanup"
    );
    fs::remove_file(path(roots))?;
    persistence::namespace_boundary(&recovery::state_dir(roots))
}
fn limits(job: HANDLE) -> Result<()> {
    let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    ensure!(
        unsafe {
            QueryInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&mut info as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&info) as u32,
                ptr::null_mut(),
            )
        } != 0,
        "Cannot query recorded job policy"
    );
    ensure!(
        info.BasicLimitInformation.LimitFlags == JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        "Recorded job policy differs"
    );
    Ok(())
}
fn active(job: HANDLE) -> Result<u32> {
    let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
    ensure!(
        unsafe {
            QueryInformationJobObject(
                job,
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                std::mem::size_of_val(&info) as u32,
                ptr::null_mut(),
            )
        } != 0,
        "Cannot query recorded job membership"
    );
    Ok(info.ActiveProcesses)
}
/// Only presence is needed for normal mutation guards: corrupt or foreign records
/// also block. This reader never opens or signals a kernel object.
pub(crate) fn ensure_clear_game(game: &Path) -> Result<()> {
    let provisional = Roots {
        mods: game.to_owned(),
        game: game.to_owned(),
    };
    safe_namespace(&provisional, &Identity::current()?)?;
    match fs::symlink_metadata(path(&provisional)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(_) => anyhow::bail!(
            "An external process recovery fence exists; settle it before modifying the game or checkpoints"
        ),
    }
}
/// A read-only persisted-fence projection, not proof of current kernel membership.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    pub present: bool,
    pub session_id: Option<String>,
    pub operation_id: Option<String>,
    pub launch_id: Option<String>,
    pub phase: Option<String>,
}
/// Validates persisted authority without opening, querying, or signaling a job.
/// A concurrent writer can invalidate this observation; mutations must recheck
/// under the matching root lock. Malformed or foreign state is always an error.
pub(crate) fn status(roots: &Roots) -> Result<Status> {
    let canonical = Roots::new(&roots.mods, &roots.game)?;
    ensure!(
        canonical.mods == roots.mods && canonical.game == roots.game,
        "External process roots changed"
    );
    let identity = Identity::current()?;
    let Some(record) = read(roots, &identity)? else {
        return Ok(Status {
            present: false,
            session_id: None,
            operation_id: None,
            launch_id: None,
            phase: None,
        });
    };
    current(&record, roots)?;
    ensure!(
        read(roots, &identity)?.as_ref() == Some(&record),
        "External process record changed during status inspection"
    );
    Ok(Status {
        present: true,
        session_id: Some(record.session_id),
        operation_id: Some(record.operation_id),
        launch_id: Some(record.launch_id),
        phase: Some(record.phase),
    })
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuiescenceReport {
    pub had_record: bool,
    pub launch_id: Option<String>,
    pub absent_job: bool,
}
/// Retains the lock borrow and non-inheritable original job handle across launch.
pub(crate) struct Lease<'a> {
    _lock: &'a recovery::Lock,
    roots: Roots,
    record: Record,
    job: OwnedHandle,
}
impl<'a> Lease<'a> {
    pub(crate) fn prepare(
        lock: &'a recovery::Lock,
        roots: &Roots,
        intent: &OperationIntent,
    ) -> Result<Self> {
        lock_binding(lock, roots)?;
        let identity = Identity::current()?;
        ensure!(
            read(roots, &identity)?.is_none(),
            "External process fence already exists"
        );
        let record = Record {
            version: 1,
            roots: binding(roots)?,
            session_id: intent.session_id.clone(),
            operation_id: intent.operation_id.clone(),
            from_checkpoint_id: intent.from_checkpoint_id.clone(),
            expected_generation: intent.expected_generation,
            component_sha256: intent.component_sha256.clone(),
            launch_id: uuid::Uuid::new_v4().to_string(),
            job_nonce: format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ),
            identity,
            phase: "LaunchIntent".into(),
        };
        record.validate(roots, &record.identity)?;
        current(&record, roots)?;
        // Compare the complete caller input too; its fields cannot replace the persisted authority.
        ensure!(
            Store::open(roots)?.pending_operations()?.as_slice() == [intent.clone()],
            "Caller component intent differs from persisted authority"
        );
        let descriptor = record.identity.descriptor()?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        let name = wide(&record.job_name());
        unsafe {
            SetLastError(0);
        }
        let raw = unsafe { CreateJobObjectW(&attributes, name.as_ptr()) };
        let error = unsafe { GetLastError() }; // capture before any other Windows call
        ensure!(!raw.is_null(), "Cannot create checkpoint job ({error})");
        let job = unsafe { OwnedHandle::from_raw_handle(raw) };
        ensure!(
            error != ERROR_ALREADY_EXISTS,
            "Checkpoint job name collision; no existing job adopted"
        );
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        ensure!(
            unsafe {
                SetInformationJobObject(
                    raw,
                    JobObjectExtendedLimitInformation,
                    (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&info) as u32,
                )
            } != 0,
            "Cannot set checkpoint job policy"
        );
        limits(raw)?;
        security::check_job(raw, &record.identity)?;
        write(roots, &record, false)?;
        Ok(Self {
            _lock: lock,
            roots: roots.clone(),
            record,
            job,
        })
    }
    /// Consumes the lease: no caller can launch a second command under a removed fence.
    pub(crate) fn run(
        self,
        command: std::process::Command,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<super::standalone::CommandReceipt> {
        ensure!(
            timeout >= Duration::from_secs(1) && timeout <= Duration::from_secs(86400),
            "Execution timeout exceeds bounds"
        );
        current(&self.record, &self.roots)?;
        ensure!(
            read(&self.roots, &self.record.identity)?.as_ref() == Some(&self.record),
            "External process lease record changed"
        );
        let result = super::standalone::windows::run_command_with_job(
            command,
            timeout,
            cancelled,
            self.job.as_raw_handle(),
        );
        // The runner retains cleanup ownership until all members drain, even on error.
        // Pre-launch failures leave the fresh job empty. Uncertainty leaves the fence.
        ensure!(
            active(self.job.as_raw_handle())? == 0,
            "Checkpoint job is not quiescent"
        );
        finish(&self.roots, &self.record)?;
        result
    }
}
/// Explicit recovery only. Normal mutation guards never call this function.
pub(crate) fn recover(
    lock: &recovery::Lock,
    roots: &Roots,
    cancelled: &AtomicBool,
) -> Result<QuiescenceReport> {
    lock_binding(lock, roots)?;
    let identity = Identity::current()?;
    let Some(record) = read(roots, &identity)? else {
        return Ok(QuiescenceReport {
            had_record: false,
            launch_id: None,
            absent_job: false,
        });
    };
    current(&record, roots)?;
    let mut absent = false;
    if record.phase != "Drained" {
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "External process recovery cancelled"
        );
        let name = wide(&record.job_name());
        let raw = unsafe {
            OpenJobObjectW(
                JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE | 0x0002_0000,
                0,
                name.as_ptr(),
            )
        };
        if raw.is_null() {
            let error = unsafe { GetLastError() };
            ensure!(
                error == ERROR_FILE_NOT_FOUND,
                "Cannot reopen checkpoint job ({error}); fence retained"
            );
            absent = true;
        } else {
            let job = unsafe { OwnedHandle::from_raw_handle(raw) };
            security::check_job(job.as_raw_handle(), &identity)?;
            limits(job.as_raw_handle())?;
            ensure!(
                unsafe { TerminateJobObject(job.as_raw_handle(), 1) } != 0,
                "Cannot terminate recorded job; fence retained"
            );
            let started = Instant::now();
            loop {
                if active(job.as_raw_handle())? == 0 {
                    break;
                }
                ensure!(
                    !cancelled.load(Ordering::Acquire),
                    "External process recovery cancelled; fence retained"
                );
                ensure!(
                    started.elapsed() < Duration::from_secs(30),
                    "Recorded job remains active; recovery fence retained"
                );
                std::thread::sleep(Duration::from_millis(40));
            }
        }
    }
    finish(roots, &record)?;
    Ok(QuiescenceReport {
        had_record: true,
        launch_id: Some(record.launch_id),
        absent_job: absent,
    })
}
