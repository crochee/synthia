//! OpenAI format handling for requests and responses

use std::{collections::HashMap, ops::Deref, sync::Arc};

use async_stream::try_stream;
use futures::{Stream, StreamExt};
use rmcp::model::{
    CreateMessageRequestParams,
    CreateMessageResult,
    Meta,
    RawContent,
    RawTextContent,
    Role,
    SamplingContent,
    SamplingMessage,
    SamplingMessageContent,
    ToolUseContent,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    Result,
    formats::{extract_tools, get_model_name},
};

#[derive(Serialize, Deserialize, Debug, Default)]
struct DeltaToolCallFunction {
    name: Option<String>,
    #[serde(default)]
    arguments: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct DeltaToolCall {
    id: Option<String>,
    function: DeltaToolCallFunction,
    index: Option<i32>,
    #[serde(rename = "type")]
    r#type: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Delta {
    content: Option<String>,
    role: Option<String>,
    tool_calls: Option<Vec<DeltaToolCall>>,
    reasoning: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct StreamingChoice {
    delta: Delta,
    index: Option<i32>,
    finish_reason: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct StreamingChunk {
    choices: Vec<StreamingChoice>,
    created: Option<i64>,
    id: Option<String>,
    usage: Option<Value>,
    model: Option<String>,
}

fn create_reasoning_meta() -> Option<Meta> {
    let mut map = serde_json::Map::new();
    map.insert("type".to_string(), json!("reasoning"));
    Some(Meta(map))
}

fn create_text_message(text: String, meta: Option<Meta>) -> SamplingMessage {
    SamplingMessage {
        role: Role::Assistant,
        content: SamplingContent::Single(SamplingMessageContent::Text(
            RawTextContent { text, meta },
        )),
        meta: None,
    }
}

fn create_tool_use_message(
    tool_calls: Vec<(String, String, String)>,
) -> SamplingMessage {
    if tool_calls.is_empty() {
        return SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: String::new(),
                    meta: None,
                },
            )),
            meta: None,
        };
    }

    if tool_calls.len() == 1 {
        let mut iter = tool_calls.into_iter();
        let (id, name, args) = iter.next().unwrap_or_default();
        let args_obj: serde_json::Map<String, Value> =
            serde_json::from_str(&args).unwrap_or_default();
        SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Single(SamplingMessageContent::ToolUse(
                ToolUseContent::new(&id, &name, args_obj),
            )),
            meta: None,
        }
    } else {
        let contents: Vec<SamplingMessageContent> = tool_calls
            .into_iter()
            .map(|(id, name, args)| {
                let args_obj: serde_json::Map<String, Value> =
                    serde_json::from_str(&args).unwrap_or_default();
                SamplingMessageContent::ToolUse(ToolUseContent::new(
                    &id, &name, args_obj,
                ))
            })
            .collect();
        SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Multiple(contents),
            meta: None,
        }
    }
}

fn create_result(
    model: String,
    stop_reason: Option<String>,
    text: String,
    meta: Option<Meta>,
) -> CreateMessageResult {
    CreateMessageResult {
        model,
        stop_reason,
        message: create_text_message(text, meta),
    }
}

fn create_tool_use_result(
    model: String,
    stop_reason: Option<String>,
    tool_calls: Vec<(String, String, String)>,
) -> CreateMessageResult {
    CreateMessageResult {
        model,
        stop_reason,
        message: create_tool_use_message(tool_calls),
    }
}

