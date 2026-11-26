use crate::{Component, Guide, Instruction};
use regex::{Captures, Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::BTreeMap, sync::LazyLock};

pub mod review;

pub const REVIEW_FLAG: &str = "DRAFT INSTRUCTIONS: parsed from guide prose by the natural-language importer. Review before installing - never auto-trusted.";
const MODS: &str = "<<modDirectory>>";
const GAME: &str = "<<kotorDirectory>>";
pub(crate) const METHOD_UNRESOLVED: &str = "Installation method requires observed archive contents and explicit review before its sources, destinations, exclusions, and installer choices can be bound; no actions were generated from the method or guide defaults.";

pub fn requires_review(component: &Component) -> bool {
    if let Some(review) = component.metadata.get("DraftReview") {
        if review.get("RequiresReview").and_then(|v| v.as_bool()) != Some(false)
            || !review::receipt_matches(component, review)
        {
            return true;
        }
        // A flag alone is not a review receipt. Keep malformed, foreign, or
        // unresolved diagnostics fenced even when a saved flag says otherwise.
        return review
            .get("Result")
            .and_then(|value| serde_json::from_value::<DraftResult>(value.clone()).ok())
            .is_none_or(|result| {
                !result.component_guid.eq_ignore_ascii_case(&component.guid)
                    || !result.unparsed_gaps.is_empty()
                    || !result.conditional_drafts.is_empty()
                    || result.rejected_drafts != 0
            });
    }
    component
        .metadata
        .get("InstallationWarning")
        .and_then(|v| v.as_str())
        .is_some_and(|s| s.contains(REVIEW_FLAG))
}

pub fn mark_reviewed(component: &mut Component) -> anyhow::Result<()> {
    use anyhow::{Context, ensure};
    fn contains_k1_native(component: &Component, depth: usize) -> bool {
        depth > 64
            || component.metadata.contains_key("K1WidescreenBinding")
            || component
                .metadata
                .get("DraftReview")
                .and_then(|record| record.get("Generator"))
                .and_then(serde_json::Value::as_str)
                == Some(crate::k1_widescreen_recipe::GENERATOR)
            || component
                .options
                .iter()
                .any(|option| contains_k1_native(option, depth + 1))
    }
    ensure!(
        !crate::archive_generation::source_plan_patcher::contains_native(component)
            && !crate::archive_generation::reviewed_master::contains_resolved(component)
            && !contains_k1_native(component, 0),
        "Native source-plan and K1 widescreen drafts require dedicated independent replay"
    );
    ensure!(
        component
            .metadata
            .get("DraftReview")
            .and_then(|record| record.get("Generator"))
            .and_then(serde_json::Value::as_str)
            .is_none_or(|generator| {
                generator != crate::vendor_installer::GENERATOR
                    && generator != crate::archive_generation::reviewed_manual::GENERATOR
                    && generator != crate::archive_generation::reviewed_manual::PATCHER_GENERATOR
            }),
        "Bound vendor and reviewed manual drafts require independent verified replay"
    );
    let reviewed_identity = review::identity(component)?;
    if let Some(record) = component.metadata.get("DraftReview") {
        anyhow::ensure!(
            record.get("Resolution").is_none() || review::receipt_matches(component, record),
            "Changed resolved drafts require a new explicit resolution"
        );
    }
    let review = component
        .metadata
        .get_mut("DraftReview")
        .context("Component has no generated draft review record")?;
    let result: DraftResult = serde_json::from_value(
        review
            .get("Result")
            .cloned()
            .context("Missing draft diagnostics")?,
    )?;
    ensure!(
        result.component_guid.eq_ignore_ascii_case(&component.guid),
        "Draft diagnostics belong to a different component"
    );
    ensure!(
        result.unparsed_gaps.is_empty()
            && result.conditional_drafts.is_empty()
            && result.rejected_drafts == 0,
        "Resolve unparsed, conditional, or rejected instructions before accepting this draft"
    );
    review["ReviewedIdentity"] = reviewed_identity.into();
    review["RequiresReview"] = false.into();
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DraftResult {
    pub component_guid: String,
    pub draft_instruction_count: usize,
    pub unparsed_gaps: Vec<String>,
    pub conditional_drafts: Vec<String>,
    pub rejected_drafts: usize,
}

/// Check before model normalization, which removes null component properties.
/// A malformed review record must never disappear or become an accepted draft.
/// Reject reserved aliases before format precedence can conceal review state.
/// Dynamic dictionaries are deliberately outside this traversal.
pub(crate) fn validate_raw_review_aliases(value: &serde_json::Value) -> anyhow::Result<()> {
    use anyhow::ensure;
    use serde_json::Value;
    fn reserved(object: &serde_json::Map<String, Value>, names: &[&str]) -> anyhow::Result<()> {
        for name in names {
            let aliases = object
                .keys()
                .filter(|key| crate::unicode_case::ordinal_eq(key, name))
                .collect::<Vec<_>>();
            ensure!(
                aliases.len() <= 1 && aliases.first().is_none_or(|key| key.as_str() == *name),
                "Noncanonical or ambiguous reserved review property"
            );
        }
        Ok(())
    }
    fn receipt(value: &Value, depth: usize, work: &mut usize) -> anyhow::Result<()> {
        ensure!(depth <= 64, "Review nesting exceeds 64");
        *work += 1;
        ensure!(*work <= 100_000, "Review record bound exceeded");
        let Some(object) = value.as_object() else {
            return Ok(());
        };
        *work += object.len();
        ensure!(*work <= 100_000, "Review property bound exceeded");
        reserved(
            object,
            &[
                "RequiresReview",
                "Result",
                "OriginalResult",
                "Resolution",
                "ReviewedIdentity",
                "Derivation",
                "Verification",
                "ComponentGuid",
                "DraftInstructionCount",
                "UnparsedGaps",
                "ConditionalDrafts",
                "RejectedDrafts",
                "expectedIdentity",
                "gapDecisions",
                "conditionDecisions",
                "rejectedDecisions",
                "reviewNote",
            ],
        )?;
        for field in ["Result", "OriginalResult", "Resolution"] {
            if let Some(value) = object.get(field) {
                receipt(value, depth + 1, work)?;
            }
        }
        if let Some(derivation) = object.get("Derivation").and_then(Value::as_object) {
            reserved(derivation, &["Filename", "Sha256", "Bytes"])?;
        }
        if let Some(verification) = object.get("Verification").and_then(Value::as_object) {
            reserved(
                verification,
                &[
                    "Reviewer",
                    "Kind",
                    "ToolVersion",
                    "ArchiveSha256",
                    "ArchiveFilename",
                    "ReplayDigest",
                    "IdentityVersion",
                ],
            )?;
        }
        Ok(())
    }
    fn visit(value: &Value, depth: usize, work: &mut usize) -> anyhow::Result<()> {
        ensure!(depth <= 64, "Review component nesting exceeds 64");
        *work += 1;
        ensure!(*work <= 100_000, "Review component record bound exceeded");
        if let Some(values) = value.as_array() {
            for value in values {
                visit(value, depth + 1, work)?;
            }
        } else if let Some(object) = value.as_object() {
            *work += object.len();
            ensure!(*work <= 100_000, "Review property bound exceeded");
            reserved(object, &["DraftReview", "UnresolvedMasters"])?;
            if let Some(review) = object.get("DraftReview") {
                ensure!(review.is_object(), "DraftReview must be an object");
                receipt(review, depth + 1, work)?;
            }
            let warnings = object
                .iter()
                .filter(|(key, _)| crate::unicode_case::ordinal_eq(key, "InstallationWarning"))
                .collect::<Vec<_>>();
            if warnings.iter().any(|(_, value)| {
                value
                    .as_str()
                    .is_some_and(|text| text.contains(REVIEW_FLAG))
            }) {
                ensure!(
                    warnings.len() == 1 && warnings[0].0 == "InstallationWarning",
                    "Ambiguous draft warning property"
                );
            }
            for (key, child) in object {
                if [
                    "Options",
                    "Option",
                    "thisMod",
                    "Components",
                    "ModComponents",
                ]
                .iter()
                .any(|field| crate::unicode_case::ordinal_eq(key, field))
                {
                    visit(child, depth + 1, work)?;
                }
            }
        }
        Ok(())
    }
    visit(value, 0, &mut 0)
}

/// YAML and XML carry scalars as text (the pinned YamlDotNet reader profile has
/// no implicit typing), and XML omits empty lists. Restore the typed review
/// record Rust wrote so its own YAML/XML exports reload. Anything that is not
/// an exact typed spelling stays as authored and fails validation.
pub(crate) fn canonicalize_text_review_records(value: &mut serde_json::Value) {
    use serde_json::Value;
    fn component(value: &mut Value, depth: usize) {
        if depth > 64 {
            return;
        }
        let Some(object) = value.as_object_mut() else {
            return;
        };
        if let Some(review) = object.get_mut("DraftReview").and_then(Value::as_object_mut) {
            if let Some(flag) = review.get_mut("RequiresReview")
                && let Some(text) = flag.as_str()
            {
                if text.eq_ignore_ascii_case("true") {
                    *flag = Value::Bool(true);
                } else if text.eq_ignore_ascii_case("false") {
                    *flag = Value::Bool(false);
                }
            }
            if let Some(bytes) = review
                .get_mut("Derivation")
                .and_then(Value::as_object_mut)
                .and_then(|derivation| derivation.get_mut("Bytes"))
                && let Some(parsed) = bytes
                    .as_str()
                    .filter(|text| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
                    .and_then(|text| text.parse::<u64>().ok())
            {
                *bytes = Value::from(parsed);
            }
            if let Some(result) = review.get_mut("Result").and_then(Value::as_object_mut) {
                for key in ["DraftInstructionCount", "RejectedDrafts"] {
                    if let Some(count) = result.get_mut(key)
                        && let Some(parsed) = count
                            .as_str()
                            .filter(|text| {
                                !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
                            })
                            .and_then(|text| text.parse::<u64>().ok())
                    {
                        *count = Value::from(parsed);
                    }
                }
                for key in ["UnparsedGaps", "ConditionalDrafts"] {
                    match result.get_mut(key) {
                        None | Some(Value::Null) => {
                            result.insert(key.into(), Value::Array(Vec::new()));
                        }
                        Some(single @ Value::String(_)) => {
                            *single = Value::Array(vec![single.take()]);
                        }
                        _ => {}
                    }
                }
            }
        }
        if let Some(options) = object.get_mut("Options").and_then(Value::as_array_mut) {
            for option in options {
                component(option, depth + 1);
            }
        }
    }
    let components = match value {
        Value::Array(components) => Some(components),
        Value::Object(object) if !object.contains_key("Guid") => ["thisMod", "Components"]
            .into_iter()
            .find(|key| object.get(*key).is_some_and(Value::is_array))
            .and_then(|key| object.get_mut(key))
            .and_then(Value::as_array_mut),
        Value::Object(_) => {
            component(value, 0);
            None
        }
        _ => None,
    };
    for item in components.into_iter().flatten() {
        component(item, 0);
    }
}

pub(crate) fn validate_review_records(value: &serde_json::Value) -> anyhow::Result<()> {
    use serde_json::Value;
    // Match Guide::from_value's supported envelopes and precedence. Resource
    // dictionaries and other arbitrary metadata are not component records.
    if let Some(components) = value.as_array() {
        for component in components {
            validate_component_review(component)?;
        }
    } else if value.get("Guid").is_some() {
        validate_component_review(value)?;
    } else if let Some(components) = value
        .get("thisMod")
        .or_else(|| value.get("Components"))
        .and_then(Value::as_array)
    {
        for component in components {
            validate_component_review(component)?;
        }
    }
    Ok(())
}

fn validate_component_review(value: &serde_json::Value) -> anyhow::Result<()> {
    use anyhow::{Context, ensure};
    use serde_json::Value;
    if let Some(review) = value.get("DraftReview") {
        let required = review
            .get("RequiresReview")
            .and_then(Value::as_bool)
            .context("DraftReview.RequiresReview must be a boolean")?;
        let result: DraftResult = serde_json::from_value(
            review
                .get("Result")
                .cloned()
                .context("DraftReview needs complete Result diagnostics")?,
        )
        .context("Invalid DraftReview.Result diagnostics")?;
        ensure!(
            value
                .get("Guid")
                .and_then(Value::as_str)
                .is_some_and(|guid| guid.eq_ignore_ascii_case(&result.component_guid)),
            "DraftReview diagnostics belong to a different component"
        );
        ensure!(
            required
                || (result.unparsed_gaps.is_empty()
                    && result.conditional_drafts.is_empty()
                    && result.rejected_drafts == 0),
            "Reviewed draft still contains unresolved diagnostics"
        );
    }
    if let Some(options) = value.get("Options").and_then(Value::as_array) {
        for option in options {
            validate_component_review(option)?;
        }
    }
    Ok(())
}

/// Drafts are attached for review, never substituted for existing authored instructions.
/// Which reading of guide prose a draft uses.
///
/// `Pinned` reproduces the C# reference byte for byte, mistakes included. Reviewed
/// recipes bind to that output as the identity of their source guide, and a guide
/// opened for install is drafted the same way, so this output must not change.
/// `Corrected` is used only when a person asks for a draft (`convert
/// --parse-directions`, the desktop "Draft instructions from prose" button).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Pinned,
    Corrected,
}

/// Draft with the pinned C# reading. Every caller that binds or loads a guide uses
/// this one; see [`Mode`].
pub fn generate(guide: &mut Guide) -> Vec<DraftResult> {
    generate_with(guide, Mode::Pinned)
}

/// Draft with the corrected reading, for an explicit request to draft from prose.
///
/// A step is drafted only when its source names a file, a wildcard or a named
/// folder. Prose such as "the files from the patch" or "the mod" leaves a review
/// gap instead of a folder guessed from ordinary words, copy-then-rename stays a
/// gap, and download advice no longer becomes an option named after a phrase.
/// The output still requires review like every other draft.
pub fn generate_corrected(guide: &mut Guide) -> Vec<DraftResult> {
    generate_with(guide, Mode::Corrected)
}

fn generate_with(guide: &mut Guide, mode: Mode) -> Vec<DraftResult> {
    let mut results = Vec::new();
    let defaults = guide_defaults(guide);
    for component in &mut guide.components {
        let method = component
            .metadata
            .get("InstallationMethod")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|method| !method.is_empty());
        let method_context = method.map(|method| method_context(method, &defaults));
        if !component.instructions.is_empty()
            || (component.directions.trim().is_empty() && method_context.is_none())
            || component.metadata.contains_key("DraftReview")
        {
            continue;
        }
        let mut result = DraftResult {
            component_guid: component.guid.clone(),
            draft_instruction_count: 0,
            unparsed_gaps: Vec::new(),
            conditional_drafts: Vec::new(),
            rejected_drafts: 0,
        };
        if let Some(context) = &method_context {
            result.unparsed_gaps.push(METHOD_UNRESOLVED.into());
            if context["Kind"] == "Unsupported" {
                result.unparsed_gaps.push(
                    "Installation method is unsupported, mixed, or exceeds 256 characters; explicit archive-aware interpretation is required."
                        .into(),
                );
            } else if context["Kind"] == "MultiRunPatcher" {
                result.unparsed_gaps.push(
                    "Multi-run patcher order and namespace selections remain unresolved.".into(),
                );
            }
        }
        let normalized = component
            .directions
            .replace("**", "")
            .replace("__", "")
            .replace('*', "");
        for (text, condition) in units(&normalized) {
            let Some(mut instruction) = parse_unit(&text, &mut result.unparsed_gaps, mode) else {
                continue;
            };
            if let Some(condition) = condition {
                result.conditional_drafts.push(format!(
                    "{} ({}) applies only if also using '{condition}' - verify before installing.",
                    instruction.action,
                    if instruction.source.is_empty() {
                        "(no source)".into()
                    } else {
                        instruction
                            .source
                            .iter()
                            .take(3)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                ));
            }
            if sanitize(&mut instruction) {
                component.instructions.push(instruction);
            } else {
                result.rejected_drafts += 1;
            }
        }
        if let Some(download) = component
            .metadata
            .get("DownloadInstructions")
            .and_then(|v| v.as_str())
        {
            component
                .options
                .extend(download_options(download, &component.guid, mode));
        }
        result.draft_instruction_count = component.instructions.len();
        if result.draft_instruction_count > 0 || method_context.is_some() {
            append_warning(component, REVIEW_FLAG, true);
        }
        for note in &result.conditional_drafts {
            append_warning(component, note, false);
        }
        if result.draft_instruction_count > 0
            || !result.unparsed_gaps.is_empty()
            || !result.conditional_drafts.is_empty()
            || result.rejected_drafts > 0
        {
            let mut record = json!({"RequiresReview": true, "Result": result});
            if let Some(context) = method_context {
                record["MethodContext"] = context;
            }
            component.metadata.insert("DraftReview".into(), record);
        }
        results.push(result);
    }
    results
}

pub fn generate_method_contexts(guide: &mut Guide) {
    let defaults = guide_defaults(guide);
    for component in &mut guide.components {
        if !component.instructions.is_empty() || component.metadata.contains_key("DraftReview") {
            continue;
        }
        let Some(method) = component
            .metadata
            .get("InstallationMethod")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|method| !method.is_empty())
        else {
            continue;
        };
        let context = method_context(method, &defaults);
        let mut result = DraftResult {
            component_guid: component.guid.clone(),
            draft_instruction_count: 0,
            unparsed_gaps: vec![METHOD_UNRESOLVED.into()],
            conditional_drafts: Vec::new(),
            rejected_drafts: 0,
        };
        match context["Kind"].as_str() {
            Some("Unsupported") => result.unparsed_gaps.push(
                "Installation method is unsupported, mixed, or exceeds 256 characters; explicit archive-aware interpretation is required.".into(),
            ),
            Some("MultiRunPatcher") => result.unparsed_gaps.push(
                "Multi-run patcher order and namespace selections remain unresolved.".into(),
            ),
            _ => {}
        }
        append_warning(component, REVIEW_FLAG, true);
        component.metadata.insert(
            "DraftReview".into(),
            json!({"RequiresReview": true, "Result": result, "MethodContext": context}),
        );
    }
}

