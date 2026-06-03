//! Shared text-selection primitives for the log and filtered views.
//!
//! Both views let the user select characters/lines with the mouse and copy
//! them; this module holds the selection types, the geometry helpers used to
//! map mouse coordinates to character columns, and the text-joining helpers
//! used to build clipboard strings. The two views differ only in how their
//! "line" coordinate is interpreted — the main view uses absolute file line
//! numbers, the filtered view uses row indices into its match list — so the
//! `Selection` coordinate is just a `u64` either view supplies.

use gpui::*;

use crate::theme::Theme;

/// An inclusive line-range selection. `anchor` is fixed at mouse-down (or at
/// the line that started a keyboard-driven selection); `head` follows the
/// cursor while dragging and is the line returned by `selected_line()`.
#[derive(Clone, Copy, Debug)]
pub struct LineSelection {
    pub anchor: u64,
    pub head: u64,
}

impl LineSelection {
    pub fn single(line: u64) -> Self {
        Self { anchor: line, head: line }
    }

    pub fn lo(&self) -> u64 {
        self.anchor.min(self.head)
    }

    pub fn hi(&self) -> u64 {
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
    pub fn new(line: u64, col: usize) -> Self {
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
    pub fn ordered(&self) -> (CharPos, CharPos) {
        let (a, h) = (self.anchor, self.head);
        if (a.line, a.col) <= (h.line, h.col) { (a, h) } else { (h, a) }
    }

    pub fn is_empty(&self) -> bool {
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
    pub fn lo_line(&self) -> u64 {
        match self {
            Selection::Line(s) => s.lo(),
            Selection::Char(s) => s.ordered().0.line,
        }
    }

    pub fn hi_line(&self) -> u64 {
        match self {
            Selection::Line(s) => s.hi(),
            Selection::Char(s) => s.ordered().1.line,
        }
    }

    pub fn head_line(&self) -> u64 {
        match self {
            Selection::Line(s) => s.head,
            Selection::Char(s) => s.head.line,
        }
    }

    pub fn highlight_for(&self, line_num: u64, line_char_len: usize) -> LineHighlight {
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
pub enum LineHighlight {
    None,
    Full,
    Range(usize, usize),
}

/// Pixel advance per character for the monospace log font. Courier New's
/// glyph advance is ≈0.6×font_size — close enough for line-content hit-testing.
pub fn char_advance_for(font_size: Pixels) -> f32 {
    f32::from(font_size) * 0.6
}

pub fn column_for_x(
    window_x: f32,
    gutter_width: f32,
    char_advance: f32,
    h_offset: f32,
    line_char_len: usize,
) -> usize {
    if char_advance <= 0.0 {
        return 0;
    }
    // Add the horizontal scroll offset so hit-testing maps to the character
    // actually under the cursor when the content is scrolled sideways.
    let local = window_x - gutter_width + h_offset;
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
pub fn word_range_at(line: &str, col: usize) -> Option<(usize, usize)> {
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

pub fn join_full_lines(lines: &[Vec<u8>]) -> String {
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

pub fn join_char_range(
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
pub fn render_line_content(line_text: &str, hi: LineHighlight, theme: Theme) -> AnyElement {
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
        assert_eq!(column_for_x(0.0, 10.0, 8.0, 0.0, 20), 0);
        assert_eq!(column_for_x(10.0, 10.0, 8.0, 0.0, 20), 0);
        assert_eq!(column_for_x(10.0 + 8.0 * 3.5, 10.0, 8.0, 0.0, 20), 3);
        assert_eq!(column_for_x(10.0 + 8.0 * 1000.0, 10.0, 8.0, 0.0, 20), 20);
        assert_eq!(column_for_x(-5.0, 10.0, 8.0, 0.0, 20), 0);
    }
}
