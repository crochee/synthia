use std::sync::{Arc, atomic::Ordering};

use crate::{
    fixtures::configs::TestConfig,
    utils::mock_server::ToolDefinition,
};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum ConnectionState {
    #[default]
    Discovered,
    Connecting,
    Connected,
    Idle,
    Disconnected,
}

#[derive(Debug)]
struct LazyMcpConnection {
    server_name: String,
    state: ConnectionState,
    connect_count: Arc<std::sync::atomic::AtomicUsize>,
    disconnect_count: Arc<std::sync::atomic::AtomicUsize>,
    idle_timeout_secs: u64,
}

impl LazyMcpConnection {
    fn new(server_name: &str, idle_timeout_secs: u64) -> Self {
        Self {
            server_name: server_name.to_string(),
            state: ConnectionState::Discovered,
            connect_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            disconnect_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            idle_timeout_secs,
        }
    }

    fn discover_tools(&self) -> Vec<ToolDefinition> {
        if matches!(
            self.state,
            ConnectionState::Discovered | ConnectionState::Disconnected
        ) {
            vec![
                ToolDefinition {
                    name: format!("{}_tool_1", self.server_name),
                    description: format!("Tool 1 from {}", self.server_name),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "arg1": {"type": "string"}
                        }
                    }),
                },
                ToolDefinition {
                    name: format!("{}_tool_2", self.server_name),
                    description: format!("Tool 2 from {}", self.server_name),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "arg2": {"type": "number"}
                        }
                    }),
                },
            ]
        } else {
            vec![]
        }
    }

    fn connect(&mut self) {
        if !matches!(
            self.state,
            ConnectionState::Connected | ConnectionState::Connecting
        ) {
            self.state = ConnectionState::Connecting;
            self.connect_count.fetch_add(1, Ordering::SeqCst);
            self.state = ConnectionState::Connected;
        }
    }

    fn disconnect(&mut self) {
        if matches!(
            self.state,
            ConnectionState::Connected | ConnectionState::Idle
        ) {
            self.state = ConnectionState::Disconnected;
            self.disconnect_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn mark_idle(&mut self) {
        if matches!(self.state, ConnectionState::Connected) {
            self.state = ConnectionState::Idle;
        }
    }

    fn state(&self) -> ConnectionState {
        self.state
    }

    fn connect_count(&self) -> usize {
        self.connect_count.load(Ordering::SeqCst)
    }

    fn disconnect_count(&self) -> usize {
        self.disconnect_count.load(Ordering::SeqCst)
    }
}

#[tokio::test]
async fn test_mcp_lazy_connection() {
    let mut connection = LazyMcpConnection::new("filesystem", 300);

    let tools = connection.discover_tools();
    assert_eq!(tools.len(), 2);
    assert_eq!(connection.state(), ConnectionState::Discovered);
    assert_eq!(connection.connect_count(), 0);

    connection.connect();
    assert_eq!(connection.state(), ConnectionState::Connected);
    assert_eq!(connection.connect_count(), 1);

    connection.mark_idle();
    assert_eq!(connection.state(), ConnectionState::Idle);

    connection.disconnect();
    assert_eq!(connection.state(), ConnectionState::Disconnected);
    assert_eq!(connection.disconnect_count(), 1);
}

#[cfg(test)]
mod mcp_integration_tests {
    use super::*;

    #[test]
    fn test_mock_mcp_server_tool_discovery() {
        let server = crate::utils::mock_server::MockMcpServer::new()
            .with_tool(
                "search",
                "Search for files matching a pattern",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": {"type": "string"}
                    }
                }),
            )
            .with_tool(
                "read",
                "Read file contents",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"}
                    }
                }),
            );

        assert_eq!(server.tools.len(), 2);
        assert_eq!(server.tools[0].name, "search");
        assert_eq!(server.tools[1].name, "read");
    }

    #[test]
    fn test_mock_mcp_server_tool_call() {
        let server = crate::utils::mock_server::MockMcpServer::new().with_tool(
            "echo",
            "Echo input",
            serde_json::json!({}),
        );

        server.set_response("echo", serde_json::json!({"result": "echoed"}));

        let result =
            server.handle_call("echo", serde_json::json!({"value": "test"}));
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["result"], "echoed");
    }

    #[tokio::test]
    async fn test_mcp_lazy_connection_no_early_connect() {
        let connection = LazyMcpConnection::new("http-server", 300);

        let tools = connection.discover_tools();
        assert!(!tools.is_empty());
        assert_eq!(connection.connect_count(), 0);
        assert!(!matches!(connection.state(), ConnectionState::Connected));
    }

    #[tokio::test]
    async fn test_mcp_connection_reuse() {
        let mut connection = LazyMcpConnection::new("test-server", 60);

        connection.connect();
        let first_connect_count = connection.connect_count();

        connection.disconnect();
        connection.connect();
        let second_connect_count = connection.connect_count();

        assert_eq!(first_connect_count, 1);
        assert_eq!(second_connect_count, 2);
    }

    #[test]
    fn test_mcp_config_lazy_connect() {
        let config = TestConfig::mcp_config();

        assert!(config.content["lazy_connect"].as_bool().unwrap());
        assert_eq!(
            config.content["idle_timeout_seconds"].as_i64().unwrap(),
            300
        );
    }

    #[test]
    fn test_mcp_server_config_structure() {
        let config = TestConfig::mcp_config();
        let servers = config.content["mcp_servers"].as_array().unwrap();

        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0]["name"], "filesystem");
        assert_eq!(servers[0]["command"], "npx");
    }
}
