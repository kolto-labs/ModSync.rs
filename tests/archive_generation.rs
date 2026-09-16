use modsync::{Component, Engine, Guide, Settings, archive, archive_generation};
use serde_json::{Value, json};
use std::{fs, io::Write, path::Path, sync::atomic::AtomicBool};

fn zip(path: &Path, files: &Value) {
    let mut archive = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, content) in files.as_object().unwrap() {
        archive
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive
            .write_all(content.as_str().unwrap().as_bytes())
            .unwrap();
    }
    archive.finish().unwrap();
}

fn normalized(component: &Component, success: bool) -> Value {
    let step = |i: &modsync::Instruction| {
        let mut value = serde_json::to_value(i).unwrap();
        value.as_object_mut().unwrap().remove("Guid");
        value.as_object_mut().unwrap().remove("Parent");
        if i.action == "Choose" {
            value["Source"] = json!(
                i.source
                    .iter()
                    .map(|id| &component
                        .options
                        .iter()
                        .find(|o| &o.guid == id)
                        .unwrap()
                        .name)
                    .collect::<Vec<_>>()
            );
        }
        value
    };
    json!({"Name":component.name,"Success":success,
        "InstallationMethod":component.metadata.get("InstallationMethod").cloned().unwrap_or(json!("")),
        "Instructions":component.instructions.iter().map(step).collect::<Vec<_>>(),
        "Options":component.options.iter().map(|o| json!({"Name":o.name,"Description":o.description,"IsSelected":o.is_selected,"Instructions":o.instructions.iter().map(step).collect::<Vec<_>>()})).collect::<Vec<_>>()})
}

#[test]
fn archive_generation_tracks_reference_outputs_and_unresolved_differences() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/archive-generation.json")).unwrap();
    let expected: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/csharp/archive-generation.json")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    for (case, expected) in cases.iter().zip(&expected) {
        let mut value = json!({"Guid":"00000000-0000-4000-8000-000000000001","Name":case["Name"]});
        if let Some(fields) = case.get("Component").and_then(Value::as_object) {
            value.as_object_mut().unwrap().extend(fields.clone());
        }
        let mut component: Component = serde_json::from_value(value).unwrap();
        let path = dir.path().join(format!(
            "{}.{}",
            case["Name"].as_str().unwrap(),
            case["Extension"].as_str().unwrap_or("zip")
        ));
        if let Some(payload) = case["Payload"].as_str() {
            fs::write(&path, payload).unwrap();
        } else {
            zip(&path, &case["Files"]);
        }
        let original = fs::read(&path).unwrap();
        let success = if case["LocalMatching"] == true {
            let mut guide = Guide {
                components: vec![component.clone()],
                ..Default::default()
            };
            let success =
                archive_generation::generate_local(&mut guide, dir.path()).unwrap()[0].success;
            component = guide.components.remove(0);
            success
        } else {
            archive_generation::generate_from_archive(
                &mut component,
                &path,
                &archive::Options::default(),
                &AtomicBool::new(false),
            )
            .unwrap()
        };
        let mut expected = expected.clone();
        if let Some(additional) = case.get("UnresolvedAdditionalInstruction") {
            // Keep the captured C# object unchanged on disk. This exact observed
            // difference remains open in the parity ledger, not a parity pass.
            expected["Instructions"]
                .as_array_mut()
                .unwrap()
                .push(additional.clone());
        }
        assert_eq!(
            normalized(&component, success),
            expected,
            "{}",
            case["Name"]
        );
        assert_eq!(fs::read(&path).unwrap(), original);
    }
    assert_eq!(cases.len(), expected.len());
}

