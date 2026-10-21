use super::{FinalMessage, native};
use anyhow::{Result, ensure};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

struct Row {
    id: String,
    parent: Option<String>,
    kind: String,
    same_session: bool,
    private: bool,
    api_error: bool,
    user_prompt: bool,
    message: Option<String>,
    stop: Option<String>,
    text: Vec<String>,
}

fn truthy(value: &Value) -> bool {
    !value.is_null() && value != false && value != "" && value != 0
}

#[derive(Default)]
struct Response {
    message: Option<String>,
    last_row: String,
    text: Vec<String>,
    complete: bool,
}

impl Response {
    fn finish(&mut self, replies: &mut Vec<FinalMessage>) {
        if self.complete {
            let text = self.text.join("\n");
            if !text.trim().is_empty() {
                replies.push(FinalMessage {
                    id: format!("claude:{}", self.last_row),
                    text,
                });
            }
        }
        *self = Self::default();
    }
}

pub fn read(home: &Path, session: &str) -> Result<Vec<FinalMessage>> {
    let session = native::session_id(session)?;
    let name = format!("{session}.jsonl");
    // Only project roots, never nested session/subagent directories. This also handles custom project keys.
    let path = native::find(&[(home.join("projects"), 1)], |candidate| candidate == name)?;
    let mut rows = Vec::new();
    let mut index = HashMap::new();
    let mut verified = false;
    native::records(&path, |value| {
        let same_session = value["sessionId"]
            .as_str()
            .and_then(|id| native::session_id(id).ok())
            .as_deref()
            == Some(&session);
        verified |= same_session;
        let Some(id) = value["uuid"].as_str().filter(|id| !id.is_empty()) else {
            return Ok(());
        };
        let Some(kind @ ("user" | "assistant" | "progress" | "system" | "attachment")) =
            value["type"].as_str()
        else {
            return Ok(());
        };
        let private =
            truthy(&value["isSidechain"]) || truthy(&value["isMeta"]) || truthy(&value["teamName"]);
        let content = &value["message"]["content"];
        let blocks = content.as_array();
        let user_prompt = kind == "user"
            && !private
            && !truthy(&value["isCompactSummary"])
            && (content.is_string()
                || blocks.is_some_and(|parts| parts.iter().any(|part| part["type"] == "text")));
        let text = if kind == "assistant" && !private && value["message"]["role"] == "assistant" {
            blocks
                .map(|parts| {
                    parts
                        .iter()
                        .filter(|part| part["type"] == "text")
                        .filter_map(|part| part["text"].as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let row = Row {
            id: id.into(),
            parent: value["parentUuid"].as_str().map(str::to_owned),
            kind: kind.into(),
            same_session,
            private,
            api_error: truthy(&value["isApiErrorMessage"]) || !value["message"]["error"].is_null(),
            user_prompt,
            message: value["message"]["id"].as_str().map(str::to_owned),
            stop: value["message"]["stop_reason"].as_str().map(str::to_owned),
            text,
        };
        index.insert(row.id.clone(), rows.len());
        rows.push(row);
        Ok(())
    })?;
    ensure!(
        verified,
        "Claude transcript session identity mismatch or missing metadata"
    );
    let parents: HashSet<_> = index
        .values()
        .filter(|position| !rows[**position].private && rows[**position].same_session)
        .filter_map(|position| rows[*position].parent.as_deref())
        .collect();
    let mut leaves = Vec::new();
    for position in index
        .values()
        .copied()
        .filter(|position| !parents.contains(rows[*position].id.as_str()))
    {
        let mut current = Some(position);
        let mut seen = HashSet::new();
        while let Some(position) = current {
            ensure!(seen.insert(position), "Claude transcript parent cycle");
            let row = &rows[position];
            if matches!(row.kind.as_str(), "user" | "assistant") {
                if !row.private && row.same_session {
                    leaves.push(position);
                }
                break;
            }
            current = row
                .parent
                .as_ref()
                .and_then(|parent| index.get(parent))
                .copied();
        }
    }
    // Prefer the latest main-conversation message by file position, not a sidechain or a timestamp.
    let Some(mut current) = leaves.into_iter().max() else {
        return Ok(Vec::new());
    };
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    loop {
        ensure!(seen.insert(current), "Claude transcript parent cycle");
        chain.push(current);
        let Some(parent) = rows[current]
            .parent
            .as_ref()
            .and_then(|parent| index.get(parent))
        else {
            break;
        };
        current = *parent;
    }
    let mut response = Response::default();
    let mut replies = Vec::new();
    for position in chain.into_iter().rev() {
        let row = &rows[position];
        if row.private || !row.same_session {
            continue;
        }
        if row.user_prompt {
            response.finish(&mut replies);
        }
        if row.kind != "assistant" {
            continue;
        }
        if row.api_error {
            response = Response::default();
            continue;
        }
        let message = row.message.as_deref().unwrap_or(&row.id);
        if response.message.as_deref() != Some(message) {
            // A continuation for the same user turn supersedes the earlier model response.
            response = Response {
                message: Some(message.into()),
                ..Response::default()
            };
        }
        response.last_row = row.id.clone();
        response.text.extend(row.text.iter().cloned());
        // Duration/system markers and null stop reasons do not prove a successful final answer.
        response.complete = row.stop.as_deref() == Some("end_turn");
    }
    response.finish(&mut replies);
    Ok(replies)
}
