//! Exact, explicitly selected vendor recipe; never a generic executable approval.
use crate::{Component, Instruction, draft, paths::Roots};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub const GENERATOR: &str = "resolved-vendor-installer";
pub const GUID: &str = "bee5a72b-d748-8f83-8261-81758a9bfc56";
pub const FILENAME: &str = "tslrcm2022.exe";
pub const SHA256: &str = "94c99c4807da4b304de6e0efed1be55e0f43d13cc5a274582ed3323ac0e2f1a6";
pub const SIZE: u64 = 137947655;
pub const LOG: &str = "modsync-tslrcm-english-setup.log";
const ARGUMENTS: &str = "/SP- /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /NOCANCEL /LANG=english /TYPE=english /DIR=Z:<<kotorDirectory>> /LOG=Z:<<modDirectory>>/modsync-tslrcm-english-setup.log";
const OUTPUTS: [(&str, u64, &str); 2] = [
    (
        "dialog.tlk",
        10171056,
        "e501c665f7763bf9c63ac44e3a034a6969abbc6f0023096d8e137a400aa23e25",
    ),
    (
        "modules/003ebo.mod",
        3853307,
        "575d7a5cb502d3e5e1ed07803e47968f66f91ddb83094f887c7597d74edd0013",
    ),
];

fn check(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Vendor verification cancelled"
    );
    Ok(())
}
fn digest(path: &Path, maximum: u64, cancelled: &AtomicBool) -> Result<(String, u64)> {
    let mut file = crate::paths::open_regular_read(path)?;
    ensure!(
        file.metadata()?.len() <= maximum,
        "Vendor input exceeds bound"
    );
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        check(cancelled)?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        ensure!(bytes <= maximum, "Vendor input grew beyond bound");
        hash.update(&buffer[..count]);
    }
    Ok((format!("{:x}", hash.finalize()), bytes))
}

/// Only this immutable guide component is understood. In particular, this does
/// not reinterpret unknown prose, mixed methods, alternate warnings or languages.
fn input_context(component: &Component) -> Result<Value> {
    let mut value = serde_json::to_value(component)?;
    let reviewed: Value = serde_json::from_str(REVIEWED_INPUT)?;
    // The shared document loader splits comma-delimited Language arrays. Bind
    // only this exact note in its raw or existing loader-normalized form, then
    // emit the loader form so independent JSON/TOML reloads remain identical.
    let canonical = serde_json::to_value(
        crate::Guide::parse_preserving_selection(REVIEWED_INPUT, "json")?
            .components
            .into_iter()
            .next()
            .context("Reviewed vendor component missing")?,
    )?;
    if value.get("Language") == reviewed.get("Language") {
        value["Language"] = canonical["Language"].clone();
    }
    ensure!(
        value == canonical,
        "Vendor component differs from its reviewed guide context"
    );
    Ok(value)
}

pub(crate) fn bind(
    component: &mut Component,
    root: &Path,
    language: &str,
    cancelled: &AtomicBool,
) -> Result<()> {
    check(cancelled)?;
    ensure!(
        language == "english",
        "Vendor binding requires an explicit supported language"
    );
    let context = input_context(component)?;
    let path = root.join(FILENAME);
    ensure!(
        digest(&path, SIZE, cancelled)? == (SHA256.into(), SIZE),
        "Vendor installer identity mismatch"
    );
    let mut bound = component.clone();
    bound
        .metadata
        .insert("Language".into(), context["Language"].clone());
    bound.instructions = vec![Instruction {
        guid: crate::markdown::generated_id(&format!("{}:vendor:english", component.guid), 0),
        parent: component.guid.clone(),
        action: "Execute".into(),
        source: vec![format!("<<modDirectory>>/{FILENAME}")],
        destination: String::new(),
        arguments: ARGUMENTS.into(),
        overwrite: true,
        dependencies: vec![],
        restrictions: vec![],
    }];
    let binding = json!({"Schema":1, "Kind":"TSLRCM-1.8.6-English", "VendorLanguage":language,
        "InputComponent":context, "Installer":{"Filename":FILENAME,"Sha256":SHA256,"Bytes":SIZE},
        "Arguments":ARGUMENTS, "Log":LOG, "ExpectedOutputs":OUTPUTS,
        "RuntimeContract":"K2 English TLK; fresh setup log; quiescent process; exact changed outputs"});
    bound
        .metadata
        .insert("VendorInstallerBinding".into(), binding);
    bound.metadata.insert("DraftReview".into(), json!({"RequiresReview":true,"Generator":GENERATOR,
        "Derivations":[{"Filename":FILENAME,"Sha256":SHA256,"Bytes":SIZE}],
        "Result":draft::DraftResult {component_guid:component.guid.clone(),draft_instruction_count:1,
            unparsed_gaps:vec![],conditional_drafts:vec![],rejected_drafts:0}}));
    check(cancelled)?;
    *component = bound;
    Ok(())
}

