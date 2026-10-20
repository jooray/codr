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

#[derive(Debug)]
pub struct FinalMessage {
    pub id: String,
    pub text: String,
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
}

pub fn final_messages_from_reader(reader: impl Read) -> Result<Vec<FinalMessage>> {
    select_finals(serde_json::from_reader(reader)?)
}

pub fn final_messages(export: &Value) -> Result<Vec<FinalMessage>> {
    select_finals(serde_json::from_value(export.clone())?)
}

fn select_finals(export: Export) -> Result<Vec<FinalMessage>> {
    let mut out = vec![];
    for message in export.messages {
        let info = message.info;
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
            out.push(FinalMessage { id: info.id, text });
        }
    }
    Ok(out)
}

pub async fn export(bin: &Path, session: &str) -> Result<Vec<FinalMessage>> {
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
    final_messages_from_reader(BufReader::new(file))
}
