use anyhow::{Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Mapping {
    pub group_id_hex: Option<String>,
    #[serde(default)]
    pub group_description: Option<String>,
    /// Last title this bridge applied or adopted for the group.
    #[serde(default)]
    pub group_name: Option<String>,
    /// The group title was changed outside codr; never overwrite it automatically.
    #[serde(default)]
    pub group_name_locked: bool,
    /// A profile update may have reached the group; do not retry automatically.
    #[serde(default)]
    pub rename_uncertain: bool,
    pub creating: bool,
    pub last_agent_status: String,
    pub last_state_change_seq: u64,
    pub final_ids: Vec<String>,
    pub output_initialised: bool,
    pub active: bool,
    pub pending_prompt: bool,
    pub seen_messages: Vec<String>,
    pub cursor: Option<serde_json::Value>,
    pub joined_at: u64,
    #[serde(default)]
    pub agent_record: Option<crate::herdr::Agent>,
    #[serde(default)]
    pub last_export_at: u64,
    #[serde(default)]
    pub blocked_context_seq: Option<u64>,
    #[serde(default)]
    pub completion_pending: bool,
    #[serde(default)]
    pub response_id: Option<String>,
    #[serde(default)]
    pub task_revision: Option<String>,
    #[serde(default)]
    pub forwarding_baseline_pending: bool,
    #[serde(default)]
    pub muted_turns: Vec<String>,
    #[serde(default)]
    pub decision: Option<crate::output::decision::Decision>,
    #[serde(default)]
    pub decision_checked_at: u64,
    #[serde(default)]
    pub decision_refresh_pending: bool,
    #[serde(default)]
    pub activity: crate::activity::Activity,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResponseDelivery {
    pub target: Option<String>,
    pub initial_text: Option<String>,
    pub sent_text: Option<String>,
    pub last_attempt_at: u64,
    pub finished: bool,
    /// Keep completed notices until delivery succeeds, unlike best-effort progress.
    #[serde(default)]
    pub durable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceDelivery {
    pub batch: String,
    /// Digests of source identity, speaker and content; no extra transcript plaintext.
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outgoing {
    pub id: String,
    pub group: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<ResponseDelivery>,
    /// An explicit owner command may reply once even while automatic forwarding is muted.
    #[serde(default)]
    pub manual: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceDelivery>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeState {
    pub version: u32,
    pub account: Option<String>,
    pub phone: Option<String>,
    pub mappings: BTreeMap<String, Mapping>,
    pub outbox: Vec<Outgoing>,
    /// Global Unix-second reference for the last confirmed new White Noise message.
    /// Edits and incoming phone commands do not advance it.
    #[serde(default)]
    pub last_confirmed_send_at: Option<u64>,
    /// Per-group source messages confirmed after every chunk in their batch was sent.
    #[serde(default)]
    pub delivered_messages: BTreeMap<String, Vec<String>>,
    #[serde(default = "forwarding_default")]
    pub forwarding_enabled: bool,
    #[serde(default)]
    pub forwarding_revision: Option<String>,
    #[serde(default)]
    pub forwarding_resume_pending: bool,
    /// Authorised inbound media awaiting download/transcription. No terminal input yet.
    #[serde(default)]
    pub media_pending: Vec<serde_json::Value>,
}

fn forwarding_default() -> bool {
    true
}

impl Default for BridgeState {
    fn default() -> Self {
        Self {
            version: 1,
            account: None,
            phone: None,
            mappings: BTreeMap::new(),
            outbox: vec![],
            last_confirmed_send_at: None,
            delivered_messages: BTreeMap::new(),
            forwarding_enabled: true,
            forwarding_revision: None,
            forwarding_resume_pending: false,
            media_pending: vec![],
        }
    }
}

impl BridgeState {
    pub fn with_mapping(key: &str, group: String) -> Self {
        let mut s = Self::default();
        s.mappings.insert(
            key.into(),
            Mapping {
                group_id_hex: Some(group),
                ..Mapping::default()
            },
        );
        s
    }

    pub fn load(path: &Path) -> Result<Self> {
        match fs::read(path) {
            Ok(bytes) => {
                let s: Self = serde_json::from_slice(&bytes)?;
                ensure!(s.version == 1, "Unsupported state version");
                Ok(s)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        save_json(path, self)
    }

    pub fn enqueue(&mut self, group: &str, text: &str) {
        if !self.forwarding_enabled {
            return;
        }
        self.enqueue_text(group, text, None);
    }

    /// Queue an owner-requested reply with stable per-chunk transport idempotency keys.
    pub fn enqueue_manual(&mut self, group: &str, text: &str, request: &str) {
        self.enqueue_text(group, text, Some(request));
    }

    pub fn source_known(&self, group: &str, message: &str) -> bool {
        self.delivered_messages
            .get(group)
            .is_some_and(|sent| sent.iter().any(|key| key == message))
            || self.outbox.iter().any(|item| {
                item.group == group
                    && item
                        .source
                        .as_ref()
                        .is_some_and(|source| source.messages.iter().any(|key| key == message))
            })
    }

    /// Source identity remains pending until every chunk is confirmed, including after restart.
    pub fn enqueue_tracked(
        &mut self,
        group: &str,
        text: &str,
        request: Option<&str>,
        messages: Vec<String>,
    ) {
        if request.is_none() && !self.forwarding_enabled {
            return;
        }
        let start = self.outbox.len();
        self.enqueue_text(group, text, request);
        if let Some(first) = self.outbox.get(start) {
            let source = SourceDelivery {
                batch: first.id.clone(),
                messages,
            };
            for item in &mut self.outbox[start..] {
                item.source = Some(source.clone());
            }
        }
    }

    pub fn confirm_send(&mut self, id: &str) {
        let Some(position) = self.outbox.iter().position(|item| item.id == id) else {
            return;
        };
        let item = self.outbox.remove(position);
        let Some(source) = item.source else {
            return;
        };
        if self.outbox.iter().any(|pending| {
            pending.group == item.group
                && pending
                    .source
                    .as_ref()
                    .is_some_and(|other| other.batch == source.batch)
        }) {
            return;
        }
        let sent = self.delivered_messages.entry(item.group).or_default();
        for message in source.messages {
            if !sent.contains(&message) {
                sent.push(message);
            }
        }
    }

    fn enqueue_text(&mut self, group: &str, text: &str, request: Option<&str>) {
        // Keep each encrypted message below MDK's message-size ceiling.
        let mut rest = text;
        let mut part = 0;
        while !rest.is_empty() {
            let mut end = rest.len().min(30000);
            while !rest.is_char_boundary(end) {
                end -= 1;
            }
            self.outbox.push(Outgoing {
                id: request.map_or_else(
                    || uuid::Uuid::new_v4().to_string(),
                    |request| format!("{request}:{part}"),
                ),
                group: group.into(),
                text: rest[..end].into(),
                response: None,
                manual: request.is_some(),
                source: None,
            });
            rest = &rest[end..];
            part += 1;
        }
    }

    pub fn begin_response(&mut self, key: &str, group: &str) {
        if !self.forwarding_enabled {
            return;
        }
        if self.mappings[key]
            .response_id
            .as_ref()
            .is_some_and(|id| self.outbox.iter().any(|item| &item.id == id))
        {
            return;
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.outbox.push(Outgoing {
            id: id.clone(),
            group: group.into(),
            text: "> working...".into(),
            response: Some(ResponseDelivery::default()),
            manual: false,
            source: None,
        });
        self.mappings.get_mut(key).unwrap().response_id = Some(id);
    }

    pub fn begin_transcription(&mut self, group: &str) -> Option<String> {
        if !self.forwarding_enabled {
            return None;
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.outbox.push(Outgoing {
            id: id.clone(),
            group: group.into(),
            text: "> transcribing".into(),
            response: Some(ResponseDelivery {
                durable: true,
                ..ResponseDelivery::default()
            }),
            manual: false,
            source: None,
        });
        Some(id)
    }

    pub fn update_response(&mut self, key: &str, group: &str, text: &str, finished: bool) {
        if !self.forwarding_enabled {
            return;
        }
        self.begin_response(key, group);
        let id = self.mappings[key].response_id.as_ref().unwrap();
        let item = self.outbox.iter_mut().find(|item| &item.id == id).unwrap();
        let mut end = text.len().min(30000);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        item.text = text[..end].into();
        item.response.as_mut().unwrap().finished = finished;
        if finished {
            self.mappings.get_mut(key).unwrap().response_id = None;
            // Preserve the full text of long terminal notices.
            self.enqueue(group, &text[end..]);
        }
    }

    pub fn complete_progress(&mut self, key: &str) {
        let Some(id) = self.mappings.get_mut(key).unwrap().response_id.take() else {
            return;
        };
        let Some(item) = self.outbox.iter_mut().find(|item| item.id == id) else {
            return;
        };
        let delivery = item.response.as_mut().unwrap();
        if delivery.target.is_none() {
            // Progress is best effort. Do not post a late working notice after the final.
            self.outbox.retain(|item| item.id != id);
            return;
        }
        item.text = if let Some(tasks) = item
            .text
            .strip_prefix("> working...\n")
            .or_else(|| item.text.strip_prefix("> working\n"))
        {
            format!("> done\n{tasks}")
        } else {
            "> done".into()
        };
        delivery.finished = true;
    }
}

pub fn lock(path: &Path) -> Result<File> {
    let file = lock_file(path)?;
    file.try_lock_exclusive()
        .map_err(|_| anyhow::anyhow!("Bridge already runs for this state path"))?;
    Ok(file)
}

pub(crate) fn lock_file(path: &Path) -> Result<File> {
    let lock_path = path.with_extension("lock");
    if let Some(parent) = lock_path.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(lock_path)?;
    Ok(file)
}

pub(crate) fn save_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| anyhow::anyhow!("State path needs a parent"))?;
    if !parent.exists() {
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    tmp.write_all(&serde_json::to_vec(value)?)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}
