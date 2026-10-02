use crate::PatcherEngine;
use anyhow::{Context, Result, ensure};
use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
#[cfg(windows)]
pub(crate) mod checkpoint_job;
mod package;
pub mod standalone;
pub use package::Binding as PackageBinding;

/// Fully resolved argv prefix. Python resources use an installed interpreter;
/// discovery never runs a process, downloads a runtime or installs packages.
#[derive(Clone, Debug)]
pub enum Invocation {
    Packaged {
        binding: Box<PackageBinding>,
    },
    Native {
        program: PathBuf,
    },
    Python {
        interpreter: PathBuf,
        entrypoint: PathBuf,
        module_roots: Vec<PathBuf>,
    },
}

impl Invocation {
    pub fn native_path(&self) -> Option<&Path> {
        match self {
            Self::Native { program } => Some(program),
            Self::Packaged { binding } => Some(binding.program()),
            Self::Python { .. } => None,
        }
    }

    /// Construct an isolated child command. The caller owns patcher arguments,
    /// cwd, containment, cancellation and the mandatory fresh-log postcondition.
    pub fn command(&self) -> Result<Command> {
        match self {
            Self::Packaged { binding } => binding.command(),
            Self::Native { program } => {
                ensure!(
                    executable(program),
                    "Patcher is no longer an executable regular file: {}",
                    program.display()
                );
                Ok(Command::new(program))
            }
            Self::Python {
                interpreter,
                entrypoint,
                module_roots,
            } => {
                ensure!(
                    executable(interpreter),
                    "Python interpreter is missing or non-executable: {}",
                    interpreter.display()
                );
                ensure!(
                    entrypoint.is_file() && module_roots.len() == 3,
                    "Bundled Python patcher resources are incomplete"
                );
                let mut command = Command::new(interpreter);
                command
                    .arg("-I")
                    .arg("-B")
                    .arg("-c")
                    .arg(PYTHON_BOOTSTRAP)
                    .arg(entrypoint)
                    .args(module_roots);
                // -I ignores PYTHON* variables. Removing these as well makes the
                // process contract explicit for embedded/venv interpreters.
                command
                    .env_remove("PYTHONPATH")
                    .env_remove("PYTHONHOME")
                    .env("PYTHON_KEYRING_BACKEND", "keyring.backends.null.Keyring");
                Ok(command)
            }
        }
    }
}

// Fixed code, with every path and patcher argument carried as a separate argv
// item. Imports fail before executing HoloPatcher; there is deliberately no pip.
const PYTHON_BOOTSTRAP: &str = r#"import sys, runpy, importlib
if sys.version_info < (3, 11):
    raise SystemExit('HoloPatcher fallback requires installed Python 3.11 or newer')
entrypoint = sys.argv[1]
roots = sys.argv[2:5]
args = sys.argv[5:]
sys.path[:0] = roots
for required in ('loggerplus', 'ply', 'pykotor', 'utility'):
    try:
        importlib.import_module(required)
    except ImportError as error:
        raise SystemExit('HoloPatcher fallback dependency unavailable: ' + required + ': ' + str(error))
sys.argv = [entrypoint] + args
runpy.run_path(entrypoint, run_name='__main__')
"#;

/// Compatibility native-only resolver. Call resolve_invocation when preparing
/// an execution, since a Python argv prefix cannot be represented by one path.
pub fn resolve(engine: PatcherEngine, configured: Option<&Path>) -> Result<Option<PathBuf>> {
    let base = application_base();
    if configured.is_none()
        && let Some(base) = &base
        && let Some(result) = package::resolve(base, engine)?
    {
        return Ok(result.map(|binding| binding.program().to_owned()));
    }
    resolve_in(
        engine,
        configured,
        base.as_deref(),
        env::var_os("PATH").as_deref(),
    )
}

pub fn resolve_invocation(
    engine: PatcherEngine,
    configured: Option<&Path>,
) -> Result<Option<Invocation>> {
    let base = application_base();
    let path = env::var_os("PATH");
    if configured.is_none()
        && let Some(base) = &base
        && let Some(result) = package::resolve(base, engine)?
    {
        return Ok(result.map(|binding| Invocation::Packaged {
            binding: Box::new(binding),
        }));
    }
    if let Some(program) = resolve_in(engine, configured, base.as_deref(), path.as_deref())? {
        return Ok(Some(Invocation::Native { program }));
    }
    if engine != PatcherEngine::HoloPatcher {
        return Ok(None);
    }
    let Some(base) = base else {
        return Ok(None);
    };
    for resources in resource_directories(&base) {
        if let Some(invocation) = python_fallback(&resources, path.as_deref())? {
            return Ok(Some(invocation));
        }
    }
    Ok(None)
}

fn application_base() -> Option<PathBuf> {
    env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_owned))
}

fn resource_directories(base: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    // UtilityHelper.GetResourcesDirectory's app-bundle convention first.
    if base
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name.eq_ignore_ascii_case("MacOS"))
        && let Some(contents) = base.parent()
        && contents
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.eq_ignore_ascii_case("Contents"))
        && let Some(bundle) = contents.parent()
    {
        roots.push(bundle.join("Resources"));
        // Also support native macOS bundle resource placement.
        roots.push(contents.join("Resources"));
    }
    let ordinary = base.join("Resources");
    if !roots.contains(&ordinary) {
        roots.push(ordinary);
    }
    roots
}

