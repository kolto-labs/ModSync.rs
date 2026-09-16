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
