//! Keyboard shortcut configuration.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// A keyboard shortcut binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBinding {
    pub key: String,
    pub modifiers: Vec<String>,
}

impl KeyBinding {
    pub fn new(key: &str, modifiers: &[&str]) -> Self {
        Self {
            key: key.to_string(),
            modifiers: modifiers.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// Shortcut configuration for the application.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Shortcuts {
    pub bindings: HashMap<String, KeyBinding>,
}

impl Default for Shortcuts {
    fn default() -> Self {
        let mut bindings = HashMap::new();

        // File operations
        bindings.insert("open_file".into(), KeyBinding::new("o", &["ctrl"]));
        bindings.insert("close_file".into(), KeyBinding::new("w", &["ctrl"]));
        bindings.insert("reload_file".into(), KeyBinding::new("r", &["ctrl"]));
        bindings.insert("quit".into(), KeyBinding::new("q", &["ctrl"]));

        // Search
        bindings.insert("search_forward".into(), KeyBinding::new("f", &["ctrl"]));
        bindings.insert("search_next".into(), KeyBinding::new("f3", &[]));
        bindings.insert("search_prev".into(), KeyBinding::new("f3", &["shift"]));

        // Navigation
        bindings.insert("scroll_top".into(), KeyBinding::new("home", &["ctrl"]));
        bindings.insert("scroll_bottom".into(), KeyBinding::new("end", &["ctrl"]));
        bindings.insert("page_up".into(), KeyBinding::new("pageup", &[]));
        bindings.insert("page_down".into(), KeyBinding::new("pagedown", &[]));

        // Bookmarks
        bindings.insert("toggle_bookmark".into(), KeyBinding::new("b", &["ctrl"]));
        bindings.insert("next_bookmark".into(), KeyBinding::new("b", &["ctrl", "shift"]));
        bindings.insert("prev_bookmark".into(), KeyBinding::new("b", &["ctrl", "alt"]));

        // Copy
        bindings.insert("copy_selection".into(), KeyBinding::new("c", &["ctrl"]));
        bindings.insert("select_all".into(), KeyBinding::new("a", &["ctrl"]));

        Self { bindings }
    }
}

impl Shortcuts {
    /// Load shortcuts from the config file.
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    /// Save shortcuts to the config file.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Get a binding by action name.
    pub fn get(&self, action: &str) -> Option<&KeyBinding> {
        self.bindings.get(action)
    }

    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("orange")
            .join("shortcuts.toml")
    }
}
