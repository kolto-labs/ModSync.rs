use super::{model::*, read::*};
use crate::{paths::Roots, recovery};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

pub struct Store {
    pub(super) roots: Roots,
    pub(super) directory: PathBuf,
    pub(super) binding: RootBinding,
}

impl Store {
    /// Validate location and root bindings without creating directories or lock files.
    pub fn open(roots: &Roots) -> Result<Self> {
        let canonical = Roots::new(&roots.mods, &roots.game)?;
        ensure!(
            canonical.mods == roots.mods && canonical.game == roots.game,
            "Checkpoint roots must already be canonical"
        );
        let binding = RootBinding {
            mods: roots
                .mods
                .to_str()
                .context("Checkpoint v1 cannot encode non-UTF8 mod roots")?
                .into(),
            game: roots
                .game
                .to_str()
                .context("Checkpoint v1 cannot encode non-UTF8 game roots")?
                .into(),
        };
        let store = Self {
            roots: canonical,
            directory: recovery::state_dir(roots).join("checkpoints-v1"),
            binding,
        };
        store.validate_location()?;
        Ok(store)
    }

    pub(super) fn validate_location(&self) -> Result<()> {
        for root in [&self.roots.mods, &self.roots.game] {
            ensure!(
                !self.directory.starts_with(root) && !root.starts_with(&self.directory),
                "Checkpoint store overlaps a captured root"
            );
            ensure!(
                safe_path(root)?.is_some_and(|m| m.is_dir()),
                "Checkpoint root is missing or unsafe"
            );
            ensure!(
                fs::canonicalize(root)? == *root,
                "Checkpoint root binding changed"
            );
        }
        if let Some(meta) = safe_path(&self.directory)? {
            ensure!(meta.is_dir(), "Checkpoint store is not a directory");
            let mut count = 0;
            for entry in fs::read_dir(&self.directory)? {
                count += 1;
                ensure!(
                    count <= 10_000,
                    "Checkpoint storage inventory exceeds bound"
                );
                let entry = entry?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("Non-UTF8 checkpoint artifact name"))?;
                ensure!(
                    matches!(
                        name.as_str(),
                        "objects" | "sessions" | "operations" | "trash" | "active.json"
                    ) || name.starts_with(".tmp-"),
                    "Unknown/foreign checkpoint artifact: {name}"
                );
                safe_path(&entry.path())?.context("Checkpoint artifact disappeared")?;
            }
        }
        for child in [
            "objects",
            "objects/sha256",
            "sessions",
            "operations",
            "trash",
        ] {
            if let Some(meta) = safe_path(&self.directory.join(child))? {
                ensure!(meta.is_dir(), "Checkpoint storage child is not a directory");
            }
        }
        if let Some(meta) = safe_path(&self.directory.join("active.json"))? {
            ensure!(meta.is_file(), "Checkpoint selector is not a regular file");
        }
        Ok(())
    }

    /// Creates a new independent baseline. Legacy recovery state is neither cleared
    /// nor interpreted as a completed prefix. The only root lock is acquired here.
    pub fn capture(
        &self,
        flag: &AtomicBool,
        observe: impl FnMut(Progress),
    ) -> Result<CaptureReport> {
        super::persistence::writer_origin()?;
        cancelled(flag)?;
        self.prepare_write()?;
        let lock = recovery::Lock::acquire(&self.roots)?;
        self.capture_locked(
            &lock,
            &InstallContext::default(),
            "baseline",
            None,
            flag,
            observe,
        )
    }

    /// T3 opt-in entry: the engine already holds both root locks. It supplies full,
    /// ordered definitions; no completed prefix is accepted from the caller.
    pub fn begin_session(
        &self,
        lock: &recovery::Lock,
        definitions: &[serde_json::Value],
        flag: &AtomicBool,
        observe: impl FnMut(Progress),
    ) -> Result<CaptureReport> {
        self.begin_session_with_context(
            lock,
            &InstallContext {
                ordered_components: definitions.to_vec(),
                ..InstallContext::default()
            },
            flag,
            observe,
        )
    }

    pub fn begin_session_with_context(
        &self,
        lock: &recovery::Lock,
        context: &InstallContext,
        flag: &AtomicBool,
        observe: impl FnMut(Progress),
    ) -> Result<CaptureReport> {
        #[cfg(windows)]
        ensure!(
            context.participants.is_empty()
                || lock.guards_checkpoint_context(&canonical_bytes(&context.participants)?),
            "New Windows participants require acquire_lock_with_context before any lock/state writes"
        );
        self.ensure_no_unfinished_install_sessions()?;
        super::history::validate_definitions(&context.ordered_components)?;
        ensure!(
            context.initial_completed_components.len() <= context.ordered_components.len()
                && canonical_bytes(&context.initial_completed_components)?
                    == canonical_bytes(
                        &context.ordered_components[..context.initial_completed_components.len()]
                    )?,
            "Initial completed components are not the exact definition prefix"
        );
        let mut report = self.capture_locked(lock, context, "install", None, flag, observe)?;
        report.operation = "beginSession";
        Ok(report)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn capture_locked(
        &self,
        lock: &recovery::Lock,
        context: &InstallContext,
        scope: &str,
        saved_shells: Option<&[String]>,
        flag: &AtomicBool,
        mut observe: impl FnMut(Progress),
    ) -> Result<CaptureReport> {
        self.require_lock(lock)?;
        self.qualify_participants(
            &context.participants,
            &self.participant_shells(&context.participants)?,
        )?;
        cancelled(flag)?;
        self.ensure_no_pending()?;
        self.validate_location()?;
        let state = self
            .directory
            .parent()
            .context("Checkpoint state has no parent")?;
        namespace_boundary(state.parent().context("Recovery directory has no parent")?)?;
        namespace_boundary(state)?;
        ensure_directory(&self.directory)?;
        ensure_directory(&self.directory.join("objects/sha256"))?;
        ensure_directory(&self.directory.join("sessions"))?;
        let shells = match saved_shells {
            Some(shells) => shells.to_vec(),
            None => self.participant_shells(&context.participants)?,
        };
        // A safety session records the folders as they are, so recovery and restore
        // still work when an older store put a device name on disk.
        let (mods, game, auxiliary_snapshots, ancestor_shells, count, bytes) = self.capture_all(
            &context.participants,
            &shells,
            scope != "safety",
            flag,
            &mut observe,
        )?;
        if !context.initial_completed_components.is_empty() {
            recovery::verify_completion_manifest(
                &self.roots,
                &context.initial_completed_components,
                &super::finalize::game_files(&game),
            )?;
        }
        let session_id = uuid::Uuid::new_v4().to_string();
        let checkpoint_id = uuid::Uuid::new_v4().to_string();
        let operation_id = uuid::Uuid::new_v4().to_string();
        let session = Session {
            schema_version: SCHEMA_VERSION,
            session_id: session_id.clone(),
            created_utc: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            roots: self.binding.clone(),
            install_scope: scope.into(),
            ordered_components: context.ordered_components.clone(),
            initial_completed_components: context.initial_completed_components.clone(),
            participants: context.participants.clone(),
            ancestor_shells: shells,
            definitions_sha256: Some(super::history::definition_hash(
                &context.ordered_components,
            )?),
            parent_session_id: None,
            parent_checkpoint_id: None,
            origin: super::persistence::writer_origin()?.into(),
            branch_kind: None,
            safety_session_id: None,
        };
        let checkpoint = Checkpoint {
            schema_version: SCHEMA_VERSION,
            checkpoint_id: checkpoint_id.clone(),
            session_id: session_id.clone(),
            parent_id: None,
            sequence: 0,
            kind: "Baseline".into(),
            completed_components: context.initial_completed_components.clone(),
            completed_sha256: Some(super::history::definition_hash(
                &context.initial_completed_components,
            )?),
            component: None,
            roots: self.binding.clone(),
            mods,
            game,
            auxiliary: BTreeMap::new(),
            auxiliary_snapshots,
            ancestor_shells,
            file_count: count,
            total_bytes: bytes,
            creator: concat!("ModSync.rs/", env!("CARGO_PKG_VERSION")).into(),
            origin: super::persistence::writer_origin()?.into(),
        };
        let head = Head {
            schema_version: SCHEMA_VERSION,
            generation: 1,
            checkpoint_id: checkpoint_id.clone(),
            status: "Ready".into(),
            operation_id,
            finalization_receipt: None,
            transition_operation: None,
        };
        let directory = self.session_directory(&session_id)?;
        // A session UUID collision never reuses another directory.
        ensure!(
            safe_path(&directory)?.is_none(),
            "Checkpoint session UUID collision"
        );
        fs::create_dir(&directory)?;
        namespace_boundary(&self.directory.join("sessions"))?;
        ensure_directory(&directory.join("checkpoints"))?;
        publish_json(
            &directory
                .join("checkpoints")
                .join(format!("{checkpoint_id}.json")),
            &checkpoint,
        )?;
        cancelled(flag)?;
        publish_json(&directory.join("session.json"), &session)?;
        // Re-read producer output and rehash the closure before the authoritative HEAD.
        self.read_session(&session_id)?;
        let persisted = self.read_checkpoint(&session_id, &checkpoint_id)?;
        for (sha256, size) in super::auxiliary::objects(&persisted) {
            ensure!(
                hash_file(&self.object_path(&sha256)?, flag)? == (sha256, size),
                "Object changed before baseline publication"
            );
        }
        cancelled(flag)?;
        publish_json(&directory.join("HEAD.json"), &head)?;
        // HEAD is the commit point. Complete selector publication and report the
        // committed checkpoint even if cancellation arrives after that point.
        replace_json(
            &self.directory.join("active.json"),
            &serde_json::json!({"sessionId": session_id}),
        )?;
        observe(Progress {
            phase: "committed",
            files: count,
            bytes,
        });
        Ok(CaptureReport {
            schema_version: SCHEMA_VERSION,
            operation: "capture",
            session_id,
            checkpoint_id,
            generation: 1,
            file_count: count,
            total_bytes: bytes,
            durability: super::persistence::durability(super::persistence::writer_origin()?)?,
        })
    }

    pub(super) fn require_lock(&self, lock: &recovery::Lock) -> Result<()> {
        super::persistence::writer_origin()?;
        ensure!(
            lock.guards(&self.roots),
            "Checkpoint mutation requires the matching held root locks"
        );
        super::ensure_external_process_clear(&self.roots.game)?;
        self.prepare_write()
    }

    /// `new_session` refuses every Windows device name before anything is stored, so a
    /// new session's first capture records none; commits and resumes record what is on
    /// disk under the older rule (`validate_tree`).
    pub(super) fn capture_trees(
        &self,
        new_session: bool,
        flag: &AtomicBool,
        observe: &mut impl FnMut(Progress),
    ) -> Result<(Tree, Tree, u64, u64)> {
        observe(Progress {
            phase: "inventory",
            files: 0,
            bytes: 0,
        });
        // On a cold CAS, stream the source bytes into a temporary object while
        // hashing them. This avoids rereading every source file to publish its
        // newly-created object. Once any object exists, use the normal scan so
        // later captures do not stage unchanged files unnecessarily.
        let stage_new_objects = self.object_store_is_empty(flag)?;
        let mods = self.scan_capture_root(&self.roots.mods, stage_new_objects, flag)?;
        let game = self.scan_capture_root(&self.roots.game, stage_new_objects, flag)?;
        if new_session {
            refuse_device_names(&mods)?;
            refuse_device_names(&game)?;
        }
        let mut count = 0u64;
        let mut bytes = 0u64;
        let mut copied = std::collections::BTreeSet::new();
        for (root, tree) in [(&self.roots.mods, &mods), (&self.roots.game, &game)] {
            for (path, entry) in &tree.entries {
                cancelled(flag)?;
                if let Entry::File { sha256, size, .. } = entry {
                    if copied.insert((sha256.clone(), *size)) && !stage_new_objects {
                        self.publish_object(&root.join(path), sha256, *size, flag)?;
                    }
                    count += 1;
                    bytes = bytes
                        .checked_add(*size)
                        .context("Checkpoint size overflow")?;
                    observe(Progress {
                        phase: "objects",
                        files: count,
                        bytes,
                    });
                }
            }
        }
        observe(Progress {
            phase: "consistency",
            files: count,
            bytes,
        });
        ensure!(
            self.scan(&self.roots.mods, flag)? == mods
                && self.scan(&self.roots.game, flag)? == game,
            "Captured roots changed during baseline capture; no checkpoint committed"
        );
        cancelled(flag)?;
        Ok((mods, game, count, bytes))
    }

    pub(super) fn scan(&self, root: &Path, flag: &AtomicBool) -> Result<Tree> {
        self.scan_capture_root(root, false, flag)
    }

    fn scan_capture_root(
        &self,
        root: &Path,
        stage_new_objects: bool,
        flag: &AtomicBool,
    ) -> Result<Tree> {
        cancelled(flag)?;
        let metadata = safe_path(root)?.context("Capture root missing")?;
        ensure!(metadata.is_dir(), "Capture root is not a directory");
        let mut tree = Tree {
            root_mode: mode(&metadata),
            entries: BTreeMap::new(),
        };
        for entry in walkdir::WalkDir::new(root).min_depth(1).follow_links(false) {
            cancelled(flag)?;
            ensure!(
                tree.entries.len() < MAX_ENTRIES,
                "Checkpoint tree exceeds {MAX_ENTRIES} entries"
            );
            let entry = entry?;
            ensure!(
                entry.path().starts_with(root),
                "Captured entry escaped its bound root"
            );
            let metadata = safe_path(entry.path())?.context("Capture path disappeared")?;
            let relative = entry
                .path()
                .strip_prefix(root)?
                .to_str()
                .context("Checkpoint v1 cannot encode non-UTF8 paths")?
                .replace(std::path::MAIN_SEPARATOR, "/");
            let value = if metadata.is_dir() {
                Entry::Directory {
                    mode: mode(&metadata),
                }
            } else {
                let (sha256, size) = if stage_new_objects {
                    self.stage_hashed_object(entry.path(), metadata.len(), flag)?
                } else {
                    hash_file(entry.path(), flag)?
                };
                ensure!(
                    size == metadata.len(),
                    "File length changed during checkpoint scan"
                );
                Entry::File {
                    sha256,
                    size,
                    mode: mode(&metadata),
                }
            };
            ensure!(
                tree.entries.insert(relative, value).is_none(),
                "Duplicate checkpoint path"
            );
        }
        validate_tree(&tree)?;
        Ok(tree)
    }

    fn object_store_is_empty(&self, flag: &AtomicBool) -> Result<bool> {
        let root = self.directory.join("objects/sha256");
        let Some(metadata) = safe_path(&root)? else {
            return Ok(true);
        };
        ensure!(metadata.is_dir(), "CAS object root is not a directory");
        let mut inspected = 0usize;
        for shard in fs::read_dir(&root)? {
            cancelled(flag)?;
            inspected += 1;
            ensure!(inspected <= 200_000, "CAS inventory exceeds bound");
            let shard = shard?;
            let path = shard.path();
            let prefix = shard
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-UTF8 CAS shard name"))?;
            ensure!(
                prefix.len() == 2
                    && prefix
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "Unknown CAS shard"
            );
            let shard_metadata = safe_path(&path)?.context("CAS shard disappeared")?;
            ensure!(shard_metadata.is_dir(), "CAS shard is not a directory");
            for object in fs::read_dir(&path)? {
                cancelled(flag)?;
                inspected += 1;
                ensure!(inspected <= 200_000, "CAS inventory exceeds bound");
                let object = object?;
                let object_path = object.path();
                let leaf = object
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("Non-UTF8 CAS object name"))?;
                let object_metadata = safe_path(&object_path)?.context("CAS object disappeared")?;
                ensure!(object_metadata.is_file(), "CAS contains a non-file");
                if leaf.starts_with(".tmp-") {
                    return Ok(false);
                }
                digest(&format!("{prefix}{leaf}"))?;
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn stage_hashed_object(
        &self,
        source: &Path,
        expected_size: u64,
        flag: &AtomicBool,
    ) -> Result<(String, u64)> {
        let mut temporary = tempfile::Builder::new()
            .prefix(".tmp-")
            .tempfile_in(&self.directory)?;
        let mut source = open_file(source)?;
        let mut observed = Sha256::new();
        let mut count = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            cancelled(flag)?;
            let n = source.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count = count
                .checked_add(n as u64)
                .context("Checkpoint object size overflow")?;
            ensure!(
                count <= expected_size,
                "Source grew while capturing checkpoint object"
            );
            temporary.write_all(&buffer[..n])?;
            observed.update(&buffer[..n]);
        }
        let hash = format!("{:x}", observed.finalize());
        ensure!(
            count == expected_size,
            "Source length changed while capturing checkpoint object"
        );
        let destination = self.object_path(&hash)?;
        let parent = destination.parent().expect("object parent");
        ensure_directory(parent)?;
        match super::persistence::publish_temp(temporary, &destination, false) {
            Ok(()) => {
                namespace_boundary(parent)?;
                namespace_boundary(&self.directory)?;
            }
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::AlreadyExists) =>
            {
                // Duplicate content can occur within this cold capture. The
                // winner's object remains authoritative and is verified below.
            }
            Err(error) => return Err(error),
        }
        ensure!(
            hash_file(&destination, flag)? == (hash.clone(), count),
            "Staged checkpoint object failed verification"
        );
        Ok((hash, count))
    }

    pub(super) fn publish_object(
        &self,
        source: &Path,
        hash: &str,
        size: u64,
        flag: &AtomicBool,
    ) -> Result<()> {
        let destination = self.object_path(hash)?;
        if safe_path(&destination)?.is_some() {
            ensure!(
                hash_file(&destination, flag)? == (hash.into(), size),
                "Existing checkpoint object is corrupt; refusing replacement"
            );
            return Ok(());
        }
        let parent = destination.parent().expect("object parent");
        ensure_directory(parent)?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".tmp-")
            .tempfile_in(parent)?;
        let mut source = open_file(source)?;
        let mut observed = Sha256::new();
        let mut count = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            cancelled(flag)?;
            let n = source.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count = count
                .checked_add(n as u64)
                .context("Checkpoint object size overflow")?;
            ensure!(
                count <= size,
                "Source grew while capturing checkpoint object"
            );
            temporary.write_all(&buffer[..n])?;
            observed.update(&buffer[..n]);
        }
        ensure!(
            count == size && format!("{:x}", observed.finalize()) == hash,
            "Source changed while capturing checkpoint object"
        );
        // publish_temp flushes this handle before every publication branch.
        match super::persistence::publish_temp(temporary, &destination, false) {
            Ok(()) => namespace_boundary(parent)?,
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::AlreadyExists) =>
            {
                ensure!(
                    hash_file(&destination, flag)? == (hash.into(), size),
                    "Concurrent checkpoint object differs"
                );
            }
            Err(error) => return Err(error),
        }
        ensure!(
            hash_file(&destination, flag)? == (hash.into(), size),
            "Published checkpoint object failed verification"
        );
        Ok(())
    }
}

