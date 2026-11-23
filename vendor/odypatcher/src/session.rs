//! Session setup shared by GUI and CLI: find the mod, its install variants
//! (`namespaces.ini`), and the configuration the chosen variant declares.
//!
//! This is the front-end glue HoloPatcher keeps in its `App` class: the
//! library does the reading ([`NamespaceReader`], [`PatcherConfig`]) and the
//! installing ([`ModInstaller`]); this module only decides which files to hand
//! it and where to write the install log afterwards.

use std::path::{Path, PathBuf};

use crate::patcher::casepath::{normcase_realpath, safe_isdir, safe_isfile};
use crate::patcher::config::{Game, LogLevel, PatcherConfig};
use crate::patcher::encoding::decode_ini_text;
use crate::patcher::error::PatchException;
use crate::patcher::installer::ModInstaller;
use crate::patcher::logger::{LogType, PatchLog, PatchLogger};
use crate::patcher::namespaces::PatcherNamespace;
use crate::patcher::reader::NamespaceReader;

/// The folder a mod keeps its files and instructions in.
pub const DATA_FOLDER: &str = "tslpatchdata";
/// The optional file listing the install variants a mod offers.
pub const NAMESPACES_FILE: &str = "namespaces.ini";

/// A mod as found on disk, with the instructions it will run.
#[derive(Debug, Clone)]
pub struct Session {
    /// The mod folder, holding `tslpatchdata` and `backup`.
    pub base_path: PathBuf,
    /// `base_path/tslpatchdata`.
    pub tslpatchdata_path: PathBuf,
    /// The install variants `namespaces.ini` offers (empty without one).
    pub namespaces: Vec<PatcherNamespace>,
    /// Which of [`Self::namespaces`] is selected (meaningless when empty).
    pub selected: usize,
    /// The variant in use: a chosen entry, or the lone `changes.ini`.
    pub namespace: PatcherNamespace,
    /// Why `namespaces.ini` could not be read, when it exists but is broken.
    pub namespace_error: Option<String>,
    /// The loaded instruction file, when it could be read.
    pub config: Option<PatcherConfig>,
    /// Why the instruction file could not be read.
    pub config_error: Option<String>,
    /// What the reader logged while loading the instruction file.
    pub load_logs: Vec<PatchLog>,
}

impl Session {
    /// Find the mod at `base_path` and read what it declares.
    ///
    /// `ini_name` / `info_name` only apply when the mod ships no
    /// `namespaces.ini`; a variant list names its own files.
    pub fn discover(base_path: &str, ini_name: &str, info_name: &str) -> Self {
        let base_path = PathBuf::from(base_path);
        let tslpatchdata_path = base_path.join(DATA_FOLDER);

        let mut namespaces = Vec::new();
        let mut namespace_error = None;

        let namespaces_file = tslpatchdata_path.join(NAMESPACES_FILE);
        if safe_isfile(&namespaces_file) {
            match NamespaceReader::from_filepath(&namespaces_file) {
                Ok(found) if !found.is_empty() => namespaces = found,
                Ok(_) => {
                    namespace_error =
                        Some(format!("{NAMESPACES_FILE} does not list any namespaces."))
                }
                Err(err) => namespace_error = Some(err.simplified()),
            }
        }

        let lone_namespace = namespaces.is_empty();
        let namespace = match namespaces.first() {
            Some(first) => first.clone(),
            None => {
                let mut lone = PatcherNamespace::new(ini_name, info_name);
                lone.name = base_path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                lone
            }
        };

        let mut session = Self {
            base_path,
            tslpatchdata_path,
            namespaces,
            selected: 0,
            namespace,
            namespace_error,
            config: None,
            config_error: None,
            load_logs: Vec::new(),
        };

        // A lone `changes.ini` names itself through `as_namespace`, but only
        // once the file is known to sit inside the mod folder.
        if lone_namespace && session.has_instructions() {
            if let Ok(found) = PatcherConfig::as_namespace(&session.ini_path()) {
                session.namespace.name = found.name;
            }
        }

        session.reload();
        session
    }

    /// Switch to one of the mod's install variants and reload.
    pub fn select_namespace(&mut self, index: usize) {
        let Some(chosen) = self.namespaces.get(index) else {
            return;
        };
        self.selected = index;
        self.namespace = chosen.clone();
        self.reload();
    }

