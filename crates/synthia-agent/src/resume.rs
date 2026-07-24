use std::sync::Arc;

use synthia_provider::types::Message;
use tokio_util::sync::CancellationToken;

use crate::{
    agent::Agent,
    checkpoint,
    config::AgentRunConfig,
    control::{AgentControl, AgentRegistry},
    events::AgentEvent,
    stream_builder::StreamBuilder,
    types::AgentInput,
};

impl Agent {
    pub fn resume(
        &self,
        user_id: String,
        session_id: String,
        cancel_token: CancellationToken,
    ) -> crate::types::AgentOutput {
        let config = self.config.clone();

        let checkpoint_dir =
            config.checkpoint_dir.clone().unwrap_or_else(|| {
                config.workspace_root.join(".agents").join("checkpoints")
            });

        let (messages, start_iteration) =
            match checkpoint::Checkpoint::load_latest_by_session(
                &checkpoint_dir,
                &session_id,
            ) {
                Ok(Some(cp)) if !cp.messages.is_empty() => {
                    let mut msgs = cp.messages;
                    checkpoint::patch_tool_calls_recovery(&mut msgs);
                    tracing::info!(
                        session_id = %session_id,
                        restored_iteration = %cp.iteration,
                        message_count = %msgs.len(),
                        "Resumed from checkpoint"
                    );
                    (msgs, cp.iteration)
                }
                _ => match self
                    .session_store
                    .load_messages_all::<Message>(&user_id, &session_id)
                {
                    Ok(msgs) if !msgs.is_empty() => {
                        tracing::info!(
                            session_id = %session_id,
                            message_count = %msgs.len(),
                            "Resumed from session JSONL (no checkpoint)"
                        );
                        (msgs, 0)
                    }
                    _ => {
                        return Box::pin(futures::stream::once(async move {
                            AgentEvent::warning(format!(
                                "No checkpoint or session data found for session '{}', cannot resume",
                                session_id
                            ))
                        }));
                    }
                },
            };

        let mut run_config = AgentRunConfig {
            provider: Arc::clone(&self.provider),
            tool_registry: self.tool_registry.clone(),
            hook_registry: Arc::clone(&self.hook_registry),
            model_router: Arc::clone(&self.model_router),
            user_id,
            session_id,
            input: AgentInput::text(""),
            config,
            context_assembler: Some(Arc::clone(&self.context_assembler)),
            session_store: self.session_store.clone(),
            steering_channel: self.steering_channel.clone(),
            session_input_queue: Some(
                self.session_manager.input_queue().clone(),
            ),
            cancel_token,
            memory_event_sender: self.memory_event_sender.clone(),
            agent_control: Some(AgentControl::new(Arc::new(
                AgentRegistry::new(),
            ))),
            fork_policy: Default::default(),
            compaction_provider: None,
            subagent_session_factory: None,
            approval_service: None,
            sandbox_manager: None,
            tool_orchestrator: None,
            guardian_coordinator: None,
            extension_manager: None,
            extension_registry: None,
            rollout_tracker: None,
            interceptor_chain: None,
            loop_services: std::sync::OnceLock::new(),
        };

        self.assemble_default_orchestrator(&mut run_config);

        #[cfg(feature = "otel")]
        let otel_ctx = crate::agent::otel_context::OtelContext::from_run_config(
            &run_config,
        );
        let mut builder = StreamBuilder::from_config(&run_config);
        builder.with_initial_state(messages, start_iteration);
        let stream = builder.run(run_config);
        #[cfg(feature = "otel")]
        {
            crate::agent::otel_context::wrap_output_with_otel(stream, otel_ctx)
        }
        #[cfg(not(feature = "otel"))]
        {
            stream
        }
    }
}
