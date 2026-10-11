//! Explicitly trusted standalone HoloPatcher execution; no shell or package installation.
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix_group;
#[cfg(windows)]
pub(crate) mod windows;
use anyhow::Context;
use anyhow::{Result, ensure};
use serde::Serialize;
use std::{
    ffi::OsString,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::{
    process::{Child, Stdio},
    time::Instant,
};

pub struct Options {
    pub executable: Option<PathBuf>,
    pub arguments: Vec<OsString>,
    pub legacy_arguments: Option<String>,
    pub working_directory: Option<PathBuf>,
    pub allow_processes: bool,
    pub timeout: Duration,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            executable: None,
            arguments: Vec::new(),
            legacy_arguments: None,
            working_directory: None,
            allow_processes: false,
            timeout: Duration::from_secs(3600),
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}
pub(crate) struct CommandReceipt {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub failure: Option<String>,
    pub cancelled: bool,
    /// Only the owned process group's settlement proves this; execution success
    /// and cancellation are independent. Does not cover escaped daemon writers.
    pub process_group_quiescent: bool,
    /// Authoritative prelaunch/spawn failure only. Never inferred from generic Err.
    pub no_child_started: bool,
}
impl CommandReceipt {
    fn no_child_started(error: anyhow::Error, cancelled: &AtomicBool) -> Self {
        Self {
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            failure: Some(format!("{error:#}")),
            cancelled: cancelled.load(Ordering::Acquire),
            process_group_quiescent: false,
            no_child_started: true,
        }
    }
    fn into_output(self) -> Result<Output> {
        if let Some(failure) = self.failure {
            anyhow::bail!(failure);
        }
        ensure!(!self.cancelled, "External program cancelled");
        Ok(Output {
            exit_code: self
                .exit_code
                .context("External program returned no exit status")?,
            stdout: self.stdout,
            stderr: self.stderr,
        })
    }
}
/// Internal callers construct commands with inherited environment plus explicit
/// overrides, never env_clear. Post-launch failures retain bounded diagnostics.
pub(crate) fn run_command(
    command: std::process::Command,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<CommandReceipt> {
    let prelaunch = (|| -> Result<()> {
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "External program cancelled"
        );
        ensure!(
            timeout >= Duration::from_secs(1) && timeout <= Duration::from_secs(86400),
            "Execution timeout exceeds supported bounds"
        );
        let mut total = 0usize;
        for (index, argument) in command.get_args().enumerate() {
            total = total
                .checked_add(argument.as_encoded_bytes().len())
                .context("Argument size overflow")?;
            ensure!(
                index < 512 && total <= 128 * 1024,
                "External command arguments exceed bounds"
            );
            ensure!(
                !argument.as_encoded_bytes().contains(&0),
                "External command contains NUL"
            );
        }
        Ok(())
    })();
    if let Err(error) = prelaunch {
        return Ok(CommandReceipt::no_child_started(error, cancelled));
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        run_unix_command(command, timeout, cancelled)
    }
    #[cfg(windows)]
    {
        windows::run_command(command, timeout, cancelled)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = command;
        Ok(CommandReceipt::no_child_started(
            anyhow::anyhow!("Owned process-tree execution unavailable on this platform"),
            cancelled,
        ))
    }
}
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn terminal_receipt(
    stdout: std::fs::File,
    stderr: std::fs::File,
    outcome: Result<i32>,
    cancelled: &AtomicBool,
) -> CommandReceipt {
    use std::io::{Read, Seek, SeekFrom};
    fn read(mut file: std::fs::File) -> (String, Option<String>) {
        let mut bytes = Vec::new();
        let outcome = (|| -> Result<()> {
            file.seek(SeekFrom::Start(0))?;
            file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() <= 1024 * 1024,
                "External program output truncated at one MiB"
            );
            Ok(())
        })();
        bytes.truncate(1024 * 1024);
        (
            String::from_utf8_lossy(&bytes).into_owned(),
            outcome.err().map(|error| format!("{error:#}")),
        )
    }
    let (stdout, out_error) = read(stdout);
    let (stderr, err_error) = read(stderr);
    let (exit_code, mut failure) = match outcome {
        Ok(code) => (Some(code), None),
        Err(error) => (None, Some(format!("{error:#}"))),
    };
    for error in [out_error, err_error].into_iter().flatten() {
        if let Some(message) = &mut failure {
            message.push_str("; ");
            message.push_str(&error);
        } else {
            failure = Some(error);
        }
    }
    CommandReceipt {
        exit_code,
        stdout,
        stderr,
        failure,
        cancelled: cancelled.load(Ordering::Acquire),
        process_group_quiescent: false,
        no_child_started: false,
    }
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
const OUTPUT_LIMIT: u64 = 1024 * 1024;

/// Run only after explicit trust. Unix descendants share an owned process group;
/// Windows launches suspended into an owned kill-on-close Job Object.
pub fn run(options: &Options, cancelled: &AtomicBool) -> Result<Output> {
    ensure!(
        options.allow_processes,
        "Standalone HoloPatcher requires --allow-processes"
    );
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Standalone HoloPatcher cancelled"
    );
    ensure!(
        options.timeout >= Duration::from_secs(1)
            && options.timeout <= Duration::from_secs(24 * 60 * 60),
        "Standalone timeout must be between 1 and 86400 seconds"
    );
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = options;
        anyhow::bail!(
            "Standalone HoloPatcher requires process-tree containment unavailable on this platform; no backend was launched"
        );
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        run_unix(options, cancelled)
    }
    #[cfg(windows)]
    {
        windows::run(options, cancelled)
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn arguments(options: &Options) -> Result<Vec<OsString>> {
    use std::os::unix::ffi::OsStrExt;
    ensure!(
        options.legacy_arguments.is_none() || options.arguments.is_empty(),
        "Use either native arguments or --args, not both"
    );
    let arguments = if let Some(text) = &options.legacy_arguments {
        ensure!(
            text.len() <= 64 * 1024 && !text.contains('\0'),
            "Legacy arguments exceed bounds or contain NUL"
        );
        shlex::split(text)
            .context("Unclosed quote in HoloPatcher arguments")?
            .into_iter()
            .map(OsString::from)
            .collect()
    } else {
        ensure!(
            options.arguments.len() <= 256,
            "HoloPatcher arguments exceed 256 items"
        );
        let mut total = 0usize;
        for argument in &options.arguments {
            total = total
                .checked_add(argument.as_bytes().len())
                .context("Argument size overflow")?;
            ensure!(total <= 64 * 1024, "HoloPatcher arguments exceed 64 KiB");
        }
        options.arguments.clone()
    };
    ensure!(
        arguments.len() <= 256,
        "HoloPatcher arguments exceed 256 items"
    );
    let mut bytes = 0usize;
    for argument in &arguments {
        let raw = argument.as_bytes();
        ensure!(!raw.contains(&0), "HoloPatcher argument contains NUL");
        bytes = bytes
            .checked_add(raw.len())
            .context("Argument size overflow")?;
        ensure!(bytes <= 64 * 1024, "HoloPatcher arguments exceed 64 KiB");
    }
    Ok(arguments)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct OwnedChild(Option<Child>, bool);
#[cfg(any(target_os = "linux", target_os = "macos"))]
impl OwnedChild {
    fn settle(&mut self) -> Result<std::process::ExitStatus> {
        let child = self.0.as_mut().context("Missing child owner")?;
        let group = child.id() as i32;
        let mut lost = self.1;
        let started = Instant::now();
        let mut warned = false;
        loop {
            if !warned && started.elapsed() >= Duration::from_secs(5) {
                crate::logging::warning(
                    "Waiting for external process-group cleanup; recovery ownership remains held until live writers are gone and membership is verifiable.",
                );
                warned = true;
            }
            let mut information: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let observed = unsafe {
                libc::waitid(
                    libc::P_PID,
                    child.id() as libc::id_t,
                    &mut information,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if observed != 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD) {
                    lost = true;
                } else {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            }
            if !lost {
                // The leader is still ours and unreaped, reserving the group ID.
                unsafe {
                    libc::kill(-group, libc::SIGKILL);
                }
                let _ = child.kill();
            }
            // Membership errors retain ownership and retry. Returning would
            // release recovery protection while an unknown writer may live.
            if let Ok(false) = unix_group::live(group) {
                if lost {
                    self.0.take();
                    anyhow::bail!(
                        "External reaper removed backend ownership; no further signals were sent"
                    );
                }
                match child.wait() {
                    Ok(status) => {
                        self.0.take();
                        return Ok(status);
                    }
                    Err(error) if error.raw_os_error() == Some(libc::ECHILD) => {
                        lost = true;
                    }
                    Err(_) => {}
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.0.is_some() {
            let _ = self.settle();
        }
    }
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn run_unix(options: &Options, cancelled: &AtomicBool) -> Result<Output> {
    use std::os::unix::process::CommandExt;
    let arguments = arguments(options)?;
    let invocation = super::resolve_invocation(crate::PatcherEngine::HoloPatcher, options.executable.as_deref())?
        .context("HoloPatcher backend unavailable; install a verified package or configure an existing backend")?;
    let mut command = invocation.command()?;
    command
        .args(arguments)
        .stdin(Stdio::null())
        .process_group(0);
    if let Some(directory) = &options.working_directory {
        let metadata = std::fs::symlink_metadata(directory)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "Standalone working directory must be an ordinary existing directory"
        );
        command.current_dir(directory);
    }
    run_command(command, options.timeout, cancelled)?.into_output()
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn run_unix_command(
    mut command: std::process::Command,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<CommandReceipt> {
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    command.stdin(Stdio::null()).process_group(0);
    let launch = (|| -> Result<_> {
        let stdout = tempfile::tempfile()?;
        let stderr = tempfile::tempfile()?;
        command
            .stdout(stdout.try_clone()?)
            .stderr(stderr.try_clone()?);
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "External program cancelled"
        );
        let child = command.spawn().context("Start external program")?;
        Ok((stdout, stderr, child))
    })();
    let (stdout, stderr, child) = match launch {
        Ok(value) => value,
        Err(error) => return Ok(CommandReceipt::no_child_started(error, cancelled)),
    };
    let mut owner = OwnedChild(Some(child), false);
    let outcome = (|| -> Result<()> {
        let started = Instant::now();
        let child = owner.0.as_mut().context("Missing child owner")?;
        loop {
            ensure!(
                !cancelled.load(Ordering::Acquire),
                "External program cancelled"
            );
            ensure!(started.elapsed() < timeout, "External program timed out");
            // Polled ceiling, not a hard disk quota; final reads are strictly bounded.
            ensure!(
                stdout.metadata()?.len() <= OUTPUT_LIMIT
                    && stderr.metadata()?.len() <= OUTPUT_LIMIT,
                "External program output exceeded one MiB per stream"
            );
            let mut information: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let result = unsafe {
                libc::waitid(
                    libc::P_PID,
                    child.id() as libc::id_t,
                    &mut information,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if result != 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                if error.raw_os_error() == Some(libc::ECHILD) {
                    // An external reaper/changed SIGCHLD policy removed our identity.
                    // Never signal its numeric PID/group after ownership was lost.
                    // Retain the loss across cleanup probes, even if another
                    // independently spawned child later reuses the numeric PID.
                    owner.1 = true;
                }
                return Err(error).context("Observe external program without reaping");
            }
            if unsafe { information.si_pid() } != 0 {
                ensure!(
                    started.elapsed() < timeout,
                    "External program timed out before exit observation"
                );
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    })();
    let settlement = owner.settle().map(|status| {
        status
            .code()
            .unwrap_or_else(|| 128 + status.signal().unwrap_or(1))
    });
    let process_group_quiescent = settlement.is_ok();
    let outcome = match (outcome, settlement) {
        (Ok(()), result) => result,
        (Err(error), Ok(_)) => Err(error),
        (Err(error), Err(cleanup)) => {
            Err(anyhow::anyhow!("{error:#}; cleanup ownership: {cleanup:#}"))
        }
    };
    drop(owner);
    let mut receipt = terminal_receipt(stdout, stderr, outcome, cancelled);
    receipt.process_group_quiescent = process_group_quiescent;
    Ok(receipt)
}
