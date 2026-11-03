//! Additive base-build selection rules from the reference wizard.
use crate::{
    Component, game,
    guid::{self, Identity},
};

type VisitKey = (bool, Identity);
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

fn component_index(components: &[Component], guid: &str) -> Option<usize> {
    components
        .iter()
        .position(|component| guid::eq_ascii(&component.guid, guid))
}

fn option_index(components: &[Component], guid: &str) -> Option<(usize, usize)> {
    components
        .iter()
        .enumerate()
        .find_map(|(component, parent)| {
            parent
                .options
                .iter()
                .position(|option| guid::eq_ascii(&option.guid, guid))
                .map(|option| (component, option))
        })
}

fn component_checked(
    components: &mut [Component],
    index: usize,
    visited_components: &mut BTreeSet<VisitKey>,
    visited_options: &mut BTreeSet<VisitKey>,
) {
    let guid = components[index].guid.clone();
    if !visited_components.insert((true, guid::key_ascii(&guid))) {
        return;
    }
    components[index].is_selected = true;
    let dependencies = components[index].dependencies.clone();
    let restrictions = components[index].restrictions.clone();
    for dependency in dependencies {
        if let Some(required) = component_index(components, &dependency) {
            component_checked(components, required, visited_components, visited_options);
        } else if let Some((parent, option)) = option_index(components, &dependency) {
            components[parent].options[option].is_selected = true;
            option_checked(
                components,
                parent,
                option,
                visited_components,
                visited_options,
            );
        }
    }
    for restriction in restrictions {
        if let Some(conflict) = component_index(components, &restriction) {
            component_unchecked(components, conflict, visited_components);
        } else if let Some((parent, option)) = option_index(components, &restriction) {
            option_unchecked(
                components,
                parent,
                option,
                visited_components,
                visited_options,
            );
        }
    }
    let reverse: Vec<_> = components
        .iter()
        .enumerate()
        .filter(|(_, component)| {
            component.is_selected
                && component
                    .restrictions
                    .iter()
                    .any(|restriction| guid::eq_ascii(restriction, &guid))
        })
        .map(|(index, _)| index)
        .collect();
    for conflict in reverse {
        component_unchecked(components, conflict, visited_components);
    }
}

fn component_unchecked(
    components: &mut [Component],
    index: usize,
    visited: &mut BTreeSet<VisitKey>,
) {
    let guid = components[index].guid.clone();
    components[index].is_selected = false;
    if !visited.insert((false, guid::key_ascii(&guid))) {
        return;
    }
    let dependents: Vec<_> = components
        .iter()
        .enumerate()
        .filter(|(_, component)| {
            component.is_selected
                && component
                    .dependencies
                    .iter()
                    .any(|dependency| guid::eq_ascii(dependency, &guid))
        })
        .map(|(index, _)| index)
        .collect();
    for dependent in dependents {
        component_unchecked(components, dependent, visited);
    }
}

fn option_checked(
    components: &mut [Component],
    parent: usize,
    option: usize,
    visited_components: &mut BTreeSet<VisitKey>,
    visited_options: &mut BTreeSet<VisitKey>,
) {
    let guid = components[parent].options[option].guid.clone();
    if !visited_options.insert((true, guid::key_ascii(&guid))) {
        return;
    }
    components[parent].options[option].is_selected = true;
    let dependencies = components[parent].options[option].dependencies.clone();
    let restrictions = components[parent].options[option].restrictions.clone();
    for dependency in dependencies {
        if let Some(required) = component_index(components, &dependency) {
            component_checked(components, required, visited_components, visited_options);
        } else if let Some((required_parent, required)) = option_index(components, &dependency) {
            option_checked(
                components,
                required_parent,
                required,
                visited_components,
                visited_options,
            );
        }
    }
    for restriction in restrictions {
        if let Some(conflict) = component_index(components, &restriction) {
            component_unchecked(components, conflict, visited_components);
        } else if let Some((conflict_parent, conflict)) = option_index(components, &restriction) {
            option_unchecked(
                components,
                conflict_parent,
                conflict,
                visited_components,
                visited_options,
            );
        }
    }
    let dependent_components: Vec<_> = components
        .iter()
        .enumerate()
        .filter(|(_, component)| {
            !component.is_selected
                && component
                    .dependencies
                    .iter()
                    .any(|dependency| guid::eq_ascii(dependency, &guid))
        })
        .map(|(index, _)| index)
        .collect();
    for dependent in dependent_components {
        component_checked(components, dependent, visited_components, visited_options);
    }
    let conflicting_components: Vec<_> = components
        .iter()
        .enumerate()
        .filter(|(_, component)| {
            component.is_selected
                && component
                    .restrictions
                    .iter()
                    .any(|restriction| guid::eq_ascii(restriction, &guid))
        })
        .map(|(index, _)| index)
        .collect();
    for conflict in conflicting_components {
        component_unchecked(components, conflict, visited_components);
    }
}

fn option_unchecked(
    components: &mut [Component],
    parent: usize,
    option: usize,
    visited_components: &mut BTreeSet<VisitKey>,
    visited_options: &mut BTreeSet<VisitKey>,
) {
    let guid = components[parent].options[option].guid.clone();
    components[parent].options[option].is_selected = false;
    if !visited_options.insert((false, guid::key_ascii(&guid))) {
        return;
    }
    let option_dependents: Vec<_> = components
        .iter()
        .enumerate()
        .flat_map(|(component, parent)| {
            parent
                .options
                .iter()
                .enumerate()
                .filter(|(_, option)| {
                    option.is_selected
                        && option
                            .dependencies
                            .iter()
                            .any(|dependency| guid::eq_ascii(dependency, &guid))
                })
                .map(move |(option, _)| (component, option))
        })
        .collect();
    for (dependent_parent, dependent) in option_dependents {
        option_unchecked(
            components,
            dependent_parent,
            dependent,
            visited_components,
            visited_options,
        );
    }
    let component_dependents: Vec<_> = components
        .iter()
        .enumerate()
        .filter(|(_, component)| {
            component.is_selected
                && component
                    .dependencies
                    .iter()
                    .any(|dependency| guid::eq_ascii(dependency, &guid))
        })
        .map(|(index, _)| index)
        .collect();
    for dependent in component_dependents {
        component_unchecked(components, dependent, visited_components);
    }
    if components[parent]
        .options
        .iter()
        .all(|option| !option.is_selected)
    {
        component_unchecked(components, parent, visited_components);
    }
}

pub fn set_component_selected(components: &mut [Component], index: usize, selected: bool) {
    let mut visited_components = BTreeSet::new();
    if selected {
        component_checked(
            components,
            index,
            &mut visited_components,
            &mut BTreeSet::new(),
        );
    } else {
        component_unchecked(components, index, &mut visited_components);
    }
}

pub fn set_option_selected(
    components: &mut [Component],
    parent: usize,
    option: usize,
    selected: bool,
) {
    let mut visited_components = BTreeSet::new();
    let mut visited_options = BTreeSet::new();
    if selected {
        if !components[parent].is_selected {
            component_checked(
                components,
                parent,
                &mut visited_components,
                &mut visited_options,
            );
        }
        option_checked(
            components,
            parent,
            option,
            &mut visited_components,
            &mut visited_options,
        );
    } else {
        option_unchecked(
            components,
            parent,
            option,
            &mut visited_components,
            &mut visited_options,
        );
    }
}

