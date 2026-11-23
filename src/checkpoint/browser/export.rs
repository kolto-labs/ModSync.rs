//! File-backed finalized history export with invocation-local object verification.
use crate::checkpoint::{
    InspectionReport, SCHEMA_VERSION, Store, VerificationReport,
    read::{cancelled, hash_file, safe_path},
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
    sync::atomic::AtomicBool,
};

const MAX_EXPORT_DOCUMENT: u64 = 128 * 1024 * 1024;
const MAX_EXPORT_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_EXPORT_DOCUMENTS: usize = 50_000;
const MAX_EXPORT_OBJECTS: usize = 1_000_000;

struct DocumentWriter {
    file: fs::File,
    hash: Sha256,
    bytes: u64,
}

impl Write for DocumentWriter {
    fn write(&mut self, body: &[u8]) -> io::Result<usize> {
        if self.bytes.saturating_add(body.len() as u64) > MAX_EXPORT_DOCUMENT {
            return Err(io::Error::other(
                "Checkpoint export document exceeds 128 MiB",
            ));
        }
        let count = self.file.write(body)?;
        self.hash.update(&body[..count]);
        self.bytes += count as u64;
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

fn write_document(path: &Path, value: &impl Serialize, total: &mut u64) -> Result<String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut writer = DocumentWriter {
        file: options.open(path)?,
        hash: Sha256::new(),
        bytes: 0,
    };
    serde_json::to_writer(&mut writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    writer.file.sync_all()?;
    *total = total
        .checked_add(writer.bytes)
        .context("Export size overflow")?;
    ensure!(
        *total <= MAX_EXPORT_BYTES,
        "Checkpoint export exceeds 64 GiB"
    );
    Ok(format!("{:x}", writer.hash.finalize()))
}

impl Store {
    /// Authenticate complete finalized lineage and hash each distinct content object once.
    /// The index is published only after all documents, objects and final fences verify.
    pub fn export_history(
        &self,
        session_id: &str,
        checkpoint_id: &str,
        destination: &Path,
        flag: &AtomicBool,
    ) -> Result<serde_json::Value> {
        cancelled(flag)?;
        self.validate_location()?;
        self.ensure_no_pending()?;
        crate::checkpoint::ensure_external_process_clear(&self.roots.game)?;
        let lineage = self.checked_lineage(session_id)?;
        let (_, final_head, _) = lineage.last().context("Empty checkpoint lineage")?;
        ensure!(
            final_head.checkpoint_id == checkpoint_id,
            "Requested checkpoint is not HEAD"
        );
        let parent = destination
            .parent()
            .context("Export directory needs a parent")?;
        ensure!(
            destination.is_absolute()
                && fs::canonicalize(parent)? == parent
                && safe_path(parent)?.is_some_and(|metadata| metadata.is_dir())
                && safe_path(destination)?.is_none(),
            "Export destination must be a new directory with a canonical safe parent"
        );
        for protected in [&self.roots.mods, &self.roots.game, &self.directory] {
            ensure!(
                !destination.starts_with(protected) && !protected.starts_with(destination),
                "Export directory overlaps checkpoint inputs"
            );
        }
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(destination)?;
        let mut verified = BTreeMap::new();
        let mut documents = Vec::new();
        let mut total = 0;
        for (session, head, history) in &lineage {
            cancelled(flag)?;
            ensure!(
                head.status == "Complete" && head.finalization_receipt.is_some(),
                "History export requires every lineage session to be finalized"
            );
            for boundary in &history.entries {
                cancelled(flag)?;
                ensure!(
                    documents.len() + 2 <= MAX_EXPORT_DOCUMENTS,
                    "Export exceeds 50000 documents"
                );
                let checkpoint = history.get(self, &boundary.checkpoint_id)?;
                let mut objects = crate::checkpoint::auxiliary::objects(&checkpoint);
                if let Some(receipt) = &head.finalization_receipt {
                    objects.insert((receipt.sha256.clone(), receipt.size));
                }
                let object_count = objects.len() as u64;
                for (sha256, size) in objects {
                    if let Some(previous_size) = verified.get(&sha256) {
                        ensure!(
                            *previous_size == size,
                            "Object identity has conflicting lengths"
                        );
                        continue;
                    }
                    ensure!(
                        verified.len() < MAX_EXPORT_OBJECTS,
                        "Export exceeds 1000000 distinct objects"
                    );
                    ensure!(
                        hash_file(&self.object_path(&sha256)?, flag)? == (sha256.clone(), size),
                        "Checkpoint object hash or length mismatch: {sha256}"
                    );
                    verified.insert(sha256, size);
                }
                let verification = VerificationReport {
                    schema_version: SCHEMA_VERSION,
                    operation: "verify",
                    session_id: session.session_id.clone(),
                    checkpoint_id: checkpoint.checkpoint_id.clone(),
                    generation: head.generation,
                    verified_objects: object_count,
                    file_count: checkpoint.file_count,
                    total_bytes: checkpoint.total_bytes,
                };
                let inspection = InspectionReport {
                    schema_version: SCHEMA_VERSION,
                    operation: "inspect",
                    session_id: session.session_id.clone(),
                    checkpoint_id: checkpoint.checkpoint_id.clone(),
                    generation: head.generation,
                    head_checkpoint_id: head.checkpoint_id.clone(),
                    session: session.clone(),
                    checkpoint,
                    pending_operations: Vec::new(),
                    finalization_receipt: head.finalization_receipt.clone(),
                };
                let name = format!("checkpoint-command-{:04}.json", documents.len());
                let sha256 = write_document(&destination.join(&name), &inspection, &mut total)?;
                documents.push(serde_json::json!({
                    "sessionId": session.session_id, "checkpointId": boundary.checkpoint_id,
                    "operation": "inspect", "document": name, "sha256": sha256
                }));
                let name = format!("checkpoint-command-{:04}.json", documents.len());
                let sha256 = write_document(&destination.join(&name), &verification, &mut total)?;
                documents.push(serde_json::json!({
                    "sessionId": session.session_id, "checkpointId": boundary.checkpoint_id,
                    "operation": "verify", "document": name, "sha256": sha256
                }));
            }
        }
        cancelled(flag)?;
        self.validate_location()?;
        self.ensure_no_pending()?;
        crate::checkpoint::ensure_external_process_clear(&self.roots.game)?;
        for (session, head, _) in &lineage {
            ensure!(
                self.read_session(&session.session_id)? == *session
                    && self.read_head(&session.session_id)? == *head,
                "Checkpoint HEAD or session changed during export; retry"
            );
        }
        let index = serde_json::json!({
            "schemaVersion": SCHEMA_VERSION, "operation": "exportHistory",
            "sessionId": session_id, "checkpointId": checkpoint_id,
            "verifiedObjects": verified.len(), "documents": documents
        });
        let index_sha256 = write_document(&destination.join("index.json"), &index, &mut total)?;
        Ok(serde_json::json!({
            "schemaVersion": SCHEMA_VERSION, "operation": "exportHistory",
            "sessionId": session_id, "checkpointId": checkpoint_id,
            "verifiedObjects": verified.len(), "indexSha256": index_sha256,
            "documents": documents.len()
        }))
    }
}
