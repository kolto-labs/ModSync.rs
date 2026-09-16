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
