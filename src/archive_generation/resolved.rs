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

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
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
/// CLI request extension; legacy public ResolvedFiles and its strict schema stay unchanged.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedRequest {
    #[serde(flatten)]
    pub files: ResolvedFiles,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor_language: Option<String>,
}
impl std::ops::Deref for ResolvedRequest {
    type Target = ResolvedFiles;
    fn deref(&self) -> &Self::Target {
        &self.files
    }
}
impl<'de> serde::Deserialize<'de> for ResolvedRequest {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            component_guid: String,
            filenames: Vec<String>,
            #[serde(default)]
            sha256: Option<String>,
            #[serde(default)]
            size_bytes: Option<u64>,
            #[serde(default)]
            vendor_language: Option<String>,
        }
        let wire = <Wire as serde::Deserialize>::deserialize(deserializer)?;
        Ok(Self {
            files: ResolvedFiles {
                component_guid: wire.component_guid,
                filenames: wire.filenames,
                sha256: wire.sha256,
                size_bytes: wire.size_bytes,
            },
            vendor_language: wire.vendor_language,
        })
    }
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
        // A bare Windows prefix (`C:`, `\\?\C:`, `\\server\share`) names a drive's
        // current directory or the volume device, not a directory on the path, and
        // querying the device fails with ERROR_INVALID_FUNCTION. The root directory
        // that follows it is checked like every other ancestor.
        if matches!(part, std::path::Component::Prefix(_)) {
            continue;
        }
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

/// A component that is only an installation method (one of `methods`): no
/// directions, instructions, choices, or extra installation/download guidance.
/// Shared by generation and by the verifier's replay so both gate identically.
fn method_only(component: &Component, methods: &[&str]) -> bool {
    component
        .metadata
        .get("InstallationMethod")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .is_some_and(|method| {
            methods
                .iter()
                .any(|known| method.eq_ignore_ascii_case(known))
        })
        && component.directions.trim().is_empty()
        && component.instructions.is_empty()
        && component.options.is_empty()
        && !["DownloadInstructions", "InstallationWarning"]
            .iter()
            .any(|key| {
                component
                    .metadata
                    .get(*key)
                    .and_then(|value| value.as_str())
                    .is_some_and(|text| !text.replace(draft::REVIEW_FLAG, "").trim().is_empty())
            })
}

fn method_only_loose_file(component: &Component) -> bool {
    method_only(component, &["Loose-File", "Loose-File Mod"])
}

fn method_only_patcher(component: &Component) -> bool {
    method_only(
        component,
        &[
            "TSLPatcher",
            "TSLPatcher Mod",
            "HoloPatcher",
            "HoloPatcher Mod",
        ],
    )
}

