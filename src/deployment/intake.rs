//! Bounded regular-file intake shared by deployment readers and writers.
use super::*;
use std::io::Read;
pub(super) const MANIFEST_LIMIT: usize = 16 * 1024 * 1024;
pub(super) const INVENTORY_LIMIT: usize = 10_000;

pub(super) fn bytes(path: &Path, limit: usize) -> Result<Vec<u8>> {
    managed_path(path, false)?;
    let mut bytes = Vec::new();
    crate::paths::open_regular_read(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= limit,
        "Deployment metadata exceeds {limit} bytes: {}",
        path.display()
    );
    Ok(bytes)
}
pub(super) fn optional_text(path: &Path) -> Result<Option<String>> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let text = String::from_utf8(bytes(path, 64 * 1024)?)?;
    ensure!(
        Path::new(text.trim()).is_absolute(),
        "Deployment ownership marker must contain an absolute path"
    );
    Ok(Some(text))
}
pub(super) fn manifest(path: &Path) -> Result<Manifest> {
    let manifest: Manifest = serde_json::from_slice(&bytes(path, MANIFEST_LIMIT)?)?;
    validate(&manifest)?;
    let filename = path
        .file_stem()
        .and_then(|s| s.to_str())
        .context("Manifest filename is not UTF-8")?;
    ensure!(
        component_id(filename)?.eq_ignore_ascii_case(&component_id(&manifest.component_guid)?),
        "Manifest component identity does not match its filename"
    );
    Ok(manifest)
}
pub(super) fn validate(manifest: &Manifest) -> Result<()> {
    component_id(&manifest.component_guid)?;
    ensure!(
        manifest.entries.len() <= 100_000,
        "Deployment manifest exceeds 100000 entries"
    );
    let mut paths = BTreeSet::new();
    for entry in &manifest.entries {
        let path = relative_parts(&entry.relative_path)?;
        let key = crate::unicode_case::ordinal_key(&path.to_string_lossy().replace('\\', "/"));
        ensure!(
            paths.insert(key),
            "Duplicate deployment path: {}",
            entry.relative_path
        );
        ensure!(
            entry.source_hash.len() == 64
                && entry.source_hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid deployment SHA-256"
        );
        if let Some(backup) = &entry.backup_relative_path {
            relative_parts(backup)?;
        }
    }
    Ok(())
}
pub(super) fn marker(root: &Path) -> Result<Option<PathBuf>> {
    let Some(marker) = optional_text(&root.join(GAME_MARKER))? else {
        return Ok(None);
    };
    let marker = marker.trim();
    ensure!(!marker.is_empty(), "Deployment game marker is empty");
    let path = PathBuf::from(marker);
    ensure!(path.is_absolute(), "Deployment game marker is not absolute");
    Ok(Some(fs::canonicalize(&path).unwrap_or(path)))
}
