//! Reusable vertical scrollbar for the log and filtered views.
//!
//! GPUI's `uniform_list` virtualizes vertically but paints no scrollbar of its
//! own, so — like `h_scrollbar.rs` for the horizontal axis — we draw a
//! draggable thumb and drive the list ourselves. Unlike the horizontal bar
//! (whose offset we own), the vertical position lives in the list's
//! [`UniformListScrollHandle`]: we *read* the current pixel offset to place the
//! thumb and *write* a new position with `scroll_to_item_strict` (line
//! granularity, which is plenty fine at one line per `line_height` pixels).
//!
//! [`render`] is generic over the owning view `V` and takes a plain function
//! pointer `accessor` that returns the embedded `&mut VScrollState`, so the
//! same widget drives both `LogViewState` and `FilteredViewState`.

use gpui::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::theme::Theme;

/// Width of the scrollbar strip.
const SCROLLBAR_WIDTH_PX: f32 = 12.0;
/// Minimum thumb height so it stays grabbable even for very tall content.
const MIN_THUMB_PX: f32 = 24.0;

/// Vertical scroll state embedded in a view. The scroll *position* itself lives
/// in the view's `UniformListScrollHandle`; this only holds the bits the bar
/// needs across frames — the active drag and the measured track bounds.
pub struct VScrollState {
    /// Active thumb drag: pixel offset of the cursor from the thumb's top edge
    /// when the drag began. `None` when not dragging.
    drag: Option<f32>,
    /// Track bounds captured by the inner `canvas` at paint time and read by
    /// the mouse listeners (and by `render` for thumb geometry). `Rc<Cell<_>>`
    /// so the render closures can hold their own clone.
    track_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl Default for VScrollState {
    fn default() -> Self {
        Self::new()
    }
}

impl VScrollState {
    pub fn new() -> Self {
        Self {
            drag: None,
            track_bounds: Rc::new(Cell::new(None)),
        }
    }

