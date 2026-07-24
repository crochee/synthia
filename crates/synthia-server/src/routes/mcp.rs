use std::sync::Arc;

use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use synthia_core::{Registry, RegistryItem};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub id: Option<Value>,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<Value>, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }
}

async fn tool_definitions(state: &Arc<AppState>) -> Vec<Value> {
    let tool_reg = state.tool_registry.read().await;
    let defs: Vec<_> = tool_reg
        .list(None)
        .await
        .map(|entries| {
            entries
                .iter()
                .map(|e| synthia_provider::ToolDefinition {
                    name: e.name().to_string(),
                    description: e.description().to_string(),
                    input_schema: e.tool_instance().parameters(),
                    cache_control: None,
                })
                .collect()
        })
        .unwrap_or_default();
    drop(tool_reg);

    defs.into_iter()
        .map(|d| {
            serde_json::json!({
                "name": d.name,
                "description": d.description,
                "inputSchema": d.input_schema
            })
        })
        .collect()
}

pub async fn handle_jsonrpc(
    State(state): State<Arc<AppState>>,
    Json(request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    match request.method.as_str() {
        "initialize" => Json(JsonRpcResponse::success(
            request.id,
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {"listChanged": true},
                    "resources": {"listChanged": true}
                },
                "serverInfo": {
                    "name": "synthia-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )),
        "tools/list" => {
            let tools = tool_definitions(&state).await;
            Json(JsonRpcResponse::success(
                request.id,
                serde_json::json!({ "tools": tools }),
            ))
        }
        "tools/call" => {
            let id = request.id.clone();
            let params = request.params.unwrap_or(Value::Null);

            let name = match params.get("name").and_then(|v| v.as_str()) {
                Some(n) => n.to_string(),
                None => {
                    return Json(JsonRpcResponse::error(
                        id,
                        -32602,
                        "Missing tool name in params".to_string(),
                    ));
                }
            };

            let _arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or(Value::Object(serde_json::Map::new()));

            Json(JsonRpcResponse::error(
                id,
                -32000,
                format!("Tool '{}' requires agent context", name),
            ))
        }
        _ => Json(JsonRpcResponse::error(
            request.id,
            -32601,
            format!("Method not found: {}", request.method),
        )),
    }
}
