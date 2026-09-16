use modsync::{Engine, Guide, Settings, draft};
use serde_json::{Value, json};
use std::fs;

#[test]
fn drafts_match_reference_service_and_preserve_authored_instructions() {
    let mut guide = Guide::parse(include_str!("fixtures/drafts.json"), "json").unwrap();
    let expected: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/csharp/drafts.json")).unwrap();
    let reports = draft::generate(&mut guide);
    assert_eq!(guide.components.len(), expected.len());
    assert_eq!(reports.len(), expected.len());
    for ((component, report), reference) in guide.components.iter().zip(&reports).zip(expected) {
        assert_eq!(
            serde_json::to_value(&component.instructions).unwrap(),
            reference["Instructions"],
            "{} instructions",
            component.name
        );
        let options: Vec<_> = component
            .options
            .iter()
            .map(|o| json!({"Name":o.name,"Description":o.description,"IsSelected":o.is_selected}))
            .collect();
        assert_eq!(
            json!(options),
            reference["Options"],
            "{} options",
            component.name
        );
        assert_eq!(
            component
                .metadata
                .get("InstallationWarning")
                .and_then(Value::as_str)
                .unwrap_or("")
                .replace("\r\n", "\n"),
            reference["InstallationWarning"]
                .as_str()
                .unwrap()
                .replace("\r\n", "\n")
        );
        let report = serde_json::to_value(report).unwrap();
        for key in ["DraftInstructionCount", "UnparsedGaps", "ConditionalDrafts"] {
            assert_eq!(
                report[key], reference["Result"][key],
                "{} {key}",
                component.name
            );
        }
        if !component.instructions.is_empty() {
            assert!(draft::requires_review(component));
        }
    }
    let instructions: Vec<_> = guide
        .components
        .iter()
        .map(|c| serde_json::to_value(&c.instructions).unwrap())
        .collect();
    draft::generate(&mut guide);
    assert_eq!(
        instructions,
        guide
            .components
            .iter()
            .map(|c| serde_json::to_value(&c.instructions).unwrap())
            .collect::<Vec<_>>()
    );
}

#[test]
fn reviewed_drafts_share_the_engine_and_unsafe_or_incomplete_drafts_stay_blocked() {
    let mut guide = Guide::parse(&json!([{"Guid":"test","Name":"Test","Directions":"Move only sample.tga to your Override."}]).to_string(), "json").unwrap();
    let result = draft::generate(&mut guide);
    assert_eq!(result[0].draft_instruction_count, 1);
    assert_eq!(
        guide.components[0].instructions[0].source,
        ["<<modDirectory>>\\sample.tga"]
    );
    let dir = tempfile::tempdir().unwrap();
    let mods = dir.path().join("mods");
    let game = dir.path().join("game");
    fs::create_dir(&mods).unwrap();
    fs::create_dir(&game).unwrap();
    fs::write(mods.join("sample.tga"), b"texture").unwrap();
    let settings = Settings::new(&mods, &game).unwrap();
    assert!(
        Engine::new(settings.clone())
            .unwrap()
            .install(&guide, |_| {})
            .unwrap_err()
            .to_string()
            .contains("unreviewed draft")
    );
    assert_eq!(fs::read_dir(&game).unwrap().count(), 0);
    assert_eq!(fs::read(mods.join("sample.tga")).unwrap(), b"texture");
    draft::mark_reviewed(&mut guide.components[0]).unwrap();
    let reloaded = Guide::parse(&guide.render("toml").unwrap(), "toml").unwrap();
    assert!(!draft::requires_review(&reloaded.components[0]));
    Engine::new(settings.clone())
        .unwrap()
        .install(&reloaded, |_| {})
        .unwrap();
    assert_eq!(
        fs::read(game.join("Override/sample.tga")).unwrap(),
        b"texture"
    );
    assert!(!mods.join("sample.tga").exists());
    let mut invalid = reloaded.clone();
    invalid.components[0].instructions[0].source = vec!["<<modDirectory>>/CON".into()];
    assert!(draft::sanitize(&mut invalid.components[0].instructions[0]));
    let error = Engine::new(settings)
        .unwrap()
        .install(&invalid, |_| {})
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("Reserved filename"),
        "{error:#}"
    );
    assert_eq!(
        fs::read(game.join("Override/sample.tga")).unwrap(),
        b"texture"
    );
    let mut incomplete = Guide::parse(include_str!("fixtures/drafts.json"), "json").unwrap();
    draft::generate(&mut incomplete);
    for name in ["Conditional", "BareCleanList", "UnknownAction"] {
        let component = incomplete
            .components
            .iter_mut()
            .find(|c| c.name == name)
            .unwrap();
        assert!(draft::mark_reviewed(component).is_err(), "{name}");
    }
    let mut instruction = guide.components[0].instructions[0].clone();
    for source in [
        "/etc/passwd",
        "<<modDirectory>>../outside",
        "<<modDirectory>>/../outside",
        "<<modDirectory>>/C:/outside",
    ] {
        instruction.source = vec![source.into()];
        assert!(!draft::sanitize(&mut instruction), "{source}");
    }
    instruction.source = vec!["<<gameDirectory>>/safe.tga".into()];
    assert!(draft::sanitize(&mut instruction));
    assert_eq!(instruction.source, ["<<kotorDirectory>>/safe.tga"]);
}

#[test]
fn cli_drafts_and_reviews_without_installing() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let markdown = dir.path().join("input.md");
    fs::write(
        &markdown,
        "### Test\n**Installation Instructions:** Move only sample.tga to your Override.\n",
    )
    .unwrap();
    assert!(
        Guide::load(&markdown).unwrap().components[0]
            .instructions
            .is_empty()
    );
    assert!(draft::requires_review(
        &Guide::load_for_install(&markdown).unwrap().components[0]
    ));
    let input = dir.path().join("input.json");
    fs::write(&input, json!([{"Guid":"test","Name":"Test","Directions":"Move only sample.tga to your Override."}]).to_string()).unwrap();
    let output = dir.path().join("draft.json");
    let result = Command::new(env!("CARGO_BIN_EXE_modsync"))
        .args([
            "convert",
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "--parse-directions",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(draft::requires_review(
        &Guide::load(&output).unwrap().components[0]
    ));
    let reviewed = dir.path().join("reviewed.json");
    let result = Command::new(env!("CARGO_BIN_EXE_modsync"))
        .args([
            "convert",
            output.to_str().unwrap(),
            reviewed.to_str().unwrap(),
            "--review-draft",
            "test",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!draft::requires_review(
        &Guide::load(&reviewed).unwrap().components[0]
    ));
}

#[test]
fn direct_markdown_rejects_unreviewed_drafts_before_game_writes() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let markdown = dir.path().join("input.md");
    let game = dir.path().join("game");
    let mods = dir.path().join("mods");
    fs::create_dir(&game).unwrap();
    fs::create_dir(&mods).unwrap();
    fs::write(mods.join("sample.tga"), "source").unwrap();
    fs::write(
        &markdown,
        "### Test\n**Installation Instructions:** Move only sample.tga to your Override.\n",
    )
    .unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_modsync"))
        .args(["install", "--direct-markdown", "--input"])
        .arg(&markdown)
        .arg("--game-dir")
        .arg(&game)
        .arg("--source-dir")
        .arg(&mods)
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("preflight stopped before game writes")
    );
    assert_eq!(fs::read_dir(&game).unwrap().count(), 0);
}
