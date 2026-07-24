// `SessionWriter` trait REMOVED 2026-06-15 in change
// `2026-06-15-p2-trait-cleanup` because it had 0 trait-bound usage, 0 dyn
// dispatch outside `perform_compaction_with_logging`, and exactly 1 real
// implementation (`NoOpSessionWriter`). Methods are now inherent on
// `NoOpSessionWriter`; `perform_compaction_with_logging` now takes
// `&NoOpSessionWriter` instead of `&dyn SessionWriter`.

use crate::types::{CompactionEvent, SummaryMessage};

pub struct NoOpSessionWriter;

impl NoOpSessionWriter {
    pub async fn write_summary(
        &self,
        _summary: &SummaryMessage,
    ) -> Result<(), crate::types::ContextError> {
        Ok(())
    }

    pub async fn log_compaction_event(
        &self,
        _event: &CompactionEvent,
    ) -> Result<(), crate::types::ContextError> {
        Ok(())
    }
}

pub fn create_compaction_event(
    session_id: &str,
    first_msg_id: String,
    last_msg_id: String,
    result: &crate::compactor::CompactionResult,
) -> CompactionEvent {
    CompactionEvent::from_compaction_result(
        uuid::Uuid::new_v4().to_string(),
        session_id.to_string(),
        first_msg_id,
        last_msg_id,
        result,
    )
}

pub async fn perform_compaction_with_logging(
    session_id: &str,
    messages: &[synthia_provider::Message],
    compact_range: std::ops::Range<usize>,
    token_budget: usize,
    provider: Option<&dyn crate::compactor::CompactionProvider>,
    previous_summary: Option<&str>,
    writer: &NoOpSessionWriter,
) -> Result<crate::compactor::CompactionResult, crate::types::ContextError> {
    let result = crate::compactor::apply_compaction(
        messages,
        compact_range.clone(),
        token_budget,
        provider,
        previous_summary,
    )
    .await?;

    writer.write_summary(&result.summary).await?;

    let first_msg_id = messages
        .get(compact_range.start)
        .and_then(|m| m.tool_call_id.clone())
        .unwrap_or_else(|| format!("msg-{}", compact_range.start));

    let last_msg_id = messages
        .get(compact_range.end.saturating_sub(1))
        .and_then(|m| m.tool_call_id.clone())
        .unwrap_or_else(|| {
            format!("msg-{}", compact_range.end.saturating_sub(1))
        });

    let event =
        create_compaction_event(session_id, first_msg_id, last_msg_id, &result);
    writer.log_compaction_event(&event).await?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        compactor::CompactionResult,
        types::{CompactionRange, SummaryMessage},
    };

    #[tokio::test]
    async fn test_noop_writer_write_summary() {
        let writer = NoOpSessionWriter;
        let summary = SummaryMessage {
            role: "assistant".to_string(),
            summary: "Test summary".to_string(),
            message_count: 2,
        };
        assert!(writer.write_summary(&summary).await.is_ok());
    }

    #[tokio::test]
    async fn test_noop_writer_log_event() {
        let writer = NoOpSessionWriter;
        let range = CompactionRange {
            first_message_id: "msg-0".to_string(),
            last_message_id: "msg-1".to_string(),
            compacted_count: 2,
            original_tokens: 100,
            compacted_tokens: 50,
        };
        let event = CompactionEvent::new(
            "evt-1".to_string(),
            "session-1".to_string(),
            range,
            "Test compaction".to_string(),
            1,
        );
        assert!(writer.log_compaction_event(&event).await.is_ok());
    }

    #[test]
    fn test_create_compaction_event() {
        let result = CompactionResult {
            compacted_indices: vec![0, 1],
            applied_level: 1,
            summary: SummaryMessage {
                role: "assistant".to_string(),
                summary: "Test summary".to_string(),
                message_count: 2,
            },
            original_tokens: 100,
            compacted_tokens: 50,
        };
        let event = create_compaction_event(
            "session-1",
            "msg-0".to_string(),
            "msg-1".to_string(),
            &result,
        );
        assert_eq!(event.session_id, "session-1");
        assert_eq!(event.applied_level, 1);
        assert_eq!(event.compacted_range.compacted_count, 2);
    }

    #[tokio::test]
    async fn test_perform_compaction_with_logging_no_provider() {
        use synthia_provider::Message;

        let writer = NoOpSessionWriter;
        let messages = vec![
            Message::user("Test message"),
            Message::assistant("Test response"),
        ];

        let result = perform_compaction_with_logging(
            "session-1",
            &messages,
            0..2,
            1000,
            None,
            None,
            &writer,
        )
        .await
        .unwrap();

        assert_eq!(result.applied_level, 2);
    }
}
