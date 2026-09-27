use modsync::{
    Engine, Guide, Settings, recovery,
    validation::{Stage, StageProgress},
};
use std::fs;

fn fixture() -> (tempfile::TempDir, Settings, Guide) {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("mods")).unwrap();
    fs::create_dir(temp.path().join("game")).unwrap();
    let settings = Settings::new(&temp.path().join("mods"), &temp.path().join("game")).unwrap();
    fs::write(settings.roots.mods.join("source.txt"), "content").unwrap();
    let guide = Guide::parse(r#"{"thisMod":[{"Guid":"a","Name":"A","IsSelected":true,"Instructions":[{"Action":"Copy","Source":["<<modDirectory>>/source.txt"],"Destination":"<<gameDirectory>>/Override"}]}]}"#, "json").unwrap();
    (temp, settings, guide)
}

fn assert_boundaries(events: &[StageProgress], stages: &[Stage], total: usize) {
    assert_eq!(events.len(), stages.len() * 2);
    for (i, (pair, stage)) in events.chunks_exact(2).zip(stages).enumerate() {
        assert_eq!(pair[0].stage, *stage);
        assert_eq!(pair[1].stage, *stage);
        assert_eq!(pair[0].step, i + 1);
        assert_eq!(pair[1].step, i + 1);
        assert_eq!(pair[0].total, total);
        assert_eq!(pair[1].total, total);
        assert_eq!(pair[1].message, format!("Completed: {}", pair[0].message));
    }
}

#[test]
fn enabled_stages_and_old_reports_match_without_writing() {
    let (_temp, settings, guide) = fixture();
    for full in [false, true] {
        for dry in [false, true] {
            for only in [false, true] {
                let mut progress = vec![];
                let mut ordinary = vec![];
                let observed = Engine::new(settings.clone())
                    .unwrap()
                    .validate_report_with_options_observed(
                        &guide,
                        |e| ordinary.push(format!("{e:?}")),
                        full,
                        dry,
                        only,
                        |p| progress.push(p),
                    );
                let mut old_events = vec![];
                let old = Engine::new(settings.clone())
                    .unwrap()
                    .validate_report_with_options(
                        &guide,
                        |e| old_events.push(format!("{e:?}")),
                        full,
                        dry,
                        only,
                    );
                assert_eq!(
                    serde_json::to_value(old).unwrap(),
                    serde_json::to_value(&observed).unwrap()
                );
                assert_eq!(ordinary, old_events);
                let mut expected = vec![];
                if full {
                    expected.extend([Stage::Environment, Stage::Conflicts, Stage::InstallOrder]);
                }
                if !only {
                    expected.push(Stage::ComponentValidation);
                }
                expected.push(Stage::FomodConfiguration);
                if dry {
                    expected.push(Stage::DryRun);
                }
                assert_boundaries(&progress, &expected, expected.len());
                assert_eq!(
                    observed.stages.iter().map(|s| s.stage).collect::<Vec<_>>(),
                    expected
                );
            }
        }
    }
    assert_eq!(
        fs::read(settings.roots.mods.join("source.txt")).unwrap(),
        b"content"
    );
    assert_eq!(fs::read_dir(&settings.roots.mods).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&settings.roots.game).unwrap().count(), 0);
    assert!(!recovery::state_dir(&settings.roots).exists());
}

#[test]
fn environment_failure_stops_at_actual_stage_and_setup_failure_emits_nothing() {
    let (_temp, settings, guide) = fixture();
    let mut events = vec![];
    assert!(
        Engine::new(settings.clone())
            .unwrap()
            .validate_report_remaining_observed(
                &guide,
                &["invalid".into()],
                |_| {},
                |p| events.push(p)
            )
            .is_err()
    );
    assert!(events.is_empty());
    fs::create_dir_all(recovery::state_dir(&settings.roots).join("pending")).unwrap();
    let report = Engine::new(settings)
        .unwrap()
        .validate_report_with_options_observed(
            &guide,
            |_| {},
            true,
            true,
            false,
            |p| events.push(p),
        );
    assert!(!report.success);
    assert_boundaries(&events, &[Stage::Environment], 6);
}

