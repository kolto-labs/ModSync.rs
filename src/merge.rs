use crate::Component;
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct Heuristics {
    pub name_similarity: f64,
    pub ignore_punctuation: bool,
}

impl Default for Heuristics {
    fn default() -> Self {
        Self {
            name_similarity: 0.85,
            ignore_punctuation: true,
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

/// Merge lists as the C# ComponentMergeService does for the shared Rust model.
pub fn components(
    existing: &[Component],
    incoming: &[Component],
    options: &Options,
) -> Vec<Component> {
    let (mut result, source, preserve_existing_guid) = if options.use_existing_order {
        (existing.to_vec(), incoming, true)
    } else {
        (incoming.to_vec(), existing, false)
    };
    let mut matched = BTreeSet::new();
    for target in &mut result {
        let found = source.iter().enumerate().find(|(index, candidate)| {
            !matched.contains(index)
                && (target.guid.eq_ignore_ascii_case(&candidate.guid)
                    || heuristic_match(target, candidate, &options.heuristics))
        });
        if let Some((index, source)) = found {
            matched.insert(index);
            update(target, source, options, preserve_existing_guid);
        }
    }
    if !options.exclude_existing_only && !options.exclude_incoming_only {
        for (index, component) in source.iter().enumerate() {
            if !matched.contains(&index) {
                result.push(component.clone());
            }
        }
    }
    if options.exclude_existing_only {
        let ids: BTreeSet<_> = incoming
            .iter()
            .map(|c| c.guid.to_ascii_lowercase())
            .collect();
        result.retain(|c| ids.contains(&c.guid.to_ascii_lowercase()));
    }
    if options.exclude_incoming_only {
        let ids: BTreeSet<_> = existing
            .iter()
            .map(|c| c.guid.to_ascii_lowercase())
            .collect();
        result.retain(|c| ids.contains(&c.guid.to_ascii_lowercase()));
    }
    result
}

fn normalized(value: &str, strip_punctuation: bool) -> String {
    value
        .trim()
        .chars()
        .filter(|c| !strip_punctuation || !c.is_ascii_punctuation())
        .flat_map(char::to_lowercase)
        .collect()
}

fn similarity(a: &str, b: &str) -> f64 {
    let a: BTreeSet<_> = a.split_whitespace().collect();
    let b: BTreeSet<_> = b.split_whitespace().collect();
    let union = a.union(&b).count();
    if union == 0 {
        0.0
    } else {
        a.intersection(&b).count() as f64 / union as f64
    }
}

fn heuristic_match(target: &Component, candidate: &Component, options: &Heuristics) -> bool {
    let name = normalized(&target.name, options.ignore_punctuation);
    let other_name = normalized(&candidate.name, options.ignore_punctuation);
    let author = normalized(&target.author, options.ignore_punctuation);
    let other_author = normalized(&candidate.author, options.ignore_punctuation);
    let mut score = if !name.is_empty() && name == other_name {
        1.0
    } else {
        0.0
    };
    if !author.is_empty() && author == other_author {
        score += 1.0;
    }
    if !name.is_empty() {
        score += similarity(&name, &other_name);
    }
    score >= options.name_similarity
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
        instruction.action.to_ascii_lowercase(),
        instruction.destination.to_ascii_lowercase(),
    )
}

fn merge_instructions(target: &mut Vec<crate::Instruction>, source: &[crate::Instruction]) {
    for incoming in source {
        if let Some(existing) = target
            .iter()
            .position(|known| instruction_key(known) == instruction_key(incoming))
        {
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
        } else {
            target.push(incoming.clone());
        }
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

fn update(
    target: &mut Component,
    source: &Component,
    options: &Options,
    preserve_target_guid: bool,
) {
    if target.guid.is_empty()
        || (!preserve_target_guid && !source.guid.is_empty() && target.guid != source.guid)
    {
        target.guid.clone_from(&source.guid);
    }
    if options.prefer(options.prefer_existing_name) && !source.name.trim().is_empty() {
        target.name.clone_from(&source.name);
    }
    if options.prefer(options.prefer_existing_author) && !source.author.trim().is_empty() {
        target.author.clone_from(&source.author);
    }
    if options.prefer(options.prefer_existing_description) && !source.description.trim().is_empty()
    {
        target.description.clone_from(&source.description);
    }
    if options.prefer(options.prefer_existing_directions) && !source.directions.trim().is_empty() {
        target.directions.clone_from(&source.directions);
    }
    if options.prefer(options.prefer_existing_tier) && !source.tier.trim().is_empty() {
        target.tier.clone_from(&source.tier);
    }
    if options.prefer(options.prefer_existing_category) && !source.category.is_empty() {
        target.category.clone_from(&source.category);
    }
    union(&mut target.dependencies, &source.dependencies);
    union(&mut target.restrictions, &source.restrictions);
    union(&mut target.install_before, &source.install_before);
    union(&mut target.install_after, &source.install_after);
    if !options.prefer(options.prefer_existing_resource_registry) {
        merge_resources(&mut target.resource_registry, &source.resource_registry);
    }
    if !options.prefer(options.prefer_existing_instructions) {
        merge_instructions(&mut target.instructions, &source.instructions);
    }
    if !options.prefer(options.prefer_existing_options) {
        merge_options(&mut target.options, &source.options);
    }
    for (key, value) in &source.metadata {
        target
            .metadata
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
}
