use modsync::{Engine, Guide, PatcherEngine, Settings, archive, paths::Roots, recovery};
use std::{
    fs,
    io::{Cursor, Write},
    path::Path,
    sync::atomic::Ordering,
};

fn setup() -> (tempfile::TempDir, Settings) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("mods")).unwrap();
    fs::create_dir(dir.path().join("game")).unwrap();
    let settings = Settings::new(&dir.path().join("mods"), &dir.path().join("game")).unwrap();
    (dir, settings)
}
fn guide(instructions: serde_json::Value) -> Guide {
    Guide::parse(&serde_json::json!({"thisMod": [{"Guid": "a", "Name": "Test", "IsSelected": true, "Instructions": instructions}]}).to_string(), "json").unwrap()
}
fn copy() -> serde_json::Value {
    serde_json::json!({"Action":"Copy", "Source":["<<modDirectory>>/sample.tga"], "Destination":"<<kotorDirectory>>/Override"})
}
fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn dry_run_tracks_extract_move_rename_delete_without_writing() {
    let (_dir, settings) = setup();
    fs::write(
        settings.roots.mods.join("mod.zip"),
        zip(&[("nested/file.txt", b"content")]),
    )
    .unwrap();
    let g = guide(serde_json::json!([
        {"Action":"Extract", "Source":"<<modDirectory>>/mod.zip"},
        {"Action":"Move", "Source":"<<modDirectory>>/mod/nested/*.txt", "Destination":"<<kotorDirectory>>/Override"},
        {"Action":"Rename", "Source":"<<kotorDirectory>>/Override/file.txt", "Destination":"renamed.txt"},
        {"Action":"Copy", "Source":"<<kotorDirectory>>/Override/renamed.txt", "Destination":"<<kotorDirectory>>/backup"},
        {"Action":"Delete", "Source":"<<kotorDirectory>>/Override/renamed.txt"}
    ]));
    Engine::new(settings.clone())
        .unwrap()
        .validate(&g, |_| {})
        .unwrap();
    assert_eq!(fs::read_dir(&settings.roots.game).unwrap().count(), 0);
    assert_eq!(fs::read_dir(&settings.roots.mods).unwrap().count(), 1);
    assert!(!recovery::state_dir(&settings.roots).exists());
    Engine::new(settings.clone())
        .unwrap()
        .install(&g, |_| {})
        .unwrap();
    assert_eq!(
        fs::read(settings.roots.game.join("backup/renamed.txt")).unwrap(),
        b"content"
    );
    assert!(!settings.roots.game.join("Override/renamed.txt").exists());
    assert!(!settings.roots.mods.join("mod/nested/file.txt").exists());
}

#[test]
fn cli_validation_report_is_one_json_document_with_dry_run() {
    let (dir, settings) = setup();
    fs::write(settings.roots.mods.join("sample.tga"), b"source").unwrap();
    let input = dir.path().join("guide.json");
    fs::write(
        &input,
        include_str!("../docs/parity/validation-pipeline-oracle/guide.json"),
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_modsync"))
        .arg("validate")
        .arg("--input")
        .arg(&input)
        .arg("--game-dir")
        .arg(&settings.roots.game)
        .arg("--source-dir")
        .arg(&settings.roots.mods)
        .args(["--use-file-selection", "--validation-report", "--dry-run"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut expected: serde_json::Value = serde_json::from_str(include_str!(
        "../docs/parity/validation-pipeline-oracle/expected.json"
    ))
    .unwrap();
    expected["inputPath"] = serde_json::json!(input);
    assert_eq!(report, expected);
    assert!(output.stderr.is_empty());
    assert_eq!(
        report["stages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|stage| stage["stage"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["ComponentValidation", "FomodConfiguration", "DryRun"]
    );
    assert_eq!(report["passedCount"], 3);
    assert_eq!(
        report["stages"][0]["summary"],
        "All components passed archive validation."
    );
    assert_eq!(
        report["stages"][2]["summary"],
        "✓ Dry-run validation passed successfully. All instructions appear to be correct."
    );
    assert_eq!(report["dryRun"]["isValid"], true);
    assert!(!settings.roots.game.join("Override/sample.tga").exists());
}

#[test]
fn real_install_uses_case_insensitive_paths_and_texture_priority() {
    let (_dir, settings) = setup();
    fs::write(settings.roots.mods.join("sample.tga"), b"new texture").unwrap();
    fs::create_dir(settings.roots.game.join("override")).unwrap();
    fs::write(
        settings.roots.game.join("override/SAMPLE.tpc"),
        b"old texture",
    )
    .unwrap();
    let g = Guide::parse(include_str!("../examples/demo.toml"), "toml").unwrap();
    Engine::new(settings.clone())
        .unwrap()
        .install(&g, |_| {})
        .unwrap();
    assert_eq!(
        fs::read(settings.roots.game.join("override/sample.tga")).unwrap(),
        b"new texture"
    );
    assert!(!settings.roots.game.join("override/SAMPLE.tpc").exists());
    let names: Vec<_> = fs::read_dir(&settings.roots.game)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, [std::ffi::OsString::from("override")]);
}

#[test]
fn cancellation_stops_later_steps_and_verified_restore_allows_retry() {
    let (_dir, settings) = setup();
    fs::write(settings.roots.mods.join("sample.tga"), b"new").unwrap();
    fs::write(settings.roots.game.join("keep.dat"), b"old").unwrap();
    let g = guide(
        serde_json::json!([copy(), {"Action":"Delete", "Source":"<<kotorDirectory>>/keep.dat"}]),
    );
    let cancel = settings.cancelled.clone();
    let result = Engine::new(settings.clone()).unwrap().install(&g, |e| {
        if e.component == "Test" {
            cancel.store(true, Ordering::Relaxed);
        }
    });
    assert!(result.is_err());
    assert!(settings.roots.game.join("Override/sample.tga").exists());
    assert!(settings.roots.game.join("keep.dat").exists());
    settings.cancelled.store(false, Ordering::Relaxed);
    assert!(
        Engine::new(settings.clone())
            .unwrap()
            .install(&g, |_| {})
            .is_err()
    );
    recovery::restore(&settings.roots).unwrap();
    assert!(!settings.roots.game.join("Override").exists());
    assert_eq!(
        fs::read(settings.roots.game.join("keep.dat")).unwrap(),
        b"old"
    );
    Engine::new(settings.clone())
        .unwrap()
        .install(&g, |_| {})
        .unwrap();
    assert!(!settings.roots.game.join("keep.dat").exists());
}

#[test]
fn corrupted_snapshot_never_overwrites_live_files() {
    let (_dir, settings) = setup();
    fs::write(settings.roots.game.join("file"), b"original").unwrap();
    let lock = recovery::Lock::acquire(&settings.roots).unwrap();
    recovery::begin(&settings.roots).unwrap();
    fs::write(settings.roots.game.join("file"), b"live").unwrap();
    fs::write(
        recovery::state_dir(&settings.roots).join("snapshot/game/file"),
        b"corrupt",
    )
    .unwrap();
    drop(lock);
    assert!(recovery::restore(&settings.roots).is_err());
    assert_eq!(fs::read(settings.roots.game.join("file")).unwrap(), b"live");
}

#[test]
fn unsafe_paths_and_overlapping_roots_are_rejected() {
    let (_dir, settings) = setup();
    for raw in [
        "/etc/passwd",
        "C:\\Windows",
        "<<modDirectory>>/../out",
        "<<modDirectory>>evil/out",
        "<<modDirectory>>/C:/out",
        "<<modDirectory>>/NUL",
        "<<modDirectory>>/foo.",
    ] {
        assert!(settings.roots.resolve(raw, false).is_err(), "{raw}");
    }
    assert!(Roots::new(&settings.roots.mods, &settings.roots.mods).is_err());
    assert!(Roots::new(&settings.roots.mods, settings.roots.mods.parent().unwrap()).is_err());
}

#[test]
fn wildcard_sources_keep_root_names_literal_and_check_existing_parents() {
    let (dir, settings) = setup();
    let mods = dir.path().join("mods[1]");
    fs::rename(&settings.roots.mods, &mods).unwrap();
    let settings = Settings::new(&mods, &settings.roots.game).unwrap();
    fs::create_dir(mods.join("existing")).unwrap();
    fs::write(mods.join("existing/sample.tga"), b"texture").unwrap();
    let g = guide(
        serde_json::json!([{"Action":"Copy", "Source":"<<modDirectory>>/existing/*.tga", "Destination":"<<kotorDirectory>>/Override"}]),
    );
    Engine::new(settings.clone())
        .unwrap()
        .install(&g, |_| {})
        .unwrap();
    assert_eq!(
        fs::read(settings.roots.game.join("Override/sample.tga")).unwrap(),
        b"texture"
    );
}

#[cfg(unix)]
#[test]
fn symlinks_cannot_escape_or_hide_inside_install_trees() {
    let (_dir, settings) = setup();
    std::os::unix::fs::symlink("/tmp", settings.roots.mods.join("escape")).unwrap();
    assert!(
        settings
            .roots
            .resolve("<<modDirectory>>/escape/file", false)
            .is_err()
    );
    assert!(Engine::new(settings).is_err());
}

#[test]
fn archive_checks_integrity_names_collisions_and_links() {
    assert_eq!(
        unpack(&zip(&[("folder/a.txt", b"ok")])).unwrap()[0].1,
        b"ok"
    );
    for name in [
        "../escape",
        "/absolute",
        "C:/windows",
        "..\\escape",
        "foo/../../escape",
    ] {
        assert!(unpack(&zip(&[(name, b"bad")])).is_err(), "{name}");
    }
    assert!(unpack(&zip(&[("a", b"a"), ("A", b"b")])).is_err());
    assert!(unpack(b"<html>Access denied</html>").is_err());
    let mut corrupt = zip(&[("test.txt", b"check crc")]);
    corrupt[38] ^= 1;
    assert!(unpack(&corrupt).is_err());
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    z.add_symlink(
        "link",
        "../../outside",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    assert!(unpack(&z.finish().unwrap().into_inner()).is_err());
}

#[test]
fn seven_zip_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("source.txt"), b"seven zip").unwrap();
    sevenz_rust2::compress_to_path(dir.path().join("source.txt"), dir.path().join("archive.7z"))
        .unwrap();
    let files = unpack(&fs::read(dir.path().join("archive.7z")).unwrap()).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].1, b"seven zip");
    let mut sfx = vec![0u8; 65533]; // Signature straddles the scanner's buffer boundary.
    sfx[..2].copy_from_slice(b"MZ");
    sfx.extend(fs::read(dir.path().join("archive.7z")).unwrap());
    let embedded = unpack(&sfx).unwrap();
    assert_eq!(embedded, files);
    assert!(unpack(b"MZnot an archive").is_err());
}

