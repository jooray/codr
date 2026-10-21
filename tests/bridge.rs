use bech32::ToBase32;
use herdr_white_noise_bridge::{
    BridgeConfig, BridgeState,
    bridge::Bridge,
    herdr::Agent,
    state::Mapping,
    wire::{Socket, read_line},
};
use serde_json::{Value, json};
use std::{
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
};
use tokio::{io::AsyncWriteExt, net::UnixListener};

// Synthetic test identity. Never use it as an installation owner.
const HEX: &str = "1111111111111111111111111111111111111111111111111111111111111111";

fn phone_npub() -> String {
    bech32::encode("npub", [0x11u8; 32].to_base32(), bech32::Variant::Bech32).unwrap()
}

fn config(dir: &std::path::Path) -> BridgeConfig {
    BridgeConfig {
        phone_npub: phone_npub(),
        herdr_socket: dir.join("herdr.sock"),
        wn_agent_socket: dir.join("wn.sock"),
        account_id_hex: "aa".repeat(32),
        state_path: dir.join("state.json"),
        opencode_bin: dir.join("opencode"),
        codex_home: dir.join("codex"),
        claude_home: dir.join("claude"),
    }
}
fn agent() -> Agent {
    serde_json::from_value(json!({"pane_id":"w1:p1","terminal_id":"term1","tab_id":"w1:t1","agent":"opencode","agent_status":"idle","state_change_seq":1,"agent_session":{"agent":"opencode","source":"herdr:opencode","kind":"id","value":"ses_1"}})).unwrap()
}
fn inbound(sender: &str, id: &str) -> Value {
    json!({"marmot_agent_control":"marmot.agent-control.v2","type":"inbound_message","account_id_hex":"aa".repeat(32),"group_id_hex":"bb".repeat(16),"message":{"message_id_hex":id,"sender":{"account_id_hex":sender},"text":"test prompt","recorded_at":u64::MAX}})
}

async fn peer(
    path: &std::path::Path,
    marmot: bool,
    calls: Arc<Mutex<Vec<Value>>>,
) -> tokio::task::JoinHandle<()> {
    peer_for_agent(path, marmot, calls, agent()).await
}

async fn peer_for_agent(
    path: &std::path::Path,
    marmot: bool,
    calls: Arc<Mutex<Vec<Value>>>,
    observed: Agent,
) -> tokio::task::JoinHandle<()> {
    let listener = UnixListener::bind(path).unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let mut reader = tokio::io::BufReader::new(stream);
            let req = read_line(&mut reader).await.unwrap();
            calls.lock().unwrap().push(req.clone());
            let mut response = if marmot {
                match req["type"].as_str().unwrap() {
                    "group_create" => {
                        json!({"type":"group_created","group_id_hex":"bb".repeat(16)})
                    }
                    "send_final" => {
                        json!({"type":"final_sent","message_ids_hex":["cc".repeat(32)]})
                    }
                    _ => panic!("unexpected request"),
                }
            } else {
                match req["method"].as_str().unwrap() {
                    "session.snapshot" => {
                        json!({"result":{"snapshot":{"agents":[observed],"tabs":[{"tab_id":observed.tab_id,"label":"Review","number":1}]}}})
                    }
                    "agent.get" => json!({"result":{"agent":observed}}),
                    "agent.prompt" => json!({"result":{"type":"agent_prompted","agent":observed}}),
                    _ => panic!("unexpected method"),
                }
            };
            response["id"] = req["id"].clone();
            if marmot {
                response["marmot_agent_control"] = json!("marmot.agent-control.v2");
            }
            let mut bytes = serde_json::to_vec(&response).unwrap();
            bytes.push(b'\n');
            reader.get_mut().write_all(&bytes).await.unwrap();
        }
    })
}

