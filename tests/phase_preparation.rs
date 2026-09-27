use modsync::{
    Engine, Guide, Settings,
    fomod::ChoicesFile,
    phase::{self, CompletionContext as C, Phase as P},
    recovery,
};
use std::{collections::BTreeMap, fs, path::Path};

fn selected() -> Guide {
    Guide::parse(&serde_json::json!({"thisMod":[
        {"Guid":"base","Name":"Base","IsSelected":true,"Instructions":[{"Action":"Move","Source":["<<modDirectory>>/base.txt"],"Destination":"<<gameDirectory>>/Override"}]},
        {"Guid":"wide","Name":"Wide","IsSelected":true,"WidescreenOnly":true,"Dependencies":["base"],"Instructions":[{"Action":"Copy","Source":["<<gameDirectory>>/Override/base.txt"],"Destination":"<<gameDirectory>>/wide"}]}
    ]}).to_string(), "json").unwrap()
}
fn choices(base: &str, wide: &str) -> ChoicesFile {
    ChoicesFile::parse(&serde_json::json!({"archives":[
        {"archiveFileName":"base.zip","selections":[{"stepName":"s","groupName":"g","plugins":[base]}]},
        {"archiveFileName":"wide.zip","selections":[{"stepName":"s","groupName":"g","plugins":[wide]}]}
    ]}).to_string()).unwrap()
}
fn manifest(root: &Path) -> BTreeMap<String, Vec<u8>> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .map(Result::unwrap)
        .map(|entry| {
            (
                entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                if entry.file_type().is_file() {
                    fs::read(entry.path()).unwrap()
                } else {
                    Vec::new()
                },
            )
        })
        .collect()
}

#[test]
fn preparation_matrix_preserves_selection_and_rejects_invalid_contexts() {
    let guide = selected();
    let ids = vec!["base".into()];
    for phase in [P::All, P::Base, P::Remaining] {
        for (context, valid) in [
            (C::None, phase != P::Remaining),
            (C::Retained { ids: &ids }, phase == P::Remaining),
            (C::Reconstruct, phase == P::Remaining),
        ] {
            assert_eq!(
                phase::prepare(&guide, phase, context, None).is_ok(),
                valid,
                "{phase:?} {context:?}"
            );
        }
    }
    let all = phase::prepare(&guide, P::All, C::None, None).unwrap();
    assert_eq!(
        serde_json::to_value(&all.guide).unwrap(),
        serde_json::to_value(&guide).unwrap()
    );
    let base = phase::prepare(&guide, P::Base, C::None, None).unwrap();
    assert_eq!(
        base.guide
            .ordered()
            .unwrap()
            .iter()
            .map(|c| c.guid.as_str())
            .collect::<Vec<_>>(),
        ["base"]
    );
    assert!(guide.components[1].is_selected);
    let mut no_base = guide.clone();
    no_base.components[0].is_selected = false;
    no_base.components[1].dependencies.clear();
    assert!(phase::prepare(&no_base, P::Base, C::None, None).is_err());
    assert!(phase::prepare(&no_base, P::Remaining, C::Reconstruct, None).is_err());
    assert!(phase::prepare(&no_base, P::Remaining, C::Retained { ids: &ids }, None).is_err());
    no_base.components[1].is_selected = false;
    let empty_all = phase::prepare(&no_base, P::All, C::None, None).unwrap();
    assert!(empty_all.guide.ordered().is_err());
    let mut invalid_all = guide.clone();
    invalid_all.components[0].dependencies = vec!["missing".into()];
    let choices = choices("base-choice", "wide-choice");
    let mut expected = invalid_all.clone();
    choices.apply_to_guide(&mut expected).unwrap();
    let prepared = phase::prepare(&invalid_all, P::All, C::None, Some(&choices)).unwrap();
    assert_eq!(
        serde_json::to_value(&prepared.guide).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
    assert!(prepared.guide.ordered().is_err());
    assert!(phase::prepare(&invalid_all, P::Base, C::None, None).is_err());
}

#[test]
fn remaining_rejects_empty_duplicate_missing_nonprefix_and_exhausted_ids() {
    let guide = selected();
    for ids in [
        vec![],
        vec!["base", "BASE"],
        vec!["missing"],
        vec!["wide"],
        vec!["base", "wide"],
    ] {
        let ids: Vec<String> = ids.into_iter().map(str::to_owned).collect();
        assert!(phase::prepare(&guide, P::Remaining, C::Retained { ids: &ids }, None).is_err());
    }
    let mut exhausted = guide.clone();
    exhausted.components[1].is_selected = false;
    assert!(phase::prepare(&exhausted, P::Remaining, C::Reconstruct, None).is_err());
    let mut interleaved = guide.clone();
    let mut late = guide.components[0].clone();
    late.guid = "late-base".into();
    interleaved.components.push(late);
    assert!(phase::prepare(&interleaved, P::Remaining, C::Reconstruct, None).is_err());
    let reversed = vec!["wide".into(), "base".into()];
    assert!(
        phase::prepare(
            &interleaved,
            P::Remaining,
            C::Retained { ids: &reversed },
            None
        )
        .is_err()
    );
}

#[test]
fn stable_reconstruction_matches_retained_and_changed_choices_fail_receipt_verification() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("mods")).unwrap();
    fs::create_dir(root.path().join("game")).unwrap();
    let settings = Settings::new(&root.path().join("mods"), &root.path().join("game")).unwrap();
    fs::write(settings.roots.mods.join("base.txt"), b"base output").unwrap();
    let guide = selected();
    let original = choices("original", "wide-original");
    let base = phase::prepare(&guide, P::Base, C::None, Some(&original)).unwrap();
    Engine::new(settings.clone())
        .unwrap()
        .install(&base.guide, |_| {})
        .unwrap();
    assert!(!settings.roots.mods.join("base.txt").exists());
    assert_eq!(
        fs::read(settings.roots.game.join("Override/base.txt")).unwrap(),
        b"base output"
    );
    let mut retained = guide.clone();
    retained.components[0] = base.guide.components[0].clone();
    let ids = vec!["base".into()];
    let prepared = phase::prepare(
        &retained,
        P::Remaining,
        C::Retained { ids: &ids },
        Some(&original),
    )
    .unwrap();
    let reconstructed =
        phase::prepare(&guide, P::Remaining, C::Reconstruct, Some(&original)).unwrap();
    assert_eq!(prepared.completed, reconstructed.completed);
    assert_eq!(
        serde_json::to_value(&prepared.guide).unwrap(),
        serde_json::to_value(&reconstructed.guide).unwrap()
    );
    let before = manifest(root.path());
    Engine::new(settings.clone())
        .unwrap()
        .validate_remaining(&reconstructed.guide, &reconstructed.completed, |_| {})
        .unwrap();
    for changed in [
        choices("changed", "wide-original"),
        choices("original", "wide-changed"),
    ] {
        let candidate =
            phase::prepare(&guide, P::Remaining, C::Reconstruct, Some(&changed)).unwrap();
        let err = Engine::new(settings.clone())
            .unwrap()
            .validate_remaining(&candidate.guide, &candidate.completed, |_| {})
            .unwrap_err();
        assert!(err.to_string().contains("definitions differ"), "{err:#}");
        let retained_candidate = phase::prepare(
            &retained,
            P::Remaining,
            C::Retained { ids: &ids },
            Some(&changed),
        )
        .unwrap();
        Engine::new(settings.clone())
            .unwrap()
            .validate_remaining(&retained_candidate.guide, &ids, |_| {})
            .unwrap();
    }
    assert_eq!(manifest(root.path()), before);
    assert!(
        !recovery::state_dir(&settings.roots)
            .join("pending")
            .exists()
    );
}

