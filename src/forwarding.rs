use anyhow::Result;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Short-lived control file, independent of the worker's routing-state lock.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Forwarding {
    pub enabled: bool,
    pub revision: String,
    #[serde(default)]
    pub wake_messages: Vec<String>,
}

impl Default for Forwarding {
    fn default() -> Self {
        Self {
            enabled: true,
            revision: "initial".into(),
            wake_messages: Vec::new(),
        }
    }
}

fn path(state_path: &Path) -> PathBuf {
    state_path.with_extension("forwarding.json")
}

pub fn load(state_path: &Path) -> Result<Forwarding> {
    match fs::read(path(state_path)) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Forwarding::default()),
        Err(error) => Err(error.into()),
    }
}

fn update(state_path: &Path, change: impl FnOnce(&mut Forwarding) -> bool) -> Result<Forwarding> {
    let control_path = path(state_path);
    let guard = crate::state::lock_file(&control_path)?;
    // Only the atomic control update holds this lock, never network or transcript I/O.
    guard.lock_exclusive()?;
    let mut current = load(state_path)?;
    let was_enabled = current.enabled;
    if !change(&mut current) {
        return Ok(current);
    }
    if was_enabled != current.enabled {
        current.revision = uuid::Uuid::new_v4().to_string();
    }
    crate::state::save_json(&control_path, &current)?;
    Ok(current)
}

pub fn set(state_path: &Path, enabled: bool) -> Result<Forwarding> {
    update(state_path, |control| {
        let changed = control.enabled != enabled;
        control.enabled = enabled;
        changed
    })
}

pub fn toggle(state_path: &Path) -> Result<Forwarding> {
    update(state_path, |control| {
        control.enabled = !control.enabled;
        true
    })
}

pub fn wake(state_path: &Path, group: &str, message: &str) -> Result<Forwarding> {
    let key = serde_json::to_string(&(group, message))?;
    update(state_path, |control| {
        if control.wake_messages.contains(&key) {
            return false;
        }
        control.wake_messages.push(key);
        control.enabled = true;
        true
    })
}
