//! Main application window.
//!
//! Provides the top-level layout with tabs, log view, filtered view,
//! overview, quick find, and status bar.

use gpui::*;

use crate::about_dialog::{AboutDialogEvent, AboutDialogState, CloseAbout, OpenAbout};
use crate::components::tab_bar::{TabBarEvent, TabBarState};
use crate::filtered_view::{FilteredViewState, ToggleFilteredView};
use crate::go_to_line::{GoToLineEvent, GoToLineState};
use crate::highlighter::HighlighterSet;
use crate::log_view::{LogViewEvent, LogViewState, ToggleTailMode};
use crate::keymap::ApplyKeymap;
use crate::options_dialog::{CloseOptions, OpenOptions, OptionsDialogEvent, OptionsDialogState};
use crate::overview::{MinimapEvent, OverviewState};
use crate::predefined_filters::{PredefinedFiltersState, ToggleFilterPanel};
use crate::quick_find::{
    CloseFind, FindNext, FindPrevious, QuickFindEvent, QuickFindState, ToggleQuickFind,
};
use crate::scratchpad::{ScratchpadState, ToggleScratchpad};
use crate::session_widget::{LoadSession, SaveSession, SessionWidgetState};
use crate::theme::{FontSettings, MinimapSettings, Theme};

// Main window actions.
actions!(
    orange,
    [
        OpenFile,
        ScrollToTop,
        ScrollToBottom,
        PageUp,
        PageDown,
        LineUp,
        LineDown,
        CloseTab,
        GoToLineDialog,
        ToggleTheme,
        ToggleMinimap,
        IncreaseFontSize,
        DecreaseFontSize,
        ResetFontSize,
        CopySelection,
        CopySelectedLines,
        SelectTab1,
        SelectTab2,
        SelectTab3,
        SelectTab4,
        SelectTab5,
        SelectTab6,
        SelectTab7,
        SelectTab8,
        SelectTab9
    ]
);

/// A single tab's data.
struct TabData {
    log_view: Entity<LogViewState>,
    filtered: Entity<FilteredViewState>,
    overview: Entity<OverviewState>,
}

/// Main window state.
pub struct MainWindowState {
    /// Tab data.
    tabs: Vec<TabData>,
    /// Active tab index.
    active_tab: usize,
    /// Tab bar.
    tab_bar: Entity<TabBarState>,
    /// Quick find bar.
    quick_find: Entity<QuickFindState>,
    /// Options dialog.
    options_dialog: Entity<OptionsDialogState>,
    /// About dialog.
    about_dialog: Entity<AboutDialogState>,
    /// Predefined filters.
    predefined_filters: Entity<PredefinedFiltersState>,
    /// Scratchpad panel.
    scratchpad: Entity<ScratchpadState>,
    /// Session manager dialog.
    session_widget: Entity<SessionWidgetState>,
    /// Go-to-line dialog.
    go_to_line: Entity<GoToLineState>,
    /// Highlighter set. Reserved for the upcoming log_view ↔ highlighter
    /// integration; constructed eagerly so the field is ready when wiring
    /// lands.
    #[allow(dead_code)]
    highlighters: HighlighterSet,
    /// Loaded application options (used to persist theme + other prefs).
    options: orange_settings::Options,
    /// Status message.
    status: String,
    /// Window-relative position of the right-click context menu, or `None`
    /// when hidden. Owned by the window (not LogView) so the overlay can be
    /// rendered at the top level — that way `.absolute()` positioning is
    /// window-relative and `event.position` can be used directly.
    context_menu: Option<Point<Pixels>>,
    /// Focus anchor for the root div. Without this, GPUI's action dispatcher
    /// has no element in the focus chain and global shortcuts never fire.
    focus_handle: FocusHandle,
}

impl MainWindowState {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Load persisted options so we can pick the initial theme.
        let options = orange_settings::Options::load().unwrap_or_default();

        // Install the Theme global before any child Entity is constructed so
        // their `cx.observe_global::<Theme>` subscriptions resolve cleanly.
        cx.set_global(Theme::from_options_flag(options.dark_theme));
        cx.set_global(FontSettings::from_options(&options));
        cx.set_global(MinimapSettings::from_options(&options));

        // Repaint MainWindow itself when the Theme global changes.
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();

