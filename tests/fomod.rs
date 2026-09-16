use modsync::fomod::{GroupSelection, GroupType, PluginSelection};

fn plugin(name: &str, required: bool, selected: bool) -> PluginSelection {
    PluginSelection {
        name: name.into(),
        required,
        selected,
    }
}
fn selected(group: &GroupSelection) -> Vec<bool> {
    group.plugins().iter().map(|p| p.selected).collect()
}

#[test]
fn selection_modes_match_pinned_csharp_oracle() {
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../docs/parity/fomod-selection-oracle/result.json"
    ))
    .unwrap();
    for case in reference["Groups"].as_array().unwrap() {
        let kind = serde_json::from_value(case["Group"].clone()).unwrap();
        let mut group = GroupSelection::new(
            "Group".into(),
            kind,
            vec![
                plugin("First", false, false),
                plugin("Second", false, false),
            ],
        );
        assert_eq!(serde_json::json!(selected(&group)), case["Defaults"]);
        assert!(group.validate().is_ok());
        group.set_selected(0, false).unwrap();
        assert_eq!(
            serde_json::json!(group.validate().err().map(|e| e.to_string())),
            case["ClearedValidation"]
        );
        group.set_selected(1, true).unwrap();
        assert_eq!(serde_json::json!(selected(&group)), case["Final"]);
        assert_eq!(
            serde_json::json!(group.validate().err().map(|e| e.to_string())),
            case["FinalValidation"]
        );
    }
}

#[test]
fn required_choices_cannot_be_removed_directly_or_through_exclusive_siblings() {
    for kind in [GroupType::SelectExactlyOne, GroupType::SelectAtMostOne] {
        let mut group = GroupSelection::new(
            "Required".into(),
            kind,
            vec![
                plugin("Required", true, false),
                plugin("Optional", false, false),
            ],
        );
        for (index, value) in [(0, false), (1, true), (2, true)] {
            assert!(group.set_selected(index, value).is_err());
            assert_eq!(selected(&group), [true, false]);
            group.validate().unwrap();
        }
        let conflicting = GroupSelection::new(
            "Conflicting".into(),
            kind,
            vec![plugin("A", true, true), plugin("B", true, true)],
        );
        assert!(conflicting.validate().is_err());
    }
    let recommended = GroupSelection::new(
        "Recommended".into(),
        GroupType::SelectExactlyOne,
        vec![plugin("A", false, false), plugin("B", false, true)],
    );
    assert_eq!(selected(&recommended), [false, true]);
    for (kind, valid) in [
        (GroupType::SelectAny, true),
        (GroupType::SelectAll, true),
        (GroupType::SelectAtMostOne, true),
        (GroupType::SelectExactlyOne, false),
        (GroupType::SelectAtLeastOne, false),
    ] {
        assert_eq!(
            GroupSelection::new("Empty".into(), kind, vec![])
                .validate()
                .is_ok(),
            valid
        );
    }
}
