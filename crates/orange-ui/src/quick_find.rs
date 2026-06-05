//! Quick find bar for incremental search within the log view.
//!
//! When visible, the bar owns keyboard focus: every character keystroke that
//! doesn't carry a primary modifier (cmd/ctrl) is appended to the query, and a
//! background literal search runs against the active log file. On match
//! navigation (Enter, F3, Cmd+G), it emits `QuickFindEvent::JumpTo(line)` so
//! the parent view can scroll/select that line in the active LogView.

use gpui::prelude::FluentBuilder;
use gpui::*;
use orange_core::LogData;
use orange_regex::{RegexEngine, RegexFlags};
use std::sync::Arc;

use crate::h_scrollbar::{self, HScrollState};
use crate::text_selection::{
    char_advance_for, column_for_x, join_char_range, join_full_lines, render_line_content,
    word_range_at, CharPos, CharSelection, LineHighlight, LineSelection, Selection,
};
use crate::theme::{FontSettings, Theme};
use crate::v_scrollbar::{self, VScrollState};

// Actions for quick find.
actions!(
    orange,
    [ToggleQuickFind, FindNext, FindPrevious, CloseFind, ExecuteSearch]
);

/// Events emitted by `QuickFindState`.
#[derive(Clone, Debug)]
pub enum QuickFindEvent {
    /// The view should scroll to and select this 0-based line number.
    JumpTo(u64),
    /// User right-clicked at this window-relative position in the results
    /// list; the window should show the copy context menu.
    ShowContextMenu(Point<Pixels>),
    /// The user changed the results-list selection by interacting with it.
    /// Lets the window route a later copy action to this view.
    SelectionChanged,
}

/// Quick find bar state.
pub struct QuickFindState {
    /// Whether the find bar is visible.
    visible: bool,
    /// Current search query.
    query: String,
    /// Optional selection over `query` as (start_byte, end_byte) with start <= end.
    /// `None` means no selection (a collapsed caret at `caret`).
    selection: Option<(usize, usize)>,
    /// Byte offset of the text insertion point into `query` (`0..=query.len()`,
    /// always on a char boundary). Where typed text is inserted and where the
    /// blinking caret is drawn when no selection is active.
    caret: usize,
    /// Blink phase: `true` paints the caret bar, `false` hides it. Toggled by
    /// the blink task; forced to `true` on any caret-moving or editing action so
    /// the caret shows solid immediately, then resumes blinking.
    caret_on: bool,
    /// Guard so at most one blink task runs at a time. Set when the task starts,
    /// cleared when it exits (on the bar being hidden).
    blinking: bool,
    /// Matching line numbers.
    matching_lines: Vec<u64>,
    /// Byte ranges within each matching line where the query hit. Indices
    /// align with `matching_lines`. Ranges are against the line content with
    /// any trailing `\n` stripped, so they're safe to slice in the renderer.
    line_matches: Vec<Vec<(usize, usize)>>,
    /// Current match index (0-based).
    current_match: usize,
    /// Reference to log data for searching.
    log_data: Option<Arc<LogData>>,
    /// Keyboard focus for the bar's text input.
    focus_handle: FocusHandle,
    /// Scroll handle for the results list — driven both by the user (mouse
    /// wheel) and by `next_match`/`prev_match` to keep the active row visible.
    results_scroll: UniformListScrollHandle,
    /// When true, `query` is treated as a regex pattern; otherwise a literal
    /// case-insensitive substring.
    regex_mode: bool,
    /// Last regex compile error, if any. Populated only in regex mode when
    /// the user's pattern doesn't compile — surfaced in the bar so they know
    /// why there are zero matches.
    regex_error: Option<String>,
    /// Byte offset where the current mouse-drag selection started. `None`
    /// means no drag is in progress. Cleared on mouse_up and on visibility
    /// toggles so a missed mouse_up doesn't poison the next interaction.
    drag_anchor: Option<usize>,
    /// User-resized results-panel height. Persists across show/hide so the
    /// user's preferred size sticks.
    panel_height: Pixels,
    /// Active resize drag: `(anchor_mouse_y, panel_height_at_anchor)`. The
    /// handle is at the TOP of the find component, so moving the mouse UP
    /// grows the panel.
    resize_anchor: Option<(Pixels, Pixels)>,
    /// Vertical scrollbar state (drag + measured track bounds) for the results
    /// list. The scroll position itself lives in `results_scroll`; this only
    /// drives the bar.
    v_scroll: VScrollState,
    /// Horizontal scroll state for the results list. Sized from the source
    /// file's longest line so the bar is stable regardless of which matches
    /// are shown; the gutter stays fixed while line content scrolls sideways.
    h_scroll: HScrollState,
    /// Selection over the *results list* (distinct from `selection`, which is
    /// over the query input). The `line` coordinate is a **row index** into
    /// `matching_lines`, since the results list shows a non-contiguous subset
    /// of the file; copy maps each row back to its source line. `None` when
    /// nothing is selected.
    result_selection: Option<Selection>,
    /// Mouse-down anchor on a non-shift single click in the results list, used
    /// to fall back to a whole-row selection when the user releases without
    /// dragging. Cleared once a drag is detected. Its `line` is a row index.
    result_pending_click: Option<CharPos>,
    /// Row index under the most recent right-click in the results list — a
    /// fallback copy target when there is no active result selection.
    result_right_click_row: Option<usize>,
}

