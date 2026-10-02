//! Strict, bounded reader for the reference's legacy Markdown metadata bullets.
//! Paths deliberately pass through the ordinary Guide normalization afterwards.
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use serde_json::{Map, Value};

/// Select legacy only from its first nonblank line, never as a structured-parser
/// fallback. Mixed structured/bullet input fails in whichever grammar starts it.
pub(super) fn recognized(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .is_some_and(|line| {
            line.starts_with("- **GUID:**") || matches!(line, "#### Instructions" | "#### Options")
        })
}

pub(super) fn parse(text: &str, default_guid: &str) -> Result<Value> {
    let lines: Vec<(usize, &str)> = text
        .lines()
        .enumerate()
        .map(|(n, line)| (n + 1, line.trim()))
        .filter(|(_, line)| !line.is_empty())
        .collect();
    let instruction_header = Regex::new(r"^\d+\.\s+\*\*GUID:\*\*")?;
    let option_header = Regex::new(r"(?i)^#+\s+Option\s+\d+")?;
    let mut component = Map::new();
    let mut instructions = Vec::new();
    let mut options = Vec::new();
    let mut at = 0;
    while let Some(&(number, line)) = lines.get(at) {
        if matches!(line, "#### Instructions" | "#### Options") {
            break;
        }
        let value = property(line, "- **", "GUID")
            .with_context(|| format!("Unsupported legacy component field at line {number}"))?;
        put(&mut component, "Guid", guid(value, number)?, number)?;
        at += 1;
    }
    if lines
        .get(at)
        .is_some_and(|(_, line)| *line == "#### Instructions")
    {
        at += 1;
        while let Some(&(number, line)) = lines.get(at) {
            if line == "#### Options" {
                break;
            }
            ensure!(
                instruction_header.is_match(line),
                "Expected legacy instruction GUID header at line {number}"
            );
            at += 1; // Obsolete instruction GUID payload is deliberately ignored.
            let mut instruction = Map::new();
            while let Some(&(number, line)) = lines.get(at) {
                if line.starts_with('#') || instruction_header.is_match(line) {
                    break;
                }
                instruction_field(&mut instruction, line, "**", number)?;
                at += 1;
            }
            finish_instruction(&instruction, number)?;
            instructions.push(Value::Object(instruction));
        }
    }
    if lines
        .get(at)
        .is_some_and(|(_, line)| *line == "#### Options")
    {
        at += 1;
        while let Some(&(number, line)) = lines.get(at) {
            ensure!(
                option_header.is_match(line),
                "Expected legacy option heading at line {number}"
            );
            at += 1;
            let mut option = Map::new();
            let mut nested = Vec::new();
            while let Some(&(number, line)) = lines.get(at) {
                if option_header.is_match(line) {
                    break;
                }
                if line.starts_with("- **Instruction:**") {
                    at += 1;
                    let mut instruction = Map::new();
                    while let Some(&(number, line)) = lines.get(at) {
                        if option_header.is_match(line) || line.starts_with("- **Instruction:**") {
                            break;
                        }
                        // Reference option properties terminate an instruction block.
                        if option_property(line) {
                            break;
                        }
                        if line.starts_with("- **GUID:**") {
                            // Legacy instruction IDs are obsolete, including in options.
                            at += 1;
                            continue;
                        }
                        instruction_field(&mut instruction, line, "- **", number)?;
                        at += 1;
                    }
                    finish_instruction(&instruction, number)?;
                    nested.push(Value::Object(instruction));
                    continue;
                }
                let (key, raw) = option_field(line).with_context(|| {
                    format!("Unsupported legacy option field or nesting at line {number}")
                })?;
                let value = match key {
                    "Guid" => guid(raw, number)?,
                    "IsSelected" | "IsDownloaded" => boolean(raw, number)?,
                    "InstallState" => {
                        let state: i32 = raw
                            .parse()
                            .with_context(|| format!("Invalid Install State at line {number}"))?;
                        ensure!(
                            (0..=5).contains(&state),
                            "Unknown Install State at line {number}"
                        );
                        state.into()
                    }
                    "Restrictions" => Value::Array(
                        list(raw)
                            .into_iter()
                            .map(|v| guid(&v, number))
                            .collect::<Result<_>>()?,
                    ),
                    _ => raw.into(),
                };
                put(&mut option, key, value, number)?;
                at += 1;
            }
            option
                .entry("Guid")
                .or_insert_with(|| uuid::Uuid::new_v4().to_string().into());
            option.insert("Instructions".into(), nested.into());
            options.push(Value::Object(option));
        }
    }
    ensure!(
        at == lines.len(),
        "Unexpected or reversed legacy section order"
    );
    component
        .entry("Guid")
        .or_insert_with(|| default_guid.into());
    component.insert("Instructions".into(), instructions.into());
    component.insert("Options".into(), options.into());
    Ok(component.into())
}

