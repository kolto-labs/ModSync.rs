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
