use modsync::fomod::{GroupSelection, GroupType, PluginSelection};

fn plugin(name: &str, required: bool, selected: bool) -> PluginSelection {
    PluginSelection {
        name: name.into(),
        description: String::new(),
        image_path: String::new(),
        files: vec![],
        required,
        flags: vec![],
        selected,
    }
}
fn selected(group: &GroupSelection) -> Vec<bool> {
    group.plugins().iter().map(|p| p.selected).collect()
}

#[test]
fn selection_modes_match_pinned_csharp_oracle() {
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../docs/parity/fomod-selection-oracle/result.json"
    ))
    .unwrap();
    for case in reference["Groups"].as_array().unwrap() {
        let kind = serde_json::from_value(case["Group"].clone()).unwrap();
        let mut group = GroupSelection::new(
            "Group".into(),
            kind,
            vec![
                plugin("First", false, false),
                plugin("Second", false, false),
            ],
        );
        assert_eq!(serde_json::json!(selected(&group)), case["Defaults"]);
        assert!(group.validate().is_ok());
        group.set_selected(0, false).unwrap();
        assert_eq!(
            serde_json::json!(group.validate().err().map(|e| e.to_string())),
            case["ClearedValidation"]
        );
        group.set_selected(1, true).unwrap();
        assert_eq!(serde_json::json!(selected(&group)), case["Final"]);
        assert_eq!(
            serde_json::json!(group.validate().err().map(|e| e.to_string())),
            case["FinalValidation"]
        );
    }
}

#[test]
fn required_choices_cannot_be_removed_directly_or_through_exclusive_siblings() {
    for kind in [GroupType::SelectExactlyOne, GroupType::SelectAtMostOne] {
        let mut group = GroupSelection::new(
            "Required".into(),
            kind,
            vec![
                plugin("Required", true, false),
                plugin("Optional", false, false),
            ],
        );
        for (index, value) in [(0, false), (1, true), (2, true)] {
            assert!(group.set_selected(index, value).is_err());
            assert_eq!(selected(&group), [true, false]);
            group.validate().unwrap();
        }
        let conflicting = GroupSelection::new(
            "Conflicting".into(),
            kind,
            vec![plugin("A", true, true), plugin("B", true, true)],
        );
        assert!(conflicting.validate().is_err());
    }
    let recommended = GroupSelection::new(
        "Recommended".into(),
        GroupType::SelectExactlyOne,
        vec![plugin("A", false, false), plugin("B", false, true)],
    );
    assert_eq!(selected(&recommended), [false, true]);
    for (kind, valid) in [
        (GroupType::SelectAny, true),
        (GroupType::SelectAll, true),
        (GroupType::SelectAtMostOne, true),
        (GroupType::SelectExactlyOne, false),
        (GroupType::SelectAtLeastOne, false),
    ] {
        assert_eq!(
            GroupSelection::new("Empty".into(), kind, vec![])
                .validate()
                .is_ok(),
            valid
        );
    }
}

