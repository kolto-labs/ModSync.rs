use super::{
    Action, Operation, Outcome, OutcomeStatus, Status, TreeDisposition, check_cancelled, fence,
    plan::{self, Entry, MAX_DOCUMENT, MAX_ENTRIES, Plan, Work},
    platform,
};
use crate::{paths::Roots, recovery::Lock};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Intent {
    version: u32,
    digest: String,
    nonce: String,
    plan: Plan,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    terminal: Option<Cursor>,
    progress_origin: Cursor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Direction {
    Applying,
    RollingBack,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Step {
    RenameToTemporary,
    RenameToDestination,
    Chmod,
    UndoRenameToTemporary,
    UndoRenameToOriginal,
    UndoChmod,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Inflight {
    index: usize,
    step: Step,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    #[serde(rename = "c")]
    completed: usize,
    #[serde(rename = "p")]
    direction: Direction,
    #[serde(rename = "i")]
    inflight: Option<Inflight>,
}

fn directory(roots: &Roots) -> PathBuf {
    crate::recovery::state_dir(roots).join("maintenance-v1")
}

fn safe_state(path: &Path) -> Result<fs::Metadata> {
    let metadata =
        crate::checkpoint::safe_path(path)?.context("Maintenance state artifact disappeared")?;
    platform::qualify(path, &metadata)?;
    Ok(metadata)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let metadata = safe_state(path)?;
    platform::qualify_private(path, &metadata)?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_DOCUMENT as u64,
        "Maintenance journal is not a bounded regular file"
    );
    let mut bytes = Vec::new();
    let identity = platform::identity_at(path, &metadata)?;
    let mut file = crate::paths::open_regular_read(path)?;
    ensure!(
        platform::identity_file(&file)? == identity,
        "Maintenance metadata changed during open"
    );
    Read::by_ref(&mut file)
        .take(MAX_DOCUMENT as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_DOCUMENT,
        "Maintenance journal exceeds 64 MiB"
    );
    let after = safe_state(path)?;
    ensure!(
        platform::identity_at(path, &after)? == identity
            && after.len() == metadata.len()
            && after.modified()? == metadata.modified()?,
        "Maintenance metadata changed during read"
    );
    Ok(bytes)
}

fn artifacts(roots: &Roots) -> Result<bool> {
    let directory = directory(roots);
    let Some(metadata) = crate::checkpoint::safe_path(&directory)? else {
        return Ok(false);
    };
    ensure!(metadata.is_dir(), "Maintenance state is not a directory");
    let mut count = 0;
    for entry in fs::read_dir(directory)? {
        count += 1;
        ensure!(
            count <= 2,
            "Foreign maintenance state artifacts fence recovery"
        );
        let entry = entry?;
        ensure!(
            matches!(
                entry.file_name().to_str(),
                Some("plan.json" | "journal.jsonl")
            ),
            "Foreign maintenance state artifact fences recovery"
        );
        ensure!(
            safe_state(&entry.path())?.is_file(),
            "Maintenance state artifact changed type"
        );
    }
    Ok(count > 0)
}

fn valid_cursor(plan: &Plan, cursor: &Cursor) -> Result<()> {
    ensure!(
        cursor.completed <= plan.operations.len(),
        "Maintenance cursor exceeds immutable operation count"
    );
    if let Some(inflight) = cursor.inflight {
        let operation = plan
            .operations
            .get(inflight.index)
            .context("Maintenance active operation index is invalid")?;
        let forward = matches!(
            inflight.step,
            Step::RenameToTemporary | Step::RenameToDestination | Step::Chmod
        );
        ensure!(
            if forward {
                cursor.direction == Direction::Applying && inflight.index == cursor.completed
            } else {
                cursor.direction == Direction::RollingBack
                    && cursor.completed > 0
                    && inflight.index == cursor.completed - 1
            },
            "Maintenance active phase/cursor is inconsistent"
        );
        ensure!(
            matches!(
                (operation, inflight.step),
                (
                    Operation::Rename { .. },
                    Step::RenameToTemporary
                        | Step::RenameToDestination
                        | Step::UndoRenameToTemporary
                        | Step::UndoRenameToOriginal
                ) | (Operation::OwnerMode { .. }, Step::Chmod | Step::UndoChmod)
            ),
            "Maintenance active phase has wrong operation kind"
        );
    }
    Ok(())
}

fn valid_transition(previous: &Cursor, next: &Cursor) -> Result<()> {
    ensure!(
        next.completed.abs_diff(previous.completed) <= 1
            && !(previous.direction == Direction::RollingBack
                && next.direction == Direction::Applying),
        "Maintenance progress direction/count transition is malformed"
    );
    if previous == next || next.direction == Direction::RollingBack {
        return Ok(());
    }
    match (previous.inflight, next.inflight) {
        (None, Some(active)) => ensure!(
            previous.completed == next.completed
                && matches!(active.step, Step::RenameToTemporary | Step::Chmod),
            "Maintenance forward intent phase is malformed"
        ),
        (Some(before), Some(after)) => ensure!(
            previous.completed == next.completed
                && before.index == after.index
                && before.step == Step::RenameToTemporary
                && after.step == Step::RenameToDestination,
            "Maintenance rename leg transition is malformed"
        ),
        (Some(active), None) => ensure!(
            next.completed == previous.completed + 1
                && matches!(active.step, Step::RenameToDestination | Step::Chmod),
            "Maintenance completion lacks its final mutation intent"
        ),
        (None, None) => anyhow::bail!("Maintenance progress advanced without a mutation intent"),
    }
    Ok(())
}

fn valid_plan(plan: &Plan) -> Result<()> {
    platform::validate_capability(&plan.capability)?;
    plan::validate_roots(&plan.roots())?;
    for identity in &plan.root_identities {
        platform::validate_identity(identity, &plan.capability)?;
    }
    let subtree = crate::paths::relative(&plan.subtree, false)?
        .to_string_lossy()
        .replace('\\', "/");
    ensure!(
        subtree == plan.subtree
            && plan.entries.len() <= MAX_ENTRIES
            && plan
                .entries
                .get(&subtree)
                .is_some_and(|entry| entry.directory),
        "Maintenance immutable subtree/inventory is malformed"
    );
    let mut siblings = BTreeMap::new();
    let mut lowercase_targets = std::collections::BTreeSet::new();
    if plan.action == Action::LowercaseNames {
        for path in plan.entries.keys().filter(|path| **path != subtree) {
            let source = Path::new(path);
            let name = source
                .file_name()
                .and_then(|name| name.to_str())
                .context("Malformed maintenance lowercase name")?;
            let parent = source.parent().unwrap_or(Path::new(""));
            let lower = super::casing::lowercase(name, plan.capability.casing.as_ref())?;
            let target = parent.join(lower).to_string_lossy().replace('\\', "/");
            ensure!(
                lowercase_targets.insert(target),
                "Maintenance immutable lowercase collision"
            );
        }
    }
    let mut entries: Vec<_> = plan.entries.iter().collect();
    entries.sort_by(|(left, _), (right, _)| {
        Path::new(right)
            .components()
            .count()
            .cmp(&Path::new(left).components().count())
            .then(left.cmp(right))
    });
    let mut expected = Vec::new();
    for (path, entry) in entries {
        platform::validate_identity(&entry.identity, &plan.capability)?;
        let relative = crate::paths::relative(path, false)?
            .to_string_lossy()
            .replace('\\', "/");
        ensure!(
            relative == *path
                && (path == &subtree
                    || subtree.is_empty()
                    || path.starts_with(&format!("{subtree}/"))),
            "Maintenance inventory path escapes its selected subtree"
        );
        platform::validate_mode(entry.mode, &plan.capability)?;
        platform::validate_entry_type(&entry.identity, entry.directory)?;
        ensure!(
            entry.directory == entry.sha256.is_none() && (!entry.directory || entry.size == 0),
            "Maintenance immutable entry type/mode is malformed"
        );
        if let Some(hash) = &entry.sha256 {
            ensure!(
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
                "Maintenance immutable content hash is malformed"
            );
        }
        if path != &subtree {
            let parent = Path::new(path)
                .parent()
                .unwrap_or(Path::new(""))
                .to_string_lossy()
                .replace('\\', "/");
            ensure!(
                plan.entries
                    .get(&parent)
                    .is_some_and(|entry| entry.directory),
                "Maintenance inventory parent is absent"
            );
        }
        match plan.action {
            Action::LowercaseNames if path != &subtree => {
                let source = Path::new(path);
                let name = source
                    .file_name()
                    .and_then(|name| name.to_str())
                    .context("Maintenance name is malformed")?;
                let lower = super::casing::lowercase(name, plan.capability.casing.as_ref())?;
                let parent = source.parent().unwrap_or(Path::new(""));
                let to = parent.join(&lower).to_string_lossy().replace('\\', "/");
                ensure!(
                    siblings.insert(to.clone(), path).is_none(),
                    "Maintenance immutable lowercase collision"
                );
                if name != lower {
                    ensure!(
                        plan.capability.platform != "windows" || entry.directory || entry.mode == 0,
                        "Recorded readonly file rename is unsupported; run OwnerWritable and create a new lowercase preview"
                    );
                    let temporary = parent
                        .join(format!(
                            ".modsync-maint-{:x}-{:x}-{}",
                            entry.identity.device,
                            entry.identity.inode,
                            expected.len()
                        ))
                        .to_string_lossy()
                        .replace('\\', "/");
                    ensure!(
                        !plan.entries.contains_key(&temporary),
                        "Maintenance intermediate conflicts with immutable inventory"
                    );
                    ensure!(
                        !lowercase_targets.contains(&temporary),
                        "Maintenance intermediate conflicts with a lowercase destination"
                    );
                    expected.push(Operation::Rename {
                        from: path.clone(),
                        to,
                        temporary,
                    });
                }
            }
            Action::OwnerWritable => {
                let after =
                    platform::owner_writable_mode(entry.mode, entry.directory, &plan.capability)?;
                if after != entry.mode {
                    expected.push(Operation::OwnerMode {
                        path: path.clone(),
                        before: entry.mode,
                        after,
                    });
                }
            }
            _ => {}
        }
    }
    ensure!(
        expected == plan.operations,
        "Maintenance operations do not derive exactly from immutable request/inventory"
    );
    Ok(())
}

type Loaded = (Intent, Cursor, Vec<u8>, usize);

fn load(roots: &Roots) -> Result<Option<Loaded>> {
    if !artifacts(roots)? {
        return Ok(None);
    }
    let plan_bytes = read_bounded(&directory(roots).join("plan.json"))?;
    let intent: Intent = serde_json::from_slice(&plan_bytes).context("Incomplete/malformed immutable maintenance intent; no caller supplied journal can authorize mutation")?;
    ensure!(
        intent.version == 1
            && intent.plan.roots().mods == roots.mods
            && intent.plan.roots().game == roots.game
            && intent.plan.digest()? == intent.digest,
        "Maintenance immutable digest/version/root binding differs"
    );
    let nonce = uuid::Uuid::parse_str(&intent.nonce)?;
    ensure!(
        !nonce.is_nil() && nonce.to_string() == intent.nonce,
        "Maintenance journal nonce is malformed"
    );
    valid_plan(&intent.plan)?;
    valid_cursor(&intent.plan, &intent.progress_origin)?;
    ensure!(
        intent.progress_origin.inflight.is_none()
            && (intent.progress_origin
                == Cursor {
                    completed: 0,
                    direction: Direction::Applying,
                    inflight: None
                }
                || valid_terminal(&intent.plan, &intent.progress_origin).is_ok()),
        "Maintenance progress origin is not initial or terminal"
    );
    if let Some(terminal) = &intent.terminal {
        valid_terminal(&intent.plan, terminal)?;
    }
    let path = directory(roots).join("journal.jsonl");
    let journal_exists = crate::checkpoint::safe_path(&path)?.is_some();
    let bytes = if journal_exists {
        read_bounded(&path)?
    } else {
        Vec::new()
    };
    ensure!(
        plan_bytes
            .len()
            .checked_add(bytes.len())
            .is_some_and(|size| size <= MAX_DOCUMENT),
        "Combined maintenance plan/journal exceeds 64 MiB"
    );
    let mut cursor = intent.progress_origin.clone();
    let mut valid_length = 0;
    let mut records = 0;
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        if !line.ends_with(b"\n") {
            break;
        }
        records += 1;
        ensure!(
            records <= MAX_ENTRIES * 10 + 10 && line.len() <= 1024,
            "Maintenance progress journal exceeds record bound"
        );
        let next: Cursor = serde_json::from_slice(&line[..line.len() - 1])
            .context("Malformed maintenance progress record")?;
        valid_cursor(&intent.plan, &next)?;
        valid_transition(&cursor, &next)?;
        cursor = next;
        valid_length += line.len();
    }
    if !journal_exists {
        cursor = intent.terminal.clone().unwrap_or(cursor);
    } else if let Some(terminal) = &intent.terminal {
        ensure!(
            cursor == *terminal && valid_length == bytes.len(),
            "Maintenance terminal header does not match durable progress"
        );
    }
    Ok(Some((intent, cursor, bytes, valid_length)))
}

pub(super) fn status(roots: &Roots) -> Result<Status> {
    let mod_fence = fence::load(&roots.mods)?;
    let Some((intent, cursor, _, _)) = load(roots)? else {
        if crate::checkpoint::safe_path(&directory(roots))?.is_some() {
            crate::checkpoint::persistence::namespace_boundary(&directory(roots))?;
        }
        if let Some(record) = mod_fence {
            ensure!(
                record.roots().game == roots.game,
                "Mod fence is bound to another game root"
            );
            return Ok(record.status());
        }
        fence::ensure_clear(&roots.mods)?;
        return Ok(Status {
            pending: false,
            digest: None,
            operation_count: 0,
            phase: None,
        });
    };
    if let Some(record) = mod_fence {
        record.matches_plan(&intent.plan, &intent.digest, &intent.nonce)?;
        let flag = AtomicBool::new(false);
        let mut work = Work::new();
        validate_terminal_pair(&intent, &cursor, &record, &flag, &mut work)?;
    }
    Ok(Status {
        pending: true,
        digest: Some(intent.digest),
        operation_count: intent.plan.operations.len(),
        phase: Some(format!("{:?}", cursor.direction)),
    })
}

fn disposition(cursor: &Cursor) -> TreeDisposition {
    match cursor.direction {
        Direction::Applying => TreeDisposition::AppliedTree,
        Direction::RollingBack => TreeDisposition::RestoredOriginalTree,
    }
}

fn validate_terminal_pair(
    intent: &Intent,
    cursor: &Cursor,
    record: &fence::Record,
    flag: &AtomicBool,
    work: &mut Work,
) -> Result<()> {
    if let Some(terminal) = &record.terminal {
        valid_terminal(&intent.plan, cursor)?;
        ensure!(
            terminal.disposition == disposition(cursor),
            "Mod and primary maintenance terminal cursors disagree"
        );
        if let Some(primary) = &intent.terminal {
            ensure!(
                primary == cursor,
                "Primary maintenance terminal differs from its durable progress"
            );
        }
        let entries = plan::project(&intent.plan, cursor.completed, None, flag, work)?;
        ensure!(
            terminal.entry_count == entries.len()
                && terminal.entries_digest == plan::inventory_digest(&entries, flag, work)?,
            "Mod terminal proof differs from the immutable primary plan"
        );
    }
    Ok(())
}

struct Writer {
    directory: PathBuf,
    file: File,
    identity: platform::Identity,
    length: u64,
    modified: std::time::SystemTime,
    plan_bytes: usize,
}

fn valid_terminal(plan: &Plan, cursor: &Cursor) -> Result<()> {
    valid_cursor(plan, cursor)?;
    ensure!(
        cursor.inflight.is_none()
            && match cursor.direction {
                Direction::Applying => cursor.completed == plan.operations.len(),
                Direction::RollingBack => cursor.completed == 0,
            },
        "Maintenance terminal header has a nonterminal cursor"
    );
    Ok(())
}

fn publish_intent(directory: &Path, intent: &Intent, replace: bool) -> Result<()> {
    let bytes = plan::document(intent)?;
    let mut temporary = platform::private_temp(
        directory
            .parent()
            .context("Maintenance state needs a parent")?,
        "maintenance-v1-private-",
    )?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    let private_identity = platform::identity_file(temporary.as_file())?;
    ensure!(
        platform::identity_at(temporary.path(), &safe_state(temporary.path())?)?
            == private_identity,
        "Maintenance private intent was replaced before publication"
    );
    platform::publish_temp(temporary, &directory.join("plan.json"), replace)?;
    ensure!(
        read_bounded(&directory.join("plan.json"))? == bytes,
        "Maintenance published intent differs from its flushed private document"
    );
    Ok(())
}

fn finish_cleanup(intent: &Intent, terminal: &Cursor, record: &fence::Record) -> Result<()> {
    let directory = directory(&intent.plan.roots());
    let current: Intent = serde_json::from_slice(&read_bounded(&directory.join("plan.json"))?)?;
    ensure!(
        current.digest == intent.digest
            && current.nonce == intent.nonce
            && current.plan.digest()? == intent.digest,
        "Maintenance immutable intent changed before terminal cleanup"
    );
    valid_terminal(&intent.plan, terminal)?;
    let mut terminal_intent = intent.clone();
    terminal_intent.terminal = Some(terminal.clone());
    publish_intent(&directory, &terminal_intent, true)?;
    // Keep the target's fence until last. Mod-only acquisition may resume
    // after a Game-target fence clears, while the terminal primary still
    // fences game-only deployment writers and preserves the Game tree proof.
    if intent.plan.target == super::TreeRoot::Game {
        fence::clear(record)?;
    }
    let journal = directory.join("journal.jsonl");
    if crate::checkpoint::safe_path(&journal)?.is_some() {
        safe_state(&journal)?;
        fs::remove_file(journal)?;
    }
    crate::checkpoint::persistence::namespace_boundary(&directory)?;
    fs::remove_file(directory.join("plan.json"))?;
    if let Err(error) = crate::checkpoint::persistence::namespace_boundary(&directory) {
        return match publish_intent(&directory, &terminal_intent, false) {
            Ok(()) => Err(error).context("Maintenance cleanup flush failed; terminal intent retained for explicit recovery"),
            Err(republish) => Err(error).context(format!("Maintenance cleanup flush and terminal intent republication failed: {republish:#}; target namespace cleanup remains unqualified; absence guards must flush before admitting writers")),
        };
    }
    if intent.plan.target == super::TreeRoot::Mods {
        fence::clear(record)?;
    }
    Ok(())
}

impl Writer {
    fn open(intent: &Intent, truncate_to: Option<usize>) -> Result<Self> {
        let directory = directory(&intent.plan.roots());
        let path = directory.join("journal.jsonl");
        let file = if crate::checkpoint::safe_path(&path)?.is_none() {
            platform::create_file(&path)?
        } else {
            safe_state(&path)?;
            let mut options = fs::OpenOptions::new();
            options.read(true).append(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options
                    .custom_flags(
                        windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
                    )
                    .access_mode(
                        windows_sys::Win32::Foundation::GENERIC_READ
                            | windows_sys::Win32::Foundation::GENERIC_WRITE,
                    );
            }
            options.open(&path)?
        };
        let metadata = file.metadata()?;
        platform::qualify_private(&path, &metadata)?;
        ensure!(
            platform::identity_file(&file)? == platform::identity_at(&path, &safe_state(&path)?)?,
            "Maintenance journal identity changed during open"
        );
        if let Some(length) = truncate_to {
            ensure!(
                length as u64 <= metadata.len(),
                "Maintenance truncation length exceeds observed journal"
            );
            file.set_len(length as u64)?;
            file.sync_all()?;
        }
        crate::checkpoint::persistence::namespace_boundary(&directory)?;
        let metadata = file.metadata()?;
        Ok(Self {
            directory,
            identity: platform::identity_file(&file)?,
            length: metadata.len(),
            modified: metadata.modified()?,
            plan_bytes: plan::document(intent)?.len() + fence::MAX_FENCE,
            file,
        })
    }
    fn append(&mut self, plan: &Plan, cursor: &Cursor) -> Result<()> {
        valid_cursor(plan, cursor)?;
        let path = self.directory.join("journal.jsonl");
        let metadata = safe_state(&path)?;
        ensure!(
            platform::identity_at(&path, &metadata)? == self.identity
                && metadata.len() == self.length
                && metadata.modified()? == self.modified,
            "Maintenance journal was edited/replaced by a foreign writer"
        );
        let mut bytes = serde_json::to_vec(cursor)?;
        bytes.push(b'\n');
        ensure!(
            bytes.len() <= 1024
                && (self.plan_bytes as u64)
                    .checked_add(self.length)
                    .and_then(|size| size.checked_add(bytes.len() as u64))
                    .is_some_and(|size| size <= MAX_DOCUMENT as u64),
            "Maintenance durable journal budget exhausted"
        );
        #[cfg(windows)]
        {
            // Full write rights are needed for interrupted-tail truncation.
            // That overrides std's append-only access mask, so seek explicitly.
            let offset = std::io::Seek::seek(&mut self.file, std::io::SeekFrom::End(0))?;
            ensure!(
                offset == self.length,
                "Maintenance journal length changed before Windows append"
            );
        }
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        self.length += bytes.len() as u64;
        self.modified = self.file.metadata()?.modified()?;
        Ok(())
    }
    fn clear(self, intent: &Intent, terminal: &Cursor, work: &mut Work) -> Result<()> {
        let current: Intent =
            serde_json::from_slice(&read_bounded(&self.directory.join("plan.json"))?)?;
        ensure!(
            current.digest == intent.digest
                && current.nonce == intent.nonce
                && current.plan.digest()? == intent.digest,
            "Maintenance immutable intent changed before cleanup"
        );
        let metadata = safe_state(&self.directory.join("journal.jsonl"))?;
        ensure!(
            platform::identity_at(&self.directory.join("journal.jsonl"), &metadata)?
                == self.identity
                && metadata.len() == self.length
                && metadata.modified()? == self.modified,
            "Maintenance journal changed before cleanup"
        );
        valid_terminal(&intent.plan, terminal)?;
        let finishing = AtomicBool::new(false);
        let record = fence::terminal(
            &intent.plan,
            &intent.digest,
            &intent.nonce,
            disposition(terminal),
            &finishing,
            work,
        )?;
        drop(self.file);
        finish_cleanup(intent, terminal, &record)
    }
}

/// Test actual spelling rather than following case aliases on the underlying FS.
fn exact_exists(path: &Path, work: &mut Work, flag: &AtomicBool) -> Result<bool> {
    let Some(parent) = path.parent() else {
        return Ok(false);
    };
    crate::checkpoint::safe_path(parent)?.context("Maintenance namespace parent disappeared")?;
    for entry in fs::read_dir(parent)? {
        work.check(flag)?;
        if entry?.file_name()
            == path
                .file_name()
                .context("Maintenance object lacks a name")?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn expected_entry(plan: &Plan, operation: &Operation) -> Result<Entry> {
    let path = match operation {
        Operation::Rename { from, .. } => from,
        Operation::OwnerMode { path, .. } => path,
    };
    Ok(plan
        .entries
        .get(path)
        .context("Maintenance operation has no original entry")?
        .clone())
}

fn check_object(
    plan: &Plan,
    relative: &str,
    expected: &Entry,
    flag: &AtomicBool,
    work: &mut Work,
) -> Result<()> {
    plan.verify_roots()?;
    ensure!(
        plan::snapshot(&plan.root().join(relative), flag, work)? == *expected,
        "Maintenance object identity/content/mode changed at '{relative}'"
    );
    Ok(())
}

fn rename(
    plan: &Plan,
    from: &str,
    to: &str,
    expected: &Entry,
    flag: &AtomicBool,
    work: &mut Work,
) -> Result<()> {
    check_object(plan, from, expected, flag, work)?;
    let source = plan.root().join(from);
    let target = plan.root().join(to);
    ensure!(
        source.parent() == target.parent(),
        "Maintenance rename cannot change its parent"
    );
    ensure!(
        !exact_exists(&target, work, flag)?,
        "Maintenance rename destination exists; never overwrite or merge"
    );
    let parent = source
        .parent()
        .context("Maintenance rename parent missing")?;
    let parent_metadata = safe_state(parent)?;
    let parent_relative = parent
        .strip_prefix(plan.root())?
        .to_str()
        .context("Maintenance parent cannot be encoded")?
        .replace('\\', "/");
    let parent_entry = plan
        .entries
        .get(&parent_relative)
        .context("Maintenance parent is outside immutable inventory")?;
    let parent_identity = platform::identity_at(parent, &parent_metadata)?;
    ensure!(
        parent_identity == parent_entry.identity
            && platform::mode(&parent_metadata)? == parent_entry.mode,
        "Maintenance namespace parent has foreign identity or mode"
    );
    check_cancelled(flag)?;
    platform::rename(&source, &target, &parent_identity)?;
    crate::checkpoint::persistence::namespace_boundary(parent)?;
    ensure!(
        platform::identity_at(parent, &safe_state(parent)?)? == parent_identity,
        "Maintenance namespace parent changed during rename"
    );
    check_object(plan, to, expected, flag, work)
}

fn forward(
    intent: &Intent,
    writer: &mut Writer,
    cursor: &mut Cursor,
    flag: &AtomicBool,
    work: &mut Work,
) -> Result<()> {
    while cursor.completed < intent.plan.operations.len() {
        work.check(flag)?;
        let index = cursor.completed;
        let operation = &intent.plan.operations[index];
        let expected = expected_entry(&intent.plan, operation)?;
        match operation {
            Operation::Rename {
                from,
                to,
                temporary,
            } => {
                cursor.inflight = Some(Inflight {
                    index,
                    step: Step::RenameToTemporary,
                });
                writer.append(&intent.plan, cursor)?;
                rename(&intent.plan, from, temporary, &expected, flag, work)?;
                cursor.inflight = Some(Inflight {
                    index,
                    step: Step::RenameToDestination,
                });
                writer.append(&intent.plan, cursor)?;
                rename(&intent.plan, temporary, to, &expected, flag, work)?;
            }
            Operation::OwnerMode { path, after, .. } => {
                check_object(&intent.plan, path, &expected, flag, work)?;
                cursor.inflight = Some(Inflight {
                    index,
                    step: Step::Chmod,
                });
                writer.append(&intent.plan, cursor)?;
                check_cancelled(flag)?;
                platform::chmod(&intent.plan.root().join(path), *after, &expected.identity)?;
                let mut after_entry = expected.clone();
                after_entry.mode = *after;
                check_object(&intent.plan, path, &after_entry, flag, work)?;
            }
        }
        cursor.completed += 1;
        cursor.inflight = None;
        writer.append(&intent.plan, cursor)?;
    }
    plan::verify_tree(
        &intent.plan,
        &plan::project(&intent.plan, cursor.completed, None, flag, work)?,
        flag,
        work,
    )
}

/// Reconcile a single interrupted intent from owned object identities and exact
/// spelling. Verify the whole projected tree before normalizing the rollback
/// cursor, so progress metadata alone never grants mutation authority.
fn reconcile(
    intent: &Intent,
    cursor: &Cursor,
    flag: &AtomicBool,
    work: &mut Work,
) -> Result<Cursor> {
    let mut result = cursor.clone();
    let mut completed = cursor.completed;
    let mut extra = None;
    if let Some(inflight) = cursor.inflight {
        let operation = &intent.plan.operations[inflight.index];
        completed = inflight.index;
        let expected = expected_entry(&intent.plan, operation)?;
        match operation {
            Operation::Rename {
                from,
                to,
                temporary,
            } => {
                let mut found = None;
                for path in [from, temporary, to] {
                    if exact_exists(&intent.plan.root().join(path), work, flag)? {
                        check_object(&intent.plan, path, &expected, flag, work)?;
                        ensure!(
                            found.replace(path).is_none(),
                            "Maintenance interrupted rename has multiple owned endpoints"
                        );
                    }
                }
                let path = found.context("Maintenance interrupted rename object is missing")?;
                if path != from {
                    extra = Some(Operation::Rename {
                        from: from.clone(),
                        to: path.clone(),
                        temporary: temporary.clone(),
                    });
                    result.completed = inflight.index + 1;
                    result.inflight = if path == temporary {
                        Some(Inflight {
                            index: inflight.index,
                            step: Step::UndoRenameToOriginal,
                        })
                    } else {
                        None
                    };
                } else {
                    result.completed = inflight.index;
                    result.inflight = None;
                }
            }
            Operation::OwnerMode {
                path,
                before,
                after,
            } => {
                let actual = plan::snapshot(&intent.plan.root().join(path), flag, work)?;
                let mut expected_after = expected.clone();
                expected_after.mode = *after;
                ensure!(
                    actual == expected || actual == expected_after,
                    "Maintenance interrupted chmod has foreign state"
                );
                if actual.mode == *after {
                    extra = Some(Operation::OwnerMode {
                        path: path.clone(),
                        before: *before,
                        after: *after,
                    });
                    result.completed = inflight.index + 1;
                } else {
                    result.completed = inflight.index;
                }
                result.inflight = None;
            }
        }
    }
    let expected = plan::project(&intent.plan, completed, extra.as_ref(), flag, work)?;
    plan::verify_tree(&intent.plan, &expected, flag, work)?;
    result.direction = Direction::RollingBack;
    valid_cursor(&intent.plan, &result)?;
    Ok(result)
}

fn rollback(
    intent: &Intent,
    writer: &mut Writer,
    cursor: &mut Cursor,
    work: &mut Work,
) -> Result<()> {
    // Cleanup is independent of the user's triggering cancellation, but its
    // own byte, namespace-visit and wall-time bounds remain enforced.
    let finishing = AtomicBool::new(false);
    *cursor = reconcile(intent, cursor, &finishing, work)?;
    writer.append(&intent.plan, cursor)?;
    while cursor.completed > 0 {
        work.check(&finishing)?;
        let index = cursor.completed - 1;
        let operation = &intent.plan.operations[index];
        let expected = expected_entry(&intent.plan, operation)?;
        match operation {
            Operation::Rename {
                from,
                to,
                temporary,
            } => {
                if cursor.inflight.is_none() {
                    cursor.inflight = Some(Inflight {
                        index,
                        step: Step::UndoRenameToTemporary,
                    });
                    writer.append(&intent.plan, cursor)?;
                    rename(&intent.plan, to, temporary, &expected, &finishing, work)?;
                }
                cursor.inflight = Some(Inflight {
                    index,
                    step: Step::UndoRenameToOriginal,
                });
                writer.append(&intent.plan, cursor)?;
                rename(&intent.plan, temporary, from, &expected, &finishing, work)?;
            }
            Operation::OwnerMode {
                path,
                before,
                after,
            } => {
                let mut changed = expected.clone();
                changed.mode = *after;
                check_object(&intent.plan, path, &changed, &finishing, work)?;
                cursor.inflight = Some(Inflight {
                    index,
                    step: Step::UndoChmod,
                });
                writer.append(&intent.plan, cursor)?;
                platform::chmod(&intent.plan.root().join(path), *before, &expected.identity)?;
                check_object(&intent.plan, path, &expected, &finishing, work)?;
            }
        }
        cursor.completed -= 1;
        cursor.inflight = None;
        writer.append(&intent.plan, cursor)?;
    }
    plan::verify_tree(&intent.plan, &intent.plan.entries, &finishing, work)
}

fn failure(error: &anyhow::Error) -> String {
    format!("{error:#}").chars().take(4096).collect()
}

pub(super) fn apply(plan: Plan, lock: &Lock, flag: &AtomicBool) -> Result<Outcome> {
    ensure!(
        lock.guards(&plan.roots()),
        "Maintenance transaction requires matching held root locks"
    );
    let digest = plan.digest()?;
    let directory = directory(&plan.roots());
    ensure!(
        !artifacts(&plan.roots())?,
        "Another maintenance intent exists"
    );
    fs::create_dir(&directory).or_else(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            Ok(())
        } else {
            Err(error)
        }
    })?;
    ensure!(
        safe_state(&directory)?.is_dir(),
        "Maintenance state directory changed type"
    );
    crate::checkpoint::persistence::namespace_boundary(directory.parent().unwrap())?;
    let intent = Intent {
        version: 1,
        digest: digest.clone(),
        nonce: uuid::Uuid::new_v4().to_string(),
        plan,
        terminal: None,
        progress_origin: Cursor {
            completed: 0,
            direction: Direction::Applying,
            inflight: None,
        },
    };
    let bytes = plan::document(&intent)?;
    let progress_reservation = intent
        .plan
        .operations
        .len()
        .checked_mul(2048)
        .context("Maintenance journal reservation overflow")?;
    // Reserve sufficient bounded progress space for forward and reverse legs.
    ensure!(
        bytes
            .len()
            .checked_add(fence::MAX_FENCE)
            .and_then(|size| size.checked_add(progress_reservation))
            .is_some_and(|size| size <= MAX_DOCUMENT),
        "Maintenance immutable plan plus recovery journal exceeds 64 MiB reservation"
    );
    if let Err(error) = publish_intent(&directory, &intent, false) {
        let error = match crate::checkpoint::safe_path(&directory.join("plan.json")) {
            Ok(None) => {
                return Err(error)
                    .context("Maintenance intent was not published; no tree operation began");
            }
            Ok(Some(_)) => error,
            Err(inspection) => error.context(format!(
                "Maintenance intent publication could not be qualified: {inspection:#}"
            )),
        };
        return Ok(Outcome {
            digest,
            completed_operations: 0,
            recovered: false,
            status: OutcomeStatus::RecoveryRequired,
            failure: Some(failure(&error)),
            disposition: None,
        });
    }
    if let Err(error) = fence::ensure_active(&intent.plan, &intent.digest, &intent.nonce) {
        return Ok(Outcome {
            digest,
            completed_operations: 0,
            recovered: false,
            status: OutcomeStatus::RecoveryRequired,
            failure: Some(failure(&error)),
            disposition: None,
        });
    }
    let mut cursor = Cursor {
        completed: 0,
        direction: Direction::Applying,
        inflight: None,
    };
    let mut writer = match Writer::open(&intent, None) {
        Ok(writer) => writer,
        Err(error) => {
            return Ok(Outcome {
                digest,
                completed_operations: 0,
                recovered: false,
                status: OutcomeStatus::RecoveryRequired,
                failure: Some(failure(&error)),
                disposition: None,
            });
        }
    };
    let mut work = Work::new();
    match forward(&intent, &mut writer, &mut cursor, flag, &mut work) {
        Ok(()) => match writer.clear(&intent, &cursor, &mut work) {
            Ok(()) => Ok(Outcome {
                digest,
                completed_operations: intent.plan.operations.len(),
                recovered: false,
                status: OutcomeStatus::Applied,
                failure: None,
                disposition: Some(TreeDisposition::AppliedTree),
            }),
            Err(error) => Ok(Outcome {
                digest,
                completed_operations: cursor.completed,
                recovered: false,
                status: OutcomeStatus::RecoveryRequired,
                failure: Some(failure(&error)),
                disposition: None,
            }),
        },
        Err(error) => {
            let applied = cursor.completed;
            let cancelled = flag.load(Ordering::Acquire);
            let mut cleanup_work = Work::new();
            let cleanup = rollback(&intent, &mut writer, &mut cursor, &mut cleanup_work)
                .and_then(|()| writer.clear(&intent, &cursor, &mut cleanup_work));
            match cleanup {
                Ok(()) => Ok(Outcome {
                    digest,
                    completed_operations: applied,
                    recovered: false,
                    status: if cancelled {
                        OutcomeStatus::CancelledAndRolledBack
                    } else {
                        OutcomeStatus::FailedAndRolledBack
                    },
                    failure: Some(failure(&error)),
                    disposition: Some(TreeDisposition::RestoredOriginalTree),
                }),
                Err(cleanup) => Ok(Outcome {
                    digest,
                    completed_operations: applied,
                    recovered: false,
                    status: OutcomeStatus::RecoveryRequired,
                    failure: Some(
                        format!(
                            "{}; rollback could not be verified: {}",
                            failure(&error),
                            failure(&cleanup)
                        )
                        .chars()
                        .take(4096)
                        .collect(),
                    ),
                    disposition: None,
                }),
            }
        }
    }
}

pub(super) fn recover(
    roots: &Roots,
    expected_digest: &str,
    lock: &Lock,
    flag: &AtomicBool,
) -> Result<Outcome> {
    ensure!(
        lock.guards(roots),
        "Maintenance recovery requires matching held root locks"
    );
    let Some((intent, mut cursor, bytes, valid_length)) = load(roots)? else {
        let record = fence::load(&roots.mods)?.context("No pending maintenance transaction")?;
        ensure!(
            record.roots().game == roots.game && record.digest == expected_digest,
            "Terminal mod fence differs from selected roots or operation identity"
        );
        check_cancelled(flag)?;
        if crate::checkpoint::safe_path(&directory(roots))?.is_some() {
            crate::checkpoint::persistence::namespace_boundary(&directory(roots))?;
        }
        let mut work = Work::new();
        let result = record
            .verify_terminal(flag, &mut work)
            .and_then(|verified| {
                fence::clear(&record)?;
                Ok(verified)
            });
        return match result {
            Ok(verified) => Ok(Outcome {
                digest: record.digest,
                completed_operations: if verified == TreeDisposition::AppliedTree {
                    record.operation_count
                } else {
                    0
                },
                recovered: true,
                status: OutcomeStatus::Applied,
                failure: None,
                disposition: Some(verified),
            }),
            Err(error) => Ok(Outcome {
                digest: record.digest,
                completed_operations: 0,
                recovered: true,
                status: OutcomeStatus::RecoveryRequired,
                failure: Some(failure(&error)),
                disposition: None,
            }),
        };
    };
    ensure!(
        intent.digest == expected_digest,
        "Maintenance recovery operation identity differs from selected digest"
    );
    check_cancelled(flag)?;
    let mut work = Work::new();
    let result = (|| -> Result<TreeDisposition> {
        let record = fence::ensure_active(&intent.plan, &intent.digest, &intent.nonce)?;
        validate_terminal_pair(&intent, &cursor, &record, flag, &mut work)?;
        if intent.terminal.is_some() || record.terminal.is_some() {
            // A verified terminal publication commits the disposition. Recovery
            // cannot change a completed apply back into an original-tree rollback.
            let terminal = intent.terminal.as_ref().unwrap_or(&cursor);
            valid_terminal(&intent.plan, terminal)?;
            let record = fence::terminal(
                &intent.plan,
                &intent.digest,
                &intent.nonce,
                disposition(terminal),
                flag,
                &mut work,
            )?;
            let verified = record.verify_terminal(flag, &mut work)?;
            let journal_path = directory(roots).join("journal.jsonl");
            if crate::checkpoint::safe_path(&journal_path)?.is_some() {
                ensure!(
                    read_bounded(&journal_path)? == bytes && bytes.len() == valid_length,
                    "Terminal maintenance journal changed or has a partial suffix"
                );
            }
            finish_cleanup(&intent, terminal, &record)?;
            return Ok(verified);
        }
        // Prove actual state before discarding an interrupted append suffix.
        let journal_path = directory(roots).join("journal.jsonl");
        let normalized = reconcile(&intent, &cursor, flag, &mut work)?;
        if crate::checkpoint::safe_path(&journal_path)?.is_some() {
            ensure!(
                read_bounded(&journal_path)? == bytes,
                "Maintenance journal changed during recovery qualification"
            );
        }
        let mut writer = Writer::open(&intent, Some(valid_length))?;
        cursor = normalized;
        rollback(&intent, &mut writer, &mut cursor, &mut work)?;
        writer.clear(&intent, &cursor, &mut work)?;
        Ok(TreeDisposition::RestoredOriginalTree)
    })();
    match result {
        Ok(verified) => Ok(Outcome {
            digest: intent.digest,
            completed_operations: if verified == TreeDisposition::AppliedTree {
                intent.plan.operations.len()
            } else {
                0
            },
            recovered: true,
            status: OutcomeStatus::Applied,
            failure: None,
            disposition: Some(verified),
        }),
        Err(error) => Ok(Outcome {
            digest: intent.digest,
            completed_operations: cursor.completed,
            recovered: true,
            status: OutcomeStatus::RecoveryRequired,
            failure: Some(failure(&error)),
            disposition: None,
        }),
    }
}
