//! A mod-root sibling fence persists independently of the game journal.
//! Terminal records prove cleanup only; they never contain write operations.
use super::{
    Action, Capability, Outcome, OutcomeStatus, Status, TreeDisposition, TreeRoot,
    plan::{self, Plan, Work},
    platform,
};
use crate::{checkpoint::RootBinding, paths::Roots};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

pub(super) const MAX_FENCE: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Terminal {
    pub disposition: TreeDisposition,
    pub entries_digest: String,
    pub entry_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Record {
    version: u32,
    pub digest: String,
    pub nonce: String,
    roots: RootBinding,
    root_identities: [platform::Identity; 2],
    target: TreeRoot,
    subtree: String,
    action: Action,
    capability: Capability,
    pub operation_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<Terminal>,
}

pub(super) fn path(root: &Path) -> Result<PathBuf> {
    ensure!(
        root.is_absolute() && root.parent().is_some(),
        "Mod maintenance fence needs an absolute non-root directory"
    );
    ensure!(
        crate::checkpoint::safe_path(root)?.is_some_and(|m| m.is_dir()),
        "Mod maintenance root is missing or linked"
    );
    ensure!(
        fs::canonicalize(root)? == root,
        "Mod maintenance root must be canonical"
    );
    let spelling = root.to_str().context("Mod maintenance root must be UTF8")?;
    let hash = format!("{:x}", Sha256::digest(spelling.as_bytes()));
    Ok(root
        .parent()
        .unwrap()
        .join(format!(".modsync-maintenance-{}.json", &hash[..16])))
}

pub(super) fn ensure_clear(root: &Path) -> Result<()> {
    let path = path(root)?;
    ensure!(
        crate::checkpoint::safe_path(&path)?.is_none(),
        "Unfinished maintenance fences mod acquisition; inspect maintenance status and recover its selected operation"
    );
    // Absence is accepted only after the containing namespace can be flushed.
    // This includes an unlink whose preceding cleanup flush failed.
    crate::checkpoint::persistence::namespace_boundary(path.parent().unwrap())
}

fn read(path: &Path) -> Result<Vec<u8>> {
    let metadata =
        crate::checkpoint::safe_path(path)?.context("Mod maintenance fence disappeared")?;
    platform::qualify_private(path, &metadata)?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_FENCE as u64,
        "Mod maintenance fence is not a bounded owned regular file"
    );
    let mut bytes = Vec::new();
    let identity = platform::identity_at(path, &metadata)?;
    let file = crate::paths::open_regular_read(path)?;
    ensure!(
        platform::identity_file(&file)? == identity,
        "Mod maintenance fence changed while opening"
    );
    file.take(MAX_FENCE as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_FENCE,
        "Mod maintenance fence exceeds 64 KiB"
    );
    let after = crate::checkpoint::safe_path(path)?
        .context("Mod maintenance fence disappeared during read")?;
    ensure!(
        platform::identity_at(path, &after)? == identity
            && after.len() == metadata.len()
            && after.modified()? == metadata.modified()?,
        "Mod maintenance fence changed during read"
    );
    Ok(bytes)
}

fn valid_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn validate(record: &Record, root: &Path) -> Result<()> {
    ensure!(
        record.version == 1
            && valid_hex(&record.digest)
            && record.operation_count <= plan::MAX_ENTRIES,
        "Mod maintenance fence version/digest/count is malformed"
    );
    let nonce = uuid::Uuid::parse_str(&record.nonce)?;
    ensure!(
        !nonce.is_nil() && nonce.to_string() == record.nonce,
        "Mod maintenance fence nonce is malformed"
    );
    let roots = record.roots();
    ensure!(
        roots.mods == root,
        "Mod maintenance fence is bound to another root"
    );
    plan::validate_roots(&roots)?;
    platform::validate_capability(&record.capability)?;
    for identity in &record.root_identities {
        platform::validate_identity(identity, &record.capability)?;
    }
    ensure!(
        crate::paths::relative(&record.subtree, false)?
            .to_string_lossy()
            .replace('\\', "/")
            == record.subtree,
        "Mod maintenance fence subtree is malformed"
    );
    if let Some(terminal) = &record.terminal {
        ensure!(
            valid_hex(&terminal.entries_digest)
                && (1..=plan::MAX_ENTRIES).contains(&terminal.entry_count),
            "Mod maintenance terminal proof is malformed"
        );
    }
    Ok(())
}

