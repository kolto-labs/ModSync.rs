use modsync::{
    Guide,
    profiles::{Profile, Selection, Store, sanitize},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

fn fixture() -> Profile {
    serde_json::from_str(include_str!("fixtures/profile.json")).unwrap()
}

#[test]
fn profile_crud_preserves_schema_and_clone_independence() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path()).unwrap();
    assert!(store.list().unwrap().is_empty());
    assert!(!store.directory().exists());
    let p = store.create("  Alpha  ").unwrap();
    assert_eq!(p.name, "Alpha");
    assert!(store.create("Alpha").is_err());
    store.save(&fixture()).unwrap();
    assert_eq!(store.require("Loadout").unwrap(), fixture());
    let mut copy = store.clone_profile("Loadout", "Beta").unwrap();
    copy.component_selections.clear();
    store.save(&copy).unwrap();
    assert_eq!(
        store.require("Loadout").unwrap().component_selections.len(),
        1
    );
    assert_eq!(copy.last_used_utc, fixture().last_used_utc);
    assert_ne!(copy.created_utc, fixture().created_utc);
    store.rename("Beta", "Gamma").unwrap();
    assert!(store.load("Beta").unwrap().is_none());
    assert!(store.rename("Gamma", "Alpha").is_err());
    store.rename("Gamma", " Gamma ").unwrap();
    fs::write(store.directory().join("broken.json"), "{").unwrap();
    fs::write(store.directory().join("empty.json"), "{}").unwrap();
    assert_eq!(
        store
            .list()
            .unwrap()
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["Alpha", "Gamma", "Loadout"]
    );
    assert!(store.load("broken").unwrap().is_none());
    assert!(store.delete("Gamma").unwrap());
    assert!(!store.delete("Gamma").unwrap());
    assert_eq!(sanitize(" ../Full\\Build ").unwrap(), ".._Full_Build");
    assert!(sanitize(".._ ").is_err());
    assert_eq!(
        store.artifact_directory("A/B").unwrap(),
        store.directory().join("A_B")
    );
}
#[test]
fn capture_apply_preserves_absent_components_and_creation_time() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path()).unwrap();
    let mut guide=Guide::parse(r#"[{"Guid":"11111111-1111-4111-8111-111111111111","Name":"One","IsSelected":true,"Options":[{"Guid":"22222222-2222-4222-8222-222222222222","Name":"Choice","IsSelected":true}]},{"Guid":"33333333-3333-4333-8333-333333333333","Name":"Unmentioned","IsSelected":false}]"#,"json").unwrap();
    store.save(&fixture()).unwrap();
    store.apply("Loadout", &mut guide).unwrap();
    assert!(!guide.components[0].is_selected);
    assert!(guide.components[0].options[0].is_selected);
    assert!(!guide.components[1].is_selected);
    let captured = store
        .capture(
            "Loadout",
            &guide,
            Some(Path::new("guide.toml")),
            Some(dir.path()),
            None,
        )
        .unwrap();
    assert_eq!(captured.created_utc, fixture().created_utc);
    assert!(
        captured
            .instruction_file_path
            .as_ref()
            .unwrap()
            .ends_with("guide.toml")
    );
    assert!(captured.mod_directory.is_none());
    guide.components[0].is_selected = true;
    guide.components[0].options[0].is_selected = false;
    store.apply("Loadout", &mut guide).unwrap();
    assert!(!guide.components[0].is_selected);
    assert!(guide.components[0].options[0].is_selected);
    let mut empty = fixture();
    empty.component_selections =
        BTreeMap::from([(guide.components[0].guid.clone(), Selection::default())]);
    empty.apply(&mut guide);
    assert!(!guide.components[0].options[0].is_selected);
}
#[test]
fn activation_merges_settings_and_rejects_corrupt_settings() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path()).unwrap();
    store.create("Active").unwrap();
    let path = dir.path().join("settings.json");
    fs::write(
        &path,
        r#"{"theme":"dark","managedDeploymentEnabled":true,"nested":{"keep":7}}"#,
    )
    .unwrap();
    store.activate("Active").unwrap();
    let value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        value,
        json!({"theme":"dark","managedDeploymentEnabled":true,"nested":{"keep":7},"activeProfileName":"Active"})
    );
    assert_eq!(store.active_name().unwrap().as_deref(), Some("Active"));
    fs::write(&path, "bad").unwrap();
    assert!(store.activate("Active").is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), "bad");
}
#[test]
fn profile_cli_manages_storage_and_applies_selections() {
    let dir = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_modsync"))
            .args(["profile", "--settings-dir"])
            .arg(dir.path())
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice::<Value>(&out.stdout).unwrap()
    };
    run(&["--action", "create", "--name", "Alpha"]);
    run(&["--action", "clone", "--from", "Alpha", "--to", "Beta"]);
    run(&["--action", "rename", "--from", "Beta", "--to", "Gamma"]);
    run(&["--action", "activate", "--name", "Gamma"]);
    let list = run(&["--action", "list", "--json"]);
    assert_eq!(list[1]["Name"], "Gamma");
    assert_eq!(list[1]["IsActive"], true);
    let store = Store::new(dir.path()).unwrap();
    store.save(&fixture()).unwrap();
    let guide = dir.path().join("guide.json");
    let output = dir.path().join("applied.json");
    let text =
        r#"[{"Guid":"11111111-1111-4111-8111-111111111111","Name":"One","IsSelected":true}]"#;
    fs::write(&guide, text).unwrap();
    run(&[
        "--action",
        "apply",
        "--name",
        "Loadout",
        "--input",
        guide.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
    ]);
    let written: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(written[0]["IsSelected"], false);
    assert_eq!(fs::read_to_string(guide).unwrap(), text);
    run(&["--action", "delete", "--name", "Alpha"]);
    assert!(store.load("Alpha").unwrap().is_none());
}

#[test]
fn concurrent_creates_do_not_replace_an_existing_profile() {
    let dir = tempfile::tempdir().unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let path = dir.path().to_owned();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let store = Store::new(&path).unwrap();
                barrier.wait();
                store.create("Shared").is_ok()
            })
        })
        .collect();
    let successes = handles
        .into_iter()
        .map(|h| h.join().unwrap() as usize)
        .sum::<usize>();
    assert_eq!(successes, 1);
    assert_eq!(Store::new(dir.path()).unwrap().list().unwrap().len(), 1);
}
