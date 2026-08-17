//! Tests for A2A StreamResponse serde serialization.
//!
//! Proto3's default-value omission rule strips `bool` fields that are
//! `false` (e.g., `append: false` in `TaskArtifactUpdateEvent`). The fix
//! is applied at the proto level by declaring `append` and `last_chunk`
//! as `optional bool` in the upstream `a2a.proto` schema, which generates
//! `Option<bool>` in Rust and preserves `Some(false)` → `"append": false`
//! in JSON output.
//!
//! These tests verify that the serde serialization of `StreamResponse`
//! correctly preserves `Option<bool>` fields.

#[cfg(test)]
mod tests {
    use a2a::{
        Artifact,
        StreamResponse,
        TaskArtifactUpdateEvent,
        TaskState,
        TaskStatus,
        TaskStatusUpdateEvent,
    };

    #[test]
    fn artifact_update_with_append_false_serializes_correctly() {
        let event = StreamResponse::ArtifactUpdate(TaskArtifactUpdateEvent {
            task_id: "t1".to_string(),
            context_id: "c1".to_string(),
            artifact: Artifact {
                artifact_id: "a1".to_string(),
                name: None,
                description: None,
                parts: vec![],
                metadata: None,
                extensions: None,
            },
            append: Some(false),
            last_chunk: Some(true),
            metadata: None,
        });

        let json = serde_json::to_string(&event).unwrap();
        assert!(
            json.contains(r#""append":false"#),
            "append:false must appear in JSON, got: {json}"
        );
        assert!(
            json.contains(r#""lastChunk":true"#),
            "lastChunk:true must appear in JSON, got: {json}"
        );
    }

    #[test]
    fn status_update_serializes_correctly() {
        let event = StreamResponse::StatusUpdate(TaskStatusUpdateEvent {
            task_id: "t1".to_string(),
            context_id: "c1".to_string(),
            status: TaskStatus {
                state: TaskState::Working,
                message: None,
                timestamp: None,
            },
            metadata: None,
        });

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("statusUpdate"));
        assert!(json.contains("TASK_STATE_WORKING"));
    }

    /// `TaskStatus.message` MUST round-trip through JSON when
    /// populated — the front-end reads
    /// `statusUpdate.status.message.parts[0].text` to display
    /// the upstream error / max-iterations reason. A serializer
    /// that drops the field silently would leave the user
    /// staring at a bare `failed` state with no actionable
    /// detail. Pin the wire shape.
    #[test]
    fn status_update_carries_error_message_through_serde() {
        use a2a::{Message, Part, Role};
        let event = StreamResponse::StatusUpdate(TaskStatusUpdateEvent {
            task_id: "t1".to_string(),
            context_id: "c1".to_string(),
            status: TaskStatus {
                state: TaskState::Failed,
                message: Some(Message::new(
                    Role::Agent,
                    vec![Part::text(
                        "stream error (http_failure): rate limited".to_string(),
                    )],
                )),
                timestamp: None,
            },
            metadata: None,
        });

        let json = serde_json::to_string(&event).unwrap();
        assert!(
            json.contains("stream error (http_failure): rate limited"),
            "status.message text MUST round-trip through serde, got: {json}"
        );
        assert!(
            json.contains("TASK_STATE_FAILED"),
            "TASK_STATE_FAILED must appear, got: {json}"
        );
    }
}