        let tab_bar = cx.new(|cx| TabBarState::new(cx));
        let quick_find = cx.new(|cx| QuickFindState::new(cx));
        let options_dialog = cx.new(|cx| OptionsDialogState::new(cx));
        let about_dialog = cx.new(|cx| AboutDialogState::new(cx));
        let predefined_filters = cx.new(|cx| PredefinedFiltersState::new(cx));
        let scratchpad = cx.new(|cx| ScratchpadState::new(cx));
        let session_widget = cx.new(|cx| SessionWidgetState::new(cx));
        let go_to_line = cx.new(|cx| GoToLineState::new(cx));

        // Scroll the active LogView to a match when QuickFind announces one.
        cx.subscribe(&quick_find, |this: &mut Self, _quick_find, event, cx| {
            match event {
                QuickFindEvent::JumpTo(line) => {
                    let line = *line;
                    this.tabs[this.active_tab].log_view.update(cx, |view, cx| {
                        view.select_line(Some(line), cx);
                        view.scroll_to_line(line, cx);
                    });
                }
            }
        })
        .detach();

        // Switch the active tab when the TabBar reports a click.
        cx.subscribe(&tab_bar, |this: &mut Self, _tab_bar, event, cx| match event {
            TabBarEvent::Selected(idx) => {
                let idx = *idx;
                if idx < this.tabs.len() {
                    this.active_tab = idx;
                    let data = this.tabs[idx].log_view.read(cx).log_data();
                    this.quick_find
                        .update(cx, |find, cx| find.set_log_data(data, cx));
                    cx.notify();
                }
            }
        })
        .detach();

        // Jump to the requested line when GoToLine submits.
        cx.subscribe(&go_to_line, |this: &mut Self, _gtl, event, cx| match event {
            GoToLineEvent::Jump(line_1based) => {
                // User-facing line numbers are 1-based; LogView is 0-based.
                let line = line_1based.saturating_sub(1);
                this.tabs[this.active_tab].log_view.update(cx, |view, cx| {
                    view.select_line(Some(line), cx);
                    view.scroll_to_line(line, cx);
                });
                this.status = format!("Jumped to line {}", line_1based);
                cx.notify();
            }
        })
        .detach();

        // MainWindow renders these child entities by calling `update(...)` on
        // them from its own render method, which does NOT create an automatic
        // subscription. Without these explicit observers, a `cx.notify()` from
        // inside QuickFind (or any other inline-rendered child) only marks
        // the child dirty — the parent never re-runs render, so the user
        // never sees state changes (e.g. typing in the find bar).
        cx.observe(&quick_find, |_, _, cx| cx.notify()).detach();
        cx.observe(&options_dialog, |_, _, cx| cx.notify()).detach();
        cx.observe(&about_dialog, |_, _, cx| cx.notify()).detach();
        // About dialog: reclaim focus on close so global shortcuts resolve
        // again. Same rationale as the options dialog subscription below.
        cx.subscribe_in(
            &about_dialog,
            window,
            |this: &mut Self, _dialog, event, window, _cx| match event {
                AboutDialogEvent::Closed => window.focus(&this.focus_handle),
            },
        )
        .detach();
        // KeymapSaved → re-bind keymap from disk. Closed → reclaim focus
        // from the dialog's focus_handle (it grabbed focus on open via
        // track_focus, and close() doesn't release it; without this step,
        // OpenOptions / other MainWindow-scoped actions wouldn't resolve
        // until the user clicked the main window).
        cx.subscribe_in(
            &options_dialog,
            window,
            |this: &mut Self, _dialog, event, window, cx| match event {
                OptionsDialogEvent::KeymapSaved => cx.dispatch_action(&ApplyKeymap),
                OptionsDialogEvent::Closed => window.focus(&this.focus_handle),
            },
        )
        .detach();
        cx.observe(&predefined_filters, |_, _, cx| cx.notify()).detach();
        cx.observe(&scratchpad, |_, _, cx| cx.notify()).detach();
        cx.observe(&session_widget, |_, _, cx| cx.notify()).detach();
        cx.observe(&go_to_line, |_, _, cx| cx.notify()).detach();
        cx.observe(&tab_bar, |_, _, cx| cx.notify()).detach();

        // Create initial tab
        let log_view = cx.new(|cx| LogViewState::new(cx));
        let filtered = cx.new(|cx| FilteredViewState::new(cx));
        let overview = cx.new(OverviewState::new);
        Self::subscribe_overview(&overview, cx);

        let tabs = vec![TabData {
            log_view,
            filtered,
            overview,
        }];

