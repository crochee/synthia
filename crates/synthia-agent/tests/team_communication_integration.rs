//! Integration tests for Lead and Member message communication
//!
//! Tests the complete flow of bidirectional communication between Team Lead and Members.

use synthia_agent::tools::team::{MessageType, TeamMessage};

mod message_type_tests {
    use super::*;

    #[test]
    fn test_all_message_types() {
        // Verify all message types are available
        assert_eq!(MessageType::Message.as_str(), "message");
        assert_eq!(MessageType::Broadcast.as_str(), "broadcast");
        assert_eq!(MessageType::ShutdownRequest.as_str(), "shutdown_request");
        assert_eq!(MessageType::ShutdownResponse.as_str(), "shutdown_response");
        assert_eq!(
            MessageType::PlanApprovalResponse.as_str(),
            "plan_approval_response"
        );
        assert_eq!(MessageType::TaskAssigned.as_str(), "task_assigned");
        assert_eq!(MessageType::TaskCompleted.as_str(), "task_completed");
        assert_eq!(MessageType::TaskBlocked.as_str(), "task_blocked");
        assert_eq!(MessageType::StatusUpdate.as_str(), "status_update");
        assert_eq!(
            MessageType::CoordinationRequest.as_str(),
            "coordination_request"
        );
        assert_eq!(
            MessageType::CoordinationResponse.as_str(),
            "coordination_response"
        );
        assert_eq!(MessageType::TaskFailed.as_str(), "task_failed");
    }

    #[test]
    fn test_message_type_from_db_string() {
        assert_eq!(
            MessageType::from_db_string("task_assigned"),
            Some(MessageType::TaskAssigned)
        );
        assert_eq!(
            MessageType::from_db_string("task_completed"),
            Some(MessageType::TaskCompleted)
        );
        assert_eq!(MessageType::from_db_string("invalid"), None);
    }
}

mod team_message_tests {
    use super::*;

    #[test]
    fn test_team_message_creation() {
        let message = TeamMessage::new(
            "alice",
            MessageType::TaskAssigned,
            "lead",
            "Please implement feature X",
        );

        assert_eq!(message.recipient, "alice");
        assert_eq!(message.msg_type, MessageType::TaskAssigned);
        assert_eq!(message.sender, "lead");
        assert_eq!(message.content, "Please implement feature X");
        assert!(!message.read);
    }

    #[test]
    fn test_team_message_with_request_id() {
        let message = TeamMessage::new(
            "alice",
            MessageType::TaskAssigned,
            "lead",
            "Task",
        )
        .with_request_id("req-123");

        assert_eq!(message.request_id, Some("req-123".to_string()));
    }

    #[test]
    fn test_team_message_serialization() {
        let message = TeamMessage::new(
            "alice",
            MessageType::TaskCompleted,
            "bob",
            "Task done",
        );

        // Serialize and deserialize
        let json = serde_json::to_string(&message).unwrap();
        let deserialized: TeamMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.recipient, "alice");
        assert_eq!(deserialized.msg_type, MessageType::TaskCompleted);
        assert_eq!(deserialized.sender, "bob");
        assert_eq!(deserialized.content, "Task done");
    }
}

mod communication_flow_tests {
    use super::*;

    #[test]
    fn test_lead_to_member_message_flow() {
        // Lead creates TaskAssigned message
        let task_assigned = TeamMessage::new(
            "alice",
            MessageType::TaskAssigned,
            "lead",
            "Implement authentication module",
        );

        // Verify message structure
        assert_eq!(task_assigned.recipient, "alice");
        assert_eq!(task_assigned.msg_type, MessageType::TaskAssigned);
        assert_eq!(task_assigned.sender, "lead");
    }

    #[test]
    fn test_member_to_lead_message_flow() {
        // Member creates TaskCompleted message
        let task_completed = TeamMessage::new(
            "lead",
            MessageType::TaskCompleted,
            "alice",
            "Authentication module completed",
        );

        // Verify message structure
        assert_eq!(task_completed.recipient, "lead");
        assert_eq!(task_completed.msg_type, MessageType::TaskCompleted);
        assert_eq!(task_completed.sender, "alice");
    }

    #[test]
    fn test_blocked_task_notification() {
        // Member reports blocked task
        let blocked_msg = TeamMessage::new(
            "lead",
            MessageType::TaskBlocked,
            "alice",
            "Blocked on dependency: task-42 not complete",
        );

        assert_eq!(blocked_msg.msg_type, MessageType::TaskBlocked);
        assert!(blocked_msg.content.contains("Blocked"));
    }

    #[test]
    fn test_status_update_request() {
        // Lead requests status update
        let status_request = TeamMessage::new(
            "alice",
            MessageType::StatusUpdate,
            "lead",
            "Please provide status update",
        );

        assert_eq!(status_request.msg_type, MessageType::StatusUpdate);
    }

    #[test]
    fn test_coordination_between_members() {
        // Alice sends coordination request to Bob
        let coord_request = TeamMessage::new(
            "bob",
            MessageType::CoordinationRequest,
            "alice",
            "Need review on PR #42",
        );

        assert_eq!(coord_request.msg_type, MessageType::CoordinationRequest);

        // Bob sends response
        let coord_response = TeamMessage::new(
            "alice",
            MessageType::CoordinationResponse,
            "bob",
            "Will review PR #42 this afternoon",
        );

        assert_eq!(coord_response.msg_type, MessageType::CoordinationResponse);
    }
}
