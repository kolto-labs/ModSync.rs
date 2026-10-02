//! Inference from already resolved provider filenames. No network or guide save.
use crate::{Component, Guide, archive, draft};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedFiles {
    pub component_guid: String,
    pub filenames: Vec<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedReport {
    pub component_guid: String,
    pub instructions_added: usize,
    pub diagnostics: Vec<String>,
}
fn check(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Resolved filename inference cancelled"
    );
    Ok(())
}
fn ordinary(path: &Path, directory: bool) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "Inference path is a symbolic link"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "Inference path is a reparse point"
        );
    }
    ensure!(
        if directory {
            metadata.is_dir()
        } else {
            metadata.is_file()
        },
        "Inference path has an unsupported file type"
    );
    Ok(())
}
pub(super) fn local_root(root: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(root)?;
    ensure!(
        absolute.components().count() <= 128,
        "Inference root is too deep"
    );
    let mut current = PathBuf::new();
    for part in absolute.components() {
        ensure!(
            !matches!(part, std::path::Component::ParentDir),
            "Inference root contains a parent traversal"
        );
        current.push(part);
        ordinary(&current, true)?;
    }
    Ok(absolute)
}
fn review(component: &mut Component, diagnostics: &[String]) -> Result<()> {
    let mut result = if let Some(record) = component.metadata.get("DraftReview") {
        let result: draft::DraftResult = serde_json::from_value(
            record
                .get("Result")
                .cloned()
                .context("Existing draft lacks diagnostics")?,
        )?;
        ensure!(
            result.component_guid == component.guid,
            "Existing draft belongs to another component"
        );
        result
    } else {
        draft::DraftResult {
            component_guid: component.guid.clone(),
            draft_instruction_count: 0,
            unparsed_gaps: Vec::new(),
            conditional_drafts: Vec::new(),
            rejected_drafts: 0,
        }
    };
    result.draft_instruction_count = component.instructions.len();
    for diagnostic in diagnostics {
        if !result.unparsed_gaps.contains(diagnostic) {
            result.unparsed_gaps.push(diagnostic.clone());
        }
    }
    // Retain earlier receipts/provenance as history, but never let them authorize
    // changed instructions. The standard explicit review workflow owns acceptance.
    let mut record = component
        .metadata
        .get("DraftReview")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let object = record
        .as_object_mut()
        .context("Existing draft review is not an object")?;
    let prior: serde_json::Map<String, serde_json::Value> = [
        "ReviewedIdentity",
        "Resolution",
        "OriginalResult",
        "PriorOriginalResult",
    ]
    .into_iter()
    .filter_map(|key| object.remove(key).map(|value| (key.to_owned(), value)))
    .collect();
    if !prior.is_empty() {
        let history = object
            .entry("InferenceHistory")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .context("Invalid inference review history")?;
        ensure!(
            history.len() < 32,
            "Inference review history exceeds 32 generations; preserve and explicitly consolidate provenance before more inference"
        );
        history.push(serde_json::Value::Object(prior));
    }
    ensure!(
        serde_json::to_vec(&object)?.len() <= 1024 * 1024,
        "Inference review provenance exceeds one MiB"
    );
    object.insert("RequiresReview".into(), true.into());
    object.insert("Result".into(), serde_json::to_value(result)?);
    object.insert("Generator".into(), "resolved-filenames".into());
    ensure!(
        serde_json::to_vec(&record)?.len() <= 1024 * 1024,
        "Inference review provenance exceeds one MiB"
    );
    component.metadata.insert("DraftReview".into(), record);
    Ok(())
}
fn merge_inferred(
    component: &mut Component,
    mut inferred: Component,
    filename: &str,
) -> Result<usize> {
    let mut remap = BTreeMap::new();
    for mut option in inferred.options.drain(..) {
        let old = option.guid.clone();
        // Reuse only exact executable semantics. Wildcard overlap and guards
        // are not proof that an authored option implements this inferred choice.
        let mut expected = serde_json::to_value(&option)?;
        expected
            .as_object_mut()
            .context("Option must be an object")?
            .remove("Guid");
        let mut reusable = None;
        for existing in &component.options {
            let mut value = serde_json::to_value(existing)?;
            let object = value.as_object_mut().context("Option must be an object")?;
            object.remove("Guid");
            object.remove("DraftReview");
            if value == expected {
                reusable = Some(existing.guid.clone());
                break;
            }
        }
        if let Some(existing) = reusable {
            remap.insert(old, existing);
            continue;
        }
        ensure!(
            component.options.len() < 4096,
            "Resolved inference exceeds 4096 options"
        );
        option.guid = crate::markdown::generated_id(
            &format!("{}:resolved:{filename}:{}", component.guid, option.name),
            component.options.len(),
        );
        ensure!(
            !component
                .options
                .iter()
                .any(|old| old.guid.eq_ignore_ascii_case(&option.guid)),
            "Generated option identity collision"
        );
        remap.insert(old, option.guid.clone());
        review(&mut option, &[])?;
        component.options.push(option);
    }
    let mut added = 0;
    for mut step in inferred.instructions {
        if step.action == "Choose" {
            for id in &mut step.source {
                if let Some(new) = remap.get(id) {
                    *id = new.clone();
                }
            }
        }
        ensure!(
            component.instructions.len() < 16384,
            "Resolved inference exceeds 16384 instructions"
        );
        if super::add_step(component, step) {
            added += 1;
        }
    }
    Ok(added)
}
/// Append safe inferred drafts atomically. The caller supplies authoritative
/// component bindings; unsupported names remain explicit review diagnostics.
pub fn generate(
    guide: &mut Guide,
    resolved: &[ResolvedFiles],
    root: Option<&Path>,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Vec<ResolvedReport>> {
    check(cancelled)?;
    ensure!(resolved.len() <= 4096, "Too many resolved component groups");
    resolved
        .iter()
        .try_fold((0usize, 0usize), |(count, bytes), item| -> Result<_> {
            let count = count
                .checked_add(item.filenames.len())
                .context("Filename count overflow")?;
            let bytes = item.filenames.iter().try_fold(bytes, |sum, name| {
                sum.checked_add(name.len())
                    .context("Filename bytes overflow")
            })?;
            ensure!(
                count <= 4096 && bytes <= 1024 * 1024 && item.component_guid.len() <= 1024,
                "Resolved filename input exceeds bounds"
            );
            Ok((count, bytes))
        })?;
    let root = root.map(local_root).transpose()?;
    let mut candidate = guide.clone();
    let mut reports = Vec::new();
    let mut seen = BTreeMap::new();
    for group in resolved {
        check(cancelled)?;
        let matches: Vec<_> = candidate
            .components
            .iter()
            .enumerate()
            .filter(|(_, c)| c.guid == group.component_guid)
            .map(|(i, _)| i)
            .collect();
        ensure!(
            matches.len() == 1,
            "Resolved filename component is missing or ambiguous"
        );
        let component = &mut candidate.components[matches[0]];
        let previous_options = component.options.len();
        let mut report = ResolvedReport {
            component_guid: component.guid.clone(),
            instructions_added: 0,
            diagnostics: Vec::new(),
        };
        for filename in &group.filenames {
            check(cancelled)?;
            ensure!(
                !filename.is_empty()
                    && filename.len() <= 255
                    && !filename.contains(['/', '\\'])
                    && !filename.chars().any(char::is_control),
                "Resolved filename is not a safe basename"
            );
            ensure!(
                crate::paths::relative(filename, false)?
                    .components()
                    .count()
                    == 1
                    && !matches!(filename.as_str(), "." | ".."),
                "Resolved filename is unsafe"
            );
            if let Some(previous) = seen.insert(
                (component.guid.clone(), filename.to_lowercase()),
                filename.clone(),
            ) {
                ensure!(
                    previous == *filename,
                    "Resolved filenames collide under case-insensitive pathing"
                );
                continue;
            }
            let extension = Path::new(filename)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let is_archive = matches!(
                extension.as_str(),
                "zip" | "7z" | "rar" | "exe" | "tar" | "gz"
            );
            let path = root.as_ref().map(|root| root.join(filename));
            let present = if let Some(path) = &path {
                match fs::symlink_metadata(path) {
                    Ok(_) => {
                        ordinary(path, false)?;
                        true
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                    Err(error) => return Err(error.into()),
                }
            } else {
                false
            };
            if is_archive && present {
                let path = path.as_ref().unwrap();
                let _handle = crate::paths::open_regular_read(path)?;
                let mut inferred = component.clone();
                inferred.instructions.clear();
                inferred.options.clear();
                if super::generate_from_archive(&mut inferred, path, options, cancelled)? {
                    report.instructions_added += merge_inferred(component, inferred, filename)?;
                } else {
                    report.diagnostics.push(format!(
                        "No supported archive contents inferred for {filename}"
                    ));
                }
            } else if is_archive || super::game_file(filename) {
                let step = super::instruction(
                    if is_archive { "Extract" } else { "Move" },
                    vec![format!("<<modDirectory>>\\{filename}")],
                    if is_archive {
                        ""
                    } else {
                        "<<gameDirectory>>\\Override"
                    },
                    "",
                );
                ensure!(
                    component.instructions.len() < 16384,
                    "Resolved inference exceeds 16384 instructions"
                );
                if super::add_step(component, step) {
                    report.instructions_added += 1;
                }
            } else {
                report
                    .diagnostics
                    .push(format!("Unsupported resolved filename: {filename}"));
            }
        }
        ensure!(
            component.instructions.len() <= 16384 && component.options.len() <= 4096,
            "Resolved inference model exceeds bounds"
        );
        if report.instructions_added > 0
            || component.options.len() != previous_options
            || !report.diagnostics.is_empty()
        {
            review(component, &report.diagnostics)?;
        }
        reports.push(report);
    }
    check(cancelled)?;
    *guide = candidate;
    Ok(reports)
}
