use super::{model::*, store::Store};
use anyhow::{Context, Result, ensure};
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, Metadata},
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub(super) const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;
pub(super) const MAX_ENTRIES: usize = 100_000;

pub(super) fn cancelled(flag: &AtomicBool) -> Result<()> {
    ensure!(
        !flag.load(Ordering::Relaxed),
        "Checkpoint operation cancelled"
    );
    Ok(())
}

pub(super) fn id(value: &str) -> Result<()> {
    let parsed = uuid::Uuid::parse_str(value).context("Checkpoint ID is not a UUID")?;
    ensure!(
        !parsed.is_nil() && parsed.to_string() == value,
        "Checkpoint UUID must use canonical lowercase spelling"
    );
    Ok(())
}

pub(super) fn digest(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Invalid SHA256 object identity"
    );
    Ok(())
}

fn ordinary(metadata: &Metadata) -> Result<()> {
    ensure!(
        !metadata.file_type().is_symlink() && (metadata.is_file() || metadata.is_dir()),
        "Checkpoint path is a symlink or special file"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "Checkpoint path is a reparse point"
        );
    }
    Ok(())
}

/// Reject links and special files at every existing ancestor, including the leaf.
/// The final open also refuses following a replaced leaf on Unix.
pub(crate) fn safe_path(path: &Path) -> Result<Option<Metadata>> {
    let mut result = None;
    for ancestor in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                ordinary(&metadata)?;
                if ancestor != path {
                    ensure!(metadata.is_dir(), "Checkpoint path has a file ancestor");
                }
                if ancestor == path {
                    result = Some(metadata);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(result)
}

pub(super) fn open_file(path: &Path) -> Result<File> {
    ensure!(
        safe_path(path)?.is_some_and(|m| m.is_file()),
        "Checkpoint file missing or not a regular file: {}",
        path.display()
    );
    let file = crate::paths::open_regular_read(path)?;
    ordinary(&file.metadata()?)?;
    ensure!(
        file.metadata()?.is_file(),
        "Checkpoint object is not a regular file"
    );
    Ok(file)
}

pub(super) fn json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let mut body = Vec::new();
    open_file(path)?
        .take(MAX_DOCUMENT_BYTES + 1)
        .read_to_end(&mut body)?;
    ensure!(
        body.len() as u64 <= MAX_DOCUMENT_BYTES,
        "Checkpoint document exceeds 64 MiB"
    );
    serde_json::from_slice(&body)
        .with_context(|| format!("Malformed checkpoint document {}", path.display()))
}

pub(super) fn hash_file(path: &Path, flag: &AtomicBool) -> Result<(String, u64)> {
    let file = open_file(path)?;
    let expected_length = file.metadata()?.len();
    let mut file = file.take(expected_length.saturating_add(1));
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        cancelled(flag)?;
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
        count = count
            .checked_add(n as u64)
            .context("Checkpoint size overflow")?;
    }
    ensure!(
        count == expected_length,
        "Checkpoint file length changed while hashing"
    );
    Ok((format!("{:x}", hash.finalize()), count))
}

pub(super) fn mode(metadata: &Metadata) -> Mode {
    #[cfg(unix)]
    let unix = {
        use std::os::unix::fs::PermissionsExt;
        Some(metadata.permissions().mode() & 0o777)
    };
    #[cfg(not(unix))]
    let unix = None;
    Mode {
        readonly: metadata.permissions().readonly(),
        unix,
    }
}

fn valid_mode(mode: &Mode) -> Result<()> {
    ensure!(
        mode.unix.is_none_or(|n| n <= 0o777),
        "Invalid checkpoint mode bits"
    );
    Ok(())
}

/// A new session's first capture refuses every Windows device name
/// (`paths::is_reserved_name`); commits and resumes record what is on disk under the
/// older rule, which `validate_tree` keeps so earlier stores load.
pub(super) fn refuse_device_names(tree: &Tree) -> Result<()> {
    for name in tree.entries.keys() {
        ensure!(
            !name.split('/').any(crate::paths::is_reserved_name),
            "Checkpoint cannot record {name}: Windows device name"
        );
    }
    Ok(())
}