pub fn resolve_selected(components: &mut [Component]) {
    let selected: Vec<_> = components
        .iter()
        .enumerate()
        .filter(|(_, component)| component.is_selected)
        .map(|(index, _)| index)
        .collect();
    for index in selected {
        set_component_selected(components, index, true);
    }
}

fn base(component: &Component) -> bool {
    !component
        .metadata
        .get("WidescreenOnly")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}
fn cli_tier_priority(tier: &str) -> Option<u8> {
    let normalized = tier.trim().to_ascii_lowercase();
    if normalized.contains("essential") {
        Some(1)
    } else if normalized.contains("recommend") {
        Some(2)
    } else if normalized.contains("suggest") {
        Some(3)
    } else if normalized.contains("option") {
        Some(4)
    } else {
        None
    }
}

/// C# CLI semantics: categories OR together, tiers OR together, and the two groups AND.
pub fn matches_category_tier(c: &Component, categories: &[String], tiers: &[String]) -> bool {
    (categories.is_empty()
        || c.category.iter().any(|category| {
            categories
                .iter()
                .any(|selected| crate::fomod::eq_ignore_case(category, selected))
        }))
        && (tiers.is_empty()
            || (!c.tier.is_empty()
                && tiers.iter().any(|tier| match cli_tier_priority(tier) {
                    Some(selected) => {
                        cli_tier_priority(&c.tier).is_some_and(|component| component <= selected)
                    }
                    None => crate::fomod::eq_ignore_case(&c.tier, tier),
                })))
}

/// Reset top-level selections from CLI selectors. An exact component name/GUID
/// takes precedence over `category:`/`tier:` parsing, preserving names containing
/// colons. Malformed prefixed selectors are ignored like the C# CLI.
pub fn apply_cli_selection(
    components: &mut [Component],
    selectors: &[String],
) -> anyhow::Result<Vec<String>> {
    let mut categories = Vec::new();
    let mut tiers = Vec::new();
    let mut exact = Vec::new();
    let mut warnings = Vec::new();

    for selector in selectors {
        let selector = selector.trim();
        if selector.is_empty() {
            warnings.push("Invalid empty selection; expected a component name/GUID, category:Name, or tier:Name".into());
        } else if components.iter().any(|c| {
            guid::eq_ascii(&c.guid, selector) || crate::fomod::eq_ignore_case(&c.name, selector)
        }) {
            exact.push(selector.to_owned());
        } else if let Some((kind, value)) = selector.split_once(':') {
            let value = value.trim();
            if kind.trim().eq_ignore_ascii_case("category") {
                categories.push(value.to_owned());
            } else if kind.trim().eq_ignore_ascii_case("tier") {
                tiers.push(value.to_owned());
            } else {
                warnings.push(format!("Unknown selection type: '{}'", kind.trim()));
            }
        } else {
            exact.push(selector.to_owned());
        }
    }

    for value in &exact {
        anyhow::ensure!(
            components
                .iter()
                .any(|c| guid::eq_ascii(&c.guid, value)
                    || crate::fomod::eq_ignore_case(&c.name, value)),
            "Unknown component: {value}"
        );
    }
    let has_filter = !categories.is_empty() || !tiers.is_empty();
    let has_valid_selector = has_filter || !exact.is_empty();
    for c in components.iter_mut() {
        let exact_match = exact.iter().any(|value| {
            guid::eq_ascii(&c.guid, value) || crate::fomod::eq_ignore_case(&c.name, value)
        });
        let filtered = has_filter && matches_category_tier(c, &categories, &tiers);
        c.is_selected = !has_valid_selector || exact_match || filtered;
    }
    Ok(warnings)
}

pub fn select_tier(components: &mut [Component], tier: &str) {
    let priority = |value: &str| match value.to_ascii_lowercase().as_str() {
        "1" => Some(1),
        "2" => Some(2),
        "3" => Some(3),
        "4" => Some(4),
        "optional" => Some(5),
        _ => None,
    };
    let selected_priority = priority(tier);
    for component in components.iter_mut().filter(|c| base(c)) {
        if selected_priority.map_or_else(
            || crate::fomod::eq_ignore_case(&component.tier, tier),
            |selected| priority(&component.tier).is_some_and(|value| value <= selected),
        ) {
            component.is_selected = true;
        }
    }
}
pub fn select_categories(components: &mut [Component], categories: &[String]) {
    for component in components.iter_mut().filter(|c| base(c)) {
        if matches_category_tier(component, categories, &[]) {
            component.is_selected = true;
        }
    }
}

fn alternate<'a>(c: &'a Component, key: &str) -> Option<&'a str> {
    c.metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.trim().is_empty())
}

pub fn display_name(c: &Component, spoiler_free: bool) -> String {
    if !spoiler_free {
        return c.name.clone();
    }
    if let Some(name) = alternate(c, "NameSpoilerFree") {
        return name.into();
    }
    let skip = [
        "a", "an", "the", "of", "in", "on", "at", "to", "for", "and", "or", "but", "is", "are",
        "was", "were", "be", "been", "being",
    ];
    let mut acronym = Vec::new();
    for word in c
        .name
        .split([' ', '-', '_', ':', '\'', '"'])
        .filter(|w| !w.trim().is_empty())
    {
        let first = word.chars().next().unwrap();
        if word.chars().count() > 1
            && !first.is_uppercase()
            && skip.contains(&word.to_ascii_lowercase().as_str())
        {
            continue;
        }
        if word.chars().any(char::is_uppercase) {
            for ch in word.chars() {
                if ch.is_uppercase() || (acronym.is_empty() && ch.is_alphabetic()) {
                    acronym.push(ch);
                    if ch.is_uppercase() {
                        break;
                    }
                }
            }
        } else if first.is_alphabetic() {
            acronym.push(first);
        }
    }
    let acronym: String = acronym.into_iter().take(8).collect();
    let mut metadata = Vec::new();
    if !c.category.is_empty() {
        metadata.push(c.category.join("/"));
    }
    if !c.tier.trim().is_empty() {
        metadata.push(format!("Tier {}", c.tier));
    }
    if !c.author.trim().is_empty() {
        metadata.push(format!("by {}", c.author));
    }
    match (acronym.is_empty(), metadata.is_empty()) {
        (true, true) => "Mod".into(),
        (false, true) => acronym,
        (true, false) => metadata.join(", "),
        (false, false) => format!("{acronym} ({})", metadata.join(", ")),
    }
}

pub fn display_description(c: &Component, spoiler_free: bool) -> String {
    if spoiler_free {
        if let Some(description) = alternate(c, "DescriptionSpoilerFree") {
            return description.into();
        }
        return if c.category.is_empty() {
            "This modification enhances your gameplay.".into()
        } else {
            format!(
                "This {} modification enhances your gameplay.",
                c.category.join(", ")
            )
        };
    }
    let units: Vec<_> = c.description.encode_utf16().collect();
    if units.len() > 200 {
        format!("{}...", String::from_utf16_lossy(&units[..197]))
    } else {
        c.description.clone()
    }
}

