//! Compiler for an already authenticated, compiled-in original plan step.
//!
//! This module supplies no owner or archive authority and never executes a vendor
//! program. Its parent must bind the complete guide, plan, owner and source pins,
//! then independently certify every selected native configuration and payload.
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize)]
pub(in crate::archive_generation) struct Program {
    pub step_id: String,
    pub source_heading: String,
    pub effects: Vec<Effect>,
}

/// Ordinals are zero-based positions in the ORIGINAL operation array. They are
/// identity even when two original operations have otherwise identical fields.
#[derive(Clone, Debug, Serialize)]
pub(in crate::archive_generation) enum Effect {
    Extract {
        ordinal: usize,
        source: String,
        destination: String,
        sha256: String,
    },
    Copy {
        ordinal: usize,
        source: String,
        destination: String,
        skip_existing: bool,
    },
    Delete {
        ordinal: usize,
        path: String,
        missing_ok: bool,
        if_any_exists: Vec<String>,
    },
    Rename {
        ordinal: usize,
        source: String,
        destination: String,
    },
    NativeRun {
        ordinal: usize,
        patcher: String,
        sha256: String,
        cwd: String,
        namespace: Option<usize>,
    },
}
impl Effect {
    pub(in crate::archive_generation) fn ordinal(&self) -> usize {
        match self {
            Self::Extract { ordinal, .. }
            | Self::Copy { ordinal, .. }
            | Self::Delete { ordinal, .. }
            | Self::Rename { ordinal, .. }
            | Self::NativeRun { ordinal, .. } => *ordinal,
        }
    }
}

const GUI_INSTALLERS: [&str; 4] = [
    "1a267454b2efcd61232332be83f4da4ffe88bd6770c5a0df517880062b1a75e8",
    "013655cd871a71e0f5b50f80ed0d32308912916ba7e12e2a366f42d992a71b16",
    "638aaeea4e40ffb6c592f68787f2140318a0dec7f2cde85908806ce3b5bc5131",
    "69b8aa25bb70af2ad9e4468756538c1736c02cecd1da2fd24448d7fdf18fcff0",
];

pub(in crate::archive_generation) fn legacy_gui_installer(sha256: &str) -> bool {
    GUI_INSTALLERS.contains(&sha256)
}
const CLI_INSTALLERS: [&str; 4] = [
    "2a108d0ba866867da4726dedf6b17efdb23db4cbf3809856adbadf4a366cecb6",
    "7a1cc0fb30c5f3653fac36b9d8283d04c5a3db308e55c247cc1593c6ef45690f",
    "a7007ec081344f416972493879c1f57239897a4fde094c4d79c3585eb9146247",
    "5cc9fc7aeafefb8b18cd584de0ad530f5c7a057e4b0a1308ed7bc6a9e4bc3fbe",
];

fn fields(value: &Value, required: &[&str], optional: &[&str]) -> Result<()> {
    let map = value
        .as_object()
        .context("Original operation must be an object")?;
    ensure!(
        required.iter().all(|k| map.contains_key(*k)),
        "Original protocol field absent"
    );
    ensure!(
        map.keys()
            .all(|k| required.contains(&k.as_str()) || optional.contains(&k.as_str())),
        "Unsupported original protocol field"
    );
    Ok(())
}
fn string(value: &Value, key: &str) -> Result<String> {
    Ok(value
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("Original {key} absent"))?
        .to_owned())
}
fn boolean(value: &Value, key: &str) -> Result<bool> {
    match value.get(key) {
        None => Ok(false),
        Some(v) => v
            .as_bool()
            .with_context(|| format!("Original {key} is not boolean")),
    }
}
fn hash(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Original SHA256 is not canonical"
    );
    Ok(())
}
fn rooted(path: &str, roots: &[&str], step: &str) -> Result<()> {
    let (root, relative) = path
        .split_once('/')
        .context("Original path has no rooted relative member")?;
    ensure!(roots.contains(&root), "Original path has unsupported root");
    ensure!(
        !relative.is_empty() && !relative.chars().any(char::is_control),
        "Original path empty or has controls"
    );
    ensure!(
        relative.split('/').all(|p| !p.is_empty()
            && p != "."
            && p != ".."
            && !p.contains(['\\', ':', '{', '}', '*', '?'])),
        "Original path is not literal and confined"
    );
    if root == "{extract}" {
        ensure!(
            relative.starts_with(&format!("{step}/")),
            "Original extraction path crosses step boundary"
        );
    }
    Ok(())
}
fn archive_pin(path: &str, pins: &BTreeMap<String, String>, step: &str) -> Result<String> {
    rooted(path, &["{archives}"], step)?;
    let sha = pins
        .get(path)
        .context("Original archive has no pinned SHA256")?;
    hash(sha)?;
    Ok(sha.clone())
}

