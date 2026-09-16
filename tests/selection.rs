use modsync::{Component, selection};

#[test]
fn tier_and_category_selection_are_additive_and_exclude_widescreen_components() {
    let make = |tier: &str, category: &str| Component {
        tier: tier.into(),
        category: vec![category.into()],
        ..Default::default()
    };
    let mut components = vec![
        make("1", "Fix"),
        make("2", "Texture"),
        make("4", "Fix"),
        make("Optional", "Extra"),
        make("Custom", "Fix"),
        make("1", "Fix"),
    ];
    components[5]
        .metadata
        .insert("WidescreenOnly".into(), true.into());
    selection::select_tier(&mut components, "2");
    assert_eq!(
        components.iter().map(|c| c.is_selected).collect::<Vec<_>>(),
        [true, true, false, false, false, false]
    );
    selection::select_tier(&mut components, "CUSTOM");
    assert!(components[4].is_selected);
    selection::select_categories(&mut components, &["fix".into()]);
    assert!(components[2].is_selected);
    assert!(components[1].is_selected); // additive: unrelated existing selections survive
    assert!(!components[5].is_selected);
    selection::select_tier(&mut components, "optional");
    assert!(components[3].is_selected);
    assert!(!components[5].is_selected);
}

#[test]
fn spoiler_display_prefers_authored_text_and_searches_only_displayed_content() {
    let mut c = Component {
        name: "Sith assassins With Lightsabers".into(),
        author: "Author".into(),
        tier: "1".into(),
        category: vec!["Story".into(), "Characters".into()],
        description: "secret revelation".into(),
        ..Default::default()
    };
    assert_eq!(
        selection::display_name(&c, true),
        "SaWL (Story/Characters, Tier 1, by Author)"
    );
    assert_eq!(
        selection::display_description(&c, true),
        "This Story, Characters modification enhances your gameplay."
    );
    assert!(!selection::matches_search(&c, "revelation", true));
    assert!(selection::matches_search(&c, "revelation", false));
    assert!(selection::matches_search(&c, "sawl", true));
    c.metadata
        .insert("NameSpoilerFree".into(), "Safe title".into());
    c.metadata
        .insert("DescriptionSpoilerFree".into(), "Safe description".into());
    assert_eq!(selection::display_name(&c, true), "Safe title");
    assert!(selection::matches_search(&c, "safe description", true));
    assert!(!selection::matches_search(&c, "sith", true));
    assert_eq!(c.name, "Sith assassins With Lightsabers");
    c.description = format!("{}hidden", "a".repeat(200));
    assert_eq!(
        selection::display_description(&c, false),
        format!("{}...", "a".repeat(197))
    );
    assert!(!selection::matches_search(&c, "hidden", false));
}

#[test]
fn spoiler_acronyms_follow_reference_algorithm_instead_of_comment_examples() {
    for (name, expected) in [
        ("JC's Weapons", "JsW"),
        ("K1 Restoration", "KR"),
        ("the Return of a Jedi", "RaJ"),
        ("fooBar", "fB"),
        ("123", "Mod"),
        ("A B C D E F G H I", "ABCDEFGH"),
    ] {
        let c = Component {
            name: name.into(),
            ..Default::default()
        };
        assert_eq!(selection::display_name(&c, true), expected);
    }
}

#[test]
fn tooltip_keeps_full_normal_description_without_leaking_it_in_spoiler_mode() {
    let c = Component {
        description: format!("{}secret", "x".repeat(220)),
        ..Default::default()
    };
    assert!(!selection::display_description(&c, false).contains("secret"));
    assert!(selection::tooltip_description(&c, false).ends_with("secret"));
    assert!(!selection::tooltip_description(&c, true).contains("secret"));
}