fn export_script(path: &std::path::Path, text: &str, id: &str) {
    let data = json!({"messages":[{"info":{"id":id,"role":"assistant","time":{"completed":1},"finish":"stop"},"parts":[{"type":"text","text":text}]}]});
    std::fs::write(path, format!("#!/bin/sh\nprintf '%s' '{}'\n", data)).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}

#[tokio::test]
async fn real_socket_round_trip_routes_one_group_and_only_new_final_answers() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    export_script(&config.opencode_bin, "historical private answer", "old");
    let calls = Arc::new(Mutex::new(vec![]));
    let mdk = peer(&config.wn_agent_socket, true, calls.clone()).await;
    let herdr = peer(&config.herdr_socket, false, calls.clone()).await;
    let mut bridge = Bridge::open(config.clone()).unwrap();
    bridge.reconcile(vec![agent()]).await.unwrap();
    bridge.flush().await.unwrap();
    bridge.reconcile(vec![agent()]).await.unwrap();
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["type"] == "group_create")
            .count(),
        1
    );
    assert!(
        calls
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["type"] == "send_final")
            .all(|r| !r["text"]
                .as_str()
                .unwrap()
                .contains("historical private answer"))
    );
    bridge
        .inbound(&inbound(&"ff".repeat(32), "evil"))
        .await
        .unwrap();
    bridge.inbound(&inbound(HEX, "m1")).await.unwrap();
    bridge.inbound(&inbound(HEX, "m1")).await.unwrap();
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["method"] == "agent.prompt")
            .count(),
        1
    );
    export_script(&config.opencode_bin, "new final", "new");
    let mut a = agent();
    a.state_change_seq = 3;
    bridge.reconcile(vec![a.clone()]).await.unwrap();
    bridge.flush().await.unwrap();
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["text"] == "new final")
            .count(),
        1
    );
    let mut restarted = Bridge::open(config).unwrap();
    restarted.reconcile(vec![a]).await.unwrap();
    restarted.flush().await.unwrap();
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["text"] == "new final")
            .count(),
        1
    );
    mdk.abort();
    herdr.abort();
}

#[tokio::test]
async fn working_notice_remains_but_done_only_forwards_the_final_reply() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    let mut bridge = Bridge::open(config.clone()).unwrap();
    bridge.state.mappings.insert(
        agent().key(),
        Mapping {
            group_id_hex: Some("bb".repeat(16)),
            active: true,
            last_agent_status: "idle".into(),
            last_state_change_seq: 1,
            final_ids: vec!["old".into()],
            pending_prompt: true,
            agent_record: Some(agent()),
            ..Mapping::default()
        },
    );
    let mut working = agent();
    working.agent_status = "working".into();
    working.state_change_seq = 2;
    bridge.reconcile(vec![working.clone()]).await.unwrap();
    bridge.reconcile(vec![working]).await.unwrap();
    export_script(&config.opencode_bin, "Completed answer", "new");
    let mut done = agent();
    done.agent_status = "done".into();
    done.state_change_seq = 3;
    bridge.reconcile(vec![done.clone()]).await.unwrap();
    bridge.reconcile(vec![done]).await.unwrap();
    let texts: Vec<_> = bridge
        .state
        .outbox
        .iter()
        .map(|m| m.text.as_str())
        .collect();
    assert_eq!(
        texts
            .iter()
            .filter(|text| **text == "State: working")
            .count(),
        1
    );
    assert!(!texts.contains(&"State: done"));
    assert_eq!(
        texts
            .iter()
            .filter(|text| **text == "Completed answer")
            .count(),
        1
    );
    assert_eq!(
        bridge.state.mappings[&agent().key()].last_agent_status,
        "done"
    );
    assert!(!bridge.state.mappings[&agent().key()].pending_prompt);
    assert!(!bridge.state.mappings[&agent().key()].completion_pending);
}

#[tokio::test]
async fn codex_session_round_trip_forwards_only_new_native_final_replies() {
    native_round_trip("codex", false).await;
}

