//! Keymap configuration: maps key combinations to named actions.
//!
//! The file lives at `~/.config/orange/keymap.json`. Action names match the
//! gpui `actions!()` identifiers exported by `orange-ui`. Key strings follow
//! the gpui `KeyBinding::new` syntax (e.g. `"ctrl-shift-t"`, `"pageup"`).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A single key → action binding as stored in the keymap file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyBindingSpec {
    /// Key combination string (gpui syntax, e.g. `"ctrl-shift-t"`).
    pub key: String,
    /// Action identifier (matches a gpui `actions!()` type name).
    pub action: String,
}

impl KeyBindingSpec {
    pub fn new(key: impl Into<String>, action: impl Into<String>) -> Self {
        Self { key: key.into(), action: action.into() }
    }
}

/// Top-level keymap file structure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Keymap {
    pub bindings: Vec<KeyBindingSpec>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self { bindings: default_bindings() }
    }
}

impl Keymap {
    /// Load the keymap from disk; fall back to defaults on any failure.
    /// Errors are logged so a malformed file does not crash startup.
    pub fn load_or_default() -> Self {
        match Self::load() {
            Ok(Some(km)) => km,
            Ok(None) => Self::default(),
            Err(e) => {
                tracing::warn!("failed to load keymap, using defaults: {e}");
                Self::default()
            }
        }
    }

    /// Load the keymap if the file exists. Returns `Ok(None)` when missing.
    pub fn load() -> anyhow::Result<Option<Self>> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(Some(serde_json::from_str(&content)?))
    }

    /// Save the keymap to the config file.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Write the default keymap to disk only if no file exists yet,
    /// so users have an editable seed without overwriting customizations.
    pub fn write_default_if_missing() -> anyhow::Result<()> {
        let path = Self::config_path();
        if path.exists() {
            return Ok(());
        }
        Self::default().save()
    }

    /// Path to the keymap file.
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("orange")
            .join("keymap.json")
    }

    /// Bind `key` to `action`, replacing any existing bindings for the same key.
    /// Returns the previously bound action name if a conflict was resolved,
    /// so the caller can surface a "Removed: <key> from <action>" notice.
    /// When multiple actions shared the same key (already unusual), only the
    /// first removed action name is returned — the rest are also unbound.
    pub fn set_binding(&mut self, key: &str, action: &str) -> Option<String> {
        let mut removed: Option<String> = None;
        self.bindings.retain(|b| {
            if b.key == key && b.action != action {
                if removed.is_none() {
                    removed = Some(b.action.clone());
                }
                false
            } else {
                true
            }
        });
        if !self.bindings.iter().any(|b| b.key == key && b.action == action) {
            self.bindings.push(KeyBindingSpec::new(key, action));
        }
        removed
    }

    /// Remove every binding for the given action. Used by the "Unbind" button.
    /// Returns the number of bindings that were removed.
    pub fn clear_binding(&mut self, action: &str) -> usize {
        let before = self.bindings.len();
        self.bindings.retain(|b| b.action != action);
        before - self.bindings.len()
    }
}

/// Default key bindings for the host platform. Picks one of the three
/// platform-specific tables at compile time. Most shortcuts use GPUI's
/// `secondary-` modifier (cmd on macOS, ctrl elsewhere); the per-platform
/// tables only diverge where the native UI convention demands it.
fn default_bindings() -> Vec<KeyBindingSpec> {
    #[cfg(target_os = "macos")]
    {
        default_bindings_macos()
    }
    #[cfg(target_os = "windows")]
    {
        default_bindings_windows()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        default_bindings_linux()
    }
}