fn method_context(method: &str, defaults: &serde_json::Value) -> serde_json::Value {
    let bounded = method.chars().take(257).count() <= 256;
    let kind = if bounded {
        match method.to_ascii_lowercase().as_str() {
            "loose-file" | "loose-file mod" => "LooseFile",
            "tslpatcher" | "tslpatcher mod" | "holopatcher" | "holopatcher mod" => "Patcher",
            "multi-run tslpatcher" | "multi-run holopatcher" => "MultiRunPatcher",
            _ => "Unsupported",
        }
    } else {
        "Unsupported"
    };
    json!({
        "Method": method.chars().take(256).collect::<String>(),
        "MethodTruncated": !bounded,
        "Kind": kind,
        "GuideDefaults": defaults,
        "ExplicitDirectionsTakePrecedence": true,
        "ArchiveBinding": "Unresolved",
    })
}

pub(crate) fn guide_defaults(guide: &Guide) -> serde_json::Value {
    let nested = guide
        .metadata
        .get("metadata")
        .and_then(|value| value.as_object());
    let preamble = guide
        .metadata
        .get("preambleContent")
        .or_else(|| nested.and_then(|fields| fields.get("preambleContent")))
        .or_else(|| nested.and_then(|fields| fields.get("PreambleContent")))
        .and_then(|value| value.as_str())
        .filter(|text| !text.trim().is_empty());
    let bounded = preamble.is_none_or(|text| text.len() <= 64 * 1024);
    let normalized = preamble
        .filter(|_| bounded)
        .unwrap_or("")
        .replace('*', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    let patcher_root = normalized.contains("if you are asked to direct a tslpatcher or holopatcher executable to a folder to begin install, that folder should always be the main game folder");
    let loose_override = normalized.contains("for mods that are not installed with the tslpatcher or holopatcher, unless otherwise noted their loose-file contents are placed directly in the override directory.");
    let no_subfolders = normalized
        .contains("you should never have any subfolders inside the modules or override folders.");
    json!({
        "PreambleStatus": if !bounded { "TooLarge" } else if preamble.is_some() { "Inspected" } else { "Absent" },
        "PatcherDestination": patcher_root.then_some(GAME),
        "LooseFilesDestination": loose_override.then(|| format!("{GAME}\\Override")),
        "NoOverrideOrModulesSubfolders": no_subfolders,
    })
}

fn append_warning(component: &mut Component, note: &str, prepend: bool) {
    let current = component
        .metadata
        .get("InstallationWarning")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if current.contains(note) {
        return;
    }
    let warning = if current.trim().is_empty() {
        note.to_owned()
    } else if prepend {
        format!("{note}\n{current}")
    } else {
        format!("{current}\n{note}")
    };
    component
        .metadata
        .insert("InstallationWarning".into(), warning.into());
}

fn compile(pattern: &str, singleline: bool) -> Regex {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .dot_matches_new_line(singleline)
        .build()
        .expect("static draft pattern")
}
fn capture<'a>(captures: &Captures<'a>, name: &str) -> Option<&'a str> {
    captures.name(name).map(|m| m.as_str().trim())
}

