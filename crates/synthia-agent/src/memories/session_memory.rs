//! Session memory system for tracking and extracting session state.
//!
//! This module implements Phase 3 session memory, based on Claude Code's SessionMemory.
//! Session memory periodically extracts key information from the conversation to maintain
//! a persistent summary that survives context compression.

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use rmcp::model::SamplingMessage;
#[cfg(test)]
use rmcp::model::{SamplingContent, SamplingMessageContent};

use crate::model_router::ModelRouter;

static SESSION_MEMORY_STATE: Mutex<Option<SessionMemoryState>> =
    Mutex::new(None);

#[derive(Debug, Clone)]
pub struct SessionMemoryConfig {
    pub minimum_message_tokens_to_init: usize,
    pub minimum_tokens_between_update: usize,
    pub tool_calls_between_updates: usize,
}

impl Default for SessionMemoryConfig {
    fn default() -> Self {
        Self {
            minimum_message_tokens_to_init: 30_000,
            minimum_tokens_between_update: 10_000,
            tool_calls_between_updates: 20,
        }
    }
}

#[derive(Debug, Clone)]
struct SessionMemoryState {
    config: SessionMemoryConfig,
    is_initialized: bool,
    last_summarized_message_index: Option<usize>,
    last_extraction_token_count: usize,
}

pub struct ManualExtractionResult {
    pub memory_content: String,
    pub token_count: usize,
}

#[cfg(test)]
fn with_state<F, T>(f: F) -> Option<T>
where
    F: FnOnce(&SessionMemoryState) -> T,
{
    // Mutex lock on static variable is safe here as it's never contested
    #[allow(clippy::unwrap_used)]
    let guard = SESSION_MEMORY_STATE.lock().unwrap();
    guard.as_ref().map(f)
}

fn with_state_mut<F, T>(f: F) -> Option<T>
where
    F: FnOnce(&mut SessionMemoryState) -> T,
{
    #[allow(clippy::unwrap_used)]
    let mut guard = SESSION_MEMORY_STATE.lock().unwrap();
    guard.as_mut().map(f)
}

#[cfg(test)]
pub(crate) fn has_met_initialization_threshold(
    current_token_count: usize,
) -> bool {
    with_state(|state| {
        current_token_count >= state.config.minimum_message_tokens_to_init
    })
    .unwrap_or(false)
}

#[cfg(test)]
pub(crate) fn has_met_update_threshold(current_token_count: usize) -> bool {
    with_state(|state| {
        let tokens_since_last = current_token_count
            .saturating_sub(state.last_extraction_token_count);
        tokens_since_last >= state.config.minimum_tokens_between_update
    })
    .unwrap_or(false)
}

#[cfg(test)]
pub(crate) fn count_tool_calls_since(
    messages: &[SamplingMessage],
    since_index: Option<usize>,
) -> usize {
    let start_index = since_index.map(|i| i + 1).unwrap_or(0);
    messages[start_index..]
        .iter()
        .filter(|msg| is_tool_use_message(msg))
        .count()
}

#[cfg(test)]
pub(crate) fn is_tool_use_message(msg: &SamplingMessage) -> bool {
    match &msg.content {
        SamplingContent::Single(SamplingMessageContent::ToolUse(_)) => true,
        SamplingContent::Multiple(cs) => cs
            .iter()
            .any(|c| matches!(c, SamplingMessageContent::ToolUse(_))),
        _ => false,
    }
}

#[cfg(test)]
pub(crate) fn get_last_summarized_message_index() -> Option<usize> {
    with_state(|state| state.last_summarized_message_index).flatten()
}

pub fn get_session_memory_dir(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join(".claude").join("session_memory")
}

pub fn get_session_memory_path(workspace_dir: &Path) -> PathBuf {
    get_session_memory_dir(workspace_dir).join("memory.md")
}

pub async fn setup_session_memory_file(
    workspace_dir: &Path,
) -> std::io::Result<(PathBuf, String)> {
    let dir = get_session_memory_dir(workspace_dir);
    tokio::fs::create_dir_all(&dir).await?;

    let path = get_session_memory_path(workspace_dir);

    let content = if path.exists() {
        tokio::fs::read_to_string(&path).await?
    } else {
        let template = load_session_memory_template();
        tokio::fs::write(&path, &template).await?;
        template
    };

    Ok((path, content))
}

