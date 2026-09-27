use modsync::{Settings, recovery};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};

struct Fixture {
    root: tempfile::TempDir,
    settings: Settings,
    guide: Value,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("game")).unwrap();
        fs::create_dir(root.path().join("mods")).unwrap();
        let settings = Settings::new(&root.path().join("mods"), &root.path().join("game")).unwrap();
        fs::write(settings.roots.mods.join("base.txt"), b"exact base bytes\n").unwrap();
        fs::write(settings.roots.mods.join("wide.txt"), b"exact wide bytes\n").unwrap();
        let guide = json!({"thisMod":[
            {"Guid":"base","Name":"Base","IsSelected":true,"Instructions":[{"Action":"Move","Source":["<<modDirectory>>/base.txt"],"Destination":"<<gameDirectory>>/Override"}]},
            {"Guid":"wide","Name":"Wide","IsSelected":true,"WidescreenOnly":true,"Dependencies":["base"],"Instructions":[{"Action":"Copy","Source":["<<modDirectory>>/wide.txt"],"Destination":"<<gameDirectory>>/Override"}]}
        ]});
        let fixture = Self {
            root,
            settings,
            guide,
        };
        fixture.save();
        fixture
    }
    fn save(&self) {
        fs::write(
            self.root.path().join("guide.json"),
            serde_json::to_vec(&self.guide).unwrap(),
        )
        .unwrap();
    }
    fn run(&self, action: &str, flags: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_modsync"));
        command
            .arg(action)
            .arg("--game-dir")
            .arg(&self.settings.roots.game)
            .arg("--source-dir")
            .arg(&self.settings.roots.mods);
        if action != "restore" {
            command
                .arg("--input")
                .arg(self.root.path().join("guide.json"));
        }
        let output = command.args(flags).output().unwrap();
        eprintln!(
            "CLI {command:?}: exit={:?}, stdout={}, stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
    fn state(&self) -> std::path::PathBuf {
        recovery::state_dir(&self.settings.roots)
    }
    fn receipt(&self) -> Value {
        serde_json::from_slice(&fs::read(self.state().join("completion.json")).unwrap()).unwrap()
    }
    fn base(&self, flags: &[&str]) {
        let mut args = vec!["--phase", "base", "--json"];
        args.extend_from_slice(flags);
        let output = self.run("install", &args);
        success(&output);
        assert_eq!(components(&output), ["Base"]);
        assert!(!self.state().join("pending").exists());
    }
    fn unchanged_failure(&self, action: &str, flags: &[&str], pipeline: Option<bool>) {
        let before = manifest(self.root.path());
        let output = self.run(action, flags);
        assert_eq!(
            output.status.code(),
            Some(1),
            "{action} {flags:?}: {output:?}"
        );
        if let Some(pipeline) = pipeline {
            assert!(output.stderr.is_empty(), "{output:?}");
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["success"], false);
            assert_eq!(report.get("stages").is_some(), pipeline, "{report}");
            assert_eq!(report.get("inputPath").is_some(), pipeline, "{report}");
            assert_eq!(report.get("error").is_some(), !pipeline, "{report}");
        } else {
            assert!(!output.stderr.is_empty(), "{output:?}");
            assert!(
                output.stdout.is_empty(),
                "unexpected event before rejection: {output:?}"
            );
        }
        assert_eq!(manifest(self.root.path()), before, "{action} {flags:?}");
    }
    fn reject_remaining(&self, extra: &[&str], pipeline: bool) {
        for action in ["validate", "conflicts", "install"] {
            let mut args = vec!["--phase", "remaining"];
            args.extend_from_slice(extra);
            if action == "validate" {
                args.push("--validation-report");
            } else if action == "install" {
                args.push("--json");
            }
            self.unchanged_failure(action, &args, (action == "validate").then_some(pipeline));
        }
        self.unchanged_failure(
            "validate",
            &[&["--phase", "remaining", "--json"][..], extra].concat(),
            None,
        );
    }
}
fn success(output: &Output) {
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}
fn components(output: &Output) -> Vec<String> {
    let mut names = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let event: Value = serde_json::from_str(line).unwrap();
        let name = event["component"].as_str().unwrap();
        if !name.is_empty() && names.last().is_none_or(|last| last != name) {
            names.push(name.to_owned());
        }
    }
    names
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
                    vec![]
                },
            )
        })
        .collect()
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn executable_all_invalid_guides_keep_pipeline_report_shape() {
    for empty in [false, true] {
        let mut f = Fixture::new();
        f.guide["thisMod"] = if empty {
            json!([{"Guid":"base","Name":"Base","IsSelected":false,"Instructions":[]}])
        } else {
            json!([{"Guid":"base","Name":"Base","IsSelected":true,"Dependencies":["missing"],"Instructions":[]}])
        };
        f.save();
        for explicit_all in [false, true] {
            let mut flags = vec!["--validation-report", "--full", "--use-file-selection"];
            if empty {
                // Loading defaults an entirely unselected file to all selected;
                // use the existing CLI filter to produce an empty selection.
                flags.extend(["--select", "category:absent"]);
            }
            if explicit_all {
                flags.extend(["--phase", "all"]);
            }
            let before = manifest(f.root.path());
            let output = f.run("validate", &flags);
            assert_eq!(output.status.code(), Some(1));
            assert!(output.stderr.is_empty(), "{output:?}");
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["success"], false);
            assert_eq!(report["inputPath"], json!(f.root.path().join("guide.json")));
            assert_eq!(report["componentCount"], usize::from(!empty));
            assert!(report.get("error").is_none(), "{report}");
            let stages = report["stages"].as_array().unwrap();
            for stage in ["Conflicts", "InstallOrder", "ComponentValidation"] {
                assert!(
                    stages
                        .iter()
                        .any(|result| result["stage"] == stage && result["passed"] == false),
                    "{report}"
                );
            }
            assert_eq!(manifest(f.root.path()), before);
        }
    }
}

