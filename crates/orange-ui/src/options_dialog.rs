//! Options dialog for application settings.
//!
//! Editable rows:
//!   - String   (Font): click to focus, type to edit, Tab/Enter/blur to commit.
//!   - Number   (Font size, Overview context): same flow but digits-only.
//!   - Boolean  (Dark theme, Follow file): click the row to toggle inline.
//!
//! Save persists the current options to disk via `Options::save()` and
//! closes the dialog. Cancel closes without saving — pending edits are
//! discarded and `load()` is called again next time the dialog opens.

use gpui::prelude::FluentBuilder;
use gpui::*;
use orange_settings::{Keymap, Options};

use crate::keymap::KNOWN_ACTIONS;
use crate::main_window::ToggleTheme;
use crate::theme::{FontSettings, Theme};

// Actions for options dialog.
actions!(orange, [OpenOptions, CloseOptions]);

/// Events emitted by the options dialog. Subscribers (currently the main
/// window) react to these without coupling to the dialog's internal state.
#[derive(Debug, Clone, Copy)]
pub enum OptionsDialogEvent {
    /// User saved the keymap; the file on disk is fresh and the live key
    /// bindings should be re-applied. Subscriber should dispatch
    /// `orange_app::ApplyKeymap` (or otherwise re-bind from disk).
    KeymapSaved,
    /// Dialog was dismissed (Cancel/Save). The dialog's focus_handle was
    /// active while it was visible; the subscriber should re-focus its own
    /// handle so global shortcuts (incl. OpenOptions) resolve again.
    Closed,
}

/// Which text/number field currently captures keystrokes, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditField {
    Font,
    FontSize,
    OverviewContext,
}

/// Sidebar tabs in the options dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OptionsTab {
    General,
    Shortcuts,
}

impl OptionsTab {
    fn label(self) -> &'static str {
        match self {
            OptionsTab::General => "General",
            OptionsTab::Shortcuts => "Keyboard shortcuts",
        }
    }

    fn id(self) -> &'static str {
        match self {
            OptionsTab::General => "tab-general",
            OptionsTab::Shortcuts => "tab-shortcuts",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            OptionsTab::General => "\u{2699}",
            OptionsTab::Shortcuts => "\u{2328}",
        }
    }
}

const TABS: &[OptionsTab] = &[OptionsTab::General, OptionsTab::Shortcuts];

/// Options dialog state.
pub struct OptionsDialogState {
    /// Whether the dialog is visible.
    visible: bool,
    /// Current options being edited (uncommitted until Save).
    options: Options,
    /// Current keymap being edited (uncommitted until Save).
    keymap: Keymap,
    /// Action whose key is currently being captured (if any).
    capturing: Option<String>,
    /// Message describing the most recent binding conflict (e.g. "Removed:
    /// secondary-f from ToggleQuickFind"). Cleared when the user opens or
    /// closes the dialog, or when the next capture starts.
    conflict_notice: Option<String>,
    /// Field currently receiving keystrokes.
    focused_field: Option<EditField>,
    /// Currently selected sidebar tab.
    tab: OptionsTab,
    /// Keyboard focus handle for the whole dialog.
    focus_handle: FocusHandle,
}

impl EventEmitter<OptionsDialogEvent> for OptionsDialogState {}

