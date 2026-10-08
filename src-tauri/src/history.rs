use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub timestamp: chrono::DateTime<chrono::Local>,
    /// "dictate" or "command".
    pub mode: String,
    /// What the speech model heard.
    pub transcript: String,
    /// What was typed (after the AI step, when it ran).
    pub output: String,
    /// The selected text that command mode acted on.
    #[serde(default)]
    pub selection: String,
    pub ai_used: bool,
    #[serde(default)]
    pub note: Option<String>,
}

pub struct History {
    path: PathBuf,
    entries: Mutex<Vec<Entry>>,
}

impl History {
    pub fn load(path: PathBuf) -> Self {
        let entries = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        Self {
            path,
            entries: Mutex::new(entries),
        }
    }

    pub fn all(&self) -> Vec<Entry> {
        self.entries.lock().unwrap().clone()
    }

    pub fn add(&self, mut entry: Entry, limit: usize) {
        let mut entries = self.entries.lock().unwrap();
        entry.id = entries.first().map_or(1, |e| e.id + 1);
        entries.insert(0, entry);
        entries.truncate(limit.max(1));
        self.persist(&entries);
    }

    pub fn remove(&self, ids: &[u64]) {
        let mut entries = self.entries.lock().unwrap();
        entries.retain(|e| !ids.contains(&e.id));
        self.persist(&entries);
    }

    pub fn clear(&self) {
        let mut entries = self.entries.lock().unwrap();
        entries.clear();
        self.persist(&entries);
    }

    fn persist(&self, entries: &[Entry]) {
        let result = serde_json::to_vec(entries)
            .map_err(anyhow::Error::from)
            .and_then(|bytes| Ok(std::fs::write(&self.path, bytes)?));
        if let Err(e) = result {
            log::warn!("couldn't save history: {e}");
        }
    }
}