#[tokio::test]
async fn claude_session_round_trip_forwards_only_new_native_final_replies() {
    native_round_trip("claude", false).await;
}

#[tokio::test]
async fn existing_native_groups_seed_history_once_without_recreating_the_group() {
    native_round_trip("codex", true).await;
    native_round_trip("claude", true).await;
}

async fn native_round_trip(provider: &str, legacy_mapping: bool) {
    const ID: &str = "11111111-1111-4111-8111-111111111111";
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    let mut observed = agent();
    observed.agent = provider.into();
    observed.agent_session = Some(herdr_white_noise_bridge::herdr::Session {
        source: format!("herdr:{provider}"),
        agent: provider.into(),
        kind: "id".into(),
        value: ID.into(),
    });
    let path = if provider == "codex" {
        config
            .codex_home
            .join("sessions/2026/01/01")
            .join(format!("rollout-test-{ID}.jsonl"))
    } else {
        config
            .claude_home
            .join("projects/project")
            .join(format!("{ID}.jsonl"))
    };
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let write = |new_reply: bool| {
        let mut rows = if provider == "codex" {
            vec![
                json!({"type":"session_meta","payload":{"id":ID}}),
                json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"old","last_agent_message":"Native old final"}}),
            ]
        } else {
            vec![
                json!({"type":"user","uuid":"u0","sessionId":ID,"message":{"role":"user","content":"Old prompt"}}),
                json!({"type":"assistant","uuid":"old","parentUuid":"u0","sessionId":ID,"message":{"role":"assistant","id":"api-old","stop_reason":"end_turn","content":[{"type":"text","text":"Native old final"}]}}),
            ]
        };
        if new_reply {
            if provider == "codex" {
                rows.push(json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"new","last_agent_message":"Native new final"}}));
            } else {
                rows.extend([
                json!({"type":"user","uuid":"u1","parentUuid":"old","sessionId":ID,"message":{"role":"user","content":"New prompt"}}),
                json!({"type":"assistant","uuid":"new","parentUuid":"u1","sessionId":ID,"message":{"role":"assistant","id":"api-new","stop_reason":"end_turn","content":[{"type":"text","text":"Native new final"}]}}),
            ]);
            }
        }
        std::fs::write(
            &path,
            rows.into_iter()
                .map(|row| row.to_string() + "\n")
                .collect::<String>(),
        )
        .unwrap();
    };
    write(false);
    let calls = Arc::new(Mutex::new(vec![]));
    let mdk = peer(&config.wn_agent_socket, true, calls.clone()).await;
    let herdr = peer_for_agent(&config.herdr_socket, false, calls.clone(), observed.clone()).await;
    let mut bridge = Bridge::open(config.clone()).unwrap();
    if legacy_mapping {
        bridge.state.mappings.insert(
            observed.key(),
            Mapping {
                group_id_hex: Some("bb".repeat(16)),
                active: true,
                agent_record: Some(observed.clone()),
                ..Mapping::default()
            },
        );
    }
    bridge.reconcile(vec![observed.clone()]).await.unwrap();
    bridge.flush().await.unwrap();
    bridge.inbound(&inbound(HEX, "new-prompt")).await.unwrap();
    assert!(bridge.state.mappings[&observed.key()].pending_prompt);
    write(true);
    observed.agent_status = "done".into();
    observed.state_change_seq += 1;
    bridge.reconcile(vec![observed.clone()]).await.unwrap();
    bridge.flush().await.unwrap();
    assert!(!bridge.state.mappings[&observed.key()].pending_prompt);
    let mut restarted = Bridge::open(config).unwrap();
    restarted.reconcile(vec![observed]).await.unwrap();
    restarted.flush().await.unwrap();
    let calls = calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .filter(|call| call["method"] == "agent.prompt")
            .count(),
        1
    );
    assert_eq!(
        calls
            .iter()
            .filter(|call| call["text"] == "Native new final")
            .count(),
        1
    );
    assert!(
        !calls
            .iter()
            .any(|call| call["text"] == "Native old final" || call["text"] == "State: done")
    );
    if legacy_mapping {
        assert!(!calls.iter().any(|call| call["type"] == "group_create"));
    }
    mdk.abort();
    herdr.abort();
}

