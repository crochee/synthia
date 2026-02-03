//! Agent event handler trait
//!
//! This module defines the trait for handling agent events.
//! It provides a unified interface for event handling that supports
//! both function pointers and closures.
//!
//! # Example
//!
//! ```rust,ignore
//! use synthia_agent::event_handler::AgentEventHandler;
//! use synthia_agent::types::AgentEvent;
//!
//! // Using a closure
//! let handler = |agent_name: &str, event: &AgentEvent| {
//!     println!("Agent {}: {:?}", agent_name, event);
//! };
//!
//! // Using the trait directly
//! async fn handle_event(handler: &dyn AgentEventHandler, name: &str, event: &AgentEvent) {
//!     handler.on_event(name, event).await;
//! }
//! ```

// Standard library
use std::sync::Arc;

// Third-party crates
use async_trait::async_trait;

// Local imports
use crate::types::AgentEvent;

/// Trait for handling agent events.
///
/// This trait defines the interface for components that need to respond
/// to agent events such as message generation, tool execution, and loop completion.
#[async_trait]
pub trait AgentEventHandler: Send + Sync {
    /// Handles an agent event.
    ///
    /// # Arguments
    ///
    /// * `agent_name` - The name of the agent that generated the event
    /// * `event` - The event to handle
    async fn on_event(&self, agent_name: &str, event: &AgentEvent);
}

/// Implementation of [`AgentEventHandler`] for function pointers.
///
/// This allows using simple functions as event handlers.
#[async_trait]
impl<F> AgentEventHandler for F
where
    F: Fn(&str, &AgentEvent) + Send + Sync,
{
    async fn on_event(&self, agent_name: &str, event: &AgentEvent) {
        self(agent_name, event)
    }
}

