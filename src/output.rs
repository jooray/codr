use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
    process::Stdio,
    time::Duration,
};
use tokio::process::Command;

pub mod claude;
pub mod codex;
mod native;
pub mod tasks;

pub fn supported(agent: &str) -> bool {
    matches!(agent, "opencode" | "codex" | "claude")
}

pub async fn read_session(
    config: &crate::config::BridgeConfig,
    session: &crate::herdr::Session,
) -> Result<Vec<FinalMessage>> {
    Ok(read_history(config, session).await?.finals)
}

#[derive(Debug, Default)]
pub struct SessionHistory {
    pub finals: Vec<FinalMessage>,
    pub latest_turn: Option<String>,
    pub turns: Vec<String>,
}

pub async fn read_history(
    config: &crate::config::BridgeConfig,
    session: &crate::herdr::Session,
) -> Result<SessionHistory> {
    ensure!(
        session.kind == "id",
        "Unsupported native session reference kind"
    );
    match session.agent.as_str() {
        "opencode" => Ok(select_history(
            export_data(&config.opencode_bin, &session.value).await?,
        )),
        "codex" | "claude" => {
            let provider = session.agent.clone();
            let id = session.value.clone();
            let home = if provider == "codex" {
                config.codex_home.clone()
            } else {
                config.claude_home.clone()
            };
            tokio::task::spawn_blocking(move || {
                if provider == "codex" {
                    codex::snapshot(&home, &id).map(|(history, _)| history)
                } else {
                    claude::snapshot(&home, &id).map(|(history, _)| history)
                }
            })
            .await
            .map_err(|_| anyhow::anyhow!("Native transcript reader failed"))?
        }
        _ => Ok(SessionHistory::default()),
    }
}

#[derive(Debug)]
pub struct FinalMessage {
    pub id: String,
    pub text: String,
    pub turn: Option<String>,
}

// Unknown fields include tool outputs, attachments and token accounting. Serde skips
// them directly from the reader instead of retaining the complete export in memory.
#[derive(Deserialize)]
struct Export {
    messages: Vec<Message>,
}
#[derive(Deserialize)]
struct Message {
    info: Info,
    parts: Vec<Part>,
}
#[derive(Deserialize)]
struct Info {
    id: String,
    role: String,
    #[serde(default)]
    time: MessageTime,
    finish: Option<String>,
    error: Option<Value>,
    #[serde(default)]
    summary: Value,
    #[serde(default, rename = "parentID")]
    parent_id: Option<String>,
}
#[derive(Default, Deserialize)]
struct MessageTime {
    completed: Option<u64>,
}
#[derive(Deserialize)]
struct Part {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
    #[serde(default)]
    synthetic: bool,
    #[serde(default)]
    ignored: bool,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    state: Value,
}

pub fn final_messages_from_reader(reader: impl Read) -> Result<Vec<FinalMessage>> {
    select_finals(serde_json::from_reader(reader)?)
}

pub fn final_messages(export: &Value) -> Result<Vec<FinalMessage>> {
    select_finals(serde_json::from_value(export.clone())?)
}

fn select_finals(export: Export) -> Result<Vec<FinalMessage>> {
    Ok(select_history(export).finals)
}

fn select_history(export: Export) -> SessionHistory {
    let mut out = vec![];
    let mut latest_turn = None;
    let mut turns = std::collections::BTreeSet::new();
    let mut has_user = false;
    for message in export.messages {
        let info = message.info;
        if info.role == "user" {
            latest_turn = Some(format!("opencode:{}", info.id));
            turns.insert(latest_turn.clone().unwrap());
            has_user = true;
            continue;
        }
        let turn = info
            .parent_id
            .as_ref()
            .map(|id| format!("opencode:{id}"))
            .or_else(|| latest_turn.clone());
        if let Some(turn) = &turn {
            turns.insert(turn.clone());
        }
        if !has_user && turn.is_some() {
            latest_turn = turn.clone();
        }
        if info.role != "assistant"
            || info.time.completed.is_none()
            || info.finish.as_deref() != Some("stop")
            || info.error.is_some()
            || info.summary == true
        {
            continue;
        }
        let text = message
            .parts
            .into_iter()
            .filter(|p| p.kind == "text" && !p.synthetic && !p.ignored)
            .filter_map(|p| p.text)
            .collect::<Vec<_>>()
            .join("\n");
        if !text.trim().is_empty() {
            out.push(FinalMessage {
                id: info.id,
                text,
                turn,
            });
        }
    }
    SessionHistory {
        finals: out,
        latest_turn,
        turns: turns.into_iter().collect(),
    }
}

pub async fn export(bin: &Path, session: &str) -> Result<Vec<FinalMessage>> {
    select_finals(export_data(bin, session).await?)
}

async fn export_data(bin: &Path, session: &str) -> Result<Export> {
    // A regular file avoids OpenCode's observed stdout-pipe truncation. The file is anonymous and owner-only.
    let mut file = tempfile::tempfile()?;
    let mut command = Command::new(bin);
    command
        .args(["--pure", "export", session])
        .stdin(Stdio::null())
        .stdout(file.try_clone()?)
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let status = tokio::time::timeout(Duration::from_secs(30), command.status()).await??;
    ensure!(status.success(), "OpenCode export failed");
    ensure!(
        file.metadata()?.len() <= 128 * 1024 * 1024,
        "OpenCode export exceeds 128 MiB"
    );
    file.seek(SeekFrom::Start(0))?;
    Ok(serde_json::from_reader(BufReader::new(file))?)
}
