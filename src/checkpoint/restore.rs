use super::{
    model::*,
    read::*,
    store::{Store, namespace_boundary},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

fn writable_directory(path: &Path) -> Result<()> {
    let metadata = safe_path(path)?.context("Restore directory missing")?;
    ensure!(metadata.is_dir(), "Restore directory changed type");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(metadata.permissions().mode() | 0o700),
        )?;
    }
    Ok(())
}
fn set_mode(path: &Path, value: &Mode) -> Result<()> {
    safe_path(path)?.context("Restore mode target missing")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(
                value
                    .unix
                    .context("Unix restore requires Unix mode metadata")?,
            ),
        )?;
    }
    #[cfg(not(unix))]
    {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_readonly(value.readonly);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}
fn inventory(path: &Path, flag: &AtomicBool) -> Result<Vec<PathBuf>> {
    cancelled(flag)?;
    let Some(metadata) = safe_path(path)? else {
        return Ok(Vec::new());
    };
    if !metadata.is_dir() {
        return Ok(vec![path.to_owned()]);
    }
    let mut paths = Vec::new();
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        cancelled(flag)?;
        ensure!(
            paths.len() <= MAX_ENTRIES,
            "Live restore tree exceeds entry bound"
        );
        let entry = entry?;
        safe_path(entry.path())?.context("Restore path disappeared")?;
        paths.push(entry.into_path());
    }
    Ok(paths)
}
fn remove_owned(path: &Path, flag: &AtomicBool) -> Result<()> {
    let mut paths = inventory(path, flag)?;
    for directory in &paths {
        cancelled(flag)?;
        if safe_path(directory)?.is_some_and(|m| m.is_dir()) {
            writable_directory(directory)?;
        }
    }
    paths.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for path in paths {
        cancelled(flag)?;
        let Some(metadata) = safe_path(&path)? else {
            continue;
        };
        if metadata.is_dir() {
            fs::remove_dir(&path)?;
        } else {
            super::persistence::writable_file(&path)?;
            fs::remove_file(&path)?;
        }
        namespace_boundary(path.parent().context("Removed path has no parent")?)?;
    }
    Ok(())
}
fn ensure_directory(path: &Path, flag: &AtomicBool) -> Result<()> {
    cancelled(flag)?;
    if let Some(metadata) = safe_path(path)? {
        if metadata.is_dir() {
            return writable_directory(path);
        }
        super::persistence::writable_file(path)?;
        fs::remove_file(path)?;
        namespace_boundary(path.parent().context("Directory has no parent")?)?;
    }
    let parent = path.parent().context("Restore directory needs parent")?;
    ensure!(
        safe_path(parent)?.is_some_and(|m| m.is_dir()),
        "Restore parent is missing or outside owned creation set"
    );
    fs::create_dir(path)?;
    namespace_boundary(parent)?;
    Ok(())
}

impl Store {
    pub(super) fn verify_target(
        &self,
        session: &Session,
        target: &Checkpoint,
        flag: &AtomicBool,
    ) -> Result<()> {
        self.validate_participants(&session.participants, &session.ancestor_shells)?;
        super::auxiliary::auxiliary_totals(target, session)?;
        for (hash, size) in super::auxiliary::objects(target) {
            cancelled(flag)?;
            ensure!(
                hash_file(&self.object_path(&hash)?, flag)? == (hash, size),
                "Restore target object is corrupt or unavailable"
            );
        }
        let check_mode = |value: &Mode| -> Result<()> {
            #[cfg(not(unix))]
            let _ = value;
            #[cfg(unix)]
            ensure!(
                value.unix.is_some(),
                "Unix restore requires captured Unix modes"
            );
            Ok(())
        };
        let check_tree = |tree: &Tree| -> Result<()> {
            check_mode(&tree.root_mode)?;
            for entry in tree.entries.values() {
                match entry {
                    Entry::Directory { mode } | Entry::File { mode, .. } => check_mode(mode)?,
                }
            }
            Ok(())
        };
        check_tree(&target.mods)?;
        check_tree(&target.game)?;
        for snapshot in target.auxiliary_snapshots.values() {
            match snapshot {
                AuxiliarySnapshot::Absent => {}
                AuxiliarySnapshot::File { mode, .. } => check_mode(mode)?,
                AuxiliarySnapshot::Directory { tree } => check_tree(tree)?,
            }
        }
        for value in target.ancestor_shells.values().flatten() {
            check_mode(value)?;
        }
        Ok(())
    }

