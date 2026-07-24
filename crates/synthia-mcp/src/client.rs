use std::{borrow::Cow, sync::Arc};

use rmcp::{
    model::{CallToolRequestParam, CallToolResult, ListToolsResult},
    service::{Peer, RoleClient},
};

use crate::{oauth::CredentialStore, types::McpError};

/// List tools from the MCP server.
pub async fn list_tools(
    peer: &mut Peer<RoleClient>,
) -> Result<ListToolsResult, McpError> {
    peer.list_tools(None).await.map_err(|e| {
        McpError::ServerNotFound(format!("list_tools error: {}", e))
    })
}

/// Call a tool on the MCP server.
pub async fn call_tool(
    peer: &mut Peer<RoleClient>,
    name: &str,
    arguments: serde_json::Value,
) -> Result<CallToolResult, McpError> {
    let params = CallToolRequestParam {
        name: Cow::Owned(name.to_string()),
        arguments: arguments.as_object().cloned(),
    };
    peer.call_tool(params).await.map_err(|e| {
        McpError::ServerNotFound(format!("call_tool error: {}", e))
    })
}

/// Send a request with automatic OAuth token refresh on auth error.
/// Detects 401 responses, refreshes the token, and retries once.
pub async fn send_request_with_auth_retry(
    peer: &mut Peer<RoleClient>,
    name: &str,
    arguments: serde_json::Value,
    credential_store: &Arc<CredentialStore>,
    server_name: &str,
) -> Result<CallToolResult, McpError> {
    let response = call_tool(peer, name, arguments.clone()).await;

    // Check if this is an authentication error
    if let Err(e) = &response {
        let is_auth_error = matches!(e, McpError::ServerNotFound(msg) if
            msg.contains("401") || msg.contains("unauthorized") || msg.contains("auth"));
        if is_auth_error {
            tracing::warn!(
                server = %server_name,
                "Received auth error, attempting token refresh"
            );

            // Try to refresh the token
            if let Err(e) = credential_store.refresh_token(server_name).await {
                tracing::error!(
                    server = %server_name,
                    error = %e,
                    "Token refresh failed"
                );
                return response; // Return the original error
            }

            // Retry the request with refreshed token
            tracing::info!(
                server = %server_name,
                "Retrying request after token refresh"
            );
            return call_tool(peer, name, arguments).await;
        }
    }

    response
}

/// Initialize is handled internally by rmcp's service during connection.
/// This is a no-op placeholder for API compatibility.
pub async fn initialize_server(
    _peer: &mut Peer<RoleClient>,
) -> Result<(), McpError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_client_module_exists() {}
}
