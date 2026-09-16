use modsync::{Component, editor};

fn component(guid: &str) -> Component {
    Component {
        guid: guid.into(),
        name: guid.into(),
        ..Default::default()
    }
}

#[test]
fn shared_editor_reorders_and_rejects_unsafe_deletion() {
    let mut components = vec![component("a"), component("b")];
    editor::move_component(&mut components, "a", 1).unwrap();
    assert_eq!(components[0].guid, "b");
    components[0].dependencies.push("a".into());
    assert!(editor::remove_component(&mut components, "a").is_err());
}

#[test]
fn instructions_have_checked_insert_and_move_boundaries() {
    let mut component = component("a");
    editor::add_instruction(&mut component, 0).unwrap();
    assert_eq!(component.instructions[0].action, "Unset");
    assert!(editor::move_instruction(&mut component, 0, 1).is_err());
    assert_eq!(
        editor::remove_instruction(&mut component, 0)
            .unwrap()
            .action,
        "Unset"
    );
    assert!(editor::remove_instruction(&mut component, 0).is_err());
}

#[test]
fn new_components_receive_a_unique_editable_identity() {
    let mut components = vec![component("component-2")];
    let index = editor::add_component(&mut components);
    assert_eq!(components[index].guid, "component-3");
    assert_eq!(components[index].name, "component-3");
}