pub(in crate::archive_generation) fn compile(
    step: &Value,
    archive_sha256: &BTreeMap<String, String>,
) -> Result<Program> {
    fields(
        step,
        &["id", "source_heading", "operations"],
        &["timeout_seconds"],
    )?;
    let step_id = string(step, "id")?;
    ensure!(
        step_id.len() == 8
            && step_id.starts_with("step-")
            && step_id[5..].bytes().all(|b| b.is_ascii_digit())
            && step_id[5..].parse::<usize>()? > 0,
        "Original step id malformed"
    );
    let source_heading = string(step, "source_heading")?;
    ensure!(
        source_heading.starts_with("### ")
            && source_heading.len() > 4
            && !source_heading.chars().any(char::is_control),
        "Original heading malformed"
    );
    if let Some(timeout) = step.get("timeout_seconds") {
        ensure!(
            timeout.as_u64().is_some_and(|n| n > 0),
            "Original timeout malformed"
        );
    }
    let ops = step["operations"]
        .as_array()
        .context("Original operation array absent")?;
    ensure!(!ops.is_empty(), "Original operation array empty");
    let mut effects = Vec::with_capacity(ops.len());
    let mut extracted = Vec::<String>::new();
    for (ordinal, op) in ops.iter().enumerate() {
        let effect = match string(op, "type")?.as_str() {
            "extract" => {
                fields(op, &["type", "source", "destination"], &[])?;
                let source = string(op, "source")?;
                let destination = string(op, "destination")?;
                let sha256 = archive_pin(&source, archive_sha256, &step_id)?;
                rooted(&destination, &["{extract}"], &step_id)?;
                ensure!(
                    !extracted.iter().any(|p| destination == *p
                        || destination.starts_with(&format!("{p}/"))
                        || p.starts_with(&format!("{destination}/"))),
                    "Original extraction slots overlap"
                );
                extracted.push(destination.clone());
                Effect::Extract {
                    ordinal,
                    source,
                    destination,
                    sha256,
                }
            }
            "copy" => {
                fields(op, &["type", "source", "destination"], &["skip_existing"])?;
                let source = string(op, "source")?;
                let destination = string(op, "destination")?;
                rooted(&source, &["{extract}", "{archives}"], &step_id)?;
                if source.starts_with("{archives}/") {
                    archive_pin(&source, archive_sha256, &step_id)?;
                }
                rooted(&destination, &["{game}"], &step_id)?;
                Effect::Copy {
                    ordinal,
                    source,
                    destination,
                    skip_existing: boolean(op, "skip_existing")?,
                }
            }
            "delete" => {
                fields(op, &["type", "path"], &["missing_ok", "if_any_exists"])?;
                let path = string(op, "path")?;
                rooted(&path, &["{game}", "{extract}"], &step_id)?;
                let if_any_exists = match op.get("if_any_exists") {
                    None => Vec::new(),
                    Some(v) => {
                        let a = v
                            .as_array()
                            .context("Original delete predicate is not an array")?;
                        ensure!(!a.is_empty(), "Original delete predicate empty");
                        a.iter()
                            .map(|p| {
                                let p = p
                                    .as_str()
                                    .context("Original delete predicate is not a path")?;
                                rooted(p, &["{game}", "{extract}"], &step_id)?;
                                Ok(p.to_owned())
                            })
                            .collect::<Result<Vec<_>>>()?
                    }
                };
                Effect::Delete {
                    ordinal,
                    path,
                    missing_ok: boolean(op, "missing_ok")?,
                    if_any_exists,
                }
            }
            "rename" => {
                fields(op, &["type", "source", "destination"], &[])?;
                let source = string(op, "source")?;
                let destination = string(op, "destination")?;
                rooted(&source, &["{game}"], &step_id)?;
                rooted(&destination, &["{game}"], &step_id)?;
                ensure!(source != destination, "Original rename is a self-rename");
                Effect::Rename {
                    ordinal,
                    source,
                    destination,
                }
            }
            "patcher" | "gui_patcher" => native(op, ordinal, &step_id)?,
            other => bail!("Unsupported original operation {other} at {ordinal}"),
        };
        // Extraction effects must precede every reference to their private slot.
        let check = |path: &str| -> Result<()> {
            if path.starts_with("{extract}/") {
                ensure!(
                    extracted
                        .iter()
                        .any(|p| path == p || path.starts_with(&format!("{p}/"))),
                    "Original operation references an unextracted slot"
                );
            }
            Ok(())
        };
        match &effect {
            Effect::Copy { source, .. } => check(source)?,
            Effect::Delete {
                path,
                if_any_exists,
                ..
            } => {
                check(path)?;
                for p in if_any_exists {
                    check(p)?;
                }
            }
            Effect::NativeRun { patcher, cwd, .. } => {
                check(patcher)?;
                check(cwd)?;
            }
            _ => {}
        }
        effects.push(effect);
    }
    ensure!(
        effects.iter().any(|e| !matches!(e, Effect::Extract { .. })),
        "Extraction-only original plan has no executable install semantics"
    );
    Ok(Program {
        step_id,
        source_heading,
        effects,
    })
}

