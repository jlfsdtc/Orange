//! Reusable horizontal scrollbar for the log and filtered views.
//!
//! GPUI's `uniform_list` virtualizes vertically but paints no scrollbar of its
//! own, so — like the minimap in `overview.rs` — we manage a horizontal pixel
//! offset ourselves and draw a draggable thumb. The offset shifts only the line
//! *content* (the line-number gutter stays fixed, klogg-style); the owning view
//! reads [`HScrollState::offset`] to translate its rows.
//!
//! [`render`] is generic over the owning view `V` and takes a plain function
//! pointer `accessor` that returns the embedded `&mut HScrollState`, so the same
//! widget drives both `LogViewState` and `FilteredViewState` without either view
//! needing to know about the other.

use gpui::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::theme::Theme;

/// Height of the scrollbar strip.
const SCROLLBAR_HEIGHT_PX: f32 = 12.0;
/// Minimum thumb width so it stays grabbable even for very wide content.
const MIN_THUMB_PX: f32 = 24.0;

#[derive(Clone, Copy)]
struct DragStart {
    /// Window x where the drag began.
    mouse_x: f32,
    /// Scroll offset when the drag began.
    offset: f32,
}

/// Horizontal scroll state embedded in a view. `offset` is the number of pixels
/// the content is scrolled to the right (`0` = fully left).
pub struct HScrollState {
    offset: f32,
    /// Active thumb drag, or `None` when not dragging.
    drag: Option<DragStart>,
    /// Track bounds captured by the inner `canvas` at paint time and read by
    /// the mouse listeners (and by `render` for thumb geometry). `Rc<Cell<_>>`
    /// so the render closures can hold their own clone.
    track_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl Default for HScrollState {
    fn default() -> Self {
        Self::new()
    }
}

impl HScrollState {
    pub fn new() -> Self {
        Self {
            offset: 0.0,
            drag: None,
            track_bounds: Rc::new(Cell::new(None)),
        }
    }

    /// Pixels the content is scrolled to the right.
    pub fn offset(&self) -> f32 {
        self.offset
    }

    /// Last known width of the track (== the content viewport width), or `None`
    /// before the first paint.
    fn track_width(&self) -> Option<f32> {
        self.track_bounds.get().map(|b| f32::from(b.size.width))
    }

    /// Maximum scrollable offset for the given total content width.
    fn max_offset(&self, content_w: f32) -> f32 {
        let viewport = self.track_width().unwrap_or(content_w);
        (content_w - viewport).max(0.0)
    }

