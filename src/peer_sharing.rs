//! Durable enrollment for finished, guide-authored downloads.
//!
//! This module does no discovery or networking. CLI producers publish into a
//! private spool; the application owner consumes it under a separate lock.
use crate::{
    Guide,
    acquisition::{SkipReason, Source, SourceKind},
    archive, download,
    download_orchestrator::{AcquisitionPlan, RunReport, TargetStatus},
    p2p,
    profiles::SettingsFile,
    recovery,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

const VERSION: u32 = 1;
const MAX_RECORD: u64 = 64 * 1024;
const MAX_INFO: usize = 32 * 1024;
pub const PAGE_SIZE: usize = 128;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum State {
    Queued,
    Verifying,
    Sharing,
    Stopping,
    Paused,
    Finished,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    /// None is an in-memory guide. Such a record cannot auto-resume after exit.
    pub guide_path: Option<PathBuf>,
    pub component_guid: String,
    /// Hash of the registry key, so authenticated URL queries are never retained.
    pub registry_key_sha256: String,
    /// Hash of authored SHA-256, magnet and TorrentInfo, including their presence.
    pub authorization_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub version: u32,
    pub identity: String,
    pub root: PathBuf,
    pub path: PathBuf,
    pub archive_name: String,
    pub authored_sha256: String,
    /// Only an xt and a safe display name; no tracker, credential or query data.
    pub magnet: String,
    pub info_hash: String,
    pub torrent_info: Option<String>,
    pub max_expanded_bytes: Option<u64>,
    pub provenance: Provenance,
    pub state: State,
    pub reason: Option<String>,
    pub active_millis: u64,
    pub uploaded_bytes: u64,
    /// Set before starting transport; an unclean exit must pause on restart.
    pub in_flight: bool,
}

impl Record {
    pub fn exhausted(&self) -> bool {
        self.state == State::Finished
    }
    pub fn archive_options(&self) -> archive::Options {
        archive::Options {
            scratch_dir: None,
            max_expanded_bytes: self.max_expanded_bytes,
        }
    }
    pub fn decoded_info(&self) -> Result<Option<Vec<u8>>> {
        self.torrent_info
            .as_deref()
            .map(p2p::base64_decode)
            .transpose()
    }
}

pub struct OwnerLock(File);
impl Drop for OwnerLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}
struct TransactionLock(File);
impl Drop for TransactionLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

#[derive(Clone, Debug)]
pub struct Catalog {
    root: PathBuf,
}

#[derive(Default, Debug)]
pub struct Enrollment {
    pub admitted: usize,
    pub duplicates: usize,
    pub rejected: usize,
    pub failures: Vec<String>,
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    ensure!(
        !fs::symlink_metadata(path)?.file_type().is_symlink(),
        "Sharing storage is a symlink: {}",
        path.display()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn lock_file(path: &Path) -> Result<File> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        ensure!(
            meta.file_type().is_file() && !meta.file_type().is_symlink(),
            "Unsafe sharing lock: {}",
            path.display()
        );
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

fn durable_write(path: &Path, bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() as u64 <= MAX_RECORD,
        "Sharing record exceeds {MAX_RECORD} bytes"
    );
    recovery::atomic_write(path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    File::open(path.parent().context("Sharing path has no parent")?)?.sync_all()?;
    Ok(())
}

fn read_record(path: &Path, identity: &str) -> Result<Record> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.file_type().is_file() && meta.len() <= MAX_RECORD,
        "Unsafe or oversized sharing record: {}",
        path.display()
    );
    let bytes = fs::read(path)?;
    let record: Record = serde_json::from_slice(&bytes)
        .with_context(|| format!("Malformed sharing record: {}", path.display()))?;
    ensure!(
        record.version == VERSION
            && record.identity == identity
            && record
                .torrent_info
                .as_ref()
                .is_none_or(|i| i.len() <= MAX_INFO * 4 / 3 + 4),
        "Unsupported or inconsistent sharing record: {}",
        path.display()
    );
    Ok(record)
}

impl Catalog {
    pub fn open(settings: &SettingsFile) -> Result<Self> {
        fs::create_dir_all(&settings.directory)?;
        ensure!(
            !fs::symlink_metadata(&settings.directory)?
                .file_type()
                .is_symlink(),
            "Settings directory is a symlink"
        );
        let root = settings.directory.join("peer-sharing-v1");
        private_dir(&root)?;
        private_dir(&root.join("pending"))?;
        private_dir(&root.join("records"))?;
        Ok(Self { root })
    }

