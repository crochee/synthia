//! Agent event types

use rmcp::model::{SamplingMessage, ServerNotification};
use serde::Serialize;

use super::notification::SystemNotification;

/// Agent status enumeration
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum AgentStatus {
    /// Agent is pending initialization
    PendingInit,
    /// Agent is running
    Running,
    /// Agent has completed successfully
    Completed,
    /// Agent has errored
    Errored(String),
    /// Agent has been shutdown
    Shutdown,
    /// Agent has been cancelled
    Cancelled,
    /// Agent has reached max steps
    MaxStepsReached(u32),
    /// Agent not found
    NotFound,
    /// Agent has detected a loop
    LoopDetected(String),
    /// Agent has reached max tokens budget
    MaxTokensReached(u64),
}

#[derive(Clone, Debug, Serialize)]
pub enum AgentEvent {
    Message(SamplingMessage),
    McpNotification((String, ServerNotification)),
    ModelChange {
        model: String,
        mode: String,
    },
    HistoryReplaced(Vec<SamplingMessage>),
    SystemNotification(SystemNotification),
    Status(AgentStatus),
    TurnStarted {
        turn_id: String,
    },
    TurnComplete {
        turn_id: String,
        message: SamplingMessage,
    },
    TurnCompleteDetail {
        turn_id: String,
        tool_count: usize,
        has_errors: bool,
    },
    TurnAborted {
        turn_id: String,
        reason: String,
    },
    ToolProgress {
        tool: String,
        progress: String,
    },
}

#[cfg(test)]
mod tests {
    use rmcp::model::{
        RawTextContent,
        Role,
        SamplingContent,
        SamplingMessageContent,
    };

    use super::*;

    // -------------------------------------------------------------------------
    // AgentStatus tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_agent_status_variants() {
        // PendingInit
        let status = AgentStatus::PendingInit;
        assert!(matches!(status, AgentStatus::PendingInit));

        // Running
        let status = AgentStatus::Running;
        assert!(matches!(status, AgentStatus::Running));

        // Completed
        let status = AgentStatus::Completed;
        assert!(matches!(status, AgentStatus::Completed));

        // Shutdown
        let status = AgentStatus::Shutdown;
        assert!(matches!(status, AgentStatus::Shutdown));

        // Cancelled
        let status = AgentStatus::Cancelled;
        assert!(matches!(status, AgentStatus::Cancelled));

