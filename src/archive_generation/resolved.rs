//! Inference from already resolved provider filenames. No network or guide save.
use crate::{Component, Guide, archive, draft};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ResolvedFiles {
    pub component_guid: String,
    pub filenames: Vec<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
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

fn verify_archive_binding(root: Option<&Path>, item: &ResolvedFiles) -> Result<()> {
    let Some(root) = root else {
        ensure!(
            item.sha256.is_none() && item.size_bytes.is_none(),
            "Archive hash binding requires a local source root"
        );
        return Ok(());
    };
    if item.sha256.is_none() && item.size_bytes.is_none() {
        return Ok(());
    }
    ensure!(
        item.filenames.len() == 1,
        "Archive hash binding requires exactly one archive"
    );
    let expected = item.sha256.as_deref().unwrap_or("");
    ensure!(
        expected.len() == 64 && expected.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Archive SHA-256 binding is invalid"
    );
    let archive = root.join(&item.filenames[0]);
    ordinary(&archive, false)?;
    let metadata = fs::metadata(&archive)?;
    if let Some(size) = item.size_bytes {
        ensure!(
            metadata.len() == size,
            "Archive size does not match its binding"
        );
    }
    ensure!(
        archive_digest(&archive)?.0.eq_ignore_ascii_case(expected),
        "Archive SHA-256 does not match its binding"
    );
    Ok(())
}
/// Lowercase SHA-256 and byte length of an ordinary local file.
pub(crate) fn archive_digest(path: &Path) -> Result<(String, u64)> {
    ordinary(path, false)?;
    let mut input = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        digest.update(&buffer[..count]);
    }
    Ok((format!("{:x}", digest.finalize()), bytes))
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
pub(crate) fn local_root(root: &Path) -> Result<PathBuf> {
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
        "Derivation",
        "Verification",
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
                reusable = Some(crate::guid::option_text(&existing.guid));
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

fn documentation(basename: &str) -> bool {
    let Some((stem, extension)) = basename.rsplit_once('.') else {
        return false;
    };
    matches!(
        stem.to_ascii_lowercase().as_str(),
        "readme" | "read me" | "read_me" | "read-me" | "license" | "licence"
    ) && matches!(
        extension.to_ascii_lowercase().as_str(),
        "txt" | "md" | "rtf" | "pdf"
    )
}

fn bind_loose_method(
    component: &mut Component,
    filename: &str,
    root: Option<&Path>,
    defaults: &serde_json::Value,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<()> {
    ensure!(
        defaults["LooseFilesDestination"] == "<<kotorDirectory>>\\Override",
        "Current guide preamble does not establish the loose-file Override default"
    );
    ensure!(
        !filename.is_empty()
            && filename.len() <= 255
            && !filename.contains(['/', '\\', '*', '?'])
            && !filename.chars().any(char::is_control)
            && crate::paths::relative(filename, false)?
                .components()
                .count()
                == 1
            && !matches!(filename, "." | ".."),
        "Method archive must have a safe literal basename"
    );
    let name = Path::new(filename);
    ensure!(
        matches!(
            name.extension()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("zip" | "7z" | "rar")
        ),
        "Method binding currently requires a ZIP, 7z, or RAR archive"
    );
    let path = root
        .context("Method binding requires a local archive root")?
        .join(filename);
    ordinary(&path, false)?;
    let mut input = crate::paths::open_regular_read(&path)?;
    let limit = 256 * 1024 * 1024;
    ensure!(
        input.metadata()?.len() <= limit,
        "Method archive exceeds 256 MiB"
    );
    let mut builder = tempfile::Builder::new();
    builder.prefix("modsync-method-");
    let mut snapshot = match &options.scratch_dir {
        Some(directory) => builder.tempfile_in(directory)?,
        None => builder.tempfile()?,
    };
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        check(cancelled)?;
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        ensure!(bytes <= limit, "Method archive exceeds 256 MiB");
        hash.update(&buffer[..count]);
        snapshot.write_all(&buffer[..count])?;
    }
    snapshot.flush()?;
    let observed_options = archive::Options {
        scratch_dir: options.scratch_dir.clone(),
        max_expanded_bytes: Some(
            options
                .max_expanded_bytes
                .unwrap_or(u64::MAX)
                .min(1024 * 1024 * 1024),
        ),
    };
    let (entries, _) =
        archive::list_original_with_size(snapshot.path(), name, &observed_options, cancelled)?;
    ensure!(
        !entries.is_empty() && entries.len() <= 4096,
        "Method archive must contain 1 to 4096 entries"
    );
    let mut payload = Vec::new();
    let mut excluded_documentation = Vec::new();
    let mut wrapper = None;
    for entry in entries.iter().filter(|entry| !entry.is_directory) {
        check(cancelled)?;
        ensure!(
            !entry.path.contains(['*', '?']),
            "Method archive contains a wildcard filename"
        );
        let (parent, basename) = entry.path.rsplit_once('/').unwrap_or(("", &entry.path));
        ensure!(
            !parent.contains('/'),
            "Method archive contains nested payload or variants"
        );
        if documentation(basename) {
            excluded_documentation.push(entry.path.clone());
            continue;
        }
        ensure!(
            super::game_file(&entry.path),
            "Method archive has an unclassified file; no contents may be silently omitted"
        );
        ensure!(
            !matches!(
                basename
                    .rsplit('.')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .as_str(),
                "are" | "bik" | "erf" | "git" | "ifo" | "mod" | "rim" | "tlk" | "wav" | "lip"
            ),
            "Method archive contains a resource whose Override destination is not established"
        );
        if let Some(previous) = wrapper {
            ensure!(
                previous == parent,
                "Method archive has multiple payload roots or variants"
            );
        } else {
            wrapper = Some(parent);
        }
        payload.push(entry.path.clone());
    }
    let wrapper = wrapper.context("Method archive has no recognized loose-file payload")?;
    ensure!(
        excluded_documentation.iter().all(|path| {
            let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
            parent.is_empty() || parent == wrapper
        }),
        "Method documentation must be at the archive root or shared payload wrapper"
    );
    ensure!(
        entries
            .iter()
            .filter(|entry| entry.is_directory)
            .all(|entry| !wrapper.is_empty() && entry.path.trim_end_matches('/') == wrapper),
        "Method archive has additional directories or variants"
    );
    payload.sort();
    excluded_documentation.sort();
    let stem = name
        .file_stem()
        .and_then(|value| value.to_str())
        .context("Invalid method archive stem")?;
    let sources: Vec<_> = payload
        .iter()
        .map(|member| format!("<<modDirectory>>\\{stem}\\{}", member.replace('/', "\\")))
        .collect();
    ensure!(
        sources.iter().map(String::len).sum::<usize>() <= 1024 * 1024,
        "Method source inventory exceeds one MiB"
    );
    let mut bound = component.clone();
    bound.instructions = vec![
        super::instruction(
            "Extract",
            vec![format!("<<modDirectory>>\\{filename}")],
            "",
            "",
        ),
        super::instruction("Move", sources, "<<kotorDirectory>>\\Override", ""),
    ];
    let archive_sha256 = format!("{:x}", hash.finalize());
    bound.metadata.insert(
        "ArchiveMethodBinding".into(),
        json!({
            "Filename": filename,
            "Sha256": archive_sha256,
            "Bytes": bytes,
            "Members": payload,
            "ExcludedDocumentation": excluded_documentation,
            "Method": component.metadata.get("InstallationMethod"),
            "GuideDefaults": defaults,
        }),
    );
    review(&mut bound, &[])?;
    let record = bound
        .metadata
        .get_mut("DraftReview")
        .context("Missing bound method review")?;
    let mut result: draft::DraftResult = serde_json::from_value(record["Result"].clone())?;
    result
        .unparsed_gaps
        .retain(|gap| gap != draft::METHOD_UNRESOLVED);
    record["Result"] = serde_json::to_value(result)?;
    record["Derivation"] = serde_json::to_value(draft::review::Derivation {
        filename: filename.to_owned(),
        sha256: archive_sha256,
        bytes,
    })?;
    record["MethodContext"] = json!({
        "Method": component.metadata.get("InstallationMethod"),
        "Kind": "LooseFile",
        "GuideDefaults": defaults,
        "ExplicitDirectionsTakePrecedence": true,
        "ArchiveBinding": "Bound",
    });
    ensure!(
        serde_json::to_vec(record)?.len() <= 1024 * 1024,
        "Method review exceeds one MiB"
    );
    check(cancelled)?;
    *component = bound;
    Ok(())
}
/// Rebuild what `generate` derived from one hash-bound local archive, on a
/// component stripped of every instruction and option. The caller compares the
/// result with the saved draft; nothing here mutates `component`.
pub(crate) fn replay_archive_derivation(
    component: &Component,
    derivation: &draft::review::Derivation,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Component> {
    check(cancelled)?;
    let filename = derivation.filename.as_str();
    ensure!(
        !filename.is_empty()
            && filename.len() <= 255
            && !filename.contains(['/', '\\'])
            && !filename.chars().any(char::is_control)
            && crate::paths::relative(filename, false)?
                .components()
                .count()
                == 1
            && !matches!(filename, "." | ".."),
        "Derived archive name is not a safe basename"
    );
    ensure!(
        derivation.sha256.len() == 64 && derivation.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "Derivation SHA-256 is invalid"
    );
    ensure!(
        matches!(
            Path::new(filename)
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("zip" | "7z" | "rar" | "exe" | "tar" | "gz")
        ),
        "Derived archive has an unsupported extension"
    );
    let path = local_root(root)?.join(filename);
    let (before, bytes) = archive_digest(&path).context("Derived archive is unavailable")?;
    ensure!(
        before.eq_ignore_ascii_case(&derivation.sha256) && bytes == derivation.bytes,
        "Archive no longer matches its recorded SHA-256"
    );
    let mut target = component.clone();
    target.instructions.clear();
    target.options.clear();
    let mut inferred = target.clone();
    ensure!(
        super::generate_from_archive(&mut inferred, &path, options, cancelled)?,
        "Archive no longer yields inferred instructions"
    );
    merge_inferred(&mut target, inferred, filename)?;
    check(cancelled)?;
    ensure!(
        archive_digest(&path)?.0 == before,
        "Archive changed during replay"
    );
    Ok(target)
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
    let defaults = draft::guide_defaults(guide);
    let mut candidate = guide.clone();
    let mut reports = Vec::new();
    let mut seen = BTreeMap::new();
    for group in resolved {
        check(cancelled)?;
        verify_archive_binding(root.as_deref(), group)?;
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
        let method = component
            .metadata
            .get("InstallationMethod")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if method.is_some()
            || component
                .metadata
                .get("DraftReview")
                .is_some_and(|record| record.get("MethodContext").is_some())
        {
            let eligible = method.is_some_and(|method| {
                method.eq_ignore_ascii_case("Loose-File")
                    || method.eq_ignore_ascii_case("Loose-File Mod")
            }) && component.directions.trim().is_empty()
                && component.instructions.is_empty()
                && component.options.is_empty()
                && !["DownloadInstructions", "InstallationWarning"]
                    .iter()
                    .any(|key| {
                        component
                            .metadata
                            .get(*key)
                            .and_then(|value| value.as_str())
                            .is_some_and(|text| {
                                !text.replace(draft::REVIEW_FLAG, "").trim().is_empty()
                            })
                    });
            if !eligible {
                report.diagnostics.push("Method binding requires a method-only LooseFile component without instructions, choices, or additional installation/download guidance; component unchanged.".into());
            } else if group.filenames.len() != 1
                || resolved
                    .iter()
                    .filter(|other| other.component_guid == group.component_guid)
                    .count()
                    != 1
            {
                report.diagnostics.push("Method binding requires exactly one archive in one resolved component group; component unchanged.".into());
            } else {
                match bind_loose_method(
                    component,
                    &group.filenames[0],
                    root.as_deref(),
                    &defaults,
                    options,
                    cancelled,
                ) {
                    Ok(()) => report.instructions_added = 2,
                    Err(error) => {
                        check(cancelled)?;
                        report
                            .diagnostics
                            .push(format!("Loose-file method remains unresolved: {error:#}"));
                    }
                }
            }
            reports.push(report);
            continue;
        }
        let single_archive = group.filenames.len() == 1
            && resolved
                .iter()
                .filter(|other| other.component_guid == group.component_guid)
                .map(|other| other.filenames.len())
                .sum::<usize>()
                == 1;
        let mut derivation = None;
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
                let bound = if single_archive {
                    Some(archive_digest(path)?)
                } else {
                    None
                };
                if super::generate_from_archive(&mut inferred, path, options, cancelled)? {
                    report.instructions_added += merge_inferred(component, inferred, filename)?;
                    derivation = bound.map(|(sha256, bytes)| draft::review::Derivation {
                        filename: filename.clone(),
                        sha256,
                        bytes,
                    });
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
            if let Some(derivation) = derivation {
                let record = component
                    .metadata
                    .get_mut("DraftReview")
                    .and_then(serde_json::Value::as_object_mut)
                    .context("Missing inferred draft review")?;
                record.insert("Derivation".into(), serde_json::to_value(derivation)?);
            }
        }
        reports.push(report);
    }
    check(cancelled)?;
    *guide = candidate;
    Ok(reports)
}