impl OptionsDialogState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        Self {
            visible: false,
            options: Options::default(),
            keymap: Keymap::load_or_default(),
            capturing: None,
            conflict_notice: None,
            focused_field: None,
            tab: OptionsTab::General,
            focus_handle: cx.focus_handle(),
        }
    }

    /// Load options and the keymap from config.
    pub fn load(&mut self) {
        if let Ok(opts) = Options::load() {
            self.options = opts;
        }
        self.keymap = Keymap::load_or_default();
    }

    /// Toggle visibility. On open, re-read the on-disk options so we never
    /// edit a stale snapshot.
    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.visible = !self.visible;
        if self.visible {
            self.load();
            self.focused_field = None;
            self.capturing = None;
            self.conflict_notice = None;
            self.tab = OptionsTab::General;
        }
        cx.notify();
    }

    /// Close without saving. Emits `Closed` so the parent window can reclaim
    /// keyboard focus from the dialog's now-unrendered focus_handle —
    /// otherwise global shortcuts (incl. OpenOptions) stay dead until the
    /// user clicks the main window.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.visible = false;
        self.focused_field = None;
        self.capturing = None;
        self.conflict_notice = None;
        self.tab = OptionsTab::General;
        cx.emit(OptionsDialogEvent::Closed);
        cx.notify();
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn options(&self) -> &Options {
        &self.options
    }

    pub fn save(&self) -> anyhow::Result<()> {
        self.options.save()
    }

    fn focus_field(&mut self, field: EditField, window: &mut Window, cx: &mut Context<Self>) {
        self.focused_field = Some(field);
        self.focus_handle.focus(window);
        cx.notify();
    }

    fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Shortcut capture mode takes priority over text-field editing.
        if let Some(action) = self.capturing.clone() {
            let ks = &event.keystroke;
            let mods = &ks.modifiers;
            // Bare Escape cancels capture without binding anything.
            if ks.key == "escape" && !mods.platform && !mods.control && !mods.alt && !mods.shift {
                self.capturing = None;
                cx.notify();
                return;
            }
            if let Some(key) = keystroke_to_key_string(ks) {
                // Replace the action's current binding, then store the new one.
                // set_binding additionally reports if another action was bumped
                // off the same key so we can surface the conflict.
                self.keymap.clear_binding(&action);
                let displaced = self.keymap.set_binding(&key, &action);
                self.conflict_notice = displaced.map(|other| {
                    format!("Removed: {} from {other}", format_key_for_display(&key))
                });
                self.capturing = None;
                cx.notify();
            }
            return;
        }

        let Some(field) = self.focused_field else {
            return;
        };
        let keystroke = &event.keystroke;
        let modifiers = &keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return;
        }

        match keystroke.key.as_str() {
            "backspace" => {
                match field {
                    EditField::Font => {
                        self.options.main_font.pop();
                    }
                    EditField::FontSize => {
                        // Edit on integer-string form: backspace, re-parse.
                        let mut s = (self.options.main_font_size as u32).to_string();
                        s.pop();
                        self.options.main_font_size = s.parse::<u32>().unwrap_or(0) as f32;
                    }
                    EditField::OverviewContext => {
                        let mut s = self.options.overview_context.to_string();
                        s.pop();
                        self.options.overview_context = s.parse::<u32>().unwrap_or(0);
                    }
                }
                cx.notify();
            }
            "enter" | "tab" => {
                // Commit by simply unfocusing — value is already in self.options.
                self.focused_field = None;
                cx.notify();
            }
            _ => {
                let Some(ch) = keystroke.key_char.as_deref() else {
                    return;
                };
                if ch.is_empty() || ch.chars().any(|c| c.is_control()) {
                    return;
                }
                match field {
                    EditField::Font => {
                        self.options.main_font.push_str(ch);
                    }
                    EditField::FontSize => {
                        if ch.chars().all(|c| c.is_ascii_digit()) {
                            let current = self.options.main_font_size as u32;
                            let mut s = current.to_string();
                            if current == 0 {
                                s.clear();
                            }
                            s.push_str(ch);
                            self.options.main_font_size =
                                s.parse::<u32>().unwrap_or(current) as f32;
                        }
                    }
                    EditField::OverviewContext => {
                        if ch.chars().all(|c| c.is_ascii_digit()) {
                            let mut s = self.options.overview_context.to_string();
                            if self.options.overview_context == 0 {
                                s.clear();
                            }
                            s.push_str(ch);
                            self.options.overview_context =
                                s.parse::<u32>().unwrap_or(self.options.overview_context);
                        }
                    }
                }
                cx.notify();
            }
        }
    }

    /// Render the dialog overlay. The body is split into title / content /
    /// buttons helpers because GPUI's view-tree macros expand deeply, and a
    /// single inline chain pushes the test target's `#[test]` expansion past
    /// the crate's `recursion_limit = "1024"`.
    pub fn render_dialog(
        &mut self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.visible {
            return None;
        }

        let content = self.render_content(theme, cx);
        let buttons = self.render_buttons(theme, cx);

        Some(
            div()
                .key_context("OptionsDialog")
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
                        .w(px(720.0))
                        .bg(theme.background)
                        .border_1()
                        .border_color(theme.selection)
                        .rounded_lg()
                        .shadow_lg()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .px_4()
                                .py_3()
                                .border_b_1()
                                .border_color(theme.selection)
                                .child(
                                    div()
                                        .text_color(theme.foreground)
                                        .text_lg()
                                        .child("Settings"),
                                ),
                        )
                        .child(content)
                        .child(buttons),
                )
                .into_any(),
        )
    }

    /// Body: sidebar + content pane. Dispatches to whichever tab pane is
    /// currently selected. Extracted from `render_dialog` to keep that chain
    /// shallow enough for macro expansion.
    fn render_content(&mut self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let sidebar = self.render_sidebar(theme, cx);
        let pane = match self.tab {
            OptionsTab::General => self.render_general_pane(theme, cx),
            OptionsTab::Shortcuts => self.render_shortcuts_pane(theme, cx),
        };

        div()
            .flex()
            .flex_row()
            .min_h(px(380.0))
            .child(sidebar)
            .child(div().flex_1().px_4().py_3().child(pane))
            .into_any()
    }

    /// Vertical tab list on the left edge of the dialog. Each row is
    /// `icon  label`; the selected tab uses `theme.selection` as background.
    fn render_sidebar(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let current = self.tab;
        let rows: Vec<AnyElement> = TABS
            .iter()
            .map(|&tab| {
                let selected = tab == current;
                div()
                    .id(ElementId::Name(tab.id().into()))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .cursor_pointer()
                    .when(selected, |this| this.bg(theme.selection))
                    .text_color(theme.foreground)
                    .child(div().w(px(16.0)).child(tab.icon()))
                    .child(tab.label())
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.tab = tab;
                        cx.notify();
                    }))
                    .into_any()
            })
            .collect();

        div()
            .w(px(220.0))
            .py_3()
            .px_2()
            .border_r_1()
            .border_color(theme.selection)
            .flex()
            .flex_col()
            .gap_1()
            .children(rows)
            .into_any()
    }

    /// General tab: editable text/number fields, theme toggle, follow-file.
    fn render_general_pane(&mut self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let dark = self.options.dark_theme;
        let follow = self.options.follow_file;
        let font = self.options.main_font.clone();
        let font_size = self.options.main_font_size;
        let overview = self.options.overview_context;
        let focused = self.focused_field;

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.render_text_row(
                "Font",
                EditField::Font,
                &font,
                focused == Some(EditField::Font),
                theme,
                cx,
            ))
            .child(self.render_number_row(
                "Font Size",
                EditField::FontSize,
                font_size as u32 as i64,
                focused == Some(EditField::FontSize),
                theme,
                cx,
            ))
            .child(self.render_theme_toggle_row(dark, theme))
            .child(self.render_bool_row(
                "Follow File",
                follow,
                theme,
                cx.listener(|this, _event, _window, cx| {
                    this.options.follow_file = !this.options.follow_file;
                    cx.notify();
                }),
            ))
            .child(self.render_number_row(
                "Overview Context",
                EditField::OverviewContext,
                overview as i64,
                focused == Some(EditField::OverviewContext),
                theme,
                cx,
            ))
            .into_any()
    }

    /// Shortcuts tab: scrollable action list + the optional conflict notice.
    /// The notice is generated by this flow, so it lives on this pane only.
    fn render_shortcuts_pane(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let notice = self.conflict_notice.clone();
        let rows: Vec<AnyElement> = KNOWN_ACTIONS
            .iter()
            .map(|&action| self.render_shortcut_row(action, theme, cx))
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .id("shortcut-list-scroll")
                    .max_h(px(320.0))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(rows),
            )
            .when_some(notice, |parent, msg| {
                parent.child(div().text_color(theme.search_current).child(msg))
            })
            .into_any()
    }

    /// Cancel + Save buttons. Save persists Options, persists Keymap, pushes
    /// the new font settings to the global, then emits `KeymapSaved` so the
    /// main window can re-bind live without a restart.
    fn render_buttons(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .justify_end()
            .gap_2()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(theme.selection)
            .child(
                div()
                    .id("opts-cancel")
                    .px_3()
                    .py_1()
                    .bg(theme.selection)
                    .rounded_md()
                    .text_color(theme.foreground)
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _event, _window, cx| this.close(cx)))
                    .child("Cancel"),
            )
            .child(
                div()
                    .id("opts-save")
                    .px_3()
                    .py_1()
                    .bg(theme.search_current)
                    .rounded_md()
                    .text_color(theme.background)
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        if let Err(e) = this.save() {
                            tracing::warn!("failed to save options: {e}");
                        }
                        if let Err(e) = this.keymap.save() {
                            tracing::warn!("failed to save keymap: {e}");
                        }
                        cx.set_global(FontSettings::from_options(&this.options));
                        cx.emit(OptionsDialogEvent::KeymapSaved);
                        this.close(cx);
                    }))
                    .child("Save"),
            )
            .into_any()
    }

    fn render_text_row(
        &self,
        label: &str,
        field: EditField,
        value: &str,
        is_focused: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let display = if is_focused {
            format!("{}\u{2502}", value)
        } else if value.is_empty() {
            "(click to set)".to_string()
        } else {
            value.to_string()
        };
        let row_id = ElementId::Name(format!("opt-{}", label).into());
        div()
            .id(row_id)
            .flex()
            .items_center()
            .justify_between()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _event, window, cx| {
                this.focus_field(field, window, cx);
            }))
            .child(div().text_color(theme.line_number).child(label.to_string()))
            .child(
                div()
                    .min_w(px(160.0))
                    .px_2()
                    .py_1()
                    .bg(theme.current_line)
                    .border_1()
                    .border_color(if is_focused { theme.search_current } else { theme.selection })
                    .rounded_sm()
                    .text_color(theme.foreground)
                    .child(display),
            )
            .into_any()
    }

    fn render_number_row(
        &self,
        label: &str,
        field: EditField,
        value: i64,
        is_focused: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let display = if is_focused {
            format!("{}\u{2502}", value)
        } else {
            value.to_string()
        };
        let row_id = ElementId::Name(format!("opt-{}", label).into());
        div()
            .id(row_id)
            .flex()
            .items_center()
            .justify_between()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _event, window, cx| {
                this.focus_field(field, window, cx);
            }))
            .child(div().text_color(theme.line_number).child(label.to_string()))
            .child(
                div()
                    .min_w(px(80.0))
                    .px_2()
                    .py_1()
                    .bg(theme.current_line)
                    .border_1()
                    .border_color(if is_focused { theme.search_current } else { theme.selection })
                    .rounded_sm()
                    .text_color(theme.foreground)
                    .child(display),
            )
            .into_any()
    }

    fn render_bool_row(
        &self,
        label: &str,
        value: bool,
        theme: Theme,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let row_id = ElementId::Name(format!("opt-{}", label).into());
        let chip = if value { "On" } else { "Off" };
        let bg = if value { theme.search_current } else { theme.current_line };
        let fg = if value { theme.background } else { theme.foreground };
        div()
            .id(row_id)
            .flex()
            .items_center()
            .justify_between()
            .cursor_pointer()
            .on_click(on_click)
            .child(div().text_color(theme.line_number).child(label.to_string()))
            .child(
                div()
                    .min_w(px(60.0))
                    .px_2()
                    .py_1()
                    .bg(bg)
                    .border_1()
                    .border_color(theme.selection)
                    .rounded_sm()
                    .text_color(fg)
                    .child(chip),
            )
            .into_any()
    }

    fn render_shortcut_row(
        &self,
        action: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let capturing = self.capturing.as_deref() == Some(action);
        let current_key = self
            .keymap
            .bindings
            .iter()
            .find(|b| b.action == action)
            .map(|b| b.key.clone());
        let display = if capturing {
            "Press a key…".to_string()
        } else {
            current_key
                .as_deref()
                .map(format_key_for_display)
                .unwrap_or_else(|| "(unbound)".to_string())
        };
        let row_id = ElementId::Name(format!("shortcut-{}", action).into());
        let unbind_id = ElementId::Name(format!("unbind-{}", action).into());
        let action_for_capture = action.to_string();
        let action_for_unbind = action.to_string();

        // Chip + optional unbind button. Build conditionally so we don't show
        // an `×` next to an already-unbound action.
        let mut chips = div().flex().items_center().gap_2().child(
            div()
                .min_w(px(160.0))
                .px_2()
                .py_1()
                .bg(theme.current_line)
                .border_1()
                .border_color(if capturing {
                    theme.search_current
                } else {
                    theme.selection
                })
                .rounded_sm()
                .text_color(theme.foreground)
                .child(display),
        );
        if current_key.is_some() {
            chips = chips.child(
                div()
                    .id(unbind_id)
                    .px_2()
                    .py_1()
                    .bg(theme.selection)
                    .rounded_sm()
                    .text_color(theme.foreground)
                    .cursor_pointer()
                    .child("×")
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.keymap.clear_binding(&action_for_unbind);
                        this.conflict_notice = None;
                        cx.notify();
                    })),
            );
        }

        div()
            .id(row_id)
            .flex()
            .items_center()
            .justify_between()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _event, window, cx| {
                this.capturing = Some(action_for_capture.clone());
                this.conflict_notice = None;
                this.focus_handle.focus(window);
                cx.notify();
            }))
            .child(div().text_color(theme.foreground).child(action))
            .child(chips)
            .into_any()
    }

    /// Render the "Dark Theme" row. Clicking the value chip dispatches
    /// `ToggleTheme`, which MainWindow listens for and updates persisted
    /// options out-of-band — we mirror the flip into our local copy so the
    /// dialog reflects the change immediately.
    fn render_theme_toggle_row(&self, dark: bool, theme: Theme) -> AnyElement {
        let value = if dark { "Dark" } else { "Light" };
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(div().text_color(theme.line_number).child("Theme"))
            .child(
                div()
                    .id("theme-toggle")
                    .px_2()
                    .py_1()
                    .bg(theme.current_line)
                    .border_1()
                    .border_color(theme.selection)
                    .rounded_sm()
                    .text_color(theme.foreground)
                    .cursor_pointer()
                    .child(format!("{}  (click to toggle)", value))
                    .on_click(|_event, window, cx| {
                        window.dispatch_action(Box::new(ToggleTheme), cx);
                    }),
            )
            .into_any()
    }
}

