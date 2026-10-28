use bech32::ToBase32;
use herdr_white_noise_bridge::{BridgeConfig, bridge::Bridge, state::Mapping, update};
use serde_json::{Value, json};

fn setup(root: &std::path::Path) -> Bridge {
    let config: BridgeConfig = serde_json::from_value(json!({
        "phone_npub": bech32::encode("npub", [0x11u8;32].to_base32(), bech32::Variant::Bech32).unwrap(),
        "account_id_hex":"aa".repeat(32), "herdr_socket":root.join("h.sock"),
        "wn_agent_socket":root.join("w.sock"), "state_path":root.join("state.json")
    })).unwrap();
    let mut bridge = Bridge::open(config).unwrap();
    for (key, group) in [("one", "bb"), ("two", "cc")] {
        bridge.state.mappings.insert(
            key.into(),
            Mapping {
                group_id_hex: Some(group.repeat(16)),
                active: true,
                pending_prompt: true,
                seen_messages: vec!["existing-receipt".into()],
                ..Mapping::default()
            },
        );
    }
    bridge.update_config_path = Some(root.join("config.toml"));
    bridge.save().unwrap();
    bridge
}

fn release() -> update::Release {
    let version = format!(
        "{}.{}.{}",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH").parse::<u64>().unwrap() + 1
    );
    let filename = format!("codr-v{version}-linux-x86_64.tar.gz");
    serde_json::from_value(json!({"version":version, "event_id":"ab".repeat(32), "commit":"cd".repeat(20),
        "asset":{"filename":filename,"url":"https://blossom.example/archive","sha256":"ef".repeat(32),"size":"100"},
        "checksum":{"filename":format!("{filename}.sha256"),"url":"https://blossom.example/checksum","sha256":"12".repeat(32),"size":"100"}})).unwrap()
}

fn event(id: &str, text: &str) -> Value {
    json!({"marmot_agent_control":"marmot.agent-control.v2","type":"inbound_message",
        "account_id_hex":"aa".repeat(32),"group_id_hex":"bb".repeat(16),
        "message":{"message_id_hex":id,"sender":{"account_id_hex":"11".repeat(32)},"text":text}})
}

#[test]
fn offer_is_persistent_chat_bound_and_wrong_or_expired_confirmation_has_no_side_effects() {
    let root = tempfile::tempdir().unwrap();
    let mut bridge = setup(root.path());
    update::store_offer(&mut bridge, "one", &"bb".repeat(16), "check", release()).unwrap();
    let offer = bridge.state.mappings["one"].update_offer.clone().unwrap();
    let config = bridge.config.clone();
    let mut bridge = Bridge::open(config).unwrap();
    bridge.update_config_path = Some(root.path().join("config.toml"));
    assert_eq!(
        bridge.state.mappings["one"]
            .update_offer
            .as_ref()
            .unwrap()
            .code,
        offer.code
    );
    assert!(
        update::prepare(
            &mut bridge,
            "two",
            &"cc".repeat(16),
            "foreign-chat",
            &offer.code
        )
        .is_err()
    );
    assert!(update::prepare(&mut bridge, "one", &"bb".repeat(16), "wrong-code", "wrong").is_err());
    bridge
        .state
        .mappings
        .get_mut("one")
        .unwrap()
        .update_offer
        .as_mut()
        .unwrap()
        .expires_at = 0;
    assert!(update::prepare(&mut bridge, "one", &"bb".repeat(16), "expired", &offer.code).is_err());
    assert!(bridge.state.update_job.is_none());
    assert!(bridge.state.mappings["one"].pending_prompt);
}

#[tokio::test]
async fn confirmation_is_durable_before_launch_and_completion_goes_only_to_original_chat() {
    let root = tempfile::tempdir().unwrap();
    let mut bridge = setup(root.path());
    update::store_offer(&mut bridge, "one", &"bb".repeat(16), "check", release()).unwrap();
    let code = bridge.state.mappings["one"]
        .update_offer
        .as_ref()
        .unwrap()
        .code
        .clone();
    let directory =
        update::prepare(&mut bridge, "one", &"bb".repeat(16), "confirm", &code).unwrap();
    assert!(bridge.state.mappings["one"].update_offer.is_none());
    assert!(
        bridge.state.mappings["one"]
            .seen_messages
            .iter()
            .any(|id| id == "confirm")
    );
    let job = bridge.state.update_job.clone().unwrap();
    let mut bridge = Bridge::open(bridge.config.clone()).unwrap();
    assert!(bridge.state.update_job.is_some());
    assert!(update::prepare(&mut bridge, "one", &"bb".repeat(16), "repeat", &code).is_err());
    update::ready(&bridge).unwrap();
    let ready: Value =
        serde_json::from_slice(&std::fs::read(directory.join("ready.json")).unwrap()).unwrap();
    assert_eq!(ready["job_id"], job.id);
    assert_eq!(ready["version"], env!("CARGO_PKG_VERSION"));
    std::fs::write(directory.join("result.json"), serde_json::to_vec(&json!({"version":job.version,"job_id":job.id,"status":"installed","message":format!("Updated codr to {}.",job.version)})).unwrap()).unwrap();
    update::poll(&mut bridge).await.unwrap();
    assert!(bridge.state.update_job.is_none());
    assert_eq!(bridge.state.outbox.last().unwrap().group, "bb".repeat(16));
    assert!(bridge.state.mappings["one"].pending_prompt);
    assert!(
        bridge.state.mappings["one"]
            .seen_messages
            .iter()
            .any(|id| id == "existing-receipt")
    );
    let count = bridge.state.outbox.len();
    update::poll(&mut bridge).await.unwrap();
    assert_eq!(bridge.state.outbox.len(), count);
}

