use crate::Component;
use regex::Regex;
use std::collections::BTreeSet;
use std::sync::LazyLock;

/// Mirrors C# `MergeHeuristicsOptions`; defaults match `CreateDefault()`.
#[derive(Clone, Debug)]
pub struct Heuristics {
    /// Minimum total score (`MinNameSimilarity`) a heuristic candidate must reach.
    pub name_similarity: f64,
    pub ignore_punctuation: bool,
    pub use_name_exact: bool,
    pub use_author_exact: bool,
    pub use_name_similarity: bool,
    pub use_author_similarity: bool,
    /// Add 0.6 when the name/author score is below 0.5 and both primary URL hosts match.
    pub match_by_domain: bool,
    pub ignore_case: bool,
    pub trim_whitespace: bool,
    /// Drop non-http(s) resource URLs while merging a matched component's registry.
    pub validate_links: bool,
    pub skip_blank_updates: bool,
}

impl Default for Heuristics {
    fn default() -> Self {
        Self {
            name_similarity: 0.85,
            ignore_punctuation: true,
            use_name_exact: true,
            use_author_exact: true,
            use_name_similarity: true,
            use_author_similarity: false,
            match_by_domain: true,
            ignore_case: true,
            trim_whitespace: true,
            validate_links: true,
            skip_blank_updates: true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub exclude_existing_only: bool,
    pub exclude_incoming_only: bool,
    pub use_existing_order: bool,
    pub prefer_existing_fields: bool,
    pub prefer_existing_name: bool,
    pub prefer_existing_author: bool,
    pub prefer_existing_description: bool,
    pub prefer_existing_directions: bool,
    pub prefer_existing_category: bool,
    pub prefer_existing_tier: bool,
    pub prefer_existing_installation_method: bool,
    pub prefer_existing_instructions: bool,
    pub prefer_existing_options: bool,
    pub prefer_existing_resource_registry: bool,
    pub heuristics: Heuristics,
}

impl Options {
    fn prefer(&self, field: bool) -> bool {
        self.prefer_existing_fields || field
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    Matched,
    TargetOnly,
    SourceOnly,
}

/// Merge lists as the C# ComponentMergeService does for the shared Rust model.
///
/// The ordering list (incoming by default, existing with `use_existing_order`) is the
/// merge target. Each target matches a source row by GUID first, then by the
/// highest heuristic score among unmatched rows. Unmatched source rows are inserted
/// before the next source row already present, preserving their relative order.
pub fn components(
    existing: &[Component],
    incoming: &[Component],
    options: &Options,
) -> Vec<Component> {
    let (mut result, source) = if options.use_existing_order {
        (existing.to_vec(), incoming)
    } else {
        (incoming.to_vec(), existing)
    };
    let mut origins = vec![Origin::TargetOnly; result.len()];
    let mut matched = vec![false; source.len()];
    for (target, origin) in result.iter_mut().zip(origins.iter_mut()) {
        // Like the C# GUID dictionary: the last duplicate wins and prior matches are ignored.
        let found = source
            .iter()
            .rposition(|candidate| same_guid(&target.guid, &candidate.guid))
            .or_else(|| best_match(target, source, &matched, &options.heuristics));
        if let Some(index) = found {
            matched[index] = true;
            *origin = Origin::Matched;
            update(target, &source[index], options);
        }
    }
    for (index, component) in source.iter().enumerate() {
        if matched[index] {
            continue;
        }
        let position = source[index + 1..]
            .iter()
            .find_map(|after| {
                result
                    .iter()
                    .position(|known| same_guid(&known.guid, &after.guid))
            })
            .unwrap_or(result.len());
        result.insert(position, component.clone());
        origins.insert(position, Origin::SourceOnly);
    }
    let (existing_only, incoming_only) = if options.use_existing_order {
        (Origin::TargetOnly, Origin::SourceOnly)
    } else {
        (Origin::SourceOnly, Origin::TargetOnly)
    };
    result
        .into_iter()
        .zip(origins)
        .filter(|(_, origin)| {
            !((options.exclude_existing_only && *origin == existing_only)
                || (options.exclude_incoming_only && *origin == incoming_only))
        })
        .map(|(component, _)| component)
        .collect()
}

fn blank_guid(guid: &str) -> bool {
    guid.chars()
        .all(|c| c == '0' || c == '-' || c == '{' || c == '}' || c.is_whitespace())
}

fn same_guid(a: &str, b: &str) -> bool {
    let trim = |g: &str| g.trim().trim_matches(['{', '}']).to_ascii_lowercase();
    trim(a) == trim(b)
}

static PUNCTUATION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{P}").unwrap());

/// C# `Normalize`: trim, drop Unicode punctuation (not symbols), then lowercase.
fn normalized(value: &str, options: &Heuristics) -> String {
    let mut text = if options.trim_whitespace {
        value.trim()
    } else {
        value
    }
    .to_string();
    if options.ignore_punctuation {
        text = PUNCTUATION.replace_all(&text, "").into_owned();
    }
    if options.ignore_case {
        text = text.to_lowercase();
    }
    text
}

/// C# `JaccardSimilarity`: word sets split on single spaces.
fn similarity(a: &str, b: &str) -> f64 {
    if a.trim().is_empty() || b.trim().is_empty() {
        return 0.0;
    }
    let a: BTreeSet<_> = a.split(' ').filter(|w| !w.is_empty()).collect();
    let b: BTreeSet<_> = b.split(' ').filter(|w| !w.is_empty()).collect();
    let union = a.union(&b).count();
    if union == 0 {
        0.0
    } else {
        a.intersection(&b).count() as f64 / union as f64
    }
}

/// Lowercased host of an absolute `scheme://authority` URL, without port or user info.
fn host(url: &str) -> Option<String> {
    let (scheme, rest) = url.trim().split_once("://")?;
    let mut chars = scheme.chars();
    if !chars.next()?.is_ascii_alphabetic()
        || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return None;
    }
    let authority = rest.split(['/', '?', '#', '\\']).next()?;
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if authority.starts_with('[') {
        authority.split_inclusive(']').next()?
    } else {
        authority.split(':').next()?
    };
    (!host.trim().is_empty()).then(|| host.to_lowercase())
}

fn primary_domain(component: &Component) -> Option<String> {
    host(component.resource_registry.keys().next()?)
}

fn likely_accessible(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://")) && host(url).is_some()
}

/// C# `FindHeuristicMatch`: best strictly-greater score at or above the threshold.
fn best_match(
    target: &Component,
    source: &[Component],
    matched: &[bool],
    options: &Heuristics,
) -> Option<usize> {
    let name = normalized(&target.name, options);
    let author = normalized(&target.author, options);
    let mut domain = None;
    let mut best = None;
    let mut best_score = 0.0;
    for (index, candidate) in source.iter().enumerate() {
        if matched[index] {
            continue;
        }
        let other_name = normalized(&candidate.name, options);
        let other_author = normalized(&candidate.author, options);
        let mut score = 0.0;
        if options.use_name_exact && !name.is_empty() && name == other_name {
            score += 1.0;
        }
        if options.use_author_exact && !author.is_empty() && author == other_author {
            score += 1.0;
        }
        if options.use_name_similarity && !name.is_empty() {
            score += similarity(&name, &other_name);
        }
        if options.use_author_similarity && !author.is_empty() {
            score += similarity(&author, &other_author) * 0.5;
        }
        if score < 0.5 && options.match_by_domain {
            let own = domain.get_or_insert_with(|| primary_domain(target));
            if own.is_some() && *own == primary_domain(candidate) {
                score += 0.6;
            }
        }
        if score > best_score && score >= options.name_similarity {
            best_score = score;
            best = Some(index);
        }
    }
    best
}

fn union(target: &mut Vec<String>, source: &[String]) {
    for value in source {
        if !target.iter().any(|known| known.eq_ignore_ascii_case(value)) {
            target.push(value.clone());
        }
    }
}

fn instruction_key(instruction: &crate::Instruction) -> (String, String) {
    (
        instruction.action.to_lowercase(),
        instruction.destination.to_lowercase(),
    )
}

/// C# key merge: keys come from the target's original instructions only; each
/// original is replaced at most once and unkeyed source instructions append.
fn merge_instructions(target: &mut Vec<crate::Instruction>, source: &[crate::Instruction]) {
    let original: Vec<_> = target.iter().map(instruction_key).collect();
    let mut replaced = vec![false; original.len()];
    for incoming in source {
        let key = instruction_key(incoming);
        let Some(existing) = original.iter().position(|known| *known == key) else {
            target.push(incoming.clone());
            continue;
        };
        if replaced[existing] {
            continue;
        }
        replaced[existing] = true;
        let mut replacement = incoming.clone();
        let prior = &target[existing];
        if incoming.action.eq_ignore_ascii_case(&prior.action)
            && incoming.source == prior.source
            && incoming
                .destination
                .eq_ignore_ascii_case(&prior.destination)
            && incoming.arguments.eq_ignore_ascii_case(&prior.arguments)
        {
            replacement.source.clone_from(&prior.source);
        }
        target[existing] = replacement;
    }
}

fn merge_resource_metadata(target: &mut serde_json::Value, source: &serde_json::Value) {
    let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) else {
        return;
    };
    for (key, source_value) in source {
        match target.get_mut(key) {
            Some(target_value) if key.eq_ignore_ascii_case("Files") => {
                let (Some(target_files), Some(source_files)) =
                    (target_value.as_object_mut(), source_value.as_object())
                else {
                    continue;
                };
                for (filename, selected) in source_files {
                    target_files
                        .entry(filename.clone())
                        .or_insert_with(|| selected.clone());
                }
            }
            Some(_) => {}
            None => {
                target.insert(key.clone(), source_value.clone());
            }
        }
    }
}

fn merge_resources(
    target: &mut serde_json::Map<String, serde_json::Value>,
    source: &serde_json::Map<String, serde_json::Value>,
) {
    for (url, source_metadata) in source {
        if url.trim().is_empty() {
            continue;
        }
        if let Some(target_metadata) = target.get_mut(url) {
            merge_resource_metadata(target_metadata, source_metadata);
        } else {
            target.insert(url.clone(), source_metadata.clone());
        }
    }
}

fn merge_options(target: &mut Vec<Component>, source: &[Component]) {
    for source_option in source {
        if let Some(target_option) = target.iter_mut().find(|option| {
            option
                .name
                .trim()
                .eq_ignore_ascii_case(source_option.name.trim())
        }) {
            if target_option.guid.is_empty() && !source_option.guid.is_empty() {
                target_option.guid.clone_from(&source_option.guid);
            }
            if !source_option.description.trim().is_empty() {
                target_option
                    .description
                    .clone_from(&source_option.description);
            }
            union(&mut target_option.dependencies, &source_option.dependencies);
            union(&mut target_option.restrictions, &source_option.restrictions);
            merge_instructions(&mut target_option.instructions, &source_option.instructions);
            merge_resources(
                &mut target_option.resource_registry,
                &source_option.resource_registry,
            );
            merge_options(&mut target_option.options, &source_option.options);
        } else {
            target.push(source_option.clone());
        }
    }
}

fn metadata_key(component: &Component, name: &str) -> Option<String> {
    component
        .metadata
        .keys()
        .find(|key| key.eq_ignore_ascii_case(name))
        .cloned()
}

fn metadata_strings(component: &Component, name: &str) -> Vec<String> {
    let key = metadata_key(component, name);
    match key.and_then(|key| component.metadata.get(&key)) {
        Some(serde_json::Value::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect(),
        Some(serde_json::Value::String(value)) => vec![value.clone()],
        _ => Vec::new(),
    }
}

/// C# `Language` merge: case-insensitive set of target values, then non-blank source values.
fn merge_language(target: &mut Component, source: &Component) {
    let incoming = metadata_strings(source, "Language");
    if incoming.is_empty() {
        return;
    }
    let mut merged = Vec::new();
    union(&mut merged, &metadata_strings(target, "Language"));
    let incoming: Vec<_> = incoming
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect();
    union(&mut merged, &incoming);
    let key = metadata_key(target, "Language").unwrap_or_else(|| "Language".into());
    target.metadata.insert(
        key,
        serde_json::Value::Array(merged.into_iter().map(Into::into).collect()),
    );
}

fn merge_installation_method(target: &mut Component, source: &Component, prefer: bool) {
    if !prefer {
        return;
    }
    let Some(value) = metadata_key(source, "InstallationMethod")
        .and_then(|key| source.metadata.get(&key))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return;
    };
    let key =
        metadata_key(target, "InstallationMethod").unwrap_or_else(|| "InstallationMethod".into());
    target.metadata.insert(key, value.into());
}

fn take(target: &mut String, source: &str, prefer: bool) {
    if prefer && !source.trim().is_empty() {
        *target = source.to_string();
    }
}

fn update(target: &mut Component, source: &Component, options: &Options) {
    if (blank_guid(&target.guid) && !blank_guid(&source.guid))
        || (!options.use_existing_order
            && !blank_guid(&target.guid)
            && !blank_guid(&source.guid)
            && !same_guid(&target.guid, &source.guid))
    {
        target.guid.clone_from(&source.guid);
    }
    take(
        &mut target.name,
        &source.name,
        options.prefer(options.prefer_existing_name),
    );
    take(
        &mut target.author,
        &source.author,
        options.prefer(options.prefer_existing_author),
    );
    take(
        &mut target.description,
        &source.description,
        options.prefer(options.prefer_existing_description),
    );
    take(
        &mut target.directions,
        &source.directions,
        options.prefer(options.prefer_existing_directions),
    );
    let blank_category = source.category.iter().all(|c| c.trim().is_empty());
    if options.prefer(options.prefer_existing_category)
        && !source.category.is_empty()
        && !(options.heuristics.skip_blank_updates && blank_category)
    {
        target.category.clone_from(&source.category);
    }
    take(
        &mut target.tier,
        &source.tier,
        options.prefer(options.prefer_existing_tier),
    );
    merge_installation_method(
        target,
        source,
        options.prefer(options.prefer_existing_installation_method),
    );
    merge_language(target, source);
    if !options.prefer(options.prefer_existing_resource_registry)
        && !source.resource_registry.is_empty()
    {
        let mut links = source.resource_registry.clone();
        if options.heuristics.validate_links {
            links.retain(|url, _| likely_accessible(url));
            target
                .resource_registry
                .retain(|url, _| likely_accessible(url));
        }
        merge_resources(&mut target.resource_registry, &links);
    }
    union(&mut target.dependencies, &source.dependencies);
    union(&mut target.restrictions, &source.restrictions);
    union(&mut target.install_after, &source.install_after);
    union(&mut target.install_before, &source.install_before);
    if !options.prefer(options.prefer_existing_instructions) {
        merge_instructions(&mut target.instructions, &source.instructions);
    }
    if !options.prefer(options.prefer_existing_options) {
        merge_options(&mut target.options, &source.options);
    }
    for (key, value) in &source.metadata {
        if key.eq_ignore_ascii_case("Language") || key.eq_ignore_ascii_case("InstallationMethod") {
            continue;
        }
        target
            .metadata
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
}