fn units(text: &str) -> Vec<(String, Option<String>)> {
    static SENTENCES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.\s+[A-Z]").unwrap());
    static CLAUSES: LazyLock<Regex> =
        LazyLock::new(|| compile(r"(?:;|,?\s+then\s+|,\s+and\s+then\s+)", false));
    static BARE: LazyLock<Regex> = LazyLock::new(|| {
        compile(
            r"^if\s+(?:you'?re\s+)?(?:also\s+)?using\s+(?<condition>[^,]+?)\s*,\s*(?:additionally|also)?\s*(?<conditional>.+)$",
            true,
        )
    });
    static CONDITIONAL: LazyLock<Regex> = LazyLock::new(|| {
        compile(
            r"^(?<main>.+?)(?:,?\s+and\s+)?,?\s*if\s+(?:you'?re\s+)?(?:also\s+)?using\s+(?<condition>[^,]+?)\s*,\s*(?:additionally|also)?\s*(?<conditional>.+)$",
            true,
        )
    });
    let mut sentences = Vec::new();
    let mut start = 0;
    for boundary in SENTENCES.find_iter(text) {
        sentences.push(&text[start..boundary.start() + 1]);
        start = boundary.end() - 1;
    }
    sentences.push(&text[start..]);
    let mut result = Vec::new();
    for sentence in sentences {
        for clause in CLAUSES.split(sentence) {
            let cleaned = clause.trim().trim_end_matches(['.', ';']).trim();
            if cleaned.chars().count() <= 15 {
                continue;
            }
            if let Some(m) = BARE.captures(cleaned) {
                result.push((
                    capture(&m, "conditional").unwrap().into(),
                    capture(&m, "condition").map(str::to_owned),
                ));
            } else if let Some(m) = CONDITIONAL.captures(cleaned) {
                let main = capture(&m, "main")
                    .unwrap()
                    .trim_end_matches(['.', ';'])
                    .trim();
                if main.chars().count() > 15 {
                    result.push((main.into(), None));
                }
                result.push((
                    capture(&m, "conditional").unwrap().into(),
                    capture(&m, "condition").map(str::to_owned),
                ));
            } else {
                result.push((cleaned.into(), None));
            }
        }
    }
    if result.is_empty() && !text.trim().is_empty() {
        result.push((text.trim().into(), None));
    }
    result
}

fn movie_destination_statement(text: &str) -> bool {
    let normalized = text
        .replace('*', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches('.')
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "bear in mind that the files from this mod go in your movies folder, not override"
            | "the files from this mod go in your movies folder, not override"
    )
}

/// Interpret only an explicit movie destination and a bounded set of download /
/// alternative-selection notes. Unknown installation language remains a gap.
/// No action is inferred from a possible batch patcher: the archive binder must
/// independently prove that every input contains only disjoint root movie files.
pub(crate) fn movie_context(component: &Component) -> Option<(serde_json::Value, Vec<String>)> {
    let method = component
        .metadata
        .get("InstallationMethod")?
        .as_str()?
        .trim();
    if !matches!(
        method.to_ascii_lowercase().as_str(),
        "loose-file" | "loose-file mod" | "loose-file components, possible .bat patcher"
    ) || component.directions.len() > 32 * 1024
    {
        return None;
    }
    static PARAGRAPHS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n\s*\n").unwrap());
    // Reviewed normalized statements from the immutable guide input. Matching
    // complete text digests keeps appended commands or altered conditions fenced.
    const ALTERNATIVE: &str = "75d7fdaf3d195891704f90b2418f86721875217fbeeaa4f21aacd7106acc4e1d";
    const OPTIONAL_VIDEO_ADVICE: &str =
        "7a7c5e4f7465fcab0f92f49133236815271791f41ef70d9aebe6d75bb8c92f93";
    const OPTIONAL_VIDEO_ADVICE_VARIANT: &str =
        "eb2ce8c0ba27441f3887a2306c64e606175d18d4e5388ca8322114d46cd6cc86";
    const REVIEWED_MOVIE_VARIANTS: &str =
        "cde758f5b3754c74b8a89793e3a601caa1a6323ad358853f6bf114279a90b7b9";
    fn text_digest(text: &str) -> String {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(text.as_bytes()))
    }
    let mut statements = Vec::new();
    let mut gaps = Vec::new();
    let mut destination = false;
    let mut variants = false;
    for paragraph in PARAGRAPHS.split(&component.directions) {
        let text = paragraph.replace('*', "");
        let normalized_paragraph = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let lower = normalized_paragraph.to_ascii_lowercase();
        if text_digest(&lower) == REVIEWED_MOVIE_VARIANTS {
            destination = true;
            variants = true;
            statements.push(json!({"Kind": "ReviewedResolutionFrameRateAndOptionalVariants",
                                    "Text": paragraph, "Resolution": "1920x1080", "FrameRate": 30,
                                    "OptionalOverlays": "None; preserve base subtitles and legal movie"}));
            continue;
        }
        // This is explicitly tentative post-install troubleshooting, not an
        // instruction to run a patcher during installation. Preserve it verbatim.
        if matches!(
            text_digest(&lower).as_str(),
            OPTIONAL_VIDEO_ADVICE | OPTIONAL_VIDEO_ADVICE_VARIANT
        ) {
            statements.push(
                json!({"Kind": "OptionalPostInstallVideoTroubleshooting", "Text": paragraph}),
            );
            continue;
        }
        for sentence in normalized_paragraph.split(". ") {
            let normalized = sentence.split_whitespace().collect::<Vec<_>>().join(" ");
            let lower = normalized.trim_end_matches('.').to_ascii_lowercase();
            if lower.is_empty() {
                continue;
            }
            let kind = if movie_destination_statement(&normalized) {
                destination = true;
                "ExplicitMoviesDestination"
            } else if text_digest(&lower) == ALTERNATIVE {
                "MutuallyExclusiveComponentSelection"
            } else if lower
                == "to install, choose the resolution option which most closely fits your monitor's screen size during the download process"
            {
                "ResolutionSelectedByDownloadedInputs"
            } else {
                gaps.push(normalized.clone());
                "Unresolved"
            };
            statements.push(json!({"Kind": kind, "Text": normalized}));
        }
    }
    if !destination || statements.len() > 64 {
        return None;
    }
    Some((
        json!({"Method": method, "Destination": format!("{GAME}\\Movies"),
                 "Statements": statements,
                 "Layout": if variants { "ReviewedMovieVariants" } else { "DisjointRootMovies" }}),
        gaps,
    ))
}

fn parse_unit(unit: &str, gaps: &mut Vec<String>, mode: Mode) -> Option<Instruction> {
    if movie_destination_statement(unit) {
        // The source files are unknown until archive binding. Do not discard an
        // imperative destination merely because its sentence is an admonition.
        gaps.push(unit.into());
        return None;
    }
    let lower = unit.to_lowercase();
    if [
        "bear in mind",
        "keep in mind",
        "note that",
        "be aware",
        "remember",
        "which ",
    ]
    .iter()
    .any(|p| lower.starts_with(p))
        || [
            "this is normal",
            "this is intended",
            "is your choice",
            "is your preference",
            "up to you",
            "which of these you choose",
            "?",
        ]
        .iter()
        .any(|p| lower.contains(p))
    {
        return None;
    }
    static CLEANLIST: LazyLock<Regex> =
        LazyLock::new(|| compile(r"\bcleanlist[\w\-]*\.txt\b", false));
    static BARE_CLEANLIST: LazyLock<Regex> = LazyLock::new(|| compile(r"\bcleanlist\b", false));
    if let Some(name) = CLEANLIST.find(unit) {
        let mut instruction = empty_instruction("CleanList");
        instruction
            .source
            .push(format!("{MODS}\\{}", name.as_str()));
        instruction.destination = format!("{GAME}\\Override");
        return Some(instruction);
    }
    if BARE_CLEANLIST.is_match(unit) {
        gaps.push(unit.into());
        return None;
    }
    // A duplicate under a new name has no instruction form: a Copy's Destination is a
    // folder, so drafting it would copy or move the original under its old name.
    static DUPLICATE: LazyLock<Regex> =
        LazyLock::new(|| compile(r"\bduplicate\b|\bcopy\s+of\b|\bcopy\b.*\brename\b", false));
    if mode == Mode::Corrected && DUPLICATE.is_match(unit) {
        gaps.push(unit.into());
        return None;
    }
    let patterns = match mode {
        Mode::Pinned => &*INSTRUCTIONS,
        Mode::Corrected => &*CORRECTED_INSTRUCTIONS,
    };
    for (action, pattern) in patterns.iter() {
        // The corrected reading takes the first match that starts a word, so the
        // "move" inside "Remove a.tga" is not a Move of a.tga.
        let found = match mode {
            Mode::Pinned => pattern.captures(unit),
            Mode::Corrected => pattern.captures_iter(unit).find(|m| {
                let start = m.get(0).map_or(0, |whole| whole.start());
                !unit[..start]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
            }),
        };
        if let Some(m) = found
            && let Some(instruction) = from_match(&m, action, unit, mode)
        {
            return Some(instruction);
        }
    }
    if [
        "move", "copy", "delete", "remove", "install", "apply", "run", "execute", "extract",
        "unzip", "place", "put", "rename", "download", "use", "select",
    ]
    .iter()
    .any(|verb| lower.starts_with(&format!("{verb} ")) || lower.contains(&format!(" {verb} ")))
    {
        gaps.push(unit.into());
    }
    None
}

