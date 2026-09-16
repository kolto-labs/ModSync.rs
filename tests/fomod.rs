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

#[test]
fn choices_file_schema_and_named_selections_reject_ambiguous_or_invalid_changes() {
    use modsync::fomod::ChoicesFile;
    let json = r#"{"archives":[{"archiveFileName":"Example.zip","selections":[{"stepName":"Step","groupName":"Group","plugins":["second","SECOND"]}]}]}"#;
    let file = ChoicesFile::parse(json).unwrap();
    let archive = file.archive("EXAMPLE.ZIP").unwrap();
    let mut group = GroupSelection::new(
        "Group".into(),
        GroupType::SelectExactlyOne,
        vec![
            plugin("First", false, false),
            plugin("Second", false, false),
        ],
    );
    group.apply_names(&archive.selections[0].plugins).unwrap();
    assert_eq!(selected(&group), [false, true]);
    for names in [
        vec![],
        vec!["Unknown".into()],
        vec!["First".into(), "Second".into()],
    ] {
        assert!(group.apply_names(&names).is_err());
        assert_eq!(selected(&group), [false, true]);
    }
    let mut required = GroupSelection::new(
        "Group".into(),
        GroupType::SelectAny,
        vec![plugin("Required", true, false)],
    );
    assert!(required.apply_names(&[]).is_err());
    assert_eq!(selected(&required), [true]);
    let mut ambiguous = GroupSelection::new(
        "Group".into(),
        GroupType::SelectAny,
        vec![plugin("Same", false, false), plugin("SAME", false, false)],
    );
    assert!(ambiguous.apply_names(&["same".into()]).is_err());
    assert_eq!(selected(&ambiguous), [false, false]);
    for invalid in [
        r#"{"version":2}"#,
        r#"{"archives":[{"archiveFileName":"a.zip"},{"archiveFileName":"A.ZIP"}]}"#,
        r#"{"archives":[{"archiveFileName":"a.zip","selections":[{"stepName":"S","groupName":"G"},{"stepName":"s","groupName":"g"}]}]}"#,
    ] {
        assert!(ChoicesFile::parse(invalid).is_err());
    }
    let saved = serde_json::to_string(&file).unwrap();
    assert!(saved.contains("archiveFileName") && saved.contains("stepName"));
    assert_eq!(ChoicesFile::parse(&saved).unwrap().version, 1);
}
