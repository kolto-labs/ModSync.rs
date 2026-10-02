//! Persistent application log, recent-message buffer and diagnostics.
//!
//! Mirrors the C# `ModSync.Core.Logger` + `NLog.config` pair: a dated file per
//! day under a `Logs/` directory, a bounded in-memory history for error context,
//! level prefixes (`[Warning] `, `[Error] `, `[Verbose] `), verbose lines gated by
//! a debug-logging switch, and exception details forwarded to telemetry as
//! hashes. Differences from C# are listed in `docs/parity/telemetry-csharp-audit.md`.
//!
//! Nothing here writes to stdout. The CLI owns stdout for machine-readable
//! output, so console mirroring goes to stderr and is opt-in.

use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// File name prefix. The dated file is `kotormodsync_YYYY-MM-DD.log`, the
/// name the C# NLog file target writes (`NLog.config:12`). C#'s unused
/// `Logger.LogFileName = "modsync_"` constant does not control that target.
pub const LOG_FILE_PREFIX: &str = "kotormodsync_";
/// Directory created under the settings directory.
pub const LOG_DIRECTORY_NAME: &str = "Logs";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Verbose,
    Info,
    Warning,
    Error,
}

impl Level {
    fn prefix(self) -> &'static str {
        match self {
            Level::Verbose => "[Verbose] ",
            Level::Info => "",
            Level::Warning => "[Warning] ",
            Level::Error => "[Error] ",
        }
    }
}

/// Typed logger history is bounded separately from the legacy string API.
pub const RECENT_RECORD_LIMIT: usize = 4096;
const RECORD_BYTES_LIMIT: usize = 1024 * 1024;
const MESSAGE_BYTES_LIMIT: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub struct Record {
    pub id: u64,
    pub timestamp: chrono::DateTime<chrono::FixedOffset>,
    pub level: Level,
    /// Original producer message, without a synthesized severity prefix.
    pub message: String,
    pub truncated: bool,
}

#[derive(Clone, Debug)]
pub struct Options {
    /// Write `Verbose` lines to the file, memory buffer and mirror.
    pub debug_logging: bool,
    /// Mirror lines to stderr (never stdout).
    pub mirror_stderr: bool,
    /// Keep at most this many dated log files (NLog `maxArchiveFiles="30"`).
    pub max_files: usize,
    /// Recent lines kept in memory (NLog memory target `maxLogsCount="200"`).
    pub memory_lines: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            debug_logging: false,
            mirror_stderr: false,
            max_files: 30,
            memory_lines: 200,
        }
    }
}

struct Sink {
    directory: Option<PathBuf>,
    file: Option<(String, File)>,
    options: Options,
    recent: VecDeque<String>,
    records: VecDeque<Record>,
    next_record: u64,
}

static STATE: OnceLock<Mutex<Sink>> = OnceLock::new();

fn state() -> MutexGuard<'static, Sink> {
    STATE
        .get_or_init(|| {
            Mutex::new(Sink {
                directory: None,
                file: None,
                options: Options::default(),
                recent: VecDeque::new(),
                records: VecDeque::new(),
                next_record: 0,
            })
        })
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `<settings directory>/Logs`.
pub fn log_directory_for(settings_directory: &Path) -> PathBuf {
    settings_directory.join(LOG_DIRECTORY_NAME)
}

