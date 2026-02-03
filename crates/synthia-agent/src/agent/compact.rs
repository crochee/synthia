//! Conversation compaction implementation

use futures::stream::BoxStream;
use rmcp::model::SamplingMessage;

use super::Agent;
use crate::{
    Result,
    config::SessionConfig,
    context::TranscriptManager,
    hooks::HookEvent,
    types::AgentEvent,
};

const COMPACTION_THINKING_TEXT: &str = "Compacting conversation history...";

impl Agent {
    pub(super) async fn compact_conversation(
        &self,
        conversation: &[SamplingMessage],
        session_config: &SessionConfig,
    ) -> Result<(Vec<SamplingMessage>, BoxStream<'static, Result<AgentEvent>>)>
    {
        let Some(result) = self.deps.context.compact(conversation).await?
        else {
            return Ok((
                conversation.to_vec(),
                Box::pin(futures::stream::empty()),
            ));
        };

        // Save transcript before compaction
        let transcript_manager =
            TranscriptManager::new(self.config.workspace_dir.clone());
        if let Err(e) = transcript_manager
            .save_transcript_sync(&session_config.id, conversation)
        {
            tracing::warn!(
                "Failed to save transcript before compaction: {}",
                e
            );
        }

        let messages_removed = result
            .metadata
            .original_count
            .saturating_sub(result.metadata.compacted_count);
        let tokens_saved = result.metadata.tokens_saved as u64;

        self.deps
            .session
            .replace_conversation(session_config, &result.messages)
            .await?;

        self.deps
            .hooks
            .emit(&HookEvent::ContextCompaction {
                messages_removed,
                tokens_saved,
            })
            .await;

        let event_stream = {
            let reason = result.reason.clone();
            let messages = result.messages.clone();
            async_stream::stream! {
                yield Ok(AgentEvent::Message(SamplingMessage::assistant_text(&reason)));
                yield Ok(AgentEvent::Message(SamplingMessage::assistant_text(COMPACTION_THINKING_TEXT)));
                yield Ok(AgentEvent::Message(SamplingMessage::assistant_text("Auto-compaction completed successfully")));
                yield Ok(AgentEvent::HistoryReplaced(messages));
            }
        };

        Ok((result.messages, Box::pin(event_stream)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compaction_thinking_text_value() {
        assert_eq!(
            COMPACTION_THINKING_TEXT,
            "Compacting conversation history..."
        );
    }

    #[test]
    fn test_compaction_thinking_text_not_empty() {
        assert!(!COMPACTION_THINKING_TEXT.is_empty());
    }

    #[test]
    fn test_compaction_thinking_text_is_static() {
        assert!(COMPACTION_THINKING_TEXT.chars().next().is_some());
    }

    #[test]
    fn test_compaction_thinking_text_contains_compacting() {
        assert!(COMPACTION_THINKING_TEXT.contains("Compacting"));
    }

    #[test]
    fn test_compaction_thinking_text_contains_history() {
        assert!(COMPACTION_THINKING_TEXT.contains("history"));
    }
}