const PANEL_MIN_PX: f32 = 60.0;
const PANEL_DEFAULT_PX: f32 = 240.0;

impl QuickFindState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe_global::<FontSettings>(|_, cx| cx.notify()).detach();
        Self {
            visible: false,
            query: String::new(),
            selection: None,
            caret: 0,
            caret_on: true,
            blinking: false,
            matching_lines: Vec::new(),
            line_matches: Vec::new(),
            current_match: 0,
            log_data: None,
            focus_handle: cx.focus_handle(),
            results_scroll: UniformListScrollHandle::default(),
            regex_mode: false,
            regex_error: None,
            drag_anchor: None,
            panel_height: px(PANEL_DEFAULT_PX),
            resize_anchor: None,
            v_scroll: VScrollState::new(),
            h_scroll: HScrollState::new(),
            result_selection: None,
            result_pending_click: None,
            result_right_click_row: None,
        }
    }

    /// Toggle regex matching mode and re-run the current search.
    pub fn toggle_regex_mode(&mut self, cx: &mut Context<Self>) {
        self.regex_mode = !self.regex_mode;
        self.execute_search(cx);
        self.emit_current_match(cx);
        cx.notify();
    }

    /// Whether the bar is in regex matching mode.
    pub fn is_regex_mode(&self) -> bool {
        self.regex_mode
    }

    /// Set the log data to search through. Called by MainWindow whenever the
    /// active tab or its loaded file changes.
    pub fn set_log_data(&mut self, data: Option<Arc<LogData>>, cx: &mut Context<Self>) {
        self.log_data = data;
        if !self.query.is_empty() {
            self.execute_search(cx);
        }
    }

    /// Toggle the find bar visibility. When showing, takes focus and replays
    /// the previous search against the current log data; when hiding, clears
    /// the query so a fresh search starts next time.
    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.visible = !self.visible;
        if self.visible {
            self.focus_handle.focus(window);
            self.caret = self.query.len();
            self.caret_on = true;
            self.start_blink(cx);
            if !self.query.is_empty() {
                self.execute_search(cx);
                self.emit_current_match(cx);
            }
        } else {
            self.query.clear();
            self.selection = None;
            self.caret = 0;
            self.matching_lines.clear();
            self.line_matches.clear();
            self.current_match = 0;
            self.regex_error = None;
            self.drag_anchor = None;
            self.resize_anchor = None;
            self.clear_result_selection();
        }
        cx.notify();
    }

    /// Make the find bar visible without toggling it off if it's already
    /// shown, and replay any existing query against the current log data.
    /// Unlike `toggle`, this never hides the bar and never grabs keyboard
    /// focus — used to auto-open the search box when a file finishes loading,
    /// so it doesn't steal focus from the freshly-loaded log view.
    pub fn show(&mut self, cx: &mut Context<Self>) {
        self.visible = true;
        self.caret = self.query.len();
        self.caret_on = true;
        self.start_blink(cx);
        if !self.query.is_empty() {
            self.execute_search(cx);
            self.emit_current_match(cx);
        }
        cx.notify();
    }

    /// Drive the blinking caret. Toggles `caret_on` every 500ms while the bar
    /// is visible and re-renders; exits (and clears `blinking`) once the bar is
    /// hidden. The `blinking` guard keeps a single task alive across repeated
    /// show calls. Caret visibility is additionally gated on focus at render
    /// time, so an unfocused-but-visible bar just toggles a hidden caret.
    fn start_blink(&mut self, cx: &mut Context<Self>) {
        if self.blinking {
            return;
        }
        self.blinking = true;
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            loop {
                executor.timer(std::time::Duration::from_millis(500)).await;
                let keep_going = cx
                    .update(|cx| {
                        this.update(cx, |state, cx| {
                            if !state.visible {
                                state.blinking = false;
                                return false;
                            }
                            state.caret_on = !state.caret_on;
                            cx.notify();
                            true
                        })
                        .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        })
        .detach();
    }

    /// Close the find bar.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.visible = false;
        self.query.clear();
        self.selection = None;
        self.caret = 0;
        self.matching_lines.clear();
        self.line_matches.clear();
        self.current_match = 0;
        self.regex_error = None;
        self.drag_anchor = None;
        self.resize_anchor = None;
        self.clear_result_selection();
        cx.notify();
    }

    /// Whether the find bar is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Current search query.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Execute the search with the current query. In literal mode, matches
    /// are case-insensitive substring hits; in regex mode, `query` is
    /// compiled with Hyperscan (caseless) and an invalid pattern is surfaced
    /// via `regex_error` instead of silently producing zero matches.
    pub fn execute_search(&mut self, cx: &mut Context<Self>) {
        self.matching_lines.clear();
        self.line_matches.clear();
        self.current_match = 0;
        self.regex_error = None;
        self.clear_result_selection();

        if self.query.is_empty() {
            cx.notify();
            return;
        }

        let Some(data) = self.log_data.clone() else {
            cx.notify();
            return;
        };

        if self.regex_mode {
            let flags = RegexFlags {
                case_insensitive: true,
                ..Default::default()
            };
            let engine = match RegexEngine::compile(&self.query, flags) {
                Ok(e) => e,
                Err(err) => {
                    self.regex_error = Some(err.to_string());
                    cx.notify();
                    return;
                }
            };
            let total = data.line_count();
            let batch = 1000u64;
            let mut line = 0u64;
            while line < total {
                let end = (line + batch).min(total);
                let lines = data.get_lines(line, (end - line) as usize);
                for (i, raw) in lines.iter().enumerate() {
                    let content = strip_trailing_nl(raw);
                    let matches = match engine.scan(content) {
                        Ok(m) => m,
                        Err(_) => continue,
                    };
                    if matches.is_empty() {
                        continue;
                    }
                    let mut ranges: Vec<(usize, usize)> = matches
                        .into_iter()
                        .filter(|m| m.from < m.to)
                        .map(|m| (m.from, m.to))
                        .collect();
                    ranges.sort_unstable();
                    self.matching_lines.push(line + i as u64);
                    self.line_matches.push(ranges);
                }
                line = end;
            }
        } else {
            let query_lower = self.query.to_lowercase();
            let total = data.line_count();
            let batch = 1000u64;
            let mut line = 0u64;
            while line < total {
                let end = (line + batch).min(total);
                let lines = data.get_lines(line, (end - line) as usize);
                for (i, raw) in lines.iter().enumerate() {
                    let content = strip_trailing_nl(raw);
                    let text = String::from_utf8_lossy(content);
                    let lower = text.to_lowercase();
                    if !lower.contains(&query_lower) {
                        continue;
                    }
                    // Compute highlight ranges only when case-folding kept
                    // byte offsets stable; otherwise we still record the
                    // line as a hit but with no highlight spans.
                    let mut ranges: Vec<(usize, usize)> = Vec::new();
                    if lower.len() == text.len() {
                        let mut pos = 0usize;
                        while let Some(rel) = lower[pos..].find(&query_lower) {
                            let start = pos + rel;
                            let end_b = start + query_lower.len();
                            if !text.is_char_boundary(start) || !text.is_char_boundary(end_b) {
                                break;
                            }
                            ranges.push((start, end_b));
                            pos = end_b;
                            if pos >= lower.len() {
                                break;
                            }
                        }
                    }
                    self.matching_lines.push(line + i as u64);
                    self.line_matches.push(ranges);
                }
                line = end;
            }
        }

        cx.notify();
    }

    /// Get the line number of the current match.
    pub fn current_match_line(&self) -> Option<u64> {
        self.matching_lines.get(self.current_match).copied()
    }

    /// Match count.
    pub fn match_count(&self) -> usize {
        self.matching_lines.len()
    }

    /// Move to the next match and emit a JumpTo event.
    pub fn next_match(&mut self, cx: &mut Context<Self>) {
        if !self.matching_lines.is_empty() {
            self.current_match = (self.current_match + 1) % self.matching_lines.len();
            self.emit_current_match(cx);
            self.scroll_current_into_view();
            cx.notify();
        }
    }

    /// Move to the previous match and emit a JumpTo event.
    pub fn prev_match(&mut self, cx: &mut Context<Self>) {
        if !self.matching_lines.is_empty() {
            self.current_match = if self.current_match == 0 {
                self.matching_lines.len() - 1
            } else {
                self.current_match - 1
            };
            self.emit_current_match(cx);
            self.scroll_current_into_view();
            cx.notify();
        }
    }

    /// Jump straight to a specific match index (used by clicks on the
    /// results list). No-op if `idx` is out of range.
    pub fn select_match(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx < self.matching_lines.len() {
            self.current_match = idx;
            self.emit_current_match(cx);
            cx.notify();
        }
    }

    /// Drop any results-list selection and its fallback anchors.
    fn clear_result_selection(&mut self) {
        self.result_selection = None;
        self.result_pending_click = None;
        self.result_right_click_row = None;
    }

    /// Number of result rows the current results selection spans (0 when
    /// nothing is selected). Mirrors `LogViewState::selection_line_count`.
    pub fn result_selection_line_count(&self) -> u64 {
        self.result_selection
            .as_ref()
            .map(|s| s.hi_line() - s.lo_line() + 1)
            .unwrap_or(0)
    }

    /// Collect the source-file bytes for result rows `lo..=hi` (row indices
    /// into `matching_lines`), in display order, for the copy helpers.
    fn result_rows_bytes(&self, lo_row: u64, hi_row: u64) -> Vec<Vec<u8>> {
        let Some(data) = self.log_data.as_ref() else {
            return Vec::new();
        };
        (lo_row..=hi_row)
            .filter_map(|r| self.matching_lines.get(r as usize).copied())
            .map(|line| data.get_line(line).unwrap_or_default())
            .collect()
    }

    /// Text of the current results selection joined by `\n`. For a character
    /// selection the first/last rows are sliced at the endpoints; for a row
    /// selection every row is returned whole. Falls back to the right-clicked
    /// row when there is no active selection. Mirrors
    /// `LogViewState::selection_text`.
    pub fn result_selection_text(&self) -> Option<String> {
        self.log_data.as_ref()?;
        let Some(sel) = self.result_selection.as_ref() else {
            let row = self.result_right_click_row?;
            return Some(join_full_lines(&self.result_rows_bytes(row as u64, row as u64)));
        };
        if let Selection::Char(s) = sel {
            if s.is_empty() {
                let row = self.result_right_click_row?;
                return Some(join_full_lines(&self.result_rows_bytes(row as u64, row as u64)));
            }
        }
        let lo_row = sel.lo_line();
        let hi_row = sel.hi_line();
        let rows = self.result_rows_bytes(lo_row, hi_row);
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

    /// Full-line text for every results row the selection touches (whole rows
    /// even for a sub-line character selection). Falls back to the
    /// right-clicked row. Mirrors `LogViewState::selected_lines_text`.
    pub fn result_selected_lines_text(&self) -> Option<String> {
        self.log_data.as_ref()?;
        let (lo_row, hi_row) = match self.result_selection.as_ref() {
            Some(sel) => (sel.lo_line(), sel.hi_line()),
            None => {
                let row = self.result_right_click_row?;
                (row as u64, row as u64)
            }
        };
        Some(join_full_lines(&self.result_rows_bytes(lo_row, hi_row)))
    }

    /// Clear the results right-click fallback. Called when the context menu
    /// closes so a later Cmd+C doesn't copy a stale row.
    pub fn clear_result_right_click(&mut self) {
        self.result_right_click_row = None;
    }

    fn handle_result_mouse_down(
        &mut self,
        row: u64,
        col: usize,
        shift: bool,
        click_count: usize,
        row_text: &str,
        cx: &mut Context<Self>,
    ) {
        // Any mouse-down in the results list means the user is now selecting
        // here; tell the window so a later Cmd+C copies from this view.
        cx.emit(QuickFindEvent::SelectionChanged);
        if shift {
            match &mut self.result_selection {
                Some(Selection::Line(sel)) => sel.head = row,
                Some(Selection::Char(sel)) => sel.head = CharPos::new(row, 0),
                None => {
                    self.result_selection = Some(Selection::Line(LineSelection::single(row)))
                }
            }
            self.result_pending_click = None;
            cx.notify();
            return;
        }
        if click_count >= 2 {
            if let Some((start, end)) = word_range_at(row_text, col) {
                self.result_selection = Some(Selection::Char(CharSelection {
                    anchor: CharPos::new(row, start),
                    head: CharPos::new(row, end),
                }));
                cx.notify();
            }
            self.result_pending_click = None;
            return;
        }
        // Single click: start a zero-width char selection so a drag can grow
        // it, but remember the click so mouse_up can fall back to a whole-row
        // jump+select when no drag happens.
        let anchor = CharPos::new(row, col);
        self.result_selection =
            Some(Selection::Char(CharSelection { anchor, head: anchor }));
        self.result_pending_click = Some(anchor);
        cx.notify();
    }

    fn handle_result_mouse_drag(&mut self, row: u64, col: usize, cx: &mut Context<Self>) {
        match &mut self.result_selection {
            Some(Selection::Char(sel)) => {
                sel.head = CharPos::new(row, col);
                self.result_pending_click = None;
                cx.notify();
            }
            Some(Selection::Line(sel)) => {
                sel.head = row;
                cx.notify();
            }
            None => {}
        }
    }

    fn handle_result_mouse_up(&mut self, cx: &mut Context<Self>) {
        // A click with no drag falls back to whole-row select AND preserves
        // the original click-to-jump behavior (scroll the main LogView to
        // this match).
        if let Some(anchor) = self.result_pending_click.take() {
            if let Some(Selection::Char(sel)) = &self.result_selection {
                if sel.is_empty() {
                    self.result_selection =
                        Some(Selection::Line(LineSelection::single(anchor.line)));
                    self.select_match(anchor.line as usize, cx);
                    cx.notify();
                }
            }
        }
    }

    fn handle_result_right_click(
        &mut self,
        row: usize,
        pos: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        // Preserve the existing selection; only remember the clicked row as a
        // fallback target for the menu's copy actions.
        self.result_right_click_row = Some(row);
        cx.emit(QuickFindEvent::ShowContextMenu(pos));
    }

    fn scroll_current_into_view(&self) {
        self.results_scroll
            .scroll_to_item(self.current_match, ScrollStrategy::Center);
    }

    /// Get all matching line numbers.
    pub fn matching_lines(&self) -> &[u64] {
        &self.matching_lines
    }

    /// Emit the current match as a JumpTo event for subscribers.
    fn emit_current_match(&mut self, cx: &mut Context<Self>) {
        if let Some(line) = self.current_match_line() {
            cx.emit(QuickFindEvent::JumpTo(line));
        }
    }

    /// Key handler for the focused find bar. Treats key events as text input:
    /// printable chars without cmd/ctrl/alt are appended to the query, and
    /// backspace removes the last char. Enter jumps to the next match. All
    /// other navigation (Esc to close, F3/Cmd+G for next/prev) is handled by
    /// the regular action bindings.
    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = &keystroke.modifiers;
        let key = keystroke.key.as_str();

        // Intercept Cmd/Ctrl + A / C / V inside the find bar so they edit the
        // query rather than firing the global log-view CopySelection action.
        // GPUI's `secondary-` modifier resolves to `platform` on macOS and
        // `control` on Linux/Windows — treat both as the editor modifier.
        let editor_mod =
            (modifiers.platform || modifiers.control) && !modifiers.alt && !modifiers.shift;
        if editor_mod {
            match key {
                "a" => {
                    if !self.query.is_empty() {
                        self.selection = Some((0, self.query.len()));
                        self.caret = self.query.len();
                        cx.notify();
                    }
                    cx.stop_propagation();
                    return;
                }
                "c" => {
                    let text = match self.selection {
                        Some((s, e)) if s < e => self.query[s..e].to_string(),
                        _ => self.query.clone(),
                    };
                    if !text.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                    cx.stop_propagation();
                    return;
                }
                "x" => {
                    if let Some((s, e)) = self.selection.filter(|(s, e)| s < e) {
                        let text = self.query[s..e].to_string();
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        self.query.replace_range(s..e, "");
                        self.selection = None;
                        self.caret = s;
                        self.caret_on = true;
                        self.execute_search(cx);
                        self.emit_current_match(cx);
                    }
                    cx.stop_propagation();
                    return;
                }
                "v" => {
                    if let Some(item) = cx.read_from_clipboard() {
                        if let Some(text) = item.text() {
                            // Drop control chars (incl. embedded newlines) so a
                            // multi-line paste doesn't break the single-line input.
                            let sanitized: String =
                                text.chars().filter(|c| !c.is_control()).collect();
                            if !sanitized.is_empty() {
                                self.replace_selection_with(&sanitized);
                                self.execute_search(cx);
                                self.emit_current_match(cx);
                            }
                        }
                    }
                    cx.stop_propagation();
                    return;
                }
                _ => {}
            }
        }

        // Let other modifier combos (e.g. Cmd+F, Cmd+G, F3) fall through to
        // their global action bindings.
        if modifiers.platform || modifiers.control || modifiers.alt {
            return;
        }

        match key {
            "backspace" => {
                if let Some((s, e)) = self.selection.filter(|(s, e)| s < e) {
                    self.query.replace_range(s..e, "");
                    self.selection = None;
                    self.caret = s;
                    self.caret_on = true;
                    self.execute_search(cx);
                    self.emit_current_match(cx);
                } else if self.caret > 0 {
                    let prev = self.prev_boundary(self.caret);
                    self.query.replace_range(prev..self.caret, "");
                    self.caret = prev;
                    self.caret_on = true;
                    self.execute_search(cx);
                    self.emit_current_match(cx);
                }
            }
            "left" => {
                self.selection = None;
                self.caret = self.prev_boundary(self.caret);
                self.caret_on = true;
                cx.notify();
            }
            "right" => {
                self.selection = None;
                self.caret = self.next_boundary(self.caret);
                self.caret_on = true;
                cx.notify();
            }
            "home" => {
                self.selection = None;
                self.caret = 0;
                self.caret_on = true;
                cx.notify();
            }
            "end" => {
                self.selection = None;
                self.caret = self.query.len();
                self.caret_on = true;
                cx.notify();
            }
            "enter" => {
                self.next_match(cx);
            }
            // "escape" intentionally does nothing: the quick-find window is
            // not closed via escape (that key is reserved for modal dialogs).
            // `CloseFind` has no default key — users can rebind it in Prefs.
            "escape" => {}
            _ => {
                if let Some(ch) = keystroke.key_char.as_deref() {
                    // Filter out control chars / empty strings.
                    if !ch.is_empty() && !ch.chars().any(|c| c.is_control()) {
                        self.replace_selection_with(ch);
                        self.execute_search(cx);
                        self.emit_current_match(cx);
                    }
                }
            }
        }
    }

    /// Replace the current selection (or insert at the caret) with `text`, then
    /// collapse the selection and place the caret just past the inserted text.
    fn replace_selection_with(&mut self, text: &str) {
        match self.selection {
            Some((s, e)) if s < e => {
                self.query.replace_range(s..e, text);
                self.caret = s + text.len();
            }
            _ => {
                self.query.insert_str(self.caret, text);
                self.caret += text.len();
            }
        }
        self.selection = None;
        self.caret_on = true;
    }

    /// Byte offset of the char boundary before `pos` (or `pos` if already 0).
    fn prev_boundary(&self, pos: usize) -> usize {
        if pos == 0 {
            return 0;
        }
        let mut p = pos - 1;
        while p > 0 && !self.query.is_char_boundary(p) {
            p -= 1;
        }
        p
    }

    /// Byte offset of the char boundary after `pos` (or `pos` if at the end).
    fn next_boundary(&self, pos: usize) -> usize {
        let len = self.query.len();
        if pos >= len {
            return len;
        }
        let mut p = pos + 1;
        while p < len && !self.query.is_char_boundary(p) {
            p += 1;
        }
        p
    }

    /// Render the find bar plus the results list underneath it. Returns
    /// `None` when hidden so callers can splice it into a `.children(...)`
    /// chain.
    pub fn render_bar(
        &mut self,
        theme: Theme,
        font: FontSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.visible {
            return None;
        }

        // Caret is painted only when the bar holds keyboard focus, the blink
        // phase is "on", and there's no active selection. Clicking elsewhere
        // blurs the focus handle (GPUI auto-transfers focus on mouse-down into
        // any other tracked element), which hides the caret per requirement 2.
        let show_caret =
            self.focus_handle.is_focused(window) && self.caret_on && self.selection.is_none();
        let caret_byte = self.caret;

        let no_match_color = Hsla { h: 0.0, s: 0.8, l: 0.5, a: 1.0 };
        let (match_text, count_color) = if self.regex_error.is_some() {
            ("Invalid regex".to_string(), no_match_color)
        } else if self.matching_lines.is_empty() && !self.query.is_empty() {
            ("No matches".to_string(), no_match_color)
        } else if !self.matching_lines.is_empty() {
            (
                format!("{} of {}", self.current_match + 1, self.matching_lines.len()),
                theme.line_number,
            )
        } else {
            (String::new(), theme.line_number)
        };

        // Build the input contents. Render each character of the query as its
        // own div so mouse_down/mouse_move can hit-test against byte offsets —
        // that's how drag-to-select stays accurate without a custom text
        // shaper. Selected characters get `theme.selection` as a background,
        // matching the previous three-span look.
        let input_box = div()
            .flex_grow()
            .h(px(22.0))
            .px_2()
            .bg(theme.background)
            .border_1()
            .border_color(theme.selection)
            .rounded_sm()
            .text_color(if self.query.is_empty() {
                theme.line_number
            } else {
                theme.foreground
            })
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    if this.drag_anchor.take().is_some() {
                        cx.notify();
                    }
                }),
            );

        // A thin vertical caret bar drawn at the insertion point. It's always
        // present in the layout (so the blink doesn't shift text); only its
        // color toggles between `theme.foreground` and transparent.
        let caret_color = if show_caret {
            theme.foreground
        } else {
            Hsla { h: 0.0, s: 0.0, l: 0.0, a: 0.0 }
        };
        let caret_el = move || {
            div()
                .flex_shrink_0()
                .w(px(1.0))
                .h(px(15.0))
                .bg(caret_color)
        };

        let input_box = if self.query.is_empty() {
            // Empty state: caret first (when focused) followed by the dimmed
            // placeholder, so the freshly-opened box looks ready for input.
            input_box.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .flex_grow()
                    .child(caret_el())
                    .child(div().child("Type to search…"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event: &MouseDownEvent, _window, cx| {
                            this.drag_anchor = Some(0);
                            this.selection = None;
                            this.caret = 0;
                            this.caret_on = true;
                            cx.notify();
                        }),
                    ),
            )
        } else {
            let sel = self.selection;
            let chars: Vec<(usize, usize, String)> = self
                .query
                .char_indices()
                .map(|(i, c)| (i, i + c.len_utf8(), c.to_string()))
                .collect();
            let end_pos = self.query.len();

            let mut spans: Vec<AnyElement> = Vec::new();
            for (cstart, cend, ch_str) in chars {
                // Caret sits immediately before the character it points at.
                if cstart == caret_byte {
                    spans.push(caret_el().into_any_element());
                }
                let in_sel = matches!(sel, Some((s, e)) if s <= cstart && e >= cend && s < e);
                let mut span = div()
                    .child(ch_str)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                            this.drag_anchor = Some(cstart);
                            this.selection = None;
                            this.caret = cstart;
                            this.caret_on = true;
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(cx.listener(
                        move |this, event: &MouseMoveEvent, _window, cx| {
                            if event.pressed_button != Some(MouseButton::Left) {
                                if this.drag_anchor.take().is_some() {
                                    cx.notify();
                                }
                                return;
                            }
                            if let Some(anchor) = this.drag_anchor {
                                let s = anchor.min(cend);
                                let e = anchor.max(cend);
                                let new_sel = if s < e { Some((s, e)) } else { None };
                                this.caret = cend;
                                if this.selection != new_sel {
                                    this.selection = new_sel;
                                    cx.notify();
                                }
                            }
                        },
                    ));
                if in_sel {
                    span = span.bg(theme.selection);
                }
                spans.push(span.into_any_element());
            }

            // Caret at end-of-query renders inside the trailing zone below.
            // Trailing zone fills the remainder of the input box so clicks
            // past the last character anchor at end-of-query.
            spans.push(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .flex_grow()
                    .when(caret_byte == end_pos, |d| d.child(caret_el()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                            this.drag_anchor = Some(end_pos);
                            this.selection = None;
                            this.caret = end_pos;
                            this.caret_on = true;
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(cx.listener(
                        move |this, event: &MouseMoveEvent, _window, cx| {
                            if event.pressed_button != Some(MouseButton::Left) {
                                if this.drag_anchor.take().is_some() {
                                    cx.notify();
                                }
                                return;
                            }
                            if let Some(anchor) = this.drag_anchor {
                                let s = anchor.min(end_pos);
                                let e = anchor.max(end_pos);
                                let new_sel = if s < e { Some((s, e)) } else { None };
                                this.caret = end_pos;
                                if this.selection != new_sel {
                                    this.selection = new_sel;
                                    cx.notify();
                                }
                            }
                        },
                    ))
                    .into_any_element(),
            );

            input_box.child(div().flex().flex_row().items_center().children(spans))
        };

        let regex_active = self.regex_mode;
        let regex_toggle = div()
            .id("quick_find_regex_toggle")
            .flex_shrink_0()
            .h(px(22.0))
            .px_2()
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(theme.selection)
            .rounded_sm()
            .bg(if regex_active {
                theme.selection
            } else {
                theme.background
            })
            .text_color(if regex_active {
                theme.foreground
            } else {
                theme.line_number
            })
            .cursor_pointer()
            .child(".*")
            .on_click(cx.listener(|this, _event, _window, cx| {
                this.toggle_regex_mode(cx);
            }));

        let bar_row = div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(32.0))
            .px_3()
            .bg(theme.current_line)
            .border_t_1()
            .border_color(theme.selection)
            .child(div().text_color(theme.line_number).child("Find:"))
            .child(input_box)
            .child(regex_toggle)
            .child(div().text_color(count_color).child(match_text));

        let results_panel = self.build_results_panel(theme, font, window, cx);

        // Drag handle sits on top of the bar so dragging UP grows the results
        // panel (which lives below the bar). The panel is always present while
        // the bar is open, so the handle is too.
        //
        // The hit zone is 12px tall (3x the visible thumb) so it's easy to
        // grab without growing the chrome much; the centered 2px stripe is
        // the visual cue. `cursor(ResizeUpDown)` confirms interactivity on
        // hover.
        let drag_handle = {
            Some(
                div()
                    .h(px(12.0))
                    .w_full()
                    .flex()
                    .items_center()
                    .cursor(CursorStyle::ResizeUpDown)
                    .child(div().h(px(2.0)).w_full().bg(theme.selection))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                            this.resize_anchor = Some((event.position.y, this.panel_height));
                            cx.notify();
                        }),
                    ),
            )
        };

        // Full-window overlay only while a drag is in progress: catches
        // mouse_move / mouse_up no matter where the cursor wanders. Without
        // it, a fast upward sweep leaves the wrapper and silently drops the
        // drag.
        let drag_overlay = self.resize_anchor.is_some().then(|| {
            div()
                .absolute()
                .inset_0()
                .cursor(CursorStyle::ResizeUpDown)
                .on_mouse_move(cx.listener(Self::on_resize_move))
                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_resize_up))
        });

        Some(
            div()
                .key_context("QuickFind")
                .track_focus(&self.focus_handle)
                .on_key_down(cx.listener(Self::on_key_down))
                // In-wrapper listeners handle the normal case where the
                // cursor stays within the find component during drag.
                .on_mouse_move(cx.listener(Self::on_resize_move))
                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_resize_up))
                .flex()
                .flex_col()
                .children(drag_handle)
                .child(bar_row)
                .children(results_panel)
                .children(drag_overlay)
                .into_any(),
        )
    }

    fn on_resize_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.pressed_button != Some(MouseButton::Left) {
            if self.resize_anchor.take().is_some() {
                cx.notify();
            }
            return;
        }
        if let Some((anchor_y, anchor_h)) = self.resize_anchor {
            let delta = anchor_y - event.position.y;
            let new_h = (anchor_h + delta).max(px(PANEL_MIN_PX));
            if self.panel_height != new_h {
                self.panel_height = new_h;
                cx.notify();
            }
        }
    }

    fn on_resize_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.resize_anchor.take().is_some() {
            cx.notify();
        }
    }

    /// Build the matching-line list shown beneath the search input. Mirrors
    /// the LogView row layout (gutter + content) so a search hit reads the
    /// same as the file content view, with the query highlighted inline and
    /// the current match's row tinted like a LogView selection.
    fn build_results_panel(
        &mut self,
        theme: Theme,
        font: FontSettings,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // The panel is always shown while the bar is open. With no matches yet
        // (empty query, a still-loading file, or a query with zero hits) render
        // an empty panel carrying the same chrome so the results window is
        // visibly "open" the moment a file loads.
        if self.matching_lines.is_empty() {
            let message = if self.query.is_empty() {
                "Type to search…"
            } else if self.regex_error.is_some() {
                "Invalid regex"
            } else {
                "No matches"
            };
            return Some(
                div()
                    .h(self.panel_height)
                    .w_full()
                    .bg(theme.background)
                    .border_t_1()
                    .border_color(theme.selection)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.line_number)
                    .child(message)
                    .into_any(),
            );
        }
        let data = self.log_data.clone()?;

        let matching = self.matching_lines.clone();
        let line_matches = self.line_matches.clone();
        let current = self.current_match;
        let selection = self.result_selection;
        let line_height = font.line_height();
        let font_size = font.size;
        let font_family = font.family.clone();
        // Size the gutter for the largest line number in the result set so
        // every row stays aligned even on huge files. Use the live font
        // advance (0.6 × font size, matching log_view::char_advance_for) so
        // the gutter scales with the user's font setting.
        let max_line = *matching.last().unwrap_or(&0);
        let gutter_digits = digits_for(max_line + 1);
        let char_advance = char_advance_for(font_size);
        let gutter_width = gutter_digits as f32 * char_advance + 16.0 + 1.0;
        // Horizontal scroll, sized from the source file's longest line (a byte
        // count, slightly over-estimating multi-byte UTF-8 width). The content
        // of each row is shifted left by `h_offset`; the gutter stays fixed.
        let h_offset = self.h_scroll.offset();
        let content_w = data.max_line_length() as f32 * char_advance;
        let entity = cx.entity();
        let total = matching.len();
        // User-resizable height (see `resize_anchor` and the drag handle in
        // `render_bar`). Falls back to a default if the user hasn't dragged.
        let panel_height = self.panel_height;

        let list = uniform_list(
            "quick_find_results",
            total,
            move |range, _window, _cx| {
                range
                    .map(|i| {
                        let line_num = matching[i];
                        let bytes = data.get_line(line_num).unwrap_or_default();
                        let line_text = String::from_utf8_lossy(&bytes).to_string();
                        let ranges = line_matches.get(i).cloned().unwrap_or_default();
                        let is_current = i == current;
                        let row = i as u64;
                        let line_char_len = line_text.chars().count();
                        let highlight = selection
                            .map(|s| s.highlight_for(row, line_char_len))
                            .unwrap_or(LineHighlight::None);

                        // A full-row selection (or the current-match row) tints
                        // the whole row like a LogView selection. A character
                        // range must NOT tint the whole row — otherwise the
                        // selected span (painted in `theme.selection` by
                        // `render_line_content`) is invisible against an
                        // identically-colored background, which happens after a
                        // double-click since the preceding click made this the
                        // current match.
                        let row_bg = match highlight {
                            LineHighlight::Full => theme.selection,
                            LineHighlight::Range(..) => theme.background,
                            LineHighlight::None if is_current => theme.selection,
                            LineHighlight::None => theme.background,
                        };

                        let down_entity = entity.clone();
                        let move_entity = entity.clone();
                        let up_entity = entity.clone();
                        let right_entity = entity.clone();
                        let down_text = line_text.clone();
                        let move_text = line_text.clone();

                        // When the selection touches this row, show the
                        // selection highlight (consistent with the main view);
                        // otherwise keep the inline query-match highlighting.
                        let content: AnyElement = match highlight {
                            LineHighlight::None => {
                                render_result_line(&line_text, &ranges, theme)
                            }
                            _ => render_line_content(&line_text, highlight, theme),
                        };

                        div()
                            .flex()
                            .items_start()
                            .h(line_height)
                            .bg(row_bg)
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
                                    this.handle_result_mouse_down(
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
                                    this.handle_result_mouse_drag(row, col, cx);
                                });
                            })
                            .on_mouse_up(MouseButton::Left, move |_event, _window, cx| {
                                up_entity.update(cx, |this, cx| {
                                    this.handle_result_mouse_up(cx);
                                });
                            })
                            .on_mouse_down(MouseButton::Right, move |event, _window, cx| {
                                let pos = event.position;
                                right_entity.update(cx, |this, cx| {
                                    this.handle_result_right_click(i, pos, cx);
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
                                // horizontal offset; the gutter stays put. The
                                // inner div is sized to the full content width
                                // so it doesn't collapse under flex.
                                div()
                                    .flex_grow()
                                    .overflow_hidden()
                                    .child(
                                        div()
                                            .w(px(content_w))
                                            .ml(px(-h_offset))
                                            .whitespace_nowrap()
                                            .child(content),
                                    ),
                            )
                            .into_any()
                    })
                    .collect()
            },
        )
        .track_scroll(self.results_scroll.clone())
        .flex_grow()
        .w_full();

        let entity_wheel = cx.entity();
        let v_scrollbar = v_scrollbar::render(
            &mut self.v_scroll,
            |this: &mut QuickFindState| &mut this.v_scroll,
            &self.results_scroll,
            total as u64,
            f32::from(line_height),
            theme,
            cx,
        );
        let scrollbar = h_scrollbar::render(
            &mut self.h_scroll,
            |this: &mut QuickFindState| &mut this.h_scroll,
            content_w,
            gutter_width,
            theme,
            cx,
        );

        Some(
            div()
                .h(panel_height)
                .w_full()
                .bg(theme.background)
                .border_t_1()
                .border_color(theme.selection)
                .flex()
                .flex_col()
                .child(
                    // List + vertical scrollbar share a row; the horizontal bar
                    // sits below them spanning the panel's width.
                    div()
                        .flex()
                        .flex_row()
                        .flex_grow()
                        .overflow_hidden()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_grow()
                                .overflow_hidden()
                                .child(list)
                                // Shift+wheel / horizontal trackpad
                                // gesture scrolls the content sideways.
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
                        .child(v_scrollbar),
                )
                .child(scrollbar)
                .into_any(),
        )
    }
}

