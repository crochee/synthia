//! The serde-tagged [`AgentEvent`] enum — five top-level variants.
//!
//! - [`AgentEvent::Model`] — Provider
//!   [`synthia_provider::ContentPart`] pass-through for raw streaming
//!   chunks (text, reasoning, tool use, tool result, image, audio,
//!   resource).
//! - [`AgentEvent::ModelDone`] — Final aggregated
//!   [`synthia_provider::SamplingResult`].
//! - [`AgentEvent::System`] — Lifecycle and diagnostic state changes
//!   (see [`SystemEvent`]).
//! - [`AgentEvent::Agent`] — Recursive subagent trace wrapped with
//!   [`AgentMeta`].

use serde::{Deserialize, Serialize};
use synthia_provider::{ContentPart, SamplingResult};

use super::{
    agent_meta::AgentMeta,
    system_event::{SystemEvent, WarningKind},
};

/// Events emitted by the agent during a session lifecycle. Serialized
/// with serde internally tagged for dispatch.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum AgentEvent {
    /// A streaming chunk from the model.
    ///
    /// This is the pass-through carrier for `ContentPart` — text
    /// deltas, reasoning deltas, tool use, tool result, image,
    /// audio, resource — encoded verbatim from the Provider.
    Model(ContentPart),
    /// Final aggregated result of one model sampling pass (text,
    /// tool calls, reasoning, usage).
    ModelDone(SamplingResult),
    /// Lifecycle, diagnostic, and terminal state changes that are
    /// not user-visible streaming content.
    System(SystemEvent),
    /// A child (subagent) trace carrying the inner [`AgentEvent`]
    /// plus the [`AgentMeta`] that ties it back to its parent
    /// session.
    Agent(AgentMeta, Box<AgentEvent>),
}

