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
