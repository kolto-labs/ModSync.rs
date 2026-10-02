//! Package manifest authority is distinct from explicit external patcher trust.
use super::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::Read;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema_version: u32,
    target: String,
    patcher: Capability,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Capability {
    available: bool,
    engine: Option<String>,
    path: Option<String>,
    sha256: Option<String>,
    source: Option<String>,
    license: Option<String>,
    license_sha256: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Provenance {
    schema_version: u32,
    patchers: Vec<TrustedPatcher>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TrustedPatcher {
    target: String,
    engine: String,
    sha256: String,
    source: String,
    license: String,
    license_sha256: String,
}

#[derive(Clone, Debug)]
pub struct Binding {
    base: PathBuf,
    root: PathBuf,
    program: PathBuf,
    manifest_sha256: String,
    captured: Manifest,
}
impl Binding {
    pub(super) fn program(&self) -> &Path {
        &self.program
    }
    pub(super) fn command(&self) -> Result<Command> {
        let (_, _, current_hash) =
            manifest(&self.base)?.context("Captured package manifest disappeared")?;
        ensure!(
            current_hash == self.manifest_sha256,
            "Package manifest changed after backend resolution"
        );
        ensure!(
            verified(&self.root, &self.captured)? == self.program,
            "Captured package backend path changed"
        );
        Ok(Command::new(&self.program))
    }
}

fn manifest(base: &Path) -> Result<Option<(PathBuf, Manifest, String)>> {
    let (root, path) = if base.file_name().is_some_and(|name| name == "MacOS")
        && base
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == "Contents")
    {
        let contents = base.parent().context("Bundle Contents missing")?;
        (
            contents.parent().context("Bundle root missing")?.to_owned(),
            contents.join("Resources/modsync-package.json"),
        )
    } else {
        (base.to_owned(), base.join("modsync-package.json"))
    };
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => (),
    }
    let mut bytes = Vec::new();
    crate::paths::open_regular_read(&path)?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 4 * 1024 * 1024,
        "Package manifest exceeds 4 MiB"
    );
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    ensure!(
        manifest.schema_version == 1,
        "Unsupported package manifest version"
    );
    let expected_os = match std::env::consts::OS {
        "linux" => "-unknown-linux-",
        "windows" => "-pc-windows-",
        "macos" => "-apple-darwin",
        _ => anyhow::bail!("Unsupported packaged operating system"),
    };
    let expected_arch = if std::env::consts::ARCH == "x86" {
        "i686-".to_owned()
    } else {
        format!("{}-", std::env::consts::ARCH)
    };
    ensure!(
        manifest.target.starts_with(&expected_arch) && manifest.target.contains(expected_os),
        "Package target differs from the running platform"
    );
    Ok(Some((
        root,
        manifest,
        format!("{:x}", Sha256::digest(&bytes)),
    )))
}
fn engine_name(engine: PatcherEngine) -> &'static str {
    match engine {
        PatcherEngine::HoloPatcher => "HoloPatcher",
        PatcherEngine::OdyPatcher => "OdyPatcher",
        PatcherEngine::KPatcher => "KPatcher",
    }
}
fn verified(root: &Path, manifest: &Manifest) -> Result<PathBuf> {
    let capability = &manifest.patcher;
    let engine = capability
        .engine
        .as_deref()
        .context("Packaged patcher engine missing")?;
    let hash = capability
        .sha256
        .as_deref()
        .context("Packaged patcher digest missing")?;
    let source = capability
        .source
        .as_deref()
        .context("Packaged patcher provenance missing")?;
    let license = capability
        .license
        .as_deref()
        .context("Packaged patcher license missing")?;
    let license_hash = capability
        .license_sha256
        .as_deref()
        .context("Packaged patcher license digest missing")?;
    let trust: Provenance =
        serde_json::from_str(include_str!("../../packaging/patcher-provenance.json"))?;
    ensure!(
        trust.schema_version == 1
            && trust.patchers.iter().any(|p| p.target == manifest.target
                && p.engine == engine
                && p.sha256 == hash
                && p.source == source
                && p.license == license
                && p.license_sha256 == license_hash),
        "Package patcher lacks reviewed compiled-in provenance"
    );
    let relative = crate::paths::relative(
        capability
            .path
            .as_deref()
            .context("Packaged patcher path missing")?,
        false,
    )?;
    ensure!(
        !relative.as_os_str().is_empty(),
        "Empty packaged patcher path"
    );
    let root = root.canonicalize()?;
    let path = root.join(relative);
    let mut cursor = root.clone();
    for part in path.strip_prefix(&root)?.components() {
        cursor.push(part);
        ensure!(
            !fs::symlink_metadata(&cursor)?.file_type().is_symlink(),
            "Packaged patcher path contains a symlink"
        );
    }
    let canonical = path.canonicalize()?;
    ensure!(
        canonical.starts_with(&root) && executable(&canonical),
        "Packaged patcher is not a contained executable"
    );
    let mut file = crate::paths::open_regular_read(&canonical)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    ensure!(
        format!("{:x}", digest.finalize()) == hash,
        "Packaged patcher digest differs from reviewed provenance"
    );
    Ok(canonical)
}

/// None: development/unpackaged layout. Some(None): package declares this backend unavailable.
pub(super) fn resolve(base: &Path, engine: PatcherEngine) -> Result<Option<Option<Binding>>> {
    let Some((root, manifest, manifest_sha256)) = manifest(base)? else {
        return Ok(None);
    };
    if !manifest.patcher.available {
        return Ok(Some(None));
    }
    let program = verified(&root, &manifest)?;
    Ok(Some(
        (manifest.patcher.engine.as_deref() == Some(engine_name(engine))).then(|| Binding {
            base: base.to_owned(),
            root,
            program,
            manifest_sha256,
            captured: manifest,
        }),
    ))
}
