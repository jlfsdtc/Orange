//! Session management: save/restore open files and view state.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A saved session containing open files and their states.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub name: String,
    pub files: Vec<SessionFile>,
    pub active_file: usize,
}

/// State of a single file in a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionFile {
    pub path: PathBuf,
    pub cursor_line: u64,
    pub first_visible_line: u64,
    pub bookmarks: Vec<u64>,
    pub active_filter: Option<String>,
}

impl Session {
    /// Save the current session to a file.
    pub fn save(&self, path: &PathBuf) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load a session from a file.
    pub fn load(path: &PathBuf) -> anyhow::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&json)?)
    }
}
