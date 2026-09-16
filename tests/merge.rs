use modsync::{
    Component, Instruction,
    merge::{self, Options},
};

fn component(guid: &str, name: &str) -> Component {
    Component {
        guid: guid.into(),
        name: name.into(),
        ..Default::default()
    }
}

#[test]
fn matches_guid_then_normalized_name_author_and_preserves_existing_guid() {
    let mut existing = component("existing", "A Mod!");
    existing.author = "Author".into();
    existing.instructions.push(Instruction {
        action: "Copy".into(),
        source: vec!["old".into()],
        destination: "Override".into(),
        guid: String::new(),
        parent: String::new(),
        arguments: String::new(),
        overwrite: true,
        dependencies: vec![],
        restrictions: vec![],
    });
    let mut incoming = component("incoming", "a mod");
    incoming.author = "author".into();
    incoming.instructions.push(Instruction {
        action: "Copy".into(),
        source: vec!["new".into()],
        destination: "Override".into(),
        guid: String::new(),
        parent: String::new(),
        arguments: String::new(),
        overwrite: true,
        dependencies: vec![],
        restrictions: vec![],
    });
    let result = merge::components(&[existing], &[incoming], &Options::default());
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].guid, "existing");
    assert_eq!(result[0].instructions.len(), 1);
}

#[test]
fn preserves_unmatched_components_and_honors_exclusion() {
    let result = merge::components(
        &[component("old", "Old")],
        &[component("new", "New")],
        &Options::default(),
    );
    assert_eq!(
        result.iter().map(|c| c.guid.as_str()).collect::<Vec<_>>(),
        ["new", "old"]
    );
    let result = merge::components(
        &[component("old", "Old")],
        &[component("new", "New")],
        &Options {
            exclude_existing_only: true,
            ..Default::default()
        },
    );
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].guid, "new");
}

#[test]
fn merges_option_content_and_resource_filenames_by_name() {
    let mut existing = component("shared", "Shared");
    let mut old_option = component("old-option", "Compatibility");
    old_option.description = "Existing description".into();
    old_option.dependencies.push("dependency-a".into());
    old_option.instructions.push(Instruction {
        action: "Copy".into(),
        source: vec!["existing-file".into()],
        destination: "Override".into(),
        guid: "old-step".into(),
        parent: String::new(),
        arguments: String::new(),
        overwrite: true,
        dependencies: vec![],
        restrictions: vec![],
    });
    old_option.resource_registry.insert(
        "https://example.test/mod".into(),
        serde_json::json!({"Files": {"existing.zip": true}}),
    );
    existing.options.push(old_option);

    let mut incoming = component("shared", "Shared");
    let mut new_option = component("new-option", "compatibility");
    new_option.dependencies.push("dependency-b".into());
    new_option.instructions.push(Instruction {
        action: "Copy".into(),
        source: vec!["incoming-file".into()],
        destination: "Override".into(),
        guid: "new-step".into(),
        parent: String::new(),
        arguments: String::new(),
        overwrite: false,
        dependencies: vec![],
        restrictions: vec![],
    });
    new_option.resource_registry.insert(
        "https://example.test/mod".into(),
        serde_json::json!({"Files": {"incoming.zip": false}}),
    );
    incoming.options.push(new_option);

    let result = merge::components(&[existing], &[incoming], &Options::default());
    let option = &result[0].options[0];
    assert_eq!(option.guid, "new-option");
    assert_eq!(option.description, "Existing description");
    assert_eq!(option.dependencies, ["dependency-b", "dependency-a"]);
    assert_eq!(option.instructions[0].guid, "old-step");
    assert_eq!(option.instructions[0].source, ["existing-file"]);
    let files = option.resource_registry["https://example.test/mod"]["Files"]
        .as_object()
        .unwrap();
    assert_eq!(files["incoming.zip"], false);
    assert_eq!(files["existing.zip"], true);
}
