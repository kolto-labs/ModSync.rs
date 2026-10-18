use modsync::{Component, Engine, Guide, Settings, archive, archive_generation, draft};
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

#[test]
fn bracketed_archive_name_installs_literally_and_never_as_a_character_class() {
    let dir = tempfile::tempdir().unwrap();
    let mods = dir.path().join("mods");
    let game = dir.path().join("game");
    fs::create_dir(&mods).unwrap();
    fs::create_dir(&game).unwrap();
    zip(
        &mods.join("[ex]_pack.zip"),
        &json!({"Override/sample.tga":"bracketed"}),
    );
    zip(
        &mods.join("e_pack.zip"),
        &json!({"Override/sample.tga":"decoy"}),
    );
    let mut guide = Guide::parse(
        r#"[{"Guid":"example","Name":"Sample","ResourceRegistry":{"[ex]_pack.zip":{}}}]"#,
        "json",
    )
    .unwrap();
    let reports = archive_generation::generate_local(&mut guide, &mods).unwrap();
    assert!(reports[0].success, "{:?}", reports[0]);
    let extract = &guide.components[0].instructions[0];
    assert_eq!(extract.action, "Extract");
    assert_eq!(extract.source, ["<<modDirectory>>\\[ex]_pack.zip"]);
    let settings = Settings::new(&mods, &game).unwrap();
    Engine::new(settings.clone())
        .unwrap()
        .validate(&guide, |_| {})
        .unwrap();
    Engine::new(settings)
        .unwrap()
        .install(&guide, |_| {})
        .unwrap();
    assert_eq!(
        fs::read(game.join("Override/sample.tga")).unwrap(),
        b"bracketed"
    );
    assert!(!mods.join("e_pack").exists());
}

#[test]
fn bracketed_patcher_folder_generates_a_literal_patcher_source() {
    let dir = tempfile::tempdir().unwrap();
    let mods = dir.path().join("mods");
    fs::create_dir(&mods).unwrap();
    zip(
        &mods.join("pack.zip"),
        &json!({
            "[Example] Texture Pack A/tslpatchdata/changes.ini":"[Settings]",
            "[Example] Texture Pack A/Setup.exe":"x"
        }),
    );
    let mut guide = Guide::parse(
        r#"[{"Guid":"example","Name":"Sample","ResourceRegistry":{"pack.zip":{}}}]"#,
        "json",
    )
    .unwrap();
    let reports = archive_generation::generate_local(&mut guide, &mods).unwrap();
    assert!(reports[0].success, "{:?}", reports[0]);
    let steps = &guide.components[0].instructions;
    assert_eq!(steps[0].action, "Extract");
    assert_eq!(steps[1].action, "Patcher");
    assert_eq!(
        steps[1].source,
        ["<<modDirectory>>\\pack/[Example] Texture Pack A\\Setup.exe"]
    );
}

#[test]
fn unusable_archive_names_skip_only_their_component() {
    let dir = tempfile::tempdir().unwrap();
    let mods = dir.path().join("mods");
    fs::create_dir(&mods).unwrap();
    zip(&mods.join("[a]_pack.zip"), &json!({"Override/one.tga":"1"}));
    zip(
        &mods.join("star*pack.zip"),
        &json!({"Override/two.tga":"2"}),
    );
    zip(
        &mods.join("what?pack.zip"),
        &json!({"Override/three.tga":"3"}),
    );
    zip(&mods.join("plain.zip"), &json!({"Override/four.tga":"4"}));
    zip(
        &mods.join("[b]_patcher.zip"),
        &json!({"tslpatchdata/changes.ini":"[Settings]","Setup.exe":"x"}),
    );
    let mut guide = Guide::parse(
        r#"[
        {"Guid":"a","Name":"One","ResourceRegistry":{"[a]_pack.zip":{}}},
        {"Guid":"b","Name":"Two","ResourceRegistry":{"star*pack.zip":{}}},
        {"Guid":"c","Name":"Three","ResourceRegistry":{"what?pack.zip":{}}},
        {"Guid":"d","Name":"Four","ResourceRegistry":{"plain.zip":{}}},
        {"Guid":"e","Name":"Five","ResourceRegistry":{"[b]_patcher.zip":{}}}
        ]"#,
        "json",
    )
    .unwrap();
    let reports = archive_generation::generate_local(&mut guide, &mods).unwrap();
    assert_eq!(
        reports.iter().map(|r| r.success).collect::<Vec<_>>(),
        [true, false, false, true, true]
    );
    assert!(
        reports[1].skip_reason.contains("wildcard"),
        "{:?}",
        reports[1]
    );
    assert!(
        reports[2].skip_reason.contains("wildcard"),
        "{:?}",
        reports[2]
    );
    assert!(!guide.components[0].instructions.is_empty());
    assert!(guide.components[1].instructions.is_empty());
    assert!(guide.components[2].instructions.is_empty());
    assert!(!guide.components[3].instructions.is_empty());
    assert_eq!(guide.components[4].instructions[1].action, "Patcher");
}

fn archive_map_guide(links: &[&str]) -> Guide {
    let registry: serde_json::Map<String, Value> = links
        .iter()
        .map(|link| ((*link).into(), json!({})))
        .collect();
    Guide::parse(
        &json!([{"Guid":"example","Name":"Texture Pack A","ResourceRegistry":registry}])
            .to_string(),
        "json",
    )
    .unwrap()
}

fn archive_map_from(value: Value) -> archive_generation::ArchiveMap {
    archive_generation::ArchiveMap::parse(&value.to_string()).unwrap()
}

fn generate_mapped(
    guide: &mut Guide,
    dir: &Path,
    map: Option<&archive_generation::ArchiveMap>,
) -> Vec<archive_generation::GenerationResult> {
    archive_generation::generate_local_with_cancel(guide, dir, map, &AtomicBool::new(false))
        .unwrap()
}

