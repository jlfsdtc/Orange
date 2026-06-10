//! Predefined filters: save and quickly apply commonly used search patterns.
//!
//! Users can save filter patterns with names and colors, then apply them
//! from a dropdown or with keyboard shortcuts.

use gpui::*;
use serde::{Deserialize, Serialize};

use crate::theme::Theme;

// Actions for predefined filters.
actions!(
    orange,
    [ToggleFilterPanel, ApplyFilter, EditFilters]
);

/// A single predefined filter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredefinedFilter {
    /// Display name.
    pub name: String,
    /// Search pattern (regex or literal).
    pub pattern: String,
    /// Whether this is a regex pattern (true) or literal (false).
    pub is_regex: bool,
    /// Case insensitive matching.
    pub case_insensitive: bool,
    /// Background color for matching lines (hex string).
    pub color: String,
    /// Keyboard shortcut index (1-9, 0 = none).
    pub shortcut: Option<u8>,
    /// Whether this filter is enabled.
    pub enabled: bool,
}

impl PredefinedFilter {
    /// Create a new filter.
    pub fn new(name: &str, pattern: &str) -> Self {
        Self {
            name: name.to_string(),
            pattern: pattern.to_string(),
            is_regex: false,
            case_insensitive: true,
            color: "#3d3520".to_string(),
            shortcut: None,
            enabled: true,
        }
    }

    /// Create a regex filter.
    pub fn regex(name: &str, pattern: &str) -> Self {
        Self {
            name: name.to_string(),
            pattern: pattern.to_string(),
            is_regex: true,
            case_insensitive: true,
            color: "#3d3520".to_string(),
            shortcut: None,
            enabled: true,
        }
    }
}

/// Predefined filters state.
pub struct PredefinedFiltersState {
    /// The saved filters.
    filters: Vec<PredefinedFilter>,
    /// Whether the filter panel is visible.
    visible: bool,
}

