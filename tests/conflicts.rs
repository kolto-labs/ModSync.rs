use modsync::{Engine, Guide, Settings};
use serde_json::json;
use std::{fs, process::Command};

#[test]
fn conflicts_follow_real_writes_and_cli_is_read_only() {
    let root = tempfile::tempdir().unwrap();
    let mods = root.path().join("mods");
    let game = root.path().join("game");
    fs::create_dir_all(mods.join("a")).unwrap();
    fs::create_dir_all(mods.join("b")).unwrap();
    fs::create_dir(&game).unwrap();
    fs::write(mods.join("a/asset.txt"), b"first").unwrap();
    fs::write(mods.join("b/ASSET.TXT"), b"last").unwrap();
    let copy = |source: &str, destination: &str, overwrite: bool| {
        json!({
            "Action":"Copy", "Source":[source], "Destination":destination,"Overwrite":overwrite
        })
    };
    let text = json!({"thisMod":[
        {"Guid":"11111111-1111-1111-1111-111111111111","Name":"First","IsSelected":true,"Instructions":[
            copy("<<modDirectory>>/a/*", "<<gameDirectory>>/Override",true),
            copy("<<modDirectory>>/a/*", "<<gameDirectory>>/Override",true),
            copy("<<modDirectory>>/a/*", "<<modDirectory>>/staging",true)
        ]},
        {"Guid":"22222222-2222-2222-2222-222222222222","Name":"Skipped overwrite","IsSelected":true,"Instructions":[
            copy("<<modDirectory>>/b/*", "<<gameDirectory>>/override",false)
        ]},
        {"Guid":"33333333-3333-3333-3333-333333333333","Name":"Last","IsSelected":true,"Instructions":[
            copy("<<modDirectory>>/b/*", "<<gameDirectory>>/override",true),
            copy("<<modDirectory>>/b/*", "<<modDirectory>>/staging",true)
        ]}
    ]})
    .to_string();
    let guide = Guide::parse(&text, "json").unwrap();
    let settings = Settings::new(&mods, &game).unwrap();
    let report = Engine::new(settings.clone())
        .unwrap()
        .analyze_conflicts(&guide)
        .unwrap();
    assert_eq!(report.analyzed_component_count, 3);
    assert_eq!(report.conflicts.len(), 1);
    let conflict = &report.conflicts[0];
    assert_eq!(
        conflict.destination_path,
        "<<kotorDirectory>>/Override/asset.txt"
    );
    assert_eq!(
        conflict.winner_component_guid,
        "33333333-3333-3333-3333-333333333333"
    );
    assert_eq!(conflict.writers.len(), 2);
    assert_eq!(conflict.writers[0].instruction_index, 1);
    assert_eq!(conflict.writers[1].instruction_index, 0);
    assert_eq!(report.conflict_counts_by_component.len(), 2);
    // Match the captured C# service output, allowing only serialization casing
    // and Rust's explicit limitations field to differ.
    fn camel_keys(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => serde_json::Value::Object(
                map.into_iter()
                    .map(|(key, value)| {
                        let key = format!("{}{}", key[..1].to_ascii_lowercase(), &key[1..]);
                        (key, camel_keys(value))
                    })
                    .collect(),
            ),
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(camel_keys).collect())
            }
            other => other,
        }
    }
    let mut reference = camel_keys(
        serde_json::from_str(include_str!("../docs/parity/conflict-oracle/result.json")).unwrap(),
    );
    assert_eq!(
        reference.as_object_mut().unwrap().remove("hasConflicts"),
        Some(json!(true))
    );
    let mut actual = serde_json::to_value(&report).unwrap();
    assert_eq!(
        actual.as_object_mut().unwrap().remove("limitations"),
        Some(json!([]))
    );
    assert_eq!(actual, reference);
    assert!(!mods.join("staging").exists());
    assert_eq!(fs::read_dir(&game).unwrap().count(), 0);

    let input = root.path().join("guide.json");
    fs::write(&input, text).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_modsync"))
        .arg("conflicts")
        .arg("--input")
        .arg(&input)
        .arg("--source-dir")
        .arg(&mods)
        .arg("--game-dir")
        .arg(&game)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cli, serde_json::to_value(report).unwrap());
    assert_eq!(fs::read_dir(&game).unwrap().count(), 0);
    assert!(!mods.join("staging").exists());
    Engine::new(settings)
        .unwrap()
        .install(&guide, |_| {})
        .unwrap();
    assert_eq!(fs::read(game.join("Override/asset.txt")).unwrap(), b"last");
}

#[test]
fn failed_or_cancelled_analysis_does_not_return_a_partial_report() {
    let root = tempfile::tempdir().unwrap();
    let mods = root.path().join("mods");
    let game = root.path().join("game");
    fs::create_dir(&mods).unwrap();
    fs::create_dir(&game).unwrap();
    let guide = Guide::parse(r#"{"thisMod":[{"Guid":"a","Name":"Missing","Instructions":[{"Action":"Copy","Source":["<<modDirectory>>/missing.txt"],"Destination":"<<gameDirectory>>/Override"}]}]}"#, "json").unwrap();
    let settings = Settings::new(&mods, &game).unwrap();
    assert!(
        Engine::new(settings.clone())
            .unwrap()
            .analyze_conflicts(&guide)
            .is_err()
    );
    settings
        .cancelled
        .store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(
        Engine::new(settings)
            .unwrap()
            .analyze_conflicts(&guide)
            .is_err()
    );
    assert_eq!(fs::read_dir(&game).unwrap().count(), 0);
}
