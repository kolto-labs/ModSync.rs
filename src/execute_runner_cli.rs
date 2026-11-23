//! Per-invocation Windows Execute configuration, independent of Patcher backends.
use anyhow::{Context, Result};
use clap::Args;
use modsync::engine::{ProvidedInstallerRunner, WindowsProcessRunner};
use std::path::PathBuf;

#[derive(Args, Clone, Debug, Default)]
pub(crate) struct ExecuteRunnerArgs {
    /// Private existing request directory for Windows Execute programs; does not grant process trust.
    #[arg(long, help_heading = "Windows Execute runner", requires_all = ["execute_xvfb", "execute_wine", "execute_wineserver", "execute_wine_prefix"])]
    execute_request_dir: Option<PathBuf>,
    /// Installed Xvfb executable for an owned display.
    #[arg(
        long,
        help_heading = "Windows Execute runner",
        requires = "execute_request_dir"
    )]
    execute_xvfb: Option<PathBuf>,
    /// Installed Wine executable for Windows Execute instructions.
    #[arg(
        long,
        help_heading = "Windows Execute runner",
        requires = "execute_request_dir"
    )]
    execute_wine: Option<PathBuf>,
    /// Installed wineserver executable for dedicated-prefix cleanup.
    #[arg(
        long,
        help_heading = "Windows Execute runner",
        requires = "execute_request_dir"
    )]
    execute_wineserver: Option<PathBuf>,
    /// Dedicated existing Wine prefix beneath the request directory.
    #[arg(
        long,
        help_heading = "Windows Execute runner",
        requires = "execute_request_dir"
    )]
    execute_wine_prefix: Option<PathBuf>,
}

impl ExecuteRunnerArgs {
    pub(crate) fn is_configured(&self) -> bool {
        self.execute_request_dir.is_some()
            || self.execute_xvfb.is_some()
            || self.execute_wine.is_some()
            || self.execute_wineserver.is_some()
            || self.execute_wine_prefix.is_some()
    }

    pub(crate) fn build(&self) -> Result<Option<ProvidedInstallerRunner>> {
        if !self.is_configured() {
            return Ok(None);
        }
        Ok(Some(ProvidedInstallerRunner {
            request_dir: self
                .execute_request_dir
                .clone()
                .context("Supply --execute-request-dir with every Windows Execute runner option")?,
            xvfb: self.execute_xvfb.clone().context("Supply --execute-xvfb")?,
            windows: Some(WindowsProcessRunner {
                executable: self.execute_wine.clone().context("Supply --execute-wine")?,
                wineserver: self
                    .execute_wineserver
                    .clone()
                    .context("Supply --execute-wineserver")?,
                prefix: self
                    .execute_wine_prefix
                    .clone()
                    .context("Supply --execute-wine-prefix")?,
            }),
        }))
    }
}
