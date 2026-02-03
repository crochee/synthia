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
pub enum TaskStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Failed,
    Stopped,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
            TaskStatus::Stopped => "stopped",
        }
    }

    pub fn from_db_string(s: &str) -> Self {
        match s {
            "in_progress" => TaskStatus::InProgress,
            "completed" => TaskStatus::Completed,
            "failed" => TaskStatus::Failed,
            "stopped" => TaskStatus::Stopped,
            _ => TaskStatus::Pending,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Stopped
        )
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
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
pub enum TaskPriority {
    #[default]
    Normal,
    Low,
    High,
    Critical,
}

impl TaskPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskPriority::Low => "low",
            TaskPriority::Normal => "normal",
            TaskPriority::High => "high",
            TaskPriority::Critical => "critical",
        }
    }

    pub fn from_db_string(s: &str) -> Self {
        match s {
            "low" => TaskPriority::Low,
            "high" => TaskPriority::High,
            "critical" => TaskPriority::Critical,
            _ => TaskPriority::Normal,
        }
    }

    pub fn level(&self) -> u8 {
        match self {
            TaskPriority::Low => 1,
            TaskPriority::Normal => 2,
            TaskPriority::High => 3,
            TaskPriority::Critical => 4,
        }
    }
}

impl std::fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaskPacket {
    pub objective: String,
    pub scope: String,
    pub repo: String,
    pub branch_policy: String,
    pub acceptance_tests: Vec<String>,
    pub commit_policy: String,
    pub reporting_contract: String,
    pub escalation_policy: String,
}