#[test]
fn choices_file_schema_and_named_selections_reject_ambiguous_or_invalid_changes() {
    use modsync::fomod::ChoicesFile;
    let json = r#"{"archives":[{"archiveFileName":"Example.zip","selections":[{"stepName":"Step","groupName":"Group","plugins":["second","SECOND"]}]}]}"#;
    let file = ChoicesFile::parse(json).unwrap();
    let archive = file.archive("EXAMPLE.ZIP").unwrap();
    let mut group = GroupSelection::new(
        "Group".into(),
        GroupType::SelectExactlyOne,
        vec![
            plugin("First", false, false),
            plugin("Second", false, false),
        ],
    );
    group.apply_names(&archive.selections[0].plugins).unwrap();
    assert_eq!(selected(&group), [false, true]);
    for names in [
        vec![],
        vec!["Unknown".into()],
        vec!["First".into(), "Second".into()],
    ] {
        assert!(group.apply_names(&names).is_err());
        assert_eq!(selected(&group), [false, true]);
    }
    let mut required = GroupSelection::new(
        "Group".into(),
        GroupType::SelectAny,
        vec![plugin("Required", true, false)],
    );
    assert!(required.apply_names(&[]).is_err());
    assert_eq!(selected(&required), [true]);
    let mut ambiguous = GroupSelection::new(
        "Group".into(),
        GroupType::SelectAny,
        vec![plugin("Same", false, false), plugin("SAME", false, false)],
    );
    assert!(ambiguous.apply_names(&["same".into()]).is_err());
    assert_eq!(selected(&ambiguous), [false, false]);
    for invalid in [
        r#"{"version":2}"#,
        r#"{"archives":[{"archiveFileName":"a.zip"},{"archiveFileName":"A.ZIP"}]}"#,
        r#"{"archives":[{"archiveFileName":"a.zip","selections":[{"stepName":"S","groupName":"G"},{"stepName":"s","groupName":"g"}]}]}"#,
    ] {
        assert!(ChoicesFile::parse(invalid).is_err());
    }
    let saved = serde_json::to_string(&file).unwrap();
    assert!(saved.contains("archiveFileName") && saved.contains("stepName"));
    assert_eq!(ChoicesFile::parse(&saved).unwrap().version, 1);
}

#[test]
fn archive_choices_route_steps_and_commit_only_when_every_group_is_valid() {
    use modsync::fomod::{ArchiveChoices, NamedSelection, StepSelection};
    let group = || {
        GroupSelection::new(
            "Group".into(),
            GroupType::SelectExactlyOne,
            vec![
                plugin("First", false, false),
                plugin("Second", false, false),
            ],
        )
    };
    let initial = vec![
        StepSelection {
            name: "One".into(),
            visible: None,
            groups: vec![group()],
        },
        StepSelection {
            name: "Two".into(),
            visible: None,
            groups: vec![group()],
        },
    ];
    let choices = |last_step: &str, last_plugin: &str| ArchiveChoices {
        archive_file_name: "Example.zip".into(),
        selections: vec![
            NamedSelection {
                step_name: "ONE".into(),
                group_name: "group".into(),
                plugins: vec!["Second".into()],
            },
            NamedSelection {
                step_name: last_step.into(),
                group_name: "Group".into(),
                plugins: vec![last_plugin.into()],
            },
        ],
    };
    let mut steps = initial.clone();
    for invalid in [
        choices("Missing", "Second"),
        choices("Two", "Missing"),
        choices("One", "First"),
    ] {
        assert!(invalid.apply_to(&mut steps).is_err());
        assert_eq!(selected(&steps[0].groups[0]), [true, false]);
        assert_eq!(selected(&steps[1].groups[0]), [true, false]);
    }
    choices("two", "second").apply_to(&mut steps).unwrap();
    assert!(
        steps
            .iter()
            .all(|s| selected(&s.groups[0]) == [false, true])
    );
    let mut ambiguous = initial.clone();
    ambiguous.push(initial[0].clone());
    assert!(choices("Two", "Second").apply_to(&mut ambiguous).is_err());
    let mut invalid_default = initial;
    invalid_default[1].groups[0].set_selected(0, false).unwrap();
    let mut only_first = choices("Two", "Second");
    only_first.selections.pop();
    assert!(only_first.apply_to(&mut invalid_default).is_err());
    assert_eq!(selected(&invalid_default[0].groups[0]), [true, false]);
}