        let this = Self {
            tabs,
            active_tab: 0,
            tab_bar,
            quick_find,
            options_dialog,
            about_dialog,
            predefined_filters,
            scratchpad,
            session_widget,
            go_to_line,
            highlighters: HighlighterSet::with_defaults(),
            options,
            status: if cfg!(target_os = "macos") {
                "Ready - Press Cmd+O to open a file".to_string()
            } else {
                "Ready - Press Ctrl+O to open a file".to_string()
            },
            context_menu: None,
            focus_handle: cx.focus_handle(),
        };

        // Subscribe to the initial tab's log view so right-clicks raise the
        // context menu. Tabs created later wire this up in open_file_in_new_tab.
        Self::subscribe_log_view(&this.tabs[0].log_view, cx);
        this
    }

    /// Wire a log view's events into the window's overlay state.
    fn subscribe_log_view(log_view: &Entity<LogViewState>, cx: &mut Context<Self>) {
        cx.subscribe(log_view, |this, _, event, cx| match event {
            LogViewEvent::ShowContextMenu(pos) => {
                this.context_menu = Some(*pos);
                cx.notify();
            }
        })
        .detach();
    }

    /// Wire a minimap's ScrollTo events to the active tab's log view.
    fn subscribe_overview(overview: &Entity<OverviewState>, cx: &mut Context<Self>) {
        cx.subscribe(overview, |this, _, event, cx| match event {
            MinimapEvent::ScrollTo(line) => {
                let target = *line;
                let log_view = this.tabs[this.active_tab].log_view.clone();
                log_view.update(cx, |view, cx| {
                    view.scroll_to_line(target, cx);
                    view.select_line(Some(target), cx);
                });
            }
        })
        .detach();
    }

    /// Toggle the minimap strip on every tab. Session-only — preference is
    /// not persisted to disk, matching the existing `OverviewState` default.
    pub fn toggle_minimap(
        &mut self,
        _action: &ToggleMinimap,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for tab in &self.tabs {
            tab.overview.update(cx, |v, cx| v.toggle(cx));
        }
        self.quick_find
            .update(cx, |qf, cx| qf.toggle_minimap(cx));
        cx.notify();
    }

    /// Flip the application theme (dark ↔ light) and persist to disk.
    pub fn toggle_theme(
        &mut self,
        _action: &ToggleTheme,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.options.dark_theme = !self.options.dark_theme;
        cx.set_global(Theme::from_options_flag(self.options.dark_theme));
        if let Err(e) = self.options.save() {
            tracing::warn!("failed to persist options after theme toggle: {e}");
            self.status = format!("Theme toggled (save failed: {e})");
        } else {
            self.status = if self.options.dark_theme {
                "Switched to dark theme".to_string()
            } else {
                "Switched to light theme".to_string()
            };
        }
        cx.notify();
    }

    /// Apply a new font size: clamp, persist, and refresh the global so the
    /// LogView / FilteredView observers repaint immediately.
    fn apply_font_size(&mut self, size: f32, cx: &mut Context<Self>) {
        let clamped = size.clamp(FontSettings::MIN, FontSettings::MAX);
        if (clamped - self.options.main_font_size).abs() < f32::EPSILON {
            return;
        }
        self.options.main_font_size = clamped;
        cx.set_global(FontSettings::from_options(&self.options));
        if let Err(e) = self.options.save() {
            tracing::warn!("failed to persist options after font-size change: {e}");
            self.status = format!("Font size: {clamped} (save failed: {e})");
        } else {
            self.status = format!("Font size: {clamped}");
        }
        cx.notify();
    }

    fn increase_font_size(
        &mut self,
        _action: &IncreaseFontSize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_font_size(self.options.main_font_size + 1.0, cx);
    }

    fn decrease_font_size(
        &mut self,
        _action: &DecreaseFontSize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_font_size(self.options.main_font_size - 1.0, cx);
    }

    fn reset_font_size(
        &mut self,
        _action: &ResetFontSize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_font_size(FontSettings::DEFAULT, cx);
    }

    /// Copy the active log view's selected lines to the system clipboard.
    /// No-op when nothing is selected so the shortcut doesn't silently
    /// stomp on a clipboard the user has filled from elsewhere.
    fn copy_selection(
        &mut self,
        _action: &CopySelection,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tracing::info!("CopySelection dispatched");
        let log_view = self.tabs[self.active_tab].log_view.clone();
        let (text, line_count) = log_view.update(cx, |view, _| {
            (view.selection_text(), view.selection_line_count())
        });
        let Some(text) = text else {
            self.status = "Copy: nothing selected (click a line first)".to_string();
            cx.notify();
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.status = if line_count == 1 {
            "Copied 1 line".to_string()
        } else {
            format!("Copied {line_count} lines")
        };
        cx.notify();
    }

    /// Copy the full lines covered by the active log view's selection. Unlike
    /// `copy_selection`, this always returns whole lines even when the user
    /// has a sub-line character range selected.
    fn copy_selected_lines(
        &mut self,
        _action: &CopySelectedLines,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let log_view = self.tabs[self.active_tab].log_view.clone();
        let (text, line_count) = log_view.update(cx, |view, _| {
            (view.selected_lines_text(), view.selection_line_count())
        });
        let Some(text) = text else {
            self.status = "Copy: nothing selected (click a line first)".to_string();
            cx.notify();
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.status = if line_count == 1 {
            "Copied 1 line".to_string()
        } else {
            format!("Copied {line_count} lines")
        };
        cx.notify();
    }

    /// Hide the right-click context menu if visible, and clear the
    /// log view's right-click fallback so later Cmd+C doesn't act on a
    /// stale row.
    fn close_context_menu(&mut self, cx: &mut Context<Self>) {
        let was_visible = self.context_menu.take().is_some();
        let log_view = self.tabs[self.active_tab].log_view.clone();
        log_view.update(cx, |view, _| view.clear_right_click());
        if was_visible {
            cx.notify();
        }
    }

    /// Build the context-menu overlay. Renders a transparent backdrop that
    /// dismisses the menu on outside click; menu items dispatch a copy action
    /// and the backdrop click that follows from event-bubbling closes the menu.
    fn render_context_menu(&self, pos: Point<Pixels>, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        div()
            .absolute()
            .inset_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, cx| this.close_context_menu(cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _: &MouseDownEvent, _, cx| this.close_context_menu(cx)),
            )
            .child(
                div()
                    .absolute()
                    .left(pos.x)
                    .top(pos.y)
                    .w(px(200.0))
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.selection)
                    .rounded_md()
                    .shadow_lg()
                    .py_1()
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .text_color(theme.foreground)
                            .hover(|s| s.bg(theme.selection))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                                    this.copy_selection(&CopySelection, window, cx);
                                    this.close_context_menu(cx);
                                }),
                            )
                            .child("Copy Selection"),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .text_color(theme.foreground)
                            .hover(|s| s.bg(theme.selection))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                                    this.copy_selected_lines(&CopySelectedLines, window, cx);
                                    this.close_context_menu(cx);
                                }),
                            )
                            .child("Copy Selected Lines"),
                    ),
            )
            .into_any()
    }

    /// Get the active tab's log view.
    ///
    /// Reserved for the split-view work — keeps the lookup centralized so
    /// future call sites stay consistent with `active_filtered` /
    /// `active_overview`.
    #[allow(dead_code)]
    fn active_log_view(&self) -> &Entity<LogViewState> {
        &self.tabs[self.active_tab].log_view
    }

    /// Open a file from a path (for command-line arguments and drag-drop).
    pub fn open_file_from_path(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        self.open_file_in_new_tab(path, cx);
    }

    /// Get the active tab's filtered view. Reserved for split-view wiring.
    #[allow(dead_code)]
    fn active_filtered(&self) -> &Entity<FilteredViewState> {
        &self.tabs[self.active_tab].filtered
    }

    /// Get the active tab's overview. Reserved for split-view wiring.
    #[allow(dead_code)]
    fn active_overview(&self) -> &Entity<OverviewState> {
        &self.tabs[self.active_tab].overview
    }

    /// Open a file dialog and load the selected file into a new tab.
    fn open_file(&mut self, _action: &OpenFile, _window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });

        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            cx.update(|cx| {
                this.update(cx, |state, cx| {
                    state.open_file_in_new_tab(&path, cx);
                })
                .ok();
            })
            .ok();
        })
        .detach();
    }

    /// Open `path` in a tab. Reuses the active tab when it's still the empty
    /// placeholder (no file loaded) so the visible tab bar stays index-aligned
    /// with `self.tabs`; otherwise pushes a new tab and makes it active.
    fn open_file_in_new_tab(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let active_has_file = self.tabs[self.active_tab]
            .log_view
            .read(cx)
            .has_file();
        if active_has_file {
            let log_view = cx.new(|cx| LogViewState::new(cx));
            let filtered = cx.new(|cx| FilteredViewState::new(cx));
            let overview = cx.new(OverviewState::new);
            Self::subscribe_log_view(&log_view, cx);
            Self::subscribe_overview(&overview, cx);
            self.tabs.push(TabData { log_view, filtered, overview });
            self.active_tab = self.tabs.len() - 1;
        }
        let log_view = self.tabs[self.active_tab].log_view.clone();

        log_view.update(cx, |view, cx| {
            view.load_file(path, cx);
        });

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let path_str = path.to_string_lossy().to_string();
        self.tab_bar.update(cx, |bar, _| {
            bar.add_tab(name.clone(), Some(path_str));
        });

        // Sync QuickFind to the newly-loaded data.
        let data = log_view.read(cx).log_data();
        self.quick_find
            .update(cx, |find, cx| find.set_log_data(data, cx));

        self.status = format!("Opened: {}", name);
        cx.notify();
    }

    /// Close the active tab. When the active tab is the last one with a file
    /// loaded, the tab is replaced with a fresh empty placeholder so the
    /// window stays renderable (render() always indexes `self.tabs[active_tab]`).
    fn close_tab(&mut self, _action: &CloseTab, _window: &mut Window, cx: &mut Context<Self>) {
        // No-op when the active tab is already an empty placeholder.
        if !self.tabs[self.active_tab].log_view.read(cx).has_file() {
            return;
        }

        let closed_idx = self.active_tab;

        if self.tabs.len() == 1 {
            let log_view = cx.new(|cx| LogViewState::new(cx));
            let filtered = cx.new(|cx| FilteredViewState::new(cx));
            let overview = cx.new(OverviewState::new);
            Self::subscribe_log_view(&log_view, cx);
            Self::subscribe_overview(&overview, cx);
            self.tabs[0] = TabData { log_view, filtered, overview };
            self.tab_bar.update(cx, |bar, _| bar.close_tab(closed_idx));
            self.quick_find.update(cx, |find, cx| find.set_log_data(None, cx));
            self.status = "Closed tab".to_string();
            cx.notify();
            return;
        }

        self.tabs.remove(closed_idx);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        // Pass the ORIGINAL index to tab_bar — it has the same length as
        // self.tabs and needs the pre-removal index to drop the right entry.
        self.tab_bar.update(cx, |bar, _| bar.close_tab(closed_idx));
        let data = self.tabs[self.active_tab].log_view.read(cx).log_data();
        self.quick_find.update(cx, |find, cx| find.set_log_data(data, cx));
        cx.notify();
    }

    /// Activate the tab at `idx` (0-based). Silently ignored when out of range
    /// so Cmd+N for an unopened slot is a no-op rather than an error.
    fn switch_to_tab(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx >= self.tabs.len() || idx == self.active_tab {
            return;
        }
        self.active_tab = idx;
        self.tab_bar.update(cx, |bar, _| bar.set_active(idx));
        let data = self.tabs[idx].log_view.read(cx).log_data();
        self.quick_find
            .update(cx, |find, cx| find.set_log_data(data, cx));
        cx.notify();
    }

    fn select_tab_1(&mut self, _: &SelectTab1, _: &mut Window, cx: &mut Context<Self>) { self.switch_to_tab(0, cx); }
    fn select_tab_2(&mut self, _: &SelectTab2, _: &mut Window, cx: &mut Context<Self>) { self.switch_to_tab(1, cx); }
    fn select_tab_3(&mut self, _: &SelectTab3, _: &mut Window, cx: &mut Context<Self>) { self.switch_to_tab(2, cx); }
    fn select_tab_4(&mut self, _: &SelectTab4, _: &mut Window, cx: &mut Context<Self>) { self.switch_to_tab(3, cx); }
    fn select_tab_5(&mut self, _: &SelectTab5, _: &mut Window, cx: &mut Context<Self>) { self.switch_to_tab(4, cx); }
    fn select_tab_6(&mut self, _: &SelectTab6, _: &mut Window, cx: &mut Context<Self>) { self.switch_to_tab(5, cx); }
    fn select_tab_7(&mut self, _: &SelectTab7, _: &mut Window, cx: &mut Context<Self>) { self.switch_to_tab(6, cx); }
    fn select_tab_8(&mut self, _: &SelectTab8, _: &mut Window, cx: &mut Context<Self>) { self.switch_to_tab(7, cx); }
    fn select_tab_9(&mut self, _: &SelectTab9, _: &mut Window, cx: &mut Context<Self>) { self.switch_to_tab(8, cx); }

    /// Toggle quick find. Also re-syncs the active LogView's log data into
    /// QuickFind so a search runs against the file currently in focus.
    fn toggle_find(
        &mut self,
        _action: &ToggleQuickFind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_data = self.tabs[self.active_tab]
            .log_view
            .read(cx)
            .log_data();
        self.quick_find.update(cx, |find, cx| {
            find.set_log_data(active_data, cx);
            find.toggle(window, cx);
        });
    }

    /// Find next match.
    fn find_next(&mut self, _action: &FindNext, _window: &mut Window, cx: &mut Context<Self>) {
        self.quick_find.update(cx, |find, cx| {
            find.next_match(cx);
        });
    }

    /// Find previous match.
    fn find_prev(
        &mut self,
        _action: &FindPrevious,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.quick_find.update(cx, |find, cx| {
            find.prev_match(cx);
        });
    }

    /// Close find bar.
    fn close_find(
        &mut self,
        _action: &CloseFind,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.quick_find.update(cx, |find, cx| {
            find.close(cx);
        });
    }

    /// Toggle filtered view.
    fn toggle_filtered(
        &mut self,
        _action: &ToggleFilteredView,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tabs[self.active_tab]
            .filtered
            .update(cx, |view, cx| {
                view.toggle(cx);
            });
    }

    /// Toggle tail mode.
    fn toggle_tail_mode(
        &mut self,
        _action: &ToggleTailMode,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tabs[self.active_tab]
            .log_view
            .update(cx, |view, cx| {
                view.toggle_tail_mode(cx);
            });
        let is_tail = self.tabs[self.active_tab]
            .log_view
            .update(cx, |view, _| view.is_tail_mode());
        self.status = if is_tail {
            "Tail mode: ON".to_string()
        } else {
            "Tail mode: OFF".to_string()
        };
        cx.notify();
    }

    /// Open options dialog.
    fn open_options(
        &mut self,
        _action: &OpenOptions,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.options_dialog.update(cx, |dlg, cx| {
            dlg.toggle(cx);
        });
    }

    /// Close options dialog.
    fn close_options(
        &mut self,
        _action: &CloseOptions,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.options_dialog.update(cx, |dlg, cx| {
            dlg.close(cx);
        });
    }

    /// Open the About dialog (or close it if already open).
    fn open_about(
        &mut self,
        _action: &OpenAbout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.about_dialog.update(cx, |dlg, cx| {
            if dlg.is_visible() {
                dlg.close(cx);
            } else {
                dlg.open(window, cx);
            }
        });
    }

    /// Close the About dialog.
    fn close_about(
        &mut self,
        _action: &CloseAbout,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.about_dialog.update(cx, |dlg, cx| dlg.close(cx));
    }

    /// Scroll to top.
    fn scroll_to_top(
        &mut self,
        _action: &ScrollToTop,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tabs[self.active_tab]
            .log_view
            .update(cx, |view, cx| {
                view.scroll_to_line(0, cx);
            });
    }

    /// Scroll to bottom.
    fn scroll_to_bottom(
        &mut self,
        _action: &ScrollToBottom,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tabs[self.active_tab]
            .log_view
            .update(cx, |view, cx| {
                let total = view.total_lines();
                if total > 0 {
                    view.scroll_to_line(total - 1, cx);
                }
            });
    }

    /// Page up.
    fn page_up(&mut self, _action: &PageUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.tabs[self.active_tab]
            .log_view
            .update(cx, |view, cx| {
                let current = view.selected_line().unwrap_or(0);
                let target = current.saturating_sub(50);
                view.select_line(Some(target), cx);
                view.scroll_to_line(target, cx);
            });
    }

    /// Page down.
    fn page_down(&mut self, _action: &PageDown, _window: &mut Window, cx: &mut Context<Self>) {
        self.tabs[self.active_tab]
            .log_view
            .update(cx, |view, cx| {
                let current = view.selected_line().unwrap_or(0);
                let total = view.total_lines();
                let target = (current + 50).min(total.saturating_sub(1));
                view.select_line(Some(target), cx);
                view.scroll_to_line(target, cx);
            });
    }

    /// Move up one line.
    fn line_up(&mut self, _action: &LineUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.tabs[self.active_tab]
            .log_view
            .update(cx, |view, cx| {
                let current = view.selected_line().unwrap_or(0);
                let target = current.saturating_sub(1);
                view.select_line(Some(target), cx);
                view.scroll_to_line(target, cx);
            });
    }

    /// Move down one line.
    fn line_down(&mut self, _action: &LineDown, _window: &mut Window, cx: &mut Context<Self>) {
        self.tabs[self.active_tab]
            .log_view
            .update(cx, |view, cx| {
                let current = view.selected_line().unwrap_or(0);
                let total = view.total_lines();
                let target = (current + 1).min(total.saturating_sub(1));
                view.select_line(Some(target), cx);
                view.scroll_to_line(target, cx);
            });
    }

    /// Show go-to-line dialog.
    fn go_to_line_dialog(
        &mut self,
        _action: &GoToLineDialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.go_to_line.update(cx, |gtl, cx| {
            gtl.open(window, cx);
        });
    }

    /// Toggle the scratchpad panel.
    fn toggle_scratchpad(
        &mut self,
        _action: &ToggleScratchpad,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.scratchpad.update(cx, |pad, cx| {
            pad.toggle(cx);
        });
    }

    /// Open the session manager (save).
    fn save_session(
        &mut self,
        _action: &SaveSession,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session_widget.update(cx, |sw, cx| {
            sw.toggle(cx);
        });
    }

    /// Open the session manager (load).
    fn load_session(
        &mut self,
        _action: &LoadSession,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session_widget.update(cx, |sw, cx| {
            sw.toggle(cx);
        });
    }

    /// Toggle predefined filter panel.
    fn toggle_filter_panel(
        &mut self,
        _action: &ToggleFilterPanel,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.predefined_filters.update(cx, |filters, cx| {
            filters.toggle(cx);
        });
    }

    /// Handle files dropped onto the window.
    fn handle_file_drop(
        &mut self,
        paths: &ExternalPaths,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for path in paths.paths() {
            if path.is_file() {
                self.open_file_from_path(path, cx);
            }
        }
    }
}

