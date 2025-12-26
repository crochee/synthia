use crate::clients::{ChunkType, LLMClient, Message, MessageRole, StreamChunk, ToolDefinition};
use crate::memory::{ContextCompressor, ConversationHistory, ToolResult};
use crate::prompts::build_code_agent_prompt;
use crate::tools::{ToolManager, ToolTrait};
use futures::Stream;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub thought: String,
    pub action: String,
    pub action_input: serde_json::Value,
    pub observation: String,
    pub raw: String,
}

impl Step {
    pub fn new(
        thought: String,
        action: String,
        action_input: serde_json::Value,
        observation: String,
        raw: String,
    ) -> Self {
        Self {
            thought,
            action,
            action_input,
            observation,
            raw,
        }
    }
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("No tools provided")]
    NoTools,
    #[error("LLM error: {0}")]
    LLMError(String),
    #[error("Tool error: {0}")]
    ToolError(String),
    #[error("Max steps exceeded")]
    MaxStepsExceeded,
    #[error("Channel closed")]
    ChannelClosed,
    #[error("Invalid response format: {0}")]
    InvalidResponseFormat(String),
}

pub struct ReactAgent {
    client: Arc<dyn LLMClient>,
    tools: ToolManager,
    max_steps: usize,
    step_callback: Option<Arc<dyn Fn(usize, Step) + Send + Sync>>,
    enable_compression: bool,
    compressor: ContextCompressor,
    history: ConversationHistory,
    step_count: Arc<AtomicUsize>,
    working_dir: PathBuf,
}

impl ReactAgent {
    pub fn new(
        client: Box<dyn LLMClient>,
        tools: ToolManager,
        working_dir: PathBuf,
        max_steps: Option<usize>,
        enable_compression: Option<bool>,
        step_callback: Option<Arc<dyn Fn(usize, Step) + Send + Sync>>,
    ) -> Self {
        Self {
            client: Arc::from(client),
            tools,
            max_steps: max_steps.unwrap_or(200),
            step_callback,
            enable_compression: enable_compression.unwrap_or(true),
            compressor: ContextCompressor::with_tokens(12000),
            history: ConversationHistory::new(50),
            step_count: Arc::new(AtomicUsize::new(0)),
            working_dir,
        }
    }