    /// Read the instruction file again.
    ///
    /// The reader's log lines are kept in [`Self::load_logs`]; a failure that
    /// stops the read ends up in [`Self::config_error`].
    pub fn reload(&mut self) {
        self.config = None;
        self.config_error = None;
        self.load_logs.clear();

        if !self.has_instructions() {
            return;
        }

        let ini_path = self.ini_path();
        let mut logger = PatchLogger::new();

        let outcome = std::fs::read(&ini_path)
            .map_err(|err| PatchException::from_io(&err, Some(&ini_path.to_string_lossy())))
            .and_then(|bytes| {
                let (text, forced) = decode_ini_text(&bytes);
                if forced {
                    logger.add_warning(format!(
                        "Could not determine encoding of '{}'. Attempting to force load...",
                        self.namespace.ini_filename
                    ));
                }
                let mut config = PatcherConfig::new();
                let patch_data_path = ini_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| self.tslpatchdata_path.clone());
                config.load(
                    &text,
                    &patch_data_path,
                    &mut logger,
                    Some(&self.tslpatchdata_path),
                )?;
                Ok(config)
            });

        self.load_logs = logger.all_logs;
        match outcome {
            Ok(config) => self.config = Some(config),
            Err(err) => self.config_error = Some(err.simplified()),
        }
    }

    /// Full path of the instruction file.
    pub fn ini_path(&self) -> PathBuf {
        self.tslpatchdata_path
            .join(self.namespace.changes_filepath())
    }

    /// Full path of the description file.
    pub fn info_path(&self) -> PathBuf {
        self.tslpatchdata_path.join(self.namespace.rtf_filepath())
    }

    /// Name of the instruction file, as the variant spells it.
    pub fn ini_name(&self) -> &str {
        &self.namespace.ini_filename
    }

    /// Name of the description file, as the variant spells it.
    pub fn info_name(&self) -> &str {
        &self.namespace.info_filename
    }

    /// True when the instruction file is there and inside the mod folder.
    pub fn has_instructions(&self) -> bool {
        let path = self.ini_path();
        safe_isfile(&path) && is_confined(&self.tslpatchdata_path, &path)
    }

    /// True when the description file is there and inside the mod folder.
    pub fn has_info(&self) -> bool {
        let path = self.info_path();
        safe_isfile(&path) && is_confined(&self.tslpatchdata_path, &path)
    }

    /// The mod's description, or placeholder text when it ships none.
    pub fn info_text(&self) -> String {
        if !self.has_info() {
            return "No description was provided for this mod.".to_string();
        }
        match std::fs::read(self.info_path()) {
            Ok(bytes) => crate::rtf::to_plain_text(&bytes),
            Err(_) => "No description was provided for this mod.".to_string(),
        }
    }

    /// What to call the window: `WindowCaption`, else the variant's name,
    /// else the program.
    pub fn window_caption(&self) -> String {
        if let Some(config) = &self.config {
            let title = config.window_title.trim();
            if !title.is_empty() {
                return title.to_string();
            }
        }
        let name = self.namespace.name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
        crate::APP_NAME.to_string()
    }

    /// `ConfirmMessage`, trimmed.
    pub fn confirm_message(&self) -> String {
        self.config
            .as_ref()
            .map(|c| c.confirm_message.trim().to_string())
            .unwrap_or_default()
    }

    /// True when the mod asked not to be confirmed (no message, or `N/A`),
    /// as HoloPatcher reads it.
    pub fn starts_without_asking(&self) -> bool {
        let message = self.confirm_message();
        message.is_empty() || message == "N/A"
    }

    /// A readable account of what the run would change.
    pub fn summary_text(&self) -> String {
        match &self.config {
            Some(config) => crate::summary::to_text(config, self.ini_name(), self.info_name()),
            None => match &self.config_error {
                Some(error) => format!("The instruction file could not be read:\n{error}\n"),
                None => String::from("No instruction file was loaded.\n"),
            },
        }
    }

    /// Why a run may not start, or `None` when it may.
    pub fn blocked_reason(&self) -> Option<String> {
        if let Some(error) = &self.namespace_error {
            return Some(format!("Cannot read {NAMESPACES_FILE}: {error}"));
        }
        if !self.has_instructions() {
            return Some(format!(
                "Cannot find the instruction file \"{}\" in {}.",
                self.ini_name(),
                self.tslpatchdata_path.display()
            ));
        }
        self.config_error.as_ref().map(|error| {
            format!(
                "Cannot read the instruction file \"{}\": {error}",
                self.ini_name()
            )
        })
    }

    /// What to call the button that starts the run.
    pub fn start_label(&self) -> &'static str {
        "Install Mod"
    }

    /// The `LogLevel` the mod asked for; `Warnings` when it said nothing.
    pub fn log_level(&self) -> LogLevel {
        self.config
            .as_ref()
            .and_then(|c| c.log_level)
            .unwrap_or(LogLevel::Warnings)
    }

    /// The game folder the registry holds for `LookupGameNumber`, if the mod
    /// gave one and the registry answers.
    pub fn looked_up_game_folder(&self) -> Option<String> {
        let number = self.config.as_ref().and_then(|c| c.game_number)?;
        let game = if number == 1 { Game::K1 } else { Game::K2 };
        crate::gamepaths::registry_game_path(game)
    }

    /// Write `installlog.txt` and `installlog.rtf` beside the mod folder.
    ///
    /// Returns the styled log's path, or the plain one when only that could
    /// be written, or `None` when neither could.
    pub fn write_install_log(&self, logs: &[PatchLog]) -> Option<PathBuf> {
        let plain_path = self.base_path.join("installlog.txt");
        let plain_written = std::fs::write(&plain_path, plain_log(logs)).is_ok();

        let rtf_path = self.base_path.join("installlog.rtf");
        let rtf_written = std::fs::write(&rtf_path, styled_log(logs)).is_ok();

        if rtf_written {
            Some(rtf_path)
        } else if plain_written {
            Some(plain_path)
        } else {
            None
        }
    }

    /// Install the selected variant into `game_path`.
    ///
    /// `logger` is handed to the [`ModInstaller`]; its observers keep feeding
    /// the terminal or window while the run goes. Afterwards the log is
    /// written beside the mod and everything comes back in a [`RunOutcome`].
    pub fn run_install(
        &self,
        game_path: &Path,
        logger: PatchLogger,
        should_cancel: Option<&dyn Fn() -> bool>,
        progress: Option<&mut dyn FnMut()>,
    ) -> RunOutcome {
        let mut installer =
            match ModInstaller::new(&self.base_path, game_path, &self.ini_path(), Some(logger)) {
                Ok(installer) => installer,
                Err(err) => {
                    return RunOutcome {
                        result: Err(err),
                        logs: Vec::new(),
                        report: RunReport::default(),
                        log_path: None,
                    }
                }
            };

        let result = installer.install(should_cancel, progress);
        let report = RunReport::from_logger(&installer.log);
        let logs = installer.log.all_logs.clone();
        let log_path = self.write_install_log(&logs);

        RunOutcome {
            result,
            logs,
            report,
            log_path,
        }
    }
}

