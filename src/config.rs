use anyhow::{Result, ensure};
use bech32::FromBase32;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeConfig {
    pub phone_npub: String,
    pub herdr_socket: PathBuf,
    pub wn_agent_socket: PathBuf,
    pub account_id_hex: String,
    pub state_path: PathBuf,
    #[serde(default = "default_opencode")]
    pub opencode_bin: PathBuf,
}

fn default_opencode() -> PathBuf {
    "opencode".into()
}

pub fn phone_hex(npub: &str) -> Result<String> {
    let (prefix, data, variant) = bech32::decode(npub)?;
    let bytes = Vec::<u8>::from_base32(&data)?;
    ensure!(
        prefix == "npub" && variant == bech32::Variant::Bech32 && bytes.len() == 32,
        "Invalid phone npub"
    );
    Ok(hex::encode(bytes))
}

impl BridgeConfig {
    pub fn from_toml(text: &str) -> Result<Self> {
        let config: Self = toml::from_str(text)?;
        phone_hex(&config.phone_npub)?;
        ensure!(
            phone_hex(&config.phone_npub)? != config.account_id_hex,
            "The bridge requires its own account, separate from the phone"
        );
        ensure!(
            config.account_id_hex.len() == 64
                && config
                    .account_id_hex
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "Invalid bridge account ID"
        );
        for path in [
            &config.herdr_socket,
            &config.wn_agent_socket,
            &config.state_path,
        ] {
            ensure!(path.is_absolute(), "Configuration paths must be absolute");
        }
        Ok(config)
    }

    pub fn load(path: &Path) -> Result<Self> {
        Self::from_toml(&std::fs::read_to_string(path)?)
    }
}
