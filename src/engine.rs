use crate::{
    archive,
    model::{Component, Guide, Instruction},
    paths::{Roots, relative},
    recovery,
};
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Debug)]
pub struct Settings {
    pub roots: Roots,
    pub allow_processes: bool,
    pub patcher: Option<PathBuf>,
    pub archive: archive::Options,
    pub cancelled: Arc<AtomicBool>,
}
impl Settings {
    pub fn new(mods: &Path, game: &Path) -> Result<Self> {
        Ok(Self {
            roots: Roots::new(mods, game)?,
            allow_processes: false,
            patcher: None,
            archive: archive::Options::default(),
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub component: String,
    pub step: usize,
    pub message: String,
}

#[derive(Clone)]
enum Content {
    Disk(PathBuf),
    Scratch(Arc<tempfile::NamedTempFile>, Option<u32>),
}
impl Content {
    fn path(&self) -> &Path {
        match self {
            Self::Disk(path) => path,
            Self::Scratch(file, _) => file.path(),
        }
    }
    fn open(&self) -> Result<fs::File> {
        Ok(fs::File::open(self.path())?)
    }
}

fn key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}

pub struct Engine {
    pub settings: Settings,
    files: BTreeMap<String, (PathBuf, Content)>,
    directories: BTreeMap<String, PathBuf>,
    install: bool,
    selected: BTreeSet<String>,
    selected_names: BTreeSet<String>,
}

impl Engine {
    pub fn new(mut settings: Settings) -> Result<Self> {
        let scratch = fs::canonicalize(
            settings
                .archive
                .scratch_dir
                .clone()
                .unwrap_or_else(std::env::temp_dir),
        )?;
        ensure!(scratch.is_dir(), "Archive scratch path must be a directory");
        ensure!(
            !scratch.starts_with(&settings.roots.game)
                && !scratch.starts_with(&settings.roots.mods),
            "Archive scratch directory must be outside game and mod trees"
        );
        settings.archive.scratch_dir = Some(scratch);
        if let Some(path) = &settings.patcher {
            settings.patcher = Some(fs::canonicalize(path).context("Patcher path is unavailable")?);
        }
        let mut engine = Self {
            settings,
            files: BTreeMap::new(),
            directories: BTreeMap::new(),
            install: false,
            selected: BTreeSet::new(),
            selected_names: BTreeSet::new(),
        };
        engine.scan()?;
        Ok(engine)
    }

    fn scan(&mut self) -> Result<()> {
        self.files.clear();
        self.directories.clear();
        for root in [&self.settings.roots.mods, &self.settings.roots.game] {
            for entry in walkdir::WalkDir::new(root) {
                let entry = entry?;
                ensure!(
                    !entry.file_type().is_symlink(),
                    "Symlinks are unsupported in install trees: {}",
                    entry.path().display()
                );
                let path = entry.path().to_path_buf();
                if entry.file_type().is_file() {
                    ensure!(
                        self.files
                            .insert(key(&path), (path.clone(), Content::Disk(path.clone())))
                            .is_none(),
                        "Case-colliding files: {}",
                        path.display()
                    );
                } else {
                    ensure!(
                        entry.file_type().is_dir(),
                        "Unsupported special file: {}",
                        path.display()
                    );
                    ensure!(
                        self.directories.insert(key(&path), path.clone()).is_none(),
                        "Case-colliding directories: {}",
                        path.display()
                    );
                }
            }
        }
        Ok(())
    }

    pub fn validate(mut self, guide: &Guide, mut emit: impl FnMut(Event)) -> Result<()> {
        recovery::ensure_clean(&self.settings.roots)?;
        self.run(guide, &mut emit)
    }

    pub fn install(mut self, guide: &Guide, mut emit: impl FnMut(Event)) -> Result<()> {
        let _lock = recovery::Lock::acquire(&self.settings.roots)?;
        recovery::ensure_clean(&self.settings.roots)?;
        self.scan()?;
        self.run(guide, &mut |_| {})?;
        self.scan()?;
        emit(Event {
            component: String::new(),
            step: 0,
            message: "Creating recovery snapshot of game and mod directories".into(),
        });
        self.check_cancelled()?;
        recovery::begin(&self.settings.roots)?;
        self.install = true;
        self.run(guide, &mut emit).with_context(|| {
            format!(
                "Install stopped. Restore before retrying; snapshot: {}",
                recovery::state_dir(&self.settings.roots).display()
            )
        })?;
        recovery::finish(&self.settings.roots)?;
        Ok(())
    }

    fn check_cancelled(&self) -> Result<()> {
        ensure!(
            !self.settings.cancelled.load(Ordering::Relaxed),
            "Cancelled; no later instructions ran"
        );
        Ok(())
    }

    fn run(&mut self, guide: &Guide, emit: &mut impl FnMut(Event)) -> Result<()> {
        self.selected = guide.selected_ids();
        self.selected_names = guide
            .components
            .iter()
            .filter(|c| c.is_selected)
            .map(|c| c.name.to_lowercase())
            .collect();
        for c in guide.ordered()? {
            ensure!(
                !c.instructions.is_empty(),
                "{} has no executable instructions; review the guide",
                c.name
            );
            self.component(c, emit)?;
        }
        Ok(())
    }

    fn component(&mut self, c: &Component, emit: &mut impl FnMut(Event)) -> Result<()> {
        self.check_cancelled()?;
        ensure!(
            !crate::draft::requires_review(c),
            "{} has unreviewed draft instructions; review them before installation",
            c.name
        );
        ensure!(
            c.metadata
                .get("UnresolvedMasters")
                .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty)),
            "{} has unresolved Markdown masters; review its dependencies",
            c.name
        );
        for dependency in &c.dependencies {
            ensure!(
                self.selected.contains(&dependency.to_ascii_lowercase()),
                "{} requires {dependency}",
                c.name
            );
        }
        for restriction in &c.restrictions {
            ensure!(
                !self.selected.contains(&restriction.to_ascii_lowercase()),
                "{} conflicts with {restriction}",
                c.name
            );
        }
        for (n, instruction) in c.instructions.iter().enumerate() {
            self.check_cancelled()?;
            if !instruction
                .dependencies
                .iter()
                .all(|id| self.selected.contains(&id.to_ascii_lowercase()))
                || instruction
                    .restrictions
                    .iter()
                    .any(|id| self.selected.contains(&id.to_ascii_lowercase()))
            {
                emit(Event {
                    component: c.name.clone(),
                    step: n + 1,
                    message: "Skipped by instruction conditions".into(),
                });
                continue;
            }
            let message = if matches!(instruction.action.as_str(), "Patcher" | "Execute" | "Run")
                && !self.install
            {
                format!(
                    "{}: inputs checked; external effects cannot be simulated",
                    instruction.action
                )
            } else {
                instruction.action.clone()
            };
            self.execute(c, instruction, emit)
                .with_context(|| format!("{} / step {} ({})", c.name, n + 1, instruction.action))?;
            emit(Event {
                component: c.name.clone(),
                step: n + 1,
                message,
            });
        }
        Ok(())
    }

