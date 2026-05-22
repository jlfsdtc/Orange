//! Scratchpad: a panel for notes and quick calculations.
//!
//! Can be toggled with the ToggleScratchpad shortcut (Cmd+Shift+S on macOS,
//! Ctrl+Shift+S elsewhere). Supports plain text notes
//! and simple arithmetic expressions.

use gpui::*;

use crate::theme::Theme;

// Scratchpad actions.
actions!(orange, [ToggleScratchpad]);

/// Scratchpad state.
pub struct ScratchpadState {
    /// Whether the scratchpad is visible.
    visible: bool,
    /// Current text content.
    content: String,
    /// Computed result (if content is an expression).
    result: Option<String>,
}

impl ScratchpadState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        Self {
            visible: false,
            content: String::new(),
            result: None,
        }
    }

    /// Toggle visibility.
    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.visible = !self.visible;
        cx.notify();
    }

    /// Whether the scratchpad is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Set content and evaluate if it's an expression.
    pub fn set_content(&mut self, content: String, cx: &mut Context<Self>) {
        self.result = Self::try_evaluate(&content);
        self.content = content;
        cx.notify();
    }

    /// Get the current content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get the computed result.
    pub fn result(&self) -> Option<&str> {
        self.result.as_deref()
    }

    /// Try to evaluate a simple arithmetic expression.
    fn try_evaluate(expr: &str) -> Option<String> {
        let expr = expr.trim();
        if expr.is_empty() {
            return None;
        }

        // Try simple arithmetic: number op number
        // Support: +, -, *, /, %
        for op in &['+', '-', '*', '/', '%'] {
            if let Some(idx) = expr.find(*op) {
                if idx == 0 {
                    continue; // Skip leading operator (negative numbers)
                }
                let left = &expr[..idx];
                let right = &expr[idx + 1..];

                if let (Ok(l), Ok(r)) = (left.trim().parse::<f64>(), right.trim().parse::<f64>()) {
                    let result = match op {
                        '+' => l + r,
                        '-' => l - r,
                        '*' => l * r,
                        '/' => {
                            if r == 0.0 {
                                return Some("Error: division by zero".to_string());
                            }
                            l / r
                        }
                        '%' => {
                            if r == 0.0 {
                                return Some("Error: division by zero".to_string());
                            }
                            l % r
                        }
                        _ => continue,
                    };

                    // Format result (remove trailing zeros for integers)
                    if result.fract() == 0.0 && result.abs() < i64::MAX as f64 {
                        return Some(format!("{}", result as i64));
                    } else {
                        return Some(format!("{}", result));
                    }
                }
            }
        }

        // Try hex conversion
        if expr.starts_with("0x") || expr.starts_with("0X") {
            if let Ok(val) = i64::from_str_radix(&expr[2..], 16) {
                return Some(format!("{} (decimal)", val));
            }
        }

        // Try decimal to hex
        if let Ok(val) = expr.parse::<i64>() {
            return Some(format!("0x{:X} (hex)", val));
        }

        None
    }

    /// Render the scratchpad panel.
    pub fn render_panel(&self, theme: Theme) -> Option<AnyElement> {
        if !self.visible {
            return None;
        }


        Some(
            div()
                .flex()
                .flex_col()
                .h(px(200.0))
                .bg(theme.background)
                .border_t_1()
                .border_color(theme.selection)
                .child(
                    // Header
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .py_1()
                        .bg(theme.current_line)
                        .border_b_1()
                        .border_color(theme.selection)
                        .child(
                            div()
                                .text_color(theme.foreground)
                                .child("Scratchpad"),
                        )
                        .child(
                            div()
                                .text_color(theme.line_number)
                                .text_sm()
                                .child(if cfg!(target_os = "macos") {
                                    "Cmd+Shift+S to toggle"
                                } else {
                                    "Ctrl+Shift+S to toggle"
                                }),
                        ),
                )
                .child(
                    // Content area
                    div()
                        .flex_grow()
                        .px_3()
                        .py_2()
                        .text_color(theme.foreground)
                        .child(
                            if self.content.is_empty() {
                                div()
                                    .text_color(theme.line_number)
                                    .child("Type notes or expressions here...")
                                    .into_any()
                            } else {
                                div()
                                    .child(self.content.clone())
                                    .into_any()
                            },
                        ),
                )
                .child(
                    // Result area
                    div()
                        .px_3()
                        .py_2()
                        .bg(theme.current_line)
                        .border_t_1()
                        .border_color(theme.selection)
                        .child(
                            div()
                                .text_color(theme.search_match)
                                .child(
                                    self.result
                                        .as_deref()
                                        .unwrap_or("")
                                        .to_string(),
                                ),
                        ),
                )
                .into_any(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scratchpad_evaluator() {
        // Arithmetic
        assert_eq!(ScratchpadState::try_evaluate("2 + 3"), Some("5".to_string()));
        assert_eq!(ScratchpadState::try_evaluate("10 - 4"), Some("6".to_string()));
        assert_eq!(ScratchpadState::try_evaluate("3 * 7"), Some("21".to_string()));
        assert_eq!(ScratchpadState::try_evaluate("15 / 3"), Some("5".to_string()));

        // Division by zero
        assert_eq!(
            ScratchpadState::try_evaluate("1 / 0"),
            Some("Error: division by zero".to_string())
        );

        // Hex conversion
        assert_eq!(
            ScratchpadState::try_evaluate("0xFF"),
            Some("255 (decimal)".to_string())
        );
        assert_eq!(
            ScratchpadState::try_evaluate("255"),
            Some("0xFF (hex)".to_string())
        );

        // No match
        assert_eq!(ScratchpadState::try_evaluate(""), None);
        assert_eq!(ScratchpadState::try_evaluate("hello"), None);
    }
}
