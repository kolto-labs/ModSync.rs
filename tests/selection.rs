use modsync::{Component, selection};

#[test]
fn interactive_selection_resolves_component_and_option_relationships() {
    let mut components: Vec<Component> = serde_json::from_value(serde_json::json!([
        {"Guid":"base","Name":"Base"},
        {"Guid":"middle","Name":"Middle","Dependencies":["base"]},
        {"Guid":"tail","Name":"Tail","Dependencies":["middle"],"Restrictions":["conflict"]},
        {"Guid":"conflict","Name":"Conflict","IsSelected":true},
        {"Guid":"choices","Name":"Choices","Options":[
            {"Guid":"feature","Name":"Feature","Dependencies":["base"]},
            {"Guid":"addon","Name":"Addon","Dependencies":["feature"]}
        ]},
        {"Guid":"needs-feature","Name":"Needs feature","Dependencies":["feature"]},
        {"Guid":"blocks-feature","Name":"Blocks feature","Restrictions":["feature"],"IsSelected":true}
    ])).unwrap();

    selection::set_component_selected(&mut components, 2, true);
    assert_eq!(
        components
            .iter()
            .take(4)
            .map(|c| c.is_selected)
            .collect::<Vec<_>>(),
        [true, true, true, false]
    );
    selection::set_component_selected(&mut components, 0, false);
    assert_eq!(
        components
            .iter()
            .take(3)
            .map(|c| c.is_selected)
            .collect::<Vec<_>>(),
        [false, false, false]
    );

    selection::set_option_selected(&mut components, 4, 1, true);
    assert!(components[0].is_selected);
    assert!(components[4].is_selected);
    assert!(components[4].options[0].is_selected);
    assert!(components[4].options[1].is_selected);
    assert!(components[5].is_selected);
    assert!(!components[6].is_selected);

    selection::set_option_selected(&mut components, 4, 0, false);
    assert!(!components[4].options[0].is_selected);
    assert!(!components[4].options[1].is_selected);
    assert!(!components[4].is_selected);
    assert!(!components[5].is_selected);
}

#[test]
fn wizard_tier_and_category_selection_are_additive_and_exclude_widescreen_components() {
    let make = |tier: &str, category: &str| Component {
        tier: tier.into(),
        category: vec![category.into()],
        ..Default::default()
    };
    let mut components = vec![
        make("1", "Fix"),
        make("2", "Texture"),
        make("1 - Essential", "Fix"),
        make("2 - Recommended", "Texture"),
        make("4 - Optional", "Fix"),
        make("Optional", "Extra"),
        make("Custom", "Fix"),
        make("1 - Essential", "Fix"),
    ];
    components[7]
        .metadata
        .insert("WidescreenOnly".into(), true.into());
    selection::select_tier(&mut components, "2");
    assert_eq!(
        components.iter().map(|c| c.is_selected).collect::<Vec<_>>(),
        [true, true, false, false, false, false, false, false]
    );
    selection::select_tier(&mut components, "2 - Recommended");
    assert!(!components[2].is_selected);
    assert!(components[3].is_selected);
    selection::select_tier(&mut components, "CUSTOM");
    assert!(components[6].is_selected);
    selection::select_categories(&mut components, &["fix".into()]);
    assert!(components[4].is_selected);
    assert!(components[1].is_selected); // additive: unrelated existing selections survive
    assert!(!components[7].is_selected);
    selection::select_tier(&mut components, " optional ");
    assert!(!components[5].is_selected); // wizard does not trim authored tier labels
    selection::select_tier(&mut components, "optional");
    assert!(components[0].is_selected);
    assert!(components[1].is_selected);
    assert!(components[5].is_selected);
    assert!(!components[7].is_selected);
}

#[test]
fn shared_category_tier_predicate_normalizes_tiers_and_ands_filter_groups() {
    let component = Component {
        tier: "2 - Recommended".into(),
        category: vec!["Graphics".into(), "UI".into()],
        ..Default::default()
    };
    assert!(selection::matches_category_tier(
        &component,
        &["graphics".into(), "Audio".into()],
        &["3 - Suggested".into()]
    ));
    assert!(!selection::matches_category_tier(
        &component,
        &["Audio".into()],
        &["Optional".into()]
    ));
    assert!(selection::matches_category_tier(
        &component,
        &[],
        &["RECOMMENDED".into()]
    ));
    assert!(!selection::matches_category_tier(
        &Component {
            tier: "1 - Essential".into(),
            ..Default::default()
        },
        &[],
        &["2".into()]
    ));
    assert!(selection::matches_category_tier(
        &Component {
            tier: "2".into(),
            ..Default::default()
        },
        &[],
        &["2".into()]
    ));
}

#[test]
fn cli_filters_reset_selection_while_gui_helpers_remain_additive() {
    let make = |name: &str, tier: &str, category: &str, selected: bool| Component {
        name: name.into(),
        guid: format!("{name}-id"),
        tier: tier.into(),
        category: vec![category.into()],
        is_selected: selected,
        ..Default::default()
    };
    let original = vec![
        make("Existing", "4 - Optional", "Audio", true),
        make("Match", "2 - Recommended", "Graphics", false),
        make("Wrong tier", "4 - Optional", "Graphics", true),
        make("Named", "4 - Optional", "Audio", false),
    ];

    let mut gui = original.clone();
    selection::select_categories(&mut gui, &["graphics".into()]);
    assert_eq!(
        gui.iter().map(|c| c.is_selected).collect::<Vec<_>>(),
        [true, true, true, false]
    );

    let mut cli = original;
    let warnings = selection::apply_cli_selection(
        &mut cli,
        &[
            "category:GRAPHICS".into(),
            "tier:recommended".into(),
            "named-id".into(),
        ],
    )
    .unwrap();
    assert!(warnings.is_empty());
    assert_eq!(
        cli.iter().map(|c| c.is_selected).collect::<Vec<_>>(),
        [false, true, false, true]
    );
}