impl PredefinedFiltersState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        Self {
            filters: Self::default_filters(),
            visible: false,
        }
    }

    /// Default predefined filters for common log levels.
    fn default_filters() -> Vec<PredefinedFilter> {
        vec![
            PredefinedFilter {
                name: "Errors".to_string(),
                pattern: r"(?i)\berror\b".to_string(),
                is_regex: true,
                case_insensitive: true,
                color: "#45203f".to_string(),
                shortcut: Some(1),
                enabled: true,
            },
            PredefinedFilter {
                name: "Warnings".to_string(),
                pattern: r"(?i)\bwarn(ing)?\b".to_string(),
                is_regex: true,
                case_insensitive: true,
                color: "#3d3520".to_string(),
                shortcut: Some(2),
                enabled: true,
            },
            PredefinedFilter {
                name: "Info".to_string(),
                pattern: r"(?i)\binfo\b".to_string(),
                is_regex: true,
                case_insensitive: true,
                color: "#1e3520".to_string(),
                shortcut: Some(3),
                enabled: false,
            },
            PredefinedFilter {
                name: "Debug".to_string(),
                pattern: r"(?i)\bdebug\b".to_string(),
                is_regex: true,
                case_insensitive: true,
                color: "#1e2035".to_string(),
                shortcut: Some(4),
                enabled: false,
            },
            PredefinedFilter {
                name: "Exceptions".to_string(),
                pattern: r"(?i)\bexception\b".to_string(),
                is_regex: true,
                case_insensitive: true,
                color: "#45203f".to_string(),
                shortcut: Some(5),
                enabled: true,
            },
            PredefinedFilter {
                name: "HTTP Errors".to_string(),
                pattern: r"\b[45]\d{2}\b".to_string(),
                is_regex: true,
                case_insensitive: false,
                color: "#3d2020".to_string(),
                shortcut: Some(6),
                enabled: true,
            },
        ]
    }

    /// Load filters from config.
    pub fn load(&mut self) {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(filters) = serde_json::from_str(&content) {
                    self.filters = filters;
                }
            }
        }
    }

    /// Save filters to config.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&self.filters)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Toggle visibility.
    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.visible = !self.visible;
        cx.notify();
    }

    /// Whether the panel is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Get all filters.
    pub fn filters(&self) -> &[PredefinedFilter] {
        &self.filters
    }

    /// Get enabled filters only.
    pub fn enabled_filters(&self) -> Vec<&PredefinedFilter> {
        self.filters.iter().filter(|f| f.enabled).collect()
    }

    /// Apply a filter by index.
    pub fn apply_filter(&self, index: usize) -> Option<&PredefinedFilter> {
        self.filters.get(index).filter(|f| f.enabled)
    }

    /// Apply a filter by shortcut number.
    pub fn apply_by_shortcut(&self, num: u8) -> Option<&PredefinedFilter> {
        self.filters
            .iter()
            .find(|f| f.shortcut == Some(num) && f.enabled)
    }

    /// Add a new filter.
    pub fn add_filter(&mut self, filter: PredefinedFilter) {
        self.filters.push(filter);
    }

    /// Remove a filter by index.
    pub fn remove_filter(&mut self, index: usize) {
        if index < self.filters.len() {
            self.filters.remove(index);
        }
    }

    /// Toggle a filter's enabled state.
    pub fn toggle_filter(&mut self, index: usize) {
        if let Some(f) = self.filters.get_mut(index) {
            f.enabled = !f.enabled;
        }
    }

    fn config_path() -> std::path::PathBuf {
        orange_settings::config_dir().join("filters.json")
    }

    /// Render the filter panel.
    pub fn render_panel(&self, theme: Theme) -> Option<AnyElement> {
        if !self.visible {
            return None;
        }


        Some(
            div()
                .absolute()
                .right_0()
                .top(px(32.0)) // Below tab bar
                .w(px(300.0))
                .max_h(px(400.0))
                .bg(theme.background)
                .border_1()
                .border_color(theme.selection)
                .rounded_lg()
                .shadow_lg()
                .child(
                    // Header
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .py_2()
                        .bg(theme.current_line)
                        .border_b_1()
                        .border_color(theme.selection)
                        .rounded_t_lg()
                        .child(
                            div()
                                .text_color(theme.foreground)
                                .child("Predefined Filters"),
                        ),
                )
                .child(
                    // Filter list
                    div()
                        .flex()
                        .flex_col()
                        .children(self.filters.iter().map(|filter| {
                            let is_enabled = filter.enabled;

                            let bg = if is_enabled {
                                theme.background
                            } else {
                                theme.current_line
                            };

                            let text_color = if is_enabled {
                                theme.foreground
                            } else {
                                theme.line_number
                            };

                            let shortcut_text = filter
                                .shortcut
                                .map(|s| format!("Alt+{}", s))
                                .unwrap_or_default();

                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .px_3()
                                .py_2()
                                .bg(bg)
                                .border_b_1()
                                .border_color(theme.selection)
                                .child(
                                    // Enable/disable checkbox
                                    div()
                                        .w(px(16.0))
                                        .h(px(16.0))
                                        .border_1()
                                        .border_color(theme.selection)
                                        .rounded_sm()
                                        .child(if is_enabled {
                                            div()
                                                .w_full()
                                                .h_full()
                                                .bg(theme.search_current)
                                                .rounded_sm()
                                                .into_any()
                                        } else {
                                            div().into_any()
                                        }),
                                )
                                .child(
                                    // Filter name
                                    div()
                                        .flex_grow()
                                        .text_color(text_color)
                                        .child(filter.name.clone()),
                                )
                                .child(
                                    // Shortcut
                                    div()
                                        .text_color(theme.line_number)
                                        .text_sm()
                                        .child(shortcut_text),
                                )
                                .into_any()
                        })),
                )
                .into_any(),
        )
    }
}
