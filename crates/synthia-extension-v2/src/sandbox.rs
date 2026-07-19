//! Typed capability-scoped sandbox for extension execution.
//!
//! Before invoking an extension callback, the sandbox checks that the
//! extension's manifest declares the required capability. If not, the
//! callback is refused and a metrics counter is incremented.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::{
    Capability,
    Extension,
    ExtensionManifest,
    ExtensionOutcome,
    events::ExtensionEvent,
};

/// Errors from sandbox execution.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// The extension lacks the required capability.
    #[error(
        "capability violation: extension {extension_id} missing {required:?}"
    )]
    CapabilityViolation {
        extension_id: String,
        required: Capability,
    },
}

/// The capability-scoped sandbox.
///
/// Wraps an `Extension` and enforces that each callback's required
/// capability is declared in the extension's manifest.
pub struct Sandbox {
    /// The extension being sandboxed.
    extension: Box<dyn Extension>,
    /// Counter of capability violations (exposed as a metric).
    violation_count: AtomicU64,
}

impl Sandbox {
    /// Create a new sandbox wrapping the given extension.
    pub fn new(extension: Box<dyn Extension>) -> Self {
        Self {
            extension,
            violation_count: AtomicU64::new(0),
        }
    }

    /// Returns the extension's id.
    pub fn id(&self) -> &str {
        self.extension.id()
    }

    /// Returns the extension's manifest.
    pub fn manifest(&self) -> &ExtensionManifest {
        self.extension.manifest()
    }

    /// Total capability violations observed.
    #[must_use]
    pub fn violation_count(&self) -> u64 {
        self.violation_count.load(Ordering::SeqCst)
    }

    /// Check whether the extension has the required capability.
    /// Returns `Ok(())` if the capability is present, `Err` otherwise.
    pub fn check_capability(
        &self,
        required: Capability,
    ) -> Result<(), SandboxError> {
        if self.extension.manifest().capabilities.contains(&required) {
            Ok(())
        } else {
            self.violation_count.fetch_add(1, Ordering::SeqCst);
            tracing::warn!(
                target: "synthia::extension_v2",
                extension_id = self.extension.id(),
                capability = ?required,
                "extension_capability_violation",
            );
            Err(SandboxError::CapabilityViolation {
                extension_id: self.extension.id().to_string(),
                required,
            })
        }
    }

    /// Dispatch an event to the extension after checking capability.
    ///
    /// Returns `Deny` if the capability check fails; otherwise delegates
    /// to the extension's callback.
    pub async fn dispatch(
        &self,
        event: &ExtensionEvent,
        required: Capability,
    ) -> ExtensionOutcome {
        if let Err(e) = self.check_capability(required) {
            return ExtensionOutcome::Deny {
                reason: e.to_string(),
            };
        }
        self.invoke_callback(event).await
    }

    /// Invoke the extension's typed callback based on the event variant.
    async fn invoke_callback(
        &self,
        event: &ExtensionEvent,
    ) -> ExtensionOutcome {
        match event {
            ExtensionEvent::SessionStart(p) => {
                self.extension.on_session_start(p).await
            }
            ExtensionEvent::SessionEnd(p) => {
                self.extension.on_session_end(p).await
            }
            ExtensionEvent::UserPromptSubmit(p) => {
                self.extension.on_user_prompt_submit(p).await
            }
            ExtensionEvent::PreToolUse(p) => {
                self.extension.on_pre_tool_use(p).await
            }
            ExtensionEvent::PostToolUse(p) => {
                self.extension.on_post_tool_use(p).await
            }
            ExtensionEvent::PreResponse(p) => {
                self.extension.on_pre_response(p).await
            }
            ExtensionEvent::PostResponse(p) => {
                self.extension.on_post_response(p).await
            }
            ExtensionEvent::PreCompact(p) => {
                self.extension.on_pre_compact(p).await
            }
            ExtensionEvent::PostCompact(p) => {
                self.extension.on_post_compact(p).await
            }
            ExtensionEvent::PreMessageDrop(p) => {
                self.extension.on_pre_message_drop(p).await
            }
            ExtensionEvent::PreSteering(p) => {
                self.extension.on_pre_steering(p).await
            }
            ExtensionEvent::PostSteering(p) => {
                self.extension.on_post_steering(p).await
            }
            ExtensionEvent::PreSubagentSpawn(p) => {
                self.extension.on_pre_subagent_spawn(p).await
            }
            ExtensionEvent::PostSubagentSpawn(p) => {
                self.extension.on_post_subagent_spawn(p).await
            }
            ExtensionEvent::PreDefinitionDrift(p) => {
                self.extension.on_pre_definition_drift(p).await
            }
            ExtensionEvent::PostDefinitionDrift(p) => {
                self.extension.on_post_definition_drift(p).await
            }
            ExtensionEvent::PreMCPRoute(p) => {
                self.extension.on_pre_mcp_route(p).await
            }
            ExtensionEvent::PostMCPRoute(p) => {
                self.extension.on_post_mcp_route(p).await
            }
            ExtensionEvent::PreOAuthFlow(p) => {
                self.extension.on_pre_oauth_flow(p).await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::{
        events::SessionStartPayload,
        manifest::{Capability, ExtensionManifest},
    };

    struct TestExt {
        manifest: ExtensionManifest,
    }

    #[async_trait::async_trait]
    impl Extension for TestExt {
        fn id(&self) -> &'static str {
            "test-ext"
        }

        fn manifest(&self) -> &ExtensionManifest {
            &self.manifest
        }
    }

    fn make_ext(caps: HashSet<Capability>) -> TestExt {
        let mut manifest = ExtensionManifest {
            name: "test-ext".into(),
            version: "0.1.0".into(),
            description: String::new(),
            capabilities: HashSet::new(),
        };
        manifest.capabilities = caps;
        TestExt { manifest }
    }

    #[tokio::test]
    async fn check_capability_all_when_present() {
        let ext = make_ext(HashSet::from([Capability::SessionRead]));
        let sandbox = Sandbox::new(Box::new(ext));
        assert!(sandbox.check_capability(Capability::SessionRead).is_ok());
    }

    #[tokio::test]
    async fn check_capability_denies_when_missing() {
        let ext = make_ext(HashSet::from([Capability::FileRead]));
        let sandbox = Sandbox::new(Box::new(ext));
        assert!(sandbox.check_capability(Capability::Network).is_err());
        assert_eq!(sandbox.violation_count(), 1);
    }

    #[tokio::test]
    async fn dispatch_returns_deny_on_capability_violation() {
        let ext = make_ext(HashSet::new());
        let sandbox = Sandbox::new(Box::new(ext));
        let event = ExtensionEvent::SessionStart(SessionStartPayload {
            session_id: "s".into(),
        });
        let outcome = sandbox.dispatch(&event, Capability::Network).await;
        assert!(matches!(outcome, ExtensionOutcome::Deny { .. }));
    }
}