impl TaskPacket {
    pub fn new(objective: impl Into<String>, scope: impl Into<String>) -> Self {
        Self {
            objective: objective.into(),
            scope: scope.into(),
            repo: String::new(),
            branch_policy: String::new(),
            acceptance_tests: Vec::new(),
            commit_policy: String::new(),
            reporting_contract: String::new(),
            escalation_policy: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaskMessage {
    pub role: String,
    pub content: String,
    pub timestamp: i64,
}

impl TaskMessage {
    pub fn new(
        role: impl Into<String>,
        content: impl Into<String>,
        timestamp: i64,
    ) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            timestamp,
        }
    }

    pub fn user(content: impl Into<String>, timestamp: i64) -> Self {
        Self::new("user", content, timestamp)
    }

    pub fn assistant(content: impl Into<String>, timestamp: i64) -> Self {
        Self::new("assistant", content, timestamp)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: TaskStatus,
    pub blocked_by: Vec<String>,
    pub blocks: Vec<String>,
    pub owner: String,
    pub team_id: Option<String>,
    pub priority: TaskPriority,
    pub task_packet: Option<TaskPacket>,
    pub deadline: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub output: String,
    pub messages: Vec<TaskMessage>,
}

impl Task {
    pub fn new(id: impl Into<String>, subject: impl Into<String>) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id: id.into(),
            subject: subject.into(),
            description: String::new(),
            status: TaskStatus::default(),
            blocked_by: Vec::new(),
            blocks: Vec::new(),
            owner: String::new(),
            team_id: None,
            priority: TaskPriority::default(),
            task_packet: None,
            deadline: None,
            created_at: now,
            updated_at: now,
            output: String::new(),
            messages: Vec::new(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = owner.into();
        self
    }

    pub fn with_team(mut self, team_id: impl Into<String>) -> Self {
        self.team_id = Some(team_id.into());
        self
    }

    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_deadline(mut self, deadline: i64) -> Self {
        self.deadline = Some(deadline);
        self
    }

    pub fn is_blocked(&self) -> bool {
        !self.blocked_by.is_empty()
    }

    pub fn is_assigned(&self) -> bool {
        !self.owner.is_empty()
    }

    pub fn add_message(
        &mut self,
        role: impl Into<String>,
        content: impl Into<String>,
    ) {
        let now = chrono::Utc::now().timestamp();
        self.messages.push(TaskMessage::new(role, content, now));
        self.updated_at = now;
    }

    pub fn append_output(&mut self, output: &str) {
        self.output.push_str(output);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp();
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct TaskPatch {
    pub subject: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub blocked_by: Option<Vec<String>>,
    pub blocks: Option<Vec<String>>,
    pub owner: Option<String>,
    pub team_id: Option<String>,
    pub priority: Option<TaskPriority>,
    pub deadline: Option<i64>,
    pub output: Option<String>,
}

impl TaskPatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self
    }

    pub fn with_team(mut self, team_id: impl Into<String>) -> Self {
        self.team_id = Some(team_id.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =====================================================================
    // TaskStatus tests
    // =====================================================================

    #[test]
    fn test_task_status_as_str() {
        assert_eq!(TaskStatus::Pending.as_str(), "pending");
        assert_eq!(TaskStatus::InProgress.as_str(), "in_progress");
        assert_eq!(TaskStatus::Completed.as_str(), "completed");
        assert_eq!(TaskStatus::Failed.as_str(), "failed");
        assert_eq!(TaskStatus::Stopped.as_str(), "stopped");
    }

    #[test]
    fn test_task_status_from_db_string() {
        assert_eq!(
            TaskStatus::from_db_string("in_progress"),
            TaskStatus::InProgress
        );
        assert_eq!(
            TaskStatus::from_db_string("completed"),
            TaskStatus::Completed
        );
        assert_eq!(TaskStatus::from_db_string("failed"), TaskStatus::Failed);
        assert_eq!(TaskStatus::from_db_string("stopped"), TaskStatus::Stopped);
    }

    #[test]
    fn test_task_status_from_db_string_unknown_defaults_to_pending() {
        assert_eq!(TaskStatus::from_db_string("unknown"), TaskStatus::Pending);
        assert_eq!(TaskStatus::from_db_string(""), TaskStatus::Pending);
        assert_eq!(TaskStatus::from_db_string("invalid"), TaskStatus::Pending);
    }

    #[test]
    fn test_task_status_is_terminal() {
        assert!(!TaskStatus::Pending.is_terminal());
        assert!(!TaskStatus::InProgress.is_terminal());
        assert!(TaskStatus::Completed.is_terminal());
        assert!(TaskStatus::Failed.is_terminal());
        assert!(TaskStatus::Stopped.is_terminal());
    }

    #[test]
    fn test_task_status_display() {
        assert_eq!(format!("{}", TaskStatus::Pending), "pending");
        assert_eq!(format!("{}", TaskStatus::Completed), "completed");
    }

    #[test]
    fn test_task_status_default() {
        let status = TaskStatus::default();
        assert_eq!(status, TaskStatus::Pending);
    }

    #[test]
    fn test_task_status_eq() {
        assert_eq!(TaskStatus::Pending, TaskStatus::Pending);
        assert_ne!(TaskStatus::Pending, TaskStatus::InProgress);
        assert_eq!(TaskStatus::Completed, TaskStatus::Completed);
    }

    // =====================================================================
    // TaskPriority tests
    // =====================================================================

    #[test]
    fn test_task_priority_as_str() {
        assert_eq!(TaskPriority::Low.as_str(), "low");
        assert_eq!(TaskPriority::Normal.as_str(), "normal");
        assert_eq!(TaskPriority::High.as_str(), "high");
        assert_eq!(TaskPriority::Critical.as_str(), "critical");
    }

    #[test]
    fn test_task_priority_from_db_string() {
        assert_eq!(TaskPriority::from_db_string("low"), TaskPriority::Low);
        assert_eq!(TaskPriority::from_db_string("high"), TaskPriority::High);
        assert_eq!(
            TaskPriority::from_db_string("critical"),
            TaskPriority::Critical
        );
    }

    #[test]
    fn test_task_priority_from_db_string_unknown_defaults_to_normal() {
        assert_eq!(
            TaskPriority::from_db_string("unknown"),
            TaskPriority::Normal
        );
        assert_eq!(TaskPriority::from_db_string(""), TaskPriority::Normal);
        assert_eq!(
            TaskPriority::from_db_string("urgent"),
            TaskPriority::Normal
        );
    }

    #[test]
    fn test_task_priority_level() {
        assert_eq!(TaskPriority::Low.level(), 1);
        assert_eq!(TaskPriority::Normal.level(), 2);
        assert_eq!(TaskPriority::High.level(), 3);
        assert_eq!(TaskPriority::Critical.level(), 4);
    }

    #[test]
    fn test_task_priority_display() {
        assert_eq!(format!("{}", TaskPriority::Low), "low");
        assert_eq!(format!("{}", TaskPriority::Critical), "critical");
    }

    #[test]
    fn test_task_priority_default() {
        let priority = TaskPriority::default();
        assert_eq!(priority, TaskPriority::Normal);
    }

    // =====================================================================
    // TaskPacket tests
    // =====================================================================

    #[test]
    fn test_task_packet_new() {
        let packet = TaskPacket::new("objective", "scope");
        assert_eq!(packet.objective, "objective");
        assert_eq!(packet.scope, "scope");
        assert!(packet.repo.is_empty());
        assert!(packet.branch_policy.is_empty());
        assert!(packet.acceptance_tests.is_empty());
        assert!(packet.commit_policy.is_empty());
        assert!(packet.reporting_contract.is_empty());
        assert!(packet.escalation_policy.is_empty());
    }

    #[test]
    fn test_task_packet_new_with_string_types() {
        let packet =
            TaskPacket::new(String::from("obj"), String::from("scope"));
        assert_eq!(packet.objective, "obj");
        assert_eq!(packet.scope, "scope");
    }

    // =====================================================================
    // TaskMessage tests
    // =====================================================================

    #[test]
    fn test_task_message_new() {
        let msg = TaskMessage::new("user", "hello", 12345);
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "hello");
        assert_eq!(msg.timestamp, 12345);
    }

    #[test]
    fn test_task_message_user() {
        let msg = TaskMessage::user("hello", 100);
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "hello");
        assert_eq!(msg.timestamp, 100);
    }

    #[test]
    fn test_task_message_assistant() {
        let msg = TaskMessage::assistant("thinking", 200);
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "thinking");
        assert_eq!(msg.timestamp, 200);
    }

    #[test]
    fn test_task_message_with_string_args() {
        let msg = TaskMessage::new(
            String::from("system"),
            String::from("content"),
            300,
        );
        assert_eq!(msg.role, "system");
        assert_eq!(msg.content, "content");
    }

    // =====================================================================
    // Task tests
    // =====================================================================

    #[test]
    fn test_task_new_sets_id_and_subject() {
        let task = Task::new("task-1", "Test Task");
        assert_eq!(task.id, "task-1");
        assert_eq!(task.subject, "Test Task");
    }

    #[test]
    fn test_task_new_sets_defaults() {
        let task = Task::new("t", "s");
        assert!(task.description.is_empty());
        assert_eq!(task.status, TaskStatus::Pending);
        assert!(task.blocked_by.is_empty());
        assert!(task.blocks.is_empty());
        assert!(task.owner.is_empty());
        assert!(task.team_id.is_none());
        assert_eq!(task.priority, TaskPriority::Normal);
        assert!(task.task_packet.is_none());
        assert!(task.deadline.is_none());
        assert!(task.output.is_empty());
        assert!(task.messages.is_empty());
        assert!(task.created_at > 0);
        assert!(task.updated_at > 0);
    }

    #[test]
    fn test_task_with_description() {
        let task = Task::new("t", "s").with_description("desc");
        assert_eq!(task.description, "desc");
    }

    #[test]
    fn test_task_with_owner() {
        let task = Task::new("t", "s").with_owner("alice");
        assert_eq!(task.owner, "alice");
    }

    #[test]
    fn test_task_with_team() {
        let task = Task::new("t", "s").with_team("team-1");
        assert_eq!(task.team_id, Some("team-1".to_string()));
    }

    #[test]
    fn test_task_with_priority() {
        let task = Task::new("t", "s").with_priority(TaskPriority::Critical);
        assert_eq!(task.priority, TaskPriority::Critical);
    }

    #[test]
    fn test_task_with_status() {
        let task = Task::new("t", "s").with_status(TaskStatus::InProgress);
        assert_eq!(task.status, TaskStatus::InProgress);
    }

    #[test]
    fn test_task_with_deadline() {
        let task = Task::new("t", "s").with_deadline(999);
        assert_eq!(task.deadline, Some(999));
    }

    #[test]
    fn test_task_builder_chaining() {
        let task = Task::new("t", "s")
            .with_description("desc")
            .with_owner("bob")
            .with_team("team-2")
            .with_priority(TaskPriority::High)
            .with_status(TaskStatus::Completed)
            .with_deadline(500);

        assert_eq!(task.id, "t");
        assert_eq!(task.subject, "s");
        assert_eq!(task.description, "desc");
        assert_eq!(task.owner, "bob");
        assert_eq!(task.team_id, Some("team-2".to_string()));
        assert_eq!(task.priority, TaskPriority::High);
        assert_eq!(task.status, TaskStatus::Completed);
        assert_eq!(task.deadline, Some(500));
    }

    #[test]
    fn test_task_is_blocked() {
        let mut task = Task::new("t", "s");
        assert!(!task.is_blocked());

        task.blocked_by = vec!["dep-1".to_string()];
        assert!(task.is_blocked());

        task.blocked_by = vec![];
        assert!(!task.is_blocked());
    }

    #[test]
    fn test_task_is_assigned() {
        let mut task = Task::new("t", "s");
        assert!(!task.is_assigned());

        task.owner = "alice".to_string();
        assert!(task.is_assigned());

        task.owner = "".to_string();
        assert!(!task.is_assigned());
    }

    #[test]
    fn test_task_add_message() {
        let mut task = Task::new("t", "s");
        let initial_updated_at = task.updated_at;

        task.add_message("user", "hello");

        assert_eq!(task.messages.len(), 1);
        assert_eq!(task.messages[0].role, "user");
        assert_eq!(task.messages[0].content, "hello");
        assert!(task.updated_at >= initial_updated_at);
    }

    #[test]
    fn test_task_add_multiple_messages() {
        let mut task = Task::new("t", "s");
        task.add_message("user", "first");
        task.add_message("assistant", "second");
        task.add_message("user", "third");

        assert_eq!(task.messages.len(), 3);
        assert_eq!(task.messages[0].content, "first");
        assert_eq!(task.messages[1].content, "second");
        assert_eq!(task.messages[2].content, "third");
    }

    #[test]
    fn test_task_append_output() {
        let mut task = Task::new("t", "s");
        let initial_updated_at = task.updated_at;

        task.append_output("hello");
        assert_eq!(task.output, "hello");
        assert!(task.updated_at >= initial_updated_at);

        task.append_output(" world");
        assert_eq!(task.output, "hello world");
    }

    #[test]
    fn test_task_touch() {
        let mut task = Task::new("t", "s");
        let original_updated_at = task.updated_at;

        task.touch();
        assert!(task.updated_at >= original_updated_at);
    }

    // =====================================================================
    // TaskPatch tests
    // =====================================================================

    #[test]
    fn test_task_patch_new_is_empty() {
        let patch = TaskPatch::new();
        assert!(patch.subject.is_none());
        assert!(patch.description.is_none());
        assert!(patch.status.is_none());
        assert!(patch.blocked_by.is_none());
        assert!(patch.blocks.is_none());
        assert!(patch.owner.is_none());
        assert!(patch.team_id.is_none());
        assert!(patch.priority.is_none());
        assert!(patch.deadline.is_none());
        assert!(patch.output.is_none());
    }

    #[test]
    fn test_task_patch_with_status() {
        let patch = TaskPatch::new().with_status(TaskStatus::Completed);
        assert_eq!(patch.status, Some(TaskStatus::Completed));
    }

    #[test]
    fn test_task_patch_with_owner() {
        let patch = TaskPatch::new().with_owner("alice");
        assert_eq!(patch.owner, Some("alice".to_string()));
    }

    #[test]
    fn test_task_patch_with_team() {
        let patch = TaskPatch::new().with_team("team-1");
        assert_eq!(patch.team_id, Some("team-1".to_string()));
    }

    #[test]
    fn test_task_patch_chaining() {
        let patch = TaskPatch::new()
            .with_status(TaskStatus::InProgress)
            .with_owner("bob")
            .with_team("team-x");

        assert_eq!(patch.status, Some(TaskStatus::InProgress));
        assert_eq!(patch.owner, Some("bob".to_string()));
        assert_eq!(patch.team_id, Some("team-x".to_string()));
    }

    #[test]
    fn test_task_patch_default() {
        let patch = TaskPatch::default();
        assert!(patch.status.is_none());
        assert!(patch.owner.is_none());
    }
}
