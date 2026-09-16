use crate::PatcherEngine;
use anyhow::{Context, Result};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub fn resolve(engine: PatcherEngine, configured: Option<&Path>) -> Result<Option<PathBuf>> {
    let base = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_owned));
    resolve_in(
        engine,
        configured,
        base.as_deref(),
        env::var_os("PATH").as_deref(),
    )
}

fn resolve_in(
    engine: PatcherEngine,
    configured: Option<&Path>,
    base: Option<&Path>,
    path: Option<&std::ffi::OsStr>,
) -> Result<Option<PathBuf>> {
    if let Some(path) = configured {
        return Ok(Some(
            fs::canonicalize(path).context("Patcher path is unavailable")?,
        ));
    }
    let (names, use_path) = match engine {
        PatcherEngine::KPatcher => (
            &["KPatcher", "KPatcher.exe", "kpatcher", "kpatcher.exe"][..],
            true,
        ),
        PatcherEngine::OdyPatcher => (
            &[
                "odypatcher",
                "odypatcher.exe",
                "OdyPatcher",
                "OdyPatcher.exe",
            ][..],
            true,
        ),
        PatcherEngine::HoloPatcher => {
            #[cfg(windows)]
            {
                (&["holopatcher.exe"][..], false)
            }
            #[cfg(not(windows))]
            {
                (
                    &["HoloPatcher.app/Contents/MacOS/holopatcher", "holopatcher"][..],
                    false,
                )
            }
        }
    };
    if use_path && let Some(path) = path {
        for directory in env::split_paths(path) {
            for name in names {
                if let Some(found) = file(&directory.join(name)) {
                    return Ok(Some(found));
                }
            }
        }
    }
    if let Some(base) = base {
        for name in names {
            if engine != PatcherEngine::HoloPatcher
                && let Some(found) = file(&base.join(name))
            {
                return Ok(Some(found));
            }
            if let Some(found) = file(&base.join("Resources").join(name)) {
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
}
fn file(path: &Path) -> Option<PathBuf> {
    path.is_file().then(|| path.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configured_path_then_path_then_resources_follow_csharp_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bin");
        let base = dir.path().join("app");
        fs::create_dir_all(path.join("unused")).unwrap();
        fs::create_dir_all(base.join("Resources")).unwrap();
        fs::write(path.join("kpatcher"), "path").unwrap();
        fs::write(base.join("Resources/holopatcher"), "holo").unwrap();
        assert_eq!(
            resolve_in(
                PatcherEngine::KPatcher,
                None,
                Some(&base),
                Some(path.as_os_str())
            )
            .unwrap(),
            Some(path.join("kpatcher"))
        );
        assert_eq!(
            resolve_in(
                PatcherEngine::HoloPatcher,
                None,
                Some(&base),
                Some(path.as_os_str())
            )
            .unwrap(),
            Some(base.join("Resources/holopatcher"))
        );
        let configured = dir.path().join("custom");
        fs::write(&configured, "configured").unwrap();
        assert_eq!(
            resolve_in(
                PatcherEngine::KPatcher,
                Some(&configured),
                Some(&base),
                Some(path.as_os_str())
            )
            .unwrap(),
            Some(fs::canonicalize(configured).unwrap())
        );
    }
}
