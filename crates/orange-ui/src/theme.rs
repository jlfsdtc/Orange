//! Theme definitions and color management.
//!
//! The `Theme` is stored as a GPUI global (`impl Global for Theme`).
//! Widgets read it via `cx.global::<Theme>()` and subscribe with
//! `cx.observe_global::<Theme>(...)` to repaint on theme change.

use anyhow::{Context as _, Result};
use gpui::{px, rgb, rgba, Global, Hsla, Pixels, SharedString};
use orange_settings::{ColorScheme, Options};

/// Application color theme. All colors live in HSLA so GPUI can blend them
/// efficiently; conversion from the serde-friendly hex strings happens once
/// at load time.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub background: Hsla,
    pub foreground: Hsla,
    pub selection: Hsla,
    pub line_number: Hsla,
    pub current_line: Hsla,
    pub search_match: Hsla,
    pub search_current: Hsla,
    pub bookmark: Hsla,
}

impl Global for Theme {}

/// Parse `#RRGGBB` or `#RRGGBBAA` into a GPUI `Hsla`.
pub fn parse_hex_color(input: &str) -> Result<Hsla> {
    let s = input.trim().trim_start_matches('#');
    match s.len() {
        6 => {
            let v = u32::from_str_radix(s, 16)
                .with_context(|| format!("invalid hex color: {input}"))?;
            Ok(rgb(v).into())
        }
        8 => {
            let v = u32::from_str_radix(s, 16)
                .with_context(|| format!("invalid hex color: {input}"))?;
            Ok(rgba(v).into())
        }
        _ => anyhow::bail!(
            "invalid hex color (expected #RRGGBB or #RRGGBBAA): {input}"
        ),
    }
}

impl Theme {
    /// Build a `Theme` from a serde-friendly `ColorScheme`.
    pub fn from_color_scheme(scheme: &ColorScheme) -> Result<Self> {
        Ok(Self {
            background: parse_hex_color(&scheme.background)?,
            foreground: parse_hex_color(&scheme.foreground)?,
            selection: parse_hex_color(&scheme.selection)?,
            line_number: parse_hex_color(&scheme.line_number)?,
            current_line: parse_hex_color(&scheme.current_line)?,
            search_match: parse_hex_color(&scheme.search_match)?,
            search_current: parse_hex_color(&scheme.search_current)?,
            bookmark: parse_hex_color(&scheme.bookmark)?,
        })
    }

    /// Parse a `ColorScheme` from a JSON string and build a `Theme`.
    pub fn from_json(json: &str) -> Result<Self> {
        let scheme: ColorScheme =
            serde_json::from_str(json).context("failed to parse theme JSON")?;
        Self::from_color_scheme(&scheme)
    }

    /// Load a theme from a JSON file on disk.
    pub fn load_json_file(path: &std::path::Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("reading theme file {}", path.display()))?;
        let json = std::str::from_utf8(&bytes).context("theme file is not valid UTF-8")?;
        Self::from_json(json)
    }

    /// Built-in dark theme (Catppuccin Mocha).
    pub fn dark() -> Self {
        Self::from_color_scheme(&ColorScheme::dark())
            .expect("built-in dark ColorScheme must parse")
    }

    /// Built-in light theme (Catppuccin Latte).
    pub fn light() -> Self {
        Self::from_color_scheme(&ColorScheme::light())
            .expect("built-in light ColorScheme must parse")
    }

    /// Pick a built-in theme by the `dark_theme` boolean from `Options`.
    pub fn from_options_flag(dark: bool) -> Self {
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }

    /// Build the theme from the user's active (possibly customized) color
    /// scheme. Falls back to the matching built-in if the scheme contains an
    /// invalid hex string — this keeps live preview safe while the user is
    /// still typing a partial `#RRGGBB` value in the Settings "Theme" tab.
    pub fn from_options(options: &Options) -> Self {
        Self::from_color_scheme(options.active_scheme())
            .unwrap_or_else(|_| Self::from_options_flag(options.dark_theme))
    }
}

/// Font size and family for log-content text. Stored as a GPUI global so
/// views can `cx.global::<FontSettings>()` and
/// `cx.observe_global::<FontSettings>(...)` to repaint when the user changes
/// the size or family at runtime.
///
/// `family` is what we actually apply via `.font_family(...)` to both the log
/// rows and the filtered/search-results rows, so the Options "Font" field
/// drives both panes from one setting. Without it, GPUI falls back to its
/// default proportional font (`.SystemUIFont`) and the `column_for_x` hit-test
/// (which assumes a fixed monospace advance) drifts further per character.
#[derive(Clone, Debug)]
pub struct FontSettings {
    pub size: Pixels,
    pub family: SharedString,
}

impl Global for FontSettings {}

impl FontSettings {
    /// Minimum and maximum sizes for the size adjustment shortcuts and the
    /// Options dialog. Clamped so the UI stays usable even with a corrupt
    /// config file.
    pub const MIN: f32 = 6.0;
    pub const MAX: f32 = 48.0;
    pub const DEFAULT: f32 = 22.0;

    /// Build from persisted Options, clamping out-of-range values.
    pub fn from_options(options: &Options) -> Self {
        Self {
            size: px(options.main_font_size.clamp(Self::MIN, Self::MAX)),
            family: SharedString::from(options.main_font.clone()),
        }
    }

    /// Row height for one line of log content. 1.4× the font size keeps the
    /// gutter aligned with the text baseline at any size.
    pub fn line_height(&self) -> Pixels {
        self.size * 1.4
    }
}

/// Minimap strip width. Stored as a GPUI global so the overview repaints
/// immediately when the user edits the value in the Options dialog —
/// same pattern as `FontSettings`.
#[derive(Clone, Copy, Debug)]
pub struct MinimapSettings {
    pub width: Pixels,
}

impl Global for MinimapSettings {}

impl MinimapSettings {
    pub const MIN: f32 = 24.0;
    pub const MAX: f32 = 128.0;
    pub const DEFAULT: f32 = 48.0;

    /// Build from persisted Options, clamping out-of-range values.
    pub fn from_options(options: &Options) -> Self {
        Self {
            width: px(options.minimap_width.clamp(Self::MIN, Self::MAX)),
        }
    }
}

// Unit tests live under `tests/theme.rs` as an integration test because the
// crate-level recursion limit (set deliberately low to bound macro-resolution
// time) does not allow `cargo test --lib` to compile the deeply-nested GPUI
// element trees in this crate.
