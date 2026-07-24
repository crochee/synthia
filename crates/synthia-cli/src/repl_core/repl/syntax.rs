//! Syntax highlighting for code blocks rendered in the REPL.
//!
//! This module provides basic ANSI-styled highlighting for code fences
//! (```` ```rust ... ``` ````) inside agent output. Highlighting is
//! intentionally lightweight — language support is limited to the
//! languages most commonly produced by LLMs (rust, python, js/ts, bash),
//! and the rules are simple regex passes over a small keyword set.
//!
//! ## Why a separate module
//!
//! Extracted from `repl.rs` so the dispatch loop and state plumbing in
//! the parent module stay focused on REPL mechanics, not presentation.
//! The highlighter is pure (text in, text out) and has no `Repl` state
//! dependencies, so it can be unit-tested in isolation.

use crossterm::style::{Color, Stylize};
use regex::Regex;

use crate::theme::Theme;

/// Lazy-compiled regex for code block detection (````language ... ````).
fn code_block_regex() -> Regex {
    Regex::new(r"(?s)```(\w*)\n(.*?)```")
        .expect("code block regex should compile")
}

/// Regex patterns for basic syntax highlighting within code blocks.
fn rust_keywords() -> Regex {
    Regex::new(
        r"\b(fn|let|mut|pub|struct|enum|impl|trait|for|if|else|loop|match|return|use|mod|where|async|await|move|ref|self|Self|static|type|const|dyn|unsafe|extern|crate|super|in|as|box)\b",
    )
    .expect("rust keywords regex should compile")
}

fn comment_regex() -> Regex {
    Regex::new(r"(//.*$|/\*.*?\*/)").expect("comment regex should compile")
}

