use anyhow::Result;
use clap::{Parser, Subcommand};
use herdr_white_noise_bridge::{BridgeConfig, BridgeState, bridge::Bridge, state};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "White Noise remote for local HERDR agent sessions")]
struct Args {
    #[arg(long)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    /// Run in the foreground. HERDR starts this worker through the plugin manifest.
    Run,
    /// Print public routing state without message contents.
    Status,
    /// Resolve an uncertain creation. Inspect MDK groups before using --retry.
    ResolveCreate {
        key: String,
        #[arg(long, conflicts_with = "retry")]
        group: Option<String>,
        #[arg(long)]
        retry: bool,
    },
    /// Send a text notice to all mapped groups.
    Notify {
        text: String,
        #[arg(long)]
        group: Option<String>,
    },
    /// Clear an uncertain prompt after inspecting the live agent. Does not resubmit it.
    ClearPrompt { key: String },
    /// Rename mapped groups to HDR | repository using the current HERDR working directories.
    RenameGroups,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config_path = args.config.unwrap_or_else(|| {
        let root = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
            });
        root.join("herdr-white-noise-bridge/config.toml")
    });
    let config = BridgeConfig::load(&config_path)?;
    if matches!(args.command, Some(Action::Status)) {
        let s = BridgeState::load(&config.state_path)?;
        for (key, m) in s.mappings {
            println!(
                "{}",
                serde_json::json!({"session":key,"group":m.group_id_hex,"creating_uncertain":m.creating,"active":m.active,"state":m.last_agent_status,"pending_prompt":m.pending_prompt})
            );
        }
        return Ok(());
    }
    let _lock = state::lock(&config.state_path)?;
    let mut bridge = Bridge::open(config)?;
    match args.command.unwrap_or(Action::Run) {
        Action::Run => bridge.run().await,
        Action::Notify { text, group } => {
            let groups: Vec<_> = bridge
                .state
                .mappings
                .values()
                .filter_map(|m| m.group_id_hex.clone())
                .filter(|id| group.as_ref().is_none_or(|g| g == id))
                .collect();
            anyhow::ensure!(!groups.is_empty(), "No mapped groups exist yet");
            for group in groups {
                bridge.state.enqueue(&group, &text);
            }
            bridge.save()?;
            bridge.flush().await
        }
        Action::ResolveCreate { key, group, retry } => {
            let m = bridge
                .state
                .mappings
                .get_mut(&key)
                .ok_or_else(|| anyhow::anyhow!("Unknown session key"))?;
            anyhow::ensure!(
                m.creating && m.group_id_hex.is_none(),
                "This mapping has no uncertain creation"
            );
            anyhow::ensure!(
                group.is_some() || retry,
                "Choose --group or --retry after inspecting MDK groups"
            );
            if let Some(group) = group {
                anyhow::ensure!(
                    !group.is_empty() && group.bytes().all(|b| b.is_ascii_hexdigit()),
                    "Invalid group ID"
                );
                m.group_id_hex = Some(group);
            }
            m.creating = false;
            bridge.save()
        }
        Action::ClearPrompt { key } => {
            bridge
                .state
                .mappings
                .get_mut(&key)
                .ok_or_else(|| anyhow::anyhow!("Unknown session key"))?
                .pending_prompt = false;
            bridge.save()
        }
        Action::RenameGroups => bridge.rename_groups().await,
        Action::Status => unreachable!(),
    }
}
