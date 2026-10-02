use super::{
    Action, Capability, Operation, Preview, Request, TreeRoot, check_cancelled,
    platform::{self, Identity},
};
use crate::{checkpoint::RootBinding, paths::Roots};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

pub(super) const MAX_ENTRIES: usize = 100_000;
pub(super) const MAX_DOCUMENT: usize = 64 * 1024 * 1024;

pub(super) fn document<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    struct Bounded(Vec<u8>);
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self
                .0
                .len()
                .checked_add(bytes.len())
                .is_none_or(|length| length > MAX_DOCUMENT)
            {
                return Err(std::io::Error::other(
                    "Maintenance serialized document exceeds 64 MiB",
                ));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Bounded(Vec::new());
    serde_json::to_writer(&mut writer, value)?;
    Ok(writer.0)
}

pub(super) struct Work {
    bytes: u64,
    visits: usize,
    started: Instant,
}
impl Work {
    pub fn new() -> Self {
        Self {
            bytes: 0,
            visits: 0,
            started: Instant::now(),
        }
    }
    pub fn check(&mut self, flag: &AtomicBool) -> Result<()> {
        check_cancelled(flag)?;
        self.visits += 1;
        ensure!(
            self.visits <= 2_000_000 && self.started.elapsed() <= Duration::from_secs(600),
            "Maintenance work/time budget exhausted"
        );
        Ok(())
    }
    fn add_bytes(&mut self, count: usize) -> Result<()> {
        self.bytes = self
            .bytes
            .checked_add(count as u64)
            .context("Maintenance hash size overflow")?;
        ensure!(
            self.bytes <= 128 * 1024 * 1024 * 1024,
            "Maintenance aggregate hashing budget exceeds 128 GiB"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Entry {
    pub identity: Identity,
    pub directory: bool,
    pub mode: u32,
    pub size: u64,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Plan {
    pub roots: RootBinding,
    pub root_identities: [Identity; 2],
    pub target: TreeRoot,
    pub subtree: String,
    pub action: Action,
    #[serde(deserialize_with = "unique_entries")]
    pub entries: BTreeMap<String, Entry>,
    pub operations: Vec<Operation>,
    pub capability: Capability,
}

fn unique_entries<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, Entry>, D::Error> {
    struct Entries;
    impl<'de> serde::de::Visitor<'de> for Entries {
        type Value = BTreeMap<String, Entry>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a bounded maintenance inventory with unique path keys")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut access: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut entries = BTreeMap::new();
            while let Some((path, entry)) = access.next_entry::<String, Entry>()? {
                if entries.len() >= MAX_ENTRIES || entries.insert(path, entry).is_some() {
                    return Err(serde::de::Error::custom(
                        "Maintenance inventory duplicate key or entry budget exceeded",
                    ));
                }
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_map(Entries)
}

pub(super) fn validate_roots(roots: &Roots) -> Result<()> {
    for root in [&roots.game, &roots.mods] {
        ensure!(
            root.is_absolute() && root.parent().is_some(),
            "Maintenance refuses filesystem roots and relative roots"
        );
        ensure!(
            crate::checkpoint::safe_path(root)?.is_some_and(|m| m.is_dir()),
            "Maintenance root has a missing, linked or non-directory ancestor"
        );
        ensure!(
            fs::canonicalize(root)? == *root,
            "Maintenance roots must use canonical spelling"
        );
        root.to_str()
            .context("Maintenance cannot journal non-UTF8 root paths")?;
    }
    ensure!(
        !roots.game.starts_with(&roots.mods) && !roots.mods.starts_with(&roots.game),
        "Maintenance roots must not overlap"
    );
    let state = crate::recovery::state_dir(roots);
    ensure!(
        !state.starts_with(&roots.game)
            && !state.starts_with(&roots.mods)
            && !roots.mods.starts_with(&state),
        "Maintenance recovery state overlaps a selected root"
    );
    Ok(())
}

pub(super) fn build(request: &Request, flag: &AtomicBool) -> Result<Plan> {
    validate_roots(&request.roots)?;
    platform::require_capability()?;
    let mut work = Work::new();
    let relative = crate::paths::relative(&request.subtree, false)?;
    let subtree = relative
        .to_str()
        .context("Non-UTF8 subtree")?
        .replace('\\', "/");
    let roots = RootBinding {
        mods: request.roots.mods.to_str().unwrap().into(),
        game: request.roots.game.to_str().unwrap().into(),
    };
    let mut identities = Vec::new();
    for root in [&request.roots.mods, &request.roots.game] {
        let metadata =
            crate::checkpoint::safe_path(root)?.context("Maintenance root disappeared")?;
        platform::qualify(root, &metadata)?;
        identities.push(platform::identity(&metadata)?);
    }
    let mut plan = Plan {
        roots,
        root_identities: identities
            .try_into()
            .map_err(|_| anyhow::anyhow!("Maintenance root identity count differs"))?,
        target: request.target,
        subtree,
        action: request.action,
        entries: BTreeMap::new(),
        operations: Vec::new(),
        capability: platform::capability(),
    };
    let root = plan.root();
    let selected = root.join(&relative);
    ensure!(
        crate::checkpoint::safe_path(&selected)?.is_some_and(|m| m.is_dir()),
        "Maintenance subtree must be an ordinary directory"
    );
    plan.entries = inventory(&root, &plan.subtree, flag, &mut work)?;
    let mut entries: Vec<_> = plan.entries.iter().collect();
    entries.sort_by(|(left, _), (right, _)| depth(right).cmp(&depth(left)).then(left.cmp(right)));
    let mut lowercase_destinations = BTreeMap::new();
    if request.action == Action::LowercaseNames {
        for (path, _) in &entries {
            if **path == plan.subtree {
                continue;
            }
            let path_obj = Path::new(path);
            let name = path_obj
                .file_name()
                .and_then(|name| name.to_str())
                .context("Non-UTF8 maintenance name")?;
            ensure!(
                name.is_ascii(),
                "Lowercase maintenance Unicode mapping is unsupported: {path}"
            );
            let destination = path_obj
                .parent()
                .unwrap_or(Path::new(""))
                .join(name.to_ascii_lowercase())
                .to_string_lossy()
                .replace('\\', "/");
            ensure!(
                lowercase_destinations
                    .insert(destination.clone(), *path)
                    .is_none(),
                "Lowercase collision at '{destination}'; directories will never be merged"
            );
        }
    }
    for (path, entry) in entries {
        work.check(flag)?;
        match request.action {
            Action::LowercaseNames if *path != plan.subtree => {
                let source = Path::new(path);
                let name = source.file_name().unwrap().to_str().unwrap();
                let lower = name.to_ascii_lowercase();
                if lower == name {
                    continue;
                }
                let parent = source.parent().unwrap_or(Path::new(""));
                let to = parent.join(lower).to_string_lossy().replace('\\', "/");
                if let Some(metadata) = crate::checkpoint::safe_path(&root.join(&to))? {
                    ensure!(
                        platform::identity(&metadata)? == entry.identity,
                        "Lowercase destination already exists: {to}"
                    );
                }
                // The name is deterministic from object identity and ordinal;
                // it is reviewed in the preview, reserved exclusively, and never
                // used without a durable journal authorizing this operation.
                let temporary = parent
                    .join(format!(
                        ".modsync-maint-{:x}-{:x}-{}",
                        entry.identity.device,
                        entry.identity.inode,
                        plan.operations.len()
                    ))
                    .to_string_lossy()
                    .replace('\\', "/");
                ensure!(
                    crate::checkpoint::safe_path(&root.join(&temporary))?.is_none(),
                    "Maintenance intermediate already exists: {temporary}"
                );
                ensure!(
                    !plan.entries.contains_key(&temporary),
                    "Maintenance intermediate collides with an authored entry"
                );
                ensure!(
                    !lowercase_destinations.contains_key(&temporary),
                    "Maintenance intermediate collides with a planned lowercase destination"
                );
                plan.operations.push(Operation::Rename {
                    from: path.clone(),
                    to,
                    temporary,
                });
            }
            Action::OwnerWritable => {
                let after = entry.mode | if entry.directory { 0o700 } else { 0o600 };
                if after != entry.mode {
                    plan.operations.push(Operation::OwnerMode {
                        path: path.clone(),
                        before: entry.mode,
                        after,
                    });
                }
            }
            _ => {}
        }
    }
    let size = document(&plan)?.len();
    ensure!(
        size.checked_add(4096)
            .and_then(|size| plan
                .operations
                .len()
                .checked_mul(2048)
                .and_then(|progress| size.checked_add(progress)))
            .is_some_and(|size| size <= MAX_DOCUMENT),
        "Maintenance plan plus rollback journal exceeds 64 MiB reservation"
    );
    Ok(plan)
}

fn depth(path: &str) -> usize {
    Path::new(path).components().count()
}

pub(super) fn snapshot(path: &Path, flag: &AtomicBool, work: &mut Work) -> Result<Entry> {
    work.check(flag)?;
    let metadata = crate::checkpoint::safe_path(path)?.context("Maintenance object disappeared")?;
    platform::qualify(path, &metadata)?;
    let identity = platform::identity(&metadata)?;
    let mode = platform::mode(&metadata)?;
    let sha256 = if metadata.is_file() {
        let mut file = crate::paths::open_regular_read(path)?;
        ensure!(
            platform::identity(&file.metadata()?)? == identity,
            "Maintenance object changed during open"
        );
        let mut hash = Sha256::new();
        let mut read = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            work.check(flag)?;
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            work.add_bytes(count)?;
            read = read
                .checked_add(count as u64)
                .context("Maintenance file size overflow")?;
            ensure!(
                read <= metadata.len(),
                "Maintenance file grew while hashing"
            );
            hash.update(&buffer[..count]);
        }
        let after = file.metadata()?;
        ensure!(
            read == metadata.len()
                && after.len() == metadata.len()
                && platform::identity(&after)? == identity
                && platform::mode(&after)? == mode
                && after.modified()? == metadata.modified()?,
            "Maintenance file changed while hashing"
        );
        Some(format!("{:x}", hash.finalize()))
    } else {
        None
    };
    Ok(Entry {
        identity,
        directory: metadata.is_dir(),
        mode,
        size: if metadata.is_file() {
            metadata.len()
        } else {
            0
        },
        sha256,
    })
}

pub(super) fn inventory(
    root: &Path,
    subtree: &str,
    flag: &AtomicBool,
    work: &mut Work,
) -> Result<BTreeMap<String, Entry>> {
    let selected = root.join(subtree);
    let mut entries = BTreeMap::new();
    let mut name_bytes = 0usize;
    for entry in walkdir::WalkDir::new(&selected)
        .follow_links(false)
        .max_depth(65)
    {
        work.check(flag)?;
        let entry = entry?;
        ensure!(entry.depth() <= 64, "Maintenance subtree exceeds depth64");
        let relative = entry.path().strip_prefix(root)?;
        let raw = relative
            .to_str()
            .context("Maintenance cannot journal non-UTF8 names")?;
        ensure!(
            crate::paths::relative(raw, false)? == relative,
            "Maintenance name is unsafe or aliases another portable path"
        );
        let path = raw.replace('\\', "/");
        ensure!(
            entries.len() < MAX_ENTRIES,
            "Maintenance subtree exceeds 100000 entries"
        );
        name_bytes = name_bytes
            .checked_add(path.len())
            .context("Maintenance name budget overflow")?;
        ensure!(
            name_bytes <= 16 * 1024 * 1024,
            "Maintenance path inventory exceeds 16 MiB"
        );
        ensure!(
            entries
                .insert(path, snapshot(entry.path(), flag, work)?)
                .is_none(),
            "Maintenance inventory duplicate path"
        );
    }
    Ok(entries)
}

impl Plan {
    pub fn roots(&self) -> Roots {
        Roots {
            mods: PathBuf::from(&self.roots.mods),
            game: PathBuf::from(&self.roots.game),
        }
    }
    pub fn root(&self) -> PathBuf {
        match self.target {
            TreeRoot::Game => PathBuf::from(&self.roots.game),
            TreeRoot::Mods => PathBuf::from(&self.roots.mods),
        }
    }
    pub fn digest(&self) -> Result<String> {
        let bytes = document(self)?;
        ensure!(
            bytes.len() <= MAX_DOCUMENT,
            "Maintenance plan exceeds document bound"
        );
        Ok(format!("{:x}", Sha256::digest(&bytes)))
    }
    pub fn preview(&self) -> Result<Preview> {
        Ok(Preview {
            digest: self.digest()?,
            operations: self.operations.clone(),
            entry_count: self.entries.len(),
            operation_count: self.operations.len(),
            capability: self.capability.clone(),
        })
    }
    pub fn verify_roots(&self) -> Result<()> {
        let roots = self.roots();
        validate_roots(&roots)?;
        for (root, identity) in [&roots.mods, &roots.game]
            .into_iter()
            .zip(&self.root_identities)
        {
            let metadata = crate::checkpoint::safe_path(root)?.context("Bound root disappeared")?;
            platform::qualify(root, &metadata)?;
            ensure!(
                platform::identity(&metadata)? == *identity,
                "Maintenance root object was replaced"
            );
        }
        Ok(())
    }
}

/// Project the immutable original inventory through the completed operations
/// and, when present, the reconciled intermediate namespace/mode mutation.
pub(super) fn project(
    plan: &Plan,
    completed: usize,
    extra: Option<&Operation>,
    flag: &AtomicBool,
    work: &mut Work,
) -> Result<BTreeMap<String, Entry>> {
    ensure!(
        completed <= plan.operations.len(),
        "Maintenance progress exceeds operation count"
    );
    let mut entries = BTreeMap::new();
    for (path, entry) in &plan.entries {
        work.check(flag)?;
        entries.insert(path.clone(), entry.clone());
    }
    for operation in plan.operations[..completed].iter().chain(extra) {
        work.check(flag)?;
        match operation {
            Operation::Rename { from, to, .. } => {
                let prefix = format!("{from}/");
                let mut children = Vec::new();
                for (path, _) in entries
                    .range(prefix.clone()..)
                    .take_while(|(path, _)| path.starts_with(&prefix))
                {
                    work.check(flag)?;
                    children.push(path.clone());
                }
                let root_entry = entries
                    .remove(from)
                    .context("Maintenance rename source is absent from projected inventory")?;
                ensure!(
                    entries.insert(to.clone(), root_entry).is_none(),
                    "Maintenance projection collides"
                );
                for path in children {
                    work.check(flag)?;
                    let entry = entries
                        .remove(&path)
                        .context("Maintenance projected descendant disappeared")?;
                    let suffix = path.strip_prefix(&prefix).unwrap();
                    ensure!(
                        entries.insert(format!("{to}/{suffix}"), entry).is_none(),
                        "Maintenance projected descendant collision"
                    );
                }
            }
            Operation::OwnerMode { path, after, .. } => {
                entries
                    .get_mut(path)
                    .context("Maintenance mode path not in immutable inventory")?
                    .mode = *after;
            }
        }
    }
    Ok(entries)
}

pub(super) fn verify_tree(
    plan: &Plan,
    expected: &BTreeMap<String, Entry>,
    flag: &AtomicBool,
    work: &mut Work,
) -> Result<()> {
    plan.verify_roots()?;
    ensure!(
        inventory(&plan.root(), &plan.subtree, flag, work)? == *expected,
        "Maintenance subtree contains foreign content, identity, names or mode changes; recovery intent retained"
    );
    Ok(())
}
