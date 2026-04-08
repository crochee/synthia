//! Event definitions grouped by phase

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::phases::HookPhase;

// =============================================================================
// Session Events
// =============================================================================

/// Emitted when a session starts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStart {
    pub session_id: String,
}

/// Emitted when a session ends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEnd {
    pub session_id: String,
    pub message_count: usize,
}

// =============================================================================
// Agent Events
// =============================================================================

/// Emitted before the agent starts processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeforeAgentStart {
    pub session_id: String,
}

/// Emitted after the agent finishes processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AfterAgentEnd {
    pub session_id: String,
    pub success: bool,
}

/// Emitted when agent status changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatusChanged {
    pub session_id: String,
    pub old_status: String,
    pub new_status: String,
}

// =============================================================================
// Mode Events
// =============================================================================

/// Emitted before agent mode is set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeforeAgentModeSet {
    pub mode: String,
}

/// Emitted after agent mode is set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AfterAgentModeSet {
    pub mode: String,
}

// =============================================================================
// LLM Events
// =============================================================================

/// Emitted before making an LLM call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeforeLLMCall {
    pub model: String,
    pub message_count: usize,
}

/// Emitted after an LLM call completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AfterLLMCall {
    pub model: String,
    pub tokens_used: Option<u64>,
    pub success: bool,
}

// =============================================================================
// Step Events
// =============================================================================

/// Emitted before a ReAct step begins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeforeStep {
    pub session_id: String,
    pub step: u32,
}

/// Emitted after a ReAct step completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AfterStep {
    pub session_id: String,
    pub step: u32,
    pub tool_count: usize,
}

/// Emitted when a step is cancelled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepCancelled {
    pub session_id: String,
    pub step: u32,
    pub reason: String,
}

// =============================================================================
// Turn Events
// =============================================================================

/// Emitted before a turn completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeforeTurnComplete {
    pub session_id: String,
    pub turn_id: String,
}

/// Emitted after a turn completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AfterTurnComplete {
    pub session_id: String,
    pub turn_id: String,
    pub has_errors: bool,
}

/// Emitted when tool scheduling plan is created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchedulingPlan {
    pub session_id: String,
    pub turn_id: String,
    pub tools: Vec<ToolInfo>,
    pub schedule: ScheduleInfo,
}

/// Information about a tool in the schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub id: String,
    pub name: String,
    pub is_read_only: bool,
    pub is_concurrency_safe: bool,
}

/// Information about the schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleInfo {
    pub total_tools: usize,
    pub phases: Vec<PhaseInfo>,
}

/// Information about an execution phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseInfo {
    pub phase_id: u32,
    pub tool_count: usize,
    pub execution_mode: String,
}

// =============================================================================
// Tool Events
// =============================================================================

/// Emitted before executing a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeforeToolCall {
    pub tool: String,
    pub args: Value,
}

/// Emitted after a tool execution completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AfterToolCall {
    pub tool: String,
    pub args: Value,
    pub success: bool,
}

/// Emitted after a batch of tools completes (within a phase).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AfterToolBatchComplete {
    pub session_id: String,
    pub batch_id: u32,
    pub tool_count: usize,
    pub has_errors: bool,
}

// =============================================================================
// Context Events
// =============================================================================

/// Emitted when context compaction occurs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextCompaction {
    pub messages_removed: usize,
    pub tokens_saved: u64,
}

// =============================================================================
// Team Events
// =============================================================================

/// Emitted when a team member joins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMemberJoined {
    pub team_id: String,
    pub member_id: String,
}

/// Emitted when a team member leaves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMemberLeft {
    pub team_id: String,
    pub member_id: String,
    pub reason: String,
}

/// Emitted when a task is assigned to a team member.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamTaskAssigned {
    pub team_id: String,
    pub task_id: String,
    pub assignee: String,
}

/// Emitted when a task is completed by a team member.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamTaskCompleted {
    pub team_id: String,
    pub task_id: String,
    pub result: String,
}

