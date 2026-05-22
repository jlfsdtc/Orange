//! Context menu for the log view.

use gpui::*;

use crate::theme::Theme;

/// Context menu actions.
actions!(
    orange,
    [
        CopyLine,
        CopySelection,
        ToggleBookmark,
        SearchSelection,
        GoToLine,
        SelectAll
    ]
);

/// Context menu item.
struct MenuItem {
    label: String,
    action: Box<dyn Action>,
    enabled: bool,
    separator_after: bool,
}

/// Context menu state.
pub struct ContextMenuState {
    items: Vec<MenuItem>,
    visible: bool,
    position: Point<Pixels>,
}

impl ContextMenuState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        Self {
            items: Vec::new(),
            visible: false,
            position: point(px(0.0), px(0.0)),
        }
    }

    /// Show the context menu at a position.
    pub fn show_at(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.position = position;
        self.visible = true;
        self.rebuild_items();
        cx.notify();
    }

    /// Hide the context menu.
    pub fn hide(&mut self, cx: &mut Context<Self>) {
        self.visible = false;
        cx.notify();
    }

    /// Whether the menu is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Rebuild menu items based on current state.
    fn rebuild_items(&mut self) {
        self.items = vec![
            MenuItem {
                label: "Copy Line".to_string(),
                action: Box::new(CopyLine),
                enabled: true,
                separator_after: false,
            },
            MenuItem {
                label: "Copy Selection".to_string(),
                action: Box::new(CopySelection),
                enabled: true,
                separator_after: true,
            },
            MenuItem {
                label: "Toggle Bookmark".to_string(),
                action: Box::new(ToggleBookmark),
                enabled: true,
                separator_after: false,
            },
            MenuItem {
                label: "Search Selection".to_string(),
                action: Box::new(SearchSelection),
                enabled: true,
                separator_after: true,
            },
            MenuItem {
                label: "Go to Line...".to_string(),
                action: Box::new(GoToLine),
                enabled: true,
                separator_after: false,
            },
            MenuItem {
                label: "Select All".to_string(),
                action: Box::new(SelectAll),
                enabled: true,
                separator_after: false,
            },
        ];
    }

    /// Render the context menu overlay.
    pub fn render(&self, theme: Theme) -> Option<AnyElement> {
        if !self.visible {
            return None;
        }

        let x = self.position.x;
        let y = self.position.y;

        Some(
            div()
                .absolute()
                .left(x)
                .top(y)
                .z_index(100)
                .w(px(200.0))
                .bg(theme.background)
                .border_1()
                .border_color(theme.selection)
                .rounded_md()
                .shadow_lg()
                .children(self.items.iter().map(|item| {
                    let bg = if item.enabled {
                        theme.background
                    } else {
                        theme.current_line
                    };

                    let text_color = if item.enabled {
                        theme.foreground
                    } else {
                        theme.line_number
                    };

                    let row = div()
                        .flex()
                        .items_center()
                        .px_3()
                        .py_1()
                        .bg(bg)
                        .text_color(text_color)
                        .child(item.label.clone());

                    if item.separator_after {
                        div()
                            .child(row)
                            .child(
                                div()
                                    .h(px(1.0))
                                    .bg(theme.selection)
                                    .mx_2(),
                            )
                            .into_any()
                    } else {
                        row.into_any()
                    }
                }))
                .into_any(),
        )
    }
}
