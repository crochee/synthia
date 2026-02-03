use anyhow::{Context, Result};
use synthia_examples::{ProviderConfig, create_session, send_message};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = ProviderConfig::from_env_auto()?;

    println!("=== {} Model Chat Example ===", config.provider_type());
    println!(
        "Connecting to {}...\nModel: {}",
        config.base_url, config.model_name
    );

    let provider = config.create_provider();
    let (session_manager, session_config) =
        create_session(config.default_message())
            .await
            .context("Failed to create session")?;

    send_message(
        CancellationToken::new(),
        &provider,
        session_manager,
        &session_config,
        &config.model_name,
        None,
        None,
    )
    .await?;

    println!("\n\n=== Complete ===");
    println!("Example completed!");

    Ok(())
}
