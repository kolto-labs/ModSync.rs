//! Held-lock retention and content-addressed repair. No operation writes install roots.
use super::{
    model::*,
    read::*,
    store::{Store, ensure_directory, namespace_boundary, publish_json},
};
use crate::recovery;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

const INVENTORY_LIMIT: usize = 200_000;
const METADATA_LIMIT: u64 = 256 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteReport {
    pub schema_version: u32,
    pub operation: &'static str,
    pub session_id: String,
    pub checkpoint_id: String,
    pub generation: u64,
    pub trash_id: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GcReport {
    pub schema_version: u32,
    pub operation: &'static str,
    pub removed_objects: u64,
    pub removed_bytes: u64,
    pub removed_metadata: u64,
    pub retained_objects: u64,
    pub retained_incomplete_sessions: u64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairIssue {
    pub sha256: String,
    pub size: u64,
    pub reason: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairReport {
    pub schema_version: u32,
    pub operation: &'static str,
    pub session_id: String,
    pub checkpoint_id: String,
    pub generation: u64,
    pub complete: bool,
    pub repaired_objects: u64,
    pub already_valid_objects: u64,
    pub unresolved: Vec<RepairIssue>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Selector {
    session_id: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Deletion {
    schema_version: u32,
    roots: RootBinding,
    expected: ExpectedHead,
    trash_id: String,
}
struct Catalog {
    sessions: BTreeMap<String, (Session, Head, Vec<Checkpoint>)>,
    incomplete: BTreeSet<String>,
    objects: BTreeMap<String, u64>,
    orphan_documents: Vec<PathBuf>,
    temporary: Vec<PathBuf>,
    active: Option<String>,
}
fn add_objects(
    target: &mut BTreeMap<String, u64>,
    values: impl IntoIterator<Item = (String, u64)>,
) -> Result<()> {
    for (hash, size) in values {
        digest(&hash)?;
        if let Some(previous) = target.insert(hash, size) {
            ensure!(
                previous == size,
                "One content identity has conflicting lengths"
            );
        }
    }
    Ok(())
}
fn children(path: &Path, budget: &mut usize, flag: &AtomicBool) -> Result<Vec<PathBuf>> {
    let Some(meta) = safe_path(path)? else {
        return Ok(Vec::new());
    };
    ensure!(meta.is_dir(), "Lifecycle inventory requires a directory");
    let mut result = Vec::new();
    for item in fs::read_dir(path)? {
        cancelled(flag)?;
        *budget += 1;
        ensure!(
            *budget <= INVENTORY_LIMIT,
            "Checkpoint lifecycle inventory exceeds bound"
        );
        let path = item?.path();
        safe_path(&path)?.context("Lifecycle artifact disappeared")?;
        result.push(path);
    }
    result.sort();
    Ok(result)
}
fn name(path: &Path) -> Result<&str> {
    path.file_name()
        .and_then(|n| n.to_str())
        .context("Non-UTF8 lifecycle artifact")
}
fn document_budget(path: &Path, total: &mut u64) -> Result<()> {
    let meta = safe_path(path)?.context("Lifecycle document disappeared")?;
    ensure!(
        meta.is_file() && meta.len() <= MAX_DOCUMENT_BYTES,
        "Invalid lifecycle document"
    );
    *total = total
        .checked_add(meta.len())
        .context("Metadata inventory overflow")?;
    ensure!(
        *total <= METADATA_LIMIT,
        "Lifecycle metadata exceeds 256 MiB budget"
    );
    Ok(())
}

impl Store {
    fn lifecycle_catalog(&self, flag: &AtomicBool) -> Result<Catalog> {
        self.validate_location()?;
        let mut result = Catalog {
            sessions: BTreeMap::new(),
            incomplete: BTreeSet::new(),
            objects: BTreeMap::new(),
            orphan_documents: Vec::new(),
            temporary: Vec::new(),
            active: None,
        };
        if safe_path(&self.directory.join("active.json"))?.is_some() {
            let selector: Selector = json(&self.directory.join("active.json"))?;
            id(&selector.session_id)?;
            result.active = Some(selector.session_id);
        }
        let mut budget = 0;
        let mut bytes = 0;
        // First bound all metadata before recursive history readers allocate documents.
        let directories = children(&self.directory.join("sessions"), &mut budget, flag)?;
        ensure!(directories.len() <= 10_000, "Too many checkpoint sessions");
        for directory in &directories {
            id(name(directory)?)?;
            for path in children(directory, &mut budget, flag)? {
                match name(&path)? {
                    "session.json" | "HEAD.json" => document_budget(&path, &mut bytes)?,
                    "checkpoints" => {
                        for checkpoint in children(&path, &mut budget, flag)? {
                            if name(&checkpoint)?.starts_with(".tmp-") {
                                ensure!(
                                    safe_path(&checkpoint)?.is_some_and(|m| m.is_file()),
                                    "Unsafe temporary artifact"
                                );
                                result.temporary.push(checkpoint);
                            } else {
                                let checkpoint_id = name(&checkpoint)?
                                    .strip_suffix(".json")
                                    .context("Unknown checkpoint artifact")?;
                                id(checkpoint_id)?;
                                document_budget(&checkpoint, &mut bytes)?;
                            }
                        }
                    }
                    value if value.starts_with(".tmp-") => {
                        ensure!(
                            safe_path(&path)?.is_some_and(|m| m.is_file()),
                            "Unsafe temporary artifact"
                        );
                        result.temporary.push(path);
                    }
                    _ => anyhow::bail!("Unknown session artifact"),
                }
            }
        }
        let any_incomplete = directories
            .iter()
            .map(|directory| safe_path(&directory.join("HEAD.json")))
            .collect::<Result<Vec<_>>>()?
            .iter()
            .any(Option::is_none);
        for directory in directories {
            cancelled(flag)?;
            let session_id = name(&directory)?.to_owned();
            let has_session = safe_path(&directory.join("session.json"))?.is_some();
            let has_head = safe_path(&directory.join("HEAD.json"))?.is_some();
            ensure!(
                !has_head || has_session,
                "Committed session lacks its immutable definition"
            );
            let history = if has_head {
                let value = self.checked_history(&session_id)?;
                add_objects(
                    &mut result.objects,
                    value.2.iter().flat_map(super::auxiliary::objects),
                )?;
                if let Some(receipt) = &value.1.finalization_receipt {
                    add_objects(
                        &mut result.objects,
                        [(receipt.sha256.clone(), receipt.size)],
                    )?;
                }
                Some(value)
            } else {
                if has_session {
                    self.read_session(&session_id)?;
                }
                result.incomplete.insert(session_id.clone());
                None
            };
            for path in children(&directory.join("checkpoints"), &mut budget, flag)? {
                if name(&path)?.starts_with(".tmp-") {
                    continue;
                }
                let checkpoint_id = name(&path)?
                    .strip_suffix(".json")
                    .context("Unknown checkpoint artifact")?;
                if has_session {
                    let document = self.read_checkpoint(&session_id, checkpoint_id)?;
                    if history.as_ref().is_some_and(|(_, _, chain)| {
                        chain.iter().any(|c| c.checkpoint_id == checkpoint_id)
                    }) {
                        continue;
                    }
                    if has_head && !any_incomplete {
                        result.orphan_documents.push(path);
                    } else {
                        add_objects(&mut result.objects, super::auxiliary::objects(&document))?;
                    }
                } else {
                    // Capture writes a full baseline before session.json. Retain these
                    // unpublished bytes conservatively, but reject malformed documents.
                    let document: Checkpoint = json(&path)?;
                    validate_uncommitted(&document, &self.binding, &session_id, checkpoint_id)?;
                    add_objects(&mut result.objects, super::auxiliary::objects(&document))?;
                }
            }
            if let Some(history) = history {
                result.sessions.insert(session_id, history);
            }
        }
        for path in children(&self.directory, &mut budget, flag)? {
            if name(&path)?.starts_with(".tmp-") {
                ensure!(
                    safe_path(&path)?.is_some_and(|m| m.is_file()),
                    "Unsafe temporary artifact"
                );
                result.temporary.push(path);
            }
        }
        Ok(result)
    }

    /// Logical deletion commits by rename. Objects are retained until a later GC.
    pub fn delete_session(
        &self,
        lock: &recovery::Lock,
        expected: &ExpectedHead,
        flag: &AtomicBool,
    ) -> Result<DeleteReport> {
        self.require_lock(lock)?;
        self.ensure_no_pending()?;
        cancelled(flag)?;
        let catalog = self.lifecycle_catalog(flag)?;
        let (session, head, _) = catalog
            .sessions
            .get(&expected.session_id)
            .context("Session is not committed")?;
        ensure!(
            head.checkpoint_id == expected.checkpoint_id && head.generation == expected.generation,
            "Checkpoint generation changed; refresh before deletion"
        );
        ensure!(
            catalog.active.as_deref() != Some(&expected.session_id),
            "Cannot delete the active session; select another session first"
        );
        ensure!(
            session.install_scope != "install"
                || matches!(head.status.as_str(), "Complete" | "Detached"),
            "Cannot delete an unfinished install session"
        );
        for (other, _, _) in catalog.sessions.values() {
            ensure!(
                other.parent_session_id.as_deref() != Some(&expected.session_id)
                    && other.safety_session_id.as_deref() != Some(&expected.session_id),
                "Session is retained by branch or safety history"
            );
        }
        // Incomplete sessions can already publish immutable lineage before HEAD.
        for incomplete in &catalog.incomplete {
            if safe_path(&self.session_directory(incomplete)?.join("session.json"))?.is_some() {
                let other = self.read_session(incomplete)?;
                ensure!(
                    other.parent_session_id.as_deref() != Some(&expected.session_id)
                        && other.safety_session_id.as_deref() != Some(&expected.session_id),
                    "Session is retained by an incomplete branch"
                );
            }
        }
        let trash_id = uuid::Uuid::new_v4().to_string();
        let trash = self
            .directory
            .join("trash")
            .join(format!("delete-{trash_id}"));
        ensure_directory(&trash)?;
        publish_json(
            &trash.join("deletion.json"),
            &Deletion {
                schema_version: SCHEMA_VERSION,
                roots: self.binding.clone(),
                expected: expected.clone(),
                trash_id: trash_id.clone(),
            },
        )?;
        cancelled(flag)?;
        super::persistence::rename(
            &self.session_directory(&expected.session_id)?,
            &trash.join("session"),
            false,
        )?;
        // After rename, never claim cancellation rolled the committed deletion back.
        namespace_boundary(&self.directory.join("sessions"))?;
        namespace_boundary(&trash)?;
        Ok(DeleteReport {
            schema_version: SCHEMA_VERSION,
            operation: "deleteSession",
            session_id: expected.session_id.clone(),
            checkpoint_id: expected.checkpoint_id.clone(),
            generation: expected.generation,
            trash_id,
        })
    }

    pub fn gc(
        &self,
        lock: &recovery::Lock,
        flag: &AtomicBool,
        mut observe: impl FnMut(Progress),
    ) -> Result<GcReport> {
        self.require_lock(lock)?;
        self.ensure_no_pending()?;
        let catalog = self.lifecycle_catalog(flag)?;
        let objects = self.lifecycle_objects(flag)?;
        // Prove retained content before deleting possible alternate repair sources.
        for (hash, size) in &catalog.objects {
            cancelled(flag)?;
            ensure!(
                hash_file(&self.object_path(hash)?, flag)? == (hash.clone(), *size),
                "Retained object is missing/corrupt; repair before GC: {hash}"
            );
        }
        let trash = self.lifecycle_trash(&catalog, flag)?;
        let mut report = GcReport {
            schema_version: SCHEMA_VERSION,
            operation: "gc",
            removed_objects: 0,
            removed_bytes: 0,
            removed_metadata: 0,
            retained_objects: catalog.objects.len() as u64,
            retained_incomplete_sessions: catalog.incomplete.len() as u64,
        };
        // Remove orphan documents before CAS; a crash therefore cannot leave an
        // apparently retained metadata document whose objects GC just removed.
        for path in catalog.orphan_documents.iter().chain(&catalog.temporary) {
            cancelled(flag)?;
            remove_regular(path)?;
            report.removed_metadata += 1;
        }
        for path in trash {
            let mut budget = 0;
            for temporary in children(&path, &mut budget, flag)? {
                if name(&temporary)?.starts_with(".tmp-") {
                    remove_regular(&temporary)?;
                    report.removed_metadata += 1;
                }
            }
            let payload = path.join("session");
            if safe_path(&payload)?.is_some() {
                report.removed_metadata += remove_tree(&payload, flag)?;
            }
            let marker = path.join("deletion.json");
            if safe_path(&marker)?.is_some() {
                remove_regular(&marker)?;
                report.removed_metadata += 1;
            }
            // Once the marker is removed, an empty UUID directory is the only
            // possible interrupted cleanup artifact and is safe to finish later.
            fs::remove_dir(&path)?;
            namespace_boundary(path.parent().context("Trash directory has no parent")?)?;
        }
        for (hash, path, size) in objects {
            cancelled(flag)?;
            if catalog.objects.contains_key(&hash) {
                continue;
            }
            remove_regular(&path)?;
            report.removed_objects += 1;
            report.removed_bytes = report
                .removed_bytes
                .checked_add(size)
                .context("GC byte count overflow")?;
            observe(Progress {
                phase: "garbageCollection",
                files: report.removed_objects,
                bytes: report.removed_bytes,
            });
        }
        Ok(report)
    }

    fn lifecycle_objects(&self, flag: &AtomicBool) -> Result<Vec<(String, PathBuf, u64)>> {
        let mut budget = 0;
        let mut objects = Vec::new();
        for child in children(&self.directory.join("objects"), &mut budget, flag)? {
            ensure!(name(&child)? == "sha256", "Unknown CAS storage artifact");
        }
        for shard in children(&self.directory.join("objects/sha256"), &mut budget, flag)? {
            let prefix = name(&shard)?;
            ensure!(
                prefix.len() == 2
                    && prefix
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "Unknown CAS shard"
            );
            for path in children(&shard, &mut budget, flag)? {
                let leaf = name(&path)?;
                let meta = safe_path(&path)?.context("Object disappeared")?;
                ensure!(meta.is_file(), "CAS contains a non-file");
                if leaf.starts_with(".tmp-") {
                    objects.push((String::new(), path, meta.len()));
                    continue;
                }
                let hash = format!("{prefix}{leaf}");
                digest(&hash)?;
                objects.push((hash, path, meta.len()));
            }
        }
        Ok(objects)
    }

    fn lifecycle_trash(&self, catalog: &Catalog, flag: &AtomicBool) -> Result<Vec<PathBuf>> {
        let mut budget = 0;
        let mut deletions = Vec::new();
        let mut metadata_bytes = 0;
        for path in children(&self.directory.join("trash"), &mut budget, flag)? {
            let artifact = name(&path)?;
            if let Some(value) = artifact.strip_prefix("repair-") {
                id(value)?;
                let entries = children(&path, &mut budget, flag)?;
                if entries
                    .iter()
                    .all(|entry| name(entry).is_ok_and(|n| n.starts_with(".tmp-")))
                {
                    for entry in &entries {
                        ensure!(
                            safe_path(entry)?.is_some_and(|m| m.is_file()),
                            "Unsafe unpublished diagnostic"
                        );
                    }
                    deletions.push(path);
                    continue;
                }
                // Diagnostic quarantine is deliberately retained, not silently GC'd.
                document_budget(&path.join("repair.json"), &mut metadata_bytes)?;
                validate_repair_archive(&path, &self.binding)?;
                for entry in entries {
                    ensure!(
                        matches!(name(&entry)?, "repair.json" | "corrupt-object")
                            && safe_path(&entry)?.is_some_and(|m| m.is_file()),
                        "Unknown repair diagnostic artifact"
                    );
                }
                continue;
            }
            let value = artifact
                .strip_prefix("delete-")
                .context("Unknown lifecycle trash artifact")?;
            id(value)?;
            let entries = children(&path, &mut budget, flag)?;
            if entries
                .iter()
                .all(|entry| name(entry).is_ok_and(|n| n.starts_with(".tmp-")))
            {
                for entry in &entries {
                    ensure!(
                        safe_path(entry)?.is_some_and(|m| m.is_file()),
                        "Unsafe unpublished deletion"
                    );
                }
                deletions.push(path);
                continue;
            }
            document_budget(&path.join("deletion.json"), &mut metadata_bytes)?;
            let record: Deletion = json(&path.join("deletion.json"))?;
            ensure!(
                record.schema_version == SCHEMA_VERSION
                    && record.roots == self.binding
                    && record.trash_id == value,
                "Deletion record binding differs"
            );
            id(&record.expected.session_id)?;
            id(&record.expected.checkpoint_id)?;
            let moved = path.join("session");
            for entry in entries {
                ensure!(
                    matches!(name(&entry)?, "deletion.json" | "session")
                        || (name(&entry)?.starts_with(".tmp-")
                            && safe_path(&entry)?.is_some_and(|m| m.is_file())),
                    "Unknown deletion artifact"
                );
            }
            if safe_path(&moved)?.is_some() {
                ensure!(
                    !catalog.sessions.contains_key(&record.expected.session_id)
                        && !catalog.incomplete.contains(&record.expected.session_id),
                    "Deleted session identity is still live"
                );
                ensure!(
                    catalog.active.as_deref() != Some(&record.expected.session_id),
                    "Deleted session is still active"
                );
                for (session, _, _) in catalog.sessions.values() {
                    ensure!(
                        session.parent_session_id.as_deref() != Some(&record.expected.session_id)
                            && session.safety_session_id.as_deref()
                                != Some(&record.expected.session_id),
                        "Deletion is referenced by retained lineage"
                    );
                }
                for entry in children(&moved, &mut budget, flag)? {
                    match name(&entry)? {
                        "session.json" => {
                            document_budget(&entry, &mut metadata_bytes)?;
                            let session: Session = json(&entry)?;
                            ensure!(
                                session.schema_version == SCHEMA_VERSION
                                    && super::persistence::known_origin(&session.origin)
                                    && session.session_id == record.expected.session_id
                                    && session.roots == self.binding,
                                "Deleted session binding differs"
                            );
                        }
                        "HEAD.json" => {
                            document_budget(&entry, &mut metadata_bytes)?;
                            let head: Head = json(&entry)?;
                            ensure!(
                                head.schema_version == SCHEMA_VERSION
                                    && head.checkpoint_id == record.expected.checkpoint_id
                                    && head.generation == record.expected.generation,
                                "Deleted HEAD binding differs"
                            );
                        }
                        "checkpoints" => {
                            for document_path in children(&entry, &mut budget, flag)? {
                                if name(&document_path)?.starts_with(".tmp-") {
                                    ensure!(
                                        safe_path(&document_path)?.is_some_and(|m| m.is_file()),
                                        "Unsafe deleted temporary artifact"
                                    );
                                    continue;
                                }
                                let checkpoint_id = name(&document_path)?
                                    .strip_suffix(".json")
                                    .context("Unknown deleted checkpoint artifact")?;
                                id(checkpoint_id)?;
                                document_budget(&document_path, &mut metadata_bytes)?;
                                let checkpoint: Checkpoint = json(&document_path)?;
                                validate_uncommitted(
                                    &checkpoint,
                                    &self.binding,
                                    &record.expected.session_id,
                                    checkpoint_id,
                                )?;
                            }
                        }
                        value if value.starts_with(".tmp-") => {
                            ensure!(
                                safe_path(&entry)?.is_some_and(|m| m.is_file()),
                                "Unsafe deleted temporary artifact"
                            );
                        }
                        _ => anyhow::bail!("Unknown deleted session artifact"),
                    }
                }
            }
            // Close any interrupted rename's durability before reclaiming its bytes.
            namespace_boundary(&self.directory.join("sessions"))?;
            namespace_boundary(&path)?;
            deletions.push(path);
        }
        Ok(deletions)
    }

    pub fn repair(
        &self,
        lock: &recovery::Lock,
        session: &str,
        checkpoint: Option<&str>,
        flag: &AtomicBool,
        mut observe: impl FnMut(Progress),
    ) -> Result<RepairReport> {
        self.require_lock(lock)?;
        // Pending intents are metadata authority even when their prepared branch
        // has no HEAD yet. Repair never infers authority from arbitrary orphans.
        let pending = self.pending_operations()?;
        let transition = pending.iter().find(|operation| {
            operation
                .transition
                .as_ref()
                .is_some_and(|binding| binding.branch_session_id == session)
        });
        let prepared_only =
            safe_path(&self.session_directory(session)?.join("HEAD.json"))?.is_none();
        let (session_doc, head, history) = if prepared_only {
            let operation =
                transition.context("Repair session has no committed or pending authority")?;
            let binding = operation
                .transition
                .as_ref()
                .context("Transition binding missing")?;
            let document = self.read_checkpoint(session, &binding.branch_checkpoint_id)?;
            let session_doc = self.read_session(session)?;
            let head = Head {
                schema_version: SCHEMA_VERSION,
                generation: 1,
                checkpoint_id: document.checkpoint_id.clone(),
                status: "Prepared".into(),
                operation_id: operation.operation_id.clone(),
                finalization_receipt: None,
                transition_operation: None,
            };
            (session_doc, head, vec![document])
        } else {
            self.checked_history(session)?
        };
        let checkpoint_id = checkpoint.unwrap_or(&head.checkpoint_id);
        let document = history
            .iter()
            .find(|c| c.checkpoint_id == checkpoint_id)
            .context("Repair checkpoint is not committed or pending ancestry")?;
        let mut targets = vec![(session_doc.clone(), document.clone())];
        let mut receipts = Vec::new();
        if let Some(receipt) = &head.finalization_receipt {
            receipts.push((
                session_doc.clone(),
                history
                    .last()
                    .context("Current checkpoint missing")?
                    .clone(),
                receipt.clone(),
            ));
        }
        for operation in &pending {
            let belongs = operation.session_id == session
                || operation
                    .transition
                    .as_ref()
                    .is_some_and(|binding| binding.branch_session_id == session);
            if !belongs {
                continue;
            }
            let (source, _, source_history) = self.checked_history(&operation.session_id)?;
            let pending_parent = source_history
                .iter()
                .find(|c| c.checkpoint_id == operation.from_checkpoint_id)
                .context("Pending operation parent missing")?;
            targets.push((source.clone(), pending_parent.clone()));
            if let Some(receipt) = &operation.finalization_receipt {
                let current = source_history
                    .iter()
                    .find(|c| c.checkpoint_id == operation.from_checkpoint_id)
                    .context("Pending receipt checkpoint missing")?;
                receipts.push((source.clone(), current.clone(), receipt.clone()));
            }
            if let Some(binding) = &operation.transition {
                let target = source_history
                    .iter()
                    .find(|c| c.checkpoint_id == binding.target_checkpoint_id)
                    .context("Pending restore target missing")?;
                targets.push((source, target.clone()));
                targets.push((
                    self.read_session(&binding.branch_session_id)?,
                    self.read_checkpoint(
                        &binding.branch_session_id,
                        &binding.branch_checkpoint_id,
                    )?,
                ));
                if let Some(safety) = &binding.safety_session_id {
                    let (safety_session, _, safety_history) = self.checked_history(safety)?;
                    for safety_checkpoint in safety_history {
                        targets.push((safety_session.clone(), safety_checkpoint));
                    }
                }
            }
        }
        let mut wanted = BTreeMap::new();
        let mut candidates: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
        for (context, current) in &targets {
            add_objects(&mut wanted, super::auxiliary::objects(current))?;
            live_candidates(&mut candidates, &self.roots.mods, &current.mods);
            live_candidates(&mut candidates, &self.roots.game, &current.game);
            for (key, snapshot) in &current.auxiliary_snapshots {
                let binding = &context.participants[key];
                match snapshot {
                    AuxiliarySnapshot::File { sha256, .. } => candidates
                        .entry(sha256.clone())
                        .or_default()
                        .push(PathBuf::from(&binding.path)),
                    AuxiliarySnapshot::Directory { tree } => {
                        live_candidates(&mut candidates, Path::new(&binding.path), tree)
                    }
                    AuxiliarySnapshot::Absent => {}
                }
            }
        }
        let mut receipt_sources = BTreeMap::new();
        for (context, current, receipt) in receipts {
            add_objects(&mut wanted, [(receipt.sha256.clone(), receipt.size)])?;
            let (bytes, expected) = super::finalize::expected_receipt(
                &self.roots,
                &context,
                &current,
                receipt.install_report.as_ref(),
            )?;
            ensure!(
                expected == receipt,
                "Pending repair receipt differs from immutable metadata"
            );
            receipt_sources.insert(receipt.sha256, bytes);
        }
        let objects = self.lifecycle_objects(flag)?;
        let mut report = RepairReport {
            schema_version: SCHEMA_VERSION,
            operation: "repair",
            session_id: session.into(),
            checkpoint_id: checkpoint_id.into(),
            generation: head.generation,
            complete: true,
            repaired_objects: 0,
            already_valid_objects: 0,
            unresolved: Vec::new(),
        };
        let mut remaining = wanted.into_iter();
        while let Some((hash, size)) = remaining.next() {
            if flag.load(std::sync::atomic::Ordering::Relaxed) {
                report.complete = false;
                report
                    .unresolved
                    .extend(std::iter::once((hash, size)).chain(remaining).map(
                        |(sha256, size)| RepairIssue {
                            sha256,
                            size,
                            reason: "Repair cancelled before this object was verified".into(),
                        },
                    ));
                break;
            }
            let destination = self.object_path(&hash)?;
            let current_valid = if safe_path(&destination)?.is_some() {
                match hash_file(&destination, flag) {
                    Ok(actual) => actual == (hash.clone(), size),
                    Err(error) if !flag.load(std::sync::atomic::Ordering::Relaxed) => {
                        report.complete = false;
                        report.unresolved.push(RepairIssue {
                            sha256: hash,
                            size,
                            reason: format!("Cannot verify existing object: {error:#}"),
                        });
                        continue;
                    }
                    Err(_) => false,
                }
            } else {
                false
            };
            if current_valid {
                report.already_valid_objects += 1;
                continue;
            }
            let mut verified_source = None;
            for candidate in candidates.get(&hash).into_iter().flatten().chain(
                objects
                    .iter()
                    .filter(|(_, _, length)| *length == size)
                    .map(|(_, path, _)| path),
            ) {
                if flag.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                if candidate == &destination {
                    continue;
                }
                if safe_path(candidate)
                    .ok()
                    .flatten()
                    .is_some_and(|m| m.is_file() && m.len() == size)
                    && hash_file(candidate, flag).is_ok_and(|actual| actual == (hash.clone(), size))
                {
                    verified_source = Some(candidate.clone());
                    break;
                }
            }
            let receipt_bytes = receipt_sources.get(&hash).map(Vec::as_slice);
            if verified_source.is_none() && receipt_bytes.is_none() {
                report.unresolved.push(RepairIssue {
                    sha256: hash,
                    size,
                    reason: "No accessible source matches the retained SHA256 and length".into(),
                });
                report.complete = false;
                continue;
            }
            if let Err(error) =
                self.repair_object(&hash, size, verified_source.as_deref(), receipt_bytes, flag)
            {
                report.complete = false;
                report.unresolved.push(RepairIssue {
                    sha256: hash,
                    size,
                    reason: format!(
                        "Repair did not finish verified durable publication: {error:#}"
                    ),
                });
                report
                    .unresolved
                    .extend(remaining.map(|(sha256, size)| {
                        RepairIssue {
                            sha256,
                            size,
                            reason:
                                "Repair stopped after the previous object failed or was cancelled"
                                    .into(),
                        }
                    }));
                break;
            }
            report.repaired_objects += 1;
            observe(Progress {
                phase: "repair",
                files: report.repaired_objects,
                bytes: size,
            });
        }
        ensure!(
            self.pending_operations()? == pending,
            "Pending checkpoint authority changed during repair"
        );
        if !prepared_only {
            ensure!(
                self.read_head(session)? == head,
                "Checkpoint HEAD changed during repair"
            );
        } else {
            ensure!(
                safe_path(&self.session_directory(session)?.join("HEAD.json"))?.is_none(),
                "Prepared session HEAD changed during repair"
            );
        }
        Ok(report)
    }

    fn repair_object(
        &self,
        hash: &str,
        size: u64,
        source: Option<&Path>,
        bytes: Option<&[u8]>,
        flag: &AtomicBool,
    ) -> Result<()> {
        let destination = self.object_path(hash)?;
        let parent = destination.parent().context("Object has no shard")?;
        ensure_directory(parent)?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".tmp-")
            .tempfile_in(parent)?;
        let mut reader: Box<dyn Read> = match source {
            Some(path) => Box::new(open_file(path)?),
            None => Box::new(std::io::Cursor::new(
                bytes.context("Repair source missing")?.to_vec(),
            )),
        };
        let mut digest = Sha256::new();
        let mut count = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            cancelled(flag)?;
            let length = reader.read(&mut buffer)?;
            if length == 0 {
                break;
            }
            count = count
                .checked_add(length as u64)
                .context("Repair length overflow")?;
            ensure!(count <= size, "Repair source grew");
            temporary.write_all(&buffer[..length])?;
            digest.update(&buffer[..length]);
        }
        ensure!(
            count == size && format!("{:x}", digest.finalize()) == hash,
            "Repair source changed before publication"
        );
        super::persistence::flush_file(temporary.as_file())?;
        let diagnostic_id = uuid::Uuid::new_v4().to_string();
        let archive = self
            .directory
            .join("trash")
            .join(format!("repair-{diagnostic_id}"));
        ensure_directory(&archive)?;
        let existed = safe_path(&destination)?.is_some();
        publish_json(
            &archive.join("repair.json"),
            &RepairDiagnostic {
                schema_version: SCHEMA_VERSION,
                roots: self.binding.clone(),
                sha256: hash.into(),
                size,
                replaced_corrupt_object: existed,
            },
        )?;
        if existed {
            super::persistence::rename(&destination, &archive.join("corrupt-object"), false)?;
            namespace_boundary(parent)?;
            namespace_boundary(&archive)?;
        }
        // No cancellation between quarantine and publication: finish this one repair.
        super::persistence::publish_temp(temporary, &destination, false)?;
        namespace_boundary(parent)?;
        let finishing = AtomicBool::new(false);
        ensure!(
            hash_file(&destination, &finishing)? == (hash.into(), size),
            "Repaired object failed verification"
        );
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RepairDiagnostic {
    schema_version: u32,
    roots: RootBinding,
    sha256: String,
    size: u64,
    replaced_corrupt_object: bool,
}
fn validate_repair_archive(path: &Path, binding: &RootBinding) -> Result<()> {
    let record: RepairDiagnostic = json(&path.join("repair.json"))?;
    ensure!(
        record.schema_version == SCHEMA_VERSION && &record.roots == binding,
        "Repair diagnostic binding differs"
    );
    digest(&record.sha256)?;
    Ok(())
}
fn live_candidates(output: &mut BTreeMap<String, Vec<PathBuf>>, root: &Path, tree: &Tree) {
    for (relative, entry) in &tree.entries {
        if let Entry::File { sha256, .. } = entry {
            output
                .entry(sha256.clone())
                .or_default()
                .push(root.join(relative));
        }
    }
}
fn remove_regular(path: &Path) -> Result<()> {
    ensure!(
        safe_path(path)?.is_some_and(|m| m.is_file()),
        "Refusing removal of unsafe artifact"
    );
    super::persistence::writable_file(path)?;
    fs::remove_file(path)?;
    namespace_boundary(path.parent().context("Artifact has no parent")?)
}
fn remove_tree(root: &Path, flag: &AtomicBool) -> Result<u64> {
    // Inventory the whole subtree first. WalkDir never follows links, and every
    // removal rechecks its path. No unchecked recursive deletion primitive.
    let mut entries = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false).max_depth(5) {
        cancelled(flag)?;
        ensure!(
            entries.len() < INVENTORY_LIMIT,
            "Trash removal exceeds bound"
        );
        let path = entry?.path().to_path_buf();
        let metadata = safe_path(&path)?.context("Trash artifact disappeared")?;
        entries.push((path, metadata.is_dir()));
    }
    let mut count = 0;
    for (path, directory) in entries.into_iter().rev() {
        cancelled(flag)?;
        let meta = safe_path(&path)?.context("Trash artifact disappeared")?;
        ensure!(meta.is_dir() == directory, "Trash artifact type changed");
        if directory {
            fs::remove_dir(&path)?;
            namespace_boundary(path.parent().context("Trash has no parent")?)?;
        } else {
            remove_regular(&path)?;
        }
        count += 1;
    }
    Ok(count)
}

fn validate_uncommitted(
    document: &Checkpoint,
    binding: &RootBinding,
    session_id: &str,
    checkpoint_id: &str,
) -> Result<()> {
    ensure!(
        document.schema_version == SCHEMA_VERSION
            && &document.roots == binding
            && document.session_id == session_id
            && document.checkpoint_id == checkpoint_id
            && super::persistence::known_origin(&document.origin),
        "Uncommitted checkpoint identity differs"
    );
    let (mut files, mut bytes) = validate_tree(&document.mods)?;
    let (game_files, game_bytes) = validate_tree(&document.game)?;
    files = files
        .checked_add(game_files)
        .context("Checkpoint count overflow")?;
    bytes = bytes
        .checked_add(game_bytes)
        .context("Checkpoint size overflow")?;
    for snapshot in document.auxiliary_snapshots.values() {
        let (count, length) = match snapshot {
            AuxiliarySnapshot::Directory { tree } => validate_tree(tree)?,
            AuxiliarySnapshot::File { sha256, size, mode } => {
                digest(sha256)?;
                validate_tree(&Tree {
                    root_mode: mode.clone(),
                    entries: BTreeMap::new(),
                })?;
                (1, *size)
            }
            AuxiliarySnapshot::Absent => (0, 0),
        };
        files = files
            .checked_add(count)
            .context("Checkpoint count overflow")?;
        bytes = bytes
            .checked_add(length)
            .context("Checkpoint size overflow")?;
    }
    ensure!(
        document.auxiliary.is_empty()
            && document.file_count == files
            && document.total_bytes == bytes,
        "Uncommitted checkpoint totals or auxiliary format differ"
    );
    Ok(())
}
