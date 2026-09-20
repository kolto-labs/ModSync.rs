//! Additive base-build selection rules from the reference wizard.
use crate::Component;
use std::collections::BTreeSet;

fn component_index(components: &[Component], guid: &str) -> Option<usize> {
    components
        .iter()
        .position(|component| component.guid.eq_ignore_ascii_case(guid))
}

fn option_index(components: &[Component], guid: &str) -> Option<(usize, usize)> {
    components
        .iter()
        .enumerate()
        .find_map(|(component, parent)| {
            parent
                .options
                .iter()
                .position(|option| option.guid.eq_ignore_ascii_case(guid))
                .map(|option| (component, option))
        })
}

fn component_checked(
    components: &mut [Component],
    index: usize,
    visited_components: &mut BTreeSet<String>,
    visited_options: &mut BTreeSet<String>,
) {
    let guid = components[index].guid.to_ascii_lowercase();
    if !visited_components.insert(guid.clone()) {
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
                    .any(|restriction| restriction.eq_ignore_ascii_case(&guid))
        })
        .map(|(index, _)| index)
        .collect();
    for conflict in reverse {
        component_unchecked(components, conflict, visited_components);
    }
}

fn component_unchecked(components: &mut [Component], index: usize, visited: &mut BTreeSet<String>) {
    let guid = components[index].guid.to_ascii_lowercase();
    components[index].is_selected = false;
    if !visited.insert(format!("-{guid}")) {
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
                    .any(|dependency| dependency.eq_ignore_ascii_case(&guid))
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
    visited_components: &mut BTreeSet<String>,
    visited_options: &mut BTreeSet<String>,
) {
    let guid = components[parent].options[option].guid.to_ascii_lowercase();
    if !visited_options.insert(guid.clone()) {
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
                    .any(|dependency| dependency.eq_ignore_ascii_case(&guid))
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
                    .any(|restriction| restriction.eq_ignore_ascii_case(&guid))
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
    visited_components: &mut BTreeSet<String>,
    visited_options: &mut BTreeSet<String>,
) {
    let guid = components[parent].options[option].guid.to_ascii_lowercase();
    components[parent].options[option].is_selected = false;
    if !visited_options.insert(format!("-{guid}")) {
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
                            .any(|dependency| dependency.eq_ignore_ascii_case(&guid))
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
                    .any(|dependency| dependency.eq_ignore_ascii_case(&guid))
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
fn priority(tier: &str) -> Option<u8> {
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
                .any(|selected| category.eq_ignore_ascii_case(selected))
        }))
        && (tiers.is_empty()
            || (!c.tier.is_empty()
                && tiers.iter().any(|tier| match priority(tier) {
                    Some(selected) => {
                        priority(&c.tier).is_some_and(|component| component <= selected)
                    }
                    None => c.tier.eq_ignore_ascii_case(tier),
                })))
}

/// Reset top-level selections from CLI selectors. Bare values retain Rust's exact
/// name/GUID support; malformed prefixed selectors are ignored like the C# CLI.
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
                .any(|c| c.guid.eq_ignore_ascii_case(value) || c.name.eq_ignore_ascii_case(value)),
            "Unknown component: {value}"
        );
    }
    let has_filter = !categories.is_empty() || !tiers.is_empty();
    let has_valid_selector = has_filter || !exact.is_empty();
    for c in components.iter_mut() {
        let exact_match = exact
            .iter()
            .any(|value| c.guid.eq_ignore_ascii_case(value) || c.name.eq_ignore_ascii_case(value));
        let filtered = has_filter && matches_category_tier(c, &categories, &tiers);
        c.is_selected = !has_valid_selector || exact_match || filtered;
    }
    Ok(warnings)
}

pub fn select_tier(components: &mut [Component], tier: &str) {
    for component in components.iter_mut().filter(|c| base(c)) {
        if matches_category_tier(component, &[], &[tier.to_owned()]) {
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
    let query = search.to_lowercase();
    display_name(c, spoiler_free)
        .to_lowercase()
        .contains(&query)
        || display_description(c, spoiler_free)
            .to_lowercase()
            .contains(&query)
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
        && (tier.is_empty() || c.tier.eq_ignore_ascii_case(tier))
        && (category.is_empty()
            || c.category
                .iter()
                .any(|cat| cat.eq_ignore_ascii_case(category)))
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
    let mut groups = std::collections::BTreeMap::<String, Vec<usize>>::new();
    for (index, c) in components
        .iter()
        .enumerate()
        .filter(|(_, c)| matches_filters(c, search, tier, category, spoiler_free))
    {
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
