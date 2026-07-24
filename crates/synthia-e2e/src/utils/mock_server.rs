use std::{collections::HashMap, sync::Arc};

use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug)]
pub struct MockMcpServer {
    pub tools: Vec<ToolDefinition>,
    responses: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    call_log: Arc<Mutex<Vec<McpCall>>>,
}

#[derive(Debug, Clone)]
pub struct McpCall {
    pub tool_name: String,
    pub input: serde_json::Value,
}

impl MockMcpServer {
    pub fn new() -> Self {
        Self {
            tools: Vec::new(),
            responses: Arc::new(Mutex::new(HashMap::new())),
            call_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_tool(
        mut self,
        name: &str,
        description: &str,
        input_schema: serde_json::Value,
    ) -> Self {
        self.tools.push(ToolDefinition {
            name: name.to_string(),
            description: description.to_string(),
            input_schema,
        });
        self
    }

    pub fn set_response(&self, tool_name: &str, response: serde_json::Value) {
        let responses_clone = self.responses.clone();
        futures::executor::block_on(async move {
            let mut r = responses_clone.lock().await;
            r.insert(tool_name.to_string(), response);
        });
    }

    pub async fn set_response_async(
        &self,
        tool_name: &str,
        response: serde_json::Value,
    ) {
        let mut responses = self.responses.lock().await;
        responses.insert(tool_name.to_string(), response);
    }

    pub fn handle_call(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let call_log_clone = self.call_log.clone();
        futures::executor::block_on(async move {
            let mut log = call_log_clone.lock().await;
            log.push(McpCall {
                tool_name: name.to_string(),
                input: input.clone(),
            });
        });

        let responses_clone = self.responses.clone();
        let response = futures::executor::block_on(async move {
            let responses = responses_clone.lock().await;
            responses.get(name).cloned()
        });

        match response {
            Some(resp) => Ok(resp),
            None => {
                if self.tools.iter().any(|t| t.name == name) {
                    Ok(serde_json::json!({
                        "success": true,
                        "message": format!("Mock response for tool: {}", name)
                    }))
                } else {
                    Err(format!("Tool '{}' not found", name))
                }
            }
        }
    }

    pub async fn handle_call_async(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let mut call_log = self.call_log.lock().await;
        call_log.push(McpCall {
            tool_name: name.to_string(),
            input: input.clone(),
        });

        let responses = self.responses.lock().await;
        match responses.get(name).cloned() {
            Some(resp) => Ok(resp),
            None => {
                if self.tools.iter().any(|t| t.name == name) {
                    Ok(serde_json::json!({
                        "success": true,
                        "message": format!("Mock response for tool: {}", name)
                    }))
                } else {
                    Err(format!("Tool '{}' not found", name))
                }
            }
        }
    }

    pub fn get_call_log(&self) -> Vec<McpCall> {
        let call_log_clone = self.call_log.clone();
        futures::executor::block_on(async move {
            call_log_clone.lock().await.clone()
        })
    }

    pub async fn get_call_log_async(&self) -> Vec<McpCall> {
        self.call_log.lock().await.clone()
    }

    pub fn call_count(&self) -> usize {
        let call_log_clone = self.call_log.clone();
        futures::executor::block_on(
            async move { call_log_clone.lock().await.len() },
        )
    }

    pub async fn call_count_async(&self) -> usize {
        self.call_log.lock().await.len()
    }

    pub fn was_called(&self, tool_name: &str) -> bool {
        self.get_call_log().iter().any(|c| c.tool_name == tool_name)
    }

    pub async fn was_called_async(&self, tool_name: &str) -> bool {
        self.call_log
            .lock()
            .await
            .iter()
            .any(|c| c.tool_name == tool_name)
    }

    pub fn reset(&self) {
        let responses_clone = self.responses.clone();
        let call_log_clone = self.call_log.clone();
        futures::executor::block_on(async move {
            responses_clone.lock().await.clear();
            call_log_clone.lock().await.clear();
        });
    }
}

impl Default for MockMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_mcp_server_creation() {
        let server = MockMcpServer::new();
        assert!(server.tools.is_empty());
    }

    #[test]
    fn test_mock_mcp_server_with_tools() {
        let server = MockMcpServer::new()
            .with_tool(
                "read_file",
                "Read a file from the filesystem",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"}
                    }
                }),
            )
            .with_tool(
                "write_file",
                "Write content to a file",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "content": {"type": "string"}
                    }
                }),
            );

        assert_eq!(server.tools.len(), 2);
        assert_eq!(server.tools[0].name, "read_file");
    }

    #[test]
    fn test_mock_mcp_server_handle_call() {
        let server = MockMcpServer::new().with_tool(
            "echo",
            "Echo back the input",
            serde_json::json!({}),
        );

        server.set_response("echo", serde_json::json!({"echoed": "hello"}));

        let result =
            server.handle_call("echo", serde_json::json!({"input": "hello"}));
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["echoed"], "hello");
    }

    #[test]
    fn test_mock_mcp_server_call_log() {
        let server = MockMcpServer::new().with_tool(
            "test",
            "Test tool",
            serde_json::json!({}),
        );

        server
            .handle_call("test", serde_json::json!({"value": 42}))
            .ok();
        server
            .handle_call("test", serde_json::json!({"value": 43}))
            .ok();

        assert_eq!(server.call_count(), 2);
        assert!(server.was_called("test"));
        assert!(!server.was_called("other"));
    }

    #[test]
    fn test_mock_mcp_server_unknown_tool() {
        let server = MockMcpServer::new();
        let result = server.handle_call("unknown", serde_json::json!({}));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_mock_mcp_server_async() {
        let server = MockMcpServer::new().with_tool(
            "async_test",
            "Async test tool",
            serde_json::json!({}),
        );

        server
            .set_response_async(
                "async_test",
                serde_json::json!({"status": "ok"}),
            )
            .await;

        let result = server
            .handle_call_async("async_test", serde_json::json!({}))
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["status"], "ok");
    }
}
