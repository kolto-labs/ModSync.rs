//! Fetch's FOMOD policy and captured-scope dispatcher. Acquisition and the
//! standalone wizard keep their existing APIs; terminal input belongs to this host.
use crate::{Guide, download_orchestrator as acquisition, fomod};
use anyhow::{Context, Result};
use serde::Serialize;
use std::{
    io::{IsTerminal, Write},
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

pub struct Options {
    pub skip: bool,
    pub choices: Option<PathBuf>,
    pub interactive: bool,
    pub non_interactive: bool,
    pub settings_dir: Option<PathBuf>,
}

enum Mode {
    Skip,
    Choices(fomod::ChoicesFile),
    Interactive,
    Warn,
}

impl Options {
    /// Resolve only after positive FOMOD detection. Explicit skip/choices never
    /// consult mode settings, preserving generic Fetch's credential/settings bypass.
    fn resolve(&self) -> Result<Mode> {
        if self.skip {
            return Ok(Mode::Skip);
        }
        let explicit = self
            .choices
            .as_ref()
            .filter(|p| !p.as_os_str().is_empty() && !p.to_string_lossy().trim().is_empty());
        let environment = std::env::var_os("MODSYNC_FOMOD_CHOICES")
            .map(PathBuf::from)
            .filter(|p| !p.to_string_lossy().trim().is_empty());
        if let Some(path) = explicit.or(environment.as_ref()) {
            return Ok(Mode::Choices(fomod::ChoicesFile::parse(
                &std::fs::read_to_string(path)
                    .with_context(|| format!("Reading FOMOD choices {}", path.display()))?,
            )?));
        }
        let settings =
            crate::profiles::SettingsFile::resolve(self.settings_dir.as_deref())?.load()?;
        let environment = std::env::var("MODSYNC_FOMOD_POST_DOWNLOAD_MODE").ok();
        let mode = settings
            .get("fomodPostDownloadMode")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .or(environment.as_deref())
            .unwrap_or("")
            .trim();
        if mode.eq_ignore_ascii_case("skip") {
            return Ok(Mode::Skip);
        }
        if self.interactive {
            return Ok(Mode::Interactive);
        }
        if mode.eq_ignore_ascii_case("warn-continue") {
            return Ok(Mode::Warn);
        }
        Ok(
            if !self.non_interactive
                && std::io::stdin().is_terminal()
                && std::io::stdout().is_terminal()
            {
                Mode::Interactive
            } else {
                Mode::Warn
            },
        )
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Resolved,
    Configured,
    Skipped,
    Warned,
    Suppressed,
    Refused,
    Eof,
    UserQuit,
    SignalCancelled,
    Error,
    NotPrompted,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumerOutcome {
    pub component_guid: String,
    pub archive: PathBuf,
    pub outcome: Outcome,
    pub usable_choices: bool,
    pub error: Option<String>,
    #[serde(skip)]
    should_prompt: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptedSession {
    pub archive: PathBuf,
    pub component_guids: Vec<String>,
    pub choices: fomod::ArchiveChoices,
}

struct Edit {
    guids: Vec<String>,
    filename: String,
    choices: Option<fomod::ArchiveChoices>,
    status: fomod::PromptStatus,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub consumers: Vec<ConsumerOutcome>,
    pub accepted_sessions: Vec<AcceptedSession>,
    pub errors: Vec<String>,
    pub unresolved: usize,
    pub cancelled: bool,
    #[serde(skip)]
    edits: Vec<Edit>,
}

impl Report {
    /// Replay only committed bound edits onto the pristine authored guide, after
    /// acquisition recording. Preserve authored selection and every unrelated field.
    pub fn apply_recorded(&self, guide: &mut Guide, completed: &[String]) -> Result<()> {
        let mut candidate = guide.clone();
        for edit in &self.edits {
            fomod::apply_bound_archive(
                &mut candidate,
                &edit.guids,
                &edit.filename,
                edit.choices.as_ref(),
                edit.status,
                completed,
            )?;
        }
        *guide = candidate;
        Ok(())
    }
    /// Follow-ups after dispatch: consumers whose top-level `should_prompt` was true at
    /// collection and still lack usable choices. Only the per-archive deduplication is
    /// shared with the desktop, whose eligibility is the recursive `offer_prompt`.
    pub fn followups(&self) -> Vec<acquisition::FomodFollowup> {
        acquisition::pending_fomod_followups(self.consumers.iter().map(|c| {
            (
                c.archive.as_path(),
                c.component_guid.as_str(),
                c.should_prompt,
                c.usable_choices,
            )
        }))
    }
    pub fn has_edits(&self) -> bool {
        !self.edits.is_empty()
    }
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty() || self.consumers.iter().any(|c| c.error.is_some())
    }
}

fn saved_valid(
    component: &crate::Component,
    module: &mut fomod::ArchiveModule,
    filename: &str,
) -> Result<bool> {
    let Some(value) = component.metadata.get("FomodChoices") else {
        return Ok(false);
    };
    let file = fomod::ChoicesFile::parse(&serde_json::to_string(value)?)?;
    let Some(choices) = file.archive(filename) else {
        return Ok(false);
    };
    let old = module.config.steps.clone();
    let checked = choices
        .apply_to(&mut module.config.steps)
        .and_then(|()| module.copy_instructions("fomod-extracted").map(|_| ()));
    module.config.steps = old;
    checked?;
    Ok(true)
}

/// The report/guide pair is the captured acquisition context. No second guide
/// scan may discover consumers. A completed GUID list is available to recovery-aware hosts.
pub fn dispatch(
    guide: &mut Guide,
    mut scan: acquisition::FomodConsumerScan,
    options: &Options,
    cancelled: &AtomicBool,
    completed: &[String],
) -> Report {
    let mut report = Report::default();
    if scan.archives.is_empty() {
        report.cancelled = cancelled.load(Ordering::SeqCst);
        return report;
    }
    // Resolved lazily, once, for the first archive that needs a policy, so malformed
    // settings or a missing choices file cannot fail a run that needs no policy.
    let mut mode: Option<Result<Mode>> = None;
    let mut input: Option<TerminalInput> = None;
    let mut stop_prompting = false;
    for archive in &mut scan.archives {
        let Some(filename) = archive
            .archive
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        let start = report.consumers.len();
        for consumer in &archive.consumers {
            let validity = guide
                .components
                .iter()
                .find(|c| c.guid == consumer.component_guid)
                .context("Captured FOMOD consumer disappeared")
                .and_then(|component| saved_valid(component, &mut archive.module, &filename));
            let (usable_choices, error) = match validity {
                Ok(value) => (value, None),
                Err(error) => (false, Some(format!("{error:#}"))),
            };
            report.consumers.push(ConsumerOutcome {
                component_guid: consumer.component_guid.clone(),
                archive: archive.archive.clone(),
                outcome: if error.is_some() {
                    Outcome::Error
                } else if usable_choices {
                    Outcome::Resolved
                } else if !consumer.should_prompt {
                    Outcome::Suppressed
                } else {
                    Outcome::NotPrompted
                },
                usable_choices,
                error,
                should_prompt: consumer.should_prompt,
            });
        }
        let pending: Vec<_> = report.consumers[start..]
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.usable_choices && c.error.is_none())
            .map(|(i, _)| start + i)
            .collect();
        if pending.is_empty() {
            continue;
        }
        if mode.is_none() && !cancelled.load(Ordering::SeqCst) {
            let resolved = options.resolve();
            if let Err(error) = &resolved {
                report.errors.push(format!("{error:#}"));
            }
            mode = Some(resolved);
        }
        let result = (|| -> Result<(Outcome, Option<Edit>)> {
            if cancelled.load(Ordering::SeqCst) {
                return Ok((Outcome::SignalCancelled, None));
            }
            let Some(Ok(mode)) = &mode else {
                return Ok((Outcome::Error, None));
            };
            let guids: Vec<_> = pending
                .iter()
                .map(|i| report.consumers[*i].component_guid.clone())
                .collect();
            let status;
            let choices;
            let outcome;
            match mode {
                Mode::Skip | Mode::Warn => {
                    status = if matches!(mode, Mode::Skip) {
                        fomod::PromptStatus::Dismissed
                    } else {
                        fomod::PromptStatus::Warned
                    };
                    outcome = if matches!(mode, Mode::Skip) {
                        Outcome::Skipped
                    } else {
                        Outcome::Warned
                    };
                    choices = None;
                }
                Mode::Choices(file) => {
                    let entry = file.archive(&filename).with_context(|| {
                        format!("Choices file has no entry for archive '{filename}'")
                    })?;
                    entry.apply_to(&mut archive.module.config.steps)?;
                    archive.module.copy_instructions("fomod-extracted")?;
                    choices = Some(
                        fomod::ChoicesFile::capture(&filename, &archive.module.config.steps)?
                            .archives
                            .remove(0),
                    );
                    status = fomod::PromptStatus::Configured;
                    outcome = Outcome::Configured;
                }
                Mode::Interactive => {
                    if stop_prompting {
                        return Ok((Outcome::NotPrompted, None));
                    }
                    if !archive
                        .consumers
                        .iter()
                        .any(|c| c.should_prompt && guids.contains(&c.component_guid))
                    {
                        return Ok((Outcome::Suppressed, None));
                    }
                    if input.is_none() {
                        input = Some(TerminalInput::new()?);
                    }
                    let input = input.as_mut().unwrap();
                    // Unlocked stderr locks per write; no lock is held across a read.
                    let mut output = std::io::stderr();
                    write!(output, "Configure FOMOD {filename} now? [y/N] ")?;
                    output.flush()?;
                    let answer = match input.next_line(cancelled)? {
                        fomod::PromptLine::Eof => fomod::PromptOutcome::Eof,
                        fomod::PromptLine::SignalCancelled => fomod::PromptOutcome::SignalCancelled,
                        fomod::PromptLine::Line(line) if line.trim().eq_ignore_ascii_case("q") => {
                            fomod::PromptOutcome::UserQuit
                        }
                        fomod::PromptLine::Line(line)
                            if matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes") =>
                        {
                            fomod::prompt_choices_with(
                                &mut archive.module.config.steps,
                                &mut || input.next_line(cancelled),
                                &mut output,
                            )?
                        }
                        _ => fomod::PromptOutcome::Refused,
                    };
                    if answer != fomod::PromptOutcome::Configured {
                        stop_prompting = matches!(
                            answer,
                            fomod::PromptOutcome::Eof
                                | fomod::PromptOutcome::UserQuit
                                | fomod::PromptOutcome::SignalCancelled
                        );
                        return Ok((
                            match answer {
                                fomod::PromptOutcome::Refused => Outcome::Refused,
                                fomod::PromptOutcome::Eof => Outcome::Eof,
                                fomod::PromptOutcome::UserQuit => Outcome::UserQuit,
                                _ => Outcome::SignalCancelled,
                            },
                            None,
                        ));
                    }
                    archive.module.copy_instructions("fomod-extracted")?;
                    choices = Some(
                        fomod::ChoicesFile::capture(&filename, &archive.module.config.steps)?
                            .archives
                            .remove(0),
                    );
                    status = fomod::PromptStatus::Configured;
                    outcome = Outcome::Configured;
                }
            }
            let mut candidate = guide.clone();
            fomod::apply_bound_archive(
                &mut candidate,
                &guids,
                &filename,
                choices.as_ref(),
                status,
                completed,
            )?;
            // A signal that arrived during validation must not publish this candidate.
            if cancelled.load(Ordering::SeqCst) {
                return Ok((Outcome::SignalCancelled, None));
            }
            if choices.is_none() {
                // Warn/Skip: skip (no warning, no save) only when every pending consumer
                // already registers this archive and its prompt status is unchanged. A
                // consumer without a matching entry keeps the edit: acquisition recording
                // may add the entry, and replaying the edit then persists the status.
                let unchanged = guids.iter().all(|guid| {
                    let before = guide.components.iter().find(|c| c.guid == *guid);
                    let after = candidate.components.iter().find(|c| c.guid == *guid);
                    before.is_some_and(|c| fomod::resource_matches_archive(c, &filename))
                        && before.map(|c| &c.resource_registry)
                            == after.map(|c| &c.resource_registry)
                });
                if unchanged {
                    return Ok((outcome, None));
                }
                eprintln!(
                    "FOMOD {filename} requires choices before installation; use --fomod-choices or --interactive."
                );
            }
            *guide = candidate;
            Ok((
                outcome,
                Some(Edit {
                    guids,
                    filename: filename.clone(),
                    choices,
                    status,
                }),
            ))
        })();
        match result {
            Ok((outcome, edit)) => {
                for index in &pending {
                    report.consumers[*index].outcome = outcome;
                }
                if let Some(edit) = edit {
                    if let Some(choices) = &edit.choices {
                        for index in &pending {
                            report.consumers[*index].usable_choices = true;
                        }
                        report.accepted_sessions.push(AcceptedSession {
                            archive: archive.archive.clone(),
                            component_guids: edit.guids.clone(),
                            choices: choices.clone(),
                        });
                    }
                    report.edits.push(edit);
                }
            }
            Err(error) => {
                for index in &pending {
                    report.consumers[*index].outcome = Outcome::Error;
                    report.consumers[*index].error = Some(format!("{error:#}"));
                }
                // An input failure ends prompting; later archives are still assessed.
                stop_prompting = true;
            }
        }
    }
    if let Some(mut input) = input
        && let Err(error) = input.finish()
    {
        report
            .errors
            .push(format!("Restoring terminal input: {error:#}"));
    }
    report.cancelled = cancelled.load(Ordering::SeqCst);
    report.unresolved = report
        .consumers
        .iter()
        .filter(|c| !c.usable_choices)
        .count();
    report
}

#[cfg(unix)]
struct TerminalInput {
    pending: Vec<u8>,
    eof: bool,
}

/// Unix input never changes file status flags: on a TTY, fds 0/1/2 share one open
/// file description, so O_NONBLOCK would leak to stdout/stderr and to the user's
/// shell if the process is stopped or killed. A bounded poll() observes the signal
/// flag, and each POLLIN readiness is followed by exactly one read(): canonical TTY
/// and pipe readiness guarantees that single read returns without blocking, and
/// regular files never block.
#[cfg(unix)]
impl TerminalInput {
    fn new() -> Result<Self> {
        // Fetch exclusively owns stdin during dispatch; no buffered stdin is used.
        Ok(Self {
            pending: Vec::new(),
            eof: false,
        })
    }
    fn next_line(&mut self, cancelled: &AtomicBool) -> Result<fomod::PromptLine> {
        loop {
            if cancelled.load(Ordering::SeqCst) {
                self.pending.clear();
                return Ok(fomod::PromptLine::SignalCancelled);
            }
            if let Some(index) = self.pending.iter().position(|b| *b == b'\n') {
                let bytes: Vec<_> = self.pending.drain(..=index).collect();
                return Ok(fomod::PromptLine::Line(String::from_utf8(bytes)?));
            }
            if self.eof {
                return if self.pending.is_empty() {
                    Ok(fomod::PromptLine::Eof)
                } else {
                    Ok(fomod::PromptLine::Line(String::from_utf8(std::mem::take(
                        &mut self.pending,
                    ))?))
                };
            }
            let mut descriptor = libc::pollfd {
                fd: libc::STDIN_FILENO,
                events: libc::POLLIN,
                revents: 0,
            };
            let ready = unsafe { libc::poll(&mut descriptor, 1, 50) };
            if ready < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error.into());
            }
            if ready == 0 {
                continue;
            }
            // One read per readiness (POLLIN, or POLLHUP/POLLERR/POLLNVAL, whose read
            // reports EOF or the error). Re-poll before any further read.
            let mut bytes = [0_u8; 4096];
            let count =
                unsafe { libc::read(libc::STDIN_FILENO, bytes.as_mut_ptr().cast(), bytes.len()) };
            if count < 0 {
                let error = std::io::Error::last_os_error();
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) {
                    continue;
                }
                return Err(error.into());
            }
            if count == 0 {
                self.eof = true;
            } else {
                self.pending.extend_from_slice(&bytes[..count as usize]);
            }
        }
    }
    fn finish(&mut self) -> Result<()> {
        // No terminal or descriptor state is changed, so there is nothing to restore.
        Ok(())
    }
}

