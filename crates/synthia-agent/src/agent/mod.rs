//! The unified [`Agent`] runtime contract + supporting types.
//!
//! ## Module Layout
//!
//! - [`descriptor`]: [`descriptor::AgentDescriptor`] /
//!   [`descriptor::AgentFilter`] / [`descriptor::AgentEntry`] —
//!   the metadata + registry entry wrappers.
//! - [`registry`]: [`registry::AgentRegistry`] — the
//!   multi-agent catalog, implementing
//!   [`synthia_core::registry::Registry`].
//! - [`re_act`]: [`re_act::ReActAgent`] — the canonical
//!   `Agent` implementation backed by the existing ReAct loop.
//!   Wires [`crate::prompt`] for system-prompt assembly.

use std::{pin::Pin, sync::Arc};

use async_trait::async_trait;
use futures::Stream;
use synthia_core::registry::RegistryItem;
use tokio_util::sync::CancellationToken;

use crate::{events::AgentEvent, input::AgentInput};

pub mod descriptor;
pub mod re_act;
pub mod registry;

pub use descriptor::{AgentDescriptor, AgentEntry, AgentFilter};
pub use re_act::ReActAgent;
pub use registry::AgentRegistry;

/// A unified, asynchronous agent runtime contract.
///
/// Every concrete agent paradigm (ReAct, pipeline, planner,
/// router, …) implements this trait. The contract is deliberately
/// minimal so new paradigms can be added without changing the
/// call sites that drive them.
#[async_trait]
pub trait Agent: RegistryItem + Send + Sync {
    /// Stable, cheaply-cloneable metadata. Returned by reference
    /// so callers can introspect without taking ownership.
    fn descriptor(&self) -> &AgentDescriptor;

    /// Run one session and stream every [`AgentEvent`] in real
    /// time.
    ///
    /// `cancel` is wrapped in `Arc` so the returned stream and any
    /// sub-tasks spawned during execution can observe cancellation
    /// without further plumbing. The token is **not** consumed by
    /// this call — the caller retains ownership.
    ///
    /// Errors are surfaced *through the stream* as
    /// [`AgentEvent::System`] variants
    /// (`SessionEnded { reason: Error(..) }` for fatal errors,
    /// `Warning { .. }` for recoverable issues). The stream itself
    /// never yields `Err(_)` — `AgentEvent` is the only stream
    /// item. Unrecoverable internal panics propagate to the
    /// consumer via standard Rust panic semantics (the
    /// surrounding `tokio::spawn` join handle or `Stream::poll_next`
    /// caller); they are **not** silently swallowed. Fatal agent
    /// errors (provider failure, malformed streaming chunks, etc.)
    /// are converted to a terminal `SessionEnded { reason: Error(..) }`
    /// event before the stream closes.
    async fn run(
        &self,
        input: AgentInput,
        cancel: Arc<CancellationToken>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        agent::descriptor::AgentDescriptor,
        events::AgentEvent,
        input::AgentInput,
    };

    /// Build a minimal valid `AgentDescriptor`.
    fn make_descriptor(name: &str) -> AgentDescriptor {
        AgentDescriptor {
            name: name.to_string(),
            description: "test".to_string(),
            kind: "test".to_string(),
            version: "0.1.0".to_string(),
            instructions: String::new(),
            capabilities: vec![],
            tools: vec![],
            model_hint: None,
            handoffs: vec![],
            handoff_hint: None,
            output_schema: None,
            owner: None,
            domain: None,
            persona: None,
            display_name: None,
        }
    }

    /// A stub agent used to verify trait dispatch + Send/Sync.
    struct StubAgent {
        descriptor: AgentDescriptor,
    }

    impl synthia_core::registry::RegistryItem for StubAgent {
        fn name(&self) -> &str {
            &self.descriptor.name
        }

        fn description(&self) -> &str {
            &self.descriptor.description
        }
    }

    #[async_trait]
    impl Agent for StubAgent {
        fn descriptor(&self) -> &AgentDescriptor {
            &self.descriptor
        }

        async fn run(
            &self,
            _input: AgentInput,
            _cancel: Arc<CancellationToken>,
        ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>> {
            Box::pin(futures::stream::empty())
        }
    }

    /// Stub agents MUST be `Send + Sync` (the trait bound).
    #[test]
    fn stub_agent_is_send_sync() {
        fn assert_send<T: Send + Sync>() {}
        assert_send::<StubAgent>();
    }

    /// `Agent::run` MUST return a stream of `AgentEvent`.
    #[tokio::test]
    async fn stub_agent_run_returns_empty_stream() {
        let agent = StubAgent {
            descriptor: make_descriptor("stub"),
        };
        let cancel = Arc::new(CancellationToken::new());
        let mut stream = agent.run(AgentInput::text(""), cancel).await;
        use futures::StreamExt;
        assert!(stream.next().await.is_none());
    }

    /// `Agent::descriptor` MUST return a reference (no clone).
    #[test]
    fn stub_agent_descriptor_returns_reference() {
        let agent = StubAgent {
            descriptor: make_descriptor("ref-test"),
        };
        let d1 = agent.descriptor();
        let d2 = agent.descriptor();
        assert_eq!(d1.name, "ref-test");
        // Both must point to the same descriptor (no re-construction).
        assert!(std::ptr::eq(d1, d2));
    }

    /// After the panel refactor `AdversarialRole` /
    /// `DebateProtocol` are gone. This test asserts that the
    /// agent descriptor's "default" identity is what every
    /// plain agent sees: no role, no panel.
    #[test]
    fn descriptor_has_no_panel_fields_after_refactor() {
        let d = make_descriptor("plain");
        // Persona's absence is the post-refactor default.
        assert!(d.persona.is_none());
        // Owner/domain stay open.
        assert!(d.owner.is_none());
        assert!(d.domain.is_none());
    }
}