fn message_to_openai(msg: &SamplingMessage) -> Vec<Value> {
    let mut values = Vec::new();
    let mut tool_calls = Vec::new();
    let mut tool_results = Vec::new();

    for c in msg.content.iter() {
        match c {
            SamplingMessageContent::ToolResult(tr) => {
                if msg.role == Role::Assistant {
                    continue;
                }
                let content_str = tr
                    .content
                    .iter()
                    .filter_map(|c| match c.deref() as &RawContent {
                        RawContent::Text(t) => Some(t.text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                tool_results.push(json!({
                    "role": "tool",
                    "content": content_str,
                    "tool_call_id": tr.tool_use_id
                }));
            }
            SamplingMessageContent::ToolUse(tu) => {
                tool_calls.push(json!({
                    "type": "function",
                    "id": tu.id,
                    "function": {
                        "name": tu.name,
                        "arguments": serde_json::to_string(&tu.input).unwrap_or_default()
                    }
                }));
            }
            SamplingMessageContent::Text(t) => {
                values.push(json!({
                    "role": if matches!(msg.role, Role::User) {
                        "user"
                    } else {
                        "assistant"
                    },
                    "content": t.text
                }));
            }
            SamplingMessageContent::Image(img) => {
                values.push(json!({
                    "role": if matches!(msg.role, Role::User) {
                        "user"
                    } else {
                        "assistant"
                    },
                    "content": { "type": "image_url", "image_url": { "url": format!("data:{};base64,{}", img.mime_type, img.data) } }
                }));
            }
            SamplingMessageContent::Audio(audio) => {
                values.push(json!({
                    "role": if matches!(msg.role, Role::User) {
                        "user"
                    } else {
                        "assistant"
                    },
                    "content": { "type": "input_audio", "input_audio": { "data": audio.data, "format": audio.mime_type.split('/').next_back().unwrap_or("wav") } }
                }));
            }
        }
    }

    // Output tool_calls first, then tool_results
    // API requires: assistant message with tool_calls must precede tool messages
    if !tool_calls.is_empty() {
        values.push(json!({
            "role": "assistant",
            "tool_calls": tool_calls
        }));
    }
    values.extend(tool_results);
    values
}

/// Validate and fix tool call/result pairs in OpenAI format messages
/// This ensures that tool messages always have corresponding tool_calls
fn validate_tool_pairs(messages: &mut Vec<Value>) {
    use std::collections::HashSet;

    let mut pending_tool_calls: HashSet<String> = HashSet::new();
    let mut indices_to_remove: Vec<usize> = Vec::new();

    for (i, msg) in messages.iter().enumerate() {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");

        // Track tool_calls from assistant messages
        if role == "assistant"
            && let Some(tool_calls) =
                msg.get("tool_calls").and_then(|tc| tc.as_array())
        {
            for tc in tool_calls {
                if let Some(id) = tc.get("id").and_then(|id| id.as_str()) {
                    pending_tool_calls.insert(id.to_string());
                }
            }
        }

        // Check tool messages
        if role == "tool"
            && let Some(tool_call_id) =
                msg.get("tool_call_id").and_then(|id| id.as_str())
        {
            if pending_tool_calls.contains(tool_call_id) {
                pending_tool_calls.remove(tool_call_id);
            } else {
                // Orphaned tool message - mark for removal
                tracing::warn!(
                    "Removing orphaned tool message with tool_call_id: {}",
                    tool_call_id
                );
                indices_to_remove.push(i);
            }
        }
    }

    // Remove orphaned tool messages in reverse order to maintain indices
    for &idx in indices_to_remove.iter().rev() {
        messages.remove(idx);
    }

    // Log pending tool_calls without results (this is okay, LLM may continue)
    if !pending_tool_calls.is_empty() {
        tracing::debug!(
            "Tool calls without results (LLM may continue): {:?}",
            pending_tool_calls
        );
    }
}

pub(crate) fn create_request(
    params: &CreateMessageRequestParams,
    stream: bool,
) -> Result<Value> {
    let mut openai_messages = Vec::new();

    if let Some(system_prompt) = &params.system_prompt
        && !system_prompt.is_empty()
    {
        openai_messages.push(json!({
            "role": "system",
            "content": system_prompt
        }));
    }

    for msg in &params.messages {
        let openai_msg = message_to_openai(msg);
        openai_messages.extend(openai_msg);
    }

    // Validate tool call/result pairs to prevent API errors
    validate_tool_pairs(&mut openai_messages);

    let tools = extract_tools(params);

    let tools_spec: Vec<Value> = tools
        .iter()
        .map(|tool| {
            let input_schema = Arc::clone(&tool.input_schema);
            let schema_value = Value::Object(input_schema.as_ref().clone());
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description.as_ref().map(AsRef::as_ref).unwrap_or(""),
                    "parameters": schema_value
                }
            })
        })
        .collect();

    let mut tools_spec = tools_spec;
    validate_tool_schemas(&mut tools_spec);

    let model = get_model_name(params);

    let mut request = json!({
        "model": model,
        "messages": openai_messages,
        "stream": stream,
        "max_tokens": params.max_tokens
    });

    if let Some(temp) = params.temperature {
        request["temperature"] = json!(temp);
    }

    if let Some(stop_sequences) = &params.stop_sequences {
        request["stop"] = json!(stop_sequences);
    }

    if !tools_spec.is_empty() {
        request["tools"] = json!(tools_spec);
    }

    if stream {
        request["stream_options"] = json!({
            "include_usage": true
        });
    }
    Ok(request)
}