fn native(op: &Value, ordinal: usize, step: &str) -> Result<Effect> {
    let gui = op["type"] == "gui_patcher";
    let common = [
        "type",
        "patcher",
        "sha256",
        "argv",
        "cwd",
        "success_log",
        "success_markers",
        "failure_regex",
        "expected_changes",
    ];
    let mut required = common.to_vec();
    if gui {
        required.extend(["wine_registry", "actions"]);
    }
    fields(op, &required, &[])?;
    let patcher = string(op, "patcher")?;
    let cwd = string(op, "cwd")?;
    let sha256 = string(op, "sha256")?;
    hash(&sha256)?;
    rooted(&patcher, &["{extract}"], step)?;
    rooted(&cwd, &["{extract}"], step)?;
    ensure!(
        patcher
            .rsplit_once('/')
            .is_some_and(|(p, f)| p == cwd && f.to_ascii_lowercase().ends_with(".exe")),
        "Original native installer is outside its working directory"
    );
    ensure!(
        op["expected_changes"] == "changed_manifest",
        "Unsupported original change observation"
    );
    let namespace = if gui {
        ensure!(
            GUI_INSTALLERS.contains(&sha256.as_str())
                && op["argv"] == json!(["/usr/bin/wine", patcher]),
            "Unknown original GUI installer protocol"
        );
        ensure!(
            op["success_log"] == format!("{cwd}/installlog.rtf")
                && op["success_markers"] == json!(["Done. All changes have been applied."])
                && op["failure_regex"]
                    == json!([
                        "Error: ",
                        "Warning: ",
                        // The pinned GUI plans contain one regex escape before
                        // the literal period; raw Rust strings preserve it.
                        r"Done\. Some changes may have been applied"
                    ]),
            "Original GUI diagnostic contract differs"
        );
        ensure!(
            op["wine_registry"]
                == json!([
                    {"key":"HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\User Shell Folders","name":"Desktop","data":"{game_win}"},
                    {"key":"HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Shell Folders","name":"Desktop","data":"{game_win}"}
                ]),
            "Original GUI destination protocol differs"
        );
        gui_namespace(&op["actions"])?
    } else {
        ensure!(
            CLI_INSTALLERS.contains(&sha256.as_str()),
            "Unknown original CLI installer family"
        );
        ensure!(
            op["success_log"] == format!("{cwd}/installlog.txt")
                && op["success_markers"]
                    == json!(["The installation is complete with 0 errors and 0 warnings."])
                && op["failure_regex"] == json!([r"^\[Error\]", r"^\[Warning\]"]),
            "Original CLI diagnostic contract differs"
        );
        let args = op["argv"].as_array().context("Original CLI argv absent")?;
        let namespace = if args.len() == 9 {
            ensure!(
                args[6] == "--namespace-option-index",
                "Unknown original CLI selection flag"
            );
            let raw = args[7]
                .as_str()
                .context("Original namespace index absent")?;
            let n = raw
                .parse::<usize>()
                .context("Original namespace index malformed")?;
            ensure!(
                raw == n.to_string(),
                "Original namespace index noncanonical"
            );
            Some(n)
        } else {
            ensure!(args.len() == 7, "Unknown original CLI argument count");
            None
        };
        let mut expected = vec![
            json!("/usr/bin/wine"),
            json!(patcher),
            json!("--game-dir"),
            json!("{game}"),
            json!("--tslpatchdata"),
            json!(cwd),
        ];
        if let Some(n) = namespace {
            expected.extend([json!("--namespace-option-index"), json!(n.to_string())]);
        }
        expected.push(json!("--install"));
        ensure!(args == &expected, "Original CLI arguments differ");
        namespace
    };
    Ok(Effect::NativeRun {
        ordinal,
        patcher,
        sha256,
        cwd,
        namespace,
    })
}