/// Start writing the persistent log. Safe to call again to move the directory
/// or change options; the in-memory history is kept. Returns today's file.
pub fn init(settings_directory: &Path, options: Options) -> Result<PathBuf> {
    let directory = log_directory_for(settings_directory);
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("Create log directory {}", directory.display()))?;
    let path = {
        let mut sink = state();
        sink.directory = Some(directory.clone());
        sink.file = None;
        sink.options = options;
        trim_recent(&mut sink);
        let date = today();
        open_file(&mut sink, &date)?;
        prune(&directory, sink.options.max_files);
        directory.join(file_name(&date))
    };
    info(format!(
        "Logging initialized at {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    Ok(path)
}

/// Change the debug-logging switch without reopening the file.
pub fn set_debug_logging(enabled: bool) {
    state().options.debug_logging = enabled;
}

/// Turn the stderr mirror on or off, e.g. off for `--output json` runs.
pub fn set_mirror_stderr(enabled: bool) {
    state().options.mirror_stderr = enabled;
}

/// The file the next line will be written to, if logging was initialized.
pub fn current_log_file() -> Option<PathBuf> {
    let sink = state();
    sink.directory
        .as_ref()
        .map(|directory| directory.join(file_name(&today())))
}

/// The `Logs` directory, if logging was initialized.
pub fn log_directory() -> Option<PathBuf> {
    state().directory.clone()
}

pub fn log(level: Level, message: impl AsRef<str>) {
    let raw_message = message.as_ref();
    let message = apply_prefix(raw_message, level);
    let mut sink = state();
    if level == Level::Verbose && !sink.options.debug_logging {
        return;
    }
    let now = chrono::Local::now();
    push_record(&mut sink, level, now.fixed_offset(), raw_message);
    sink.recent
        .push_back(format!("{} {message}", now.format("%H:%M:%S")));
    trim_recent(&mut sink);
    if sink.options.mirror_stderr {
        let _ = writeln!(
            std::io::stderr(),
            "[{}] {message}",
            now.format("%Y-%m-%d %H:%M:%S")
        );
    }
    if sink.directory.is_some() {
        let date = now.format("%Y-%m-%d").to_string();
        if sink.file.as_ref().is_none_or(|(open, _)| *open != date) {
            let _ = open_file(&mut sink, &date);
            if let Some(directory) = sink.directory.clone() {
                prune(&directory, sink.options.max_files);
            }
        }
        if let Some((_, file)) = sink.file.as_mut() {
            // NLog `${longdate}`: four fractional digits. chrono has no `%.4f`.
            let _ = writeln!(
                file,
                "{}.{:04} {message}",
                now.format("%Y-%m-%d %H:%M:%S"),
                now.timestamp_subsec_micros() / 100
            );
        }
    }
}

pub fn verbose(message: impl AsRef<str>) {
    log(Level::Verbose, message);
}
pub fn info(message: impl AsRef<str>) {
    log(Level::Info, message);
}
pub fn warning(message: impl AsRef<str>) {
    log(Level::Warning, message);
}
pub fn error(message: impl AsRef<str>) {
    log(Level::Error, message);
}

/// Log an error with its full cause chain, then forward a hashed record to
/// telemetry (C# `Logger.LogExceptionDetailsAsync` -> `RecordError`). The raw
/// text stays local; telemetry only sees SHA-256 prefixes.
pub fn log_error(error_value: &anyhow::Error, context: &str) {
    let summary = if context.trim().is_empty() {
        error_value.to_string()
    } else {
        context.to_owned()
    };
    error(&summary);
    error(format!("Exception: {error_value:#}"));
    let chain: Vec<String> = error_value.chain().map(ToString::to_string).collect();
    crate::telemetry::global().record_error("anyhow::Error", &summary, Some(&chain.join("\n")));
}

/// Most recent lines, oldest first (C# `Logger.GetRecentLogMessages`).
pub fn recent(count: usize) -> Vec<String> {
    let sink = state();
    let skip = sink.recent.len().saturating_sub(count);
    sink.recent.iter().skip(skip).cloned().collect()
}

/// C# `Logger.ClearLogHistory`.
pub fn clear_recent() {
    let mut sink = state();
    sink.recent.clear();
    sink.records.clear();
}

/// Newest bounded typed records, oldest first. No severity or timestamp parsing.
/// Retention is at most min(memory_lines, 4096) records and 1 MiB of message
/// bytes; individual messages are UTF-8 safely truncated at 64 KiB and marked.
pub fn recent_records(count: usize) -> Vec<Record> {
    let sink = state();
    sink.records
        .iter()
        .skip(sink.records.len().saturating_sub(count))
        .cloned()
        .collect()
}

/// Nonblocking UI snapshot. None means busy, not an empty history.
pub fn try_recent_records(count: usize) -> Option<Vec<Record>> {
    let Some(mutex) = STATE.get() else {
        return Some(Vec::new());
    };
    let sink = match mutex.try_lock() {
        Ok(sink) => sink,
        Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => return None,
    };
    Some(
        sink.records
            .iter()
            .skip(sink.records.len().saturating_sub(count))
            .cloned()
            .collect(),
    )
}

/// Nonblocking clear of both recent histories. False means retry later.
pub fn try_clear_recent() -> bool {
    let Some(mutex) = STATE.get() else {
        return true;
    };
    let mut sink = match mutex.try_lock() {
        Ok(sink) => sink,
        Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => return false,
    };
    sink.recent.clear();
    sink.records.clear();
    true
}

/// Log panics before the previous hook runs (C# `UnhandledException` hook).
/// Chains whatever hook was installed before, so default stderr output stays.
pub fn install_panic_hook() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let location = info
                .location()
                .map(|location| format!("{}:{}", location.file(), location.line()))
                .unwrap_or_else(|| "unknown location".into());
            let payload = info
                .payload()
                .downcast_ref::<&str>()
                .map(|text| (*text).to_owned())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "non-text panic payload".into());
            // Panic hooks run before the panicking thread releases its guards.
            // Never reacquire a sink/exporter mutex or wait for a flush here.
            if let Some(mutex) = STATE.get()
                && let Ok(mut sink) = mutex.try_lock()
            {
                let raw = format!("Unhandled panic at {location}: {payload}");
                push_record(
                    &mut sink,
                    Level::Error,
                    chrono::Local::now().fixed_offset(),
                    &raw,
                );
                let message = format!("[Error] {raw}");
                sink.recent.push_back(message.clone());
                trim_recent(&mut sink);
                if let Some((_, file)) = sink.file.as_mut() {
                    let _ = writeln!(file, "{message}");
                }
            }
            crate::telemetry::global().try_record_panic(&payload, &location);
            previous(info);
        }));
    });
}