impl Focusable for MainWindowState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MainWindowState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.global::<Theme>();
        let font = cx.global::<FontSettings>().clone();
        let log_view = self.tabs[self.active_tab].log_view.clone();
        let filtered = self.tabs[self.active_tab].filtered.clone();
        let overview = self.tabs[self.active_tab].overview.clone();
        let quick_find = self.quick_find.clone();
        let options_dialog = self.options_dialog.clone();
        let about_dialog = self.about_dialog.clone();
        let tab_bar = self.tab_bar.clone();

        // Status bar info
        let (file_info, line_info) = log_view.update(cx, |view, _| {
            let file = view
                .file_name()
                .unwrap_or_else(|| "(no file)".to_string());
            let lines = view.total_lines();
            let sel = match view.selection_range() {
                Some((lo, hi)) if hi > lo => {
                    format!("  Sel: {}-{} ({} lines)", lo + 1, hi + 1, hi - lo + 1)
                }
                Some((lo, _)) => format!("  Sel: {}", lo + 1),
                None => String::new(),
            };
            let tail = if view.is_tail_mode() { "  [TAIL]" } else { "" };
            (file, format!("Lines: {}{}{}", lines, sel, tail))
        });

        // Quick find bar
        let find_bar =
            quick_find.update(cx, |find, cx| find.render_bar(theme, font.clone(), window, cx));

        // Options dialog overlay
        let options_overlay = options_dialog.update(cx, |dlg, cx| dlg.render_dialog(theme, cx));

        // About dialog overlay
        let about_overlay = about_dialog.update(cx, |dlg, cx| dlg.render_dialog(theme, cx));

        // Predefined filters overlay (absolute-positioned panel)
        let filter_panel = self
            .predefined_filters
            .update(cx, |f, _| f.render_panel(theme));

        // Scratchpad bottom panel (above status bar) and session manager dialog.
        let scratchpad_panel = self.scratchpad.update(cx, |p, _| p.render_panel(theme));
        let session_overlay = self
            .session_widget
            .update(cx, |sw, _| sw.render_dialog(theme));
        let go_to_line_overlay = self
            .go_to_line
            .update(cx, |gtl, cx| gtl.render_dialog(theme, cx));

        // Tab bar
        let tab_bar_el = tab_bar.update(cx, |bar, cx| bar.render(theme, cx));

        // Filtered view (if visible)
        let filtered_visible = filtered.update(cx, |v, _| v.is_visible());

        // Overview minimap: push latest state (line totals, matches,
        // bookmarks, viewport) so the strip stays in sync with the active
        // tab, then render. Viewport height isn't directly available at this
        // layer, so we approximate it from the window's content size; the
        // minimap clamps to the strip's actual height anyway.
        let total_lines = log_view.read(cx).total_lines();
        let matches: Vec<u64> = self
            .quick_find
            .read(cx)
            .matching_lines()
            .to_vec();
        let bookmarks: Vec<u64> = log_view.read(cx).bookmarks().to_vec();
        let viewport_height_px = f32::from(window.viewport_size().height)
            - /* approx chrome: tabs + status */ 60.0;
        let (vp_start, vp_size) = log_view
            .read(cx)
            .viewport_lines(&font, viewport_height_px.max(0.0));
        let overview_el = overview.update(cx, |v, cx| {
            v.set_total_lines(total_lines, cx);
            v.set_matches(matches, cx);
            v.set_bookmarks(bookmarks, cx);
            v.set_viewport(vp_start, vp_size, cx);
            v.render(theme, window, cx)
        });

        div()
            .key_context("MainWindow")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::open_file))
            .on_action(cx.listener(Self::close_tab))
            .on_action(cx.listener(Self::toggle_find))
            .on_action(cx.listener(Self::find_next))
            .on_action(cx.listener(Self::find_prev))
            .on_action(cx.listener(Self::close_find))
            .on_action(cx.listener(Self::toggle_filtered))
            .on_action(cx.listener(Self::toggle_tail_mode))
            .on_action(cx.listener(Self::open_options))
            .on_action(cx.listener(Self::close_options))
            .on_action(cx.listener(Self::open_about))
            .on_action(cx.listener(Self::close_about))
            .on_action(cx.listener(Self::scroll_to_top))
            .on_action(cx.listener(Self::scroll_to_bottom))
            .on_action(cx.listener(Self::page_up))
            .on_action(cx.listener(Self::page_down))
            .on_action(cx.listener(Self::line_up))
            .on_action(cx.listener(Self::line_down))
            .on_action(cx.listener(Self::go_to_line_dialog))
            .on_action(cx.listener(Self::toggle_filter_panel))
            .on_action(cx.listener(Self::toggle_theme))
            .on_action(cx.listener(Self::toggle_minimap))
            .on_action(cx.listener(Self::increase_font_size))
            .on_action(cx.listener(Self::decrease_font_size))
            .on_action(cx.listener(Self::reset_font_size))
            .on_action(cx.listener(Self::copy_selection))
            .on_action(cx.listener(Self::copy_selected_lines))
            .on_action(cx.listener(Self::toggle_scratchpad))
            .on_action(cx.listener(Self::save_session))
            .on_action(cx.listener(Self::load_session))
            .on_action(cx.listener(Self::select_tab_1))
            .on_action(cx.listener(Self::select_tab_2))
            .on_action(cx.listener(Self::select_tab_3))
            .on_action(cx.listener(Self::select_tab_4))
            .on_action(cx.listener(Self::select_tab_5))
            .on_action(cx.listener(Self::select_tab_6))
            .on_action(cx.listener(Self::select_tab_7))
            .on_action(cx.listener(Self::select_tab_8))
            .on_action(cx.listener(Self::select_tab_9))
            .on_drop(cx.listener(Self::handle_file_drop))
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            // Tab bar
            .child(tab_bar_el)
            // Main content area
            .child(
                div()
                    .flex_grow()
                    .flex()
                    .flex_row()
                    // Log view (or split with filtered view)
                    .child(if filtered_visible {
                        div()
                            .flex()
                            .flex_col()
                            .flex_grow()
                            .child(div().flex_grow().child(log_view))
                            .child(
                                div()
                                    .h(px(1.0))
                                    .bg(theme.selection),
                            )
                            .child(div().h(px(200.0)).child(filtered.update(cx, |v, cx| {
                                v.render_view(window, cx)
                            })))
                            .into_any()
                    } else {
                        div().flex_grow().child(log_view).into_any()
                    })
                    // Overview minimap
                    .child(overview_el),
            )
            // Scratchpad panel (sits between main content and quick find)
            .children(scratchpad_panel)
            // Quick find bar
            .children(find_bar)
            // Status bar
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(24.0))
                    .px_2()
                    .bg(theme.current_line)
                    .text_color(theme.line_number)
                    .child(format!("{}  |  {}", file_info, line_info))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(self.status.clone())
                            .child(
                                div()
                                    .id("status-settings")
                                    .px_2()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme.selection).text_color(theme.foreground))
                                    .child("\u{2699} Settings")
                                    .on_click(|_event, window, cx| {
                                        window.dispatch_action(Box::new(OpenOptions), cx);
                                    }),
                            ),
                    ),
            )
            // Options dialog overlay
            .children(options_overlay)
            // About dialog overlay
            .children(about_overlay)
            // Predefined filters panel overlay
            .children(filter_panel)
            // Session manager dialog overlay
            .children(session_overlay)
            // Go-to-line dialog overlay
            .children(go_to_line_overlay)
            // Right-click context menu overlay (rendered last so it sits on top).
            .children(self.context_menu.map(|pos| self.render_context_menu(pos, theme, cx)))
    }
}
