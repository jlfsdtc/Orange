//! Go-to-line dialog: a small modal that captures a line number and emits
//! a jump event when the user presses Enter.
//!
//! The input field is a hand-rolled text field modeled on `QuickFindState`:
//! it has a real byte-offset caret (`caret`) with a focus-gated blinking bar,
//! click/drag selection, and Cmd/Ctrl + A/C/V/X clipboard support. It stays a
//! numeric field — typed and pasted text is filtered to ASCII digits before
//! insertion. While visible, the dialog owns keyboard focus; Enter parses the
//! buffer and emits `GoToLineEvent::Jump(line)`, Esc closes it.

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::theme::Theme;

/// Events emitted by `GoToLineState`.
#[derive(Clone, Debug)]
pub enum GoToLineEvent {
    /// The parent should scroll to and select this 1-based line number.
    Jump(u64),
    /// The dialog closed (Esc or after submit). The parent should reclaim
    /// keyboard focus — the dialog grabbed it on `open` via `track_focus` and
    /// doesn't release it, so MainWindow-scoped actions (e.g. `GoToLineDialog`
    /// on Cmd/Ctrl+L) wouldn't resolve again until the user clicked the window.
    Closed,
}

/// Go-to-line dialog state.
pub struct GoToLineState {
    visible: bool,
    /// Current input buffer (digits only).
    input: String,
    /// Optional selection over `input` as (start_byte, end_byte) with start <= end.
    /// `None` means no selection (a collapsed caret at `caret`).
    selection: Option<(usize, usize)>,
    /// Byte offset of the text insertion point into `input` (`0..=input.len()`,
    /// always on a char boundary). Where typed text is inserted and where the
    /// blinking caret is drawn when no selection is active.
    caret: usize,
    /// Blink phase: `true` paints the caret bar, `false` hides it. Toggled by
    /// the blink task; forced to `true` on any caret-moving or editing action so
    /// the caret shows solid immediately, then resumes blinking.
    caret_on: bool,
    /// Guard so at most one blink task runs at a time. Set when the task starts,
    /// cleared when it exits (on the dialog being hidden).
    blinking: bool,
    /// Byte offset where the current mouse-drag selection started. `None` means
    /// no drag is in progress. Cleared on mouse_up and on close.
    drag_anchor: Option<usize>,
    focus_handle: FocusHandle,
}

