//! REPL module - interactive command-line interface
pub mod history;
pub mod repl;

// Re-export all types for backward compatibility with external consumers
pub use repl::{
    CommandAction,
    Repl,
    ReplConfig,
    ReplContext,
    SessionState,
    format_with_syntax_highlighting,
    highlight_rust_code,
    run,
    run_with_context,
};