/// Implementation of [`AgentEventHandler`] for Arc-wrapped function pointers.
///
/// This allows sharing event handlers across multiple agents.
#[async_trait]
impl AgentEventHandler for Arc<dyn Fn(&str, &AgentEvent) + Send + Sync> {
    async fn on_event(&self, agent_name: &str, event: &AgentEvent) {
        self(agent_name, event)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rmcp::model::{
        RawTextContent,
        Role,
        SamplingContent,
        SamplingMessageContent,
    };

    use super::*;
    use crate::types::{
        AgentEvent,
        SystemNotification,
        SystemNotificationType,
    };

    #[derive(Debug, Default)]
    struct TestEventHandler {
        events: std::sync::Mutex<Vec<(String, AgentEvent)>>,
    }

    impl TestEventHandler {
        fn new() -> Self {
            Self {
                events: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn get_events(&self) -> Vec<(String, AgentEvent)> {
            self.events.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl AgentEventHandler for TestEventHandler {
        async fn on_event(&self, agent_name: &str, event: &AgentEvent) {
            self.events
                .lock()
                .unwrap()
                .push((agent_name.to_string(), event.clone()));
        }
    }

    fn make_test_message() -> rmcp::model::SamplingMessage {
        rmcp::model::SamplingMessage {
            role: Role::User,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: "test".to_string(),
                    meta: None,
                },
            )),
            meta: None,
        }
    }

    #[tokio::test]
    async fn test_agent_event_handler_trait() {
        let handler = TestEventHandler::new();
        let event = AgentEvent::Status(crate::types::AgentStatus::Running);

        handler.on_event("test-agent", &event).await;

        let events = handler.get_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "test-agent");
    }

    #[tokio::test]
    async fn test_agent_event_handler_function_pointer() {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);

        let handler = move |agent_name: &str, event: &AgentEvent| {
            events_clone
                .lock()
                .unwrap()
                .push((agent_name.to_string(), event.clone()));
        };

        let event = AgentEvent::Status(crate::types::AgentStatus::Completed);
        handler.on_event("func-agent", &event).await;

        let captured = events.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].0, "func-agent");
    }

    #[tokio::test]
    async fn test_agent_event_handler_arc_function_pointer() {
        type EventHandler = Arc<dyn Fn(&str, &AgentEvent) + Send + Sync>;
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);

        let handler: EventHandler =
            Arc::new(move |agent_name: &str, event: &AgentEvent| {
                events_clone
                    .lock()
                    .unwrap()
                    .push((agent_name.to_string(), event.clone()));
            });

        let event = AgentEvent::Status(crate::types::AgentStatus::Running);
        handler.on_event("arc-agent", &event).await;

        let captured = events.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].0, "arc-agent");
    }

    #[tokio::test]
    async fn test_agent_event_handler_multiple_events() {
        let handler = TestEventHandler::new();

        let msg = make_test_message();
        handler
            .on_event("agent1", &AgentEvent::Message(msg.clone()))
            .await;
        handler
            .on_event(
                "agent2",
                &AgentEvent::Status(crate::types::AgentStatus::Running),
            )
            .await;

        let events = handler.get_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, "agent1");
        assert_eq!(events[1].0, "agent2");
    }

    #[tokio::test]
    async fn test_agent_event_handler_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TestEventHandler>();
        assert_send_sync::<Arc<dyn Fn(&str, &AgentEvent) + Send + Sync>>();
    }

    #[tokio::test]
    async fn test_agent_event_handler_model_change() {
        let handler = TestEventHandler::new();

        handler
            .on_event(
                "agent",
                &AgentEvent::ModelChange {
                    model: "claude-3".to_string(),
                    mode: "chat".to_string(),
                },
            )
            .await;

        let events = handler.get_events();
        assert_eq!(events.len(), 1);
        if let AgentEvent::ModelChange { model, mode } = &events[0].1 {
            assert_eq!(model, "claude-3");
            assert_eq!(mode, "chat");
        } else {
            panic!("Expected ModelChange event");
        }
    }

    #[tokio::test]
    async fn test_agent_event_handler_turn_started() {
        let handler = TestEventHandler::new();

        handler
            .on_event(
                "agent",
                &AgentEvent::TurnStarted {
                    turn_id: "turn-1".to_string(),
                },
            )
            .await;

        let events = handler.get_events();
        assert_eq!(events.len(), 1);
        if let AgentEvent::TurnStarted { turn_id } = &events[0].1 {
            assert_eq!(turn_id, "turn-1");
        } else {
            panic!("Expected TurnStarted event");
        }
    }

    #[tokio::test]
    async fn test_agent_event_handler_system_notification() {
        let handler = TestEventHandler::new();

        let notification = SystemNotification {
            notification_type: SystemNotificationType::InlineMessage,
            msg: "context low".to_string(),
            data: None,
        };
        handler
            .on_event("agent", &AgentEvent::SystemNotification(notification))
            .await;

        let events = handler.get_events();
        assert_eq!(events.len(), 1);
        if let AgentEvent::SystemNotification(n) = &events[0].1 {
            assert_eq!(n.msg, "context low");
        } else {
            panic!("Expected SystemNotification event");
        }
    }

    #[tokio::test]
    async fn test_agent_event_handler_turn_complete() {
        let handler = TestEventHandler::new();

        let msg = make_test_message();
        handler
            .on_event(
                "agent",
                &AgentEvent::TurnComplete {
                    turn_id: "turn-2".to_string(),
                    message: msg.clone(),
                },
            )
            .await;

        let events = handler.get_events();
        assert_eq!(events.len(), 1);
        if let AgentEvent::TurnComplete {
            turn_id,
            message: _,
        } = &events[0].1
        {
            assert_eq!(turn_id, "turn-2");
        } else {
            panic!("Expected TurnComplete event");
        }
    }

    #[tokio::test]
    async fn test_agent_event_handler_turn_aborted() {
        let handler = TestEventHandler::new();

        handler
            .on_event(
                "agent",
                &AgentEvent::TurnAborted {
                    turn_id: "turn-3".to_string(),
                    reason: "timeout".to_string(),
                },
            )
            .await;

        let events = handler.get_events();
        assert_eq!(events.len(), 1);
        if let AgentEvent::TurnAborted { turn_id, reason } = &events[0].1 {
            assert_eq!(turn_id, "turn-3");
            assert_eq!(reason, "timeout");
        } else {
            panic!("Expected TurnAborted event");
        }
    }

    #[tokio::test]
    async fn test_agent_event_handler_tool_progress() {
        let handler = TestEventHandler::new();

        handler
            .on_event(
                "agent",
                &AgentEvent::ToolProgress {
                    tool: "grep".to_string(),
                    progress: "50%".to_string(),
                },
            )
            .await;

        let events = handler.get_events();
        assert_eq!(events.len(), 1);
        if let AgentEvent::ToolProgress { tool, progress } = &events[0].1 {
            assert_eq!(tool, "grep");
            assert_eq!(progress, "50%");
        } else {
            panic!("Expected ToolProgress event");
        }
    }

    // AgentStatus tests

    #[test]
    fn test_agent_status_errored() {
        let status =
            crate::types::AgentStatus::Errored("connection failed".to_string());
        assert!(
            matches!(status, crate::types::AgentStatus::Errored(msg) if msg == "connection failed")
        );
    }

    #[test]
    fn test_agent_status_shutdown() {
        let status = crate::types::AgentStatus::Shutdown;
        assert!(matches!(status, crate::types::AgentStatus::Shutdown));
    }

    #[test]
    fn test_agent_status_cancelled() {
        let status = crate::types::AgentStatus::Cancelled;
        assert!(matches!(status, crate::types::AgentStatus::Cancelled));
    }

    #[test]
    fn test_agent_status_max_steps_reached() {
        let status = crate::types::AgentStatus::MaxStepsReached(10);
        assert!(
            matches!(status, crate::types::AgentStatus::MaxStepsReached(n) if n == 10)
        );
    }

    #[test]
    fn test_agent_status_not_found() {
        let status = crate::types::AgentStatus::NotFound;
        assert!(matches!(status, crate::types::AgentStatus::NotFound));
    }

    #[test]
    fn test_agent_status_loop_detected() {
        let status = crate::types::AgentStatus::LoopDetected(
            "infinite loop".to_string(),
        );
        assert!(
            matches!(status, crate::types::AgentStatus::LoopDetected(msg) if msg == "infinite loop")
        );
    }

    #[test]
    fn test_agent_status_max_tokens_reached() {
        let status = crate::types::AgentStatus::MaxTokensReached(100000);
        assert!(
            matches!(status, crate::types::AgentStatus::MaxTokensReached(n) if n == 100000)
        );
    }

    #[test]
    fn test_agent_status_pending_init() {
        let status = crate::types::AgentStatus::PendingInit;
        assert!(matches!(status, crate::types::AgentStatus::PendingInit));
    }

    #[test]
    fn test_agent_status_running() {
        let status = crate::types::AgentStatus::Running;
        assert!(matches!(status, crate::types::AgentStatus::Running));
    }

    #[test]
    fn test_agent_status_completed() {
        let status = crate::types::AgentStatus::Completed;
        assert!(matches!(status, crate::types::AgentStatus::Completed));
    }

    #[test]
    fn test_agent_status_clone() {
        let status = crate::types::AgentStatus::Errored("test".to_string());
        let cloned = status.clone();
        assert_eq!(status, cloned);
    }

    #[test]
    fn test_agent_status_debug() {
        let status = crate::types::AgentStatus::MaxStepsReached(5);
        let debug_str = format!("{status:?}");
        assert!(debug_str.contains("MaxStepsReached"));
    }

    // AgentEvent tests for remaining variants

    #[test]
    fn test_agent_event_mcp_notification() {
        // ServerNotification is an enum, so we match on structure only
        let event =
            AgentEvent::McpNotification(("session-1".to_string(), unsafe {
                std::mem::zeroed()
            }));
        if let AgentEvent::McpNotification((session_id, _)) = event {
            assert_eq!(session_id, "session-1");
        } else {
            panic!("Expected McpNotification variant");
        }
    }

    #[test]
    fn test_agent_event_turn_aborted() {
        let event = AgentEvent::TurnAborted {
            turn_id: "t1".to_string(),
            reason: "cancelled".to_string(),
        };
        if let AgentEvent::TurnAborted { turn_id, reason } = event {
            assert_eq!(turn_id, "t1");
            assert_eq!(reason, "cancelled");
        } else {
            panic!("Expected TurnAborted variant");
        }
    }

    #[test]
    fn test_agent_event_tool_progress() {
        let event = AgentEvent::ToolProgress {
            tool: "read".to_string(),
            progress: "75%".to_string(),
        };
        if let AgentEvent::ToolProgress { tool, progress } = event {
            assert_eq!(tool, "read");
            assert_eq!(progress, "75%");
        } else {
            panic!("Expected ToolProgress variant");
        }
    }

    #[test]
    fn test_agent_event_clone() {
        let event = AgentEvent::Status(crate::types::AgentStatus::Running);
        let cloned = event.clone();
        match (event, cloned) {
            (AgentEvent::Status(a), AgentEvent::Status(b)) => assert_eq!(a, b),
            _ => panic!("Expected Status variant"),
        }
    }

    #[test]
    fn test_agent_event_debug() {
        let event = AgentEvent::Status(crate::types::AgentStatus::Completed);
        let debug_str = format!("{event:?}");
        assert!(debug_str.contains("Status"));
    }
}
