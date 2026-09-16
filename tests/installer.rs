use modsync::{Engine, Guide, Settings, archive, paths::Roots, recovery};
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
        archive::unpack(&zip(&[("folder/a.txt", b"ok")])).unwrap()[0].1,
        b"ok"
    );
    for name in [
        "../escape",
        "/absolute",
        "C:/windows",
        "..\\escape",
        "foo/../../escape",
    ] {
        assert!(archive::unpack(&zip(&[(name, b"bad")])).is_err(), "{name}");
    }
    assert!(archive::unpack(&zip(&[("a", b"a"), ("A", b"b")])).is_err());
    assert!(archive::unpack(b"<html>Access denied</html>").is_err());
    let mut corrupt = zip(&[("test.txt", b"check crc")]);
    corrupt[38] ^= 1;
    assert!(archive::unpack(&corrupt).is_err());
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    z.add_symlink(
        "link",
        "../../outside",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    assert!(archive::unpack(&z.finish().unwrap().into_inner()).is_err());
}

#[test]
fn seven_zip_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("source.txt"), b"seven zip").unwrap();
    sevenz_rust2::compress_to_path(dir.path().join("source.txt"), dir.path().join("archive.7z"))
        .unwrap();
    let files = archive::unpack(&fs::read(dir.path().join("archive.7z")).unwrap()).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].1, b"seven zip");
}

#[test]
fn rar_reads_and_checks_crc_without_extracting_paths() {
    let files = archive::unpack(include_bytes!("fixtures/version.rar")).unwrap();
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
    fs::write(&stub, "#!/bin/sh\n[ \"$(cat \"$2/tslpatchdata/changes.ini\")\" = alternate ] || exit 3\n[ ! -f \"$2/tslpatchdata/namespaces.ini\" ] || exit 4\nprintf patched > \"$4/patched.txt\"\nexit 7\n").unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    settings.patcher = Some(stub);
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
