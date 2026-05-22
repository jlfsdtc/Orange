//! Application options and configuration persistence.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main application options.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// Main font family name.
    pub main_font: String,
    /// Main font size in points.
    pub main_font_size: f32,
    /// Foreground color for text.
    pub text_foreground_color: String,
    /// Background color for text area.
    pub text_background_color: String,
    /// Whether to follow file changes (tail mode).
    pub follow_file: bool,
    /// Number of lines to keep in overview.
    pub overview_context: u32,
    /// Maximum number of recent files.
    pub max_recent_files: usize,
    /// Whether to use dark theme.
    pub dark_theme: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            main_font: "Courier New".to_string(),
            main_font_size: 22.0,
            text_foreground_color: "#000000".to_string(),
            text_background_color: "#FFFFFF".to_string(),
            follow_file: false,
            overview_context: 4,
            max_recent_files: 20,
            dark_theme: true,
        }
    }
}

impl Options {
    /// Load options from the config file.
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    /// Save options to the config file.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Get the path to the config file.
    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("orange")
            .join("config.toml")
    }
}
