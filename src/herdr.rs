use crate::wire::Socket;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;

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
    pub tab_id: String,
    #[serde(default)]
    pub tab_label: Option<String>,
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
        let fallback = if self.tab_id.is_empty() {
            "session"
        } else {
            self.tab_id.as_str()
        };
        let raw = self
            .tab_label
            .as_deref()
            .filter(|label| !label.trim().is_empty())
            .unwrap_or(fallback);
        let label: String = raw
            .chars()
            .filter(|character| !character.is_control())
            .collect();
        let mut name = format!(
            "HDR | {}",
            if label.trim().is_empty() {
                "session"
            } else {
                label.trim()
            }
        );
        while name.len() > 256 {
            name.pop();
        }
        name
    }
}

pub async fn snapshot(socket: &Socket) -> Result<Vec<Agent>> {
    let response = socket.herdr("session.snapshot", json!({})).await?;
    let agents = &response["result"]["snapshot"]["agents"];
    ensure!(agents.is_array(), "HERDR did not return a session snapshot");
    let mut agents: Vec<Agent> = serde_json::from_value(agents.clone())?;
    let labels: HashMap<&str, String> = response["result"]["snapshot"]["tabs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tab| {
            let id = tab["tab_id"].as_str()?;
            let label = tab["label"]
                .as_str()
                .filter(|label| !label.trim().is_empty())
                .map(str::to_owned)
                .or_else(|| tab["number"].as_u64().map(|number| number.to_string()))?;
            Some((id, label))
        })
        .collect();
    for agent in &mut agents {
        if let Some(label) = labels.get(agent.tab_id.as_str()) {
            agent.tab_label = Some(label.clone());
        }
    }
    Ok(agents)
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

pub async fn choose(socket: &Socket, agent: &Agent, choice: usize) -> Result<()> {
    ensure!((1..=9).contains(&choice), "Invalid decision shortcut");
    let response = socket
        .herdr(
            "agent.send_keys",
            json!({"target":agent.pane_id,"keys":[choice.to_string()]}),
        )
        .await?;
    ensure!(
        response["result"]["type"] == "ok",
        "Decision shortcut was not confirmed"
    );
    Ok(())
}