#[test]
fn rar_reads_and_checks_crc_without_extracting_paths() {
    let files = unpack(include_bytes!("fixtures/version.rar")).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].0, Path::new("VERSION"));
    assert_eq!(files[0].1, b"unrar-0.4.0");
}

#[test]
fn dependency_order_is_stable_and_cycles_missing_dependencies_conflicts_fail() {
    let mut g = Guide::parse(r#"{"thisMod":[{"Guid":"a","Name":"A","IsSelected":true,"Dependencies":["C"]},{"Guid":"b","Name":"B","IsSelected":true},{"Guid":"c","Name":"C","IsSelected":true}]}"#, "json").unwrap();
    assert_eq!(
        g.ordered()
            .unwrap()
            .iter()
            .map(|c| c.guid.as_str())
            .collect::<Vec<_>>(),
        ["b", "c", "a"]
    );
    g.components[2].dependencies.push("a".into());
    assert!(g.ordered().is_err());
    g.components[2].dependencies.clear();
    g.components[2].is_selected = false;
    assert!(g.ordered().is_err());
    g.components[2].is_selected = true;
    g.components[1].restrictions.push("A".into());
    assert!(g.ordered().is_err());
}

#[test]
fn choose_executes_only_selected_options_and_conditions() {
    let (_dir, settings) = setup();
    fs::write(settings.roots.mods.join("sample.tga"), b"chosen").unwrap();
    let mut g = guide(serde_json::json!([
        {"Action":"Choose", "Source":["yes", "no"]},
        {"Action":"Delete", "Source":"<<kotorDirectory>>/Override/sample.tga", "Restrictions":["yes"]}
    ]));
    let option = |id: &str, selected: bool| {
        serde_json::from_value(serde_json::json!({"Guid": id, "Name": id, "IsSelected": selected, "Instructions": [copy()]})).unwrap()
    };
    g.components[0].options = vec![option("yes", true), option("no", false)];
    Engine::new(settings.clone())
        .unwrap()
        .install(&g, |_| {})
        .unwrap();
    assert!(settings.roots.game.join("Override/sample.tga").exists());
}

#[test]
fn cleanlist_mandatory_and_selected_names_only() {
    let (_dir, settings) = setup();
    for name in ["a", "b", "c"] {
        fs::write(settings.roots.game.join(name), name).unwrap();
    }
    fs::write(
        settings.roots.mods.join("clean.csv"),
        "Mandatory Deletions,a\nTest,b\nNot selected,c\n",
    )
    .unwrap();
    let g = guide(
        serde_json::json!([{"Action":"CleanList", "Source":"<<modDirectory>>/clean.csv", "Destination":"<<gameDirectory>>"}]),
    );
    Engine::new(settings.clone())
        .unwrap()
        .install(&g, |_| {})
        .unwrap();
    assert!(!settings.roots.game.join("a").exists());
    assert!(!settings.roots.game.join("b").exists());
    assert!(settings.roots.game.join("c").exists());
}

#[test]
fn unsupported_or_missing_inputs_fail_before_mutation() {
    let (_dir, settings) = setup();
    fs::write(settings.roots.mods.join("sample.tga"), b"source").unwrap();
    for action in ["Unknown", "Execute", "Patcher"] {
        let g = guide(
            serde_json::json!([copy(), {"Action":action, "Source":"<<modDirectory>>/sample.tga"}]),
        );
        assert!(
            Engine::new(settings.clone())
                .unwrap()
                .install(&g, |_| {})
                .is_err()
        );
        assert_eq!(fs::read_dir(&settings.roots.game).unwrap().count(), 0);
    }
    assert!(Guide::parse("# Install some mods\nCopy things.", "md").is_err());
    assert!(Guide::parse(r#"{"thisMod":[{"Guid":"a","Name":"A","Instructions":[{"Action":"Copy","OverwriteTypo":true}]}]}"#, "json").is_err());
}

#[test]
fn formats_roundtrip_and_cli_use_real_engine() {
    let (dir, settings) = setup();
    fs::write(settings.roots.mods.join("sample.tga"), b"source").unwrap();
    let g = guide(serde_json::json!([copy()]));
    for ext in ["toml", "json", "yaml"] {
        let file = dir.path().join(format!("guide.{ext}"));
        g.save(&file).unwrap();
        let parsed = Guide::load(&file).unwrap();
        assert_eq!(
            parsed.components[0].instructions[0].source,
            ["<<modDirectory>>/sample.tga"]
        );
    }
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_modsync"))
        .args(["validate", "--input"])
        .arg(dir.path().join("guide.toml"))
        .arg("--game-dir")
        .arg(&settings.roots.game)
        .arg("--source-dir")
        .arg(&settings.roots.mods)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let event: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(event["message"], "Copy");
    assert!(!Path::new(&settings.roots.game.join("Override")).exists());
}

#[test]
fn exclusive_game_lock_prevents_concurrent_installs() {
    let (_dir, settings) = setup();
    let lock = recovery::Lock::acquire(&settings.roots).unwrap();
    assert!(recovery::Lock::acquire(&settings.roots).is_err());
    let other_game = settings.roots.game.parent().unwrap().join("other-game");
    fs::create_dir(&other_game).unwrap();
    let other = Roots::new(&settings.roots.mods, &other_game).unwrap();
    assert!(recovery::Lock::acquire(&other).is_err());
    drop(lock);
    assert!(recovery::Lock::acquire(&settings.roots).is_ok());
}

#[cfg(unix)]
#[test]
fn external_failure_halts_and_named_ini_staging_keeps_original_config() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, mut settings) = setup();
    let patch = settings.roots.mods.join("patch/tslpatchdata");
    fs::create_dir_all(&patch).unwrap();
    fs::write(patch.join("changes.ini"), "original").unwrap();
    fs::write(patch.join("alternate.ini"), "alternate").unwrap();
    fs::write(patch.join("namespaces.ini"), "original namespaces").unwrap();
    let stub = dir.path().join("patcher");
    fs::write(&stub, "#!/bin/sh\n[ \"$1\" = --install ] && [ \"$2\" = --game-dir ] && [ \"$4\" = --tslpatchdata ] || exit 2\n[ \"$(cat \"$5/tslpatchdata/changes.ini\")\" = alternate ] || exit 3\n[ ! -f \"$5/tslpatchdata/namespaces.ini\" ] || exit 4\n[ \"$6\" = --cli ] && [ \"$7\" = -y ] || exit 5\nprintf patched > \"$3/patched.txt\"\nexit 7\n").unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    settings.patcher = Some(stub);
    settings.patcher_engine = PatcherEngine::OdyPatcher;
    settings.allow_processes = true;
    fs::write(settings.roots.mods.join("sample.tga"), b"later step").unwrap();
    let g = guide(serde_json::json!([
        {"Action":"Patcher", "Source":"<<modDirectory>>/patch", "Arguments":"alternate.ini"}, copy()
    ]));
    Engine::new(settings.clone())
        .unwrap()
        .validate(&g, |_| {})
        .unwrap();
    assert!(!settings.roots.game.join("patched.txt").exists());
    assert!(
        Engine::new(settings.clone())
            .unwrap()
            .install(&g, |_| {})
            .is_err()
    );
    assert_eq!(
        fs::read(settings.roots.game.join("patched.txt")).unwrap(),
        b"patched"
    );
    assert!(!settings.roots.game.join("Override/sample.tga").exists());
    assert_eq!(fs::read(patch.join("changes.ini")).unwrap(), b"original");
    assert!(patch.join("namespaces.ini").exists());
    recovery::restore(&settings.roots).unwrap();
    assert!(!settings.roots.game.join("patched.txt").exists());
}

#[cfg(unix)]
#[test]
fn patcher_log_errors_halt_later_components() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, mut settings) = setup();
    let data = settings.roots.mods.join("patch/tslpatchdata");
    fs::create_dir_all(&data).unwrap();
    let stub = dir.path().join("patcher");
    fs::write(
        &stub,
        r#"#!/bin/sh
printf '[Error] patch failed\n' > "$5/installlog.txt"
exit 0
"#,
    )
    .unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    settings.allow_processes = true;
    settings.patcher = Some(stub);
    settings.patcher_engine = PatcherEngine::OdyPatcher;
    fs::write(settings.roots.mods.join("sample.tga"), b"later step").unwrap();
    let g = guide(serde_json::json!([
        {"Action":"Patcher", "Source":"<<modDirectory>>/patch"}, copy()
    ]));
    let error = Engine::new(settings.clone())
        .unwrap()
        .install(&g, |_| {})
        .unwrap_err();
    assert!(format!("{error:#}").contains("Patcher install log reported errors"));
    assert!(!settings.roots.game.join("Override/sample.tga").exists());
}

