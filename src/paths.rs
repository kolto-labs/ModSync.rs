use anyhow::{Result, bail, ensure};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Matching policy for the engine's virtual filesystem. Exact mode is currently
/// confined to Unix unmanaged execution. Case-insensitive keys use the reference
/// `StringComparer.OrdinalIgnoreCase` identity shared with managed deployment
/// and conflict analysis and checkpoint tree alias checks.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PathPolicy {
    pub(crate) case_insensitive: bool,
}
impl PathPolicy {
    pub(crate) fn new(case_insensitive: bool) -> Result<Self> {
        ensure!(
            case_insensitive || cfg!(unix),
            "Exact path matching is currently supported only on Unix unmanaged installs"
        );
        Ok(Self { case_insensitive })
    }
    pub(crate) fn key(self, path: &Path) -> String {
        if self.case_insensitive {
            crate::unicode_case::ordinal_key(&path.to_string_lossy().replace('\\', "/"))
        } else {
            path.to_string_lossy().into_owned()
        }
    }
    /// A case-insensitive Unix volume can resolve a differently spelled authored
    /// name. Refuse that alias in exact mode before publishing through the OS.
    pub(crate) fn check_spelling(
        self,
        root: &Path,
        path: &Path,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<()> {
        if self.case_insensitive {
            return Ok(());
        }
        let mut current = root.to_path_buf();
        for part in path.strip_prefix(root)?.components() {
            ensure!(
                !cancelled.load(std::sync::atomic::Ordering::Acquire),
                "Cancelled while checking exact path spelling"
            );
            let parent = current.clone();
            current.push(part.as_os_str());
            match fs::symlink_metadata(&current) {
                Ok(_) => {
                    let mut exact = false;
                    for (index, entry) in fs::read_dir(&parent)?.enumerate() {
                        ensure!(
                            index < 100_000,
                            "Exact path directory exceeds inventory bound"
                        );
                        ensure!(
                            !cancelled.load(std::sync::atomic::Ordering::Acquire),
                            "Cancelled while checking exact path spelling"
                        );
                        if entry?.file_name() == part.as_os_str() {
                            exact = true;
                            break;
                        }
                    }
                    ensure!(
                        exact,
                        "The filesystem aliases a differently spelled path; exact matching refuses {}",
                        current.display()
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    pub(crate) fn glob_matches(self, pattern: &glob::Pattern, candidate: &str) -> bool {
        pattern.matches_with(
            candidate,
            glob::MatchOptions {
                case_sensitive: !self.case_insensitive,
                require_literal_separator: true,
                require_literal_leading_dot: false,
            },
        )
    }
}

#[derive(Clone, Debug)]
pub struct Roots {
    pub mods: PathBuf,
    pub game: PathBuf,
}

/// Windows names are rejected on every host so a guide cannot change meaning across platforms.
pub fn relative(raw: &str, wildcards: bool) -> Result<PathBuf> {
    let refused: &[char] = if wildcards {
        &[]
    } else {
        &['*', '?', '[', ']']
    };
    checked_relative(raw, refused, is_reserved_name)
}

/// A relative path whose names are taken literally, never as a pattern: an archive
/// member or a checkpoint tree entry. C# extraction
/// (`TryGetZipSafeArchiveEntryExtractPath`) writes `[` and `]` as ordinary
/// characters, so they are allowed here; `*` and `?` stay refused because no
/// Windows file can carry them.
pub fn literal_relative(raw: &str) -> Result<PathBuf> {
    checked_relative(raw, &['*', '?'], is_reserved_name)
}

/// Reads a path from state Rust wrote earlier (checkpoint stores and history,
/// maintenance journals and fences) with the device-name rule those writers used
/// (`legacy_reserved_name`), so state recorded before the wider rule stays readable.
/// `literal` is `literal_relative`'s character rule, else `relative`'s without wildcards.
pub(crate) fn stored_relative(raw: &str, literal: bool) -> Result<PathBuf> {
    let refused: &[char] = if literal {
        &['*', '?']
    } else {
        &['*', '?', '[', ']']
    };
    checked_relative(raw, refused, legacy_reserved_name)
}

fn checked_relative(raw: &str, refused: &[char], reserved: fn(&str) -> bool) -> Result<PathBuf> {
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
            !part.contains(refused),
            "Wildcards are not allowed here: {raw}"
        );
        ensure!(!reserved(part), "Reserved filename: {raw}");
        path.push(part);
    }
    Ok(path)
}

/// A Windows device name, with or without an extension: `CON`, `PRN`, `AUX`, `NUL`,
/// `CONIN$`, `CONOUT$`, and `COM`/`LPT` followed by one digit `0`-`9` or one of the
/// superscripts `¹`, `²`, `³`, which Windows also treats as device numbers. The stem ends
/// at the first `.`, and trailing spaces before it are ignored, as Windows does.
/// C# `PathValidator` refuses the same `COM0`/`LPT0`; a longer stem such as `COM10` or
/// `com1x` is an ordinary name.
pub(crate) fn is_reserved_name(part: &str) -> bool {
    let stem = part
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    let numbered = |prefix| {
        let mut number = stem.strip_prefix(prefix).unwrap_or_default().chars();
        matches!(
            (number.next(), number.next()),
            (Some('0'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}'), None)
        )
    };
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || numbered("COM")
        || numbered("LPT")
}

/// The device-name rule before 2026-10-04: `CON`, `PRN`, `AUX`, `NUL` and
/// `COM1`-`COM9`/`LPT1`-`LPT9`, the stem ending at the first `.`. Only for reading
/// stored state (`stored_relative`).
fn legacy_reserved_name(part: &str) -> bool {
    let stem = part
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

/// True for a `<<modDirectory>>` part that names a Peer download staging folder
/// (`.peer-<nonce>.part`, `download_orchestrator::is_peer_staging`), matched without ASCII
/// case so a case-insensitive volume cannot reach one either. Scans skip these folders,
/// so an instruction path into one would write where validation cannot see.
fn is_peer_staging_part(part: &std::ffi::OsStr) -> bool {
    part.to_str().is_some_and(|part| {
        crate::download_orchestrator::is_peer_staging(std::ffi::OsStr::new(
            &part.to_ascii_lowercase(),
        ))
    })
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
                // A bare root (`<<kotorDirectory>>`, `<<kotorDirectory>>/`) is the root
                // itself, as C# FixPathFormatting trims the trailing separator.
                // `root.join("")` would add one and miss the root's directory key.
                let tail = relative(tail.trim_start_matches(['/', '\\']), wildcards)?;
                ensure!(
                    prefix != "<<modDirectory>>" || !tail.iter().any(is_peer_staging_part),
                    "Path is inside a Peer download staging folder: {raw}"
                );
                let p = if tail.as_os_str().is_empty() {
                    root.clone()
                } else {
                    root.join(tail)
                };
                let mut literal = root.clone();
                for part in p.strip_prefix(root)?.components() {
                    if wildcards && part.as_os_str().to_string_lossy().contains(['*', '?']) {
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

    /// Whether `path` is one of the install roots itself. A bare root Source
    /// (`<<modDirectory>>`) names no file and has no descendants, as in C#.
    pub fn is_root(&self, path: &Path) -> bool {
        path == self.mods || path == self.game
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

/// Open a regular archive without following a replaced final symlink or blocking
/// on a FIFO. Parent-directory ownership remains the caller's responsibility.
pub(crate) fn open_regular_read(path: &Path) -> Result<fs::File> {
    ensure!(
        std::fs::symlink_metadata(path)?.file_type().is_file(),
        "Archive source must be a regular file, not a symlink or special file"
    );
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        ensure!(
            file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
            "Archive source is a reparse point"
        );
    }
    ensure!(
        file.metadata()?.is_file(),
        "Archive source is not a regular file"
    );
    Ok(file)
}