    fn transaction(&self) -> Result<TransactionLock> {
        let file = lock_file(&self.root.join("transaction.lock"))?;
        fs2::FileExt::lock_exclusive(&file)?;
        Ok(TransactionLock(file))
    }

    /// Hold this for the desktop owner's whole lifetime. Producers never take it.
    pub fn try_owner(&self) -> Result<Option<OwnerLock>> {
        let file = lock_file(&self.root.join("owner.lock"))?;
        match fs2::FileExt::try_lock_exclusive(&file) {
            Ok(()) => Ok(Some(OwnerLock(file))),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn path(&self, dir: &str, identity: &str) -> Result<PathBuf> {
        ensure!(
            identity.len() == 64 && identity.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid sharing identity"
        );
        Ok(self.root.join(dir).join(format!("{identity}.json")))
    }

    /// Atomic, lossless pending admission. A duplicate does not rewrite a
    /// catalog entry, pending entry, pause, or cumulative accounting.
    pub fn admit(&self, record: &Record) -> Result<bool> {
        ensure!(
            record.version == VERSION,
            "Unsupported sharing record version"
        );
        let bytes = serde_json::to_vec(record)?;
        let _tx = self.transaction()?;
        let pending = self.path("pending", &record.identity)?;
        let existing = self.path("records", &record.identity)?;
        if existing.exists() {
            read_record(&existing, &record.identity)?;
            return Ok(false);
        }
        if pending.exists() {
            read_record(&pending, &record.identity)?;
            return Ok(false);
        }
        durable_write(&pending, &bytes)?;
        Ok(true)
    }

    /// Move up to one bounded page. A crash after catalog commit replays the
    /// pending file as a duplicate; an error leaves pending data recoverable.
    pub fn consume_page(&self) -> Result<usize> {
        let _tx = self.transaction()?;
        // Interrupted atomic writes leave hidden temporary files; they are not
        // records and must not fill the bounded page.
        let mut names = fs::read_dir(self.root.join("pending"))?
            .filter(|entry| !matches!(entry, Ok(entry) if entry.file_name().to_string_lossy().starts_with('.')))
            .take(PAGE_SIZE)
            .collect::<std::io::Result<Vec<_>>>()?;
        names.sort_by_key(|entry| entry.file_name());
        let mut consumed = 0;
        for entry in names.into_iter().take(PAGE_SIZE) {
            let name = entry.file_name();
            let name = name.to_str().context("Non-UTF-8 pending record name")?;
            let identity = name
                .strip_suffix(".json")
                .context("Unexpected pending record")?;
            let pending = self.path("pending", identity)?;
            let record = read_record(&pending, identity)?;
            let target = self.path("records", identity)?;
            if target.exists() {
                read_record(&target, identity)?;
            } else {
                durable_write(&target, &serde_json::to_vec(&record)?)?;
            }
            fs::remove_file(&pending)?;
            File::open(self.root.join("pending"))?.sync_all()?;
            consumed += 1;
        }
        Ok(consumed)
    }

    /// Bounded page for a disk-backed queue. Cursor is the last identity seen.
    pub fn page(&self, after: Option<&str>) -> Result<Vec<Record>> {
        let mut entries = BTreeMap::new();
        for entry in fs::read_dir(self.root.join("records"))? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-UTF-8 catalog record name"))?;
            if name.starts_with('.') {
                continue;
            }
            if after.is_some_and(|cursor| name.as_str() <= cursor) {
                continue;
            }
            ensure!(
                name.strip_suffix(".json")
                    .is_some_and(|identity| identity.len() == 64
                        && identity.bytes().all(|b| b.is_ascii_hexdigit())),
                "Unexpected catalog record: {name}"
            );
            entries.insert(name, entry.path());
            if entries.len() > PAGE_SIZE {
                entries.pop_last();
            }
        }
        entries
            .into_iter()
            .map(|(name, path)| {
                let identity = name
                    .strip_suffix(".json")
                    .context("Unexpected catalog record")?;
                read_record(&path, identity)
            })
            .collect()
    }

    pub fn update(&self, record: &Record) -> Result<()> {
        let _tx = self.transaction()?;
        let path = self.path("records", &record.identity)?;
        read_record(&path, &record.identity)?;
        durable_write(&path, &serde_json::to_vec(record)?)
    }
}

fn authored_metadata<'a>(guide: &'a Guide, component_guid: &str, url: &str) -> Option<&'a Value> {
    guide
        .components
        .iter()
        .find(|component| component.guid.eq_ignore_ascii_case(component_guid))?
        .resource_registry
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(url))
        .map(|(_, value)| value)
}