fn string_regex() -> Regex {
    Regex::new(r#""(?:[^"\\]|\\.)*""#).expect("string regex should compile")
}

/// Format text with code block detection and basic syntax highlighting (Task 10.14).
pub fn format_with_syntax_highlighting(text: &str, theme: &Theme) -> String {
    let code_re = code_block_regex();
    let mut result = String::new();
    let mut last_end = 0;

    for cap in code_re.captures_iter(text) {
        let full_match = cap.get(0).unwrap();
        let lang = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let code = cap.get(2).map(|m| m.as_str()).unwrap_or("");

        // Push any text before this code block
        result.push_str(&text[last_end..full_match.start()]);

        // Format the code block with syntax highlighting
        let highlighted = highlight_code_block(lang, code, theme);
        result.push_str(&highlighted);

        last_end = full_match.end();
    }

    // Push remaining text after last code block
    result.push_str(&text[last_end..]);
    result
}

/// Apply basic syntax highlighting to a code block (Task 10.14).
fn highlight_code_block(lang: &str, code: &str, theme: &Theme) -> String {
    let mut result = String::new();

    // Add backtick fence with language tag
    if !lang.is_empty() {
        result.push_str(&format!("```{lang}\n"));
    } else {
        result.push_str("```\n");
    }

    // Apply syntax highlighting for recognized languages
    let highlighted = match lang {
        "rust" => highlight_rust_code(code, theme),
        "python" | "py" => highlight_python_code(code, theme),
        "javascript" | "js" | "typescript" | "ts" => {
            highlight_js_code(code, theme)
        }
        "bash" | "sh" => highlight_bash_code(code, theme),
        _ => theme.format_text(code),
    };

    result.push_str(&highlighted);
    result.push_str("\n```");
    result
}

/// Basic Rust syntax highlighting (Task 10.14).
pub fn highlight_rust_code(code: &str, theme: &Theme) -> String {
    let mut result = code.to_string();

    // Apply comments first (to avoid conflicts with other patterns)
    result = apply_color_regex(&result, &comment_regex(), Color::DarkGrey);

    // Apply strings (avoid coloring inside comments)
    result = apply_color_regex(&result, &string_regex(), Color::Green);

    // Apply keywords (avoid coloring inside comments/strings)
    result =
        apply_color_regex(&result, &rust_keywords(), theme.tool_call_color);

    result
}

/// Basic Python syntax highlighting (Task 10.14).
fn highlight_python_code(code: &str, theme: &Theme) -> String {
    let mut result = code.to_string();
    result = apply_color_regex(
        &result,
        &Regex::new(r"#.*$").unwrap(),
        Color::DarkGrey,
    );
    result = apply_color_regex(&result, &string_regex(), Color::Green);
    result = apply_color_regex(
        &result,
        &Regex::new(
            r"\b(def|class|if|elif|else|for|while|return|import|from|as|with|try|except|finally|raise|pass|break|continue|and|or|not|is|in|lambda|yield|async|await|self|True|False|None)\b",
        )
        .unwrap(),
        theme.tool_call_color,
    );
    result
}

/// Basic JavaScript/TypeScript syntax highlighting (Task 10.14).
fn highlight_js_code(code: &str, theme: &Theme) -> String {
    let mut result = code.to_string();
    result = apply_color_regex(&result, &comment_regex(), Color::DarkGrey);
    result = apply_color_regex(&result, &string_regex(), Color::Green);
    result = apply_color_regex(
        &result,
        &Regex::new(
            r"\b(const|let|var|function|class|if|else|for|while|return|import|export|from|async|await|try|catch|finally|throw|new|this|self|true|false|null|undefined|typeof|instanceof)\b",
        )
        .unwrap(),
        theme.tool_call_color,
    );
    result
}

/// Basic Bash syntax highlighting (Task 10.14).
fn highlight_bash_code(code: &str, theme: &Theme) -> String {
    let mut result = code.to_string();
    result = apply_color_regex(
        &result,
        &Regex::new(r"#.*$").unwrap(),
        Color::DarkGrey,
    );
    result = apply_color_regex(&result, &string_regex(), Color::Green);
    result = apply_color_regex(
        &result,
        &Regex::new(
            r"\b(if|then|else|elif|fi|for|while|do|done|case|esac|function|return|exit|echo|export|source|local)\b",
        )
        .unwrap(),
        theme.tool_call_color,
    );
    result
}

/// Apply a regex pattern with a color, being careful not to re-color already colored regions.
fn apply_color_regex(text: &str, re: &Regex, color: Color) -> String {
    let mut result = String::new();
    let mut last_end = 0;

    for cap in re.captures_iter(text) {
        let full_match = cap.get(0).unwrap();

        // Skip if this match is inside an ANSI escape sequence region
        let before = &text[last_end..full_match.start()];
        if before.contains("\x1b[") {
            continue;
        }

        result.push_str(&text[last_end..full_match.start()]);
        result.push_str(&cap[0].with(color).to_string());
        last_end = full_match.end();
    }

    result.push_str(&text[last_end..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_with_no_code_blocks() {
        let theme = Theme::default();
        let text = "Just plain text\nwith multiple lines";
        let result = format_with_syntax_highlighting(text, &theme);
        assert!(result.contains("plain text"));
    }

    #[test]
    fn test_format_with_rust_code_block() {
        let theme = Theme::default();
        let text = "Here is some code:\n```rust\nfn main() {}\n```\nDone";
        let result = format_with_syntax_highlighting(text, &theme);
        assert!(result.contains("```rust"));
        assert!(result.contains("```"));
        assert!(result.contains("Done"));
    }

    #[test]
    fn test_format_with_untyped_code_block() {
        let theme = Theme::default();
        let text = "```\nsome code\n```";
        let result = format_with_syntax_highlighting(text, &theme);
        assert!(result.contains("```\n"));
    }

    #[test]
    fn test_highlight_rust_basic() {
        let theme = Theme::default();
        let code = "fn main() { let x = 1; }";
        let result = highlight_rust_code(code, &theme);
        // Should contain the original code
        assert!(result.contains("fn"));
        assert!(result.contains("main"));
    }

    #[test]
    fn test_highlight_python_basic() {
        let theme = Theme::default();
        let code = "def hello():\n    pass";
        let result = highlight_python_code(code, &theme);
        assert!(result.contains("def"));
    }

    #[test]
    fn test_highlight_js_basic() {
        let theme = Theme::default();
        let code = "const x = 1;";
        let result = highlight_js_code(code, &theme);
        assert!(result.contains("const"));
    }

    #[test]
    fn test_highlight_bash_basic() {
        let theme = Theme::default();
        let code = "if [ -f file ]; then echo yes; fi";
        let result = highlight_bash_code(code, &theme);
        assert!(result.contains("if"));
    }

    #[test]
    fn test_highlight_code_block_rust() {
        let theme = Theme::default();
        let code = "fn main() {}";
        let result = highlight_code_block("rust", code, &theme);
        assert!(result.contains("```rust"));
    }

    #[test]
    fn test_highlight_code_block_unknown_lang() {
        let theme = Theme::default();
        let code = "some unknown code";
        let result = highlight_code_block("xyz", code, &theme);
        // No language tag, but still wrapped
        assert!(result.contains("```"));
    }

    #[test]
    fn test_apply_color_regex_basic() {
        let re = Regex::new(r"foo").unwrap();
        let result = apply_color_regex("hello foo world", &re, Color::Red);
        assert!(result.contains("hello"));
        assert!(result.contains("foo"));
        assert!(result.contains("world"));
    }

    #[test]
    fn test_apply_color_regex_skips_inside_ansi() {
        let re = Regex::new(r"foo").unwrap();
        // Text with ANSI sequence - the foo after \x1b[ should be skipped
        let text = "before \x1b[31mfoo after";
        let result = apply_color_regex(text, &re, Color::Red);
        // Result should not duplicate "foo"
        assert_eq!(result.matches("foo").count(), 1);
    }

    #[test]
    fn test_format_multiple_code_blocks() {
        let theme = Theme::default();
        let text = "```rust\nlet x = 1;\n```\nBetween\n```python\nx = 2\n```";
        let result = format_with_syntax_highlighting(text, &theme);
        assert!(result.contains("Between"));
        assert!(result.contains("let"));
        assert!(result.contains("x = 2"));
    }
}
