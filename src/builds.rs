//! Mod build catalog: the community mod builds plus links players add, each an
//! instruction file fetched over HTTPS and kept in a local cache so a build can
//! be opened again without a connection.
//!
//! The catalog lives in `<settings>/builds`: `custom.json` lists added builds,
//! `cache/<key>.md` holds the last fetched copy of each build. Fetching uses the
//! bounded instruction fetcher ([`crate::protocol::fetch_instruction_with_cancel`])
//! and replaces a cached copy only after a complete download.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

const COMMUNITY: &str =
    "https://raw.githubusercontent.com/KOTOR-Community-Portal/mod-builds/master/content";
/// The community portal pages whose Markdown source lives in [`COMMUNITY`].
const PORTAL_PREFIX: &str = "https://kotor.neocities.org/modding/mod_builds/";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Build {
    /// Stable identifier, also the cache file name.
    pub key: String,
    pub label: String,
    /// `KOTOR1` or `KOTOR2`.
    pub game: String,
    /// Where the instruction file is fetched from.
    pub url: String,
    /// Added by the player (can be removed).
    #[serde(default)]
    pub custom: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fetched {
    pub build: Build,
    pub path: PathBuf,
    /// The cached copy was used instead of downloading.
    pub from_cache: bool,
    /// Why a fresh copy could not be downloaded, when the cached one was used instead.
    pub warning: Option<String>,
}

/// The community builds shipped with the catalog.
pub fn community() -> Vec<Build> {
    [
        ("k1_full", "KOTOR 1 Full Build", "KOTOR1", "k1/full.md"),
        (
            "k1_spoiler_free",
            "KOTOR 1 Spoiler-Free Build",
            "KOTOR1",
            "k1/spoiler-free.md",
        ),
        ("k2_full", "KOTOR 2 Full Build", "KOTOR2", "k2/full.md"),
        (
            "k2_spoiler_free",
            "KOTOR 2 Spoiler-Free Build",
            "KOTOR2",
            "k2/spoiler-free.md",
        ),
    ]
    .into_iter()
    .map(|(key, label, game, path)| Build {
        key: key.into(),
        label: label.into(),
        game: game.into(),
        url: format!("{COMMUNITY}/{path}"),
        custom: false,
    })
    .collect()
}

/// The Markdown source for a link. Community portal pages are HTML renderings
/// of Markdown kept on GitHub, so their source is fetched instead.
pub fn source_url(url: &str) -> Result<String> {
    let url = url.trim();
    ensure!(
        url.starts_with("https://"),
        "Build links must start with https://"
    );
    if let Some(rest) = url.strip_prefix(PORTAL_PREFIX) {
        let rest = rest.trim_end_matches('/');
        let rest = rest.split(['?', '#']).next().unwrap_or(rest);
        ensure!(
            !rest.is_empty()
                && rest
                    .split('/')
                    .all(|part| !part.is_empty() && part != "." && part != ".."),
            "This community portal link does not name a build"
        );
        let rest = rest.trim_end_matches(".html");
        return Ok(format!("{COMMUNITY}/{rest}.md"));
    }
    Ok(url.to_owned())
}

fn slug(label: &str) -> String {
    let mut out = String::new();
    for ch in label.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    let out = out.trim_end_matches('_');
    out.chars().take(40).collect()
}

fn normalized_game(game: &str) -> Result<&'static str> {
    match game
        .trim()
        .to_ascii_uppercase()
        .replace([' ', '_'], "")
        .as_str()
    {
        "KOTOR1" | "K1" | "1" => Ok("KOTOR1"),
        "KOTOR2" | "K2" | "2" | "TSL" => Ok("KOTOR2"),
        _ => bail!("The game must be KOTOR1 or KOTOR2"),
    }
}

pub struct Catalog {
    dir: PathBuf,
}

impl Catalog {
    /// The catalog in the ModSync settings directory (or an explicit one).
    pub fn open(settings: Option<&Path>) -> Result<Self> {
        Ok(Self::at(
            crate::profiles::settings_directory(settings)?.join("builds"),
        ))
    }

    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn custom_path(&self) -> PathBuf {
        self.dir.join("custom.json")
    }

