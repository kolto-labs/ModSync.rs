//! No-shell OS helper execution with bounded output reads and explicit child reap.
use anyhow::{Context, Result, ensure};
use std::{
    ffi::OsStr,
    io::Read,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
const LIMIT: u64 = 256 * 1024;
// Output is redirected to files so descendants cannot hold a read pipe open.
// The cap is polled, not a hard filesystem quota; reads are strictly bounded.
struct ChildGuard(Option<std::process::Child>);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            #[cfg(unix)]
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
pub(super) struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}
pub(super) fn run(program: &OsStr, arguments: &[&OsStr], cancelled: &AtomicBool) -> Result<Output> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Protocol registration cancelled"
    );
    let stdout = tempfile::tempfile()?;
    let stderr = tempfile::tempfile()?;
    let mut command = Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut guard = ChildGuard(Some(
        command
            .spawn()
            .context("Start protocol registration helper")?,
    ));
    let child = guard.0.as_mut().expect("spawned child");
    let started = Instant::now();
    let status = loop {
        let failure = if cancelled.load(Ordering::Acquire) {
            Some("Protocol registration cancelled")
        } else if started.elapsed() > Duration::from_secs(15) {
            Some("Protocol registration helper timed out")
        } else if stdout.metadata()?.len() > LIMIT || stderr.metadata()?.len() > LIMIT {
            Some("Protocol registration helper output exceeded bound")
        } else {
            None
        };
        if let Some(message) = failure {
            anyhow::bail!(message);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => return Err(error.into()),
        }
    };
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    guard.0.take();
    fn read(mut file: std::fs::File) -> Result<String> {
        use std::io::{Seek, SeekFrom};
        ensure!(
            file.metadata()?.len() <= LIMIT,
            "Protocol registration helper output exceeded bound"
        );
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        file.take(LIMIT + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= LIMIT,
            "Protocol registration helper output exceeded bound"
        );
        String::from_utf8(bytes).context("Protocol registration helper output is not UTF-8")
    }
    Ok(Output {
        success: status.success(),
        stdout: read(stdout)?,
        stderr: read(stderr)?,
    })
}
pub(super) fn checked(
    program: &str,
    arguments: &[&OsStr],
    cancelled: &AtomicBool,
) -> Result<String> {
    let output = run(OsStr::new(program), arguments, cancelled)?;
    ensure!(
        output.success,
        "Protocol helper {program} failed: {}",
        output.stderr.trim()
    );
    Ok(output.stdout.trim().into())
}