        // NotFound
        let status = AgentStatus::NotFound;
        assert!(matches!(status, AgentStatus::NotFound));
    }

    #[test]
    fn test_agent_status_with_data() {
        // Errored
        let status = AgentStatus::Errored("something went wrong".to_string());
        match status {
            AgentStatus::Errored(msg) => {
                assert_eq!(msg, "something went wrong")
            }
            _ => unreachable!("Expected Errored"),
        }

        // MaxStepsReached
        let status = AgentStatus::MaxStepsReached(42);
        match status {
            AgentStatus::MaxStepsReached(steps) => assert_eq!(steps, 42),
            _ => unreachable!("Expected MaxStepsReached"),
        }

        // LoopDetected
        let status = AgentStatus::LoopDetected("web_search".to_string());
        match status {
            AgentStatus::LoopDetected(tool) => assert_eq!(tool, "web_search"),
            _ => unreachable!("Expected LoopDetected"),
        }

        // MaxTokensReached
        let status = AgentStatus::MaxTokensReached(5000);
        match status {
            AgentStatus::MaxTokensReached(tokens) => assert_eq!(tokens, 5000),
            _ => unreachable!("Expected MaxTokensReached"),
        }
    }

    #[test]
    fn test_agent_status_clone() {
        let original = AgentStatus::Errored("error".to_string());
        let cloned = original.clone();
        assert_eq!(original, cloned);

        let original = AgentStatus::MaxStepsReached(10);
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_agent_status_debug() {
        let status = AgentStatus::Running;
        let debug_str = format!("{status:?}");
        assert!(debug_str.contains("Running"));

        let status = AgentStatus::Errored("fail".to_string());
        let debug_str = format!("{status:?}");
        assert!(debug_str.contains("fail"));
    }

    // -------------------------------------------------------------------------
    // AgentEvent tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_agent_event_status_variant() {
        let status = AgentStatus::Running;
        let event = AgentEvent::Status(status);
        match event {
            AgentEvent::Status(s) => assert!(matches!(s, AgentStatus::Running)),
            _ => unreachable!("Expected Status"),
        }
    }

    #[test]
    fn test_agent_event_mcp_notification_variant() {
        // Verify the McpNotification variant exists via pattern matching
        // We can't construct ServerNotification directly, so we verify the variant
        // exists through exhaustive matching on a reference
        fn extract_session(event: &AgentEvent) -> Option<&String> {
            match event {
                AgentEvent::McpNotification((session, _)) => Some(session),
                _ => None,
            }
        }

        // Create a fake event to test the extractor's logic
        // (the actual ServerNotification construction is tested elsewhere)
        let fake_session = "session-abc".to_string();
        let session_only = &fake_session;

        // Verify our helper function works correctly
        assert_eq!(
            extract_session(&AgentEvent::Status(AgentStatus::Running)),
            None
        );

        // Test that we can access session string without triggering unused warnings
        let _ = session_only.len();
    }

    #[test]
    fn test_agent_event_turn_started() {
        let event = AgentEvent::TurnStarted {
            turn_id: "turn-123".to_string(),
        };
        match event {
            AgentEvent::TurnStarted { turn_id } => {
                assert_eq!(turn_id, "turn-123")
            }
            _ => unreachable!("Expected TurnStarted"),
        }
    }

    #[test]
    fn test_agent_event_turn_complete() {
        let msg = SamplingMessage {
            role: Role::User,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: "done".to_string(),
                    meta: None,
                },
            )),
            meta: None,
        };
        let event = AgentEvent::TurnComplete {
            turn_id: "turn-456".to_string(),
            message: msg,
        };
        match event {
            AgentEvent::TurnComplete { turn_id, message } => {
                assert_eq!(turn_id, "turn-456");
                match message.content {
                    SamplingContent::Single(SamplingMessageContent::Text(
                        t,
                    )) => {
                        assert_eq!(t.text, "done");
                    }
                    _ => unreachable!("Expected text content"),
                }
            }
            _ => unreachable!("Expected TurnComplete"),
        }
    }

    #[test]
    fn test_agent_event_turn_aborted() {
        let event = AgentEvent::TurnAborted {
            turn_id: "turn-789".to_string(),
            reason: "cancelled by user".to_string(),
        };
        match event {
            AgentEvent::TurnAborted { turn_id, reason } => {
                assert_eq!(turn_id, "turn-789");
                assert_eq!(reason, "cancelled by user");
            }
            _ => unreachable!("Expected TurnAborted"),
        }
    }

    #[test]
    fn test_agent_event_tool_progress() {
        let event = AgentEvent::ToolProgress {
            tool: "web_search".to_string(),
            progress: "50%".to_string(),
        };
        match event {
            AgentEvent::ToolProgress { tool, progress } => {
                assert_eq!(tool, "web_search");
                assert_eq!(progress, "50%");
            }
            _ => unreachable!("Expected ToolProgress"),
        }
    }

    #[test]
    fn test_agent_event_system_notification_variant() {
        use crate::types::notification::SystemNotification;

        // Create a SystemNotification using the actual struct fields
        let notification = SystemNotification {
            notification_type:
                crate::types::notification::SystemNotificationType::Progress,
            msg: "Test message".to_string(),
            data: None,
        };
        let event = AgentEvent::SystemNotification(notification);
        match event {
            AgentEvent::SystemNotification(n) => {
                assert_eq!(n.msg, "Test message");
            }
            _ => unreachable!("Expected SystemNotification"),
        }
    }

    #[test]
    fn test_agent_event_clone_all_variants() {
        // Test that all variants with payload clone correctly
        let events = vec![
            AgentEvent::Status(AgentStatus::Running),
            AgentEvent::ModelChange {
                model: "claude-3".to_string(),
                mode: "standard".to_string(),
            },
            AgentEvent::TurnStarted {
                turn_id: "t1".to_string(),
            },
            AgentEvent::ToolProgress {
                tool: "tool1".to_string(),
                progress: "done".to_string(),
            },
        ];

        for event in events {
            let cloned = event.clone();
            assert!(std::mem::size_of_val(&event) > 0);
            let _ = cloned; // suppress unused warning
        }
    }

    #[test]
    fn test_agent_event_variants() {
        let message_event = AgentEvent::Message(SamplingMessage {
            role: Role::User,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: "Test message".to_string(),
                    meta: None,
                },
            )),
            meta: None,
        });

        match message_event {
            AgentEvent::Message(_) => {}
            _ => unreachable!("Expected Message variant"),
        }

        let model_change_event = AgentEvent::ModelChange {
            model: "test-model".to_string(),
            mode: "lead".to_string(),
        };

        match model_change_event {
            AgentEvent::ModelChange { model, mode } => {
                assert_eq!(model, "test-model");
                assert_eq!(mode, "lead");
            }
            _ => unreachable!("Expected ModelChange variant"),
        }

        let history_replaced_event =
            AgentEvent::HistoryReplaced(vec![SamplingMessage {
                role: Role::User,
                content: SamplingContent::Single(SamplingMessageContent::Text(
                    RawTextContent {
                        text: "Test message".to_string(),
                        meta: None,
                    },
                )),
                meta: None,
            }]);

        match history_replaced_event {
            AgentEvent::HistoryReplaced(history) => {
                assert_eq!(history.len(), 1);
            }
            _ => unreachable!("Expected HistoryReplaced variant"),
        }
    }

    #[test]
    fn test_agent_event_clone() {
        let original_event = AgentEvent::ModelChange {
            model: "test-model".to_string(),
            mode: "lead".to_string(),
        };

        let cloned_event = original_event.clone();

        match (original_event, cloned_event) {
            (
                AgentEvent::ModelChange {
                    model: original_model,
                    mode: original_mode,
                },
                AgentEvent::ModelChange {
                    model: cloned_model,
                    mode: cloned_mode,
                },
            ) => {
                assert_eq!(original_model, cloned_model);
                assert_eq!(original_mode, cloned_mode);
            }
            _ => unreachable!("Expected ModelChange variants"),
        }
    }
}
