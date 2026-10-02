//! Explicit maintenance commands over the shared preview and recovery contract.
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand, ValueEnum};
use modsync::{maintenance, paths::Roots};
use std::{
    path::{Component, Path, PathBuf},
    sync::atomic::Ordering,
};

#[derive(Args)]
pub(crate) struct MaintenanceArgs {
    #[command(subcommand)]
    action: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Preview descendant lowercasing or access for the current owner.
    Preview(PreviewArgs),
    /// Apply only the exact plan identified by a reviewed preview digest.
    Apply {
        #[command(flatten)]
        request: PreviewArgs,
        #[arg(long)]
        expect: String,
    },
    /// Inspect interrupted maintenance without changing the roots.
    Status(RootArgs),
    /// Recover the pending operation identified by its durable digest.
    Recover {
        #[command(flatten)]
        roots: RootArgs,
        #[arg(long)]
        operation: String,
    },
}

#[derive(Args)]
struct RootArgs {
    #[arg(long)]
    game_dir: PathBuf,
    #[arg(long)]
    source_dir: PathBuf,
    /// Emit one structured result, including failures, on stdout.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Target {
    Game,
    Mods,
}

#[derive(Clone, Copy, ValueEnum)]
enum Action {
    LowercaseNames,
    OwnerWritable,
}

#[derive(Args)]
struct PreviewArgs {
    #[command(flatten)]
    roots: RootArgs,
    #[arg(long, value_enum)]
    target: Target,
    /// Relative subtree; pass an empty string to select the configured root.
    #[arg(long)]
    subtree: String,
    /// Owner-writable preserves ownership; ACL takeover is unsupported.
    #[arg(long, value_enum)]
    action: Action,
}

/// Validate authored root spellings before canonicalization could hide links.
/// The desktop also calls this from its worker, never from the paint path.
pub(crate) fn resolve_roots(source: &Path, game: &Path) -> Result<Roots> {
    for root in [source, game] {
        ensure!(
            !root.as_os_str().is_empty(),
            "Set both game and mod directories"
        );
        ensure!(
            !root
                .components()
                .any(|part| matches!(part, Component::ParentDir)),
            "Maintenance root cannot contain parent traversal"
        );
        let absolute = std::path::absolute(root)?;
        for ancestor in absolute.ancestors().collect::<Vec<_>>().into_iter().rev() {
            let metadata = std::fs::symlink_metadata(ancestor)
                .with_context(|| format!("Read maintenance root {}", ancestor.display()))?;
            ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "Maintenance roots and their ancestors must be ordinary directories"
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "Maintenance root is a reparse point"
                );
            }
        }
    }
    Roots::new(source, game)
}

pub(crate) fn outcome_exit(outcome: &maintenance::Outcome) -> i32 {
    match &outcome.status {
        maintenance::OutcomeStatus::Applied => 0,
        maintenance::OutcomeStatus::CancelledAndRolledBack => 130,
        maintenance::OutcomeStatus::FailedAndRolledBack
        | maintenance::OutcomeStatus::RecoveryRequired => 1,
    }
}

impl MaintenanceArgs {
    pub(crate) fn run(self) -> Result<()> {
        let (operation, root_args, request, expected) = match self.action {
            Command::Preview(request) => (
                "preview",
                request.roots,
                Some((request.target, request.subtree, request.action)),
                None,
            ),
            Command::Apply { request, expect } => (
                "apply",
                request.roots,
                Some((request.target, request.subtree, request.action)),
                Some(expect),
            ),
            Command::Status(roots) => ("status", roots, None, None),
            Command::Recover { roots, operation } => ("recover", roots, None, Some(operation)),
        };
        let json = root_args.json;
        let mut bound_roots = None;
        let mut cancelled = None;
        let result = (|| -> Result<(serde_json::Value, i32)> {
            let roots = resolve_roots(&root_args.source_dir, &root_args.game_dir)?;
            bound_roots = Some(roots.clone());
            let flag = modsync::p2p::stop_signal_flag()?;
            cancelled = Some(flag.clone());
            let request = request.map(|(target, subtree, action)| maintenance::Request {
                roots: roots.clone(),
                target: match target {
                    Target::Game => maintenance::TreeRoot::Game,
                    Target::Mods => maintenance::TreeRoot::Mods,
                },
                subtree,
                action: match action {
                    Action::LowercaseNames => maintenance::Action::LowercaseNames,
                    Action::OwnerWritable => maintenance::Action::OwnerWritable,
                },
            });
            match operation {
                "preview" => {
                    let preview = maintenance::preview(
                        request.as_ref().context("Missing preview request")?,
                        &flag,
                    )?;
                    let status = maintenance::status(&roots)?;
                    let exit = if flag.load(Ordering::Acquire) {
                        130
                    } else if status.pending {
                        1
                    } else {
                        0
                    };
                    Ok((
                        serde_json::json!({ "operation": operation, "preview": preview, "status": status }),
                        exit,
                    ))
                }
                "apply" | "recover" => {
                    let expected = expected
                        .as_deref()
                        .context("Supply a reviewed operation digest")?;
                    let outcome = if operation == "apply" {
                        maintenance::apply(
                            request.as_ref().context("Missing apply request")?,
                            expected,
                            &flag,
                        )?
                    } else {
                        maintenance::recover(&roots, expected, &flag)?
                    };
                    let mut exit = outcome_exit(&outcome);
                    let (status, status_error) = match maintenance::status(&roots) {
                        Ok(status) => {
                            if status.pending && exit == 0 {
                                exit = 1;
                            }
                            (Some(status), None)
                        }
                        Err(error) => {
                            if exit == 0 {
                                exit = 1;
                            }
                            (None, Some(format!("{error:#}")))
                        }
                    };
                    Ok((
                        serde_json::json!({ "operation": operation, "outcome": outcome, "status": status, "statusError": status_error }),
                        exit,
                    ))
                }
                _ => {
                    let status = maintenance::status(&roots)?;
                    let exit = if flag.load(Ordering::Acquire) {
                        130
                    } else if status.pending {
                        1
                    } else {
                        0
                    };
                    Ok((
                        serde_json::json!({ "operation": operation, "status": status }),
                        exit,
                    ))
                }
            }
        })();
        let (mut report, exit) = match result {
            Ok(report) => report,
            Err(error) => {
                let exit = if cancelled
                    .as_ref()
                    .is_some_and(|flag| flag.load(Ordering::Acquire))
                {
                    130
                } else {
                    1
                };
                let status = bound_roots.as_ref().map(maintenance::status).transpose();
                let (status, status_error) = match status {
                    Ok(status) => (status, None),
                    Err(error) => (None, Some(format!("{error:#}"))),
                };
                (
                    serde_json::json!({ "operation": operation, "success": false, "exitCode": exit, "error": format!("{error:#}"), "status": status, "statusError": status_error }),
                    exit,
                )
            }
        };
        report["success"] = serde_json::json!(exit == 0);
        report["exitCode"] = serde_json::json!(exit);
        if json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            println!("Maintenance {operation}:");
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        if exit != 0 {
            return Err(crate::cli_session::Exit(exit).into());
        }
        Ok(())
    }
}
