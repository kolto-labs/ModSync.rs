//! Shared acquisition types: where an archive can come from and what happened to it.
//!
//! Every frontend (CLI, GUI) formats these facts; none re-derives them. The orchestrator
//! in `download_orchestrator` decides source order, fallback and supersession.
use crate::download_hosts::Provider;
use anyhow::Result;
use serde::Serialize;
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "provider")]
pub enum SourceKind {
    Existing,
    Origin(Provider),
    Peer,
    Manual,
}

/// Why a person must finish this download in a browser.
///
/// `LoginRequired`, `Captcha`, `FreeUserWait` and `HtmlLanding` are gates: the target ends
/// with no Peer fallback, because a peer copy must not get around a host gate.
/// `Unsupported` (host not automated) may fall back to Peer when share-eligible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ManualReason {
    LoginRequired,
    Captcha,
    HtmlLanding,
    FreeUserWait,
    Unsupported,
}

impl ManualReason {
    pub fn is_gate(self) -> bool {
        !matches!(self, Self::Unsupported)
    }
}

/// A peer the tunnel proxy can reach. librqbit's SOCKS path dials a `SocketAddr`.
/// Peers come only from the tunnel's discovery or from entries the user types;
/// guide-authored peer addresses are never honored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct PeerEndpoint(pub SocketAddr);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OriginRequirement {
    /// Direct HTTPS, or an nxm link carrying key+expires.
    None,
    /// A nexusmods.com URL with an API key. `run` validates premium once per run.
    NexusPremium,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Source {
    /// A verified local file satisfies the target.
    Existing { path: PathBuf },
    /// Direct HTTPS or a host API.
    Origin {
        url: String,
        provider: Provider,
        requires: OriginRequirement,
    },
    /// Only with an authored sha256 and magnet; peers from user settings only.
    Peer {
        magnet: String,
        sha256: String,
        peers: Vec<PeerEndpoint>,
    },
    /// Terminal; never automated around.
    Manual { url: String, reason: ManualReason },
}

impl Source {
    pub fn kind(&self) -> SourceKind {
        match self {
            Self::Existing { .. } => SourceKind::Existing,
            Self::Origin { provider, .. } => SourceKind::Origin(*provider),
            Self::Peer { .. } => SourceKind::Peer,
            Self::Manual { .. } => SourceKind::Manual,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveTarget {
    pub component_guid: String,
    /// The `ResourceRegistry` key this target came from; write-back goes here.
    pub registry_url: String,
    /// `None` until an origin reveals it. Always exactly one normal path component.
    pub filename: Option<String>,
    /// `ResourceRegistry[url].ContentHashSHA256`.
    pub expected_sha256: Option<String>,
    /// Already in fallback order.
    pub sources: Vec<Source>,
}

impl ArchiveTarget {
    /// Where an acquirer publishes: the file path when the name is known, otherwise the
    /// directory itself. In the directory case the acquirer names the file after transfer
    /// and applies `download::one_component` before publish.
    pub fn destination(&self, source_dir: &Path) -> PathBuf {
        match &self.filename {
            Some(name) => source_dir.join(name),
            None => source_dir.to_path_buf(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Acquired {
    pub path: PathBuf,
    pub sha256: String,
}

/// One acquisition source. Implementations must remove their temp file on every error path,
/// publish only after verification, and report a gate as `download::FailureClass::ManualRequired`.
pub trait Acquire: Send + Sync {
    fn kind(&self) -> SourceKind;
    fn acquire(
        &self,
        target: &ArchiveTarget,
        destination: &Path,
        cancelled: &AtomicBool,
        on: &mut dyn FnMut(AcquisitionEvent),
    ) -> Result<Acquired>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkipReason {
    AlreadyPresent,
    UrlDisabled,
    AllFilesFlaggedFalse,
    ResolutionMismatch,
    UnsafeFilename,
    InvalidUrl,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "phase")]
pub enum Phase {
    Resolving,
    Transferring {
        bytes: u64,
        total: Option<u64>,
        /// librqbit fact; `None` for Origin.
        peers: Option<u32>,
    },
    Verifying,
    Completed {
        path: PathBuf,
        sha256: String,
    },
    Skipped {
        reason: SkipReason,
    },
    SourceFailed {
        error: String,
        will_fallback: bool,
    },
    /// Decided by the orchestrator, never by error text.
    Superseded,
    ManualRequired {
        url: String,
        reason: ManualReason,
    },
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquisitionEvent {
    pub target_index: usize,
    pub component_guid: String,
    pub filename: Option<String>,
    pub source: SourceKind,
    pub phase: Phase,
}