#[test]
fn cli_selection_reports_and_ignores_malformed_or_unknown_selectors() {
    let mut components = vec![Component {
        name: "One".into(),
        category: vec!["Fix".into()],
        is_selected: true,
        ..Default::default()
    }];
    let warnings =
        selection::apply_cli_selection(&mut components, &["kind:value".into(), "tier:".into()])
            .unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(!components[0].is_selected);

    let error = selection::apply_cli_selection(&mut components, &["Missing".into()])
        .unwrap_err()
        .to_string();
    assert_eq!(error, "Unknown component: Missing");
    assert!(!components[0].is_selected);

    let warnings =
        selection::apply_cli_selection(&mut components, &["category:Missing".into()]).unwrap();
    assert!(warnings.is_empty());
    assert!(!components[0].is_selected);
}

#[test]
fn cli_malformed_filter_warning_stays_on_stderr_in_json_mode() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("guide.json");
    let game = dir.path().join("game");
    let source = dir.path().join("mods");
    std::fs::create_dir(&game).unwrap();
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("sample.tga"), b"sample").unwrap();
    std::fs::write(
        &input,
        r#"[{"Guid":"one","Name":"One","IsSelected":true,"Instructions":[{"Action":"Copy","Source":["<<modDirectory>>/sample.tga"],"Destination":"<<kotorDirectory>>/Override"}]}]"#,
    )
    .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_modsync"))
        .args(["validate", "--input"])
        .arg(&input)
        .arg("--game-dir")
        .arg(&game)
        .arg("--source-dir")
        .arg(&source)
        .args(["--select", "kind:value", "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for line in String::from_utf8(output.stdout).unwrap().lines() {
        serde_json::from_str::<serde_json::Value>(line).unwrap();
    }
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("Selection warning: Unknown selection type: 'kind'")
    );
}

#[test]
fn cli_exact_colon_names_take_precedence_over_filter_syntax() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("guide.json");
    let game = dir.path().join("game");
    let source = dir.path().join("mods");
    std::fs::create_dir(&game).unwrap();
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("sample.tga"), b"sample").unwrap();
    std::fs::write(
        &input,
        r#"[
          {"Guid":"colon","Name":"Foo: Bar","Instructions":[{"Action":"Copy","Source":["<<modDirectory>>/sample.tga"],"Destination":"<<kotorDirectory>>/Override"}]},
          {"Guid":"collision","Name":"category:Foo","Instructions":[{"Action":"Copy","Source":["<<modDirectory>>/sample.tga"],"Destination":"<<kotorDirectory>>/Override"}]},
          {"Guid":"other","Name":"Unrelated","Category":["Foo"],"Instructions":[{"Action":"Copy","Source":["<<modDirectory>>/sample.tga"],"Destination":"<<kotorDirectory>>/Override"}]}
        ]"#,
    )
    .unwrap();

    for (selector, expected) in [
        ("Foo: Bar", "OK: Foo: Bar"),
        ("category:Foo", "OK: category:Foo"),
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_modsync"))
            .args(["validate", "--input"])
            .arg(&input)
            .arg("--game-dir")
            .arg(&game)
            .arg("--source-dir")
            .arg(&source)
            .args(["--select", selector, "--validation-report"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["componentCount"], 1);
        assert_eq!(
            report["stages"][0]["messages"],
            serde_json::json!([expected])
        );
    }
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

#[test]
fn required_badge_counts_selected_dependency_declarations_not_targets_or_tiers() {
    let components: Vec<Component> = serde_json::from_value(serde_json::json!([
        {"Guid":"target", "Tier":"1", "IsSelected":true},
        {"Guid":"dependent", "Tier":"Optional", "Dependencies":["target","other"], "IsSelected":true},
        {"Dependencies":["target"], "IsSelected":false},
        {"Dependencies":["target"], "WidescreenOnly":true,"IsSelected":true},
        {"Dependencies":[], "WidescreenOnly":true,"IsSelected":true},
        {"Dependencies":[], "IsSelected":true}
    ])).unwrap();
    assert_eq!(selection::selection_badges(&components), (1, 2));
    assert_eq!(selection::selection_badges(&[]), (0, 0));
}

#[test]
fn category_groups_sort_display_without_reordering_guide_or_using_safe_names() {
    let components: Vec<Component> = serde_json::from_value(serde_json::json!([
        {"Name":"Zulu", "NameSpoilerFree":"A", "Category":["Fix","Texture"]},
        {"Name":"Alpha", "NameSpoilerFree":"Z", "Category":["Fix"]},
        {"Name":"Other mod"},
        {"Name":"Lowercase", "Category":["fix"]}
    ]))
    .unwrap();
    assert_eq!(
        selection::category_groups(&components, "", "", "", true),
        vec![
            ("Fix".into(), vec![1, 0]),
            ("Other".into(), vec![2]),
            ("fix".into(), vec![3])
        ]
    );
    assert_eq!(
        selection::category_groups(&components, "", "", "Texture", false),
        vec![("Fix".into(), vec![0])]
    );
    assert!(selection::category_groups(&components, "missing", "", "", false).is_empty());
    assert_eq!(components[0].name, "Zulu");
}
