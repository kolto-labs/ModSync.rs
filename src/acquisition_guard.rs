//! Owned root locks for standalone acquisition adapters. Low-level transfers
//! remain unlocked so an orchestrator can hold one guard over all its workers.
use crate::recovery::ModRootLock;
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

/// Locks out cooperating writers for the complete acquisition lifetime.
/// A declared source root coordinates nested outputs with install/maintenance.
/// Without one, only each destination's immediate parent is in scope.
pub struct Scope {
    locks: Vec<ModRootLock>,
    destinations: Vec<PathBuf>,
    bindings: BTreeMap<PathBuf, usize>,
}

impl Scope {
    pub fn acquire(
        destinations: &[PathBuf],
        source_root: Option<&Path>,
        cancelled: &AtomicBool,
    ) -> Result<Self> {
        ensure!(
            destinations.len() <= 10_000,
            "Acquisition exceeds 10000 destinations"
        );
        let mut bindings = Vec::with_capacity(destinations.len());
        let mut roots = BTreeMap::new();
        for destination in destinations {
            check_cancelled(cancelled)?;
            ensure!(
                destination.as_os_str().len() <= 16 * 1024
                    && !destination
                        .components()
                        .any(|part| matches!(part, Component::ParentDir)),
                "Acquisition destination is too long or contains parent traversal"
            );
            let absolute = std::path::absolute(destination)?;
            let parent = absolute
                .parent()
                .context("Acquisition destination needs a parent")?;
            ensure!(
                crate::checkpoint::safe_path(parent)?.is_some_and(|metadata| metadata.is_dir()),
                "Acquisition destination parent must already be an ordinary directory"
            );
            let parent = std::fs::canonicalize(parent)?;
            let name = absolute
                .file_name()
                .context("Acquisition destination needs a filename")?;
            let name = name
                .to_str()
                .context("Acquisition filename must be UTF-8")?;
            crate::download::one_component(name)?;
            let output = parent.join(name);
            ensure!(
                crate::checkpoint::safe_path(&output)?.is_none(),
                "Acquisition destination already exists"
            );
            let authored_root = source_root.unwrap_or(&parent);
            ensure!(
                !authored_root
                    .components()
                    .any(|part| matches!(part, Component::ParentDir)),
                "Acquisition source root contains parent traversal"
            );
            let authored_root = std::path::absolute(authored_root)?;
            ensure!(
                crate::checkpoint::safe_path(&authored_root)?
                    .is_some_and(|metadata| metadata.is_dir()),
                "Acquisition source root must already be an ordinary directory"
            );
            let root = std::fs::canonicalize(&authored_root)?;
            ensure!(
                output.starts_with(&root) && output != root,
                "Acquisition output is outside its declared source root"
            );
            roots.entry(root.clone()).or_insert(authored_root);
            bindings.push((root, output));
        }
        let mut locks = Vec::with_capacity(roots.len());
        let mut root_indices = BTreeMap::new();
        for (canonical, authored) in roots {
            check_cancelled(cancelled)?;
            let lock = ModRootLock::acquire(&authored)?;
            ensure!(
                lock.root() == canonical,
                "Acquisition source root changed while acquiring its lock"
            );
            root_indices.insert(canonical, locks.len());
            locks.push(lock);
        }
        let mut outputs = std::collections::BTreeSet::new();
        let mut destination_bindings = BTreeMap::new();
        for (root, output) in &bindings {
            check_cancelled(cancelled)?;
            let index = *root_indices
                .get(root)
                .context("Missing acquisition root guard")?;
            let lock = &locks[index];
            lock.contains_destination(output)?;
            ensure!(
                crate::checkpoint::safe_path(output)?.is_none(),
                "Acquisition destination appeared before starting"
            );
            ensure!(
                outputs.insert(output.clone()),
                "Acquisition has duplicate destinations"
            );
            destination_bindings.insert(output.clone(), index);
        }
        Ok(Self {
            locks,
            destinations: bindings.into_iter().map(|(_, path)| path).collect(),
            bindings: destination_bindings,
        })
    }

    pub fn destinations(&self) -> &[PathBuf] {
        &self.destinations
    }

    /// Repeat root identity and safe ancestor checks while retaining the locks.
    pub fn verify(&self) -> Result<()> {
        for destination in &self.destinations {
            self.verify_destination(destination)?;
        }
        Ok(())
    }

    pub fn verify_destination(&self, destination: &Path) -> Result<()> {
        let index = *self
            .bindings
            .get(destination)
            .context("Acquisition destination is not bound to this guard")?;
        self.locks[index].contains_destination(destination)
    }
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Acquisition cancelled before starting"
    );
    Ok(())
}
