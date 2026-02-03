//! ANSI color codes for terminal output
//!
//! This module provides constants for colored terminal output.

/// ANSI color codes
pub mod colors {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const CYAN: &str = "\x1b[36m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const RED: &str = "\x1b[31m";
    pub const MAGENTA: &str = "\x1b[35m";
    pub const GRAY: &str = "\x1b[90m";
    pub const BLUE: &str = "\x1b[34m";
}

use colors::*;

/// Print a separator line
pub fn print_separator(char: char, width: usize) {
    println!("{}{}{}", CYAN, char.to_string().repeat(width), RESET);
}

/// Print a banner with title
pub fn print_banner(title: &str, version: &str) {
    let width = 60;
    print_separator('═', width);
    let padding = (width - title.len() - version.len()) / 2;
    println!(
        "{}{}{}{}{}{}",
        BOLD,
        BLUE,
        " ".repeat(padding),
        title,
        version,
        RESET
    );
    print_separator('═', width);
    println!();
}

/// Print a help line
pub fn print_help_item(command: &str, description: &str) {
    println!("  {}{}{} - {}", YELLOW, command, RESET, description);
}
