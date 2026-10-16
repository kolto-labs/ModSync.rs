//! Durable enrollment for finished, guide-authored downloads.
//!
//! CLI producers publish into a private spool; the application owner consumes
//! it under a separate lock and runs one outbound proxied sharing session.
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
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const VERSION: u32 = 1;
const MAX_RECORD: u64 = 64 * 1024;
pub(crate) const MAX_INFO: usize = 32 * 1024;
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
    #[serde(default)]
    pub verified_length: Option<u64>,
    /// Set before starting transport; an unclean exit must pause on restart.
    pub in_flight: bool,
    #[serde(default)]
    pub deferred_until: u64,
    #[serde(default)]
    pub session_authorization: Option<String>,
    #[serde(default)]
    pub requires_context_approval: bool,
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
    /// Verified identities, including duplicates; used only for explicit
    /// process-local authorization of an unsaved captured guide.
    pub identities: Vec<String>,
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
    sync_dir(path.parent().context("Sharing path has no parent")?)?;
    Ok(())
}

/// Persists a rename in `dir`. Windows cannot open a directory as a file; its
/// rename durability comes from the atomic replace itself.
fn sync_dir(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(dir)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = dir;
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
        // A symlinked settings directory is a legitimate user choice; only the
        // sharing root below it must be a real private directory.
        fs::create_dir_all(&settings.directory)?;
        let root = settings.directory.join("peer-sharing-v1");
        private_dir(&root)?;
        private_dir(&root.join("pending"))?;
        private_dir(&root.join("records"))?;
        private_dir(&root.join("history"))?;
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
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
            {
                Ok(None)
            }
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
        let history = self.path("history", &record.identity)?;
        if history.exists() {
            read_record(&history, &record.identity)?;
            return Ok(false);
        }
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

    /// Publish a verified batch under one transaction. Cancellation or a write
    /// failure removes only this batch's new pending files; existing entries and
    /// their accounting remain intact. The owner cannot consume a partial batch.
    fn admit_batch_cancelled(
        &self,
        records: &[Record],
        cancelled: &AtomicBool,
    ) -> Result<Vec<bool>> {
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "Sharing enrollment cancelled"
        );
        if records.is_empty() {
            return Ok(Vec::new());
        }
        let file = lock_file(&self.root.join("transaction.lock"))?;
        loop {
            ensure!(
                !cancelled.load(Ordering::Acquire),
                "Sharing enrollment cancelled"
            );
            match fs2::FileExt::try_lock_exclusive(&file) {
                Ok(()) => break,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
                {
                    thread::sleep(Duration::from_millis(20));
                }
                Err(error) => return Err(error.into()),
            }
        }
        let _tx = TransactionLock(file);
        let mut created = Vec::new();
        let result = (|| -> Result<Vec<bool>> {
            let mut admissions = Vec::with_capacity(records.len());
            for record in records {
                ensure!(
                    !cancelled.load(Ordering::Acquire),
                    "Sharing enrollment cancelled"
                );
                ensure!(
                    record.version == VERSION,
                    "Unsupported sharing record version"
                );
                let pending = self.path("pending", &record.identity)?;
                let existing = self.path("records", &record.identity)?;
                let history = self.path("history", &record.identity)?;
                if let Some(path) = [&history, &existing, &pending]
                    .into_iter()
                    .find(|path| path.exists())
                {
                    read_record(path, &record.identity)?;
                    admissions.push(false);
                    continue;
                }
                let bytes = serde_json::to_vec(record)?;
                // Track before publication: a directory-sync failure can occur
                // after durable_write has already renamed the pending file.
                created.push(pending.clone());
                durable_write(&pending, &bytes)?;
                admissions.push(true);
            }
            ensure!(
                !cancelled.load(Ordering::Acquire),
                "Sharing enrollment cancelled"
            );
            Ok(admissions)
        })();
        match result {
            Ok(admissions) => Ok(admissions),
            Err(error) => {
                let mut rollback_errors = Vec::new();
                for path in created.into_iter().rev() {
                    match fs::remove_file(&path) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => rollback_errors.push(error.to_string()),
                    }
                }
                if let Err(error) = sync_dir(&self.root.join("pending")) {
                    rollback_errors.push(error.to_string());
                }
                if rollback_errors.is_empty() {
                    Err(error)
                } else {
                    Err(error.context(format!(
                        "Sharing batch rollback failed: {}",
                        rollback_errors.join("; ")
                    )))
                }
            }
        }
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
            let history = self.path("history", identity)?;
            if history.exists() {
                read_record(&history, identity)?;
            } else if target.exists() {
                read_record(&target, identity)?;
            } else {
                let destination = if matches!(
                    record.state,
                    State::Paused | State::Finished | State::Failed
                ) {
                    &history
                } else {
                    &target
                };
                durable_write(destination, &serde_json::to_vec(&record)?)?;
            }
            fs::remove_file(&pending)?;
            sync_dir(&self.root.join("pending"))?;
            consumed += 1;
        }
        Ok(consumed)
    }

    /// Bounded page for a disk-backed queue. Cursor is the last identity seen.
    pub fn page(&self, after: Option<&str>) -> Result<Vec<Record>> {
        self.page_directory("records", after)
    }

    fn page_directory(&self, directory: &str, after: Option<&str>) -> Result<Vec<Record>> {
        let mut entries = BTreeMap::new();
        for entry in fs::read_dir(self.root.join(directory))? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-UTF-8 catalog record name"))?;
            if name.starts_with('.') {
                continue;
            }
            if after.is_some_and(|cursor| name.strip_suffix(".json").unwrap_or(&name) <= cursor) {
                continue;
            }
            ensure!(
                name.strip_suffix(".json")
                    .is_some_and(|identity| identity.len() == 64
                        && identity.bytes().all(|b| b.is_ascii_hexdigit())),
                "Unexpected catalog record: {name}"
            );
            // A crash during records -> history publication can leave both.
            // The durable pause/exhaustion tombstone always wins that conflict.
            if directory == "records" && self.root.join("history").join(&name).exists() {
                continue;
            }
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
        let active = self.path("records", &record.identity)?;
        let history = self.path("history", &record.identity)?;
        read_record(
            if history.exists() { &history } else { &active },
            &record.identity,
        )?;
        let dormant = matches!(
            record.state,
            State::Paused | State::Finished | State::Failed
        );
        let (target, stale) = if dormant {
            (&history, &active)
        } else {
            (&active, &history)
        };
        let mut stored = record.clone();
        if stored.state == State::Finished {
            stored.torrent_info = None;
        }
        durable_write(target, &serde_json::to_vec(&stored)?)?;
        if stale.exists() {
            fs::remove_file(stale)?;
            sync_dir(stale.parent().context("Sharing record has no parent")?)?;
        }
        Ok(())
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

type ShareAuthorization = (String, String, String, Option<Vec<u8>>, String);

fn authorization(metadata: &Value) -> Result<ShareAuthorization> {
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
    file_hash_cancelled(path, &AtomicBool::new(false))
}
fn file_hash_cancelled(path: &Path, cancelled: &AtomicBool) -> Result<String> {
    let mut file = crate::paths::open_regular_read(path)?;
    let mut digest = Sha256::new();
    let mut block = [0u8; 64 * 1024];
    loop {
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "Sharing enrollment cancelled"
        );
        let n = file.read(&mut block)?;
        if n == 0 {
            return Ok(format!("{:x}", digest.finalize()));
        }
        digest.update(&block[..n]);
    }
}

