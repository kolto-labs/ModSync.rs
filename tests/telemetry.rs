use modsync::telemetry::{self, Config, Event, Export};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    thread,
};

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

#[test]
fn telemetry_export_requires_consent_and_signs_the_sent_event() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let mut config = Config {
        enabled: true,
        endpoint: format!("http://{address}"),
        session_id: "session-1".into(),
        signing_secret: Some("secret".into()),
        ..Default::default()
    };
    let event = Event {
        name: "guide.loaded".into(),
        timestamp: 1,
        attributes: BTreeMap::new(),
    };
    assert_eq!(
        telemetry::export(&config, "/v1/events", &event).unwrap(),
        Export::Disabled
    );
    config.user_consented = true;
    let server = thread::spawn(move || {
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(connection) => break connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => thread::yield_now(),
                Err(error) => panic!("{error}"),
            }
        };
        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = stream.read(&mut buffer).unwrap();
            request.extend_from_slice(&buffer[..count]);
        }
        let (header_end, content_length) = {
            let headers = String::from_utf8_lossy(&request);
            (
                headers.find("\r\n\r\n").unwrap() + 4,
                headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .unwrap()
                    .parse::<usize>()
                    .unwrap(),
            )
        };
        while request.len() < header_end + content_length {
            let count = stream.read(&mut buffer).unwrap();
            request.extend_from_slice(&buffer[..count]);
        }
        let request = String::from_utf8(request).unwrap();
        assert!(request.contains("POST /v1/events HTTP/1.1"));
        assert!(request.contains("x-kms-session-id: session-1"));
        assert!(request.contains("x-kms-signature: "));
        assert!(request.contains("guide.loaded"));
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
    });
    assert_eq!(
        telemetry::export(&config, "/v1/events", &event).unwrap(),
        Export::Sent
    );
    server.join().unwrap();
}
