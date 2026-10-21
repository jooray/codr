use super::{FinalMessage, native};
use anyhow::{Result, ensure};
use std::{collections::HashSet, path::Path};

pub fn read(home: &Path, session: &str) -> Result<Vec<FinalMessage>> {
    let session = native::session_id(session)?;
    let suffix = format!("-{session}.jsonl");
    let path = native::find(
        &[
            (home.join("sessions"), 3),
            (home.join("archived_sessions"), 3),
        ],
        |name| name.starts_with("rollout-") && name.ends_with(&suffix),
    )?;
    let mut verified = false;
    let mut seen = HashSet::new();
    let mut replies = Vec::new();
    native::records(&path, |row| {
        let payload = &row["payload"];
        if row["type"] == "session_meta" {
            ensure!(
                payload["id"]
                    .as_str()
                    .and_then(|id| native::session_id(id).ok())
                    .as_deref()
                    == Some(&session),
                "Codex transcript session identity mismatch"
            );
            verified = true;
        }
        if row["type"] != "event_msg"
            || !matches!(
                payload["type"].as_str(),
                Some("task_complete" | "turn_complete")
            )
            || !payload["error"].is_null()
        {
            return Ok(());
        }
        let Some(text) = payload["last_agent_message"]
            .as_str()
            .filter(|text| !text.trim().is_empty())
        else {
            return Ok(());
        };
        let turn = payload["turn_id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Codex completion lacks its turn ID"))?;
        let id = format!("codex:{turn}");
        if seen.insert(id.clone()) {
            replies.push(FinalMessage {
                id,
                text: text.into(),
            });
        }
        Ok(())
    })?;
    ensure!(
        verified,
        "Codex transcript has no verified session metadata"
    );
    Ok(replies)
}
