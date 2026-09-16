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
}