#[test]
fn archive_map_exact_hit_overrides_heuristic() {
    let dir = tempfile::tempdir().unwrap();
    zip(
        &dir.path().join("texture-pack-a.zip"),
        &json!({"decoy.tga":"x"}),
    );
    zip(
        &dir.path().join("example-mod.zip"),
        &json!({"mapped.tga":"x"}),
    );
    let link = "https://example.com/files/file/123-texture-pack-a/";
    let map = archive_map_from(json!({ link: "example-mod.zip" }));
    let mut guide = archive_map_guide(&[link]);
    let reports = generate_mapped(&mut guide, dir.path(), Some(&map));
    assert!(reports[0].success, "{:?}", reports[0]);
    let sources: Vec<_> = guide.components[0]
        .instructions
        .iter()
        .flat_map(|step| step.source.clone())
        .collect();
    assert!(sources.contains(&"<<modDirectory>>\\example-mod.zip".to_string()));
    assert!(
        !sources
            .iter()
            .any(|source| source.contains("texture-pack-a"))
    );
}

#[test]
fn archive_map_missing_file_is_skipped_without_heuristic_fallback() {
    let dir = tempfile::tempdir().unwrap();
    zip(
        &dir.path().join("texture-pack-a.zip"),
        &json!({"decoy.tga":"x"}),
    );
    let link = "https://example.com/files/file/123-texture-pack-a/";
    let map = archive_map_from(json!({ link: "example-mod.zip" }));
    let mut guide = archive_map_guide(&[link]);
    let before = serde_json::to_value(&guide).unwrap();
    let reports = generate_mapped(&mut guide, dir.path(), Some(&map));
    assert!(!reports[0].success);
    assert!(
        reports[0].skip_reason.contains("example-mod.zip")
            && reports[0].skip_reason.contains("not found"),
        "{}",
        reports[0].skip_reason
    );
    assert_eq!(serde_json::to_value(&guide).unwrap(), before);
}

#[test]
fn archive_map_unmapped_link_uses_heuristic_and_matches_default_output() {
    let dir = tempfile::tempdir().unwrap();
    zip(
        &dir.path().join("texture-pack-a.zip"),
        &json!({"decoy.tga":"x"}),
    );
    zip(
        &dir.path().join("example-mod.zip"),
        &json!({"mapped.tga":"x"}),
    );
    let link = "https://example.com/files/file/123-texture-pack-a/";
    let map = archive_map_from(json!({ "https://example.com/other/": "example-mod.zip" }));
    let mut mapped = archive_map_guide(&[link]);
    let mut plain = archive_map_guide(&[link]);
    let mapped_reports = generate_mapped(&mut mapped, dir.path(), Some(&map));
    let plain_reports = generate_mapped(&mut plain, dir.path(), None);
    assert!(plain_reports[0].success);
    assert_eq!(
        serde_json::to_value(&mapped_reports).unwrap(),
        serde_json::to_value(&plain_reports).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&mapped).unwrap(),
        serde_json::to_value(&plain).unwrap()
    );
    assert_eq!(
        mapped.components[0].instructions[0].source,
        ["<<modDirectory>>\\texture-pack-a.zip"]
    );
}

#[test]
fn archive_map_canonicalizes_query_and_fragment_on_both_sides() {
    let dir = tempfile::tempdir().unwrap();
    zip(
        &dir.path().join("example-mod.zip"),
        &json!({"mapped.tga":"x"}),
    );
    for (guide_link, map_key) in [
        (
            "https://example.com/files/file/9-unrelated/?do=download&r=77#comments",
            "https://example.com/files/file/9-unrelated/",
        ),
        (
            "https://example.com/files/file/9-unrelated/",
            "https://example.com/files/file/9-unrelated/?do=download#top",
        ),
    ] {
        let map = archive_map_from(json!({ map_key: "example-mod.zip" }));
        let mut guide = archive_map_guide(&[guide_link]);
        let reports = generate_mapped(&mut guide, dir.path(), Some(&map));
        assert!(reports[0].success, "{guide_link}: {:?}", reports[0]);
        assert_eq!(
            guide.components[0].instructions[0].source,
            ["<<modDirectory>>\\example-mod.zip"]
        );
    }
}

#[test]
fn archive_map_multi_file_value_installs_each_archive_in_order() {
    let dir = tempfile::tempdir().unwrap();
    zip(&dir.path().join("part-two.zip"), &json!({"two.tga":"2"}));
    zip(&dir.path().join("part-one.zip"), &json!({"one.tga":"1"}));
    let link = "https://example.com/files/file/5-texture-pack-a/";
    let map = archive_map_from(json!({ link: ["part-one.zip", "part-two.zip"] }));
    let mut guide = archive_map_guide(&[link]);
    let reports = generate_mapped(&mut guide, dir.path(), Some(&map));
    assert!(reports[0].success, "{:?}", reports[0]);
    let extracts: Vec<_> = guide.components[0]
        .instructions
        .iter()
        .filter(|step| step.action == "Extract")
        .flat_map(|step| step.source.clone())
        .collect();
    assert_eq!(
        extracts,
        [
            "<<modDirectory>>\\part-one.zip",
            "<<modDirectory>>\\part-two.zip"
        ]
    );
    let missing = archive_map_from(json!({ link: ["part-one.zip", "absent.zip"] }));
    let mut guide = archive_map_guide(&[link]);
    let reports = generate_mapped(&mut guide, dir.path(), Some(&missing));
    assert!(!reports[0].success);
    assert!(reports[0].skip_reason.contains("absent.zip"));
    assert!(guide.components[0].instructions.is_empty());
}

