//! Integration tests for the keymap registry.
//!
//! Placed under `tests/` (not in a `#[cfg(test)] mod`) to avoid the
//! recursion limit issues that other ui modules trip over during macro
//! expansion. The lib is compiled in non-test mode here.

use orange_settings::{KeyBindingSpec, Keymap};
use orange_ui::keymap::{build_key_bindings, KNOWN_ACTIONS};

#[test]
fn every_default_action_resolves() {
    let km = Keymap::default();
    let built = build_key_bindings(&km.bindings);
    assert_eq!(
        built.len(),
        km.bindings.len(),
        "some default action names failed to resolve in build_key_bindings"
    );
}

#[test]
fn unknown_action_is_skipped() {
    let specs = vec![
        KeyBindingSpec::new("ctrl-o", "OpenFile"),
        KeyBindingSpec::new("ctrl-x", "NoSuchAction"),
    ];
    let built = build_key_bindings(&specs);
    assert_eq!(built.len(), 1, "unknown action should be skipped, not crash");
}

#[test]
fn known_actions_list_matches_default_keymap() {
    let km = Keymap::default();
    let default_actions: std::collections::HashSet<&str> =
        km.bindings.iter().map(|b| b.action.as_str()).collect();
    let known: std::collections::HashSet<&str> = KNOWN_ACTIONS.iter().copied().collect();

    // Every default action must appear in KNOWN_ACTIONS (resolvable).
    for action in &default_actions {
        assert!(
            known.contains(action),
            "default keymap references {action:?} but KNOWN_ACTIONS does not list it",
        );
    }

    // Every KNOWN_ACTIONS entry must appear in defaults (no orphan registry entries).
    for action in &known {
        assert!(
            default_actions.contains(action),
            "KNOWN_ACTIONS lists {action:?} but no default binding uses it",
        );
    }
}

#[test]
fn asset_default_keymap_matches_in_memory_default() {
    // Pick the asset that matches the host OS. The Keymap::default() value
    // is itself cfg-selected, so the comparison stays per-platform.
    #[cfg(target_os = "macos")]
    let (asset, name) = (
        include_str!("../../../assets/keymaps/default-macos.json"),
        "default-macos.json",
    );
    #[cfg(target_os = "windows")]
    let (asset, name) = (
        include_str!("../../../assets/keymaps/default-windows.json"),
        "default-windows.json",
    );
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let (asset, name) = (
        include_str!("../../../assets/keymaps/default-linux.json"),
        "default-linux.json",
    );

    let parsed: Keymap = serde_json::from_str(asset)
        .unwrap_or_else(|e| panic!("assets/keymaps/{name} must be valid Keymap JSON: {e}"));
    assert_eq!(
        parsed,
        Keymap::default(),
        "assets/keymaps/{name} diverged from Keymap::default(); regenerate the asset",
    );
}

#[test]
fn empty_keymap_produces_no_bindings() {
    let built = build_key_bindings(&[]);
    assert!(built.is_empty());
}