#[test]
fn visibility_uses_ordered_selected_flags_and_only_validates_visible_steps() {
    use modsync::fomod::{ArchiveChoices, FlagCondition, StepSelection, visible_steps};
    let flag = |name: &str, value: &str| FlagCondition::Flag {
        name: name.into(),
        value: value.into(),
    };
    let mut first = plugin("First", false, true);
    first.flags = vec![("Choice".into(), "old".into())];
    let mut hidden = plugin("Hidden", false, true);
    hidden.flags = vec![("CHOICE".into(), "new".into())];
    let mut steps = vec![
        StepSelection {
            name: "Base".into(),
            visible: None,
            groups: vec![GroupSelection::new(
                "Base".into(),
                GroupType::SelectAny,
                vec![first],
            )],
        },
        StepSelection {
            name: "Hidden".into(),
            visible: Some(flag("missing", "yes")),
            groups: vec![
                GroupSelection::new("Hidden".into(), GroupType::SelectAny, vec![hidden]),
                GroupSelection::new("Empty".into(), GroupType::SelectExactlyOne, vec![]),
            ],
        },
        StepSelection {
            name: "Visible".into(),
            visible: Some(FlagCondition::All(vec![
                flag("choice", "new"),
                FlagCondition::Any(vec![]),
            ])),
            groups: vec![],
        },
        StepSelection {
            name: "Wrong case".into(),
            visible: Some(flag("choice", "NEW")),
            groups: vec![],
        },
    ];
    assert_eq!(visible_steps(&steps), [0, 2]);
    ArchiveChoices {
        archive_file_name: "a.zip".into(),
        selections: vec![],
    }
    .apply_to(&mut steps)
    .unwrap();
    steps[1].groups[0].set_selected(0, false).unwrap();
    assert_eq!(visible_steps(&steps), [0]);
    steps[1].visible = Some(FlagCondition::Any(vec![
        flag("Choice", "old"),
        flag("missing", "yes"),
    ]));
    assert_eq!(visible_steps(&steps), [0, 1]);
    assert!(
        ArchiveChoices {
            archive_file_name: "a.zip".into(),
            selections: vec![]
        }
        .apply_to(&mut steps)
        .is_err()
    );
}

#[test]
fn module_xml_drives_selection_defaults_flags_and_visibility_without_installing() {
    use modsync::fomod::{parse_selection_xml, visible_steps};
    let xml = r#"<config xmlns="urn:fomod"><installSteps><installStep name="First &amp; Second"><optionalFileGroups><group name="Choose" type="SelectExactlyOne"><plugins><plugin name="A"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin><plugin name="B"><conditionFlags><flag name="Picked"><![CDATA[ yes ]]></flag></conditionFlags><typeDescriptor><dependencyType><defaultType name="Recommended"/></dependencyType></typeDescriptor></plugin></plugins></group></optionalFileGroups></installStep><installStep name="Conditional"><visible operator="And"><dependencies operator="Or"><flagDependency flag="picked" value="yes"/></dependencies></visible></installStep></installSteps></config>"#;
    let mut steps = parse_selection_xml(xml).unwrap();
    assert_eq!(steps[0].name, "First & Second");
    assert_eq!(selected(&steps[0].groups[0]), [false, true]);
    assert_eq!(visible_steps(&steps), [0, 1]);
    steps[0].groups[0].set_selected(0, true).unwrap();
    assert_eq!(visible_steps(&steps), [0]);
    for invalid in [
        "<wrong/>",
        "<config>",
        "<config/><config/>",
        "<!DOCTYPE config [<!ENTITY e SYSTEM 'file:///etc/passwd'>]><config>&e;</config>",
    ] {
        assert!(parse_selection_xml(invalid).is_err());
    }
    assert!(
        parse_selection_xml(&xml.replace(
            "flagDependency flag=\"picked\" value=\"yes\"",
            "fileDependency file=\"a\" state=\"Active\""
        ))
        .is_err()
    );
}

#[test]
fn xml_plugin_details_preserve_authored_text_without_opening_image_paths() {
    use modsync::fomod::parse_selection_xml;
    let xml = r#"<config><installSteps><installStep name="Details"><optionalFileGroups><group><plugins><plugin name="Described"><description> Before <b>bold &amp; bright</b> after <![CDATA[<literal>]]> </description><image path="../outside &amp; preview.png"/></plugin><plugin name="Empty"/></plugins></group></optionalFileGroups></installStep></installSteps></config>"#;
    let steps = parse_selection_xml(xml).unwrap();
    let plugins = steps[0].groups[0].plugins();
    assert_eq!(
        plugins[0].description,
        "Before bold & bright after <literal>"
    );
    assert_eq!(plugins[0].image_path, "../outside & preview.png");
    assert!(plugins[1].description.is_empty() && plugins[1].image_path.is_empty());
}

