//! Anthropic format handling for requests and responses

use std::sync::Arc;

use async_stream::try_stream;
use futures::{Stream, StreamExt};
use rmcp::model::{
    CreateMessageRequestParams,
    CreateMessageResult,
    RawTextContent,
    Role,
    SamplingContent,
    SamplingMessage,
    SamplingMessageContent,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    Result,
    formats::{extract_tools, get_model_name},
};

/// Convert SamplingMessageContent to Anthropic format
fn sampling_content_to_anthropic(
    content: &SamplingMessageContent,
) -> Vec<Value> {
    match content {
        SamplingMessageContent::Text(text_content) => {
            vec![json!({
                "type": "text",
                "text": text_content.text
            })]
        }
        SamplingMessageContent::Image(image_content) => {
            vec![json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": image_content.mime_type,
                    "data": image_content.data
                }
            })]
        }
        SamplingMessageContent::Audio(audio_content) => {
            vec![json!({
                "type": "audio",
                "source": {
                    "type": "base64",
                    "media_type": audio_content.mime_type,
                    "data": audio_content.data
                }
            })]
        }
        SamplingMessageContent::ToolUse(tool_use) => {
            vec![json!({
                "type": "tool_use",
                "id": tool_use.id,
                "name": tool_use.name,
                "input": tool_use.input
            })]
        }
        SamplingMessageContent::ToolResult(tool_result) => {
            vec![json!({
                "type": "tool_result",
                "tool_use_id": tool_result.tool_use_id,
                "content": tool_result.content
            })]
        }
    }
}

/// Convert SamplingContent to Anthropic format
fn content_to_anthropic(
    content: &SamplingContent<SamplingMessageContent>,
) -> Vec<Value> {
    content
        .iter()
        .flat_map(sampling_content_to_anthropic)
        .collect()
}

/// Create an Anthropic request from CreateMessageRequestParams
pub(crate) fn create_request(
    params: &CreateMessageRequestParams,
    stream: bool,
) -> Result<Value> {
    let mut anthropic_messages = Vec::new();

    // Add system message if provided
    if let Some(system_prompt) = &params.system_prompt
        && !system_prompt.is_empty()
    {
        anthropic_messages.push(json!({
            "role": "system",
            "content": [{
                "type": "text",
                "text": system_prompt
            }]
        }));
    }

    // Convert SamplingMessage to Anthropic format
    for msg in &params.messages {
        let role = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };

        let content_parts = content_to_anthropic(&msg.content);

        if !content_parts.is_empty() {
            anthropic_messages.push(json!({
                "role": role,
                "content": content_parts
            }));
        }
    }

    // Extract tools from metadata
    let tools = extract_tools(params);

    // Build tools array
    let mut anthropic_tools = Vec::new();
    for tool in &tools {
        let input_schema = Arc::clone(&tool.input_schema);
        let schema_value = Value::Object(input_schema.as_ref().clone());
        anthropic_tools.push(json!({
            "name": tool.name,
            "description": tool.description.as_ref().map(AsRef::as_ref).unwrap_or(""),
            "input_schema": schema_value
        }));
    }

    // Get model name
    let model = get_model_name(params);

    // Build the request
    let mut request = json!({
        "model": model,
        "messages": anthropic_messages,
        "stream": stream,
        "max_tokens": params.max_tokens
    });

    // Add temperature if provided
    if let Some(temp) = params.temperature {
        request["temperature"] = json!(temp);
    }

    // Add stop sequences if provided
    if let Some(stop_sequences) = &params.stop_sequences {
        request["stop_sequences"] = json!(stop_sequences);
    }

    // Add tools if provided
    if !anthropic_tools.is_empty() {
        request["tools"] = json!(anthropic_tools);
    }

    Ok(request)
}

/// Process streaming response from Anthropic's API
pub(crate) fn anthropic_to_message_stream<S>(
    mut stream: S,
) -> impl Stream<Item = Result<CreateMessageResult>> + 'static
where
    S: Stream<Item = Result<String>> + Unpin + Send + 'static,
{
    #[derive(Serialize, Deserialize, Debug)]
    struct StreamingEvent {
        #[serde(rename = "type")]
        event_type: String,
        #[serde(flatten)]
        data: Value,
    }

    try_stream! {
        let mut accumulated_tool_calls: std::collections::HashMap<String, (String, String)> = std::collections::HashMap::new();
        let mut current_tool_id: Option<String> = None;
        let mut model_name = String::new();

        while let Some(line_result) = stream.next().await {
            let line = line_result?;

            if line.trim().is_empty() || !line.starts_with("data: ") {
                continue;
            }

            let data_part = line.strip_prefix("data: ").unwrap_or(&line);

            if data_part.trim() == "[DONE]" {
                break;
            }

            let event: StreamingEvent = match serde_json::from_str(data_part) {
                Ok(event) => event,
                Err(_) => {
                    continue;
                }
            };

            match event.event_type.as_str() {
                "message_start" => {
                    if let Some(message_data) = event.data.get("message")
                        && let Some(model) = message_data.get("model").and_then(|v| v.as_str())
                    {
                        model_name = model.to_string();
                    }
                    continue;
                }
                "content_block_start" => {
                    if let Some(content_block) = event.data.get("content_block")
                        && content_block.get("type") == Some(&json!("tool_use"))
                        && let Some(id) = content_block.get("id").and_then(|v| v.as_str())
                        && let Some(name) = content_block.get("name").and_then(|v| v.as_str())
                    {
                        current_tool_id = Some(id.to_string());
                        accumulated_tool_calls.insert(id.to_string(), (name.to_string(), String::new()));
                    }
                    continue;
                }
                "content_block_delta" => {
                    if let Some(delta) = event.data.get("delta") {
                        if delta.get("type") == Some(&json!("text_delta")) {
                            if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                                let message = SamplingMessage {
                                    role: Role::Assistant,
                                    content: SamplingContent::Single(SamplingMessageContent::Text(RawTextContent {
                                        text: text.to_string(),
                                        meta: None,
                                    })),
                                    meta: None,
                                };
                                yield CreateMessageResult {
                                    model: model_name.clone(),
                                    stop_reason: None,
                                    message,
                                };
                            }
                        } else if delta.get("type") == Some(&json!("input_json_delta"))
                            && let Some(tool_id) = &current_tool_id
                            && let Some(partial_json) = delta.get("partial_json").and_then(|v| v.as_str())
                            && let Some((_name, args)) = accumulated_tool_calls.get_mut(tool_id)
                        {
                            args.push_str(partial_json);
                        }
                    }
                    continue;
                }
                "content_block_stop" => {
                    if let Some(tool_id) = current_tool_id.take()
                        && let Some((name, args)) = accumulated_tool_calls.remove(&tool_id)
                    {
                        let message = SamplingMessage {
                            role: Role::Assistant,
                            content: SamplingContent::Single(SamplingMessageContent::Text(RawTextContent {
                                text: format!("[Tool Call: {name} with args: {args}]"),
                                meta: None,
                            })),
                            meta: None,
                        };
                        yield CreateMessageResult {
                            model: model_name.clone(),
                            stop_reason: Some("tool_use".to_string()),
                            message,
                        };
                    }
                    continue;
                }
                "message_delta" => {
                    continue;
                }
                "message_stop" => {
                    break;
                }
                _ => {
                    continue;
                }
            }
        }
    }
}