    /// Last known height of the track (== the list viewport height), or `None`
    /// before the first paint.
    fn track_height(&self) -> Option<f32> {
        self.track_bounds.get().map(|b| f32::from(b.size.height))
    }
}

/// Pixels the content is scrolled down (`0` = top). The handle stores a
/// negative `y` offset as the user scrolls down, so we negate it.
fn scrolled_px(handle: &UniformListScrollHandle) -> f32 {
    let state = handle.0.borrow();
    let y = f32::from(state.base_handle.offset().y);
    drop(state);
    (-y).max(0.0)
}

/// Thumb height/top in pixels for the current scroll position, or `None` when
/// the content fits (no scrollbar needed) or the track hasn't been measured.
fn thumb_geometry(scrolled: f32, content_h: f32, track_h: f32) -> Option<(f32, f32)> {
    if track_h <= 0.0 || content_h <= track_h {
        return None;
    }
    let thumb_h = (track_h * track_h / content_h).clamp(MIN_THUMB_PX.min(track_h), track_h);
    let travel = track_h - thumb_h;
    let max_off = content_h - track_h;
    let top = if max_off > 0.0 {
        (scrolled / max_off) * travel
    } else {
        0.0
    };
    Some((thumb_h, top.clamp(0.0, travel)))
}

/// Map a thumb-top position (in track pixels) to the line that should sit at
/// the top of the viewport.
fn line_for_top(
    target_top: f32,
    content_h: f32,
    track_h: f32,
    line_height: f32,
    total_lines: u64,
) -> u64 {
    let Some((thumb_h, _)) = thumb_geometry(0.0, content_h, track_h) else {
        return 0;
    };
    let travel = track_h - thumb_h;
    let max_off = content_h - track_h;
    let scrolled = if travel > 0.0 {
        (target_top / travel) * max_off
    } else {
        0.0
    };
    if line_height <= 0.0 {
        return 0;
    }
    let line = (scrolled / line_height).round() as u64;
    line.min(total_lines.saturating_sub(1))
}

/// Render the vertical scrollbar strip for view `V`.
///
/// - `state` is the view's embedded scroll state (drag + measured bounds).
/// - `accessor` re-fetches that same `&mut VScrollState` at event time.
/// - `scroll_handle` is the list's handle: read for the thumb position, driven
///   (via `scroll_to_item_strict`) on click/drag.
/// - `total_lines` is the row count; `line_height` the per-row height in px.
pub fn render<V: 'static>(
    state: &mut VScrollState,
    accessor: fn(&mut V) -> &mut VScrollState,
    scroll_handle: &UniformListScrollHandle,
    total_lines: u64,
    line_height: f32,
    theme: Theme,
    cx: &mut Context<V>,
) -> AnyElement {
    let content_h = total_lines as f32 * line_height;
    let scrolled = scrolled_px(scroll_handle);

    // Whether a thumb drag is in progress. While it is, the canvas paint
    // closure registers window-level listeners so the drag keeps tracking even
    // when the cursor leaves the thin strip.
    let dragging = state.drag.is_some();

    let thumb = state
        .track_height()
        .and_then(|th| thumb_geometry(scrolled, content_h, th));

    // Bounds capture + drag tracking, mirroring `h_scrollbar`.
    let bounds_cell = state.track_bounds.clone();
    let weak = cx.entity().downgrade();
    let weak_paint = cx.entity().downgrade();
    let handle_paint = scroll_handle.clone();
    let capture = canvas(
        move |bounds, _window, cx| {
            let changed = bounds_cell.get().map(|b| b.size.height) != Some(bounds.size.height);
            bounds_cell.set(Some(bounds));
            if changed {
                weak.update(cx, |_, cx| cx.notify()).ok();
            }
        },
        move |_bounds, _prepaint, window, _cx| {
            if !dragging {
                return;
            }
            // Drag move: anywhere in the window, place the thumb at the cursor
            // (minus the grab offset) and scroll the list to the matching line.
            let move_weak = weak_paint.clone();
            let move_handle = handle_paint.clone();
            window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                let y = f32::from(ev.position.y);
                let pressed = ev.pressed_button;
                let handle = move_handle.clone();
                move_weak
                    .update(cx, |this, cx| {
                        let st = accessor(this);
                        let Some(grab) = st.drag else { return };
                        if pressed != Some(MouseButton::Left) {
                            st.drag = None;
                            cx.notify();
                            return;
                        }
                        let Some(bounds) = st.track_bounds.get() else { return };
                        let track_h = f32::from(bounds.size.height);
                        let Some((thumb_h, _)) = thumb_geometry(0.0, content_h, track_h) else {
                            return;
                        };
                        let travel = track_h - thumb_h;
                        if travel <= 0.0 {
                            return;
                        }
                        let local_y = y - f32::from(bounds.origin.y);
                        let target_top = (local_y - grab).clamp(0.0, travel);
                        let line =
                            line_for_top(target_top, content_h, track_h, line_height, total_lines);
                        handle.scroll_to_item_strict(line as usize, ScrollStrategy::Top);
                        cx.notify();
                    })
                    .ok();
            });
            // Drag end: release anywhere finishes the drag.
            let up_weak = weak_paint.clone();
            window.on_mouse_event(move |_ev: &MouseUpEvent, phase, _window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                up_weak
                    .update(cx, |this, cx| {
                        if accessor(this).drag.take().is_some() {
                            cx.notify();
                        }
                    })
                    .ok();
            });
        },
    )
    .absolute()
    .size_full();

    let mut track = div()
        .relative()
        .w(px(SCROLLBAR_WIDTH_PX))
        .h_full()
        .flex_shrink_0()
        .bg(theme.current_line)
        .border_l_1()
        .border_color(theme.selection)
        .on_mouse_down(MouseButton::Left, {
            let entity = cx.entity();
            let handle = scroll_handle.clone();
            move |ev: &MouseDownEvent, _window, cx| {
                let y = f32::from(ev.position.y);
                let handle = handle.clone();
                entity.update(cx, |this, cx| {
                    let scrolled = scrolled_px(&handle);
                    let content_h = total_lines as f32 * line_height;
                    let st = accessor(this);
                    let Some(bounds) = st.track_bounds.get() else { return };
                    let track_h = f32::from(bounds.size.height);
                    let Some((thumb_h, thumb_top)) =
                        thumb_geometry(scrolled, content_h, track_h)
                    else {
                        return;
                    };
                    let local_y = y - f32::from(bounds.origin.y);
                    let travel = track_h - thumb_h;
                    if local_y >= thumb_top && local_y <= thumb_top + thumb_h {
                        // Grab the thumb → drag from where it was grabbed.
                        st.drag = Some(local_y - thumb_top);
                    } else {
                        // Click off the thumb → center it on the cursor, jump,
                        // then drag from there.
                        let target_top = (local_y - thumb_h / 2.0).clamp(0.0, travel);
                        let line = line_for_top(
                            target_top, content_h, track_h, line_height, total_lines,
                        );
                        handle.scroll_to_item_strict(line as usize, ScrollStrategy::Top);
                        st.drag = Some(thumb_h / 2.0);
                    }
                    cx.notify();
                });
            }
        })
        // Ongoing drag move/up are handled by window-level listeners registered
        // in the canvas paint closure above, so the thumb keeps tracking even
        // when the cursor leaves this thin strip.
        .child(capture);

    if let Some((thumb_h, thumb_top)) = thumb {
        track = track.child(
            div()
                .absolute()
                .top(px(thumb_top))
                .left(px(2.0))
                .right(px(2.0))
                .h(px(thumb_h))
                .rounded(px(3.0))
                .bg(theme.line_number),
        );
    }

    track.into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_thumb_when_content_fits() {
        assert!(thumb_geometry(0.0, 100.0, 200.0).is_none());
        assert!(thumb_geometry(0.0, 200.0, 200.0).is_none());
    }

    #[test]
    fn thumb_sized_by_ratio() {
        // content twice the viewport → thumb half the track.
        let (h, top) = thumb_geometry(0.0, 400.0, 200.0).unwrap();
        assert_eq!(h, 100.0);
        assert_eq!(top, 0.0);
    }

    #[test]
    fn thumb_at_max_scroll_sits_at_track_end() {
        let track_h = 200.0;
        let content_h = 400.0;
        let max_off = content_h - track_h; // 200
        let (h, top) = thumb_geometry(max_off, content_h, track_h).unwrap();
        assert!((top - (track_h - h)).abs() < 0.001);
    }

    #[test]
    fn line_for_top_maps_endpoints() {
        // 1000 lines, 10px each → content 10000, track 500.
        let content_h = 10_000.0;
        let track_h = 500.0;
        let lh = 10.0;
        let total = 1000;
        // Top of track → first line.
        assert_eq!(line_for_top(0.0, content_h, track_h, lh, total), 0);
        // Bottom of travel → last scrollable line (clamped to total-1).
        let (thumb_h, _) = thumb_geometry(0.0, content_h, track_h).unwrap();
        let travel = track_h - thumb_h;
        let line = line_for_top(travel, content_h, track_h, lh, total);
        assert!(line <= total - 1);
        assert!(line >= 940); // max scroll ≈ (content-track)/lh = 950
    }
}
