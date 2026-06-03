//! Filtered view: displays only lines matching a search pattern.
//!
//! Uses orange-core's LogFilteredData with roaring bitmaps for efficient
//! storage and fast navigation between matches.

use gpui::*;
use orange_core::LogData;
use orange_regex::RegexFlags;
use std::sync::Arc;

use crate::h_scrollbar::{self, HScrollState};
use crate::text_selection::{
    char_advance_for, column_for_x, join_char_range, join_full_lines, render_line_content,
    word_range_at, CharPos, CharSelection, LineHighlight, LineSelection, Selection,
};
use crate::theme::{FontSettings, Theme};

// Actions for filtered view.
actions!(orange, [ToggleFilteredView]);

/// Events emitted by the filtered view to the surrounding window.
pub enum FilteredViewEvent {
    /// User right-clicked at this window-relative position; show the menu.
    ShowContextMenu(Point<Pixels>),
    /// The user changed the selection by interacting with this view. Lets the
    /// window route a later copy action to the view the user last touched.
    SelectionChanged,
}

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
    /// Horizontal scroll state. Sized from the source file's longest line, so
    /// the bar is stable regardless of which matches are shown. The gutter
    /// stays fixed while line content scrolls sideways.
    h_scroll: HScrollState,
    /// Active selection. The `line` coordinate is a **row index** into
    /// `matching_lines` (not an absolute file line), since the filtered view
    /// shows a non-contiguous subset; copy maps each row back to its source
    /// line. `None` when nothing is selected.
    selection: Option<Selection>,
    /// Mouse-down anchor recorded on a non-shift single click, used to fall
    /// back to a whole-row selection if the user releases without dragging.
    /// Cleared as soon as a drag is detected. Its `line` is a row index.
    pending_click: Option<CharPos>,
    /// Row index under the most recent right-click, a fallback copy target
    /// when there is no active selection.
    right_click_row: Option<usize>,
}