#[cfg(unix)]
#[test]
fn kpatcher_uses_console_without_ody_flags() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, mut settings) = setup();
    let data = settings.roots.mods.join("patch/tslpatchdata");
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join("changes.ini"), "config").unwrap();
    let stub = dir.path().join("kpatcher");
    fs::write(
        &stub,
        r#"#!/bin/sh
[ "$1" = --console ] && [ "$2" = --install ] && [ "$3" = --game-dir ] && [ "$5" = --tslpatchdata ] || exit 2
[ "${7-}" != --cli ] && [ "${7-}" != -y ] || exit 3
printf kpatched > "$4/kpatched.txt"
"#,
    )
    .unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    settings.allow_processes = true;
    settings.patcher = Some(stub);
    settings.patcher_engine = PatcherEngine::KPatcher;
    let g = guide(serde_json::json!([
        {"Action":"Patcher", "Source":"<<modDirectory>>/patch"}
    ]));
    Engine::new(settings.clone())
        .unwrap()
        .install(&g, |_| {})
        .unwrap();
    assert_eq!(
        fs::read(settings.roots.game.join("kpatched.txt")).unwrap(),
        b"kpatched"
    );
}

fn unpack(bytes: &[u8]) -> anyhow::Result<Vec<(std::path::PathBuf, Vec<u8>)>> {
    let mut file = tempfile::NamedTempFile::new()?;
    std::io::Write::write_all(&mut file, bytes)?;
    archive::unpack(
        file.path(),
        std::path::Path::new("archive.zip"),
        &archive::Options::default(),
        &std::sync::atomic::AtomicBool::new(false),
    )?
    .into_iter()
    .map(|entry| Ok((entry.path, fs::read(entry.file.path())?)))
    .collect()
}

#[test]
fn tar_gzip_streaming_rejects_links_corruption_and_budget_overruns() {
    use std::sync::atomic::AtomicBool;
    let dir = tempfile::tempdir().unwrap();
    let scratch = dir.path().join("scratch");
    fs::create_dir(&scratch).unwrap();
    let options = archive::Options {
        scratch_dir: Some(scratch.clone()),
        max_expanded_bytes: Some(10),
    };
    let cancel = AtomicBool::new(false);
    let path = dir.path().join("data.tar.gz");
    let file = fs::File::create(&path).unwrap();
    let gzip = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
    let mut tar = tar::Builder::new(gzip);
    let mut root = tar::Header::new_gnu();
    root.set_size(0);
    root.set_mode(0o755);
    root.set_entry_type(tar::EntryType::Directory);
    root.set_cksum();
    tar.append_data(&mut root, "./", std::io::empty()).unwrap();
    let mut header = tar::Header::new_gnu();
    header.set_size(7);
    header.set_mode(0o755);
    header.set_cksum();
    tar.append_data(&mut header, "nested/run.sh", &b"content"[..])
        .unwrap();
    tar.into_inner().unwrap().finish().unwrap();
    let entries = archive::unpack(&path, &path, &options, &cancel).unwrap();
    assert_eq!(entries[0].path, Path::new("nested/run.sh"));
    assert_eq!(entries[0].mode, Some(0o755));
    assert_eq!(fs::read(entries[0].file.path()).unwrap(), b"content");
    drop(entries);
    assert_eq!(fs::read_dir(&scratch).unwrap().count(), 0);
    archive::verify(&path, &path, &options, &cancel).unwrap();
    let small = archive::Options {
        max_expanded_bytes: Some(6),
        ..options.clone()
    };
    assert!(archive::unpack(&path, &path, &small, &cancel).is_err());
    assert_eq!(fs::read_dir(&scratch).unwrap().count(), 0);
    cancel.store(true, Ordering::Relaxed);
    assert!(archive::unpack(&path, &path, &options, &cancel).is_err());
    cancel.store(false, Ordering::Relaxed);
    let mut corrupt = fs::read(&path).unwrap();
    let end = corrupt.len();
    corrupt[end - 8] ^= 0xff;
    fs::write(&path, corrupt).unwrap();
    assert!(archive::verify(&path, &path, &options, &cancel).is_err());
    let path = dir.path().join("link.tar");
    let mut tar = tar::Builder::new(fs::File::create(&path).unwrap());
    let mut header = tar::Header::new_gnu();
    header.set_size(0);
    header.set_mode(0o777);
    header.set_entry_type(tar::EntryType::Symlink);
    tar.append_link(&mut header, "evil", "../../outside")
        .unwrap();
    tar.finish().unwrap();
    assert!(archive::unpack(&path, &path, &options, &cancel).is_err());
    assert_eq!(fs::read_dir(&scratch).unwrap().count(), 0);
    let path = dir.path().join("file.txt.gz");
    let mut gzip = flate2::write::GzEncoder::new(
        fs::File::create(&path).unwrap(),
        flate2::Compression::fast(),
    );
    gzip.write_all(b"plain").unwrap();
    gzip.finish().unwrap();
    let entries = archive::unpack(&path, &path, &options, &cancel).unwrap();
    assert_eq!(entries[0].path, Path::new("file.txt"));
    assert_eq!(fs::read(entries[0].file.path()).unwrap(), b"plain");
}

