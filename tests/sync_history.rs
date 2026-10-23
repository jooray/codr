use herdr_white_noise_bridge::{BridgeConfig, herdr::Session, output};
use serde_json::json;
use std::{
    fs::{self, File, FileTimes},
    os::unix::fs::PermissionsExt,
    time::{Duration, UNIX_EPOCH},
};

fn config(root: &std::path::Path) -> BridgeConfig {
    serde_json::from_value(json!({
        "phone_npub":"unused", "account_id_hex":"aa", "herdr_socket":root.join("herdr.sock"),
        "wn_agent_socket":root.join("wn.sock"), "state_path":root.join("state.json"),
        "opencode_bin":root.join("opencode"), "codex_home":root.join("codex"),
        "claude_home":root.join("claude")
    }))
    .unwrap()
}

/// Manual long replies keep stable, distinct chunk keys across persistence and retries.
#[test]
fn sync_replies_chunk_utf8_without_loss_or_ordinary_mute_bypass() {
    let mut state = herdr_white_noise_bridge::BridgeState {
        forwarding_enabled: false,
        ..Default::default()
    };
    let text = "🦄".repeat(20000);
    state.enqueue_manual("group", &text, "sync:request:group");
    assert_eq!(
        state
            .outbox
            .iter()
            .map(|item| item.text.as_str())
            .collect::<String>(),
        text
    );
    assert!(
        state
            .outbox
            .iter()
            .all(|item| item.manual && item.text.len() <= 30000)
    );
    assert_eq!(
        state
            .outbox
            .iter()
            .map(|item| &item.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        state.outbox.len()
    );
    let saved = state.outbox.clone();
    state.enqueue("group", "must not be queued");
    assert_eq!(state.outbox, saved);
    let restored: herdr_white_noise_bridge::BridgeState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(restored.outbox, saved);
    let mut legacy = serde_json::to_value(&saved[0]).unwrap();
    legacy.as_object_mut().unwrap().remove("manual");
    let old: herdr_white_noise_bridge::state::Outgoing = serde_json::from_value(legacy).unwrap();
    assert!(!old.manual);
}

/// Both OpenCode export formats use source message clocks, not the export time.
#[tokio::test]
async fn sync_history_uses_opencode_v1_and_v2_source_activity() {
    for data in [
        json!({"messages":[
            {"info":{"id":"old","role":"assistant","time":{"completed":1000},"finish":"stop"},"parts":[{"type":"text","text":"latest reply"}]},
            {"info":{"id":"recent","role":"user","time":{"created":12345000}},"parts":[{"type":"text","text":"new prompt"}]}
        ]}),
        json!({"messages":[
            {"type":"assistant","id":"old","time":{"completed":1000,"streamed":12345000},"finish":"stop","content":[{"type":"text","text":"latest reply"}]},
            {"type":"user","id":"recent","time":{"created":1000},"text":"new prompt"}
        ]}),
        json!({"messages":[
            {"info":{"id":"reply","role":"assistant","time":{"created":1000,"updated":12345000,"completed":1000},"finish":"stop"},"parts":[{"type":"text","text":"latest reply"}]}
        ]}),
    ] {
        let root = tempfile::tempdir().unwrap();
        let config = config(root.path());
        fs::write(root.path().join("data.json"), data.to_string()).unwrap();
        fs::write(
            &config.opencode_bin,
            "#!/bin/sh\ncat \"$(dirname \"$0\")/data.json\"\n",
        )
        .unwrap();
        fs::set_permissions(&config.opencode_bin, fs::Permissions::from_mode(0o700)).unwrap();
        let history = output::read_history(
            &config,
            &Session {
                source: "herdr:opencode".into(),
                agent: "opencode".into(),
                kind: "id".into(),
                value: "ses_test".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(history.activity_at, Some(12345));
        assert_eq!(history.finals.last().unwrap().text, "latest reply");
    }
}

/// Native logs retain their own modification time even after a fresh bridge read.
#[tokio::test]
async fn sync_history_native_activity_comes_from_the_verified_session_log() {
    let session = "00000000-0000-0000-0000-000000000001";
    for agent in ["codex", "claude"] {
        let root = tempfile::tempdir().unwrap();
        let config = config(root.path());
        let (path, rows) = if agent == "codex" {
            (
                config
                    .codex_home
                    .join(format!("sessions/rollout-{session}.jsonl")),
                vec![
                    json!({"type":"session_meta","payload":{"id":session}}),
                    json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn","last_agent_message":"latest reply"}}),
                ],
            )
        } else {
            (
                config
                    .claude_home
                    .join(format!("projects/project/{session}.jsonl")),
                vec![
                    json!({"sessionId":session,"uuid":"user","type":"user","message":{"content":"prompt"}}),
                    json!({"sessionId":session,"uuid":"answer","parentUuid":"user","type":"assistant","message":{"id":"model","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"latest reply"}]}}),
                ],
            )
        };
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            rows.iter()
                .map(|row| format!("{row}\n"))
                .collect::<String>(),
        )
        .unwrap();
        File::open(&path)
            .unwrap()
            .set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(12345)))
            .unwrap();
        let history = output::read_history(
            &config,
            &Session {
                source: format!("herdr:{agent}"),
                agent: agent.into(),
                kind: "id".into(),
                value: session.into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(history.activity_at, Some(12345));
        assert_eq!(history.finals.last().unwrap().text, "latest reply");
    }
}