/// Number of decimal digits needed to render `n` right-aligned. Mirrors the
/// helper in `log_view` so result-row gutters line up with file-content rows.
fn digits_for(n: u64) -> usize {
    if n == 0 {
        return 1;
    }
    (n as f64).log10().floor() as usize + 1
}

/// Render a search-result line: split the text on each precomputed range
/// and paint those spans with `theme.search_match`. Ranges are byte offsets
/// into the line content (without any trailing newline), sorted ascending.
/// Out-of-bounds or non-UTF-8-boundary ranges are skipped so we never slice
/// across a code point.
fn render_result_line(text: &str, ranges: &[(usize, usize)], theme: Theme) -> AnyElement {
    let no_nl = text.strip_suffix('\n').unwrap_or(text);

    if ranges.is_empty() {
        return div()
            .flex_grow()
            .text_color(theme.foreground)
            .child(no_nl.to_string())
            .into_any_element();
    }

    let mut spans: Vec<AnyElement> = Vec::new();
    let mut last = 0usize;
    for &(start, end) in ranges {
        if start < last || end > no_nl.len() || start >= end {
            continue;
        }
        if !no_nl.is_char_boundary(start) || !no_nl.is_char_boundary(end) {
            continue;
        }
        if start > last {
            spans.push(
                div()
                    .child(no_nl[last..start].to_string())
                    .into_any_element(),
            );
        }
        spans.push(
            div()
                .bg(theme.search_match)
                .child(no_nl[start..end].to_string())
                .into_any_element(),
        );
        last = end;
    }
    if last < no_nl.len() {
        spans.push(
            div()
                .child(no_nl[last..].to_string())
                .into_any_element(),
        );
    }

    div()
        .flex_grow()
        .flex()
        .flex_row()
        .text_color(theme.foreground)
        .children(spans)
        .into_any_element()
}

/// Strip a single trailing `\n` (and a preceding `\r` if present) from a
/// raw line, returning the visible content bytes.
fn strip_trailing_nl(raw: &[u8]) -> &[u8] {
    let mut end = raw.len();
    if end > 0 && raw[end - 1] == b'\n' {
        end -= 1;
        if end > 0 && raw[end - 1] == b'\r' {
            end -= 1;
        }
    }
    &raw[..end]
}

impl Focusable for QuickFindState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<QuickFindEvent> for QuickFindState {}