#[tokio::test]
async fn legacy_native_session_that_exited_before_upgrade_does_not_replay_history() {
    const ID: &str = "11111111-1111-4111-8111-111111111111";
    for provider in ["codex", "claude"] {
        let directory = tempfile::tempdir().unwrap();
        let config = config(directory.path());
        let mut observed = agent();
        observed.agent = provider.into();
        observed.agent_session = Some(herdr_white_noise_bridge::herdr::Session {
            source: format!("herdr:{provider}"),
            agent: provider.into(),
            kind: "id".into(),
            value: ID.into(),
        });
        let (path, rows) = if provider == "codex" {
            (
                config
                    .codex_home
                    .join("sessions/2026/01/01")
                    .join(format!("rollout-test-{ID}.jsonl")),
                vec![
                    json!({"type":"session_meta","payload":{"id":ID}}),
                    json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"old","last_agent_message":"Historical answer"}}),
                ],
            )
        } else {
            (
                config
                    .claude_home
                    .join("projects/project")
                    .join(format!("{ID}.jsonl")),
                vec![
                    json!({"type":"assistant","uuid":"old","sessionId":ID,"message":{"id":"api-old","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"Historical answer"}]}}),
                ],
            )
        };
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            rows.into_iter()
                .map(|row| row.to_string() + "\n")
                .collect::<String>(),
        )
        .unwrap();
        let key = observed.key();
        let mut bridge = Bridge::open(config).unwrap();
        bridge.state.mappings.insert(
            key.clone(),
            Mapping {
                group_id_hex: Some("bb".repeat(16)),
                active: true,
                agent_record: Some(observed),
                ..Mapping::default()
            },
        );
        bridge.reconcile(vec![]).await.unwrap();
        assert_eq!(bridge.state.outbox.len(), 1);
        assert!(bridge.state.outbox[0].text.starts_with("Session ended"));
        assert!(bridge.state.mappings[&key].output_initialised);
        assert!(!bridge.state.mappings[&key].active);
    }
}

#[tokio::test]
async fn uncertain_group_creation_is_journalled_and_never_retried_on_restart() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    export_script(&config.opencode_bin, "old", "old");
    let listener = UnixListener::bind(&config.wn_agent_socket).unwrap();
    let first = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let req = read_line(&mut tokio::io::BufReader::new(stream))
            .await
            .unwrap();
        assert_eq!(req["type"], "group_create");
    });
    let mut bridge = Bridge::open(config.clone()).unwrap();
    bridge.reconcile(vec![agent()]).await.unwrap();
    first.await.unwrap();
    assert!(BridgeState::load(&config.state_path).unwrap().mappings[&agent().key()].creating);
    let mut restarted = Bridge::open(config).unwrap();
    restarted.reconcile(vec![agent()]).await.unwrap();
    assert!(
        restarted.state.mappings[&agent().key()]
            .group_id_hex
            .is_none()
    );
}

#[test]
fn state_is_private_and_utf8_chunks_preserve_all_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let mut state = BridgeState::default();
    let text = "🙂".repeat(16000);
    state.enqueue("group", &text);
    state.save(&path).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(state.outbox.iter().all(|i| i.text.len() <= 30000));
    assert_eq!(
        state
            .outbox
            .iter()
            .map(|i| i.text.as_str())
            .collect::<String>(),
        text
    );
}

#[tokio::test]
async fn response_id_mismatch_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream
            .write_all(b"{\"id\":\"wrong\",\"result\":{}}\n")
            .await
            .unwrap();
    });
    assert!(Socket { path }.herdr("ping", json!({})).await.is_err());
    task.await.unwrap();
}