#[test]
fn inference_preserves_inputs_and_model_on_failure_and_installs_chosen_files() {
    let dir = tempfile::tempdir().unwrap();
    let mods = dir.path().join("mods");
    let game = dir.path().join("game");
    let scratch = dir.path().join("scratch");
    for path in [&mods, &game, &scratch] {
        fs::create_dir(path).unwrap();
    }
    let path = mods.join("sample.zip");
    zip(
        &path,
        &json!({"Red/sample.tga":"red","Blue/sample.tga":"blue"}),
    );
    let original = fs::read(&path).unwrap();
    let mut guide = Guide::parse(r#"[{"Guid":"example","Name":"Sample","ResourceRegistry":{"https://example.com/123-sample/":{}}}]"#, "json").unwrap();
    let reports = archive_generation::generate_local(&mut guide, &mods).unwrap();
    assert!(reports[0].success, "{:?}", reports[0]);
    guide.components[0].options[1].is_selected = true;
    let settings = Settings::new(&mods, &game).unwrap();
    Engine::new(settings.clone())
        .unwrap()
        .validate(&guide, |_| {})
        .unwrap();
    assert_eq!(fs::read_dir(&game).unwrap().count(), 0);
    assert_eq!(fs::read_dir(&mods).unwrap().count(), 1);
    assert_eq!(fs::read(&path).unwrap(), original);
    Engine::new(settings)
        .unwrap()
        .install(&guide, |_| {})
        .unwrap();
    assert_eq!(fs::read(game.join("Override/sample.tga")).unwrap(), b"blue");
    assert_eq!(
        fs::read(mods.join("sample/Red/sample.tga")).unwrap(),
        b"red"
    );
    let saved = serde_json::to_value(&guide).unwrap();
    let reports = archive_generation::generate_local(&mut guide, &mods).unwrap();
    assert!(reports[0].skip_reason.contains("already has instructions"));
    assert_eq!(serde_json::to_value(&guide).unwrap(), saved);

    let mut empty = Component {
        guid: "empty".into(),
        name: "Empty".into(),
        ..Default::default()
    };
    let before = serde_json::to_value(&empty).unwrap();
    for (name, files) in [
        ("traversal.zip", json!({"../escape.tga":"bad"})),
        ("budget.zip", json!({"sample.tga":"too many bytes"})),
    ] {
        let path = mods.join(name);
        zip(&path, &files);
        let bytes = fs::read(&path).unwrap();
        let options = archive::Options {
            scratch_dir: Some(scratch.clone()),
            max_expanded_bytes: Some(1),
        };
        assert!(
            archive_generation::generate_from_archive(
                &mut empty,
                &path,
                &options,
                &AtomicBool::new(false)
            )
            .is_err()
        );
        assert_eq!(serde_json::to_value(&empty).unwrap(), before);
        assert_eq!(fs::read(path).unwrap(), bytes);
        assert_eq!(fs::read_dir(&scratch).unwrap().count(), 0);
    }
    fs::write(mods.join("corrupt.zip"), b"not an archive").unwrap();
    assert!(
        archive_generation::generate_from_archive(
            &mut empty,
            &mods.join("corrupt.zip"),
            &archive::Options::default(),
            &AtomicBool::new(false)
        )
        .is_err()
    );
    assert_eq!(
        fs::read(mods.join("corrupt.zip")).unwrap(),
        b"not an archive"
    );
    assert_eq!(serde_json::to_value(&empty).unwrap(), before);
}

#[test]
fn cli_local_generation_reports_skips_and_requires_source_directory() {
    let dir = tempfile::tempdir().unwrap();
    zip(
        &dir.path().join("sample.zip"),
        &json!({"sample.tga":"texture"}),
    );
    let source = dir.path().join("input.json");
    let output = dir.path().join("output.json");
    let text = r#"[{"Guid":"example","Name":"Sample","ResourceRegistry":{"sample.zip":{}}},{"Guid":"skip","Name":"Missing","ResourceRegistry":{}}]"#;
    fs::write(&source, text).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_modsync"))
        .args(["convert", "--input"])
        .arg(&source)
        .args(["--output"])
        .arg(&output)
        .args(["--format", "json", "--auto-generate-local", "--source-path"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let reports: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(reports[0]["Success"], true);
    assert_eq!(reports[1]["Success"], false);
    assert!(!reports[1]["SkipReason"].as_str().unwrap().is_empty());
    let guide = Guide::load(&output).unwrap();
    assert_eq!(guide.components[0].instructions.len(), 2);
    assert!(guide.components[1].instructions.is_empty());
    assert_eq!(fs::read_to_string(source).unwrap(), text);
    let missing = std::process::Command::new(env!("CARGO_BIN_EXE_modsync"))
        .args(["convert", "--auto-generate-local"])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("--source-path"));
}

#[test]
fn archive_matching_prefers_exact_then_newest_and_preserves_unrelated_options() {
    let dir = tempfile::tempdir().unwrap();
    for (name, seconds) in [
        ("sample.zip", 1000),
        ("sample-v1.zip", 2000),
        ("sample-v2.zip", 3000),
    ] {
        let path = dir.path().join(name);
        zip(&path, &json!({"sample.tga":"texture"}));
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds))
            .unwrap();
    }
    let original = Guide::parse(r#"[{"Guid":"sample","Name":"Sample","ResourceRegistry":{"https://example.com/123-Sam_ple/":{}}}]"#, "json").unwrap();
    let mut guide = original.clone();
    assert!(archive_generation::generate_local(&mut guide, dir.path()).unwrap()[0].success);
    assert_eq!(
        guide.components[0].instructions[0].source,
        ["<<modDirectory>>\\sample.zip"]
    );
    fs::remove_file(dir.path().join("sample.zip")).unwrap();
    guide = original.clone();
    assert!(archive_generation::generate_local(&mut guide, dir.path()).unwrap()[0].success);
    assert_eq!(
        guide.components[0].instructions[0].source,
        ["<<modDirectory>>\\sample-v2.zip"]
    );
    guide = original.clone();
    guide.components[0].options.push(Component {
        guid: "option".into(),
        name: "Authored".into(),
        ..Default::default()
    });
    let saved = serde_json::to_value(&guide).unwrap();
    assert!(archive_generation::generate_local(&mut guide, dir.path()).unwrap()[0].success);
    assert_eq!(
        serde_json::to_value(&guide.components[0].options).unwrap(),
        saved["thisMod"][0]["Options"]
    );
    guide = original;
    guide.components[0]
        .resource_registry
        .insert("another.zip".into(), json!({}));
    assert!(archive_generation::generate_local(&mut guide, dir.path()).unwrap()[0].success);
    assert_eq!(
        guide.components[0].instructions[0].source,
        ["<<modDirectory>>\\sample-v2.zip"]
    );
    let mut component = Component {
        name: "Remove Duplicate TGA/TPC".into(),
        ..Default::default()
    };
    assert!(
        archive_generation::generate_from_archive(
            &mut component,
            &dir.path().join("sample-v2.zip"),
            &archive::Options::default(),
            &AtomicBool::new(true)
        )
        .is_err()
    );
    assert!(component.instructions.is_empty());
}

#[test]
fn inferred_choices_reuse_authored_identity_selection_and_install_behavior() {
    let dir = tempfile::tempdir().unwrap();
    let mods = dir.path().join("mods");
    let game = dir.path().join("game");
    fs::create_dir(&mods).unwrap();
    fs::create_dir(&game).unwrap();
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/archive-generation.json")).unwrap();
    let case = cases.iter().find(|c| c["Name"] == "reuse").unwrap();
    zip(&mods.join("reuse.zip"), &case["Files"]);
    let mut component: Component = serde_json::from_value(case["Component"].clone()).unwrap();
    component.guid = "component".into();
    component.name = "reuse".into();
    component.is_selected = true;
    let authored = component.options[0].clone();
    archive_generation::generate_from_archive(
        &mut component,
        &mods.join("reuse.zip"),
        &archive::Options::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(component.options.len(), 2);
    assert_eq!(
        serde_json::to_value(&component.options[0]).unwrap(),
        serde_json::to_value(&authored).unwrap()
    );
    let choose = component
        .instructions
        .iter()
        .find(|i| i.action == "Choose")
        .unwrap();
    assert_eq!(choose.source[0], authored.guid);
    let guide = Guide {
        components: vec![component],
        ..Default::default()
    };
    Engine::new(Settings::new(&mods, &game).unwrap())
        .unwrap()
        .install(&guide, |_| {})
        .unwrap();
    assert_eq!(fs::read(game.join("Override/sample.tga")).unwrap(), b"red");
    assert_eq!(
        fs::read(mods.join("reuse/Blue/sample.tga")).unwrap(),
        b"blue"
    );
}

#[test]
fn executable_inference_never_runs_the_input_or_bypasses_engine_trust_and_cancellation() {
    let dir = tempfile::tempdir().unwrap();
    let mods = dir.path().join("mods");
    let game = dir.path().join("game");
    fs::create_dir(&mods).unwrap();
    fs::create_dir(&game).unwrap();
    let executable = mods.join("installer.exe");
    let bytes = b"MZfixture-not-a-runnable-program";
    fs::write(&executable, bytes).unwrap();
    let mut guide = Guide::parse(
        r#"[{"Guid":"installer","Name":"Installer","ResourceRegistry":{"installer.exe":{}}}]"#,
        "json",
    )
    .unwrap();
    let reports = archive_generation::generate_local(&mut guide, &mods).unwrap();
    assert!(reports[0].success, "{:?}", reports[0]);
    assert_eq!(guide.components[0].instructions[0].action, "Execute");
    assert_eq!(
        guide.components[0].metadata["InstallationMethod"],
        "Executable Installer"
    );
    let settings = Settings::new(&mods, &game).unwrap();
    let error = Engine::new(settings.clone())
        .unwrap()
        .install(&guide, |_| {})
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("needs explicit trust"),
        "{error:#}"
    );
    let mut trusted = settings;
    trusted.allow_processes = true;
    // Even with trust configured, validation only checks the executable's presence.
    // Launching this deliberately invalid image would fail on every supported OS.
    Engine::new(trusted)
        .unwrap()
        .validate(&guide, |_| {})
        .unwrap();
    assert_eq!(fs::read(&executable).unwrap(), bytes);
    assert_eq!(fs::read_dir(&mods).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&game).unwrap().count(), 0);
    let mut component = Component {
        guid: "cancelled".into(),
        name: "Cancelled".into(),
        ..Default::default()
    };
    assert!(
        archive_generation::generate_from_archive(
            &mut component,
            &executable,
            &archive::Options::default(),
            &AtomicBool::new(true)
        )
        .is_err()
    );
    assert!(component.instructions.is_empty());
    assert!(component.metadata.is_empty());
}

