//! Smart Compaction Agent (OpenSpec Task 5).
//!
//! Provides context-aware compaction with LLM-driven decisions:
//! - [`select_tokens`] - backward walk token selection preserving recent suffix
//! - [`CompactionMessage`] - structured compaction message with summary and recent
//! - [`SmartCompactionAgent`] - orchestrates LLM summarization with incremental chaining

use synthia_provider::Message;

pub use crate::compaction::level1::CompactionProvider;
use crate::types::ContextError;

/// Maximum LLM summary output cap (4K tokens).
const MAX_SUMMARY_TOKENS: usize = 4096;

/// Select tokens by walking backward from most recent message.
///
/// Preserves suffix of most recent message if it would overflow `keep_tokens`.
/// Filters out prior `compaction` messages.
///
/// Returns `(selected, recent)` where:
/// - `selected`: messages to summarize
/// - `recent`: suffix of overflowing message (empty if no overflow)
pub fn select_tokens(
    entries: &[Message],
    keep_tokens: usize,
) -> (Vec<Message>, Option<Message>) {
    let mut total_tokens: usize = 0;
    let mut result = Vec::with_capacity(entries.len());
    let mut recent_suffix: Option<Message> = None;

    // Walk backward
    for entry in entries.iter().rev() {
        let entry_tokens = estimate_message_tokens_compaction(entry);

        if total_tokens + entry_tokens <= keep_tokens {
            result.insert(0, entry.clone());
            total_tokens += entry_tokens;
        } else if recent_suffix.is_none() {
            // This entry overflows - keep the suffix of it
            let text = extract_message_text_compaction(entry);
            // Keep as much of the message as fits
            let chars_to_keep = keep_tokens.saturating_sub(total_tokens) * 4;
            if chars_to_keep > 0 && chars_to_keep < text.len() {
                let truncated = truncate_to_chars(&text, chars_to_keep);
                recent_suffix = Some(Message::new(
                    entry.role,
                    synthia_provider::Content::text(truncated),
                ));
            } else if entry_tokens > keep_tokens {
                // Entry itself is larger than keep_tokens, keep what we can
                let truncated = truncate_to_chars(&text, chars_to_keep.max(1));
                recent_suffix = Some(Message::new(
                    entry.role,
                    synthia_provider::Content::text(truncated),
                ));
            }
            break;
        } else {
            // Already have recent_suffix, skip older messages
            break;
        }
    }

    // Filter out prior compaction messages
    let filtered: Vec<Message> = result
        .into_iter()
        .filter(|m| !is_compaction_message(m))
        .collect();

    (filtered, recent_suffix)
}

/// Check if a message is a prior compaction message.
fn is_compaction_message(msg: &Message) -> bool {
    let text = extract_message_text_compaction(msg);
    text.contains("[Compacted")
        || text.contains("<previous-summary>")
        || text.contains("## Goal") && text.contains("## Progress")
}

fn extract_message_text_compaction(msg: &Message) -> String {
    msg.content.extract_text().unwrap_or_default()
}

fn estimate_message_tokens_compaction(msg: &Message) -> usize {
    let text = extract_message_text_compaction(msg);
    text.chars().count() / 4
}

fn truncate_to_chars(s: &str, max_chars: usize) -> String {
    let mut chars_kept = 0;
    s.chars()
        .take_while(|_| {
            if chars_kept >= max_chars {
                false
            } else {
                chars_kept += 1;
                true
            }
        })
        .collect()
}

/// Compaction message type with summary and recent suffix.
///
/// Created after successful compaction to preserve context.
#[derive(Debug, Clone)]
pub struct CompactionMessage {
    /// The LLM-generated or fallback summary.
    pub summary: String,
    /// Suffix of the overflowing message (if any).
    pub recent: Option<String>,
}

impl CompactionMessage {
    /// Create a new compaction message.
    pub fn new(summary: String, recent: Option<String>) -> Self {
        Self { summary, recent }
    }

    /// Convert to a system message for context injection.
    pub fn to_system_message(&self) -> Message {
        let mut content = self.summary.clone();
        if let Some(ref recent) = self.recent {
            content.push_str("\n\n[Recent context preserved]: ");
            content.push_str(recent);
        }
        Message::system(&content)
    }
}

/// Smart compaction agent that orchestrates LLM-driven context compression.
pub struct SmartCompactionAgent<P: CompactionProvider> {
    provider: P,
    max_summary_tokens: usize,
}