/// Rebuild what `bind_loose_method` or `bind_patcher_method` derived for one
/// method-only loose-file or patcher component, from the hash-verified archive, on a copy stripped of its
/// instructions, options and prior draft record. The recorded guide defaults
/// come from the draft's own binding receipt (the verifier sees no guide
/// preamble); they only unlock the fixed Override destination, and the caller
/// compares the result with the saved draft. Nothing here mutates `component`.
pub(crate) fn replay_method_binding(
    component: &Component,
    derivation: &draft::review::Derivation,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Component> {
    check(cancelled)?;
    let record = component
        .metadata
        .get("DraftReview")
        .context("Missing draft review")?;
    let context = record
        .get("MethodContext")
        .context("Missing method context")?;
    let patcher = context["Kind"] == "Patcher";
    ensure!(
        (patcher || context["Kind"] == "LooseFile") && context["ArchiveBinding"] == "Bound",
        "Method context is not an archive-bound loose-file or patcher binding"
    );
    let binding = component
        .metadata
        .get("ArchiveMethodBinding")
        .context("Missing archive method binding")?;
    ensure!(
        binding["Filename"] == derivation.filename.as_str()
            && binding["Sha256"]
                .as_str()
                .is_some_and(|hash| hash.eq_ignore_ascii_case(&derivation.sha256))
            && binding["Bytes"] == derivation.bytes
            && binding["GuideDefaults"] == context["GuideDefaults"],
        "Method binding does not match its archive derivation"
    );
    let defaults = binding["GuideDefaults"].clone();
    let mut target = component.clone();
    target.instructions.clear();
    target.options.clear();
    target.metadata.remove("ArchiveMethodBinding");
    target.metadata.remove("DraftReview");
    ensure!(
        if patcher {
            method_only_patcher(&target)
        } else {
            method_only_loose_file(&target)
        },
        "Component is not a method-only loose-file or patcher component"
    );
    let root = local_root(root)?;
    let (before, bytes) = archive_digest(&root.join(&derivation.filename))
        .context("Derived archive is unavailable")?;
    ensure!(
        before.eq_ignore_ascii_case(&derivation.sha256) && bytes == derivation.bytes,
        "Archive no longer matches its recorded SHA-256"
    );
    let rebind = if patcher {
        bind_patcher_method
    } else {
        bind_loose_method
    };
    rebind(
        &mut target,
        &derivation.filename,
        Some(&root),
        &defaults,
        options,
        cancelled,
    )
    .context("Archive no longer yields the recorded method binding")?;
    Ok(target)
}

struct MethodArchive {
    entries: Vec<archive::OriginalEntry>,
    sha256: String,
    bytes: u64,
    expanded_bytes: u64,
}

/// Hash and list one safely named local method archive from a private snapshot,
/// so the digest and the listing describe the same bytes.
fn snapshot_method_archive(
    filename: &str,
    root: Option<&Path>,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<MethodArchive> {
    snapshot_bounded_archive(
        filename,
        root,
        options,
        cancelled,
        256 * 1024 * 1024,
        1024 * 1024 * 1024,
    )
}

fn snapshot_bounded_archive(
    filename: &str,
    root: Option<&Path>,
    options: &archive::Options,
    cancelled: &AtomicBool,
    limit: u64,
    expanded_limit: u64,
) -> Result<MethodArchive> {
    ensure!(
        !filename.is_empty()
            && filename.len() <= 255
            && !filename.contains(['/', '\\', '*', '?'])
            && !filename.chars().any(char::is_control)
            && crate::paths::literal_relative(filename)?
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
    ensure!(
        input.metadata()?.len() <= limit,
        "Method archive exceeds its bounded input size"
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
        ensure!(
            bytes <= limit,
            "Method archive exceeds its bounded input size"
        );
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
                .min(expanded_limit),
        ),
    };
    let (entries, expanded_bytes) =
        archive::list_original_with_size(snapshot.path(), name, &observed_options, cancelled)?;
    ensure!(
        !entries.is_empty() && entries.len() <= 4096,
        "Method archive must contain 1 to 4096 entries"
    );
    Ok(MethodArchive {
        entries,
        sha256: format!("{:x}", hash.finalize()),
        bytes,
        expanded_bytes,
    })
}

fn reviewed_movie_variant_members(
    snapshot: &MethodArchive,
) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    ensure!(
        snapshot.sha256 == "5b60a4b3e559a40c1a06477431e6a60be3794d116cdb7e594fc2f5e32dc93d26"
            && snapshot.bytes == 16_567_032_171
            && snapshot.expanded_bytes == 16_844_254_424,
        "Movie variants do not match the reviewed resolution and frame-rate archive"
    );
    let mut inventory: Vec<_> = snapshot
        .entries
        .iter()
        .map(|entry| {
            (
                entry.path.trim_end_matches('/').to_owned(),
                entry.is_directory,
            )
        })
        .collect();
    inventory.sort();
    ensure!(
        format!("{:x}", Sha256::digest(serde_json::to_vec(&inventory)?))
            == "2de3f93825fbb7cf8ad9d6d5259001b5bad0dc9b0a2fbf91b7a62f4296ef8bae",
        "Movie variant layout differs from the complete reviewed inventory"
    );
    let mut selected = Vec::new();
    let mut optional = Vec::new();
    let mut directories = Vec::new();
    for (path, directory) in inventory {
        if directory {
            directories.push(path);
        } else if path.starts_with("1920x1080/") && path.matches('/').count() == 1 {
            selected.push(path);
        } else if path.starts_with("1920x1080/alts/") {
            optional.push(path);
        } else {
            anyhow::bail!("Movie payload has no reviewed selection classification");
        }
    }
    ensure!(
        selected.len() == 61 && optional.len() == 11 && directories.len() == 5,
        "Movie variant partition is incomplete"
    );
    ensure!(
        optional.iter().all(|alternative| selected
            .iter()
            .any(|base| base.rsplit('/').next() == alternative.rsplit('/').next())),
        "An optional movie lacks its selected base variant"
    );
    Ok((selected, optional, directories))
}

/// Bind explicitly directed movies from disjoint flat ZIPs or the exact reviewed
/// 7z variant inventory. Optional overlays are partitioned and recorded, never
/// flattened into the default selection. No inferred executable or ordering is used.
pub(crate) fn bind_movie_archives(
    component: &mut Component,
    filenames: &[String],
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<bool> {
    super::ensure_generic_archive_inference(component)?;
    let Some((context, gaps)) = draft::movie_context(component) else {
        return Ok(false);
    };
    ensure!(
        component.instructions.is_empty() && component.options.is_empty(),
        "Movie binding must not replace authored instructions or choices"
    );
    if let Some(record) = component.metadata.get("DraftReview") {
        ensure!(
            record
                .get("RequiresReview")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
                && record.get("Resolution").is_none()
                && record.get("Verification").is_none(),
            "Movie binding must not replace reviewed provenance"
        );
    }
    ensure!(
        !filenames.is_empty() && filenames.len() <= 16,
        "Movie binding needs 1 to 16 archives"
    );
    let root = local_root(root)?;
    let mut names = std::collections::BTreeSet::new();
    let mut payload_names = std::collections::BTreeSet::new();
    let mut manifests = Vec::new();
    let mut derivations = Vec::new();
    let mut bound = component.clone();
    let variants = context["Layout"] == "ReviewedMovieVariants";
    for filename in filenames {
        check(cancelled)?;
        ensure!(
            names.insert(filename.to_lowercase()),
            "Movie archive names collide"
        );
        ensure!(
            Path::new(filename)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case(if variants { "7z" } else { "zip" }))
                && (!variants || filenames.len() == 1),
            "Movie archive format or selection is unsupported"
        );
        let snapshot = snapshot_bounded_archive(
            filename,
            Some(&root),
            options,
            cancelled,
            (if variants { 20 } else { 4 }) * 1024 * 1024 * 1024,
            (if variants { 20 } else { 8 }) * 1024 * 1024 * 1024,
        )?;
        let (mut members, excluded, directories) = if variants {
            reviewed_movie_variant_members(&snapshot)?
        } else {
            let mut members = Vec::new();
            for entry in &snapshot.entries {
                ensure!(
                    !entry.is_directory
                        && !entry.path.contains(['/', '\\'])
                        && entry.path.to_ascii_lowercase().ends_with(".bik"),
                    "Movie archive contains a directory, script, documentation, or non-movie payload"
                );
                members.push(entry.path.clone());
            }
            (members, Vec::new(), Vec::new())
        };
        for member in &members {
            let basename = member.rsplit('/').next().context("Empty movie member")?;
            super::vet_archive_name(std::ffi::OsStr::new(basename))?;
            ensure!(
                payload_names.insert(basename.to_lowercase()),
                "Movie archives overlap; overwrite order needs explicit authoring"
            );
        }
        members.sort();
        ensure!(
            !members.is_empty() && payload_names.len() <= 4096,
            "Movie payload inventory exceeds its bound"
        );
        let stem = Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .context("Invalid movie archive stem")?;
        let mut extract = super::instruction(
            "Extract",
            vec![format!("<<modDirectory>>\\{filename}")],
            "",
            "",
        );
        let mut install = super::instruction(
            "Move",
            members
                .iter()
                .map(|member| format!("<<modDirectory>>\\{stem}\\{}", member.replace('/', "\\")))
                .collect(),
            "<<kotorDirectory>>\\Movies",
            "",
        );
        for step in [&mut extract, &mut install] {
            step.guid = crate::markdown::generated_id(
                &format!("{}:movie:{filename}", component.guid),
                bound.instructions.len(),
            );
            step.parent = component.guid.clone();
            bound.instructions.push(step.clone());
        }
        manifests.push(json!({"Filename": filename, "Sha256": snapshot.sha256,
            "Bytes": snapshot.bytes, "ExpandedBytes": snapshot.expanded_bytes, "Members": members,
            "ExcludedOptionalMembers": excluded, "Directories": directories,
            "OptionalOverlaySelection": if variants { "None; selected base movies preserve original subtitles and legal movie" } else { "No variants present" }}));
        derivations.push(draft::review::Derivation {
            filename: filename.clone(),
            sha256: snapshot.sha256,
            bytes: snapshot.bytes,
        });
    }
    let binding = json!({"Schema": 1, "Context": context, "Archives": manifests,
                         "InputDraftReview": component.metadata.get("DraftReview")});
    ensure!(
        serde_json::to_vec(&binding)?.len() <= 1024 * 1024,
        "Movie binding exceeds one MiB"
    );
    bound.metadata.insert("ArchiveMovieBinding".into(), binding);
    bound.metadata.insert(
        "DraftReview".into(),
        json!({"RequiresReview": true,
        "Generator": "resolved-movie-archives", "Derivations": derivations,
        "Result": draft::DraftResult {component_guid: component.guid.clone(),
            draft_instruction_count: bound.instructions.len(), unparsed_gaps: gaps,
            conditional_drafts: Vec::new(), rejected_drafts: 0}}),
    );
    check(cancelled)?;
    *component = bound;
    Ok(true)
}

pub(crate) fn replay_movie_binding(
    component: &Component,
    root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Component> {
    let binding = component
        .metadata
        .get("ArchiveMovieBinding")
        .context("Missing movie binding")?;
    let record = component
        .metadata
        .get("DraftReview")
        .context("Missing movie review")?;
    let derivations: Vec<draft::review::Derivation> = serde_json::from_value(
        record
            .get("Derivations")
            .cloned()
            .context("Missing movie derivations")?,
    )?;
    ensure!(
        !derivations.is_empty() && derivations.len() <= 16,
        "Invalid movie derivation count"
    );
    let mut target = component.clone();
    target.instructions.clear();
    ensure!(
        target.options.is_empty(),
        "Movie binding acquired unsupported options"
    );
    target.metadata.remove("ArchiveMovieBinding");
    target.metadata.remove("DraftReview");
    if let Some(input) = binding
        .get("InputDraftReview")
        .filter(|value| !value.is_null())
    {
        target.metadata.insert("DraftReview".into(), input.clone());
    }
    ensure!(
        bind_movie_archives(
            &mut target,
            &derivations
                .iter()
                .map(|item| item.filename.clone())
                .collect::<Vec<_>>(),
            root,
            options,
            cancelled
        )?,
        "Guide no longer supports movie binding"
    );
    ensure!(
        target.metadata.get("ArchiveMovieBinding") == Some(binding)
            && target
                .metadata
                .get("DraftReview")
                .and_then(|r| r.get("Derivations"))
                == record.get("Derivations")
            && target
                .metadata
                .get("DraftReview")
                .and_then(|r| r.get("Result"))
                == record.get("Result"),
        "Movie guide context, archive hashes, sizes, members, or diagnostics differ from replay"
    );
    Ok(target)
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
    let MethodArchive {
        entries,
        sha256: archive_sha256,
        bytes,
        ..
    } = snapshot_method_archive(filename, root, options, cancelled)?;
    let name = Path::new(filename);
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
        if documentation(basename)
            || super::reviewed_manual::is_reviewed_documentation(
                &component.guid,
                &archive_sha256,
                &entry.path,
            )
        {
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
    finish_method_binding(
        component,
        bound,
        "LooseFile",
        json!({
            "Filename": filename,
            "Sha256": archive_sha256,
            "Bytes": bytes,
            "Members": payload,
            "ExcludedDocumentation": excluded_documentation,
            "Method": component.metadata.get("InstallationMethod"),
            "GuideDefaults": defaults,
        }),
        draft::review::Derivation {
            filename: filename.to_owned(),
            sha256: archive_sha256,
            bytes,
        },
        cancelled,
    )
}

/// The method-only guide component an archive method binding was made from, for
/// definition comparisons only. The binding adds instructions and its receipt to
/// a component that had neither, so removing them restores the input; it is
/// admitted only if the result is method-only again. Never proof of the archive
/// or of installation: replay re-derives the binding from the archive bytes.
pub(crate) fn declared_original_definition(component: &Component) -> Result<Option<Component>> {
    if !component.metadata.contains_key("ArchiveMethodBinding") {
        return Ok(None);
    }
    let mut original = component.clone();
    original.instructions.clear();
    original.options.clear();
    original.metadata.remove("ArchiveMethodBinding");
    original.metadata.remove("DraftReview");
    ensure!(
        method_only_patcher(&original) || method_only_loose_file(&original),
        "Component is not a method-only loose-file or patcher component"
    );
    Ok(Some(original))
}

/// Record the binding receipt, the archive derivation and the bound method
/// context on a freshly bound component, then publish it.
fn finish_method_binding(
    component: &mut Component,
    mut bound: Component,
    kind: &str,
    binding: serde_json::Value,
    derivation: draft::review::Derivation,
    cancelled: &AtomicBool,
) -> Result<()> {
    let defaults = binding["GuideDefaults"].clone();
    bound
        .metadata
        .insert("ArchiveMethodBinding".into(), binding);
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
    record["Derivation"] = serde_json::to_value(derivation)?;
    record["MethodContext"] = json!({
        "Method": component.metadata.get("InstallationMethod"),
        "Kind": kind,
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

struct PatcherLayout {
    members: Vec<String>,
    excluded_documentation: Vec<String>,
    installer: String,
}

/// Accept one single-run patcher tree, either at the archive root or inside
/// exactly one structural wrapper directory. Its sole installer must sit beside
/// `tslpatchdata/changes.ini`; namespaces and other payload roots stay refused.
/// Documentation may sit at the root or beside the installer. Keep every full
/// authored path in the receipt so replay checks the same exhaustive layout.
fn patcher_layout(entries: &[archive::OriginalEntry]) -> Result<PatcherLayout> {
    let files: Vec<_> = entries.iter().filter(|entry| !entry.is_directory).collect();
    for entry in &files {
        ensure!(
            !entry.path.contains(['*', '?', '\\']),
            "Method archive contains a wildcard or backslash filename"
        );
    }
    let lower_name = |path: &str| path.rsplit('/').next().unwrap_or("").to_ascii_lowercase();
    ensure!(
        files
            .iter()
            .all(|entry| lower_name(&entry.path) != "namespaces.ini"),
        "Method archive carries namespaces.ini; namespace selections stay unresolved"
    );
    let changes: Vec<_> = files
        .iter()
        .filter(|entry| lower_name(&entry.path) == "changes.ini")
        .collect();
    ensure!(
        changes.len() == 1,
        "Method archive must contain exactly one tslpatchdata/changes.ini"
    );
    let changes_parts: Vec<_> = changes[0].path.split('/').collect();
    ensure!(
        matches!(changes_parts.len(), 2 | 3)
            && changes_parts[changes_parts.len() - 2].eq_ignore_ascii_case("tslpatchdata"),
        "Method archive changes.ini must be inside root-level tslpatchdata or one structural wrapper"
    );
    let wrapper = (changes_parts.len() == 3).then_some(changes_parts[0]);
    let in_data = |path: &str, directory: bool| {
        let parts: Vec<_> = path.trim_end_matches('/').split('/').collect();
        let offset = usize::from(wrapper.is_some());
        parts.len() > offset
            && wrapper.is_none_or(|wrapper| parts[0] == wrapper)
            && parts[offset].eq_ignore_ascii_case("tslpatchdata")
            && (directory || parts.len() > offset + 1)
    };
    let mut members = Vec::new();
    let mut excluded_documentation = Vec::new();
    let mut installers = Vec::new();
    for entry in &files {
        let path = entry.path.as_str();
        let executable = path.to_ascii_lowercase().ends_with(".exe");
        if in_data(path, false) {
            ensure!(
                !executable,
                "Method archive has an executable inside tslpatchdata"
            );
            members.push(path.to_owned());
        } else if executable {
            ensure!(
                path.rsplit_once('/').map_or("", |(parent, _)| parent) == wrapper.unwrap_or(""),
                "Method archive installer is not beside tslpatchdata"
            );
            installers.push(path.to_owned());
        } else {
            let (parent, basename) = path.rsplit_once('/').unwrap_or(("", path));
            ensure!(
                (parent.is_empty() || Some(parent) == wrapper) && documentation(basename),
                "Method archive has payload outside tslpatchdata"
            );
            excluded_documentation.push(path.to_owned());
        }
    }
    ensure!(
        installers.len() == 1,
        "Method archive must contain exactly one patcher executable"
    );
    ensure!(
        entries
            .iter()
            .filter(|entry| entry.is_directory)
            .all(|entry| {
                in_data(&entry.path, true)
                    || wrapper.is_some_and(|wrapper| entry.path.trim_end_matches('/') == wrapper)
            }),
        "Method archive has additional directories or variants"
    );
    members.sort();
    excluded_documentation.sort();
    Ok(PatcherLayout {
        members,
        excluded_documentation,
        installer: installers.remove(0),
    })
}

fn bind_patcher_method(
    component: &mut Component,
    filename: &str,
    root: Option<&Path>,
    defaults: &serde_json::Value,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<()> {
    ensure!(
        defaults["PatcherDestination"] == "<<kotorDirectory>>",
        "Current guide preamble does not establish the patcher game-folder default"
    );
    let MethodArchive {
        entries,
        sha256: archive_sha256,
        bytes,
        ..
    } = snapshot_method_archive(filename, root, options, cancelled)?;
    let layout = patcher_layout(&entries)?;
    let path = root
        .context("Method binding requires a local archive root")?
        .join(filename);
    let mut inferred = component.clone();
    inferred.instructions.clear();
    inferred.options.clear();
    ensure!(
        super::generate_from_archive(&mut inferred, &path, options, cancelled)?,
        "Archive no longer yields inferred instructions"
    );
    ensure!(
        archive_digest(&path)?.0 == archive_sha256,
        "Method archive changed during binding"
    );
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|value| value.to_str())
        .context("Invalid method archive stem")?;
    let installer_source = format!(
        "<<modDirectory>>\\{stem}\\{}",
        layout.installer.replace('/', "\\")
    );
    ensure!(
        inferred.options.is_empty()
            && matches!(
                inferred.instructions.as_slice(),
                [extract, patch]
                    if extract.action == "Extract"
                        && extract.source == [format!("<<modDirectory>>\\{filename}")]
                        && extract.destination.is_empty()
                        && patch.action == "Patcher"
                        // Native inference retains '/' in its structural wrapper.
                        // Compare separators only; keep its serialized source and
                        // every prefix, segment, spelling, and order unchanged.
                        && patch.source.len() == 1
                        && patch.source[0].replace('/', "\\") == installer_source
                        && patch.destination == "<<gameDirectory>>"
            ),
        "Patcher archive does not yield exactly an Extract and a Patcher instruction"
    );
    let mut bound = component.clone();
    bound.instructions = inferred.instructions;
    finish_method_binding(
        component,
        bound,
        "Patcher",
        json!({
            "Filename": filename,
            "Sha256": archive_sha256,
            "Bytes": bytes,
            "Members": layout.members,
            "ExcludedDocumentation": layout.excluded_documentation,
            "Installer": layout.installer,
            "Method": component.metadata.get("InstallationMethod"),
            "GuideDefaults": defaults,
        }),
        draft::review::Derivation {
            filename: filename.to_owned(),
            sha256: archive_sha256,
            bytes,
        },
        cancelled,
    )
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
            && crate::paths::literal_relative(filename)?
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
    generate_with_vendor_languages(guide, resolved, &BTreeMap::new(), root, options, cancelled)
}

pub fn generate_with_vendor_languages(
    guide: &mut Guide,
    resolved: &[ResolvedFiles],
    vendor_languages: &BTreeMap<String, String>,
    root: Option<&Path>,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Vec<ResolvedReport>> {
    check(cancelled)?;
    ensure!(
        vendor_languages.len() <= 1
            && vendor_languages
                .iter()
                .all(|(guid, language)| guid == crate::vendor_installer::GUID
                    && language == "english"
                    && resolved.iter().any(|item| &item.component_guid == guid)),
        "Vendor language selection is unsupported or has no resolved component"
    );
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
        if super::source_plan_namespace::has_owner(&component.guid)? {
            if resolved
                .iter()
                .filter(|other| other.component_guid == group.component_guid)
                .count()
                != 1
            {
                report.diagnostics.push(
                    "Source-plan selected namespace owner requires one exhaustive resolved filename group; binding remains unresolved.".into(),
                );
            } else if let Some(root) = root.as_deref() {
                match super::source_plan_namespace::bind(
                    component,
                    &group.filenames,
                    root,
                    options,
                    cancelled,
                ) {
                    Ok(()) => report.instructions_added = component.instructions.len(),
                    Err(error) => {
                        check(cancelled)?;
                        report.diagnostics.push(format!(
                            "Source-plan selected namespace owner remains unresolved: {error:#}"
                        ));
                    }
                }
            } else {
                report.diagnostics.push(
                    "Source-plan selected namespace owner requires verified local archives; binding remains unresolved.".into(),
                );
            }
            if !report.diagnostics.is_empty() {
                review(component, &report.diagnostics)?;
                component
                    .metadata
                    .get_mut("DraftReview")
                    .and_then(serde_json::Value::as_object_mut)
                    .context("Missing unresolved source-plan review")?
                    .insert(
                        "Generator".into(),
                        super::source_plan_namespace::GENERATOR.into(),
                    );
            }
            reports.push(report);
            continue;
        }
        if super::reviewed_manual::has_recipe(&component.guid)? {
            if resolved
                .iter()
                .filter(|other| other.component_guid == group.component_guid)
                .count()
                != 1
            {
                report.diagnostics.push(
                    "Reviewed manual recipe requires one exhaustive resolved filename group; component unchanged.".into(),
                );
            } else if let Some(root) = root.as_deref() {
                match super::reviewed_manual::bind(
                    component,
                    &group.filenames,
                    root,
                    options,
                    cancelled,
                ) {
                    Ok(()) => report.instructions_added = component.instructions.len(),
                    Err(error) => {
                        check(cancelled)?;
                        report.diagnostics.push(format!(
                            "Reviewed manual file effects remain unresolved: {error:#}"
                        ));
                    }
                }
            } else {
                report.diagnostics.push(
                    "Reviewed manual recipe requires verified local archives; component unchanged."
                        .into(),
                );
            }
            reports.push(report);
            continue;
        }
        if super::source_plan_patcher::has_owner(&component.guid)? {
            if resolved
                .iter()
                .filter(|other| other.component_guid == group.component_guid)
                .count()
                != 1
            {
                report.diagnostics.push(
                    "Source-plan native owner requires one exhaustive resolved filename group; binding remains unresolved.".into(),
                );
            } else if let Some(root) = root.as_deref() {
                match super::source_plan_patcher::bind(
                    component,
                    &group.filenames,
                    root,
                    options,
                    cancelled,
                ) {
                    Ok(()) => report.instructions_added = component.instructions.len(),
                    Err(error) => {
                        check(cancelled)?;
                        if super::source_plan_namespace::eligible_for_fallback(&component.guid)? {
                            match super::source_plan_namespace::bind(
                                component,
                                &group.filenames,
                                root,
                                options,
                                cancelled,
                            ) {
                                Ok(()) => {
                                    report.instructions_added = component.instructions.len();
                                    // Report-only context: changing the independently derived model would invalidate replay.
                                    let cause =
                                        format!("{error:#}").chars().take(2048).collect::<String>();
                                    report.diagnostics.push(format!("Original pinned program compiler bound successfully after compatibility admission declined: {cause}"));
                                }
                                Err(fallback) => {
                                    check(cancelled)?;
                                    report.diagnostics.push(format!("Source-plan native owner remains unresolved: compatibility admission: {error:#}; original program admission: {fallback:#}"));
                                }
                            }
                        } else {
                            report.diagnostics.push(format!(
                                "Source-plan native owner remains unresolved: {error:#}"
                            ));
                        }
                    }
                }
            } else {
                report.diagnostics.push(
                    "Source-plan native owner requires verified local archives; binding remains unresolved.".into(),
                );
            }
            if report.instructions_added == 0 && !report.diagnostics.is_empty() {
                review(component, &report.diagnostics)?;
                component
                    .metadata
                    .get_mut("DraftReview")
                    .and_then(serde_json::Value::as_object_mut)
                    .context("Missing unresolved source-plan review")?
                    .insert(
                        "Generator".into(),
                        super::source_plan_patcher::GENERATOR.into(),
                    );
            }
            reports.push(report);
            continue;
        }
        if let Some(language) = vendor_languages.get(&group.component_guid) {
            ensure!(
                group.component_guid == crate::vendor_installer::GUID
                    && group.filenames.len() == 1
                    && group.filenames[0] == crate::vendor_installer::FILENAME
                    && group.sha256.as_deref() == Some(crate::vendor_installer::SHA256)
                    && group.size_bytes == Some(crate::vendor_installer::SIZE)
                    && resolved
                        .iter()
                        .filter(|other| other.component_guid == group.component_guid)
                        .count()
                        == 1,
                "Explicit vendor selection requires the exact reviewed installer group"
            );
            crate::vendor_installer::bind(
                component,
                root.as_deref()
                    .context("Vendor binding requires local source")?,
                language,
                cancelled,
            )?;
            report.instructions_added = component.instructions.len();
            reports.push(report);
            continue;
        }
        if component.instructions.is_empty()
            && let Some(root) = root.as_deref()
            && bind_movie_archives(component, &group.filenames, root, options, cancelled)?
        {
            report.instructions_added = component.instructions.len();
            reports.push(report);
            continue;
        }
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
            let patcher = method_only_patcher(component);
            if !patcher && !method_only_loose_file(component) {
                report.diagnostics.push("Method binding requires a method-only LooseFile or Patcher component without instructions, choices, or additional installation/download guidance; component unchanged.".into());
            } else if group.filenames.len() != 1
                || resolved
                    .iter()
                    .filter(|other| other.component_guid == group.component_guid)
                    .count()
                    != 1
            {
                report.diagnostics.push("Method binding requires exactly one archive in one resolved component group; component unchanged.".into());
            } else {
                let bound = if patcher {
                    bind_patcher_method
                } else {
                    bind_loose_method
                };
                match bound(
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
                        report.diagnostics.push(format!(
                            "{} method remains unresolved: {error:#}",
                            if patcher { "Patcher" } else { "Loose-file" }
                        ));
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
                crate::paths::literal_relative(filename)?
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
    super::reviewed_master::bind_in_guide(&mut candidate)?;
    super::duplicate_cleanup::bind_guide(&mut candidate, cancelled)?;
    *guide = candidate;
    Ok(reports)
}