#[test]
fn executable_phases_consume_base_then_validate_conflict_and_install_only_suffix() {
    let f = Fixture::new();
    let original = manifest(f.root.path());
    let all = f.run(
        "validate",
        &[
            "--phase",
            "all",
            "--validation-report",
            "--full",
            "--dry-run",
        ],
    );
    success(&all);
    let report: Value = serde_json::from_slice(&all.stdout).unwrap();
    assert_eq!(report["componentCount"], 2);
    assert_eq!(report["dryRun"]["isValid"], true);
    assert_eq!(manifest(f.root.path()), original);
    f.base(&[]);
    assert!(!f.settings.roots.mods.join("base.txt").exists());
    assert_eq!(
        fs::read(f.settings.roots.game.join("Override/base.txt")).unwrap(),
        b"exact base bytes\n"
    );
    assert!(!f.settings.roots.game.join("Override/wide.txt").exists());
    let base_receipt = f.receipt();
    assert_eq!(base_receipt["components"].as_array().unwrap().len(), 1);
    let before = manifest(f.root.path());
    eprintln!(
        "base manifest sha256 by path: {:?}",
        before
            .iter()
            .map(|(path, bytes)| (path, hash(bytes)))
            .collect::<BTreeMap<_, _>>()
    );
    for flags in [
        vec!["--phase", "remaining", "--validation-report"],
        vec![
            "--phase",
            "remaining",
            "--validation-report",
            "--full",
            "--dry-run",
        ],
    ] {
        let output = f.run("validate", &flags);
        success(&output);
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["componentCount"], 1);
        assert_eq!(report["dryRun"]["isValid"], true);
        assert_eq!(report["stages"].as_array().unwrap().len(), 6);
        assert_eq!(manifest(f.root.path()), before);
    }
    let conflict = f.run("conflicts", &["--phase", "remaining"]);
    success(&conflict);
    assert_eq!(
        serde_json::from_slice::<Value>(&conflict.stdout).unwrap()["analyzedComponentCount"],
        1
    );
    assert_eq!(manifest(f.root.path()), before);
    let validate = f.run("validate", &["--phase", "remaining", "--json"]);
    success(&validate);
    assert_eq!(components(&validate), ["Wide"]);
    f.unchanged_failure("install", &["--phase", "base", "--json"], None);
    let output = f.run("install", &["--phase", "remaining", "--json"]);
    success(&output);
    assert_eq!(components(&output), ["Wide"]);
    assert_eq!(
        fs::read(f.settings.roots.game.join("Override/base.txt")).unwrap(),
        b"exact base bytes\n"
    );
    assert_eq!(
        fs::read(f.settings.roots.game.join("Override/wide.txt")).unwrap(),
        b"exact wide bytes\n"
    );
    let receipt = f.receipt();
    assert_eq!(receipt["components"][0], base_receipt["components"][0]);
    assert_eq!(
        receipt["components"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["Guid"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["base", "wide"]
    );
    assert!(!f.state().join("pending").exists());
    eprintln!(
        "final manifest sha256 by path: {:?}",
        manifest(f.root.path())
            .iter()
            .map(|(path, bytes)| (path.clone(), hash(bytes)))
            .collect::<BTreeMap<_, _>>()
    );
    f.reject_remaining(&[], true);
    eprintln!(
        "phase proof guide sha256={} base receipt sha256={} final receipt sha256={} events Base -> Wide; source consumed, exact output bytes, manifests unchanged for validation/conflicts, pending absent",
        hash(&original["guide.json"]),
        hash(&serde_json::to_vec(&base_receipt).unwrap()),
        hash(&serde_json::to_vec(&receipt).unwrap())
    );
}

#[test]
fn executable_remaining_rejects_stale_receipts_and_tampering_without_writes() {
    for mutation in 0..5 {
        let mut f = Fixture::new();
        f.base(&[]);
        let receipt = f.state().join("completion.json");
        match mutation {
            0 => fs::remove_file(receipt).unwrap(),
            1 => fs::write(receipt, "corrupt").unwrap(),
            2 => {
                let mut value = f.receipt();
                value["game"] = json!("/wrong/root");
                fs::write(receipt, value.to_string()).unwrap();
            }
            3 => {
                f.guide["thisMod"][0]["Name"] = json!("Changed");
                f.save();
            }
            _ => fs::write(f.settings.roots.game.join("Override/base.txt"), "tampered").unwrap(),
        }
        f.reject_remaining(&[], true);
        assert!(!f.state().join("pending").exists());
    }
}

#[test]
fn executable_preparation_rejects_invalid_prefixes_empty_phases_and_report_flags() {
    for mutation in 0..4 {
        let mut f = Fixture::new();
        match mutation {
            0 => {
                f.guide["thisMod"][0]["WidescreenOnly"] = json!(true);
            }
            1 => {
                f.guide["thisMod"][1]["WidescreenOnly"] = json!(false);
            }
            2 => {
                let mut late = f.guide["thisMod"][0].clone();
                late["Guid"] = json!("late");
                f.guide["thisMod"].as_array_mut().unwrap().push(late);
            }
            _ => {
                f.guide["thisMod"] = json!([]);
            }
        }
        f.save();
        f.reject_remaining(&[], false);
        if mutation == 0 || mutation == 3 {
            f.unchanged_failure("install", &["--phase", "base", "--json"], None);
        }
    }
    let f = Fixture::new();
    for phase in ["base", "remaining"] {
        f.unchanged_failure(
            "validate",
            &["--phase", phase, "--direct-markdown", "--validation-report"],
            Some(false),
        );
        for action in ["install", "conflicts"] {
            for flag in ["--full", "--dry-run", "--dry-run-only"] {
                f.unchanged_failure(
                    action,
                    &["--phase", phase, "--validation-report", flag],
                    None,
                );
            }
            f.unchanged_failure(action, &["--phase", phase, "--validation-report"], None);
        }
    }
    f.unchanged_failure(
        "validate",
        &[
            "--phase",
            "remaining",
            "--validation-report",
            "--dry-run-only",
        ],
        Some(false),
    );
}

fn choices(base: &str, wide: &str) -> Value {
    json!({"archives":[
        {"archiveFileName":"base.zip","selections":[{"stepName":"s","groupName":"g","plugins":[base]}]},
        {"archiveFileName":"wide.zip","selections":[{"stepName":"s","groupName":"g","plugins":[wide]}]}
    ]})
}

#[test]
fn executable_choices_reconstruction_is_stable_and_whole_payload_changes_fail_closed() {
    for mutation in 0..6 {
        let f = Fixture::new();
        let path = f.root.path().join("choices.json");
        let original = choices("original", "original").to_string();
        fs::write(&path, &original).unwrap();
        let args = ["--fomod-choices", path.to_str().unwrap()];
        f.base(&args);
        match mutation {
            0 => fs::remove_file(&path).unwrap(),
            1 => fs::write(&path, "corrupt").unwrap(),
            2 => {} // Omitting the prior choices must not recover them from the receipt.
            3 => fs::write(&path, choices("changed", "original").to_string()).unwrap(),
            4 => fs::write(&path, choices("original", "suffix-only-change").to_string()).unwrap(),
            _ => {
                let before = manifest(f.root.path());
                let output = f.run(
                    "validate",
                    &[
                        "--phase",
                        "remaining",
                        "--validation-report",
                        args[0],
                        args[1],
                    ],
                );
                success(&output);
                assert_eq!(manifest(f.root.path()), before);
                success(&f.run(
                    "install",
                    &["--phase", "remaining", "--json", args[0], args[1]],
                ));
                eprintln!("stable choices sha256={}", hash(original.as_bytes()));
                continue;
            }
        }
        f.reject_remaining(if mutation == 2 { &[] } else { &args }, mutation >= 2);
        assert!(!f.state().join("pending").exists());
    }
}

#[test]
fn executable_explicit_base_retains_ordinary_copy_repeat_semantics() {
    let mut f = Fixture::new();
    f.guide["thisMod"][0]["Instructions"][0]["Action"] = json!("Copy");
    f.guide["thisMod"][0]["Instructions"][0]["Overwrite"] = json!(true);
    f.save();
    f.base(&[]);
    f.base(&[]);
    assert!(!f.settings.roots.game.join("Override/wide.txt").exists());
}

#[cfg(unix)]
#[test]
fn executable_failed_suffix_blocks_operations_until_explicit_restore_then_retries() {
    use std::os::unix::fs::PermissionsExt;
    let mut f = Fixture::new();
    let patch = f.settings.roots.mods.join("patch/tslpatchdata");
    fs::create_dir_all(&patch).unwrap();
    fs::write(patch.join("changes.ini"), "fixture").unwrap();
    let stub = f.root.path().join("patcher");
    fs::write(&stub, "#!/bin/sh\nexit 7\n").unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    f.guide["thisMod"][1]["Instructions"]
        .as_array_mut()
        .unwrap()
        .push(json!({"Action":"Patcher","Source":"<<modDirectory>>/patch"}));
    f.save();
    f.base(&[]);
    let receipt = fs::read(f.state().join("completion.json")).unwrap();
    let args = ["--allow-processes", "--patcher", stub.to_str().unwrap()];
    let output = f.run(
        "install",
        &[&["--phase", "remaining", "--json"][..], &args].concat(),
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(!output.stderr.is_empty());
    assert!(f.state().join("pending").exists());
    assert!(f.settings.roots.game.join("Override/wide.txt").exists());
    f.reject_remaining(&args, true);
    success(&f.run("restore", &[]));
    assert!(!f.state().join("pending").exists());
    assert!(!f.settings.roots.game.join("Override/wide.txt").exists());
    assert_eq!(
        fs::read(f.state().join("completion.json")).unwrap(),
        receipt
    );
    assert!(!f.settings.roots.mods.join("base.txt").exists());
    fs::write(&stub, "#!/bin/sh\nexit 0\n").unwrap();
    let output = f.run(
        "install",
        &[&["--phase", "remaining", "--json"][..], &args].concat(),
    );
    success(&output);
    assert_eq!(components(&output), ["Wide"]);
    assert!(!f.state().join("pending").exists());
    assert_eq!(
        fs::read(f.settings.roots.game.join("Override/base.txt")).unwrap(),
        b"exact base bytes\n"
    );
    assert_eq!(
        fs::read(f.settings.roots.game.join("Override/wide.txt")).unwrap(),
        b"exact wide bytes\n"
    );
}