#[test]
fn nested_archive_validation_cleans_scratch_without_mutating_install_trees() {
    let (dir, mut settings) = setup();
    let scratch = dir.path().join("scratch");
    fs::create_dir(&scratch).unwrap();
    settings.archive.scratch_dir = Some(scratch.clone());
    fs::write(
        settings.roots.mods.join("outer.zip"),
        zip(&[("inner.zip", &zip(&[("file.txt", b"nested")]))]),
    )
    .unwrap();
    let g = guide(serde_json::json!([
        {"Action":"Extract", "Source":"<<modDirectory>>/outer.zip"},
        {"Action":"Extract", "Source":"<<modDirectory>>/outer/inner.zip"},
        {"Action":"Copy", "Source":"<<modDirectory>>/outer/inner/file.txt", "Destination":"<<kotorDirectory>>/Override"}
    ]));
    Engine::new(settings.clone())
        .unwrap()
        .validate(&g, |_| {})
        .unwrap();
    assert_eq!(fs::read_dir(&scratch).unwrap().count(), 0);
    assert_eq!(fs::read_dir(&settings.roots.mods).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&settings.roots.game).unwrap().count(), 0);
    settings.archive.scratch_dir = Some(settings.roots.mods.clone());
    assert!(Engine::new(settings).is_err());
}

#[test]
fn cancelled_stream_copy_keeps_existing_destination() {
    use std::sync::atomic::AtomicBool;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("old");
    fs::write(&dest, b"original").unwrap();
    let cancelled = AtomicBool::new(false);
    struct CancelReader<'a>(&'a AtomicBool);
    impl std::io::Read for CancelReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            buffer[0] = 42;
            self.0.store(true, Ordering::Relaxed);
            Ok(1)
        }
    }
    assert!(recovery::atomic_copy(&dest, CancelReader(&cancelled), &cancelled).is_err());
    assert_eq!(fs::read(&dest).unwrap(), b"original");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
#[ignore = "Explicit streaming regression writes a 513 MiB file; run with --ignored"]
fn archive_and_copy_above_512_mib() {
    use std::{io::Read, sync::atomic::AtomicBool};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.gz");
    let size = 513 * 1024 * 1024;
    let mut gzip = flate2::write::GzEncoder::new(
        fs::File::create(&path).unwrap(),
        flate2::Compression::fast(),
    );
    std::io::copy(&mut std::io::repeat(0x5a).take(size), &mut gzip).unwrap();
    gzip.finish().unwrap();
    let cancelled = AtomicBool::new(false);
    let entries = archive::unpack(&path, &path, &archive::Options::default(), &cancelled).unwrap();
    assert_eq!(entries[0].file.as_file().metadata().unwrap().len(), size);
    let dest = dir.path().join("copy.bin");
    recovery::atomic_copy(
        &dest,
        fs::File::open(entries[0].file.path()).unwrap(),
        &cancelled,
    )
    .unwrap();
    assert_eq!(fs::metadata(&dest).unwrap().len(), size);
    let mut reader = fs::File::open(dest).unwrap();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buffer).unwrap();
        if n == 0 {
            break;
        }
        assert!(buffer[..n].iter().all(|byte| *byte == 0x5a));
    }
}

#[test]
fn cleanlist_matches_reference_substrings_tokens_and_stopwords() {
    let (_dir, settings) = setup();
    let rows = [
        ("Jedi Robes", "substring", true),
        ("\"Robes, Jedi\"", "unmatched", false),
        ("Robes Jedi Recolored HD", "tokens", true),
        ("Jedi Battle Armour", "one_shared", false),
        ("kotor mod pack", "stopwords", false),
        ("Mandatory Deletions - always", "mandatory", true),
        ("all", "not_a_magic_name", false),
    ];
    let mut csv = String::new();
    for (name, file, _) in rows {
        fs::write(settings.roots.game.join(file), b"keep unless selected").unwrap();
        csv.push_str(&format!("{name},{file}\n"));
    }
    fs::write(settings.roots.mods.join("clean.csv"), csv).unwrap();
    let mut g = guide(
        serde_json::json!([{"Action":"CleanList", "Source":"<<modDirectory>>/clean.csv", "Destination":"<<gameDirectory>>"}]),
    );
    g.components[0].name = "K1 Jedi Robes by Author".into();
    Engine::new(settings.clone())
        .unwrap()
        .install(&g, |_| {})
        .unwrap();
    for (_, file, deleted) in rows {
        assert_eq!(!settings.roots.game.join(file).exists(), deleted, "{file}");
    }
}

#[test]
fn remaining_install_retains_base_dependencies_without_replaying_base() {
    let (_dir, settings) = setup();
    fs::write(settings.roots.mods.join("base.txt"), b"original base").unwrap();
    fs::write(settings.roots.mods.join("wide.txt"), b"wide").unwrap();
    let mut g = Guide::parse(&serde_json::json!({"thisMod":[
        {"Guid":"base","Name":"Base","IsSelected":true,"Instructions":[{"Action":"Copy","Source":["<<modDirectory>>/base.txt"],"Destination":"<<gameDirectory>>/Override","Overwrite":true}]},
        {"Guid":"wide","Name":"Wide","IsSelected":false,"Dependencies":["base"],"WidescreenOnly":true,"Instructions":[{"Action":"Copy","Source":["<<modDirectory>>/wide.txt"],"Destination":"<<gameDirectory>>/Override"}]}
    ]}).to_string(), "json").unwrap();
    Engine::new(settings.clone())
        .unwrap()
        .install(&g, |_| {})
        .unwrap();
    fs::write(settings.roots.mods.join("base.txt"), b"must not replay").unwrap();
    g.components[1].is_selected = true;
    let mut events = Vec::new();
    Engine::new(settings.clone())
        .unwrap()
        .install_remaining(&g, &["BASE".into()], |e| events.push(e))
        .unwrap();
    assert_eq!(
        fs::read(settings.roots.game.join("Override/base.txt")).unwrap(),
        b"original base"
    );
    assert_eq!(
        fs::read(settings.roots.game.join("Override/wide.txt")).unwrap(),
        b"wide"
    );
    assert!(!events.iter().any(|e| e.component == "Base"));
    for invalid in [
        vec!["wide".into()],
        vec!["missing".into()],
        vec!["base".into(), "BASE".into()],
    ] {
        assert!(
            Engine::new(settings.clone())
                .unwrap()
                .install_remaining(&g, &invalid, |_| {})
                .is_err()
        );
    }
}