    fn path(&self, raw: &str, wildcard: bool) -> Result<PathBuf> {
        self.settings.roots.resolve(raw, wildcard)
    }

    fn matches(&self, sources: &[String], missing_ok: bool) -> Result<Vec<PathBuf>> {
        ensure!(!sources.is_empty(), "Instruction has no Source");
        let mut found = BTreeMap::new();
        for source in sources {
            let path = self.path(source, true)?;
            let root = [&self.settings.roots.mods, &self.settings.roots.game]
                .into_iter()
                .find(|root| path.starts_with(root))
                .unwrap();
            let pattern = glob::Pattern::new(&format!(
                "{}/{}",
                glob::Pattern::escape(&key(root)),
                key(path.strip_prefix(root)?)
            ))?;
            let prefix = format!("{}/", key(&path));
            let matches: Vec<_> = self
                .files
                .iter()
                .filter(|(k, _)| {
                    pattern.matches_with(
                        k,
                        glob::MatchOptions {
                            case_sensitive: false,
                            require_literal_separator: true,
                            require_literal_leading_dot: false,
                        },
                    ) || k.starts_with(&prefix)
                })
                .collect();
            ensure!(
                missing_ok || !matches.is_empty(),
                "Source matches no files: {source}"
            );
            for (k, (p, _)) in matches {
                self.settings.roots.check(p)?;
                found.insert(k.clone(), p.clone());
            }
        }
        Ok(found.into_values().collect())
    }