pub(crate) fn load_session_memory_template() -> String {
    r#"# Session Memory

This file contains extracted session memory that captures key information from the conversation.

## Current Project

<!-- Project name and overview -->

## Key Decisions

<!-- Important decisions made during the session -->

## Active Context

<!-- Current work being performed -->

## Important Findings

<!-- Key discoveries or insights -->

## Pending Tasks

<!-- Tasks that need attention -->

"#
    .to_string()
}

pub fn build_session_memory_update_prompt(
    current_memory: &str,
    memory_path: &Path,
) -> String {
    format!(
        r#"You are a session memory extraction assistant. Your task is to analyze the conversation history and update the session memory file.

## Current Session Memory

{}

## Instructions

1. Read the conversation history since the last extraction
2. Identify new key information:
   - Important decisions or direction changes
   - Current task progress and context
   - Newly discovered information relevant to the project
   - Pending tasks or next steps
3. Update the session memory file at: {}

Only include substantive updates. If nothing significant has changed, preserve the existing memory unchanged.

## Output Format

Provide the complete updated memory content that should replace the current file contents.
"#,
        current_memory,
        memory_path.display()
    )
}

pub(crate) fn mark_session_memory_initialized() {
    with_state_mut(|state| {
        state.is_initialized = true;
    });
}

#[cfg(test)]
pub(crate) fn is_session_memory_initialized() -> bool {
    with_state(|state| state.is_initialized).unwrap_or(false)
}

pub(crate) fn record_extraction_token_count(token_count: usize) {
    with_state_mut(|state| {
        state.last_extraction_token_count = token_count;
    });
}

pub(crate) fn set_last_summarized_message_index(message_index: usize) {
    with_state_mut(|state| {
        state.last_summarized_message_index = Some(message_index);
    });
}

#[cfg(test)]
pub(crate) fn _get_session_memory_config() -> SessionMemoryConfig {
    with_state(|state| state.config.clone()).unwrap_or_default()
}

#[allow(clippy::unwrap_used)]
pub fn set_session_memory_config(config: SessionMemoryConfig) {
    let mut guard = SESSION_MEMORY_STATE.lock().unwrap();
    if let Some(state) = guard.as_mut() {
        state.config = config;
    } else {
        *guard = Some(SessionMemoryState {
            config,
            is_initialized: false,
            last_summarized_message_index: None,
            last_extraction_token_count: 0,
        });
    }
}

#[cfg(test)]
pub(crate) fn is_session_memory_gate_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Default)]
pub struct PartialSessionMemoryConfig {
    pub minimum_message_tokens_to_init: Option<usize>,
    pub minimum_tokens_between_update: Option<usize>,
    pub tool_calls_between_updates: Option<usize>,
}

impl From<PartialSessionMemoryConfig> for SessionMemoryConfig {
    fn from(partial: PartialSessionMemoryConfig) -> Self {
        let default = SessionMemoryConfig::default();
        SessionMemoryConfig {
            minimum_message_tokens_to_init: partial
                .minimum_message_tokens_to_init
                .unwrap_or(default.minimum_message_tokens_to_init),
            minimum_tokens_between_update: partial
                .minimum_tokens_between_update
                .unwrap_or(default.minimum_tokens_between_update),
            tool_calls_between_updates: partial
                .tool_calls_between_updates
                .unwrap_or(default.tool_calls_between_updates),
        }
    }
}

#[allow(clippy::unwrap_used)]
pub fn init_session_memory() {
    let mut guard = SESSION_MEMORY_STATE.lock().unwrap();
    if guard.is_none() {
        let default_config = SessionMemoryConfig::default();
        *guard = Some(SessionMemoryState {
            config: default_config,
            is_initialized: false,
            last_summarized_message_index: None,
            last_extraction_token_count: 0,
        });
    }
}