#[test]
fn augmentation_is_idempotent_and_retains_authored_copy_destination() {
    let dir = tempfile::tempdir().unwrap();
    let mods = dir.path().join("mods");
    let game = dir.path().join("game");
    fs::create_dir(&mods).unwrap();
    fs::create_dir(&game).unwrap();
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/archive-generation.json")).unwrap();
    let case = cases.iter().find(|c| c["Name"] == "augment-copy").unwrap();
    let path = mods.join("augment-copy.zip");
    zip(&path, &case["Files"]);
    let mut component: Component = serde_json::from_value(case["Component"].clone()).unwrap();
    component.guid = "augment".into();
    component.name = "Augment".into();
    component.is_selected = true;
    let authored = serde_json::to_value(&component.instructions).unwrap();
    for _ in 0..2 {
        assert!(
            archive_generation::generate_from_archive(
                &mut component,
                &path,
                &archive::Options::default(),
                &AtomicBool::new(false)
            )
            .unwrap()
        );
        assert_eq!(
            serde_json::to_value(&component.instructions).unwrap(),
            authored
        );
    }
    let guide = Guide {
        components: vec![component],
        ..Default::default()
    };
    Engine::new(Settings::new(&mods, &game).unwrap())
        .unwrap()
        .install(&guide, |_| {})
        .unwrap();
    assert_eq!(
        fs::read(game.join("Modules/sample.tga")).unwrap(),
        b"texture"
    );
    assert_eq!(
        fs::read(mods.join("augment-copy/sample.tga")).unwrap(),
        b"texture"
    );
    assert!(!game.join("Override/sample.tga").exists());
}

