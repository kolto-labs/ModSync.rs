use modsync::telemetry::{self, Config};

#[test]
fn telemetry_stays_off_without_consent_and_never_serializes_secret() {
    let mut config = Config {
        enabled: true,
        signing_secret: Some("secret".into()),
        ..Default::default()
    };
    assert!(!config.active());
    config.user_consented = true;
    assert!(config.active());
    assert!(!serde_json::to_string(&config).unwrap().contains("secret"));
    assert_eq!(
        telemetry::signature("key", "session", "/v1/traces", 1),
        "ae80d32f1bfd20acb09171cb25947aec148f4783fd95076fa44819864b83ff12"
    );
}
