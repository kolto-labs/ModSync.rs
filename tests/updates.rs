use modsync::{
    Guide,
    updates::{self, Versions},
};
use std::collections::BTreeMap;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread::JoinHandle,
};
use tempfile::tempdir;

fn nexus_server(bodies: Vec<&'static str>) -> (String, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for body in bodies {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let size = stream.read(&mut request).unwrap();
            requests.push(String::from_utf8_lossy(&request[..size]).into_owned());
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        requests
    });
    (format!("http://{address}/v1/"), worker)
}

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

#[test]
fn nexus_check_deduplicates_resources_and_applies_provider_versions() {
    let (base_url, server) = nexus_server(vec![r#"{"name":"One","version":"2.0"}"#]);
    let mut guide = Guide::parse(
        r#"[{"Guid":"one","Name":"One","ResourceRegistry":{"https://www.nexusmods.com/kotor/mods/12":{"ModVersion":"1.0"}}},{"Guid":"two","Name":"Two","ResourceRegistry":{"https://www.nexusmods.com/kotor/mods/12?tab=files":{},"https://example.com/x":{}}}]"#,
        "json",
    )
    .unwrap();
    let mut client =
        updates::NexusClient::with_base_url(Some("test-key".into()), &base_url).unwrap();

    let summary = updates::check(&mut guide, &mut client);
    let requests = server.join().unwrap();

    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /v1/games/kotor/mods/12.json HTTP/1.1"));
    assert!(requests[0].contains("apikey: test-key"));
    assert_eq!(summary.checked_count, 2);
    assert_eq!(summary.skipped_count, 1);
    assert_eq!(summary.updates_found.len(), 1);
    assert_eq!(summary.updates_found[0].component_name, "One");
    assert_eq!(
        guide.components[1].resource_registry["https://www.nexusmods.com/kotor/mods/12?tab=files"]
            ["ModVersion"],
        "2.0"
    );
}

#[test]
fn nexus_check_stops_before_a_new_request_when_the_api_budget_is_empty() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        let size = stream.read(&mut request).unwrap();
        let body = r#"{"name":"One","version":"1.0"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-RL-Daily-Remaining: 0\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        String::from_utf8_lossy(&request[..size]).into_owned()
    });
    let mut guide = Guide::parse(
        r#"[{"Guid":"one","Name":"One","ResourceRegistry":{"https://www.nexusmods.com/kotor/mods/1":{}}},{"Guid":"two","Name":"Two","ResourceRegistry":{"https://www.nexusmods.com/kotor/mods/2":{}}}]"#,
        "json",
    )
    .unwrap();
    let mut client =
        updates::NexusClient::with_base_url(None, &format!("http://{address}/v1/")).unwrap();

    let summary = updates::check(&mut guide, &mut client);

    assert!(
        server
            .join()
            .unwrap()
            .contains("/v1/games/kotor/mods/1.json")
    );
    assert!(summary.rate_limit_reached);
    assert_eq!(summary.checked_count, 1);
    assert!(guide.components[1].resource_registry["https://www.nexusmods.com/kotor/mods/2"]
        ["LastUpdateCheck"]
        .is_null());
}

#[test]
fn nexus_client_parses_files_md5_matches_and_key_validation() {
    let (base_url, server) = nexus_server(vec![
        r#"{"files":[{"file_id":7,"file_name":"One.zip","version":"1.0","category_name":"MAIN","md5":"ABCDEF"}]}"#,
        r#"[{"mod":{"mod_id":12,"name":"One","version":"2.0"},"file_details":{"file_id":7,"file_name":"One.zip","md5":null}}]"#,
        r#"{"name":"tester","is_premium":true}"#,
    ]);
    let mut client =
        updates::NexusClient::with_base_url(Some("test-key".into()), &base_url).unwrap();

    let files = client.get_mod_files("kotor", "12").unwrap();
    let matches = client.md5_search("kotor", "ABCDEF").unwrap();
    let validation = client.validate().unwrap();
    let requests = server.join().unwrap();

    assert_eq!(files[0].md5, "abcdef");
    assert_eq!(matches[0].mod_info.as_ref().unwrap().version, "2.0");
    assert_eq!(matches[0].file.as_ref().unwrap().md5, "");
    assert!(validation.is_valid && validation.is_premium);
    assert_eq!(validation.user_name, "tester");
    assert!(requests[0].contains("/v1/games/kotor/mods/12/files.json"));
    assert!(requests[1].contains("/v1/games/kotor/mods/md5_search/abcdef.json"));
    assert!(requests[2].contains("/v1/users/validate.json"));
}

#[test]
fn nexus_validation_and_md5_inputs_fail_closed_without_a_request() {
    let mut client = updates::NexusClient::new(None).unwrap();
    assert!(!client.validate().unwrap().is_valid);
    assert!(client.md5_search("kotor", " ").is_err());
}

#[test]
fn nxm_link_resolves_metadata_and_authorized_https_mirror() {
    let (base_url, server) = nexus_server(vec![
        r#"{"file_id":34,"file_name":"Example.zip"}"#,
        r#"[{"URI":"https://mirror.example/Example.zip"}]"#,
    ]);
    let mut client =
        updates::NexusClient::with_base_url(Some("test-key".into()), &base_url).unwrap();
    let link = modsync::protocol::parse(
        "nxm://kotor/mods/12/files/34?key=a%20b&expires=99",
    )
    .unwrap();
    let result = client.nxm_download_link(&link).unwrap();
    let requests = server.join().unwrap();
    assert_eq!(result.file_name, "Example.zip");
    assert_eq!(result.uri, "https://mirror.example/Example.zip");
    assert!(requests[0].contains("/v1/games/kotor/mods/12/files/34.json"));
    assert!(requests[1].contains("download_link.json?key=a%20b&expires=99"));
    assert!(requests.iter().all(|request| request.contains("apikey: test-key")));
}

#[test]
fn component_badge_reflects_any_resource_update_flag() {
    let guide = Guide::parse(
        r#"[{"Guid":"one","Name":"One","ResourceRegistry":{"https://example.com/a":{"UpdateAvailable":false},"https://www.nexusmods.com/kotor/mods/1":{"UpdateAvailable":true}}}]"#,
        "json",
    )
    .unwrap();
    assert!(updates::component_update_available(&guide.components[0]));
}
