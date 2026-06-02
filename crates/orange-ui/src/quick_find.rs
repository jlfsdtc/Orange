//! Quick find bar for incremental search within the log view.
//!
//! When visible, the bar owns keyboard focus: every character keystroke that
//! doesn't carry a primary modifier (cmd/ctrl) is appended to the query, and a
//! background literal search runs against the active log file. On match
//! navigation (Enter, F3, Cmd+G), it emits `QuickFindEvent::JumpTo(line)` so
//! the parent view can scroll/select that line in the active LogView.

use gpui::*;
use orange_core::LogData;
use orange_regex::{RegexEngine, RegexFlags};
use std::sync::Arc;

use crate::h_scrollbar::{self, HScrollState};
use crate::overview::{MinimapEvent, OverviewState};
use crate::theme::{FontSettings, Theme};

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
}

/// Quick find bar state.
pub struct QuickFindState {
    /// Whether the find bar is visible.
    visible: bool,
    /// Current search query.
    query: String,
    /// Optional selection over `query` as (start_byte, end_byte) with start <= end.
    /// `None` means no selection (caret at end of query).
    selection: Option<(usize, usize)>,
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
    /// Whole-file minimap shown to the right of the results list. Its line
    /// counts (matches) come from `matching_lines`; its viewport reflects
    /// which results are currently visible in the panel.
    minimap: Entity<OverviewState>,
    /// Horizontal scroll state for the results list. Sized from the source
    /// file's longest line so the bar is stable regardless of which matches
    /// are shown; the gutter stays fixed while line content scrolls sideways.
    h_scroll: HScrollState,
}

const PANEL_MIN_PX: f32 = 60.0;
const PANEL_DEFAULT_PX: f32 = 240.0;

