use herdr_white_noise_bridge::{BridgeConfig, output};
use serde_json::json;
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
};

#[tokio::test]
async fn service_reads_reuse_history_and_never_launch_the_cli() {
    let root = tempfile::tempdir().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let paths = Arc::new(Mutex::new(Vec::new()));
    let seen = paths.clone();
    let server = std::thread::spawn(move || {
        for _ in 0..7 {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = [0; 8192];
            let size = socket.read(&mut bytes).unwrap();
            let request = String::from_utf8_lossy(&bytes[..size]);
            assert!(request.to_lowercase().contains("authorization: basic "));
            let path = request.split_whitespace().nth(1).unwrap().to_string();
            seen.lock().unwrap().push(path.clone());
            let body = if path.contains("/log?") {
                log(5)
            } else if path.ends_with("/export") {
                export("Hello")
            } else {
                json!({"data":[]}).to_string()
            };
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        }
    });
    let file = root.path().join("service.json");
    std::fs::write(
        &file,
        json!({"url":url,"pid":1,"password":"fixture-only"}).to_string(),
    )
    .unwrap();
    let mut value: toml::Value = toml::from_str(include_str!("../config.example.toml")).unwrap();
    value.as_table_mut().unwrap().insert(
        "opencode_service_file".into(),
        file.to_string_lossy().to_string().into(),
    );
    value["opencode_bin"] = "/nonexistent/cli-must-not-run".into();
    let config: BridgeConfig = value.try_into().unwrap();
    let reader = output::service::Reader::default();
    for _ in 0..2 {
        let snapshot = reader.read(&config, "ses_fixture").await.unwrap();
        assert_eq!(snapshot.history.recent_messages[0].text, "Hello");
    }
    server.join().unwrap();
    let paths = paths.lock().unwrap();
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.ends_with("/export"))
            .count(),
        1
    );
    assert!(
        paths
            .iter()
            .filter(|path| path.contains("/log?"))
            .all(|path| path.ends_with("after=9007199254740991&follow=false"))
    );
    assert!(
        paths
            .iter()
            .filter(|path| path.contains("/log?") || path.ends_with("/export"))
            .all(|path| path.starts_with("/api/experimental/session/ses_fixture/"))
    );
    assert!(
        paths
            .iter()
            .filter(|path| path.ends_with("/form") || path.ends_with("/permission"))
            .all(|path| path.starts_with("/api/session/ses_fixture/"))
    );
}

#[tokio::test]
async fn missing_service_is_retryable_without_cli_fallback() {
    let mut value: toml::Value = toml::from_str(include_str!("../config.example.toml")).unwrap();
    value.as_table_mut().unwrap().insert(
        "opencode_service_file".into(),
        "/nonexistent/service.json".into(),
    );
    value["opencode_bin"] = "/nonexistent/cli-must-not-run".into();
    let config: BridgeConfig = value.try_into().unwrap();
    let reader = output::service::Reader::default();
    for _ in 0..2 {
        assert!(reader.read(&config, "ses_fixture").await.is_err());
    }
    let session = herdr_white_noise_bridge::herdr::Session {
        source: "test".into(),
        agent: "opencode".into(),
        kind: "id".into(),
        value: "ses_fixture".into(),
    };
    assert!(
        output::read_history(&config, &session)
            .await
            .unwrap_err()
            .to_string()
            .contains("registration unavailable")
    );
}

fn config(file: &std::path::Path) -> BridgeConfig {
    let mut value: toml::Value = toml::from_str(include_str!("../config.example.toml")).unwrap();
    value.as_table_mut().unwrap().insert(
        "opencode_service_file".into(),
        file.to_string_lossy().to_string().into(),
    );
    value["opencode_bin"] = "/nonexistent/cli-must-not-run".into();
    value.try_into().unwrap()
}

fn fixture(responses: Vec<(u16, String)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut paths = Vec::new();
        for (status, body) in responses {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut chunk = [0; 1024];
                let size = socket.read(&mut chunk).unwrap();
                assert!(size > 0);
                bytes.extend_from_slice(&chunk[..size]);
                if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            paths.push(
                String::from_utf8_lossy(&bytes)
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .to_string(),
            );
            write!(
                socket,
                "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        }
        paths
    });
    (url, server)
}

fn log(seq: u64) -> String {
    format!(
        "event: message\r\ndata: {}\r\n\r\n",
        json!({"type":"log.synced","aggregateID":"ses_fixture","seq":seq})
    )
}

fn export(text: &str) -> String {
    json!({"data":{"messages":[{"type":"user","id":"msg_1","text":text,"time":{"created":1000}}]}})
        .to_string()
}

