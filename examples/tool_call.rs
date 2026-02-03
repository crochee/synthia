use anyhow::{Context, Result};
use synthia_examples::{
    ProviderConfig,
    create_session,
    create_tool_registry,
    get_tool_definitions,
    handle_tool_calls_with_session,
    send_message,
};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = ProviderConfig::from_env_auto()?;

    println!("=== {} Tool Call Example ===", config.provider_type());
    println!(
        "Connecting to {}...\nModel: {}",
        config.base_url, config.model_name
    );

    let system_prompt = "你是一个有用的助手。当需要获取当前时间时，请使用 bash 工具执行 'date' 命令。";

    println!("Sending request to model...\n=== Model Response (Streaming) ===");

    let provider = config.create_provider();
    let tool_registry = create_tool_registry().await;
    let tools = get_tool_definitions(&tool_registry);

    let (session_manager, session_config) =
        create_session(config.default_tool_message())
            .await
            .context("Failed to create session")?;
    let cancel_token = CancellationToken::new();
    let pending_tool_calls = send_message(
        cancel_token.clone(),
        &provider,
        std::sync::Arc::clone(&session_manager),
        &session_config,
        &config.model_name,
        Some(tools.clone()),
        Some(system_prompt.to_string()),
    )
    .await?;

    if !pending_tool_calls.is_empty() {
        handle_tool_calls_with_session(
            std::sync::Arc::clone(&session_manager),
            &session_config,
            &tool_registry,
            pending_tool_calls,
        )
        .await?;
    }
    println!("\n=== Sending Tool Results Back to Model ===");
    let pending_tool_calls = send_message(
        cancel_token,
        &provider,
        session_manager,
        &session_config,
        &config.model_name,
        Some(tools),
        Some(system_prompt.to_string()),
    )
    .await?;
    println!("\n\n=== Complete ===");
    if !pending_tool_calls.is_empty() {
        println!("Pending tool calls: {pending_tool_calls:?}");
    }

    Ok(())
}