impl Focusable for OptionsDialogState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Render a stored keymap key string (e.g. `"secondary-shift-t"`) using
/// platform-native modifier names so the shortcuts UI matches what the user
/// sees on the physical keyboard. On macOS the `secondary-` prefix shows as
/// `Command` and a standalone `ctrl-` shows as `Control`; on Linux/Windows
/// both render as `Ctrl`. The non-modifier tail is normalized too
/// (`f4` → `F4`, `pageup` → `PageUp`, single letters uppercased).
fn format_key_for_display(key: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut rest = key;
    loop {
        let lower = rest.to_ascii_lowercase();
        let matched = if let Some(tail) = lower.strip_prefix("secondary-") {
            Some((secondary_label(), rest.len() - tail.len()))
        } else if let Some(tail) = lower.strip_prefix("cmd-") {
            Some((secondary_label(), rest.len() - tail.len()))
        } else if let Some(tail) = lower.strip_prefix("platform-") {
            Some((secondary_label(), rest.len() - tail.len()))
        } else if let Some(tail) = lower.strip_prefix("ctrl-") {
            Some((ctrl_label(), rest.len() - tail.len()))
        } else if let Some(tail) = lower.strip_prefix("control-") {
            Some((ctrl_label(), rest.len() - tail.len()))
        } else if let Some(tail) = lower.strip_prefix("alt-") {
            Some((alt_label(), rest.len() - tail.len()))
        } else if let Some(tail) = lower.strip_prefix("shift-") {
            Some(("Shift".to_string(), rest.len() - tail.len()))
        } else {
            lower
                .strip_prefix("fn-")
                .map(|tail| ("Fn".to_string(), rest.len() - tail.len()))
        };
        match matched {
            Some((label, advance)) => {
                parts.push(label);
                rest = &rest[advance..];
            }
            None => break,
        }
    }
    if !rest.is_empty() {
        parts.push(format_key_name(rest));
    }
    parts.join("+")
}