#[tokio::test]
async fn ended_groups_do_not_route_to_a_replacement_agent() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    let mut bridge = Bridge::open(config).unwrap();
    bridge.state.mappings.insert(
        agent().key(),
        Mapping {
            group_id_hex: Some("bb".repeat(16)),
            active: true,
            ..Mapping::default()
        },
    );
    bridge.reconcile(vec![]).await.unwrap();
    bridge.inbound(&inbound(HEX, "later")).await.unwrap();
    assert!(
        bridge
            .state
            .outbox
            .iter()
            .any(|m| m.text.contains("No prompt was sent"))
    );
}

#[tokio::test]
async fn a_missing_connector_before_submission_retries_when_it_appears() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    export_script(&config.opencode_bin, "old", "old");
    let mut bridge = Bridge::open(config.clone()).unwrap();
    bridge.reconcile(vec![agent()]).await.unwrap();
    assert!(!bridge.state.mappings[&agent().key()].creating);
    let calls = Arc::new(Mutex::new(vec![]));
    let server = peer(&config.wn_agent_socket, true, calls.clone()).await;
    bridge.reconcile(vec![agent()]).await.unwrap();
    assert!(bridge.state.mappings[&agent().key()].group_id_hex.is_some());
    assert_eq!(calls.lock().unwrap().len(), 1);
    server.abort();
}

#[tokio::test]
async fn failed_prompt_preflight_does_not_lock_the_group() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    let mut bridge = Bridge::open(config).unwrap();
    bridge.herdr_available = true;
    bridge.agents.insert(agent().key(), agent());
    bridge.state.mappings.insert(
        agent().key(),
        Mapping {
            group_id_hex: Some("bb".repeat(16)),
            active: true,
            ..Mapping::default()
        },
    );
    assert!(bridge.inbound(&inbound(HEX, "m1")).await.is_err());
    assert!(!bridge.state.mappings[&agent().key()].pending_prompt);
    assert!(
        bridge.state.mappings[&agent().key()]
            .seen_messages
            .is_empty()
    );
}

#[tokio::test]
async fn an_outage_defers_phone_input_without_consuming_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::open(config(dir.path())).unwrap();
    bridge.state.mappings.insert(
        agent().key(),
        Mapping {
            group_id_hex: Some("bb".repeat(16)),
            active: true,
            ..Mapping::default()
        },
    );
    assert!(bridge.inbound(&inbound(HEX, "offline")).await.is_err());
    assert!(
        bridge.state.mappings[&agent().key()]
            .seen_messages
            .is_empty()
    );
    assert!(bridge.state.outbox.is_empty());
}

#[tokio::test]
async fn truncated_recovered_input_never_reaches_herdr() {
    let dir = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::open(config(dir.path())).unwrap();
    bridge.herdr_available = true;
    bridge.agents.insert(agent().key(), agent());
    bridge.state.mappings.insert(
        agent().key(),
        Mapping {
            group_id_hex: Some("bb".repeat(16)),
            active: true,
            ..Mapping::default()
        },
    );
    let mut event = inbound(HEX, "long");
    event["message"]["text_truncated"] = json!(true);
    bridge.inbound(&event).await.unwrap();
    assert!(!bridge.state.mappings[&agent().key()].pending_prompt);
    assert!(
        bridge
            .state
            .outbox
            .iter()
            .any(|m| m.text.contains("truncated"))
    );
}

#[tokio::test]
async fn final_output_is_collected_even_if_the_agent_exits_between_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    export_script(&config.opencode_bin, "last answer", "last");
    let mut bridge = Bridge::open(config).unwrap();
    bridge.state.mappings.insert(
        agent().key(),
        Mapping {
            group_id_hex: Some("bb".repeat(16)),
            active: true,
            agent_record: Some(agent()),
            ..Mapping::default()
        },
    );
    bridge.reconcile(vec![]).await.unwrap();
    assert_eq!(bridge.state.outbox[0].text, "last answer");
    assert!(bridge.state.outbox[1].text.starts_with("Session ended"));
    bridge.reconcile(vec![]).await.unwrap();
    assert_eq!(bridge.state.outbox.len(), 2);
}