/// Returns the phase for a given event type.
pub fn get_event_phase<T: 'static>() -> Option<HookPhase> {
    let phase = std::any::type_name::<T>();
    if phase.contains("Session") {
        Some(HookPhase::Session)
    } else if phase.contains("Agent") {
        Some(HookPhase::Agent)
    } else if phase.contains("Mode") {
        Some(HookPhase::Mode)
    } else if phase.contains("LLM") {
        Some(HookPhase::LLM)
    } else if phase.contains("Step") {
        Some(HookPhase::Step)
    } else if phase.contains("Turn") || phase.contains("ToolScheduling") {
        Some(HookPhase::Turn)
    } else if phase.contains("Tool") {
        Some(HookPhase::Tool)
    } else if phase.contains("Context") {
        Some(HookPhase::Context)
    } else if phase.contains("Team") {
        Some(HookPhase::Team)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_start_serialization() {
        let event = SessionStart {
            session_id: "test-session".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: SessionStart = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.session_id, "test-session");
    }

    #[test]
    fn test_session_end_serialization() {
        let event = SessionEnd {
            session_id: "test-session".to_string(),
            message_count: 10,
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: SessionEnd = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.session_id, "test-session");
        assert_eq!(parsed.message_count, 10);
    }

    #[test]
    fn test_before_step_serialization() {
        let event = BeforeStep {
            session_id: "test-session".to_string(),
            step: 5,
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: BeforeStep = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.session_id, "test-session");
        assert_eq!(parsed.step, 5);
    }

    #[test]
    fn test_after_step_serialization() {
        let event = AfterStep {
            session_id: "test-session".to_string(),
            step: 5,
            tool_count: 3,
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: AfterStep = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.step, 5);
        assert_eq!(parsed.tool_count, 3);
    }

    #[test]
    fn test_step_cancelled_serialization() {
        let event = StepCancelled {
            session_id: "test-session".to_string(),
            step: 5,
            reason: "user_request".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: StepCancelled = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.step, 5);
        assert_eq!(parsed.reason, "user_request");
    }

    #[test]
    fn test_before_tool_call_serialization() {
        let event = BeforeToolCall {
            tool: "Read".to_string(),
            args: serde_json::json!({"path": "/tmp"}),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: BeforeToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tool, "Read");
        assert_eq!(parsed.args["path"], "/tmp");
    }

    #[test]
    fn test_after_tool_call_serialization() {
        let event = AfterToolCall {
            tool: "Read".to_string(),
            args: serde_json::json!({"path": "/tmp"}),
            success: true,
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: AfterToolCall = serde_json::from_str(&json).unwrap();
        assert!(parsed.success);
    }

    #[test]
    fn test_context_compaction_serialization() {
        let event = ContextCompaction {
            messages_removed: 5,
            tokens_saved: 1000,
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: ContextCompaction = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.messages_removed, 5);
        assert_eq!(parsed.tokens_saved, 1000);
    }

    #[test]
    fn test_team_member_joined_serialization() {
        let event = TeamMemberJoined {
            team_id: "team-1".to_string(),
            member_id: "member-1".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: TeamMemberJoined = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.team_id, "team-1");
        assert_eq!(parsed.member_id, "member-1");
    }

    #[test]
    fn test_team_task_assigned_serialization() {
        let event = TeamTaskAssigned {
            team_id: "team-1".to_string(),
            task_id: "task-1".to_string(),
            assignee: "member-1".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: TeamTaskAssigned = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.task_id, "task-1");
        assert_eq!(parsed.assignee, "member-1");
    }

    #[test]
    fn test_tool_scheduling_plan_serialization() {
        let event = ToolSchedulingPlan {
            session_id: "test-session".to_string(),
            turn_id: "turn-1".to_string(),
            tools: vec![ToolInfo {
                id: "1".to_string(),
                name: "Read".to_string(),
                is_read_only: true,
                is_concurrency_safe: true,
            }],
            schedule: ScheduleInfo {
                total_tools: 1,
                phases: vec![PhaseInfo {
                    phase_id: 0,
                    tool_count: 1,
                    execution_mode: "Parallel".to_string(),
                }],
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: ToolSchedulingPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tools.len(), 1);
        assert_eq!(parsed.schedule.total_tools, 1);
    }

    #[test]
    fn test_after_tool_batch_complete_serialization() {
        let event = AfterToolBatchComplete {
            session_id: "test-session".to_string(),
            batch_id: 0,
            tool_count: 3,
            has_errors: false,
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: AfterToolBatchComplete =
            serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.batch_id, 0);
        assert_eq!(parsed.tool_count, 3);
        assert!(!parsed.has_errors);
    }

    #[test]
    fn test_agent_status_changed_serialization() {
        let event = AgentStatusChanged {
            session_id: "test-session".to_string(),
            old_status: "Running".to_string(),
            new_status: "Completed".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: AgentStatusChanged = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.old_status, "Running");
        assert_eq!(parsed.new_status, "Completed");
    }
}