#[test]
fn module_file_directives_keep_reference_order_paths_and_priority_defaults() {
    use modsync::fomod::parse_module_xml;
    let xml = r#"<config><moduleName>Example &amp; files</moduleName><requiredInstallFiles><file source="a.txt" destination="Override/b.txt" priority=" -4 "/><folder source="textures" destination="Override" priority="2147483648"/><file source="../untrusted" priority="bad"/></requiredInstallFiles><installSteps><installStep><optionalFileGroups><group><plugins><plugin name="Optional"><files><file source="optional.tga" destination="Override" priority="12"/></files></plugin></plugins></group></optionalFileGroups></installStep></installSteps><conditionalFileInstalls><patterns><pattern><dependencies><flagDependency flag="option" value="yes"/></dependencies><files><folder source="conditional" destination="Override"/></files></pattern><pattern><files><file source="always.txt"/></files></pattern></patterns></conditionalFileInstalls></config>"#;
    let module = parse_module_xml(xml).unwrap();
    assert_eq!(module.name, "Example & files");
    assert_eq!(
        module
            .required_files
            .iter()
            .map(|f| f.priority)
            .collect::<Vec<_>>(),
        [-4, 0, 0]
    );
    assert_eq!(module.required_files[0].destination, "Override/b.txt");
    assert!(module.required_files[1].is_folder);
    assert_eq!(module.required_files[2].source, "../untrusted");
    assert_eq!(module.steps[0].groups[0].plugins()[0].files[0].priority, 12);
    assert_eq!(module.conditional_files.len(), 2);
    assert_eq!(module.conditional_files[0].files[0].source, "conditional");
    assert!(module.conditional_files[1].files[0].destination.is_empty());
}

