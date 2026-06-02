//! Integration tests for the Theme module.

use orange_settings::{ColorScheme, Options};
use orange_ui::theme::{parse_hex_color, Theme};

#[test]
fn parse_hex_rgb() {
    let c = parse_hex_color("#1e1e2e").unwrap();
    assert!(c.l > 0.0 && c.l < 1.0);
    assert_eq!(c.a, 1.0);
}

#[test]
fn parse_hex_rgba_alpha() {
    let c = parse_hex_color("#1e1e2e80").unwrap();
    assert!((c.a - 0.5019608).abs() < 1e-3);
}

#[test]
fn parse_hex_rejects_bad_length() {
    assert!(parse_hex_color("#abc").is_err());
    assert!(parse_hex_color("nope").is_err());
    assert!(parse_hex_color("").is_err());
}

#[test]
fn parse_hex_accepts_without_pound() {
    assert!(parse_hex_color("1e1e2e").is_ok());
}

#[test]
fn dark_and_light_differ() {
    let d = Theme::dark();
    let l = Theme::light();
    assert_ne!(d.background.l, l.background.l);
    assert!(d.background.l < l.background.l);
}

#[test]
fn from_options_flag_picks_dark_when_true() {
    let dark = Theme::from_options_flag(true);
    let light = Theme::from_options_flag(false);
    assert!(dark.background.l < light.background.l);
}

#[test]
fn from_options_uses_active_custom_scheme() {
    let mut opts = Options::default();
    opts.dark_theme = false;
    opts.custom_light.background = "#123456".to_string();

    let theme = Theme::from_options(&opts);
    let expected = parse_hex_color("#123456").unwrap();
    assert_eq!(theme.background, expected);
}

#[test]
fn from_options_falls_back_to_builtin_on_invalid_hex() {
    let mut opts = Options::default();
    opts.dark_theme = true;
    // Half-typed value as the user would have mid-edit in the Theme tab.
    opts.custom_dark.background = "#12".to_string();

    let theme = Theme::from_options(&opts);
    // Should not panic; falls back to the built-in dark scheme.
    assert_eq!(theme.background, Theme::dark().background);
}

#[test]
fn json_roundtrip_via_default_scheme() {
    let scheme = ColorScheme::default();
    let json = serde_json::to_string(&scheme).unwrap();
    let from_json = Theme::from_json(&json).unwrap();
    let direct = Theme::from_color_scheme(&scheme).unwrap();
    assert_eq!(from_json.background, direct.background);
    assert_eq!(from_json.foreground, direct.foreground);
}

#[test]
fn loading_asset_theme_files_succeeds() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root");
    let dark = workspace.join("assets/themes/dark.json");
    let light = workspace.join("assets/themes/light.json");

    let dark_theme = Theme::load_json_file(&dark).expect("dark.json must parse");
    let light_theme = Theme::load_json_file(&light).expect("light.json must parse");

    // Dark from asset should match built-in dark exactly.
    let builtin_dark = Theme::dark();
    assert_eq!(dark_theme.background, builtin_dark.background);
    assert_eq!(dark_theme.foreground, builtin_dark.foreground);

    // Light from asset should match built-in light exactly.
    let builtin_light = Theme::light();
    assert_eq!(light_theme.background, builtin_light.background);
}
