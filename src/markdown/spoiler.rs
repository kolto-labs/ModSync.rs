//! Atomic, bounded authoring overlay for the reference spoiler-free fields.
use crate::Guide;
use anyhow::{Result, ensure};
use serde::Serialize;
use std::collections::BTreeMap;

const LIMIT: usize = 8 * 1024 * 1024;
const COMPONENT_LIMIT: usize = 10_000;
const FIELDS: [(&str, &str); 5] = [
    ("Description", "DescriptionSpoilerFree"),
    ("Directions", "DirectionsSpoilerFree"),
    ("DownloadInstructions", "DownloadInstructionsSpoilerFree"),
    ("UsageWarning", "UsageWarningSpoilerFree"),
    ("Screenshots", "ScreenshotsSpoilerFree"),
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub component: String,
    pub message: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overlay {
    pub guide: Guide,
    pub complete: bool,
    pub applied_components: usize,
    pub applied_fields: usize,
    pub diagnostics: Vec<Diagnostic>,
    /// Informational projection omissions; these do not reject the overlay.
    pub ignored_fields: Vec<Diagnostic>,
}
#[derive(Default)]
struct Entry {
    name: String,
    fields: BTreeMap<&'static str, String>,
    ignored: Vec<String>,
}
fn issue(diagnostics: &mut Vec<Diagnostic>, name: &str, message: impl Into<String>) {
    if diagnostics.len() > COMPONENT_LIMIT {
        return;
    }
    if diagnostics.len() == COMPONENT_LIMIT {
        diagnostics.push(Diagnostic {
            component: String::new(),
            message: "Additional diagnostics omitted after 10000 entries; overlay rejected".into(),
        });
        return;
    }
    diagnostics.push(Diagnostic {
        component: name.chars().take(512).collect(),
        message: message.into().chars().take(1024).collect(),
    });
}
fn field(label: &str) -> Option<&'static str> {
    let key: String = label.chars().filter(|c| !c.is_whitespace()).collect();
    let key = if key.eq_ignore_ascii_case("InstallationInstructions") {
        "Directions"
    } else {
        &key
    };
    FIELDS
        .iter()
        .find(|(plain, spoiler)| {
            key.eq_ignore_ascii_case(plain) || key.eq_ignore_ascii_case(spoiler)
        })
        .map(|(_, spoiler)| *spoiler)
}
fn ordinary_field(label: &str) -> bool {
    let key: String = label.chars().filter(|c| c.is_alphanumeric()).collect();
    [
        "Name",
        "Author",
        "GUID",
        "IsSelected",
        "Tier",
        "Category",
        "CategoryTier",
        "Masters",
        "Language",
        "NonEnglishFunctionality",
        "InstallationMethod",
        "Dependencies",
        "Restrictions",
        "InstallBefore",
        "InstallAfter",
        "Instructions",
        "Options",
        "ResourceRegistry",
        "KnownBugs",
        "InstallationWarning",
        "CompatibilityWarning",
        "SteamNotes",
        "ModLink",
        "Links",
        "Version",
        "Website",
    ]
    .iter()
    .any(|name| key.eq_ignore_ascii_case(name))
}
fn ignored(entries: &mut Vec<Diagnostic>, name: &str, field: &str) {
    if entries.len() == COMPONENT_LIMIT {
        entries.push(Diagnostic {
            component: String::new(),
            message: "Additional ignored fields omitted after 10000 entries".into(),
        });
    } else if entries.len() < COMPONENT_LIMIT {
        entries.push(Diagnostic {
            component: name.chars().take(512).collect(),
            message: format!(
                "Ignored non-overlay field: {}",
                field.chars().take(512).collect::<String>()
            ),
        });
    }
}
fn put(entry: &mut Entry, label: &str, value: &str, diagnostics: &mut Vec<Diagnostic>) {
    let Some(key) = field(label) else {
        if ordinary_field(label) {
            if entry.ignored.len() < COMPONENT_LIMIT {
                entry.ignored.push(label.chars().take(512).collect());
            }
            return;
        }
        issue(
            diagnostics,
            &entry.name,
            format!("Unsupported overlay field: {label}"),
        );
        return;
    };
    if value.trim().is_empty() {
        issue(
            diagnostics,
            &entry.name,
            format!("Empty overlay field: {label}"),
        );
    } else if entry.fields.insert(key, value.trim().to_owned()).is_some() {
        issue(
            diagnostics,
            &entry.name,
            format!("Duplicate overlay field: {label}"),
        );
    }
}
fn flush(
    entry: &mut Entry,
    active: &mut Option<(String, String)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some((label, value)) = active.take() {
        put(entry, &label, &value, diagnostics);
    }
}

