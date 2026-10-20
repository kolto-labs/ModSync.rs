use crate::{download::LoopbackCa, profiles, single_instance};
use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::Receiver,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toolkit {
    Qt,
    Egui,
}

impl Toolkit {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Qt => "qt",
            Self::Egui => "egui",
        }
    }

    pub const fn compiled() -> Option<Self> {
        if cfg!(feature = "ui-qt") {
            Some(Self::Qt)
        } else if cfg!(feature = "ui-egui") {
            Some(Self::Egui)
        } else {
            None
        }
    }
}

/// Hidden probe launch context: explicit key, API base and loopback CA only.
/// Never persisted, and never combined with the per-user instance socket.
pub struct Probe {
    pub nexus_api_key: Option<String>,
    pub nexus_api_base: Option<String>,
    pub extra_ca_file: Option<PathBuf>,
}

pub struct ProbeContext {
    pub nexus_api_key: Option<String>,
    pub nexus_api_base: Option<String>,
    pub loopback_ca: Option<LoopbackCa>,
}

pub struct LaunchRequest {
    pub input: Option<PathBuf>,
    pub game: Option<PathBuf>,
    pub mods: Option<PathBuf>,
    pub settings_dir: Option<PathBuf>,
    pub protocol: Option<String>,
    pub allow_multiple_instances: bool,
    pub probe: Option<Probe>,
    pub easy_session: Option<PathBuf>,
}

pub struct PreparedLaunch {
    pub input: Option<PathBuf>,
    pub game: Option<PathBuf>,
    pub mods: Option<PathBuf>,
    pub settings_dir: Option<PathBuf>,
    pub protocol: Option<String>,
    pub probe: Option<ProbeContext>,
    pub easy_session: Option<PathBuf>,
    pub primary: Option<(Receiver<String>, single_instance::Guard)>,
}

pub trait UiBackend {
    const TOOLKIT: Toolkit;

    fn run(request: LaunchRequest) -> Result<()>;
}

pub fn prepare(request: LaunchRequest) -> Result<Option<PreparedLaunch>> {
    let LaunchRequest {
        input,
        game,
        mods,
        settings_dir,
        protocol,
        allow_multiple_instances,
        probe,
        easy_session,
    } = request;
    let probe = match probe {
        Some(probe) => Some(probe_context(probe, settings_dir.as_deref())?),
        None => None,
    };
    let separate = allow_multiple_instances || probe.is_some() || easy_session.is_some();
    let primary = if separate {
        None
    } else {
        let identity = crate::protocol_registration::package_identity(&std::env::current_exe()?)?;
        single_instance::claim_or_forward_for_channel(protocol.as_deref(), &identity.channel)?
    };
    if primary.is_none() && !separate {
        return Ok(None);
    }
    Ok(Some(PreparedLaunch {
        input,
        game,
        mods,
        settings_dir,
        protocol,
        probe,
        easy_session,
        primary,
    }))
}

/// Canonicalizes the nearest existing ancestor of `path` and re-appends the missing
/// rest, so a not-yet-created directory still compares through resolved aliases.
pub fn canonical_with_missing_tail(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut existing = absolute.as_path();
    let mut tail = Vec::new();
    loop {
        if let Ok(resolved) = std::fs::canonicalize(existing) {
            return tail.iter().rev().fold(resolved, |acc, part| acc.join(part));
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name.to_owned());
                existing = parent;
            }
            _ => return absolute,
        }
    }
}

/// Mirrors the CLI's probe rules: an API base needs an explicit nonblank key, the CA is
/// per invocation, and the settings directory must be explicit, existing and neither the
/// default settings directory nor nested with it, compared after resolving aliases.
pub fn probe_context(probe: Probe, settings_dir: Option<&Path>) -> Result<ProbeContext> {
    let directory = settings_dir
        .filter(|path| !path.as_os_str().is_empty())
        .context("Probe options require --settings-dir with a private settings directory")?;
    let resolved = std::fs::canonicalize(directory)
        .context("The probe settings directory must be an existing directory")?;
    ensure!(
        resolved.is_dir(),
        "The probe settings directory must be an existing directory"
    );
    if let Ok(default) = profiles::settings_directory(None) {
        let default = canonical_with_missing_tail(&default);
        ensure!(
            !resolved.starts_with(&default) && !default.starts_with(&resolved),
            "Probe options require a settings directory separate from the default settings directory"
        );
    }
    let nexus_api_key = probe.nexus_api_key.filter(|key| !key.trim().is_empty());
    ensure!(
        probe.nexus_api_base.is_none() || nexus_api_key.is_some(),
        "--nexus-api-base requires an explicit --nexus-mods-api-key"
    );
    let loopback_ca = probe
        .extra_ca_file
        .as_deref()
        .map(LoopbackCa::from_pem_file)
        .transpose()?;
    Ok(ProbeContext {
        nexus_api_key,
        nexus_api_base: probe.nexus_api_base,
        loopback_ca,
    })
}
