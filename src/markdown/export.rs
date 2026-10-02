//! Public Markdown layout, following the reference documentation formatter.
use crate::{Component, Guide};
use anyhow::{Result, ensure};
use serde_json::Value;

const DEFAULT_WIDESCREEN_WARNING: &str =
    "Please install manually the widescreen implementations, e.g. uniws, before continuing.";

fn nonblank(value: &str) -> bool {
    !value.trim().is_empty()
}

fn component_text<'a>(component: &'a Component, key: &str) -> &'a str {
    component
        .metadata
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn content<'a>(metadata: &'a Value, key: &str) -> &'a str {
    let pascal = format!("{}{}", key[..1].to_ascii_uppercase(), &key[1..]);
    metadata
        .get(&pascal)
        .or_else(|| metadata.get(key))
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn field(output: &mut String, name: &str, value: &str, before: bool, after: bool) {
    if !nonblank(value) {
        return;
    }
    if before {
        output.push('\n');
    }
    output.push_str("**");
    output.push_str(name);
    output.push_str(":** ");
    output.push_str(value);
    output.push('\n');
    if after {
        output.push('\n');
    }
}

fn language(component: &Component) -> String {
    let languages: Vec<&str> = match component.metadata.get("Language") {
        Some(Value::String(text)) => vec![text],
        Some(Value::Array(values)) => values.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    let has = |text: &str| languages.iter().any(|lang| lang.eq_ignore_ascii_case(text));
    if languages.is_empty() || (languages.len() == 1 && has("UNKNOWN")) {
        return String::new();
    }
    if has("All") || has("YES") || has("Universal") {
        return "YES".into();
    }
    if languages.len() == 1 && (has("English") || has("EN") || has("NO")) {
        return "NO".into();
    }
    if languages
        .iter()
        .any(|lang| lang.to_ascii_lowercase().contains("partial"))
    {
        return "PARTIAL - Some text will be blank or in English".into();
    }
    if languages.len() > 1 && has("English") {
        return format!("PARTIAL - Supported languages: {}", languages.join(", "));
    }
    if languages.len() == 1 {
        let upper = languages[0].trim_start().to_ascii_uppercase();
        if upper.starts_with("YES")
            || upper.starts_with("NO")
            || upper.starts_with("PARTIAL")
            || upper.contains("ONLY")
        {
            return languages[0].into();
        }
    }
    format!("Supported languages: {}", languages.join(", "))
}

fn masters(guide: &Guide, component: &Component) -> String {
    let original = component
        .metadata
        .get("DependencyGuidToOriginalName")
        .and_then(Value::as_object);
    // Use positional original spellings only when the importer supplied the full
    // list. Embedded dependencies can otherwise make this a partial name list.
    let names = component
        .metadata
        .get("DependencyNames")
        .and_then(Value::as_array)
        .filter(|names| names.len() == component.dependencies.len());
    component
        .dependencies
        .iter()
        .enumerate()
        .filter_map(|(index, guid)| {
            original
                .and_then(|map| map.iter().find(|(key, _)| key.eq_ignore_ascii_case(guid)))
                .and_then(|(_, value)| value.as_str())
                .or_else(|| {
                    names
                        .and_then(|names| names.get(index))
                        .and_then(Value::as_str)
                })
                .or_else(|| {
                    guide
                        .components
                        .iter()
                        .find(|c| c.guid.eq_ignore_ascii_case(guid))
                        .map(|c| c.name.as_str())
                })
                .filter(|name| nonblank(name))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn validate_prose(text: &str) -> Result<()> {
    // component() searches for this exact marker using ASCII-insensitive
    // matching, including inside code fences and ordinary HTML comments.
    ensure!(
        !text.to_ascii_lowercase().contains("<!--<<modsync>>"),
        "Markdown prose contains an executable ModSync metadata marker"
    );
    Ok(())
}

fn reject_structured_fences(text: &str) -> Result<()> {
    // parse_with_selection_default scans triple-backtick blocks before handling
    // HTML comments or prose sections. Reject every supported opener regardless
    // of fence parity: an earlier prose field can change that parity.
    for block in text.split("```").skip(1) {
        if let Some((language, _)) = block.split_once('\n') {
            ensure!(
                !matches!(language.trim(), "toml" | "json" | "yaml" | "yml" | "xml"),
                "Markdown export contains a fenced executable guide"
            );
        }
    }
    Ok(())
}

fn prepare_metadata(value: &mut Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    if object
        .get("Heading")
        .and_then(Value::as_str)
        .is_some_and(|heading| !nonblank(heading))
    {
        object.remove("Heading");
    }
    object.remove("_HasInstructions");
    object.remove("OptionsInstructions");
    for key in [
        "Author",
        "Description",
        "Directions",
        "Tier",
        "Category",
        "Dependencies",
        "Restrictions",
        "InstallBefore",
        "InstallAfter",
        "Instructions",
        "Options",
        "ResourceRegistry",
    ] {
        if object.get(key).is_some_and(|value| {
            value.as_str().is_some_and(|text| !nonblank(text))
                || value.as_array().is_some_and(Vec::is_empty)
                || value.as_object().is_some_and(serde_json::Map::is_empty)
        }) {
            object.remove(key);
        }
    }
    if let Some(instructions) = object.get_mut("Instructions").and_then(Value::as_array_mut) {
        for instruction in instructions {
            if let Some(action) = instruction.get_mut("Action")
                && let Some(text) = action.as_str()
            {
                *action = text.to_ascii_lowercase().into();
            }
            if let Some(instruction) = instruction.as_object_mut() {
                for key in [
                    "Source",
                    "Destination",
                    "Arguments",
                    "Dependencies",
                    "Restrictions",
                ] {
                    if instruction.get(key).is_some_and(|value| {
                        value.as_str().is_some_and(|text| !nonblank(text))
                            || value.as_array().is_some_and(Vec::is_empty)
                    }) {
                        instruction.remove(key);
                    }
                }
                let action = instruction
                    .get("Action")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let default_overwrite = !action.eq_ignore_ascii_case("delete");
                if instruction.get("Overwrite").and_then(Value::as_bool) == Some(default_overwrite)
                {
                    instruction.remove("Overwrite");
                }
            }
        }
    }
    if let Some(options) = object.get_mut("Options").and_then(Value::as_array_mut) {
        for option in options {
            prepare_metadata(option);
        }
    }
}

pub(crate) fn render(guide: &Guide, metadata: &Value) -> Result<String> {
    let mut output = String::new();
    let preamble = content(metadata, "preambleContent");
    if nonblank(preamble) {
        validate_prose(preamble)?;
        output.push_str(preamble);
        if !preamble.ends_with('\n') {
            output.push('\n');
        }
        output.push('\n');
    }
    output.push_str("## Mod List\n");
    let mut aspyr_written = false;
    let mut widescreen_written = false;
    for (index, component) in guide.components.iter().enumerate() {
        let prose_start = output.len();
        for (flag, key, written) in [
            (
                "AspyrExclusive",
                "aspyrExclusiveWarningContent",
                &mut aspyr_written,
            ),
            (
                "WidescreenOnly",
                "widescreenWarningContent",
                &mut widescreen_written,
            ),
        ] {
            let warning = content(metadata, key);
            if !*written
                && component.metadata.get(flag).and_then(Value::as_bool) == Some(true)
                && nonblank(warning)
                && (flag != "WidescreenOnly" || warning != DEFAULT_WIDESCREEN_WARNING)
            {
                output.push('\n');
                output.push_str(warning.trim_end());
                output.push_str("\n\n");
                *written = true;
            }
        }
        if index > 0 {
            output.push_str("___\n");
        }
        output.push('\n');
        output.push_str("### ");
        let heading = component_text(component, "Heading");
        output.push_str(if nonblank(heading) {
            heading
        } else {
            &component.name
        });
        output.push_str("\n\n");
        let authored = component_text(component, "NameFieldContent");
        let name = if nonblank(authored) {
            authored.to_owned()
        } else if let Some(first) = component
            .resource_registry
            .keys()
            .next()
            .filter(|url| nonblank(url))
        {
            let mut name = format!("[{}]({first})", component.name);
            for url in component
                .resource_registry
                .keys()
                .skip(1)
                .filter(|url| nonblank(url))
            {
                name.push_str(&format!(" and [**Patch**]({url})"));
            }
            name
        } else {
            component.name.clone()
        };
        field(&mut output, "Name", &name, false, true);
        field(&mut output, "Author", &component.author, false, true);
        field(
            &mut output,
            "Description",
            &component.description,
            false,
            true,
        );
        field(
            &mut output,
            "Screenshots",
            component_text(component, "Screenshots"),
            false,
            true,
        );
        let category = match component.category.split_last() {
            None => "Uncategorized".into(),
            Some((last, [])) => last.clone(),
            Some((last, rest)) => format!("{} & {last}", rest.join(", ")),
        };
        let tier = if nonblank(&component.tier) {
            &component.tier
        } else {
            "Unspecified"
        };
        field(
            &mut output,
            "Category & Tier",
            &format!("{category} / {tier}"),
            false,
            true,
        );
        field(
            &mut output,
            "Non-English Functionality",
            &language(component),
            false,
            true,
        );
        field(
            &mut output,
            "Installation Method",
            component_text(component, "InstallationMethod"),
            false,
            false,
        );
        for (label, key) in [
            ("Known Bugs", "KnownBugs"),
            ("Installation Warning", "InstallationWarning"),
            ("Compatibility Warning", "CompatibilityWarning"),
            ("Steam Notes", "SteamNotes"),
        ] {
            field(
                &mut output,
                label,
                component_text(component, key),
                true,
                false,
            );
        }
        field(
            &mut output,
            "Masters",
            &masters(guide, component),
            true,
            false,
        );
        field(
            &mut output,
            "Download Instructions",
            component_text(component, "DownloadInstructions"),
            true,
            false,
        );
        field(
            &mut output,
            "Installation Instructions",
            &component.directions,
            true,
            false,
        );
        field(
            &mut output,
            "Usage Warning",
            component_text(component, "UsageWarning"),
            true,
            false,
        );
        output.push('\n');
        validate_prose(&output[prose_start..])?;
        if !component.instructions.is_empty()
            || !component.options.is_empty()
            || component.metadata.contains_key("DraftReview")
        {
            let mut value = serde_json::to_value(component)?;
            crate::draft::validate_review_records(&value)?;
            crate::model::encode_nullable_files(&mut value);
            crate::model::yaml_legacy_resources(&mut value);
            prepare_metadata(&mut value);
            let yaml = serde_yaml_ng::to_string(&value)?;
            ensure!(
                !yaml.contains("-->"),
                "Markdown metadata contains an HTML comment terminator"
            );
            output.push_str("<!--<<ModSync>>\n");
            output.push_str(&yaml);
            if !yaml.ends_with('\n') {
                output.push('\n');
            }
            output.push_str("-->\n\n");
        }
    }
    let epilogue = content(metadata, "epilogueContent");
    if nonblank(epilogue) {
        validate_prose(epilogue)?;
        output.push('\n');
        output.push_str(epilogue);
        if !epilogue.ends_with('\n') {
            output.push('\n');
        }
    }
    reject_structured_fences(&output)?;
    Ok(output)
}
