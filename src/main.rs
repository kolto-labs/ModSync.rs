use anyhow::{Context, Result, ensure};
use clap::{Args, Parser, Subcommand};
use modsync::{Engine, Guide, Settings, download, paths::Roots, recovery};
use std::{path::PathBuf, sync::atomic::AtomicBool};

#[cfg(feature = "desktop")]
mod ui;

#[derive(Parser)]
#[command(
    version,
    about = "Install ModSync guides with a Rust desktop app or CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Action>,
}
#[derive(Subcommand)]
enum Action {
    /// Open the desktop installer (also the default with no command).
    Gui {
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        game_dir: Option<PathBuf>,
        #[arg(long)]
        source_dir: Option<PathBuf>,
    },
    /// List components and their download links as JSON.
    List { input: PathBuf },
    /// Simulate instructions without changing the game or mod directory.
    Validate(InstallArgs),
    /// Validate, snapshot, then install in dependency order. Stop at the first failure.
    Install(InstallArgs),
    /// Restore both directories from the verified pre-install snapshot after a failure.
    Restore {
        #[arg(long)]
        game_dir: PathBuf,
        #[arg(long)]
        source_dir: PathBuf,
    },
    /// Convert TOML, JSON, YAML, XML, or Markdown input.
    Convert {
        input: Option<PathBuf>,
        output: Option<PathBuf>,
        #[arg(short = 'i', long = "input", conflicts_with_all = ["input", "stdin"])]
        input_path: Option<PathBuf>,
        #[arg(short = 'o', long = "output", conflicts_with = "output")]
        output_path: Option<PathBuf>,
        #[arg(long, conflicts_with = "input")]
        stdin: bool,
        /// Output format; defaults to TOML, or the positional output extension.
        #[arg(short, long)]
        format: Option<String>,
        /// Draft instructions from prose; generated steps require review before installation.
        #[arg(long)]
        parse_directions: bool,
        /// Mark a component's displayed drafts reviewed (GUID; repeat for multiple components).
        #[arg(long)]
        review_draft: Vec<String>,
    },
    /// Fetch and verify a direct HTTPS archive URL.
    Download {
        url: String,
        output: PathBuf,
        #[arg(long)]
        sha256: Option<String>,
        /// Optional expanded-byte budget for the integrity check.
        #[arg(long)]
        max_expanded_bytes: Option<u64>,
    },
}
#[derive(Args)]
struct InstallArgs {
    #[arg(long, short)]
    input: PathBuf,
    #[arg(long)]
    game_dir: PathBuf,
    #[arg(long)]
    source_dir: PathBuf,
    /// Honor IsSelected flags; by default select all top-level components.
    #[arg(long)]
    use_file_selection: bool,
    /// Select only these component names or GUIDs (repeat the flag).
    #[arg(long)]
    select: Vec<String>,
    /// Select option GUIDs (repeat the flag). Existing option selections are retained.
    #[arg(long)]
    option: Vec<String>,
    /// Trust this guide to run programs with your user permissions.
    #[arg(long)]
    allow_processes: bool,
    /// OdyPatcher executable for Patcher instructions.
    #[arg(long)]
    patcher: Option<PathBuf>,
    /// Existing scratch folder outside the game and mod trees (defaults to system temp).
    #[arg(long)]
    scratch_dir: Option<PathBuf>,
    /// Optional expanded-byte budget per archive; extraction also checks free scratch space.
    #[arg(long)]
    max_expanded_bytes: Option<u64>,
    /// Emit newline-delimited JSON events.
    #[arg(long)]
    json: bool,
}

