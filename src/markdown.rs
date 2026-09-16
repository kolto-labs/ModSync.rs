use crate::{Component, Guide};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

// Field names used by the reference's default MarkdownImportProfile.
const FIELDS: &[(&str, &str)] = &[
    ("Name", "Name"),
    ("Author", "Author"),
    ("Description", "Description"),
    ("Masters", "Masters"),
    ("Category & Tier", "CategoryTier"),
    ("Non-English Functionality", "Language"),
    ("Installation Method", "InstallationMethod"),
    ("Download Instructions", "DownloadInstructions"),
    ("Installation Instructions", "Directions"),
    ("Install Instructions", "Directions"),
    ("Usage Warning", "UsageWarning"),
    ("Screenshots", "Screenshots"),
    ("Known Bugs", "KnownBugs"),
    ("Installation Warning", "InstallationWarning"),
    ("Compatibility Warning", "CompatibilityWarning"),
    ("Steam Notes", "SteamNotes"),
];

pub(crate) fn parse(text: &str) -> Result<Guide> {
    let text = text.replace("\r\n", "\n");
    // Preserve guides wrapped in fenced blocks, including documents with decorative headings.
    // Reject mixed executable/prose input rather than silently dropping either set of components.
    let mut structured = Guide::default();
    let mut outside = String::new();
    for (index, block) in text.split("```").enumerate() {
        if index % 2 == 0 {
            outside.push_str(block);
        } else if let Some((lang, content)) = block.split_once('\n')
            && matches!(lang.trim(), "toml" | "json" | "yaml" | "yml" | "xml")
        {
            let parsed = Guide::parse(content, lang.trim())?;
            structured.components.extend(parsed.components);
            structured.metadata.extend(parsed.metadata);
        }
    }
    if !structured.components.is_empty() {
        ensure!(
            fields(&format!("\n{outside}")).is_empty()
                && !outside.to_ascii_lowercase().contains("<!--<<modsync>>"),
            "Mixed prose components and fenced instruction guides need separate documents"
        );
        return Ok(structured);
    }
    let mut sections = Vec::new();
    let mut start = None;
    let mut offset = 0;
    let mut comment = false;
    let mut fence: Option<(char, usize)> = None;
    let mod_list = text.find("## Mod List");
    let mut end = text.len();
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        let protected = comment || fence.is_some();
        if !protected {
            if mod_list.is_some_and(|i| offset > i) && line.starts_with("## ") {
                end = offset;
                break;
            }
            if line.starts_with("### ") || line.starts_with("## ") || trimmed == "___" {
                if let Some(begin) = start.take() {
                    sections.push(&text[begin..offset]);
                }
                if line.starts_with("### ") && mod_list.is_none_or(|i| offset >= i) {
                    start = Some(offset);
                }
            }
        }
        if !comment && let Some(ch @ ('`' | '~')) = trimmed.chars().next() {
            let count = trimmed.chars().take_while(|c| *c == ch).count();
            if let Some((open, width)) = fence {
                if ch == open && count >= width && trimmed[count..].trim().is_empty() {
                    fence = None;
                }
            } else if count >= 3 {
                fence = Some((ch, count));
            }
        }
        if fence.is_none() {
            if line.contains("<!--") {
                comment = true;
            }
            if line.contains("-->") {
                comment = false;
            }
        }
        offset += line.len();
    }
    if let Some(begin) = start {
        sections.push(&text[begin..end]);
    }
    ensure!(
        !sections.is_empty(),
        "Markdown has no component sections or structured guide blocks"
    );
    let mut guide = Guide::default();
    let mut masters = Vec::new();
    for (index, section) in sections.iter().enumerate() {
        let (component, dependencies) = component(section, index)?;
        guide.components.push(component);
        masters.push(dependencies);
    }
    let mut names = BTreeMap::new();
    for c in &guide.components {
        names.insert(c.name.to_lowercase(), c.guid.clone());
        if let Some(heading) = c.metadata.get("Heading").and_then(Value::as_str) {
            names
                .entry(heading.to_lowercase())
                .or_insert(c.guid.clone());
        }
    }
    for (c, masters) in guide.components.iter_mut().zip(masters) {
        for name in masters
            .split([',', ';', '+', '&'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if let Some(id) = names.get(&name.to_lowercase()) {
                if !c.dependencies.contains(id) {
                    c.dependencies.push(id.clone());
                }
            } else {
                c.metadata
                    .entry("UnresolvedMasters".into())
                    .or_insert(json!([]))
                    .as_array_mut()
                    .unwrap()
                    .push(name.into());
            }
        }
    }
    if let Some(index) = mod_list {
        guide
            .metadata
            .insert("preambleContent".into(), text[..index].trim_end().into());
    }
    if end < text.len() {
        guide
            .metadata
            .insert("epilogueContent".into(), text[end..].trim_start().into());
    }
    Ok(guide)
}

