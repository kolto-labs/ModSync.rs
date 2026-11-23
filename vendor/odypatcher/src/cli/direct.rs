//! Unattended CLI: `--tslpatchdata`, `--game-dir`, and one action flag.
//!
//! Mirrors HoloPatcher's console mode: the same steps, the same `ExitCode`
//! numbers.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::patcher::config::LogLevel;
use crate::patcher::logger::PatchLogger;
use crate::session::{self, Session};

use super::Options;

/// What the process reports when it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    /// Finished, with nothing to report.
    Success = 0,
    /// Something went wrong before the run could start.
    UnknownStartupError = 1,
    /// The command line did not make sense.
    NumberOfArgs = 2,
    /// No mod was found at the path given.
    NamespacesIniNotFound = 3,
    /// The install variant asked for does not exist.
    NamespaceIndexOutOfRange = 4,
    /// The mod ships no instruction file.
    ChangesIniNotFound = 5,
    /// The run was refused before changing anything.
    AbortInstallUnsafe = 6,
    /// The run stopped partway through.
    ExceptionDuringInstall = 7,
    /// The run finished, but steps failed along the way.
    InstallCompletedWithErrors = 8,
}

impl ExitCode {
    /// The number the process exits with.
    pub fn code(self) -> i32 {
        self as i32
    }
}

/// Report a problem and give the code to exit with.
fn fail(message: &str, code: ExitCode) -> i32 {
    eprintln!("[Error] {message}");
    code.code()
}

/// True when the command line asks for an unattended run.
pub fn is_requested(options: &Options) -> bool {
    options.install
        || options.uninstall
        || options.validate
        || options.tslpatchdata.is_some()
        || options.namespace_option_index.is_some()
}

/// Carry out an unattended run, reporting the process exit code.
pub fn run(options: Options) -> i32 {
    let Some(mod_argument) = options.tslpatchdata.clone() else {
        return fail(
            "No mod path specified. Use --tslpatchdata <path>",
            ExitCode::NumberOfArgs,
        );
    };

    let Some(mod_root) = session::locate_mod(&mod_argument) else {
        return fail(
            &format!(
                "Failed to load mod: No namespaces.ini or changes.ini found in {}",
                Path::new(&mod_argument).join("tslpatchdata").display()
            ),
            ExitCode::NamespacesIniNotFound,
        );
    };

    let mut mod_session = Session::discover(&mod_root.to_string_lossy(), "changes.ini", "info.rtf");

    if let Some(error) = &mod_session.namespace_error {
        return fail(
            &format!("Failed to load mod: {error}"),
            ExitCode::NamespacesIniNotFound,
        );
    }

    // A mod with no installations to choose from still has one to run.
    let choices = mod_session.namespaces.len().max(1);
    if let Some(index) = options.namespace_option_index {
        if index >= choices {
            return fail(
                &format!(
                    "Namespace index {index} out of range (max: {})",
                    choices - 1
                ),
                ExitCode::NamespaceIndexOutOfRange,
            );
        }

        if !mod_session.namespaces.is_empty() {
            mod_session.select_namespace(index);
        }
    }

    if !mod_session.has_instructions() {
        return fail(
            &format!(
                "Failed to load mod: {} not found in {}",
                mod_session.ini_name(),
                mod_session.tslpatchdata_path.display()
            ),
            ExitCode::ChangesIniNotFound,
        );
    }

    let chosen = usize::from(options.install)
        + usize::from(options.uninstall)
        + usize::from(options.validate);

    if chosen > 1 {
        return fail(
            "Cannot run more than one of [--install, --uninstall, --validate]",
            ExitCode::NumberOfArgs,
        );
    }
    if chosen == 0 {
        return fail(
            "Must specify one of [--install, --uninstall, --validate] for CLI mode",
            ExitCode::NumberOfArgs,
        );
    }

    let game_argument = options
        .game_dir
        .clone()
        .or_else(|| options.game_folder.clone())
        .map(PathBuf::from);

    if options.validate {
        let game_folder = game_argument.filter(|p| p.is_dir());
        return validate(&mod_session, game_folder.as_deref());
    }

    let Some(game_folder) = game_argument else {
        return fail(
            "No game directory specified. Use --game-dir <path>",
            ExitCode::NumberOfArgs,
        );
    };

    if !game_folder.is_dir() {
        return fail(
            &format!("Invalid game directory: {}", game_folder.display()),
            ExitCode::NumberOfArgs,
        );
    }

    if options.install {
        install(&mod_session, &mod_root, &game_folder, &options)
    } else {
        uninstall(&mod_session, &game_folder)
    }
}

