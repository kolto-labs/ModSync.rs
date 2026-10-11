//! Production entry points for the desktop-independent authoring history.
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand};
use modsync::editor::{self, easy::Session};
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct EasyArgs {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Import an archive privately and autosave its availability in undo/redo history.
    ImportArchive {
        directory: PathBuf,
        source: PathBuf,
        #[arg(long, default_value_t = 8 * 1024 * 1024 * 1024)]
        max_bytes: u64,
    },
    /// Recover only the checkpoint owned by the pending recorded action.
    DiskRecover { directory: PathBuf },
    /// Restore a saved filesystem boundary without replaying installation actions.
    DiskSeek { directory: PathBuf, cursor: usize },
    /// Apply exactly the next recorded action with a recoverable filesystem checkpoint.
    Apply {
        directory: PathBuf,
        #[arg(long)]
        source_dir: PathBuf,
        #[arg(long)]
        game_dir: PathBuf,
        #[arg(long)]
        component: usize,
        #[arg(long)]
        instruction: usize,
        #[arg(long)]
        allow_processes: bool,
        #[arg(long)]
        patcher: Option<PathBuf>,
        /// Abort if the current authoring revision changed before acquiring its lock.
        #[arg(long)]
        expected_revision: Option<String>,
        /// Private existing request storage; explicitly selects supplied installers.
        #[arg(long, requires = "provided_xvfb", conflicts_with = "patcher")]
        provided_request_dir: Option<PathBuf>,
        /// Installed Xvfb executable; a new owned display is allocated per launch.
        #[arg(long, requires = "provided_request_dir")]
        provided_xvfb: Option<PathBuf>,
        #[arg(long, requires = "provided_request_dir")]
        provided_wine: Option<PathBuf>,
        #[arg(long, requires = "provided_request_dir")]
        provided_wineserver: Option<PathBuf>,
        #[arg(long, requires = "provided_request_dir")]
        provided_wine_prefix: Option<PathBuf>,
    },
    /// Read a mod's patcher description, options, and instruction summary.
    PatcherInspect {
        source: String,
        #[arg(long)]
        source_dir: PathBuf,
        #[arg(long)]
        game_dir: PathBuf,
        #[arg(long)]
        namespace: Option<usize>,
        /// Inspect at a saved authoring point, including earlier archive actions.
        #[arg(long, requires_all = ["component", "instructions"])]
        directory: Option<PathBuf>,
        #[arg(long, requires = "directory")]
        component: Option<usize>,
        #[arg(long, requires = "directory")]
        instructions: Option<usize>,
    },
    /// Compare two installation trees byte for byte; no exclusions by default.
    Diff {
        left: PathBuf,
        right: PathBuf,
        /// Exact relative file or directory to exclude on both sides.
        #[arg(long)]
        exclude: Vec<String>,
    },
    /// Inspect predicted files at a chosen point without changing install roots.
    Predict {
        directory: PathBuf,
        #[arg(long)]
        source_dir: PathBuf,
        #[arg(long)]
        game_dir: PathBuf,
        /// Zero-based top-level mod index.
        #[arg(long)]
        component: usize,
        /// Number of this mod's actions to simulate.
        #[arg(long)]
        instructions: usize,
        /// Reject prediction if the current saved revision changed.
        #[arg(long)]
        expected_revision: Option<String>,
    },
    /// Create a new session from a guide, preserving its current selections.
    Create { directory: PathBuf, input: PathBuf },
    /// Inspect the saved revision cursor and labels.
    Status { directory: PathBuf },
    /// Apply an edit transaction and save the accepted revision automatically.
    Edit {
        directory: PathBuf,
        /// JSON operation list; use - to read up to 8 MiB from standard input.
        operations: PathBuf,
        #[arg(long)]
        label: String,
        /// Reject edits if another writer changed the current saved revision.
        #[arg(long)]
        expected_revision: Option<String>,
    },
    /// Return to the previous saved guide revision; does not alter installed files.
    Undo {
        directory: PathBuf,
        #[arg(long)]
        expected_revision: Option<String>,
    },
    /// Reapply the next saved guide revision; does not alter installed files.
    Redo {
        directory: PathBuf,
        #[arg(long)]
        expected_revision: Option<String>,
    },
    /// Export the current guide without changing authoring history.
    Export { directory: PathBuf, output: PathBuf },
}

