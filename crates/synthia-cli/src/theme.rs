use crossterm::style::{Color, Stylize};

/// Color theme configuration for CLI output.
pub struct Theme {
    pub tool_call_color: Color,
    pub text_color: Color,
    pub error_color: Color,
    pub success_color: Color,
    pub prompt_color: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            tool_call_color: Color::Cyan,
            text_color: Color::White,
            error_color: Color::Red,
            success_color: Color::Green,
            prompt_color: Color::Yellow,
        }
    }
}

impl Theme {
    /// Create a new theme with the given colors.
    pub fn new(
        tool_call_color: Color,
        text_color: Color,
        error_color: Color,
        success_color: Color,
        prompt_color: Color,
    ) -> Self {
        Self {
            tool_call_color,
            text_color,
            error_color,
            success_color,
            prompt_color,
        }
    }

    /// Format a tool call label with the theme color.
    pub fn format_tool_call(&self, text: &str) -> String {
        text.with(self.tool_call_color).to_string()
    }

    /// Format error text with the theme color.
    pub fn format_error(&self, text: &str) -> String {
        text.with(self.error_color).to_string()
    }

    /// Format success text with the theme color.
    pub fn format_success(&self, text: &str) -> String {
        text.with(self.success_color).to_string()
    }

    /// Format prompt text with the theme color.
    pub fn format_prompt(&self, text: &str) -> String {
        text.with(self.prompt_color).to_string()
    }

    /// Format regular text with the theme color.
    pub fn format_text(&self, text: &str) -> String {
        text.with(self.text_color).to_string()
    }
}
