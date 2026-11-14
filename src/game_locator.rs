//! Find KOTOR installs on this computer so players do not have to browse for
//! them. Read-only: folders are listed and inspected with [`crate::game::detect`],
//! nothing is created or changed.
//!
//! Covers Steam (the main library plus every extra library in
//! `libraryfolders.vdf`, including the Flatpak and Snap copies on Linux) and, on
//! Windows, the GOG and Origin/EA folders: the default ones, the install folders
//! GOG records in the registry, and every game folder directly under `GOG Games`
//! or GOG Galaxy's `Games` folder, whatever it is named.
use crate::game::{self, Detected};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

/// Which game a found folder holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Game {
    #[serde(rename = "KOTOR1")]
    Kotor1,
    #[serde(rename = "KOTOR2")]
    Kotor2,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundInstall {
    pub game: Game,
    /// The folder as found (not canonicalized), suitable for a game directory.
    pub path: PathBuf,
    /// Where it was found: `steam`, `gog` or `origin`.
    pub source: &'static str,
    /// The KOTOR 2 Aspyr update, which some guides treat differently.
    pub aspyr: bool,
}

/// Folder names under `steamapps/common`, compared case-insensitively.
const STEAM_FOLDERS: &[&str] = &[
    "swkotor",
    "star wars - knights of the old republic",
    "knights of the old republic ii",
    "star wars knights of the old republic ii",
];

/// Default GOG and Origin/EA install folders under a Windows drive root.
const STORE_FOLDERS: &[(&str, &str)] = &[
    ("GOG Games/Star Wars - KotOR", "gog"),
    ("GOG Games/Star Wars - KotOR2", "gog"),
    ("GOG Games/Star Wars - Knights of the Old Republic", "gog"),
    (
        "GOG Games/Star Wars - Knights of the Old Republic II - The Sith Lords",
        "gog",
    ),
    (
        "Program Files (x86)/GOG Galaxy/Games/Star Wars - KotOR",
        "gog",
    ),
    (
        "Program Files (x86)/GOG Galaxy/Games/Star Wars - KotOR2",
        "gog",
    ),
    (
        "Program Files (x86)/Origin Games/Star Wars Knights of the Old Republic",
        "origin",
    ),
    (
        "Program Files/EA Games/Star Wars Knights of the Old Republic",
        "origin",
    ),
];

/// Folders under a Windows drive root whose game folders are all checked,
/// whatever they are named, since GOG lets players choose the folder name.
const STORE_PARENTS: &[(&str, &str)] = &[
    ("GOG Games", "gog"),
    ("Program Files (x86)/GOG Galaxy/Games", "gog"),
];

/// GOG product ids of KOTOR 1 and KOTOR 2 under `SOFTWARE\GOG.com\Games`.
const GOG_PRODUCTS: &[&str] = &["1207666283", "1421404581"];

fn steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    {
        if let Some(path) = windows_steam_path() {
            roots.push(path);
        }
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(base) = std::env::var_os(var) {
                roots.push(PathBuf::from(base).join("Steam"));
            }
        }
    }
    if let Some(home) = dirs::home_dir() {
        #[cfg(target_os = "macos")]
        roots.push(home.join("Library/Application Support/Steam"));
        #[cfg(all(unix, not(target_os = "macos")))]
        for relative in [
            ".local/share/Steam",
            ".steam/steam",
            ".steam/root",
            ".var/app/com.valvesoftware.Steam/.local/share/Steam",
            "snap/steam/common/.local/share/Steam",
        ] {
            roots.push(home.join(relative));
        }
        #[cfg(windows)]
        let _ = home;
    }
    roots
}

/// `HKCU\Software\Valve\Steam\SteamPath`, written by the Steam client. Steam
/// stores it lowercased with forward slashes, so it is spelled the way Windows
/// shows the folder.
#[cfg(windows)]
fn windows_steam_path() -> Option<PathBuf> {
    use windows_sys::Win32::System::Registry::HKEY_CURRENT_USER;
    registry_string(HKEY_CURRENT_USER, "Software\\Valve\\Steam", "SteamPath").map(native)
}

/// Install folders GOG records for KOTOR 1 and KOTOR 2, in both registry views.
#[cfg(windows)]
fn gog_registry_paths() -> Vec<PathBuf> {
    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    let mut paths = Vec::new();
    for product in GOG_PRODUCTS {
        for base in [
            "SOFTWARE\\WOW6432Node\\GOG.com\\Games",
            "SOFTWARE\\GOG.com\\Games",
        ] {
            let key = format!("{base}\\{product}");
            if let Some(path) = registry_string(HKEY_LOCAL_MACHINE, &key, "path") {
                paths.push(native(path));
            }
        }
    }
    paths
}

