use crate::{Component, Guide, Instruction};
use regex::{Captures, Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::BTreeMap, sync::LazyLock};

pub mod review;

pub const REVIEW_FLAG: &str = "DRAFT INSTRUCTIONS: parsed from guide prose by the natural-language importer. Review before installing - never auto-trusted.";
const MODS: &str = "<<modDirectory>>";
const GAME: &str = "<<kotorDirectory>>";

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
pub fn generate(guide: &mut Guide) -> Vec<DraftResult> {
    let mut results = Vec::new();
    for component in &mut guide.components {
        if !component.instructions.is_empty()
            || component.directions.trim().is_empty()
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
        let normalized = component
            .directions
            .replace("**", "")
            .replace("__", "")
            .replace('*', "");
        for (text, condition) in units(&normalized) {
            let Some(mut instruction) = parse_unit(&text, &mut result.unparsed_gaps) else {
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
                .extend(download_options(download, &component.guid));
        }
        result.draft_instruction_count = component.instructions.len();
        if result.draft_instruction_count > 0 {
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
            component.metadata.insert(
                "DraftReview".into(),
                json!({"RequiresReview": true, "Result": result}),
            );
        }
        results.push(result);
    }
    results
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

fn parse_unit(unit: &str, gaps: &mut Vec<String>) -> Option<Instruction> {
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
    for (action, pattern) in INSTRUCTIONS.iter() {
        if let Some(m) = pattern.captures(unit)
            && let Some(instruction) = from_match(&m, action, unit)
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

fn from_match(m: &Captures<'_>, action: &str, unit: &str) -> Option<Instruction> {
    let mut instruction = empty_instruction(action);
    if let Some(source) = capture(m, "source") {
        instruction.source = sources(source, unit, action);
        if let Some(other) = capture(m, "source2") {
            instruction.source.extend(sources(other, unit, action));
        }
    } else if let (Some(start), Some(end)) = (capture(m, "start"), capture(m, "end")) {
        instruction.source = file_range(start, end);
    }
    instruction.destination = capture(m, "destination")
        .map(|s| destination(s, unit))
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
        let only = sources(capture(&m, "only").unwrap_or(""), unit, action);
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

fn sources(source: &str, unit: &str, action: &str) -> Vec<String> {
    if let Some(m) = ENTITIES["file_range"].captures(unit) {
        return file_range(capture(&m, "start").unwrap(), capture(&m, "end").unwrap());
    }
    // The reference's numeric-range regex has no "start"/"end" captures. Its existing
    // fallback consequently produces two root placeholders, which remain review-only.
    if numeric_range(unit) {
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

/// Root placeholders are mandatory; final filesystem resolution remains the engine's responsibility.
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
    if !instruction.destination.is_empty() && !safe(&instruction.destination) {
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

fn download_options(text: &str, parent: &str) -> Vec<Component> {
    static VERSION: LazyLock<Regex> = LazyLock::new(|| {
        compile(
            r"(?:download|use|choose|install|get|select)\s+(?:the\s+)?(?<option>[^.,;!?\r\n]+?)\s+(?:version|variant|option|file|edition)",
            false,
        )
    });
    let mut result: Vec<Component> = Vec::new();
    for (index, pattern) in RECOMMENDATIONS
        .iter()
        .chain(std::iter::once(&*VERSION))
        .enumerate()
    {
        for m in pattern.captures_iter(text) {
            let Some(name) = capture(&m, "option") else {
                continue;
            };
            if name.chars().count() < 3
                || [
                    "this", "that", "these", "those", "it", "them", "the", "a", "an", "all",
                    "both", "either", "you", "your", "my", "our",
                ]
                .contains(&name.to_lowercase().as_str())
                || result.iter().any(|c| c.name.eq_ignore_ascii_case(name))
            {
                continue;
            }
            let recommended = index < RECOMMENDATIONS.len();
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
        (
            "Move",
            compile(
                r###"(?:move|copy|transfer)\s+(?:all|everything|all\s+(?:the\s+)?(?:files?|content|loose\s+files?))\s+(?:from\s+)?(?:the\s+)?(?<source>[\w\s\-_/\\"'&,]+?)\s+(?:folder|directory)?\s+to\s+(?:your\s+)?(?:game'?s?\s+)?(?<destination>[\w\s\-_/\\]+)"###,
                false,
            ),
        ),
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
        (
            "Move",
            compile(
                r###"(?:move|copy|place|put|drag)\s+(?:the\s+)?(?<source>[\w\s\-_/\\*.,()]+?)(?:\s+(?:to|into)\s+(?<destination>[\w\s\-_/\\]+?))?"###,
                false,
            ),
        ),
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
        (
            "Extract",
            compile(
                r###"(?:extract|unzip|decompress|unpack)\s+(?:the\s+)?(?<source>[\w\s\-_/\\]+?)(?:\s+to\s+(?<destination>[\w\s\-_/\\]+?))?"###,
                false,
            ),
        ),
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
static RECOMMENDATIONS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        compile(
            r###"(?:I\s+)?(?:recommend|suggest|advise)\s+(?:the\s+)?(?<option>[^.,;]+?)(?:\s+version|\s+option|\s+install)?(?:[.,;]|$)"###,
            false,
        ),
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