#[test]
fn remaining_validation_does_not_replay_consumed_move() {
    let (_temp, settings, mut guide) = fixture();
    guide.components[0].instructions[0].action = "Move".into();
    Engine::new(settings.clone())
        .unwrap()
        .install(&guide, |_| {})
        .unwrap();
    assert!(!settings.roots.mods.join("source.txt").exists());
    let mut suffix = guide.components[0].clone();
    suffix.guid = "b".into();
    suffix.name = "B".into();
    suffix.instructions[0].action = "Copy".into();
    suffix.instructions[0].source = vec!["<<gameDirectory>>/Override/source.txt".into()];
    suffix.instructions[0].destination = "<<gameDirectory>>/wide".into();
    guide.components.push(suffix);
    let receipt = fs::read(recovery::state_dir(&settings.roots).join("snapshot.json")).unwrap();
    let mut events = vec![];
    let report = Engine::new(settings.clone())
        .unwrap()
        .validate_report_remaining_observed(&guide, &["a".into()], |_| {}, |p| events.push(p))
        .unwrap();
    let old = Engine::new(settings.clone())
        .unwrap()
        .validate_report_remaining(&guide, &["a".into()], |_| {})
        .unwrap();
    assert_eq!(
        serde_json::to_value(&report).unwrap(),
        serde_json::to_value(old).unwrap()
    );
    assert_eq!(report.component_count, 1);
    assert!(report.success, "{report:?}");
    assert_boundaries(
        &events,
        &[
            Stage::Environment,
            Stage::Conflicts,
            Stage::InstallOrder,
            Stage::ComponentValidation,
            Stage::FomodConfiguration,
            Stage::DryRun,
        ],
        6,
    );
    assert_eq!(
        fs::read(recovery::state_dir(&settings.roots).join("snapshot.json")).unwrap(),
        receipt
    );
    assert!(!settings.roots.mods.join("source.txt").exists());
    assert!(!settings.roots.game.join("wide").exists());
}

#[cfg(unix)]
#[test]
fn observed_validation_never_executes_a_trusted_patcher() {
    use std::os::unix::fs::PermissionsExt;
    let (temp, mut settings, _) = fixture();
    let patch = settings.roots.mods.join("patch/tslpatchdata");
    fs::create_dir_all(&patch).unwrap();
    fs::write(patch.join("changes.ini"), "[Settings]\n").unwrap();
    let marker = temp.path().join("executed");
    let executable = temp.path().join("patcher");
    fs::write(
        &executable,
        format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    settings.patcher = Some(executable);
    settings.patcher_engine = modsync::PatcherEngine::OdyPatcher;
    settings.allow_processes = true;
    let guide = Guide::parse(r#"{"thisMod":[{"Guid":"a","Name":"A","IsSelected":true,"Instructions":[{"Action":"Patcher","Source":"<<modDirectory>>/patch"}]}]}"#, "json").unwrap();
    let report = Engine::new(settings.clone())
        .unwrap()
        .validate_report_with_options_observed(&guide, |_| {}, true, true, false, |_| {});
    assert!(report.success, "{report:?}");
    assert!(!marker.exists());
    assert_eq!(fs::read_dir(&settings.roots.game).unwrap().count(), 0);
    assert!(!recovery::state_dir(&settings.roots).exists());
}

#[test]
fn missing_copy_source_remains_an_engine_error_without_gui_reclassification() {
    let (_temp, settings, guide) = fixture();
    fs::remove_file(settings.roots.mods.join("source.txt")).unwrap();
    let report = Engine::new(settings)
        .unwrap()
        .validate_report_with_options_observed(&guide, |_| {}, true, true, false, |_| {});
    assert!(!report.success);
    assert!(report.error_count > 0);
    assert!(
        report
            .dry_run
            .unwrap()
            .issues
            .iter()
            .any(|i| i.severity == "Error" && i.category == "EngineSimulation")
    );
}