#[allow(clippy::too_many_arguments)]
fn make_record(
    guide: &Guide,
    plan: &AcquisitionPlan,
    component_guid: &str,
    url: &str,
    path: &Path,
    options: &archive::Options,
    guide_path: Option<&Path>,
    cancelled: &AtomicBool,
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
        .context("Non-UTF-8 archive name")?
        .to_owned();
    download::one_component(&name)?;
    ensure!(
        file_hash_cancelled(&path, cancelled)? == hash,
        "Completed archive differs from authored SHA-256"
    );
    archive::verify(&path, &path, options, cancelled)?;
    let verified_length = Some(fs::metadata(&path)?.len());
    let identity = sha256(&serde_json::to_vec(&(&root, &path, &hash, &info_hash))?);
    let guide_path = guide_path.map(fs::canonicalize).transpose()?;
    let requires_context_approval = guide_path.is_none();
    Ok(Record {
        version: VERSION,
        identity,
        root,
        path,
        archive_name: name,
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
        state: if requires_context_approval {
            State::Paused
        } else {
            State::Queued
        },
        reason: requires_context_approval.then(|| "Captured guide approval required".into()),
        active_millis: 0,
        uploaded_bytes: 0,
        verified_length,
        in_flight: false,
        deferred_until: 0,
        session_authorization: None,
        requires_context_approval,
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
    enroll_report_cancelled(
        catalog,
        guide,
        plan,
        report,
        options,
        guide_path,
        accepted,
        &AtomicBool::new(false),
    )
}

/// Cancellable enrollment sibling for desktop background admission. Passing no
/// guide path keeps every new record approval-held until explicit authorization.
#[allow(clippy::too_many_arguments)]
pub fn enroll_report_cancelled(
    catalog: &Catalog,
    guide: &Guide,
    plan: &AcquisitionPlan,
    report: &RunReport,
    options: &archive::Options,
    guide_path: Option<&Path>,
    accepted: bool,
    cancelled: &AtomicBool,
) -> Enrollment {
    let mut result = Enrollment::default();
    let mut records = Vec::new();
    if !accepted
        || report.cancelled
        || plan.peer_mode == crate::download_orchestrator::PeerMode::Off
    {
        return result;
    }
    for outcome in &report.outcomes {
        if cancelled.load(Ordering::Acquire) {
            result.failures.push("Sharing enrollment cancelled".into());
            return result;
        }
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
        match make_record(guide, plan, &outcome.component_guid, &outcome.registry_url, path, options, guide_path, cancelled) {
            Ok(record) if outcome.sha256.as_ref().is_some_and(|hash| hash.eq_ignore_ascii_case(&record.authored_sha256))
                && target.expected_sha256.as_ref().is_some_and(|hash| hash.eq_ignore_ascii_case(&record.authored_sha256))
                && outcome.filename.as_deref() == Some(record.archive_name.as_str())
                && target.filename.as_deref().is_none_or(|name| name == record.archive_name)
                && target.sources.iter().any(|source| matches!(source, Source::Peer { magnet, sha256, .. }
                    if sha256.eq_ignore_ascii_case(&record.authored_sha256)
                    && p2p::sanitize_magnet(magnet, &[]).is_ok_and(|m| m.info_hash == record.info_hash))) => records.push(record),
            _ => result.rejected += 1,
        }
    }
    for skipped in &report.skipped {
        if cancelled.load(Ordering::Acquire) {
            result.failures.push("Sharing enrollment cancelled".into());
            return result;
        }
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
            cancelled,
        ) {
            Ok(record) => records.push(record),
            Err(_) => result.rejected += 1,
        }
    }
    match catalog.admit_batch_cancelled(&records, cancelled) {
        Ok(admissions) => {
            for (record, admitted) in records.into_iter().zip(admissions) {
                if admitted {
                    result.admitted += 1;
                } else {
                    result.duplicates += 1;
                }
                result.identities.push(record.identity);
            }
        }
        Err(error) => result
            .failures
            .push(format!("Sharing enrollment: {error:#}")),
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

const TOTAL_ACTIVE: Duration = Duration::from_secs(24 * 60 * 60);
const QUANTUM: Duration = Duration::from_secs(5 * 60);
const IDLE: Duration = Duration::from_secs(60);
const PREPARATION: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseReason {
    User,
    Settings,
    Mutation,
}
impl PauseReason {
    fn text(self) -> &'static str {
        match self {
            Self::User => "Stopped by user; explicit Resume required",
            Self::Settings => "Sharing disabled in settings; explicit Resume required",
            Self::Mutation => {
                "Source or guide mutation barrier; fresh validation and Resume required"
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct SharingSnapshot {
    pub generation: u64,
    pub identity: Option<String>,
    pub state: State,
    pub reason: Option<String>,
    pub uploaded_bytes: u64,
    pub active_millis: u64,
    pub owner_stopped: bool,
}

pub type Barrier = mpsc::Receiver<std::result::Result<(), String>>;
type Reply = mpsc::Sender<std::result::Result<(), String>>;
enum Command {
    Pause(PauseReason, Reply),
    Resume(Reply),
    ResumeMutation(Reply),
    Authorize(Guide, Vec<String>, Option<PathBuf>, bool, Reply),
    Shutdown,
}

/// One application-lifetime controller and at most one transfer worker. UI
/// callers use request_pause's acknowledgement, never a blocking UI-thread join.
pub struct SharingOwner {
    commands: mpsc::Sender<Command>,
    active_cancel: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    interrupted: Arc<AtomicBool>,
    snapshot: Arc<Mutex<SharingSnapshot>>,
    join: Option<JoinHandle<()>>,
    terminal_error: Arc<Mutex<Option<String>>>,
}

impl SharingOwner {
    pub fn start(settings: SettingsFile, generation: u64) -> Result<Option<Self>> {
        Self::start_with_pause(settings, generation, false)
    }

    /// Desktop startup handshake: no transport may start before the UI has the
    /// owner handle and decides whether a pending Stop/Off/close takes priority.
    pub fn start_paused(settings: SettingsFile, generation: u64) -> Result<Option<Self>> {
        Self::start_with_pause(settings, generation, true)
    }

    fn start_with_pause(
        settings: SettingsFile,
        generation: u64,
        initially_paused: bool,
    ) -> Result<Option<Self>> {
        let catalog = Catalog::open(&settings)?;
        let Some(lock) = catalog.try_owner()? else {
            return Ok(None);
        };
        let (commands, receiver) = mpsc::channel();
        let active_cancel = Arc::new(Mutex::new(None));
        let interrupted = Arc::new(AtomicBool::new(initially_paused));
        let shared_interrupted = interrupted.clone();
        let snapshot = Arc::new(Mutex::new(SharingSnapshot {
            generation,
            identity: None,
            state: State::Queued,
            reason: None,
            uploaded_bytes: 0,
            active_millis: 0,
            owner_stopped: false,
        }));
        let terminal_error = Arc::new(Mutex::new(None));
        let shared_terminal_error = terminal_error.clone();
        let shared_cancel = active_cancel.clone();
        let shared_snapshot = snapshot.clone();
        let join = thread::Builder::new()
            .name("finished-sharing-owner".into())
            .spawn(move || {
                let _lock = lock;
                let mut controller = Controller::new(
                    settings,
                    catalog,
                    receiver,
                    shared_cancel,
                    shared_interrupted,
                    shared_snapshot.clone(),
                );
                if initially_paused {
                    controller.paused = Some(PauseReason::Mutation.text().into());
                }
                if let Err(error) = controller.run() {
                    // Joining in Drop preserves copy/session ownership even on a
                    // catalog failure; in-flight records remain crash-paused.
                    controller.cancel_transfer();
                    controller.join_transfer();
                    let mut state = shared_snapshot.lock().unwrap();
                    state.state = State::Failed;
                    let message = format!("Sharing owner stopped: {error:#}");
                    state.reason = Some(message.clone());
                    *shared_terminal_error.lock().unwrap() = Some(message);
                }
                shared_snapshot.lock().unwrap().owner_stopped = true;
            })?;
        Ok(Some(Self {
            commands,
            active_cancel,
            interrupted,
            snapshot,
            join: Some(join),
            terminal_error,
        }))
    }

    pub fn snapshot(&self) -> SharingSnapshot {
        self.snapshot.lock().unwrap().clone()
    }

    fn cancel(&self) {
        self.interrupted.store(true, Ordering::Release);
        if let Some(cancelled) = self.active_cancel.lock().unwrap().as_ref() {
            cancelled.store(true, Ordering::Release);
            self.snapshot.lock().unwrap().state = State::Stopping;
        }
    }

    pub fn request_pause(&self, reason: PauseReason) -> Barrier {
        self.cancel();
        let (send, receive) = mpsc::channel();
        let _ = self.commands.send(Command::Pause(reason, send));
        receive
    }

    pub fn request_resume(&self) -> Barrier {
        let (send, receive) = mpsc::channel();
        let _ = self.commands.send(Command::Resume(send));
        receive
    }

    /// Finish an accepted acquisition metadata handoff without undoing a user
    /// Stop or Settings pause. Only mutation-paused eligible records are released.
    pub fn request_resume_after_mutation(&self) -> Barrier {
        let (send, receive) = mpsc::channel();
        let _ = self.commands.send(Command::ResumeMutation(send));
        receive
    }

    /// Must follow a settled mutation barrier when the displayed guide changes.
    /// The captured guide authorizes only identities verified by this enrollment.
    pub fn authorize_enrollment(&self, guide: &Guide, enrollment: &Enrollment) -> Barrier {
        self.authorize_enrollment_from(guide, enrollment, None)
    }

    /// Promote deferred desktop enrollment only after the caller has checked
    /// its captured UI context. A saved path must still authorize every record.
    pub fn authorize_enrollment_from(
        &self,
        guide: &Guide,
        enrollment: &Enrollment,
        guide_path: Option<PathBuf>,
    ) -> Barrier {
        self.cancel();
        let (send, receive) = mpsc::channel();
        let _ = self.commands.send(Command::Authorize(
            guide.clone(),
            enrollment.identities.clone(),
            guide_path,
            false,
            send,
        ));
        receive
    }

    /// Saved promotion is scoped to identities still authorized by this guide.
    /// Old admissions from a previous guide are ignored; malformed catalog records
    /// and an eligible record missing from the saved guide remain visible failures.
    pub fn promote_saved_enrollment(
        &self,
        guide: &Guide,
        enrollment: &Enrollment,
        guide_path: PathBuf,
    ) -> Barrier {
        self.cancel();
        let (send, receive) = mpsc::channel();
        let _ = self.commands.send(Command::Authorize(
            guide.clone(),
            enrollment.identities.clone(),
            Some(guide_path),
            true,
            send,
        ));
        receive
    }

    pub fn pause_and_join(&self, reason: PauseReason) -> Result<()> {
        self.request_pause(reason)
            .recv()
            .context("Sharing owner unavailable")?
            .map_err(anyhow::Error::msg)
    }

    pub fn shutdown(&mut self) -> Result<()> {
        self.cancel();
        let _ = self.commands.send(Command::Shutdown);
        if let Some(join) = self.join.take() {
            join.join()
                .map_err(|_| anyhow::anyhow!("Sharing owner panicked"))?;
        }
        if let Some(error) = self.terminal_error.lock().unwrap().as_ref() {
            anyhow::bail!("{error}");
        }
        Ok(())
    }
}
impl Drop for SharingOwner {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[derive(Default)]
struct Observed {
    started: bool,
    active: Duration,
    uploaded: u64,
}
struct Transfer {
    record: Record,
    cancelled: Arc<AtomicBool>,
    observed: Arc<Mutex<Observed>>,
    join: JoinHandle<TransferResult>,
    preparing_since: Instant,
    last_change: Instant,
    last_uploaded: u64,
    transition: Option<&'static str>,
    transport_seen: bool,
    last_checkpoint: Instant,
    guide_stamp: Option<(u64, SystemTime)>,
}
struct TransferResult {
    outcome: Result<p2p::ShareOutcome>,
    policy: Option<Arc<p2p::PeerPolicy>>,
}
struct Controller {
    settings: SettingsFile,
    catalog: Catalog,
    commands: mpsc::Receiver<Command>,
    active_cancel: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    interrupted: Arc<AtomicBool>,
    snapshot: Arc<Mutex<SharingSnapshot>>,
    active: Option<Transfer>,
    policy: Option<Arc<p2p::PeerPolicy>>,
    guide: Option<Arc<Guide>>,
    session: String,
    cursor: Option<String>,
    paused: Option<String>,
    settings_signature: Option<String>,
}
impl Controller {
    fn new(
        settings: SettingsFile,
        catalog: Catalog,
        commands: mpsc::Receiver<Command>,
        active_cancel: Arc<Mutex<Option<Arc<AtomicBool>>>>,
        interrupted: Arc<AtomicBool>,
        snapshot: Arc<Mutex<SharingSnapshot>>,
    ) -> Self {
        let session = sha256(format!("{}:{:?}", std::process::id(), SystemTime::now()).as_bytes());
        Self {
            settings,
            catalog,
            commands,
            active_cancel,
            interrupted,
            snapshot,
            active: None,
            policy: None,
            guide: None,
            session,
            cursor: None,
            paused: None,
            settings_signature: None,
        }
    }

    fn cancel_transfer(&self) {
        if let Some(active) = &self.active {
            active.cancelled.store(true, Ordering::Release);
        }
    }

    // Called only while unwinding a controller error: keep in_flight durable,
    // so incomplete final accounting cannot be mistaken for a clean stop.
    fn join_transfer(&mut self) {
        if let Some(active) = self.active.take() {
            let _ = active.join.join();
        }
        *self.active_cancel.lock().unwrap() = None;
    }

    fn publish(&self, record: &Record) {
        let mut snapshot = self.snapshot.lock().unwrap();
        snapshot.identity = Some(record.identity.clone());
        snapshot.state = record.state.clone();
        snapshot.reason = record.reason.clone();
        snapshot.uploaded_bytes = record.uploaded_bytes;
        snapshot.active_millis = record.active_millis;
    }

    fn visit(&self, mut change: impl FnMut(&mut Record) -> bool) -> Result<()> {
        for directory in ["records", "history"] {
            let mut cursor = None;
            loop {
                let page = self.catalog.page_directory(directory, cursor.as_deref())?;
                if page.is_empty() {
                    break;
                }
                for mut record in page {
                    cursor = Some(record.identity.clone());
                    if change(&mut record) {
                        self.catalog.update(&record)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn pause_records(&self, reason: &str) -> Result<()> {
        self.visit(|record| {
            if matches!(record.state, State::Finished | State::Failed) {
                return false;
            }
            let explicit =
                reason == PauseReason::User.text() || reason == PauseReason::Settings.text();
            // A later explicit Stop must survive an earlier mutation barrier.
            // Keep separate approval/in-flight holds and all accumulated budgets.
            if record.state == State::Paused {
                if !explicit || record.reason.as_deref() != Some(PauseReason::Mutation.text()) {
                    return false;
                }
                record.reason = Some(reason.into());
                return true;
            }
            if record.in_flight || record.requires_context_approval {
                return false;
            }
            record.state = State::Paused;
            record.reason = Some(reason.into());
            record.in_flight = false;
            true
        })
    }

    fn consume(&self) -> Result<()> {
        self.catalog.consume_page()?;
        if let Some(reason) = &self.paused {
            self.pause_records(reason)?;
        }
        Ok(())
    }

    fn run(&mut self) -> Result<()> {
        let cursor_path = self.catalog.root.join("cursor.json");
        if cursor_path.exists() {
            ensure!(
                fs::metadata(&cursor_path)?.len() <= MAX_RECORD,
                "Oversized sharing cursor"
            );
            self.cursor = serde_json::from_slice(&fs::read(&cursor_path)?)
                .context("Malformed sharing cursor")?;
        }
        self.visit(|record| {
            if record.in_flight {
                record.state = State::Paused;
                record.reason = Some("Unclean sharing stop; explicit Resume required".into());
                // Retain the marker until Resume explicitly acknowledges lost accounting.
                return true;
            }
            if record.provenance.guide_path.is_none() && record.state != State::Finished {
                record.state = State::Paused;
                record.requires_context_approval = true;
                record.session_authorization = None;
                if record.reason.as_deref().is_none_or(|reason| {
                    !reason.contains("explicit Resume") && !reason.contains("mutation barrier")
                }) {
                    record.reason = Some("Captured guide approval required".into());
                }
                return true;
            }
            if record.reason.as_deref().is_some_and(|reason| {
                matches!(
                    reason,
                    "No working proxy; explicit Resume or restart required"
                        | "Peer transport unavailable; Resume or restart required"
                ) || reason == p2p::PeerUnavailable::NoWorkingProxy.to_string()
                    || reason == p2p::PeerUnavailable::NoPeerSource.to_string()
            }) {
                record.state = State::Queued;
                record.reason = None;
                return true;
            }
            false
        })?;
        let mut last_poll = Instant::now() - Duration::from_secs(1);
        let mut pending = None;
        loop {
            if pending.is_none() {
                match self.commands.recv_timeout(Duration::from_millis(100)) {
                    Ok(command) => {
                        self.cancel_transfer();
                        pending = Some(command);
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        self.cancel_transfer();
                        pending = Some(Command::Shutdown);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
            } else {
                thread::sleep(Duration::from_millis(100));
            }
            self.observe()?;
            if self
                .active
                .as_ref()
                .is_some_and(|active| active.join.is_finished())
            {
                self.finish()?;
            }
            if self.active.is_none()
                && let Some(command) = pending.take()
            {
                match command {
                    Command::Shutdown => {
                        return Ok(());
                    }
                    Command::Pause(reason, reply) => {
                        let effective = if matches!(reason, PauseReason::Mutation)
                            && self.paused.as_deref().is_some_and(|current| {
                                current == PauseReason::User.text()
                                    || current == PauseReason::Settings.text()
                            }) {
                            self.paused.clone().unwrap()
                        } else {
                            reason.text().into()
                        };
                        self.paused = Some(effective.clone());
                        self.policy = None;
                        let result = self.consume().and_then(|_| self.pause_records(&effective));
                        if result.is_ok() {
                            self.snapshot.lock().unwrap().state = State::Paused;
                        }
                        let _ = reply.send(result.map_err(|error| format!("{error:#}")));
                    }
                    Command::ResumeMutation(reply) => {
                        let result = if self.paused.as_deref() == Some(PauseReason::Mutation.text())
                        {
                            self.visit(|record| {
                                if record.state != State::Paused
                                    || record.requires_context_approval
                                    || record.in_flight
                                    || record.reason.as_deref()
                                        != Some(PauseReason::Mutation.text())
                                {
                                    return false;
                                }
                                record.state = State::Queued;
                                record.reason = None;
                                true
                            })
                        } else {
                            Ok(())
                        };
                        if result.is_ok()
                            && self.paused.as_deref() == Some(PauseReason::Mutation.text())
                        {
                            self.policy = None;
                            self.paused = None;
                            self.interrupted.store(false, Ordering::Release);
                        }
                        let _ = reply.send(result.map_err(|error| format!("{error:#}")));
                    }
                    Command::Resume(reply) => {
                        self.policy = None;
                        let result = self.catalog.consume_page().and_then(|_| {
                            self.visit(|record| {
                                if record.state == State::Finished
                                    || record.requires_context_approval
                                {
                                    return false;
                                }
                                record.state = State::Queued;
                                record.reason = None;
                                record.in_flight = false;
                                record.deferred_until = 0;
                                true
                            })
                        });
                        if result.is_ok() {
                            self.paused = None;
                            self.interrupted.store(false, Ordering::Release);
                        }
                        let _ = reply.send(result.map_err(|error| format!("{error:#}")));
                    }
                    Command::Authorize(guide, identities, guide_path, current_only, reply) => {
                        self.guide = Some(Arc::new(guide));
                        let result =
                            self.authorize(&identities, guide_path.as_deref(), current_only);
                        if result.is_ok() && self.paused.is_none() {
                            self.interrupted.store(false, Ordering::Release);
                        }
                        let _ = reply.send(result.map_err(|error| format!("{error:#}")));
                    }
                }
            }
            if last_poll.elapsed() >= Duration::from_secs(1) {
                last_poll = Instant::now();
                let settings = match self.settings.load() {
                    Ok(value) => p2p::PeerSettings::from_settings(&value),
                    Err(error) => {
                        self.paused = Some(format!("Sharing settings unavailable: {error:#}"));
                        self.cancel_transfer();
                        self.snapshot.lock().unwrap().reason = self.paused.clone();
                        let _ = self.consume();
                        continue;
                    }
                };
                let signature = sha256(format!("{settings:?}").as_bytes());
                if self
                    .settings_signature
                    .as_ref()
                    .is_some_and(|old| old != &signature)
                {
                    self.cancel_transfer();
                    self.policy = None;
                }
                if self.active.is_none() {
                    self.settings_signature = Some(signature);
                }
                if !settings.sharing
                    || !settings.keep_sharing
                    || settings.effective_mode() == p2p::PeerMode::Off
                {
                    self.paused = Some(PauseReason::Settings.text().into());
                    self.cancel_transfer();
                    self.policy = None;
                }
                // Storage failures stay visible and pending files remain recoverable.
                if let Err(error) = self.consume() {
                    self.paused = Some(format!(
                        "Sharing storage unavailable: {error:#}; Resume after repair"
                    ));
                    self.cancel_transfer();
                    self.snapshot.lock().unwrap().reason = self.paused.clone();
                    continue;
                }
                if self.active.is_none() && pending.is_none() && self.paused.is_none() {
                    self.schedule(settings)?;
                }
            }
        }
    }

    fn authorize(
        &self,
        identities: &[String],
        guide_path: Option<&Path>,
        current_only: bool,
    ) -> Result<()> {
        let guide_path = guide_path.map(fs::canonicalize).transpose()?;
        let saved = guide_path
            .as_deref()
            .map(Guide::load_preserving_selection)
            .transpose()?;
        let captured = self.guide.as_ref().context("No captured guide")?;
        let _tx = self.catalog.transaction()?;
        for identity in identities {
            let existing = self.catalog.path("records", identity)?;
            let history = self.catalog.path("history", identity)?;
            let pending = self.catalog.path("pending", identity)?;
            let mut selected = None;
            for candidate in [&history, &existing, &pending] {
                match fs::symlink_metadata(candidate) {
                    Ok(_) => {
                        selected = Some(candidate.clone());
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            let path = match selected {
                Some(path) => path,
                None if current_only => continue,
                None => anyhow::bail!("Sharing enrollment record disappeared: {identity}"),
            };
            let mut record = read_record(&path, identity)?;
            if current_only && check_provenance(&record, captured).is_err() {
                continue;
            }
            check_provenance(&record, captured)?;
            if let Some(saved) = &saved {
                check_provenance(&record, saved)?;
            }
            if record.provenance.guide_path.is_none() {
                record.provenance.guide_path = guide_path.clone();
                record.session_authorization = Some(self.session.clone());
            }
            if record.requires_context_approval {
                record.requires_context_approval = false;
                if record.reason.as_deref() == Some("Captured guide approval required") {
                    record.state = if self.paused.is_some() {
                        State::Paused
                    } else {
                        State::Queued
                    };
                    record.reason = self.paused.clone();
                }
            }
            let target = if record.state == State::Queued && path != pending {
                &existing
            } else {
                &path
            };
            durable_write(target, &serde_json::to_vec(&record)?)?;
            if target != &path {
                fs::remove_file(&path)?;
                sync_dir(path.parent().context("Sharing record has no parent")?)?;
            }
        }
        Ok(())
    }

    fn schedule(&mut self, settings: p2p::PeerSettings) -> Result<()> {
        if self.interrupted.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut page = self.catalog.page(self.cursor.as_deref())?;
        if page.is_empty() {
            self.cursor = None;
            page = self.catalog.page(None)?;
        }
        for mut record in page {
            self.cursor = Some(record.identity.clone());
            durable_write(
                &self.catalog.root.join("cursor.json"),
                &serde_json::to_vec(&self.cursor)?,
            )?;
            if record.in_flight
                || record.requires_context_approval
                || matches!(
                    record.state,
                    State::Paused | State::Finished | State::Failed
                )
                || record.deferred_until > epoch_seconds()
            {
                continue;
            }
            if record.active_millis >= TOTAL_ACTIVE.as_millis() as u64
                || record
                    .verified_length
                    .is_some_and(|total| total > 0 && record.uploaded_bytes >= total)
            {
                record.state = State::Finished;
                record.reason = Some("Cumulative sharing budget exhausted".into());
                self.catalog.update(&record)?;
                self.publish(&record);
                continue;
            }
            record.state = State::Verifying;
            record.reason = None;
            record.in_flight = true;
            self.catalog.update(&record)?;
            self.publish(&record);
            let cancelled = Arc::new(AtomicBool::new(false));
            *self.active_cancel.lock().unwrap() = Some(cancelled.clone());
            if self.interrupted.load(Ordering::Acquire) {
                cancelled.store(true, Ordering::Release);
            }
            let observed = Arc::new(Mutex::new(Observed::default()));
            let guide_stamp = record
                .provenance
                .guide_path
                .as_deref()
                .and_then(source_stamp);
            let candidate = record.clone();
            let worker_cancel = cancelled.clone();
            let worker_observed = observed.clone();
            let guide = self.guide.clone();
            let session = self.session.clone();
            let policy = self.policy.clone();
            let cache = self.settings.directory.join("peer-cache");
            let join = thread::Builder::new()
                .name("finished-share-transfer".into())
                .spawn(move || {
                    run_transfer(
                        candidate,
                        guide,
                        &session,
                        settings,
                        cache,
                        policy,
                        worker_cancel,
                        worker_observed,
                    )
                })?;
            self.active = Some(Transfer {
                record,
                cancelled,
                observed,
                join,
                preparing_since: Instant::now(),
                last_change: Instant::now(),
                last_uploaded: 0,
                transition: None,
                transport_seen: false,
                last_checkpoint: Instant::now(),
                guide_stamp,
            });
            break;
        }
        Ok(())
    }

    fn observe(&mut self) -> Result<()> {
        let Some(active) = &mut self.active else {
            return Ok(());
        };
        let observed = active.observed.lock().unwrap();
        if active
            .record
            .provenance
            .guide_path
            .as_deref()
            .and_then(source_stamp)
            != active.guide_stamp
        {
            active.transition = Some("Saved guide changed; revalidation and Resume required");
            active.cancelled.store(true, Ordering::Release);
        }
        if observed.started && !active.transport_seen {
            active.transport_seen = true;
            active.last_change = Instant::now();
        }
        if observed.uploaded > active.last_uploaded {
            active.last_uploaded = observed.uploaded;
            active.last_change = Instant::now();
        }
        if !observed.started && active.preparing_since.elapsed() >= PREPARATION {
            active.transition = Some("Preparation timed out; Resume required");
            active.cancelled.store(true, Ordering::Release);
        }
        if observed.started {
            if observed.active >= QUANTUM {
                active.transition = Some("quantum");
                active.cancelled.store(true, Ordering::Release);
            } else if active.last_change.elapsed() >= IDLE {
                active.transition = Some("idle");
                active.cancelled.store(true, Ordering::Release);
            }
        }
        if active.last_checkpoint.elapsed() >= Duration::from_secs(1) {
            let mut checkpoint = active.record.clone();
            checkpoint.active_millis = checkpoint
                .active_millis
                .saturating_add(observed.active.as_millis().min(u64::MAX as u128) as u64);
            checkpoint.uploaded_bytes = checkpoint.uploaded_bytes.saturating_add(observed.uploaded);
            checkpoint.state = if observed.started {
                State::Sharing
            } else {
                State::Verifying
            };
            self.catalog.update(&checkpoint)?;
            active.last_checkpoint = Instant::now();
        }
        let mut snapshot = self.snapshot.lock().unwrap();
        snapshot.state = if active.cancelled.load(Ordering::Acquire) {
            State::Stopping
        } else if observed.started {
            State::Sharing
        } else {
            State::Verifying
        };
        snapshot.uploaded_bytes = active
            .record
            .uploaded_bytes
            .saturating_add(observed.uploaded);
        snapshot.active_millis = active
            .record
            .active_millis
            .saturating_add(observed.active.as_millis().min(u64::MAX as u128) as u64);
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        let active = self.active.take().context("Missing sharing worker")?;
        let result = active
            .join
            .join()
            .map_err(|_| anyhow::anyhow!("Sharing transfer panicked"))?;
        *self.active_cancel.lock().unwrap() = None;
        let observed = active.observed.lock().unwrap();
        let mut record = active.record;
        record.uploaded_bytes = record.uploaded_bytes.saturating_add(observed.uploaded);
        record.active_millis = record
            .active_millis
            .saturating_add(observed.active.as_millis().min(u64::MAX as u128) as u64);
        record.in_flight = false;
        self.policy = result.policy;
        match result.outcome {
            Ok(p2p::ShareOutcome::Stopped {
                reason: p2p::ShareStop::Ratio | p2p::ShareStop::Duration,
                ..
            }) => {
                record.state = State::Finished;
                record.reason = Some("Cumulative sharing budget exhausted".into());
            }
            Ok(p2p::ShareOutcome::Stopped {
                reason: p2p::ShareStop::Cancelled,
                ..
            })
            | Err(_)
                if active.cancelled.load(Ordering::Acquire) =>
            {
                match active.transition {
                    Some("quantum") => {
                        record.state = State::Queued;
                        record.reason = None;
                    }
                    Some("idle") => {
                        record.state = State::Queued;
                        record.reason =
                            Some("Waiting for reachable peers; deferred for five minutes".into());
                        record.deferred_until = epoch_seconds().saturating_add(300);
                    }
                    Some(reason) => {
                        record.state = State::Failed;
                        record.reason = Some(reason.into());
                    }
                    None => {
                        record.state = State::Queued;
                        record.reason = None;
                    }
                }
            }
            Ok(p2p::ShareOutcome::SharingOff) => {
                record.state = State::Paused;
                record.reason = Some(PauseReason::Settings.text().into());
            }
            Ok(p2p::ShareOutcome::Stopped {
                reason: p2p::ShareStop::NoWorkingProxy,
                ..
            }) => {
                record.state = State::Failed;
                record.reason =
                    Some("No working proxy; explicit Resume or restart required".into());
                self.paused = record.reason.clone();
                self.policy = None;
            }
            Err(error) if error.downcast_ref::<p2p::PeerUnavailable>().is_some() => {
                record.state = State::Failed;
                record.reason = Some(error.to_string());
                self.paused = Some("Peer transport unavailable; Resume or restart required".into());
                self.policy = None;
            }
            Ok(_) => {
                record.state = State::Failed;
                record.reason = Some(
                    "Archive or authored torrent identity changed; revalidation required".into(),
                );
            }
            Err(_) => {
                record.state = State::Failed;
                record.reason =
                    Some("Sharing preparation or transport failed; revalidate and Resume".into());
            }
        }
        self.catalog.update(&record)?;
        self.publish(&record);
        Ok(())
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.cancel_transfer();
        self.join_transfer();
    }
}

fn source_stamp(path: &Path) -> Option<(u64, SystemTime)> {
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.len(), metadata.modified().ok()?))
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn check_provenance(record: &Record, guide: &Guide) -> Result<()> {
    let metadata = guide
        .components
        .iter()
        .find(|c| {
            c.guid
                .eq_ignore_ascii_case(&record.provenance.component_guid)
        })
        .and_then(|c| {
            c.resource_registry
                .iter()
                .find(|(url, _)| sha256(url.as_bytes()) == record.provenance.registry_key_sha256)
        })
        .map(|(_, metadata)| metadata)
        .context("Guide no longer authorizes this resource")?;
    let (hash, _, info_hash, _, fingerprint) = authorization(metadata)?;
    ensure!(
        hash == record.authored_sha256
            && info_hash == record.info_hash
            && fingerprint == record.provenance.authorization_sha256,
        "Guide authorization changed"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_transfer(
    record: Record,
    session_guide: Option<Arc<Guide>>,
    session: &str,
    settings: p2p::PeerSettings,
    cache: PathBuf,
    mut policy: Option<Arc<p2p::PeerPolicy>>,
    cancelled: Arc<AtomicBool>,
    observed: Arc<Mutex<Observed>>,
) -> TransferResult {
    let outcome = (|| -> Result<p2p::ShareOutcome> {
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "Sharing preparation cancelled"
        );
        let guide = match &record.provenance.guide_path {
            Some(path) => Arc::new(Guide::load_preserving_selection(path)?),
            None => {
                ensure!(
                    record.session_authorization.as_deref() == Some(session),
                    "Unsaved guide requires explicit current-process enrollment authorization"
                );
                session_guide.context("Unsaved guide requires current-process revalidation")?
            }
        };
        check_provenance(&record, &guide)?;
        let root = fs::canonicalize(&record.root)?;
        let path = fs::canonicalize(&record.path)?;
        ensure!(
            path == record.path && root == record.root && path.parent() == Some(root.as_path()),
            "Sharing source moved outside captured root"
        );
        let mut file = crate::paths::open_regular_read(&path)?;
        let total = file.metadata()?.len();
        ensure!(
            record.verified_length.is_none_or(|length| length == total),
            "Source archive length changed"
        );
        let mut hash = Sha256::new();
        let mut block = [0u8; 64 * 1024];
        loop {
            ensure!(
                !cancelled.load(Ordering::Acquire),
                "Sharing verification cancelled"
            );
            let read = file.read(&mut block)?;
            if read == 0 {
                break;
            }
            hash.update(&block[..read]);
        }
        ensure!(
            format!("{:x}", hash.finalize()) == record.authored_sha256,
            "Source archive hash changed"
        );
        archive::verify(&path, &path, &record.archive_options(), &cancelled)?;
        let info = record.decoded_info()?;
        if let Some(info) = &info {
            p2p::check_torrent_info(info, &record.info_hash)?;
        }
        if record.uploaded_bytes >= total.max(1)
            || record.active_millis >= TOTAL_ACTIVE.as_millis() as u64
        {
            return Ok(p2p::ShareOutcome::Stopped {
                reason: if record.uploaded_bytes >= total.max(1) {
                    p2p::ShareStop::Ratio
                } else {
                    p2p::ShareStop::Duration
                },
                info_hash: record.info_hash.clone(),
                uploaded_bytes: 0,
                seconds: 0,
            });
        }
        if policy.is_none() {
            policy = settings.sharing_policy(cache, &cancelled)?.map(Arc::new);
        }
        let Some(policy) = &policy else {
            return Ok(p2p::ShareOutcome::SharingOff);
        };
        let total = total.max(1);
        p2p::share_verified_authored_observed(
            &path,
            p2p::AuthoredTorrent {
                magnet: &record.magnet,
                torrent_info: info.as_deref(),
            },
            &record.authored_sha256,
            policy,
            p2p::ShareUntil {
                max_duration: Some(
                    TOTAL_ACTIVE.saturating_sub(Duration::from_millis(record.active_millis)),
                ),
                max_ratio: Some(total.saturating_sub(record.uploaded_bytes) as f64 / total as f64),
            },
            &cancelled,
            |event| {
                let mut observed = observed.lock().unwrap();
                match event {
                    p2p::ShareEvent::Started { .. } => observed.started = true,
                    p2p::ShareEvent::Progress {
                        uploaded_bytes,
                        active,
                    } => {
                        observed.uploaded = observed.uploaded.max(uploaded_bytes);
                        observed.active = observed.active.max(active);
                    }
                }
            },
        )
    })();
    TransferResult { outcome, policy }
}
