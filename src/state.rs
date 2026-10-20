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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outgoing {
    pub id: String,
    pub group: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeState {
    pub version: u32,
    pub account: Option<String>,
    pub phone: Option<String>,
    pub mappings: BTreeMap<String, Mapping>,
    pub outbox: Vec<Outgoing>,
}

impl Default for BridgeState {
    fn default() -> Self {
        Self {
            version: 1,
            account: None,
            phone: None,
            mappings: BTreeMap::new(),
            outbox: vec![],
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
        tmp.write_all(&serde_json::to_vec(self)?)?;
        tmp.as_file().sync_all()?;
        tmp.persist(path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    }

    pub fn enqueue(&mut self, group: &str, text: &str) {
        // Keep each encrypted message below MDK's message-size ceiling.
        let mut rest = text;
        while !rest.is_empty() {
            let mut end = rest.len().min(30000);
            while !rest.is_char_boundary(end) {
                end -= 1;
            }
            self.outbox.push(Outgoing {
                id: uuid::Uuid::new_v4().to_string(),
                group: group.into(),
                text: rest[..end].into(),
            });
            rest = &rest[end..];
        }
    }
}

pub fn lock(path: &Path) -> Result<File> {
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
    file.try_lock_exclusive()
        .map_err(|_| anyhow::anyhow!("Bridge already runs for this state path"))?;
    Ok(file)
}