impl QuickFindState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe_global::<FontSettings>(|_, cx| cx.notify()).detach();
        let minimap = cx.new(OverviewState::new);
        // A click on the minimap maps a file-line back to the closest
        // result row and scrolls/selects it.
        cx.subscribe(&minimap, |this, _, event, cx| match event {
            MinimapEvent::ScrollTo(line) => {
                if this.matching_lines.is_empty() {
                    return;
                }
                let line = *line;
                let idx = this.matching_lines.partition_point(|&l| l < line);
                let idx = idx.min(this.matching_lines.len() - 1);
                this.select_match(idx, cx);
            }
        })
        .detach();
        Self {
            visible: false,
            query: String::new(),
            selection: None,
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
            minimap,
            h_scroll: HScrollState::new(),
        }
    }

    /// Toggle the QuickFind minimap visibility. Called from
    /// `MainWindowState::toggle_minimap` so the strip on both sides of the
    /// app stays in sync.
    pub fn toggle_minimap(&mut self, cx: &mut Context<Self>) {
        self.minimap.update(cx, |m, cx| m.toggle(cx));
        cx.notify();
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
            if !self.query.is_empty() {
                self.execute_search(cx);
                self.emit_current_match(cx);
            }
        } else {
            self.query.clear();
            self.selection = None;
            self.matching_lines.clear();
            self.line_matches.clear();
            self.current_match = 0;
            self.regex_error = None;
            self.drag_anchor = None;
            self.resize_anchor = None;
        }
        cx.notify();
    }

    /// Close the find bar.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.visible = false;
        self.query.clear();
        self.selection = None;
        self.matching_lines.clear();
        self.line_matches.clear();
        self.current_match = 0;
        self.regex_error = None;
        self.drag_anchor = None;
        self.resize_anchor = None;
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
                    self.execute_search(cx);
                    self.emit_current_match(cx);
                } else if self.query.pop().is_some() {
                    self.execute_search(cx);
                    self.emit_current_match(cx);
                }
            }
            "enter" => {
                self.next_match(cx);
            }
            // "escape" is bound to CloseFind globally; let it bubble.
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

    /// Replace the current selection (or append at end) with `text`, then
    /// clear the selection.
    fn replace_selection_with(&mut self, text: &str) {
        match self.selection {
            Some((s, e)) if s < e => {
                self.query.replace_range(s..e, text);
            }
            _ => {
                self.query.push_str(text);
            }
        }
        self.selection = None;
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
        let _ = window;

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

        let input_box = if self.query.is_empty() {
            input_box.child(
                div()
                    .flex_grow()
                    .child("Type to search…")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event: &MouseDownEvent, _window, cx| {
                            this.drag_anchor = Some(0);
                            this.selection = None;
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
                let in_sel = matches!(sel, Some((s, e)) if s <= cstart && e >= cend && s < e);
                let mut span = div()
                    .child(ch_str)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                            this.drag_anchor = Some(cstart);
                            this.selection = None;
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

            // Trailing zone fills the remainder of the input box so clicks
            // past the last character anchor at end-of-query.
            spans.push(
                div()
                    .flex_grow()
                    .child("\u{2502}")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                            this.drag_anchor = Some(end_pos);
                            this.selection = None;
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
                                if this.selection != new_sel {
                                    this.selection = new_sel;
                                    cx.notify();
                                }
                            }
                        },
                    ))
                    .into_any_element(),
            );

            input_box.child(div().flex().flex_row().children(spans))
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
        // panel (which lives below the bar). Only useful when results exist.
        //
        // The hit zone is 12px tall (3x the visible thumb) so it's easy to
        // grab without growing the chrome much; the centered 2px stripe is
        // the visual cue. `cursor(ResizeUpDown)` confirms interactivity on
        // hover.
        let has_results = !self.matching_lines.is_empty();
        let drag_handle = if has_results {
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
        } else {
            None
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.matching_lines.is_empty() {
            return None;
        }
        let data = self.log_data.clone()?;

        let matching = self.matching_lines.clone();
        let line_matches = self.line_matches.clone();
        let current = self.current_match;
        let line_height = font.line_height();
        let font_size = font.size;
        let font_family = font.family.clone();
        // Size the gutter for the largest line number in the result set so
        // every row stays aligned even on huge files. Use the live font
        // advance (0.6 × font size, matching log_view::char_advance_for) so
        // the gutter scales with the user's font setting.
        let max_line = *matching.last().unwrap_or(&0);
        let gutter_digits = digits_for(max_line + 1);
        let char_advance = f32::from(font_size) * 0.6;
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

        // Push whole-file match density / viewport into the minimap before
        // we render. Viewport spans the file lines covered by the result
        // rows currently visible in the panel.
        let total_file_lines = data.line_count();
        let line_height_px = f32::from(line_height);
        // Mirror the trick used in log_view::viewport_lines — reach into
        // the underlying `ScrollHandle` because `logical_scroll_top_index`
        // is `cfg(test-support)`-only.
        let first_visible_result = {
            let state = self.results_scroll.0.borrow();
            let off_y = f32::from(state.base_handle.offset().y);
            if line_height_px > 0.0 {
                (((-off_y) / line_height_px).max(0.0)) as usize
            } else {
                0
            }
        };
        let visible_results = if line_height_px > 0.0 {
            (f32::from(self.panel_height) / line_height_px).ceil() as usize
        } else {
            0
        };
        let vp_first_line = matching
            .get(first_visible_result.min(total.saturating_sub(1)))
            .copied()
            .unwrap_or(0);
        let vp_last_line = matching
            .get((first_visible_result + visible_results).min(total.saturating_sub(1)))
            .copied()
            .unwrap_or(vp_first_line);
        let vp_size = vp_last_line.saturating_sub(vp_first_line).max(1);
        let matches_for_minimap = matching.clone();
        let minimap_window_handle = self.minimap.clone();
        let minimap_el = minimap_window_handle.update(cx, |m, cx| {
            m.set_total_lines(total_file_lines, cx);
            m.set_matches(matches_for_minimap, cx);
            m.set_viewport(vp_first_line, vp_size, cx);
            m.render(theme, window, cx)
        });

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
                        let row_bg = if is_current {
                            theme.selection
                        } else {
                            theme.background
                        };

                        let click_entity = entity.clone();

                        div()
                            .flex()
                            .items_start()
                            .h(line_height)
                            .bg(row_bg)
                            .w_full()
                            .text_size(font_size)
                            .font_family(font_family.clone())
                            .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                                click_entity.update(cx, |this, cx| {
                                    this.select_match(i, cx);
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
                                            .child(render_result_line(
                                                &line_text, &ranges, theme,
                                            )),
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
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .size_full()
                        // List column: results stacked above the horizontal
                        // scrollbar; the minimap sits to the right.
                        .child(
                            div()
                                .flex()
                                .flex_col()
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
                                .child(scrollbar),
                        )
                        .child(minimap_el),
                )
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