#[test]
fn archive_map_cli_flag_applies_map_and_rejects_bad_files() {
    let dir = tempfile::tempdir().unwrap();
    zip(
        &dir.path().join("texture-pack-a.zip"),
        &json!({"decoy.tga":"x"}),
    );
    zip(
        &dir.path().join("example-mod.zip"),
        &json!({"mapped.tga":"x"}),
    );
    let link = "https://example.com/files/file/123-texture-pack-a/";
    let input = dir.path().join("input.json");
    let output = dir.path().join("output.json");
    fs::write(
        &input,
        serde_json::to_string(&archive_map_guide(&[link])).unwrap(),
    )
    .unwrap();
    let run = |map: &Path| {
        std::process::Command::new(env!("CARGO_BIN_EXE_modsync"))
            .args(["convert", "--input"])
            .arg(&input)
            .arg("--output")
            .arg(&output)
            .args(["--format", "json", "--auto-generate-local", "--source-path"])
            .arg(dir.path())
            .arg("--archive-map")
            .arg(map)
            .output()
            .unwrap()
    };
    let good = dir.path().join("map.json");
    fs::write(&good, json!({ link: "example-mod.zip" }).to_string()).unwrap();
    let result = run(&good);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let guide = Guide::load(&output).unwrap();
    assert_eq!(
        guide.components[0].instructions[0].source[0].replace('\\', "/"),
        "<<modDirectory>>/example-mod.zip"
    );
    let broken = dir.path().join("broken.json");
    fs::write(&broken, "[1, 2").unwrap();
    for path in [broken, dir.path().join("absent.json")] {
        let result = run(&path);
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("archive map"),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let no_source = std::process::Command::new(env!("CARGO_BIN_EXE_modsync"))
        .args(["convert", "--archive-map"])
        .arg(&good)
        .output()
        .unwrap();
    assert!(!no_source.status.success());
    assert!(String::from_utf8_lossy(&no_source.stderr).contains("--source-path"));
}

fn zip_bytes(path: &Path, files: &[(&str, &[u8])]) {
    let mut archive = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, content) in files {
        archive
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(content).unwrap();
    }
    archive.finish().unwrap();
}

fn namespace_option_names(ini: &[u8]) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    zip_bytes(
        &dir.path().join("texture-pack-a.zip"),
        &[
            ("tslpatchdata/namespaces.ini", ini),
            ("tslpatchdata/first/changes.ini", b"[Settings]"),
            ("Setup.exe", b"x"),
        ],
    );
    let mut guide = archive_map_guide(&["https://example.com/files/file/1-texture-pack-a/"]);
    let reports = generate_mapped(&mut guide, dir.path(), None);
    assert!(reports[0].success, "{:?}", reports[0]);
    guide.components[0]
        .options
        .iter()
        .map(|option| option.name.clone())
        .collect()
}

#[test]
fn namespaces_ini_accepts_ansi_utf8_and_bom_text() {
    let ansi =
        b"[Namespaces]\r\n0=first\r\n[first]\r\nName=Pack\x92s Option\r\nDescription=Caf\xE9\r\n";
    let names = namespace_option_names(ansi);
    assert!(
        names.iter().any(|name| name == "Pack\u{2019}s Option"),
        "{names:?}"
    );

    let utf8 = "[Namespaces]\n0=first\n[first]\nName=Pack\u{2019}s Option\n".as_bytes();
    let names = namespace_option_names(utf8);
    assert!(
        names.iter().any(|name| name == "Pack\u{2019}s Option"),
        "{names:?}"
    );

    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(utf8);
    let names = namespace_option_names(&bom);
    assert!(
        names.iter().any(|name| name == "Pack\u{2019}s Option"),
        "{names:?}"
    );
}

#[test]
fn failing_mapped_archive_skip_reason_names_the_archive() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("example-mod.zip"), b"not a real archive").unwrap();
    let link = "https://example.com/files/file/123-texture-pack-a/";
    let map = archive_map_from(json!({ link: "example-mod.zip" }));
    let mut guide = archive_map_guide(&[link]);
    let reports = generate_mapped(&mut guide, dir.path(), Some(&map));
    assert!(!reports[0].success);
    assert!(
        reports[0].skip_reason.contains("example-mod.zip"),
        "{}",
        reports[0].skip_reason
    );
}

const EXAMPLE_GUID: &str = "00000000-0000-4000-8000-0000000000aa";

fn example_guide() -> Guide {
    Guide::parse(
        &json!([{"Guid":EXAMPLE_GUID,"Name":"Texture Pack A"}]).to_string(),
        "json",
    )
    .unwrap()
}