/// Parse the reference Mod List headings, bold fields, and installation admonition.
/// Unknown fields, duplicate headings, and unmatched names prevent the entire overlay.
/// Content before `## Mod List` is document preamble, as in the reference parser.
pub fn apply_markdown(base: &Guide, text: &str) -> Result<Overlay> {
    ensure!(text.len() <= LIMIT, "Spoiler overlay exceeds 8 MiB");
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut entries = Vec::new();
    let mut entry: Option<Entry> = None;
    let mut active: Option<(String, String)> = None;
    let mut diagnostics = Vec::new();
    let mut started = false;
    let mut note = false;
    for line in normalized.lines() {
        let line = line.trim_start();
        if !started {
            if line == "## Mod List" {
                started = true;
            }
            continue;
        }
        if let Some(name) = line.strip_prefix("### ") {
            if let Some(mut previous) = entry.take() {
                flush(&mut previous, &mut active, &mut diagnostics);
                if note {
                    issue(&mut diagnostics, &previous.name, "Unclosed admonition");
                }
                entries.push(previous);
            }
            ensure!(
                entries.len() < COMPONENT_LIMIT,
                "Spoiler overlay exceeds 10000 components"
            );
            entry = Some(Entry {
                name: name.trim().to_owned(),
                ..Default::default()
            });
            note = false;
            continue;
        }
        let Some(entry) = entry.as_mut() else {
            if !line.trim().is_empty() {
                issue(
                    &mut diagnostics,
                    "",
                    "Content before first component heading",
                );
            }
            continue;
        };
        if matches!(line, ":::note" | ":::warning" | ":::tip") {
            flush(entry, &mut active, &mut diagnostics);
            if note {
                issue(&mut diagnostics, &entry.name, "Nested admonition");
            }
            note = true;
        } else if line == ":::" && note {
            flush(entry, &mut active, &mut diagnostics);
            note = false;
        } else if note && line == "Installation Instructions" {
            flush(entry, &mut active, &mut diagnostics);
            active = Some(("Directions".into(), String::new()));
        } else if let Some((label, value)) =
            line.strip_prefix("**").and_then(|s| s.split_once(":**"))
        {
            flush(entry, &mut active, &mut diagnostics);
            active = Some((label.trim().into(), value.trim().into()));
        } else if !line.trim().is_empty() && !line.starts_with("___") {
            if let Some((_, value)) = active.as_mut() {
                if !value.is_empty() {
                    value.push('\n');
                }
                value.push_str(if note {
                    line.strip_prefix(':').unwrap_or(line).trim()
                } else {
                    line
                });
            } else {
                issue(&mut diagnostics, &entry.name, "Unassigned overlay content");
            }
        }
        ensure!(
            diagnostics.len() <= COMPONENT_LIMIT,
            "Too many spoiler overlay diagnostics"
        );
    }
    if let Some(mut entry) = entry {
        flush(&mut entry, &mut active, &mut diagnostics);
        if note {
            issue(&mut diagnostics, &entry.name, "Unclosed admonition");
        }
        entries.push(entry);
    }
    if !started {
        issue(&mut diagnostics, "", "Missing ## Mod List heading");
    }
    apply_entries(base, entries, diagnostics, Vec::new())
}