fn run() -> Result<()> {
    match Cli::parse().command {
        None => gui(None, None, None),
        Some(Action::Gui {
            input,
            game_dir,
            source_dir,
        }) => gui(input, game_dir, source_dir),
        Some(Action::List { input }) => {
            println!("{}", serde_json::to_string_pretty(&Guide::load(&input)?)?);
            Ok(())
        }
        Some(Action::Convert {
            input,
            output,
            input_path,
            output_path,
            stdin,
            format,
            parse_directions,
            review_draft,
        }) => {
            let mut guide = if stdin {
                use std::io::Read;
                let mut text = String::new();
                std::io::stdin().read_to_string(&mut text)?;
                Guide::parse(&text, "auto")?
            } else {
                Guide::load(
                    &input_path
                        .or(input)
                        .context("Use --input <file> or --stdin")?,
                )?
            };
            if parse_directions {
                modsync::draft::generate(&mut guide);
            }
            for id in review_draft {
                let component = guide
                    .components
                    .iter_mut()
                    .find(|c| c.guid.eq_ignore_ascii_case(&id))
                    .with_context(|| format!("Unknown component GUID: {id}"))?;
                modsync::draft::mark_reviewed(component)?;
            }
            let format = format.as_deref().unwrap_or_else(|| {
                output
                    .as_ref()
                    .and_then(|p| p.extension())
                    .and_then(|s| s.to_str())
                    .unwrap_or("toml")
            });
            let text = guide.render(format)?;
            if let Some(path) = output_path.as_ref().or(output.as_ref()) {
                recovery::atomic_write(&std::path::absolute(path)?, text.as_bytes())?;
            } else {
                println!("{text}");
            }
            Ok(())
        }
        Some(Action::Restore {
            game_dir,
            source_dir,
        }) => recovery::restore(&Roots::new(&source_dir, &game_dir)?),
        Some(Action::Download {
            url,
            output,
            sha256,
            max_expanded_bytes,
        }) => download::fetch_with_options(
            &url,
            &output,
            sha256.as_deref(),
            &AtomicBool::new(false),
            &modsync::archive::Options {
                max_expanded_bytes,
                ..Default::default()
            },
        ),
        Some(action @ (Action::Install(_) | Action::Validate(_))) => {
            let install = matches!(action, Action::Install(_));
            let (Action::Install(args) | Action::Validate(args)) = action else {
                unreachable!()
            };
            let mut guide = Guide::load_for_install(&args.input)?;
            for requested in &args.select {
                ensure!(
                    guide
                        .components
                        .iter()
                        .any(|c| c.guid.eq_ignore_ascii_case(requested)
                            || c.name.eq_ignore_ascii_case(requested)),
                    "Unknown component: {requested}"
                );
            }
            for c in &mut guide.components {
                if !args.select.is_empty() {
                    c.is_selected = args
                        .select
                        .iter()
                        .any(|s| c.guid.eq_ignore_ascii_case(s) || c.name.eq_ignore_ascii_case(s));
                } else if !args.use_file_selection {
                    c.is_selected = true;
                }
            }
            fn select_option(c: &mut modsync::Component, id: &str) -> bool {
                for o in &mut c.options {
                    if o.guid.eq_ignore_ascii_case(id) {
                        o.is_selected = true;
                        return true;
                    }
                    if select_option(o, id) {
                        return true;
                    }
                }
                false
            }
            for id in &args.option {
                ensure!(
                    guide.components.iter_mut().any(|c| select_option(c, id)),
                    "Unknown option: {id}"
                );
            }
            let mut settings = Settings::new(&args.source_dir, &args.game_dir)?;
            settings.allow_processes = args.allow_processes;
            settings.patcher = args.patcher;
            settings.archive.scratch_dir = args.scratch_dir;
            settings.archive.max_expanded_bytes = args.max_expanded_bytes;
            let engine = Engine::new(settings)?;
            let emit = |event: modsync::Event| {
                if args.json {
                    println!("{}", serde_json::to_string(&event).unwrap());
                } else {
                    println!("{} / {}: {}", event.component, event.step, event.message);
                }
            };
            if install {
                engine.install(&guide, emit)
            } else {
                engine.validate(&guide, emit)
            }
        }
    }
}

fn gui(input: Option<PathBuf>, game: Option<PathBuf>, mods: Option<PathBuf>) -> Result<()> {
    #[cfg(feature = "desktop")]
    {
        ui::launch(input, game, mods)
    }
    #[cfg(not(feature = "desktop"))]
    {
        let _ = (input, game, mods);
        anyhow::bail!(
            "Desktop support is disabled. Build with the default features, or use --help for CLI commands."
        )
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}