fn property<'a>(line: &'a str, prefix: &str, label: &str) -> Option<&'a str> {
    line.strip_prefix(&format!("{prefix}{label}:**"))
        .map(str::trim)
}

fn option_field(line: &str) -> Option<(&'static str, &str)> {
    [
        ("GUID", "Guid"),
        ("Name", "Name"),
        ("Description", "Description"),
        ("Is Selected", "IsSelected"),
        ("Install State", "InstallState"),
        ("Is Downloaded", "IsDownloaded"),
        ("Restrictions", "Restrictions"),
    ]
    .into_iter()
    .find_map(|(label, key)| property(line, "- **", label).map(|v| (key, v)))
}

fn option_property(line: &str) -> bool {
    option_field(line).is_some_and(|(key, _)| key != "Guid")
}

fn instruction_field(
    map: &mut Map<String, Value>,
    line: &str,
    prefix: &str,
    number: usize,
) -> Result<()> {
    for key in ["Action", "Overwrite", "Source", "Destination"] {
        if let Some(raw) = property(line, prefix, key) {
            let value = match key {
                "Action" => {
                    let action = [
                        "Extract",
                        "Execute",
                        "Patcher",
                        "Move",
                        "Copy",
                        "Rename",
                        "Delete",
                        "DelDuplicate",
                        "Choose",
                        "Run",
                        "CleanList",
                    ]
                    .into_iter()
                    .find(|v| v.eq_ignore_ascii_case(raw))
                    .with_context(|| format!("Invalid or unset legacy Action at line {number}"))?;
                    action.into()
                }
                "Overwrite" => boolean(raw, number)?,
                "Source" => list(raw).into(),
                _ => raw.into(),
            };
            return put(map, key, value, number);
        }
    }
    bail!("Unsupported legacy instruction field at line {number}")
}

fn finish_instruction(map: &Map<String, Value>, number: usize) -> Result<()> {
    let action = map
        .get("Action")
        .and_then(Value::as_str)
        .with_context(|| format!("Legacy instruction at line {number} has no Action"))?;
    if action != "DelDuplicate" {
        ensure!(
            map.get("Source")
                .and_then(Value::as_array)
                .is_some_and(|v| !v.is_empty()),
            "Legacy instruction at line {number} has no Source"
        );
    }
    if matches!(action, "Copy" | "Move" | "Rename") {
        ensure!(
            map.get("Destination")
                .and_then(Value::as_str)
                .is_some_and(|v| !v.is_empty()),
            "Legacy {action} at line {number} has no Destination"
        );
    }
    Ok(())
}

fn list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect()
}

fn guid(raw: &str, number: usize) -> Result<Value> {
    Ok(uuid::Uuid::parse_str(raw)
        .with_context(|| format!("Invalid legacy GUID at line {number}"))?
        .to_string()
        .into())
}

fn boolean(raw: &str, number: usize) -> Result<Value> {
    if raw.eq_ignore_ascii_case("true") {
        Ok(true.into())
    } else if raw.eq_ignore_ascii_case("false") {
        Ok(false.into())
    } else {
        bail!("Invalid legacy boolean at line {number}")
    }
}

fn put(map: &mut Map<String, Value>, key: &str, value: Value, number: usize) -> Result<()> {
    ensure!(
        map.get(key).is_none_or(|old| old == &value),
        "Contradictory legacy {key} at line {number}"
    );
    map.insert(key.into(), value);
    Ok(())
}
