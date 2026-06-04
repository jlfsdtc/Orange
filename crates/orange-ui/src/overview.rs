//! Minimap (heat-strip overview) shown on the right side of a log view.
//!
//! Renders a klogg-style strip rather than a VSCode zoomed-text minimap —
//! GPUI is div-only in this codebase, so we aggregate match/bookmark line
//! numbers into fixed bins and draw thin colored strips at percent offsets.
//! A semi-transparent rectangle shows the current viewport; the user can
//! click to jump, drag the rectangle to scroll, or hover for a tooltip.
//! The parent view (LogView, QuickFind) consumes `MinimapEvent::ScrollTo`
//! to drive its scroll handle.

use gpui::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::theme::{MinimapSettings, Theme};

/// Fallback minimap width if the `MinimapSettings` global hasn't been
/// installed yet. Matches `MinimapSettings::DEFAULT`. The user-facing
/// width is read from the global at render time.
pub const MINIMAP_WIDTH_PX: f32 = MinimapSettings::DEFAULT;

/// Number of vertical bins used to aggregate matches/bookmarks. 600 is more
/// than enough rows for any realistic minimap height — anything finer would
/// alias against the actual screen pixels anyway.
const BIN_ROWS: u32 = 600;

/// Pixel height of a single strip drawn for a populated bin.
const STRIP_HEIGHT_PX: f32 = 2.0;

/// Event emitted by `MinimapState`. The parent view should scroll so that
/// `line` becomes visible.
pub enum MinimapEvent {
    ScrollTo(u64),
}

