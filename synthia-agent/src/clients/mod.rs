use async_trait::async_trait;
use futures::Stream;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChunkType {
    Content,
    ToolCall,
    ToolArgs,
    Done,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamChunk {
    pub content: String,
    pub chunk_type: ChunkType,
    pub delta: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub supports_streaming: bool,
}

#[derive(Debug, Error)]
pub enum LLMError {
    #[error("Request failed: {0}")]
    RequestFailed(String),
    #[error("API error: {0}")]
    ApiError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

#[async_trait]
pub trait LLMClient: Send + Sync {
    async fn stream_complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDefinition>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LLMError>> + Send>>, LLMError>;

    fn model_info(&self) -> ModelInfo;
}

pub struct OpenAIClient {
    api_key: String,
    model: String,
    client: reqwest::Client,
    timeout: Duration,
    base_url: String,
}

impl OpenAIClient {
    pub fn new(api_key: String, model: String, base_url: Option<String>) -> Self {
        Self {
            api_key,
            model,
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(600),
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_string()),
        }
    }

    fn build_request(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDefinition>,
    ) -> Result<serde_json::Value, LLMError> {
        let messages_json: Vec<serde_json::Value> = messages
            .into_iter()
            .map(|msg| {
                let mut map = serde_json::Map::new();
                map.insert(
                    "role".to_string(),
                    serde_json::Value::String(match msg.role {
                        MessageRole::System => "system".to_string(),
                        MessageRole::User => "user".to_string(),
                        MessageRole::Assistant => "assistant".to_string(),
                        MessageRole::Tool => "tool".to_string(),
                    }),
                );
                map.insert("content".to_string(), serde_json::Value::String(msg.content));

                if let Some(tool_calls) = msg.tool_calls {
                    let tool_calls_json: Vec<serde_json::Value> = tool_calls
                        .into_iter()
                        .map(|tc| {
                            serde_json::json!({
                                "id": tc.id,
                                "type": "function",
                                "function": {
                                    "name": tc.function.name,
                                    "arguments": tc.function.arguments
                                }
                            })
                        })
                        .collect();
                    map.insert(
                        "tool_calls".to_string(),
                        serde_json::Value::Array(tool_calls_json),
                    );
                }

                serde_json::Value::Object(map)
            })
            .collect();

        let mut request = serde_json::Map::new();
        request.insert("model".to_string(), serde_json::Value::String(self.model.clone()));
        request.insert("messages".to_string(), serde_json::Value::Array(messages_json));
        request.insert("stream".to_string(), serde_json::Value::Bool(true));

        if !tools.is_empty() {
            let tools_json: Vec<serde_json::Value> = tools
                .into_iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters
                        }
                    })
                })
                .collect();
            request.insert("tools".to_string(), serde_json::Value::Array(tools_json));
        }

        Ok(serde_json::Value::Object(request))
    }
}

fn parse_stream(
    response: reqwest::Response,
) -> impl Stream<Item = Result<StreamChunk, LLMError>> + Send {
    let mut buffer = String::new();
    let mut current_tool_call: Option<(String, String)> = None;
    let mut in_tool_call = false;

    async_stream::stream! {
        let mut stream = response.bytes_stream();
        let mut full_response = String::new();

        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    if let Ok(s) = String::from_utf8(bytes.to_vec()) {
                        full_response.push_str(&s);
                        
                        // Try to parse as SSE first
                        let mut lines = s.lines().peekable();
                        while let Some(line) = lines.next() {
                            if line.starts_with("data: ") {
                                let data = &line[6..];
                                if data == "[DONE]" {
                                    yield Ok(StreamChunk {
                                        content: String::new(),
                                        chunk_type: ChunkType::Done,
                                        delta: false,
                                    });
                                    return;
                                }

                                match serde_json::from_str::<serde_json::Value>(data) {
                                    Ok(json) => {
                                        if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                                            for choice in choices {
                                                if let Some(delta) = choice.get("delta").and_then(|d| d.as_object()) {
                                                    if let Some(content) = delta.get("content") {
                                                        if let Some(s) = content.as_str() {
                                                            if !s.is_empty() {
                                                                yield Ok(StreamChunk {
                                                                    content: s.to_string(),
                                                                    chunk_type: ChunkType::Content,
                                                                    delta: true,
                                                                });
                                                            }
                                                        }
                                                    }

                                                    if let Some(tool_calls) = delta.get("tool_calls") {
                                                        if let Some(tc_array) = tool_calls.as_array() {
                                                            for tc in tc_array {
                                                                if let Some(tc_obj) = tc.as_object() {
                                                                    if let Some(function) = tc_obj.get("function") {
                                                                        if let Some(fn_obj) = function.as_object() {
                                                                            if let Some(name) = fn_obj.get("name").and_then(|n| n.as_str()) {
                                                                                if !name.is_empty() {
                                                                                    in_tool_call = true;
                                                                                    current_tool_call = Some((name.to_string(), String::new()));
                                                                                }
                                                                            }
                                                                            if let Some(args) = fn_obj.get("arguments").and_then(|a| a.as_str()) {
                                                                                if let Some(ref mut call) = current_tool_call {
                                                                                    call.1.push_str(args);
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Err(_) => {
                                        // Not SSE format, try to parse as full response when stream ends
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    yield Err(LLMError::RequestFailed(e.to_string()));
                    return;
                }
            }
        }

        // Try to parse the full response as a non-streaming response
        match serde_json::from_str::<serde_json::Value>(&full_response) {
            Ok(json) => {
                if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                    for choice in choices {
                        if let Some(message) = choice.get("message").and_then(|m| m.as_object()) {
                            if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                                if !content.is_empty() {
                                    yield Ok(StreamChunk {
                                        content: content.to_string(),
                                        chunk_type: ChunkType::Content,
                                        delta: false,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            Err(_) => {
                yield Err(LLMError::ParseError(format!("Failed to parse response: {}", full_response)));
            }
        }

        // End of stream
        yield Ok(StreamChunk {
            content: String::new(),
            chunk_type: ChunkType::Done,
            delta: false,
        });
    }
}

#[async_trait]
impl LLMClient for OpenAIClient {
    async fn stream_complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDefinition>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LLMError>> + Send>>, LLMError> {
        let request = self.build_request(messages, tools)?;

        let response = self
            .client
            .post(&self.base_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&request)
            .send()
            .await
            .map_err(|e| LLMError::RequestFailed(e.to_string()))?;

        Ok(Box::pin(parse_stream(response)))
    }

    fn model_info(&self) -> ModelInfo {
        ModelInfo {
            name: self.model.clone(),
            max_tokens: Some(16384),
            supports_streaming: true,
        }
    }
}

pub fn create_llm_client(provider: &str, api_key: String, model: String, base_url: Option<String>) -> Result<Box<dyn LLMClient>, LLMError> {
    match provider {
        "openai" | "OpenAI" => Ok(Box::new(OpenAIClient::new(api_key, model, base_url))),
        _ => Err(LLMError::ConfigError(format!("Unknown provider: {}", provider))),
    }
}
