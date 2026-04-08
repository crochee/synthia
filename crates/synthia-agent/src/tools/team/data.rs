use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum TeammateStatus {
    #[default]
    Working,
    Idle,
    Shutdown,
}

impl TeammateStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TeammateStatus::Working => "working",
            TeammateStatus::Idle => "idle",
            TeammateStatus::Shutdown => "shutdown",
        }
    }

    pub fn from_db_string(s: &str) -> Self {
        match s {
            "idle" => TeammateStatus::Idle,
            "shutdown" => TeammateStatus::Shutdown,
            _ => TeammateStatus::Working,
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, TeammateStatus::Idle | TeammateStatus::Working)
    }
}

impl std::fmt::Display for TeammateStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Teammate {
    pub name: String,
    pub role: String,
    pub status: TeammateStatus,
    pub team_id: Option<String>,
    pub current_task: Option<String>,
    pub capabilities: Vec<String>,
}

impl Teammate {
    pub fn new(name: impl Into<String>, role: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            role: role.into(),
            status: TeammateStatus::default(),
            team_id: None,
            current_task: None,
            capabilities: Vec::new(),
        }
    }

    pub fn with_team(mut self, team_id: impl Into<String>) -> Self {
        self.team_id = Some(team_id.into());
        self
    }

    pub fn with_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn assign_task(&mut self, task_id: impl Into<String>) {
        self.current_task = Some(task_id.into());
        self.status = TeammateStatus::Working;
    }

    pub fn clear_task(&mut self) {
        self.current_task = None;
        self.status = TeammateStatus::Idle;
    }

    pub fn is_available(&self) -> bool {
        self.status.is_available() && self.current_task.is_none()
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum TeamStatus {
    #[default]
    Created,
    Running,
    Completed,
    Deleted,
}

impl TeamStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TeamStatus::Created => "created",
            TeamStatus::Running => "running",
            TeamStatus::Completed => "completed",
            TeamStatus::Deleted => "deleted",
        }
    }

    pub fn from_db_string(s: &str) -> Self {
        match s {
            "running" => TeamStatus::Running,
            "completed" => TeamStatus::Completed,
            "deleted" => TeamStatus::Deleted,
            _ => TeamStatus::Created,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, TeamStatus::Created | TeamStatus::Running)
    }
}

impl std::fmt::Display for TeamStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Team {
    pub team_id: String,
    pub name: String,
    pub task_ids: Vec<String>,
    pub status: TeamStatus,
    pub created_at: u64,
    pub updated_at: u64,
    pub lead: Option<String>,
}

impl Team {
    pub fn new(name: impl Into<String>) -> Self {
        let now = chrono::Utc::now().timestamp() as u64;
        Self {
            team_id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            task_ids: Vec::new(),
            status: TeamStatus::default(),
            created_at: now,
            updated_at: now,
            lead: None,
        }
    }

    pub fn with_lead(mut self, lead: impl Into<String>) -> Self {
        self.lead = Some(lead.into());
        self
    }

    pub fn add_task(&mut self, task_id: impl Into<String>) {
        let task_id = task_id.into();
        if !self.task_ids.contains(&task_id) {
            self.task_ids.push(task_id);
            self.touch();
        }
    }

    pub fn remove_task(&mut self, task_id: &str) -> bool {
        let len_before = self.task_ids.len();
        self.task_ids.retain(|id| id != task_id);
        if self.task_ids.len() != len_before {
            self.touch();
            true
        } else {
            false
        }
    }

    pub fn start(&mut self) {
        self.status = TeamStatus::Running;
        self.touch();
    }

    pub fn complete(&mut self) {
        self.status = TeamStatus::Completed;
        self.touch();
    }

    pub fn delete(&mut self) {
        self.status = TeamStatus::Deleted;
        self.touch();
    }

    fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp() as u64;
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct TeamPatch {
    pub name: Option<String>,
    pub status: Option<TeamStatus>,
    pub lead: Option<String>,
    pub task_ids: Option<Vec<String>>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema,
)]
pub enum MessageType {
    Message,
    Broadcast,
    ShutdownRequest,
    ShutdownResponse,
    PlanApprovalResponse,
    TaskAssigned,
    TaskCompleted,
    TaskBlocked,
    StatusUpdate,
    CoordinationRequest,
    CoordinationResponse,
    TaskFailed,
    /// Member reports progress on a task (includes task_id, status, output, confidence)
    ProgressReport,
    /// Member submits a plan for Lead approval
    PlanSubmitted,
}

