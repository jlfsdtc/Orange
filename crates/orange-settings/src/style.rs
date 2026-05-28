//! Theme and style configuration.
//!
//! Color schemes are stored as hex strings (`#RRGGBB` or `#RRGGBBAA`) so they
//! can be serialised to JSON / TOML without depending on the GUI layer. The
//! UI crate is responsible for parsing these into GPUI's native `Hsla`.

use serde::{Deserialize, Serialize};

/// Color scheme for the application.
///
/// Field values must be hex strings of the form `#RRGGBB` or `#RRGGBBAA`.
/// The default dark scheme is Vitesse Dark Soft (Warp-compatible);
/// the light scheme is Catppuccin Latte.
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
    /// The default scheme is the dark scheme (Vitesse Dark Soft).
    fn default() -> Self {
        Self::dark()
    }
}

impl ColorScheme {
    /// Vitesse Dark Soft — the canonical dark scheme.
    /// Mirrors the Warp theme of the same name by antfu.
    pub fn dark() -> Self {
        Self {
            background: "#222222".to_string(),
            foreground: "#dbd7ca".to_string(),
            selection: "#393a34".to_string(),
            line_number: "#777777".to_string(),
            current_line: "#2c2c2c".to_string(),
            search_match: "#e6cc77".to_string(),
            search_current: "#4d9375".to_string(),
            bookmark: "#6394bf".to_string(),
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
