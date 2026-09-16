use anyhow::{Result, bail, ensure};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct Roots {
    pub mods: PathBuf,
    pub game: PathBuf,
}

/// Windows names are rejected on every host so a guide cannot change meaning across platforms.
pub fn relative(raw: &str, wildcards: bool) -> Result<PathBuf> {
    let text = raw.replace('\\', "/");
    ensure!(!text.starts_with('/'), "Absolute path is forbidden: {raw}");
    let mut path = PathBuf::new();
    for part in text.split('/').filter(|p| !p.is_empty() && *p != ".") {
        ensure!(
            part != ".."
                && !part.contains([':', '\0', '<', '>', '|', '"'])
                && !part.ends_with([' ', '.']),
            "Unsafe path: {raw}"
        );
        ensure!(
            wildcards || !part.contains(['*', '?', '[', ']']),
            "Wildcards are not allowed here: {raw}"
        );
        let stem = part.split('.').next().unwrap().to_ascii_uppercase();
        ensure!(
            !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                && !(stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && matches!(stem.as_bytes()[3], b'1'..=b'9')),
            "Reserved filename: {raw}"
        );
        path.push(part);
    }
    Ok(path)
}

impl Roots {
    pub fn new(mods: &Path, game: &Path) -> Result<Self> {
        let mods = mods.canonicalize()?;
        let game = game.canonicalize()?;
        ensure!(
            mods.is_dir() && game.is_dir(),
            "Choose existing mod and game directories"
        );
        ensure!(
            !mods.starts_with(&game) && !game.starts_with(&mods),
            "Game and mod directories must not overlap"
        );
        ensure!(
            mods.parent().is_some() && game.parent().is_some(),
            "Do not use a filesystem root"
        );
        Ok(Self { mods, game })
    }

    pub fn resolve(&self, raw: &str, wildcards: bool) -> Result<PathBuf> {
        for (prefix, root) in [
            ("<<modDirectory>>", &self.mods),
            ("<<kotorDirectory>>", &self.game),
            ("<<gameDirectory>>", &self.game),
        ] {
            if let Some(tail) = raw.strip_prefix(prefix) {
                ensure!(
                    tail.is_empty() || tail.starts_with(['/', '\\']),
                    "Missing separator after {prefix}"
                );
                let p = root.join(relative(tail.trim_start_matches(['/', '\\']), wildcards)?);
                let mut literal = root.clone();
                for part in p.strip_prefix(root)?.components() {
                    if wildcards
                        && part
                            .as_os_str()
                            .to_string_lossy()
                            .contains(['*', '?', '[', ']'])
                    {
                        break;
                    }
                    literal.push(part);
                }
                self.check(&literal)?;
                return Ok(p);
            }
        }
        bail!("Path must start with <<modDirectory>> or <<kotorDirectory>>: {raw}")
    }

    pub fn check(&self, path: &Path) -> Result<()> {
        let root = [&self.mods, &self.game]
            .into_iter()
            .find(|r| path.starts_with(r))
            .ok_or_else(|| anyhow::anyhow!("Path escapes install roots: {}", path.display()))?;
        ensure!(
            !fs::symlink_metadata(root)?.file_type().is_symlink(),
            "Install root became a symlink"
        );
        let mut current = root.clone();
        for part in path.strip_prefix(root)?.components() {
            ensure!(
                matches!(part, std::path::Component::Normal(_)),
                "Unsafe path: {}",
                path.display()
            );
            current.push(part);
            match fs::symlink_metadata(&current) {
                Ok(meta) => ensure!(
                    !meta.file_type().is_symlink(),
                    "Symbolic links are not allowed: {}",
                    current.display()
                ),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}