impl GoToLineState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        Self {
            visible: false,
            input: String::new(),
            selection: None,
            caret: 0,
            caret_on: true,
            blinking: false,
            drag_anchor: None,
            focus_handle: cx.focus_handle(),
        }
    }

    /// Show the dialog and take focus.
    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.visible = true;
        self.input.clear();
        self.selection = None;
        self.caret = 0;
        self.caret_on = true;
        self.drag_anchor = None;
        self.focus_handle.focus(window);
        self.start_blink(cx);
        cx.notify();
    }

    /// Close without jumping.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.visible = false;
        self.input.clear();
        self.selection = None;
        self.caret = 0;
        self.drag_anchor = None;
        cx.emit(GoToLineEvent::Closed);
        cx.notify();
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Drive the blinking caret. Toggles `caret_on` every 500ms while the dialog
    /// is visible and re-renders; exits (and clears `blinking`) once it's hidden.
    /// The `blinking` guard keeps a single task alive across repeated open calls.
    /// Caret visibility is additionally gated on focus at render time.
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

    fn submit(&mut self, cx: &mut Context<Self>) {
        if let Ok(line) = self.input.trim().parse::<u64>() {
            if line > 0 {
                cx.emit(GoToLineEvent::Jump(line));
            }
        }
        self.close(cx);
    }

    /// Replace the current selection (or insert at the caret) with `text`, then
    /// collapse the selection and place the caret just past the inserted text.
    fn replace_selection_with(&mut self, text: &str) {
        match self.selection {
            Some((s, e)) if s < e => {
                self.input.replace_range(s..e, text);
                self.caret = s + text.len();
            }
            _ => {
                self.input.insert_str(self.caret, text);
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
        while p > 0 && !self.input.is_char_boundary(p) {
            p -= 1;
        }
        p
    }

    /// Byte offset of the char boundary after `pos` (or `pos` if at the end).
    fn next_boundary(&self, pos: usize) -> usize {
        let len = self.input.len();
        if pos >= len {
            return len;
        }
        let mut p = pos + 1;
        while p < len && !self.input.is_char_boundary(p) {
            p += 1;
        }
        p
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = &keystroke.modifiers;
        let key = keystroke.key.as_str();

        // Intercept Cmd/Ctrl + A / C / V / X so they edit the input rather than
        // firing global actions. GPUI's `secondary-` modifier resolves to
        // `platform` on macOS and `control` on Linux/Windows — treat both.
        let editor_mod =
            (modifiers.platform || modifiers.control) && !modifiers.alt && !modifiers.shift;
        if editor_mod {
            match key {
                "a" => {
                    if !self.input.is_empty() {
                        self.selection = Some((0, self.input.len()));
                        self.caret = self.input.len();
                        cx.notify();
                    }
                    cx.stop_propagation();
                    return;
                }
                "c" => {
                    let text = match self.selection {
                        Some((s, e)) if s < e => self.input[s..e].to_string(),
                        _ => self.input.clone(),
                    };
                    if !text.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                    cx.stop_propagation();
                    return;
                }
                "x" => {
                    if let Some((s, e)) = self.selection.filter(|(s, e)| s < e) {
                        let text = self.input[s..e].to_string();
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        self.input.replace_range(s..e, "");
                        self.selection = None;
                        self.caret = s;
                        self.caret_on = true;
                        cx.notify();
                    }
                    cx.stop_propagation();
                    return;
                }
                "v" => {
                    if let Some(item) = cx.read_from_clipboard() {
                        if let Some(text) = item.text() {
                            // Numeric field: keep only ASCII digits.
                            let sanitized: String =
                                text.chars().filter(|c| c.is_ascii_digit()).collect();
                            if !sanitized.is_empty() {
                                self.replace_selection_with(&sanitized);
                                cx.notify();
                            }
                        }
                    }
                    cx.stop_propagation();
                    return;
                }
                _ => {}
            }
        }

        // Let other modifier combos fall through to their global bindings.
        if modifiers.platform || modifiers.control || modifiers.alt {
            return;
        }

        match key {
            "backspace" => {
                if let Some((s, e)) = self.selection.filter(|(s, e)| s < e) {
                    self.input.replace_range(s..e, "");
                    self.selection = None;
                    self.caret = s;
                    self.caret_on = true;
                    cx.notify();
                } else if self.caret > 0 {
                    let prev = self.prev_boundary(self.caret);
                    self.input.replace_range(prev..self.caret, "");
                    self.caret = prev;
                    self.caret_on = true;
                    cx.notify();
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
                self.caret = self.input.len();
                self.caret_on = true;
                cx.notify();
            }
            "enter" => {
                self.submit(cx);
            }
            // "escape" closes locally so the dialog disappears immediately.
            "escape" => {
                self.close(cx);
            }
            _ => {
                if let Some(ch) = keystroke.key_char.as_deref() {
                    // Only accept digits — this is a numeric input.
                    if !ch.is_empty() && ch.chars().all(|c| c.is_ascii_digit()) {
                        self.replace_selection_with(ch);
                        cx.notify();
                    }
                }
            }
        }
    }

    /// Render the modal. Returns `None` when hidden.
    pub fn render_dialog(
        &mut self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.visible {
            return None;
        }

        // Caret is painted only when the dialog holds keyboard focus, the blink
        // phase is "on", and there's no active selection. Clicking elsewhere
        // blurs the focus handle, which hides the caret.
        let show_caret =
            self.focus_handle.is_focused(window) && self.caret_on && self.selection.is_none();
        let caret_byte = self.caret;

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

        // The input field: render each character of the input as its own div so
        // mouse_down/mouse_move can hit-test against byte offsets — that's how
        // drag-to-select stays accurate without a custom text shaper.
        let input_field = div()
            .h(px(28.0))
            .px_2()
            .py_1()
            .flex()
            .items_center()
            .bg(theme.current_line)
            .border_1()
            .border_color(theme.selection)
            .rounded_sm()
            .text_color(if self.input.is_empty() {
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

        let input_field = if self.input.is_empty() {
            // Empty state: caret first (when focused) followed by the dimmed
            // placeholder, so the freshly-opened box looks ready for input.
            input_field.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .flex_grow()
                    .child(caret_el())
                    .child(div().child("Enter line number…"))
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
                .input
                .char_indices()
                .map(|(i, c)| (i, i + c.len_utf8(), c.to_string()))
                .collect();
            let end_pos = self.input.len();

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

            // Trailing zone fills the remainder of the field so clicks past the
            // last character anchor at end-of-input; carries the end caret.
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

            input_field.child(div().flex().flex_row().items_center().w_full().children(spans))
        };

        Some(
            div()
                .key_context("GoToLine")
                .track_focus(&self.focus_handle)
                .on_key_down(cx.listener(Self::on_key_down))
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba(0x00000080))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .w(px(320.0))
                        .p_4()
                        .bg(theme.background)
                        .border_1()
                        .border_color(theme.selection)
                        .rounded_lg()
                        .shadow_lg()
                        .child(
                            div()
                                .text_color(theme.foreground)
                                .child("Go to line"),
                        )
                        .child(input_field)
                        .child(
                            div()
                                .text_color(theme.line_number)
                                .text_sm()
                                .child("Enter to jump • Esc to cancel"),
                        ),
                )
                .into_any(),
        )
    }
}

impl Focusable for GoToLineState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<GoToLineEvent> for GoToLineState {}
