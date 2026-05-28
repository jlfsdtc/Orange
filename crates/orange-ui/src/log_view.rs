//! Log view: the core component for displaying log file content.
//!
//! Uses GPUI's uniform_list for virtual scrolling over potentially
//! billions of lines without loading them all into memory.

use gpui::*;
use orange_core::LogData;
use std::sync::Arc;

use crate::theme::{FontSettings, Theme};

// Actions for log view.
actions!(orange, [ToggleTailMode]);

/// Events emitted by the log view to the surrounding window.
pub enum LogViewEvent {
    /// User right-clicked at this window-relative position; show the menu.
    ShowContextMenu(Point<Pixels>),
}

/// An inclusive line-range selection. `anchor` is fixed at mouse-down (or at
/// the line that started a keyboard-driven selection); `head` follows the
/// cursor while dragging and is the line returned by `selected_line()`.
#[derive(Clone, Copy, Debug)]
pub struct LineSelection {
    pub anchor: u64,
    pub head: u64,
}

impl LineSelection {
    fn single(line: u64) -> Self {
        Self { anchor: line, head: line }
    }

    fn lo(&self) -> u64 {
        self.anchor.min(self.head)
    }

    fn hi(&self) -> u64 {
        self.anchor.max(self.head)
    }
}

/// A character position inside the log. `col` is a **char index** (not a
/// byte offset) so the public API stays UTF-8 safe; we convert to byte
/// offsets only when slicing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CharPos {
    pub line: u64,
    pub col: usize,
}

impl CharPos {
    fn new(line: u64, col: usize) -> Self {
        Self { line, col }
    }
}

/// A character-range selection (double-click word, or left-drag).
#[derive(Clone, Copy, Debug)]
pub struct CharSelection {
    pub anchor: CharPos,
    pub head: CharPos,
}

impl CharSelection {
    fn ordered(&self) -> (CharPos, CharPos) {
        let (a, h) = (self.anchor, self.head);
        if (a.line, a.col) <= (h.line, h.col) { (a, h) } else { (h, a) }
    }

    fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

/// The active selection — either a whole-line range or an arbitrary
/// character range.
#[derive(Clone, Copy, Debug)]
pub enum Selection {
    Line(LineSelection),
    Char(CharSelection),
}

impl Selection {
    fn lo_line(&self) -> u64 {
        match self {
            Selection::Line(s) => s.lo(),
            Selection::Char(s) => s.ordered().0.line,
        }
    }

    fn hi_line(&self) -> u64 {
        match self {
            Selection::Line(s) => s.hi(),
            Selection::Char(s) => s.ordered().1.line,
        }
    }

    fn head_line(&self) -> u64 {
        match self {
            Selection::Line(s) => s.head,
            Selection::Char(s) => s.head.line,
        }
    }