fn authorization(metadata: &Value) -> Result<(String, String, String, Option<Vec<u8>>, String)> {
    let hash = metadata
        .get("ContentHashSHA256")
        .and_then(Value::as_str)
        .context("No authored SHA-256")?
        .trim()
        .to_ascii_lowercase();
    ensure!(
        hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid authored SHA-256"
    );
    let raw_magnet = metadata
        .get("Magnet")
        .and_then(Value::as_str)
        .context("No authored magnet")?;
    let magnet = p2p::sanitize_magnet(raw_magnet, &[])?;
    let info = metadata
        .get("TorrentInfo")
        .map(|value| value.as_str().context("Malformed TorrentInfo"))
        .transpose()?
        .map(p2p::base64_decode)
        .transpose()?;
    ensure!(
        info.as_ref().is_none_or(|bytes| bytes.len() <= MAX_INFO),
        "TorrentInfo exceeds size limit"
    );
    let name = match &info {
        Some(bytes) => p2p::check_torrent_info(bytes, &magnet.info_hash)?.name,
        None => magnet
            .name
            .clone()
            .context("Authored magnet needs a safe display name")?,
    };
    download::one_component(&name)?;
    let fingerprint = sha256(&serde_json::to_vec(&(
        hash.as_str(),
        raw_magnet,
        metadata.get("TorrentInfo"),
    ))?);
    let safe_magnet = format!(
        "magnet:?xt=urn:btih:{}&dn={}",
        magnet.info_hash,
        reqwest::Url::parse_with_params("https://localhost/", &[("dn", name.as_str())])?
            .query()
            .unwrap_or("")
            .trim_start_matches("dn=")
    );
    Ok((hash, safe_magnet, magnet.info_hash, info, fingerprint))
}

fn file_hash(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut block = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut block)?;
        if n == 0 {
            return Ok(format!("{:x}", digest.finalize()));
        }
        digest.update(&block[..n]);
    }
}

fn make_record(
    guide: &Guide,
    plan: &AcquisitionPlan,
    component_guid: &str,
    url: &str,
    path: &Path,
    options: &archive::Options,
    guide_path: Option<&Path>,
) -> Result<Record> {
    let metadata = authored_metadata(guide, component_guid, url)
        .context("Resource is not in captured guide")?;
    let (hash, magnet, info_hash, info, fingerprint) = authorization(metadata)?;
    let root = fs::canonicalize(&plan.source_dir).context("Missing captured source root")?;
    let path = fs::canonicalize(path).context("Missing completed archive")?;
    ensure!(
        path.parent() == Some(root.as_path()) && path.is_file(),
        "Completed file is outside captured source root"
    );
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .context("Non-UTF-8 archive name")?;
    download::one_component(name)?;
    ensure!(
        file_hash(&path)? == hash,
        "Completed archive differs from authored SHA-256"
    );
    archive::verify(&path, &path, options, &AtomicBool::new(false))?;
    let identity = sha256(&serde_json::to_vec(&(&root, &path, &hash, &info_hash))?);
    let guide_path = guide_path.map(fs::canonicalize).transpose()?;
    Ok(Record {
        version: VERSION,
        identity,
        root,
        path,
        archive_name: name.to_owned(),
        authored_sha256: hash,
        magnet,
        info_hash,
        torrent_info: info.as_deref().map(p2p::base64_encode),
        max_expanded_bytes: options.max_expanded_bytes,
        provenance: Provenance {
            guide_path,
            component_guid: component_guid.to_owned(),
            registry_key_sha256: sha256(url.as_bytes()),
            authorization_sha256: fingerprint,
        },
        state: State::Queued,
        reason: None,
        active_millis: 0,
        uploaded_bytes: 0,
        in_flight: false,
    })
}