impl AgentEvent {
    /// Short, stable label for log lines and metrics.
    ///
    /// Unlike [`AgentEvent::is_durable`] (which inspects inner
    /// `ContentPart` kinds), `kind` collapses every
    /// `Model(ContentPart)` into the single `"Model"` bucket so
    /// log queries can filter on the outer variant without
    /// enumerating every `ContentPart` shape.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Model(_) => "Model",
            Self::ModelDone(_) => "ModelDone",
            Self::System(_) => "System",
            Self::Agent(_, _) => "Agent",
        }
    }

    /// Single-shot size estimate for log lines and metrics.
    ///
    /// Uses one `serde_json::to_vec` call instead of
    /// `to_value(...).to_string().len()` (two serializations) or
    /// `to_string(...).len()` + later re-serialization in the
    /// broadcaster (three serializations total). Cheap, lock-free,
    /// allocation-free on the `else` branch.
    ///
    /// Returns 0 on serialization failure — log lines must never
    /// fail because of a measurement error.
    pub fn serialized_size(&self) -> usize {
        serde_json::to_vec(self).map(|v| v.len()).unwrap_or(0)
    }

    /// Returns `true` if this event is durable (state-changing).
    ///
    /// Durable events must be replayed to reconstruct session state.
    /// Ephemeral events are observable side-effects (streaming
    /// deltas, progress, warnings) that can be skipped during replay
    /// without affecting projected state.
    ///
    /// Per the `event-durability-classification` spec:
    /// - Durable: `Model(Text | ToolUse | ToolResult | Resource)`
    /// - Ephemeral: everything else, including
    ///   `Model(Reasoning | Image | Audio)`, `ModelDone`, every
    ///   `System` variant, and the
    ///   `Agent(meta, inner)` recursive wrapper (its inner event's
    ///   durability is decided by recursively unwrapping).
    pub fn is_durable(&self) -> bool {
        match self {
            Self::Model(ContentPart::Text(_))
            | Self::Model(ContentPart::ToolUse(_))
            | Self::Model(ContentPart::ToolResult(_))
            | Self::Model(ContentPart::Resource(_)) => true,
            Self::Model(_) | Self::ModelDone(_) | Self::System(_) => false,
            Self::Agent(_, inner) => inner.is_durable(),
        }
    }

    /// Convenience constructor for an ephemeral text delta.
    pub fn text_delta(text: impl Into<String>) -> Self {
        Self::Model(ContentPart::Text(synthia_provider::TextContent {
            text: text.into(),
            cache_control: None,
        }))
    }

    /// Convenience constructor for an ephemeral reasoning delta.
    pub fn reasoning_delta(
        text: impl Into<String>,
        signature: Option<String>,
    ) -> Self {
        Self::Model(ContentPart::Reasoning(
            synthia_provider::ReasoningContent {
                text: text.into(),
                signature,
            },
        ))
    }

    /// Convenience constructor for a system-level progress event.
    pub fn progress(
        message: impl Into<String>,
        step: usize,
        total: usize,
    ) -> Self {
        Self::System(SystemEvent::Progress {
            message: message.into(),
            step,
            total,
        })
    }

    /// Convenience constructor for a system-level warning.
    pub fn warning(message: impl Into<String>) -> Self {
        Self::System(SystemEvent::Warning {
            kind: WarningKind::Hook,
            message: message.into(),
            iteration: None,
        })
    }

    /// Convenience constructor for a system-level warning of an
    /// arbitrary [`WarningKind`].
    pub fn warning_kind(kind: WarningKind, message: impl Into<String>) -> Self {
        Self::System(SystemEvent::Warning {
            kind,
            message: message.into(),
            iteration: None,
        })
    }

    /// Convenience constructor for a [`SystemEvent::Recovery`].
    pub fn recovery(
        level_number: u32,
        tool_name: Option<String>,
        message: impl Into<String>,
        iteration: Option<usize>,
    ) -> Self {
        Self::System(SystemEvent::Recovery {
            level_number,
            tool_name,
            message: message.into(),
            iteration,
        })
    }

    /// Convenience constructor for a [`SystemEvent::Usage`].
    pub fn usage(
        input_tokens: usize,
        output_tokens: usize,
        cache_read_tokens: Option<usize>,
        cache_creation_tokens: Option<usize>,
    ) -> Self {
        Self::System(SystemEvent::Usage {
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::agent_meta::AgentMeta;

    fn make_text(text: &str) -> ContentPart {
        ContentPart::Text(synthia_provider::TextContent {
            text: text.to_string(),
            cache_control: None,
        })
    }

    fn make_reasoning(text: &str) -> ContentPart {
        ContentPart::Reasoning(synthia_provider::ReasoningContent {
            text: text.to_string(),
            signature: None,
        })
    }

    fn make_image() -> ContentPart {
        ContentPart::Image(synthia_provider::ImageContent {
            data: String::new(),
            mime_type: "image/png".to_string(),
            detail: None,
        })
    }

    fn make_tool_use() -> ContentPart {
        ContentPart::ToolUse(synthia_provider::ToolUse {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({}),
        })
    }

    fn make_tool_result() -> ContentPart {
        ContentPart::ToolResult(synthia_provider::ToolResult {
            tool_use_id: "call-1".to_string(),
            tool_name: None,
            content: vec![],
            structured_content: None,
            is_error: None,
            metadata: Default::default(),
            truncated_by: None,
        })
    }

    fn make_resource() -> ContentPart {
        ContentPart::Resource(synthia_provider::ResourceLink {
            uri: "file:///tmp/x".to_string(),
            name: "x".to_string(),
            title: None,
            description: None,
            mime_type: None,
        })
    }

    fn make_meta() -> AgentMeta {
        AgentMeta::new("parent-1", "agent-1", 1)
    }

    // -- kind() 5-way enum mapping ---------------------------------------

    /// `kind()` is the stable label used by log
    /// queries and metrics. Pin all 5 variants so
    /// a refactor that changes a label breaks
    /// observability dashboards loudly.
    #[test]
    fn kind_returns_stable_label_for_every_variant() {
        assert_eq!(AgentEvent::Model(make_text("x")).kind(), "Model");
        assert_eq!(
            AgentEvent::ModelDone(synthia_provider::SamplingResult::default())
                .kind(),
            "ModelDone"
        );
        assert_eq!(
            AgentEvent::System(SystemEvent::SessionStarted {
                session_id: "s".into()
            })
            .kind(),
            "System"
        );
        let inner = AgentEvent::Model(make_text("inner"));
        assert_eq!(
            AgentEvent::Agent(make_meta(), Box::new(inner)).kind(),
            "Agent"
        );
    }

    /// `kind()` returns `&'static str` so it can
    /// be stored in formatters without
    /// allocation. Compile-time check.
    #[test]
    fn kind_returns_static_str() {
        let event = AgentEvent::text_delta("x");
        let label: &'static str = event.kind();
        assert_eq!(label, "Model");
    }

    // -- is_durable() 4-way + recursive Agent ----------------------------

    /// Per the `event-durability-classification`
    /// spec, durable `Model` events are
    /// exactly Text, ToolUse, ToolResult, and
    /// Resource. Anything else (Reasoning,
    /// Image, Audio) is ephemeral. Pin the
    /// full matrix.
    #[test]
    fn is_durable_model_text_tool_use_tool_result_resource_are_durable() {
        assert!(AgentEvent::Model(make_text("hello")).is_durable());
        assert!(AgentEvent::Model(make_tool_use()).is_durable());
        assert!(AgentEvent::Model(make_tool_result()).is_durable());
        assert!(AgentEvent::Model(make_resource()).is_durable());
    }

    #[test]
    fn is_durable_model_reasoning_image_audio_are_ephemeral() {
        assert!(!AgentEvent::Model(make_reasoning("x")).is_durable());
        assert!(!AgentEvent::Model(make_image()).is_durable());
        // Pin the audio-ephemeral contract via a
        // different construction path: build
        // the variant tag through a JSON round
        // trip and verify the match arm.
        let audio_json = serde_json::json!({
            "type": "audio",
            "data": "",
            "mime_type": "audio/wav",
            "format": null
        });
        let audio_part: ContentPart =
            serde_json::from_value(audio_json).expect("parse audio");
        assert!(
            !AgentEvent::Model(audio_part).is_durable(),
            "Audio content parts MUST be ephemeral"
        );
    }

    #[test]
    fn is_durable_model_done_is_ephemeral() {
        // ModelDone is the aggregated final
        // result of one model sampling pass; it
        // is NOT replayed because the
        // individual Model events that compose
        // it are durable.
        assert!(
            !AgentEvent::ModelDone(synthia_provider::SamplingResult::default())
                .is_durable()
        );
    }

    #[test]
    fn is_durable_system_events_are_ephemeral() {
        // System events are diagnostic; they
        // do not change the projected state.
        let sys = AgentEvent::System(SystemEvent::SessionStarted {
            session_id: "s".into(),
        });
        assert!(!sys.is_durable());
    }

    /// The recursive `Agent(meta, inner)`
    /// wrapper MUST delegate durability to the
    /// inner event — the meta wrapper itself
    /// is invisible to replay logic. Pin
    /// both directions.
    #[test]
    fn is_durable_agent_recurses_to_inner_event() {
        // durable inner → durable.
        let durable_inner = AgentEvent::Model(make_text("hello"));
        assert!(
            AgentEvent::Agent(make_meta(), Box::new(durable_inner))
                .is_durable()
        );
        // ephemeral inner → ephemeral.
        let ephemeral_inner = AgentEvent::Model(make_reasoning("x"));
        assert!(
            !AgentEvent::Agent(make_meta(), Box::new(ephemeral_inner))
                .is_durable()
        );
        // System inner → ephemeral.
        let sys_inner = AgentEvent::System(SystemEvent::SessionStarted {
            session_id: "s".into(),
        });
        assert!(
            !AgentEvent::Agent(make_meta(), Box::new(sys_inner)).is_durable()
        );
    }

    // -- serialized_size() log-safety contract ---------------------------

    /// `serialized_size()` MUST return 0 on
    /// serialization failure rather than
    /// panicking — log lines must never fail
    /// because of a measurement error. For
    /// well-formed events, it MUST return the
    /// byte length of the JSON encoding (so
    /// it's non-zero for non-trivial events).
    #[test]
    fn serialized_size_is_non_zero_for_well_formed_event() {
        let event = AgentEvent::text_delta("hello world");
        let size = event.serialized_size();
        assert!(
            size > 0,
            "well-formed event MUST have positive serialized size"
        );
    }

    /// `serialized_size()` returns the byte
    /// length of the JSON encoding (verified
    /// via independent re-serialization).
    /// Pin: callers may rely on this for log
    /// sampling / rate limiting.
    #[test]
    fn serialized_size_matches_serde_json_len() {
        let event = AgentEvent::text_delta("the quick brown fox");
        let expected = serde_json::to_vec(&event).unwrap().len();
        assert_eq!(event.serialized_size(), expected);
    }

    // -- Convenience constructors ---------------------------------------

    #[test]
    fn text_delta_constructor_builds_model_text() {
        match AgentEvent::text_delta("hi") {
            AgentEvent::Model(ContentPart::Text(t)) => {
                assert_eq!(t.text, "hi");
                assert!(t.cache_control.is_none());
            }
            _ => panic!("expected Model(Text)"),
        }
    }

    #[test]
    fn reasoning_delta_constructor_builds_model_reasoning() {
        match AgentEvent::reasoning_delta("think", Some("sig".into())) {
            AgentEvent::Model(ContentPart::Reasoning(r)) => {
                assert_eq!(r.text, "think");
                assert_eq!(r.signature, Some("sig".into()));
            }
            _ => panic!("expected Model(Reasoning)"),
        }
    }

    #[test]
    fn progress_constructor_builds_system_progress() {
        match AgentEvent::progress("step", 3, 10) {
            AgentEvent::System(SystemEvent::Progress {
                message,
                step,
                total,
            }) => {
                assert_eq!(message, "step");
                assert_eq!(step, 3);
                assert_eq!(total, 10);
            }
            _ => panic!("expected System(Progress)"),
        }
    }

    #[test]
    fn warning_constructor_defaults_to_hook_kind() {
        match AgentEvent::warning("oops") {
            AgentEvent::System(SystemEvent::Warning {
                kind,
                message,
                iteration,
            }) => {
                assert_eq!(kind, WarningKind::Hook);
                assert_eq!(message, "oops");
                assert!(iteration.is_none());
            }
            _ => panic!("expected System(Warning)"),
        }
    }

    #[test]
    fn warning_kind_constructor_uses_supplied_kind() {
        match AgentEvent::warning_kind(WarningKind::Loop, "loop detected") {
            AgentEvent::System(SystemEvent::Warning {
                kind,
                message,
                iteration,
            }) => {
                assert_eq!(kind, WarningKind::Loop);
                assert_eq!(message, "loop detected");
                assert!(iteration.is_none());
            }
            _ => panic!("expected System(Warning)"),
        }
    }

    #[test]
    fn recovery_constructor_propagates_all_fields() {
        match AgentEvent::recovery(
            2,
            Some("llm_sample".into()),
            "truncated",
            Some(7),
        ) {
            AgentEvent::System(SystemEvent::Recovery {
                level_number,
                tool_name,
                message,
                iteration,
            }) => {
                assert_eq!(level_number, 2);
                assert_eq!(tool_name, Some("llm_sample".into()));
                assert_eq!(message, "truncated");
                assert_eq!(iteration, Some(7));
            }
            _ => panic!("expected System(Recovery)"),
        }
    }

    #[test]
    fn usage_constructor_propagates_all_fields() {
        match AgentEvent::usage(100, 50, Some(20), None) {
            AgentEvent::System(SystemEvent::Usage {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_creation_tokens,
            }) => {
                assert_eq!(input_tokens, 100);
                assert_eq!(output_tokens, 50);
                assert_eq!(cache_read_tokens, Some(20));
                assert_eq!(cache_creation_tokens, None);
            }
            _ => panic!("expected System(Usage)"),
        }
    }
}
