//! orange-settings: Configuration management for Orange.

pub mod keymap;
pub mod options;
pub mod shortcuts;
pub mod style;

pub use keymap::{Keymap, KeyBindingSpec};
pub use options::Options;
pub use shortcuts::Shortcuts;
pub use style::ColorScheme;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_options_default() {
        let opts = Options::default();
        assert_eq!(opts.main_font, "Courier New");
        assert_eq!(opts.main_font_size, 22.0);
        assert!(opts.dark_theme);
    }

    #[test]
    fn test_options_roundtrip() {
        let opts = Options {
            main_font: "Fira Code".to_string(),
            main_font_size: 14.0,
            dark_theme: false,
            ..Default::default()
        };

        let toml_str = toml::to_string_pretty(&opts).unwrap();
        let restored: Options = toml::from_str(&toml_str).unwrap();

        assert_eq!(restored.main_font, "Fira Code");
        assert_eq!(restored.main_font_size, 14.0);
        assert!(!restored.dark_theme);
    }

    #[test]
    fn test_active_scheme_follows_dark_flag() {
        let mut opts = Options::default();
        opts.custom_dark.background = "#101010".to_string();
        opts.custom_light.background = "#fafafa".to_string();

        opts.dark_theme = true;
        assert_eq!(opts.active_scheme().background, "#101010");
        opts.dark_theme = false;
        assert_eq!(opts.active_scheme().background, "#fafafa");

        // Mutable access targets the active scheme too.
        opts.active_scheme_mut().background = "#000000".to_string();
        assert_eq!(opts.custom_light.background, "#000000");
    }

    #[test]
    fn test_custom_schemes_roundtrip_toml() {
        let mut opts = Options::default();
        opts.custom_dark.foreground = "#abcdef".to_string();

        let toml_str = toml::to_string_pretty(&opts).unwrap();
        let restored: Options = toml::from_str(&toml_str).unwrap();
        assert_eq!(restored.custom_dark.foreground, "#abcdef");
        // Untouched fields keep their defaults.
        assert_eq!(restored.custom_light.background, ColorScheme::light().background);
    }

    #[test]
    fn test_options_partial_toml() {
        // Loading partial TOML should fill in defaults for missing fields
        let toml_str = r#"main_font = "Monaco""#;
        let opts: Options = toml::from_str(toml_str).unwrap();

        assert_eq!(opts.main_font, "Monaco");
        assert_eq!(opts.main_font_size, 22.0); // default
    }

    #[test]
    fn test_shortcuts_default() {
        let shortcuts = Shortcuts::default();
        assert!(shortcuts.get("open_file").is_some());
        assert!(shortcuts.get("search_forward").is_some());
        assert!(shortcuts.get("toggle_bookmark").is_some());
        assert!(shortcuts.get("nonexistent").is_none());
    }

    #[test]
    fn test_shortcuts_roundtrip() {
        let shortcuts = Shortcuts::default();
        let toml_str = toml::to_string_pretty(&shortcuts).unwrap();
        let restored: Shortcuts = toml::from_str(&toml_str).unwrap();

        assert_eq!(
            restored.get("open_file").unwrap().key,
            shortcuts.get("open_file").unwrap().key
        );
    }

    #[test]
    fn test_color_scheme_default() {
        let scheme = ColorScheme::default();
        assert_eq!(scheme.background, "#222222");
        assert_eq!(scheme.foreground, "#dbd7ca");
    }

    #[test]
    fn test_color_scheme_roundtrip() {
        let scheme = ColorScheme {
            background: "#000000".to_string(),
            ..Default::default()
        };

        let toml_str = toml::to_string_pretty(&scheme).unwrap();
        let restored: ColorScheme = toml::from_str(&toml_str).unwrap();

        assert_eq!(restored.background, "#000000");
        assert_eq!(restored.foreground, "#dbd7ca"); // default
    }
}
