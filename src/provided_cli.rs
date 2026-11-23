//! Shared explicit configuration for the supplied GUI installer runner.
use anyhow::{Result, ensure};
use clap::Args;
use modsync::engine::{ProvidedInstallerRunner, WindowsProcessRunner};
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct ProvidedArgs {
    /// Private existing request storage; explicitly selects supplied installers.
    #[arg(long, requires = "provided_xvfb", conflicts_with = "patcher")]
    provided_request_dir: Option<PathBuf>,
    /// Installed Xvfb executable; an owned display is allocated per launch.
    #[arg(long, requires = "provided_request_dir")]
    provided_xvfb: Option<PathBuf>,
    #[arg(long, requires_all = ["provided_request_dir", "provided_wineserver", "provided_wine_prefix"])]
    provided_wine: Option<PathBuf>,
    #[arg(long, requires = "provided_wine")]
    provided_wineserver: Option<PathBuf>,
    #[arg(long, requires = "provided_wine")]
    provided_wine_prefix: Option<PathBuf>,
}

impl ProvidedArgs {
    pub(crate) fn resolve(self, allow_processes: bool) -> Result<Option<ProvidedInstallerRunner>> {
        let Some(request_dir) = self.provided_request_dir else {
            return Ok(None);
        };
        ensure!(
            allow_processes,
            "Supplied installer execution requires --allow-processes"
        );
        let windows = match (
            self.provided_wine,
            self.provided_wineserver,
            self.provided_wine_prefix,
        ) {
            (None, None, None) => None,
            (Some(executable), Some(wineserver), Some(prefix)) => Some(WindowsProcessRunner {
                executable,
                wineserver,
                prefix,
            }),
            _ => anyhow::bail!(
                "Configure --provided-wine, --provided-wineserver and --provided-wine-prefix together"
            ),
        };
        Ok(Some(ProvidedInstallerRunner {
            request_dir,
            xvfb: self
                .provided_xvfb
                .ok_or_else(|| anyhow::anyhow!("Supplied runner requires --provided-xvfb"))?,
            windows,
        }))
    }
}