impl MessageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageType::Message => "message",
            MessageType::Broadcast => "broadcast",
            MessageType::ShutdownRequest => "shutdown_request",
            MessageType::ShutdownResponse => "shutdown_response",
            MessageType::PlanApprovalResponse => "plan_approval_response",
            MessageType::TaskAssigned => "task_assigned",
            MessageType::TaskCompleted => "task_completed",
            MessageType::TaskBlocked => "task_blocked",
            MessageType::StatusUpdate => "status_update",
            MessageType::CoordinationRequest => "coordination_request",
            MessageType::CoordinationResponse => "coordination_response",
            MessageType::TaskFailed => "task_failed",
            MessageType::ProgressReport => "progress_report",
            MessageType::PlanSubmitted => "plan_submitted",
        }
    }

    pub fn from_db_string(s: &str) -> Option<Self> {
        match s {
            "message" => Some(MessageType::Message),
            "broadcast" => Some(MessageType::Broadcast),
            "shutdown_request" => Some(MessageType::ShutdownRequest),
            "shutdown_response" => Some(MessageType::ShutdownResponse),
            "plan_approval_response" => Some(MessageType::PlanApprovalResponse),
            "task_assigned" => Some(MessageType::TaskAssigned),
            "task_completed" => Some(MessageType::TaskCompleted),
            "task_blocked" => Some(MessageType::TaskBlocked),
            "status_update" => Some(MessageType::StatusUpdate),
            "coordination_request" => Some(MessageType::CoordinationRequest),
            "coordination_response" => Some(MessageType::CoordinationResponse),
            "task_failed" => Some(MessageType::TaskFailed),
            "progress_report" => Some(MessageType::ProgressReport),
            "plan_submitted" => Some(MessageType::PlanSubmitted),
            _ => None,
        }
    }
}

/// Message priority levels for queue ordering
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum MessagePriority {
    /// High priority - urgent messages like task failures
    High,
    /// Normal priority - default for most messages
    #[default]
    Normal,
    /// Low priority - informational messages
    Low,
}

impl MessagePriority {
    pub fn as_str(self) -> &'static str {
        match self {
            MessagePriority::High => "high",
            MessagePriority::Normal => "normal",
            MessagePriority::Low => "low",
        }
    }

    pub fn from_db_string(s: &str) -> Self {
        match s {
            "high" => MessagePriority::High,
            "low" => MessagePriority::Low,
            _ => MessagePriority::Normal,
        }
    }
}

impl std::fmt::Display for MessagePriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMessage {
    pub id: i64,
    pub recipient: String,
    pub msg_type: MessageType,
    pub sender: String,
    pub content: String,
    pub timestamp: f64,
    pub request_id: Option<String>,
    pub read: bool,
    pub priority: MessagePriority,
}

