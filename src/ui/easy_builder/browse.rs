//! Pure browser state over a predicted tree: no egui, no disk access.
use modsync::{editor::easy::Action, engine::PredictedEntry};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Root {
    #[default]
    Game,
    Mods,
}
impl Root {
    pub fn token(self) -> &'static str {
        match self {
            Self::Game => "<<kotorDirectory>>",
            Self::Mods => "<<modDirectory>>",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Game => "Game folder",
            Self::Mods => "Mod folder",
        }
    }
}

/// A folder in one of the two roots, as path components below that root.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub root: Root,
    pub dir: Vec<String>,
}
impl Location {
    pub fn path(&self, roots: &Roots) -> PathBuf {
        let mut path = roots.base(self.root).to_owned();
        path.extend(&self.dir);
        path
    }
    pub fn token(&self) -> String {
        self.dir.iter().fold(self.root.token().to_owned(), |text, part| format!("{text}\\{part}"))
    }
    pub fn parent(&self) -> Option<Self> {
        let mut dir = self.dir.clone();
        dir.pop().map(|_| Self { root: self.root, dir })
    }
    pub fn child(&self, name: &str) -> Self {
        let mut dir = self.dir.clone();
        dir.push(name.into());
        Self { root: self.root, dir }
    }
    pub fn label(&self) -> String {
        self.dir.last().cloned().unwrap_or_else(|| self.root.label().into())
    }
}

#[derive(Clone, Debug, Default)]
pub struct Roots {
    pub game: PathBuf,
    pub mods: PathBuf,
}
impl Roots {
    pub fn base(&self, root: Root) -> &Path {
        match root {
            Root::Game => &self.game,
            Root::Mods => &self.mods,
        }
    }
    /// The deeper root wins if one root is nested in the other.
    pub fn locate(&self, path: &Path) -> Option<Location> {
        [Root::Game, Root::Mods]
            .into_iter()
            .filter_map(|root| {
                let rest = path.strip_prefix(self.base(root)).ok()?;
                let dir = rest.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
                Some((self.base(root).components().count(), Location { root, dir }))
            })
            .max_by_key(|(depth, _)| *depth)
            .map(|(_, location)| location)
    }
    pub fn token(&self, path: &Path) -> Option<String> {
        self.locate(path).map(|location| location.token())
    }
    pub fn tokens(&self, paths: &BTreeSet<PathBuf>) -> Option<Vec<String>> {
        paths.iter().map(|path| self.token(path)).collect()
    }
}