/// Production verification and durable admission from a captured, accepted run.
/// Rejected candidates do not become sharing records; storage failures are
/// returned separately so the caller can report sharing enrollment failure.
pub fn enroll_report(
    catalog: &Catalog,
    guide: &Guide,
    plan: &AcquisitionPlan,
    report: &RunReport,
    options: &archive::Options,
    guide_path: Option<&Path>,
    accepted: bool,
) -> Enrollment {
    let mut result = Enrollment::default();
    if !accepted
        || report.cancelled
        || plan.peer_mode == crate::download_orchestrator::PeerMode::Off
    {
        return result;
    }
    for outcome in &report.outcomes {
        if outcome.status != TargetStatus::Completed
            || outcome.shared_with.is_some()
            || !matches!(
                outcome.source,
                Some(SourceKind::Origin(_) | SourceKind::Peer | SourceKind::Existing)
            )
        {
            result.rejected += 1;
            continue;
        }
        let Some(target) = plan.targets.get(outcome.target_index) else {
            result.rejected += 1;
            continue;
        };
        if target.component_guid != outcome.component_guid
            || target.registry_url != outcome.registry_url
            || !target
                .sources
                .iter()
                .any(|source| matches!(source, Source::Peer { .. }))
        {
            result.rejected += 1;
            continue;
        }
        let Some(path) = &outcome.path else {
            result.rejected += 1;
            continue;
        };
        match make_record(guide, plan, &outcome.component_guid, &outcome.registry_url, path, options, guide_path) {
            Ok(record) if outcome.sha256.as_ref().is_some_and(|hash| hash.eq_ignore_ascii_case(&record.authored_sha256))
                && target.expected_sha256.as_ref().is_some_and(|hash| hash.eq_ignore_ascii_case(&record.authored_sha256))
                && outcome.filename.as_deref() == Some(record.archive_name.as_str())
                && target.filename.as_deref().is_none_or(|name| name == record.archive_name)
                && target.sources.iter().any(|source| matches!(source, Source::Peer { magnet, sha256, .. }
                    if sha256.eq_ignore_ascii_case(&record.authored_sha256)
                    && p2p::sanitize_magnet(magnet, &[]).is_ok_and(|m| m.info_hash == record.info_hash))) => match catalog.admit(&record) {
                Ok(true) => result.admitted += 1,
                Ok(false) => result.duplicates += 1,
                Err(error) => result.failures.push(format!("{}: {error:#}", record.identity)),
            },
            _ => result.rejected += 1,
        }
    }
    for skipped in &report.skipped {
        if skipped.reason != SkipReason::AlreadyPresent {
            continue;
        }
        if !plan.skipped.iter().any(|planned| planned == skipped) {
            result.rejected += 1;
            continue;
        }
        let Some(path) = &skipped.path else {
            result.rejected += 1;
            continue;
        };
        match make_record(
            guide,
            plan,
            &skipped.component_guid,
            &skipped.registry_url,
            path,
            options,
            guide_path,
        ) {
            Ok(record) => match catalog.admit(&record) {
                Ok(true) => result.admitted += 1,
                Ok(false) => result.duplicates += 1,
                Err(error) => result
                    .failures
                    .push(format!("{}: {error:#}", record.identity)),
            },
            Err(_) => result.rejected += 1,
        }
    }
    result
}

/// Restart admission is independent of cached computed hashes. A changed or
/// missing guide, source, archive, or unclean in-flight record pauses visibly.
pub fn restart_decision(record: &Record) -> Result<(), String> {
    if record.in_flight {
        return Err("Unclean sharing stop; explicit Resume required".into());
    }
    if matches!(
        record.state,
        State::Paused | State::Finished | State::Failed
    ) {
        return Err(record
            .reason
            .clone()
            .unwrap_or_else(|| "Sharing is paused or exhausted".into()));
    }
    let guide_path = record
        .provenance
        .guide_path
        .as_ref()
        .ok_or("Unsaved guide requires revalidation after restart")?;
    let guide = Guide::load_preserving_selection(guide_path)
        .map_err(|e| format!("Saved guide unavailable: {e:#}"))?;
    let metadata = guide
        .components
        .iter()
        .find(|component| {
            component
                .guid
                .eq_ignore_ascii_case(&record.provenance.component_guid)
        })
        .and_then(|component| {
            component
                .resource_registry
                .iter()
                .find(|(url, _)| sha256(url.as_bytes()) == record.provenance.registry_key_sha256)
        })
        .map(|(_, value)| value)
        .ok_or("Guide no longer authorizes this resource")?;
    let (hash, _, info_hash, _, fingerprint) =
        authorization(metadata).map_err(|e| format!("Changed guide authorization: {e:#}"))?;
    if hash != record.authored_sha256
        || info_hash != record.info_hash
        || fingerprint != record.provenance.authorization_sha256
    {
        return Err("Guide authorization changed".into());
    }
    let root = fs::canonicalize(&record.root).map_err(|e| format!("Source root missing: {e}"))?;
    let path =
        fs::canonicalize(&record.path).map_err(|e| format!("Source archive missing: {e}"))?;
    if path.parent() != Some(root.as_path()) {
        return Err("Source archive moved outside captured root".into());
    }
    if file_hash(&path).map_err(|e| e.to_string())? != hash {
        return Err("Source archive hash changed".into());
    }
    archive::verify(
        &path,
        &path,
        &record.archive_options(),
        &AtomicBool::new(false),
    )
    .map_err(|e| format!("Source archive invalid: {e:#}"))?;
    Ok(())
}
