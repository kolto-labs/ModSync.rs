//! Concrete Peer acquisition source: `Acquire` over `p2p::fetch_authored`, with per-run
//! policy caching and authored `TorrentInfo` looked up by component GUID and registry URL.
//! One adapter is built per run and shared (it is already handed to the orchestrator behind
//! `Arc<dyn Acquire>`), so its policy is bootstrapped at most once, lazily, from the first
//! eligible `acquire` call: never for a dry run, peer-off, an existing-only target, invalid
//! metadata, a pre-cancelled request, or a gated Origin (the orchestrator never calls in
//! only those cases).
use crate::{
    Guide,
    acquisition::{Acquire, Acquired, AcquisitionEvent, ArchiveTarget, Phase, Source, SourceKind},
    archive, download,
    p2p::{self, AuthoredTorrent, PeerPolicy, PeerProgress, PeerSettings},
};
use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

/// A file name that is safe to create inside a ModSync folder: one path component, no
/// separators, no drive or parent references. Mirrors `p2p`'s private `safe_file_name`
/// (built from the same public `paths::relative`), since that helper is not exported.
fn safe_component_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.contains(['/', '\\'])
        && crate::paths::relative(name, false)
            .is_ok_and(|path| path.as_os_str() == std::ffi::OsStr::new(name))
}

/// A present authored `TorrentInfo` must be a nonempty base64 string. Preserve
/// invalid values so the affected target fails before policy bootstrap.
enum TorrentInfoEntry {
    Base64(String),
    Invalid,
}

pub struct PeerAcquire {
    settings: PeerSettings,
    archive: archive::Options,
    cache_dir: Option<PathBuf>,
    /// `TorrentInfo`, keyed by `(component_guid, registry_url)`. Only top-level components
    /// carry acquisition resources (`selection::download_candidates`), so only their
    /// `resource_registry` is indexed; the guide itself is not retained.
    torrent_info: HashMap<(String, String), TorrentInfoEntry>,
    /// Built at most once per run; `None` when sharing/mode is off.
    policy: OnceLock<Option<PeerPolicy>>,
}

impl PeerAcquire {
    pub fn new(
        guide: &Guide,
        settings: PeerSettings,
        archive: archive::Options,
        cache_dir: Option<PathBuf>,
    ) -> Self {
        let mut torrent_info = HashMap::new();
        for component in &guide.components {
            for (url, metadata) in &component.resource_registry {
                let Some(value) = metadata.get("TorrentInfo") else {
                    continue;
                };
                let entry = match value.as_str() {
                    Some(s) if s.trim().is_empty() => TorrentInfoEntry::Invalid,
                    Some(s) => TorrentInfoEntry::Base64(s.trim().to_owned()),
                    None => TorrentInfoEntry::Invalid,
                };
                torrent_info.insert((component.guid.clone(), url.clone()), entry);
            }
        }
        Self {
            settings,
            archive,
            cache_dir,
            torrent_info,
            policy: OnceLock::new(),
        }
    }

    /// The run's cached policy. `None` (sharing/mode off) is remembered the same way as a
    /// policy: fail-closed, both stick for the rest of the run.
    fn policy(&self) -> Option<&PeerPolicy> {
        self.policy
            .get_or_init(|| self.settings.policy(self.cache_dir.clone()))
            .as_ref()
    }
}

impl Acquire for PeerAcquire {
    fn kind(&self) -> SourceKind {
        SourceKind::Peer
    }