/// Plain-text snapshot for a "copy diagnostics" action or a bug report. It
/// names whether a signing secret exists but never includes it.
pub fn diagnostics_report(
    settings_directory: Option<&Path>,
    telemetry: Option<&crate::telemetry::Config>,
    recent_lines: usize,
) -> String {
    let mut report = String::new();
    let mut line = |text: String| {
        report.push_str(&text);
        report.push('\n');
    };
    line(format!("ModSync.rs {}", env!("CARGO_PKG_VERSION")));
    line(format!(
        "Platform: {} {} ({})",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::consts::FAMILY
    ));
    line(format!(
        "Time: {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S %:z")
    ));
    if let Some(directory) = settings_directory {
        line(format!("Settings directory: {}", directory.display()));
    }
    line(format!(
        "Log file: {}",
        current_log_file()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "not initialized".into())
    ));
    line(format!(
        "Debug logging: {}",
        if state().options.debug_logging {
            "on"
        } else {
            "off"
        }
    ));
    if let Some(config) = telemetry {
        line(format!(
            "Telemetry: consented={} enabled={} active={} signing_secret_present={} endpoint={}",
            config.user_consented,
            config.enabled,
            config.active(),
            config
                .signing_secret
                .as_deref()
                .is_some_and(|s| !s.is_empty()),
            config.endpoint_display()
        ));
        line(format!(
            "Telemetry exporter running: {} (records dropped: {})",
            crate::telemetry::global().is_running(),
            crate::telemetry::global().dropped_records()
        ));
    }
    let lines = recent(recent_lines);
    line(format!("Recent log lines ({}):", lines.len()));
    for entry in lines {
        line(format!("  {entry}"));
    }
    report
}

fn apply_prefix(message: &str, level: Level) -> String {
    let prefix = level.prefix();
    if prefix.is_empty() || message.starts_with(prefix) {
        message.to_owned()
    } else {
        format!("{prefix}{message}")
    }
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn file_name(date: &str) -> String {
    format!("{LOG_FILE_PREFIX}{date}.log")
}

fn open_file(sink: &mut Sink, date: &str) -> Result<()> {
    let Some(directory) = sink.directory.clone() else {
        return Ok(());
    };
    let path = directory.join(file_name(date));
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("Open log file {}", path.display()))?;
    sink.file = Some((date.to_owned(), file));
    Ok(())
}

fn push_record(
    sink: &mut Sink,
    level: Level,
    timestamp: chrono::DateTime<chrono::FixedOffset>,
    message: &str,
) {
    if sink.next_record == u64::MAX {
        sink.records.clear();
        sink.next_record = 0;
    }
    sink.next_record += 1;
    let mut end = message.len().min(MESSAGE_BYTES_LIMIT);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    sink.records.push_back(Record {
        id: sink.next_record,
        timestamp,
        level,
        message: message[..end].to_owned(),
        truncated: end < message.len(),
    });
}

fn trim_recent(sink: &mut Sink) {
    let limit = sink.options.memory_lines.max(1);
    let mut bytes: usize = sink.records.iter().map(|record| record.message.len()).sum();
    while sink.records.len() > limit.min(RECENT_RECORD_LIMIT) || bytes > RECORD_BYTES_LIMIT {
        if let Some(record) = sink.records.pop_front() {
            bytes -= record.message.len();
        }
    }
    while sink.recent.len() > limit {
        sink.recent.pop_front();
    }
}

/// Keep the newest `max_files` dated logs. Names sort by date, so the
/// lexicographic order is the age order.
fn prune(directory: &Path, max_files: usize) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut logs: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(LOG_FILE_PREFIX) && name.ends_with(".log"))
        })
        .collect();
    if logs.len() <= max_files.max(1) {
        return;
    }
    logs.sort();
    let excess = logs.len() - max_files.max(1);
    for path in logs.into_iter().take(excess) {
        let _ = std::fs::remove_file(path);
    }
}