#[derive(Clone, Debug)]
pub struct Row {
    pub name: String,
    pub path: PathBuf,
    pub directory: bool,
    pub known: bool,
    pub bytes: Option<u64>,
}
impl Row {
    pub fn extension(&self) -> String {
        Path::new(&self.name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
    }
    pub fn kind(&self) -> String {
        match (self.directory, self.extension().as_str()) {
            (true, _) => "File folder".into(),
            (_, "") => "File".into(),
            (_, extension) => format!("{} file", extension.to_uppercase()),
        }
    }
    pub fn is_archive(&self) -> bool {
        !self.directory && matches!(self.extension().as_str(), "zip" | "7z" | "rar" | "tar" | "gz" | "exe")
    }
}

/// Parent folder -> children, built once per prediction.
#[derive(Default)]
pub struct Index {
    dirs: BTreeMap<PathBuf, BTreeMap<String, Row>>,
    all: Vec<Row>,
}
impl Index {
    pub fn build(entries: &[PredictedEntry]) -> Self {
        let mut index = Self::default();
        for entry in entries {
            let leaf = Row {
                name: entry.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                path: entry.path.clone(),
                directory: entry.directory,
                known: entry.content_known,
                bytes: entry.bytes,
            };
            index.all.push(leaf);
            let mut child = entry.path.as_path();
            let mut is_leaf = true;
            while let (Some(parent), Some(name)) = (child.parent(), child.file_name()) {
                let name = name.to_string_lossy().into_owned();
                let map = index.dirs.entry(parent.to_owned()).or_default();
                let row = map.entry(name.clone()).or_insert_with(|| Row {
                    name, path: child.to_owned(), directory: true, known: true, bytes: None,
                });
                if is_leaf {
                    row.directory = entry.directory;
                    if !entry.directory {
                        row.known = entry.content_known;
                        row.bytes = entry.bytes;
                    }
                }
                child = parent;
                is_leaf = false;
            }
        }
        index
    }
    /// Direct children of a folder.
    pub fn list(&self, base: &Path) -> Vec<Row> {
        self.dirs.get(base).map(|m| m.values().cloned().collect()).unwrap_or_default()
    }
    pub fn has_subfolders(&self, base: &Path) -> bool {
        self.dirs.get(base).is_some_and(|m| m.values().any(|r| r.directory))
    }
    pub fn contains(&self, path: &Path) -> bool {
        path.parent().and_then(|p| self.dirs.get(p)).zip(path.file_name())
            .is_some_and(|(m, n)| m.contains_key(&*n.to_string_lossy()))
    }
    /// Everything below a folder whose relative path contains the text.
    pub fn search(&self, base: &Path, text: &str) -> Vec<Row> {
        let needle = text.to_lowercase();
        let mut found: BTreeMap<&Path, Row> = BTreeMap::new();
        for row in self.dirs.values().flat_map(|m| m.values()) {
            let Ok(rest) = row.path.strip_prefix(base) else { continue };
            let name = rest.to_string_lossy().into_owned();
            if rest.as_os_str().is_empty() || !name.to_lowercase().contains(&needle) { continue; }
            found.insert(row.path.as_path(), Row { name, ..row.clone() });
        }
        found.into_values().collect()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Name,
    Size,
    Kind,
}

/// Folders first, then by the chosen key.
pub fn sort_rows(rows: &mut [Row], sort: Sort, descending: bool) {
    let name = |a: &Row, b: &Row| a.name.to_lowercase().cmp(&b.name.to_lowercase());
    rows.sort_by(|a, b| {
        b.directory.cmp(&a.directory).then_with(|| {
            let order = match sort {
                Sort::Name => name(a, b),
                Sort::Size => a.bytes.cmp(&b.bytes).then_with(|| name(a, b)),
                Sort::Kind => a.kind().cmp(&b.kind()).then_with(|| name(a, b)),
            };
            if descending { order.reverse() } else { order }
        })
    });
}

pub fn filter_rows(rows: &mut Vec<Row>, text: &str) {
    let needle = text.trim().to_lowercase();
    if !needle.is_empty() {
        rows.retain(|row| row.name.to_lowercase().contains(&needle));
    }
}

pub fn breadcrumbs(location: &Location) -> Vec<(String, Location)> {
    let mut trail = vec![(location.root.label().to_owned(), Location { root: location.root, dir: vec![] })];
    for i in 0..location.dir.len() {
        trail.push((location.dir[i].clone(), Location { root: location.root, dir: location.dir[..=i].to_vec() }));
    }
    trail
}

#[derive(Default)]
pub struct Selection {
    pub paths: BTreeSet<PathBuf>,
    anchor: Option<usize>,
    cursor: Option<usize>,
}
impl Selection {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn click(&mut self, rows: &[Row], index: usize, ctrl: bool, shift: bool) {
        let Some(row) = rows.get(index) else { return };
        if shift && let Some(anchor) = self.anchor.filter(|a| *a < rows.len()) {
            let (lo, hi) = (anchor.min(index), anchor.max(index));
            self.paths = rows[lo..=hi].iter().map(|r| r.path.clone()).collect();
        } else if ctrl {
            if !self.paths.remove(&row.path) { self.paths.insert(row.path.clone()); }
            self.anchor = Some(index);
        } else {
            self.paths = BTreeSet::from([row.path.clone()]);
            self.anchor = Some(index);
        }
        self.cursor = Some(index);
    }
    /// Arrow-key movement; `extend` keeps the anchor like Shift+Arrow.
    pub fn step(&mut self, rows: &[Row], delta: isize, extend: bool) {
        if rows.is_empty() { return; }
        let at = self.cursor.map_or(if delta >= 0 { -1 } else { rows.len() as isize }, |c| c as isize);
        let next = (at + delta).clamp(0, rows.len() as isize - 1) as usize;
        self.click(rows, next, false, extend);
    }
    pub fn all(&mut self, rows: &[Row]) {
        self.paths = rows.iter().map(|r| r.path.clone()).collect();
    }
    pub fn cursor_row<'a>(&self, rows: &'a [Row]) -> Option<&'a Row> {
        self.cursor.and_then(|c| rows.get(c))
    }
    pub fn selected<'a>(&self, rows: &'a [Row]) -> Vec<&'a Row> {
        rows.iter().filter(|r| self.paths.contains(&r.path)).collect()
    }
}