#[test]
fn file_plan_preserves_conditional_attachment_and_never_skips_unsupported_patterns() {
    use modsync::fomod::parse_module_xml;
    let xml = r#"<config><requiredInstallFiles><file source="required"/></requiredInstallFiles><installSteps><installStep><optionalFileGroups><group type="SelectAll"><plugins><plugin name="A"><files><file source="a"/></files><conditionFlags><flag name="mode">yes</flag></conditionFlags></plugin><plugin name="B"><files><file source="b"/></files><conditionFlags><flag name="mode">yes</flag></conditionFlags></plugin></plugins></group></optionalFileGroups></installStep></installSteps><conditionalFileInstalls><patterns><pattern><dependencies><flagDependency flag="MODE" value="yes"/></dependencies><files><file source="attached"/></files></pattern><pattern><files><file source="trailing"/></files></pattern></patterns></conditionalFileInstalls></config>"#;
    let mut module = parse_module_xml(xml).unwrap();
    let sources = |m: &modsync::fomod::ModuleConfig| {
        m.selected_files()
            .unwrap()
            .iter()
            .map(|f| f.source.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        sources(&module),
        ["required", "a", "attached", "b", "trailing"]
    );
    module.steps[0].groups[0].kind = GroupType::SelectAny;
    module.steps[0].groups[0].set_selected(0, false).unwrap();
    assert_eq!(sources(&module), ["required", "b", "trailing"]);
    let unsupported = xml.replace("<dependencies>", "<dependencies><dependencies/>");
    assert!(
        parse_module_xml(&unsupported)
            .unwrap()
            .selected_files()
            .is_err()
    );
    let missing = xml.replace("value=\"yes\"", "value=\"missing\"");
    assert!(
        parse_module_xml(&missing)
            .unwrap()
            .selected_files()
            .is_err()
    );
}

#[test]
fn mapped_file_copies_use_shared_preflight_and_install_without_mutating_sources() {
    use modsync::{Component, Engine, Guide, Settings, fomod::parse_module_xml};
    let temp = tempfile::tempdir().unwrap();
    let mods = temp.path().join("mods");
    let game = temp.path().join("game");
    std::fs::create_dir_all(mods.join("package/textures")).unwrap();
    std::fs::create_dir(&game).unwrap();
    std::fs::write(mods.join("package/textures/a.tga"), b"texture").unwrap();
    let xml = r#"<config><requiredInstallFiles><file source="textures\a.tga" destination="Override/a.tga"/></requiredInstallFiles></config>"#;
    let module = parse_module_xml(xml).unwrap();
    let guide = Guide {
        components: vec![Component {
            guid: "fomod".into(),
            name: "FOMOD".into(),
            is_selected: true,
            instructions: module.copy_instructions("package").unwrap(),
            ..Default::default()
        }],
        ..Default::default()
    };
    Engine::new(Settings::new(&mods, &game).unwrap())
        .unwrap()
        .validate(&guide, |_| {})
        .unwrap();
    assert!(!game.join("Override").exists());
    Engine::new(Settings::new(&mods, &game).unwrap())
        .unwrap()
        .install(&guide, |_| {})
        .unwrap();
    assert_eq!(
        std::fs::read(game.join("Override/a.tga")).unwrap(),
        b"texture"
    );
    assert_eq!(
        std::fs::read(mods.join("package/textures/a.tga")).unwrap(),
        b"texture"
    );
    for source in ["../escape", "/absolute", "C:\\escape", "*.tga"] {
        let bad = xml.replace("textures\\a.tga", source);
        assert!(
            parse_module_xml(&bad)
                .unwrap()
                .copy_instructions("package")
                .is_err()
        );
    }
    assert!(module.copy_instructions("../package").is_err());
    for bad in [
        xml.replace("Override/a.tga", "../escape"),
        xml.replace("Override/a.tga", "Override/b.tga"),
    ] {
        assert!(
            parse_module_xml(&bad)
                .unwrap()
                .copy_instructions("package")
                .is_err()
        );
    }
}

#[test]
fn folder_mapping_matches_reference_top_level_only_copy() {
    use modsync::{Component, Engine, Guide, Settings, fomod::parse_module_xml};
    let temp = tempfile::tempdir().unwrap();
    let mods = temp.path().join("mods");
    let game = temp.path().join("game");
    std::fs::create_dir_all(mods.join("package/textures/nested")).unwrap();
    std::fs::create_dir(&game).unwrap();
    std::fs::write(mods.join("package/textures/top.txt"), b"top").unwrap();
    std::fs::write(mods.join("package/textures/nested/deep.txt"), b"deep").unwrap();
    let module = parse_module_xml("<config><requiredInstallFiles><folder source='textures' destination='Override'/></requiredInstallFiles></config>").unwrap();
    let instructions = module.copy_instructions("package").unwrap();
    assert_eq!(
        instructions[0].source,
        ["<<modDirectory>>/package/textures/*"]
    );
    let guide = Guide {
        components: vec![Component {
            guid: "folder".into(),
            name: "Folder".into(),
            is_selected: true,
            instructions,
            ..Default::default()
        }],
        ..Default::default()
    };
    Engine::new(Settings::new(&mods, &game).unwrap())
        .unwrap()
        .validate(&guide, |_| {})
        .unwrap();
    assert!(!game.join("Override").exists());
    Engine::new(Settings::new(&mods, &game).unwrap())
        .unwrap()
        .install(&guide, |_| {})
        .unwrap();
    assert_eq!(
        std::fs::read(game.join("Override/top.txt")).unwrap(),
        b"top"
    );
    assert_eq!(std::fs::read_dir(game.join("Override")).unwrap().count(), 1);
    assert_eq!(
        std::fs::read(mods.join("package/textures/nested/deep.txt")).unwrap(),
        b"deep"
    );
}

#[test]
fn info_xml_preserves_reference_metadata_and_rejects_unsafe_xml() {
    use modsync::fomod::{FomodInfo, parse_info_xml};
    assert_eq!(parse_info_xml("<anything/>").unwrap(), FomodInfo::default());
    let info = parse_info_xml(
        r#"<fomod xmlns:m="urn:test">
      <m:nAmE> First &amp; Last </m:nAmE><Name>ignored duplicate</Name>
      <Author>Author</Author><Version>1.2</Version>
      <Website>https://example.invalid/?a=1&amp;b=2</Website>
      <Description>Before <b>bold</b><![CDATA[ after]]></Description>
      <wrapper><Name>not a direct child</Name></wrapper>
    </fomod>"#,
    )
    .unwrap();
    assert_eq!(
        info,
        FomodInfo {
            name: " First & Last ".into(),
            author: "Author".into(),
            version: "1.2".into(),
            website: "https://example.invalid/?a=1&b=2".into(),
            description: "Before bold after".into(),
        }
    );
    for invalid in [
        "",
        "<fomod>",
        "<fomod/><other/>",
        "<!DOCTYPE fomod [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><fomod><Name>&x;</Name></fomod>",
    ] {
        assert!(parse_info_xml(invalid).is_err());
    }
}

#[test]
fn archive_inspection_reads_nested_metadata_and_cleans_scratch_on_failure() {
    use modsync::{archive::Options, fomod::inspect_archive};
    use std::{io::Write, sync::atomic::AtomicBool};
    let temp = tempfile::tempdir().unwrap();
    let scratch = temp.path().join("scratch");
    std::fs::create_dir(&scratch).unwrap();
    let path = temp.path().join("mod.zip");
    let options = Options {
        scratch_dir: Some(scratch.clone()),
        max_expanded_bytes: Some(4096),
    };
    let write = |files: &[(&str, &str)]| {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for (name, content) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    };
    write(&[
        (
            "Wrapper/FoMoD/ModuleConfig.xml",
            "<config><moduleName>Module</moduleName></config>",
        ),
        (
            "Wrapper/FoMoD/INFO.XML",
            "<fomod><Name>Metadata</Name></fomod>",
        ),
    ]);
    let original = std::fs::read(&path).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_modsync"))
        .arg("fomod-inspect")
        .arg(&path)
        .arg("--scratch-dir")
        .arg(&scratch)
        .arg("--max-expanded-bytes")
        .arg("4096")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["contentRoot"], "Wrapper");
    assert_eq!(report["name"], "Module");
    assert_eq!(report["info"]["name"], "Metadata");
    assert_eq!(report["steps"], serde_json::json!([]));

    let result = inspect_archive(&path, &options, &AtomicBool::new(false))
        .unwrap()
        .unwrap();
    assert_eq!(result.content_root, std::path::Path::new("Wrapper"));
    assert_eq!(result.config.name, "Module");
    assert_eq!(result.info.unwrap().name, "Metadata");
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 0);
    assert!(inspect_archive(&path, &options, &AtomicBool::new(true)).is_err());
    for files in [
        vec![("fomod/ModuleConfig.xml", "<broken>")],
        vec![
            ("fomod/ModuleConfig.xml", "<config/>"),
            ("other/fomod/ModuleConfig.xml", "<config/>"),
        ],
        vec![("../fomod/ModuleConfig.xml", "<config/>")],
    ] {
        write(&files);
        assert!(inspect_archive(&path, &options, &AtomicBool::new(false)).is_err());
        assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 0);
    }
    write(&[("ordinary.txt", "ordinary")]);
    assert!(
        inspect_archive(&path, &options, &AtomicBool::new(false))
            .unwrap()
            .is_none()
    );
    std::fs::write(&path, b"corrupt").unwrap();
    assert!(inspect_archive(&path, &options, &AtomicBool::new(false)).is_err());
}