/// The reviewed guide component a vendor binding was made from, for definition
/// comparisons only. It is admitted only while it still equals the reviewed
/// context, and it is never proof that the installer ran: `before` replays that.
pub(crate) fn declared_original_definition(component: &Component) -> Result<Option<Component>> {
    let Some(binding) = component.metadata.get("VendorInstallerBinding") else {
        return Ok(None);
    };
    let original: Component = serde_json::from_value(
        binding
            .get("InputComponent")
            .cloned()
            .context("Missing vendor input context")?,
    )?;
    input_context(&original)?;
    Ok(Some(original))
}

pub(crate) fn replay(
    component: &Component,
    root: &Path,
    cancelled: &AtomicBool,
) -> Result<Component> {
    let binding = component
        .metadata
        .get("VendorInstallerBinding")
        .context("Missing vendor binding")?;
    let language = binding
        .get("VendorLanguage")
        .and_then(Value::as_str)
        .context("Missing vendor language")?;
    let mut replayed: Component = serde_json::from_value(
        binding
            .get("InputComponent")
            .cloned()
            .context("Missing vendor input context")?,
    )?;
    bind(&mut replayed, root, language, cancelled)?;
    let mut actual = serde_json::to_value(component)?;
    let mut expected = serde_json::to_value(&replayed)?;
    actual
        .as_object_mut()
        .context("Invalid vendor component")?
        .remove("DraftReview");
    expected
        .as_object_mut()
        .context("Invalid vendor replay")?
        .remove("DraftReview");
    ensure!(
        actual == expected,
        "Vendor context, executable recipe or outputs differ from replay"
    );
    let record = component
        .metadata
        .get("DraftReview")
        .context("Missing vendor draft")?;
    let expected = &replayed.metadata["DraftReview"];
    for key in ["Generator", "Derivations", "Result"] {
        ensure!(
            record.get(key) == expected.get(key),
            "Vendor diagnostics differ from replay"
        );
    }
    Ok(replayed)
}

pub(crate) fn replay_digest(component: &Component) -> Result<String> {
    // Map ordering can change across JSON/TOML reloads. Sort only object keys;
    // array order, paths, arguments and every semantic value remain committed.
    fn canonical(value: &mut Value) {
        match value {
            Value::Object(object) => {
                let mut entries: Vec<_> = std::mem::take(object).into_iter().collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                for (key, mut value) in entries {
                    canonical(&mut value);
                    object.insert(key, value);
                }
            }
            Value::Array(values) => values.iter_mut().for_each(canonical),
            _ => {}
        }
    }
    let mut value = serde_json::to_value(component)?;
    canonical(&mut value);
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(&value)?)))
}

pub(crate) struct ExecutionProof {
    previous: Vec<Option<(String, u64)>>,
}