    pub async fn run(
        &mut self,
        task: &str,
    ) -> Result<Vec<Step>, AgentError> {
        let task = task.to_string();
        let working_dir = self.working_dir.clone();
        let tool_manager = std::mem::replace(&mut self.tools, ToolManager::new());
        let tools_definitions = tool_manager.get_definitions();
        let client = self.client.clone();

        let system_prompt = build_code_agent_prompt(&tools_definitions, None);
        let system_message = Message {
            role: MessageRole::System,
            content: system_prompt,
            tool_calls: None,
        };

        self.history.add_message(system_message.clone());

        let initial_message = Message {
            role: MessageRole::User,
            content: task.clone(),
            tool_calls: None,
        };

        self.history.add_message(initial_message.clone());

        let step_count = self.step_count.clone();

        let mut current_step = 0;
        let mut current_thought = String::new();
        let mut current_action = String::new();
        let mut current_action_input = serde_json::json!({});
        let mut raw_response = String::new();
        let mut in_thought = true;
        let mut in_action = false;
        let mut tool_call_buffer = String::new();

        let mut messages = vec![system_message.clone(), initial_message.clone()];
        let mut steps = Vec::new();

        loop {
            current_step += 1;

            let mut stream = client
                .stream_complete(messages.clone(), tools_definitions.clone())
                .await
                .map_err(|e| AgentError::LLMError(e.to_string()))?;

            let mut has_content = false;
            let mut has_tool_call = false;

            use futures::stream::StreamExt;

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        has_content = true;

                        match chunk.chunk_type {
                            ChunkType::Content => {
                                raw_response.push_str(&chunk.content);

                                if in_thought {
                                    current_thought.push_str(&chunk.content);
                                    if current_thought.contains("TOOL_CALL:") {
                                        let parts: Vec<&str> = current_thought.split("TOOL_CALL:").collect();
                                        if parts.len() > 1 {
                                            let new_thought = parts[0].to_string();
                                            let new_tool_call = parts[1].to_string();
                                            current_thought = new_thought;
                                            in_thought = false;
                                            in_action = true;
                                            tool_call_buffer = new_tool_call;
                                        }
                                    }
                                } else if in_action {
                                    tool_call_buffer.push_str(&chunk.content);
                                }
                            }
                            ChunkType::ToolCall => {
                                has_tool_call = true;
                            }
                            ChunkType::ToolArgs => {
                                has_tool_call = true;
                            }
                            ChunkType::Done => {
                                break;
                            }
                            ChunkType::Error => {
                                return Err(AgentError::LLMError(chunk.content));
                            }
                        }
                    }
                    Err(e) => {
                        return Err(AgentError::LLMError(e.to_string()));
                    }
                }
            }

            if !has_content {
                return Err(AgentError::LLMError("No content received".to_string()));
            }

            if in_action {
                let cleaned = tool_call_buffer.trim().trim_end_matches('`').trim().to_string();

                if let Some((tool_name, args_str)) = cleaned.split_once(':') {
                    let tool_name = tool_name.trim().to_string();
                    let args_str = args_str.trim().to_string();

                    let action_input: serde_json::Value = if args_str.starts_with('{') {
                        serde_json::from_str(&args_str).unwrap_or(serde_json::json!({}))
                    } else {
                        serde_json::json!({ "input": args_str })
                    };

                    current_action = tool_name.clone();
                    current_action_input = action_input.clone();

                    let assistant_message = Message {
                        role: MessageRole::Assistant,
                        content: format!("TOOL_CALL:{}:{}", tool_name, args_str),
                        tool_calls: Some(vec![crate::clients::ToolCall {
                            id: format!("call_{}", current_step),
                            function: crate::clients::ToolFunction {
                                name: tool_name.clone(),
                                arguments: args_str,
                            },
                        }]),
                    };
                    messages.push(assistant_message.clone());

                    let tool = tool_manager.get(&tool_name)
                        .ok_or_else(|| AgentError::ToolError(format!("Unknown tool: {}", tool_name)))?;

                    let result = tool.execute(action_input.clone())
                        .await
                        .map_err(|e| AgentError::ToolError(e.to_string()))?;

                    let tool_result_msg = Message {
                        role: MessageRole::Tool,
                        content: serde_json::to_string(&result).unwrap_or_default(),
                        tool_calls: None,
                    };
                    messages.push(tool_result_msg.clone());

                    let step = Step {
                        thought: current_thought.clone(),
                        action: tool_name.clone(),
                        action_input: action_input.clone(),
                        observation: serde_json::to_string(&result).unwrap_or_default(),
                        raw: raw_response.clone(),
                    };

                    steps.push(step.clone());

                    if let Some(ref callback) = self.step_callback {
                        callback(steps.len(), step);
                    }

                    current_thought.clear();
                    current_action.clear();
                    current_action_input = serde_json::json!({});
                    raw_response.clear();
                    in_thought = true;
                    in_action = false;
                    tool_call_buffer.clear();
                }
            } else if !current_thought.is_empty() {
                let step = Step {
                    thought: current_thought.clone(),
                    action: current_action.clone(),
                    action_input: current_action_input.clone(),
                    observation: String::new(),
                    raw: raw_response.clone(),
                };

                steps.push(step.clone());

                if let Some(ref callback) = self.step_callback {
                    callback(steps.len(), step);
                }

                current_thought.clear();
                current_action.clear();
                current_action_input = serde_json::json!({});
                raw_response.clear();
                in_thought = true;
                in_action = false;
            }

            if current_step >= self.max_steps {
                return Err(AgentError::MaxStepsExceeded);
            }

            if !has_tool_call && has_content {
                if let Some(final_content) = current_thought.split("FINAL:").nth(1) {
                    if !final_content.trim().is_empty() {
                        let final_message = Message {
                            role: MessageRole::User,
                            content: format!("Task completed. Final response: {}", final_content.trim()),
                            tool_calls: None,
                        };
                        messages.push(final_message);
                        break;
                    }
                }
            }
        }

        Ok(steps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clients::OpenAIClient;
    use std::path::PathBuf;

    #[test]
    fn test_step_new() {
        let step = Step::new(
            "Thinking".to_string(),
            "read_file".to_string(),
            serde_json::json!({"path": "test.txt"}),
            "File content".to_string(),
            "raw response".to_string(),
        );

        assert_eq!(step.thought, "Thinking");
        assert_eq!(step.action, "read_file");
    }

    #[test]
    fn test_react_agent_new() {
        let client = Box::new(OpenAIClient::new("test_key".to_string(), "gpt-4".to_string()));
        let tools = ToolManager::new();
        let working_dir = PathBuf::from("/tmp");

        let agent = ReactAgent::new(
            client,
            tools,
            working_dir,
            Some(50),
            Some(true),
            None,
        );

        assert_eq!(agent.max_steps, 50);
    }
}