    fn highlight_for(&self, line_num: u64, line_char_len: usize) -> LineHighlight {
        match self {
            Selection::Line(s) => {
                if line_num >= s.lo() && line_num <= s.hi() {
                    LineHighlight::Full
                } else {
                    LineHighlight::None
                }
            }
            Selection::Char(s) => {
                let (lo, hi) = s.ordered();
                if line_num < lo.line || line_num > hi.line {
                    LineHighlight::None
                } else if lo.line == hi.line {
                    LineHighlight::Range(lo.col, hi.col)
                } else if line_num == lo.line {
                    LineHighlight::Range(lo.col, line_char_len)
                } else if line_num == hi.line {
                    LineHighlight::Range(0, hi.col)
                } else {
                    LineHighlight::Range(0, line_char_len)
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum LineHighlight {
    None,
    Full,
    Range(usize, usize),
}

/// State for the log view component.
pub struct LogViewState {
    log_data: Option<Arc<LogData>>,
    total_lines: u64,
    gutter_digits: usize,
    selection: Option<Selection>,
    scroll_handle: UniformListScrollHandle,
    tail_mode: bool,
    /// Mouse-down anchor recorded on a non-shift single click. If the user
    /// releases without dragging, the selection falls back to a whole-line
    /// selection so today's "click a line, see it highlighted" behavior is
    /// preserved. Cleared as soon as a drag is detected.
    pending_click: Option<CharPos>,
    /// Line under the most recent right-click. Acts as a fallback target for
    /// the context-menu copy actions when there is no active selection, so
    /// right-clicking on an unselected row copies that row without disturbing
    /// any prior selection the user had.
    right_click_line: Option<u64>,
}

impl LogViewState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe_global::<FontSettings>(|_, cx| cx.notify()).detach();
        Self {
            log_data: None,
            total_lines: 0,
            gutter_digits: 6,
            selection: None,
            scroll_handle: UniformListScrollHandle::default(),
            tail_mode: false,
            pending_click: None,
            right_click_line: None,
        }
    }

    pub fn load_file(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        match LogData::open(path) {
            Ok(data) => {
                self.total_lines = data.line_count();
                self.gutter_digits = digits_for(self.total_lines);
                self.log_data = Some(Arc::new(data));
                self.selection = None;
                cx.notify();
            }
            Err(e) => {
                tracing::error!("Failed to open file: {}", e);
            }
        }
    }

    pub fn scroll_to_line(&mut self, line: u64, cx: &mut Context<Self>) {
        let line = line.min(self.total_lines.saturating_sub(1));
        self.scroll_handle
            .scroll_to_item(line as usize, ScrollStrategy::Top);
        cx.notify();
    }

    /// Replace any existing selection with a single-line selection at `line`
    /// (or clear it when `None`).
    pub fn select_line(&mut self, line: Option<u64>, cx: &mut Context<Self>) {
        self.selection = line.map(|l| Selection::Line(LineSelection::single(l)));
        self.pending_click = None;
        cx.notify();
    }

    /// Keep the existing anchor and move the head to `line`. For a char
    /// selection this jumps the head to the start of `line`.
    pub fn extend_selection_to(&mut self, line: u64, cx: &mut Context<Self>) {
        match &mut self.selection {
            Some(Selection::Line(sel)) => sel.head = line,
            Some(Selection::Char(sel)) => sel.head = CharPos::new(line, 0),
            None => {
                self.selection = Some(Selection::Line(LineSelection::single(line)));
            }
        }
        cx.notify();
    }

    pub fn selected_line(&self) -> Option<u64> {
        self.selection.as_ref().map(|s| s.head_line())
    }

    pub fn selection_range(&self) -> Option<(u64, u64)> {
        self.selection.as_ref().map(|s| (s.lo_line(), s.hi_line()))
    }

    pub fn selection_line_count(&self) -> u64 {
        self.selection_range()
            .map(|(lo, hi)| hi - lo + 1)
            .unwrap_or(0)
    }

    /// Concatenated text of the current selection, joined by `\n`. For line
    /// selections this is every whole line; for character selections the
    /// first/last lines are sliced at the selection endpoints. When there is
    /// no selection but a right-click row is recorded, returns that line.
    /// Returns `None` if nothing is selected or the selection is an empty
    /// char range and no right-click row is set.
    pub fn selection_text(&self) -> Option<String> {
        let data = self.log_data.as_ref()?;
        let Some(sel) = self.selection.as_ref() else {
            let line = self.right_click_line?;
            let lines = data.get_lines(line, 1);
            return Some(join_full_lines(&lines));
        };
        if let Selection::Char(s) = sel {
            if s.is_empty() {
                let line = self.right_click_line?;
                let lines = data.get_lines(line, 1);
                return Some(join_full_lines(&lines));
            }
        }
        let lo_line = sel.lo_line();
        let hi_line = sel.hi_line();
        let count = (hi_line - lo_line + 1) as usize;
        let lines = data.get_lines(lo_line, count);
        match sel {
            Selection::Line(_) => Some(join_full_lines(&lines)),
            Selection::Char(c) => {
                let (lo, hi) = c.ordered();
                Some(join_char_range(&lines, lo.col, hi.col, lo_line, hi_line))
            }
        }
    }

    /// Full-line text covering the current selection. Identical to
    /// `selection_text()` for line selections; for character selections it
    /// returns the entire content of every line the selection touches —
    /// what users mean by "the selected lines". Falls back to the
    /// right-click row when no selection is active.
    pub fn selected_lines_text(&self) -> Option<String> {
        let data = self.log_data.as_ref()?;
        let (lo_line, hi_line) = match self.selection.as_ref() {
            Some(sel) => (sel.lo_line(), sel.hi_line()),
            None => {
                let line = self.right_click_line?;
                (line, line)
            }
        };
        let count = (hi_line - lo_line + 1) as usize;
        let lines = data.get_lines(lo_line, count);
        Some(join_full_lines(&lines))
    }

    pub fn total_lines(&self) -> u64 {
        self.total_lines
    }

    pub fn has_file(&self) -> bool {
        self.log_data.is_some()
    }

    pub fn file_name(&self) -> Option<String> {
        self.log_data.as_ref().and_then(|d| {
            d.path()
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
    }

    pub fn is_tail_mode(&self) -> bool {
        self.tail_mode
    }

    pub fn log_data(&self) -> Option<Arc<LogData>> {
        self.log_data.clone()
    }

    /// Read-only access to the scroll handle, used by the minimap to
    /// compute the current viewport range.
    pub fn scroll_handle(&self) -> &UniformListScrollHandle {
        &self.scroll_handle
    }

    /// Bookmarked line numbers. Returns an empty slice for now — bookmarks
    /// aren't persisted yet, but the minimap reads through this method so
    /// the rest of the wiring stays unchanged once they land.
    pub fn bookmarks(&self) -> &[u64] {
        &[]
    }

    /// Approximate `(first_visible_line, visible_count)` for the current
    /// scroll position. Used by the minimap to draw the viewport rectangle.
    /// `window_height_px` is the height of the LogView's list area.
    pub fn viewport_lines(&self, font: &FontSettings, window_height_px: f32) -> (u64, u64) {
        let lh = f32::from(font.line_height());
        // Reach inside the public `UniformListScrollHandle` to read the
        // inner `ScrollHandle` offset. `logical_scroll_top_index()` is
        // available only with `test-support`, so we replicate its math:
        // top item = `-offset.y / item_height`. Negative offset means the
        // user has scrolled down.
        let state = self.scroll_handle.0.borrow();
        let offset_y = f32::from(state.base_handle.offset().y);
        drop(state);
        let first = if lh > 0.0 {
            ((-offset_y) / lh).max(0.0) as u64
        } else {
            0
        };
        let visible = if lh > 0.0 {
            (window_height_px / lh).ceil() as u64
        } else {
            0
        };
        (first.min(self.total_lines), visible.max(1))
    }

    pub fn toggle_tail_mode(&mut self, cx: &mut Context<Self>) {
        self.tail_mode = !self.tail_mode;
        if self.tail_mode {
            self.start_tail_poll(cx);
        }
        cx.notify();
    }

    pub fn check_and_refresh(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self
            .log_data
            .as_ref()
            .map(|d| d.has_changed())
            .unwrap_or(false);
        if changed {
            self.refresh(cx);
            return true;
        }
        false
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(data) = &self.log_data {
            match LogData::open(data.path()) {
                Ok(new_data) => {
                    self.total_lines = new_data.line_count();
                    self.gutter_digits = digits_for(self.total_lines);
                    self.log_data = Some(Arc::new(new_data));
                    if self.tail_mode && self.total_lines > 0 {
                        self.scroll_handle.scroll_to_item(
                            self.total_lines as usize - 1,
                            ScrollStrategy::Bottom,
                        );
                    }
                    cx.notify();
                }
                Err(e) => {
                    tracing::error!("Failed to refresh file: {}", e);
                }
            }
        }
    }

    fn start_tail_poll(&self, cx: &mut Context<Self>) {
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            loop {
                executor.timer(std::time::Duration::from_millis(500)).await;
                let should_continue = cx
                    .update(|cx| {
                        this.update(cx, |view, cx| {
                            if !view.tail_mode {
                                return false;
                            }
                            view.check_and_refresh(cx);
                            true
                        })
                        .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        })
        .detach();
    }
}

impl EventEmitter<LogViewEvent> for LogViewState {}

impl Render for LogViewState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.global::<Theme>();
        let font = cx.global::<FontSettings>().clone();
        let total = self.total_lines as usize;

        if total == 0 {
            return div()
                .size_full()
                .bg(theme.background)
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.line_number)
                .child(if cfg!(target_os = "macos") {
                    "No file loaded. Press Cmd+O to open a file."
                } else {
                    "No file loaded. Press Ctrl+O to open a file."
                })
                .into_any();
        }

        let gutter_digits = self.gutter_digits;
        let line_height = font.line_height();
        let font_size = font.size;
        let font_family = font.family.clone();
        let selection = self.selection;
        let log_data = self.log_data.clone();
        let char_advance = char_advance_for(font_size);
        // Match the actual font advance so the largest line number always
        // fits; 1.0 trailing px guards against fractional rounding clipping
        // the right edge of the last digit.
        let gutter_width = gutter_digits as f32 * char_advance + 16.0 + 1.0;
        let entity = cx.entity();

        uniform_list(
            "log_lines",
            total,
            move |range, _window, _cx| {
                range
                    .map(|i| {
                        let line_num = i as u64;
                        let line_text = log_data
                            .as_ref()
                            .and_then(|d| d.get_line(line_num))
                            .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
                            .unwrap_or_default();
                        let line_char_len = line_text.chars().count();
                        let highlight = selection
                            .map(|s| s.highlight_for(line_num, line_char_len))
                            .unwrap_or(LineHighlight::None);

                        let row_bg = match highlight {
                            LineHighlight::Full => theme.selection,
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
                                    down_text.chars().count(),
                                );
                                down_entity.update(cx, |this, cx| {
                                    this.handle_mouse_down(
                                        line_num,
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
                                    move_text.chars().count(),
                                );
                                move_entity.update(cx, |this, cx| {
                                    this.handle_mouse_drag(line_num, col, cx);
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
                                    this.handle_right_click(line_num, pos, cx);
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
                            .child(render_line_content(&line_text, highlight, theme))
                            .into_any()
                    })
                    .collect()
            },
        )
        .track_scroll(self.scroll_handle.clone())
        .size_full()
        .into_any()
    }
}

impl LogViewState {
    fn handle_mouse_down(
        &mut self,
        line: u64,
        col: usize,
        shift: bool,
        click_count: usize,
        line_text: &str,
        cx: &mut Context<Self>,
    ) {
        if shift {
            self.extend_selection_to(line, cx);
            self.pending_click = None;
            return;
        }
        if click_count >= 2 {
            if let Some((start, end)) = word_range_at(line_text, col) {
                self.selection = Some(Selection::Char(CharSelection {
                    anchor: CharPos::new(line, start),
                    head: CharPos::new(line, end),
                }));
                cx.notify();
            }
            self.pending_click = None;
            return;
        }
        // Single click: start a zero-width char selection so a drag can grow
        // it, but remember the click so mouse_up can fall back to whole-line
        // select when no drag happens.
        let anchor = CharPos::new(line, col);
        self.selection = Some(Selection::Char(CharSelection { anchor, head: anchor }));
        self.pending_click = Some(anchor);
        cx.notify();
    }

    fn handle_mouse_drag(&mut self, line: u64, col: usize, cx: &mut Context<Self>) {
        match &mut self.selection {
            Some(Selection::Char(sel)) => {
                sel.head = CharPos::new(line, col);
                self.pending_click = None;
                cx.notify();
            }
            Some(Selection::Line(_)) => {
                self.extend_selection_to(line, cx);
            }
            None => {}
        }
    }

    fn handle_right_click(&mut self, line: u64, pos: Point<Pixels>, cx: &mut Context<Self>) {
        // Preserve the user's existing selection — only remember the clicked
        // row as a fallback target for the menu's copy actions when no
        // selection is active.
        self.right_click_line = Some(line);
        cx.emit(LogViewEvent::ShowContextMenu(pos));
    }

    /// Clear the right-click fallback. Called when the context menu closes
    /// so a later Cmd+C doesn't accidentally copy a stale row.
    pub fn clear_right_click(&mut self) {
        self.right_click_line = None;
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
}

/// Pixel advance per character for the monospace log font. Courier New's
/// glyph advance is ≈0.6×font_size — close enough for line-content hit-testing.
fn char_advance_for(font_size: Pixels) -> f32 {
    f32::from(font_size) * 0.6
}

fn column_for_x(window_x: f32, gutter_width: f32, char_advance: f32, line_char_len: usize) -> usize {
    if char_advance <= 0.0 {
        return 0;
    }
    let local = window_x - gutter_width;
    if local <= 0.0 {
        return 0;
    }
    let col = (local / char_advance).floor() as isize;
    col.clamp(0, line_char_len as isize) as usize
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Expand the word boundaries around `col` (a char index). Returns
/// `Some((start, end))` for a non-empty word; `None` when the cursor is on
/// whitespace or punctuation. `end` is exclusive in char-index space.
fn word_range_at(line: &str, col: usize) -> Option<(usize, usize)> {
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return None;
    }
    if col >= chars.len() {
        // Caret past last char: if the char immediately to the left is a
        // word char, select that word so clicking just past a word still
        // selects it.
        if col > 0 && is_word_char(chars[col - 1]) {
            let mut start = col - 1;
            while start > 0 && is_word_char(chars[start - 1]) {
                start -= 1;
            }
            return Some((start, col));
        }
        return None;
    }
    if !is_word_char(chars[col]) {
        return None;
    }
    let mut start = col;
    while start > 0 && is_word_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = col + 1;
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }
    Some((start, end))
}

fn char_col_to_byte(s: &str, col: usize) -> usize {
    s.char_indices()
        .nth(col)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

fn join_full_lines(lines: &[Vec<u8>]) -> String {
    let mut out = String::new();
    for (i, bytes) in lines.iter().enumerate() {
        let s = String::from_utf8_lossy(bytes);
        let trimmed = s.strip_suffix('\n').unwrap_or(&s);
        out.push_str(trimmed);
        if i + 1 < lines.len() {
            out.push('\n');
        }
    }
    out
}

fn join_char_range(
    lines: &[Vec<u8>],
    start_col: usize,
    end_col: usize,
    lo_line: u64,
    hi_line: u64,
) -> String {
    let mut out = String::new();
    for (i, bytes) in lines.iter().enumerate() {
        let s = String::from_utf8_lossy(bytes);
        let no_nl = s.strip_suffix('\n').unwrap_or(&s);
        let line_num = lo_line + i as u64;
        let slice: &str = if lo_line == hi_line {
            let lo_b = char_col_to_byte(no_nl, start_col);
            let hi_b = char_col_to_byte(no_nl, end_col);
            &no_nl[lo_b..hi_b]
        } else if line_num == lo_line {
            let lo_b = char_col_to_byte(no_nl, start_col);
            &no_nl[lo_b..]
        } else if line_num == hi_line {
            let hi_b = char_col_to_byte(no_nl, end_col);
            &no_nl[..hi_b]
        } else {
            no_nl
        };
        out.push_str(slice);
        if i + 1 < lines.len() {
            out.push('\n');
        }
    }
    out
}

/// Render the line-content cell. For `LineHighlight::Range`, splits the
/// text into up to three spans so the selected range gets a background
/// color without disturbing the rest of the row.
fn render_line_content(line_text: &str, hi: LineHighlight, theme: Theme) -> AnyElement {
    match hi {
        LineHighlight::None | LineHighlight::Full => div()
            .flex_grow()
            .text_color(theme.foreground)
            .child(line_text.to_string())
            .into_any_element(),
        LineHighlight::Range(start, end) => {
            let (s, e) = if start <= end { (start, end) } else { (end, start) };
            let no_nl = line_text.strip_suffix('\n').unwrap_or(line_text);
            let s_b = char_col_to_byte(no_nl, s);
            let e_b = char_col_to_byte(no_nl, e);
            let pre = no_nl[..s_b].to_string();
            let mid = no_nl[s_b..e_b].to_string();
            let post = no_nl[e_b..].to_string();
            div()
                .flex_grow()
                .flex()
                .flex_row()
                .text_color(theme.foreground)
                .child(pre)
                .child(div().bg(theme.selection).child(mid))
                .child(post)
                .into_any_element()
        }
    }
}

fn digits_for(n: u64) -> usize {
    if n == 0 {
        return 1;
    }
    (n as f64).log10().floor() as usize + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_range_at_splits_on_punctuation() {
        let line = "foo.bar-baz_qux";
        assert_eq!(word_range_at(line, 0), Some((0, 3)));
        assert_eq!(word_range_at(line, 2), Some((0, 3)));
        assert_eq!(word_range_at(line, 3), None);
        assert_eq!(word_range_at(line, 4), Some((4, 7)));
        assert_eq!(word_range_at(line, 7), None);
        // baz_qux: underscore is a word char
        assert_eq!(word_range_at(line, 8), Some((8, 15)));
        assert_eq!(word_range_at(line, 14), Some((8, 15)));
    }

    #[test]
    fn word_range_at_handles_empty_and_whitespace() {
        assert_eq!(word_range_at("", 0), None);
        assert_eq!(word_range_at("   ", 1), None);
    }

    #[test]
    fn join_char_range_single_line() {
        let lines = vec![b"hello world".to_vec()];
        assert_eq!(join_char_range(&lines, 6, 11, 0, 0), "world");
    }

    #[test]
    fn join_char_range_multi_line() {
        let lines = vec![
            b"first line text".to_vec(),
            b"middle whole line".to_vec(),
            b"last bit".to_vec(),
        ];
        // col 6 on line 0 -> col 4 on line 2
        let out = join_char_range(&lines, 6, 4, 0, 2);
        assert_eq!(out, "line text\nmiddle whole line\nlast");
    }

    #[test]
    fn join_full_lines_strips_trailing_newlines() {
        let lines = vec![b"a\n".to_vec(), b"b\n".to_vec(), b"c".to_vec()];
        assert_eq!(join_full_lines(&lines), "a\nb\nc");
    }

    #[test]
    fn column_for_x_clamps_and_floors() {
        assert_eq!(column_for_x(0.0, 10.0, 8.0, 20), 0);
        assert_eq!(column_for_x(10.0, 10.0, 8.0, 20), 0);
        assert_eq!(column_for_x(10.0 + 8.0 * 3.5, 10.0, 8.0, 20), 3);
        assert_eq!(column_for_x(10.0 + 8.0 * 1000.0, 10.0, 8.0, 20), 20);
        assert_eq!(column_for_x(-5.0, 10.0, 8.0, 20), 0);
    }
}
