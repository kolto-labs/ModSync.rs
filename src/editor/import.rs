use super::{EditOperation, apply_operations};
use crate::Guide;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase", deny_unknown_fields)]
pub enum ImportMode {
    Append {
        parent: Option<String>,
        index: usize,
    },
    Replace {
        guid: String,
    },
}

/// Preview is immutable proposed data. Application checks the captured guide
/// identity, then executes exactly these operations through the shared validator.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportPreview {
    pub schema_version: u32,
    pub input_identity: String,
    pub operations: Vec<EditOperation>,
    pub warnings: Vec<String>,
}

fn identity(guide: &Guide) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(guide)?)))
}

/// Local/raw and already-fetched URL documents share the same parser. Network
/// retrieval remains in `protocol`; no requests or writes occur in this method.
pub fn preview_import(
    guide: &Guide,
    text: &str,
    format: &str,
    mode: ImportMode,
) -> Result<ImportPreview> {
    ensure!(
        text.len() <= 8 * 1024 * 1024,
        "Import document exceeds 8 MiB"
    );
    let imported = Guide::parse_without_dedup(text, format, false)?;
    preview_guide(guide, imported, mode)
}

pub(super) fn preview_guide(
    guide: &Guide,
    imported: Guide,
    mode: ImportMode,
) -> Result<ImportPreview> {
    ensure!(
        imported.components.len() <= 10_000,
        "Import has too many components"
    );
    let mut warnings = Vec::new();
    if !imported.metadata.is_empty() {
        warnings.push("Component import preserves component fields; document-level metadata is not merged. Use a guide metadata edit to transfer selected fields.".into());
    }
    let operations = match mode {
        ImportMode::Append { parent, index } => imported
            .components
            .into_iter()
            .enumerate()
            .map(|(offset, component)| {
                Ok(EditOperation::InsertComponent {
                    parent: parent.clone(),
                    index: index
                        .checked_add(offset)
                        .ok_or_else(|| anyhow::anyhow!("Import insertion index overflow"))?,
                    component: Box::new(component),
                })
            })
            .collect::<Result<Vec<_>>>()?,
        ImportMode::Replace { guid } => {
            ensure!(
                imported.components.len() == 1,
                "Replacement import needs exactly one component"
            );
            vec![EditOperation::ReplaceComponent {
                guid,
                component: Box::new(imported.components.into_iter().next().unwrap()),
            }]
        }
    };
    // Preview failures are actionable before the user confirms any mutation.
    apply_operations(guide, &operations)?;
    Ok(ImportPreview {
        schema_version: 1,
        input_identity: identity(guide)?,
        operations,
        warnings,
    })
}

pub fn apply_preview(guide: &Guide, preview: &ImportPreview) -> Result<Guide> {
    ensure!(
        preview.schema_version == 1,
        "Unsupported editor import preview version"
    );
    ensure!(
        preview.input_identity == identity(guide)?,
        "The guide changed after this import preview; preview again before applying"
    );
    Ok(apply_operations(guide, &preview.operations)?)
}
