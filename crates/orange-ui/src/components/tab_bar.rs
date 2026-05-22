//! Tab bar component for managing multiple open files.

use gpui::*;

use crate::theme::Theme;

/// Events emitted by `TabBarState`.
#[derive(Clone, Debug)]
pub enum TabBarEvent {
    /// User clicked the tab at this index — parent should switch to it.
    Selected(usize),
}

/// A single tab.
pub struct Tab {
    /// Display name (file name).
    pub name: String,
    /// Full path (for tooltip).
    pub path: Option<String>,
    /// Whether this tab is active.
    pub active: bool,
    /// Whether there are unsaved changes (for future use).
    pub modified: bool,
}

/// Tab bar state.
pub struct TabBarState {
    /// The tabs.
    tabs: Vec<Tab>,
    /// Currently active tab index.
    active: usize,
}

impl TabBarState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        Self {
            tabs: Vec::new(),
            active: 0,
        }
    }

    /// Add a new tab and make it active.
    pub fn add_tab(&mut self, name: String, path: Option<String>) -> usize {
        // Check if tab already exists
        if let Some(idx) = self.tabs.iter().position(|t| t.path.as_deref() == path.as_deref()) {
            self.active = idx;
            return idx;
        }

        self.tabs.push(Tab {
            name,
            path,
            active: false,
            modified: false,
        });
        self.active = self.tabs.len() - 1;
        self.active
    }

    /// Close a tab by index.
    pub fn close_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.tabs.remove(index);
            if self.active >= self.tabs.len() && !self.tabs.is_empty() {
                self.active = self.tabs.len() - 1;
            }
        }
    }

    /// Set the active tab.
    pub fn set_active(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
        }
    }

    /// Get the active tab index.
    pub fn active_index(&self) -> usize {
        self.active
    }

    /// Get the number of tabs.
    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    /// Whether there are no tabs.
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// Get tab names for rendering.
    pub fn tab_names(&self) -> Vec<(&str, bool)> {
        self.tabs
            .iter()
            .enumerate()
            .map(|(i, t)| (t.name.as_str(), i == self.active))
            .collect()
    }

    /// Render the tab bar. Each tab is a stateful `.id(...)` div with an
    /// `on_click` listener that flips local active state and emits
    /// `TabBarEvent::Selected(idx)` so the parent can react.
    pub fn render(&mut self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        if self.tabs.is_empty() {
            return div().into_any();
        }

        let active = self.active;
        let tabs: Vec<AnyElement> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let is_active = i == active;

                let bg = if is_active {
                    theme.background
                } else {
                    theme.current_line
                };

                let border = if is_active {
                    theme.search_current
                } else {
                    theme.current_line
                };

                div()
                    .id(("tab", i))
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_3()
                    .py_1()
                    .bg(bg)
                    .border_b_2()
                    .border_color(border)
                    .text_color(theme.foreground)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        if i < this.tabs.len() {
                            this.active = i;
                            cx.emit(TabBarEvent::Selected(i));
                            cx.notify();
                        }
                    }))
                    .child(tab.name.clone())
                    .into_any()
            })
            .collect();

        div()
            .flex()
            .h(px(32.0))
            .bg(theme.current_line)
            .border_b_1()
            .border_color(theme.selection)
            .children(tabs)
            .into_any()
    }
}

impl EventEmitter<TabBarEvent> for TabBarState {}
