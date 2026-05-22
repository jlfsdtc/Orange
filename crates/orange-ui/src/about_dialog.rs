//! About dialog: a small modal that shows the application name, version,
//! source repository, and license. Closing the dialog re-focuses the parent
//! window via `AboutDialogEvent::Closed` so global shortcuts stay live.

use gpui::*;

use crate::theme::Theme;

// Actions for the about dialog.
actions!(orange, [OpenAbout, CloseAbout]);

/// Events emitted by `AboutDialogState`. Mirrors the options dialog pattern
/// so the parent window can reclaim focus when the modal goes away.
#[derive(Debug, Clone, Copy)]
pub enum AboutDialogEvent {
    Closed,
}

const APP_NAME: &str = "Orange";
const APP_DESCRIPTION: &str = "A fast log file viewer";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_LICENSE: &str = env!("CARGO_PKG_LICENSE");
const APP_REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

pub struct AboutDialogState {
    visible: bool,
    focus_handle: FocusHandle,
}

impl EventEmitter<AboutDialogEvent> for AboutDialogState {}

impl AboutDialogState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        Self {
            visible: false,
            focus_handle: cx.focus_handle(),
        }
    }

    /// Show the dialog and take focus so Escape resolves locally.
    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.visible = true;
        self.focus_handle.focus(window);
        cx.notify();
    }

    /// Close and notify the parent window so it can reclaim focus from
    /// our now-unrendered focus_handle.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if !self.visible {
            return;
        }
        self.visible = false;
        cx.emit(AboutDialogEvent::Closed);
        cx.notify();
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        let mods = &ks.modifiers;
        if ks.key == "escape" && !mods.platform && !mods.control && !mods.alt && !mods.shift {
            self.close(cx);
        }
    }

    pub fn render_dialog(
        &mut self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.visible {
            return None;
        }

        let body = div()
            .flex()
            .flex_col()
            .gap_3()
            .px_4()
            .py_4()
            .child(self.render_row("Name", APP_NAME, theme))
            .child(self.render_row("Version", APP_VERSION, theme))
            .child(self.render_repo_row(theme, cx))
            .child(self.render_row("License", APP_LICENSE, theme))
            .child(
                div()
                    .text_color(theme.line_number)
                    .text_sm()
                    .child(APP_DESCRIPTION),
            );

        let header = div()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(theme.selection)
            .text_color(theme.foreground)
            .text_lg()
            .child("About Orange");

        let buttons = div()
            .flex()
            .justify_end()
            .gap_2()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(theme.selection)
            .child(
                div()
                    .id("about-close")
                    .px_3()
                    .py_1()
                    .bg(theme.selection)
                    .rounded_md()
                    .text_color(theme.foreground)
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _event, _window, cx| this.close(cx)))
                    .child("Close"),
            );

        Some(
            div()
                .key_context("AboutDialog")
                .track_focus(&self.focus_handle)
                .on_key_down(cx.listener(Self::on_key_down))
                .absolute()
                .inset_0()
                .bg(Hsla { h: 0.0, s: 0.0, l: 0.0, a: 0.5 })
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(440.0))
                        .bg(theme.background)
                        .border_1()
                        .border_color(theme.selection)
                        .rounded_lg()
                        .shadow_lg()
                        .child(header)
                        .child(body)
                        .child(buttons),
                )
                .into_any(),
        )
    }

    fn render_row(&self, label: &str, value: &str, theme: Theme) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_color(theme.line_number)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_color(theme.foreground)
                    .child(value.to_string()),
            )
            .into_any()
    }

    /// Repository row — the URL is rendered as a clickable link that opens
    /// the system browser via `cx.open_url`.
    fn render_repo_row(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(div().text_color(theme.line_number).child("Repository"))
            .child(
                div()
                    .id("about-repo-link")
                    .text_color(theme.search_current)
                    .cursor_pointer()
                    .child(APP_REPOSITORY)
                    .on_click(cx.listener(|_, _event, _window, cx| {
                        cx.open_url(APP_REPOSITORY);
                    })),
            )
            .into_any()
    }
}

impl Focusable for AboutDialogState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