pub fn matches_search(c: &Component, search: &str, spoiler_free: bool) -> bool {
    let query = crate::fomod::fold_case(search);
    crate::fomod::fold_case(&display_name(c, spoiler_free)).contains(&query)
        || crate::fomod::fold_case(&display_description(c, spoiler_free)).contains(&query)
}

/// The reference tooltip retains the full ordinary description.
pub fn tooltip_description(c: &Component, spoiler_free: bool) -> String {
    if spoiler_free {
        display_description(c, true)
    } else {
        c.description.clone()
    }
}

/// Category descriptions from the pinned C# CategoryTierDefinitions.
pub fn category_description(category: &str) -> String {
    match category {
        "" => "No category specified.".into(),
        "Patch" => "Official or community patches that fix bugs and issues in the base game.".into(),
        "Bugfix" => "Mods that fix specific bugs, glitches, or technical issues.".into(),
        "Bug Fix" => "Mods that fix specific bugs, glitches, or technical issues.".into(),
        "Graphics Improvement" => "Mods that enhance visual quality, textures, models, or lighting.".into(),
        "Graphical Improvement" => "Mods that enhance visual quality, textures, models, or lighting.".into(),
        "Graphics Improvement & Bugfix" => "Mods that both fix bugs and improve visual quality.".into(),
        "Bugfix & Graphics Improvement" => "Mods that both fix bugs and improve visual quality.".into(),
        "Bugfix & Graphics Improvement & Immersion" => "Mods that fix bugs, improve graphics, and enhance immersion.".into(),
        "Bugfix, Graphics Improvement & Immersion" => "Mods that fix bugs, improve graphics, and enhance immersion.".into(),
        "Bugfix, Immersion, Mechanics Change & Restored Content" => "Comprehensive mods that fix bugs, enhance immersion, change mechanics, and restore content.".into(),
        "Mechanics Change" => "Mods that alter game mechanics, rules, or gameplay systems.".into(),
        "Mechanics Change & Patch" => "Mods that change game mechanics and include patches.".into(),
        "Mechanics Change & Immersion" => "Mods that change game mechanics while enhancing immersion.".into(),
        "Mechanics Change, Bugfix & Immersion" => "Mods that change mechanics, fix bugs, and enhance immersion.".into(),
        "Gameplay" => "Mods that modify gameplay elements, combat, or game systems.".into(),
        "Appearance Change" => "Mods that change the appearance of characters, items, or environments.".into(),
        "Appearance Change & Graphics Improvement" => "Mods that change appearances and improve graphics quality.".into(),
        "Appearance Change & Bugfix" => "Mods that change appearances and fix related bugs.".into(),
        "Appearance Change, Immersion & Graphics Improvement" => "Mods that change appearances, enhance immersion, and improve graphics.".into(),
        "Appearance Change, Bugfix & Graphics Improvement" => "Mods that change appearances, fix bugs, and improve graphics.".into(),
        "Immersion" => "Mods that enhance role-playing immersion, dialogue, or story elements.".into(),
        "Immersion & Appearance Change" => "Mods that enhance immersion and change appearances.".into(),
        "Immersion & Mechanics Change" => "Mods that enhance immersion and change game mechanics.".into(),
        "Immersion & Graphics Improvement" => "Mods that enhance immersion and improve graphics.".into(),
        "Story" => "Mods that add, modify, or enhance story content and narrative elements.".into(),
        "Restored Content" => "Mods that restore content that was cut or unused in the original game.".into(),
        "Restored Content & Immersion" => "Mods that restore content and enhance immersion.".into(),
        "Added Content" => "Mods that add new content, areas, characters, or features to the game.".into(),
        "Added Content, Appearance Change & Immersion" => "Mods that add content, change appearances, and enhance immersion.".into(),
        "Added Content & Immersion" => "Mods that add new content and enhance immersion.".into(),
        "UI" => "Mods that modify the user interface, menus, or HUD elements.".into(),
        "Audio" => "Mods that change or improve audio, music, or sound effects.".into(),
        "Graphics Improvement & Immersion" => "Mods that improve graphics and enhance immersion.".into(),
        "Graphics Improvement & Appearance Change" => "Mods that improve graphics and change appearances.".into(),
        "Patch & Graphics Improvement" => "Mods that provide patches and improve graphics.".into(),
        "Bugfix & Immersion" => "Mods that fix bugs and enhance immersion.".into(),
        "Bugfix, Graphics Improvement & Appearance Change" => "Mods that fix bugs, improve graphics, and change appearances.".into(),
        "Appearance Change & Immersion" => "Mods that change appearances and enhance immersion.".into(),
        _ => format!("Custom category: {category}"),
    }
}

pub fn tier_description(tier: &str) -> String {
    if tier.is_empty() {
        return "No tier specified.".into();
    }
    let normalized = tier.trim().to_lowercase();
    if normalized.contains("essential") {
        "Critical mods that fix major bugs or provide essential improvements.".into()
    } else if normalized.contains("recommend") {
        "High-quality mods that significantly improve the game experience. Strongly recommended for most players.".into()
    } else if normalized.contains("suggest") {
        "Good quality mods that enhance specific aspects of the game. Recommended for players who want more content.".into()
    } else if normalized.contains("option") {
        "Mods that are nice to have but not necessary. Install based on personal preference.".into()
    } else {
        format!("Custom tier: {tier}")
    }
}

