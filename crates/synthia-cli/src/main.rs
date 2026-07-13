use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use synthia_cli::{
    cli::{Commands, SkillCommands},
    repl,
    run_wire_client,
    skill_cmd,
    workspace,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(
    name = "synthia",
    about = "Synthia AI Agent - Interactive CLI",
    version = VERSION,
    long_about = None
)]
struct Cli {
    /// Path to the workspace directory
    #[arg(short, long, default_value = ".")]
    workspace: PathBuf,

    /// Connect to a remote server over the wire protocol
    /// (`synthia_protocol::Submission` over HTTP, `EventMsg` over
    /// WebSocket) instead of running the in-process REPL.
    ///
    /// Example: `--wire http://localhost:8080`
    #[arg(long, value_name = "SERVER_URL")]
    wire: Option<String>,

    /// Initialize a new workspace without starting REPL
    #[command(subcommand)]
    command: Option<Commands>,
}

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    if let Some(server_url) = cli.wire.as_deref() {
        // Round 6: wire-protocol mode. Skip the REPL entirely.
        let rt = tokio::runtime::Runtime::new()?;
        return rt.block_on(run_wire_client(server_url));
    }

    match cli.command {
        Some(Commands::Init) => {
            let info = workspace::ensure_workspace(&cli.workspace)?;
            println!("Workspace initialized at: {}", info.root.display());
            Ok(())
        }
        Some(Commands::Skill(skill_cmd_inner)) => match skill_cmd_inner {
            SkillCommands::List { json } => {
                skill_cmd::list_skills(&cli.workspace, json)
            }
            SkillCommands::Info { name, json } => {
                skill_cmd::show_skill_info(&cli.workspace, &name, json)
            }
            SkillCommands::Validate { path } => {
                skill_cmd::validate_skill(&path, false)
            }
            SkillCommands::Install { path, hash } => {
                skill_cmd::install_skill(&cli.workspace, &path, hash.as_deref())
            }
            SkillCommands::Uninstall { name } => {
                skill_cmd::uninstall_skill(&cli.workspace, &name)
            }
            SkillCommands::Installed { json } => {
                skill_cmd::list_installed_skills(&cli.workspace, json)
            }
            SkillCommands::Stats { json } => {
                skill_cmd::show_skill_stats(&cli.workspace, json)
            }
            SkillCommands::Report { name, json } => {
                skill_cmd::show_skill_report(&cli.workspace, &name, json)
            }
        },
        None => {
            // Verify or initialize workspace before entering REPL
            let info = workspace::ensure_workspace(&cli.workspace)?;

            // Enter REPL loop
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(repl::run(&info))?;

            Ok(())
        }
    }
}
