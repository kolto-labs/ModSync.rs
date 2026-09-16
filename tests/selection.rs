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

#[test]
fn complete_tooltip_preserves_reference_sections_and_spoiler_boundary() {
    let c: Component = serde_json::from_value(serde_json::json!({
        "Name": "Secret title", "NameSpoilerFree": "Safe title", "Author": "A",
        "Description": "Secret description", "DescriptionSpoilerFree": "Safe description",
        "Category": ["UI", "custom"], "Tier": "2 recommended",
        "Instructions": [{"Action": "Copy", "Source": ["a"], "Destination": "b"}],
        "Options": [{"Name": "Choice"}], "InstallationMethod": "Loose files",
        "Language": ["English", "French"], "ResourceRegistry": {"https://example.invalid": {}}
    }))
    .unwrap();
    assert_eq!(
        selection::tooltip(&c, true),
        concat!(
            "📦 Safe title\n\n👤 Author: A\n\n🏷️ Categories:\n",
            "• UI: Mods that modify the user interface, menus, or HUD elements.\n",
            "• custom: Custom category: custom\n\n",
            "⭐ Tier: 2 recommended - High-quality mods that significantly improve the game experience. Strongly recommended for most players.\n\n",
            "📝 Description: Safe description\n\n",
            "⚙️ Installation: 1 installation step(s), 1 customization option(s), uses Loose files\n\n",
            "🌐 Language support: English, French\n\n🔗 1 download source(s) available"
        )
    );
    assert!(selection::tooltip(&c, false).contains("Secret description"));
    assert!(!selection::tooltip(&c, true).contains("Secret"));
    assert_eq!(selection::tooltip(&Component::default(), false), "📦 ");
    assert_eq!(selection::category_description("ui"), "Custom category: ui");
    assert_eq!(selection::tier_description("2"), "Custom tier: 2");
    assert_eq!(selection::tier_description(""), "No tier specified.");
    assert!(selection::tier_description(" 3 - SUGGESTED ").starts_with("Good quality"));
    assert!(selection::tier_description("Optional").starts_with("Mods that are nice"));
    assert!(selection::tier_description("Essential 1").starts_with("Critical mods"));
}

#[test]
fn visible_selection_combines_filters_and_deselect_preserves_widescreen() {
    let mut components: Vec<Component> = serde_json::from_value(serde_json::json!([
        {"Name":"Hidden name", "NameSpoilerFree":"Safe", "Tier":"2", "Category":["Fix"]},
        {"Name":"Safe", "Tier":"1", "Category":["Fix"]},
        {"Name":"Safe", "Tier":"2", "Category":["Other"], "IsSelected":true},
        {"Name":"Safe", "Tier":"2", "Category":["Fix"], "WidescreenOnly":true},
        {"Name":"Safe", "Tier":"2", "Category":["Fix"], "WidescreenOnly":true,"IsSelected":true}
    ]))
    .unwrap();
    selection::select_visible(&mut components, "Safe", "2", "fix", true);
    assert_eq!(
        components.iter().map(|c| c.is_selected).collect::<Vec<_>>(),
        [true, false, true, false, true]
    );
    assert!(!selection::matches_filters(
        &components[0],
        "Hidden",
        "",
        "",
        true
    ));
    assert!(selection::matches_filters(
        &components[0],
        "Hidden",
        "",
        "",
        false
    ));
    selection::deselect_base(&mut components);
    assert_eq!(
        components.iter().map(|c| c.is_selected).collect::<Vec<_>>(),
        [false, false, false, false, true]
    );
    selection::select_visible(&mut components, "", "", "", false);
    assert_eq!(
        components.iter().map(|c| c.is_selected).collect::<Vec<_>>(),
        [true, true, true, false, true]
    );
}

#[test]
fn selection_and_filter_summaries_keep_hidden_selections_and_exclude_widescreen() {
    let mut components: Vec<Component> = serde_json::from_value(serde_json::json!([
        {"Name":"One", "Tier":"1", "Category":["Fix"], "IsSelected":true},
        {"Name":"Two", "Tier":"2", "Category":["Texture"]},
        {"Name":"Wide", "WidescreenOnly":true, "IsSelected":true}
    ]))
    .unwrap();
    assert_eq!(
        selection::selection_summary(&components),
        "1 of 2 mods selected — 1 mods not selected"
    );
    assert_eq!(
        selection::filter_summary(&components, "Two", "2", "Texture", false),
        "Showing 1 of 2 mods • Filters: Search: \"Two\", Category: Texture, Tier: 2"
    );
    assert_eq!(
        selection::filter_summary(&components, "", "", "", true),
        "Showing 2 of 2 mods • Filters: Spoiler-Free Mode"
    );
    assert_eq!(
        selection::filter_summary(&components, "", "", "", false),
        "Showing all 2 mods"
    );
    selection::deselect_base(&mut components);
    assert_eq!(
        selection::selection_summary(&components),
        "0 of 2 mods selected — Select the mods you want to install"
    );
    selection::select_visible(&mut components, "", "", "", false);
    assert_eq!(
        selection::selection_summary(&components),
        "2 of 2 mods selected — All mods selected for installation"
    );
    assert_eq!(
        selection::selection_summary(&[]),
        "0 of 0 mods selected — Select the mods you want to install"
    );
}
