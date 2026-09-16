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