    fn actual(&self, path: &Path) -> PathBuf {
        if let Some((p, _)) = self.files.get(&key(path)) {
            return p.clone();
        }
        if let Some(p) = self.directories.get(&key(path)) {
            return p.clone();
        }
        if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
            let actual = self.actual(parent);
            return actual.join(name);
        }
        path.to_path_buf()
    }

    fn put(&mut self, path: &Path, content: Content, overwrite: bool) -> Result<()> {
        let path = self.actual(path);
        self.settings.roots.check(&path)?;
        ensure!(
            !self.directories.contains_key(&key(&path)),
            "Cannot overwrite directory with file: {}",
            path.display()
        );
        if self.files.contains_key(&key(&path)) && !overwrite {
            return Ok(());
        }
        let mut parent = path.parent();
        while let Some(dir) = parent {
            ensure!(
                !self.files.contains_key(&key(dir)),
                "File blocks destination directory: {}",
                dir.display()
            );
            self.directories.insert(key(dir), dir.to_path_buf());
            parent = dir.parent();
        }
        let content = if self.install {
            let permissions = match &content {
                Content::Disk(source) => Some(fs::metadata(source)?.permissions()),
                Content::Scratch(_, mode) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        mode.map(|mode| fs::Permissions::from_mode(mode & 0o777))
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = mode;
                        None
                    }
                }
            };
            recovery::atomic_copy(&path, content.open()?, &self.settings.cancelled)?;
            if let Some(permissions) = permissions {
                fs::set_permissions(&path, permissions)?;
            }
            Content::Disk(path.clone())
        } else {
            content
        };
        self.files.insert(key(&path), (path, content));
        Ok(())
    }

    fn delete(&mut self, path: &Path) -> Result<()> {
        self.settings.roots.check(path)?;
        if self.install {
            fs::remove_file(path)?;
        }
        self.files.remove(&key(path));
        Ok(())
    }

    fn execute(
        &mut self,
        component: &Component,
        i: &Instruction,
        emit: &mut impl FnMut(Event),
    ) -> Result<()> {
        match i.action.as_str() {
            "Choose" => {
                for id in &i.source {
                    ensure!(
                        component
                            .options
                            .iter()
                            .any(|o| o.guid.eq_ignore_ascii_case(id)),
                        "Unknown option {id}"
                    );
                }
                for option in &component.options {
                    if option.is_selected
                        && i.source
                            .iter()
                            .any(|id| option.guid.eq_ignore_ascii_case(id))
                    {
                        self.component(option, emit)?;
                    }
                }
            }
            "Extract" => {
                for source in self.matches(&i.source, false)? {
                    let content = self.files[&key(&source)].1.clone();
                    let default = source.parent().unwrap().join(source.file_stem().unwrap());
                    let dest = if i.destination.is_empty() {
                        default
                    } else {
                        let p = self.path(&i.destination, false)?;
                        if key(&p) == key(source.parent().unwrap()) {
                            default
                        } else {
                            p
                        }
                    };
                    let entries = archive::unpack(
                        content.path(),
                        &source,
                        &self.settings.archive,
                        &self.settings.cancelled,
                    )?;
                    ensure!(
                        !entries
                            .iter()
                            .any(|entry| key(&entry.path).ends_with("fomod/moduleconfig.xml")),
                        "FOMOD choices are unsupported; refusing to install unconfigured archive content"
                    );
                    for entry in entries {
                        self.check_cancelled()?;
                        self.put(
                            &dest.join(entry.path),
                            Content::Scratch(entry.file, entry.mode),
                            i.overwrite,
                        )?;
                    }
                }
            }
            "Copy" | "Move" | "Rename" => {
                let dest = if i.action == "Rename" && !i.destination.starts_with("<<") {
                    let path = relative(&i.destination, false)?;
                    ensure!(
                        path.components().count() == 1,
                        "Rename needs one filename or a placeholder path"
                    );
                    path
                } else {
                    self.path(&i.destination, false)?
                };
                for source in self.matches(&i.source, false)? {
                    self.check_cancelled()?;
                    let target = if i.action == "Rename" {
                        if dest.is_absolute() {
                            dest.clone()
                        } else {
                            source.parent().unwrap().join(&dest)
                        }
                    } else {
                        dest.join(source.file_name().unwrap())
                    };
                    ensure!(
                        key(&source) != key(&target),
                        "Source equals destination: {}",
                        source.display()
                    );
                    let exists = self.files.contains_key(&key(&target));
                    if exists && !i.overwrite {
                        ensure!(
                            i.action != "Rename",
                            "Rename target exists: {}",
                            target.display()
                        );
                        continue;
                    }
                    let data = self.files[&key(&source)].1.clone();
                    self.put(&target, data, i.overwrite)?;
                    if i.action != "Copy" {
                        self.delete(&source)?;
                    }
                }
            }
            "Delete" => {
                for source in self.matches(&i.source, !i.overwrite)? {
                    self.delete(&source)?;
                }
            }
            "DelDuplicate" => {
                let dir = self.path(&i.destination, false)?;
                let extensions = if i.source.is_empty() {
                    vec![".tga".to_string(), ".tpc".to_string()]
                } else {
                    i.source.iter().map(|s| s.to_lowercase()).collect()
                };
                let mut groups: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
                for (p, _) in self
                    .files
                    .values()
                    .filter(|(p, _)| p.parent().is_some_and(|d| key(d) == key(&dir)))
                {
                    let ext = format!(".{}", p.extension().unwrap_or_default().to_string_lossy())
                        .to_lowercase();
                    if extensions.contains(&ext) {
                        groups
                            .entry(p.file_stem().unwrap().to_string_lossy().to_lowercase())
                            .or_default()
                            .push(p.clone());
                    }
                }
                ensure!(
                    !i.arguments.is_empty(),
                    "DelDuplicate Arguments must name the extension to remove"
                );
                for group in groups.values().filter(|g| g.len() > 1) {
                    for p in group {
                        if format!(".{}", p.extension().unwrap_or_default().to_string_lossy())
                            .eq_ignore_ascii_case(&i.arguments)
                        {
                            self.delete(p)?;
                        }
                    }
                }
            }
            "CleanList" => {
                let dir = self.path(&i.destination, false)?;
                let sources = self.matches(&i.source, false)?;
                let source = sources.first().context("CleanList needs a source file")?;
                let data = self.files[&key(source)].1.open()?;
                for line in BufReader::new(data).lines() {
                    for line in line?.split('\r') {
                        self.check_cancelled()?;
                        let Some((name, files)) = line.split_once(',') else {
                            continue;
                        };
                        if cleanlist_selected(name.trim(), &self.selected_names) {
                            for raw in files.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                                let p = self.actual(&dir.join(relative(raw, false)?));
                                if self.files.contains_key(&key(&p)) {
                                    self.delete(&p)?;
                                }
                            }
                        }
                    }
                }
            }
            "Execute" | "Run" | "Patcher" => self.process(i, emit)?,
            other => bail!("Unsupported action {other}; refusing to skip it"),
        }
        Ok(())
    }

    fn process(&mut self, i: &Instruction, emit: &mut impl FnMut(Event)) -> Result<()> {
        ensure!(
            self.settings.allow_processes,
            "{} needs explicit trust: enable external programs (--allow-processes)",
            i.action
        );
        ensure!(
            i.source.len() == 1,
            "External programs need exactly one source"
        );
        let path = self.actual(&self.path(&i.source[0], false)?);
        ensure!(
            self.files.contains_key(&key(&path)) || self.directories.contains_key(&key(&path)),
            "Missing external program/mod: {}",
            path.display()
        );
        let mut staged_patcher = None;
        let mut command = if i.action == "Patcher" {
            let patcher = self
                .settings
                .patcher
                .as_ref()
                .context("Configure the OdyPatcher executable with --patcher")?;
            ensure!(patcher.is_file(), "Patcher executable is missing");
            let dest = if i.destination.is_empty() {
                self.settings.roots.game.clone()
            } else {
                self.actual(&self.path(&i.destination, false)?)
            };
            let base = if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            {
                path.parent().unwrap()
            } else {
                &path
            };
            let mut cmd = Command::new(patcher);
            cmd.arg("--tslpatchdata")
                .arg(base)
                .arg("--game-dir")
                .arg(dest)
                .arg("--install");
            if !i.arguments.trim().is_empty() {
                if let Ok(index) = i.arguments.trim().parse::<usize>() {
                    cmd.arg("--namespace-option-index").arg(index.to_string());
                } else {
                    let name = relative(i.arguments.trim(), false)?;
                    ensure!(
                        name.components().count() == 1
                            && name
                                .extension()
                                .is_some_and(|e| e.eq_ignore_ascii_case("ini")),
                        "Patcher Arguments must be an INI filename or namespace index"
                    );
                    let data = if base
                        .file_name()
                        .is_some_and(|s| s.eq_ignore_ascii_case("tslpatchdata"))
                    {
                        base.to_path_buf()
                    } else {
                        base.join("tslpatchdata")
                    };
                    let ini = self.actual(&data.join(name));
                    ensure!(
                        self.files.contains_key(&key(&ini)),
                        "Missing patcher INI: {}",
                        ini.display()
                    );
                    if self.install {
                        let stage = tempfile::tempdir()?;
                        let target = stage.path().join("tslpatchdata");
                        recovery::copy_tree(&self.actual(&data), &target)?;
                        recovery::atomic_copy(
                            &target.join("changes.ini"),
                            self.files[&key(&ini)].1.open()?,
                            &self.settings.cancelled,
                        )?;
                        for entry in fs::read_dir(&target)? {
                            let entry = entry?;
                            if entry.file_name().eq_ignore_ascii_case("namespaces.ini") {
                                fs::remove_file(entry.path())?;
                            }
                        }
                        cmd = Command::new(patcher);
                        cmd.arg("--tslpatchdata")
                            .arg(stage.path())
                            .arg("--game-dir")
                            .arg(if i.destination.is_empty() {
                                self.settings.roots.game.clone()
                            } else {
                                self.actual(&self.path(&i.destination, false)?)
                            })
                            .arg("--install");
                        staged_patcher = Some(stage);
                    }
                }
            }
            cmd
        } else {
            let mut cmd = Command::new(&path);
            cmd.args(shlex::split(&i.arguments).context("Unclosed quote in Arguments")?);
            cmd
        };
        if self.install {
            let stdout = tempfile::NamedTempFile::new()?;
            let stderr = tempfile::NamedTempFile::new()?;
            let mut child = command
                .current_dir(path.parent().unwrap())
                .stdout(stdout.reopen()?)
                .stderr(stderr.reopen()?)
                .spawn()
                .context("Start external program")?;
            let status = loop {
                if self.settings.cancelled.load(Ordering::Relaxed) {
                    let _ = child.kill();
                    let _ = child.wait();
                    bail!("External program cancelled; restore before retrying");
                }
                if let Some(status) = child.try_wait()? {
                    break status;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            };
            for file in [stdout, stderr] {
                let mut output = String::new();
                file.reopen()?
                    .take(1024 * 1024)
                    .read_to_string(&mut output)?;
                if !output.is_empty() {
                    emit(Event {
                        component: i.action.clone(),
                        step: 0,
                        message: output,
                    });
                }
            }
            ensure!(status.success(), "External program failed with {status}");
            self.scan()?;
        }
        drop(staged_patcher);
        Ok(())
    }
}