fn resolve_into(guide: &mut Guide, dir: &Path, groups: &[&[&str]]) {
    let resolved: Vec<_> = groups
        .iter()
        .map(|names| archive_generation::resolved::ResolvedFiles {
            component_guid: EXAMPLE_GUID.into(),
            filenames: names.iter().map(|name| (*name).into()).collect(),
            sha256: None,
            size_bytes: None,
        })
        .collect();
    archive_generation::resolved::generate(
        guide,
        &resolved,
        Some(dir),
        &archive::Options::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
}

fn verify(component: &mut Component, dir: &Path) -> anyhow::Result<()> {
    draft::review::verify_archive_derived(
        component,
        dir,
        &archive::Options::default(),
        &AtomicBool::new(false),
    )
}

fn review_record(component: &Component) -> &Value {
    &component.metadata["DraftReview"]
}

fn validation(dir: &Path, guide: &Guide) -> anyhow::Result<()> {
    let game = tempfile::tempdir().unwrap();
    Engine::new(Settings::new(dir, game.path()).unwrap())
        .unwrap()
        .validate(guide, |_| {})
}

fn tslpatchdata_zip(dir: &Path) {
    zip_bytes(
        &dir.join("texture-pack-a.zip"),
        &[
            ("tslpatchdata/changes.ini", b"[Settings]"),
            ("tslpatchdata/sample.tga", b"texture"),
            ("Setup.exe", b"x"),
        ],
    );
}

fn namespaces_zip(dir: &Path) {
    zip_bytes(
        &dir.join("texture-pack-a.zip"),
        &[
            (
                "tslpatchdata/namespaces.ini",
                b"[Namespaces]\r\n0=first\r\n1=second\r\n[first]\r\nName=Option One\r\n[second]\r\nName=Option Two\r\n",
            ),
            ("tslpatchdata/first/changes.ini", b"[Settings]"),
            ("tslpatchdata/second/changes.ini", b"[Settings]"),
            ("Setup.exe", b"x"),
        ],
    );
}

fn drafted(dir: &Path, build: fn(&Path)) -> Guide {
    build(dir);
    let mut guide = example_guide();
    resolve_into(&mut guide, dir, &[&["texture-pack-a.zip"]]);
    assert!(draft::requires_review(&guide.components[0]));
    guide
}

#[test]
fn archive_replay_clears_tslpatchdata_zip_and_validation_agrees() {
    let dir = tempfile::tempdir().unwrap();
    let mut guide = drafted(dir.path(), tslpatchdata_zip);
    assert!(
        format!("{:#}", validation(dir.path(), &guide).unwrap_err()).contains("unreviewed draft")
    );
    assert!(review_record(&guide.components[0])["Derivation"]["Sha256"].is_string());
    verify(&mut guide.components[0], dir.path()).unwrap();
    assert!(!draft::requires_review(&guide.components[0]));
    let verification = &review_record(&guide.components[0])["Verification"];
    assert_eq!(verification["Reviewer"], "modsync-ci-tool");
    assert_eq!(verification["Kind"], "ArchiveDerived");
    assert_eq!(verification["ArchiveFilename"], "texture-pack-a.zip");
    assert_eq!(
        verification["ArchiveSha256"],
        review_record(&guide.components[0])["Derivation"]["Sha256"]
    );
    validation(dir.path(), &guide).unwrap();
}

#[test]
fn archive_replay_clears_namespaces_zip_with_its_option_subtree() {
    let dir = tempfile::tempdir().unwrap();
    let mut guide = drafted(dir.path(), namespaces_zip);
    assert_eq!(guide.components[0].options.len(), 2);
    assert!(
        guide.components[0]
            .options
            .iter()
            .all(draft::requires_review)
    );
    assert!(validation(dir.path(), &guide).is_err());
    verify(&mut guide.components[0], dir.path()).unwrap();
    assert!(!draft::requires_review(&guide.components[0]));
    assert!(
        guide.components[0]
            .options
            .iter()
            .all(|option| !draft::requires_review(option))
    );
    validation(dir.path(), &guide).unwrap();
}

#[test]
fn archive_replay_refuses_without_a_matching_derived_archive() {
    let dir = tempfile::tempdir().unwrap();

    let mut absent = example_guide();
    resolve_into(&mut absent, dir.path(), &[&["missing-pack.zip"]]);
    assert!(
        review_record(&absent.components[0])
            .get("Derivation")
            .is_none()
    );
    let before = serde_json::to_value(&absent.components[0]).unwrap();
    assert!(verify(&mut absent.components[0], dir.path()).is_err());
    assert_eq!(serde_json::to_value(&absent.components[0]).unwrap(), before);

    let mut bare = example_guide();
    resolve_into(&mut bare, dir.path(), &[&["N_Example01.tga"]]);
    assert!(
        review_record(&bare.components[0])
            .get("Derivation")
            .is_none()
    );
    assert!(verify(&mut bare.components[0], dir.path()).is_err());
    assert!(draft::requires_review(&bare.components[0]));

    let mut multi = drafted(dir.path(), tslpatchdata_zip);
    zip_bytes(
        &dir.path().join("texture-pack-b.zip"),
        &[("sample.tga", b"b")],
    );
    let mut grouped = example_guide();
    resolve_into(
        &mut grouped,
        dir.path(),
        &[&["texture-pack-a.zip", "texture-pack-b.zip"]],
    );
    assert!(
        review_record(&grouped.components[0])
            .get("Derivation")
            .is_none()
    );
    assert!(verify(&mut grouped.components[0], dir.path()).is_err());
    let mut separate = example_guide();
    resolve_into(
        &mut separate,
        dir.path(),
        &[&["texture-pack-a.zip"], &["texture-pack-b.zip"]],
    );
    assert!(verify(&mut separate.components[0], dir.path()).is_err());
    assert!(draft::requires_review(&separate.components[0]));
    verify(&mut multi.components[0], dir.path()).unwrap();
}

#[test]
fn archive_replay_refuses_hash_drift_and_edited_drafts() {
    let dir = tempfile::tempdir().unwrap();
    let mut guide = drafted(dir.path(), tslpatchdata_zip);
    let pristine = guide.components[0].clone();

    let mut appended = pristine.clone();
    appended.instructions.push(appended.instructions[0].clone());
    appended.instructions.last_mut().unwrap().guid = "extra".into();
    appended.instructions.last_mut().unwrap().source = vec!["<<modDirectory>>\\extra.tga".into()];
    appended.instructions.last_mut().unwrap().action = "Move".into();
    appended.instructions.last_mut().unwrap().destination = "<<kotorDirectory>>\\Override".into();
    assert!(verify(&mut appended, dir.path()).is_err());
    assert!(draft::requires_review(&appended));
    let count = appended.instructions.len();
    appended.metadata.get_mut("DraftReview").unwrap()["Result"]["DraftInstructionCount"] =
        json!(count);
    let error = verify(&mut appended, dir.path()).unwrap_err();
    assert!(
        format!("{error:#}").contains("differ from the archive replay"),
        "{error:#}"
    );
    assert!(draft::requires_review(&appended));

    let mut gapful = pristine.clone();
    gapful.metadata.get_mut("DraftReview").unwrap()["Result"]["UnparsedGaps"] =
        json!(["Unsupported resolved filename: notes.xyz"]);
    let error = verify(&mut gapful, dir.path()).unwrap_err();
    assert!(
        format!("{error:#}").contains("unresolved gaps"),
        "{error:#}"
    );
    assert!(draft::requires_review(&gapful));

    let mut masters = pristine.clone();
    masters
        .metadata
        .insert("UnresolvedMasters".into(), json!(["Texture Pack B"]));
    let error = verify(&mut masters, dir.path()).unwrap_err();
    assert!(format!("{error:#}").contains("masters"), "{error:#}");

    zip_bytes(
        &dir.path().join("texture-pack-a.zip"),
        &[
            ("tslpatchdata/changes.ini", b"[Settings]"),
            ("Setup.exe", b"y"),
        ],
    );
    let error = verify(&mut guide.components[0], dir.path()).unwrap_err();
    assert!(format!("{error:#}").contains("SHA-256"), "{error:#}");
    assert!(draft::requires_review(&guide.components[0]));
    fs::remove_file(dir.path().join("texture-pack-a.zip")).unwrap();
    assert!(verify(&mut guide.components[0], dir.path()).is_err());
}

#[test]
fn archive_replay_never_clears_prose_or_method_drafts() {
    let dir = tempfile::tempdir().unwrap();
    let mut prose = Guide::parse(
        &json!([{"Guid":EXAMPLE_GUID,"Name":"Texture Pack A","Directions":"Move only sample.tga to your Override."}]).to_string(),
        "json",
    )
    .unwrap();
    draft::generate(&mut prose);
    assert!(draft::requires_review(&prose.components[0]));
    assert!(verify(&mut prose.components[0], dir.path()).is_err());
    assert!(draft::requires_review(&prose.components[0]));

    let mut installer = example_guide();
    fs::write(dir.path().join("setup-pack.exe"), b"not an archive").unwrap();
    resolve_into(&mut installer, dir.path(), &[&["setup-pack.exe"]]);
    assert_eq!(installer.components[0].instructions[0].action, "Execute");
    assert!(
        review_record(&installer.components[0])
            .get("Derivation")
            .is_some()
    );
    let error = verify(&mut installer.components[0], dir.path()).unwrap_err();
    assert!(
        format!("{error:#}").contains("external installer"),
        "{error:#}"
    );
    assert!(draft::requires_review(&installer.components[0]));

    let mut forged = prose.components[0].clone();
    let record = forged.metadata.get_mut("DraftReview").unwrap();
    record["Generator"] = "resolved-filenames".into();
    record["Derivation"] =
        json!({"Filename":"texture-pack-a.zip","Sha256":"0".repeat(64),"Bytes":1});
    assert!(verify(&mut forged, dir.path()).is_err());
    assert!(draft::requires_review(&forged));
}

const OVERRIDE_DEFAULT: &str = "If you are asked to direct a TSLPatcher or HoloPatcher executable to a folder to begin install, that folder should always be the main game folder. For mods that are not installed with the TSLPatcher or HoloPatcher, unless otherwise noted their loose-file contents are placed directly in the Override directory.";

fn method_guide(method: &str, preamble: bool) -> Guide {
    let mut guide = Guide::parse(
        &json!([{"Guid":EXAMPLE_GUID,"Name":"Texture Pack A","InstallationMethod":method}])
            .to_string(),
        "json",
    )
    .unwrap();
    if preamble {
        guide
            .metadata
            .insert("preambleContent".into(), json!(OVERRIDE_DEFAULT));
    }
    guide
}

fn loose_zip(dir: &Path) {
    zip_bytes(
        &dir.join("texture-pack-a.zip"),
        &[("sample.tga", b"texture"), ("readme.txt", b"notes")],
    );
}

fn bound_method_guide(dir: &Path) -> Guide {
    loose_zip(dir);
    let mut guide = method_guide("Loose-File", true);
    draft::generate(&mut guide);
    resolve_into(&mut guide, dir, &[&["texture-pack-a.zip"]]);
    let record = review_record(&guide.components[0]);
    assert_eq!(
        record["MethodContext"]["ArchiveBinding"], "Bound",
        "{record}"
    );
    assert_eq!(record["Derivation"]["Filename"], "texture-pack-a.zip");
    assert!(draft::requires_review(&guide.components[0]));
    guide
}

#[test]
fn archive_replay_clears_a_draft_that_method_binding_produced() {
    let dir = tempfile::tempdir().unwrap();
    let mut guide = bound_method_guide(dir.path());
    assert_eq!(guide.components[0].instructions.len(), 2);
    verify(&mut guide.components[0], dir.path()).unwrap();
    let cleared = guide.components[0].clone();
    assert!(!draft::requires_review(&cleared));
    assert_eq!(
        review_record(&cleared)["Verification"]["Kind"],
        "ArchiveDerived"
    );
    assert_eq!(
        review_record(&cleared)["Verification"]["ArchiveFilename"],
        "texture-pack-a.zip"
    );

    let mut edited = cleared.clone();
    edited.instructions[1].destination = "<<kotorDirectory>>\\Elsewhere".into();
    assert!(draft::requires_review(&edited));

    let reloaded = Guide::parse(&guide.render("toml").unwrap(), "toml").unwrap();
    assert!(!draft::requires_review(&reloaded.components[0]));
}

#[test]
fn bound_method_drafts_survive_a_toml_round_trip_before_verification() {
    let dir = tempfile::tempdir().unwrap();
    let guide = bound_method_guide(dir.path());
    let mut reloaded = Guide::parse(&guide.render("toml").unwrap(), "toml").unwrap();
    assert!(draft::requires_review(&reloaded.components[0]));
    verify(&mut reloaded.components[0], dir.path()).unwrap();
    assert!(!draft::requires_review(&reloaded.components[0]));
}

#[test]
fn toml_input_keeps_the_preamble_default_for_method_binding() {
    let dir = tempfile::tempdir().unwrap();
    loose_zip(dir.path());
    let source = method_guide("Loose-File", true);
    let mut guide = Guide::parse(&source.render("toml").unwrap(), "toml").unwrap();
    draft::generate(&mut guide);
    resolve_into(&mut guide, dir.path(), &[&["texture-pack-a.zip"]]);
    assert_eq!(guide.components[0].instructions[1].action, "Move");
    verify(&mut guide.components[0], dir.path()).unwrap();

    let plain = method_guide("Loose-File", false);
    let mut guide = Guide::parse(&plain.render("toml").unwrap(), "toml").unwrap();
    resolve_into(&mut guide, dir.path(), &[&["texture-pack-a.zip"]]);
    assert!(guide.components[0].instructions.is_empty());
}

#[test]
fn edited_bound_method_drafts_stay_fenced() {
    let dir = tempfile::tempdir().unwrap();
    let guide = bound_method_guide(dir.path());
    let fenced = |edit: &dyn Fn(&mut Component)| {
        let mut component = guide.components[0].clone();
        edit(&mut component);
        assert!(verify(&mut component, dir.path()).is_err());
        assert!(draft::requires_review(&component));
    };
    fenced(&|c| c.instructions[1].destination = "<<kotorDirectory>>\\Elsewhere".into());
    fenced(&|c| {
        c.instructions[1]
            .source
            .push("<<modDirectory>>\\extra.tga".into())
    });
    fenced(&|c| c.instructions[0].source = vec!["<<modDirectory>>\\other.zip".into()]);
    fenced(&|c| {
        c.instructions.pop();
    });
    fenced(&|c| c.options.push(c.clone()));
    fenced(&|c| {
        c.metadata
            .insert("InstallationMethod".into(), json!("TSLPatcher"));
    });
    fenced(&|c| {
        c.metadata.get_mut("ArchiveMethodBinding").unwrap()["Members"] = json!(["forged.tga"]);
    });
    fenced(&|c| {
        c.metadata.get_mut("ArchiveMethodBinding").unwrap()["GuideDefaults"]["LooseFilesDestination"] =
            json!("<<kotorDirectory>>\\Modules");
    });
    fenced(&|c| {
        c.metadata.get_mut("DraftReview").unwrap()["MethodContext"]["ArchiveBinding"] =
            json!("Unresolved");
    });
    fenced(&|c| {
        c.metadata.remove("ArchiveMethodBinding");
    });
    fenced(&|c| {
        c.metadata
            .get_mut("DraftReview")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("Derivation");
    });
    fenced(&|c| {
        c.metadata.get_mut("DraftReview").unwrap()["InferenceHistory"] = json!([]);
    });
    fenced(&|c| {
        c.metadata.get_mut("DraftReview").unwrap()["Result"]["UnparsedGaps"] = json!(["open"]);
    });
}

#[test]
fn bound_method_replay_refuses_prose_and_non_loose_file_methods() {
    let dir = tempfile::tempdir().unwrap();
    let guide = bound_method_guide(dir.path());

    let mut prose = guide.components[0].clone();
    prose.directions = "Move only sample.tga to your Override.".into();
    assert!(verify(&mut prose, dir.path()).is_err());
    assert!(draft::requires_review(&prose));

    let mut warned = guide.components[0].clone();
    warned
        .metadata
        .insert("InstallationWarning".into(), json!("Extra guidance"));
    assert!(verify(&mut warned, dir.path()).is_err());

    let mut patcher = method_guide("TSLPatcher", true);
    draft::generate(&mut patcher);
    resolve_into(&mut patcher, dir.path(), &[&["texture-pack-a.zip"]]);
    assert!(patcher.components[0].instructions.is_empty());
    assert!(verify(&mut patcher.components[0], dir.path()).is_err());
    assert!(draft::requires_review(&patcher.components[0]));

    let mut unbound = method_guide("Loose-File", true);
    draft::generate(&mut unbound);
    assert!(verify(&mut unbound.components[0], dir.path()).is_err());
    assert!(draft::requires_review(&unbound.components[0]));
}

#[test]
fn tampered_or_missing_archives_keep_bound_method_drafts_fenced() {
    let dir = tempfile::tempdir().unwrap();
    let guide = bound_method_guide(dir.path());

    let mut swapped = guide.components[0].clone();
    zip_bytes(
        &dir.path().join("texture-pack-a.zip"),
        &[("sample.tga", b"changed"), ("readme.txt", b"notes")],
    );
    let error = verify(&mut swapped, dir.path()).unwrap_err();
    assert!(format!("{error:#}").contains("no longer"), "{error:#}");
    assert!(draft::requires_review(&swapped));

    let mut extra = guide.components[0].clone();
    zip_bytes(
        &dir.path().join("texture-pack-a.zip"),
        &[
            ("sample.tga", b"texture"),
            ("readme.txt", b"notes"),
            ("second.tga", b"more"),
        ],
    );
    assert!(verify(&mut extra, dir.path()).is_err());
    assert!(draft::requires_review(&extra));

    let mut missing = guide.components[0].clone();
    let empty = tempfile::tempdir().unwrap();
    assert!(verify(&mut missing, empty.path()).is_err());
    assert!(draft::requires_review(&missing));
}

const PATCHER_FILES: [(&str, &[u8]); 4] = [
    ("tslpatchdata/changes.ini", b"[Settings]"),
    ("tslpatchdata/sample.tga", b"texture"),
    ("Setup.exe", b"x"),
    ("readme.txt", b"notes"),
];

fn patcher_bound_guide(dir: &Path, method: &str, files: &[(&str, &[u8])]) -> Guide {
    zip_bytes(&dir.join("texture-pack-a.zip"), files);
    let mut guide = method_guide(method, true);
    draft::generate(&mut guide);
    resolve_into(&mut guide, dir, &[&["texture-pack-a.zip"]]);
    guide
}

fn assert_patcher_fenced(guide: &Guide, dir: &Path) {
    let mut component = guide.components[0].clone();
    assert!(component.instructions.is_empty(), "{component:?}");
    assert_eq!(
        review_record(&component)["MethodContext"]["ArchiveBinding"],
        "Unresolved"
    );
    assert!(draft::requires_review(&component));
    assert!(verify(&mut component, dir).is_err());
    assert!(draft::requires_review(&component));
}

#[test]
fn patcher_method_binding_emits_extract_and_patcher_and_verifies() {
    for method in ["TSLPatcher", "HoloPatcher Mod"] {
        let dir = tempfile::tempdir().unwrap();
        let mut guide = patcher_bound_guide(dir.path(), method, &PATCHER_FILES);
        let component = &guide.components[0];
        let record = review_record(component);
        assert_eq!(record["MethodContext"]["Kind"], "Patcher", "{record}");
        assert_eq!(record["MethodContext"]["ArchiveBinding"], "Bound");
        assert_eq!(record["Derivation"]["Filename"], "texture-pack-a.zip");
        assert_eq!(component.metadata["InstallationMethod"], method);
        assert_eq!(component.instructions.len(), 2);
        assert_eq!(component.instructions[0].action, "Extract");
        assert_eq!(
            component.instructions[0].source,
            ["<<modDirectory>>\\texture-pack-a.zip"]
        );
        assert_eq!(component.instructions[1].action, "Patcher");
        assert_eq!(
            component.instructions[1].source,
            ["<<modDirectory>>\\texture-pack-a\\Setup.exe"]
        );
        assert_eq!(component.instructions[1].destination, "<<gameDirectory>>");
        assert!(component.options.is_empty());
        assert!(draft::requires_review(component));

        verify(&mut guide.components[0], dir.path()).unwrap();
        let cleared = guide.components[0].clone();
        assert!(!draft::requires_review(&cleared));
        assert_eq!(
            review_record(&cleared)["Verification"]["Kind"],
            "ArchiveDerived"
        );
        let mut edited = cleared.clone();
        edited.instructions[1].destination = "<<kotorDirectory>>\\Elsewhere".into();
        assert!(draft::requires_review(&edited));
    }
}

#[test]
fn patcher_method_binding_stays_fenced_outside_its_strict_gates() {
    let namespaces: &[(&str, &[u8])] = &[
        (
            "tslpatchdata/namespaces.ini",
            b"[Namespaces]\r\n0=first\r\n[first]\r\nName=One\r\n",
        ),
        ("tslpatchdata/first/changes.ini", b"[Settings]"),
        ("Setup.exe", b"x"),
    ];
    let namespaces_beside_changes: &[(&str, &[u8])] = &[
        ("tslpatchdata/namespaces.ini", b"[Namespaces]"),
        ("tslpatchdata/changes.ini", b"[Settings]"),
        ("Setup.exe", b"x"),
    ];
    let extra_payload: &[(&str, &[u8])] = &[
        ("tslpatchdata/changes.ini", b"[Settings]"),
        ("Setup.exe", b"x"),
        ("extra.tga", b"texture"),
    ];
    let two_exes: &[(&str, &[u8])] = &[
        ("tslpatchdata/changes.ini", b"[Settings]"),
        ("Setup.exe", b"x"),
        ("Setup64.exe", b"x"),
    ];
    let wrapped: &[(&str, &[u8])] = &[
        ("wrapper/tslpatchdata/changes.ini", b"[Settings]"),
        ("wrapper/Setup.exe", b"x"),
    ];
    let no_exe: &[(&str, &[u8])] = &[("tslpatchdata/changes.ini", b"[Settings]")];
    let two_changes: &[(&str, &[u8])] = &[
        ("tslpatchdata/changes.ini", b"[Settings]"),
        ("tslpatchdata/second/changes.ini", b"[Settings]"),
        ("Setup.exe", b"x"),
    ];
    for files in [
        namespaces,
        namespaces_beside_changes,
        extra_payload,
        two_exes,
        wrapped,
        no_exe,
        two_changes,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let guide = patcher_bound_guide(dir.path(), "TSLPatcher", files);
        assert_patcher_fenced(&guide, dir.path());
    }

    let dir = tempfile::tempdir().unwrap();
    zip_bytes(&dir.path().join("texture-pack-a.zip"), &PATCHER_FILES);
    let mut no_preamble = method_guide("TSLPatcher", false);
    draft::generate(&mut no_preamble);
    resolve_into(&mut no_preamble, dir.path(), &[&["texture-pack-a.zip"]]);
    assert_patcher_fenced(&no_preamble, dir.path());

    let mut directions = method_guide("TSLPatcher", true);
    draft::generate(&mut directions);
    directions.components[0].directions = "Run the installer twice.".into();
    resolve_into(&mut directions, dir.path(), &[&["texture-pack-a.zip"]]);
    assert!(directions.components[0].instructions.is_empty());
    assert!(draft::requires_review(&directions.components[0]));
    assert!(verify(&mut directions.components[0], dir.path()).is_err());

    let mut warned = method_guide("TSLPatcher", true);
    draft::generate(&mut warned);
    warned.components[0].metadata.insert(
        "DownloadInstructions".into(),
        json!("Pick the second file."),
    );
    resolve_into(&mut warned, dir.path(), &[&["texture-pack-a.zip"]]);
    assert!(warned.components[0].instructions.is_empty());
    assert!(draft::requires_review(&warned.components[0]));
}

#[test]
fn patcher_method_replay_rejects_mismatch_and_drift() {
    let dir = tempfile::tempdir().unwrap();
    let guide = patcher_bound_guide(dir.path(), "TSLPatcher", &PATCHER_FILES);
    assert_eq!(guide.components[0].instructions.len(), 2);

    let mut edited = guide.components[0].clone();
    edited.instructions[1].source = vec!["<<modDirectory>>\\texture-pack-a\\Other.exe".into()];
    let error = verify(&mut edited, dir.path()).unwrap_err();
    assert!(format!("{error:#}").contains("differ"), "{error:#}");
    assert!(draft::requires_review(&edited));

    let mut kind = guide.components[0].clone();
    kind.metadata.get_mut("DraftReview").unwrap()["MethodContext"]["Kind"] = "LooseFile".into();
    assert!(verify(&mut kind, dir.path()).is_err());
    assert!(draft::requires_review(&kind));

    let mut forged = guide.components[0].clone();
    forged.metadata.get_mut("ArchiveMethodBinding").unwrap()["Installer"] = "Other.exe".into();
    assert!(verify(&mut forged, dir.path()).is_err());
    assert!(draft::requires_review(&forged));

    let mut warned = guide.components[0].clone();
    warned
        .metadata
        .insert("InstallationWarning".into(), json!("Extra guidance"));
    assert!(verify(&mut warned, dir.path()).is_err());

    let mut drifted = guide.components[0].clone();
    zip_bytes(
        &dir.path().join("texture-pack-a.zip"),
        &[
            ("tslpatchdata/changes.ini", b"[Settings]\r\nchanged=1"),
            ("Setup.exe", b"x"),
        ],
    );
    let error = verify(&mut drifted, dir.path()).unwrap_err();
    assert!(format!("{error:#}").contains("no longer"), "{error:#}");
    assert!(draft::requires_review(&drifted));

    let mut extra = guide.components[0].clone();
    zip_bytes(
        &dir.path().join("texture-pack-a.zip"),
        &[
            ("tslpatchdata/changes.ini", b"[Settings]"),
            ("Setup.exe", b"x"),
            ("Setup64.exe", b"x"),
        ],
    );
    assert!(verify(&mut extra, dir.path()).is_err());
    assert!(draft::requires_review(&extra));
}

#[test]
fn archive_replay_receipt_is_bound_to_instructions_and_verification() {
    let dir = tempfile::tempdir().unwrap();
    let mut guide = drafted(dir.path(), namespaces_zip);
    verify(&mut guide.components[0], dir.path()).unwrap();
    let verified = guide.components[0].clone();
    assert!(!draft::requires_review(&verified));

    let mut edited = verified.clone();
    edited.instructions[0].destination = "<<kotorDirectory>>\\Elsewhere".into();
    assert!(draft::requires_review(&edited));

    let mut option_edit = verified.clone();
    option_edit.options[0].instructions[0].arguments = "--changed".into();
    assert!(draft::requires_review(&option_edit));

    let mut forged = verified.clone();
    forged.metadata.get_mut("DraftReview").unwrap()["Verification"]["ArchiveSha256"] =
        "f".repeat(64).into();
    assert!(draft::requires_review(&forged));

    let mut unkinded = verified.clone();
    unkinded.metadata.get_mut("DraftReview").unwrap()["Verification"]["Kind"] = "Manual".into();
    assert!(draft::requires_review(&unkinded));

    let mut stripped = verified.clone();
    stripped
        .metadata
        .get_mut("DraftReview")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("Verification");
    assert!(draft::requires_review(&stripped));

    assert!(verify(&mut guide.components[0], dir.path()).is_err());
}

#[test]
fn archive_replay_verification_round_trips_in_every_format() {
    let dir = tempfile::tempdir().unwrap();
    let mut guide = drafted(dir.path(), namespaces_zip);
    verify(&mut guide.components[0], dir.path()).unwrap();
    let expected = serde_json::to_value(&guide.components[0].metadata["DraftReview"]).unwrap();
    for format in ["json", "toml", "yaml", "xml"] {
        let text = guide.render(format).unwrap();
        let reloaded = Guide::parse(&text, format).unwrap_or_else(|error| {
            panic!("{format}: {error:#}\n{text}");
        });
        let component = &reloaded.components[0];
        assert!(!draft::requires_review(component), "{format}");
        assert!(
            component.options.iter().all(|o| !draft::requires_review(o)),
            "{format}"
        );
        assert_eq!(
            component.metadata["DraftReview"]["Verification"], expected["Verification"],
            "{format}"
        );
        assert_eq!(
            component.metadata["DraftReview"]["Derivation"]["Bytes"],
            expected["Derivation"]["Bytes"],
            "{format}"
        );
    }
    let mut pending = drafted(dir.path(), namespaces_zip);
    for format in ["toml", "yaml", "xml"] {
        let reloaded = Guide::parse(&pending.render(format).unwrap(), format).unwrap();
        assert!(draft::requires_review(&reloaded.components[0]), "{format}");
        let mut component = reloaded.components[0].clone();
        verify(&mut component, dir.path()).unwrap();
        assert!(!draft::requires_review(&component), "{format}");
    }
    pending.components[0]
        .metadata
        .get_mut("DraftReview")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert("derivation".into(), json!({}));
    let text = serde_json::to_string(&pending).unwrap();
    assert!(Guide::parse(&text, "json").is_err());
}

#[test]
fn convert_review_verified_drafts_clears_only_replayable_components() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("mods");
    fs::create_dir(&source).unwrap();
    namespaces_zip(&source);
    let input = dir.path().join("guide.json");
    fs::write(
        &input,
        json!([
            {"Guid":EXAMPLE_GUID,"Name":"Texture Pack A"},
            {"Guid":"prose","Name":"Texture Pack B","Directions":"Move only sample.tga to your Override."}
        ])
        .to_string(),
    )
    .unwrap();
    let bindings = dir.path().join("resolved.json");
    fs::write(
        &bindings,
        json!([{"componentGuid":EXAMPLE_GUID,"filenames":["texture-pack-a.zip"]}]).to_string(),
    )
    .unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_modsync"))
            .arg("convert")
            .args(args)
            .output()
            .unwrap()
    };
    let drafted = dir.path().join("drafted.json");
    let result = run(&[
        input.to_str().unwrap(),
        drafted.to_str().unwrap(),
        "--parse-directions",
        "--format",
        "json",
        "--source-path",
        source.to_str().unwrap(),
        "--resolved-archives",
        bindings.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let missing_source = run(&[
        drafted.to_str().unwrap(),
        dir.path().join("x.json").to_str().unwrap(),
        "--review-verified-drafts",
    ]);
    assert!(!missing_source.status.success());

    let reviewed = dir.path().join("reviewed.toml");
    let result = run(&[
        drafted.to_str().unwrap(),
        reviewed.to_str().unwrap(),
        "--source-path",
        source.to_str().unwrap(),
        "--review-verified-drafts",
    ]);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(result.status.success(), "{stderr}");
    let report: Value =
        serde_json::from_str(stderr.trim()).unwrap_or_else(|error| panic!("{error}: {stderr}"));
    assert_eq!(report["cleared"], json!([EXAMPLE_GUID]));
    assert_eq!(report["skipped"][0]["guid"], "prose");
    assert_eq!(report["skipped"][0]["name"], "Texture Pack B");
    assert!(report["skipped"][0]["reason"].is_string());
    let guide = Guide::load(&reviewed).unwrap();
    assert!(!draft::requires_review(&guide.components[0]));
    assert!(draft::requires_review(&guide.components[1]));
    assert!(
        guide.components[0]
            .options
            .iter()
            .all(|o| !draft::requires_review(o))
    );

    let manual = dir.path().join("manual.json");
    let result = run(&[
        drafted.to_str().unwrap(),
        manual.to_str().unwrap(),
        "--review-draft",
        "prose",
        "--format",
        "json",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let guide = Guide::load(&manual).unwrap();
    assert!(draft::requires_review(&guide.components[0]));
    assert!(!draft::requires_review(&guide.components[1]));
}
