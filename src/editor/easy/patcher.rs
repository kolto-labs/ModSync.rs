//! Read-only patcher inspection through the pinned OdyPatcher library.
use crate::{
    editor,
    paths::{self, Roots},
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::path::Path;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchOption {
    pub index: usize,
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchPreview {
    pub source: String,
    pub backend: &'static str,
    pub version: &'static str,
    pub options: Vec<PatchOption>,
    pub selected: Option<usize>,
    /// Patcher Arguments uses one whole filename, not shell tokenization.
    pub arguments: String,
    pub instructions: String,
    pub description: String,
    pub summary: String,
    pub blocked_reason: Option<String>,
}

fn bounded_file(roots: &Roots, path: &Path, optional: bool) -> Result<()> {
    roots.check(path)?;
    if optional && !path.exists() {
        return Ok(());
    }
    let file = paths::open_regular_read(path)?;
    ensure!(
        file.metadata()?.len() <= 8 * 1024 * 1024,
        "Patcher document exceeds 8 MiB: {}",
        path.display()
    );
    Ok(())
}

pub fn inspect(roots: &Roots, source: &str, selected: Option<usize>) -> Result<PatchPreview> {
    let path = roots.resolve_literal(source)?;
    let metadata = std::fs::symlink_metadata(&path)?;
    ensure!(
        metadata.is_file() || metadata.is_dir(),
        "Choose an ordinary patcher file or folder"
    );
    // Files inside tslpatchdata have the same mod root as the folder itself.
    // Otherwise picking changes.ini in the explorer would inspect a nonexistent
    // tslpatchdata/tslpatchdata tree instead of the selected installer.
    let folder = if metadata.is_dir() {
        path.clone()
    } else {
        path.parent()
            .context("Patcher file has no parent")?
            .to_path_buf()
    };
    let in_data = folder
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("tslpatchdata"));
    let ini_name = if metadata.is_file()
        && in_data
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
        && !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("namespaces.ini"))
    {
        path.file_name()
            .and_then(|name| name.to_str())
            .context("Patcher instruction filename is not UTF-8")?
    } else {
        "changes.ini"
    };
    let base = if in_data {
        folder
            .parent()
            .context("Patcher data folder has no parent")?
            .to_path_buf()
    } else {
        folder
    };
    roots.check(&base)?;
    let data = base.join("tslpatchdata");
    roots.check(&data)?;
    let namespaces = data.join("namespaces.ini");
    bounded_file(roots, &namespaces, true)?;
    let variants = if namespaces.exists() {
        odypatcher::patcher::reader::NamespaceReader::from_filepath(&namespaces)
            .map_err(|error| anyhow::anyhow!("Read patcher options: {error}"))?
    } else {
        Vec::new()
    };
    ensure!(
        variants.len() <= 1024,
        "Patcher offers more than 1024 options"
    );
    for variant in &variants {
        let folder = variant
            .data_folderpath
            .to_str()
            .context("Patcher option path is not UTF-8")?;
        let folder = data.join(paths::relative(folder, false)?);
        let ini = folder.join(paths::relative(&variant.ini_filename, false)?);
        let info = folder.join(paths::relative(&variant.info_filename, false)?);
        bounded_file(roots, &ini, true)?;
        bounded_file(roots, &info, true)?;
    }
    if variants.is_empty() {
        bounded_file(roots, &data.join(ini_name), true)?;
        bounded_file(roots, &data.join("info.rtf"), true)?;
        ensure!(
            selected.is_none(),
            "This patcher does not offer namespace options"
        );
    } else if let Some(index) = selected {
        ensure!(index < variants.len(), "Patcher option is out of range");
    }
    let mut session = odypatcher::Session::discover(
        base.to_str().context("Patcher path is not UTF-8")?,
        ini_name,
        "info.rtf",
    );
    if let Some(index) = selected {
        session.select_namespace(index);
    }
    let instructions = editor::variable_path(roots, &session.ini_path())?;
    Ok(PatchPreview {
        source: source.into(),
        backend: odypatcher::APP_NAME,
        version: odypatcher::VERSION,
        options: session
            .namespaces
            .iter()
            .enumerate()
            .map(|(index, variant)| PatchOption {
                index,
                name: variant.name.clone(),
                description: variant.description.clone(),
            })
            .collect(),
        selected: (!session.namespaces.is_empty()).then_some(session.selected),
        arguments: if !session.namespaces.is_empty() {
            session.selected.to_string()
        } else if ini_name.eq_ignore_ascii_case("changes.ini") {
            String::new()
        } else {
            ini_name.to_owned()
        },
        instructions,
        description: session.info_text(),
        summary: session.summary_text(),
        blocked_reason: session.blocked_reason(),
    })
}
