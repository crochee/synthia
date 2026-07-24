use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Checkpoint error: {0}")]
    Checkpoint(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPart {
    pub content: String,
    pub source: String,
    pub token_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionPart {
    pub content: String,
    pub original_tokens: usize,
    pub compacted_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryMessage {
    pub role: String,
    pub summary: String,
    pub message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeMarker {
    pub start_index: usize,
    pub end_index: usize,
    pub compaction_metadata: Option<serde_json::Value>,
}

impl RangeMarker {
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            start_index: start,
            end_index: end,
            compaction_metadata: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionEvent {
    pub event_type: String,
    pub compacted_range: CompactionRange,
    pub summary: String,
    pub id: String,
    pub session_id: String,
    pub applied_level: usize,
    pub original_tokens: usize,
    pub compacted_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionRange {
    pub first_message_id: String,
    pub last_message_id: String,
    pub compacted_count: usize,
    pub original_tokens: usize,
    pub compacted_tokens: usize,
}

impl CompactionEvent {
    pub fn new(
        id: String,
        session_id: String,
        range: CompactionRange,
        summary: String,
        applied_level: usize,
    ) -> Self {
        Self {
            event_type: "compaction".to_string(),
            compacted_range: range.clone(),
            summary,
            id,
            session_id,
            applied_level,
            original_tokens: range.original_tokens,
            compacted_tokens: range.compacted_tokens,
        }
    }

    pub fn from_compaction_result(
        id: String,
        session_id: String,
        first_msg_id: String,
        last_msg_id: String,
        result: &crate::compactor::CompactionResult,
    ) -> Self {
        Self {
            event_type: "compaction".to_string(),
            compacted_range: CompactionRange {
                first_message_id: first_msg_id,
                last_message_id: last_msg_id,
                compacted_count: result.compacted_indices.len(),
                original_tokens: result.original_tokens,
                compacted_tokens: result.compacted_tokens,
            },
            summary: result.summary.summary.clone(),
            id,
            session_id,
            applied_level: result.applied_level,
            original_tokens: result.original_tokens,
            compacted_tokens: result.compacted_tokens,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_part() {
        let part = ContextPart {
            content: "hello".to_string(),
            source: "user".to_string(),
            token_count: 10,
        };
        assert_eq!(part.content, "hello");
    }

    #[test]
    fn test_range_marker() {
        let marker = RangeMarker::new(0, 5);
        assert_eq!(marker.start_index, 0);
        assert_eq!(marker.end_index, 5);
        assert!(marker.compaction_metadata.is_none());
    }

    #[test]
    fn test_summary_message() {
        let summary = SummaryMessage {
            role: "assistant".to_string(),
            summary: "User asked about Rust".to_string(),
            message_count: 2,
        };
        assert_eq!(summary.message_count, 2);
    }

    #[test]
    fn test_compaction_event_new() {
        let range = CompactionRange {
            first_message_id: "msg-0".to_string(),
            last_message_id: "msg-4".to_string(),
            compacted_count: 5,
            original_tokens: 1000,
            compacted_tokens: 150,
        };
        let event = CompactionEvent::new(
            "evt-1".to_string(),
            "session-1".to_string(),
            range,
            "Compacted 5 messages".to_string(),
            2,
        );
        assert_eq!(event.event_type, "compaction");
        assert_eq!(event.compacted_range.compacted_count, 5);
        assert_eq!(event.applied_level, 2);
        assert!(event.compacted_tokens > 0);
    }

    #[test]
    fn test_compaction_range() {
        let range = CompactionRange {
            first_message_id: "msg-0".to_string(),
            last_message_id: "msg-9".to_string(),
            compacted_count: 10,
            original_tokens: 2000,
            compacted_tokens: 200,
        };
        assert_eq!(range.compacted_count, 10);
        assert!(range.original_tokens > range.compacted_tokens);
    }
}