#[tokio::test]
async fn delayed_native_identity_does_not_create_a_provisional_opencode_group() {
    let dir = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::open(config(dir.path())).unwrap();
    let mut a = agent();
    a.agent_session = None;
    bridge.reconcile(vec![a]).await.unwrap();
    assert!(bridge.state.mappings.is_empty());
}

#[tokio::test]
async fn consumed_message_ids_are_not_evicted_while_history_can_replay_them() {
    let dir = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::open(config(dir.path())).unwrap();
    bridge.herdr_available = true;
    let mut event = inbound(HEX, "original");
    event["message"]["text"] = json!("/status");
    bridge.state.mappings.insert(
        agent().key(),
        Mapping {
            group_id_hex: Some("bb".repeat(16)),
            seen_messages: (0..4096).map(|i| i.to_string()).collect(),
            ..Mapping::default()
        },
    );
    bridge.inbound(&event).await.unwrap();
    assert_eq!(bridge.state.mappings[&agent().key()].seen_messages[0], "0");
    assert_eq!(
        bridge.state.mappings[&agent().key()].seen_messages.len(),
        4097
    );
}

#[tokio::test]
async fn failed_session_export_does_not_starve_a_healthy_session() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    std::fs::write(
        &config.opencode_bin,
        "#!/bin/sh\ncase \"$3\" in bad) exit 1;; esac\nprintf '%s' '{\"messages\":[]}'\n",
    )
    .unwrap();
    std::fs::set_permissions(&config.opencode_bin, std::fs::Permissions::from_mode(0o700)).unwrap();
    let calls = Arc::new(Mutex::new(vec![]));
    let server = peer(&config.wn_agent_socket, true, calls.clone()).await;
    let mut bridge = Bridge::open(config).unwrap();
    let mut bad = agent();
    bad.agent_session.as_mut().unwrap().value = "bad".into();
    bridge.reconcile(vec![bad, agent()]).await.unwrap();
    assert!(bridge.state.mappings[&agent().key()].group_id_hex.is_some());
    server.abort();
}

#[tokio::test]
async fn recovery_uses_real_cursors_pages_all_history_and_recovers_late_deliveries() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    let listener = UnixListener::bind(&config.wn_agent_socket).unwrap();
    let rows: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new((0..60).map(|i|json!({"message_id_hex":format!("{i:064x}"),"recorded_at":100+i,"availability":"available","kind":9,"sender":{"account_id_hex":HEX},"text":"/status","attachments":[]})).collect()));
    let data = rows.clone();
    let server = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let mut reader = tokio::io::BufReader::new(stream);
            let req = read_line(&mut reader).await.unwrap();
            assert_eq!(req["type"], "timeline_list");
            assert!(req.get("after").is_none());
            let mut response = if req["group_id_hex"] == "dd".repeat(16) {
                json!({"type":"error","code":"app_error"})
            } else {
                let rows = data.lock().unwrap();
                let end = if let Some(cursor) = req.get("before") {
                    rows.iter()
                        .position(|row| row["message_id_hex"] == cursor["message_id_hex"])
                        .expect("MDK requires an existing cursor row")
                } else {
                    rows.len()
                };
                let start = end.saturating_sub(50);
                json!({"type":"timeline_page","messages":rows[start..end],"has_more_before":start>0,"has_more_after":false})
            };
            response["id"] = req["id"].clone();
            response["marmot_agent_control"] = json!("marmot.agent-control.v2");
            let mut bytes = serde_json::to_vec(&response).unwrap();
            bytes.push(b'\n');
            reader.get_mut().write_all(&bytes).await.unwrap();
        }
    });
    let mut bridge = Bridge::open(config).unwrap();
    bridge.herdr_available = true;
    bridge.state.mappings.insert(
        "healthy".into(),
        Mapping {
            group_id_hex: Some("bb".repeat(16)),
            joined_at: u64::MAX,
            ..Mapping::default()
        },
    );
    bridge.recover_inbound().await.unwrap();
    assert_eq!(bridge.state.mappings["healthy"].seen_messages.len(), 60);
    let mut late = rows.lock().unwrap()[0].clone();
    late["message_id_hex"] = json!("ee".repeat(32));
    late["recorded_at"] = json!(1);
    rows.lock().unwrap().insert(0, late);
    bridge.state.mappings.insert(
        "failing-first".into(),
        Mapping {
            group_id_hex: Some("dd".repeat(16)),
            ..Mapping::default()
        },
    );
    assert!(bridge.recover_inbound().await.is_err());
    assert_eq!(bridge.state.mappings["healthy"].seen_messages.len(), 61);
    server.abort();
}