#[tokio::test]
async fn update_controls_never_reach_agent_and_reject_other_senders_or_attachments() {
    let root = tempfile::tempdir().unwrap();
    let mut bridge = setup(root.path());
    let mut foreign = event("foreign", "/update cancel");
    foreign["message"]["sender"]["account_id_hex"] = json!("22".repeat(32));
    bridge.inbound(&foreign).await.unwrap();
    assert!(bridge.state.outbox.is_empty());
    let mut attachment = event("attachment", "/update confirm code");
    attachment["message"]["attachments"] = json!([{"media_type":"image/png"}]);
    bridge.inbound(&attachment).await.unwrap();
    assert!(
        bridge
            .state
            .outbox
            .last()
            .unwrap()
            .text
            .contains("standalone")
    );
    bridge
        .inbound(&event("cancel", "/update cancel"))
        .await
        .unwrap();
    assert!(
        bridge
            .state
            .outbox
            .last()
            .unwrap()
            .text
            .contains("cancelled")
    );
    let count = bridge.state.outbox.len();
    bridge
        .inbound(&event("cancel", "/update cancel"))
        .await
        .unwrap();
    assert_eq!(bridge.state.outbox.len(), count);
    assert!(bridge.state.update_job.is_none());
    assert!(bridge.state.mappings["one"].pending_prompt);
}

#[tokio::test]
async fn unsupported_managed_install_does_not_leave_a_false_in_progress_job() {
    let root = tempfile::tempdir().unwrap();
    let mut bridge = setup(root.path());
    update::store_offer(&mut bridge, "one", &"bb".repeat(16), "check", release()).unwrap();
    let code = bridge.state.mappings["one"]
        .update_offer
        .as_ref()
        .unwrap()
        .code
        .clone();
    // The test executable is not the standard installed worker. Preflight must
    // refuse it before making any service-manager or network calls.
    bridge
        .inbound(&event(
            "confirm-unsupported",
            &format!("/update confirm {code}"),
        ))
        .await
        .unwrap();
    assert!(bridge.state.update_job.is_none());
    assert!(
        bridge
            .state
            .outbox
            .last()
            .unwrap()
            .text
            .contains("nothing was installed")
    );
    assert!(bridge.state.mappings["one"].pending_prompt);
}

#[tokio::test]
async fn a_result_for_another_job_cannot_clear_the_pending_installation() {
    let root = tempfile::tempdir().unwrap();
    let mut bridge = setup(root.path());
    update::store_offer(&mut bridge, "one", &"bb".repeat(16), "check", release()).unwrap();
    let code = bridge.state.mappings["one"]
        .update_offer
        .as_ref()
        .unwrap()
        .code
        .clone();
    let directory =
        update::prepare(&mut bridge, "one", &"bb".repeat(16), "confirm", &code).unwrap();
    std::fs::write(
        directory.join("result.json"),
        serde_json::to_vec(&json!({"job_id":"other","version":release().version,"status":"installed","message":"success"})).unwrap(),
    )
    .unwrap();
    assert!(update::poll(&mut bridge).await.is_err());
    let before = bridge.state.update_job.clone();
    bridge
        .inbound(&event("cancel-running", "/update cancel"))
        .await
        .unwrap();
    assert_eq!(bridge.state.update_job, before);
    assert!(bridge.state.outbox.last().unwrap().text.contains("pending"));
}

#[test]
fn failed_confirmation_save_preserves_offer_and_does_not_consume_input() {
    let root = tempfile::tempdir().unwrap();
    let mut bridge = setup(root.path());
    update::store_offer(&mut bridge, "one", &"bb".repeat(16), "check", release()).unwrap();
    let code = bridge.state.mappings["one"]
        .update_offer
        .as_ref()
        .unwrap()
        .code
        .clone();
    let before = serde_json::to_value(&bridge.state).unwrap();
    std::fs::remove_file(&bridge.config.state_path).unwrap();
    std::fs::create_dir(&bridge.config.state_path).unwrap();
    assert!(update::prepare(&mut bridge, "one", &"bb".repeat(16), "confirm", &code).is_err());
    assert_eq!(serde_json::to_value(&bridge.state).unwrap(), before);
}

#[test]
fn a_stale_offer_cannot_reinstall_or_downgrade_the_current_version() {
    let root = tempfile::tempdir().unwrap();
    let mut bridge = setup(root.path());
    let mut offer = release();
    offer.version = env!("CARGO_PKG_VERSION").into();
    update::store_offer(&mut bridge, "one", &"bb".repeat(16), "check", offer).unwrap();
    let code = bridge.state.mappings["one"]
        .update_offer
        .as_ref()
        .unwrap()
        .code
        .clone();
    assert!(update::prepare(&mut bridge, "one", &"bb".repeat(16), "stale", &code).is_err());
    assert!(bridge.state.update_job.is_none());
}