    fn acquire(
        &self,
        target: &ArchiveTarget,
        destination: &Path,
        cancelled: &AtomicBool,
        on: &mut dyn FnMut(AcquisitionEvent),
    ) -> Result<Acquired> {
        // P2: cancellation, then metadata validity and path/name safety, all before any
        // policy (and so any network) is touched.
        if cancelled.load(Ordering::SeqCst) {
            return Err(download::cancelled(anyhow::anyhow!(
                "Peer acquisition cancelled before start"
            )));
        }
        let Some(Source::Peer { magnet, sha256, .. }) = target
            .sources
            .iter()
            .find(|s| matches!(s, Source::Peer { .. }))
        else {
            bail!("Peer adapter invoked without a Peer source");
        };
        // Pure string parsing: extracts the BTv1 infohash without touching the network or
        // building a policy. The same call happens again inside `fetch_authored` with the
        // real tracker set; this one is only to validate `TorrentInfo` early.
        let precheck = p2p::sanitize_magnet(magnet, &[]).context("Invalid Peer magnet")?;
        let torrent_info_bytes = match self
            .torrent_info
            .get(&(target.component_guid.clone(), target.registry_url.clone()))
        {
            None => None,
            Some(TorrentInfoEntry::Invalid) => {
                bail!("TorrentInfo must be a nonempty base64 string")
            }
            Some(TorrentInfoEntry::Base64(b64)) => {
                Some(p2p::base64_decode(b64).context("TorrentInfo is not base64")?)
            }
        };
        let checked = torrent_info_bytes
            .as_deref()
            .map(|bytes| p2p::check_torrent_info(bytes, &precheck.info_hash))
            .transpose()
            .context("TorrentInfo is invalid")?;

        let destination_file = match &target.filename {
            // Known filename: `destination` is already the exact file path (one_component
            // validated by `ArchiveTarget::destination`/`PeerStaging::create`). Never join a
            // second name to it.
            Some(name) => {
                download::one_component(name)?;
                destination.to_path_buf()
            }
            // Unknown filename: `destination` is a directory. Derive one safe path component
            // from the checked info dictionary's name, else a sanitized magnet `dn`.
            None => {
                let name = match &checked {
                    Some(checked) => checked.name.clone(),
                    None => {
                        let dn = precheck.name.as_deref().context(
                            "Peer target has no known file name and no usable magnet dn",
                        )?;
                        ensure!(safe_component_name(dn), "Magnet dn is not a safe file name");
                        dn.to_owned()
                    }
                };
                destination.join(name)
            }
        };

        // The name is known before any transfer: authored, or learned from the torrent.
        // When it is already present (for a known name, because an ungrouped target
        // published it first), nothing is fetched or replaced: the orchestrator adopts
        // the present file only if it has the authored digest, which every Peer copy
        // must have. (In a race `destination` is in an empty staging folder instead, and
        // `PeerStaging::publish` settles the final name the same way.)
        let present = || {
            anyhow::Error::new(download::DestinationExists {
                path: destination_file.clone(),
                sha256: sha256.to_ascii_lowercase(),
            })
        };
        if std::fs::symlink_metadata(&destination_file).is_ok() {
            return Err(present());
        }

        // P3: policy is bootstrapped only now, from this eligible call.
        let policy = self
            .policy()
            .context("Peer sharing is off or the tunnel is unavailable")?;

        let known_filename = target.filename.clone().or_else(|| {
            destination_file
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_owned)
        });
        let mut emit = |phase: Phase| {
            on(AcquisitionEvent {
                target_index: 0,
                component_guid: target.component_guid.clone(),
                filename: known_filename.clone(),
                source: SourceKind::Peer,
                phase,
            });
        };
        let authored = AuthoredTorrent {
            magnet,
            torrent_info: torrent_info_bytes.as_deref(),
        };
        let result = p2p::fetch_authored(
            authored,
            &destination_file,
            sha256,
            policy,
            cancelled,
            &self.archive,
            |progress| match progress {
                PeerProgress::Transferring {
                    bytes,
                    total,
                    peers,
                    ..
                } => {
                    emit(Phase::Transferring {
                        bytes,
                        total,
                        peers,
                    });
                }
                PeerProgress::Verifying => emit(Phase::Verifying),
                // P4: proxy endpoints never reach acquisition events; the current schema
                // has no proxy-rotation phase either.
                PeerProgress::ProxyRotated { .. } => {}
            },
        );
        match result {
            Ok(sha256) => Ok(Acquired {
                path: destination_file,
                sha256,
            }),
            Err(error) if cancelled.load(Ordering::SeqCst) => Err(download::cancelled(error)),
            // The name was taken while this copy transferred: its no-clobber move lost.
            Err(error)
                if error.chain().any(|e| {
                    e.downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::AlreadyExists)
                }) && std::fs::symlink_metadata(&destination_file).is_ok() =>
            {
                Err(present())
            }
            Err(error) => Err(error),
        }
    }
}