    pub(super) fn preflight_materialization(
        &self,
        session: &Session,
        target: &Checkpoint,
        flag: &AtomicBool,
    ) -> Result<()> {
        for path in [&self.roots.mods, &self.roots.game] {
            ensure!(
                safe_path(path)?.is_some_and(|metadata| metadata.is_dir()),
                "Both install roots must remain directories; root replacement is forbidden"
            );
            inventory(path, flag)?;
        }
        for binding in session.participants.values() {
            if binding.kind == ParticipantKind::File {
                ensure!(
                    safe_path(Path::new(&binding.path))?.is_none_or(|metadata| metadata.is_file()),
                    "File participant changed into a directory; recursive ownership is not authorized"
                );
            }
            inventory(Path::new(&binding.path), flag)?;
        }
        // No shell grants recursive ownership of siblings. Refuse an absent-shell
        // target before any mutation if foreign descendants would prevent removal.
        for (shell, desired) in &target.ancestor_shells {
            if desired.is_some() {
                continue;
            }
            for path in inventory(Path::new(shell), flag)? {
                if path == Path::new(shell) {
                    continue;
                }
                ensure!(
                    session
                        .participants
                        .values()
                        .any(|binding| path.starts_with(&binding.path))
                        || session
                            .ancestor_shells
                            .iter()
                            .any(|known| path == Path::new(known)),
                    "Foreign descendant prevents restoring absent auxiliary ancestor {}",
                    shell
                );
            }
        }
        Ok(())
    }

    pub(super) fn materialize(
        &self,
        session: &Session,
        target: &Checkpoint,
        operation_id: &str,
        flag: &AtomicBool,
        observe: &mut impl FnMut(Progress),
    ) -> Result<()> {
        self.verify_target(session, target, flag)?;
        self.preflight_materialization(session, target, flag)?;
        let mut shells: Vec<_> = target.ancestor_shells.keys().collect();
        shells.sort_by_key(|path| Path::new(path).components().count());
        for shell in &shells {
            // Only create a shell when some target participant needs it.
            if target.ancestor_shells[*shell].is_some() {
                ensure_directory(Path::new(shell), flag)?;
            } else if safe_path(Path::new(shell))?.is_some() {
                writable_directory(Path::new(shell))?;
            }
        }
        let mut files = 0u64;
        let mut bytes = 0u64;
        self.restore_tree(
            &self.roots.mods,
            &target.mods,
            operation_id,
            flag,
            &mut files,
            &mut bytes,
            observe,
        )?;
        self.restore_tree(
            &self.roots.game,
            &target.game,
            operation_id,
            flag,
            &mut files,
            &mut bytes,
            observe,
        )?;
        for (name, binding) in &session.participants {
            let path = Path::new(&binding.path);
            match &target.auxiliary_snapshots[name] {
                AuxiliarySnapshot::Absent => remove_owned(path, flag)?,
                AuxiliarySnapshot::File { sha256, size, mode } => {
                    if safe_path(path)?.is_some_and(|m| m.is_dir()) {
                        remove_owned(path, flag)?;
                    }
                    self.restore_file(path, sha256, *size, mode, operation_id, flag)?;
                    files += 1;
                    bytes += size;
                }
                AuxiliarySnapshot::Directory { tree } => {
                    ensure_directory(path, flag)?;
                    self.restore_tree(
                        path,
                        tree,
                        operation_id,
                        flag,
                        &mut files,
                        &mut bytes,
                        observe,
                    )?;
                }
            }
        }
        shells.reverse();
        for shell in shells {
            cancelled(flag)?;
            if let Some(value) = &target.ancestor_shells[shell] {
                set_mode(Path::new(shell), value)?;
                namespace_boundary(Path::new(shell))?;
            } else if safe_path(Path::new(shell))?.is_some() {
                // remove_dir is deliberately nonrecursive and refuses foreign data.
                fs::remove_dir(shell)?;
                namespace_boundary(Path::new(shell).parent().context("Shell has no parent")?)?;
            }
        }
        ensure!(
            self.live_matches(session, target, flag)?,
            "Restored roots or auxiliary inventory differ from target"
        );
        observe(Progress {
            phase: "restored",
            files,
            bytes,
        });
        Ok(())
    }

    pub(super) fn live_matches(
        &self,
        session: &Session,
        target: &Checkpoint,
        flag: &AtomicBool,
    ) -> Result<bool> {
        Ok(self.scan(&self.roots.mods, flag)? == target.mods
            && self.scan(&self.roots.game, flag)? == target.game
            && self.scan_auxiliary(&session.participants, &session.ancestor_shells, flag)?
                == (
                    target.auxiliary_snapshots.clone(),
                    target.ancestor_shells.clone(),
                ))
    }