#[test]
fn legacy_download_links_reach_local_inference_and_shared_install_engine() {
    for source in [
        r#"[{"Guid":"legacy","Name":"Different Name","ModLinkFilenames":{"https://example.com/sample":{}}}]"#,
        r#"[{"Guid":"legacy","Name":"Different Name","ResourceRegistry":{"old-content-hash":{"PrimaryUrl":"https://example.com/sample","ContentKey":"retained"}}}]"#,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("mods");
        let game = dir.path().join("game");
        fs::create_dir(&mods).unwrap();
        fs::create_dir(&game).unwrap();
        zip(
            &mods.join("sample.zip"),
            &json!({"sample.tga":"legacy texture"}),
        );
        let mut guide = Guide::parse(source, "json").unwrap();
        let report = archive_generation::generate_local(&mut guide, &mods).unwrap();
        assert!(report[0].success);
        let engine = Engine::new(Settings::new(&mods, &game).unwrap()).unwrap();
        engine.validate(&guide, |_| {}).unwrap();
        assert!(!game.join("Override").exists());
        Engine::new(Settings::new(&mods, &game).unwrap())
            .unwrap()
            .install(&guide, |_| {})
            .unwrap();
        assert_eq!(
            fs::read(game.join("Override/sample.tga")).unwrap(),
            b"legacy texture"
        );
        assert!(mods.join("sample.zip").exists());
    }
}