pub(crate) fn validate_tool_schemas(tools: &mut [Value]) {
    for tool in tools.iter_mut() {
        if let Some(function) = tool.get_mut("function")
            && let Some(parameters) = function.get_mut("parameters")
            && parameters.is_object()
        {
            ensure_valid_json_schema(parameters);
        }
    }
}

fn ensure_valid_json_schema(schema: &mut Value) {
    if let Some(params_obj) = schema.as_object_mut() {
        let is_object_type = params_obj
            .get("type")
            .and_then(|t| t.as_str())
            .is_none_or(|t| t == "object");

        if is_object_type {
            params_obj.entry("properties").or_insert_with(|| json!({}));
            params_obj.entry("required").or_insert_with(|| json!([]));
            params_obj.entry("type").or_insert_with(|| json!("object"));

            if let Some(properties) = params_obj.get_mut("properties")
                && let Some(properties_obj) = properties.as_object_mut()
            {
                for (_key, prop) in properties_obj.iter_mut() {
                    if prop.is_object()
                        && prop.get("type").and_then(|t| t.as_str())
                            == Some("object")
                    {
                        ensure_valid_json_schema(prop);
                    }
                }
            }
        }
    }
}

fn strip_data_prefix(line: &str) -> Option<&str> {
    line.strip_prefix("data: ").map(str::trim)
}