#[test]
fn remaining_phase_failure_restores_completed_base_before_ordered_retry() {
    let (_dir, settings) = setup();
    fs::create_dir(settings.roots.mods.join("wide")).unwrap();
    fs::write(settings.roots.mods.join("base.txt"), b"installed base").unwrap();
    fs::write(settings.roots.mods.join("wide/base.txt"), b"wide overwrite").unwrap();
    fs::write(settings.roots.mods.join("later.txt"), b"later").unwrap();
    let mut g = Guide::parse(&serde_json::json!({"thisMod":[
        {"Guid":"base","Name":"Base","IsSelected":true,"Instructions":[{"Action":"Copy","Source":["<<modDirectory>>/base.txt"],"Destination":"<<gameDirectory>>/Override","Overwrite":true}]},
        {"Guid":"wide","Name":"Wide","IsSelected":false,"Dependencies":["base"],"Instructions":[
            {"Action":"Copy","Source":["<<modDirectory>>/wide/base.txt"],"Destination":"<<gameDirectory>>/Override","Overwrite":true},
            {"Action":"Copy","Source":["<<modDirectory>>/later.txt"],"Destination":"<<gameDirectory>>/Override"}
        ]}
    ]}).to_string(), "json").unwrap();
    Engine::new(settings.clone())
        .unwrap()
        .install(&g, |_| {})
        .unwrap();
    fs::write(settings.roots.mods.join("base.txt"), b"do not replay").unwrap();
    g.components[1].is_selected = true;
    let completed = vec!["base".to_owned()];
    let cancelled = settings.cancelled.clone();
    let error = Engine::new(settings.clone())
        .unwrap()
        .install_remaining(&g, &completed, |e| {
            if e.component == "Wide" && e.step == 1 {
                cancelled.store(true, Ordering::Relaxed);
            }
        })
        .unwrap_err();
    assert!(format!("{error:#}").contains("Cancelled"));
    assert_eq!(
        fs::read(settings.roots.game.join("Override/base.txt")).unwrap(),
        b"wide overwrite"
    );
    assert!(!settings.roots.game.join("Override/later.txt").exists());
    settings.cancelled.store(false, Ordering::Relaxed);
    assert!(
        Engine::new(settings.clone())
            .unwrap()
            .install_remaining(&g, &completed, |_| {})
            .is_err()
    );
    recovery::restore(&settings.roots).unwrap();
    assert_eq!(
        fs::read(settings.roots.game.join("Override/base.txt")).unwrap(),
        b"installed base"
    );
    assert_eq!(
        fs::read(settings.roots.mods.join("base.txt")).unwrap(),
        b"do not replay"
    );
    assert!(!settings.roots.game.join("Override/later.txt").exists());
    let mut steps = Vec::new();
    Engine::new(settings.clone())
        .unwrap()
        .install_remaining(&g, &completed, |e| {
            if !e.component.is_empty() {
                steps.push((e.component, e.step));
            }
        })
        .unwrap();
    assert_eq!(steps, vec![("Wide".into(), 1), ("Wide".into(), 2)]);
    assert_eq!(
        fs::read(settings.roots.game.join("Override/base.txt")).unwrap(),
        b"wide overwrite"
    );
    assert_eq!(
        fs::read(settings.roots.game.join("Override/later.txt")).unwrap(),
        b"later"
    );
    assert!(
        !recovery::state_dir(&settings.roots)
            .join("pending")
            .exists()
    );
}

#[test]
fn remaining_validation_skips_consumed_base_sources_and_does_not_write() {
    let (_dir, settings) = setup();
    fs::write(settings.roots.mods.join("base.txt"), b"base").unwrap();
    let mut g = Guide::parse(&serde_json::json!({"thisMod":[
        {"Guid":"base","Name":"Base","IsSelected":true,"Instructions":[{"Action":"Move","Source":["<<modDirectory>>/base.txt"],"Destination":"<<gameDirectory>>/Override"}]},
        {"Guid":"wide","Name":"Wide","IsSelected":false,"Dependencies":["base"],"Instructions":[{"Action":"Copy","Source":["<<gameDirectory>>/Override/base.txt"],"Destination":"<<gameDirectory>>/wide"}]}
    ]}).to_string(), "json").unwrap();
    Engine::new(settings.clone())
        .unwrap()
        .install(&g, |_| {})
        .unwrap();
    assert!(!settings.roots.mods.join("base.txt").exists());
    let snapshot_before =
        fs::read(recovery::state_dir(&settings.roots).join("snapshot.json")).unwrap();
    g.components[1].is_selected = true;
    assert!(
        Engine::new(settings.clone())
            .unwrap()
            .validate(&g, |_| {})
            .is_err()
    );
    let mut events = Vec::new();
    Engine::new(settings.clone())
        .unwrap()
        .validate_remaining(&g, &["base".into()], |e| events.push(e))
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].component, "Wide");
    assert!(!settings.roots.game.join("wide").exists());
    assert!(!settings.roots.mods.join("base.txt").exists());
    assert_eq!(
        fs::read(settings.roots.game.join("Override/base.txt")).unwrap(),
        b"base"
    );
    assert_eq!(
        fs::read(recovery::state_dir(&settings.roots).join("snapshot.json")).unwrap(),
        snapshot_before
    );
    assert!(
        !recovery::state_dir(&settings.roots)
            .join("pending")
            .exists()
    );
    assert!(
        Engine::new(settings)
            .unwrap()
            .validate_remaining(&g, &["wide".into()], |_| {})
            .is_err()
    );
}

#[test]
fn remaining_conflict_analysis_skips_base_writes_and_keeps_both_trees_unchanged() {
    let (_dir, settings) = setup();
    fs::write(settings.roots.mods.join("base.txt"), b"base").unwrap();
    fs::write(settings.roots.mods.join("wide.txt"), b"wide").unwrap();
    let mut g = Guide::parse(&serde_json::json!({"thisMod":[
        {"Guid":"base","Name":"Base","IsSelected":true,"Instructions":[{"Action":"Move","Source":["<<modDirectory>>/base.txt"],"Destination":"<<gameDirectory>>/Override"}]},
        {"Guid":"wide","Name":"Wide","IsSelected":false,"Dependencies":["base"],"Instructions":[{"Action":"Copy","Source":["<<modDirectory>>/wide.txt"],"Destination":"<<gameDirectory>>/Override"}]}
    ]}).to_string(), "json").unwrap();
    Engine::new(settings.clone())
        .unwrap()
        .install(&g, |_| {})
        .unwrap();
    g.components[1].is_selected = true;
    let before_game = fs::read(settings.roots.game.join("Override/base.txt")).unwrap();
    let report = Engine::new(settings.clone())
        .unwrap()
        .analyze_conflicts_remaining(&g, &["base".into()])
        .unwrap();
    assert_eq!(report.analyzed_component_count, 1);
    assert!(
        report.conflicts.is_empty(),
        "base writes are outside the remaining phase"
    );
    assert_eq!(
        fs::read(settings.roots.game.join("Override/base.txt")).unwrap(),
        before_game
    );
    assert!(settings.roots.mods.join("wide.txt").exists());
    assert!(
        Engine::new(settings)
            .unwrap()
            .analyze_conflicts_remaining(&g, &["wide".into()])
            .is_err()
    );
}

#[test]
fn saved_completion_rejects_missing_changed_or_corrupt_base_evidence() {
    let (_dir, settings) = setup();
    fs::write(settings.roots.mods.join("sample.tga"), b"base").unwrap();
    let mut g = guide(serde_json::json!([copy()]));
    let mut tail = g.components[0].clone();
    tail.guid = "tail".into();
    tail.name = "Tail".into();
    tail.dependencies = vec!["a".into()];
    g.components.push(tail);
    let completed = vec!["a".to_owned()];
    assert!(
        Engine::new(settings.clone())
            .unwrap()
            .validate_remaining(&g, &completed, |_| {})
            .unwrap_err()
            .to_string()
            .contains("No saved completion")
    );
    g.components[1].is_selected = false;
    Engine::new(settings.clone())
        .unwrap()
        .install(&g, |_| {})
        .unwrap();
    g.components[1].is_selected = true;
    let receipt_path = recovery::state_dir(&settings.roots).join("completion.json");
    let receipt = fs::read(&receipt_path).unwrap();
    g.components[0].name = "Changed".into();
    assert!(
        Engine::new(settings.clone())
            .unwrap()
            .validate_remaining(&g, &completed, |_| {})
            .unwrap_err()
            .to_string()
            .contains("definitions differ")
    );
    g.components[0].name = "Test".into();
    fs::write(settings.roots.game.join("Override/sample.tga"), b"tampered").unwrap();
    assert!(
        Engine::new(settings.clone())
            .unwrap()
            .install_remaining(&g, &completed, |_| {})
            .unwrap_err()
            .to_string()
            .contains("game files differ")
    );
    assert!(
        !recovery::state_dir(&settings.roots)
            .join("pending")
            .exists()
    );
    fs::write(settings.roots.game.join("Override/sample.tga"), b"base").unwrap();
    fs::write(&receipt_path, b"broken json").unwrap();
    assert!(
        Engine::new(settings.clone())
            .unwrap()
            .validate_remaining(&g, &completed, |_| {})
            .is_err()
    );
    fs::write(receipt_path, receipt).unwrap();
    Engine::new(settings)
        .unwrap()
        .validate_remaining(&g, &completed, |_| {})
        .unwrap();
}