/// Shared bindings used on every platform. These either use `secondary-`
/// (auto-resolved to cmd/ctrl) or are plain function/arrow/page keys, so
/// they need no per-OS adjustment.
fn common_bindings() -> Vec<KeyBindingSpec> {
    vec![
        KeyBindingSpec::new("secondary-o", "OpenFile"),
        KeyBindingSpec::new("secondary-home", "ScrollToTop"),
        KeyBindingSpec::new("secondary-end", "ScrollToBottom"),
        KeyBindingSpec::new("pageup", "PageUp"),
        KeyBindingSpec::new("pagedown", "PageDown"),
        // NOTE: `ToggleQuickFind` is intentionally left unbound by default —
        // the quick-find box opens automatically when a file is loaded, and
        // `secondary-f` is deliberately freed so users can rebind it. The
        // action stays resolvable (Find menu + Shortcuts editor) so it can be
        // re-assigned a key from Preferences.
        KeyBindingSpec::new("f3", "FindNext"),
        KeyBindingSpec::new("shift-f3", "FindPrevious"),
        KeyBindingSpec::new("secondary-g", "FindNext"),
        KeyBindingSpec::new("secondary-shift-g", "FindPrevious"),
        KeyBindingSpec::new("escape", "CloseFind"),
        KeyBindingSpec::new("secondary-t", "ToggleTailMode"),
        KeyBindingSpec::new("secondary-,", "OpenOptions"),
        KeyBindingSpec::new("secondary-w", "CloseTab"),
        KeyBindingSpec::new("up", "LineUp"),
        KeyBindingSpec::new("down", "LineDown"),
        KeyBindingSpec::new("secondary-l", "GoToLineDialog"),
        KeyBindingSpec::new("secondary-shift-p", "ToggleFilterPanel"),
        KeyBindingSpec::new("secondary-shift-t", "ToggleTheme"),
        KeyBindingSpec::new("secondary-shift-m", "ToggleMinimap"),
        KeyBindingSpec::new("secondary-shift-s", "ToggleScratchpad"),
        KeyBindingSpec::new("secondary-shift-d", "SaveSession"),
        KeyBindingSpec::new("secondary-shift-o", "LoadSession"),
        // Font size: cmd-= (no shift) and cmd-+ (shift-=) both zoom in,
        // matching editor/browser convention on every platform.
        KeyBindingSpec::new("secondary-=", "IncreaseFontSize"),
        KeyBindingSpec::new("secondary-shift-=", "IncreaseFontSize"),
        KeyBindingSpec::new("secondary--", "DecreaseFontSize"),
        KeyBindingSpec::new("secondary-0", "ResetFontSize"),
        KeyBindingSpec::new("secondary-c", "CopySelection"),
        KeyBindingSpec::new("secondary-1", "SelectTab1"),
        KeyBindingSpec::new("secondary-2", "SelectTab2"),
        KeyBindingSpec::new("secondary-3", "SelectTab3"),
        KeyBindingSpec::new("secondary-4", "SelectTab4"),
        KeyBindingSpec::new("secondary-5", "SelectTab5"),
        KeyBindingSpec::new("secondary-6", "SelectTab6"),
        KeyBindingSpec::new("secondary-7", "SelectTab7"),
        KeyBindingSpec::new("secondary-8", "SelectTab8"),
        KeyBindingSpec::new("secondary-9", "SelectTab9"),
    ]
}

/// macOS defaults — `secondary-` resolves to `cmd-`, so the table reads as
/// the familiar Cmd-prefixed shortcuts.
pub fn default_bindings_macos() -> Vec<KeyBindingSpec> {
    let mut v = common_bindings();
    // Cmd+Shift+R for the filtered view (no browser conflict on macOS).
    v.push(KeyBindingSpec::new("secondary-shift-r", "ToggleFilteredView"));
    v
}

/// Linux defaults — `secondary-` resolves to `ctrl-`. ToggleFilteredView
/// avoids `ctrl-shift-r` (browser hard-reload) and uses `F4` instead;
/// CloseTab gets `Ctrl+F4` as an alias for MDI-style apps.
pub fn default_bindings_linux() -> Vec<KeyBindingSpec> {
    let mut v = common_bindings();
    v.push(KeyBindingSpec::new("f4", "ToggleFilteredView"));
    v.push(KeyBindingSpec::new("ctrl-f4", "CloseTab"));
    v
}