impl Record {
    pub fn roots(&self) -> Roots {
        Roots {
            mods: PathBuf::from(&self.roots.mods),
            game: PathBuf::from(&self.roots.game),
        }
    }
    pub fn from_plan(plan: &Plan, digest: &str, nonce: &str) -> Self {
        Self {
            version: 1,
            digest: digest.into(),
            nonce: nonce.into(),
            roots: plan.roots.clone(),
            root_identities: plan.root_identities.clone(),
            target: plan.target,
            subtree: plan.subtree.clone(),
            action: plan.action,
            capability: plan.capability.clone(),
            operation_count: plan.operations.len(),
            terminal: None,
        }
    }
    pub fn matches_plan(&self, plan: &Plan, digest: &str, nonce: &str) -> Result<()> {
        let mut bound = self.clone();
        bound.terminal = None;
        ensure!(
            bound == Self::from_plan(plan, digest, nonce),
            "Mod maintenance fence differs from the primary immutable intent"
        );
        Ok(())
    }
    pub fn status(&self) -> Status {
        Status {
            pending: true,
            digest: Some(self.digest.clone()),
            operation_count: self.operation_count,
            phase: Some(
                if self.terminal.is_some() {
                    "TerminalCleanup"
                } else {
                    "ModFencePending"
                }
                .into(),
            ),
        }
    }
    pub fn verify_terminal(&self, flag: &AtomicBool, work: &mut Work) -> Result<TreeDisposition> {
        let terminal = self
            .terminal
            .as_ref()
            .context("Nonterminal mod fence cannot authorize cleanup without its primary intent")?;
        let roots = self.roots();
        validate(self, &roots.mods)?;
        for (root, expected) in [&roots.mods, &roots.game]
            .into_iter()
            .zip(&self.root_identities)
        {
            work.check(flag)?;
            let metadata = crate::checkpoint::safe_path(root)?
                .context("Mod maintenance bound root disappeared")?;
            platform::qualify(root, &metadata)?;
            ensure!(
                platform::identity_at(root, &metadata)? == *expected,
                "Mod maintenance bound root was replaced"
            );
        }
        let selected = match self.target {
            TreeRoot::Game => &roots.game,
            TreeRoot::Mods => &roots.mods,
        };
        let entries = plan::inventory(selected, &self.subtree, flag, work)?;
        ensure!(
            entries.len() == terminal.entry_count
                && plan::inventory_digest(&entries, flag, work)? == terminal.entries_digest,
            "Terminal mod fence tree proof differs; metadata retained"
        );
        Ok(terminal.disposition)
    }
}

pub(super) fn load(root: &Path) -> Result<Option<Record>> {
    let path = path(root)?;
    if crate::checkpoint::safe_path(&path)?.is_none() {
        return Ok(None);
    }
    let record: Record = serde_json::from_slice(&read(&path)?)
        .context("Malformed mod maintenance fence; acquisition remains fenced")?;
    validate(&record, root)?;
    Ok(Some(record))
}

fn publish(record: &Record, replace: bool) -> Result<()> {
    let root = record.roots().mods;
    validate(record, &root)?;
    let path = path(&root)?;
    let bytes = plan::document(record)?;
    ensure!(
        bytes.len() <= MAX_FENCE,
        "Mod maintenance fence exceeds 64 KiB"
    );
    let mut temporary =
        platform::private_temp(path.parent().unwrap(), "modsync-maintenance-private-")?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    let metadata =
        crate::checkpoint::safe_path(temporary.path())?.context("Private mod fence disappeared")?;
    platform::qualify_private(temporary.path(), &metadata)?;
    ensure!(
        platform::identity_at(temporary.path(), &metadata)?
            == platform::identity_file(temporary.as_file())?,
        "Private mod fence was replaced"
    );
    platform::publish_temp(temporary, &path, replace)?;
    ensure!(
        read(&path)? == bytes,
        "Published mod fence differs from its flushed document"
    );
    Ok(())
}