fn empty_instruction(action: &str) -> Instruction {
    Instruction {
        action: action.into(),
        source: Vec::new(),
        destination: String::new(),
        arguments: String::new(),
        overwrite: action != "Delete",
        guid: String::new(),
        parent: String::new(),
        dependencies: Vec::new(),
        restrictions: Vec::new(),
    }
}

fn from_match(m: &Captures<'_>, action: &str, unit: &str, mode: Mode) -> Option<Instruction> {
    let mut instruction = empty_instruction(action);
    if let Some(source) = capture(m, "source") {
        instruction.source = sources(source, unit, action, mode);
        if let Some(other) = capture(m, "source2") {
            instruction
                .source
                .extend(sources(other, unit, action, mode));
        }
    } else if let (Some(start), Some(end)) = (capture(m, "start"), capture(m, "end")) {
        instruction.source = match mode {
            Mode::Pinned => file_range(start, end),
            // "DXN_BWa1 through DXN_BWa8.tpc": the extension written once applies to
            // both ends, giving DXN_BWa*.tpc rather than every DXN_BWa* file.
            Mode::Corrected => match end.rsplit_once('.') {
                Some((_, extension)) if !has_extension(start) && !extension.is_empty() => {
                    file_range(&format!("{start}.{extension}"), end)
                }
                _ => file_range(start, end),
            },
        };
    } else if mode == Mode::Corrected {
        // The pinned reading ignores these captures and falls through to the
        // catch-all, which drafts one letter of "everything" or "files" as a folder.
        if let Some(folders) = capture(m, "folders") {
            instruction.source = corrected_sources(&format!("{folders} folders"), unit, action);
        } else if let (Some(first), Some(second)) = (capture(m, "source1"), capture(m, "source2")) {
            instruction.source = corrected_sources(&format!("{first} folder"), unit, action);
            let second = corrected_sources(&format!("{second} folder"), unit, action);
            if instruction.source.is_empty() || second.is_empty() {
                instruction.source.clear();
            } else {
                instruction.source.extend(second);
            }
        }
    }
    instruction.destination = capture(m, "destination")
        .map(|s| {
            let name = s.trim_matches(['\"', '\'']);
            // C# RenameFile takes the Destination as the new name in the source's folder.
            if action == "Rename" && is_file_name(name) {
                name.to_owned()
            } else {
                destination(s, unit)
            }
        })
        .unwrap_or_else(|| infer_destination(unit, action));
    if action == "Patcher" {
        instruction.arguments = capture(m, "option").unwrap_or("").into();
    }
    if let Some(extension) = capture(m, "extension") {
        instruction.arguments = format!(".{}", extension.to_lowercase());
    }
    if ENTITIES["overwrite"].is_match(unit) {
        instruction.overwrite = true;
    } else if ENTITIES["no_overwrite"].is_match(unit) {
        instruction.overwrite = false;
    }
    for (entity, field) in [("except", "exceptions"), ("ignore", "ignore")] {
        if let Some(m) = ENTITIES[entity].captures(unit) {
            let excluded = exclusions(capture(&m, field).unwrap_or(""));
            instruction.source.retain(|s| {
                !excluded
                    .iter()
                    .any(|e| s.to_lowercase().contains(&e.to_lowercase()))
            });
        }
    }
    static ONLY: LazyLock<Regex> = LazyLock::new(|| {
        compile(
            r"(?:only|just)\s+(?:move|use|install)\s+(?:the\s+)?(?<only>.+?)(?:\.(?:\s|$)|;|,\s+(?:not|ignore)|$)",
            false,
        )
    });
    if let Some(m) = ONLY.captures(unit) {
        let only = sources(capture(&m, "only").unwrap_or(""), unit, action, mode);
        if !only.is_empty() {
            instruction.source = only;
        }
    }
    if matches!(
        action,
        "Move" | "Copy" | "Delete" | "Rename" | "Extract" | "Execute"
    ) && instruction.source.is_empty()
        || matches!(action, "Move" | "Copy") && instruction.destination.trim().is_empty()
    {
        return None;
    }
    Some(instruction)
}

fn sources(source: &str, unit: &str, action: &str, mode: Mode) -> Vec<String> {
    // Patcher, Execute and DelDuplicate sources are tool keywords ("installer",
    // "DelDuplicateTGA-TPC") that the engine resolves; they keep the pinned reading.
    if mode == Mode::Corrected
        && matches!(action, "Move" | "Copy" | "Delete" | "Rename" | "Extract")
    {
        return corrected_sources(source, unit, action);
    }
    // "Rename a.tga to b.tga" names one source and its new name, not a file range.
    let range = action != "Rename";
    if range && let Some(m) = ENTITIES["file_range"].captures(unit) {
        return file_range(capture(&m, "start").unwrap(), capture(&m, "end").unwrap());
    }
    // The reference's numeric-range regex has no "start"/"end" captures. Its existing
    // fallback consequently produces two root placeholders, which remain review-only.
    if range && numeric_range(unit) {
        return file_range("", "");
    }
    for kind in ["folder_from", "folder_name"] {
        if let Some(m) = ENTITIES[kind].captures(source) {
            return ["folder", "folder2"]
                .into_iter()
                .filter_map(|key| capture(&m, key))
                .map(|s| format!("{MODS}\\{}\\*", s.trim_matches(['\"', '\'', ' '])))
                .collect();
        }
    }
    if let Some(m) = ENTITIES["file_list"].captures(source) {
        let files = file_list(capture(&m, "files").unwrap());
        if !files.is_empty() {
            return files.iter().map(|s| format!("{MODS}\\{s}")).collect();
        }
    }
    for (entity, group) in [("wildcard", "pattern"), ("single_file", "file")] {
        if let Some(m) = ENTITIES[entity].captures(source) {
            return vec![format!(
                "{MODS}\\{}",
                capture(&m, group).unwrap().trim_matches(['\"', '\''])
            )];
        }
    }
    let cleaned = source.trim_matches(['\"', '\'', ' ', ',', ';']);
    if cleaned.is_empty() {
        return Vec::new();
    }
    let folder = !has_extension(cleaned)
        && !cleaned.contains(['*', '?'])
        && matches!(action, "Move" | "Copy");
    vec![format!(
        "{MODS}\\{cleaned}{}",
        if folder { "\\*" } else { "" }
    )]
}

/// Sources for the corrected reading: a file range, a file list, a named folder, a
/// wildcard, a file name, a quoted name or one path. Anything else is prose such as
/// "the files from the patch", and drafts no source, so the step stays a review gap.
fn corrected_sources(source: &str, unit: &str, action: &str) -> Vec<String> {
    static FOLDER: LazyLock<Regex> = LazyLock::new(|| {
        compile(
            r"^(?<before>.*?)\s+(?:(?<plural>folders|directories)|folder|directory)\b",
            false,
        )
    });
    let folderish = |name: &str| {
        let folder = !has_extension(name) && !name.contains(['*', '?']);
        format!(
            "{MODS}\\{name}{}",
            if folder && matches!(action, "Move" | "Copy") {
                "\\*"
            } else {
                ""
            }
        )
    };
    let range = action != "Rename";
    if range && let Some(m) = ENTITIES["file_range"].captures(unit) {
        return file_range(capture(&m, "start").unwrap(), capture(&m, "end").unwrap());
    }
    // "PFBI01 through PFBI04" gives no extension to build a pattern from. The pinned
    // reading drafts the mod directory root itself here.
    if range && numeric_range(unit) {
        return Vec::new();
    }
    // "the resolution folder corresponding to the HQ Cockpit Skyboxes option" picks a
    // folder by description; its words are not its name.
    static QUALIFIED: LazyLock<Regex> = LazyLock::new(|| {
        compile(
            r"\b(?:folders?|director(?:y|ies))\s+(?:corresponding|matching|appropriate|that\s+(?:matches|corresponds)|which\s+(?:matches|corresponds))\b",
            false,
        )
    });
    // The words ahead of the last "from"/"in": "w_lghtsbr_001.tga from the alt_texture".
    static HEAD: LazyLock<Regex> =
        LazyLock::new(|| compile(r"^(?<head>.*)\s(?:in|from|within|inside)\s", true));
    if let Some(m) = FOLDER.captures(source) {
        if QUALIFIED.is_match(unit) {
            return Vec::new();
        }
        let before = capture(&m, "before").unwrap_or("");
        let labels = folder_labels(before, m.name("plural").is_some());
        let files = HEAD
            .captures(before)
            .and_then(|head| capture(&head, "head").map(named_files))
            .unwrap_or_default();
        return match (labels.as_slice(), files.is_empty()) {
            // One named file inside one named folder.
            ([label], false) => files
                .iter()
                .map(|file| format!("{MODS}\\{label}\\{file}"))
                .collect(),
            // Named files in a folder the words do not name: where they are is unknown.
            (_, false) => Vec::new(),
            (_, true) => labels
                .iter()
                .map(|label| format!("{MODS}\\{label}\\*"))
                .collect(),
        };
    }
    if let Some(m) = ENTITIES["file_list"].captures(source) {
        let files = file_list(capture(&m, "files").unwrap());
        if !files.is_empty() {
            return files.iter().map(|s| format!("{MODS}\\{s}")).collect();
        }
    }
    for (entity, group) in [("wildcard", "pattern"), ("single_file", "file")] {
        if let Some(m) = ENTITIES[entity].captures(source) {
            return vec![format!(
                "{MODS}\\{}",
                capture(&m, group).unwrap().trim_matches(['\"', '\''])
            )];
        }
    }
    // `"name"` or `files from "name"`; not `files beginning "PFBBL"`, which picks out
    // some files by prefix.
    static WHOLE_QUOTED: LazyLock<Regex> = LazyLock::new(|| {
        compile(
            r#"^(?:.*\b(?:from|in|within|inside)\s+(?:the\s+)?)?(?:"(?<double>[^"]+)"|'(?<single>[^']+)')$"#,
            false,
        )
    });
    if let Some(m) = WHOLE_QUOTED.captures(source.trim()) {
        let name = capture(&m, "double").or_else(|| capture(&m, "single"));
        if let Some(name) = name.filter(|name| !name.is_empty()) {
            return vec![folderish(name)];
        }
    }
    let cleaned = source.trim_matches(['\"', '\'', ' ', ',', ';']);
    if !cleaned.is_empty()
        && !cleaned.contains(char::is_whitespace)
        && cleaned.contains(['/', '\\'])
    {
        return vec![folderish(cleaned)];
    }
    Vec::new()
}