async fn process_tool_calls<S>(
    stream: &mut S,
    chunk: &StreamingChunk,
) -> Result<(Vec<(String, String, String)>, Option<String>)>
where
    S: Stream<Item = Result<String>> + Unpin + Send + 'static,
{
    let mut tool_call_data: HashMap<i32, (String, String, String)> =
        HashMap::new();
    let mut stop_reason: Option<String> = None;

    if let Some(tool_calls) = &chunk.choices[0].delta.tool_calls {
        for tool_call in tool_calls {
            if let (Some(index), Some(id), Some(name)) =
                (tool_call.index, &tool_call.id, &tool_call.function.name)
            {
                tool_call_data.insert(
                    index,
                    (
                        id.clone(),
                        name.clone(),
                        tool_call.function.arguments.clone(),
                    ),
                );
            }
        }
    }

    let is_complete =
        chunk.choices[0].finish_reason == Some("tool_calls".to_string());
    if is_complete {
        stop_reason = Some("tool_calls".to_string());
    }

    if !is_complete {
        let mut done = false;
        while !done {
            if let Some(response_chunk) = stream.next().await {
                if response_chunk.as_ref().is_ok_and(|s| s == "data: [DONE]") {
                    break;
                }
                let response_str = response_chunk?;
                if let Some(line) = strip_data_prefix(&response_str) {
                    if line.is_empty() {
                        continue;
                    }

                    let tool_chunk: StreamingChunk =
                        serde_json::from_str(line)?;

                    if !tool_chunk.choices.is_empty() {
                        if let Some(delta_tool_calls) =
                            &tool_chunk.choices[0].delta.tool_calls
                        {
                            for delta_call in delta_tool_calls {
                                if let Some(index) = delta_call.index {
                                    if let Some((_, _, args)) =
                                        tool_call_data.get_mut(&index)
                                    {
                                        args.push_str(
                                            &delta_call.function.arguments,
                                        );
                                    } else if let (Some(id), Some(name)) = (
                                        &delta_call.id,
                                        &delta_call.function.name,
                                    ) {
                                        tool_call_data.insert(
                                            index,
                                            (
                                                id.clone(),
                                                name.clone(),
                                                delta_call
                                                    .function
                                                    .arguments
                                                    .clone(),
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                        if let Some(finish) =
                            &tool_chunk.choices[0].finish_reason
                        {
                            stop_reason = Some(finish.clone());
                            done = true;
                        }
                    } else {
                        done = true;
                    }
                }
            } else {
                break;
            }
        }
    }

    let mut sorted_indices: Vec<_> = tool_call_data.keys().cloned().collect();
    sorted_indices.sort();

    let results: Vec<_> = sorted_indices
        .iter()
        .filter_map(|index| tool_call_data.remove(index))
        .collect();

    Ok((results, stop_reason))
}

pub(crate) fn response_to_streaming_message<S>(
    mut stream: S,
) -> impl Stream<Item = Result<CreateMessageResult>> + 'static
where
    S: Stream<Item = Result<String>> + Unpin + Send + 'static,
{
    try_stream! {
        let mut model_name = String::new();
        let mut accumulated_reasoning = String::new();
        let mut accumulated_content = String::new();
        let mut has_tool_calls = false;

        while let Some(response) = stream.next().await {
            if response.as_ref().is_ok_and(|s| s == "data: [DONE]") {
                break;
            }

            let response_str = response?;
            let line = match strip_data_prefix(&response_str) {
                Some(l) if !l.is_empty() => l,
                _ => continue,
            };

            let chunk: StreamingChunk = serde_json::from_str(line)?;

            if let Some(model) = &chunk.model {
                model_name = model.clone();
            }

            if chunk.choices.is_empty() {
                continue;
            }

            let stop_reason = chunk.choices[0].finish_reason.clone();

            if chunk.choices[0].delta.tool_calls.as_ref().is_some_and(|tc| !tc.is_empty()) {
                has_tool_calls = true;
                let (tool_calls, tool_stop_reason) = process_tool_calls(&mut stream, &chunk).await?;
                if !tool_calls.is_empty() {
                    if !accumulated_reasoning.is_empty() {
                        yield create_result(
                            model_name.clone(),
                            None,
                            accumulated_reasoning.clone(),
                            create_reasoning_meta(),
                        );
                        accumulated_reasoning.clear();
                    }
                    yield create_tool_use_result(
                        model_name.clone(),
                        tool_stop_reason.or(stop_reason),
                        tool_calls
                    );
                }
            } else if let Some(reasoning) = chunk.choices[0].delta.reasoning.as_ref()
                && !reasoning.is_empty()
            {
                accumulated_reasoning.push_str(reasoning);
            } else if let Some(text) = chunk.choices[0].delta.content.as_ref()
                && !text.is_empty()
            {
                accumulated_content.push_str(text);
            } else if stop_reason.is_some() && !has_tool_calls {
                if !accumulated_reasoning.is_empty() {
                    yield create_result(
                        model_name.clone(),
                        stop_reason.clone(),
                        accumulated_reasoning.clone(),
                        create_reasoning_meta(),
                    );
                    accumulated_reasoning.clear();
                }
                if !accumulated_content.is_empty() {
                    yield create_result(
                        model_name.clone(),
                        stop_reason,
                        accumulated_content.clone(),
                        None,
                    );
                    accumulated_content.clear();
                }
            }
        }

        if !has_tool_calls {
            if !accumulated_reasoning.is_empty() {
                yield create_result(
                    model_name.clone(),
                    None,
                    accumulated_reasoning,
                    create_reasoning_meta(),
                );
            }
            if !accumulated_content.is_empty() {
                yield create_result(
                    model_name,
                    None,
                    accumulated_content,
                    None,
                );
            }
        }
    }
}

pub async fn collect_stream(
    mut stream: impl futures::Stream<Item = Result<CreateMessageResult>> + Unpin,
) -> Result<CreateMessageResult> {
    let mut accumulated_text = String::new();
    let mut accumulated_reasoning = String::new();
    let mut model_name = String::new();
    let mut stop_reason: Option<String> = None;
    let mut other_messages = Vec::new();

    while let Some(result) = stream.next().await {
        let msg_result = result?;

        model_name = msg_result.model;

        if msg_result.stop_reason.is_some() {
            stop_reason = msg_result.stop_reason;
        }

        for content in msg_result.message.content.iter() {
            match content {
                SamplingMessageContent::Text(t) => {
                    let is_reasoning = t
                        .meta
                        .as_ref()
                        .and_then(|m| m.0.get("type"))
                        .and_then(|v| v.as_str())
                        == Some("reasoning");
                    if is_reasoning {
                        accumulated_reasoning.push_str(&t.text);
                    } else {
                        accumulated_text.push_str(&t.text);
                    }
                }
                _ => {
                    other_messages.push(content.clone());
                }
            }
        }
    }

    let mut messages = vec![
        SamplingMessageContent::text(accumulated_text),
        SamplingMessageContent::Text(RawTextContent {
            text: accumulated_reasoning,
            meta: create_reasoning_meta(),
        }),
    ];
    messages.extend(other_messages);

    Ok(CreateMessageResult {
        model: model_name,
        stop_reason,
        message: SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Multiple(messages),
            meta: None,
        },
    })
}
