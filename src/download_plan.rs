//! Read-only filename selection matching the reference download cache service.
use crate::{Component, reference_paths::pattern_matches};
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value};
use std::path::Path;

fn files<'a>(component: &'a Component, url: &str) -> Option<&'a Map<String, Value>> {
    component
        .resource_registry
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(url))
        .and_then(|(_, metadata)| metadata.get("Files"))
        .and_then(Value::as_object)
}

pub fn url_enabled(component: &Component, url: &str) -> bool {
    if url.trim().is_empty() {
        return true;
    }
    files(component, url).is_none_or(|files| {
        files.is_empty() || files.values().any(|flag| flag != &Value::Bool(false))
    })
}

fn filename(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
}
fn matches(name: &str, source: &str) -> bool {
    let pattern = source
        .replace("<<modDirectory>>\\", "")
        .replace("<<modDirectory>>/", "");
    let pattern = pattern.trim_start_matches(['\\', '/']);
    pattern_matches(&format!("<<modDirectory>>\\{name}"), source)
        || pattern_matches(name, filename(pattern))
}
fn add_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|s| s.eq_ignore_ascii_case(value)) {
        values.push(value.into());
    }
}

fn unsatisfied(component: &Component, source: &Path) -> Result<Vec<String>> {
    let patterns: Vec<_> = component
        .instructions
        .iter()
        .chain(component.options.iter().flat_map(|o| &o.instructions))
        .filter(|i| i.action == "Extract")
        .flat_map(|i| &i.source)
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .collect();
    let mut available = Vec::new();
    for meta in component.resource_registry.values() {
        if let Some(files) = meta.get("Files").and_then(Value::as_object) {
            for name in files.keys().filter(|s| !s.trim().is_empty()) {
                add_unique(&mut available, name);
                add_unique(&mut available, filename(name));
            }
        }
    }
    if source.is_dir() {
        // A Peer staging folder left by a killed race holds an unaccepted copy, not an
        // archive that is present.
        let walk = walkdir::WalkDir::new(source)
            .into_iter()
            .filter_entry(|entry| {
                entry.depth() == 0
                    || !(entry.file_type().is_dir()
                        && crate::download_orchestrator::is_peer_staging(entry.file_name()))
            });
        for entry in walk {
            let entry = entry?;
            ensure!(
                !entry.file_type().is_symlink(),
                "Symlinks are unsupported in download planning: {}",
                entry.path().display()
            );
            if entry.file_type().is_file() {
                let relative = entry
                    .path()
                    .strip_prefix(source)?
                    .to_str()
                    .context("Non-UTF-8 download filename")?;
                add_unique(&mut available, relative);
                add_unique(&mut available, filename(relative));
            }
        }
    }
    Ok(patterns
        .into_iter()
        .filter(|pattern| !available.iter().any(|name| matches(name, pattern)))
        .collect())
}

pub fn select_filenames(
    component: &Component,
    url: &str,
    candidates: &[String],
    source: Option<&Path>,
) -> Result<Vec<String>> {
    if url.trim().is_empty() || candidates.is_empty() {
        return Ok(Vec::new());
    }
    let explicit = files(component, url).filter(|f| !f.is_empty());
    if explicit.is_some() {
        ensure!(source.is_some(), "MainConfig.SourcePath not set");
    }
    let Some(source) = source else {
        return Ok(candidates.to_vec());
    };
    let has_extract = component
        .instructions
        .iter()
        .chain(component.options.iter().flat_map(|o| &o.instructions))
        .any(|i| i.action == "Extract");
    let patterns = if has_extract {
        unsatisfied(component, source)?
    } else {
        Vec::new()
    };
    let mut result = Vec::new();
    for name in candidates {
        let known = explicit.and_then(|files| {
            files
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value)
        });
        let allowed = known.map_or_else(
            || {
                !has_extract
                    || (!name.trim().is_empty()
                        && patterns.iter().any(|pattern| matches(name, pattern)))
            },
            |flag| flag != &Value::Bool(false),
        );
        if allowed {
            // Explicit filtering uses LINQ Where; auto-discovery uses a HashSet.
            if explicit.is_some() || !has_extract {
                result.push(name.clone());
            } else {
                add_unique(&mut result, name);
            }
        }
    }
    Ok(result)
}
