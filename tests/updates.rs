use modsync::{
    Guide,
    updates::{self, Versions},
};
use std::collections::BTreeMap;
use tempfile::tempdir;

#[test]
fn applies_versions_once_and_preserves_first_check_baseline() {
    let mut guide = Guide::parse(r#"[{"Guid":"one","Name":"One","ResourceRegistry":{"https://www.nexusmods.com/kotor/mods/12":{"ModVersion":"1.0"},"https://example.com/x":{}}}]"#, "json").unwrap();
    let report = updates::apply(
        &mut guide,
        &Versions(BTreeMap::from([("kotor/12".into(), "2.0".into())])),
    );
    assert_eq!(report.checked_count, 1);
    assert_eq!(report.skipped_count, 1);
    assert_eq!(report.updates_found[0].installed_version, "1.0");
    assert_eq!(
        guide.components[0].resource_registry["https://www.nexusmods.com/kotor/mods/12"]["UpdateAvailable"],
        true
    );
}

#[test]
fn stores_and_loads_the_last_update_summary() {
    let directory = tempdir().unwrap();
    let summary = updates::Summary {
        checked_count: 2,
        skipped_count: 1,
        ..Default::default()
    };
    updates::save_summary(directory.path(), &summary).unwrap();
    assert_eq!(
        updates::load_summary(directory.path())
            .unwrap()
            .unwrap()
            .checked_count,
        2
    );
}
