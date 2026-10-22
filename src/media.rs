use crate::{BridgeConfig, wire::Socket};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    process::Stdio,
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MAX_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
#[error("MDK has not supplied complete media download references")]
pub struct MediaUnavailable;

pub fn complete_refs(message: &Value) -> bool {
    message["media"].as_array().is_some_and(|items| {
        !items.is_empty()
            && items.iter().all(|item| {
                [
                    "media_type",
                    "file_name",
                    "ciphertext_sha256",
                    "plaintext_sha256",
                    "nonce_hex",
                    "version",
                ]
                .iter()
                .all(|field| item[*field].as_str().is_some_and(|s| !s.is_empty()))
                    && item["source_epoch"].as_u64().is_some()
                    && item["locators"]
                        .as_array()
                        .is_some_and(|locators| !locators.is_empty())
            })
    })
}

pub fn audio(message: &Value) -> bool {
    message["media"].as_array().is_some_and(|items| {
        items.iter().any(|m| {
            m["media_type"]
                .as_str()
                .is_some_and(|t| t.starts_with("audio/"))
        })
    })
}

#[derive(Debug)]
pub struct Prepared {
    pub prompt: String,
    pub transcript: Option<String>,
}

pub fn transcript_notice(text: &str) -> String {
    let quote = text
        .lines()
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut notice = format!("> transcribed\n\n{quote}");
    const SUFFIX: &str = "\n\n> … Transcript shortened for display.";
    if notice.len() > 30000 {
        let mut end = 30000 - SUFFIX.len();
        while !notice.is_char_boundary(end) {
            end -= 1;
        }
        notice.truncate(end);
        notice.push_str(SUFFIX);
    }
    notice
}

pub async fn prepare(
    config: &BridgeConfig,
    socket: &Socket,
    group: &str,
    message: &Value,
) -> Result<Prepared> {
    let mut attachments = message["media"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Missing media"))?
        .clone();
    ensure!(
        !attachments.is_empty() && attachments.len() <= 4,
        "Send at most four attachments"
    );
    let mut parts = Vec::new();
    let mut transcripts = Vec::new();
    let caption = message["text"].as_str().unwrap_or("").trim();
    if !caption.is_empty() {
        parts.push(caption.to_owned());
    }
    if !complete_refs(message)
        && attachments.iter().all(|m| {
            m["media_type"]
                .as_str()
                .is_some_and(|t| t.starts_with("image/") || t.starts_with("audio/"))
        })
    {
        let recovered = socket.marmot(json!({"type":"timeline_message_get", "account_id_hex":config.account_id_hex,
            "group_id_hex":group, "message_id_hex":message["message_id_hex"], "include_media":true}))
            .await.map_err(|_| MediaUnavailable)?;
        let row = &recovered["message"];
        if recovered["type"] != "timeline_message"
            || recovered["account_id_hex"] != config.account_id_hex
            || recovered["group_id_hex"] != group
            || recovered["message_id_hex"] != message["message_id_hex"]
            || row["message_id_hex"] != message["message_id_hex"]
            || row["availability"] != "available"
            || row["kind"] != 9
            || row["sender"]["account_id_hex"].as_str().is_none()
            || row["sender"]["account_id_hex"] != message["sender"]["account_id_hex"]
            || row["text_truncated"] == true
            || row["attachments_truncated"] == true
            || row["text"].as_str().unwrap_or("").trim() != caption
            || !complete_refs(&json!({"media":recovered["media"]}))
            || recovered["media"].as_array().unwrap().len() != attachments.len()
        {
            return Err(MediaUnavailable.into());
        }
        attachments = recovered["media"].as_array().unwrap().clone();
    }
    for attachment in &attachments {
        let kind = attachment["media_type"]
            .as_str()
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim();
        let extension = match kind {
            "image/png" => Some("png"),
            "image/jpeg" => Some("jpg"),
            "image/webp" => Some("webp"),
            "image/gif" => Some("gif"),
            _ => None,
        };
        ensure!(
            extension.is_some() || kind.starts_with("audio/"),
            "Only images and audio are supported"
        );
        if extension.is_none() {
            ensure!(
                !config.transcribe_command.is_empty(),
                "Audio transcription is not configured"
            );
        }
        let downloaded = socket
            .marmot(
                json!({"type":"download_media", "account_id_hex":config.account_id_hex,
            "group_id_hex":group, "media":attachment}),
            )
            .await?;
        ensure!(
            downloaded["type"] == "media_downloaded",
            "Unexpected media response"
        );
        let path = downloaded["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing media path"))?;
        let metadata = tokio::fs::symlink_metadata(path).await?;
        ensure!(
            metadata.is_file()
                && metadata.len() <= MAX_BYTES
                && metadata.len() > 0
                && downloaded["size_bytes"].as_u64() == Some(metadata.len()),
            "Invalid or oversized media file"
        );
        ensure!(
            downloaded["media_type"].as_str().is_some_and(|t| t
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                == kind),
            "Media type changed"
        );
        if let Some(extension) = extension {
            let directory = config.state_path.parent().unwrap().join("media");
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&directory)?;
            ensure!(
                std::fs::symlink_metadata(&directory)?.is_dir(),
                "Invalid media directory"
            );
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
            let target = directory.join(format!("{}.{extension}", uuid::Uuid::new_v4()));
            let mut source = tokio::fs::File::open(path).await?.take(MAX_BYTES + 1);
            let mut bytes = Vec::new();
            source.read_to_end(&mut bytes).await?;
            ensure!(
                bytes.len() as u64 == metadata.len(),
                "Media file changed during read"
            );
            let mut output = tokio::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&target)
                .await?;
            output.write_all(&bytes).await?;
            output.flush().await?;
            parts.push(format!(
                "Attached image: {}",
                serde_json::to_string(&target)?
            ));
        } else {
            let command = &config.transcribe_command;
            let mut child = tokio::process::Command::new(&command[0])
                .args(&command[1..])
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()?;
            let text = tokio::time::timeout(Duration::from_secs(180), async {
                let mut bytes = Vec::new();
                child
                    .stdout
                    .take()
                    .unwrap()
                    .take(65537)
                    .read_to_end(&mut bytes)
                    .await?;
                ensure!(bytes.len() <= 65536, "Transcription is too large");
                ensure!(child.wait().await?.success(), "Transcription failed");
                let text = String::from_utf8(bytes)?;
                ensure!(!text.trim().is_empty(), "Transcription was empty");
                Ok::<_, anyhow::Error>(text.trim().to_owned())
            })
            .await??;
            transcripts.push(text.clone());
            parts.push(text);
        }
    }
    let text = parts.join("\n\n");
    ensure!(
        !text.is_empty() && text.len() <= 30000,
        "Media prompt is empty or too large"
    );
    Ok(Prepared {
        prompt: text,
        transcript: (!transcripts.is_empty()).then(|| transcripts.join("\n\n")),
    })
}
