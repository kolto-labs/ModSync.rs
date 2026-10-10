use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand};
use modsync::{Settings, editor::easy::{Action, Session}};
use std::path::PathBuf;

/// Scriptable front end for the easy-mode authoring session. The desktop
/// builder calls the same `Session` methods.
#[derive(Args)]
pub struct EasyArgs {
    /// Authoring session directory. Keep it outside the game and mod folders.
    session: PathBuf,
    #[command(subcommand)]
    command: Step,
}

#[derive(Subcommand)]
enum Step {
    /// Start an empty session.
    New,
    /// Add a mod to the end of the build and print its id.
    Mod { name: String },
    /// Record one action, such as {"kind":"copy","from":["a.tga"],"to":"<<kotorDirectory>>\\Override"}.
    Act {
        #[arg(long = "mod")]
        component: String,
        /// Action as JSON.
        #[arg(long)]
        action: String,
        #[command(flatten)]
        roots: Roots,
        /// Also perform the action on the real game folder now.
        #[arg(long)]
        real: bool,
    },
    Undo,
    Redo,
    /// List revisions and the current position.
    Status,
    /// Print the predicted tree after a mod's current actions.
    Predict {
        #[arg(long = "mod")]
        component: String,
        #[command(flatten)]
        roots: Roots,
    },
    /// Save the finished build as an instruction file.
    Export { output: PathBuf },
}

#[derive(Args)]
struct Roots {
    #[arg(long)]
    game_dir: Option<PathBuf>,
    #[arg(long)]
    source_dir: Option<PathBuf>,
}

impl Roots {
    fn settings(&self) -> Result<Settings> {
        Settings::new(
            self.source_dir.as_deref().context("Supply --source-dir")?,
            self.game_dir.as_deref().context("Supply --game-dir")?,
        )
    }
}

impl EasyArgs {
    pub fn run(self) -> Result<()> {
        if let Step::New = self.command {
            Session::create(&self.session, &modsync::Guide::default())?;
            println!("created {}", self.session.display());
            return Ok(());
        }
        let mut session = Session::open(&self.session)?;
        match self.command {
            Step::New => unreachable!(),
            Step::Mod { name } => println!("{}", session.add_mod(&name)?),
            Step::Act { component, action, roots, real } => {
                let action: Action = serde_json::from_str(&action).context("Invalid action JSON")?;
                if real {
                    Session::apply_to_disk(&action, roots.settings()?)?;
                }
                let prediction = session.act(&component, &action, roots.settings()?)?;
                println!("{} ({} paths predicted)", action.label(), prediction.entries.len());
            }
            Step::Undo => {
                ensure!(session.can_undo(), "Nothing to undo");
                session.seek(session.cursor() - 1)?;
                println!("at {}", session.revisions()[session.cursor()].label);
            }
            Step::Redo => {
                ensure!(session.can_redo(), "Nothing to redo");
                session.seek(session.cursor() + 1)?;
                println!("at {}", session.revisions()[session.cursor()].label);
            }
            Step::Status => {
                for (i, r) in session.revisions().iter().enumerate() {
                    println!("{} {i}: {}", if i == session.cursor() { ">" } else { " " }, r.label);
                }
            }
            Step::Predict { component, roots } => {
                let index = session.guide().components.iter()
                    .position(|c| c.guid.eq_ignore_ascii_case(&component)).context("Unknown mod")?;
                let count = session.guide().components[index].instructions.len();
                let prediction = session.predict(roots.settings()?, index, count)?;
                println!("{}", modsync::editor::render_preview(&prediction)?);
            }
            Step::Export { output } => session.export(&output)?,
        }
        Ok(())
    }
}
