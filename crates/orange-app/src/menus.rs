//! Application menu bar (visible on macOS as the top-of-screen system menu,
//! and on Linux/Windows as a per-window menu where GPUI supports it).
//!
//! The menu items dispatch the same `actions!()` types that `orange-ui` already
//! registers via the keymap, so behavior stays consistent between menu clicks
//! and keyboard shortcuts.

use gpui::{Menu, MenuItem};
use orange_ui::about_dialog::OpenAbout;
use orange_ui::filtered_view::ToggleFilteredView;
use orange_ui::log_view::ToggleTailMode;
use orange_ui::main_window::{
    CloseTab, GoToLineDialog, LineDown, LineUp, OpenFile, PageDown, PageUp, ScrollToBottom,
    ScrollToTop, ToggleTheme,
};
use orange_ui::options_dialog::OpenOptions;
use orange_ui::predefined_filters::ToggleFilterPanel;
use orange_ui::quick_find::{FindNext, FindPrevious, ToggleQuickFind};

/// Build the application menu bar.
pub fn app_menus() -> Vec<Menu> {
    vec![
        Menu {
            name: "Orange".into(),
            items: vec![
                MenuItem::action("About Orange", OpenAbout),
                MenuItem::separator(),
                MenuItem::action("Preferences…", OpenOptions),
                MenuItem::separator(),
                MenuItem::action("Quit", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("Open…", OpenFile),
                MenuItem::separator(),
                MenuItem::action("Close Tab", CloseTab),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Scroll to Top", ScrollToTop),
                MenuItem::action("Scroll to Bottom", ScrollToBottom),
                MenuItem::action("Page Up", PageUp),
                MenuItem::action("Page Down", PageDown),
                MenuItem::action("Line Up", LineUp),
                MenuItem::action("Line Down", LineDown),
                MenuItem::separator(),
                MenuItem::action("Toggle Tail Mode", ToggleTailMode),
                MenuItem::action("Toggle Filtered View", ToggleFilteredView),
                MenuItem::action("Toggle Filter Panel", ToggleFilterPanel),
                MenuItem::separator(),
                MenuItem::action("Toggle Theme", ToggleTheme),
            ],
        },
        Menu {
            name: "Find".into(),
            items: vec![
                MenuItem::action("Find…", ToggleQuickFind),
                MenuItem::action("Find Next", FindNext),
                MenuItem::action("Find Previous", FindPrevious),
                MenuItem::separator(),
                MenuItem::action("Go to Line…", GoToLineDialog),
            ],
        },
    ]
}

// --- Stub actions for menu-only entries (no body yet) -----------------------

gpui::actions!(orange, [Quit]);
