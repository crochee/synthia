//! 19 typed event payloads for the extension system.
//!
//! Each variant in [`ExtensionEvent`] carries a distinct payload struct.
//! The enum is exhaustive so consumers must handle every event type
//! (or explicitly match `_`).

use serde::{Deserialize, Serialize};

// ── Payload structs ───────────────────────────────────────────────

/// Session started.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStartPayload {
    pub session_id: String,
}

/// Session ended.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEndPayload {
    pub session_id: String,
    pub reason: String,
}

/// User submitted a prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPromptSubmitPayload {
    pub session_id: String,
    pub prompt_length: usize,
}

/// Before a tool is invoked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreToolUsePayload {
    pub tool_name: String,
    pub session_id: String,
    pub input_summary: String,
}

/// After a tool invocation completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostToolUsePayload {
    pub tool_name: String,
    pub session_id: String,
    pub success: bool,
    pub output_summary: String,
}

/// Before the LLM generates a response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreResponsePayload {
    pub session_id: String,
    pub iteration: usize,
}

/// After the LLM generates a response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostResponsePayload {
    pub session_id: String,
    pub iteration: usize,
    pub response_length: usize,
}

/// Before context compaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreCompactPayload {
    pub session_id: String,
    pub current_tokens: usize,
}

/// After context compaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostCompactPayload {
    pub session_id: String,
    pub old_tokens: usize,
    pub new_tokens: usize,
}

/// Before a message is dropped (Synthia 独有 — JSONL stream interrupt).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreMessageDropPayload {
    pub session_id: String,
    pub reason: String,
}

/// Before steering input is injected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreSteeringPayload {
    pub session_id: String,
    pub steering_source: String,
}

/// After steering input is injected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostSteeringPayload {
    pub session_id: String,
    pub accepted: bool,
}

/// Before a subagent is spawned.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreSubagentSpawnPayload {
    pub parent_session_id: String,
    pub child_agent_path: String,
}

/// After a subagent completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostSubagentSpawnPayload {
    pub parent_session_id: String,
    pub child_agent_path: String,
    pub success: bool,
}

/// Before definition drift is checked (subagent governance).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreDefinitionDriftPayload {
    pub session_id: String,
    pub file_path: String,
}

/// After definition drift is detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostDefinitionDriftPayload {
    pub session_id: String,
    pub file_path: String,
    pub drift_detected: bool,
}

/// Before an MCP route is resolved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreMCPRoutePayload {
    pub session_id: String,
    pub server_name: String,
}

/// After an MCP route completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostMCPRoutePayload {
    pub session_id: String,
    pub server_name: String,
    pub success: bool,
}

/// Before an OAuth flow is initiated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreOAuthFlowPayload {
    pub session_id: String,
    pub provider: String,
}

/// The 19 typed event enum. Each variant wraps its distinct payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExtensionEvent {
    SessionStart(SessionStartPayload),
    SessionEnd(SessionEndPayload),
    UserPromptSubmit(UserPromptSubmitPayload),
    PreToolUse(PreToolUsePayload),
    PostToolUse(PostToolUsePayload),
    PreResponse(PreResponsePayload),
    PostResponse(PostResponsePayload),
    PreCompact(PreCompactPayload),
    PostCompact(PostCompactPayload),
    PreMessageDrop(PreMessageDropPayload),
    PreSteering(PreSteeringPayload),
    PostSteering(PostSteeringPayload),
    PreSubagentSpawn(PreSubagentSpawnPayload),
    PostSubagentSpawn(PostSubagentSpawnPayload),
    PreDefinitionDrift(PreDefinitionDriftPayload),
    PostDefinitionDrift(PostDefinitionDriftPayload),
    PreMCPRoute(PreMCPRoutePayload),
    PostMCPRoute(PostMCPRoutePayload),
    PreOAuthFlow(PreOAuthFlowPayload),
}

impl ExtensionEvent {
    /// Returns the event name as a static string.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::SessionStart(_) => "SessionStart",
            Self::SessionEnd(_) => "SessionEnd",
            Self::UserPromptSubmit(_) => "UserPromptSubmit",
            Self::PreToolUse(_) => "PreToolUse",
            Self::PostToolUse(_) => "PostToolUse",
            Self::PreResponse(_) => "PreResponse",
            Self::PostResponse(_) => "PostResponse",
            Self::PreCompact(_) => "PreCompact",
            Self::PostCompact(_) => "PostCompact",
            Self::PreMessageDrop(_) => "PreMessageDrop",
            Self::PreSteering(_) => "PreSteering",
            Self::PostSteering(_) => "PostSteering",
            Self::PreSubagentSpawn(_) => "PreSubagentSpawn",
            Self::PostSubagentSpawn(_) => "PostSubagentSpawn",
            Self::PreDefinitionDrift(_) => "PreDefinitionDrift",
            Self::PostDefinitionDrift(_) => "PostDefinitionDrift",
            Self::PreMCPRoute(_) => "PreMCPRoute",
            Self::PostMCPRoute(_) => "PostMCPRoute",
            Self::PreOAuthFlow(_) => "PreOAuthFlow",
        }
    }
}

/// Compile-time assertion: exactly 19 variants (enforced by exhaustive match test).
const _: () = ();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_name_returns_correct_string() {
        let e = ExtensionEvent::SessionStart(SessionStartPayload {
            session_id: "s".into(),
        });
        assert_eq!(e.name(), "SessionStart");
    }

    #[test]
    fn serde_roundtrip() {
        let e = ExtensionEvent::PreToolUse(PreToolUsePayload {
            tool_name: "bash".into(),
            session_id: "s".into(),
            input_summary: "ls".into(),
        });
        let json = serde_json::to_string(&e).unwrap();
        let back: ExtensionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(e.name(), back.name());
    }

    #[test]
    fn exhaustive_match_compiles() {
        // If this compiles, all 19 variants are covered.
        let e = ExtensionEvent::SessionEnd(SessionEndPayload {
            session_id: String::new(),
            reason: String::new(),
        });
        let _name = match &e {
            ExtensionEvent::SessionStart(_) => "SessionStart",
            ExtensionEvent::SessionEnd(_) => "SessionEnd",
            ExtensionEvent::UserPromptSubmit(_) => "UserPromptSubmit",
            ExtensionEvent::PreToolUse(_) => "PreToolUse",
            ExtensionEvent::PostToolUse(_) => "PostToolUse",
            ExtensionEvent::PreResponse(_) => "PreResponse",
            ExtensionEvent::PostResponse(_) => "PostResponse",
            ExtensionEvent::PreCompact(_) => "PreCompact",
            ExtensionEvent::PostCompact(_) => "PostCompact",
            ExtensionEvent::PreMessageDrop(_) => "PreMessageDrop",
            ExtensionEvent::PreSteering(_) => "PreSteering",
            ExtensionEvent::PostSteering(_) => "PostSteering",
            ExtensionEvent::PreSubagentSpawn(_) => "PreSubagentSpawn",
            ExtensionEvent::PostSubagentSpawn(_) => "PostSubagentSpawn",
            ExtensionEvent::PreDefinitionDrift(_) => "PreDefinitionDrift",
            ExtensionEvent::PostDefinitionDrift(_) => "PostDefinitionDrift",
            ExtensionEvent::PreMCPRoute(_) => "PreMCPRoute",
            ExtensionEvent::PostMCPRoute(_) => "PostMCPRoute",
            ExtensionEvent::PreOAuthFlow(_) => "PreOAuthFlow",
        };
    }
}