pub(super) fn validate_tree(tree: &Tree) -> Result<(u64, u64)> {
    valid_mode(&tree.root_mode)?;
    ensure!(
        tree.entries.len() <= MAX_ENTRIES,
        "Checkpoint tree exceeds entry bound"
    );
    let mut aliases = BTreeSet::new();
    let mut count = 0u64;
    let mut bytes = 0u64;
    for (name, entry) in &tree.entries {
        let path = crate::paths::stored_relative(name, true)?;
        ensure!(
            !name.is_empty() && !name.contains('\\') && path.to_str() == Some(name.as_str()),
            "Checkpoint path is not portable canonical relative text"
        );
        ensure!(
            aliases.insert(crate::unicode_case::ordinal_key(name)),
            "Checkpoint tree contains case aliases"
        );
        let mut parent = path.parent();
        while let Some(p) = parent.filter(|p| !p.as_os_str().is_empty()) {
            ensure!(
                matches!(
                    tree.entries
                        .get(p.to_str().context("Non-UTF8 checkpoint path")?),
                    Some(Entry::Directory { .. })
                ),
                "Checkpoint file ancestor or missing directory entry"
            );
            parent = p.parent();
        }
        match entry {
            Entry::Directory { mode } => valid_mode(mode)?,
            Entry::File { sha256, size, mode } => {
                digest(sha256)?;
                valid_mode(mode)?;
                count += 1;
                bytes = bytes
                    .checked_add(*size)
                    .context("Checkpoint byte count overflow")?;
            }
        }
    }
    Ok((count, bytes))
}