fn resolve_in(
    engine: PatcherEngine,
    configured: Option<&Path>,
    base: Option<&Path>,
    path: Option<&OsStr>,
) -> Result<Option<PathBuf>> {
    if let Some(path) = configured {
        let path = fs::canonicalize(path).context("Configured patcher path is unavailable")?;
        ensure!(
            executable(&path),
            "Configured patcher is not an executable regular file: {}",
            path.display()
        );
        return Ok(Some(path));
    }
    let (names, use_path): (&[&str], bool) = match engine {
        PatcherEngine::KPatcher => (
            &["KPatcher", "KPatcher.exe", "kpatcher", "kpatcher.exe"],
            true,
        ),
        PatcherEngine::OdyPatcher => (
            &[
                "odypatcher",
                "odypatcher.exe",
                "OdyPatcher",
                "OdyPatcher.exe",
            ],
            true,
        ),
        PatcherEngine::HoloPatcher => {
            #[cfg(windows)]
            {
                (&["holopatcher.exe"], false)
            }
            #[cfg(not(windows))]
            {
                (
                    &["HoloPatcher.app/Contents/MacOS/holopatcher", "holopatcher"],
                    false,
                )
            }
        }
    };
    if use_path && let Some(path) = path {
        for directory in
            env::split_paths(path).filter(|directory| !directory.as_os_str().is_empty())
        {
            for name in names {
                if let Some(found) = file(&directory.join(name)) {
                    return Ok(Some(found));
                }
            }
        }
    }
    if let Some(base) = base {
        let resources = resource_directories(base);
        if engine == PatcherEngine::HoloPatcher {
            // Each reference resource directory searches app executable first,
            // then the plain binary, before considering any Python resources.
            for root in &resources {
                for name in names {
                    if let Some(found) = file(&root.join(name)) {
                        return Ok(Some(found));
                    }
                }
            }
        } else {
            for name in names {
                if let Some(found) = file(&base.join(name)) {
                    return Ok(Some(found));
                }
                for root in &resources {
                    if let Some(found) = file(&root.join(name)) {
                        return Ok(Some(found));
                    }
                }
            }
        }
    }
    Ok(None)
}

fn python_fallback(resources: &Path, path: Option<&OsStr>) -> Result<Option<Invocation>> {
    let distribution = resources.join("PyKotor");
    if !distribution.exists() {
        return Ok(None);
    }
    let roots = [
        distribution.join("Tools/HoloPatcher/src"),
        distribution.join("Libraries/PyKotor/src"),
        distribution.join("Libraries/Utility/src"),
    ];
    for (root, package) in roots.iter().zip(["holopatcher", "pykotor", "utility"]) {
        ensure!(
            root.join(package).join("__init__.py").is_file(),
            "Incomplete bundled HoloPatcher Python resources: missing {}",
            root.join(package).join("__init__.py").display()
        );
    }
    let entrypoint = roots[0].join("holopatcher/__main__.py");
    ensure!(
        entrypoint.is_file(),
        "Bundled HoloPatcher entry point is missing: {}",
        entrypoint.display()
    );
    #[cfg(windows)]
    let names = ["python.exe"];
    #[cfg(not(windows))]
    let names = ["python3", "python"];
    let runtime = resources.join("Python");
    let mut interpreter = None;
    for directory in [runtime.join("bin"), runtime.clone()] {
        for name in names {
            if let Some(found) = interpreter_file(&directory.join(name)) {
                interpreter = Some(found);
                break;
            }
        }
        if interpreter.is_some() {
            break;
        }
    }
    if interpreter.is_none()
        && let Some(path) = path
    {
        for directory in
            env::split_paths(path).filter(|directory| !directory.as_os_str().is_empty())
        {
            for name in names {
                if let Some(found) = interpreter_file(&directory.join(name)) {
                    interpreter = Some(found);
                    break;
                }
            }
            if interpreter.is_some() {
                break;
            }
        }
    }
    let interpreter = interpreter.context("Bundled HoloPatcher Python source exists, but no executable installed Python interpreter was found in Resources/Python or PATH; package Python 3.11+ with loggerplus and ply")?;
    Ok(Some(Invocation::Python {
        interpreter,
        entrypoint: fs::canonicalize(entrypoint)?,
        module_roots: roots
            .into_iter()
            .map(fs::canonicalize)
            .collect::<std::io::Result<Vec<_>>>()?,
    }))
}

// Preserve a virtual environment's interpreter symlink: resolving it to the
// system binary loses the adjacent pyvenv.cfg and installed dependencies.
fn interpreter_file(path: &Path) -> Option<PathBuf> {
    executable(path)
        .then(|| std::path::absolute(path).ok())
        .flatten()
}

fn executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return false;
        }
    }
    true
}

fn file(path: &Path) -> Option<PathBuf> {
    executable(path)
        .then(|| fs::canonicalize(path).ok())
        .flatten()
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