fn component(section: &str, index: usize) -> Result<(Component, String)> {
    let mut prose = section.to_owned();
    let mut metadata = None;
    if let Some(start) = prose.to_ascii_lowercase().find("<!--<<modsync>>") {
        let body = start + "<!--<<ModSync>>".len();
        let end = body
            + prose[body..]
                .find("-->")
                .context("Unclosed ModSync metadata comment")?;
        metadata = Some(prose[body..end].trim().to_owned());
        prose.replace_range(start..end + 3, "");
        ensure!(
            !prose.to_ascii_lowercase().contains("<!--<<modsync>>"),
            "Multiple ModSync metadata comments in one component"
        );
    }
    let heading = prose
        .lines()
        .next()
        .context("Missing Markdown heading")?
        .trim_start_matches('#')
        .trim();
    let fields = fields(&prose);
    let mut value = json!({"Guid": generated_id(section, index), "Name": heading});
    let object = value.as_object_mut().unwrap();
    for (key, value) in &fields {
        if !matches!(*key, "Masters" | "CategoryTier" | "Language") {
            object.insert((*key).into(), value.clone().into());
        }
    }
    if let Some(name) = fields.get("Name") {
        object.insert("NameFieldContent".into(), name.clone().into());
        let links = links(name);
        if let Some((label, _)) = links.first().filter(|_| name.starts_with('[')) {
            object.insert("Name".into(), (*label).into());
        }
        let registry: BTreeMap<_, _> = links
            .into_iter()
            .map(|(_, url)| (url, json!({"Files": {}, "HandlerMetadata": {}})))
            .collect();
        object.insert("ResourceRegistry".into(), serde_json::to_value(registry)?);
    }
    if object["Name"].as_str() != Some(heading) {
        object.insert("Heading".into(), heading.into());
    }
    if let Some((category, tier)) = fields.get("CategoryTier").and_then(|s| s.split_once('/')) {
        object.insert(
            "Category".into(),
            category
                .split([',', '&'])
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .into(),
        );
        object.insert("Tier".into(), tier.trim().into());
    }
    if let Some(language) = fields.get("Language") {
        object.insert("Language".into(), json!([language]));
    }
    if let Some(metadata) = metadata {
        let parsed: Value = if metadata.contains("[[thisMod]]")
            || metadata.contains("[thisMod]")
            || metadata.lines().any(|line| {
                line.split_once('=')
                    .is_some_and(|(key, _)| key.trim() == "Guid")
            }) {
            toml::from_str(&metadata).context("Parse Markdown TOML metadata")?
        } else {
            serde_yaml_ng::from_str(&metadata)
                .context("Parse Markdown YAML metadata (legacy bullet metadata is not supported)")?
        };
        let source = Guide::from_value(parsed).context("Invalid Markdown component metadata")?;
        ensure!(
            source.components.len() == 1,
            "A Markdown section must contain exactly one metadata component"
        );
        let source = serde_json::to_value(&source.components[0])?;
        for key in [
            "Guid",
            "IsSelected",
            "Instructions",
            "Options",
            "Dependencies",
            "Restrictions",
            "Category",
            "Language",
            "Author",
            "Description",
            "Tier",
            "ResourceRegistry",
            "Heading",
        ] {
            if let Some(item) = source.get(key) {
                let empty = item.is_null()
                    || item.as_str().is_some_and(str::is_empty)
                    || item.as_array().is_some_and(Vec::is_empty)
                    || item.as_object().is_some_and(serde_json::Map::is_empty);
                if !empty
                    && (key != "Heading"
                        || (!object.contains_key("Heading") && item != &object["Name"]))
                {
                    object.insert(key.into(), item.clone());
                }
            }
        }
    }
    let language = value.get("Language").cloned();
    let parsed = Guide::from_value(value)?;
    let mut component = parsed.components.into_iter().next().unwrap();
    // Prose language notes are one description; structured metadata was normalized above.
    if let Some(language) = language {
        component.metadata.insert("Language".into(), language);
    }
    Ok((
        component,
        fields.get("Masters").cloned().unwrap_or_default(),
    ))
}