#[test]
fn fomod_detection_requires_exact_directory_segment_before_refusing_extraction() {
    for (path, blocked) in [
        ("nested/FoMoD/ModuleConfig.XML", true),
        ("notfomod/moduleconfig.xml", false),
    ] {
        let (_dir, settings) = setup();
        fs::write(
            settings.roots.mods.join("mod.zip"),
            zip(&[(path, b"<config/>")]),
        )
        .unwrap();
        let g = guide(serde_json::json!([
            {"Action":"Extract", "Source":"<<modDirectory>>/mod.zip"}
        ]));
        let result = Engine::new(settings.clone()).unwrap().validate(&g, |_| {});
        if blocked {
            assert!(format!("{:#}", result.unwrap_err()).contains("FOMOD"));
        } else {
            result.unwrap();
        }
        assert_eq!(fs::read_dir(&settings.roots.mods).unwrap().count(), 1);
        assert_eq!(fs::read_dir(&settings.roots.game).unwrap().count(), 0);
    }
    for path in [
        "fomod/ModuleConfig.xml",
        r"root\FOMOD\moduleconfig.xml",
        "/root/fomod/moduleconfig.xml/",
    ] {
        assert!(modsync::fomod::is_module_config_path(path));
    }
    for path in [
        "",
        "ModuleConfig.xml",
        "notfomod/ModuleConfig.xml",
        "fomod//ModuleConfig.xml",
        "fomod/ModuleConfig.xml.bak",
    ] {
        assert!(!modsync::fomod::is_module_config_path(path));
    }
}

#[test]
fn configured_fomod_installs_selected_bytes_through_shared_engine() {
    let (_dir, settings) = setup();
    fs::write(settings.roots.mods.join("package.zip"), zip(&[
        ("Wrapper/fomod/ModuleConfig.xml", br#"<config><installSteps><installStep name="Step"><optionalFileGroups><group name="Group" type="SelectExactlyOne"><plugins><plugin name="First"><files><file source="first.txt" destination="Override"/></files></plugin><plugin name="Second"><files><file source="second.txt" destination="Override"/></files></plugin></plugins></group></optionalFileGroups></installStep></installSteps></config>"#),
        ("Wrapper/first.txt", b"first"), ("Wrapper/second.txt", b"second")
    ])).unwrap();
    let mut g =
        guide(serde_json::json!([{"Action":"Extract","Source":"<<modDirectory>>/package.zip"}]));
    assert!(
        Engine::new(settings.clone())
            .unwrap()
            .validate(&g, |_| {})
            .is_err()
    );
    g.components[0].metadata.insert(
        "FomodChoices".into(),
        serde_json::json!({
            "version":1,"archives":[{"archiveFileName":"package.zip","selections":[
                {"stepName":"Step","groupName":"Group","plugins":["Second"]}
            ]}]
        }),
    );
    Engine::new(settings.clone())
        .unwrap()
        .validate(&g, |_| {})
        .unwrap();
    assert_eq!(fs::read_dir(&settings.roots.mods).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&settings.roots.game).unwrap().count(), 0);
    fs::write(settings.roots.game.join("keep.dat"), b"baseline").unwrap();
    let archive_before = fs::read(settings.roots.mods.join("package.zip")).unwrap();
    g.components[0].instructions.push(
        serde_json::from_value(serde_json::json!({
            "Action":"Delete", "Source":"<<kotorDirectory>>/keep.dat"
        }))
        .unwrap(),
    );
    let cancel = settings.cancelled.clone();
    let failed = Engine::new(settings.clone()).unwrap().install(&g, |event| {
        if event.component == "Test" && event.step == 1 {
            cancel.store(true, Ordering::Relaxed);
        }
    });
    assert!(failed.is_err());
    assert!(settings.roots.game.join("Override/second.txt").exists());
    assert_eq!(
        fs::read(settings.roots.game.join("keep.dat")).unwrap(),
        b"baseline"
    );
    settings.cancelled.store(false, Ordering::Relaxed);
    assert!(
        Engine::new(settings.clone())
            .unwrap()
            .install(&g, |_| {})
            .is_err()
    );
    recovery::restore(&settings.roots).unwrap();
    assert_eq!(fs::read_dir(&settings.roots.mods).unwrap().count(), 1);
    assert_eq!(
        fs::read(settings.roots.mods.join("package.zip")).unwrap(),
        archive_before
    );
    assert_eq!(fs::read_dir(&settings.roots.game).unwrap().count(), 1);
    assert_eq!(
        fs::read(settings.roots.game.join("keep.dat")).unwrap(),
        b"baseline"
    );
    Engine::new(settings.clone())
        .unwrap()
        .install(&g, |_| {})
        .unwrap();
    assert_eq!(
        fs::read(settings.roots.game.join("Override/second.txt")).unwrap(),
        b"second"
    );
    assert!(!settings.roots.game.join("keep.dat").exists());
    assert!(
        !recovery::state_dir(&settings.roots)
            .join("pending")
            .exists()
    );
    assert!(!settings.roots.game.join("Override/first.txt").exists());
}

#[test]
fn cli_fomod_choices_validate_and_install_without_rewriting_guide() {
    let (dir, settings) = setup();
    fs::write(settings.roots.mods.join("package.zip"), zip(&[
        ("fomod/ModuleConfig.xml", br#"<config><installSteps><installStep name="Style"><optionalFileGroups><group name="Texture" type="SelectExactlyOne"><plugins><plugin name="Default"><files><file source="default.txt" destination="Override"/></files></plugin><plugin name="Alternate"><files><file source="asset.txt" destination="Override"/></files></plugin></plugins></group></optionalFileGroups></installStep></installSteps></config>"#),
        ("asset.txt", b"configured asset"), ("default.txt", b"default asset")
    ])).unwrap();
    let g =
        guide(serde_json::json!([{"Action":"Extract","Source":"<<modDirectory>>/package.zip"}]));
    let input = dir.path().join("guide.json");
    let original = g.render("json").unwrap();
    fs::write(&input, &original).unwrap();
    let choices = dir.path().join("choices.json");
    let archive_before = fs::read(settings.roots.mods.join("package.zip")).unwrap();
    let mut interactive = std::process::Command::new(env!("CARGO_BIN_EXE_modsync"))
        .arg("fomod-inspect")
        .arg(settings.roots.mods.join("package.zip"))
        .arg("--interactive")
        .arg("--save-choices")
        .arg(&choices)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    interactive.stdin.take().unwrap().write_all(b"2\n").unwrap();
    let output = interactive.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["steps"][0]["groups"][0]["plugins"][1]["selected"],
        true
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("Numbers separated"));
    assert_eq!(
        fs::read(settings.roots.mods.join("package.zip")).unwrap(),
        archive_before
    );
    let run = |action: &str| {
        std::process::Command::new(env!("CARGO_BIN_EXE_modsync"))
            .arg(action)
            .arg("--input")
            .arg(&input)
            .arg("--source-dir")
            .arg(&settings.roots.mods)
            .arg("--game-dir")
            .arg(&settings.roots.game)
            .arg("--fomod-choices")
            .arg(&choices)
            .output()
            .unwrap()
    };
    let output = run("validate");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_dir(&settings.roots.mods).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&settings.roots.game).unwrap().count(), 0);
    let output = run("install");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(settings.roots.game.join("Override/asset.txt")).unwrap(),
        b"configured asset"
    );
    assert!(!settings.roots.game.join("Override/default.txt").exists());
    assert_eq!(fs::read_to_string(&input).unwrap(), original);
    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(recovery::state_dir(&settings.roots).join("completion.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        receipt["components"][0]["FomodChoices"]["archives"][0]["selections"][0]["plugins"],
        serde_json::json!(["Alternate"])
    );
    assert_eq!(
        receipt["components"][0]["FomodChoices"]["archives"][0]["archiveFileName"],
        "package.zip"
    );
}

