//! Reply method implementation

use std::sync::Arc;

use futures::stream::{BoxStream, StreamExt};
use rmcp::model::SamplingMessage;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use super::Agent;
use crate::{
    Result,
    config::SessionConfig,
    hooks::HookEvent,
    types::AgentEvent,
};

impl Agent {
    #[instrument(skip_all, fields(session_id = %session_config.id))]
    pub async fn reply<'a>(
        &'a self,
        user_message: SamplingMessage,
        session_config: &'a SessionConfig,
        cancel_token: CancellationToken,
    ) -> Result<BoxStream<'a, Result<AgentEvent>>> {
        self.deps
            .hooks
            .emit(&HookEvent::BeforeAgentStart {
                session_id: session_config.id.clone(),
            })
            .await;

        self.deps
            .session
            .add_message(session_config, &user_message)
            .await?;

        // Spawn name update task in background
        let agent = Arc::new(self.clone());
        let session_config_clone = session_config.clone();
        let cancel_token_clone = cancel_token.clone();
        tokio::spawn(async move {
            if let Err(e) = agent
                .maybe_update_name(&session_config_clone, cancel_token_clone)
                .await
            {
                tracing::warn!("Failed to generate session description: {}", e);
            }
        });

        let conversation =
            self.deps.session.fix_conversation(session_config).await?;
        let (_, compact_events) = self
            .compact_conversation(&conversation, session_config)
            .await?;

        let session_config_clone = session_config.clone();
        let hook_registry = Arc::clone(&self.deps.hooks);

        Ok(Box::pin(async_stream::stream! {
            tokio::pin!(compact_events);
            while let Some(event) = compact_events.next().await {
                yield event;
            }

            let loop_stream = self.react(session_config_clone.clone(), cancel_token.clone()).await;

            tokio::pin!(loop_stream);
            while let Some(event) = loop_stream.next().await {
                yield Ok(event);
            }

            hook_registry
                .emit(&HookEvent::AfterAgentEnd {
                    session_id: session_config_clone.id,
                    success: true,
                })
                .await;
        }))
    }
}
