//! Additive base-build selection rules from the reference wizard.
use crate::Component;

fn base(component: &Component) -> bool {
    !component
        .metadata
        .get("WidescreenOnly")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}
fn priority(tier: &str) -> Option<u8> {
    match tier {
        "1" => Some(1),
        "2" => Some(2),
        "3" => Some(3),
        "4" => Some(4),
        s if s.eq_ignore_ascii_case("Optional") => Some(5),
        _ => None,
    }
}
pub fn select_tier(components: &mut [Component], tier: &str) {
    for component in components.iter_mut().filter(|c| base(c)) {
        let matches = match priority(tier) {
            Some(selected) => priority(&component.tier).is_some_and(|p| p <= selected),
            None => component.tier.eq_ignore_ascii_case(tier),
        };
        if matches {
            component.is_selected = true;
        }
    }
}
pub fn select_categories(components: &mut [Component], categories: &[String]) {
    for component in components.iter_mut().filter(|c| base(c)) {
        if component.category.iter().any(|cat| {
            categories
                .iter()
                .any(|selected| cat.eq_ignore_ascii_case(selected))
        }) {
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
    for c in components.iter_mut().filter(|c| base(c)) {
        c.is_selected = false;
    }
}
