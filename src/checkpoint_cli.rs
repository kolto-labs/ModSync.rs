//! Thin CLI adapter for the shared checkpoint store.
use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};
use modsync::{
    checkpoint::{Store, TreeRoot},
    paths::Roots,
};
use std::{path::PathBuf, sync::atomic::Ordering};

#[derive(Args)]
pub struct CheckpointArgs {
    #[command(subcommand)]
    action: Action,
}

#[derive(Args)]
pub(crate) struct ResumeArgs {
    #[command(flatten)]
    pub install: crate::InstallArgs,
    /// Session whose current boundary supplies the verified completed prefix.
    #[arg(long)]
    pub session: String,
    /// Accept changed source archives after restore; game and managed state must match.
    #[arg(long)]
    pub accept_source_changes: bool,
}

pub(crate) struct ResumeSelection {
    pub session: String,
    pub accept_source_changes: bool,
}

#[derive(Args)]
struct RootArgs {
    #[arg(long)]
    source_dir: PathBuf,
    #[arg(long)]
    game_dir: PathBuf,
}

impl RootArgs {
    fn roots(&self) -> Result<Roots> {
        Roots::new(
            &self.source_dir,
            &modsync::game::resolve_install_root(&self.game_dir),
        )
    }

    fn store(&self) -> Result<Store> {
        Store::open(&self.roots()?)
    }
}

