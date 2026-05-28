//! Filtered view: displays only lines matching a search pattern.
//!
//! Uses orange-core's LogFilteredData with roaring bitmaps for efficient
//! storage and fast navigation between matches.

use gpui::*;
use orange_core::LogData;
use orange_regex::RegexFlags;
use std::sync::Arc;

use crate::theme::{FontSettings, Theme};

// Actions for filtered view.
actions!(orange, [ToggleFilteredView]);

/// State for the filtered view component.
pub struct FilteredViewState {
    /// The source log data.
    log_data: Option<Arc<LogData>>,
    /// Matching line numbers (from roaring bitmap).
    matching_lines: Vec<u64>,
    /// Total match count.
    match_count: usize,
    /// Current position in match list.
    current_index: usize,
    /// Whether the filtered view is visible.
    visible: bool,
    /// Current search pattern.
    pattern: String,
    /// Scroll handle.
    scroll_handle: UniformListScrollHandle,
}

impl FilteredViewState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe_global::<FontSettings>(|_, cx| cx.notify()).detach();
        Self {
            log_data: None,
            matching_lines: Vec::new(),
            match_count: 0,
            current_index: 0,
            visible: false,
            pattern: String::new(),
            scroll_handle: UniformListScrollHandle::default(),
        }
    }

    /// Toggle visibility.
    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.visible = !self.visible;
        cx.notify();
    }

    /// Whether the view is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Set the source log data.
    pub fn set_log_data(&mut self, data: Option<Arc<LogData>>, cx: &mut Context<Self>) {
        self.log_data = data;
        self.matching_lines.clear();
        self.match_count = 0;
        self.current_index = 0;
        cx.notify();
    }

    /// Run a search and populate the filtered view.
    pub fn search(&mut self, pattern: &str, cx: &mut Context<Self>) {
        self.pattern = pattern.to_string();

        let Some(data) = &self.log_data else {
            self.matching_lines.clear();
            self.match_count = 0;
            cx.notify();
            return;
        };

        // Use orange-regex to search
        let flags = RegexFlags::default();
        let engine = match orange_regex::RegexEngine::compile(pattern, flags) {
            Ok(e) => e,
            Err(_) => {
                self.matching_lines.clear();
                self.match_count = 0;
                cx.notify();
                return;
            }
        };

        let mut matches = Vec::new();
        let total = data.line_count();
        let batch = 1000u64;

        let mut line = 0u64;
        while line < total {
            let end = (line + batch).min(total);
            let lines = data.get_lines(line, (end - line) as usize);
            for (i, text) in lines.iter().enumerate() {
                if engine.scan_first(text).ok().flatten().is_some() {
                    matches.push(line + i as u64);
                }
            }
            line = end;
        }

        self.match_count = matches.len();
        self.matching_lines = matches;
        self.current_index = 0;
        self.visible = true;
        cx.notify();
    }

    /// Navigate to the next match.
    pub fn next_match(&mut self, cx: &mut Context<Self>) {
        if self.match_count > 0 {
            self.current_index = (self.current_index + 1) % self.match_count;
            self.scroll_handle
                .scroll_to_item(self.current_index, ScrollStrategy::Center);
            cx.notify();
        }
    }

    /// Navigate to the previous match.
    pub fn prev_match(&mut self, cx: &mut Context<Self>) {
        if self.match_count > 0 {
            self.current_index = if self.current_index == 0 {
                self.match_count - 1
            } else {
                self.current_index - 1
            };
            self.scroll_handle
                .scroll_to_item(self.current_index, ScrollStrategy::Center);
            cx.notify();
        }
    }

    /// Get the original line number of the current match.
    pub fn current_match_line(&self) -> Option<u64> {
        self.matching_lines.get(self.current_index).copied()
    }

    /// Match count.
    pub fn match_count(&self) -> usize {
        self.match_count
    }

    /// Current pattern.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Get matching line numbers.
    pub fn matching_lines(&self) -> &[u64] {
        &self.matching_lines
    }

    /// Render the filtered view.
    pub fn render_view(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if !self.visible {
            return div().into_any();
        }

        let total = self.matching_lines.len();
        let theme = *cx.global::<Theme>();
        let font = cx.global::<FontSettings>().clone();
        let line_height = font.line_height();
        let font_size = font.size;
        let current_index = self.current_index;
        let matching_lines = self.matching_lines.clone();
        let log_data = self.log_data.clone();
        let pattern = self.pattern.clone();

        // Size the gutter for the largest line number in the result set so
        // big files still render their full row number. Char advance matches
        // log_view::char_advance_for (0.6 × font size).
        let max_line = *matching_lines.last().unwrap_or(&0);
        let gutter_digits = filtered_digits_for(max_line + 1);
        let char_advance = f32::from(font_size) * 0.6;
        let gutter_width = gutter_digits as f32 * char_advance + 16.0 + 1.0;

        // Header
        let header = div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(28.0))
            .px_3()
            .bg(theme.current_line)
            .border_b_1()
            .border_color(theme.selection)
            .child(
                div()
                    .text_color(theme.search_match)
                    .child(format!("Filtered: {}", pattern)),
            )
            .child(
                div()
                    .text_color(theme.line_number)
                    .child(format!("{} matches", total)),
            );

        // Match list
        let list = if total == 0 {
            div()
                .flex_grow()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.line_number)
                .child("No matches")
                .into_any()
        } else {
            uniform_list(
                "filtered_lines",
                total,
                move |range, _window, _cx| {
                    range
                        .map(|i| {
                            let line_num = matching_lines[i];
                            let is_current = i == current_index;

                            let line_text = log_data
                                .as_ref()
                                .and_then(|d| d.get_line(line_num))
                                .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
                                .unwrap_or_default();

                            let bg = if is_current {
                                theme.search_current
                            } else {
                                theme.background
                            };

                            div()
                                .flex()
                                .items_start()
                                .h(line_height)
                                .bg(bg)
                                .w_full()
                                .text_size(font_size)
                                .child(
                                    div()
                                        .w(px(gutter_width))
                                        .flex_shrink_0()
                                        .text_color(theme.line_number)
                                        .child(format!(
                                            "{:>width$}",
                                            line_num + 1,
                                            width = gutter_digits
                                        )),
                                )
                                .child(
                                    div()
                                        .flex_grow()
                                        .text_color(theme.foreground)
                                        .child(line_text),
                                )
                                .into_any()
                        })
                        .collect()
                },
            )
            .track_scroll(self.scroll_handle.clone())
            .flex_grow()
            .into_any()
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            .child(header)
            .child(list)
            .into_any()
    }
}

fn filtered_digits_for(n: u64) -> usize {
    if n == 0 {
        return 1;
    }
    (n as f64).log10().floor() as usize + 1
}
