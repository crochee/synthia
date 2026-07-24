//! The 4 hook-fire methods on
//! [`super::core::AgentPluginLoader`]. Each takes the
//! shared `HookRunner` lock, fires the corresponding
//! [`synthia_plugin::HookEvent`], and logs a warning on
//! failure (does not propagate the error).
//!
//! - [`AgentPluginLoader::fire_agent_start`] — `AgentStart`.
//! - [`AgentPluginLoader::fire_session_start`] — `SessionStart`.
//! - [`AgentPluginLoader::fire_pre_tool_use`] —
//!   `PreToolUse` (tool name in metadata).
//! - [`AgentPluginLoader::fire_post_tool_use`] —
//!   `PostToolUse` (tool name + `success` extra in
//!   metadata).

use synthia_plugin::{HookEvent as PluginHookEvent, HookMetadata};

use super::{core::AgentPluginLoader, error::PluginLoaderError};

impl AgentPluginLoader {
    /// Fire the AgentStart hook event.
    pub async fn fire_agent_start(
        &self,
        session_id: &str,
    ) -> Result<(), PluginLoaderError> {
        let metadata = HookMetadata::new(session_id);
        let event = PluginHookEvent::AgentStart;

        let runner = self.hook_runner.lock().await;
        if let Err(e) = runner.fire(event, metadata).await {
            tracing::warn!(error = %e, "AgentStart hook failed");
        }

        Ok(())
    }

    /// Fire the SessionStart hook event.
    pub async fn fire_session_start(
        &self,
        session_id: &str,
    ) -> Result<(), PluginLoaderError> {
        let metadata = HookMetadata::new(session_id);
        let event = PluginHookEvent::SessionStart;

        let runner = self.hook_runner.lock().await;
        if let Err(e) = runner.fire(event, metadata).await {
            tracing::warn!(error = %e, "SessionStart hook failed");
        }

        Ok(())
    }

    /// Fire the PreToolUse hook event.
    pub async fn fire_pre_tool_use(
        &self,
        tool_name: &str,
    ) -> Result<(), PluginLoaderError> {
        let metadata = HookMetadata::new(tool_name);
        let event = PluginHookEvent::PreToolUse;

        let runner = self.hook_runner.lock().await;
        if let Err(e) = runner.fire(event, metadata).await {
            tracing::warn!(
                tool = %tool_name,
                error = %e,
                "PreToolUse hook failed"
            );
        }

        Ok(())
    }

    /// Fire the PostToolUse hook event.
    pub async fn fire_post_tool_use(
        &self,
        tool_name: &str,
        success: bool,
    ) -> Result<(), PluginLoaderError> {
        let metadata = HookMetadata::new(tool_name)
            .with_extra("success", if success { "true" } else { "false" });
        let event = PluginHookEvent::PostToolUse;

        let runner = self.hook_runner.lock().await;
        if let Err(e) = runner.fire(event, metadata).await {
            tracing::warn!(
                tool = %tool_name,
                error = %e,
                "PostToolUse hook failed"
            );
        }

        Ok(())
    }
}