impl Store {
    pub(super) fn session_directory(&self, session: &str) -> Result<PathBuf> {
        id(session)?;
        Ok(self.directory.join("sessions").join(session))
    }
    pub(super) fn object_path(&self, hash: &str) -> Result<PathBuf> {
        digest(hash)?;
        Ok(self
            .directory
            .join("objects/sha256")
            .join(&hash[..2])
            .join(&hash[2..]))
    }
    pub(super) fn read_session(&self, session_id: &str) -> Result<Session> {
        let session: Session = json(&self.session_directory(session_id)?.join("session.json"))?;
        ensure!(
            session.schema_version == SCHEMA_VERSION
                && session.session_id == session_id
                && session.roots == self.binding
                && super::persistence::known_origin(&session.origin),
            "Session schema, identity, origin or roots differ"
        );
        ensure!(
            matches!(
                session.install_scope.as_str(),
                "baseline" | "install" | "safety"
            ) && session.parent_session_id.is_some() == session.parent_checkpoint_id.is_some(),
            "Unsupported checkpoint session scope or lineage"
        );
        match (
            &session.parent_session_id,
            &session.parent_checkpoint_id,
            &session.branch_kind,
        ) {
            (None, None, None) => {}
            (Some(parent), Some(checkpoint), Some(kind)) => {
                id(parent)?;
                id(checkpoint)?;
                ensure!(
                    parent != session_id
                        && session.install_scope == "install"
                        && matches!(kind.as_str(), "Restore" | "Resume" | "SourceRebase"),
                    "Invalid checkpoint branch lineage"
                );
            }
            _ => anyhow::bail!("Incomplete checkpoint branch lineage"),
        }
        if let Some(safety) = &session.safety_session_id {
            id(safety)?;
            ensure!(
                safety != session_id && session.branch_kind.as_deref() == Some("Restore"),
                "Invalid safety session reference"
            );
        }
        super::history::validate_definitions(&session.ordered_components)?;
        ensure!(
            session.initial_completed_components.len() <= session.ordered_components.len()
                && super::store::canonical_bytes(&session.initial_completed_components)?
                    == super::store::canonical_bytes(
                        &session.ordered_components[..session.initial_completed_components.len()]
                    )?,
            "Session initial completed prefix differs"
        );
        self.validate_participants(&session.participants, &session.ancestor_shells)?;
        ensure!(
            session
                .ancestor_shells
                .windows(2)
                .all(|pair| pair[0] < pair[1]),
            "Ancestor shell inventory is not uniquely sorted"
        );
        if session.install_scope == "baseline" {
            ensure!(
                session.ordered_components.is_empty()
                    && session.initial_completed_components.is_empty()
                    && session.participants.is_empty()
                    && session.ancestor_shells.is_empty(),
                "Standalone baseline has component definitions"
            );
        }
        if let Some(hash) = &session.definitions_sha256 {
            digest(hash)?;
            ensure!(
                *hash == super::history::definition_hash(&session.ordered_components)?,
                "Session definition digest differs"
            );
        } else {
            ensure!(
                session.ordered_components.is_empty(),
                "Install session has no definition digest"
            );
        }
        chrono::DateTime::parse_from_rfc3339(&session.created_utc)
            .context("Invalid session UTC timestamp")?;
        ensure!(
            session.created_utc.ends_with('Z') || session.created_utc.ends_with("+00:00"),
            "Session timestamp is not UTC"
        );
        Ok(session)
    }
    pub(super) fn read_head(&self, session: &str) -> Result<Head> {
        let head: Head = json(&self.session_directory(session)?.join("HEAD.json"))?;
        ensure!(
            head.schema_version == SCHEMA_VERSION
                && head.generation > 0
                && matches!(
                    head.status.as_str(),
                    "Ready"
                        | "Complete"
                        | "CleanupRequired"
                        | "FinalizeCleanupRequired"
                        | "Restored"
                        | "Detached"
                        | "TransitionCleanupRequired"
                ),
            "Unsupported checkpoint HEAD schema/generation/status"
        );
        if let Some(receipt) = &head.finalization_receipt {
            digest(&receipt.sha256)?;
            ensure!(
                receipt.size > 0
                    && receipt.size <= MAX_DOCUMENT_BYTES
                    && matches!(
                        head.status.as_str(),
                        "Complete" | "FinalizeCleanupRequired" | "Detached"
                    ),
                "Invalid HEAD finalization receipt"
            );
        }
        ensure!(
            head.status != "FinalizeCleanupRequired" || head.finalization_receipt.is_some(),
            "Finalize cleanup HEAD has no receipt"
        );
        ensure!(
            head.transition_operation.is_some() == (head.status == "TransitionCleanupRequired"),
            "HEAD transition fence is incomplete"
        );
        id(&head.checkpoint_id)?;
        id(&head.operation_id)?;
        Ok(head)
    }
    pub(super) fn read_checkpoint(&self, session: &str, checkpoint: &str) -> Result<Checkpoint> {
        id(checkpoint)?;
        let document: Checkpoint = json(
            &self
                .session_directory(session)?
                .join("checkpoints")
                .join(format!("{checkpoint}.json")),
        )?;
        ensure!(
            document.schema_version == SCHEMA_VERSION
                && document.session_id == session
                && document.checkpoint_id == checkpoint
                && document.roots == self.binding
                && super::persistence::known_origin(&document.origin),
            "Checkpoint schema/identity/root binding differs"
        );
        ensure!(
            document.auxiliary.is_empty() && !document.creator.is_empty(),
            "Unsupported checkpoint auxiliary bindings or missing creator"
        );
        let session = self.read_session(session)?;
        ensure!(
            document.origin == session.origin,
            "Checkpoint/session capability mismatch"
        );
        if document.kind == "Baseline" {
            ensure!(
                document.sequence == 0
                    && document.parent_id.is_none()
                    && document.completed_components == session.initial_completed_components
                    && document.component.is_none(),
                "Malformed baseline checkpoint"
            );
        } else {
            ensure!(
                document.kind == "Component"
                    && document.sequence > 0
                    && document
                        .sequence
                        .checked_add(session.initial_completed_components.len() as u64)
                        == Some(document.completed_components.len() as u64),
                "Malformed component checkpoint sequence"
            );
            id(document
                .parent_id
                .as_deref()
                .context("Component checkpoint has no parent")?)?;
            ensure!(
                document.component.as_ref()
                    == Some(&super::history::component_identity(
                        document
                            .completed_components
                            .last()
                            .context("Component checkpoint has no completed definition")?
                    )?),
                "Checkpoint component identity differs from full definition"
            );
        }
        if let Some(hash) = &document.completed_sha256 {
            digest(hash)?;
            ensure!(
                *hash == super::history::definition_hash(&document.completed_components)?,
                "Completed prefix digest differs"
            );
        } else {
            ensure!(
                document.completed_components.is_empty(),
                "Component checkpoint has no prefix digest"
            );
        }
        let (af, ab) = super::auxiliary::auxiliary_totals(&document, &session)?;
        let (mf, mb) = validate_tree(&document.mods)?;
        let (gf, gb) = validate_tree(&document.game)?;
        ensure!(
            document.file_count == mf + gf + af
                && document.total_bytes
                    == mb
                        .checked_add(gb)
                        .and_then(|sum| sum.checked_add(ab))
                        .context("Checkpoint size overflow")?,
            "Checkpoint inventory totals differ"
        );
        Ok(document)
    }
    pub fn list(&self) -> Result<ListReport> {
        self.validate_location()?;
        let pending = self.pending_operations()?;
        let root = self.directory.join("sessions");
        let mut report = ListReport {
            schema_version: SCHEMA_VERSION,
            operation: "list",
            sessions: Vec::new(),
        };
        let Some(metadata) = safe_path(&root)? else {
            return Ok(report);
        };
        ensure!(
            metadata.is_dir(),
            "Checkpoint sessions path is not a directory"
        );
        for entry in fs::read_dir(root)? {
            ensure!(
                report.sessions.len() < 10_000,
                "Checkpoint session listing exceeds 10000 sessions"
            );
            let entry = entry?;
            let session_id = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-UTF8 session ID"))?;
            id(&session_id)?;
            ensure!(
                safe_path(&entry.path())?.is_some_and(|m| m.is_dir()),
                "Session entry is not a directory"
            );
            let session = if safe_path(&entry.path().join("session.json"))?.is_some() {
                Some(self.read_session(&session_id)?)
            } else {
                None
            };
            if safe_path(&entry.path().join("HEAD.json"))?.is_none() {
                report.sessions.push(SessionSummary {
                    session_id,
                    checkpoint_id: None,
                    generation: None,
                    status: "Incomplete".into(),
                    created_utc: session.map(|s| s.created_utc),
                });
                continue;
            }
            let session = session.context("Committed checkpoint session has no session.json")?;
            let (_, head, _) = self.checked_history(&session_id)?;
            ensure!(
                self.read_head(&session_id)? == head,
                "Checkpoint HEAD changed while listing; retry"
            );
            let status = if pending
                .iter()
                .any(|operation| operation.session_id == session_id)
            {
                "Pending".into()
            } else {
                head.status
            };
            report.sessions.push(SessionSummary {
                session_id,
                checkpoint_id: Some(head.checkpoint_id),
                generation: Some(head.generation),
                status,
                created_utc: Some(session.created_utc),
            });
        }
        report
            .sessions
            .sort_by(|a, b| a.session_id.cmp(&b.session_id));
        Ok(report)
    }
    pub fn verify(
        &self,
        session: &str,
        checkpoint: Option<&str>,
        flag: &AtomicBool,
    ) -> Result<VerificationReport> {
        self.validate_location()?;
        cancelled(flag)?;
        self.ensure_no_pending()?;
        let (_, head, history) = self.checked_history(session)?;
        let checkpoint = checkpoint.unwrap_or(&head.checkpoint_id);
        id(checkpoint)?;
        let document = history
            .iter()
            .find(|document| document.checkpoint_id == checkpoint)
            .context("Checkpoint is not in committed HEAD ancestry")?;
        let mut objects = super::auxiliary::objects(document);
        if let Some(receipt) = &head.finalization_receipt {
            objects.insert((receipt.sha256.clone(), receipt.size));
        }
        for (sha256, size) in &objects {
            cancelled(flag)?;
            ensure!(
                hash_file(&self.object_path(sha256)?, flag)? == (sha256.clone(), *size),
                "Checkpoint object hash or length mismatch: {sha256}"
            );
        }
        ensure!(
            self.read_head(session)? == head,
            "Checkpoint HEAD changed while verifying; retry"
        );
        Ok(VerificationReport {
            schema_version: SCHEMA_VERSION,
            operation: "verify",
            session_id: session.into(),
            checkpoint_id: checkpoint.into(),
            generation: head.generation,
            verified_objects: objects.len() as u64,
            file_count: document.file_count,
            total_bytes: document.total_bytes,
        })
    }
}