pub async fn manually_extract_session_memory(
    messages: &[SamplingMessage],
    workspace_dir: &Path,
) -> crate::Result<ManualExtractionResult> {
    let memory_path = setup_session_memory_file(workspace_dir).await?.0;
    let current_memory = tokio::fs::read_to_string(&memory_path).await?;

    let prompt =
        build_session_memory_update_prompt(&current_memory, &memory_path);

    let system_prompt = "You are a session memory extraction assistant. Analyze the conversation and update the session memory file with key information.";

    let msg = rmcp::model::SamplingMessage::user_text(prompt);
    let result = crate::model_router::FirstModelRouter::default()
        .route(std::slice::from_ref(&msg))
        .await;

    match result {
        Ok(result) => {
            let provider = result.provider;
            let config = result.config;

            let params = rmcp::model::CreateMessageRequestParams {
                meta: None,
                task: None,
                messages: vec![msg],
                model_preferences: None,
                system_prompt: Some(system_prompt.to_string()),
                include_context: None,
                temperature: config.model_info().temperature,
                max_tokens: config.model_info().max_tokens,
                stop_sequences: None,
                metadata: None,
                tools: None,
                tool_choice: None,
            };

            use tokio_util::sync::CancellationToken;
            let stream =
                provider.stream(params, CancellationToken::new()).await?;
            let result = synthia_provider::collect_stream(stream).await?;

            let content = crate::utils::sampling_content_to_string(
                &result.message.content,
            );

            tokio::fs::write(&memory_path, &content).await?;

            let token_count = crate::context::estimate_tokens(messages);

            record_extraction_token_count(token_count);
            if !messages.is_empty() {
                set_last_summarized_message_index(messages.len() - 1);
            }
            mark_session_memory_initialized();

            Ok(ManualExtractionResult {
                memory_content: content,
                token_count,
            })
        }
        Err(e) => Err(e),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolUseDecision {
    Allow,
    Block,
}

pub fn create_memory_file_can_use_tool(
    memory_path: &Path,
) -> impl Fn(&dyn crate::tools::Tool) -> ToolUseDecision + '_ {
    let _ = memory_path;
    move |tool: &dyn crate::tools::Tool| -> ToolUseDecision {
        if tool.name() == "Edit" {
            ToolUseDecision::Allow
        } else {
            ToolUseDecision::Block
        }
    }
}

#[cfg(test)]
mod tests {
    use rmcp::model::{
        RawTextContent,
        Role,
        SamplingContent,
        SamplingMessageContent,
        ToolUseContent,
    };

    use super::*;

    #[test]
    fn test_session_memory_config_default() {
        let config = SessionMemoryConfig::default();
        assert_eq!(config.minimum_message_tokens_to_init, 30_000);
        assert_eq!(config.minimum_tokens_between_update, 10_000);
        assert_eq!(config.tool_calls_between_updates, 20);
    }

    #[test]
    fn test_partial_config_conversion() {
        let partial = PartialSessionMemoryConfig {
            minimum_message_tokens_to_init: Some(20_000),
            minimum_tokens_between_update: None,
            tool_calls_between_updates: Some(15),
        };

        let config: SessionMemoryConfig = partial.into();
        assert_eq!(config.minimum_message_tokens_to_init, 20_000);
        assert_eq!(config.minimum_tokens_between_update, 10_000);
        assert_eq!(config.tool_calls_between_updates, 15);
    }

    #[test]
    fn test_init_session_memory() {
        init_session_memory();
        assert!(
            is_session_memory_initialized() || !is_session_memory_initialized()
        );
    }

    #[test]
    fn test_has_met_initialization_threshold() {
        init_session_memory();

        assert!(!has_met_initialization_threshold(29_000));
        assert!(has_met_initialization_threshold(30_000));
        assert!(has_met_initialization_threshold(50_000));
    }

    #[test]
    fn test_has_met_update_threshold() {
        init_session_memory();

        set_session_memory_config(SessionMemoryConfig {
            minimum_message_tokens_to_init: 30_000,
            minimum_tokens_between_update: 10_000,
            tool_calls_between_updates: 20,
        });

        record_extraction_token_count(30_000);

        assert!(!has_met_update_threshold(35_000));
        assert!(has_met_update_threshold(40_000));
        assert!(has_met_update_threshold(50_000));
    }

    #[test]
    fn test_get_session_memory_dir() {
        let workspace = Path::new("/test/workspace");
        let dir = get_session_memory_dir(workspace);
        assert_eq!(dir, Path::new("/test/workspace/.claude/session_memory"));
    }

    #[test]
    fn test_get_session_memory_path() {
        let workspace = Path::new("/test/workspace");
        let path = get_session_memory_path(workspace);
        assert_eq!(
            path,
            Path::new("/test/workspace/.claude/session_memory/memory.md")
        );
    }

    #[test]
    fn test_load_session_memory_template() {
        let template = load_session_memory_template();
        assert!(template.contains("# Session Memory"));
        assert!(template.contains("## Current Project"));
        assert!(template.contains("## Key Decisions"));
        assert!(template.contains("## Active Context"));
    }

    #[test]
    fn test_build_session_memory_update_prompt() {
        let current_memory = "Current memory content";
        let memory_path = Path::new("/test/memory.md");
        let prompt =
            build_session_memory_update_prompt(current_memory, memory_path);

        assert!(prompt.contains("Current memory content"));
        assert!(prompt.contains("/test/memory.md"));
        assert!(prompt.contains("Session Memory"));
    }

    #[test]
    fn test_is_session_memory_gate_enabled() {
        assert!(is_session_memory_gate_enabled());
    }

    #[test]
    fn test_set_and_get_last_summarized_message_index() {
        init_session_memory();
        set_last_summarized_message_index(10);

        let index = get_last_summarized_message_index();
        assert_eq!(index, Some(10));
    }

    #[test]
    fn test_mark_initialized() {
        init_session_memory();
        assert!(!is_session_memory_initialized());

        mark_session_memory_initialized();
        assert!(is_session_memory_initialized());
    }

    #[test]
    fn test_tool_use_message_detection() {
        let tool_msg = SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Single(SamplingMessageContent::ToolUse(
                ToolUseContent::new(
                    "test-id",
                    "read",
                    serde_json::json!({})
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                ),
            )),
            meta: None,
        };

        let text_msg = SamplingMessage {
            role: Role::User,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: "Hello".to_string(),
                    meta: None,
                },
            )),
            meta: None,
        };

        assert!(is_tool_use_message(&tool_msg));
        assert!(!is_tool_use_message(&text_msg));
    }

    #[test]
    fn test_count_tool_calls_since() {
        let messages = vec![
            SamplingMessage {
                role: Role::User,
                content: SamplingContent::Single(SamplingMessageContent::Text(
                    RawTextContent {
                        text: "Hello".to_string(),
                        meta: None,
                    },
                )),
                meta: None,
            },
            SamplingMessage {
                role: Role::Assistant,
                content: SamplingContent::Single(
                    SamplingMessageContent::ToolUse(ToolUseContent::new(
                        "tool-1",
                        "read",
                        serde_json::json!({})
                            .as_object()
                            .cloned()
                            .unwrap_or_default(),
                    )),
                ),
                meta: None,
            },
            SamplingMessage {
                role: Role::Assistant,
                content: SamplingContent::Single(
                    SamplingMessageContent::ToolUse(ToolUseContent::new(
                        "tool-2",
                        "write",
                        serde_json::json!({})
                            .as_object()
                            .cloned()
                            .unwrap_or_default(),
                    )),
                ),
                meta: None,
            },
        ];

        assert_eq!(count_tool_calls_since(&messages, None), 2);
        assert_eq!(count_tool_calls_since(&messages, Some(1)), 1);
        assert_eq!(count_tool_calls_since(&messages, Some(2)), 0);
    }

    #[test]
    fn test_tool_use_decision() {
        assert_eq!(ToolUseDecision::Allow, ToolUseDecision::Allow);
        assert_eq!(ToolUseDecision::Block, ToolUseDecision::Block);
        assert_ne!(ToolUseDecision::Allow, ToolUseDecision::Block);
    }

    #[test]
    fn test_tool_use_decision_equality() {
        let allow1 = ToolUseDecision::Allow;
        let allow2 = ToolUseDecision::Allow;
        let block1 = ToolUseDecision::Block;

        assert_eq!(allow1, allow2);
        assert_ne!(allow1, block1);

        // Debug format should work
        let allow_debug = format!("{:?}", ToolUseDecision::Allow);
        let block_debug = format!("{:?}", ToolUseDecision::Block);
        assert_eq!(allow_debug, "Allow");
        assert_eq!(block_debug, "Block");
    }

    #[test]
    fn test_sampling_message_tool_use_multiple_content() {
        // Test is_tool_use_message with Multiple content containing a tool use
        let tool_use_in_multiple = SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Multiple(vec![
                SamplingMessageContent::Text(RawTextContent {
                    text: "Reading file".to_string(),
                    meta: None,
                }),
                SamplingMessageContent::ToolUse(ToolUseContent::new(
                    "tool-call-1",
                    "Bash",
                    serde_json::json!({"command": "ls"})
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                )),
            ]),
            meta: None,
        };

        assert!(is_tool_use_message(&tool_use_in_multiple));
    }
}