pub(super) fn ensure_active(plan: &Plan, digest: &str, nonce: &str) -> Result<Record> {
    if let Some(record) = load(&plan.roots().mods)? {
        record.matches_plan(plan, digest, nonce)?;
        return Ok(record);
    }
    let record = Record::from_plan(plan, digest, nonce);
    publish(&record, false)?;
    Ok(record)
}

pub(super) fn terminal(
    plan: &Plan,
    digest: &str,
    nonce: &str,
    disposition: TreeDisposition,
    flag: &AtomicBool,
    work: &mut Work,
) -> Result<Record> {
    let mut record = ensure_active(plan, digest, nonce)?;
    let count = match disposition {
        TreeDisposition::AppliedTree => plan.operations.len(),
        TreeDisposition::RestoredOriginalTree => 0,
    };
    let entries = plan::project(plan, count, None, flag, work)?;
    let proof = Terminal {
        disposition,
        entry_count: entries.len(),
        entries_digest: plan::inventory_digest(&entries, flag, work)?,
    };
    if let Some(existing) = &record.terminal {
        ensure!(
            *existing == proof,
            "Conflicting maintenance terminal proofs fence cleanup"
        );
    } else {
        record.terminal = Some(proof);
        publish(&record, true)?;
    }
    Ok(record)
}

pub(super) fn clear(record: &Record) -> Result<()> {
    ensure!(
        record.terminal.is_some(),
        "Nonterminal mod fence cannot be removed"
    );
    let root = record.roots().mods;
    let current = load(&root)?.context("Mod maintenance fence disappeared before cleanup")?;
    ensure!(
        current == *record,
        "Mod maintenance fence changed before cleanup"
    );
    let path = path(&root)?;
    fs::remove_file(&path)?;
    if let Err(error) = crate::checkpoint::persistence::namespace_boundary(path.parent().unwrap()) {
        return match publish(record, false) {
            Ok(()) => Err(error).context("Mod fence cleanup flush failed; terminal fence re-published"),
            Err(republish) => Err(error).context(format!("Mod fence cleanup flush and republication failed: {republish:#}; absence guards must flush before admitting acquisition")),
        };
    }
    Ok(())
}

pub(super) fn recover_only(root: &Path, digest: &str, flag: &AtomicBool) -> Result<Outcome> {
    let record = load(root)?.context("No mod maintenance fence exists")?;
    ensure!(
        record.digest == digest,
        "Mod fence recovery identity differs from selected digest"
    );
    ensure!(
        record.target == TreeRoot::Mods,
        "Game-target terminal cleanup requires both root locks and paired maintenance recovery"
    );
    crate::recovery::ensure_checkpoint_clean(&record.roots())?;
    ensure!(
        crate::checkpoint::safe_path(&crate::recovery::state_dir(&record.roots()).join("pending"))?
            .is_none(),
        "Legacy install recovery must complete before terminal mod fence cleanup"
    );
    // A remaining primary intent must use paired recovery. Root-only admission
    // cannot bypass the other root's lock or cleanup ownership checks.
    let directory = crate::recovery::state_dir(&record.roots()).join("maintenance-v1");
    if let Some(metadata) = crate::checkpoint::safe_path(&directory)? {
        ensure!(
            metadata.is_dir() && fs::read_dir(&directory)?.next().is_none(),
            "Primary maintenance artifacts require paired recovery"
        );
        crate::checkpoint::persistence::namespace_boundary(&directory)?;
    }
    let mut work = Work::new();
    let result = record
        .verify_terminal(flag, &mut work)
        .and_then(|disposition| {
            clear(&record)?;
            Ok(disposition)
        });
    match result {
        Ok(disposition) => Ok(Outcome {
            digest: record.digest,
            completed_operations: if disposition == TreeDisposition::AppliedTree {
                record.operation_count
            } else {
                0
            },
            recovered: true,
            status: OutcomeStatus::Applied,
            failure: None,
            disposition: Some(disposition),
        }),
        Err(error) => Ok(Outcome {
            digest: record.digest,
            completed_operations: 0,
            recovered: true,
            status: OutcomeStatus::RecoveryRequired,
            failure: Some(format!("{error:#}").chars().take(4096).collect()),
            disposition: None,
        }),
    }
}
