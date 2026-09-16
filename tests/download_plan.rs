use modsync::{Guide, download_plan};
use serde_json::Value;
use std::fs;

#[test]
fn plans_match_csharp_file_selection_and_unsatisfied_patterns() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/download-plan.json")).unwrap();
    let expected: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/csharp/download-plan.json")).unwrap();
    assert_eq!(cases.len(), expected.len());
    for (case, expected) in cases.iter().zip(expected) {
        let temp = tempfile::tempdir().unwrap();
        if let Some(files) = case["DiskFiles"].as_array() {
            for file in files {
                let path = temp.path().join(file.as_str().unwrap());
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, "fixture").unwrap();
            }
        }
        let guide =
            Guide::parse(&serde_json::to_string(&case["Component"]).unwrap(), "json").unwrap();
        let component = &guide.components[0];
        let url = case["Url"].as_str().unwrap();
        let candidates: Vec<String> = serde_json::from_value(case["Candidates"].clone()).unwrap();
        let source = case["SourceConfigured"]
            .as_bool()
            .unwrap()
            .then_some(temp.path());
        assert_eq!(
            download_plan::url_enabled(component, url),
            expected["UrlEnabled"].as_bool().unwrap(),
            "{}",
            case["Name"]
        );
        let result = download_plan::select_filenames(component, url, &candidates, source);
        if let Some(error) = expected["Error"].as_str() {
            assert_eq!(result.unwrap_err().to_string(), error, "{}", case["Name"]);
        } else {
            assert_eq!(
                serde_json::to_value(result.unwrap()).unwrap(),
                expected["Filenames"],
                "{}",
                case["Name"]
            );
        }
    }
}

#[test]
fn cli_plan_keeps_guide_and_source_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("mods");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("existing.zip"), "unchanged").unwrap();
    let input = temp.path().join("guide.json");
    let text = r#"[{"Guid":"plan","Name":"Plan","ResourceRegistry":{"https://example.com/mod":{"Files":{"enabled.zip":true,"disabled.zip":false,"unknown.zip":null}}}}]"#;
    fs::write(&input, text).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_modsync"))
        .arg("download-plan")
        .arg(&input)
        .args([
            "--component",
            "plan",
            "--url",
            "https://example.com/mod",
            "--filename",
            "enabled.zip",
            "--filename",
            "disabled.zip",
            "--filename",
            "unknown.zip",
            "--source-dir",
        ])
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report,
        serde_json::json!({"UrlEnabled":true,"Filenames":["enabled.zip","unknown.zip"]})
    );
    assert_eq!(fs::read_to_string(input).unwrap(), text);
    assert_eq!(fs::read_dir(&source).unwrap().count(), 1);
    assert_eq!(
        fs::read_to_string(source.join("existing.zip")).unwrap(),
        "unchanged"
    );
}