impl EventEmitter<FilteredViewEvent> for FilteredViewState {}

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
            h_scroll: HScrollState::new(),
            selection: None,
            pending_click: None,
            right_click_row: None,
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
        self.clear_selection();
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
        self.clear_selection();
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

    /// Drop the active selection and its fallback anchors.
    fn clear_selection(&mut self) {
        self.selection = None;
        self.pending_click = None;
        self.right_click_row = None;
    }

    /// Number of result rows the current selection spans (0 when nothing is
    /// selected). Mirrors `LogViewState::selection_line_count`.
    pub fn selection_line_count(&self) -> u64 {
        self.selection
            .as_ref()
            .map(|s| s.hi_line() - s.lo_line() + 1)
            .unwrap_or(0)
    }

    /// Collect the source-file bytes for the result rows `lo..=hi` (row
    /// indices into `matching_lines`), in display order. Used by the copy
    /// helpers before joining.
    fn rows_bytes(&self, lo_row: u64, hi_row: u64) -> Vec<Vec<u8>> {
        let Some(data) = self.log_data.as_ref() else {
            return Vec::new();
        };
        (lo_row..=hi_row)
            .filter_map(|r| self.matching_lines.get(r as usize).copied())
            .map(|line| data.get_line(line).unwrap_or_default())
            .collect()
    }

    /// Text of the current selection joined by `\n`. For a character selection
    /// the first/last rows are sliced at the selection endpoints; for a row
    /// (line) selection every row is returned whole. Falls back to the
    /// right-clicked row when there is no active selection. Mirrors
    /// `LogViewState::selection_text`.
    pub fn selection_text(&self) -> Option<String> {
        self.log_data.as_ref()?;
        let Some(sel) = self.selection.as_ref() else {
            let row = self.right_click_row?;
            return Some(join_full_lines(&self.rows_bytes(row as u64, row as u64)));
        };
        if let Selection::Char(s) = sel {
            if s.is_empty() {
                let row = self.right_click_row?;
                return Some(join_full_lines(&self.rows_bytes(row as u64, row as u64)));
            }
        }
        let lo_row = sel.lo_line();
        let hi_row = sel.hi_line();
        let rows = self.rows_bytes(lo_row, hi_row);
        match sel {
            Selection::Line(_) => Some(join_full_lines(&rows)),
            Selection::Char(c) => {
                let (lo, hi) = c.ordered();
                // `join_char_range` only uses lo/hi to detect the first and
                // last entries, so passing the row-index range works.
                Some(join_char_range(&rows, lo.col, hi.col, lo_row, hi_row))
            }
        }
    }

    /// Full-line text for every row the selection touches (whole rows even for
    /// a sub-line character selection). Falls back to the right-clicked row.
    /// Mirrors `LogViewState::selected_lines_text`.
    pub fn selected_lines_text(&self) -> Option<String> {
        self.log_data.as_ref()?;
        let (lo_row, hi_row) = match self.selection.as_ref() {
            Some(sel) => (sel.lo_line(), sel.hi_line()),
            None => {
                let row = self.right_click_row?;
                (row as u64, row as u64)
            }
        };
        Some(join_full_lines(&self.rows_bytes(lo_row, hi_row)))
    }

    /// Clear the right-click fallback. Called when the context menu closes so
    /// a later Cmd+C doesn't copy a stale row.
    pub fn clear_right_click(&mut self) {
        self.right_click_row = None;
    }

    fn handle_mouse_down(
        &mut self,
        row: u64,
        col: usize,
        shift: bool,
        click_count: usize,
        row_text: &str,
        cx: &mut Context<Self>,
    ) {
        // Any mouse-down here means the user is now selecting in the filtered
        // view; tell the window so a later Cmd+C copies from this view.
        cx.emit(FilteredViewEvent::SelectionChanged);
        if shift {
            match &mut self.selection {
                Some(Selection::Line(sel)) => sel.head = row,
                Some(Selection::Char(sel)) => sel.head = CharPos::new(row, 0),
                None => self.selection = Some(Selection::Line(LineSelection::single(row))),
            }
            self.pending_click = None;
            cx.notify();
            return;
        }
        if click_count >= 2 {
            if let Some((start, end)) = word_range_at(row_text, col) {
                self.selection = Some(Selection::Char(CharSelection {
                    anchor: CharPos::new(row, start),
                    head: CharPos::new(row, end),
                }));
                cx.notify();
            }
            self.pending_click = None;
            return;
        }
        // Single click: start a zero-width char selection so a drag can grow
        // it, but remember the click so mouse_up can fall back to whole-row
        // select when no drag happens.
        let anchor = CharPos::new(row, col);
        self.selection = Some(Selection::Char(CharSelection { anchor, head: anchor }));
        self.pending_click = Some(anchor);
        cx.notify();
    }

    fn handle_mouse_drag(&mut self, row: u64, col: usize, cx: &mut Context<Self>) {
        match &mut self.selection {
            Some(Selection::Char(sel)) => {
                sel.head = CharPos::new(row, col);
                self.pending_click = None;
                cx.notify();
            }
            Some(Selection::Line(sel)) => {
                sel.head = row;
                cx.notify();
            }
            None => {}
        }
    }

    fn handle_mouse_up(&mut self, cx: &mut Context<Self>) {
        if let Some(anchor) = self.pending_click.take() {
            if let Some(Selection::Char(sel)) = &self.selection {
                if sel.is_empty() {
                    self.selection = Some(Selection::Line(LineSelection::single(anchor.line)));
                    cx.notify();
                }
            }
        }
    }

    fn handle_right_click(&mut self, row: usize, pos: Point<Pixels>, cx: &mut Context<Self>) {
        // Preserve the existing selection; only remember the clicked row as a
        // fallback target for the menu's copy actions.
        self.right_click_row = Some(row);
        cx.emit(FilteredViewEvent::ShowContextMenu(pos));
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
        let font_family = font.family.clone();
        let current_index = self.current_index;
        let matching_lines = self.matching_lines.clone();
        let log_data = self.log_data.clone();
        let pattern = self.pattern.clone();
        let selection = self.selection;
        let entity = cx.entity();

        // Size the gutter for the largest line number in the result set so
        // big files still render their full row number.
        let max_line = *matching_lines.last().unwrap_or(&0);
        let gutter_digits = filtered_digits_for(max_line + 1);
        let char_advance = char_advance_for(font_size);
        let gutter_width = gutter_digits as f32 * char_advance + 16.0 + 1.0;

        // Horizontal scroll, sized from the source file's longest line (a byte
        // count, slightly over-estimating multi-byte UTF-8 width). The content
        // of each row is shifted left by `h_offset`; the gutter stays fixed.
        let h_offset = self.h_scroll.offset();
        let content_w = self
            .log_data
            .as_ref()
            .map(|d| d.max_line_length())
            .unwrap_or(0) as f32
            * char_advance;

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
                            let row = i as u64;

                            let line_text = log_data
                                .as_ref()
                                .and_then(|d| d.get_line(line_num))
                                .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
                                .unwrap_or_default();
                            let line_char_len = line_text.chars().count();
                            let highlight = selection
                                .map(|s| s.highlight_for(row, line_char_len))
                                .unwrap_or(LineHighlight::None);

                            // A full-row selection wins over the current-match
                            // tint; otherwise keep the search-current highlight.
                            let bg = match highlight {
                                LineHighlight::Full => theme.selection,
                                _ if is_current => theme.search_current,
                                _ => theme.background,
                            };

                            let down_entity = entity.clone();
                            let move_entity = entity.clone();
                            let up_entity = entity.clone();
                            let right_entity = entity.clone();
                            let down_text = line_text.clone();
                            let move_text = line_text.clone();

                            div()
                                .flex()
                                .items_start()
                                .h(line_height)
                                .bg(bg)
                                .w_full()
                                .text_size(font_size)
                                .font_family(font_family.clone())
                                .on_mouse_down(MouseButton::Left, move |event, _window, cx| {
                                    let shift = event.modifiers.shift;
                                    let click_count = event.click_count;
                                    let col = column_for_x(
                                        f32::from(event.position.x),
                                        gutter_width,
                                        char_advance,
                                        h_offset,
                                        down_text.chars().count(),
                                    );
                                    down_entity.update(cx, |this, cx| {
                                        this.handle_mouse_down(
                                            row,
                                            col,
                                            shift,
                                            click_count,
                                            &down_text,
                                            cx,
                                        );
                                    });
                                })
                                .on_mouse_move(move |event, _window, cx| {
                                    if !event.dragging() {
                                        return;
                                    }
                                    let col = column_for_x(
                                        f32::from(event.position.x),
                                        gutter_width,
                                        char_advance,
                                        h_offset,
                                        move_text.chars().count(),
                                    );
                                    move_entity.update(cx, |this, cx| {
                                        this.handle_mouse_drag(row, col, cx);
                                    });
                                })
                                .on_mouse_up(MouseButton::Left, move |_event, _window, cx| {
                                    up_entity.update(cx, |this, cx| {
                                        this.handle_mouse_up(cx);
                                    });
                                })
                                .on_mouse_down(MouseButton::Right, move |event, _window, cx| {
                                    let pos = event.position;
                                    right_entity.update(cx, |this, cx| {
                                        this.handle_right_click(i, pos, cx);
                                    });
                                })
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
                                    // Clip the content and shift it left by the
                                    // horizontal offset; the gutter stays put.
                                    // The inner div is sized to the full content
                                    // width so it doesn't collapse under flex.
                                    div()
                                        .flex_grow()
                                        .overflow_hidden()
                                        .child(
                                            div()
                                                .w(px(content_w))
                                                .ml(px(-h_offset))
                                                .whitespace_nowrap()
                                                .child(render_line_content(
                                                    &line_text, highlight, theme,
                                                )),
                                        ),
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

        let entity_wheel = cx.entity();
        let scrollbar = h_scrollbar::render(
            &mut self.h_scroll,
            |this: &mut FilteredViewState| &mut this.h_scroll,
            content_w,
            gutter_width,
            theme,
            cx,
        );

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            .child(header)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_grow()
                    .overflow_hidden()
                    .child(list)
                    .on_scroll_wheel(move |event, _window, cx| {
                        let delta = event.delta.pixel_delta(line_height);
                        let dx = if event.shift {
                            f32::from(delta.y)
                        } else {
                            f32::from(delta.x)
                        };
                        if dx != 0.0 {
                            entity_wheel.update(cx, |this, cx| {
                                if this.h_scroll.scroll_by(-dx, content_w) {
                                    cx.notify();
                                }
                            });
                        }
                    }),
            )
            .child(scrollbar)
            .into_any()
    }
}

fn filtered_digits_for(n: u64) -> usize {
    if n == 0 {
        return 1;
    }
    (n as f64).log10().floor() as usize + 1
}