impl TeamMessage {
    pub fn new(
        recipient: impl Into<String>,
        msg_type: MessageType,
        sender: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: 0,
            recipient: recipient.into(),
            msg_type,
            sender: sender.into(),
            content: content.into(),
            timestamp: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
            request_id: None,
            read: false,
            priority: MessagePriority::default(),
        }
    }

    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    pub fn with_priority(mut self, priority: MessagePriority) -> Self {
        self.priority = priority;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownRequest {
    pub request_id: String,
    pub target: String,
    pub status: String,
    pub created_at: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanRequest {
    pub request_id: String,
    pub sender: String,
    pub plan: String,
    pub status: String,
    pub created_at: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub agent_id: String,
    pub name: String,
    pub agent_type: Option<String>,
    pub status: &'static str,
    pub current_tasks: Vec<String>,
}

impl AgentStatus {
    pub fn new(
        agent_id: String,
        name: String,
        agent_type: Option<String>,
        current_tasks: Vec<String>,
    ) -> Self {
        let status = if current_tasks.is_empty() {
            "idle"
        } else {
            "busy"
        };
        Self {
            agent_id,
            name,
            agent_type,
            status,
            current_tasks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_teammate_new() {
        let teammate = Teammate::new("alice", "developer");
        assert_eq!(teammate.name, "alice");
        assert_eq!(teammate.role, "developer");
        assert_eq!(teammate.status, TeammateStatus::Working);
        assert!(teammate.team_id.is_none());
        assert!(teammate.current_task.is_none());
        assert!(teammate.capabilities.is_empty());
    }

    #[test]
    fn test_teammate_with_team() {
        let teammate = Teammate::new("bob", "tester").with_team("team-1");
        assert_eq!(teammate.team_id, Some("team-1".to_string()));
    }

    #[test]
    fn test_teammate_with_capabilities() {
        let caps = vec!["rust".to_string(), "python".to_string()];
        let teammate =
            Teammate::new("carol", "devops").with_capabilities(caps.clone());
        assert_eq!(teammate.capabilities, caps);
    }

    #[test]
    fn test_teammate_assign_task() {
        let mut teammate = Teammate::new("dave", "developer");
        assert!(teammate.current_task.is_none());
        assert_eq!(teammate.status, TeammateStatus::Working);

        teammate.assign_task("task-42");
        assert_eq!(teammate.current_task, Some("task-42".to_string()));
        assert_eq!(teammate.status, TeammateStatus::Working);
    }

    #[test]
    fn test_teammate_clear_task() {
        let mut teammate = Teammate::new("eve", "developer");
        teammate.assign_task("task-1");
        assert!(teammate.current_task.is_some());

        teammate.clear_task();
        assert!(teammate.current_task.is_none());
        assert_eq!(teammate.status, TeammateStatus::Idle);
    }

    #[test]
    fn test_teammate_is_available() {
        let mut teammate = Teammate::new("frank", "developer");
        assert!(teammate.is_available());

        teammate.assign_task("task-1");
        assert!(!teammate.is_available());

        teammate.clear_task();
        assert!(teammate.is_available());

        teammate.status = TeammateStatus::Shutdown;
        assert!(!teammate.is_available());
    }

    #[test]
    fn test_team_new() {
        let team = Team::new("Alpha");
        assert_eq!(team.name, "Alpha");
        assert!(!team.team_id.is_empty());
        assert!(team.task_ids.is_empty());
        assert_eq!(team.status, TeamStatus::Created);
        assert!(team.lead.is_none());
        assert!(team.created_at > 0);
        assert!(team.updated_at > 0);
    }

    #[test]
    fn test_team_with_lead() {
        let team = Team::new("Beta").with_lead("alice");
        assert_eq!(team.lead, Some("alice".to_string()));
    }

    #[test]
    fn test_team_add_task() {
        let mut team = Team::new("Gamma");
        assert!(team.task_ids.is_empty());

        team.add_task("task-1");
        assert_eq!(team.task_ids.len(), 1);

        team.add_task("task-2");
        assert_eq!(team.task_ids.len(), 2);

        team.add_task("task-1");
        assert_eq!(team.task_ids.len(), 2);
    }

    #[test]
    fn test_team_remove_task() {
        let mut team = Team::new("Delta");
        team.add_task("task-1");
        team.add_task("task-2");

        let removed = team.remove_task("task-1");
        assert!(removed);
        assert_eq!(team.task_ids.len(), 1);
        assert!(!team.task_ids.contains(&"task-1".to_string()));

        let not_removed = team.remove_task("nonexistent");
        assert!(!not_removed);
    }

    #[test]
    fn test_team_lifecycle() {
        let mut team = Team::new("Epsilon");
        assert_eq!(team.status, TeamStatus::Created);

        team.start();
        assert_eq!(team.status, TeamStatus::Running);

        team.complete();
        assert_eq!(team.status, TeamStatus::Completed);

        team.delete();
        assert_eq!(team.status, TeamStatus::Deleted);
    }

    #[test]
    fn test_team_status_as_str() {
        assert_eq!(TeamStatus::Created.as_str(), "created");
        assert_eq!(TeamStatus::Running.as_str(), "running");
        assert_eq!(TeamStatus::Completed.as_str(), "completed");
        assert_eq!(TeamStatus::Deleted.as_str(), "deleted");
    }

    #[test]
    fn test_team_status_from_db_string() {
        assert_eq!(TeamStatus::from_db_string("running"), TeamStatus::Running);
        assert_eq!(
            TeamStatus::from_db_string("completed"),
            TeamStatus::Completed
        );
        assert_eq!(TeamStatus::from_db_string("deleted"), TeamStatus::Deleted);
        assert_eq!(TeamStatus::from_db_string("unknown"), TeamStatus::Created);
    }

    #[test]
    fn test_team_status_is_active() {
        assert!(TeamStatus::Created.is_active());
        assert!(TeamStatus::Running.is_active());
        assert!(!TeamStatus::Completed.is_active());
        assert!(!TeamStatus::Deleted.is_active());
    }

    #[test]
    fn test_teammate_status_as_str() {
        assert_eq!(TeammateStatus::Working.as_str(), "working");
        assert_eq!(TeammateStatus::Idle.as_str(), "idle");
        assert_eq!(TeammateStatus::Shutdown.as_str(), "shutdown");
    }

    #[test]
    fn test_teammate_status_from_db_string() {
        assert_eq!(
            TeammateStatus::from_db_string("idle"),
            TeammateStatus::Idle
        );
        assert_eq!(
            TeammateStatus::from_db_string("shutdown"),
            TeammateStatus::Shutdown
        );
        assert_eq!(
            TeammateStatus::from_db_string("unknown"),
            TeammateStatus::Working
        );
    }

    #[test]
    fn test_teammate_status_is_available() {
        assert!(TeammateStatus::Working.is_available());
        assert!(TeammateStatus::Idle.is_available());
        assert!(!TeammateStatus::Shutdown.is_available());
    }

    #[test]
    fn test_team_message_new() {
        let msg =
            TeamMessage::new("alice", MessageType::Message, "bob", "Hello");
        assert_eq!(msg.recipient, "alice");
        assert_eq!(msg.msg_type, MessageType::Message);
        assert_eq!(msg.sender, "bob");
        assert_eq!(msg.content, "Hello");
        assert!(msg.request_id.is_none());
        assert!(!msg.read);
        assert!(msg.timestamp > 0.0);
    }

    #[test]
    fn test_team_message_with_request_id() {
        let msg =
            TeamMessage::new("alice", MessageType::Broadcast, "bob", "Hello")
                .with_request_id("req-123");
        assert_eq!(msg.request_id, Some("req-123".to_string()));
    }

    #[test]
    fn test_message_type_as_str() {
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
        assert_eq!(MessageType::ProgressReport.as_str(), "progress_report");
        assert_eq!(MessageType::PlanSubmitted.as_str(), "plan_submitted");
    }

    #[test]
    fn test_message_type_from_db_string() {
        assert_eq!(
            MessageType::from_db_string("message"),
            Some(MessageType::Message)
        );
        assert_eq!(
            MessageType::from_db_string("broadcast"),
            Some(MessageType::Broadcast)
        );
        assert_eq!(
            MessageType::from_db_string("shutdown_request"),
            Some(MessageType::ShutdownRequest)
        );
        assert_eq!(
            MessageType::from_db_string("task_failed"),
            Some(MessageType::TaskFailed)
        );
        assert_eq!(
            MessageType::from_db_string("progress_report"),
            Some(MessageType::ProgressReport)
        );
        assert_eq!(
            MessageType::from_db_string("plan_submitted"),
            Some(MessageType::PlanSubmitted)
        );
        assert_eq!(MessageType::from_db_string("unknown"), None);
    }

    #[test]
    fn test_agent_status_new_idle() {
        let status = AgentStatus::new(
            "a1".to_string(),
            "alice".to_string(),
            Some("coder".to_string()),
            vec![],
        );
        assert_eq!(status.agent_id, "a1");
        assert_eq!(status.name, "alice");
        assert_eq!(status.agent_type, Some("coder".to_string()));
        assert_eq!(status.status, "idle");
        assert!(status.current_tasks.is_empty());
    }

    #[test]
    fn test_agent_status_new_busy() {
        let status = AgentStatus::new(
            "a2".to_string(),
            "bob".to_string(),
            None,
            vec!["t1".to_string()],
        );
        assert_eq!(status.status, "busy");
        assert_eq!(status.current_tasks.len(), 1);
    }

    #[test]
    fn test_team_patch_default() {
        let patch = TeamPatch::default();
        assert!(patch.name.is_none());
        assert!(patch.status.is_none());
        assert!(patch.lead.is_none());
        assert!(patch.task_ids.is_none());
    }

    #[test]
    fn test_message_priority_as_str() {
        assert_eq!(MessagePriority::High.as_str(), "high");
        assert_eq!(MessagePriority::Normal.as_str(), "normal");
        assert_eq!(MessagePriority::Low.as_str(), "low");
    }

    #[test]
    fn test_message_priority_from_db_string() {
        assert_eq!(
            MessagePriority::from_db_string("high"),
            MessagePriority::High
        );
        assert_eq!(
            MessagePriority::from_db_string("low"),
            MessagePriority::Low
        );
        assert_eq!(
            MessagePriority::from_db_string("normal"),
            MessagePriority::Normal
        );
        assert_eq!(
            MessagePriority::from_db_string("unknown"),
            MessagePriority::Normal
        );
    }

    #[test]
    fn test_message_priority_ordering() {
        // Note: Ord is derived based on definition order
        // High=0, Normal=1, Low=2, so High < Normal < Low
        assert!(MessagePriority::High < MessagePriority::Normal);
        assert!(MessagePriority::Normal < MessagePriority::Low);
        assert!(MessagePriority::High < MessagePriority::Low);
    }

    #[test]
    fn test_message_priority_default() {
        assert_eq!(MessagePriority::default(), MessagePriority::Normal);
    }

    #[test]
    fn test_team_message_with_priority() {
        let msg = TeamMessage::new(
            "lead",
            MessageType::TaskFailed,
            "member1",
            "Task failed",
        )
        .with_priority(MessagePriority::High);

        assert_eq!(msg.priority, MessagePriority::High);
    }
}