#[derive(Subcommand)]
enum Action {
    /// Resume the ordered suffix from a verified checkpoint boundary.
    Resume(Box<ResumeArgs>),
    /// Restore a committed boundary into a new linked history branch.
    Restore {
        #[command(flatten)]
        roots: RootArgs,
        #[arg(long)]
        session: String,
        #[arg(long)]
        checkpoint: String,
    },
    /// Finish an interrupted checkpoint operation before any further installation.
    Recover {
        #[command(flatten)]
        roots: RootArgs,
        /// Select a session when more than one requires attention.
        #[arg(long)]
        session: Option<String>,
    },
    /// Move an unreferenced, inactive session into checkpoint trash.
    DeleteSession {
        #[command(flatten)]
        roots: RootArgs,
        #[arg(long)]
        session: String,
    },
    /// Reclaim content that no retained checkpoint or receipt references.
    Gc {
        #[command(flatten)]
        roots: RootArgs,
    },
    /// Repair content objects using bytes that match their retained hash and size.
    Repair {
        #[command(flatten)]
        roots: RootArgs,
        #[arg(long)]
        session: String,
        #[arg(long)]
        checkpoint: Option<String>,
    },
    /// Capture both roots into a new independent baseline session.
    /// This does not install, restore, or assert any completed components.
    Capture {
        #[command(flatten)]
        roots: RootArgs,
    },
    /// List stored checkpoint sessions without creating or repairing a store.
    List {
        #[command(flatten)]
        roots: RootArgs,
    },
    /// List every committed boundary in a session with compact metadata.
    History {
        #[command(flatten)]
        roots: RootArgs,
        #[arg(long)]
        session: String,
    },
    /// Verify checkpoint metadata and every referenced content object.
    Verify {
        #[command(flatten)]
        roots: RootArgs,
        #[arg(long)]
        session: String,
        /// Defaults to the session HEAD checkpoint.
        #[arg(long)]
        checkpoint: Option<String>,
    },
    /// Inspect a committed checkpoint and its session provenance.
    Inspect {
        #[command(flatten)]
        roots: RootArgs,
        #[arg(long)]
        session: String,
        #[arg(long)]
        checkpoint: Option<String>,
    },
    /// Compare the file and directory inventories of two committed checkpoints.
    Diff {
        #[command(flatten)]
        roots: RootArgs,
        #[arg(long)]
        session: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
    /// Show captured changes for one portable relative path through checkpoint history.
    Provenance {
        #[command(flatten)]
        roots: RootArgs,
        #[arg(long)]
        session: String,
        #[arg(long)]
        checkpoint: Option<String>,
        #[arg(long, value_enum)]
        tree: TreeArg,
        #[arg(long)]
        path: String,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum TreeArg {
    Mods,
    Game,
}

impl CheckpointArgs {
    pub(crate) fn into_resume(
        self,
    ) -> std::result::Result<(crate::InstallArgs, ResumeSelection), Box<Self>> {
        match self.action {
            Action::Resume(args) => Ok((
                args.install,
                ResumeSelection {
                    session: args.session,
                    accept_source_changes: args.accept_source_changes,
                },
            )),
            action => Err(Box::new(Self { action })),
        }
    }

    pub fn run(self) -> Result<()> {
        let read = match &self.action {
            Action::List { roots } => {
                let store = roots.store()?;
                let mut value = serde_json::to_value(store.list()?)?;
                value["externalProcess"] = serde_json::to_value(store.external_process_status()?)?;
                Some(value)
            }
            Action::History { roots, session } => {
                let store = roots.store()?;
                let mut value = serde_json::to_value(store.history(session)?)?;
                value["capability"] = serde_json::to_value(store.capability(session)?)?;
                value["externalProcess"] = serde_json::to_value(store.external_process_status()?)?;
                Some(value)
            }
            Action::Inspect {
                roots,
                session,
                checkpoint,
            } => Some(serde_json::to_value(
                roots.store()?.inspect(session, checkpoint.as_deref())?,
            )?),
            Action::Diff {
                roots,
                session,
                from,
                to,
            } => Some(serde_json::to_value(
                roots.store()?.diff(session, from, to)?,
            )?),
            Action::Provenance {
                roots,
                session,
                checkpoint,
                tree,
                path,
            } => Some(serde_json::to_value(roots.store()?.provenance(
                session,
                checkpoint.as_deref(),
                match tree {
                    TreeArg::Mods => TreeRoot::Mods,
                    TreeArg::Game => TreeRoot::Game,
                },
                path,
            )?)?),
            _ => None,
        };
        if let Some(report) = read {
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
        let cancelled = modsync::p2p::stop_signal_flag()?;
        let mut incomplete_repair = false;
        let result = (|| -> Result<serde_json::Value> {
            Ok(match self.action {
                Action::DeleteSession { roots, session } => {
                    let bound = roots.roots()?;
                    let store = Store::open(&bound)?;
                    let current = store.inspect(&session, None)?;
                    let expected = modsync::checkpoint::ExpectedHead {
                        session_id: current.session_id,
                        checkpoint_id: current.head_checkpoint_id,
                        generation: current.generation,
                    };
                    store.prepare_write()?;
                    let lock = modsync::recovery::Lock::acquire(&bound)?;
                    serde_json::to_value(store.delete_session(&lock, &expected, &cancelled)?)?
                }
                Action::Gc { roots } => {
                    let bound = roots.roots()?;
                    let store = Store::open(&bound)?;
                    store.prepare_write()?;
                    let lock = modsync::recovery::Lock::acquire(&bound)?;
                    serde_json::to_value(store.gc(&lock, &cancelled, |_| {})?)?
                }
                Action::Repair {
                    roots,
                    session,
                    checkpoint,
                } => {
                    let bound = roots.roots()?;
                    let store = Store::open(&bound)?;
                    store.prepare_write()?;
                    let lock = modsync::recovery::Lock::acquire(&bound)?;
                    let report =
                        store.repair(&lock, &session, checkpoint.as_deref(), &cancelled, |_| {})?;
                    incomplete_repair = !report.complete;
                    serde_json::to_value(report)?
                }
                Action::Restore {
                    roots,
                    session,
                    checkpoint,
                } => {
                    let bound = roots.roots()?;
                    let store = Store::open(&bound)?;
                    let current = store.inspect(&session, None)?;
                    let expected = modsync::checkpoint::ExpectedHead {
                        session_id: current.session_id,
                        checkpoint_id: current.head_checkpoint_id,
                        generation: current.generation,
                    };
                    store.prepare_write()?;
                    let lock = modsync::recovery::Lock::acquire(&bound)?;
                    serde_json::to_value(store.restore_checkpoint(
                        &lock,
                        &expected,
                        &checkpoint,
                        &cancelled,
                        |_| {},
                    )?)?
                }
                Action::Recover { roots, session } => {
                    let bound = roots.roots()?;
                    let store = Store::open(&bound)?;
                    store.prepare_write()?;
                    let lock = modsync::recovery::Lock::acquire(&bound)?;
                    serde_json::to_value(store.recover_with_process_quiescence(
                        &lock,
                        session.as_deref(),
                        &cancelled,
                        |_| {},
                    )?)?
                }
                Action::Capture { roots } => {
                    serde_json::to_value(roots.store()?.capture(&cancelled, |_| {})?)?
                }
                Action::Verify {
                    roots,
                    session,
                    checkpoint,
                } => serde_json::to_value(roots.store()?.verify(
                    &session,
                    checkpoint.as_deref(),
                    &cancelled,
                )?)?,
                _ => unreachable!("read-only operation returned above"),
            })
        })();
        match result {
            Ok(report) => {
                println!("{}", serde_json::to_string_pretty(&report)?);
                if incomplete_repair
                    || report.get("complete").and_then(serde_json::Value::as_bool) == Some(false)
                {
                    eprintln!(
                        "Checkpoint operation is incomplete; inspect the recovery or unresolved-object details in the report."
                    );
                    return Err(
                        crate::cli_session::Exit(if cancelled.load(Ordering::Relaxed) {
                            130
                        } else {
                            1
                        })
                        .into(),
                    );
                }
            }
            Err(error) if cancelled.load(Ordering::Relaxed) => {
                eprintln!("Checkpoint operation cancelled: {error:#}");
                return Err(crate::cli_session::Exit(130).into());
            }
            Err(error) => return Err(error),
        }
        Ok(())
    }
}