/// Minimap state. The same widget is used both next to the main log view
/// and next to the QuickFind results panel — the parent feeds it line totals
/// and listens for `MinimapEvent::ScrollTo` to drive its own scroll handle.
pub struct OverviewState {
    total_lines: u64,
    matches: Vec<u64>,
    bookmarks: Vec<u64>,
    viewport_start: u64,
    viewport_size: u64,
    visible: bool,
    /// Hovered line, shown as a tooltip near the cursor. `None` when the
    /// mouse is outside the strip or while a drag is in progress.
    hover_line: Option<u64>,
    /// Hover y in pixels, relative to the strip's top edge.
    hover_y: Option<f32>,
    /// Active drag of the viewport rect: offset (in pixels) of the mouse
    /// from the rect's top edge when the drag began. `None` when no drag is
    /// in progress.
    drag_offset: Option<f32>,
    /// Captured bounds of the strip, written by the inner `canvas` element
    /// at prepaint and read by mouse listeners. `Rc<Cell<_>>` so the render
    /// closure can keep its own clone alive after `render` returns.
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl EventEmitter<MinimapEvent> for OverviewState {}

impl OverviewState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Repaint when the user resizes the minimap from the Options dialog.
        cx.observe_global::<MinimapSettings>(|_, cx| cx.notify()).detach();
        Self {
            total_lines: 0,
            matches: Vec::new(),
            bookmarks: Vec::new(),
            viewport_start: 0,
            viewport_size: 0,
            visible: true,
            hover_line: None,
            hover_y: None,
            drag_offset: None,
            bounds: Rc::new(Cell::new(None)),
        }
    }

    pub fn set_total_lines(&mut self, lines: u64, cx: &mut Context<Self>) {
        if self.total_lines != lines {
            self.total_lines = lines;
            cx.notify();
        }
    }

    pub fn set_matches(&mut self, matches: Vec<u64>, cx: &mut Context<Self>) {
        if self.matches != matches {
            self.matches = matches;
            cx.notify();
        }
    }

    pub fn set_bookmarks(&mut self, bookmarks: Vec<u64>, cx: &mut Context<Self>) {
        if self.bookmarks != bookmarks {
            self.bookmarks = bookmarks;
            cx.notify();
        }
    }

    pub fn set_viewport(&mut self, start: u64, size: u64, cx: &mut Context<Self>) {
        if self.viewport_start != start || self.viewport_size != size {
            self.viewport_start = start;
            self.viewport_size = size;
            cx.notify();
        }
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.visible = !self.visible;
        cx.notify();
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Width to reserve in the parent's layout. Returns 0 when the minimap
    /// is hidden so the parent's flex layout collapses the slot cleanly.
    pub fn slot_width(&self) -> f32 {
        if self.visible { MINIMAP_WIDTH_PX } else { 0.0 }
    }

    /// Convert a y-offset within the strip into a line number. Used for
    /// click-to-jump and the hover tooltip, where the wanted line is simply
    /// "the one at this y".
    fn line_for_y(&self, y_px: f32, strip_height_px: f32) -> u64 {
        if self.total_lines == 0 || strip_height_px <= 0.0 {
            return 0;
        }
        let fraction = (y_px / strip_height_px).clamp(0.0, 1.0);
        let line = (fraction * self.total_lines as f32).floor() as u64;
        line.min(self.total_lines.saturating_sub(1))
    }

    /// Map a dragged thumb/rectangle *top* position to the first visible line,
    /// using standard scrollbar semantics: the full travel
    /// `[0, track_h - thumb_h]` maps onto the scrollable line range
    /// `[0, total_lines - viewport_size]`. Unlike [`Self::line_for_y`], this
    /// guarantees that dragging to the very bottom shows the file's *last*
    /// line — the naive mapping stops short by the thumb's height, leaving the
    /// tail unreachable on large files (where the thumb is at its minimum size).
    fn line_for_thumb_top(&self, target_top: f32, thumb_h: f32, track_h: f32) -> u64 {
        let max_line = self.total_lines.saturating_sub(self.viewport_size.max(1));
        let travel = (track_h - thumb_h).max(0.0);
        if max_line == 0 || travel <= 0.0 {
            return 0;
        }
        let frac = (target_top / travel).clamp(0.0, 1.0);
        (frac * max_line as f32).round() as u64
    }

    fn viewport_top_fraction(&self) -> f32 {
        if self.total_lines == 0 {
            0.0
        } else {
            (self.viewport_start as f32 / self.total_lines as f32).clamp(0.0, 1.0)
        }
    }

    fn viewport_size_fraction(&self) -> f32 {
        if self.total_lines == 0 {
            1.0
        } else {
            (self.viewport_size as f32 / self.total_lines as f32).clamp(0.02, 1.0)
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(bounds) = self.bounds.get() else { return };
        let strip_height = f32::from(bounds.size.height);
        let local_y = f32::from(event.position.y - bounds.origin.y);
        if local_y < 0.0 || local_y > strip_height {
            return;
        }

        let vp_top = self.viewport_top_fraction() * strip_height;
        let vp_height = self.viewport_size_fraction() * strip_height;
        if local_y >= vp_top && local_y <= vp_top + vp_height {
            // Inside the viewport rectangle → start a drag.
            self.drag_offset = Some(local_y - vp_top);
            self.hover_line = None;
            cx.notify();
        } else {
            // Outside → jump directly.
            let line = self.line_for_y(local_y, strip_height);
            cx.emit(MinimapEvent::ScrollTo(line));
        }
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(bounds) = self.bounds.get() else { return };
        let strip_height = f32::from(bounds.size.height);
        let local_y = f32::from(event.position.y - bounds.origin.y);

        if let Some(offset) = self.drag_offset {
            if event.pressed_button != Some(MouseButton::Left) {
                // Button released outside our element — clear the drag.
                self.drag_offset = None;
                cx.notify();
                return;
            }
            let vp_height = self.viewport_size_fraction() * strip_height;
            let target_top = (local_y - offset).clamp(0.0, (strip_height - vp_height).max(0.0));
            let line = self.line_for_thumb_top(target_top, vp_height, strip_height);
            cx.emit(MinimapEvent::ScrollTo(line));
            return;
        }

        if local_y < 0.0 || local_y > strip_height {
            if self.hover_line.take().is_some() {
                self.hover_y = None;
                cx.notify();
            }
            return;
        }

        let line = self.line_for_y(local_y, strip_height);
        let changed = self.hover_line != Some(line);
        self.hover_line = Some(line);
        self.hover_y = Some(local_y);
        if changed {
            cx.notify();
        }
    }

    fn on_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.drag_offset.take().is_some() {
            cx.notify();
        }
    }

    /// Render the minimap. Returns an empty div when hidden or when there
    /// are no lines to represent.
    pub fn render(
        &mut self,
        theme: Theme,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.visible || self.total_lines == 0 {
            return div().into_any();
        }

        let width = cx.global::<MinimapSettings>().width;

        let bounds_handle = self.bounds.clone();
        let total = self.total_lines;
        let match_bins = bin_positions(&self.matches, total, BIN_ROWS);
        let bookmark_bins = bin_positions(&self.bookmarks, total, BIN_ROWS);

        let vp_top_pct = self.viewport_top_fraction();
        let vp_height_pct = self.viewport_size_fraction();

        // Heat strip: fills the minimap. Carries the click-to-jump /
        // drag-the-viewport handlers and the bounds canvas.
        let mut container = div()
            .relative()
            .flex_grow()
            .h_full()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            // Bounds capture: the canvas's prepaint stores the strip's
            // bounds so the mouse listeners can convert window coordinates
            // into a y-offset within the strip.
            .child(
                canvas(
                    move |bounds, _window, _cx| bounds_handle.set(Some(bounds)),
                    |_bounds, _prepaint, _window, _cx| {},
                )
                .absolute()
                .size_full(),
            );

        // Match strips (yellow, alpha scaled by bin density).
        let max_match = *match_bins.iter().max().unwrap_or(&0);
        if max_match > 0 {
            for (row, &count) in match_bins.iter().enumerate() {
                if count == 0 {
                    continue;
                }
                let alpha = (count as f32 / max_match as f32).clamp(0.3, 1.0);
                let top_pct = row as f32 / BIN_ROWS as f32;
                let mut color = theme.search_match;
                color.a = alpha;
                container = container.child(
                    div()
                        .absolute()
                        .top(relative(top_pct))
                        .left_0()
                        .right_0()
                        .h(px(STRIP_HEIGHT_PX))
                        .bg(color),
                );
            }
        }

        // Bookmark strips (blue) drawn after matches so they layer on top.
        let max_bookmark = *bookmark_bins.iter().max().unwrap_or(&0);
        if max_bookmark > 0 {
            for (row, &count) in bookmark_bins.iter().enumerate() {
                if count == 0 {
                    continue;
                }
                let alpha = (count as f32 / max_bookmark as f32).clamp(0.4, 1.0);
                let top_pct = row as f32 / BIN_ROWS as f32;
                let mut color = theme.bookmark;
                color.a = alpha;
                container = container.child(
                    div()
                        .absolute()
                        .top(relative(top_pct))
                        .left_0()
                        .right_0()
                        .h(px(STRIP_HEIGHT_PX))
                        .bg(color),
                );
            }
        }

        // Viewport rectangle.
        let mut vp_color = theme.selection;
        vp_color.a = 0.45;
        container = container.child(
            div()
                .absolute()
                .top(relative(vp_top_pct))
                .left_0()
                .right_0()
                .h(relative(vp_height_pct))
                .bg(vp_color)
                .border_1()
                .border_color(theme.line_number),
        );

        let mut wrapper = div()
            .flex()
            .flex_row()
            .relative()
            .w(width)
            .h_full()
            .bg(theme.current_line)
            .border_l_1()
            .border_color(theme.selection)
            .child(container);

        // Hover tooltip (skip while dragging the viewport). Kept on the outer
        // wrapper so its right-edge anchor stays relative to the whole minimap.
        if let (Some(line), Some(y), None) =
            (self.hover_line, self.hover_y, self.drag_offset)
        {
            let label = format!("line {}", line + 1);
            // Anchor tooltip a few pixels above the cursor; clamp to the
            // strip so it doesn't render outside.
            let tooltip_y = (y - 18.0).max(0.0);
            wrapper = wrapper.child(
                div()
                    .absolute()
                    .top(px(tooltip_y))
                    .right(width + px(4.0))
                    .px_2()
                    .py(px(2.0))
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.selection)
                    .text_color(theme.foreground)
                    .child(label),
            );
        }

        wrapper.into_any()
    }
}

