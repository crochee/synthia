//! Synthia CLI - Interactive chat interface
//!
//! This is the command-line interface for Synthia, providing an
//! interactive chat experience with AI models.

mod agent;
mod cli;
mod color;
mod commands;
mod config;
mod handler;
mod input;
mod modes;
mod output;
mod runner;
mod scheduler;

use std::path::Path;

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, Commands};

use crate::config::AppConfig;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let cli = Cli::parse();
    init_logging(&cli.log_level);

    let current_dir = get_current_dir(&cli.directory)?;
    let config = load_config(&current_dir, &cli.config).await?;
    let model_override = cli.model.clone();

    match &cli.command {
        Some(Commands::Config { verbose }) => print_config(&config, *verbose),
        Some(Commands::Resume { session_id, last }) => {
            let (agent, setup) = agent::build_agent(
                &config,
                &current_dir,
                model_override.clone(),
            )
            .await?;
            modes::interactive::run_with_session(
                &config,
                &current_dir,
                agent,
                &setup,
                session_id.as_deref(),
                *last,
                None,
            )
            .await?;
        }
        Some(Commands::Fork { session_id, last }) => {
            let (agent, setup) = agent::build_agent(
                &config,
                &current_dir,
                model_override.clone(),
            )
            .await?;
            modes::interactive::run_with_session(
                &config,
                &current_dir,
                agent,
                &setup,
                session_id.as_deref(),
                *last,
                session_id.as_deref(),
            )
            .await?;
        }
        Some(Commands::Run {
            query,
            file,
            agent: _,
            output_format,
            max_steps: _,
            session_id,
        }) => {
            modes::non_interactive::run(
                &config,
                &current_dir,
                session_id.as_deref(),
                query.as_deref(),
                file.as_deref(),
                *output_format,
            )
            .await?;
        }
        Some(Commands::Chat) | None => {
            let (agent, setup) = agent::build_agent(
                &config,
                &current_dir,
                model_override.clone(),
            )
            .await?;
            modes::interactive::run(&config, &current_dir, agent, &setup)
                .await?;
        }
    }

    tracing::info!("Shutting down Synthia CLI");
    Ok(())
}

fn init_logging(log_level: &str) {
    let config = synthia_tracing::TracingConfig::from_env()
        .with_service_name("synthia-cli")
        .with_log_level(log_level)
        .with_service_version(VERSION);

    if let Err(e) = synthia_tracing::init_tracing(config) {
        eprintln!("Warning: failed to initialize tracing: {}", e);
    }
}

fn get_current_dir(
    directory: &Option<std::path::PathBuf>,
) -> Result<std::path::PathBuf> {
    match directory {
        Some(dir) => Ok(dir.clone()),
        None => {
            std::env::current_dir().context("Failed to get current directory")
        }
    }
}

async fn load_config(
    current_dir: &Path,
    config_path: &Path,
) -> Result<AppConfig> {
    AppConfig::load(&current_dir.join(config_path)).with_context(|| {
        format!("Failed to load config from {}", config_path.display())
    })
}

fn print_config(config: &AppConfig, verbose: bool) {
    if verbose {
        match serde_json::to_string_pretty(config) {
            Ok(json) => println!("{json}"),
            Err(e) => eprintln!("Failed to serialize config: {}", e),
        }
    } else {
        println!(
            "Providers: {:?}",
            config.providers.keys().collect::<Vec<_>>()
        );
        println!("MCPs: {:?}", config.mcps.keys().collect::<Vec<_>>());
        println!("Agents: {:?}", config.agents.keys().collect::<Vec<_>>());
    }
}
