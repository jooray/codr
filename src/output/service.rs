//! Non-owning local V2 reads. No CLI, health recovery, registration writes, or process signals.
use super::{ExportV2, SessionSnapshot, decision, from_v2, history_from_data, permission, tasks};
use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Deserialize)]
struct Registration {
    url: String,
    pid: u32,
    password: Option<String>,
}

#[derive(Default)]
struct Cache {
    owner: Option<(String, u32)>,
    cursor: Option<u64>,
    snapshot: Option<SessionSnapshot>,
}

type Sessions = HashMap<(PathBuf, String), Arc<tokio::sync::Mutex<Cache>>>;

pub struct Reader {
    client: reqwest::Client,
    sessions: Mutex<Sessions>,
}

impl Default for Reader {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(15))
                .build()
                .expect("Local HTTP client configuration"),
            sessions: Mutex::default(),
        }
    }
}

impl Reader {
    pub(crate) fn retain(&self, sessions: &HashSet<String>) {
        if let Ok(mut cache) = self.sessions.lock() {
            cache.retain(|(_, session), _| sessions.contains(session));
        }
    }

    pub async fn read(
        &self,
        config: &crate::BridgeConfig,
        session: &str,
    ) -> Result<SessionSnapshot> {
        ensure!(
            session.starts_with("ses_")
                && session
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')),
            "Invalid OpenCode session identity"
        );
        let file = config
            .opencode_service_file
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("OpenCode service registration is not configured"))?;
        // Never include registration contents (which contain authentication) in diagnostics.
        let bytes = tokio::fs::read(file)
            .await
            .map_err(|_| anyhow::anyhow!("OpenCode service registration unavailable"))?;
        ensure!(
            bytes.len() <= 64 * 1024,
            "OpenCode service registration too large"
        );
        let registration: Registration = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("Invalid OpenCode service registration"))?;
        let url = reqwest::Url::parse(&registration.url)
            .map_err(|_| anyhow::anyhow!("Invalid OpenCode service address"))?;
        ensure!(
            url.scheme() == "http"
                && url
                    .host_str()
                    .and_then(|host| host
                        .trim_matches(['[', ']'])
                        .parse::<std::net::IpAddr>()
                        .ok())
                    .is_some_and(|ip| ip.is_loopback())
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && url.path() == "/"
                && registration.pid > 0,
            "OpenCode service must use a loopback HTTP address"
        );
        let cached = {
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|_| anyhow::anyhow!("OpenCode reader unavailable"))?;
            sessions
                .entry((file.clone(), session.to_owned()))
                .or_default()
                .clone()
        };
        let mut cache = cached.lock().await;
        let owner = (registration.url.clone(), registration.pid);
        if cache.owner.as_ref() != Some(&owner) {
            *cache = Cache::default();
        }
        // A cursor ahead of the log returns only its current watermark. Event
        // contents are not needed here; never replay an unbounded backlog just
        // to decide whether the projected history needs rebuilding.
        let after = 9_007_199_254_740_991_u64;
        let log = self
            .get(
                &registration,
                &format!("/api/experimental/session/{session}/log?after={after}&follow=false"),
                64 * 1024,
            )
            .await?;
        let cursor = watermark(&log, session)?;
        let mut snapshot = if cache.snapshot.is_some() && cache.cursor == cursor {
            cache.snapshot.clone().unwrap()
        } else {
            let bytes = self
                .get(
                    &registration,
                    &format!("/api/experimental/session/{session}/export"),
                    128 * 1024 * 1024,
                )
                .await?;
            #[derive(Deserialize)]
            struct Response {
                data: ExportV2,
            }
            let exported: Response = serde_json::from_slice(&bytes)?;
            let export = from_v2(exported.data);
            SessionSnapshot {
                history: history_from_data(&export),
                tasks: tasks::from_data(&export),
                ..Default::default()
            }
        };
        // Forms and permissions need their own reads: not all changes share the
        // session's durable event log. Failures must not clear pending prompts.
        let forms: Value = serde_json::from_slice(
            &self
                .get(
                    &registration,
                    &format!("/api/session/{session}/form"),
                    1024 * 1024,
                )
                .await?,
        )?;
        snapshot.decision = decision::from_forms(&forms, session)?;
        let permissions: Value = serde_json::from_slice(
            &self
                .get(
                    &registration,
                    &format!("/api/session/{session}/permission"),
                    1024 * 1024,
                )
                .await?,
        )?;
        snapshot.permission = permission::from_response(&permissions, session)?;
        cache.owner = Some(owner);
        cache.cursor = cursor;
        cache.snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }

    async fn get(&self, registration: &Registration, path: &str, max: usize) -> Result<Vec<u8>> {
        let mut request = self
            .client
            .get(format!("{}{path}", registration.url.trim_end_matches('/')));
        if let Some(password) = &registration.password {
            request = request.basic_auth("opencode", Some(password));
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("OpenCode read unavailable; retry later"))?;
        ensure!(
            response.status().is_success(),
            "OpenCode read failed (HTTP {})",
            response.status().as_u16()
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("OpenCode read interrupted; retry later"))?
        {
            ensure!(
                bytes.len().saturating_add(chunk.len()) <= max,
                "OpenCode read exceeded size limit"
            );
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}

fn watermark(bytes: &[u8], session: &str) -> Result<Option<u64>> {
    let mut marker = None;
    let mut data = String::new();
    for line in std::str::from_utf8(bytes)?.split('\n') {
        let line = line.trim_end_matches('\r');
        if let Some(value) = line.strip_prefix("data:") {
            data.push_str(value.strip_prefix(' ').unwrap_or(value));
            data.push('\n');
            continue;
        }
        if !line.is_empty() || data.is_empty() {
            continue;
        }
        let item: Value = serde_json::from_str(&data)?;
        data.clear();
        ensure!(
            marker.is_none(),
            "Unexpected data after OpenCode log watermark"
        );
        if item["type"] == "log.synced" {
            ensure!(
                item["aggregateID"] == session,
                "OpenCode log belongs to another session"
            );
            let seq = match item.get("seq") {
                None => None,
                Some(value) => Some(
                    value
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("Invalid OpenCode log watermark"))?,
                ),
            };
            marker = Some(seq);
        }
    }
    ensure!(data.is_empty(), "Incomplete OpenCode log frame");
    marker.ok_or_else(|| anyhow::anyhow!("OpenCode log watermark missing"))
}
