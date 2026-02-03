use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "synthia-cli")]
#[command(version = "0.1.0")]
#[command(about = "Synthia CLI - Interactive chat tool with AI agents", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(long, short = 'm', help = "Model to use")]
    pub model: Option<String>,

    #[arg(
        short,
        long,
        default_value = "config.yaml",
        help = "Path to configuration file"
    )]
    pub config: PathBuf,

    #[arg(short, long, help = "Working directory")]
    pub directory: Option<PathBuf>,

    #[arg(
        short,
        long,
        default_value = "error",
        help = "Log level (error, warn, info, debug)"
    )]
    pub log_level: String,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(about = "Start an interactive chat session")]
    Chat,

    #[command(about = "Resume an existing session")]
    Resume {
        #[arg(short, long, help = "Session ID to resume")]
        session_id: Option<String>,

        #[arg(long, default_value_t = false, help = "Resume the last session")]
        last: bool,
    },

    #[command(about = "Fork an existing session to a new one")]
    Fork {
        #[arg(short, long, help = "Session ID to fork")]
        session_id: Option<String>,

        #[arg(long, default_value_t = false, help = "Fork the last session")]
        last: bool,
    },

    #[command(about = "Show configuration")]
    Config {
        #[arg(long, help = "Show full configuration")]
        verbose: bool,
    },

    #[command(about = "Run a single query in non-interactive mode")]
    Run {
        #[arg(help = "The query to execute")]
        query: Option<String>,

        #[arg(short, long, help = "Read queries from file (one per line)")]
        file: Option<PathBuf>,

        #[arg(short, long, help = "Use specific agent")]
        agent: Option<String>,

        #[arg(short, long, help = "Output format", default_value = "text")]
        output_format: OutputFormat,

        #[arg(
            short,
            long,
            help = "Maximum steps for agent execution",
            default_value = "30"
        )]
        max_steps: u32,

        #[arg(short, long, help = "Session ID")]
        session_id: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
    Jsonl,
}
