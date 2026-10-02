//! Compatibility for reference GUI preload switches and convert --merge.
//! Unrelated modern subcommands are untouched. This performs no I/O or fetching.
use anyhow::{Result, bail, ensure};
use std::{collections::BTreeMap, ffi::OsString};

/// Normalize top-level legacy GUI arguments before `Cli::parse_from`.
/// Keep argv[0] verbatim. Recognized preload values use the reference's equals
/// form and quote trimming; unknown or mixed arguments never gain GUI authority.
pub fn normalize_args(arguments: Vec<OsString>) -> Result<Vec<OsString>> {
    ensure!(
        !arguments.is_empty(),
        "Command line is missing its executable name"
    );
    let Some(first) = arguments.get(1) else {
        return Ok(arguments);
    };
    let mut verb = 1;
    while arguments
        .get(verb)
        .and_then(|value| value.to_str())
        .is_some_and(|value| matches!(value, "-v" | "--verbose" | "--plaintext"))
    {
        verb += 1;
    }
    if arguments.get(verb).is_some_and(|value| value == "convert") {
        return normalize_convert_merge(arguments, verb);
    }
    if first
        .to_str()
        .is_none_or(|text| !text.starts_with('-') && !protocol(text))
    {
        return Ok(arguments);
    }
    let legacy = arguments.iter().skip(1).any(|argument| {
        argument.to_str().is_some_and(|text| {
            let name = text.split_once('=').map_or(text, |(name, _)| name);
            legacy_name(name).is_some()
        })
    });
    if !legacy {
        return Ok(arguments);
    }

    let mut fields = BTreeMap::<&'static str, String>::new();
    let mut multiple = false;
    for argument in arguments.iter().skip(1) {
        let text = argument
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Legacy GUI arguments must be valid Unicode"))?;
        if protocol(text) {
            insert(&mut fields, "--protocol", trim_value(text.trim()))?;
            continue;
        }
        let (name, value) = text
            .split_once('=')
            .map_or((text, None), |(name, value)| (name, Some(value)));
        let Some(target) = legacy_name(name) else {
            // Do not echo unknown arguments: they may contain URL/API secrets.
            bail!(
                "Legacy GUI preload arguments cannot be mixed with unknown or modern arguments; use the gui subcommand explicitly"
            );
        };
        if target == "--allow-multiple-instances" {
            ensure!(
                value.is_none() && !multiple,
                "Legacy allow-multiple-instances must appear once without a value"
            );
            multiple = true;
        } else {
            let value = value.ok_or_else(|| {
                anyhow::anyhow!("Legacy GUI preload values require --name=value syntax")
            })?;
            insert(&mut fields, target, trim_value(value))?;
        }
    }
    let mut normalized = vec![arguments[0].clone(), "gui".into()];
    for (name, value) in fields {
        // A single equals argument prevents an authored value beginning with '-'
        // from becoming another clap option. No shell interpretation occurs.
        normalized.push(format!("{name}={value}").into());
    }
    if multiple {
        normalized.push("--allow-multiple-instances".into());
    }
    Ok(normalized)
}

fn legacy_name(name: &str) -> Option<&'static str> {
    if !name.starts_with('-') {
        return None;
    }
    let name = name.trim_start_matches('-').to_ascii_lowercase();
    match name.as_str() {
        "instructionfile" => Some("--input"),
        "kotorpath" => Some("--game-dir"),
        "moddirectory" => Some("--source-dir"),
        "nxm" | "modsync" => Some("--protocol"),
        "allow-multiple-instances" => Some("--allow-multiple-instances"),
        _ => None,
    }
}
fn trim_value(value: &str) -> String {
    value.trim_matches(['\'', '"']).to_owned()
}
fn protocol(value: &str) -> bool {
    let value = value.trim().trim_matches(['\'', '"']);
    value
        .get(..6)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("nxm://"))
        || value
            .get(..10)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("modsync://"))
}
fn insert(
    fields: &mut BTreeMap<&'static str, String>,
    name: &'static str,
    value: String,
) -> Result<()> {
    ensure!(
        !value.trim().is_empty(),
        "Legacy GUI preload value cannot be empty"
    );
    if name == "--protocol" {
        ensure!(
            protocol(&value),
            "Legacy protocol preload needs an nxm:// or modsync:// URL"
        );
    }
    ensure!(
        fields.insert(name, value).is_none(),
        "Duplicate or ambiguous legacy GUI preload target {name}"
    );
    Ok(())
}

/// Convert's legacy merge spelling delegates to the normal Merge parser and
/// dispatcher. Only named sources are accepted, so positional conversion input
/// cannot accidentally become a merge source. Values are never scanned as flags.
fn normalize_convert_merge(arguments: Vec<OsString>, verb: usize) -> Result<Vec<OsString>> {
    let mut marker = None;
    let mut index = verb + 1;
    while index < arguments.len() {
        let text = arguments[index].to_str().unwrap_or("");
        if text == "--" {
            break;
        }
        if matches!(text, "--merge" | "-m") {
            ensure!(marker.is_none(), "Specify the convert merge flag only once");
            marker = Some(index);
        } else if conversion_value_flag(text) {
            // A value that happens to spell --merge is data, not routing authority.
            index += 1;
        }
        index += 1;
    }
    let Some(marker) = marker else {
        return Ok(arguments);
    };
    let mut normalized = arguments[..verb].to_vec();
    normalized.push("merge".into());
    let mut named_existing = false;
    let mut named_incoming = false;
    let mut help = false;
    let mut index = verb + 1;
    while index < arguments.len() {
        if index == marker {
            index += 1;
            continue;
        }
        let text = arguments[index]
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Expected a named option in convert merge mode"))?;
        let (name, inline) = text.split_once('=').map_or_else(
            || {
                if text.len() > 2
                    && let Some(short) = text.get(..2)
                    && conversion_value_flag(short)
                {
                    (short, true)
                } else {
                    (text, false)
                }
            },
            |(name, _)| (name, true),
        );
        ensure!(
            !matches!(
                name,
                "--input" | "-i" | "--stdin" | "--parse-directions" | "--review-draft"
            ),
            "Convert merge mode cannot be combined with conversion input or draft-review options; use --existing and --incoming"
        );
        ensure!(
            name.starts_with('-') && name != "--",
            "Convert merge mode requires named --existing, --incoming and optional --output arguments"
        );
        help |= matches!(name, "--help" | "-h") && !inline;
        named_existing |= matches!(name, "--existing" | "-e");
        named_incoming |= matches!(name, "--incoming" | "-n");
        normalized.push(arguments[index].clone());
        if conversion_value_flag(name) && !inline {
            index += 1;
            let value = arguments.get(index).ok_or_else(|| {
                anyhow::anyhow!("Missing named option value in convert merge mode")
            })?;
            // Preserve the exact OsString, including non-Unicode filesystem paths.
            normalized.push(value.clone());
        }
        index += 1;
    }
    ensure!(
        help || (named_existing && named_incoming),
        "Convert merge mode requires --existing/-e and --incoming/-n"
    );
    Ok(normalized)
}

fn conversion_value_flag(name: &str) -> bool {
    matches!(
        name,
        "--input"
            | "-i"
            | "--output"
            | "-o"
            | "--existing"
            | "-e"
            | "--incoming"
            | "-n"
            | "--format"
            | "-f"
            | "--select"
            | "-s"
            | "--source-path"
            | "--nexus-mods-api-key"
            | "--settings-dir"
            | "--spoiler-free"
            | "--fomod-choices"
            | "--review-draft"
    )
}
