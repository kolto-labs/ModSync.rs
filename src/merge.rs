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
    pub heuristics: Heuristics,
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
            update(
                target,
                source,
                options.prefer_existing_fields,
                preserve_existing_guid,
            );
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

fn update(
    target: &mut Component,
    source: &Component,
    prefer_source: bool,
    preserve_target_guid: bool,
) {
    if target.guid.is_empty()
        || (!preserve_target_guid && !source.guid.is_empty() && target.guid != source.guid)
    {
        target.guid.clone_from(&source.guid);
    }
    if prefer_source {
        if !source.name.trim().is_empty() {
            target.name.clone_from(&source.name);
        }
        if !source.author.trim().is_empty() {
            target.author.clone_from(&source.author);
        }
        if !source.description.trim().is_empty() {
            target.description.clone_from(&source.description);
        }
        if !source.directions.trim().is_empty() {
            target.directions.clone_from(&source.directions);
        }
        if !source.tier.trim().is_empty() {
            target.tier.clone_from(&source.tier);
        }
        if !source.category.is_empty() {
            target.category.clone_from(&source.category);
        }
    }
    union(&mut target.dependencies, &source.dependencies);
    union(&mut target.restrictions, &source.restrictions);
    union(&mut target.install_before, &source.install_before);
    union(&mut target.install_after, &source.install_after);
    if !prefer_source {
        for (url, metadata) in &source.resource_registry {
            target
                .resource_registry
                .entry(url.clone())
                .or_insert_with(|| metadata.clone());
        }
        for instruction in &source.instructions {
            if !target.instructions.iter().any(|known| {
                known.action.eq_ignore_ascii_case(&instruction.action)
                    && known
                        .destination
                        .eq_ignore_ascii_case(&instruction.destination)
            }) {
                target.instructions.push(instruction.clone());
            }
        }
    }
}