    /// Adjust the offset by `dx` pixels (e.g. from a wheel / trackpad gesture),
    /// clamped to the scrollable range. Returns `true` if the offset changed.
    pub fn scroll_by(&mut self, dx: f32, content_w: f32) -> bool {
        let max = self.max_offset(content_w);
        let new = (self.offset + dx).clamp(0.0, max);
        if new != self.offset {
            self.offset = new;
            true
        } else {
            false
        }
    }
}

/// Thumb width/left in pixels for the current state, or `None` when the content
/// fits (no scrollbar needed) or the track hasn't been measured yet.
fn thumb_geometry(offset: f32, content_w: f32, track_w: f32) -> Option<(f32, f32)> {
    if track_w <= 0.0 || content_w <= track_w {
        return None;
    }
    let thumb_w = (track_w * track_w / content_w).clamp(MIN_THUMB_PX.min(track_w), track_w);
    let travel = track_w - thumb_w;
    let max_off = content_w - track_w;
    let left = if max_off > 0.0 {
        (offset / max_off) * travel
    } else {
        0.0
    };
    Some((thumb_w, left.clamp(0.0, travel)))
}

/// Render the horizontal scrollbar strip for view `V`.
///
/// - `state` is the view's embedded scroll state (borrowed for geometry and to
///   re-clamp the offset when the viewport changes).
/// - `accessor` re-fetches that same `&mut HScrollState` at event time.
/// - `content_w` is the total content width in pixels (longest line).
/// - `gutter_width` is the fixed line-number gutter; the track is offset by it
///   so the thumb aligns under the scrollable text area.
pub fn render<V: 'static>(
    state: &mut HScrollState,
    accessor: fn(&mut V) -> &mut HScrollState,
    content_w: f32,
    gutter_width: f32,
    theme: Theme,
    cx: &mut Context<V>,
) -> AnyElement {
    // Re-clamp in case the viewport widened (e.g. window resize) since last frame.
    let max_off = state.max_offset(content_w);
    if state.offset > max_off {
        state.offset = max_off;
    }

    // Whether a thumb drag is currently in progress. When it is, we register
    // window-level mouse listeners (in the canvas paint closure below) so the
    // drag keeps tracking even when the cursor leaves the thin strip — a plain
    // `on_mouse_move` on the track only fires while the cursor is over it.
    let dragging = state.drag.is_some();

    let thumb = state
        .track_width()
        .and_then(|tw| thumb_geometry(state.offset, content_w, tw));

    // Bounds capture + drag tracking. The prepaint closure stores the track's
    // painted bounds (and requests one repaint when the width changes so
    // `render` can size the thumb). The paint closure registers window-level
    // mouse listeners while a drag is active, so the thumb keeps following the
    // cursor even when it strays off the thin strip vertically.
    let bounds_cell = state.track_bounds.clone();
    let weak = cx.entity().downgrade();
    let weak_paint = cx.entity().downgrade();
    let capture = canvas(
        move |bounds, _window, cx| {
            let changed = bounds_cell.get().map(|b| b.size.width) != Some(bounds.size.width);
            bounds_cell.set(Some(bounds));
            if changed {
                weak.update(cx, |_, cx| cx.notify()).ok();
            }
        },
        move |_bounds, _prepaint, window, _cx| {
            if !dragging {
                return;
            }
            // Drag move: anywhere in the window, drive the offset from the
            // horizontal delta since the drag began.
            let move_weak = weak_paint.clone();
            window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                let x = f32::from(ev.position.x);
                let pressed = ev.pressed_button;
                move_weak
                    .update(cx, |this, cx| {
                        let st = accessor(this);
                        let Some(ds) = st.drag else { return };
                        if pressed != Some(MouseButton::Left) {
                            st.drag = None;
                            cx.notify();
                            return;
                        }
                        let Some(bounds) = st.track_bounds.get() else { return };
                        let track_w = f32::from(bounds.size.width);
                        let Some((thumb_w, _)) = thumb_geometry(ds.offset, content_w, track_w)
                        else {
                            return;
                        };
                        let travel = track_w - thumb_w;
                        if travel <= 0.0 {
                            return;
                        }
                        let max_off = content_w - track_w;
                        let dx = x - ds.mouse_x;
                        let new = (ds.offset + dx / travel * max_off).clamp(0.0, max_off);
                        if new != st.offset {
                            st.offset = new;
                            cx.notify();
                        }
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
        .flex_grow()
        .h(px(SCROLLBAR_HEIGHT_PX))
        .bg(theme.current_line)
        .border_t_1()
        .border_color(theme.selection)
        .on_mouse_down(MouseButton::Left, {
            let entity = cx.entity();
            move |ev: &MouseDownEvent, _window, cx| {
                let x = f32::from(ev.position.x);
                entity.update(cx, |this, cx| {
                    let st = accessor(this);
                    let Some(bounds) = st.track_bounds.get() else { return };
                    let track_w = f32::from(bounds.size.width);
                    let Some((thumb_w, thumb_left)) =
                        thumb_geometry(st.offset, content_w, track_w)
                    else {
                        return;
                    };
                    let local_x = x - f32::from(bounds.origin.x);
                    let travel = track_w - thumb_w;
                    let max_off = (content_w - track_w).max(0.0);
                    if local_x < thumb_left || local_x > thumb_left + thumb_w {
                        // Click on the track outside the thumb → jump so the
                        // thumb centers on the cursor, then drag from there.
                        let target_left = (local_x - thumb_w / 2.0).clamp(0.0, travel);
                        st.offset = if travel > 0.0 {
                            (target_left / travel * max_off).clamp(0.0, max_off)
                        } else {
                            0.0
                        };
                    }
                    st.drag = Some(DragStart {
                        mouse_x: x,
                        offset: st.offset,
                    });
                    cx.notify();
                });
            }
        })
        // Ongoing drag move/up are handled by window-level listeners registered
        // in the canvas paint closure above, so the thumb keeps tracking even
        // when the cursor leaves this thin strip.
        .child(capture);

    if let Some((thumb_w, thumb_left)) = thumb {
        track = track.child(
            div()
                .absolute()
                .top(px(2.0))
                .bottom(px(2.0))
                .left(px(thumb_left))
                .w(px(thumb_w))
                .rounded(px(3.0))
                .bg(theme.line_number),
        );
    }

    div()
        .flex()
        .flex_row()
        .w_full()
        .h(px(SCROLLBAR_HEIGHT_PX))
        .flex_shrink_0()
        // Spacer under the line-number gutter so the track aligns with the text.
        .child(div().w(px(gutter_width)).flex_shrink_0())
        .child(track)
        .into_any()
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
        let (w, left) = thumb_geometry(0.0, 400.0, 200.0).unwrap();
        assert_eq!(w, 100.0);
        assert_eq!(left, 0.0);
    }

    #[test]
    fn thumb_at_max_offset_sits_at_track_end() {
        let track_w = 200.0;
        let content_w = 400.0;
        let max_off = content_w - track_w; // 200
        let (w, left) = thumb_geometry(max_off, content_w, track_w).unwrap();
        assert!((left - (track_w - w)).abs() < 0.001);
    }

    #[test]
    fn scroll_by_clamps() {
        let mut s = HScrollState::new();
        // No measured track → viewport defaults to content_w → max 0.
        assert!(!s.scroll_by(50.0, 400.0));
        // Simulate a measured 200px track.
        s.track_bounds
            .set(Some(Bounds::new(point(px(0.0), px(0.0)), size(px(200.0), px(12.0)))));
        assert!(s.scroll_by(50.0, 400.0));
        assert_eq!(s.offset(), 50.0);
        assert!(s.scroll_by(1000.0, 400.0));
        assert_eq!(s.offset(), 200.0); // clamped to content_w - track_w
        assert!(!s.scroll_by(10.0, 400.0));
    }
}
