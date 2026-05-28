//! Resolve `KeyBindingSpec` (action name + key string) into gpui `KeyBinding`.
//!
//! This is the registry that maps action-name strings to the concrete
//! `actions!()` types declared across `orange-ui` modules. Keep it in sync
//! with `orange_settings::keymap::default_bindings`.

use gpui::{KeyBinding, actions};
use orange_settings::KeyBindingSpec;

// Dispatched after the in-app keymap editor saves changes to disk. The
// app-level handler in `orange-app` reads the file and re-applies bindings.
// Declared here (not in `orange-app`) so both the dispatcher (main_window)
// and the handler (main.rs) can refer to the same action type.
actions!(orange, [ApplyKeymap]);

/// Build gpui key bindings from a list of specs. Unknown action names are
/// logged and skipped — a malformed entry will not abort startup.
pub fn build_key_bindings(specs: &[KeyBindingSpec]) -> Vec<KeyBinding> {
    specs.iter().filter_map(spec_to_binding).collect()
}

fn spec_to_binding(spec: &KeyBindingSpec) -> Option<KeyBinding> {
    use crate::filtered_view;
    use crate::log_view;
    use crate::main_window;
    use crate::options_dialog;
    use crate::predefined_filters;
    use crate::quick_find;
    use crate::scratchpad;
    use crate::session_widget;

    let key = spec.key.as_str();
    match spec.action.as_str() {
        // main_window
        "OpenFile" => Some(KeyBinding::new(key, main_window::OpenFile, None)),
        "ScrollToTop" => Some(KeyBinding::new(key, main_window::ScrollToTop, None)),
        "ScrollToBottom" => Some(KeyBinding::new(key, main_window::ScrollToBottom, None)),
        "PageUp" => Some(KeyBinding::new(key, main_window::PageUp, None)),
        "PageDown" => Some(KeyBinding::new(key, main_window::PageDown, None)),
        "CloseTab" => Some(KeyBinding::new(key, main_window::CloseTab, None)),
        "LineUp" => Some(KeyBinding::new(key, main_window::LineUp, None)),
        "LineDown" => Some(KeyBinding::new(key, main_window::LineDown, None)),
        "GoToLineDialog" => Some(KeyBinding::new(key, main_window::GoToLineDialog, None)),
        "ToggleTheme" => Some(KeyBinding::new(key, main_window::ToggleTheme, None)),
        "ToggleMinimap" => Some(KeyBinding::new(key, main_window::ToggleMinimap, None)),
        "IncreaseFontSize" => {
            Some(KeyBinding::new(key, main_window::IncreaseFontSize, None))
        }
        "DecreaseFontSize" => {
            Some(KeyBinding::new(key, main_window::DecreaseFontSize, None))
        }
        "ResetFontSize" => Some(KeyBinding::new(key, main_window::ResetFontSize, None)),
        "CopySelection" => Some(KeyBinding::new(key, main_window::CopySelection, None)),
        "SelectTab1" => Some(KeyBinding::new(key, main_window::SelectTab1, None)),
        "SelectTab2" => Some(KeyBinding::new(key, main_window::SelectTab2, None)),
        "SelectTab3" => Some(KeyBinding::new(key, main_window::SelectTab3, None)),
        "SelectTab4" => Some(KeyBinding::new(key, main_window::SelectTab4, None)),
        "SelectTab5" => Some(KeyBinding::new(key, main_window::SelectTab5, None)),
        "SelectTab6" => Some(KeyBinding::new(key, main_window::SelectTab6, None)),
        "SelectTab7" => Some(KeyBinding::new(key, main_window::SelectTab7, None)),
        "SelectTab8" => Some(KeyBinding::new(key, main_window::SelectTab8, None)),
        "SelectTab9" => Some(KeyBinding::new(key, main_window::SelectTab9, None)),

        // quick_find
        "ToggleQuickFind" => Some(KeyBinding::new(key, quick_find::ToggleQuickFind, None)),
        "FindNext" => Some(KeyBinding::new(key, quick_find::FindNext, None)),
        "FindPrevious" => Some(KeyBinding::new(key, quick_find::FindPrevious, None)),
        "CloseFind" => Some(KeyBinding::new(key, quick_find::CloseFind, None)),

        // log_view / filtered_view / options_dialog / predefined_filters
        "ToggleTailMode" => Some(KeyBinding::new(key, log_view::ToggleTailMode, None)),
        "ToggleFilteredView" => {
            Some(KeyBinding::new(key, filtered_view::ToggleFilteredView, None))
        }
        "OpenOptions" => Some(KeyBinding::new(key, options_dialog::OpenOptions, None)),
        "ToggleFilterPanel" => {
            Some(KeyBinding::new(key, predefined_filters::ToggleFilterPanel, None))
        }
        "ToggleScratchpad" => Some(KeyBinding::new(key, scratchpad::ToggleScratchpad, None)),
        "SaveSession" => Some(KeyBinding::new(key, session_widget::SaveSession, None)),
        "LoadSession" => Some(KeyBinding::new(key, session_widget::LoadSession, None)),

        unknown => {
            tracing::warn!(
                "ignoring unknown action {unknown:?} in keymap (key {key:?})"
            );
            None
        }
    }
}

/// All action names recognized by `spec_to_binding`. Kept as a public list so
/// that tests and tooling can verify completeness without parsing source.
pub const KNOWN_ACTIONS: &[&str] = &[
    "OpenFile",
    "ScrollToTop",
    "ScrollToBottom",
    "PageUp",
    "PageDown",
    "CloseTab",
    "LineUp",
    "LineDown",
    "GoToLineDialog",
    "ToggleTheme",
    "ToggleMinimap",
    "IncreaseFontSize",
    "DecreaseFontSize",
    "ResetFontSize",
    "CopySelection",
    "SelectTab1",
    "SelectTab2",
    "SelectTab3",
    "SelectTab4",
    "SelectTab5",
    "SelectTab6",
    "SelectTab7",
    "SelectTab8",
    "SelectTab9",
    "ToggleQuickFind",
    "FindNext",
    "FindPrevious",
    "CloseFind",
    "ToggleTailMode",
    "ToggleFilteredView",
    "OpenOptions",
    "ToggleFilterPanel",
    "ToggleScratchpad",
    "SaveSession",
    "LoadSession",
];