    #[allow(clippy::too_many_arguments)]
    fn restore_tree(
        &self,
        root: &Path,
        tree: &Tree,
        operation: &str,
        flag: &AtomicBool,
        files: &mut u64,
        bytes: &mut u64,
        observe: &mut impl FnMut(Progress),
    ) -> Result<()> {
        writable_directory(root)?;
        let mut existing = inventory(root, flag)?;
        for path in &existing {
            if safe_path(path)?.is_some_and(|m| m.is_dir()) {
                writable_directory(path)?;
            }
        }
        existing.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        for path in existing {
            cancelled(flag)?;
            if path == root {
                continue;
            }
            let relative = path
                .strip_prefix(root)?
                .to_str()
                .context("Non-UTF8 live path")?
                .replace(std::path::MAIN_SEPARATOR, "/");
            let Some(metadata) = safe_path(&path)? else {
                continue;
            };
            let keep = match tree.entries.get(&relative) {
                Some(Entry::Directory { .. }) => metadata.is_dir(),
                Some(Entry::File { .. }) => metadata.is_file(),
                None => false,
            };
            if !keep {
                if metadata.is_dir() {
                    fs::remove_dir(&path)?;
                } else {
                    super::persistence::writable_file(&path)?;
                    fs::remove_file(&path)?;
                }
                namespace_boundary(path.parent().context("Restore path has no parent")?)?;
            }
        }
        let mut directories: Vec<_> = tree
            .entries
            .iter()
            .filter_map(|(path, entry)| {
                if let Entry::Directory { mode } = entry {
                    Some((path, mode))
                } else {
                    None
                }
            })
            .collect();
        directories.sort_by_key(|(path, _)| Path::new(path).components().count());
        for (path, _) in &directories {
            ensure_directory(&root.join(path), flag)?;
        }
        for (path, entry) in &tree.entries {
            if let Entry::File { sha256, size, mode } = entry {
                self.restore_file(&root.join(path), sha256, *size, mode, operation, flag)?;
                *files += 1;
                *bytes += size;
                observe(Progress {
                    phase: "materializing",
                    files: *files,
                    bytes: *bytes,
                });
            }
        }
        directories.reverse();
        for (path, mode) in directories {
            set_mode(&root.join(path), mode)?;
            namespace_boundary(&root.join(path))?;
        }
        set_mode(root, &tree.root_mode)?;
        namespace_boundary(root)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn restore_file(
        &self,
        target: &Path,
        hash: &str,
        size: u64,
        mode: &Mode,
        operation: &str,
        flag: &AtomicBool,
    ) -> Result<()> {
        cancelled(flag)?;
        let parent = target.parent().context("Restore file has no parent")?;
        ensure!(
            safe_path(parent)?.is_some_and(|m| m.is_dir()),
            "Restore file parent missing"
        );
        if let Some(metadata) = safe_path(target)? {
            ensure!(metadata.is_file(), "Restore file target changed type");
        }
        let identity = format!(
            "{:x}",
            Sha256::digest(target.as_os_str().as_encoded_bytes())
        );
        let temporary = parent.join(format!(".modsync-restore-{operation}-{}", &identity[..16]));
        if safe_path(&temporary)?.is_some() {
            ensure!(
                safe_path(&temporary)?.is_some_and(|m| m.is_file()),
                "Restore temporary changed type"
            );
            super::persistence::writable_file(&temporary)?;
            fs::remove_file(&temporary)?;
        }
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        let mut output = options.open(&temporary)?;
        let result = (|| -> Result<()> {
            let mut input = open_file(&self.object_path(hash)?)?;
            let mut observed = Sha256::new();
            let mut written = 0u64;
            let mut buffer = [0u8; 65536];
            loop {
                cancelled(flag)?;
                let n = input.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                written += n as u64;
                ensure!(written <= size, "Restore object grew");
                output.write_all(&buffer[..n])?;
                observed.update(&buffer[..n]);
            }
            ensure!(
                written == size && format!("{:x}", observed.finalize()) == hash,
                "Restore object hash differs"
            );
            // Flush through the writable descriptor and close before Windows rename.
            #[cfg(unix)]
            set_mode(&temporary, mode)?;
            super::persistence::flush_file(&output)?;
            drop(output);
            cancelled(flag)?;
            super::persistence::writable_file(target)?;
            super::persistence::rename(&temporary, target, true)?;
            #[cfg(not(unix))]
            set_mode(target, mode)?;
            namespace_boundary(parent)?;
            Ok(())
        })();
        if result.is_err() {
            // Best-effort removal stays inside this operation-owned temporary.
            let _ = super::persistence::writable_file(&temporary);
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}