/// Browser position, history and quick filter.
#[derive(Default)]
pub struct View {
    pub location: Location,
    pub back: Vec<Location>,
    pub forward: Vec<Location>,
    pub selection: Selection,
    pub sort: Sort,
    pub descending: bool,
    pub filter: String,
    pub deep: bool,
}
impl View {
    pub fn go(&mut self, location: Location) {
        if location != self.location {
            self.back.push(std::mem::replace(&mut self.location, location));
            self.forward.clear();
        }
        self.selection.clear();
    }
    pub fn up(&mut self) {
        if let Some(parent) = self.location.parent() { self.go(parent); }
    }
    pub fn back(&mut self) {
        if let Some(previous) = self.back.pop() {
            self.forward.push(std::mem::replace(&mut self.location, previous));
            self.selection.clear();
        }
    }
    pub fn forward(&mut self) {
        if let Some(next) = self.forward.pop() {
            self.back.push(std::mem::replace(&mut self.location, next));
            self.selection.clear();
        }
    }
    pub fn set_sort(&mut self, sort: Sort) {
        self.descending = self.sort == sort && !self.descending;
        self.sort = sort;
    }
    /// Rows for the current folder: filtered, searched and sorted.
    pub fn rows(&self, index: &Index, roots: &Roots) -> Vec<Row> {
        let base = self.location.path(roots);
        let mut rows = if self.deep && !self.filter.trim().is_empty() {
            index.search(&base, self.filter.trim())
        } else {
            let mut rows = index.list(&base);
            filter_rows(&mut rows, &self.filter);
            rows
        };
        sort_rows(&mut rows, self.sort, self.descending);
        rows
    }
}

/// Most recent first, no duplicates, capped.
pub fn remember(recent: &mut Vec<Location>, location: &Location) {
    recent.retain(|l| l != location);
    recent.insert(0, location.clone());
    recent.truncate(12);
}

pub fn paste(cut: bool, from: Vec<String>, to: &Location) -> Action {
    let to = to.token();
    if cut { Action::Move { from, to, overwrite: true } } else { Action::Copy { from, to, overwrite: true } }
}

pub fn rename(from: String, new_name: &str) -> Option<Action> {
    let name = new_name.trim();
    (!name.is_empty() && !name.contains(['/', '\\'])).then(|| Action::Rename { from, to: name.into(), overwrite: false })
}

/// Apply the last action's shape to the current selection.
pub fn repeat(last: &Action, tokens: Vec<String>) -> Option<Action> {
    let first = tokens.first().cloned();
    match last {
        Action::Copy { to, overwrite, .. } if !tokens.is_empty() => Some(Action::Copy { from: tokens, to: to.clone(), overwrite: *overwrite }),
        Action::Move { to, overwrite, .. } if !tokens.is_empty() => Some(Action::Move { from: tokens, to: to.clone(), overwrite: *overwrite }),
        Action::Delete { .. } if !tokens.is_empty() => Some(Action::Delete { path: tokens }),
        Action::Extract { .. } => first.map(|archive| Action::Extract { archive }),
        Action::Patcher { arguments, .. } => first.map(|folder| Action::Patcher { folder, arguments: arguments.clone() }),
        _ => None,
    }
}

/// The folder that holds a patcher: the selected folder, or the parent of its
/// `tslpatchdata`. Namespace options are the folders inside `tslpatchdata`.
pub fn patcher_target(index: &Index, folder: &Path) -> (PathBuf, Vec<String>) {
    let folder = if folder.file_name().is_some_and(|n| n.eq_ignore_ascii_case("tslpatchdata")) {
        folder.parent().unwrap_or(folder)
    } else {
        folder
    };
    let data = index.list(folder).into_iter().find(|r| r.directory && r.name.eq_ignore_ascii_case("tslpatchdata"));
    let options = data.map(|d| index.list(&d.path).into_iter().filter(|r| r.directory).map(|r| r.name).collect()).unwrap_or_default();
    (folder.to_owned(), options)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Touched {
    pub path: PathBuf,
    pub added: bool,
    pub known: bool,
}

/// Files whose predicted state differs between two predictions. `known` is the
/// prediction's `content_known`; unknown content is never a verified result.
pub fn touched(before: &[PredictedEntry], after: &[PredictedEntry]) -> Vec<Touched> {
    let old: BTreeMap<&Path, (bool, Option<u64>)> = before.iter().map(|e| (e.path.as_path(), (e.content_known, e.bytes))).collect();
    after.iter().filter(|e| !e.directory).filter_map(|e| {
        let added = !old.contains_key(e.path.as_path());
        (added || old[e.path.as_path()] != (e.content_known, e.bytes))
            .then(|| Touched { path: e.path.clone(), added, known: e.content_known })
    }).collect()
}

pub fn size_text(bytes: Option<u64>, known: bool) -> String {
    match (bytes, known) {
        (Some(b), _) if b >= 1 << 20 => format!("{:.1} MB", b as f64 / 1048576.0),
        (Some(b), _) if b >= 1 << 10 => format!("{:.1} KB", b as f64 / 1024.0),
        (Some(b), _) => format!("{b} B"),
        (None, false) => "unknown".into(),
        (None, true) => String::new(),
    }
}
