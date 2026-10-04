use super::{model::*, read::*, store::Store};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path},
    sync::atomic::AtomicBool,
};

type AuxiliaryInventory = (
    BTreeMap<String, AuxiliarySnapshot>,
    BTreeMap<String, Option<Mode>>,
);
pub(super) type CaptureInventory = (
    Tree,
    Tree,
    BTreeMap<String, AuxiliarySnapshot>,
    BTreeMap<String, Option<Mode>>,
    u64,
    u64,
);

fn binding_path(value: &str) -> Result<&Path> {
    let path = Path::new(value);
    ensure!(
        path.is_absolute()
            && !value.is_empty()
            && !path
                .components()
                .any(|part| matches!(part, Component::CurDir | Component::ParentDir)),
        "Auxiliary binding must be an absolute normalized path"
    );
    ensure!(
        std::path::absolute(path)?.as_os_str() == path.as_os_str(),
        "Auxiliary binding is not normalized"
    );
    super::persistence::require_canonical_location(path)?;
    Ok(path)
}
fn key(path: &Path) -> String {
    icu_normalizer::ComposingNormalizer::new_nfc()
        .normalize(&path.to_string_lossy())
        .to_lowercase()
}
fn overlaps(a: &str, b: &str) -> bool {
    a == b
        || a.strip_prefix(b)
            .is_some_and(|rest| rest.starts_with(std::path::MAIN_SEPARATOR))
        || b.strip_prefix(a)
            .is_some_and(|rest| rest.starts_with(std::path::MAIN_SEPARATOR))
}

impl Store {
    pub(super) fn validate_participants(
        &self,
        participants: &BTreeMap<String, ParticipantBinding>,
        shells: &[String],
    ) -> Result<()> {
        ensure!(
            participants.len() <= 32 && shells.len() <= 64,
            "Auxiliary binding inventory exceeds bound"
        );
        let recovery_root = crate::recovery::state_dir(&self.roots);
        let protected = [&self.roots.mods, &self.roots.game, &recovery_root];
        let mut paths: Vec<String> = Vec::new();
        for (name, binding) in participants {
            ensure!(
                !name.is_empty()
                    && name.len() <= 64
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
                "Invalid auxiliary participant identifier"
            );
            let path = binding_path(&binding.path)?;
            let identity = key(path);
            for protected in protected {
                ensure!(
                    !overlaps(&identity, &key(protected)),
                    "Auxiliary participant overlaps install roots or checkpoint store"
                );
            }
            ensure!(
                paths.iter().all(|other| !overlaps(&identity, other)),
                "Auxiliary participants alias or overlap"
            );
            paths.push(identity);
        }
        let mut seen = BTreeSet::new();
        for shell in shells {
            let path = binding_path(shell)?;
            ensure!(seen.insert(key(path)), "Duplicate auxiliary ancestor shell");
            ensure!(
                participants
                    .values()
                    .any(|binding| Path::new(&binding.path) != path
                        && Path::new(&binding.path).starts_with(path)),
                "Ancestor shell does not contain an owned participant"
            );
            ensure!(
                !protected
                    .iter()
                    .any(|root| overlaps(&key(path), &key(root))),
                "Ancestor shell lies inside protected roots"
            );
            ensure!(
                !participants
                    .values()
                    .any(|binding| path.starts_with(&binding.path)),
                "Ancestor shell overlaps an owned participant"
            );
        }
        Ok(())
    }

    pub(super) fn participant_shells(
        &self,
        participants: &BTreeMap<String, ParticipantBinding>,
    ) -> Result<Vec<String>> {
        self.validate_participants(participants, &[])?;
        let mut shells = BTreeSet::new();
        for binding in participants.values() {
            let path = binding_path(&binding.path)?;
            safe_path(path)?;
            for parent in path.ancestors().skip(1) {
                if let Some(metadata) = safe_path(parent)? {
                    ensure!(metadata.is_dir(), "Auxiliary parent is not a directory");
                    break;
                }
                shells.insert(
                    parent
                        .to_str()
                        .context("Non-UTF8 auxiliary ancestor")?
                        .to_owned(),
                );
                ensure!(
                    shells.len() <= 64,
                    "Auxiliary ancestor inventory exceeds bound"
                );
            }
        }
        let shells: Vec<_> = shells.into_iter().collect();
        self.validate_participants(participants, &shells)?;
        Ok(shells)
    }

    pub(super) fn scan_auxiliary(
        &self,
        participants: &BTreeMap<String, ParticipantBinding>,
        shells: &[String],
        flag: &AtomicBool,
    ) -> Result<AuxiliaryInventory> {
        self.validate_participants(participants, shells)?;
        let mut snapshots = BTreeMap::new();
        for (name, binding) in participants {
            cancelled(flag)?;
            let path = Path::new(&binding.path);
            let snapshot = match safe_path(path)? {
                None => AuxiliarySnapshot::Absent,
                Some(metadata) => match binding.kind {
                    ParticipantKind::Directory => {
                        ensure!(metadata.is_dir(), "Auxiliary directory changed type");
                        AuxiliarySnapshot::Directory {
                            tree: self.scan(path, flag)?,
                        }
                    }
                    ParticipantKind::File => {
                        ensure!(metadata.is_file(), "Auxiliary file changed type");
                        let (sha256, size) = hash_file(path, flag)?;
                        ensure!(
                            size == metadata.len(),
                            "Auxiliary file changed during capture"
                        );
                        AuxiliarySnapshot::File {
                            sha256,
                            size,
                            mode: mode(&metadata),
                        }
                    }
                },
            };
            snapshots.insert(name.clone(), snapshot);
        }
        let mut modes = BTreeMap::new();
        for shell in shells {
            cancelled(flag)?;
            let value = if let Some(metadata) = safe_path(Path::new(shell))? {
                ensure!(metadata.is_dir(), "Auxiliary ancestor shell changed type");
                Some(mode(&metadata))
            } else {
                None
            };
            modes.insert(shell.clone(), value);
        }
        Ok((snapshots, modes))
    }