impl<P: CompactionProvider> SmartCompactionAgent<P> {
    /// Create a new SmartCompactionAgent.
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            max_summary_tokens: MAX_SUMMARY_TOKENS,
        }
    }

    /// Generate a summary using the LLM provider.
    ///
    /// Uses incremental chaining when `previous_summary` is provided.
    /// Falls back to heuristic summary on provider failure.
    ///
    /// Returns `Ok("")` if summary is empty (caller should abandon compaction).
    pub async fn summarize(
        &self,
        entries: &[Message],
        previous_summary: Option<&str>,
    ) -> Result<String, ContextError> {
        if entries.is_empty() {
            return Ok(String::new());
        }

        // Try LLM summarization
        match self
            .provider
            .generate_summary(entries, previous_summary)
            .await
        {
            Ok(summary) if !summary.is_empty() => {
                if summary.chars().count() > self.max_summary_tokens * 4 {
                    // Truncate to max_summary_tokens
                    Ok(truncate_to_chars(&summary, self.max_summary_tokens))
                } else {
                    Ok(summary)
                }
            }
            Ok(_) | Err(_) => {
                // Fallback to heuristic structured summary
                Ok(self.build_heuristic_fallback(entries, previous_summary))
            }
        }
    }

    /// Build a heuristic structured summary when LLM is unavailable.
    fn build_heuristic_fallback(
        &self,
        entries: &[Message],
        previous_summary: Option<&str>,
    ) -> String {
        use crate::compaction::level1::PREVIOUS_SUMMARY_MAX_CHARS;

        let mut decisions: Vec<String> = Vec::new();
        let mut user_requests: Vec<String> = Vec::new();

        let mut i = 0;
        while i < entries.len() {
            let msg = &entries[i];

            if matches!(msg.role, synthia_provider::Role::User) {
                let text = extract_message_text_compaction(msg);
                if !text.is_empty() {
                    user_requests.push(truncate_to_chars(&text, 100));
                }
                i += 1;
            } else if matches!(msg.role, synthia_provider::Role::Assistant) {
                let text = extract_message_text_compaction(msg);
                if !text.is_empty() {
                    decisions.push(truncate_to_chars(&text, 150));
                }
                i += 1;
            } else {
                i += 1;
            }
        }

        let mut sections = Vec::new();
        sections.push(format!("[Summary of {} messages]", entries.len()));

        if !user_requests.is_empty() {
            sections
                .push(format!("User Requests: {}", user_requests.join("; ")));
        }
        if !decisions.is_empty() {
            sections
                .push(format!("Assistant Responses: {}", decisions.join("; ")));
        }

        if sections.len() == 1 {
            sections.push("[No significant content]".to_string());
        }

        let body = sections.join(" | ");

        // Chain with previous summary if provided
        match previous_summary {
            Some(prev) if !prev.is_empty() => {
                let truncated =
                    truncate_to_chars(prev, PREVIOUS_SUMMARY_MAX_CHARS);
                format!(
                    "<previous-summary>\n{}\n</previous-summary>\n{}",
                    truncated, body
                )
            }
            _ => body,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_msg(text: &str) -> Message {
        Message::user(text)
    }

    fn assistant_msg(text: &str) -> Message {
        Message::assistant(text)
    }

    #[test]
    fn test_select_tokens_simple() {
        let msgs = vec![user_msg("hello"), assistant_msg("world")];
        // Each ~5 chars = ~1-2 tokens, 2 messages = ~10 chars = ~2-3 tokens
        let (selected, recent) = select_tokens(&msgs, 1000);
        assert_eq!(selected.len(), 2);
        assert!(recent.is_none());
    }

    #[test]
    fn test_select_tokens_filters_compaction() {
        let msgs = vec![
            user_msg("hello"),
            assistant_msg("world"),
            user_msg("[Compacted: 10 messages]"),
            assistant_msg("## Goal\nBuild things\n## Progress\nDone"),
        ];
        let (selected, _recent) = select_tokens(&msgs, 1000);
        // Compaction messages should be filtered out
        assert!(selected.iter().all(|m| !is_compaction_message(m)));
    }

    #[test]
    fn test_select_tokens_overflow() {
        let long_text = "a".repeat(1000);
        let msgs = vec![user_msg(&long_text)];

        // Only 100 tokens = ~400 chars
        let (selected, recent) = select_tokens(&msgs, 100);
        assert!(selected.is_empty());
        assert!(recent.is_some());
    }

    #[test]
    fn test_compaction_message_to_system_message() {
        let msg = CompactionMessage::new(
            "## Summary\nOld context summarized".to_string(),
            Some("recent part".to_string()),
        );
        let system = msg.to_system_message();
        let text = extract_message_text_compaction(&system);
        assert!(text.contains("## Summary"));
        assert!(text.contains("recent part"));
    }

    #[tokio::test]
    async fn test_summarize_empty_entries() {
        use async_trait::async_trait;

        struct MockProvider;
        #[async_trait]
        impl CompactionProvider for MockProvider {
            async fn generate_summary(
                &self,
                _messages: &[Message],
                _previous_summary: Option<&str>,
            ) -> Result<String, ContextError> {
                Ok("summary".to_string())
            }
        }

        let agent = SmartCompactionAgent::new(MockProvider);
        let result = agent.summarize(&[], None).await.unwrap();
        assert_eq!(result, "");
    }
}