#[test]
fn configured_fomod_never_uses_stale_extraction_sources() {
    for (directive, extra, overwrite) in [
        (
            "<file source='missing.txt' destination='Override'/>",
            None,
            true,
        ),
        (
            "<folder source='' destination='Override'/>",
            Some("present.txt"),
            true,
        ),
        (
            "<file source='missing.txt' destination='Override'/>",
            Some("missing.txt"),
            false,
        ),
    ] {
        let (_dir, settings) = setup();
        let xml =
            format!("<config><requiredInstallFiles>{directive}</requiredInstallFiles></config>");
        let mut entries = vec![("fomod/ModuleConfig.xml", xml.as_bytes())];
        if let Some(name) = extra {
            entries.push((name, b"new"));
        }
        fs::write(settings.roots.mods.join("package.zip"), zip(&entries)).unwrap();
        fs::create_dir(settings.roots.mods.join("package")).unwrap();
        fs::write(settings.roots.mods.join("package/missing.txt"), b"stale").unwrap();
        let mut g = guide(serde_json::json!([{"Action":"Extract",
            "Source":"<<modDirectory>>/package.zip", "Overwrite":overwrite}]));
        g.components[0].metadata.insert("FomodChoices".into(),
            serde_json::json!({"version":1,"archives":[{"archiveFileName":"package.zip","selections":[]}]}));
        assert!(
            Engine::new(settings.clone())
                .unwrap()
                .install(&g, |_| {})
                .is_err()
        );
        assert_eq!(fs::read_dir(&settings.roots.game).unwrap().count(), 0);
        assert_eq!(
            fs::read(settings.roots.mods.join("package/missing.txt")).unwrap(),
            b"stale"
        );
        assert!(!settings.roots.mods.join("package/fomod").exists());
        assert!(
            !recovery::state_dir(&settings.roots)
                .join("pending")
                .exists()
        );
    }
}

#[test]
fn validation_report_has_ordered_shared_engine_stages() {
    let (_dir, settings) = setup();
    fs::write(settings.roots.mods.join("sample.tga"), b"content").unwrap();
    let report = Engine::new(settings)
        .unwrap()
        .validate_report(&guide(serde_json::json!([copy()])), |_| {});
    assert!(report.success);
    assert_eq!(report.error_count, 0);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["stages"][0]["stage"], "Environment");
    assert!(json.get("inputPath").is_none());
    assert!(json["stages"][0].get("messages").is_none());
    assert_eq!(
        report
            .stages
            .iter()
            .map(|stage| stage.stage)
            .collect::<Vec<_>>(),
        vec![
            modsync::validation::Stage::Environment,
            modsync::validation::Stage::Conflicts,
            modsync::validation::Stage::InstallOrder,
            modsync::validation::Stage::ComponentValidation,
            modsync::validation::Stage::FomodConfiguration,
        ]
    );
}

#[test]
fn validation_report_keeps_component_failure_as_structured_error() {
    let (_dir, settings) = setup();
    let report = Engine::new(settings).unwrap().validate_report(
        &guide(serde_json::json!([{"Action":"Copy", "Source":"<<modDirectory>>/missing.tga", "Destination":"<<kotorDirectory>>/Override"}])),
        |_| {},
    );
    assert!(!report.success);
    assert_eq!(report.exit_code, 1);
    assert_eq!(report.error_count, 2);
    assert!(
        !report
            .stages
            .iter()
            .find(|stage| stage.stage == modsync::validation::Stage::Conflicts)
            .unwrap()
            .passed
    );
    let component = report
        .stages
        .iter()
        .find(|stage| stage.stage == modsync::validation::Stage::ComponentValidation)
        .unwrap();
    assert!(!component.passed);
    assert!(!component.messages.is_empty());
}

#[test]
fn validation_report_gates_registered_fomod_without_extract_instruction() {
    let (_dir, settings) = setup();
    fs::create_dir(settings.roots.mods.join("nested")).unwrap();
    fs::write(
        settings.roots.mods.join("nested/package.zip"),
        zip(&[("fomod/ModuleConfig.xml", b"<config/>")]),
    )
    .unwrap();
    let guide = Guide::parse(
        r#"{"thisMod":[{"Guid":"a","Name":"Registered","IsSelected":true,"ResourceRegistry":{"source":{"Files":{"nested/package.zip":true}}}}]}"#,
        "json",
    )
    .unwrap();
    let report = Engine::new(settings)
        .unwrap()
        .validate_report(&guide, |_| {});
    let stage = report
        .stages
        .iter()
        .find(|stage| stage.stage == modsync::validation::Stage::FomodConfiguration)
        .unwrap();
    assert!(!stage.passed);
    assert!(
        stage
            .messages
            .iter()
            .any(|message| message.contains("package.zip"))
    );
}

#[test]
fn validation_report_rejects_missing_registered_archive_after_fomod_state() {
    let (_dir, settings) = setup();
    let guide = Guide::parse(
        r#"{"thisMod":[{"Guid":"a","Name":"Registered","IsSelected":true,"ResourceRegistry":{"source":{"Files":{"nested/package.zip":true},"HandlerMetadata":{"fomodPromptStatus":{"package.zip":"configured"}}}}}]}"#,
        "json",
    )
    .unwrap();
    let report = Engine::new(settings)
        .unwrap()
        .validate_report(&guide, |_| {});
    let stage = report
        .stages
        .iter()
        .find(|stage| stage.stage == modsync::validation::Stage::FomodConfiguration)
        .unwrap();
    assert!(!stage.passed);
    assert!(
        stage
            .messages
            .iter()
            .any(|message| message.contains("missing"))
    );
}

#[test]
fn validation_report_rejects_unreadable_registered_archive() {
    let (_dir, settings) = setup();
    fs::write(settings.roots.mods.join("package.zip"), b"not an archive").unwrap();
    let guide = Guide::parse(
        r#"{"thisMod":[{"Guid":"a","Name":"Registered","IsSelected":true,"ResourceRegistry":{"source":{"Files":{"package.zip":true}}}}]}"#,
        "json",
    )
    .unwrap();
    let report = Engine::new(settings)
        .unwrap()
        .validate_report(&guide, |_| {});
    let stage = report
        .stages
        .iter()
        .find(|stage| stage.stage == modsync::validation::Stage::FomodConfiguration)
        .unwrap();
    assert!(!stage.passed);
    assert!(
        stage
            .messages
            .iter()
            .any(|message| message.contains("cannot be inspected"))
    );
}

#[test]
fn validation_report_rejects_unconfigured_fomod_archive() {
    let (_dir, settings) = setup();
    fs::write(
        settings.roots.mods.join("package.zip"),
        zip(&[("fomod/ModuleConfig.xml", b"<config/>")]),
    )
    .unwrap();
    let report = Engine::new(settings).unwrap().validate_report(
        &guide(serde_json::json!([{"Action":"Extract", "Source":"<<modDirectory>>/package.zip"}])),
        |_| {},
    );
    let stage = report
        .stages
        .iter()
        .find(|stage| stage.stage == modsync::validation::Stage::FomodConfiguration)
        .unwrap();
    assert!(!stage.passed);
    assert!(stage.messages[0].contains("no explicit FomodChoices"));
}