#[tokio::test]
async fn changed_history_rebuilds_and_failed_prompts_do_not_advance_cursor() {
    let ok = |body: String| (200, body);
    let empty = || ok(json!({"data":[]}).to_string());
    let (url, server) = fixture(vec![
        ok(log(5)),
        ok(export("Before")),
        empty(),
        empty(),
        ok(log(6)),
        ok(export("After")),
        (503, "unavailable".into()),
        ok(log(6)),
        ok(export("After")),
        empty(),
        empty(),
        ok(log(6)),
        empty(),
        empty(),
    ]);
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("service.json");
    std::fs::write(&file, json!({"url":url,"pid":1}).to_string()).unwrap();
    let config = config(&file);
    let reader = output::service::Reader::default();
    assert_eq!(
        reader
            .read(&config, "ses_fixture")
            .await
            .unwrap()
            .history
            .recent_messages[0]
            .text,
        "Before"
    );
    assert!(reader.read(&config, "ses_fixture").await.is_err());
    for _ in 0..2 {
        assert_eq!(
            reader
                .read(&config, "ses_fixture")
                .await
                .unwrap()
                .history
                .recent_messages[0]
                .text,
            "After"
        );
    }
    let paths = server.join().unwrap();
    for index in [0, 4, 7, 11] {
        assert!(paths[index].ends_with("after=9007199254740991&follow=false"));
    }
}

#[tokio::test]
async fn a_new_registered_owner_rebuilds_history_at_the_same_watermark() {
    let empty = json!({"data":[]}).to_string();
    let (url, server) = fixture(vec![
        (200, log(5)),
        (200, export("Old owner")),
        (200, empty.clone()),
        (200, empty.clone()),
        (200, log(5)),
        (200, export("New owner")),
        (200, empty.clone()),
        (200, empty),
    ]);
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("service.json");
    let config = config(&file);
    let reader = output::service::Reader::default();
    for (pid, text) in [(1, "Old owner"), (2, "New owner")] {
        std::fs::write(&file, json!({"url":url,"pid":pid}).to_string()).unwrap();
        assert_eq!(
            reader
                .read(&config, "ses_fixture")
                .await
                .unwrap()
                .history
                .recent_messages[0]
                .text,
            text
        );
    }
    assert_eq!(
        server
            .join()
            .unwrap()
            .iter()
            .filter(|path| path.ends_with("/export"))
            .count(),
        2
    );
}

#[tokio::test]
async fn invalid_logs_and_http_errors_never_fall_back_to_cli() {
    let (url, server) = fixture(vec![
        (503, "unavailable".into()),
        (200, "".into()),
        (
            200,
            "data: {\"type\":\"log.synced\",\"aggregateID\":\"ses_other\",\"seq\":5}\r\n\r\n".into(),
        ),
        (
            200,
            "data: {\"type\":\"log.synced\",\"aggregateID\":\"ses_fixture\",\"seq\":\"bad\"}\r\n\r\n".into(),
        ),
        (200, log(5) + "data: {}\n\n"),
    ]);
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("service.json");
    std::fs::write(&file, json!({"url":url,"pid":1}).to_string()).unwrap();
    let config = config(&file);
    let reader = output::service::Reader::default();
    for _ in 0..5 {
        assert!(reader.read(&config, "ses_fixture").await.is_err());
    }
    assert!(
        server
            .join()
            .unwrap()
            .iter()
            .all(|path| path.contains("/log?"))
    );
}

#[tokio::test]
async fn registration_rejects_nonlocal_and_credential_bearing_urls() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("service.json");
    let config = config(&file);
    let reader = output::service::Reader::default();
    for url in [
        "http://192.0.2.1:1234",
        "https://127.0.0.1:1234",
        "http://user:secret@127.0.0.1:1234",
        "http://127.0.0.1:1234/path",
    ] {
        std::fs::write(&file, json!({"url":url,"pid":1}).to_string()).unwrap();
        assert_eq!(
            reader
                .read(&config, "ses_fixture")
                .await
                .unwrap_err()
                .to_string(),
            "OpenCode service must use a loopback HTTP address"
        );
    }
}

#[tokio::test]
async fn representative_history_is_read_once_without_replaying_its_event_log() {
    let messages: Vec<_> = (0..5000)
        .map(|i| {
            json!({
                "type":"user", "id":format!("msg_{i}"), "text":"x".repeat(256),
                "time":{"created":1000+i},
            })
        })
        .collect();
    let history = json!({"data":{"messages":messages}}).to_string();
    assert!(history.len() > 1024 * 1024);
    let empty = json!({"data":[]}).to_string();
    let (url, server) = fixture(vec![
        (200, ": heartbeat\r\n\r\n".to_string() + &log(50_000)),
        (200, history),
        (200, empty.clone()),
        (200, empty.clone()),
        (200, log(50_000)),
        (200, empty.clone()),
        (200, empty),
    ]);
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("service.json");
    std::fs::write(&file, json!({"url":url,"pid":1}).to_string()).unwrap();
    let config = config(&file);
    let reader = output::service::Reader::default();
    for _ in 0..2 {
        let snapshot = reader.read(&config, "ses_fixture").await.unwrap();
        assert_eq!(snapshot.history.recent_messages.len(), 3);
        assert_eq!(snapshot.history.last_messages.len(), 100);
        assert_eq!(snapshot.history.last_messages[99].id, "msg_4999");
        assert_eq!(snapshot.history.recent_messages[2].id, "msg_4999");
    }
    let paths = server.join().unwrap();
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.ends_with("/export"))
            .count(),
        1
    );
    assert!(
        paths
            .iter()
            .filter(|path| path.contains("/log?"))
            .all(|path| path.ends_with("after=9007199254740991&follow=false"))
    );
}