// Match ModComponent.ExecuteSingleInstructionAsync's CleanList selection rules.
fn cleanlist_selected(name: &str, selected: &BTreeSet<String>) -> bool {
    let name = name.to_lowercase();
    if name.trim().is_empty() {
        return false;
    }
    if name.starts_with("mandatory deletions") {
        return true;
    }
    fn tokens(name: &str) -> BTreeSet<String> {
        const STOP: &[&str] = &[
            "by",
            "for",
            "the",
            "and",
            "of",
            "mod",
            "pack",
            "k1",
            "k2",
            "hd",
            "kotor",
            "kotorn",
            "version",
            "recolored",
            "reskin",
            "retexture",
            "fix",
            "fixes",
        ];
        name.split(|c: char| !c.is_ascii_lowercase() && !c.is_ascii_digit())
            .filter(|word| !word.is_empty() && !STOP.contains(word))
            .map(|word| {
                if word.len() > 3 {
                    word.strip_suffix('s').unwrap_or(word)
                } else {
                    word
                }
            })
            .map(str::to_owned)
            .collect()
    }
    let target = tokens(&name);
    if target.is_empty() {
        return false;
    }
    selected.iter().any(|candidate| {
        if name.contains(candidate) || candidate.contains(&name) {
            return true;
        }
        let other = tokens(candidate);
        let shared = target.intersection(&other).count();
        shared >= 2 && shared * 5 >= target.len().min(other.len()) * 3
    })
}
