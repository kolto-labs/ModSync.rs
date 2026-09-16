use modsync::protocol::{self, Link};
use std::process::Command;

#[test]
fn parses_reference_nxm_and_modsync_forms_without_fetching() {
    assert_eq!(
        protocol::parse("nxm://kotor/mods/12/files/34?key=a%20b&expires=5&user_id=6").unwrap(),
        Link::Nxm {
            game_domain: "kotor".into(),
            mod_id: 12,
            file_id: 34,
            key: Some("a b".into()),
            expires: 5,
            user_id: 6
        }
    );
    assert_eq!(
        protocol::parse(
            "modsync://kotor/install?url=https%3A%2F%2Fexample.com%2Fguide.toml&game=kotor"
        )
        .unwrap(),
        Link::Modsync {
            action: "install".into(),
            game: Some("kotor".into()),
            instruction_url: "https://example.com/guide.toml".into()
        }
    );
    assert!(matches!(
        protocol::parse("modsync://install?url=https%3A%2F%2Fexample.com%2Fguide.toml"),
        Ok(Link::Modsync { action, game: None, .. }) if action == "install"
    ));
}

#[test]
fn rejects_unsafe_or_ambiguous_protocol_shapes_before_fetching() {
    for value in [
        "nxm://kotor/mods/one/files/2",
        "modsync://install?url=file:///tmp/guide.toml",
        "modsync://kotor2/install?url=https%3A%2F%2Fexample.com%2Fx&game=kotor",
        "modsync://unknown?url=https%3A%2F%2Fexample.com%2Fx",
    ] {
        assert!(protocol::parse(value).is_err(), "{value}");
    }
}

#[test]
fn instruction_urls_reject_local_targets_before_fetching() {
    for url in [
        "file:///tmp/guide.toml",
        "https://localhost/guide.toml",
        "https://127.0.0.1/guide.toml",
        "https://10.0.0.1/guide.toml",
    ] {
        assert!(protocol::safe_instruction_url(url).is_err(), "{url}");
    }
    assert!(protocol::safe_instruction_url("https://example.com/guide.toml").is_ok());
}

#[test]
fn protocol_fetch_cli_rejects_unsafe_urls_without_creating_output() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("guide.toml");
    let result = Command::new(env!("CARGO_BIN_EXE_modsync"))
        .args(["protocol-fetch", "http://127.0.0.1/guide.toml"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!output.exists());
    assert!(String::from_utf8_lossy(&result.stderr).contains("private address"));
}

#[test]
fn protocol_open_rejects_unsafe_modsync_instruction_urls_before_fetching() {
    let result = Command::new(env!("CARGO_BIN_EXE_modsync"))
        .args([
            "protocol-open",
            "modsync://install?url=http%3A%2F%2F127.0.0.1%2Fguide.toml",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("private address"));
}

#[test]
fn nxm_resolution_matches_one_top_level_registry_component_and_rejects_ambiguity() {
    let guide = modsync::Guide::parse(
        r#"[
          {"Guid":"other","Name":"Other","ResourceRegistry":{"https://deadlystream.com/files/file/1/":{}}},
          {"Guid":"target","Name":"Target","ResourceRegistry":{"https://www.nexusmods.com/kotor/mods/1234?tab=files":{}}}
        ]"#,
        "json",
    )
    .unwrap();
    let link = protocol::parse("nxm://kotor/mods/1234/files/5678").unwrap();
    let (status, matched) = protocol::resolve_nxm(&link, &guide.components);
    assert_eq!(status, protocol::NxmResolveStatus::Matched);
    assert_eq!(matched.unwrap().component_name, "Target");

    let ambiguous = modsync::Guide::parse(
        r#"[
          {"Guid":"a","Name":"A","ResourceRegistry":{"https://www.nexusmods.com/kotor/mods/1234":{}}},
          {"Guid":"b","Name":"B","ResourceRegistry":{"https://nexusmods.com/kotor/mods/1234?tab=files":{}}}
        ]"#,
        "json",
    )
    .unwrap();
    assert_eq!(
        protocol::resolve_nxm(&link, &ambiguous.components).0,
        protocol::NxmResolveStatus::Ambiguous
    );
}

#[test]
fn protocol_resolve_cli_reports_a_read_only_match() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("guide.json");
    std::fs::write(
        &input,
        r#"[{"Guid":"target","Name":"Target","ResourceRegistry":{"https://www.nexusmods.com/kotor/mods/12":{}}}]"#,
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_modsync"))
        .args(["protocol-resolve"])
        .arg(&input)
        .arg("nxm://kotor/mods/12/files/34")
        .output()
        .unwrap();
    assert!(result.status.success());
    let output: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(output["Status"], "Matched");
    assert_eq!(output["Match"]["ComponentName"], "Target");
}

#[test]
fn bare_protocol_url_is_accepted_like_a_registered_handler_invocation() {
    let result = Command::new(env!("CARGO_BIN_EXE_modsync"))
        .arg("nxm://kotor/mods/12/files/34")
        .output()
        .unwrap();
    assert!(result.status.success());
    let output: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(output["scheme"], "nxm");
    assert_eq!(output["mod_id"], 12);
}
