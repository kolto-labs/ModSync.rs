//! Explicit per-user protocol association commands over the shared owner API.
use anyhow::{Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use modsync::protocol_registration::owned::{self, Request, Scheme, State};
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct ProtocolArgs {
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
    /// Inspect associations without changing them.
    Status(Options),
    /// Register this executable for the requested link schemes.
    Register(Options),
    /// Remove only associations still owned by this package and executable.
    Unregister(Options),
}
#[derive(Clone, Copy, ValueEnum)]
enum Schemes {
    Both,
    Modsync,
    Nxm,
}
#[derive(Args)]
struct Options {
    #[arg(long)]
    executable: Option<PathBuf>,
    #[arg(long)]
    package_token: Option<String>,
    #[arg(long)]
    channel: Option<String>,
    #[arg(long, value_enum, default_value = "both")]
    scheme: Schemes,
    /// Explicitly authorize replacing a different current handler on registration.
    #[arg(long)]
    replace: bool,
    #[arg(long)]
    json: bool,
}

impl ProtocolArgs {
    pub fn run(self) -> Result<()> {
        let (operation, options) = match self.action {
            Action::Status(options) => ("status", options),
            Action::Register(options) => ("register", options),
            Action::Unregister(options) => ("unregister", options),
        };
        anyhow::ensure!(
            !options.replace || operation == "register",
            "--replace applies only to registration"
        );
        let executable = match options.executable {
            Some(path) => path,
            None => {
                let executable = std::env::current_exe().context("Find protocol executable")?;
                #[cfg(windows)]
                let executable = {
                    let launcher = executable
                        .parent()
                        .context("Executable has no directory")?
                        .join("modsync-gui.exe");
                    anyhow::ensure!(
                        launcher.is_file(),
                        "Windows registration needs the packaged GUI launcher or explicit --executable"
                    );
                    launcher
                };
                executable
            }
        };
        let identity = if options.package_token.is_none() || options.channel.is_none() {
            Some(modsync::protocol_registration::package_identity(
                &executable,
            )?)
        } else {
            None
        };
        let request = Request {
            executable,
            package_token: options
                .package_token
                .unwrap_or_else(|| identity.as_ref().unwrap().package_token.clone()),
            channel: options
                .channel
                .unwrap_or_else(|| identity.as_ref().unwrap().channel.clone()),
            schemes: match options.scheme {
                Schemes::Both => vec![Scheme::Modsync, Scheme::Nxm],
                Schemes::Modsync => vec![Scheme::Modsync],
                Schemes::Nxm => vec![Scheme::Nxm],
            },
            replace: options.replace,
        };
        let cancelled = modsync::p2p::stop_signal_flag()?;
        let result = match operation {
            "register" => owned::register(&request, &cancelled),
            "unregister" => owned::unregister(&request, &cancelled),
            _ => owned::status(&request, &cancelled),
        };
        let report = match result {
            Err(error) if cancelled.load(std::sync::atomic::Ordering::Acquire) => {
                eprintln!("Protocol operation cancelled: {error:#}");
                return Err(crate::cli_session::Exit(130).into());
            }
            other => other?,
        };
        if options.json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            for entry in &report.entries {
                println!(
                    "{}: {:?}{}",
                    entry.scheme.name(),
                    entry.state,
                    if entry.changed { " (changed)" } else { "" }
                );
                if let Some(message) = &entry.message {
                    println!("  {message}");
                }
            }
            for warning in &report.warnings {
                eprintln!("Warning: {warning}");
            }
        }
        if report
            .entries
            .iter()
            .any(|entry| entry.state == State::Unsupported)
        {
            return Err(crate::cli_session::Exit(1).into());
        }
        Ok(())
    }
}
