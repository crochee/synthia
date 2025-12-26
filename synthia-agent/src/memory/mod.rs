use crate::clients::{Message, MessageRole};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::num::NonZeroUsize;

const DEFAULT_MAX_TOKENS: usize = 8000;
const DEFAULT_COMPRESSION_RATIO: f64 = 0.7;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationContext {
    pub messages: Vec<Message>,
    pub tool_results: Vec<ToolResult>,
    pub metadata: ContextMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub result: serde_json::Value,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextMetadata {
    pub total_tokens: usize,
    pub compressed: bool,
    pub compression_count: usize,
}

impl Default for ContextMetadata {
    fn default() -> Self {
        Self {
            total_tokens: 0,
            compressed: false,
            compression_count: 0,
        }
    }
}

pub struct ContextCompressor {
    max_tokens: NonZeroUsize,
    compression_ratio: f64,
    preserve_recent: usize,
}

impl ContextCompressor {
    pub fn new(max_tokens: usize, compression_ratio: f64, preserve_recent: usize) -> Self {
        Self {
            max_tokens: NonZeroUsize::new(max_tokens).unwrap_or(NonZeroUsize::new(DEFAULT_MAX_TOKENS).unwrap()),
            compression_ratio: if compression_ratio > 0.0 && compression_ratio < 1.0 {
                compression_ratio
            } else {
                DEFAULT_COMPRESSION_RATIO
            },
            preserve_recent,
        }
    }

    pub fn with_tokens(max_tokens: usize) -> Self {
        Self::new(max_tokens, DEFAULT_COMPRESSION_RATIO, 3)
    }

    pub fn compress(
        &self,
        messages: &[Message],
        tool_results: &[ToolResult],
    ) -> (Vec<Message>, Vec<ToolResult>, ContextMetadata) {
        let mut compressed_messages = messages.to_vec();
        let mut compressed_tool_results = tool_results.to_vec();

        let current_tokens = self.count_tokens(&compressed_messages, &compressed_tool_results);

        if current_tokens <= self.max_tokens.get() {
            return (
                compressed_messages,
                compressed_tool_results,
                ContextMetadata {
                    total_tokens: current_tokens,
                    compressed: false,
                    compression_count: 0,
                },
            );
        }

        let system_messages: Vec<Message> = compressed_messages
            .iter()
            .filter(|m| m.role == MessageRole::System)
            .cloned()
            .collect();

        let other_messages: Vec<Message> = compressed_messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .cloned()
            .collect();

        let recent_count = std::cmp::min(self.preserve_recent, other_messages.len());
        let recent_messages: Vec<Message> = other_messages[..recent_count].to_vec();
        let old_messages: Vec<Message> = other_messages[recent_count..].to_vec();

        let summary = self.summarize_messages(&old_messages);

        let mut final_messages = system_messages;
        final_messages.push(Message {
            role: MessageRole::User,
            content: format!(
                "[Previous conversation summarized: {}]",
                summary
            ),
            tool_calls: None,
        });
        final_messages.extend(recent_messages.clone());

        compressed_tool_results = compressed_tool_results
            .into_iter()
            .filter(|tr| {
                recent_messages.iter().any(|m| {
                    m.tool_calls.as_ref().is_some_and(|tc| {
                        tc.iter().any(|call| call.function.name == tr.tool_name)
                    })
                })
            })
            .collect();

        let final_tokens = self.count_tokens(&final_messages, &compressed_tool_results);

        (
            final_messages,
            compressed_tool_results,
            ContextMetadata {
                total_tokens: final_tokens,
                compressed: true,
                compression_count: 1,
            },
        )
    }

    fn summarize_messages(&self, messages: &[Message]) -> String {
        if messages.is_empty() {
            return "No previous conversation".to_string();
        }

        let tool_calls: usize = messages
            .iter()
            .filter(|m| m.tool_calls.is_some())
            .flat_map(|m| m.tool_calls.as_ref().unwrap())
            .count();

        let user_messages: usize = messages
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .count();

        let assistant_messages: usize = messages
            .iter()
            .filter(|m| m.role == MessageRole::Assistant)
            .count();

        format!(
            "{} user messages, {} assistant responses, {} tool calls",
            user_messages,
            assistant_messages,
            tool_calls
        )
    }

    fn count_tokens(&self, messages: &[Message], tool_results: &[ToolResult]) -> usize {
        let message_tokens: usize = messages
            .iter()
            .map(|m| {
                m.content.len() / 4
                    + m.tool_calls.as_ref().map_or(0, |tc| tc.len() * 20)
            })
            .sum();

        let tool_result_tokens: usize = tool_results
            .iter()
            .map(|tr| {
                tr.tool_name.len() / 4
                    + tr.arguments.to_string().len() / 4
                    + tr.result.to_string().len() / 4
            })
            .sum();

        message_tokens + tool_result_tokens
    }
}

pub struct ConversationHistory {
    messages: VecDeque<Message>,
    tool_results: VecDeque<ToolResult>,
    max_messages: usize,
}

impl ConversationHistory {
    pub fn new(max_messages: usize) -> Self {
        Self {
            messages: VecDeque::with_capacity(max_messages),
            tool_results: VecDeque::new(),
            max_messages,
        }
    }

    pub fn add_message(&mut self, message: Message) {
        while self.messages.len() >= self.max_messages {
            self.messages.pop_front();
        }
        self.messages.push_back(message);
    }

    pub fn add_tool_result(&mut self, result: ToolResult) {
        self.tool_results.push_back(result);
    }

    pub fn get_messages(&self) -> Vec<Message> {
        self.messages.iter().cloned().collect()
    }

    pub fn get_tool_results(&self) -> Vec<ToolResult> {
        self.tool_results.iter().cloned().collect()
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.tool_results.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_compressor_new() {
        let compressor = ContextCompressor::new(10000, 0.8, 5);
        assert_eq!(compressor.max_tokens.get(), 10000);
    }

    #[test]
    fn test_compress_no_compression_needed() {
        let compressor = ContextCompressor::with_tokens(10000);
        let messages = vec![Message {
            role: MessageRole::User,
            content: "Hello".to_string(),
            tool_calls: None,
        }];

        let (compressed, _, metadata) = compressor.compress(&messages, &[]);

        assert_eq!(compressed.len(), 1);
        assert!(!metadata.compressed);
    }

    #[test]
    fn test_conversation_history() {
        let mut history = ConversationHistory::new(5);

        history.add_message(Message {
            role: MessageRole::User,
            content: "Test".to_string(),
            tool_calls: None,
        });

        assert_eq!(history.get_messages().len(), 1);
    }
}
