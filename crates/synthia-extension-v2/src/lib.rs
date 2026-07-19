//! `synthia-extension-v2` — Typed extension system for Synthia.
//!
//! Provides an `Extension` trait operating on 19 typed event payloads,
//! `ExtensionManifest` for declarative registration, capability-scoped
//! sandbox execution, and `ExtensionRegistry` with dual registration
//! into `ServiceRegistry`.
//!
//! See `specs/extension-system/spec.md` for normative requirements.

pub mod event_renderer;
pub mod events;
pub mod manifest;
pub mod registry;
pub mod sandbox;

use async_trait::async_trait;
pub use event_renderer::{
    EventRenderer,
    EventRendererError,
    EventRendererRegistry,
    JsonEventRenderer,
};
pub use events::ExtensionEvent;
pub use manifest::{Capability, ExtensionManifest, ExtensionManifestError};
pub use registry::{ExtensionRegistry, ExtensionRegistryError};
pub use sandbox::{Sandbox, SandboxError};

/// The core trait every extension must implement.
///
/// Each method corresponds to one of the 19 typed events. The default
/// implementation returns `ExtensionOutcome::Allow`, so implementors
/// only override the events they care about.
#[async_trait]
pub trait Extension: Send + Sync + 'static {
    /// Stable extension identifier (used for double-registration and logging).
    fn id(&self) -> &str;

    /// The manifest declaring this extension's capabilities and subscriptions.
    fn manifest(&self) -> &ExtensionManifest;

    // ── 19 typed event callbacks ──────────────────────────────────

    async fn on_session_start(
        &self,
        _event: &events::SessionStartPayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_session_end(
        &self,
        _event: &events::SessionEndPayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_user_prompt_submit(
        &self,
        _event: &events::UserPromptSubmitPayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_pre_tool_use(
        &self,
        _event: &events::PreToolUsePayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_post_tool_use(
        &self,
        _event: &events::PostToolUsePayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_pre_response(
        &self,
        _event: &events::PreResponsePayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_post_response(
        &self,
        _event: &events::PostResponsePayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_pre_compact(
        &self,
        _event: &events::PreCompactPayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_post_compact(
        &self,
        _event: &events::PostCompactPayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_pre_message_drop(
        &self,
        _event: &events::PreMessageDropPayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_pre_steering(
        &self,
        _event: &events::PreSteeringPayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_post_steering(
        &self,
        _event: &events::PostSteeringPayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_pre_subagent_spawn(
        &self,
        _event: &events::PreSubagentSpawnPayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_post_subagent_spawn(
        &self,
        _event: &events::PostSubagentSpawnPayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_pre_definition_drift(
        &self,
        _event: &events::PreDefinitionDriftPayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_post_definition_drift(
        &self,
        _event: &events::PostDefinitionDriftPayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_pre_mcp_route(
        &self,
        _event: &events::PreMCPRoutePayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_post_mcp_route(
        &self,
        _event: &events::PostMCPRoutePayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }

    async fn on_pre_oauth_flow(
        &self,
        _event: &events::PreOAuthFlowPayload,
    ) -> ExtensionOutcome {
        ExtensionOutcome::Allow
    }
}

/// Outcome of an extension callback.
///
/// Mirrors the `HookOutcome` 3-state from `hook-system-unification`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionOutcome {
    /// The extension allows the action to proceed.
    Allow,
    /// The extension denies the action with a reason.
    Deny { reason: String },
    /// Forward the action to the main agent (subagent context).
    ForwardToMainAgent { hint: String },
}

impl From<ExtensionOutcome> for synthia_hook::HookOutcome {
    fn from(outcome: ExtensionOutcome) -> Self {
        match outcome {
            ExtensionOutcome::Allow => synthia_hook::HookOutcome::Allow,
            ExtensionOutcome::Deny { reason } => {
                synthia_hook::HookOutcome::Deny { reason }
            }
            ExtensionOutcome::ForwardToMainAgent { hint } => {
                synthia_hook::HookOutcome::ForwardToMainAgent { hint }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_outcome_allow_converts_to_hook_outcome() {
        let outcome: synthia_hook::HookOutcome = ExtensionOutcome::Allow.into();
        assert_eq!(outcome, synthia_hook::HookOutcome::Allow);
    }

    #[test]
    fn extension_outcome_deny_converts_to_hook_outcome() {
        let outcome: synthia_hook::HookOutcome = ExtensionOutcome::Deny {
            reason: "blocked".into(),
        }
        .into();
        assert_eq!(
            outcome,
            synthia_hook::HookOutcome::Deny {
                reason: "blocked".into()
            }
        );
    }

    #[test]
    fn extension_outcome_forward_converts_to_hook_outcome() {
        let outcome: synthia_hook::HookOutcome =
            ExtensionOutcome::ForwardToMainAgent {
                hint: "review".into(),
            }
            .into();
        assert_eq!(
            outcome,
            synthia_hook::HookOutcome::ForwardToMainAgent {
                hint: "review".into()
            }
        );
    }
}