/// A logger that prints every line it is shown at `level`.
pub fn printing_logger(level: LogLevel) -> PatchLogger {
    let mut logger = PatchLogger::new();
    logger.add_observer(Box::new(move |log| {
        if !session::shows(level, log.log_type) {
            return;
        }
        if log.log_type == crate::patcher::logger::LogType::Error {
            eprintln!("{}", log.formatted_message());
        } else {
            println!("{}", log.formatted_message());
        }
    }));
    logger
}

/// Apply the mod to the game.
fn install(session: &Session, mod_root: &Path, game_folder: &Path, options: &Options) -> i32 {
    if let Some(reason) = session.blocked_reason() {
        return fail(&reason, ExitCode::AbortInstallUnsafe);
    }

    println!(
        "[Info] Installing mod from {} to {}",
        mod_root.display(),
        game_folder.display()
    );

    let level = options
        .log_level
        .and_then(|l| LogLevel::from_int(i64::from(l)).ok())
        .unwrap_or_else(|| session.log_level());

    let started = Instant::now();
    let outcome = session.run_install(game_folder, printing_logger(level), None, None);
    let elapsed = started.elapsed();

    if let Some(path) = &outcome.log_path {
        println!("[Info] Install log written to {}", path.display());
    }

    if let Err(err) = &outcome.result {
        return fail(&err.simplified(), ExitCode::ExceptionDuringInstall);
    }

    let report = outcome.report;
    println!(
        "[Info] Install completed: {} errors, {} warnings, {} patches ({} completed, {} skipped, {} failed)",
        report.errors,
        report.warnings,
        report.patches_configured,
        report.patches_completed,
        report.patches_skipped,
        report.patches_failed
    );
    println!("[Info] Install time: {}", format_install_time(elapsed));

    if report.completed_with_errors() {
        return ExitCode::InstallCompletedWithErrors.code();
    }

    ExitCode::Success.code()
}

/// Put the files this mod backed up back into the game.
fn uninstall(session: &Session, game_folder: &Path) -> i32 {
    println!("[Info] Uninstalling mod from {}", game_folder.display());

    let backup = session.base_path.join("backup");
    if !backup.is_dir() {
        return fail(
            &format!("Backup folder not found: {}", backup.display()),
            ExitCode::ExceptionDuringInstall,
        );
    }

    let report = crate::tools::restore_backup(&session.base_path, game_folder);

    for log in &report.logs {
        println!("{}", log.formatted_message());
    }
    for (title, message) in &report.messages {
        println!("[{title}] {message}");
    }

    if report.restored {
        println!("[Info] Uninstall completed successfully");
        ExitCode::Success.code()
    } else {
        fail(
            "Uninstall did not restore a backup",
            ExitCode::ExceptionDuringInstall,
        )
    }
}

/// Check the mod's instructions without changing anything.
fn validate(session: &Session, game_folder: Option<&Path>) -> i32 {
    println!("[Info] Validating mod configuration");

    let findings = crate::tools::validate(session, game_folder);
    let mut blocking = 0usize;

    for finding in &findings {
        if finding.blocking {
            blocking += 1;
            eprintln!("[Error] {}", finding.message);
        } else {
            println!("[Warning] {}", finding.message);
        }
    }

    if blocking > 0 {
        eprintln!("[Error] Validation found {blocking} problems that would stop a run");
        return ExitCode::ExceptionDuringInstall.code();
    }

    println!("[Info] Validation completed successfully");
    ExitCode::Success.code()
}

/// How long the run took, in words.
pub fn format_install_time(elapsed: Duration) -> String {
    let total = elapsed.as_secs();

    let days = total / 86_400;
    let hours = (total % 86_400) / 3_600;
    let minutes = (total % 3_600) / 60;
    let seconds = total % 60;

    let mut out = String::new();

    if days > 0 {
        out.push_str(&format!("{days} days, "));
    }
    if hours > 0 {
        out.push_str(&format!("{hours} hours, "));
    }
    if minutes > 0 || (days == 0 && hours == 0) {
        out.push_str(&format!("{minutes} minutes, "));
    }
    out.push_str(&format!("{seconds} seconds"));

    out
}