/// Aggregate `positions` (sorted or unsorted line numbers, all < `total`)
/// into `rows` evenly-sized bins. Returns the per-bin count. Returns an
/// empty vector if `total` is zero.
fn bin_positions(positions: &[u64], total: u64, rows: u32) -> Vec<u32> {
    if total == 0 || rows == 0 {
        return Vec::new();
    }
    let mut bins = vec![0u32; rows as usize];
    let rows_f = rows as f32;
    let total_f = total as f32;
    for &line in positions {
        if line >= total {
            continue;
        }
        let idx = ((line as f32 / total_f) * rows_f).floor() as usize;
        let idx = idx.min(rows as usize - 1);
        bins[idx] = bins[idx].saturating_add(1);
    }
    bins
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bin_empty() {
        assert!(bin_positions(&[], 0, 10).is_empty());
        assert_eq!(bin_positions(&[], 100, 5), vec![0; 5]);
    }

    #[test]
    fn bin_evenly_distributed() {
        // 100 positions evenly across 10 rows of 100 lines → 10 per bin.
        let positions: Vec<u64> = (0..100).collect();
        let bins = bin_positions(&positions, 100, 10);
        assert_eq!(bins, vec![10u32; 10]);
    }

    #[test]
    fn bin_clamps_overflow() {
        // A stray out-of-range line is dropped, not panicked.
        let bins = bin_positions(&[5, 200], 100, 10);
        let total: u32 = bins.iter().sum();
        assert_eq!(total, 1);
    }
}
