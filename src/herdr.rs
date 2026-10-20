use crate::wire::Socket;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::Path,
    process::{Command, Stdio},
};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Session {
    pub source: String,
    pub agent: String,
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Agent {
    pub pane_id: String,
    pub terminal_id: String,
    #[serde(default)]
    pub agent: String,
    pub agent_status: String,
    pub agent_session: Option<Session>,
    #[serde(default)]
    pub state_change_seq: u64,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub foreground_cwd: Option<String>,
    pub terminal_title_stripped: Option<String>,
}

impl Agent {
    pub fn key(&self) -> String {
        match &self.agent_session {
            Some(s) => serde_json::to_string(&(&s.source, &s.agent, &s.kind, &s.value))
                .expect("strings serialize"),
            None => {
                serde_json::to_string(&(&self.terminal_id, &self.agent)).expect("strings serialize")
            }
        }
    }
    pub fn settled(&self) -> bool {
        matches!(self.agent_status.as_str(), "idle" | "done")
    }
    pub fn title(&self) -> String {
        let raw = self
            .terminal_title_stripped
            .as_deref()
            .unwrap_or(&self.agent);
        raw.chars().filter(|c| !c.is_control()).take(60).collect()
    }

    pub fn group_name(&self) -> String {
        let cwd = self
            .foreground_cwd
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.cwd);
        let root = if cwd.is_empty() {
            None
        } else {
            Command::new("git")
                .arg("-C")
                .arg(cwd)
                .args(["rev-parse", "--show-toplevel"])
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
        };
        let path = Path::new(root.as_deref().map(str::trim_end).unwrap_or(cwd));
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("session");
        let name: String = name.chars().filter(|c| !c.is_control()).take(60).collect();
        format!(
            "HDR | {}",
            if name.trim().is_empty() {
                "session"
            } else {
                name.trim()
            }
        )
    }
}

pub async fn snapshot(socket: &Socket) -> Result<Vec<Agent>> {
    let response = socket.herdr("session.snapshot", json!({})).await?;
    let agents = &response["result"]["snapshot"]["agents"];
    ensure!(agents.is_array(), "HERDR did not return a session snapshot");
    Ok(serde_json::from_value(agents.clone())?)
}

pub async fn get(socket: &Socket, agent: &Agent) -> Result<Agent> {
    let response = socket
        .herdr("agent.get", json!({"target":agent.pane_id}))
        .await?;
    let current: Agent = serde_json::from_value(response["result"]["agent"].clone())?;
    ensure!(
        current.key() == agent.key() && current.terminal_id == agent.terminal_id,
        "HERDR agent changed"
    );
    Ok(current)
}

pub async fn prompt(socket: &Socket, agent: &Agent, text: &str) -> Result<Value> {
    socket
        .herdr("agent.prompt", json!({"target":agent.pane_id,"text":text}))
        .await
}