/// Complete selection-card content, shared by components and options.
pub fn tooltip(c: &Component, spoiler_free: bool) -> String {
    let mut parts = vec![format!("📦 {}", display_name(c, spoiler_free))];
    if !c.author.trim().is_empty() {
        parts.push(format!("👤 Author: {}", c.author));
    }
    if !c.category.is_empty() {
        parts.push(format!(
            "🏷️ Categories:\n{}",
            c.category
                .iter()
                .map(|cat| format!("• {cat}: {}", category_description(cat)))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !c.tier.trim().is_empty() {
        parts.push(format!(
            "⭐ Tier: {} - {}",
            c.tier,
            tier_description(&c.tier)
        ));
    }
    let description = tooltip_description(c, spoiler_free);
    if !description.trim().is_empty() {
        parts.push(format!("📝 Description: {description}"));
    }
    let mut install = Vec::new();
    if !c.instructions.is_empty() {
        install.push(format!("{} installation step(s)", c.instructions.len()));
    }
    if !c.options.is_empty() {
        install.push(format!("{} customization option(s)", c.options.len()));
    }
    if let Some(method) = alternate(c, "InstallationMethod") {
        install.push(format!("uses {method}"));
    }
    if !install.is_empty() {
        parts.push(format!("⚙️ Installation: {}", install.join(", ")));
    }
    if let Some(languages) = c
        .metadata
        .get("Language")
        .and_then(serde_json::Value::as_array)
        && !languages.is_empty()
    {
        parts.push(format!(
            "🌐 Language support: {}",
            languages
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !c.resource_registry.is_empty() {
        parts.push(format!(
            "🔗 {} download source(s) available",
            c.resource_registry.len()
        ));
    }
    parts.join("\n\n")
}

/// The same predicate drives visible rows and filtered bulk selection.
pub fn matches_filters(
    c: &Component,
    search: &str,
    tier: &str,
    category: &str,
    spoiler_free: bool,
) -> bool {
    matches_search(c, search, spoiler_free)
        && (tier.is_empty() || crate::fomod::eq_ignore_case(&c.tier, tier))
        && (category.is_empty()
            || c.category
                .iter()
                .any(|cat| crate::fomod::eq_ignore_case(cat, category)))
}

pub fn select_visible(
    components: &mut [Component],
    search: &str,
    tier: &str,
    category: &str,
    spoiler_free: bool,
) {
    for c in components
        .iter_mut()
        .filter(|c| base(c) && matches_filters(c, search, tier, category, spoiler_free))
    {
        c.is_selected = true;
    }
}

pub fn deselect_base(components: &mut [Component]) {
    let selected: Vec<_> = components
        .iter()
        .enumerate()
        .filter(|(_, component)| base(component) && component.is_selected)
        .map(|(index, _)| index)
        .collect();
    for index in selected {
        set_component_selected(components, index, false);
    }
}

/// Base-build counts remain independent of the current filter.
pub fn selection_summary(components: &[Component]) -> String {
    let total = components.iter().filter(|c| base(c)).count();
    let selected = components
        .iter()
        .filter(|c| base(c) && c.is_selected)
        .count();
    let detail = if selected == 0 {
        "Select the mods you want to install".to_owned()
    } else if selected == total {
        "All mods selected for installation".to_owned()
    } else {
        format!("{} mods not selected", total - selected)
    };
    format!("{selected} of {total} mods selected — {detail}")
}

pub fn filter_summary(
    components: &[Component],
    search: &str,
    tier: &str,
    category: &str,
    spoiler_free: bool,
) -> String {
    let total = components.iter().filter(|c| base(c)).count();
    let visible = components
        .iter()
        .filter(|c| base(c) && matches_filters(c, search, tier, category, spoiler_free))
        .count();
    let mut filters = Vec::new();
    if !search.is_empty() {
        filters.push(format!("Search: \"{search}\""));
    }
    if !category.is_empty() {
        filters.push(format!("Category: {category}"));
    }
    if !tier.is_empty() {
        filters.push(format!("Tier: {tier}"));
    }
    if spoiler_free {
        filters.push("Spoiler-Free Mode".into());
    }
    if filters.is_empty() {
        format!("Showing all {total} mods")
    } else {
        format!(
            "Showing {visible} of {total} mods • Filters: {}",
            filters.join(", ")
        )
    }
}

/// Reference badges classify selected base mods by whether they declare dependencies.
/// They do not count dependency targets or infer required tiers.
pub fn selection_badges(components: &[Component]) -> (usize, usize) {
    let mut required = 0;
    let mut optional = 0;
    for c in components.iter().filter(|c| base(c) && c.is_selected) {
        if c.dependencies.is_empty() {
            optional += 1;
        } else {
            required += 1;
        }
    }
    (required, optional)
}

/// Display ordering only; indices retain the original install order in the guide.
pub fn category_groups(
    components: &[Component],
    search: &str,
    tier: &str,
    category: &str,
    spoiler_free: bool,
) -> Vec<(String, Vec<usize>)> {
    grouped(components, |c| {
        matches_filters(c, search, tier, category, spoiler_free)
    })
}

fn grouped(
    components: &[Component],
    keep: impl Fn(&Component) -> bool,
) -> Vec<(String, Vec<usize>)> {
    let mut groups = std::collections::BTreeMap::<String, Vec<usize>>::new();
    for (index, c) in components.iter().enumerate().filter(|(_, c)| keep(c)) {
        groups
            .entry(
                c.category
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Other".into()),
            )
            .or_default()
            .push(index);
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
    for (_, indices) in &mut groups {
        indices.sort_by(|a, b| {
            components[*a]
                .name
                .encode_utf16()
                .cmp(components[*b].name.encode_utf16())
        });
    }
    groups
}

// ---------------------------------------------------------------------------
// Shared selection actions. Both frontends route every selection change
// through `apply`; the functions above stay as wrappers with their old rules.
// ---------------------------------------------------------------------------

/// Filter state of the base Mod Selection page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub search: String,
    pub tier: String,
    pub category: String,
    pub spoiler_free: bool,
}

impl Filter {
    pub fn matches(&self, c: &Component) -> bool {
        matches_filters(
            c,
            &self.search,
            &self.tier,
            &self.category,
            self.spoiler_free,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Single item, full cascade. `guid` may also be a component name
    /// (case-insensitive) when no GUID matches.
    SetComponent { guid: String, selected: bool },
    /// Single item at any depth, full cascade. Checking selects the parent chain first.
    SetOption { guid: String, selected: bool },
    /// Additive, base only, then the dependency closure.
    SelectVisible(Filter),
    /// Base only. No cascade and no closure, so widescreen mods stay as they are.
    DeselectBase,
    /// Group = first category or `Other`, from `base_category_groups`.
    /// Selecting adds the dependency closure; deselecting is a direct mutation only.
    SetGroup {
        category: String,
        filter: Filter,
        selected: bool,
    },
    /// Cumulative over normalized tiers, additive, base only, then the dependency closure.
    SelectThroughTier(String),
    /// Additive, base only, case-insensitive, then the dependency closure.
    /// An empty list is a no-op.
    SelectCategories(Vec<String>),
    /// CLI `--select` reset semantics, then the dependency closure.
    ResetFromSelectors(Vec<String>),
    /// Single widescreen item, full cascade.
    SetWidescreen { guid: String, selected: bool },
    /// Removes selected Aspyr-exclusive mods (and their dependents) for a known
    /// non-Aspyr game. Run once per boundary; `None`/`Unknown` only warns.
    EnforceGame,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// GUIDs newly selected (components and options), in guide order.
    pub selected: Vec<String>,
    pub deselected: Vec<String>,
    /// Nodes whose final value differs from what the action assigned (or that
    /// changed without being assigned). Usually a subset of `selected`; after
    /// `ResetFromSelectors` it also names closure additions that were selected
    /// before the reset.
    pub auto_selected: Vec<String>,
    /// The same rule for deselection.
    pub auto_deselected: Vec<String>,
    pub warnings: Vec<String>,
}

impl Outcome {
    /// True when the selected set changed; adapters invalidate exactly then.
    pub fn changed(&self) -> bool {
        !self.selected.is_empty() || !self.deselected.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub selected: usize,
    pub total: usize,
    /// Selected base mods that declare dependencies (reference REQUIRED badge).
    pub required: usize,
    pub optional: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    Base,
    Widescreen,
    All,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadCandidate {
    /// Index into the top-level components, in guide order.
    pub index: usize,
    pub guid: String,
}

pub fn is_base(component: &Component) -> bool {
    base(component)
}

/// Selected top-level components with at least one resource, in guide order.
pub fn download_candidates(components: &[Component], scope: Scope) -> Vec<DownloadCandidate> {
    components
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            c.is_selected
                && !c.resource_registry.is_empty()
                && match scope {
                    Scope::Base => base(c),
                    Scope::Widescreen => !base(c),
                    Scope::All => true,
                }
        })
        .map(|(index, c)| DownloadCandidate {
            index,
            guid: c.guid.clone(),
        })
        .collect()
}

/// Base-only counts, independent of the current filter.
pub fn counts(components: &[Component]) -> Counts {
    let mut counts = Counts::default();
    for c in components.iter().filter(|c| base(c)) {
        counts.total += 1;
        if c.is_selected {
            counts.selected += 1;
            if !c.dependencies.is_empty() {
                counts.required += 1;
            }
        }
    }
    counts.optional = counts.selected - counts.required;
    counts
}

/// `{s} of {t} mods selected`.
pub fn summary_count(counts: &Counts) -> String {
    format!("{} of {} mods selected", counts.selected, counts.total)
}

/// The header detail line, without any prefix.
pub fn summary_detail(counts: &Counts) -> String {
    if counts.selected == 0 {
        "Select the mods you want to install".into()
    } else if counts.selected == counts.total {
        "All mods selected for installation".into()
    } else {
        format!("{} mods not selected", counts.total - counts.selected)
    }
}

/// GUIDs of every selected component and option, in guide order (pre-order).
pub fn selected_guids(components: &[Component]) -> Vec<String> {
    all_nodes(components)
        .into_iter()
        .map(|path| node(components, &path))
        .filter(|c| c.is_selected)
        .map(|c| c.guid.clone())
        .collect()
}

/// `Name (GUID)` for a component or option at any depth, or the GUID alone.
pub fn describe(components: &[Component], guid: &str) -> String {
    all_nodes(components)
        .into_iter()
        .map(|path| node(components, &path))
        .find(|c| guid::eq_ascii(&c.guid, guid))
        .map_or_else(|| guid.to_owned(), |c| format!("{} ({})", c.name, c.guid))
}

/// Distinct non-widescreen tiers (non-empty) and categories, UTF-16 ordinal order.
pub fn filter_options(components: &[Component]) -> (Vec<String>, Vec<String>) {
    let mut tiers = Vec::<String>::new();
    let mut categories = Vec::<String>::new();
    for c in components.iter().filter(|c| base(c)) {
        if !c.tier.is_empty() && !tiers.contains(&c.tier) {
            tiers.push(c.tier.clone());
        }
        for category in &c.category {
            if !categories.contains(category) {
                categories.push(category.clone());
            }
        }
    }
    tiers.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    categories.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    (tiers, categories)
}

/// `category_groups` over non-widescreen mods only.
pub fn base_category_groups(
    components: &[Component],
    filter: &Filter,
) -> Vec<(String, Vec<usize>)> {
    grouped(components, |c| base(c) && filter.matches(c))
}

/// Cumulative tier priority: named tiers first, then a bare `1`-`4`.
/// A bare `Optional` is already covered by the named rule.
fn tier_priority(tier: &str) -> Option<u8> {
    let tier = tier.trim();
    cli_tier_priority(tier).or(match tier {
        "1" => Some(1),
        "2" => Some(2),
        "3" => Some(3),
        "4" => Some(4),
        _ => None,
    })
}

fn tier_matches(component_tier: &str, selected: &str) -> bool {
    !component_tier.is_empty()
        && match tier_priority(selected) {
            Some(selected) => tier_priority(component_tier).is_some_and(|tier| tier <= selected),
            None => crate::fomod::eq_ignore_case(component_tier, selected),
        }
}

/// Path to a node: `[component, option, nested option, ...]`.
type NodePath = Vec<usize>;

fn node<'a>(components: &'a [Component], path: &[usize]) -> &'a Component {
    let mut current = &components[path[0]];
    for &index in &path[1..] {
        current = &current.options[index];
    }
    current
}

fn node_mut<'a>(components: &'a mut [Component], path: &[usize]) -> &'a mut Component {
    let mut current = &mut components[path[0]];
    for &index in &path[1..] {
        current = &mut current.options[index];
    }
    current
}

/// Every node in guide order (pre-order): components, then their options depth-first.
fn all_nodes(components: &[Component]) -> Vec<NodePath> {
    fn walk(c: &Component, path: &mut NodePath, out: &mut Vec<NodePath>) {
        out.push(path.clone());
        for (index, option) in c.options.iter().enumerate() {
            path.push(index);
            walk(option, path, out);
            path.pop();
        }
    }
    let mut out = Vec::new();
    for (index, c) in components.iter().enumerate() {
        walk(c, &mut vec![index], &mut out);
    }
    out
}

/// First option at any depth with this GUID, in guide order.
fn find_option(components: &[Component], guid: &str) -> Option<NodePath> {
    all_nodes(components)
        .into_iter()
        .find(|path| path.len() > 1 && guid::eq_ascii(&node(components, path).guid, guid))
}

enum Target {
    Component(usize),
    Option(NodePath),
}

/// A top-level component wins over an option, as in the old helpers.
fn resolve(components: &[Component], guid: &str) -> Option<Target> {
    component_index(components, guid)
        .map(Target::Component)
        .or_else(|| find_option(components, guid).map(Target::Option))
}

fn lists(values: &[String], guid: &str) -> bool {
    values.iter().any(|value| guid::eq_ascii(value, guid))
}

/// Selected and every ancestor selected.
fn effective(components: &[Component], path: &[usize]) -> bool {
    (1..=path.len()).all(|depth| node(components, &path[..depth]).is_selected)
}

#[derive(Default)]
struct Visited {
    components: BTreeSet<VisitKey>,
    options: BTreeSet<VisitKey>,
}

// The cascade below mirrors component_checked / component_unchecked /
// option_checked / option_unchecked rule for rule. It widens only the option
// lookup (any depth) and selects an option's unselected ancestors before the
// option, which the old helpers do only in set_option_selected.

fn cascade_component_checked(components: &mut [Component], index: usize, visited: &mut Visited) {
    let guid = components[index].guid.clone();
    if !visited.components.insert((true, guid::key_ascii(&guid))) {
        return;
    }
    components[index].is_selected = true;
    let dependencies = components[index].dependencies.clone();
    let restrictions = components[index].restrictions.clone();
    for dependency in dependencies {
        match resolve(components, &dependency) {
            Some(Target::Component(required)) => {
                cascade_component_checked(components, required, visited)
            }
            Some(Target::Option(path)) => cascade_option_chain_checked(components, &path, visited),
            None => {}
        }
    }
    for restriction in restrictions {
        match resolve(components, &restriction) {
            Some(Target::Component(conflict)) => {
                component_unchecked(components, conflict, &mut visited.components)
            }
            Some(Target::Option(path)) => cascade_option_unchecked(components, &path, visited),
            None => {}
        }
    }
    let reverse: Vec<_> = components
        .iter()
        .enumerate()
        .filter(|(_, c)| c.is_selected && lists(&c.restrictions, &guid))
        .map(|(index, _)| index)
        .collect();
    for conflict in reverse {
        component_unchecked(components, conflict, &mut visited.components);
    }
}

/// Selects the unselected ancestors (outermost first), then the option itself.
fn cascade_option_chain_checked(
    components: &mut [Component],
    path: &[usize],
    visited: &mut Visited,
) {
    for depth in 1..path.len() {
        let ancestor = &path[..depth];
        if !node(components, ancestor).is_selected {
            if depth == 1 {
                cascade_component_checked(components, ancestor[0], visited);
            } else {
                cascade_option_checked(components, ancestor, visited);
            }
        }
    }
    node_mut(components, path).is_selected = true;
    cascade_option_checked(components, path, visited);
}

fn cascade_option_checked(components: &mut [Component], path: &[usize], visited: &mut Visited) {
    let option = node(components, path);
    let guid = option.guid.clone();
    if !visited.options.insert((true, guid::key_ascii(&guid))) {
        return;
    }
    let dependencies = option.dependencies.clone();
    let restrictions = option.restrictions.clone();
    node_mut(components, path).is_selected = true;
    for dependency in dependencies {
        match resolve(components, &dependency) {
            Some(Target::Component(required)) => {
                cascade_component_checked(components, required, visited)
            }
            Some(Target::Option(required)) => {
                cascade_option_chain_checked(components, &required, visited)
            }
            None => {}
        }
    }
    for restriction in restrictions {
        match resolve(components, &restriction) {
            Some(Target::Component(conflict)) => {
                component_unchecked(components, conflict, &mut visited.components)
            }
            Some(Target::Option(conflict)) => {
                cascade_option_unchecked(components, &conflict, visited)
            }
            None => {}
        }
    }
    let dependents: Vec<_> = components
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.is_selected && lists(&c.dependencies, &guid))
        .map(|(index, _)| index)
        .collect();
    for dependent in dependents {
        cascade_component_checked(components, dependent, visited);
    }
    let conflicts: Vec<_> = components
        .iter()
        .enumerate()
        .filter(|(_, c)| c.is_selected && lists(&c.restrictions, &guid))
        .map(|(index, _)| index)
        .collect();
    for conflict in conflicts {
        component_unchecked(components, conflict, &mut visited.components);
    }
}

fn cascade_option_unchecked(components: &mut [Component], path: &[usize], visited: &mut Visited) {
    let guid = node(components, path).guid.clone();
    node_mut(components, path).is_selected = false;
    if !visited.options.insert((false, guid::key_ascii(&guid))) {
        return;
    }
    let option_dependents: Vec<_> = all_nodes(components)
        .into_iter()
        .filter(|candidate| {
            candidate.len() > 1 && {
                let option = node(components, candidate);
                option.is_selected && lists(&option.dependencies, &guid)
            }
        })
        .collect();
    for dependent in option_dependents {
        cascade_option_unchecked(components, &dependent, visited);
    }
    let component_dependents: Vec<_> = components
        .iter()
        .enumerate()
        .filter(|(_, c)| c.is_selected && lists(&c.dependencies, &guid))
        .map(|(index, _)| index)
        .collect();
    for dependent in component_dependents {
        component_unchecked(components, dependent, &mut visited.components);
    }
    let parent = &path[..path.len() - 1];
    if node(components, parent)
        .options
        .iter()
        .all(|option| !option.is_selected)
    {
        if parent.len() == 1 {
            component_unchecked(components, parent[0], &mut visited.components);
        } else {
            cascade_option_unchecked(components, parent, visited);
        }
    }
}

/// Bulk resolve step: add the dependency closure of the effective selection
/// (options bring their ancestor chain), never deselect anything, and report
/// each restriction conflict left in the set once, naming both GUIDs.
fn close_dependencies(components: &mut [Component], warnings: &mut Vec<String>) {
    loop {
        let mut additions = Vec::new();
        for path in all_nodes(components) {
            if !effective(components, &path) {
                continue;
            }
            for dependency in &node(components, &path).dependencies {
                match resolve(components, dependency) {
                    Some(Target::Component(index)) => additions.push(vec![index]),
                    Some(Target::Option(option)) => additions.push(option),
                    None => {}
                }
            }
        }
        let mut changed = false;
        for path in additions {
            for depth in 1..=path.len() {
                let target = node_mut(components, &path[..depth]);
                if !target.is_selected {
                    target.is_selected = true;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut seen = BTreeSet::new();
    for path in all_nodes(components) {
        if !effective(components, &path) {
            continue;
        }
        let owner = node(components, &path);
        for restriction in &owner.restrictions {
            let conflict = match resolve(components, restriction) {
                Some(Target::Component(index)) => vec![index],
                Some(Target::Option(option)) => option,
                None => continue,
            };
            if !effective(components, &conflict) {
                continue;
            }
            let other = node(components, &conflict);
            let (a, b) = (guid::key_ascii(&owner.guid), guid::key_ascii(&other.guid));
            let key = if a <= b { (a, b) } else { (b, a) };
            if seen.insert(key) {
                warnings.push(format!(
                    "Restriction conflict: '{}' ({}) conflicts with '{}' ({})",
                    owner.name, owner.guid, other.name, other.guid
                ));
            }
        }
    }
}

/// CLI `--select` reset with the cumulative tier rule on both sides.
/// Errors are raised before any mutation.
fn reset_from_selectors(
    components: &mut [Component],
    game: Option<game::Detected>,
    selectors: &[String],
    direct: &mut BTreeMap<Identity, bool>,
) -> anyhow::Result<Vec<String>> {
    let mut categories = Vec::new();
    let mut tiers = Vec::new();
    let mut exact = Vec::new();
    let mut warnings = Vec::new();
    let named = |c: &Component, value: &str| {
        guid::eq_ascii(&c.guid, value) || crate::fomod::eq_ignore_case(&c.name, value)
    };

    for selector in selectors {
        let selector = selector.trim();
        if selector.is_empty() {
            warnings.push("Invalid empty selection; expected a component name/GUID, category:Name, or tier:Name".into());
        } else if components.iter().any(|c| named(c, selector)) {
            exact.push(selector.to_owned());
        } else if let Some((kind, value)) = selector.split_once(':') {
            let value = value.trim();
            if kind.trim().eq_ignore_ascii_case("category") {
                categories.push(value.to_owned());
            } else if kind.trim().eq_ignore_ascii_case("tier") {
                tiers.push(value.to_owned());
            } else {
                warnings.push(format!("Unknown selection type: '{}'", kind.trim()));
            }
        } else {
            exact.push(selector.to_owned());
        }
    }
    for value in &exact {
        anyhow::ensure!(
            components.iter().any(|c| named(c, value)),
            "Unknown component: {value}"
        );
    }
    let has_filter = !categories.is_empty() || !tiers.is_empty();
    let has_valid_selector = has_filter || !exact.is_empty();
    for c in components.iter_mut() {
        let exact_match = exact.iter().any(|value| named(c, value));
        let filtered = has_filter
            && (categories.is_empty() || c.category.iter().any(|cat| lists(&categories, cat)))
            && (tiers.is_empty() || tiers.iter().any(|tier| tier_matches(&c.tier, tier)));
        let wanted = !has_valid_selector || exact_match || filtered;
        if wanted && refused(game, c) {
            // Refused before the closure runs; a previously selected copy is
            // reported as auto-deselected.
            warnings.push(refusal(c));
            c.is_selected = false;
            continue;
        }
        c.is_selected = wanted;
        direct.insert(guid::key_ascii(&c.guid), wanted);
    }
    Ok(warnings)
}

fn snapshot(components: &[Component], nodes: &[NodePath]) -> Vec<bool> {
    nodes
        .iter()
        .map(|path| node(components, path).is_selected)
        .collect()
}

fn push_unique(list: &mut Vec<String>, guid: &str) {
    if !list.iter().any(|seen| guid::eq_ascii(seen, guid)) {
        list.push(guid.to_owned());
    }
}

/// Read-only archive and required-dependency eligibility, separate from saved choices.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    pub unavailable: Vec<Unavailable>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Unavailable {
    pub guid: String,
    pub reason: String,
}

impl Availability {
    pub fn reason(&self, guid: &str) -> Option<&str> {
        self.unavailable
            .iter()
            .find(|entry| crate::guid::eq_ascii(&entry.guid, guid))
            .map(|entry| entry.reason.as_str())
    }

    /// Propagate required edges to a fixed point, including option ancestors.
    /// Restrictions and ordering edges are not required dependencies.
    pub fn from_archive_failures(components: &[Component], failures: Vec<Unavailable>) -> Self {
        let nodes = all_nodes(components);
        let mut blocked: BTreeMap<Identity, String> = failures
            .into_iter()
            .map(|entry| (guid::key_ascii(&entry.guid), entry.reason))
            .collect();
        loop {
            let mut additions = Vec::new();
            for path in &nodes {
                let component = node(components, path);
                let key = guid::key_ascii(&component.guid);
                if blocked.contains_key(&key) {
                    continue;
                }
                let parent =
                    (path.len() > 1).then(|| &node(components, &path[..path.len() - 1]).guid);
                for dependency in component.dependencies.iter().chain(parent) {
                    let required = nodes
                        .iter()
                        .map(|path| node(components, path))
                        .find(|required| guid::eq_ascii(&required.guid, dependency));
                    let reason = match required {
                        None => Some(format!(
                            "Required dependency is absent from the guide: {dependency}"
                        )),
                        Some(required) => blocked.get(&guid::key_ascii(dependency)).map(|reason| {
                            format!("Requires {} ({}): {reason}", required.name, required.guid)
                        }),
                    };
                    if let Some(reason) = reason {
                        additions.push((key, reason));
                        break;
                    }
                }
            }
            if additions.is_empty() {
                break;
            }
            blocked.extend(additions);
        }
        Self {
            unavailable: nodes
                .iter()
                .map(|path| node(components, path))
                .filter_map(|component| {
                    blocked
                        .get(&guid::key_ascii(&component.guid))
                        .map(|reason| Unavailable {
                            guid: component.guid.clone(),
                            reason: reason.clone(),
                        })
                })
                .collect(),
        }
    }
}

/// Use the same graph as ordinary selection, then refuse unavailable additions.
/// Existing selections are retained: an unavailable saved choice remains visible
/// and validation still refuses it; merely inspecting archives never rewrites it.
pub fn apply_available(
    components: &mut [Component],
    game: Option<game::Detected>,
    action: Action,
    availability: &Availability,
) -> anyhow::Result<Outcome> {
    // Resolve the public action's supported name alias before refusal, so a
    // rejected alias cannot run its restriction cascade against saved choices.
    let action = match action {
        Action::SetComponent {
            guid,
            selected: true,
        } => {
            let index = component_index(components, &guid).or_else(|| {
                components
                    .iter()
                    .position(|component| crate::fomod::eq_ignore_case(&component.name, &guid))
            });
            Action::SetComponent {
                guid: index.map_or(guid, |index| components[index].guid.clone()),
                selected: true,
            }
        }
        other => other,
    };
    let nodes = all_nodes(components);
    let before = snapshot(components, &nodes);
    let mut outcome = apply_internal(components, game, action, Some(availability))?;
    for (path, was_selected) in nodes.iter().zip(&before) {
        let component = node_mut(components, path);
        if component.is_selected
            && !was_selected
            && let Some(reason) = availability.reason(&component.guid)
        {
            component.is_selected = false;
            outcome.warnings.push(format!(
                "Cannot select {} ({}): {reason}",
                component.name, component.guid
            ));
        }
    }
    outcome.selected.retain(|guid| {
        nodes.iter().any(|path| {
            let component = node(components, path);
            guid::eq_ascii(&component.guid, guid) && component.is_selected
        })
    });
    outcome.auto_selected.retain(|guid| {
        outcome
            .selected
            .iter()
            .any(|selected| guid::eq_ascii(selected, guid))
    });
    Ok(outcome)
}

/// Apply one selection action: direct mutation, the shared cascade or
/// dependency closure, game enforcement, then a before/after diff.
///
/// `game` of `None` or `Unknown` never enforces; only `Action::EnforceGame`
/// warns about it, so each boundary warns once.
pub fn apply(
    components: &mut [Component],
    game: Option<game::Detected>,
    action: Action,
) -> anyhow::Result<Outcome> {
    apply_internal(components, game, action, None)
}

fn apply_internal(
    components: &mut [Component],
    game: Option<game::Detected>,
    action: Action,
    availability: Option<&Availability>,
) -> anyhow::Result<Outcome> {
    if let Action::SetComponent {
        guid,
        selected: true,
    }
    | Action::SetOption {
        guid,
        selected: true,
    }
    | Action::SetWidescreen {
        guid,
        selected: true,
    } = &action
        && let Some(reason) = availability.and_then(|available| available.reason(guid))
    {
        return Ok(Outcome {
            warnings: vec![format!(
                "Cannot select {}: {reason}",
                describe(components, guid)
            )],
            ..Outcome::default()
        });
    }
    let nodes = all_nodes(components);
    let before = snapshot(components, &nodes);
    let before_top: Vec<bool> = components.iter().map(|c| c.is_selected).collect();
    let mut outcome = Outcome::default();
    // GUID value (or legacy ASCII identity) -> the action's assigned value.
    let mut direct = BTreeMap::<Identity, bool>::new();
    let mut close = false;
    let enforcing = action == Action::EnforceGame;

    match action {
        Action::SetComponent { guid, selected } => {
            let index = component_index(components, &guid)
                .or_else(|| {
                    components
                        .iter()
                        .position(|c| crate::fomod::eq_ignore_case(&c.name, &guid))
                })
                .ok_or_else(|| anyhow::anyhow!("Unknown component: {guid}"))?;
            set_single(
                components,
                game,
                index,
                selected,
                &mut direct,
                &mut outcome.warnings,
            );
        }
        Action::SetWidescreen { guid, selected } => {
            let index = component_index(components, &guid)
                .ok_or_else(|| anyhow::anyhow!("Unknown component: {guid}"))?;
            anyhow::ensure!(
                !base(&components[index]),
                "Not a widescreen component: {guid}"
            );
            set_single(
                components,
                game,
                index,
                selected,
                &mut direct,
                &mut outcome.warnings,
            );
        }
        Action::SetOption { guid, selected } => {
            let path = find_option(components, &guid)
                .ok_or_else(|| anyhow::anyhow!("Unknown option: {guid}"))?;
            if selected && refused(game, &components[path[0]]) {
                // Refused without mutation; enforcement and the diff still run.
                outcome.warnings.push(refusal(&components[path[0]]));
            } else {
                direct.insert(guid::key_ascii(&guid), selected);
                let mut visited = Visited::default();
                if selected {
                    cascade_option_chain_checked(components, &path, &mut visited);
                } else {
                    cascade_option_unchecked(components, &path, &mut visited);
                }
            }
        }
        Action::SelectVisible(filter) => {
            for c in components
                .iter_mut()
                .filter(|c| base(c) && filter.matches(c))
            {
                select_direct(c, game, availability, &mut direct, &mut outcome.warnings);
            }
            close = true;
        }
        Action::DeselectBase => {
            for c in components.iter_mut().filter(|c| base(c)) {
                c.is_selected = false;
                direct.insert(guid::key_ascii(&c.guid), false);
            }
        }
        Action::SetGroup {
            category,
            filter,
            selected,
        } => {
            let groups = base_category_groups(components, &filter);
            match groups.into_iter().find(|(name, _)| *name == category) {
                Some((_, members)) => {
                    for index in members {
                        if selected {
                            select_direct(
                                &mut components[index],
                                game,
                                availability,
                                &mut direct,
                                &mut outcome.warnings,
                            );
                        } else {
                            components[index].is_selected = false;
                            direct.insert(guid::key_ascii(&components[index].guid), false);
                        }
                    }
                    close = selected;
                }
                None => outcome
                    .warnings
                    .push(format!("No visible mods in category: {category}")),
            }
        }
        Action::SelectThroughTier(tier) => {
            for c in components
                .iter_mut()
                .filter(|c| base(c) && tier_matches(&c.tier, &tier))
            {
                select_direct(c, game, availability, &mut direct, &mut outcome.warnings);
            }
            close = true;
        }
        Action::SelectCategories(categories) => {
            if !categories.is_empty() {
                for c in components
                    .iter_mut()
                    .filter(|c| base(c) && matches_category_tier(c, &categories, &[]))
                {
                    select_direct(c, game, availability, &mut direct, &mut outcome.warnings);
                }
                close = true;
            }
        }
        Action::ResetFromSelectors(selectors) => {
            outcome.warnings = reset_from_selectors(components, game, &selectors, &mut direct)?;
            close = true;
        }
        Action::EnforceGame => {
            if matches!(game, None | Some(game::Detected::Unknown)) {
                let kept = components
                    .iter()
                    .filter(|c| c.is_selected && game::aspyr_exclusive(c))
                    .count();
                if kept > 0 {
                    outcome.warnings.push(format!(
                        "Game version unknown; left {kept} Aspyr-exclusive mod(s) selected"
                    ));
                }
            }
        }
    }

    if close {
        close_dependencies(components, &mut outcome.warnings);
    }

    if let Some(detected) = game.filter(|detected| *detected != game::Detected::Unknown) {
        for (index, c) in components.iter().enumerate() {
            // Outside EnforceGame, warn only for an Aspyr-only mod this action
            // brought in: unselected before, or assigned false by the action and
            // re-selected by its cascade. Pre-existing selections get no warning;
            // their removal is listed in this action's own `auto_deselected`.
            let brought_in =
                !before_top[index] || direct.get(&guid::key_ascii(&c.guid)) == Some(&false);
            if c.is_selected && !game::selectable(c, detected) && !enforcing && brought_in {
                outcome.warnings.push(refusal(c));
            }
        }
        enforce_game(components, detected);
    }

    let after = snapshot(components, &nodes);
    for (position, path) in nodes.iter().enumerate() {
        let (was, now) = (before[position], after[position]);
        let guid = &node(components, path).guid;
        if now && !was {
            push_unique(&mut outcome.selected, guid);
        } else if was && !now {
            push_unique(&mut outcome.deselected, guid);
        }
        // Auto lists compare the final value with what the action itself
        // assigned. After ResetFromSelectors this names every closure addition,
        // including one that was already selected before the reset.
        let automatic = match direct.get(&guid::key_ascii(guid)) {
            Some(assigned) => *assigned != now,
            None => was != now,
        };
        if automatic {
            if now {
                push_unique(&mut outcome.auto_selected, guid);
            } else {
                push_unique(&mut outcome.auto_deselected, guid);
            }
        }
    }
    // A refused mod can be reported by the direct step and again after the closure.
    let mut seen = BTreeSet::new();
    outcome
        .warnings
        .retain(|warning| seen.insert(warning.clone()));
    Ok(outcome)
}

/// True when a known game refuses this component (C# editor: refuse before any cascade).
fn refused(game: Option<game::Detected>, c: &Component) -> bool {
    matches!(game, Some(detected) if detected != game::Detected::Unknown && !game::selectable(c, detected))
}

fn refusal(c: &Component) -> String {
    format!(
        "'{}' ({}) is Aspyr-exclusive and cannot be selected for this game",
        c.name, c.guid
    )
}

/// Bulk direct select of one component, refusing it without mutation when the game forbids it.
fn select_direct(
    c: &mut Component,
    game: Option<game::Detected>,
    availability: Option<&Availability>,
    direct: &mut BTreeMap<Identity, bool>,
    warnings: &mut Vec<String>,
) {
    if let Some(reason) = availability.and_then(|available| available.reason(&c.guid)) {
        warnings.push(format!("Cannot select {} ({}): {reason}", c.name, c.guid));
        return;
    }
    if refused(game, c) {
        warnings.push(refusal(c));
        return;
    }
    c.is_selected = true;
    direct.insert(guid::key_ascii(&c.guid), true);
}

/// `game::enforce_selectable`, then clear the options of every component it
/// removed and run the option-uncheck rule on them, so anything depending on
/// those options is deselected too. Repeats until no removed component keeps a
/// selected option.
fn enforce_game(components: &mut [Component], detected: game::Detected) {
    let before: Vec<bool> = components.iter().map(|c| c.is_selected).collect();
    game::enforce_selectable(components, detected);
    loop {
        let stale: Vec<NodePath> = all_nodes(components)
            .into_iter()
            .filter(|path| {
                path.len() > 1
                    && before[path[0]]
                    && !components[path[0]].is_selected
                    && node(components, path).is_selected
            })
            .collect();
        if stale.is_empty() {
            break;
        }
        let mut visited = Visited::default();
        for path in stale {
            if node(components, &path).is_selected {
                cascade_option_unchecked(components, &path, &mut visited);
            }
        }
    }
}

fn set_single(
    components: &mut [Component],
    game: Option<game::Detected>,
    index: usize,
    selected: bool,
    direct: &mut BTreeMap<Identity, bool>,
    warnings: &mut Vec<String>,
) {
    if selected && refused(game, &components[index]) {
        warnings.push(refusal(&components[index]));
        return;
    }
    direct.insert(guid::key_ascii(&components[index].guid), selected);
    let mut visited = Visited::default();
    if selected {
        cascade_component_checked(components, index, &mut visited);
    } else {
        component_unchecked(components, index, &mut visited.components);
    }
}