/// No subprocesses are used here, including during validation/simulation.
pub(crate) fn before(
    component: &Component,
    roots: &Roots,
    install: bool,
    cancelled: &AtomicBool,
) -> Result<Option<ExecutionProof>> {
    if !component.metadata.contains_key("VendorInstallerBinding") {
        ensure!(
            component
                .metadata
                .get("DraftReview")
                .and_then(|r| r.get("Generator"))
                .and_then(Value::as_str)
                != Some(GENERATOR),
            "Vendor draft lost its binding"
        );
        return Ok(None);
    }
    let replayed = replay(component, &roots.mods, cancelled)?;
    let record = &component.metadata["DraftReview"];
    let verification = record
        .get("Verification")
        .context("Vendor requires independent verified replay")?;
    ensure!(
        record.get("RequiresReview").and_then(Value::as_bool) == Some(false)
            && record.get("Resolution").is_none()
            && record.get("OriginalResult").is_none()
            && verification.get("Reviewer").and_then(Value::as_str)
                == Some(draft::review::VERIFIER_NAME)
            && verification.get("Kind").and_then(Value::as_str)
                == Some(draft::review::VERIFICATION_KIND)
            && verification.get("ReplayDigest").and_then(Value::as_str)
                == Some(replay_digest(&replayed)?.as_str()),
        "Vendor approval is not an independent replay receipt"
    );
    ensure!(
        matches!(
            crate::game::detect(&roots.game),
            crate::game::Detected::Kotor2Legacy | crate::game::Detected::Kotor2Aspyr
        ),
        "Vendor requires an identified K2 game"
    );
    ensure!(
        roots.mods.to_str().is_some_and(|p| p.starts_with('/'))
            && roots.game.to_str().is_some_and(|p| p.starts_with('/')),
        "Reviewed Wine vendor recipe requires absolute POSIX roots"
    );
    let tlk = roots.game.join("dialog.tlk");
    roots.check(&tlk)?;
    let mut header = [0u8; 12];
    crate::paths::open_regular_read(&tlk)?.read_exact(&mut header)?;
    ensure!(
        &header[..8] == b"TLK V3.0" && u32::from_le_bytes(header[8..12].try_into()?) == 0,
        "English vendor selection does not match actual game TLK language"
    );
    let log = roots.mods.join(LOG);
    roots.check(&log)?;
    if !install {
        return Ok(None);
    }
    match fs::symlink_metadata(&log) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
        Ok(_) => anyhow::bail!("Vendor setup log already exists; restore before retrying"),
    }
    let mut previous = Vec::new();
    for (relative, _, _) in OUTPUTS {
        let path = roots.game.join(relative);
        roots.check(&path)?;
        match fs::symlink_metadata(&path) {
            Ok(_) => previous.push(Some(digest(&path, 64 * 1024 * 1024, cancelled)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => previous.push(None),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(Some(ExecutionProof { previous }))
}

/// Called only after process_path has waited for RunnerSession quiescence.
pub(crate) fn after(proof: ExecutionProof, roots: &Roots, cancelled: &AtomicBool) -> Result<()> {
    check(cancelled)?;
    let path = roots.mods.join(LOG);
    roots.check(&path)?;
    let mut log = Vec::new();
    crate::paths::open_regular_read(&path)?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut log)?;
    ensure!(
        !log.is_empty() && log.len() <= 4 * 1024 * 1024,
        "Vendor log missing or oversized"
    );
    let text = if log.starts_with(&[0xff, 0xfe]) {
        ensure!(log.len() % 2 == 0, "Truncated vendor UTF16 log");
        String::from_utf16(
            &log[2..]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect::<Vec<_>>(),
        )?
    } else {
        String::from_utf8(log)?
    };
    let command_lines: Vec<_> = text
        .lines()
        .filter_map(|line| {
            line.split_once("Setup command line: ")
                .map(|(_, value)| value)
        })
        .collect();
    ensure!(
        command_lines.len() == 1,
        "Vendor log has ambiguous setup command identity"
    );
    let arguments = [
        "/SP-".to_owned(),
        "/VERYSILENT".into(),
        "/SUPPRESSMSGBOXES".into(),
        "/NORESTART".into(),
        "/NOCANCEL".into(),
        "/LANG=english".into(),
        "/TYPE=english".into(),
        format!(
            "/DIR=Z:{}",
            roots.game.to_str().context("Non-UTF8 game root")?
        ),
        format!(
            "/LOG=Z:{}/{}",
            roots.mods.to_str().context("Non-UTF8 mods root")?,
            LOG
        ),
    ];
    for argument in arguments {
        let pattern = format!(r#"(?:^|\s)"?{}"?(?:\s|$)"#, regex::escape(&argument));
        ensure!(
            regex::Regex::new(&pattern)?.is_match(command_lines[0]),
            "Vendor log does not attest the reviewed language, arguments and roots"
        );
    }
    ensure!(
        text.contains("Installation process succeeded.")
            && text.contains("Need to restart Windows? No"),
        "Vendor setup log lacks successful completion"
    );
    let failure = regex::Regex::new(
        r"(?i)Rolling back changes\.|Got EAbort exception\.|Exception message:|Defaulting to \w+ for suppressed message box",
    )?;
    ensure!(!failure.is_match(&text), "Vendor setup log reports failure");
    let mut changed = false;
    for (index, (relative, size, expected)) in OUTPUTS.iter().enumerate() {
        let path = roots.game.join(relative);
        roots.check(&path)?;
        let observed = digest(&path, *size, cancelled)?;
        ensure!(
            observed == ((*expected).into(), *size),
            "Vendor output identity mismatch: {relative}"
        );
        changed |= proof.previous[index].as_ref() != Some(&observed);
    }
    ensure!(changed, "Vendor process produced no verified game changes");
    Ok(())
}

// Exact imported guide component at mod-builds67dc5fd8, including all language,
// compatibility warnings and unresolved input diagnostics. No generic prose waiver.
const REVIEWED_INPUT: &str = r###"{
  "Guid": "bee5a72b-d748-8f83-8261-81758a9bfc56",
  "Name": "The Sith Lords Restored Content Mod",
  "Author": "zbyl2, DarthStoney, Hassat Hunter & VarsityPuppet",
  "Description": "The whole reason why you would mod KOTOR 2, the legendary mod that includes bugfixes, restored content, and altered scenes to return the game to what it was meant to be. This mod isn't just essential—it's mandatory.",
  "Directions": "",
  "Tier": "1 - Essential",
  "Category": [
    "Bugfix",
    "Immersion",
    "Mechanics Change",
    "Restored Content"
  ],
  "IsSelected": true,
  "Dependencies": [],
  "Restrictions": [],
  "InstallBefore": [],
  "InstallAfter": [],
  "Instructions": [],
  "Options": [],
  "ResourceRegistry": {
    "https://deadlystream.com/files/file/578-tsl-restored-content-mod/": {
      "Files": {},
      "HandlerMetadata": {}
    }
  },
  "CompatibilityWarning": "Recall that no version of M4-78 is compatible with this build; do not use it alongside TSLRCM! Remember also that the Workshop version of this or any other mod is NOT to be used!",
  "DraftReview": {
    "RequiresReview": true,
    "Result": {
      "ComponentGuid": "bee5a72b-d748-8f83-8261-81758a9bfc56",
      "DraftInstructionCount": 0,
      "UnparsedGaps": [
        "Installation method requires observed archive contents and explicit review before its sources, destinations, exclusions, and installer choices can be bound; no actions were generated from the method or guide defaults.",
        "Installation method is unsupported, mixed, or exceeds 256 characters; explicit archive-aware interpretation is required."
      ],
      "ConditionalDrafts": [],
      "RejectedDrafts": 0
    },
    "MethodContext": {
      "Method": "Executable",
      "MethodTruncated": false,
      "Kind": "Unsupported",
      "GuideDefaults": {
        "PreambleStatus": "Inspected",
        "PatcherDestination": "<<kotorDirectory>>",
        "LooseFilesDestination": "<<kotorDirectory>>\\Override",
        "NoOverrideOrModulesSubfolders": true
      },
      "ExplicitDirectionsTakePrecedence": true,
      "ArchiveBinding": "Unresolved"
    }
  },
  "Heading": "TSLRCM",
  "InstallationMethod": "Executable",
  "InstallationWarning": "DRAFT INSTRUCTIONS: parsed from guide prose by the natural-language importer. Review before installing - never auto-trusted.",
  "Language": [
    "YES, *but* you must install the version of TSLRCM which corresponds to your language of choice."
  ],
  "NameFieldContent": "[The Sith Lords Restored Content Mod](https://deadlystream.com/files/file/578-tsl-restored-content-mod/)",
  "WidescreenOnly": false
}"###;
