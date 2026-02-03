//! Non-interactive mode for CLI
//!
//! Provides single query execution for automation and testing.

use std::path::Path;

use anyhow::Result;

use crate::{
    cli::OutputFormat,
    config::AppConfig,
    modes::setup_agent_and_session,
    runner::{run_queries_from_file, run_query},
};

/// Run the CLI in non-interactive mode
pub async fn run(
    config: &AppConfig,
    current_dir: &Path,
    session_id: Option<&str>,
    query: Option<&str>,
    file: Option<&Path>,
    output_format: OutputFormat,
) -> Result<()> {
    let (agent, session_config) =
        setup_agent_and_session(config, current_dir, session_id).await?;

    match (query, file) {
        (Some(q), None) => {
            run_query(agent, session_config, q, output_format).await?;
        }
        (None, Some(f)) => {
            run_queries_from_file(agent, session_config, f, output_format)
                .await?;
        }
        (Some(q), Some(_)) => {
            // If both provided, use query
            run_query(agent, session_config, q, output_format).await?;
        }
        (None, None) => {
            anyhow::bail!(
                "Either query or --file must be provided for run command"
            );
        }
    }

    Ok(())
}