#[test]
fn validation_report_dry_run_is_structured_and_does_not_write() {
    let (_dir, settings) = setup();
    fs::write(settings.roots.mods.join("sample.tga"), b"content").unwrap();
    let game = settings.roots.game.clone();
    let report = Engine::new(settings).unwrap().validate_report_with_dry_run(
        &guide(serde_json::json!([copy()])),
        |_| {},
        true,
    );
    assert!(report.success);
    assert!(report.dry_run.as_ref().unwrap().is_valid);
    assert_eq!(
        report.stages.last().unwrap().stage,
        modsync::validation::Stage::DryRun
    );
    assert_eq!(
        serde_json::to_value(&report).unwrap()["dryRun"]["isValid"],
        true
    );
    assert_eq!(
        serde_json::to_value(&report).unwrap()["dryRun"]["issues"],
        serde_json::json!([])
    );
    assert!(fs::read_dir(game).unwrap().next().is_none());
}

#[test]
fn validation_report_dry_run_identifies_the_failing_instruction() {
    let (_dir, settings) = setup();
    let report = Engine::new(settings).unwrap().validate_report_with_dry_run(
        &guide(serde_json::json!([{"Action":"Copy", "Source":"<<modDirectory>>/missing.tga", "Destination":"<<kotorDirectory>>/Override"}])),
        |_| {},
        true,
    );
    let issue = &report.dry_run.unwrap().issues[0];
    assert_eq!(issue.component_name.as_deref(), Some("Test"));
    assert_eq!(issue.component_guid.as_deref(), Some("a"));
    assert_eq!(issue.instruction_index, Some(1));
    let json = serde_json::to_value(issue).unwrap();
    assert_eq!(json["componentName"], "Test");
    assert_eq!(json["componentGuid"], "a");
    assert_eq!(json["instructionIndex"], 1);
    assert_eq!(json["affectedPath"], "<<modDirectory>>/missing.tga");
    assert!(json.get("solution").is_none());
}

#[test]
fn validation_report_dry_run_collects_failures_across_components() {
    let (_dir, settings) = setup();
    let guide = Guide::parse(
        &serde_json::json!({"thisMod": [
            {"Guid":"a", "Name":"First", "IsSelected":true, "Instructions":[{"Action":"Copy", "Source":"<<modDirectory>>/missing-a.tga", "Destination":"<<kotorDirectory>>/Override"}]},
            {"Guid":"b", "Name":"Second", "IsSelected":true, "Instructions":[{"Action":"Copy", "Source":"<<modDirectory>>/missing-b.tga", "Destination":"<<kotorDirectory>>/Override"}]}
        ]})
        .to_string(),
        "json",
    )
    .unwrap();
    let report = Engine::new(settings)
        .unwrap()
        .validate_report_with_dry_run(&guide, |_| {}, true);
    let issues = &report.dry_run.as_ref().unwrap().issues;
    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].component_name.as_deref(), Some("First"));
    assert_eq!(issues[1].component_name.as_deref(), Some("Second"));
    assert_eq!(issues[0].instruction_index, Some(1));
    assert_eq!(issues[1].instruction_index, Some(1));
}

#[test]
fn validation_report_dry_run_collects_failures_across_instructions() {
    let (_dir, settings) = setup();
    let report = Engine::new(settings).unwrap().validate_report_with_dry_run(
        &guide(serde_json::json!([
            {"Action":"Copy", "Source":"<<modDirectory>>/missing-a.tga", "Destination":"<<kotorDirectory>>/Override"},
            {"Action":"Copy", "Source":"<<modDirectory>>/missing-b.tga", "Destination":"<<kotorDirectory>>/Override"}
        ])),
        |_| {},
        true,
    );
    let issues = &report.dry_run.as_ref().unwrap().issues;
    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].instruction_index, Some(1));
    assert_eq!(issues[1].instruction_index, Some(2));
    assert_eq!(
        issues[0].affected_path.as_deref(),
        Some("<<modDirectory>>/missing-a.tga")
    );
    assert_eq!(
        issues[1].affected_path.as_deref(),
        Some("<<modDirectory>>/missing-b.tga")
    );
}

#[test]
fn validation_report_dry_run_collects_reference_file_warnings() {
    let (_dir, settings) = setup();
    fs::write(settings.roots.mods.join("sample.tga"), b"new").unwrap();
    fs::create_dir(settings.roots.game.join("Override")).unwrap();
    fs::write(settings.roots.game.join("Override/sample.tga"), b"existing").unwrap();
    let report = Engine::new(settings.clone())
        .unwrap()
        .validate_report_with_dry_run(
            &guide(serde_json::json!([
                {"Action":"Copy", "Source":"<<modDirectory>>/sample.tga", "Destination":"<<kotorDirectory>>/Override", "Overwrite":false},
                {"Action":"Delete", "Source":"<<kotorDirectory>>/missing.tga"}
            ])),
            |_| {},
            true,
        );
    let dry_run = report.dry_run.unwrap();
    assert!(dry_run.is_valid && dry_run.has_warnings);
    assert_eq!(dry_run.issues.len(), 2);
    assert_eq!(dry_run.issues[0].severity, "Warning");
    assert_eq!(dry_run.issues[0].category, "CopyFile");
    assert!(dry_run.issues[0].message.contains("overwrite is false"));
    assert_eq!(dry_run.issues[1].category, "DeleteFile");
    assert!(dry_run.issues[1].message.contains("non-existent file"));
    assert_eq!(
        fs::read(settings.roots.game.join("Override/sample.tga")).unwrap(),
        b"existing"
    );
}

#[test]
fn overwrite_false_and_missing_delete_remain_nonfatal_noops() {
    let (_dir, settings) = setup();
    fs::write(settings.roots.mods.join("source.txt"), b"source").unwrap();
    fs::write(settings.roots.mods.join("target.txt"), b"target").unwrap();
    let guide = guide(serde_json::json!([
        {"Action":"Rename", "Source":"<<modDirectory>>/source.txt", "Destination":"target.txt", "Overwrite":false},
        {"Action":"Delete", "Source":"<<kotorDirectory>>/missing.txt"}
    ]));
    Engine::new(settings.clone())
        .unwrap()
        .validate(&guide, |_| {})
        .unwrap();
    Engine::new(settings.clone())
        .unwrap()
        .install(&guide, |_| {})
        .unwrap();
    assert_eq!(
        fs::read(settings.roots.mods.join("source.txt")).unwrap(),
        b"source"
    );
    assert_eq!(
        fs::read(settings.roots.mods.join("target.txt")).unwrap(),
        b"target"
    );
}

#[test]
fn cli_validation_report_emits_structured_error_when_setup_fails() {
    let (dir, settings) = setup();
    // A guide path that does not exist fails before any pipeline stage runs.
    let missing = dir.path().join("does-not-exist.json");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_modsync"))
        .arg("validate")
        .arg("--input")
        .arg(&missing)
        .arg("--game-dir")
        .arg(&settings.roots.game)
        .arg("--source-dir")
        .arg(&settings.roots.mods)
        .args(["--use-file-selection", "--validation-report"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    // The structured error report is the sole machine output on stdout; stderr
    // carries no extra human error line, matching the C# SerializeError contract.
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["success"], false);
    assert_eq!(report["exitCode"], 1);
    assert_eq!(report["errorCount"], 0);
    assert_eq!(report["warningCount"], 0);
    assert_eq!(report["passedCount"], 0);
    assert_eq!(report["componentCount"], 0);
    assert!(report["error"].as_str().is_some_and(|e| !e.is_empty()));
    // inputPath, stages, and dryRun are omitted for the error form.
    let object = report.as_object().unwrap();
    assert!(!object.contains_key("stages"));
    assert!(!object.contains_key("dryRun"));
    assert!(!object.contains_key("inputPath"));
}
