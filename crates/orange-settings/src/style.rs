//! Theme and style configuration.
//!
//! Color schemes are stored as hex strings (`#RRGGBB` or `#RRGGBBAA`) so they
//! can be serialised to JSON / TOML without depending on the GUI layer. The
//! UI crate is responsible for parsing these into GPUI's native `Hsla`.

use serde::{Deserialize, Serialize};

/// Color scheme for the application.
///
/// Field values must be hex strings of the form `#RRGGBB` or `#RRGGBBAA`.
/// The defaults follow the Catppuccin palette (Mocha for dark, Latte for light).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ColorScheme {
    pub background: String,
    pub foreground: String,
    pub selection: String,
    pub line_number: String,
    pub current_line: String,
    pub search_match: String,
    pub search_current: String,
    pub bookmark: String,
}

impl Default for ColorScheme {
    /// The default scheme is the dark scheme (Catppuccin Mocha).
    fn default() -> Self {
        Self::dark()
    }
}

impl ColorScheme {
    /// Catppuccin Mocha — the canonical dark scheme.
    pub fn dark() -> Self {
        Self {
            background: "#1e1e2e".to_string(),
            foreground: "#cdd6f4".to_string(),
            selection: "#45475a".to_string(),
            line_number: "#6c7086".to_string(),
            current_line: "#313244".to_string(),
            search_match: "#f9e2af".to_string(),
            search_current: "#a6e3a1".to_string(),
            bookmark: "#89b4fa".to_string(),
        }
    }

    /// Catppuccin Latte — the canonical light scheme.
    pub fn light() -> Self {
        Self {
            background: "#eff1f5".to_string(),
            foreground: "#4c4f69".to_string(),
            selection: "#bcc0cc".to_string(),
            line_number: "#9ca0b0".to_string(),
            current_line: "#ccd0da".to_string(),
            search_match: "#df8e1d".to_string(),
            search_current: "#40a02b".to_string(),
            bookmark: "#1e66f5".to_string(),
        }
    }
}
