//! Bridge adapter: wraps [`SubagentSessionFactory`] (agent-side trait)
//! as a [`GuardianSubagentFactory`] (guardian-local trait).
//!
//! # Why a bridge?
//!
//! `synthia-guardian` cannot depend on `synthia-agent` (circular
//! dependency), so it defines its own [`GuardianSubagentFactory`] trait
//! mirroring the `run_child` subset of [`SubagentSessionFactory`]. This
//! adapter lives in `synthia-agent` (which already depends on
//! `synthia-guardian`) and performs pure type conversion:
//!
//! - `AgentResult` → `GuardianSubagentOutput` (success = `Completed`)
//! - `SubagentSessionError` → `GuardianSubagentSpawnError`
//!
//! # Contractual obligations (deferred)
//!
//! The factory implementation MUST apply the three-layer lockdown
//! (runtime / registry / permission) and inject the prompt-cache key
//! `guardian:{parent_session_id}` when spawning the child session.
//! These cannot be enforced through the opaque `run_child` interface
//! (see `synthia-guardian/src/subagent_reviewer.rs` module docs). Tasks
//! 2.5 and 2.6 are deferred because the concrete
//! `SubagentSessionFactory` implementation lives in `synthia-server`,
//! not in `synthia-agent`.

use std::sync::Arc;

use async_trait::async_trait;
use synthia_guardian::{
    GuardianSubagentFactory,
    GuardianSubagentOutput,
    GuardianSubagentSpawnError,
};

use crate::{
    agent_instance::AgentStatus,
    subagent::{SubagentSessionError, SubagentSessionFactory},
};

/// Bridge that adapts [`SubagentSessionFactory`] (agent-side) to
/// [`GuardianSubagentFactory`] (guardian-side).
///
/// Performs type conversion only — lockdown enforcement is a
/// contractual obligation on the concrete `SubagentSessionFactory`
/// implementation (lives in `synthia-server`).
pub struct GuardianSubagentFactoryBridge {
    inner: Arc<dyn SubagentSessionFactory>,
}

impl GuardianSubagentFactoryBridge {
    pub fn new(inner: Arc<dyn SubagentSessionFactory>) -> Self {
        Self { inner }
    }
}

impl std::fmt::Debug for GuardianSubagentFactoryBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuardianSubagentFactoryBridge").finish()
    }
}

#[async_trait]
impl GuardianSubagentFactory for GuardianSubagentFactoryBridge {
    async fn run_child(
        &self,
        user_id: String,
        parent_session_id: String,
        prompt: String,
    ) -> Result<GuardianSubagentOutput, GuardianSubagentSpawnError> {
        // The guardian-side trait does not carry spawn depth (guardian
        // subagents are review workers, not user-facing nested agents).
        // Pass 0 so the inner factory treats this as a root-context
        // spawn. The depth limit is enforced on the user-facing
        // `AgentTool::call` path, not here.
        //
        // `maybe_id` is `None` because the guardian bridge does not
        // need to register the child session for subtree cancellation
        // (guardian subagents are leaf review workers with no further
        // nesting).
        match self
            .inner
            .run_child(user_id, parent_session_id, prompt, 0, None)
            .await
        {
            Ok(result) => Ok(GuardianSubagentOutput {
                output: result.output,
                success: result.status == AgentStatus::Completed,
            }),
            Err(e) => Err(map_session_error(e)),
        }
    }
}

fn map_session_error(e: SubagentSessionError) -> GuardianSubagentSpawnError {
    match e {
        SubagentSessionError::ParentNotFound(id) => {
            GuardianSubagentSpawnError::ParentNotFound(id)
        }
        SubagentSessionError::Unauthorized(msg)
        | SubagentSessionError::CreationFailed(msg) => {
            GuardianSubagentSpawnError::SpawnFailed(msg)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::agent_instance::{AgentResult, AgentStatus, AgentTokenUsage};

    /// Mock `SubagentSessionFactory` that returns a canned `AgentResult`.
    ///
    /// `AgentResult` is not `Clone`, so we store the components and
    /// reconstruct a fresh `AgentResult` on each `run_child` call.
    struct MockSessionFactory {
        output: Mutex<String>,
        status: AgentStatus,
    }

    impl MockSessionFactory {
        fn completed(output: &str) -> Self {
            Self {
                output: Mutex::new(output.to_string()),
                status: AgentStatus::Completed,
            }
        }

        fn errored(output: &str) -> Self {
            Self {
                output: Mutex::new(output.to_string()),
                status: AgentStatus::Errored,
            }
        }

        fn build_result(&self) -> AgentResult {
            AgentResult {
                output: self.output.lock().unwrap().clone(),
                status: self.status,
                token_usage: AgentTokenUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                },
            }
        }
    }

    #[async_trait]
    impl SubagentSessionFactory for MockSessionFactory {
        async fn create_child(
            &self,
            _user_id: String,
            _parent_session_id: String,
            _maybe_id: Option<String>,
            _parent_depth: usize,
        ) -> Result<crate::subagent::ChildSessionHandle, SubagentSessionError>
        {
            unimplemented!("create_child not used in bridge tests")
        }

        async fn run_child(
            &self,
            _user_id: String,
            _parent_session_id: String,
            _prompt: String,
            _parent_depth: usize,
            _maybe_id: Option<String>,
        ) -> Result<AgentResult, SubagentSessionError> {
            Ok(self.build_result())
        }
    }

    #[tokio::test]
    async fn bridge_maps_completed_to_success() {
        let factory = Arc::new(MockSessionFactory::completed(
            r#"{"risk_level":"low","risk_score":30}"#,
        ));
        let bridge = GuardianSubagentFactoryBridge::new(factory);

        let output = bridge
            .run_child("u".into(), "p".into(), "prompt".into())
            .await
            .expect("completed run should succeed");

        assert!(output.success);
        assert!(output.output.contains("risk_level"));
    }

    #[tokio::test]
    async fn bridge_maps_errored_to_failure() {
        let factory = Arc::new(MockSessionFactory::errored("panic"));
        let bridge = GuardianSubagentFactoryBridge::new(factory);

        let output = bridge
            .run_child("u".into(), "p".into(), "prompt".into())
            .await
            .expect("errored run still returns output");

        assert!(!output.success);
        assert_eq!(output.output, "panic");
    }
}