#[tokio::test]
async fn failed_export_does_not_repeat_a_lifecycle_notice() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    std::fs::write(&config.opencode_bin, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&config.opencode_bin, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut bridge = Bridge::open(config).unwrap();
    bridge.state.mappings.insert(
        agent().key(),
        Mapping {
            group_id_hex: Some("bb".repeat(16)),
            active: true,
            last_agent_status: "working".into(),
            agent_record: Some(agent()),
            ..Mapping::default()
        },
    );
    bridge.reconcile(vec![agent()]).await.unwrap();
    bridge.reconcile(vec![agent()]).await.unwrap();
    assert_eq!(
        bridge
            .state
            .outbox
            .iter()
            .filter(|m| m.text == "State: idle")
            .count(),
        1
    );
}

#[tokio::test]
async fn temporarily_missing_backend_keeps_an_existing_session_present() {
    let dir = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::open(config(dir.path())).unwrap();
    bridge.state.mappings.insert(
        agent().key(),
        Mapping {
            group_id_hex: Some("bb".repeat(16)),
            active: true,
            agent_record: Some(agent()),
            ..Mapping::default()
        },
    );
    let mut pending = agent();
    pending.agent = String::new();
    bridge.reconcile(vec![pending]).await.unwrap();
    assert!(bridge.state.mappings[&agent().key()].active);
    assert!(
        !bridge
            .state
            .outbox
            .iter()
            .any(|m| m.text.starts_with("Session ended"))
    );
}

#[tokio::test]
async fn successful_export_retry_releases_a_completed_prompt_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    std::fs::write(&config.opencode_bin, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&config.opencode_bin, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut bridge = Bridge::open(config.clone()).unwrap();
    bridge.state.mappings.insert(
        agent().key(),
        Mapping {
            group_id_hex: Some("bb".repeat(16)),
            active: true,
            pending_prompt: true,
            last_agent_status: "working".into(),
            agent_record: Some(agent()),
            ..Mapping::default()
        },
    );
    bridge.reconcile(vec![agent()]).await.unwrap();
    assert!(bridge.state.mappings[&agent().key()].completion_pending);
    export_script(&config.opencode_bin, "Recovered final", "final");
    let mut restarted = Bridge::open(config).unwrap();
    restarted.reconcile(vec![agent()]).await.unwrap();
    assert!(!restarted.state.mappings[&agent().key()].pending_prompt);
    assert!(!restarted.state.mappings[&agent().key()].completion_pending);
    assert_eq!(
        restarted
            .state
            .outbox
            .iter()
            .filter(|m| m.text == "Recovered final")
            .count(),
        1
    );
}

