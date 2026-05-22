//! orange-ui: GPUI-based user interface for Orange.
//!
//! This crate implements the entire GUI layer using GPUI:
//! - Main window with menu bar, tab bar, status bar
//! - Log view with virtual scrolling for large files
//! - Filtered view for search results
//! - Overview/minimap
//! - Highlighter sets
//! - Quick find bar
//! - Options dialog
//! - Session management

#![recursion_limit = "1024"]

pub mod main_window;
pub mod log_view;
pub mod filtered_view;
pub mod overview;
pub mod highlighter;
pub mod keymap;
pub mod quick_find;
pub mod scratchpad;
pub mod options_dialog;
pub mod about_dialog;
pub mod go_to_line;
pub mod session_widget;
pub mod predefined_filters;
pub mod components;
pub mod theme;
