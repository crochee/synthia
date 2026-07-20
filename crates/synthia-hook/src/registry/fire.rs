//! The 6 `fire_*` methods on [`HookRegistry`].
//!
//! Each method snapshots the set of non-failed hook IDs, then
//! iterates them and calls the corresponding
//! [`crate::traits::AgentHook`] method through the
//! `safe_hook_fail_open` wrapper. `fire_before_tool` is the
//! only one that can short-circuit (a `ToolAction::Skip` or
//! `ToolAction::Cancel` from any hook returns immediately).

use synthia_core::Error;
use ulid::Ulid;

use super::{registry::HookRegistry, safety::safe_hook_fail_open};
#[allow(deprecated)]
use crate::traits::{AgentContext, ToolAction};

#[allow(deprecated)]
impl HookRegistry {
    /// Snapshot the IDs of all non-failed hooks.
    fn non_failed_ids(&self) -> Vec<Ulid> {
        self.hooks
            .read()
            .map(|h| {
                h.keys().copied().filter(|id| !self.is_failed(id)).collect()
            })
            .unwrap_or_default()
    }

    pub async fn fire_before_llm(
        &self,
        ctx: &mut AgentContext,
    ) -> Result<(), Error> {
        for id in self.non_failed_ids() {
            if let Some(hook) =
                self.hooks.read().ok().and_then(|h| h.get(&id).cloned())
            {
                let f = hook.on_before_llm(ctx);
                let policy = hook.fail_policy();
                if let Err(e) =
                    safe_hook_fail_open(f, &id, (), (), policy).await
                {
                    self.record_failure(id);
                    tracing::warn!(hook_id = %id, error = %e, "Hook error (fail-open)");
                }
            }
        }
        Ok(())
    }

    pub async fn fire_after_llm(
        &self,
        ctx: &AgentContext,
        response: &serde_json::Value,
    ) -> Result<(), Error> {
        for id in self.non_failed_ids() {
            if let Some(hook) =
                self.hooks.read().ok().and_then(|h| h.get(&id).cloned())
            {
                let f = hook.on_after_llm(ctx, response);
                let policy = hook.fail_policy();
                match safe_hook_fail_open(f, &id, (), (), policy).await {
                    Ok(()) => {}
                    Err(e) => {
                        self.record_failure(id);
                        tracing::error!(hook_id = %id, error = %e, "Hook panicked in fire_after_llm");
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn fire_before_tool(
        &self,
        ctx: &AgentContext,
        call: &serde_json::Value,
    ) -> Result<ToolAction, Error> {
        for id in self.non_failed_ids() {
            if let Some(hook) =
                self.hooks.read().ok().and_then(|h| h.get(&id).cloned())
            {
                let f = hook.on_before_tool(ctx, call);
                let policy = hook.fail_policy();
                match safe_hook_fail_open(
                    f,
                    &id,
                    ToolAction::Proceed,
                    ToolAction::Skip,
                    policy,
                )
                .await
                {
                    Ok(action) => {
                        if action != ToolAction::Proceed {
                            return Ok(action);
                        }
                    }
                    Err(e) => {
                        self.record_failure(id);
                        tracing::warn!(hook_id = %id, error = %e, "Hook error in fire_before_tool (fail-open)");
                    }
                }
            }
        }
        Ok(ToolAction::Proceed)
    }

    pub async fn fire_after_tool(
        &self,
        ctx: &AgentContext,
        call: &serde_json::Value,
        result: &serde_json::Value,
    ) -> Result<(), Error> {
        for id in self.non_failed_ids() {
            if let Some(hook) =
                self.hooks.read().ok().and_then(|h| h.get(&id).cloned())
            {
                let f = hook.on_after_tool(ctx, call, result);
                let policy = hook.fail_policy();
                match safe_hook_fail_open(f, &id, (), (), policy).await {
                    Ok(()) => {}
                    Err(e) => {
                        self.record_failure(id);
                        tracing::error!(hook_id = %id, error = %e, "Hook panicked in fire_after_tool");
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn fire_iteration_end(
        &self,
        ctx: &AgentContext,
        iteration: usize,
    ) -> Result<(), Error> {
        for id in self.non_failed_ids() {
            if let Some(hook) =
                self.hooks.read().ok().and_then(|h| h.get(&id).cloned())
            {
                let f = hook.on_iteration_end(ctx, iteration);
                let policy = hook.fail_policy();
                match safe_hook_fail_open(f, &id, (), (), policy).await {
                    Ok(()) => {}
                    Err(e) => {
                        self.record_failure(id);
                        tracing::error!(hook_id = %id, error = %e, "Hook panicked in fire_iteration_end");
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn fire_complete(&self, ctx: &AgentContext) -> Result<(), Error> {
        for id in self.non_failed_ids() {
            if let Some(hook) =
                self.hooks.read().ok().and_then(|h| h.get(&id).cloned())
            {
                let f = hook.on_complete(ctx);
                let policy = hook.fail_policy();
                match safe_hook_fail_open(f, &id, (), (), policy).await {
                    Ok(()) => {}
                    Err(e) => {
                        self.record_failure(id);
                        tracing::error!(hook_id = %id, error = %e, "Hook panicked in fire_complete");
                    }
                }
            }
        }
        Ok(())
    }
}