pub(crate) use super::persistence::namespace_boundary;
pub(super) fn ensure_directory(path: &Path) -> Result<()> {
    if let Some(meta) = safe_path(path)? {
        ensure!(meta.is_dir(), "Checkpoint directory path is a file");
        return Ok(());
    }
    let parent = path
        .parent()
        .context("Checkpoint directory has no parent")?;
    ensure_directory(parent)?;
    match fs::create_dir(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    ensure!(
        safe_path(path)?.is_some_and(|m| m.is_dir()),
        "Created checkpoint directory is unsafe"
    );
    namespace_boundary(path)?;
    namespace_boundary(parent)
}
pub(super) fn canonical_bytes(value: &(impl Serialize + ?Sized)) -> Result<Vec<u8>> {
    fn sorted(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(object) => {
                let values: BTreeMap<_, _> =
                    object.into_iter().map(|(k, v)| (k, sorted(v))).collect();
                serde_json::Value::Object(values.into_iter().collect())
            }
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(sorted).collect())
            }
            other => other,
        }
    }
    let bytes = serde_json::to_vec(&sorted(serde_json::to_value(value)?))?;
    ensure!(
        bytes.len() as u64 <= MAX_DOCUMENT_BYTES,
        "Checkpoint document exceeds 64 MiB"
    );
    Ok(bytes)
}
pub(super) fn publish_json(path: &Path, value: &impl Serialize) -> Result<()> {
    ensure!(
        safe_path(path)?.is_none(),
        "Immutable checkpoint document already exists"
    );
    let parent = path.parent().context("Checkpoint document has no parent")?;
    ensure_directory(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".tmp-")
        .tempfile_in(parent)?;
    temporary.write_all(&canonical_bytes(value)?)?;
    super::persistence::flush_file(temporary.as_file())?;
    super::persistence::publish_temp(temporary, path, false)?;
    namespace_boundary(parent)
}
pub(super) fn replace_json(path: &Path, value: &impl Serialize) -> Result<()> {
    replace_bytes(path, &canonical_bytes(value)?)
}
pub(super) fn replace_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(metadata) = safe_path(path)? {
        ensure!(metadata.is_file(), "Checkpoint selector is not a file");
    }
    let parent = path.parent().context("Checkpoint selector has no parent")?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".tmp-")
        .tempfile_in(parent)?;
    temporary.write_all(bytes)?;
    super::persistence::flush_file(temporary.as_file())?;
    super::persistence::publish_temp(temporary, path, true)?;
    namespace_boundary(parent)
}