    fn custom(&self) -> Result<Vec<Build>> {
        match std::fs::read(self.custom_path()) {
            Ok(bytes) => {
                let mut builds: Vec<Build> =
                    serde_json::from_slice(&bytes).context("The saved build list is damaged")?;
                for build in &mut builds {
                    build.custom = true;
                }
                Ok(builds)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }

    fn save_custom(&self, builds: &[Build]) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        crate::recovery::atomic_write(&self.custom_path(), &serde_json::to_vec_pretty(builds)?)
    }

    /// Community builds first, then added builds in the order they were added.
    pub fn list(&self) -> Result<Vec<Build>> {
        let mut builds = community();
        builds.extend(self.custom()?);
        Ok(builds)
    }

    pub fn find(&self, key: &str) -> Result<Build> {
        self.list()?
            .into_iter()
            .find(|build| build.key == key)
            .with_context(|| format!("There is no build called {key}"))
    }

    /// Add a build link. The link is checked and normalized but not fetched.
    pub fn add(&self, label: &str, game: &str, url: &str) -> Result<Build> {
        let label = label.trim();
        ensure!(!label.is_empty(), "Give the build a name");
        ensure!(label.chars().count() <= 120, "The build name is too long");
        let game = normalized_game(game)?;
        let url = source_url(url)?;
        let mut custom = self.custom()?;
        ensure!(
            !self.list()?.iter().any(|build| build.url == url),
            "This build is already in the list"
        );
        let base = format!("custom_{}", slug(label));
        let mut key = base.clone();
        let mut n = 2;
        while self.list()?.iter().any(|build| build.key == key) {
            key = format!("{base}_{n}");
            n += 1;
        }
        let build = Build {
            key,
            label: label.into(),
            game: game.into(),
            url,
            custom: true,
        };
        custom.push(build.clone());
        self.save_custom(&custom)?;
        Ok(build)
    }

    /// Remove an added build and its cached copy. Community builds stay.
    pub fn remove(&self, key: &str) -> Result<bool> {
        ensure!(
            !community().iter().any(|build| build.key == key),
            "Community builds cannot be removed"
        );
        let mut custom = self.custom()?;
        let before = custom.len();
        custom.retain(|build| build.key != key);
        if custom.len() == before {
            return Ok(false);
        }
        self.save_custom(&custom)?;
        match std::fs::remove_file(self.cached(key)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(true)
    }

    /// Where a build's last fetched copy is kept.
    pub fn cached(&self, key: &str) -> PathBuf {
        self.dir.join("cache").join(format!("{key}.md"))
    }

    /// The instruction file for a build: the cached copy unless `refresh` is
    /// set or there is none, otherwise a fresh download that replaces it.
    pub fn fetch(&self, key: &str, refresh: bool, cancelled: &AtomicBool) -> Result<Fetched> {
        let build = self.find(key)?;
        let cached = self.cached(key);
        if !refresh && cached.is_file() {
            return Ok(Fetched {
                build,
                path: cached,
                from_cache: true,
                warning: None,
            });
        }
        let cache = cached.parent().context("Missing build cache folder")?;
        std::fs::create_dir_all(cache)?;
        let partial = tempfile::Builder::new()
            .prefix(".build-")
            .tempdir_in(cache)?;
        let download = partial.path().join("build.md");
        match crate::protocol::fetch_instruction_with_cancel(&build.url, &download, cancelled) {
            Ok(()) => {
                std::fs::rename(&download, &cached)?;
                Ok(Fetched {
                    build,
                    path: cached,
                    from_cache: false,
                    warning: None,
                })
            }
            // Offline or the site is down: keep using the last good copy.
            Err(error) if cached.is_file() => Ok(Fetched {
                build,
                path: cached,
                from_cache: true,
                warning: Some(format!("{error:#}")),
            }),
            Err(error) => Err(error.context(format!("Could not download {}", build.label))),
        }
    }
}