#[tokio::test]
async fn unknown_native_identity_never_reuses_an_ended_group_or_consumes_input() {
    let dir = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::open(config(dir.path())).unwrap();
    bridge.state.mappings.insert(
        agent().key(),
        Mapping {
            group_id_hex: Some("bb".repeat(16)),
            active: true,
            agent_record: Some(agent()),
            ..Mapping::default()
        },
    );
    let mut uncertain = agent();
    uncertain.agent_session = None;
    uncertain.agent = "claude".into();
    bridge.reconcile(vec![uncertain]).await.unwrap();
    assert!(bridge.state.mappings[&agent().key()].active);
    assert!(bridge.inbound(&inbound(HEX, "uncertain")).await.is_err());
    assert!(
        bridge.state.mappings[&agent().key()]
            .seen_messages
            .is_empty()
    );
    assert_eq!(bridge.state.mappings.len(), 1);
}

#[tokio::test]
async fn renaming_existing_groups_keeps_their_mapping_and_skips_matching_names() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    let listener = UnixListener::bind(&config.wn_agent_socket).unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let mdk = tokio::spawn(async move {
        let mut subject = "HERDR · OC | Previous chat topic".to_string();
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let mut reader = tokio::io::BufReader::new(stream);
            let req = read_line(&mut reader).await.unwrap();
            captured.lock().unwrap().push(req.clone());
            let mut response = match req["type"].as_str().unwrap() {
                "group_info" => json!({"type":"group_info","subject":subject}),
                "group_profile_update" => {
                    assert_eq!(req["group_id_hex"], "bb".repeat(16));
                    assert!(req.get("description").is_none());
                    subject = req["name"].as_str().unwrap().to_owned();
                    json!({"type":"group_profile_updated","message_ids_hex":["cc".repeat(32)]})
                }
                _ => panic!("Group rename must not create or replace a group"),
            };
            response["id"] = req["id"].clone();
            response["marmot_agent_control"] = json!("marmot.agent-control.v2");
            let mut bytes = serde_json::to_vec(&response).unwrap();
            bytes.push(b'\n');
            reader.get_mut().write_all(&bytes).await.unwrap();
        }
    });
    let herdr = peer(&config.herdr_socket, false, Arc::new(Mutex::new(vec![]))).await;
    let mut bridge = Bridge::open(config).unwrap();
    bridge.state.mappings.insert(
        agent().key(),
        Mapping {
            group_id_hex: Some("bb".repeat(16)),
            agent_record: Some(agent()),
            ..Mapping::default()
        },
    );
    let original = bridge.state.clone();
    bridge.rename_groups().await.unwrap();
    bridge.rename_groups().await.unwrap();
    assert_eq!(bridge.state, original);
    let requests = requests.lock().unwrap();
    let updates: Vec<_> = requests
        .iter()
        .filter(|r| r["type"] == "group_profile_update")
        .collect();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0]["name"], "HDR | Review");
    mdk.abort();
    herdr.abort();
}

#[tokio::test]
async fn renaming_preserves_ended_groups_without_a_known_historical_tab_label() {
    let directory = tempfile::tempdir().unwrap();
    let config = config(directory.path());
    let calls = Arc::new(Mutex::new(vec![]));
    let herdr = peer(&config.herdr_socket, false, calls.clone()).await;
    let mdk = peer(&config.wn_agent_socket, true, calls.clone()).await;
    let mut old = agent();
    old.tab_id.clear();
    old.tab_label = None;
    old.agent_session.as_mut().unwrap().value = "ses_ended".into();
    let mut bridge = Bridge::open(config).unwrap();
    bridge.state.mappings.insert(
        old.key(),
        Mapping {
            group_id_hex: Some("bb".repeat(16)),
            agent_record: Some(old),
            ..Mapping::default()
        },
    );
    bridge.rename_groups().await.unwrap();
    assert!(
        calls
            .lock()
            .unwrap()
            .iter()
            .all(|request| request.get("type").is_none())
    );
    herdr.abort();
    mdk.abort();
}