#[cfg(windows)]
struct TerminalInput;
#[cfg(windows)]
impl TerminalInput {
    fn new() -> Result<Self> {
        Ok(Self)
    }
    fn finish(&mut self) -> Result<()> {
        Ok(())
    }
    fn next_line(&mut self, cancelled: &AtomicBool) -> Result<fomod::PromptLine> {
        use std::{
            os::windows::io::AsRawHandle,
            sync::{Arc, mpsc},
            time::{Duration, Instant},
        };
        use windows_sys::Win32::{
            Foundation::{
                ERROR_BROKEN_PIPE, ERROR_HANDLE_EOF, ERROR_NOT_FOUND, ERROR_OPERATION_ABORTED,
                GetLastError, SetLastError,
            },
            Storage::FileSystem::ReadFile,
            System::{
                Console::{
                    CONSOLE_READCONSOLE_CONTROL, GetConsoleMode, GetStdHandle, ReadConsoleW,
                    STD_INPUT_HANDLE,
                },
                IO::CancelSynchronousIo,
            },
        };
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let (send, receive) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let read = || -> Result<fomod::PromptLine> {
                let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
                let mut mode = 0;
                let console = unsafe { GetConsoleMode(handle, &mut mode) } != 0;
                let mut bytes = Vec::new();
                let mut wide = Vec::new();
                // Match cooked-console EOF without changing the console mode.
                let control = CONSOLE_READCONSOLE_CONTROL {
                    nLength: std::mem::size_of::<CONSOLE_READCONSOLE_CONTROL>() as u32,
                    nInitialChars: 0,
                    dwCtrlWakeupMask: 1 << 0x1a,
                    dwControlKeyState: 0,
                };
                loop {
                    if worker_stop.load(Ordering::SeqCst) {
                        return Ok(fomod::PromptLine::SignalCancelled);
                    }
                    let mut count = 0;
                    let mut byte = 0_u8;
                    let mut unit = 0_u16;
                    let ok = unsafe {
                        SetLastError(0);
                        if console {
                            ReadConsoleW(
                                handle,
                                (&mut unit as *mut u16).cast(),
                                1,
                                &mut count,
                                &control,
                            )
                        } else {
                            ReadFile(handle, &mut byte, 1, &mut count, std::ptr::null_mut())
                        }
                    };
                    let last_error = unsafe { GetLastError() };
                    if worker_stop.load(Ordering::SeqCst) {
                        return Ok(fomod::PromptLine::SignalCancelled);
                    }
                    // ReadConsoleW can succeed with zero characters and an aborted
                    // status on Ctrl-C. Let the supervisor observe the signal and
                    // cancel/join; do not misclassify that race as EOF.
                    if last_error == ERROR_OPERATION_ABORTED && (ok == 0 || count == 0) {
                        continue;
                    }
                    if console && count > 0 && unit == 0x1a {
                        count = 0;
                    }
                    if ok == 0 {
                        let error = std::io::Error::from_raw_os_error(last_error as i32);
                        if !matches!(last_error, ERROR_BROKEN_PIPE | ERROR_HANDLE_EOF) {
                            return Err(error.into());
                        }
                        count = 0;
                    }
                    if count == 0 {
                        if bytes.is_empty() && wide.is_empty() {
                            return Ok(fomod::PromptLine::Eof);
                        }
                        break;
                    }
                    if console {
                        wide.push(unit);
                        if unit == 10 {
                            break;
                        }
                    } else {
                        bytes.push(byte);
                        if byte == b'\n' {
                            break;
                        }
                    }
                }
                Ok(fomod::PromptLine::Line(if console {
                    String::from_utf16(&wide)?
                } else {
                    String::from_utf8(bytes)?
                }))
            };
            let _ = send.send(read());
        });
        let mut cancellation_error: Option<std::io::Error> = None;
        let mut cancelled_at: Option<Instant> = None;
        let mut reported = false;
        let result = loop {
            if cancelled.load(Ordering::SeqCst) {
                stop.store(true, Ordering::SeqCst);
                let since = *cancelled_at.get_or_insert_with(Instant::now);
                // Benign misses (no pending read yet) are retried; a real failure stops
                // retrying and is surfaced while the mandatory wait continues.
                if cancellation_error.is_none()
                    && unsafe { CancelSynchronousIo(worker.as_raw_handle()) } == 0
                {
                    let error = std::io::Error::last_os_error();
                    if !matches!(
                        error.raw_os_error().map(|c| c as u32),
                        Some(ERROR_NOT_FOUND | ERROR_OPERATION_ABORTED)
                    ) {
                        cancellation_error = Some(error);
                    }
                }
                if !reported && since.elapsed() >= Duration::from_secs(2) {
                    reported = true;
                    match &cancellation_error {
                        Some(error) => eprintln!(
                            "FOMOD input reader has not stopped after cancellation ({error}); waiting for it to finish."
                        ),
                        None => eprintln!(
                            "FOMOD input reader has not stopped after cancellation; waiting for it to finish."
                        ),
                    }
                }
            }
            match receive.recv_timeout(Duration::from_millis(25)) {
                Ok(result) => break result,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(error) => break Err(error.into()),
            }
        };
        // Mandatory join: even cancellation errors never detach a blocked input reader.
        worker
            .join()
            .map_err(|_| anyhow::anyhow!("FOMOD input worker panicked"))?;
        if let Some(error) = cancellation_error {
            return Err(error.into());
        }
        if cancelled.load(Ordering::SeqCst) {
            return Ok(fomod::PromptLine::SignalCancelled);
        }
        result
    }
}

#[cfg(not(any(unix, windows)))]
struct TerminalInput;
#[cfg(not(any(unix, windows)))]
impl TerminalInput {
    fn new() -> Result<Self> {
        anyhow::bail!("Cancellable FOMOD input is unavailable on this platform")
    }
    fn next_line(&mut self, _: &AtomicBool) -> Result<fomod::PromptLine> {
        unreachable!()
    }
    fn finish(&mut self) -> Result<()> {
        Ok(())
    }
}
