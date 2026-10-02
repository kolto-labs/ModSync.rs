//! Suspended launch into a private kill-on-close Job Object; no shell.
use super::{CommandReceipt, Options, Output};
use anyhow::{Context, Result, ensure};
use std::{
    ffi::{OsStr, OsString},
    fs::{File, OpenOptions},
    os::windows::{ffi::OsStrExt, io::AsRawHandle},
    ptr,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, INVALID_HANDLE_VALUE,
        WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Globalization::{CSTR_EQUAL, CompareStringOrdinal},
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        Threading::{
            CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
            DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess,
            GetExitCodeProcess, INFINITE, InitializeProcThreadAttributeList,
            LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            PROC_THREAD_ATTRIBUTE_JOB_LIST, PROCESS_INFORMATION, ResumeThread,
            STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute,
            WaitForSingleObject,
        },
    },
};
const OUTPUT_LIMIT: u64 = 1024 * 1024;
struct Handle(HANDLE);
impl Handle {
    fn new(raw: HANDLE) -> Result<Self> {
        ensure!(
            !raw.is_null() && raw != INVALID_HANDLE_VALUE,
            "Windows handle creation failed: {}",
            std::io::Error::last_os_error()
        );
        Ok(Self(raw))
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct Attributes {
    memory: Vec<usize>,
    initialized: bool,
}
impl Attributes {
    fn pointer(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.memory.as_mut_ptr().cast()
    }
    fn new(handles: &mut [HANDLE], jobs: Option<&mut [HANDLE]>) -> Result<Self> {
        let count = if jobs.is_some() { 2 } else { 1 };
        let mut bytes = 0;
        unsafe {
            InitializeProcThreadAttributeList(ptr::null_mut(), count, 0, &mut bytes);
        }
        ensure!(
            bytes > 0 && bytes <= 65536,
            "Unexpected Windows startup attribute size"
        );
        let mut result = Self {
            memory: vec![0; bytes.div_ceil(std::mem::size_of::<usize>())],
            initialized: false,
        };
        checked(
            unsafe { InitializeProcThreadAttributeList(result.pointer(), count, 0, &mut bytes) },
            "Initialize restricted handle inheritance",
        )?;
        result.initialized = true;
        checked(
            unsafe {
                UpdateProcThreadAttribute(
                    result.pointer(),
                    0,
                    PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                    handles.as_ptr().cast(),
                    std::mem::size_of_val(handles),
                    ptr::null_mut(),
                    ptr::null(),
                )
            },
            "Restrict inherited handles",
        )?;
        if let Some(jobs) = jobs {
            checked(
                unsafe {
                    UpdateProcThreadAttribute(
                        result.pointer(),
                        0,
                        PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                        jobs.as_ptr().cast(),
                        std::mem::size_of_val(jobs),
                        ptr::null_mut(),
                        ptr::null(),
                    )
                },
                "Assign checkpoint job during process creation",
            )?;
        }
        Ok(result)
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                DeleteProcThreadAttributeList(self.pointer());
            }
        }
    }
}
fn checked(value: i32, context: &str) -> Result<()> {
    ensure!(value != 0, "{context}: {}", std::io::Error::last_os_error());
    Ok(())
}
fn inheritable(file: &File) -> Result<Handle> {
    let mut raw = ptr::null_mut();
    checked(
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                file.as_raw_handle(),
                GetCurrentProcess(),
                &mut raw,
                0,
                1,
                DUPLICATE_SAME_ACCESS,
            )
        },
        "Duplicate explicit standard handle",
    )?;
    Handle::new(raw)
}
struct ProcessOwner {
    process: Handle,
    _thread: Handle,
    job: Handle,
    assigned: bool,
}
impl ProcessOwner {
    fn settle(&mut self) {
        let started = Instant::now();
        let mut warned = false;
        loop {
            if !warned && started.elapsed() >= std::time::Duration::from_secs(5) {
                crate::logging::warning(
                    "Waiting for external Job Object cleanup; recovery ownership remains held until all owned processes have terminated.",
                );
                warned = true;
            }
            unsafe {
                if self.assigned {
                    TerminateJobObject(self.job.0, 1);
                } else {
                    TerminateProcess(self.process.0, 1);
                }
            }
            if !self.assigned {
                if unsafe { WaitForSingleObject(self.process.0, INFINITE) } == WAIT_OBJECT_0 {
                    return;
                }
            } else {
                let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
                let queried = unsafe {
                    QueryInformationJobObject(
                        self.job.0,
                        JobObjectBasicAccountingInformation,
                        (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                        std::mem::size_of_val(&accounting) as u32,
                        ptr::null_mut(),
                    )
                };
                if queried != 0
                    && accounting.ActiveProcesses == 0
                    && unsafe { WaitForSingleObject(self.process.0, INFINITE) } == WAIT_OBJECT_0
                {
                    return;
                }
            }
            // Keep the job and the engine's recovery lock on uncertain membership
            // or termination failure; cancellation cannot abandon owned writers.
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}
impl Drop for ProcessOwner {
    fn drop(&mut self) {
        self.settle();
    }
}
fn wide(value: &OsStr) -> Result<Vec<u16>> {
    let result: Vec<_> = value.encode_wide().take(32768).collect();
    ensure!(
        result.len() < 32768 && !result.contains(&0),
        "Windows command field exceeds 32767 code units or contains NUL"
    );
    Ok(result)
}
fn nul(value: &OsStr) -> Result<Vec<u16>> {
    let mut result = wide(value)?;
    result.push(0);
    Ok(result)
}
fn quoted(value: &OsStr) -> Result<Vec<u16>> {
    let mut output = vec![b'"' as u16];
    let mut backslashes = 0usize;
    for character in wide(value)? {
        if character == b'\\' as u16 {
            backslashes += 1;
            continue;
        }
        if character == b'"' as u16 {
            output.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2 + 1));
        } else {
            output.extend(std::iter::repeat_n(b'\\' as u16, backslashes));
        }
        backslashes = 0;
        output.push(character);
    }
    output.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2));
    output.push(b'"' as u16);
    Ok(output)
}
fn same_key(a: &OsStr, b: &OsStr) -> Result<bool> {
    let a = wide(a)?;
    let b = wide(b)?;
    let result =
        unsafe { CompareStringOrdinal(a.as_ptr(), a.len() as i32, b.as_ptr(), b.len() as i32, 1) };
    ensure!(result != 0, "Cannot compare Windows environment keys");
    Ok(result == CSTR_EQUAL)
}
fn environment(command: &std::process::Command) -> Result<Vec<u16>> {
    // Invocation::command is the sole constructor: all current variants inherit
    // the environment and Python adds/removes explicit keys; none uses env_clear.
    let mut values = Vec::<(OsString, OsString)>::new();
    let mut inherited_units = 0usize;
    for (key, value) in std::env::vars_os() {
        inherited_units += wide(&key)?.len() + wide(&value)?.len() + 2;
        ensure!(
            values.len() < 4096 && inherited_units <= 32766,
            "Inherited Windows environment exceeds bounds"
        );
        values.push((key, value));
    }
    for (key, value) in command.get_envs() {
        let mut retained = Vec::new();
        for (old, value) in values {
            if !same_key(&old, key)? {
                retained.push((old, value));
            }
        }
        if let Some(value) = value {
            retained.push((key.to_owned(), value.to_owned()));
        }
        values = retained;
    }
    // Windows expects case-insensitive Unicode ordinal ordering.
    let mut encoded = values
        .into_iter()
        .map(|(k, v)| Ok((wide(&k)?, wide(&v)?)))
        .collect::<Result<Vec<_>>>()?;
    let mut comparison_failed = false;
    encoded.sort_by(|(a, _), (b, _)| {
        let order = unsafe {
            CompareStringOrdinal(a.as_ptr(), a.len() as i32, b.as_ptr(), b.len() as i32, 1)
        };
        if order == 0 {
            comparison_failed = true;
        }
        order.cmp(&CSTR_EQUAL)
    });
    ensure!(!comparison_failed, "Cannot order Windows environment keys");
    let mut block = Vec::new();
    for (key, value) in encoded {
        ensure!(!key.is_empty(), "Empty Windows environment key");
        block.extend(key);
        block.push(b'=' as u16);
        block.extend(value);
        block.push(0);
        ensure!(
            block.len() <= 32766,
            "Windows environment block exceeds bounds"
        );
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok(block)
}
pub(super) fn run(options: &Options, cancelled: &AtomicBool) -> Result<Output> {
    ensure!(
        options.arguments.is_empty() || options.legacy_arguments.is_none(),
        "Use either native arguments or --args, not both"
    );
    let arguments: Vec<OsString> = if let Some(text) = &options.legacy_arguments {
        ensure!(
            text.len() <= 64 * 1024 && !text.contains('\0'),
            "Legacy arguments exceed bounds or contain NUL"
        );
        crate::engine::process_arguments::windows_split(text)?
            .into_iter()
            .map(OsString::from)
            .collect()
    } else {
        ensure!(options.arguments.len() <= 256, "Too many backend arguments");
        let mut units = 0usize;
        for argument in &options.arguments {
            units += wide(argument)?.len();
            ensure!(units <= 32768, "Backend arguments exceed 64 KiB");
        }
        options.arguments.clone()
    };
    ensure!(arguments.len() <= 256, "Too many backend arguments");
    let mut argument_bytes = 0usize;
    for value in &arguments {
        argument_bytes += wide(value)?.len() * 2;
        ensure!(argument_bytes <= 65536, "Backend arguments exceed 64 KiB");
    }
    let invocation = super::super::resolve_invocation(
        crate::PatcherEngine::HoloPatcher,
        options.executable.as_deref(),
    )?
    .context("HoloPatcher backend unavailable")?;
    let mut command = invocation.command()?;
    command.args(arguments);
    if let Some(directory) = &options.working_directory {
        command.current_dir(directory);
    }
    super::run_command(command, options.timeout, cancelled)?.into_output()
}
pub(super) fn run_command(
    command: std::process::Command,
    timeout: std::time::Duration,
    cancelled: &AtomicBool,
) -> Result<CommandReceipt> {
    run_command_inner(command, timeout, cancelled, None)
}

pub(crate) fn run_command_with_job(
    command: std::process::Command,
    timeout: std::time::Duration,
    cancelled: &AtomicBool,
    job: HANDLE,
) -> Result<CommandReceipt> {
    run_command_inner(command, timeout, cancelled, Some(job))
}

fn run_command_inner(
    command: std::process::Command,
    timeout: std::time::Duration,
    cancelled: &AtomicBool,
    checkpoint_job: Option<HANDLE>,
) -> Result<CommandReceipt> {
    ensure!(
        timeout >= std::time::Duration::from_secs(1)
            && timeout <= std::time::Duration::from_secs(86400),
        "Execution timeout exceeds supported bounds"
    );
    ensure!(
        command.get_args().count() <= 512,
        "Too many external command arguments"
    );
    let program = nul(command.get_program())?;
    let mut line = quoted(command.get_program())?;
    for argument in command.get_args() {
        line.push(b' ' as u16);
        line.extend(quoted(argument)?);
        ensure!(
            line.len() < 32767,
            "Windows command line exceeds 32767 code units"
        );
    }
    ensure!(
        line.len() < 32767,
        "Windows command line exceeds 32767 code units"
    );
    line.push(0);
    let environment = environment(&command)?;
    let cwd = command
        .get_current_dir()
        .map(|path| -> Result<Vec<u16>> {
            use std::os::windows::fs::MetadataExt;
            let metadata = std::fs::symlink_metadata(path)?;
            ensure!(
                metadata.is_dir() && metadata.file_attributes() & 0x400 == 0,
                "Working directory must be an ordinary directory"
            );
            nul(path.as_os_str())
        })
        .transpose()?;
    let stdout = tempfile::tempfile()?;
    let stderr = tempfile::tempfile()?;
    let input = OpenOptions::new().read(true).open("NUL")?;
    let output_handle = inheritable(&stdout)?;
    let error_handle = inheritable(&stderr)?;
    let input_handle = inheritable(&input)?;
    let mut handles = [output_handle.0, error_handle.0, input_handle.0];
    let job = if let Some(existing) = checkpoint_job {
        let mut duplicate = ptr::null_mut();
        checked(
            unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    existing,
                    GetCurrentProcess(),
                    &mut duplicate,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            },
            "Duplicate checkpoint job ownership",
        )?;
        Handle::new(duplicate)?
    } else {
        let job = Handle::new(unsafe { CreateJobObjectW(ptr::null(), ptr::null()) })?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        checked(
            unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&limits) as u32,
                )
            },
            "Set private job termination policy",
        )?;
        job
    };
    let mut jobs = [job.0];
    let mut attributes = Attributes::new(
        &mut handles,
        if checkpoint_job.is_some() {
            Some(&mut jobs)
        } else {
            None
        },
    )?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = input_handle.0;
    startup.StartupInfo.hStdOutput = output_handle.0;
    startup.StartupInfo.hStdError = error_handle.0;
    startup.lpAttributeList = attributes.pointer();
    let mut process = PROCESS_INFORMATION::default();
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "External program cancelled"
    );
    checked(
        unsafe {
            CreateProcessW(
                program.as_ptr(),
                line.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                1,
                CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
                environment.as_ptr().cast(),
                cwd.as_ref().map_or(ptr::null(), |v| v.as_ptr()),
                &startup.StartupInfo,
                &mut process,
            )
        },
        "Create suspended external program",
    )?;
    // CreateProcessW success guarantees both handles. Establish cleanup before
    // any fallible assignment/resume operation; an unassigned child stays suspended.
    let mut owner = ProcessOwner {
        process: Handle(process.hProcess),
        _thread: Handle(process.hThread),
        job,
        assigned: checkpoint_job.is_some(),
    };
    let outcome = (|| -> Result<i32> {
        if !owner.assigned {
            checked(
                unsafe { AssignProcessToJobObject(owner.job.0, owner.process.0) },
                "Assign suspended backend to private job",
            )?;
            owner.assigned = true;
        }
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "External program cancelled"
        );
        ensure!(
            unsafe { ResumeThread(owner._thread.0) } != u32::MAX,
            "Resume backend failed: {}",
            std::io::Error::last_os_error()
        );
        let started = Instant::now();
        loop {
            ensure!(
                !cancelled.load(Ordering::Acquire),
                "External program cancelled"
            );
            ensure!(started.elapsed() < timeout, "External program timed out");
            ensure!(
                stdout.metadata()?.len() <= OUTPUT_LIMIT
                    && stderr.metadata()?.len() <= OUTPUT_LIMIT,
                "Backend output exceeds one MiB per stream"
            );
            match unsafe { WaitForSingleObject(owner.process.0, 20) } {
                WAIT_OBJECT_0 => break,
                WAIT_TIMEOUT => {}
                _ => anyhow::bail!(
                    "Wait for backend failed: {}",
                    std::io::Error::last_os_error()
                ),
            }
        }
        ensure!(
            started.elapsed() < timeout,
            "External program timed out before exit observation"
        );
        let mut exit = 0;
        checked(
            unsafe { GetExitCodeProcess(owner.process.0, &mut exit) },
            "Read backend exit status",
        )?;
        Ok(exit as i32)
    })();
    drop(owner);
    Ok(super::terminal_receipt(stdout, stderr, outcome, cancelled))
}
