use modsync::Guide;
use serde_json::Value;

const FIXTURE: &str = include_str!("fixtures/guide.json");

#[test]
fn csharp_shapes_roundtrip_collections_options_paths_and_resources() {
    let mut guide = Guide::parse(FIXTURE, "json").unwrap();
    guide.components[0].options[0].resource_registry =
        guide.components[0].resource_registry.clone();
    assert_eq!(guide.components[0].category, ["Textures", "Models"]);
    assert_eq!(guide.components[0].instructions[0].action, "Copy");
    assert!(!guide.components[0].instructions[1].overwrite);
    assert_eq!(
        guide.components[0].options[0].instructions[0].action,
        "Patcher"
    );
    let expected = serde_json::to_value(&guide.components).unwrap();
    for format in ["toml", "JSON", "YAML", "xml"] {
        let text = guide.render(format).unwrap();
        let actual = Guide::parse(&text, format).unwrap();
        assert_eq!(
            serde_json::to_value(actual.components).unwrap(),
            expected,
            "{format}"
        );
        assert_eq!(
            serde_json::to_value(Guide::parse(&text, "auto").unwrap().components).unwrap(),
            expected,
            "auto {format}"
        );
    }
    assert!(
        serde_json::from_str::<Value>(&guide.render("json").unwrap())
            .unwrap()
            .is_array()
    );
    assert!(guide.render("yaml").unwrap().starts_with("---\n"));
}

#[test]
fn xml_legacy_roots_singletons_cdata_entities_and_rejection() {
    let xml = r#"<KOTORModSync><ModComponents><Guid>11111111-1111-1111-1111-111111111111</Guid><Name>A &amp; B &#x26; C</Name><Description><![CDATA[<raw>& text]]></Description><Category>Textures</Category><Instructions><Instruction><Action>copy</Action><Source>&lt;&lt;modDirectory&gt;&gt;/one</Source><Overwrite>false</Overwrite></Instruction></Instructions><Options><Option><Guid>22222222-2222-2222-2222-222222222222</Guid><Name>Only option</Name></Option></Options></ModComponents></KOTORModSync>"#;
    let g = Guide::parse(xml, "xml").unwrap();
    assert_eq!(g.components[0].name, "A & B & C");
    assert_eq!(g.components[0].description, "<raw>& text");
    assert_eq!(
        g.components[0].instructions[0].source,
        ["<<modDirectory>>/one"]
    );
    assert_eq!(g.components[0].options.len(), 1);
    for input in [
        "<!DOCTYPE x [<!ENTITY leak SYSTEM 'file:///etc/passwd'>]><x>&leak;</x>",
        "<ModSync><ModComponents></ModSync>",
        "<ModSync/><ModSync/>",
        "<ModSync><ModComponents>&unknown;</ModComponents></ModSync>",
    ] {
        assert!(Guide::parse(input, "xml").is_err(), "{input}");
    }
}

#[test]
fn yaml_documents_metadata_aliases_and_extension_detection() {
    let yaml = "---\n# Metadata\nfileFormatVersion: '2.0'\nbuildName: Demo\n---\nGuid: 11111111-1111-1111-1111-111111111111\nName: First\nInstructions:\n- Action: extract\n  Source: '<<modDirectory>>/a.zip'\n---\nGuid: 22222222-2222-2222-2222-222222222222\nName: Second\n";
    let guide = Guide::parse(yaml, "yml").unwrap();
    assert_eq!(guide.components.len(), 2);
    assert_eq!(guide.metadata["metadata"]["buildName"], "Demo");
    let roundtrip = Guide::parse(&guide.render("yaml").unwrap(), "yaml").unwrap();
    assert_eq!(roundtrip.metadata, guide.metadata);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/guide.XML");
    guide.save(&path).unwrap();
    assert_eq!(Guide::load(&path).unwrap().components.len(), 2);
    std::fs::write(dir.path().join("unknown.extension"), yaml).unwrap();
    assert_eq!(
        Guide::load(&dir.path().join("unknown.extension"))
            .unwrap()
            .components
            .len(),
        2
    );
    assert_eq!(
        Guide::parse(&guide.render("tml").unwrap(), "tml")
            .unwrap()
            .components
            .len(),
        2
    );
    assert!(Guide::parse("---\nGuid: wrong\n", "yaml").is_err());
}