    pub(super) fn capture_all(
        &self,
        participants: &BTreeMap<String, ParticipantBinding>,
        shells: &[String],
        new_session: bool,
        flag: &AtomicBool,
        observe: &mut impl FnMut(Progress),
    ) -> Result<CaptureInventory> {
        let (auxiliary, ancestor_shells) = self.scan_auxiliary(participants, shells, flag)?;
        if new_session {
            for snapshot in auxiliary.values() {
                if let AuxiliarySnapshot::Directory { tree } = snapshot {
                    refuse_device_names(tree)?;
                }
            }
        }
        let (mods, game, mut count, mut bytes) = self.capture_trees(new_session, flag, observe)?;
        for (name, snapshot) in &auxiliary {
            let root = Path::new(&participants[name].path);
            let files: Vec<_> = match snapshot {
                AuxiliarySnapshot::Absent => Vec::new(),
                AuxiliarySnapshot::File { sha256, size, .. } => {
                    vec![(root.to_owned(), sha256, *size)]
                }
                AuxiliarySnapshot::Directory { tree } => tree
                    .entries
                    .iter()
                    .filter_map(|(path, entry)| {
                        if let Entry::File { sha256, size, .. } = entry {
                            Some((root.join(path), sha256, *size))
                        } else {
                            None
                        }
                    })
                    .collect(),
            };
            for (path, hash, size) in files {
                cancelled(flag)?;
                self.publish_object(&path, hash, size, flag)?;
                count = count
                    .checked_add(1)
                    .context("Checkpoint file count overflow")?;
                bytes = bytes
                    .checked_add(size)
                    .context("Checkpoint byte count overflow")?;
                observe(Progress {
                    phase: "auxiliary",
                    files: count,
                    bytes,
                });
            }
        }
        ensure!(
            self.scan(&self.roots.mods, flag)? == mods
                && self.scan(&self.roots.game, flag)? == game
                && self.scan_auxiliary(participants, shells, flag)?
                    == (auxiliary.clone(), ancestor_shells.clone()),
            "Install roots or auxiliary state changed during capture"
        );
        Ok((mods, game, auxiliary, ancestor_shells, count, bytes))
    }
}

pub(super) fn auxiliary_totals(document: &Checkpoint, session: &Session) -> Result<(u64, u64)> {
    ensure!(
        document
            .auxiliary_snapshots
            .keys()
            .eq(session.participants.keys())
            && document
                .ancestor_shells
                .keys()
                .eq(session.ancestor_shells.iter()),
        "Checkpoint auxiliary bindings differ from immutable session"
    );
    let mut count = 0u64;
    let mut bytes = 0u64;
    for (name, snapshot) in &document.auxiliary_snapshots {
        let (files, size) = match snapshot {
            AuxiliarySnapshot::Absent => (0, 0),
            AuxiliarySnapshot::File { sha256, size, mode } => {
                ensure!(
                    session.participants[name].kind == ParticipantKind::File,
                    "Auxiliary snapshot kind differs"
                );
                digest(sha256)?;
                validate_tree(&Tree {
                    root_mode: mode.clone(),
                    entries: BTreeMap::new(),
                })?;
                (1, *size)
            }
            AuxiliarySnapshot::Directory { tree } => {
                ensure!(
                    session.participants[name].kind == ParticipantKind::Directory,
                    "Auxiliary snapshot kind differs"
                );
                validate_tree(tree)?
            }
        };
        count = count
            .checked_add(files)
            .context("Auxiliary count overflow")?;
        bytes = bytes
            .checked_add(size)
            .context("Auxiliary bytes overflow")?;
    }
    for (shell, state) in &document.ancestor_shells {
        if state.is_none() {
            ensure!(
                session
                    .participants
                    .iter()
                    .all(
                        |(name, binding)| !Path::new(&binding.path).starts_with(shell)
                            || matches!(
                                document.auxiliary_snapshots[name],
                                AuxiliarySnapshot::Absent
                            )
                    ),
                "Absent shell contains a present participant"
            );
            ensure!(
                document
                    .ancestor_shells
                    .iter()
                    .all(|(child, value)| child == shell
                        || !Path::new(child).starts_with(shell)
                        || value.is_none()),
                "Absent shell contains a present ancestor shell"
            );
        }
    }
    for mode in document.ancestor_shells.values().flatten() {
        validate_tree(&Tree {
            root_mode: mode.clone(),
            entries: BTreeMap::new(),
        })?;
    }
    Ok((count, bytes))
}

pub(super) fn objects(document: &Checkpoint) -> BTreeSet<(String, u64)> {
    let mut objects = BTreeSet::new();
    let mut add = |entry: &Entry| {
        if let Entry::File { sha256, size, .. } = entry {
            objects.insert((sha256.clone(), *size));
        }
    };
    for entry in document
        .mods
        .entries
        .values()
        .chain(document.game.entries.values())
    {
        add(entry);
    }
    for snapshot in document.auxiliary_snapshots.values() {
        match snapshot {
            AuxiliarySnapshot::Absent => {}
            AuxiliarySnapshot::File { sha256, size, mode } => add(&Entry::File {
                sha256: sha256.clone(),
                size: *size,
                mode: mode.clone(),
            }),
            AuxiliarySnapshot::Directory { tree } => {
                for entry in tree.entries.values() {
                    add(entry);
                }
            }
        }
    }
    objects
}
