//! Filename-only provider discovery for authoring. Never acquires archive bytes.
use crate::{
    Guide,
    archive_generation::resolved::ResolvedFiles,
    download,
    download_hosts::{self, Provider},
    updates::NexusClient,
};
use anyhow::{Result, ensure};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
};

const MAX_RESOURCES: usize = 4096;
const MAX_NAMES: usize = 4096;
const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_NAME_BYTES: usize = 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub component_guid: String,
    /// Zero-based position in this component's resource registry; no secret URL.
    pub resource_index: usize,
    /// Fixed code, never a provider error body or URL.
    pub code: &'static str,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub resolved: Vec<ResolvedFiles>,
    pub diagnostics: Vec<Diagnostic>,
    pub cancelled: bool,
}

/// Inspect all top-level components, independent of install selection, as convert
/// does. Nonempty authored Files dictionaries are authoritative (false disables a
/// file); otherwise query the provider's metadata surface. Partial discovery has
/// diagnostics and must not be presented as complete generation.
pub fn resolve(guide: &Guide, nexus_key: Option<String>, cancelled: &AtomicBool) -> Result<Report> {
    ensure!(
        serde_json::to_vec(guide)?.len() <= MAX_INPUT_BYTES,
        "Filename discovery guide exceeds 8 MiB"
    );
    ensure!(
        guide.components.len() <= MAX_RESOURCES,
        "Too many filename discovery components"
    );
    let count: usize = guide
        .components
        .iter()
        .map(|c| c.resource_registry.len())
        .sum();
    ensure!(
        count <= MAX_RESOURCES,
        "Too many filename discovery resources"
    );
    let mut report = Report {
        resolved: Vec::new(),
        diagnostics: Vec::new(),
        cancelled: false,
    };
    let mut nexus = None;
    let mut key = nexus_key;
    let mut bytes = 0usize;
    let mut total = 0usize;
    let mut identities = std::collections::BTreeSet::new();
    for component in &guide.components {
        ensure!(
            !component.guid.is_empty()
                && component.guid.len() <= 128
                && identities.insert(crate::fomod::fold_case(&component.guid)),
            "Filename discovery requires bounded unique component identities"
        );
    }
    for component in &guide.components {
        let mut names = BTreeMap::<String, String>::new();
        let mut ordered_names = Vec::new();
        for (resource_index, (url, metadata)) in component.resource_registry.iter().enumerate() {
            if cancelled.load(Ordering::Acquire) {
                report.cancelled = true;
                break;
            }
            let found = (|| -> Result<Vec<String>> {
                if let Some(files) = metadata.get("Files") {
                    let files = files
                        .as_object()
                        .ok_or_else(|| anyhow::anyhow!("Malformed Files"))?;
                    if !files.is_empty() {
                        return files
                            .iter()
                            .filter(|(_, flag)| **flag != serde_json::Value::Bool(false))
                            .map(|(name, _)| checked(name).map(str::to_owned))
                            .collect();
                    }
                }
                let parsed = reqwest::Url::parse(url)?;
                ensure!(
                    parsed.username().is_empty() && parsed.password().is_none(),
                    "Credential URL rejected"
                );
                match download_hosts::provider(url)? {
                    Provider::DeadlyStream => Ok(download_hosts::deadlystream::resolve(
                        url, None, None, cancelled,
                    )?
                    .into_iter()
                    .map(|file| file.name)
                    .collect()),
                    Provider::Mega => {
                        let link = download_hosts::mega::parse_link(url)?;
                        let node =
                            download_hosts::mega::resolve(&link, url, true, None, None, cancelled)?;
                        Ok(vec![
                            node.name
                                .ok_or_else(|| anyhow::anyhow!("Missing node filename"))?,
                        ])
                    }
                    Provider::Nexus => {
                        if nexus.is_none() {
                            let mut client = NexusClient::new(key.take())?;
                            client.defer_rate_limits();
                            nexus = Some(client);
                        }
                        let client = nexus.as_mut().expect("initialized Nexus client");
                        let (game, mod_id, file_id) = nexus_identity(&parsed)?;
                        if let Some(id) = file_id {
                            let file = client.get_mod_file_with_cancel(
                                &game,
                                &mod_id.to_string(),
                                &id.to_string(),
                                cancelled,
                            )?;
                            Ok(vec![
                                file.file_name
                                    .ok_or_else(|| anyhow::anyhow!("Missing file name"))?,
                            ])
                        } else {
                            Ok(client
                                .get_mod_files_with_cancel(&game, &mod_id.to_string(), cancelled)?
                                .into_iter()
                                .filter(|file| {
                                    super::NEXUS_PAGE_CATEGORIES.iter().any(|category| {
                                        file.category_name.eq_ignore_ascii_case(category)
                                    })
                                })
                                .map(|file| file.file_name)
                                .collect())
                        }
                    }
                    Provider::GameFront => anyhow::bail!("Unsupported provider"),
                    Provider::Direct => {
                        let outcome = crate::host_metadata::resolve(url, None, None, cancelled);
                        let name = outcome
                            .metadata
                            .and_then(|mut metadata| metadata.remove("fileName"))
                            .and_then(|value| value.as_str().map(str::to_owned))
                            .ok_or_else(|| anyhow::anyhow!("No filename metadata"))?;
                        Ok(vec![name])
                    }
                }
            })();
            if cancelled.load(Ordering::Acquire) {
                report.cancelled = true;
                break;
            }
            let diagnostic = |code| Diagnostic {
                component_guid: component.guid.clone(),
                resource_index,
                code,
            };
            let found = match found {
                Ok(found) => found,
                Err(_) => {
                    let code = if download_hosts::provider(url).ok() == Some(Provider::GameFront) {
                        "unsupportedGameFront"
                    } else {
                        "filenameResolutionFailed"
                    };
                    report.diagnostics.push(diagnostic(code));
                    continue;
                }
            };
            // Authored all-disabled Files means intentional exclusion, not failure.
            if found.is_empty()
                && metadata
                    .get("Files")
                    .and_then(serde_json::Value::as_object)
                    .is_none_or(|files| files.is_empty())
            {
                report.diagnostics.push(diagnostic("noEligibleFilenames"));
            }
            if found.iter().any(|name| checked(name).is_err()) {
                report
                    .diagnostics
                    .push(diagnostic("unsafeProviderFilename"));
                continue;
            }
            for name in found {
                if cancelled.load(Ordering::Acquire) {
                    report.cancelled = true;
                    break;
                }
                let folded = crate::fomod::fold_case(&name);
                if let Some(previous) = names.get(&folded) {
                    if previous != &name {
                        report.diagnostics.push(diagnostic("ambiguousFilenameCase"));
                    }
                    continue;
                }
                total += 1;
                bytes = bytes
                    .checked_add(name.len())
                    .ok_or_else(|| anyhow::anyhow!("Filename size overflow"))?;
                ensure!(
                    total <= MAX_NAMES && bytes <= MAX_NAME_BYTES,
                    "Filename discovery output limit exceeded"
                );
                ordered_names.push(name.clone());
                names.insert(folded, name);
            }
            ensure!(
                report.diagnostics.len() <= MAX_RESOURCES,
                "Too many filename discovery diagnostics"
            );
        }
        if !names.is_empty() {
            report.resolved.push(ResolvedFiles {
                component_guid: component.guid.clone(),
                filenames: ordered_names,
                sha256: None,
                size_bytes: None,
            });
        }
        if report.cancelled {
            break;
        }
    }
    report.cancelled |= cancelled.load(Ordering::Acquire);
    Ok(report)
}
/// A provider file name is literal: `[` and `]` are ordinary characters, as at install.
fn checked(name: &str) -> Result<&str> {
    ensure!(
        name.len() <= 255 && !name.contains(['*', '?']),
        "Invalid filename"
    );
    download::one_component(name)
}
fn nexus_identity(url: &reqwest::Url) -> Result<(String, u64, Option<u64>)> {
    let parts: Vec<_> = url.path_segments().into_iter().flatten().collect();
    ensure!(
        parts.len() >= 3 && parts[1].eq_ignore_ascii_case("mods"),
        "Invalid Nexus path"
    );
    ensure!(
        !parts[0].is_empty()
            && parts[0]
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_')),
        "Invalid Nexus domain"
    );
    let mod_id = parts[2].parse::<u64>()?;
    ensure!(mod_id > 0, "Invalid Nexus mod id");
    let values: Vec<_> = url
        .query_pairs()
        .filter(|(key, _)| key.eq_ignore_ascii_case("file_id"))
        .collect();
    ensure!(values.len() <= 1, "Ambiguous Nexus file id");
    let file = values
        .first()
        .map(|(_, value)| value.parse::<u64>())
        .transpose()?;
    ensure!(file != Some(0), "Invalid Nexus file id");
    Ok((parts[0].to_owned(), mod_id, file))
}
