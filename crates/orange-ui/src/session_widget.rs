//! Session management: save and restore application state.
//!
//! Sessions capture the list of open files, cursor positions,
//! bookmarks, and active filters.

use gpui::*;
use orange_core::session::{Session, SessionFile};
use std::path::PathBuf;

use crate::theme::Theme;

// Session actions.
actions!(orange, [SaveSession, LoadSession]);

/// Session widget state.
pub struct SessionWidgetState {
    /// Whether the session dialog is visible.
    visible: bool,
    /// Available sessions.
    sessions: Vec<Session>,
    /// Currently active session name.
    active_session: Option<String>,
}

impl SessionWidgetState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        Self {
            visible: false,
            sessions: Vec::new(),
            active_session: None,
        }
    }

    /// Load available sessions from disk.
    pub fn load_sessions(&mut self) {
        let session_dir = Self::session_dir();
        if !session_dir.exists() {
            return;
        }

        self.sessions.clear();
        if let Ok(entries) = std::fs::read_dir(&session_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Ok(session) = Session::load(&path) {
                        self.sessions.push(session);
                    }
                }
            }
        }
    }

    /// Save current state as a session.
    pub fn save_session(
        &mut self,
        name: &str,
        files: Vec<SessionFile>,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let session = Session {
            name: name.to_string(),
            files,
            active_file: 0,
        };

        let session_dir = Self::session_dir();
        std::fs::create_dir_all(&session_dir)?;

        let path = session_dir.join(format!("{}.json", name));
        session.save(&path)?;

        self.active_session = Some(name.to_string());
        self.load_sessions();
        cx.notify();
        Ok(())
    }

    /// Load a session by name.
    pub fn load_session(&self, name: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.name == name)
    }

    /// Get available session names.
    pub fn session_names(&self) -> Vec<&str> {
        self.sessions.iter().map(|s| s.name.as_str()).collect()
    }

    /// Toggle visibility.
    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.visible = !self.visible;
        if self.visible {
            self.load_sessions();
        }
        cx.notify();
    }

    /// Whether the dialog is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    fn session_dir() -> PathBuf {
        orange_settings::config_dir().join("sessions")
    }

    /// Render the session dialog.
    pub fn render_dialog(&self, theme: Theme) -> Option<AnyElement> {
        if !self.visible {
            return None;
        }


        Some(
            div()
                .absolute()
                .inset_0()
                .bg(Hsla {
                    h: 0.0,
                    s: 0.0,
                    l: 0.0,
                    a: 0.5,
                })
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(400.0))
                        .max_h(px(500.0))
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
                                        .child("Sessions"),
                                ),
                        )
                        .child(
                            div()
                                .px_4()
                                .py_3()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .children(self.sessions.iter().map(|session| {
                                    let is_active = self.active_session.as_deref()
                                        == Some(&session.name);

                                    let bg = if is_active {
                                        theme.selection
                                    } else {
                                        theme.background
                                    };

                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .px_3()
                                        .py_2()
                                        .bg(bg)
                                        .rounded_md()
                                        .text_color(theme.foreground)
                                        .child(session.name.clone())
                                        .child(
                                            div()
                                                .text_color(theme.line_number)
                                                .text_sm()
                                                .child(format!(
                                                    "{} files",
                                                    session.files.len()
                                                )),
                                        )
                                        .into_any()
                                })),
                        )
                        .child(
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
                                        .px_3()
                                        .py_1()
                                        .bg(theme.selection)
                                        .rounded_md()
                                        .text_color(theme.foreground)
                                        .child("Close"),
                                ),
                        ),
                )
                .into_any(),
        )
    }
}
