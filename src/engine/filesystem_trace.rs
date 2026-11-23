//! Opt-in, run-local filesystem evidence. Disk lanes are scanned independently
//! of the engine's virtual inventory so a bookkeeping error cannot prove itself.
use super::*;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

#[derive(Serialize)]
struct Entry {
    directory: bool,
    bytes: Option<u64>,
    sha256: Option<String>,
}

#[derive(Default, Serialize)]
struct Identity<'a> {
    owner_guid: Option<&'a str>,
    instruction: Option<usize>,
    action: Option<&'a str>,
    skipped: bool,
}

#[derive(Serialize)]
struct Manifest<'a> {
    version: u32,
    #[serde(flatten)]
    identity: Identity<'a>,
    entries: BTreeMap<String, Entry>,
}

pub(super) struct Trace {
    directory: PathBuf,
    next: usize,
    active: bool,
    // A private cache lives only for this run. Unix ctime/inode stamps invalidate
    // same-size replacements and edits whose mtime was restored by an installer.
    cache: BTreeMap<PathBuf, (String, u64, String)>,
    ignored_files: BTreeMap<PathBuf, Content>,
    ignored_directories: Vec<(PathBuf, String)>,
}

impl Engine {
    /// Retain portable, content-addressed inventories outside both install roots.
    /// The directory must be fresh. Final capture precedes recovery finalization.
    pub fn with_filesystem_trace(mut self, directory: &Path) -> Result<Self> {
        let parent = fs::canonicalize(
            directory
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?;
        let directory = parent.join(
            directory
                .file_name()
                .context("Trace needs a directory name")?,
        );
        for root in [&self.settings.roots.game, &self.settings.roots.mods] {
            ensure!(
                !directory.starts_with(root) && !root.starts_with(&directory),
                "Filesystem trace storage must be outside install roots"
            );
        }
        ensure!(
            self.filesystem_trace.is_none(),
            "Filesystem trace already configured"
        );
        // Peer download staging is deliberately unavailable to instructions, but
        // remains part of an exact inventory of the original mod tree.
        let mut ignored_files = BTreeMap::new();
        let mut ignored_directories = Vec::new();
        let mut ignored_roots = Vec::new();
        let walk = walkdir::WalkDir::new(&self.settings.roots.mods)
            .into_iter()
            .filter_entry(|entry| {
                if entry.file_type().is_dir()
                    && crate::download_orchestrator::is_peer_staging(entry.file_name())
                {
                    ignored_roots.push(entry.path().to_owned());
                    false
                } else {
                    true
                }
            });
        for entry in walk {
            self.check_cancelled()?;
            entry?;
        }
        for root in ignored_roots {
            for entry in walkdir::WalkDir::new(root) {
                self.check_cancelled()?;
                let entry = entry?;
                self.settings.roots.check(entry.path())?;
                if entry.file_type().is_dir() {
                    let stamp = selection_file_stamp(entry.path())?;
                    ignored_directories.push((entry.into_path(), stamp));
                } else {
                    ensure!(
                        entry.file_type().is_file(),
                        "Trace encountered an unsupported staging entry"
                    );
                    let path = entry.into_path();
                    ignored_files.insert(path.clone(), Content::Disk(path));
                }
            }
        }
        fs::create_dir(&directory).context("Filesystem trace directory must be fresh")?;
        self.filesystem_trace = Some(Trace {
            directory,
            next: 0,
            active: false,
            cache: BTreeMap::new(),
            ignored_files,
            ignored_directories,
        });
        Ok(self)
    }

    pub(super) fn begin_filesystem_trace(&mut self) -> Result<()> {
        if let Some(trace) = &mut self.filesystem_trace {
            trace.active = true;
        }
        self.capture_filesystem_trace(Identity::default(), false)
    }

    pub(super) fn trace_instruction(
        &mut self,
        owner: &Component,
        index: usize,
        instruction: &Instruction,
        skipped: bool,
    ) -> Result<()> {
        self.capture_filesystem_trace(
            Identity {
                owner_guid: Some(&owner.guid),
                instruction: Some(index),
                action: Some(&instruction.action),
                skipped,
            },
            false,
        )
    }

    pub(super) fn finish_filesystem_trace(&mut self) -> Result<()> {
        self.capture_filesystem_trace(Identity::default(), true)?;
        if let Some(trace) = &mut self.filesystem_trace {
            trace.active = false;
        }
        Ok(())
    }

    fn capture_filesystem_trace(
        &mut self,
        identity: Identity<'_>,
        final_state: bool,
    ) -> Result<()> {
        // Managed installation performs internal read-only component preflights.
        // They do not represent reached on-disk instructions.
        if !self.install && !self.exact_patcher_prediction {
            return Ok(());
        }
        let Some(mut trace) = self.filesystem_trace.take() else {
            return Ok(());
        };
        let result = if trace.active {
            self.write_filesystem_trace(&mut trace, identity, final_state)
        } else {
            Ok(())
        };
        self.filesystem_trace = Some(trace);
        result
    }

    fn write_filesystem_trace(
        &self,
        trace: &mut Trace,
        identity: Identity<'_>,
        final_state: bool,
    ) -> Result<()> {
        self.check_cancelled()?;
        let mut files = BTreeMap::new();
        let mut directories = Vec::new();
        let mut directory_stamps = Vec::new();
        if self.install {
            for root in [&self.settings.roots.game, &self.settings.roots.mods] {
                for entry in walkdir::WalkDir::new(root) {
                    self.check_cancelled()?;
                    let entry = entry?;
                    ensure!(
                        !entry.file_type().is_symlink(),
                        "Trace encountered a symlink"
                    );
                    if entry.file_type().is_dir() {
                        self.settings.roots.check(entry.path())?;
                        directory_stamps
                            .push((entry.path().to_owned(), selection_file_stamp(entry.path())?));
                        directories.push(entry.into_path());
                    } else {
                        ensure!(
                            entry.file_type().is_file(),
                            "Trace encountered a special file"
                        );
                        let path = entry.into_path();
                        files.insert(path.clone(), Content::Disk(path));
                    }
                }
            }
        } else {
            // Virtual parent bookkeeping can include ancestors above the roots.
            directories.extend(
                self.directories
                    .values()
                    .filter(|path| {
                        path.starts_with(&self.settings.roots.game)
                            || path.starts_with(&self.settings.roots.mods)
                    })
                    .cloned(),
            );
            files.extend(self.files.values().cloned());
            files.extend(
                trace
                    .ignored_files
                    .iter()
                    .map(|(path, content)| (path.clone(), content.clone())),
            );
            for (path, stamp) in &trace.ignored_directories {
                ensure!(
                    selection_file_stamp(path)? == *stamp,
                    "Ignored peer staging changed during prediction"
                );
                directories.push(path.clone());
                directory_stamps.push((path.clone(), stamp.clone()));
            }
        }
        // Include directory stamps in the stability check. A file can be added
        // during hashing without changing any already-observed file's stamp.
        let mut entries = BTreeMap::new();
        for directory in directories {
            entries.insert(
                self.trace_path(&directory)?,
                Entry {
                    directory: true,
                    bytes: None,
                    sha256: None,
                },
            );
        }
        let mut file_stamps = Vec::new();
        for (logical, content) in files {
            self.check_cancelled()?;
            let backing = content.path()?;
            self.settings.roots.check(&logical)?;
            let before = selection_file_stamp(backing)?;
            let cached = trace
                .cache
                .get(backing)
                .filter(|(stamp, _, _)| !final_state && cfg!(unix) && stamp == &before);
            let (bytes, sha256) = match cached {
                Some((_, bytes, hash)) => (*bytes, hash.clone()),
                None => {
                    let mut stream = crate::paths::open_regular_read(backing)?;
                    ensure!(
                        selection_metadata_stamp(&stream.metadata()?)? == before,
                        "Trace file replaced before hashing"
                    );
                    let mut hash = Sha256::new();
                    let mut bytes = 0u64;
                    let mut buffer = [0u8; 128 * 1024];
                    loop {
                        self.check_cancelled()?;
                        let count = stream.read(&mut buffer)?;
                        if count == 0 {
                            break;
                        }
                        hash.update(&buffer[..count]);
                        bytes += count as u64;
                    }
                    ensure!(
                        selection_metadata_stamp(&stream.metadata()?)? == before,
                        "Trace file changed while hashing"
                    );
                    ensure!(
                        stream.metadata()?.len() == bytes,
                        "Trace file changed while hashing"
                    );
                    (bytes, format!("{:x}", hash.finalize()))
                }
            };
            ensure!(
                selection_file_stamp(backing)? == before,
                "Trace input changed while hashing"
            );
            file_stamps.push((backing.to_owned(), before.clone()));
            trace
                .cache
                .insert(backing.to_owned(), (before, bytes, sha256.clone()));
            let previous = entries.insert(
                self.trace_path(&logical)?,
                Entry {
                    directory: false,
                    bytes: Some(bytes),
                    sha256: Some(sha256),
                },
            );
            ensure!(
                previous.is_none(),
                "Trace has a file/directory path collision"
            );
        }
        for (path, before) in directory_stamps.iter().chain(file_stamps.iter()) {
            ensure!(
                selection_file_stamp(path)? == *before,
                "Filesystem changed during trace scan"
            );
        }
        // Remove deleted/replaced backing files so cache size follows live input.
        let retained: BTreeSet<_> = file_stamps.iter().map(|(path, _)| path).collect();
        trace.cache.retain(|path, _| retained.contains(path));
        let manifest = Manifest {
            version: 1,
            identity,
            entries,
        };
        let name = if final_state {
            "final.json".into()
        } else {
            format!("step_{:06}.json", trace.next)
        };
        let mut temporary = tempfile::NamedTempFile::new_in(&trace.directory)?;
        serde_json::to_writer(&mut temporary, &manifest)?;
        temporary.write_all(b"\n")?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(trace.directory.join(name))?;
        trace.next += 1;
        Ok(())
    }

    fn trace_path(&self, path: &Path) -> Result<String> {
        for (label, root) in [
            ("game", &self.settings.roots.game),
            ("mods", &self.settings.roots.mods),
        ] {
            if let Ok(relative) = path.strip_prefix(root) {
                let relative = crate::paths::portable_text(relative)
                    .context("Trace requires UTF-8 portable paths")?;
                return Ok(if relative.is_empty() {
                    label.into()
                } else {
                    format!("{label}/{relative}")
                });
            }
        }
        bail!("Trace entry is outside install roots: {}", path.display())
    }
}
