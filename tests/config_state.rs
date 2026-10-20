use herdr_white_noise_bridge::{BridgeConfig, BridgeState};

#[test]
fn config_rejects_an_invalid_phone_npub() {
    let text = r#"phone_npub = "not-an-npub"
herdr_socket = "/run/herdr.sock"
wn_agent_socket = "/run/wn-agent.sock"
account_id_hex = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
state_path = "/tmp/state.json"
"#;

    assert!(BridgeConfig::from_toml(text).is_err());
}

#[test]
fn state_round_trip_keeps_the_session_group_mapping() {
    let state = BridgeState::with_mapping("herdr:codex:session-1", "ab".repeat(16));
    let path = std::env::temp_dir().join(format!(
        "herdr-white-noise-bridge-state-{}-{}.json",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));

    state.save(&path).unwrap();
    assert_eq!(BridgeState::load(&path).unwrap(), state);
    std::fs::remove_file(path).unwrap();
}