/// Windows defaults — same shape as Linux. ToggleFilteredView is on `F4`
/// (avoiding the browser hard-reload collision) and CloseTab also accepts
/// the classic Windows `Ctrl+F4`.
pub fn default_bindings_windows() -> Vec<KeyBindingSpec> {
    let mut v = common_bindings();
    v.push(KeyBindingSpec::new("f4", "ToggleFilteredView"));
    v.push(KeyBindingSpec::new("ctrl-f4", "CloseTab"));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    // `ToggleQuickFind` is intentionally NOT required: it has no default key
    // (the quick-find box auto-opens on file load), though it stays resolvable
    // so users can rebind it from Preferences.
    const REQUIRED_ACTIONS: &[&str] = &[
        "OpenFile", "ScrollToTop", "ScrollToBottom", "PageUp", "PageDown",
        "FindNext", "FindPrevious", "CloseFind",
        "ToggleTailMode", "ToggleFilteredView", "OpenOptions", "CloseTab",
        "LineUp", "LineDown", "GoToLineDialog", "ToggleFilterPanel", "ToggleTheme",
    ];

    fn assert_has_required(table: &[KeyBindingSpec], label: &str) {
        let actions: Vec<&str> = table.iter().map(|b| b.action.as_str()).collect();
        for expected in REQUIRED_ACTIONS {
            assert!(
                actions.contains(expected),
                "{label} keymap missing {expected}"
            );
        }
    }

    #[test]
    fn default_has_all_actions() {
        assert_has_required(&Keymap::default().bindings, "current-platform");
    }

    #[test]
    fn macos_defaults_have_all_actions() {
        assert_has_required(&default_bindings_macos(), "macOS");
    }

    #[test]
    fn linux_defaults_have_all_actions() {
        assert_has_required(&default_bindings_linux(), "Linux");
    }

    #[test]
    fn windows_defaults_have_all_actions() {
        assert_has_required(&default_bindings_windows(), "Windows");
    }

    #[test]
    fn linux_avoids_ctrl_shift_r_for_filtered_view() {
        // The browser hard-reload muscle memory is the reason for the swap.
        let has_conflict = default_bindings_linux()
            .iter()
            .any(|b| b.key == "secondary-shift-r" && b.action == "ToggleFilteredView");
        assert!(!has_conflict, "Linux defaults must not bind ToggleFilteredView to secondary-shift-r");
    }

    #[test]
    fn windows_avoids_ctrl_shift_r_for_filtered_view() {
        let has_conflict = default_bindings_windows()
            .iter()
            .any(|b| b.key == "secondary-shift-r" && b.action == "ToggleFilteredView");
        assert!(!has_conflict, "Windows defaults must not bind ToggleFilteredView to secondary-shift-r");
    }

    #[test]
    fn json_roundtrip() {
        let km = Keymap::default();
        let json = serde_json::to_string_pretty(&km).unwrap();
        let restored: Keymap = serde_json::from_str(&json).unwrap();
        assert_eq!(km, restored);
    }

    #[test]
    fn partial_json_uses_defaults_for_missing_fields() {
        // Empty object falls back to default bindings (because of #[serde(default)])
        let km: Keymap = serde_json::from_str("{}").unwrap();
        assert_eq!(km, Keymap::default());
    }

    #[test]
    fn empty_bindings_list_is_respected() {
        // Explicit empty list should NOT be replaced by defaults.
        let km: Keymap = serde_json::from_str(r#"{"bindings": []}"#).unwrap();
        assert!(km.bindings.is_empty());
    }

    #[test]
    fn set_binding_adds_new_binding() {
        let mut km = Keymap { bindings: vec![] };
        let removed = km.set_binding("ctrl-shift-x", "OpenFile");
        assert!(removed.is_none());
        assert_eq!(km.bindings.len(), 1);
        assert_eq!(km.bindings[0].key, "ctrl-shift-x");
        assert_eq!(km.bindings[0].action, "OpenFile");
    }

    #[test]
    fn set_binding_replaces_existing_key_and_reports_old_action() {
        let mut km = Keymap {
            bindings: vec![
                KeyBindingSpec::new("secondary-f", "ToggleQuickFind"),
                KeyBindingSpec::new("secondary-g", "FindNext"),
            ],
        };
        let removed = km.set_binding("secondary-f", "OpenFile");
        assert_eq!(removed.as_deref(), Some("ToggleQuickFind"));
        assert_eq!(km.bindings.len(), 2);
        assert!(km.bindings.iter().any(|b| b.key == "secondary-f" && b.action == "OpenFile"));
        assert!(!km.bindings.iter().any(|b| b.action == "ToggleQuickFind"));
    }

    #[test]
    fn set_binding_is_idempotent_for_same_key_action_pair() {
        let mut km = Keymap {
            bindings: vec![KeyBindingSpec::new("secondary-f", "ToggleQuickFind")],
        };
        let removed = km.set_binding("secondary-f", "ToggleQuickFind");
        assert!(removed.is_none(), "no conflict to report when re-binding identical pair");
        assert_eq!(km.bindings.len(), 1, "must not duplicate the same binding");
    }

    #[test]
    fn clear_binding_removes_all_bindings_for_action() {
        let mut km = Keymap {
            bindings: vec![
                KeyBindingSpec::new("secondary-g", "FindNext"),
                KeyBindingSpec::new("f3", "FindNext"),
                KeyBindingSpec::new("secondary-f", "ToggleQuickFind"),
            ],
        };
        let removed = km.clear_binding("FindNext");
        assert_eq!(removed, 2);
        assert_eq!(km.bindings.len(), 1);
        assert_eq!(km.bindings[0].action, "ToggleQuickFind");
    }

    #[test]
    fn clear_binding_returns_zero_for_unknown_action() {
        let mut km = Keymap { bindings: vec![KeyBindingSpec::new("f1", "ShowHelp")] };
        assert_eq!(km.clear_binding("DoesNotExist"), 0);
        assert_eq!(km.bindings.len(), 1);
    }
}