/// A parsed guide supplies Description/Directions and the three corresponding
/// metadata fields. Explicit *SpoilerFree fields take precedence only when the
/// ordinary field is empty; two competing values are diagnosed, never discarded.
/// Other authoring fields are projected out and reported as informational omissions.
pub fn apply_guide(base: &Guide, source: &Guide) -> Result<Overlay> {
    ensure!(
        serde_json::to_vec(source)?.len() <= LIMIT,
        "Spoiler guide exceeds 8 MiB"
    );
    ensure!(
        source.components.len() <= COMPONENT_LIMIT,
        "Too many spoiler components"
    );
    let mut entries = Vec::new();
    let mut diagnostics = Vec::new();
    let mut ignored_fields = Vec::new();
    for key in source.metadata.keys() {
        ignored(&mut ignored_fields, "", key);
    }
    for component in &source.components {
        let mut entry = Entry {
            name: component.name.clone(),
            ..Default::default()
        };
        if !component.description.is_empty() {
            put(
                &mut entry,
                "Description",
                &component.description,
                &mut diagnostics,
            );
        }
        if !component.directions.is_empty() {
            put(
                &mut entry,
                "Directions",
                &component.directions,
                &mut diagnostics,
            );
        }
        for (key, value) in &component.metadata {
            if field(key).is_none() && !key.to_ascii_lowercase().contains("spoilerfree") {
                ignored(&mut ignored_fields, &entry.name, key);
                continue;
            }
            if let Some(value) = value.as_str() {
                put(&mut entry, key, value, &mut diagnostics);
            } else {
                issue(
                    &mut diagnostics,
                    &entry.name,
                    format!("Overlay field {key} must be text"),
                );
            }
        }
        for (key, populated) in [
            ("GUID", !component.guid.is_empty()),
            ("IsSelected", true),
            ("Author", !component.author.is_empty()),
            ("Tier", !component.tier.is_empty()),
            ("Category", !component.category.is_empty()),
            ("Dependencies", !component.dependencies.is_empty()),
            ("Restrictions", !component.restrictions.is_empty()),
            ("InstallBefore", !component.install_before.is_empty()),
            ("InstallAfter", !component.install_after.is_empty()),
            ("Instructions", !component.instructions.is_empty()),
            ("Options", !component.options.is_empty()),
            ("ResourceRegistry", !component.resource_registry.is_empty()),
        ] {
            if populated {
                ignored(&mut ignored_fields, &entry.name, key);
            }
        }
        entries.push(entry);
    }
    apply_entries(base, entries, diagnostics, ignored_fields)
}
fn apply_entries(
    base: &Guide,
    entries: Vec<Entry>,
    mut diagnostics: Vec<Diagnostic>,
    mut ignored_fields: Vec<Diagnostic>,
) -> Result<Overlay> {
    ensure!(
        serde_json::to_vec(base)?.len() <= LIMIT,
        "Target guide exceeds 8 MiB"
    );
    ensure!(
        base.components.len() <= COMPONENT_LIMIT,
        "Too many target components"
    );
    let mut targets: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, component) in base.components.iter().enumerate() {
        targets
            .entry(crate::fomod::fold_case(&component.name))
            .or_default()
            .push(index);
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut updates = Vec::new();
    if entries.is_empty() {
        issue(&mut diagnostics, "", "Overlay contains no components");
    }
    for entry in entries {
        for label in &entry.ignored {
            ignored(&mut ignored_fields, &entry.name, label);
        }
        if entry.fields.is_empty() {
            ignored(
                &mut ignored_fields,
                &entry.name,
                "component without overlay values",
            );
            continue;
        }
        let key = crate::fomod::fold_case(&entry.name);
        if entry.name.trim().is_empty() || !seen.insert(key.clone()) {
            issue(
                &mut diagnostics,
                &entry.name,
                "Empty or duplicate overlay component name",
            );
            continue;
        }
        match targets.get(&key).map(Vec::as_slice) {
            Some([index]) => updates.push((*index, entry.fields)),
            Some(_) => issue(
                &mut diagnostics,
                &entry.name,
                "Ambiguous target component name",
            ),
            None => issue(
                &mut diagnostics,
                &entry.name,
                "No target component matches this name",
            ),
        }
    }
    let mut result = Overlay {
        guide: base.clone(),
        complete: diagnostics.is_empty(),
        applied_components: 0,
        applied_fields: 0,
        diagnostics,
        ignored_fields,
    };
    if result.complete {
        for (index, fields) in updates {
            result.applied_components += 1;
            for (key, value) in fields {
                result.guide.components[index]
                    .metadata
                    .insert(key.into(), value.into());
                result.applied_fields += 1;
            }
        }
        ensure!(
            serde_json::to_vec(&result.guide)?.len() <= LIMIT,
            "Resulting spoiler guide exceeds 8 MiB"
        );
    }
    Ok(result)
}
