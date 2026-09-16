use modsync::deployment::{Manifest, Service};
use std::fs;

fn setup() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let temp = tempfile::tempdir().unwrap();
    let game = temp.path().join("game");
    let staging = temp.path().join("staging");
    let manifests = temp.path().join("managed");
    fs::create_dir_all(&game).unwrap();
    fs::create_dir_all(&staging).unwrap();
    (temp, game, staging, manifests)
}

#[test]
fn deploy_uninstall_restores_backups_and_removes_only_manifested_files() {
    let (_temp, game, staging, root) = setup();
    let component = "11111111-1111-1111-1111-111111111111";
    fs::create_dir_all(game.join("Override")).unwrap();
    fs::write(game.join("Override/a.txt"), b"original").unwrap();
    fs::create_dir_all(staging.join(component).join("Override")).unwrap();
    fs::write(
        staging.join(component).join("Override/a.txt"),
        b"replacement",
    )
    .unwrap();
    fs::write(staging.join(component).join("Override/b.txt"), b"added").unwrap();
    let service = Service::new(&game, &staging, &root).unwrap();

    let manifest = service.deploy(component, Some("First".into())).unwrap();
    assert_eq!(manifest.entries.len(), 2);
    assert_eq!(
        fs::read(game.join("Override/a.txt")).unwrap(),
        b"replacement"
    );
    assert_eq!(fs::read(game.join("Override/b.txt")).unwrap(), b"added");
    assert!(
        root.join("manifests")
            .join(format!("{component}.json"))
            .exists()
    );
    let saved =
        fs::read_to_string(root.join("manifests").join(format!("{component}.json"))).unwrap();
    assert!(saved.contains("\"DeploymentMethod\": 0") || saved.contains("\"DeploymentMethod\": 1"));
    assert_eq!(service.deployed().unwrap().len(), 1);

    assert!(service.uninstall(component).unwrap());
    assert_eq!(fs::read(game.join("Override/a.txt")).unwrap(), b"original");
    assert!(!game.join("Override/b.txt").exists());
    assert!(service.deployed().unwrap().is_empty());
    assert!(!service.uninstall(component).unwrap());
}

#[test]
fn uninstall_preserves_user_changed_deployment_and_its_backup() {
    let (_temp, game, staging, root) = setup();
    let component = "22222222-2222-2222-2222-222222222222";
    fs::create_dir_all(game.join("Override")).unwrap();
    fs::write(game.join("Override/a.txt"), b"original").unwrap();
    fs::create_dir_all(staging.join(component).join("Override")).unwrap();
    fs::write(staging.join(component).join("Override/a.txt"), b"deployed").unwrap();
    let service = Service::new(&game, &staging, &root).unwrap();
    service.deploy(component, None).unwrap();
    fs::write(game.join("Override/a.txt"), b"user changed").unwrap();

    assert!(service.uninstall(component).unwrap());
    assert_eq!(
        fs::read(game.join("Override/a.txt")).unwrap(),
        b"user changed"
    );
    assert_eq!(
        fs::read(root.join("backups").join(component).join("Override/a.txt")).unwrap(),
        b"original"
    );
}

#[test]
fn purge_unwinds_newest_component_first_and_restores_the_original_file() {
    let (_temp, game, staging, root) = setup();
    let first = "33333333-3333-3333-3333-333333333333";
    let second = "44444444-4444-4444-4444-444444444444";
    fs::create_dir_all(game.join("Override")).unwrap();
    fs::write(game.join("Override/a.txt"), b"original").unwrap();
    for (component, bytes) in [(first, b"first".as_slice()), (second, b"second".as_slice())] {
        fs::create_dir_all(staging.join(component).join("Override")).unwrap();
        fs::write(staging.join(component).join("Override/a.txt"), bytes).unwrap();
    }
    let service = Service::new(&game, &staging, &root).unwrap();
    service.deploy(first, None).unwrap();
    service.deploy(second, None).unwrap();
    assert_eq!(fs::read(game.join("Override/a.txt")).unwrap(), b"second");
    assert_eq!(service.purge().unwrap(), 2);
    assert_eq!(fs::read(game.join("Override/a.txt")).unwrap(), b"original");
}

#[test]
fn malformed_or_unsafe_manifests_do_not_reach_outside_the_game_tree() {
    let (_temp, game, staging, root) = setup();
    let component = "55555555-5555-5555-5555-555555555555";
    let service = Service::new(&game, &staging, &root).unwrap();
    fs::write(game.join("safe.txt"), b"safe").unwrap();
    let mut manifest = Manifest {
        component_guid: component.into(),
        component_name: None,
        deployed_utc: chrono::Utc::now(),
        entries: vec![],
    };
    manifest.entries.push(modsync::deployment::Entry {
        relative_path: "../safe.txt".into(),
        source_hash: "irrelevant".into(),
        size: 0,
        deployment_method: modsync::deployment::Method::Copy,
        overwrote_existing: false,
        backup_relative_path: None,
    });
    fs::write(
        root.join("manifests").join(format!("{component}.json")),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(service.uninstall(component).unwrap());
    assert_eq!(fs::read(game.join("safe.txt")).unwrap(), b"safe");
}
