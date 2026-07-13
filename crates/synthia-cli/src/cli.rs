/// CLI argument definitions using clap.
///
/// Note: The actual CLI entry point is in `main.rs`. This module provides
/// reusable argument structures for programmatic CLI usage.
use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Synthia AI Agent CLI
#[derive(Parser)]
#[command(
    name = "synthia",
    about = "Synthia AI Agent - Interactive CLI",
    version = env!("CARGO_PKG_VERSION"),
)]
pub struct CliArgs {
    /// Path to the workspace directory
    #[arg(short, long, default_value = ".")]
    pub workspace: PathBuf,

    /// Connect to a remote server over the wire protocol
    /// (`synthia_protocol::Submission` over HTTP, `EventMsg` over
    /// WebSocket) instead of running the in-process REPL.
    ///
    /// Example: `--wire http://localhost:8080`
    #[arg(long, value_name = "SERVER_URL")]
    pub wire: Option<String>,

    /// Subcommands
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Available subcommands
#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new workspace
    Init,
    /// Manage skills
    #[command(subcommand)]
    Skill(SkillCommands),
}

/// Skill management subcommands
#[derive(Subcommand)]
pub enum SkillCommands {
    /// List all loaded skills
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show detailed information about a skill
    Info {
        /// Name of the skill
        name: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Validate a SKILL.md file format
    Validate {
        /// Path to the SKILL.md file
        path: PathBuf,
    },
    /// Install a skill from a .skill ZIP archive
    Install {
        /// Path to the .skill ZIP archive
        path: PathBuf,
        /// Expected SHA-256 hash of the archive for integrity verification
        #[arg(long)]
        hash: Option<String>,
    },
    /// Uninstall a skill by name
    Uninstall {
        /// Name of the skill to uninstall
        name: String,
    },
    /// List installed user skills
    Installed {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show global skill usage statistics
    Stats {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show usage report for a specific skill
    Report {
        /// Name of the skill
        name: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn parses_wire_flag_correctly() {
        let args = CliArgs::try_parse_from([
            "synthia",
            "--wire",
            "http://localhost:8080",
        ])
        .expect("--wire should parse");
        assert_eq!(
            args.wire.as_deref(),
            Some("http://localhost:8080"),
            "--wire <SERVER_URL> must capture the URL value"
        );
        assert!(args.command.is_none());
    }
}
