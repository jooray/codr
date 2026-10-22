use bech32::ToBase32;
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use tokio::{io::AsyncWriteExt, net::UnixListener};

async fn check_readers(agents: Value, export_fails: bool) -> (std::process::Output, bool) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path();
    let socket = path.join("herdr.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut reader = tokio::io::BufReader::new(stream);
        let request = herdr_white_noise_bridge::wire::read_line(&mut reader)
            .await
            .unwrap();
        assert_eq!(request["method"], "session.snapshot");
        let reply = json!({"id":request["id"],"result":{"snapshot":{"agents":agents,"tabs":[],"workspaces":[]}}});
        reader
            .get_mut()
            .write_all(format!("{reply}\n").as_bytes())
            .await
            .unwrap();
    });
    let cli = path.join("opencode");
    std::fs::write(
        &cli,
        if export_fails {
            "#!/bin/sh\nprintf 'PRIVATE-TRANSCRIPT-CONTENT' >&2\nexit 17\n"
        } else {
            "#!/bin/sh\nprintf '%s\\n' '{\"messages\":[]}'\n"
        },
    )
    .unwrap();
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
    let owner = bech32::encode("npub", [0x11u8; 32].to_base32(), bech32::Variant::Bech32).unwrap();
    let config = path.join("config.toml");
    std::fs::write(&config, format!("phone_npub = {owner:?}\naccount_id_hex = {:?}\nherdr_socket = {:?}\nwn_agent_socket = {:?}\nstate_path = {:?}\nopencode_bin = {:?}\n", "aa".repeat(32), socket, path.join("wn.sock"), path.join("state.json"), cli)).unwrap();
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_herdr-white-noise-bridge"))
        .args(["--config", config.to_str().unwrap(), "check-readers"])
        .output()
        .await
        .unwrap();
    server.abort();
    (output, path.join("state.json").exists())
}

#[tokio::test]
async fn reader_check_reports_when_validation_requires_a_live_session_without_changing_state() {
    let (output, state_exists) = check_readers(json!([]), false).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["checked_sessions"],
        0
    );
    assert!(!state_exists);
}

#[tokio::test]
async fn reader_check_fails_without_exposing_transcript_or_cli_error_text() {
    let agent = json!({"pane_id":"pane","terminal_id":"terminal","agent":"opencode","agent_status":"idle","agent_session":{"source":"herdr:opencode","agent":"opencode","kind":"id","value":"ses_fixture"}});
    let (output, state_exists) = check_readers(json!([agent]), true).await;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Session history check failed"), "{stderr}");
    assert!(!stderr.contains("PRIVATE-TRANSCRIPT-CONTENT"));
    assert!(!state_exists);
}

#[tokio::test]
async fn malformed_snapshot_errors_do_not_echo_received_values() {
    let agent = json!({"pane_id":"pane","terminal_id":"terminal","agent":"opencode","agent_status":"idle","state_change_seq":"PRIVATE-SNAPSHOT-CONTENT","agent_session":{"source":"herdr:opencode","agent":"opencode","kind":"id","value":"ses_fixture"}});
    let (output, state_exists) = check_readers(json!([agent]), false).await;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("PRIVATE-SNAPSHOT-CONTENT"), "{stderr}");
    assert!(stderr.contains("Session history check failed"));
    assert!(!state_exists);
}

#[tokio::test]
async fn reader_check_validates_a_live_session_without_creating_state() {
    let agent = json!({"pane_id":"pane","terminal_id":"terminal","agent":"opencode","agent_status":"idle","agent_session":{"source":"herdr:opencode","agent":"opencode","kind":"id","value":"ses_fixture"}});
    let (output, state_exists) = check_readers(json!([agent]), false).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["checked_sessions"],
        1
    );
    assert!(!state_exists);
}
