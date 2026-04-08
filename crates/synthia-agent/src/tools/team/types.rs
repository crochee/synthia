use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use super::data::{MessageType, Teammate, TeammateStatus};

/// Configuration for spawning a new team member
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemberConfig {
    /// Name of the member
    pub name: String,
    /// Role of the member
    pub role: String,
    /// Initial prompt for the member
    pub prompt: String,
    /// Working directory for the member
    pub cwd: Option<String>,
    /// Model to use for the member
    pub model: Option<String>,
    /// Whether to use split pane for the member
    pub use_splitpane: Option<bool>,
    /// Whether plan mode is required
    pub plan_mode_required: Option<bool>,
    /// Type of agent
    pub agent_type: Option<String>,
    /// Team ID the member belongs to
    pub team_id: Option<String>,
    /// Member status
    pub status: TeammateStatus,
}

impl MemberConfig {
    /// Create a new MemberConfig from a SpawnTeammateRequest
    pub fn from_request(
        request: &SpawnTeammateRequest,
        team_id: Option<String>,
    ) -> Self {
        Self {
            name: request.name.clone(),
            role: request.role.clone(),
            prompt: request.prompt.clone(),
            cwd: request.cwd.clone(),
            model: request.model.clone(),
            use_splitpane: request.use_splitpane,
            plan_mode_required: request.plan_mode_required,
            agent_type: request.agent_type.clone(),
            team_id,
            status: TeammateStatus::Idle,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpawnTeammateRequest {
    pub name: String,
    pub role: String,
    pub prompt: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub use_splitpane: Option<bool>,
    #[serde(default)]
    pub plan_mode_required: Option<bool>,
    #[serde(default)]
    pub agent_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SendMessageRequest {
    pub to: String,
    pub content: String,
    #[serde(default)]
    pub msg_type: Option<MessageType>,
    /// Optional task ID for progress reporting and task-related messages
    #[serde(default)]
    pub task_id: Option<String>,
}

/// Progress report from a member agent to the lead
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProgressReport {
    /// ID of the task being reported on
    pub task_id: String,
    /// Current status (e.g., "in_progress", "completed", "blocked")
    pub status: String,
    /// Optional output or result from the task
    #[serde(default)]
    pub output: Option<String>,
    /// Confidence level of the progress (0.0 to 1.0)
    #[serde(default)]
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BroadcastRequest {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShutdownRequest {
    pub teammate: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShutdownResponseRequest {
    pub request_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlanApprovalRequest {
    pub request_id: String,
    pub approve: bool,
    #[serde(default)]
    pub feedback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TeamMessage {
    pub msg_type: MessageType,
    pub from: String,
    pub content: String,
    pub timestamp: f64,
    pub request_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_teammate_request_serialization() {
        let json = serde_json::json!({
            "name": "alice",
            "role": "developer",
            "prompt": "You are a developer"
        });

        let request: SpawnTeammateRequest =
            serde_json::from_value(json).unwrap();
        assert_eq!(request.name, "alice");
        assert_eq!(request.role, "developer");
        assert_eq!(request.prompt, "You are a developer");
        assert!(request.cwd.is_none());
        assert!(request.model.is_none());
    }

    #[test]
    fn test_spawn_teammate_request_with_optionals() {
        let json = serde_json::json!({
            "name": "bob",
            "role": "tester",
            "prompt": "You test things",
            "cwd": "/home/user",
            "model": "claude-3",
            "use_splitpane": true,
            "plan_mode_required": false,
            "agent_type": "researcher"
        });

        let request: SpawnTeammateRequest =
            serde_json::from_value(json).unwrap();
        assert_eq!(request.cwd, Some("/home/user".to_string()));
        assert_eq!(request.model, Some("claude-3".to_string()));
        assert_eq!(request.use_splitpane, Some(true));
        assert_eq!(request.plan_mode_required, Some(false));
        assert_eq!(request.agent_type, Some("researcher".to_string()));
    }

    #[test]
    fn test_send_message_request_serialization() {
        let json = serde_json::json!({
            "to": "alice",
            "content": "Hello there"
        });

        let request: SendMessageRequest = serde_json::from_value(json).unwrap();
        assert_eq!(request.to, "alice");
        assert_eq!(request.content, "Hello there");
        assert!(request.msg_type.is_none());
    }

    #[test]
    fn test_send_message_request_with_type() {
        let json = serde_json::json!({
            "to": "bob",
            "content": "Broadcast",
            "msg_type": "Broadcast"
        });

        let request: SendMessageRequest = serde_json::from_value(json).unwrap();
        assert_eq!(request.msg_type, Some(MessageType::Broadcast));
    }

    #[test]
    fn test_broadcast_request_serialization() {
        let json = serde_json::json!({
            "content": "Hello everyone"
        });

        let request: BroadcastRequest = serde_json::from_value(json).unwrap();
        assert_eq!(request.content, "Hello everyone");
    }

    #[test]
    fn test_shutdown_request_serialization() {
        let json = serde_json::json!({
            "teammate": "alice"
        });

        let request: ShutdownRequest = serde_json::from_value(json).unwrap();
        assert_eq!(request.teammate, "alice");
    }

    #[test]
    fn test_shutdown_response_request_serialization() {
        let json = serde_json::json!({
            "request_id": "req-123"
        });

        let request: ShutdownResponseRequest =
            serde_json::from_value(json).unwrap();
        assert_eq!(request.request_id, "req-123");
    }

    #[test]
    fn test_plan_approval_request_serialization() {
        let json = serde_json::json!({
            "request_id": "plan-1",
            "approve": true,
            "feedback": "Looks good"
        });

        let request: PlanApprovalRequest =
            serde_json::from_value(json).unwrap();
        assert_eq!(request.request_id, "plan-1");
        assert!(request.approve);
        assert_eq!(request.feedback, Some("Looks good".to_string()));
    }

    #[test]
    fn test_plan_approval_request_without_feedback() {
        let json = serde_json::json!({
            "request_id": "plan-2",
            "approve": false
        });

        let request: PlanApprovalRequest =
            serde_json::from_value(json).unwrap();
        assert!(!request.approve);
        assert!(request.feedback.is_none());
    }

    #[test]
    fn test_team_message_serialization() {
        let json = serde_json::json!({
            "msg_type": "Message",
            "from": "alice",
            "content": "Hello",
            "timestamp": 1234567890.0
        });

        let msg: TeamMessage = serde_json::from_value(json).unwrap();
        assert_eq!(msg.msg_type, MessageType::Message);
        assert_eq!(msg.from, "alice");
        assert_eq!(msg.content, "Hello");
        assert!(msg.request_id.is_none());
    }

    #[test]
    fn test_team_message_with_request_id() {
        let json = serde_json::json!({
            "msg_type": "Broadcast",
            "from": "bob",
            "content": "Hi",
            "timestamp": 1234567890.0,
            "request_id": "req-456"
        });

        let msg: TeamMessage = serde_json::from_value(json).unwrap();
        assert_eq!(msg.request_id, Some("req-456".to_string()));
    }

    #[test]
    fn test_member_config_from_request() {
        let request = SpawnTeammateRequest {
            name: "alice".to_string(),
            role: "developer".to_string(),
            prompt: "You are a developer".to_string(),
            cwd: Some("/home/user".to_string()),
            model: Some("claude-3".to_string()),
            use_splitpane: Some(true),
            plan_mode_required: Some(false),
            agent_type: Some("researcher".to_string()),
        };

        let config =
            MemberConfig::from_request(&request, Some("team-1".to_string()));
        assert_eq!(config.name, "alice");
        assert_eq!(config.role, "developer");
        assert_eq!(config.prompt, "You are a developer");
        assert_eq!(config.cwd, Some("/home/user".to_string()));
        assert_eq!(config.model, Some("claude-3".to_string()));
        assert_eq!(config.use_splitpane, Some(true));
        assert_eq!(config.plan_mode_required, Some(false));
        assert_eq!(config.agent_type, Some("researcher".to_string()));
        assert_eq!(config.team_id, Some("team-1".to_string()));
        assert_eq!(config.status, TeammateStatus::Idle);
    }
}
