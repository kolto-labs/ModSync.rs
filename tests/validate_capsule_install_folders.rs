#![cfg(unix)]

use modsync::{Engine, Guide, Settings};
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

struct Fixture {
    _temp: tempfile::TempDir,
    settings: Settings,
}

fn fixture(folders: &[&str]) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("mods")).unwrap();
    fs::create_dir(temp.path().join("game")).unwrap();
    let mut settings = Settings::new(&temp.path().join("mods"), &temp.path().join("game")).unwrap();
    let data = settings.roots.mods.join("patch/tslpatchdata");
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join("thing.utc"), "payload").unwrap();
    fs::write(data.join("changes.ini"), listing(folders)).unwrap();
    let executable = temp.path().join("patcher");
    fs::write(&executable, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    settings.patcher = Some(executable);
    settings.patcher_engine = modsync::PatcherEngine::OdyPatcher;
    settings.allow_processes = true;
    Fixture {
        _temp: temp,
        settings,
    }
}

fn listing(folders: &[&str]) -> String {
    let mut ini = String::from("[InstallList]\n");
    for (index, folder) in folders.iter().enumerate() {
        ini.push_str(&format!("install_folder{index}={folder}\n"));
    }
    for (index, _) in folders.iter().enumerate() {
        ini.push_str(&format!("[install_folder{index}]\nFile0=thing.utc\n"));
    }
    ini
}

fn validate(fixture: &Fixture) -> modsync::validation::Report {
    let guide = Guide::parse(r#"{"thisMod":[{"Guid":"a","Name":"A","IsSelected":true,"Instructions":[{"Action":"Patcher","Source":"<<modDirectory>>/patch"}]}]}"#, "json").unwrap();
    Engine::new(fixture.settings.clone())
        .unwrap()
        .validate_report_with_options_observed(&guide, |_| {}, true, true, false, |_| {})
}

fn simulation_warnings(report: &modsync::validation::Report) -> Vec<String> {
    report
        .dry_run
        .as_ref()
        .map(|dry| {
            dry.issues
                .iter()
                .filter(|issue| issue.category == "PatcherSimulation")
                .map(|issue| issue.message.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn game_file(fixture: &Fixture, relative: &str) -> std::path::PathBuf {
    let path = fixture.settings.roots.game.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    path
}

fn assert_untouched_file(path: &Path, content: &[u8]) {
    assert!(path.is_file());
    assert_eq!(fs::read(path).unwrap(), content);
}

#[test]
fn present_capsule_is_left_unchanged_without_warning() {
    let fixture = fixture(&["Modules/sample_a.mod"]);
    let capsule = game_file(&fixture, "Modules/sample_a.mod");
    fs::write(&capsule, "capsule").unwrap();
    let report = validate(&fixture);
    assert!(report.success, "{report:?}");
    assert!(simulation_warnings(&report).is_empty(), "{report:?}");
    assert_untouched_file(&capsule, b"capsule");
}

#[test]
fn mixed_case_capsule_suffix_is_a_file() {
    let fixture = fixture(&["Modules/Sample_B.MoD", "Modules/sample_c.RIM"]);
    for name in ["Modules/Sample_B.MoD", "Modules/sample_c.RIM"] {
        fs::write(game_file(&fixture, name), "capsule").unwrap();
    }
    let report = validate(&fixture);
    assert!(report.success, "{report:?}");
    assert!(simulation_warnings(&report).is_empty(), "{report:?}");
    for name in ["Modules/Sample_B.MoD", "Modules/sample_c.RIM"] {
        assert_untouched_file(&fixture.settings.roots.game.join(name), b"capsule");
    }
}

#[test]
fn missing_mod_capsule_warns_that_the_patcher_builds_it() {
    let fixture = fixture(&["Modules/sample_a.MOD"]);
    fs::create_dir_all(fixture.settings.roots.game.join("Modules")).unwrap();
    let report = validate(&fixture);
    assert!(report.success, "{report:?}");
    let warnings = simulation_warnings(&report);
    assert_eq!(warnings.len(), 1, "{report:?}");
    assert!(warnings[0].contains(".rim"), "{warnings:?}");
    assert!(
        !fixture
            .settings
            .roots
            .game
            .join("Modules/sample_a.MOD")
            .exists()
    );
}

#[test]
fn missing_rim_or_erf_capsule_fails_like_the_patcher() {
    for name in ["Modules/sample_a.rim", "Modules/sample_a.ERF"] {
        let fixture = fixture(&[name]);
        fs::create_dir_all(fixture.settings.roots.game.join("Modules")).unwrap();
        let report = validate(&fixture);
        assert!(!report.success, "{name}: {report:?}");
        assert!(
            report
                .dry_run
                .as_ref()
                .unwrap()
                .issues
                .iter()
                .any(|issue| issue.severity == "Error"
                    && issue.message.contains("capsule is missing")),
            "{name}: {report:?}"
        );
        assert!(!fixture.settings.roots.game.join(name).exists());
    }
}

#[test]
fn override_folder_keeps_directory_behavior() {
    let fixture = fixture(&["Override"]);
    let report = validate(&fixture);
    assert!(report.success, "{report:?}");
    assert_eq!(simulation_warnings(&report).len(), 0, "{report:?}");
    assert!(!fixture.settings.roots.game.join("Override").exists());
}