fn secondary_label() -> String {
    if cfg!(target_os = "macos") { "Command".to_string() } else { "Ctrl".to_string() }
}

fn ctrl_label() -> String {
    if cfg!(target_os = "macos") { "Control".to_string() } else { "Ctrl".to_string() }
}

fn alt_label() -> String {
    if cfg!(target_os = "macos") { "Option".to_string() } else { "Alt".to_string() }
}

fn format_key_name(key: &str) -> String {
    match key.to_ascii_lowercase().as_str() {
        "pageup" => "PageUp".to_string(),
        "pagedown" => "PageDown".to_string(),
        "home" => "Home".to_string(),
        "end" => "End".to_string(),
        "up" => "Up".to_string(),
        "down" => "Down".to_string(),
        "left" => "Left".to_string(),
        "right" => "Right".to_string(),
        "escape" => "Esc".to_string(),
        "enter" | "return" => "Enter".to_string(),
        "tab" => "Tab".to_string(),
        "space" => "Space".to_string(),
        "backspace" => "Backspace".to_string(),
        "delete" => "Delete".to_string(),
        k if k.len() == 1 => k.to_ascii_uppercase(),
        k if k.starts_with('f')
            && k.len() > 1
            && k[1..].chars().all(|c| c.is_ascii_digit()) =>
        {
            format!("F{}", &k[1..])
        }
        _ => key.to_string(),
    }
}

/// Convert a captured `Keystroke` into a key string compatible with
/// `KeyBinding::new` and the on-disk keymap format. Returns `None` for
/// modifier-only presses (which the user hasn't finished committing yet).
///
/// `cmd` (mac platform) and `ctrl` (linux/windows control) both map to the
/// portable `secondary-` prefix, matching the rest of the keymap convention
/// so a captured shortcut on macOS reads the same as on Linux.
fn keystroke_to_key_string(ks: &Keystroke) -> Option<String> {
    // Skip lone modifier presses — the user is still composing the chord.
    if matches!(
        ks.key.as_str(),
        "" | "shift" | "control" | "alt" | "platform" | "function"
    ) {
        return None;
    }
    let mut out = String::new();
    if ks.modifiers.platform || ks.modifiers.control {
        out.push_str("secondary-");
    }
    if ks.modifiers.alt {
        out.push_str("alt-");
    }
    if ks.modifiers.shift {
        out.push_str("shift-");
    }
    out.push_str(&ks.key);
    Some(out)
}