#[test]
fn cli_converts_stdin_and_named_paths_without_changing_inputs() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_modsync"))
        .args(["convert", "--stdin", "--format", "xml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(FIXTURE.as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        Guide::parse(&String::from_utf8(result.stdout).unwrap(), "xml")
            .unwrap()
            .components
            .len(),
        2
    );
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.json");
    let output = dir.path().join("nested/output.yaml");
    std::fs::write(&input, FIXTURE).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_modsync"))
        .arg("convert")
        .arg("--input")
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .args(["--format", "yaml"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(Guide::load(&output).unwrap().components.len(), 2);
    assert_eq!(std::fs::read_to_string(input).unwrap(), FIXTURE);
}

#[test]
fn reads_pinned_csharp_exports_without_losing_components_or_instructions() {
    for (format, input) in [
        ("json", include_str!("fixtures/csharp/csharp.json")),
        ("yaml", include_str!("fixtures/csharp/csharp.yaml")),
        ("xml", include_str!("fixtures/csharp/csharp.xml")),
        ("toml", include_str!("fixtures/csharp/csharp.toml")),
    ] {
        let guide = Guide::parse(input, format).unwrap();
        assert_eq!(guide.components.len(), 2, "{format}");
        assert_eq!(guide.components[0].name, "Robes & Lights <HD>");
        assert_eq!(guide.components[0].instructions.len(), 2, "{format}");
        assert_eq!(guide.components[0].instructions[0].action, "Copy");
        assert_eq!(
            guide.components[0].instructions[0].source,
            ["<<modDirectory>>/sample.tga"]
        );
        assert_eq!(guide.components[0].options.len(), 1, "{format}");
        assert_eq!(
            guide.components[0].options[0].instructions.len(),
            1,
            "{format}"
        );
        assert_eq!(
            guide.components[0].options[0].instructions[0].action,
            "Patcher"
        );
    }
}

#[test]
fn markdown_fields_and_embedded_instructions_match_csharp() {
    let fenced = format!("### Instructions\n```json\n{FIXTURE}\n```\n");
    assert_eq!(Guide::parse(&fenced, "md").unwrap().components.len(), 2);
    assert!(
        Guide::parse(
            &format!("{fenced}\n### Another mod\nName: Unexecuted\n"),
            "md"
        )
        .is_err()
    );
    let text = include_str!("fixtures/guide.md");
    let notes = Guide::parse(
        &text
            .replace("Functionality:** All", "Functionality:** French, German")
            .replace(
                "Copy the chosen textures.",
                "**For Windows:** Copy textures.\n\n    **For Linux:** Copy textures too.",
            ),
        "md",
    )
    .unwrap();
    assert_eq!(
        notes.components[0].metadata["Language"],
        serde_json::json!(["French, German"])
    );
    assert!(notes.components[0].directions.contains("**For Linux:**"));
    let actual = Guide::parse(text, "md").unwrap();
    let expected = Guide::parse(include_str!("fixtures/csharp/markdown.json"), "json").unwrap();
    assert_eq!(actual.components.len(), 2);
    let a = serde_json::to_value(&actual.components).unwrap();
    let b = serde_json::to_value(&expected.components).unwrap();
    for (a, b) in a.as_array().unwrap().iter().zip(b.as_array().unwrap()) {
        for key in [
            "Guid",
            "Name",
            "Author",
            "Description",
            "Tier",
            "Category",
            "Directions",
            "IsSelected",
            "Instructions",
            "Options",
            "Dependencies",
            "Restrictions",
        ] {
            assert_eq!(a[key], b[key], "{key}");
        }
    }
    assert_eq!(actual.components[0].metadata["Heading"], "Display title");
    assert_eq!(
        actual.components[0].metadata["Language"],
        serde_json::json!(["All"])
    );
    assert!(
        actual.components[0]
            .resource_registry
            .contains_key("https://example.com/base.zip")
    );
    assert!(
        actual.metadata["preambleContent"]
            .as_str()
            .unwrap()
            .contains("Read before")
    );
    assert!(
        actual.metadata["epilogueContent"]
            .as_str()
            .unwrap()
            .contains("This is not a mod")
    );
    let crlf = Guide::parse(&text.replace('\n', "\r\n"), "markdown").unwrap();
    assert_eq!(serde_json::to_value(crlf.components).unwrap(), a);
    assert_eq!(Guide::parse(text, "auto").unwrap().components.len(), 2);
    for malformed in [
        text.replace("Action: Copy", "Unrecognized: Copy"),
        text.replace("-->\n___", "\n___"),
        text.replace("Instructions:\n", "Instructions: [broken\n"),
    ] {
        assert!(Guide::parse(&malformed, "md").is_err());
    }
    let dir = tempfile::tempdir().unwrap();
    let mods = dir.path().join("mods");
    let game = dir.path().join("game");
    std::fs::create_dir(&mods).unwrap();
    std::fs::create_dir(&game).unwrap();
    std::fs::write(mods.join("sample.tga"), b"base").unwrap();
    std::fs::write(mods.join("extra.tga"), b"extra").unwrap();
    let settings = modsync::Settings::new(&mods, &game).unwrap();
    modsync::Engine::new(settings.clone())
        .unwrap()
        .validate(&actual, |_| {})
        .unwrap();
    assert_eq!(std::fs::read_dir(&game).unwrap().count(), 0);
    modsync::Engine::new(settings)
        .unwrap()
        .install(&actual, |_| {})
        .unwrap();
    assert_eq!(
        std::fs::read(game.join("Override/sample.tga")).unwrap(),
        b"base"
    );
    assert_eq!(
        std::fs::read(game.join("Override/extra.tga")).unwrap(),
        b"extra"
    );
}

#[test]
fn prose_import_resolves_masters_but_does_not_invent_executable_instructions() {
    let text = "### First\nAuthor: Author\n\n### Second\nMasters: First\n:::warning\nInstallation Instructions\n: Copy files manually.\n:::\n";
    let guide = Guide::parse(text, "md").unwrap();
    assert_eq!(guide.components.len(), 2);
    assert_eq!(
        guide.components[1].dependencies,
        [guide.components[0].guid.clone()]
    );
    assert_eq!(guide.components[1].directions, "Copy files manually.");
    assert!(guide.components.iter().all(|c| c.instructions.is_empty()));
    assert_eq!(
        guide.components[0].guid,
        Guide::parse(text, "md").unwrap().components[0].guid
    );
    let unresolved =
        Guide::parse(&text.replace("Masters: First", "Masters: Missing"), "md").unwrap();
    assert_eq!(
        unresolved.components[1].metadata["UnresolvedMasters"],
        serde_json::json!(["Missing"])
    );
    let dir = tempfile::tempdir().unwrap();
    let mods = dir.path().join("mods");
    let game = dir.path().join("game");
    std::fs::create_dir(&mods).unwrap();
    std::fs::create_dir(&game).unwrap();
    let settings = modsync::Settings::new(&mods, &game).unwrap();
    let error = modsync::Engine::new(settings.clone())
        .unwrap()
        .install(&guide, |_| {})
        .unwrap_err();
    assert!(
        error.to_string().contains("no executable instructions"),
        "{error:#}"
    );
    let mut authored = Guide::parse(include_str!("fixtures/guide.md"), "md").unwrap();
    authored.components[0]
        .metadata
        .insert("UnresolvedMasters".into(), serde_json::json!(["Missing"]));
    let error = modsync::Engine::new(settings)
        .unwrap()
        .install(&authored, |_| {})
        .unwrap_err();
    assert!(
        error.to_string().contains("unresolved Markdown masters"),
        "{error:#}"
    );
    assert_eq!(std::fs::read_dir(mods).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(game).unwrap().count(), 0);
}
