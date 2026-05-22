//! HighlighterSet: configurable pattern-based line highlighting.
//!
//! Each highlight pattern has a regex and a color. Lines matching
//! patterns are rendered with the specified background color.

use gpui::*;
use orange_regex::RegexEngine;
use orange_regex::RegexFlags;
use serde::{Deserialize, Serialize};

/// A single highlight pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightPattern {
    /// Name for this pattern.
    pub name: String,
    /// Regex pattern string.
    pub pattern: String,
    /// Background color as hex string (e.g., "#ff0000").
    pub color: String,
    /// Whether this pattern is enabled.
    pub enabled: bool,
}

impl HighlightPattern {
    /// Compile the regex engine for this pattern.
    pub fn compile(&self) -> Option<RegexEngine> {
        if !self.enabled || self.pattern.is_empty() {
            return None;
        }
        RegexEngine::compile(&self.pattern, RegexFlags::default()).ok()
    }

    /// Parse the color hex string to an Hsla color.
    pub fn color_hsla(&self) -> Option<Hsla> {
        parse_hex_color(&self.color)
    }
}

/// A set of highlight patterns.
pub struct HighlighterSet {
    /// The highlight patterns.
    patterns: Vec<HighlightPattern>,
    /// Compiled regex engines (cached).
    engines: Vec<Option<RegexEngine>>,
}

impl HighlighterSet {
    pub fn new() -> Self {
        Self {
            patterns: Vec::new(),
            engines: Vec::new(),
        }
    }

    /// Create with some default patterns for common log levels.
    pub fn with_defaults() -> Self {
        let patterns = vec![
            HighlightPattern {
                name: "ERROR".to_string(),
                pattern: r"(?i)\berror\b".to_string(),
                color: "#45203f".to_string(), // dark red
                enabled: true,
            },
            HighlightPattern {
                name: "WARNING".to_string(),
                pattern: r"(?i)\bwarn(ing)?\b".to_string(),
                color: "#3d3520".to_string(), // dark yellow
                enabled: true,
            },
            HighlightPattern {
                name: "INFO".to_string(),
                pattern: r"(?i)\binfo\b".to_string(),
                color: "#1e3520".to_string(), // dark green
                enabled: false,
            },
            HighlightPattern {
                name: "DEBUG".to_string(),
                pattern: r"(?i)\bdebug\b".to_string(),
                color: "#1e2035".to_string(), // dark blue
                enabled: false,
            },
        ];

        let engines = patterns.iter().map(|p| p.compile()).collect();

        Self { patterns, engines }
    }

    /// Set the patterns and recompile engines.
    pub fn set_patterns(&mut self, patterns: Vec<HighlightPattern>) {
        self.engines = patterns.iter().map(|p| p.compile()).collect();
        self.patterns = patterns;
    }

    /// Get the patterns.
    pub fn patterns(&self) -> &[HighlightPattern] {
        &self.patterns
    }

    /// Find the first matching highlight color for a line.
    /// Returns the background color of the first matching pattern.
    pub fn highlight_line(&self, line: &[u8]) -> Option<Hsla> {
        for (i, engine) in self.engines.iter().enumerate() {
            if let Some(engine) = engine {
                if engine.scan_first(line).ok().flatten().is_some() {
                    return self.patterns[i].color_hsla();
                }
            }
        }
        None
    }

    /// Check if any patterns are enabled.
    pub fn has_active_patterns(&self) -> bool {
        self.patterns.iter().any(|p| p.enabled)
    }
}

/// Parse a hex color string like "#ff0000" or "#ff000080" to Hsla.
fn parse_hex_color(s: &str) -> Option<Hsla> {
    let s = s.trim_start_matches('#');
    match s.len() {
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            Some(rgb_to_hsla(r, g, b, 255))
        }
        8 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            let a = u8::from_str_radix(&s[6..8], 16).ok()?;
            Some(rgb_to_hsla(r, g, b, a))
        }
        _ => None,
    }
}

/// Convert 8-bit-per-channel sRGB+alpha to GPUI's `Hsla` (all channels 0..=1,
/// hue normalized to [0, 1) rather than degrees). Follows the standard
/// HSL formula; the chroma-zero case (max == min) keeps hue/saturation at 0.
fn rgb_to_hsla(r: u8, g: u8, b: u8, a: u8) -> Hsla {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let a = a as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    let (h, s) = if (max - min).abs() < f32::EPSILON {
        (0.0, 0.0)
    } else {
        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };
        let h = if (max - r).abs() < f32::EPSILON {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if (max - g).abs() < f32::EPSILON {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        (h / 6.0, s)
    };

    Hsla { h, s, l, a }
}