#[test]
fn remaining_conflicts_verify_all_evidence_without_any_tree_or_snapshot_changes() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("mods")).unwrap();
    fs::create_dir(root.path().join("game")).unwrap();
    let settings = Settings::new(&root.path().join("mods"), &root.path().join("game")).unwrap();
    fs::write(settings.roots.mods.join("base.txt"), b"completed bytes").unwrap();
    let guide = selected();
    let base = phase::prepare(&guide, P::Base, C::None, None).unwrap();
    Engine::new(settings.clone())
        .unwrap()
        .install(&base.guide, |_| {})
        .unwrap();
    let receipt = recovery::state_dir(&settings.roots).join("completion.json");
    let saved = fs::read(&receipt).unwrap();
    let ids = vec!["base".into()];
    let before = manifest(root.path());
    let report = Engine::new(settings.clone())
        .unwrap()
        .analyze_conflicts_remaining(&guide, &ids)
        .unwrap();
    assert_eq!(report.analyzed_component_count, 1);
    assert_eq!(manifest(root.path()), before);
    assert!(!settings.roots.mods.join("base.txt").exists());
    for mutation in 0..5 {
        let mut candidate = guide.clone();
        match mutation {
            0 => fs::remove_file(&receipt).unwrap(),
            1 => fs::write(&receipt, b"corrupt").unwrap(),
            2 => candidate.components[0].name = "Changed definition".into(),
            3 => fs::write(settings.roots.game.join("Override/base.txt"), b"tampered").unwrap(),
            _ => {
                let mut value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
                value["game"] = serde_json::json!("/different/root");
                fs::write(&receipt, serde_json::to_vec(&value).unwrap()).unwrap();
            }
        }
        let before_reject = manifest(root.path());
        assert!(
            Engine::new(settings.clone())
                .unwrap()
                .analyze_conflicts_remaining(&candidate, &ids)
                .is_err(),
            "mutation {mutation}"
        );
        assert_eq!(manifest(root.path()), before_reject);
        assert!(
            !recovery::state_dir(&settings.roots)
                .join("pending")
                .exists()
        );
        fs::write(&receipt, &saved).unwrap();
        fs::write(
            settings.roots.game.join("Override/base.txt"),
            b"completed bytes",
        )
        .unwrap();
    }
}
