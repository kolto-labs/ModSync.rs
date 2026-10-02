//! Configurable, bounded regex import with explicit preview/apply separation.
//! Patterns use Rust regex syntax. Unsupported constructs are diagnosed before
//! parsing; they are never silently stripped or delegated to an external tool.
use super::{ImportMode, ImportPreview, import::preview_guide};
use crate::{Component, Guide};
use anyhow::{Context, Result, ensure};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct RegexFlags {
    pub multiline: bool,
    pub singleline: bool,
    pub ignore_case: bool,
    pub global: bool,
}
impl Default for RegexFlags {
    fn default() -> Self {
        Self {
            multiline: true,
            singleline: true,
            ignore_case: false,
            global: true,
        }
    }
}

/// Resetting a custom profile uses DefaultMarkdown, the production guide parser.
/// Raw mode reads named captures from each match. Individual mode runs a field
/// pattern inside each section; its capture name is the map key, or `value`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase", deny_unknown_fields)]
pub enum RegexImportProfile {
    #[default]
    DefaultMarkdown,
    Raw {
        pattern: String,
        #[serde(default)]
        flags: RegexFlags,
    },
    Individual {
        section_pattern: String,
        fields: BTreeMap<String, String>,
        #[serde(default)]
        flags: RegexFlags,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchTrace {
    pub component_index: usize,
    pub field: String,
    /// UTF-8 byte offsets into the exact original input, suitable for highlighting.
    pub start: usize,
    pub end: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegexImportPreview {
    pub preview: ImportPreview,
    pub matches: Vec<MatchTrace>,
    pub unmatched_lines: Vec<usize>,
}

const FIELDS: &[&str] = &[
    "heading",
    "name",
    "name_plain",
    "name_link",
    "author",
    "description",
    "masters",
    "category",
    "tier",
    "category_tier",
    "method",
    "installation_method",
    "download",
    "directions",
    "installation_instructions",
    "warning",
    "screenshots",
    "bugs",
    "installwarning",
    "compatwarning",
    "steamnotes",
    "language",
    "non_english",
    "link",
];

fn compile(pattern: &str, flags: &RegexFlags) -> Result<Regex> {
    ensure!(
        !pattern.is_empty() && pattern.len() <= 64 * 1024,
        "Regex pattern must contain 1 to 65536 bytes"
    );
    RegexBuilder::new(pattern)
        .multi_line(flags.multiline)
        .dot_matches_new_line(flags.singleline)
        .case_insensitive(flags.ignore_case)
        .size_limit(8 * 1024 * 1024)
        .dfa_size_limit(8 * 1024 * 1024)
        .build()
        .context("Unsupported or invalid import regex; patterns use Rust regex syntax")
}

pub fn preview_regex_import(
    guide: &Guide,
    text: &str,
    profile: &RegexImportProfile,
    mode: ImportMode,
) -> Result<RegexImportPreview> {
    ensure!(
        text.len() <= 8 * 1024 * 1024,
        "Import document exceeds 8 MiB"
    );
    if matches!(profile, RegexImportProfile::DefaultMarkdown) {
        let mut preview = super::preview_import(guide, text, "md", mode)?;
        preview.warnings.push("Default Markdown parsing does not provide custom regex capture highlights or unmatched-line diagnostics.".into());
        return Ok(RegexImportPreview {
            preview,
            matches: Vec::new(),
            unmatched_lines: Vec::new(),
        });
    }
    let (pattern, flags, field_patterns) = match profile {
        RegexImportProfile::Raw { pattern, flags } => (pattern, flags, None),
        RegexImportProfile::Individual {
            section_pattern,
            fields,
            flags,
        } => (section_pattern, flags, Some(fields)),
        RegexImportProfile::DefaultMarkdown => unreachable!(),
    };
    let outer = compile(pattern, flags)?;
    let mut fields = BTreeMap::new();
    if let Some(patterns) = field_patterns {
        for (field, pattern) in patterns {
            ensure!(
                FIELDS.contains(&field.as_str()),
                "Unknown regex import field {field}"
            );
            let regex = compile(pattern, flags)?;
            ensure!(
                regex
                    .capture_names()
                    .flatten()
                    .any(|name| name == field || name == "value"),
                "Field {field} pattern needs a named {field} or value capture"
            );
            fields.insert(field.clone(), regex);
        }
    } else {
        for name in outer.capture_names().flatten() {
            ensure!(FIELDS.contains(&name), "Unknown raw import capture {name}");
        }
    }
    let mut components = Vec::new();
    let mut masters = Vec::new();
    let mut traces = Vec::new();
    for capture in outer.captures_iter(text) {
        ensure!(
            components.len() < 10_000,
            "Regex import exceeds 10000 components"
        );
        let section = capture.get(0).unwrap();
        ensure!(
            !section.is_empty(),
            "Component section regex matched empty text"
        );
        let mut values: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut record = |field: &str, value: regex::Match<'_>, offset: usize| -> Result<()> {
            let cleaned = value
                .as_str()
                .trim()
                .lines()
                .map(|line| {
                    line.strip_prefix(": ")
                        .or_else(|| line.strip_prefix(':'))
                        .unwrap_or(line)
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !cleaned.is_empty() {
                ensure!(
                    traces.len() < 100_000,
                    "Regex import exceeds 100000 captures"
                );
                values.entry(field.into()).or_default().push(cleaned);
                traces.push(MatchTrace {
                    component_index: components.len(),
                    field: field.into(),
                    start: offset + value.start(),
                    end: offset + value.end(),
                });
            }
            Ok(())
        };
        if field_patterns.is_some() {
            for (field, regex) in &fields {
                for captures in regex.captures_iter(section.as_str()) {
                    if let Some(value) = captures.name(field).or_else(|| captures.name("value")) {
                        record(field, value, section.start())?;
                    }
                    if field != "link" {
                        break;
                    }
                }
            }
        } else {
            for field in FIELDS {
                if let Some(value) = capture.name(field) {
                    record(field, value, 0)?;
                }
            }
        }
        let first = |keys: &[&str]| {
            keys.iter()
                .find_map(|key| values.get(*key).and_then(|values| values.first()).cloned())
        };
        let name = first(&["name", "name_plain", "name_link", "heading"])
            .context("Matched regex section has no name or heading capture")?;
        let mut object = Map::new();
        object.insert(
            "Guid".into(),
            crate::markdown::generated_id(section.as_str(), components.len()).into(),
        );
        object.insert("Name".into(), name.into());
        for (target, sources) in [
            ("Author", &["author"][..]),
            ("Description", &["description"]),
            ("Directions", &["directions", "installation_instructions"]),
            ("Heading", &["heading"]),
            ("InstallationMethod", &["method", "installation_method"]),
            ("DownloadInstructions", &["download"]),
            ("UsageWarning", &["warning"]),
            ("Screenshots", &["screenshots"]),
            ("KnownBugs", &["bugs"]),
            ("InstallationWarning", &["installwarning"]),
            ("CompatibilityWarning", &["compatwarning"]),
            ("SteamNotes", &["steamnotes"]),
        ] {
            if let Some(value) = first(sources) {
                object.insert(target.into(), value.into());
            }
        }
        let category_tier = first(&["category_tier"]);
        let split = category_tier
            .as_deref()
            .and_then(|value| value.split_once('/'));
        if let Some(category) =
            first(&["category"]).or_else(|| split.map(|(category, _)| category.to_owned()))
        {
            object.insert(
                "Category".into(),
                json!(
                    category
                        .split([',', '&'])
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                ),
            );
        }
        if let Some(tier) =
            first(&["tier"]).or_else(|| split.map(|(_, tier)| tier.trim().to_owned()))
        {
            object.insert("Tier".into(), tier.into());
        }
        if let Some(language) = first(&["language", "non_english"]) {
            object.insert("Language".into(), json!([language]));
        }
        let mut registry = Map::new();
        for link in values.get("link").into_iter().flatten() {
            let url = reqwest::Url::parse(link).context("Invalid captured resource URL")?;
            ensure!(
                matches!(url.scheme(), "http" | "https"),
                "Captured resource URL must use HTTP(S)"
            );
            if !registry.keys().any(|old| old.eq_ignore_ascii_case(link)) {
                registry.insert(link.clone(), json!({"Files": {}, "HandlerMetadata": {}}));
            }
        }
        object.insert("ResourceRegistry".into(), Value::Object(registry));
        masters.push(first(&["masters"]).unwrap_or_default());
        let component: Component = serde_json::from_value(Value::Object(object))?;
        components.push(component);
        if !flags.global {
            break;
        }
    }
    ensure!(!components.is_empty(), "Regex import matched no components");
    let mut names: BTreeMap<String, Vec<String>> = BTreeMap::new();
    fn add_names(items: &[Component], names: &mut BTreeMap<String, Vec<String>>) {
        for component in items {
            for name in [&component.name, &component.guid].into_iter().chain(
                component
                    .metadata
                    .get("Heading")
                    .and_then(Value::as_str)
                    .map(|s| s.to_owned())
                    .as_ref(),
            ) {
                let ids = names.entry(name.to_lowercase()).or_default();
                if !ids.contains(&component.guid) {
                    ids.push(component.guid.clone());
                }
            }
            add_names(&component.options, names);
        }
    }
    add_names(&guide.components, &mut names);
    add_names(&components, &mut names);
    for (component, masters) in components.iter_mut().zip(masters) {
        for master in masters
            .split([',', ';', '+', '&'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let ids = names
                .get(&master.to_lowercase())
                .with_context(|| format!("Unresolved dependency name {master:?}"))?;
            ensure!(
                ids.len() == 1,
                "Ambiguous dependency name {master:?}; use its GUID"
            );
            if !component.dependencies.contains(&ids[0]) {
                component.dependencies.push(ids[0].clone());
            }
        }
    }
    let preview = preview_guide(
        guide,
        Guide {
            components,
            ..Default::default()
        },
        mode,
    )?;
    let mut offset = 0;
    let mut unmatched_lines = Vec::new();
    let mut ranges: Vec<_> = traces
        .iter()
        .map(|trace| (trace.start, trace.end))
        .collect();
    ranges.sort_unstable();
    let mut next = 0;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        let end = offset + line.len();
        while next < ranges.len() && ranges[next].1 <= offset {
            next += 1;
        }
        if !line.trim().is_empty() && (next == ranges.len() || ranges[next].0 >= end) {
            unmatched_lines.push(index + 1);
        }
        offset = end;
    }
    Ok(RegexImportPreview {
        preview,
        matches: traces,
        unmatched_lines,
    })
}