/// File names, a file list or a wildcard named in `text`, without a root.
fn named_files(text: &str) -> Vec<String> {
    if let Some(m) = ENTITIES["file_list"].captures(text) {
        let files = file_list(capture(&m, "files").unwrap());
        if !files.is_empty() {
            return files;
        }
    }
    for (entity, group) in [("wildcard", "pattern"), ("single_file", "file")] {
        if let Some(m) = ENTITIES[entity].captures(text) {
            return vec![
                capture(&m, group)
                    .unwrap()
                    .trim_matches(['\"', '\''])
                    .to_owned(),
            ];
        }
    }
    Vec::new()
}

/// Quoted names in `text`. A single quote counts only at a word edge, so the
/// apostrophes in "Ashton's" or "don't" never open a name.
fn quoted(text: &str) -> Vec<String> {
    static QUOTED: LazyLock<Regex> = LazyLock::new(|| {
        compile(
            r#""(?<double>[^"]+)"|(?:^|[\s(])'(?<single>[^']+)'(?:$|[\s),.;:])"#,
            false,
        )
    });
    QUOTED
        .captures_iter(text)
        .filter_map(|m| capture(&m, "double").or_else(|| capture(&m, "single")))
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The folder names in the words before "folder": quoted names, or else the words
/// after the last "in"/"from"/"within"/"inside" without a leading article, split on
/// "and" and commas when the guide says "folders". "move all the files in the
/// Creatures folder" names `Creatures`, not `files in the Creatures`. A choice ("A or
/// B"), a pronoun or a word such as "files" names nothing, and then no folder is
/// returned at all, because a partial list would read as the whole step.
fn folder_labels(before: &str, plural: bool) -> Vec<String> {
    static LEAD: LazyLock<Regex> =
        LazyLock::new(|| compile(r"^(?:.*\s)?(?:in|from|within|inside)\s+", true));
    static ARTICLES: LazyLock<Regex> = LazyLock::new(|| {
        compile(
            r"^(?:(?:the|your|both|all|these|those|(?:the\s+)?mod'?s)\s+)+",
            false,
        )
    });
    static LIST: LazyLock<Regex> =
        LazyLock::new(|| compile(r"\s*(?:,\s*(?:and\s+)?|\s+and\s+|\s+&\s+)\s*", false));
    const GENERIC: [&str; 24] = [
        "a",
        "an",
        "the",
        "it",
        "them",
        "this",
        "that",
        "these",
        "those",
        "all",
        "any",
        "both",
        "each",
        "either",
        "everything",
        "file",
        "files",
        "loose files",
        "content",
        "contents",
        "mod",
        "mods",
        "following",
        "same",
    ];
    // "your main game folder" or "the game's Override folder" is where files go, not a
    // folder in the mod.
    static GAME_OWNED: LazyLock<Regex> =
        LazyLock::new(|| compile(r"^(?:your|(?:the\s+)?(?:game|kotor|swkotor)'?s)\s", false));
    // A quoted name counts only when it is the folder's own name ("the "green"
    // folder"). Quotes earlier in the words ("files beginning "PFBBL" from the Player
    // Clothing folder") pick out some of the folder's files, which a folder source
    // cannot say, so the step stays a gap.
    let mut labels = quoted(before);
    let named_by_quotes = before.trim_end().ends_with(['"', '\'']);
    if !labels.is_empty() && !named_by_quotes {
        return Vec::new();
    }
    if labels.is_empty() {
        let text = LEAD.replace(before.trim(), "");
        if GAME_OWNED.is_match(text.trim()) {
            return Vec::new();
        }
        let text = ARTICLES.replace(text.trim(), "");
        labels = if plural {
            LIST.split(text.trim()).map(str::to_owned).collect()
        } else {
            vec![text.trim().to_owned()]
        };
    } else if !plural {
        labels.drain(..labels.len() - 1);
    }
    // "one of those", "which ever you chose": a choice, not a name.
    const CHOOSING: [&str; 12] = [
        "one",
        "each",
        "either",
        "any",
        "which",
        "whichever",
        "this",
        "that",
        "these",
        "those",
        "its",
        "their",
    ];
    // "the corresponding folder", "the base folder": which folder depends on the
    // archive or on a choice, so the guide's words do not name one.
    const DESCRIPTIVE: [&str; 12] = [
        "corresponding",
        "appropriate",
        "relevant",
        "respective",
        "chosen",
        "preferred",
        "desired",
        "selected",
        "base",
        "main",
        "root",
        "top-level",
    ];
    let named = |label: &String| {
        let lower = label.trim().to_lowercase();
        let first = lower.split_whitespace().next().unwrap_or("");
        let quoted_or = |test: bool| named_by_quotes || test;
        !lower.is_empty()
            && quoted_or(!CHOOSING.contains(&first))
            && quoted_or(!DESCRIPTIVE.contains(&lower.as_str()))
            // "the 7 folders within ..." counts folders, it does not name one.
            && quoted_or(!lower.chars().all(|c| c.is_ascii_digit()))
            && quoted_or(
                ![" or ", " to ", " into "]
                    .iter()
                    .any(|word| lower.contains(word)),
            )
            && !GENERIC.contains(&lower.as_str())
    };
    if !labels.iter().all(named) {
        return Vec::new();
    }
    labels.iter().map(|label| label.trim().to_owned()).collect()
}

fn numeric_range(text: &str) -> bool {
    static PAIR: LazyLock<Regex> =
        LazyLock::new(|| compile(r"(?<left>[\w\-_.]+)\s+through\s+(?<right>[\w\-_.]+)", false));
    for m in PAIR.captures_iter(text) {
        let left = capture(&m, "left").unwrap();
        let right = capture(&m, "right").unwrap().to_lowercase();
        for (start, _) in left.char_indices() {
            let token = &left[start..];
            for (digit, ch) in token.char_indices().skip(1) {
                if !ch.is_ascii_digit() {
                    continue;
                }
                let prefix = &token[..digit];
                if !prefix
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-'))
                {
                    continue;
                }
                let Some(rest) = right.strip_prefix(&prefix.to_lowercase()) else {
                    continue;
                };
                let left_digits = token[digit..]
                    .bytes()
                    .take_while(u8::is_ascii_digit)
                    .count();
                let right_digits = rest.bytes().take_while(u8::is_ascii_digit).count();
                for a in (1..=left_digits).rev() {
                    let suffix = token[digit + a..].to_lowercase();
                    for b in (1..=right_digits).rev() {
                        if rest[b..].starts_with(&suffix)
                            || rest[b..].strip_prefix('.').is_some_and(|s| {
                                s.chars()
                                    .next()
                                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
                            })
                        {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// One file name with an extension that the engine takes as a Rename Destination
/// (`engine::rename_file_name`): no folder part, placeholder or drive, and valid on
/// Windows. Any other drafted new name keeps the placeholder Destination.
fn is_file_name(name: &str) -> bool {
    has_extension(name) && crate::engine::rename_file_name(name).is_ok()
}

fn has_extension(path: &str) -> bool {
    path.rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .rsplit_once('.')
        .is_some_and(|(_, ext)| !ext.is_empty())
}
fn file_range(start: &str, end: &str) -> Vec<String> {
    let (start_base, extension) = start
        .rsplit_once('.')
        .map(|(base, ext)| (base, format!(".{ext}")))
        .unwrap_or((start, String::new()));
    let end_base = end.rsplit_once('.').map_or(end, |(base, _)| base);
    let prefix: String = start_base
        .chars()
        .zip(end_base.chars())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a)
        .collect();
    if !prefix.is_empty() {
        vec![format!("{MODS}\\{prefix}*{extension}")]
    } else {
        vec![format!("{MODS}\\{start}"), format!("{MODS}\\{end}")]
    }
}
fn file_list(text: &str) -> Vec<String> {
    static SPLIT: LazyLock<Regex> =
        LazyLock::new(|| compile(r"\s*(?:,|;|\s+and\s+|\s+&\s+)\s*", false));
    SPLIT
        .split(text)
        .map(|s| s.trim().trim_matches(['\"', '\'', ' ']))
        .filter(|s| {
            !s.is_empty()
                && !s.eq_ignore_ascii_case("and")
                && !s.eq_ignore_ascii_case("the")
                && (has_extension(s) || s.contains('*'))
        })
        .map(str::to_owned)
        .collect()
}
fn exclusions(text: &str) -> Vec<String> {
    static PARENS: LazyLock<Regex> = LazyLock::new(|| compile(r"\((?<files>[^)]+)\)", false));
    let captures = PARENS.captures(text);
    let text = captures
        .as_ref()
        .and_then(|m| capture(m, "files"))
        .unwrap_or(text);
    let mut result = file_list(text);
    if let Some(m) = ENTITIES["folder_name"].captures(text) {
        result.push(capture(&m, "folder").unwrap().into());
    }
    result
}

fn destination(text: &str, unit: &str) -> String {
    let lower = text.trim().to_lowercase();
    for (phrase, tail) in [
        ("override", "\\Override"),
        ("override folder", "\\Override"),
        ("override directory", "\\Override"),
        ("game's override", "\\Override"),
        ("your override", "\\Override"),
        ("main game directory", ""),
        ("main directory", ""),
        ("game directory", ""),
        ("movies folder", "\\Movies"),
        ("movies", "\\Movies"),
        ("your movies folder", "\\Movies"),
    ] {
        if lower.contains(phrase) || lower == phrase.replace(' ', "") {
            return format!("{GAME}{tail}");
        }
    }
    let unit = unit.to_lowercase();
    if unit.contains("override") {
        return format!("{GAME}\\Override");
    }
    if unit.contains("movies") {
        return format!("{GAME}\\Movies");
    }
    if text.contains(['/', '\\']) {
        return format!("{MODS}\\{text}");
    }
    format!("{GAME}\\Override")
}
fn infer_destination(unit: &str, action: &str) -> String {
    let lower = unit.to_lowercase();
    if ["to override", "to your override", "in override"]
        .iter()
        .any(|s| lower.contains(s))
    {
        return format!("{GAME}\\Override");
    }
    if ["to movies", "in your movies", "movies folder"]
        .iter()
        .any(|s| lower.contains(s))
    {
        return format!("{GAME}\\Movies");
    }
    if action == "Patcher"
        || ["main game directory", "main directory", "game directory"]
            .iter()
            .any(|s| lower.contains(s))
    {
        return GAME.into();
    }
    if matches!(action, "Move" | "Copy") {
        return format!("{GAME}\\Override");
    }
    String::new()
}

/// Root placeholders are mandatory, except for a Rename's new file name; final filesystem
/// resolution remains the engine's responsibility.
pub fn sanitize(instruction: &mut Instruction) -> bool {
    fn safe(path: &str) -> bool {
        [MODS, GAME].iter().any(|root| {
            path.strip_prefix(root).is_some_and(|tail| {
                tail.is_empty()
                    || (tail.starts_with(['/', '\\'])
                        && tail[1..]
                            .split(['/', '\\'])
                            .all(|part| part != ".." && !part.contains(':')))
            })
        })
    }
    instruction.destination = instruction.destination.replace("<<gameDirectory>>", GAME);
    let new_name = instruction.action == "Rename" && is_file_name(&instruction.destination);
    if !instruction.destination.is_empty() && !new_name && !safe(&instruction.destination) {
        return false;
    }
    instruction.source = instruction
        .source
        .iter()
        .map(|s| s.replace("<<gameDirectory>>", GAME))
        .filter(|s| safe(s))
        .collect();
    if matches!(
        instruction.action.as_str(),
        "Move" | "Copy" | "Delete" | "Rename" | "Extract" | "Execute" | "Patcher"
    ) && instruction.source.is_empty()
    {
        return false;
    }
    !matches!(instruction.action.as_str(), "Move" | "Copy")
        || !instruction.destination.trim().is_empty()
}

fn download_options(text: &str, parent: &str, mode: Mode) -> Vec<Component> {
    static VERSION: LazyLock<Regex> = LazyLock::new(|| {
        compile(
            r"(?:download|use|choose|install|get|select)\s+(?:the\s+)?(?<option>[^.,;!?\r\n]+?)\s+(?:version|variant|option|file|edition)",
            false,
        )
    });
    let recommendations = match mode {
        Mode::Pinned => &*RECOMMENDATIONS,
        Mode::Corrected => &*CORRECTED_RECOMMENDATIONS,
    };
    let mut result: Vec<Component> = Vec::new();
    for (index, pattern) in recommendations
        .iter()
        .chain(std::iter::once(&*VERSION))
        .enumerate()
    {
        for m in pattern.captures_iter(text) {
            let Some(name) = capture(&m, "option") else {
                continue;
            };
            // "Download only the Damaged version" offers "Damaged".
            static LEADING: LazyLock<Regex> =
                LazyLock::new(|| compile(r"^(?:(?:only|just)\s+)?(?:the\s+)?", false));
            let name = match mode {
                Mode::Pinned => name,
                Mode::Corrected => LEADING
                    .find(name)
                    .map_or(name, |lead| name[lead.end()..].trim()),
            };
            let start = m.get(0).map_or(0, |whole| whole.start());
            if negated_recommendation(&text[..start], name)
                || name.chars().count() < 3
                || [
                    "this", "that", "these", "those", "it", "them", "the", "a", "an", "all",
                    "both", "either", "you", "your", "my", "our",
                ]
                .contains(&name.to_lowercase().as_str())
                || result.iter().any(|c| c.name.eq_ignore_ascii_case(name))
                || mode == Mode::Corrected
                    && (unnamed_option(name) || conditional_sentence(&text[..start]))
            {
                continue;
            }
            let recommended = index < recommendations.len();
            result.push(Component {
                guid: crate::markdown::generated_id(&format!("{parent}:{name}"), result.len()),
                name: name.into(),
                description: format!(
                    "{}: {name}",
                    if recommended {
                        "Recommended option"
                    } else {
                        "Option"
                    }
                ),
                is_selected: recommended && text.to_lowercase().contains("recommend"),
                ..Component::default()
            });
        }
    }
    result
}

/// A download recommendation phrased against its option names something not to choose:
/// `recommend against the "ZaalbarHD (...)" version`, `which I recommend against`, or a
/// verb after a negation, as in `do not download the "stunbaton 2025" file`. The pinned
/// C# parser drafts these as options (`against`, `"stunbaton 2025"`) and preselects
/// them; Rust skips them. `before` is the text ahead of the match.
/// A captured phrase that names no option: "and install both" from "Download and
/// install both files", "" left of "only the" in "Download only the version marked
/// ...", or words such as "main" or "mod" alone.
fn unnamed_option(name: &str) -> bool {
    const LEADING: [&str; 4] = ["and", "or", "then", "also"];
    const FILLER: [&str; 20] = [
        "main", "a", "an", "the", "this", "that", "these", "those", "it", "them", "both", "either",
        "all", "any", "one", "mod", "mods", "file", "files", "version",
    ];
    let words: Vec<String> = name
        .split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect();
    words
        .first()
        .is_some_and(|word| LEADING.contains(&word.as_str()))
        || words.iter().all(|word| FILLER.contains(&word.as_str()))
}

/// The match sits in a sentence that starts with "If": "If wishing to use a
/// non-English mod version, download ..." describes a case, not a choice to draft.
fn conditional_sentence(before: &str) -> bool {
    let start = before.rfind(['.', '!', '?', '\n']).map_or(0, |i| i + 1);
    before[start..]
        .trim_start()
        .to_lowercase()
        .starts_with("if ")
}

fn negated_recommendation(before: &str, name: &str) -> bool {
    static AGAINST: LazyLock<Regex> = LazyLock::new(|| compile(r"^(?:against|not)\b", false));
    static NEGATION: LazyLock<Regex> =
        LazyLock::new(|| compile(r"(?:\bnot|n't|\bnever|\bavoid|\bskip|\bignore)\s*$", false));
    AGAINST.is_match(name) || NEGATION.is_match(before)
}

// Ordered patterns from pinned C# NaturalLanguageInstructionParser; see docs/parity/reference.json.
static INSTRUCTIONS: LazyLock<Vec<(&str, Regex)>> = LazyLock::new(|| {
    vec![
        (
            "Move",
            compile(
                r###"move\s+everything\s+from\s+the\s+(?<folders>(?:[\w\s\-_]+?,\s+)+(?:and\s+)?[\w\s\-_]+?)\s+folders?\s+to\s+(?:your\s+)?(?<destination>[\w\s\-_/\\]+)"###,
                false,
            ),
        ),
        (
            "Move",
            compile(
                r###"move\s+(?:the\s+)?files?\s+from\s+(?:both\s+)?(?:the\s+)?(?<source1>[\w\s\-_/\\"']+?)\s+(?:and|&)\s+(?<source2>[\w\s\-_/\\"']+?)\s+folders?\s+to\s+(?:your\s+)?(?<destination>[\w\s\-_/\\]+)"###,
                false,
            ),
        ),
        ("Move", compile(PINNED_MOVE_ALL, false)),
        (
            "Move",
            compile(
                r###"(?:move|copy)\s+(?:the\s+)?(?:files?|content|everything)\s+from\s+["'](?<source>[^"']+)["']\s+to\s+(?:your\s+)?(?<destination>[\w\s\-_/\\]+)"###,
                false,
            ),
        ),
        (
            "Move",
            compile(
                r###"(?:move|copy)\s+(?<source>(?:[\w\-_.]+\.[\w]+(?:\s+&\s+\.[\w]+)?(?:,\s*|\s+and\s+|\s+&\s+|\s+plus\s+))+[\w\-_.]+\.[\w]+)(?:\s+to\s+(?<destination>[\w\s\-_/\\]+?))?"###,
                false,
            ),
        ),
        (
            "Move",
            compile(
                r###"(?:(?:move|copy|install)\s+(?:only|just|specifically)|(?:only|just)\s+(?:move|copy|install))\s+(?:the\s+)?(?<source>[\w\s\-_.&/\\()"']+?)(?:\s+to\s+(?:your\s+)?(?<destination>[\w\s\-_/\\]+?))?(?:\.(?:\s|$)|$|,)"###,
                false,
            ),
        ),
        (
            "Move",
            compile(
                r###"move\s+ONLY\s+(?<source>[\w\s\-_.&/\\()]+?)(?:\s+to\s+(?<destination>[\w\s\-_/\\]+?))?(?:\.(?:\s|$)|$)"###,
                false,
            ),
        ),
        (
            "Move",
            compile(
                r###"move\s+all\s+["']?(?<source>[\w\-_*?]+)["']?\s+files?"###,
                false,
            ),
        ),
        (
            "Move",
            compile(
                r###"move\s+the\s+(?:[\w]+)\s+files?\s+within(?:\s+\(NOT\s+including\s+(?<exclude>.+?)\))?"###,
                false,
            ),
        ),
        (
            "Move",
            compile(
                r###"(?:the\s+)?(?:files?\s+)?(?:from\s+)?(?<source>[\w\s\-_/\\"']+?)\s+should\s+be\s+moved\s+(?:from\s+(?:the\s+)?(?<source_folder>[\w\s\-_]+?)\s+)?(?:to\s+)?(?:(?:the\s+)?(?<destination>[\w\s\-_/\\]+?))?"###,
                false,
            ),
        ),
        ("Move", compile(PINNED_MOVE_ANY, false)),
        (
            "Delete",
            compile(
                r###"delete\s+["'](?<start>[\w\-_.]+?)["']\s+through\s+["'](?<end>[\w\-_.]+?)["']?\s+before"###,
                false,
            ),
        ),
        (
            "Delete",
            compile(
                r###"\(sorted\s+by\s+name\)\s+(?<start>[\w\-_.]+?)\s+through\s+(?<end>[\w\-_.]+)"###,
                false,
            ),
        ),
        (
            "Delete",
            compile(
                r###"(?:delete|remove)\s+(?<source>(?:[\w\-_.]+(?:\s+&\s+\.[\w]+)?(?:,\s*|\s+and\s+|\s+&\s+|\s+plus\s+))+[\w\-_.]+)(?:\s+before|\s+after|\.(?:\s|$)|\s+from|$)"###,
                false,
            ),
        ),
        (
            "Delete",
            compile(
                r###"delete\s+everything\s+inside\s+(?:the\s+)?(?<source>[\w\s\-_/\\]+?)\s+except\s+(?<exceptions>.+?)(?:\.(?:\s|$)|$)"###,
                false,
            ),
        ),
        (
            "Delete",
            compile(
                r###"if\s+present\s+in\s+your\s+override,?\s+(?<source>[\w\s\-_.&/\\]+?)\s+must\s+be\s+deleted"###,
                false,
            ),
        ),
        (
            "Delete",
            compile(
                r###"(?:delete|remove)\s+(?:the\s+)?(?:file\s+)?(?<source>[\w\s\-_/\\*.,()&"']+?)(?:\s+before|\s+after|\s+from|\.(?:\s|$)|\s+in\s+your|$)"###,
                false,
            ),
        ),
        ("Extract", compile(PINNED_EXTRACT_ANY, false)),
        (
            "Extract",
            compile(
                r###"(?:once\s+)?(?:the\s+)?(?:mod\s+is\s+)?(?:extracted|downloaded),?\s+(?:enter|navigate\s+to|go\s+into)\s+(?:the\s+)?(?<source>[\w\s\-_/\\]+?)\s+folder"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"(?:run|execute)\s+(?:the\s+)?(?:installer|patcher)\s+(?<times>[\w]+)\s+times?,?\s+(?:once\s+)?(?:to\s+install|for)\s+(?:each\s+of\s+)?(?:the\s+)?(?:options\s+)?(?<options>.+?)(?:\:|$)"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"install\s+(?:the\s+)?(?:main\s+mod|base\s+mod),?\s+then\s+re-run\s+(?:the\s+)?(?:patcher|installer)\s+and\s+select\s+(?<option>.+?)(?:\.(?:\s|$)|$|,\s+if)"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"run\s+the\s+(?:patcher|installer)\s+using\s+the\s+(?<option>.+?)\s+(?:install\s+)?option"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"when\s+installing,?\s+there\s+will\s+be\s+several\s+optional\s+files"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"select\s+the\s+(?<option>[\w\s\-_]+?)-compatible\s+installation"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"use\s+the\s+(?<option>[\w\s\-_/\\.,&()]+?)\s+(?:version|install|installation|option)"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"install\s+(?<option1>.+?),?\s+then\s+re-run\s+(?:it\s+)?(?:once\s+more|twice\s+more|again)?(?:,?\s+(?:once\s+)?for\s+(?:each\s+of\s+)?(?:the\s+)?)?(?<option2>.+?)(?:\.(?:\s|$)|$)"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"(?:the\s+)?installer\s+(?:for\s+this\s+mod\s+)?will\s+need\s+to\s+be\s+run\s+(?<times>[\w]+)\s+times"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"don't\s+use\s+the\s+complete\s+installer(?:,\s+instead\s+selecting\s+the\s+(?<option>.+?))?"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"run\s+the\s+holopatcher\s+executable(?:\.\s+select\s+the\s+(?<option>.+?)\s+install)?"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"go\s+into\s+the\s+(?<folder>[\w\s\-_"']+?)\s+folder\s+and\s+(?:install|apply)\s+(?:that\s+)?(?<option>[\w\s\-_]+)"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"(?:run|execute|apply|use|install)\s+(?:the\s+)?(?<source>(?:installer|patcher|tslpatcher|holopatcher))(?:\s+(?:first|to\s+install|for|executable))?"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"(?:install|apply|select|use|choose)\s+(?:the\s+)?(?<option>[\w\s\-_/\\.,&()"']+?)\s+(?:install\s+)?(?:option|version|patch|compatibility|compatch|install)(?:\s+(?:if\s+using|when\s+using))?"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"re-run\s+(?:the\s+)?(?:patcher|installer)(?:\s+(?:and|to))?\s+(?:select|install|apply)?\s*(?<option>[\w\s\-_/\\.,&()"']*?)(?:\s+option)?(?:\.(?:\s|$)|$|,)"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"apply\s+the\s+(?<option>(?:patch|main\s+installation|contents?|base\s+mod))(?:\s+(?:first|to))?"###,
                false,
            ),
        ),
        (
            "Copy",
            compile(
                r###"make\s+a\s+copy\s+of\s+(?:the\s+)?(?:file\s+)?["']?(?<source>[\w\-_.]+?)["']?\s+(?:and\s+)?paste\s+it\s+into\s+the\s+same\s+(?:directory|folder|location)(?:.*?)rename\s+(?:this\s+duplicate|it)\s+to\s+["']?(?<destination>[\w\-_.]+)["']?"###,
                true,
            ),
        ),
        (
            "Copy",
            compile(
                r###"make\s+a\s+copy\s+of\s+(?:the\s+)?(?:file\s+)?["']?(?<source>[\w\-_.]+?)["']?\s+and\s+rename\s+(?:it\s+)?(?:to\s+)?["']?(?<destination>[\w\-_.]+)["']?"###,
                false,
            ),
        ),
        (
            "Copy",
            compile(
                r###"(?:copy|duplicate)\s+(?:the\s+)?(?:file\s+)?(?<source>[\w\-_.]+?).*?rename\s+(?:it|that|this\s+duplicate)\s+to\s+["']?(?<destination>[\w\-_.]+)["']?"###,
                true,
            ),
        ),
        (
            "Rename",
            compile(
                r###"rename\s+(?:the\s+)?files?\s+["']?(?<source>[\w\-_.]+?)["']?\s+(?:to|as)\s+["']?(?<destination>[\w\-_.]+)["']?"###,
                false,
            ),
        ),
        (
            "Rename",
            compile(
                r###"rename\s+(?<source>[\w\-_.]+?)\s+to\s+(?<destination>[\w\-_.]+)"###,
                false,
            ),
        ),
        (
            "Delete",
            compile(
                r###"before\s+moving(?:\s+the\s+files?)?(?:\s+to\s+(?:your\s+)?(?:override|game).*?)?,?\s+(?:be\s+sure\s+to\s+)?delete\s+(?:the\s+following(?:\s+files?)?:?\s*)?(?<source>.+?)(?:\.(?:\s|$)|$)"###,
                false,
            ),
        ),
        (
            "Delete",
            compile(
                r###"delete\s+(?<source>[\w\-_.]+?)(?:\s+&\s+\.[\w]+)+\s+before"###,
                false,
            ),
        ),
        (
            "Delete",
            compile(
                r###"delete\s+["']?(?<start>[\w\-_.]+?)["']?\s+through\s+["']?(?<end>[\w\-_.]+)["']?(?:\s+before|\s+in)?"###,
                false,
            ),
        ),
        (
            "Delete",
            compile(
                r###"\(sorted\s+by\s+name\)\s+(?<start>[\w\-_.]+?)\s+(?:\*\*)?through(?:\*\*)?\s+(?<end>[\w\-_.]+)"###,
                false,
            ),
        ),
        (
            "Delete",
            compile(
                r###"delete\s+the\s+following(?:\s+files?)?:?\s+(?<source>.+?)(?:\.(?:\s|$)|$)"###,
                false,
            ),
        ),
        (
            "Delete",
            compile(
                r###"(?<source>[\w\s\-_.&/\\()]+?)\s+must\s+be\s+deleted"###,
                false,
            ),
        ),
        (
            "Delete",
            compile(
                r###"(?:delete|remove)\s+(?<source>[\w\s\-_/\\*.,()&"']+?)(?:\s+before|\s+after|\.(?:\s|$)|\s+from)"###,
                false,
            ),
        ),
        (
            "Extract",
            compile(r###"(?:extract|unzip)\s+(?:the\s+)?mod"###, false),
        ),
        (
            "Extract",
            compile(
                r###"(?:extract|unzip|decompress)\s+(?<source>[\w\s\-_/\\]+?)(?:\s+to\s+(?<destination>[\w\s\-_/\\]+?))?"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"(?:when\s+)?installing,?\s+(?:select|use|choose)\s+(?<option>.+?)(?:\.(?:\s|$)|;|$)"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"re-run\s+the\s+(?:patcher|installer)(?:\s+and)?\s+(?:select|install)?\s*(?<option>.*?)(?:\.(?:\s|$)|$|,\s+if)"###,
                false,
            ),
        ),
        (
            "Patcher",
            compile(
                r###"(?:use|apply)\s+the\s+tslpatcher(?:\s+installer\s+method)?"###,
                false,
            ),
        ),
        (
            "Execute",
            compile(
                r###"place\s+(?<source>[\w\s\-_.]+?)\s+in\s+(?:your\s+)?(?<destination>[\w\s\-_/\\]+?)(?:,\s+|\s+and\s+)run\s+(?:it|the\s+file)"###,
                false,
            ),
        ),
        (
            "Execute",
            compile(
                r###"download\s+(?:one\s+of\s+)?(?:the\s+)?(?<source>[\w\s\-_.]+?).*?(?:run|execute)\s+(?:the\s+)?(?:executable|file)"###,
                true,
            ),
        ),
        (
            "DelDuplicate",
            compile(
                r###"place\s+(?<source>DelDuplicateTGA-TPC|bash\s+script\s+version)\s+in\s+your\s+main\s+game\s+folder"###,
                false,
            ),
        ),
        (
            "DelDuplicate",
            compile(
                r###"say\s+that\s+\*\*(?<extension>TPC|TGA)\s+should\s+be\s+deleted\*\*"###,
                false,
            ),
        ),
        (
            "DelDuplicate",
            compile(
                r###"run\s+it,?\s+say\s+that\s+(?<extension>TPC|TGA)\s+should\s+be\s+deleted"###,
                false,
            ),
        ),
    ]
});
static ENTITIES: LazyLock<BTreeMap<&str, Regex>> = LazyLock::new(|| {
    BTreeMap::from([
        (
            "file_range",
            compile(
                r###"["']?(?<start>[\w\-_]+\.[\w]+)["']?\s+(?:through|to|-)\s+["']?(?<end>[\w\-_]+\.[\w]+)["']?"###,
                false,
            ),
        ),
        (
            "file_list",
            compile(
                r###"(?<files>(?:[\w\-_.]+\.[\w]+(?:\s+&\s+\.[\w]+)?(?:,\s*|\s+and\s+|\s+&\s+))+[\w\-_.]+\.[\w]+)"###,
                false,
            ),
        ),
        (
            "single_file",
            compile(r###"["']?(?<file>[\w\-_]+\.[\w]{2,4})["']?"###, false),
        ),
        (
            "folder_from",
            compile(
                r###"from\s+(?:the\s+)?(?:both\s+the\s+)?(?<folder>[\w\s\-_&/\\]+?)\s+(?:and\s+(?<folder2>[\w\s\-_/\\]+?)\s+)?(?:folder|directory)"###,
                false,
            ),
        ),
        (
            "folder_name",
            compile(
                r###"(?:the\s+)?["']?(?<folder>[\w\s\-_/\\]+?)["']?\s+folder"###,
                false,
            ),
        ),
        (
            "override_dest",
            compile(
                r###"(?:to\s+)?(?:your\s+)?(?:game'?s?\s+)?override(?:\s+(?:folder|directory))?"###,
                false,
            ),
        ),
        (
            "main_dir_dest",
            compile(
                r###"(?:to\s+)?(?:the\s+)?main\s+(?:game\s+)?(?:directory|folder)"###,
                false,
            ),
        ),
        (
            "movies_dest",
            compile(
                r###"(?:to\s+)?(?:your\s+)?(?:game'?s?\s+)?movies\s+folder"###,
                false,
            ),
        ),
        (
            "wildcard",
            compile(r###"(?<pattern>[\w\-_*?]+\*[\w\-_*?]*)"###, false),
        ),
        (
            "except",
            compile(
                r###"(?:except|excluding|but\s+not|not\s+including)(?:\s+(?:for|the))?\s+(?:the\s+)?(?<exceptions>.+?)(?:\s*(?:\(|:|;|\.(?:\s|$)|$))"###,
                true,
            ),
        ),
        (
            "ignore",
            compile(
                r###"(?:ignore|skip|don't\s+(?:install|move|use))\s+(?:the\s+)?(?<ignore>.+?)(?:\.(?:\s|$)|;|,\s+you|\s+unless|$)"###,
                false,
            ),
        ),
        (
            "overwrite",
            compile(
                r###"(?:overwrite|replace)(?:\s+when\s+prompted|\s+if\s+(?:asked|prompted))?"###,
                false,
            ),
        ),
        (
            "no_overwrite",
            compile(r###"(?:do\s+not|don't)\s+overwrite"###, false),
        ),
        (
            "file_types",
            compile(
                r###"(?<types>(?:\.[\w]+(?:\s+&\s+|,\s*|\s+and\s+))+\.[\w]+)"###,
                false,
            ),
        ),
    ])
});
// Pinned C# patterns that the corrected reading replaces. Keep them byte for byte:
// the corrected tables find them by pattern text.
const PINNED_MOVE_ALL: &str = r###"(?:move|copy|transfer)\s+(?:all|everything|all\s+(?:the\s+)?(?:files?|content|loose\s+files?))\s+(?:from\s+)?(?:the\s+)?(?<source>[\w\s\-_/\\"'&,]+?)\s+(?:folder|directory)?\s+to\s+(?:your\s+)?(?:game'?s?\s+)?(?<destination>[\w\s\-_/\\]+)"###;
const PINNED_MOVE_ANY: &str = r###"(?:move|copy|place|put|drag)\s+(?:the\s+)?(?<source>[\w\s\-_/\\*.,()]+?)(?:\s+(?:to|into)\s+(?<destination>[\w\s\-_/\\]+?))?"###;
const PINNED_EXTRACT_ANY: &str = r###"(?:extract|unzip|decompress|unpack)\s+(?:the\s+)?(?<source>[\w\s\-_/\\]+?)(?:\s+to\s+(?<destination>[\w\s\-_/\\]+?))?"###;
const PINNED_RECOMMEND: &str = r###"(?:I\s+)?(?:recommend|suggest|advise)\s+(?:the\s+)?(?<option>[^.,;]+?)(?:\s+version|\s+option|\s+install)?(?:[.,;]|$)"###;

// "move all the files in the Creatures folder": the longer "all the files" form is
// tried before plain "all", "in"/"within" are accepted as well as "from", and the
// folder word is part of the source so it is read as a named folder. Copying is a
// Copy, not a Move that would take the original away.
const CORRECTED_MOVE_ALL: &str = r###"(?:move|transfer)\s+(?:all\s+(?:the\s+)?(?:loose\s+files?|files?|content)|everything|all)\s+(?:(?:from|in|within|inside)\s+)?(?:the\s+)?(?<source>[\w\s\-_/\\"'&,]+?\s+(?:folders?|directory))\s+to\s+(?:your\s+)?(?:game'?s?\s+)?(?<destination>[\w\s\-_/\\]+)"###;
const CORRECTED_COPY_ALL: &str = r###"copy\s+(?:all\s+(?:the\s+)?(?:loose\s+files?|files?|content)|everything|all)\s+(?:(?:from|in|within|inside)\s+)?(?:the\s+)?(?<source>[\w\s\-_/\\"'&,]+?\s+(?:folders?|directory))\s+to\s+(?:your\s+)?(?:game'?s?\s+)?(?<destination>[\w\s\-_/\\]+)"###;
// The pinned catch-alls end on a lazy source with nothing required after it, so they
// capture one letter ("f" from "files"). These require the source to run to a
// destination, punctuation, a joining word or the end of the step.
const CORRECTED_MOVE_ANY: &str = r###"\b(?:move|place|put|drag)\s+(?:the\s+)?(?<source>[\w\s\-_/\\*.,()"']+?)(?:\s+(?:directly\s+)?(?:to|into)\s+(?<destination>[\w\s\-_/\\']+?))?(?:\s*(?:[,;:(]|\.(?:\s|$))|\s+(?:and|then|if|unless|before|after|once|but|so|as\s+well)\b|$)"###;
const CORRECTED_COPY_ANY: &str = r###"\bcopy\s+(?:the\s+)?(?<source>[\w\s\-_/\\*.,()"']+?)(?:\s+(?:directly\s+)?(?:to|into)\s+(?<destination>[\w\s\-_/\\']+?))?(?:\s*(?:[,;:(]|\.(?:\s|$))|\s+(?:and|then|if|unless|before|after|once|but|so|as\s+well)\b|$)"###;
const CORRECTED_EXTRACT_ANY: &str = r###"\b(?:extract|unzip|decompress|unpack)\s+(?:the\s+)?(?<source>[\w\s\-_/\\."']+?)(?:\s+(?:to|into)\s+(?<destination>[\w\s\-_/\\]+?))?(?:\s*(?:[,;:(]|\.(?:\s|$))|\s+(?:and|then|if|unless|before|after|once|but|so|as\s+well)\b|$)"###;
// "I recommend the Medium texture option for the best balance ..." names "Medium
// texture", not the whole clause.
const CORRECTED_RECOMMEND: &str = r###"(?:I\s+)?(?:recommend|suggest|advise)\s+(?:the\s+)?(?<option>[^.,;]+?)(?:\s+(?:version|option|variant)\b[^.,;]*)?(?:[.,;]|$)"###;

/// The pinned instruction patterns with the corrected replacements swapped in.
static CORRECTED_INSTRUCTIONS: LazyLock<Vec<(&str, Regex)>> = LazyLock::new(|| {
    let mut result = Vec::with_capacity(INSTRUCTIONS.len() + 2);
    let mut replaced = 0;
    for (action, pattern) in INSTRUCTIONS.iter() {
        match pattern.as_str() {
            PINNED_MOVE_ALL => {
                result.push(("Move", compile(CORRECTED_MOVE_ALL, false)));
                result.push(("Copy", compile(CORRECTED_COPY_ALL, false)));
                replaced += 1;
            }
            PINNED_MOVE_ANY => {
                result.push(("Move", compile(CORRECTED_MOVE_ANY, false)));
                result.push(("Copy", compile(CORRECTED_COPY_ANY, false)));
                replaced += 1;
            }
            PINNED_EXTRACT_ANY => {
                result.push(("Extract", compile(CORRECTED_EXTRACT_ANY, false)));
                replaced += 1;
            }
            _ => result.push((*action, pattern.clone())),
        }
    }
    assert_eq!(
        replaced, 3,
        "corrected draft patterns must replace their pinned originals"
    );
    result
});

static CORRECTED_RECOMMENDATIONS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    let result: Vec<Regex> = RECOMMENDATIONS
        .iter()
        .map(|pattern| match pattern.as_str() {
            PINNED_RECOMMEND => compile(CORRECTED_RECOMMEND, false),
            _ => pattern.clone(),
        })
        .collect();
    assert!(
        result.iter().any(|p| p.as_str() == CORRECTED_RECOMMEND),
        "corrected recommendation pattern must replace its pinned original"
    );
    result
});

static RECOMMENDATIONS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        compile(PINNED_RECOMMEND, false),
        compile(
            r###"(?:I\s+)?(?:strongly\s+)?recommend\s+(?:using\s+)?(?:the\s+)?(?<option>[^.,;]+?)"###,
            false,
        ),
        compile(
            r###"(?:your\s+)?(?:choice|preference)(?:\s+(?:of\s+which|whether))?"###,
            false,
        ),
        compile(
            r###"(?:download|use|choose|select)\s+(?:the\s+)?(?<option>[^.,;]+?)\s+(?:version|option|variant|file|edition)"###,
            false,
        ),
    ]
});