/// Counts taken from a [`PatchLogger`] once a run has ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunReport {
    /// Lines logged as errors.
    pub errors: usize,
    /// Lines logged as warnings.
    pub warnings: usize,
    /// Operations the run planned.
    pub patches_configured: usize,
    /// Operations that finished.
    pub patches_completed: usize,
    /// Operations that were skipped.
    pub patches_skipped: usize,
    /// Operations that failed.
    pub patches_failed: usize,
}

impl RunReport {
    /// Read the counts off a logger.
    pub fn from_logger(logger: &PatchLogger) -> Self {
        Self {
            errors: logger.errors().len(),
            warnings: logger.warnings().len(),
            patches_configured: logger.patches_configured,
            patches_completed: logger.patches_completed,
            patches_skipped: logger.patches_skipped,
            patches_failed: logger.patches_failed,
        }
    }

    /// Nothing was logged as a warning or an error.
    pub fn is_clean(&self) -> bool {
        self.errors == 0 && self.warnings == 0
    }

    /// The run finished, but not everything was applied (HoloPatcher's
    /// `InstallCompletedWithErrors` condition).
    pub fn completed_with_errors(&self) -> bool {
        self.errors > 0 || self.patches_failed > 0
    }
}

/// Everything a run leaves behind.
#[derive(Debug)]
pub struct RunOutcome {
    /// `Ok` when the install loop ran to its end; `Err` when it stopped early
    /// (an unrecognised game folder, an unsafe path, ...).
    pub result: Result<(), PatchException>,
    /// Every line the run logged.
    pub logs: Vec<PatchLog>,
    /// The counts.
    pub report: RunReport,
    /// Where the install log was written, if it could be.
    pub log_path: Option<PathBuf>,
}

