use std::env;

use synthia_message_proxy::MessageProxyServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let addr = env::var("MESSAGE_PROXY_ADDR")
        .unwrap_or_else(|_| "/var/run/synthia/message-proxy.sock".to_string());

    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(&addr).parent() {
        std::fs::create_dir_all(parent)?;
    }

    let server = MessageProxyServer::new(addr.clone());
    tracing::info!("MessageProxy listening on {}", addr);

    server.serve().await
}
