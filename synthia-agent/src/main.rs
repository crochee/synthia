use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::str::FromStr;
use tokio::io::AsyncBufReadExt;
use synthia_agent::clients::{create_llm_client, LLMClient, Message, MessageRole, OpenAIClient, ToolDefinition};
use synthia_agent::core::ReactAgent;
use synthia_agent::mcp::{load_mcp_config, MCPManager};
use synthia_agent::tools::default_tools;
use tokio::io::{self, AsyncWriteExt};

#[derive(Parser, Debug)]
#[command(name = "synthia-agent")]
#[command(author = "Synthia")]
#[command(version = "0.1.0")]
#[command(about = "AI-powered coding assistant", long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, global = true)]
    api_key: Option<String>,

    #[arg(short, long, global = true, default_value = "gpt-4o")]
    model: String,

    #[arg(short, long, global = true)]
    provider: Option<String>,

    #[arg(short, long, global = true, help = "Base URL for the LLM API")]
    base_url: Option<String>,

    #[arg(short, long, global = true, default_value = ".")]
    workdir: PathBuf,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Run a task with the agent")]
    Run {
        #[arg(short, long, help = "Task description")]
        task: String,

        #[arg(short = 's', long, help = "Maximum steps")]
        max_steps: Option<usize>,

        #[arg(long, help = "No streaming output")]
        no_stream: bool,
    },

    #[command(about = "Interactive mode")]
    Interactive {
        #[arg(long, help = "Maximum steps")]
        max_steps: Option<usize>,

        #[arg(long, help = "No streaming output")]
        no_stream: bool,
    },

    #[command(about = "Check MCP configuration")]
    CheckMcp {
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
}

fn get_api_key() -> Result<String, String> {
    std::env::var("OPENAI_API_KEY").map_err(|_| {
        "API key not found. Please set OPENAI_API_KEY environment variable or use --api-key flag.".to_string()
    })
}

async fn handle_streaming_output(
    agent: &mut ReactAgent,
    task: &str,
) -> Result<()> {
    let mut buffer = io::stdout();
    let mut step_num = 0;

    let step_callback = |step_idx: usize, step: synthia_agent::core::Step| {
        let _ = buffer.write_all(format!("\n--- Step {} ---\n", step_idx).as_bytes());
        let _ = buffer.write_all(format!("Thought: {}\n", step.thought).as_bytes());

        if !step.action.is_empty() {
            let _ = buffer.write_all(format!("Action: {}\n", step.action).as_bytes());
            let _ = buffer.write_all(format!("Action Input: {}\n", step.action_input).as_bytes());
        }

        if !step.observation.is_empty() {
            let _ = buffer.write_all(format!("Observation: {}\n", step.observation).as_bytes());
        }

        let _ = buffer.write_all(b"\n> ");
        let _ = buffer.flush();
    };

    let steps = agent.run(task).await?;

    let _ = buffer.write_all(b"\n=== Execution Complete ===\n\n").await;
    let _ = buffer.write_all(format!("Total steps: {}\n", steps.len()).as_bytes());

    for (i, step) in steps.iter().enumerate() {
        let _ = buffer.write_all(format!("{}. {}: {}", i + 1, step.action, step.observation).as_bytes());
    }

    let _ = buffer.write_all(b"\n").await;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let workdir = args.workdir.clone();
    let max_steps = match &args.command {
        Commands::Run { max_steps, .. } => *max_steps,
        Commands::Interactive { max_steps, .. } => *max_steps,
        _ => Some(50),
    };

    match &args.command {
        Commands::Run { task, no_stream, .. } => {
            let api_key = match args.api_key {
                Some(key) => key,
                None => get_api_key().map_err(|e| anyhow::anyhow!(e))?,
            };

            let client = OpenAIClient::new(api_key, args.model.clone(), args.base_url.clone());

            let tools = default_tools(workdir.clone());

            let mut agent = ReactAgent::new(
                Box::new(client),
                tools,
                workdir.clone(),
                max_steps,
                Some(true),
                None,
            );

            println!("Starting agent with task: {}", task);
            println!("Working directory: {:?}", workdir);
            println!("Press Ctrl+C to interrupt...\n");

            if *no_stream {
                let steps = agent.run(task).await?;
                println!("\n=== Execution Complete ===");
                println!("Total steps: {}", steps.len());
            } else {
                handle_streaming_output(&mut agent, task).await?;
            }
        }

        Commands::Interactive { no_stream, .. } => {
            let api_key = match args.api_key {
                Some(key) => key,
                None => get_api_key().map_err(|e| anyhow::anyhow!(e))?,
            };

            let client = OpenAIClient::new(api_key, args.model.clone(), args.base_url.clone());

            let tools = default_tools(workdir.clone());

            let mut agent = ReactAgent::new(
                Box::new(client),
                tools,
                workdir.clone(),
                max_steps,
                Some(true),
                None,
            );

            println!("Interactive mode started. Type 'exit' or 'quit' to end.");
            println!("Working directory: {:?}", workdir);
            println!();

            let stdin = tokio::io::stdin();
            let mut reader = tokio::io::BufReader::new(stdin);
            let mut line = String::new();

            loop {
                print!("> ");
                io::stdout().flush().await?;

                line.clear();
                reader.read_line(&mut line).await?;

                let input = line.trim();

                if input.is_empty() {
                    continue;
                }

                if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
                    println!("Goodbye!");
                    break;
                }

                if *no_stream {
                    let steps = agent.run(input).await?;
                    println!("\n=== Execution Complete ===");
                    println!("Total steps: {}", steps.len());
                } else {
                    handle_streaming_output(&mut agent, input).await?;
                }

                println!();
            }
        }

        Commands::CheckMcp { config } => {
            let config_path = config.clone().unwrap_or_else(|| PathBuf::from("mcp_config.json"));

            println!("Checking MCP configuration at: {:?}", config_path);

            match load_mcp_config(&config_path).await {
                Ok(config) => {
                    println!("MCP Configuration loaded successfully.");
                    println!("Number of configured servers: {}", config.servers.len());

                    for (name, server_config) in &config.servers {
                        println!("  - {}: {} {:?}", name, server_config.command, server_config.args);
                    }
                }
                Err(e) => {
                    println!("Failed to load MCP configuration: {}", e);
                    println!("Using default (empty) MCP configuration.");
                }
            }
        }
    }

    Ok(())
}
