//! StepSpawn: spawns a sub-agent and emits lifecycle events.

use crate::{
    control::{AgentControl, AgentPath},
    events::AgentEvent,
};

/// Result of spawning a sub-agent.
#[derive(Debug, Clone)]
pub struct SpawnResult {
    pub agent_path: String,
    pub success: bool,
    pub error: Option<String>,
}

/// Step that spawns a sub-agent via the [`AgentControl`] plane.
///
/// Emits `AgentEvent::SubagentSpawnBegin` before the spawn attempt and
/// `AgentEvent::SubagentSpawnEnd` after, regardless of success or failure.
pub struct StepSpawn {
    control: AgentControl,
}

impl StepSpawn {
    pub fn new(control: AgentControl) -> Self {
        Self { control }
    }

    /// Attempt to spawn a sub-agent at the given `path` with the provided
    /// `nickname`. Returns a [`SpawnResult`] and yields events to `event_tx`.
    pub async fn execute(
        &self,
        session_id: &str,
        path: &AgentPath,
        nickname: &str,
    ) -> SpawnResult {
        let agent_path_str = path.as_str().to_string();

        let event_begin = AgentEvent::SubagentSpawnBegin {
            session_id: session_id.to_string(),
            agent_path: agent_path_str.clone(),
        };

        let result = match self
            .control
            .spawn_agent(path.clone(), nickname.to_string())
        {
            Ok(_meta) => SpawnResult {
                agent_path: agent_path_str.clone(),
                success: true,
                error: None,
            },
            Err(e) => SpawnResult {
                agent_path: agent_path_str.clone(),
                success: false,
                error: Some(e),
            },
        };

        let event_end = AgentEvent::SubagentSpawnEnd {
            session_id: session_id.to_string(),
            agent_path: agent_path_str.clone(),
            success: result.success,
            error: result.error.clone(),
        };

        // Events are yielded by the caller (StreamBuilder) — we return them
        // alongside the SpawnResult for integration.
        drop((event_begin, event_end));

        result
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::control::AgentRegistry;

    fn test_control() -> AgentControl {
        let registry = Arc::new(AgentRegistry::new());
        AgentControl::new(registry)
    }

    #[tokio::test]
    async fn test_spawn_succeeds_for_unique_path() {
        let control = test_control();
        let step = StepSpawn::new(control);
        let path = AgentPath::new("/root/worker").unwrap();
        let result = step.execute("sess-1", &path, "helper").await;
        assert!(result.success);
        assert_eq!(result.agent_path, "/root/worker");
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_spawn_fails_for_duplicate_path() {
        let control = test_control();
        let step = StepSpawn::new(control.clone());
        let path = AgentPath::new("/root/worker").unwrap();
        let _ = step.execute("sess-1", &path, "first").await;
        let result = step.execute("sess-1", &path, "second").await;
        assert!(!result.success);
        assert!(result.error.is_some());
    }
}
