//! Overview/Minimap: a miniature representation of the entire log file.

use gpui::*;

use crate::theme::Theme;

/// Overview state.
pub struct OverviewState {
    total_lines: u64,
    matches: Vec<u64>,
    bookmarks: Vec<u64>,
    viewport_start: u64,
    viewport_size: u64,
    visible: bool,
}

impl OverviewState {
    pub fn new() -> Self {
        Self {
            total_lines: 0,
            matches: Vec::new(),
            bookmarks: Vec::new(),
            viewport_start: 0,
            viewport_size: 50,
            visible: true,
        }
    }

    pub fn set_total_lines(&mut self, lines: u64) {
        self.total_lines = lines;
    }

    pub fn set_matches(&mut self, matches: Vec<u64>) {
        self.matches = matches;
    }

    pub fn set_bookmarks(&mut self, bookmarks: Vec<u64>) {
        self.bookmarks = bookmarks;
    }

    pub fn set_viewport(&mut self, start: u64, size: u64) {
        self.viewport_start = start;
        self.viewport_size = size;
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Render the overview as a colored div with markers.
    pub fn render(&self, theme: Theme) -> AnyElement {
        if !self.visible || self.total_lines == 0 {
            return div().into_any();
        }

        let total = self.total_lines as f32;

        // Calculate viewport position as percentage
        let vp_pct = (self.viewport_start as f32 / total * 100.0).min(100.0);
        let vp_size_pct = (self.viewport_size as f32 / total * 100.0).max(2.0).min(100.0 - vp_pct);

        div()
            .w(px(48.0))
            .h_full()
            .bg(theme.current_line)
            .border_l_1()
            .border_color(theme.selection)
            .relative()
            // Viewport indicator
            .child(
                div()
                    .absolute()
                    .top(rems(vp_pct / 100.0 * 20.0)) // Approximate positioning
                    .w_full()
                    .h(rems(vp_size_pct / 100.0 * 20.0))
                    .bg(theme.selection)
                    .border_1()
                    .border_color(theme.line_number),
            )
            // Match indicators (simplified - just show count)
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .w_full()
                    .px_1()
                    .text_xs()
                    .text_color(theme.line_number)
                    .child(format!("M:{}", self.matches.len())),
            )
            .into_any()
    }
}
