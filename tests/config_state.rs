use bech32::ToBase32;
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

#[test]
fn native_transcript_roots_default_for_old_configs_and_reject_relative_overrides() {
    let owner = bech32::encode("npub", [0x11u8; 32].to_base32(), bech32::Variant::Bech32).unwrap();
    let base = format!(
        "phone_npub = {owner:?}\nherdr_socket = '/run/herdr.sock'\nwn_agent_socket = '/run/wn.sock'\naccount_id_hex = '{}'\nstate_path = '/tmp/state.json'\n",
        "aa".repeat(32)
    );
    let config = BridgeConfig::from_toml(&base).unwrap();
    assert!(config.codex_home.is_absolute());
    assert!(config.claude_home.is_absolute());
    assert!(config.opencode_db.is_none());
    assert!(BridgeConfig::from_toml(&(base.clone() + "opencode_db = 'relative.db'\n")).is_err());
    assert!(BridgeConfig::from_toml(&(base + "codex_home = 'relative'\n")).is_err());
}