/// True when the real path of `path` lies under the real path of `root`.
fn is_confined(root: &Path, path: &Path) -> bool {
    normcase_realpath(path).starts_with(normcase_realpath(root))
}

/// Whether a line of severity `log_type` is shown at `level`, following
/// the documented meaning of `LogLevel`: nothing; progress; progress and
/// errors; progress, errors and warnings; everything.
pub fn shows(level: LogLevel, log_type: LogType) -> bool {
    match level {
        LogLevel::Nothing => false,
        LogLevel::General => log_type == LogType::Note,
        LogLevel::Errors => matches!(log_type, LogType::Note | LogType::Error),
        LogLevel::Warnings => log_type != LogType::Verbose,
        LogLevel::Full => true,
    }
}

/// The colour a line is shown in: grey for detail, black for progress,
/// dark yellow for warnings, red for errors.
pub fn log_color(log_type: LogType) -> (u8, u8, u8) {
    match log_type {
        LogType::Verbose => (128, 128, 128),
        LogType::Note => (0, 0, 0),
        LogType::Warning => (176, 120, 0),
        LogType::Error => (200, 0, 0),
    }
}

/// Render the log as plain text.
fn plain_log(logs: &[PatchLog]) -> String {
    let mut body = String::new();
    for log in logs {
        body.push_str(&log.formatted_message());
        body.push_str("\r\n");
    }
    body
}

/// Render the log as styled text, one colour per kind of line.
fn styled_log(logs: &[PatchLog]) -> Vec<u8> {
    let mut colors: Vec<(u8, u8, u8)> = Vec::new();
    for log in logs {
        let color = log_color(log.log_type);
        if !colors.contains(&color) {
            colors.push(color);
        }
    }

    let mut body = String::from("{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0\\fmodern Courier New;}}");

    body.push_str("{\\colortbl ;");
    for (r, g, b) in &colors {
        body.push_str(&format!("\\red{r}\\green{g}\\blue{b};"));
    }
    body.push('}');
    body.push_str("\\f0\\fs16\r\n");

    for log in logs {
        // Colour indexes start at 1; index 0 is the default.
        let index = colors
            .iter()
            .position(|c| *c == log_color(log.log_type))
            .unwrap_or(0)
            + 1;
        body.push_str(&format!("\\cf{index} "));
        body.push_str(&escape_rtf(&log.formatted_message()));
        body.push_str("\\par\r\n");
    }

    body.push('}');
    crate::compat::latin1::encode(&body)
}

/// Escape the characters that would otherwise be read as markup.
fn escape_rtf(text: &str) -> String {
    let mut out = String::with_capacity(text.len());

    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\t' => out.push_str("\\tab "),
            '\n' => out.push_str("\\line "),
            '\r' => {}
            // CP1252 0x95 is the bullet; the low byte of U+2022 is not.
            '\u{2022}' => out.push_str("\\'95"),
            c if (c as u32) > 127 => out.push_str(&format!("\\'{:02x}", c as u32 & 0xFF)),
            c => out.push(c),
        }
    }

    out
}

/// Find the mod folder for a path the operator picked.
///
/// Accepts the folder holding `tslpatchdata` or `tslpatchdata` itself, and
/// answers with the former.
pub fn locate_mod(chosen: &str) -> Option<PathBuf> {
    let chosen = PathBuf::from(chosen.trim_end_matches(['\\', '/']));
    let inner = chosen.join(DATA_FOLDER);

    let data_folder = if safe_isdir(&inner) {
        inner
    } else if chosen
        .file_name()
        .map(|n| n.to_string_lossy().eq_ignore_ascii_case(DATA_FOLDER))
        .unwrap_or(false)
    {
        chosen
    } else {
        return None;
    };

    let has_instructions = safe_isfile(&data_folder.join(NAMESPACES_FILE))
        || safe_isfile(&data_folder.join(PatcherNamespace::DEFAULT_INI_FILENAME));

    if !has_instructions {
        return None;
    }

    // The session works from the folder that contains `tslpatchdata`.
    let root = data_folder
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    Some(if root.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        root
    })
}

/// True when a mod's data folder sits beside the program.
pub fn mod_is_beside_program(base_path: &str) -> bool {
    safe_isdir(&Path::new(base_path).join(DATA_FOLDER))
}

/// The folder this program is running from.
pub fn program_folder() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_else(|| ".".to_string())
}