fn gui_namespace(value: &Value) -> Result<Option<usize>> {
    let actions = value.as_array().context("Original GUI actions absent")?;
    ensure!(
        matches!(actions.len(), 9 | 12 | 14),
        "Unknown original GUI action template"
    );
    let prefix = actions.len() - 9;
    let tail = &actions[prefix..];
    let title = tail[0]
        .get("wait_window")
        .and_then(Value::as_str)
        .context("Original GUI title absent")?;
    ensure!(
        title.starts_with("^(") && title.len() <= 256 && !title.chars().any(char::is_control),
        "Original GUI title malformed"
    );
    ensure!(
        tail == [
            json!({"wait_window":title,"timeout":120}),
            json!({"sleep":3}),
            json!({"click":[502,433],"window":title}),
            json!({"wait_window":"^Browse for Folder$","timeout":120}),
            json!({"sleep":2}),
            json!({"click":[62,69],"window":"^Browse for Folder$"}),
            json!({"sleep":1}),
            json!({"click":[157,274],"window":"^Browse for Folder$"}),
            json!({"wait_log":600})
        ],
        "Original GUI installation actions differ"
    );
    if prefix == 0 {
        return Ok(None);
    }
    let dialog = "^TSLPatcher - Select installation type$";
    let mut expected = vec![
        json!({"wait_window":dialog,"timeout":120}),
        json!({"sleep":2}),
    ];
    let index = if prefix == 5 {
        let keys = actions[2]
            .get("key")
            .and_then(Value::as_array)
            .context("Original namespace keys absent")?;
        ensure!(
            !keys.is_empty() && keys.iter().all(|k| k == "Down"),
            "Original namespace selection is not ordered Down-only"
        );
        expected.extend([json!({"key":keys,"window":dialog}), json!({"sleep":1})]);
        keys.len()
    } else {
        0
    };
    expected.push(json!({"click":[248,181],"window":dialog}));
    ensure!(
        actions[..prefix] == expected,
        "Original GUI namespace actions differ"
    );
    Ok(Some(index))
}