pub(crate) fn run(args: EasyArgs) -> Result<()> {
    let session = match args.command {
        Command::ImportArchive {
            directory,
            source,
            max_bytes,
        } => {
            let mut session = Session::open(&directory)?;
            session.import_archive(
                &source,
                max_bytes,
                &std::sync::atomic::AtomicBool::new(false),
            )?;
            session
        }
        Command::DiskRecover { directory } => {
            let mut session = Session::open(&directory)?;
            session.recover_disk_action(&std::sync::atomic::AtomicBool::new(false))?;
            session
        }
        Command::DiskSeek { directory, cursor } => {
            let mut session = Session::open(&directory)?;
            session.seek_disk(
                cursor,
                &std::sync::atomic::AtomicBool::new(false),
                |progress| eprintln!("{}", progress.message),
            )?;
            session
        }
        Command::Apply {
            directory,
            source_dir,
            game_dir,
            component,
            instruction,
            allow_processes,
            patcher,
            expected_revision,
            provided_request_dir,
            provided_xvfb,
            provided_wine,
            provided_wineserver,
            provided_wine_prefix,
        } => {
            let mut session = Session::open(&directory)?;
            require_revision(&session, expected_revision.as_deref())?;
            let supplied = provided_request_dir.is_some()
                || provided_xvfb.is_some()
                || provided_wine.is_some()
                || provided_wineserver.is_some()
                || provided_wine_prefix.is_some();
            let provided_installer = if supplied {
                ensure!(
                    allow_processes,
                    "Supplied installer execution requires --allow-processes"
                );
                ensure!(
                    patcher.is_none(),
                    "Do not combine --patcher with supplied installer execution"
                );
                let request_dir = provided_request_dir
                    .context("Supplied runner requires --provided-request-dir")?;
                let xvfb = provided_xvfb.context("Supplied runner requires --provided-xvfb")?;
                let windows = match (provided_wine, provided_wineserver, provided_wine_prefix) {
                    (None, None, None) => None,
                    (Some(executable), Some(wineserver), Some(prefix)) => {
                        Some(modsync::engine::WindowsProcessRunner {
                            executable,
                            wineserver,
                            prefix,
                        })
                    }
                    _ => anyhow::bail!(
                        "Configure --provided-wine, --provided-wineserver and --provided-wine-prefix together"
                    ),
                };
                Some(modsync::engine::ProvidedInstallerRunner {
                    request_dir,
                    xvfb,
                    windows,
                })
            } else {
                None
            };
            let mut settings = modsync::Settings::new(&source_dir, &game_dir)?;
            settings.allow_processes = allow_processes;
            settings.patcher = patcher;
            settings.provided_installer = provided_installer;
            let report = session.apply_to_disk(
                settings,
                component,
                instruction,
                |event| eprintln!("{}: {}", event.component, event.message),
                |progress| eprintln!("{}", progress.message),
            )?;
            println!("{}", editor::render_preview(&report)?);
            return Ok(());
        }
        Command::PatcherInspect {
            source,
            source_dir,
            game_dir,
            namespace,
            directory,
            component,
            instructions,
        } => {
            let settings = modsync::Settings::new(&source_dir, &game_dir)?;
            let preview = if let Some(directory) = directory {
                Session::open(&directory)?.inspect_patcher(
                    settings,
                    component.context("Missing component")?,
                    instructions.context("Missing instruction point")?,
                    &source,
                    namespace,
                )?
            } else {
                editor::easy::patcher::inspect(&settings.roots, &source, namespace)?
            };
            println!("{}", editor::render_preview(&preview)?);
            return Ok(());
        }
        Command::Diff {
            left,
            right,
            exclude,
        } => {
            let diff = editor::easy::diff::compare(&left, &right, &exclude)?;
            println!("{}", editor::render_preview(&diff)?);
            ensure!(diff.identical, "Installation trees differ");
            return Ok(());
        }
        Command::Predict {
            directory,
            source_dir,
            game_dir,
            component,
            instructions,
            expected_revision,
        } => {
            let session = Session::open(&directory)?;
            require_revision(&session, expected_revision.as_deref())?;
            let prediction = session.predict(
                modsync::Settings::new(&source_dir, &game_dir)?,
                component,
                instructions,
            )?;
            println!("{}", editor::render_preview(&prediction)?);
            return Ok(());
        }
        Command::Create { directory, input } => {
            Session::create(&directory, &editor::read_authoring_guide(&input)?)?
        }
        Command::Status { directory } => Session::open(&directory)?,
        Command::Edit {
            directory,
            operations,
            label,
            expected_revision,
        } => {
            let operations = if operations == std::path::Path::new("-") {
                use std::io::Read;
                const LIMIT: u64 = 8 * 1024 * 1024;
                let mut bytes = Vec::new();
                std::io::stdin()
                    .lock()
                    .take(LIMIT + 1)
                    .read_to_end(&mut bytes)?;
                ensure!(bytes.len() as u64 <= LIMIT, "Editor document exceeds 8 MiB");
                serde_json::from_str::<Vec<editor::EditOperation>>(std::str::from_utf8(&bytes)?)?
            } else {
                editor::load_operations(&operations)?
            };
            let mut session = Session::open(&directory)?;
            require_revision(&session, expected_revision.as_deref())?;
            session.edit(&label, &operations)?;
            session
        }
        Command::Undo {
            directory,
            expected_revision,
        } => {
            let mut session = Session::open(&directory)?;
            require_revision(&session, expected_revision.as_deref())?;
            ensure!(
                session.can_undo(),
                "Already at the first authoring revision"
            );
            session.seek(session.cursor() - 1)?;
            session
        }
        Command::Redo {
            directory,
            expected_revision,
        } => {
            let mut session = Session::open(&directory)?;
            require_revision(&session, expected_revision.as_deref())?;
            ensure!(session.can_redo(), "No redo revision is available");
            session.seek(session.cursor() + 1)?;
            session
        }
        Command::Export { directory, output } => {
            let session = Session::open(&directory)?;
            session.export(&output)?;
            session
        }
    };
    println!(
        "{}",
        editor::render_preview(&serde_json::json!({
            "directory": session.directory(),
            "cursor": session.cursor(),
            "currentRevision": session.revisions()[session.cursor()].id,
            "canUndo": session.can_undo(),
            "canRedo": session.can_redo(),
            "revisions": session.revisions(),
            "filesystem": session.disk_history()?,
        }))?
    );
    Ok(())
}

/// Session::open holds the exclusive OS lock until this transaction completes.
fn require_revision(session: &Session, expected: Option<&str>) -> Result<()> {
    if let Some(expected) = expected {
        ensure!(
            session.revisions()[session.cursor()].id == expected,
            "Authoring revision changed; reload the session before editing"
        );
    }
    Ok(())
}