/// A folder written with forward slashes or in another letter case, spelled
/// the way Windows shows it (`C:\Program Files (x86)\Steam`). Left as it is
/// when the folder cannot be read.
#[cfg(windows)]
fn native(path: PathBuf) -> PathBuf {
    let path = PathBuf::from(path.to_string_lossy().replace('/', "\\"));
    let Ok(real) = std::fs::canonicalize(&path) else {
        return path;
    };
    match real.to_string_lossy().strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with(r"UNC\") => PathBuf::from(rest),
        _ => path,
    }
}

/// A `REG_SZ` value as a path, or `None` when the key or value is missing.
#[cfg(windows)]
fn registry_string(
    root: windows_sys::Win32::System::Registry::HKEY,
    key: &str,
    value: &str,
) -> Option<PathBuf> {
    use windows_sys::Win32::System::Registry::{RRF_RT_REG_SZ, RegGetValueW};
    let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let key = wide(key);
    let value = wide(value);
    let mut buffer = vec![0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: the key and value names are NUL-terminated and outlive the call;
    // `buffer` holds `size` bytes, and the call writes at most that many.
    let status = unsafe {
        RegGetValueW(
            root,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != 0 {
        return None;
    }
    let length = (size as usize / 2).saturating_sub(1).min(buffer.len());
    let text = String::from_utf16_lossy(&buffer[..length]);
    let text = text.trim_end_matches('\0').trim();
    (!text.is_empty()).then(|| PathBuf::from(text))
}

/// The Steam root plus the extra libraries named in `libraryfolders.vdf`.
fn steam_libraries(root: &Path) -> Vec<PathBuf> {
    let mut libraries = vec![root.to_path_buf()];
    for relative in ["steamapps/libraryfolders.vdf", "config/libraryfolders.vdf"] {
        let Ok(text) = std::fs::read_to_string(root.join(relative)) else {
            continue;
        };
        libraries.extend(vdf_paths(&text));
    }
    libraries
}

/// Values of `"path"` keys in a VDF file, with VDF's doubled backslashes undone.
fn vdf_paths(text: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for line in text.lines() {
        let mut quoted = line.split('"').skip(1).step_by(2);
        if let (Some(key), Some(value)) = (quoted.next(), quoted.next())
            && key.eq_ignore_ascii_case("path")
            && !value.trim().is_empty()
        {
            paths.push(PathBuf::from(value.replace("\\\\", "\\")));
        }
    }
    paths
}

/// Child directories whose name matches `name` case-insensitively (Linux
/// libraries may spell `steamapps` or `common` differently).
fn child_named(folder: &Path, name: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
        })
        .map(|entry| entry.path())
        .collect()
}

fn classify(path: &Path) -> Option<(Game, bool)> {
    match game::detect(&game::resolve_install_root(path)) {
        Detected::Kotor1 => Some((Game::Kotor1, false)),
        Detected::Kotor2Legacy => Some((Game::Kotor2, false)),
        Detected::Kotor2Aspyr => Some((Game::Kotor2, true)),
        Detected::Unknown => None,
    }
}

/// Every KOTOR install found, without duplicates, in discovery order.
pub fn find_installs() -> Vec<FoundInstall> {
    let mut candidates: Vec<(PathBuf, &'static str)> = Vec::new();
    for root in steam_roots().into_iter().filter(|root| root.is_dir()) {
        for library in steam_libraries(&root) {
            for apps in child_named(&library, "steamapps") {
                for common in child_named(&apps, "common") {
                    for name in STEAM_FOLDERS {
                        for folder in child_named(&common, name) {
                            candidates.push((folder, "steam"));
                        }
                    }
                }
            }
        }
    }
    #[cfg(windows)]
    {
        for path in gog_registry_paths() {
            candidates.push((path, "gog"));
        }
        for drive in b'C'..=b'Z' {
            let root = PathBuf::from(format!("{}:\\", drive as char));
            if !root.is_dir() {
                continue;
            }
            for (relative, source) in STORE_FOLDERS {
                candidates.push((root.join(relative), source));
            }
            for (relative, source) in STORE_PARENTS {
                let Ok(entries) = std::fs::read_dir(root.join(relative)) else {
                    continue;
                };
                for entry in entries.flatten() {
                    if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                        candidates.push((entry.path(), source));
                    }
                }
            }
        }
    }
    #[cfg(not(windows))]
    let _ = (STORE_FOLDERS, STORE_PARENTS, GOG_PRODUCTS);
    let mut seen = BTreeSet::new();
    let mut found = Vec::new();
    for (path, source) in candidates {
        if !path.is_dir() {
            continue;
        }
        let key = std::fs::canonicalize(&path)
            .unwrap_or_else(|_| path.clone())
            .to_string_lossy()
            .to_lowercase();
        if !seen.insert(key) {
            continue;
        }
        if let Some((game, aspyr)) = classify(&path) {
            found.push(FoundInstall {
                game,
                path,
                source,
                aspyr,
            });
        }
    }
    found
}
