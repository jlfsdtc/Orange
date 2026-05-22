//! Go-to-line dialog: a small modal that captures a line number and emits
//! a jump event when the user presses Enter.
//!
//! Behaviour mirrors `QuickFindState`: while visible, the dialog owns
//! keyboard focus; digits are appended to the input buffer, backspace pops,
//! Enter parses the buffer and emits `GoToLineEvent::Jump(line)`, and the
//! global `escape` binding closes the dialog.

use gpui::*;

use crate::theme::Theme;

/// Events emitted by `GoToLineState`.
#[derive(Clone, Debug)]
pub enum GoToLineEvent {
    /// The parent should scroll to and select this 1-based line number.
    Jump(u64),
}

/// Go-to-line dialog state.
pub struct GoToLineState {
    visible: bool,
    input: String,
    focus_handle: FocusHandle,
}

impl GoToLineState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        Self {
            visible: false,
            input: String::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    /// Show the dialog and take focus.
    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.visible = true;
        self.input.clear();
        self.focus_handle.focus(window);
        cx.notify();
    }

    /// Close without jumping.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.visible = false;
        self.input.clear();
        cx.notify();
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if let Ok(line) = self.input.trim().parse::<u64>() {
            if line > 0 {
                cx.emit(GoToLineEvent::Jump(line));
            }
        }
        self.close(cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = &keystroke.modifiers;

        if modifiers.platform || modifiers.control || modifiers.alt {
            return;
        }

        match keystroke.key.as_str() {
            "backspace" => {
                if self.input.pop().is_some() {
                    cx.notify();
                }
            }
            "enter" => {
                self.submit(cx);
            }
            "escape" => {
                // Let CloseFind global / parent close handler bubble; we
                // also close locally so the dialog disappears immediately.
                self.close(cx);
            }
            _ => {
                if let Some(ch) = keystroke.key_char.as_deref() {
                    // Only accept digits — this is a numeric input.
                    if ch.chars().all(|c| c.is_ascii_digit()) && !ch.is_empty() {
                        self.input.push_str(ch);
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
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.visible {
            return None;
        }

        let display = if self.input.is_empty() {
            "Enter line number…".to_string()
        } else {
            format!("{}\u{2502}", self.input)
        };
        let text_color = if self.input.is_empty() {
            theme.line_number
        } else {
            theme.foreground
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
                        .child(
                            div()
                                .h(px(28.0))
                                .px_2()
                                .py_1()
                                .bg(theme.current_line)
                                .border_1()
                                .border_color(theme.selection)
                                .rounded_sm()
                                .text_color(text_color)
                                .child(display),
                        )
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