fn fields(prose: &str) -> BTreeMap<&'static str, String> {
    let mut result = BTreeMap::<&str, String>::new();
    let mut current = None;
    let mut admonition = false;
    for original in prose.lines().skip(1) {
        let line = original.trim();
        if line.starts_with(":::") {
            admonition = line != ":::";
            current = None;
            continue;
        }
        if !admonition && (line.starts_with('#') || line == "___" || line == "---") {
            current = None;
            continue;
        }
        let plain = line.strip_prefix("**").unwrap_or(line);
        let field = FIELDS.iter().find_map(|(title, key)| {
            if admonition && current.is_some() {
                return None;
            }
            let prefix = plain.get(..title.len())?;
            if !prefix.eq_ignore_ascii_case(title) {
                return None;
            }
            let rest = &plain[title.len()..];
            if let Some(rest) = rest.strip_prefix(':') {
                Some((*key, rest.strip_prefix("**").unwrap_or(rest).trim()))
            } else if rest.is_empty() {
                Some((*key, ""))
            } else {
                None
            }
        });
        if let Some((key, content)) = field {
            // Reference uses the first matching field.
            current = if result.contains_key(key) {
                None
            } else {
                Some(key)
            };
            result.entry(key).or_insert_with(|| content.to_owned());
            if matches!(
                key,
                "Name" | "Author" | "Masters" | "CategoryTier" | "Language" | "InstallationMethod"
            ) {
                current = None;
            }
        } else if !admonition && line.starts_with("**") && line.contains(":**") {
            current = None;
        } else if let Some(key) = current {
            let value = result.get_mut(key).unwrap();
            let line = line
                .strip_prefix(':')
                .map(str::trim_start)
                .unwrap_or(original);
            if !value.is_empty() {
                value.push('\n');
            }
            value.push_str(line);
        }
    }
    for value in result.values_mut() {
        *value = value.trim().to_owned();
    }
    result
}

fn links(mut text: &str) -> Vec<(&str, &str)> {
    let mut result = Vec::new();
    while let Some(start) = text.find('[') {
        text = &text[start + 1..];
        let Some((label, rest)) = text.split_once("](") else {
            break;
        };
        let Some((url, rest)) = rest.split_once(')') else {
            break;
        };
        result.push((label, url.trim()));
        text = rest;
    }
    result
}

pub(crate) fn generated_id(section: &str, index: usize) -> String {
    // Stable UUIDv8 for unannotated prose; C# generates random component GUIDs.
    let mut hash = Sha256::new();
    hash.update((index as u64).to_le_bytes());
    hash.update(section.as_bytes());
    let mut bytes: [u8; 16] = hash.finalize()[..16].try_into().unwrap();
    bytes[6] = (bytes[6] & 15) | 0x80;
    bytes[8] = (bytes[8] & 63) | 0x80;
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}
