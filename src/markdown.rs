use crate::{Component, Guide};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

mod extract;

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
                    sections.push((begin, &text[begin..offset]));
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
        sections.push((begin, &text[begin..end]));
    }
    ensure!(
        !sections.is_empty(),
        "Markdown has no component sections or structured guide blocks"
    );
    let mut guide = Guide::default();
    let mut masters = Vec::new();
    let mut positions = Vec::new();
    for (index, (position, section)) in sections.iter().enumerate() {
        // The reference drops sections with neither a Name field nor a heading.
        let Some((mut component, dependencies)) = component(section, index)? else {
            continue;
        };
        component
            .metadata
            .insert("WidescreenOnly".into(), false.into());
        guide.components.push(component);
        masters.push(dependencies);
        positions.push(*position);
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
    // Reference author-possessive fallback: "JC's Cloaked Jedi Robes" resolves
    // to the component named "Cloaked Jedi Robes" whose author contains "JC".
    let authored: Vec<_> = guide
        .components
        .iter()
        .map(|c| {
            (
                c.name.to_lowercase(),
                c.author.to_lowercase(),
                c.guid.clone(),
            )
        })
        .collect();
    let possessive = |name: &str| {
        let (author, component) = name.split_once("'s ")?;
        let (author, component) = (
            author.trim().to_lowercase(),
            component.trim().to_lowercase(),
        );
        let alias = match author.as_str() {
            "jc" => Some("jcarter426"),
            "jcarter426" => Some("jc"),
            _ => None,
        };
        authored
            .iter()
            .find(|(name, owner, _)| {
                *name == component
                    && !author.is_empty()
                    && !owner.trim().is_empty()
                    && (owner.contains(&author) || alias.is_some_and(|a| owner.contains(a)))
            })
            .map(|(_, _, id)| id.clone())
    };
    for (c, masters) in guide.components.iter_mut().zip(masters) {
        for name in masters
            .split([',', ';', '+', '&'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if let Some(id) = names
                .get(&name.to_lowercase())
                .cloned()
                .or_else(|| possessive(name))
            {
                if !c.dependencies.contains(&id) {
                    c.dependencies.push(id);
                    c.metadata
                        .entry("DependencyNames".into())
                        .or_insert(json!([]))
                        .as_array_mut()
                        .unwrap()
                        .push(name.into());
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
    // The reference extracts this from the complete document, even when its
    // Mod List boundary excludes widescreen components from parsing.
    if let Some(header) = text.to_ascii_lowercase().find("## optional widescreen") {
        let mut position = header;
        let mut first_mod = None;
        let mut next_section = None;
        for line in text[header..].split_inclusive('\n') {
            if first_mod.is_none()
                && line
                    .strip_prefix("###")
                    .is_some_and(|s| s.starts_with(char::is_whitespace))
            {
                first_mod = Some(position);
            }
            if position > header
                && line.strip_prefix("##").is_some_and(|s| {
                    s.starts_with(char::is_whitespace) && !s.trim_start().starts_with('#')
                })
                && next_section.is_none()
            {
                next_section = Some(position);
            }
            position += line.len();
        }
        if let Some(first_mod) = first_mod {
            for (component, position) in guide.components.iter_mut().zip(&positions) {
                component.metadata.insert(
                    "WidescreenOnly".into(),
                    (*position >= first_mod && next_section.is_none_or(|end| *position < end))
                        .into(),
                );
            }
            guide.metadata.insert(
                "widescreenWarningContent".into(),
                text[header..first_mod].trim().into(),
            );
        }
        if let Some(next_section) = next_section {
            end = next_section;
        }
    }
    if end < text.len() {
        guide
            .metadata
            .insert("epilogueContent".into(), text[end..].trim_start().into());
    }
    Ok(guide)
}

fn component(section: &str, index: usize) -> Result<Option<(Component, String)>> {
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
    if metadata.is_some() {
        prose = prose.trim().to_owned();
    }
    let text = prose.as_str();
    // Each field is the reference's first (leftmost) pattern match in the section.
    let heading = extract::heading(text);
    let Some(name) = extract::name(text).or_else(|| heading.clone()) else {
        return Ok(None);
    };
    let mut value = json!({"Guid": generated_id(section, index), "Name": name});
    let object = value.as_object_mut().unwrap();
    for (key, title) in [
        ("Author", "Author"),
        ("InstallationMethod", "Installation Method"),
    ] {
        if let Some(found) = extract::line_field(text, title) {
            object.insert(key.into(), found.into());
        }
    }
    for (field, key) in extract::Field::ALL {
        if let Some(found) = extract::field(text, field) {
            object.insert(key.into(), found.into());
        }
    }
    if let Some(content) = extract::name_field(text) {
        if extract::name(text).is_some() {
            object.insert("NameFieldContent".into(), content.into());
        }
        let registry: serde_json::Map<_, _> = links(content)
            .into_iter()
            .map(|url| (url, json!({"Files": {}, "HandlerMetadata": {}})))
            .collect();
        object.insert("ResourceRegistry".into(), serde_json::to_value(registry)?);
    }
    if let Some(heading) = heading.filter(|heading| *heading != name) {
        object.insert("Heading".into(), heading.into());
    }
    if let Some((category, tier)) = extract::category_tier(text) {
        object.insert("Category".into(), category.into());
        object.insert("Tier".into(), tier.into());
    }
    if let Some(language) = extract::line_field(text, "Non-English Functionality") {
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
    Ok(Some((
        component,
        extract::line_field(text, "Masters").unwrap_or_default(),
    )))
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

/// Reference `ModLinkPattern` matches: `[label](link)`, trimmed, non-empty,
/// case-insensitively distinct in first-seen order.
fn links(text: &str) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    let mut at = 0;
    while let Some(open) = text[at..].find('[').map(|i| at + i) {
        at = open + 1;
        let Some(close) = text[at..].find(']').map(|i| at + i) else {
            break;
        };
        if close == at || !text[close..].starts_with("](") {
            continue;
        }
        let Some(end) = text[close + 2..].find(')').map(|i| close + 2 + i) else {
            continue;
        };
        if end == close + 2 {
            continue;
        }
        let url = text[close + 2..end].trim();
        if !url.is_empty() && !result.iter().any(|seen| seen.eq_ignore_ascii_case(url)) {
            result.push(url.to_owned());
        }
        at = end + 1;
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
