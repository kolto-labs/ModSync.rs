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
