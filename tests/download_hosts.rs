use modsync::download_hosts::{self, Provider};

#[test]
fn source_order_keeps_specific_hosts_ahead_of_direct_fallback() {
    assert_eq!(
        download_hosts::provider("https://deadlystream.com/files/x").unwrap(),
        Provider::DeadlyStream
    );
    assert_eq!(
        download_hosts::provider("https://mega.nz/file/x#key").unwrap(),
        Provider::Mega
    );
    assert_eq!(
        download_hosts::provider("https://www.nexusmods.com/kotor/mods/1").unwrap(),
        Provider::Nexus
    );
    assert_eq!(
        download_hosts::provider("https://example.com/x").unwrap(),
        Provider::Direct
    );
    assert!(download_hosts::provider("file:///tmp/x").is_err());
}
