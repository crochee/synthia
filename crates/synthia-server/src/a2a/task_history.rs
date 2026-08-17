//! `TaskHistoryBuilder` — A2A-protocol-faithful transcript recorder
//! for `Task.history`.
//!
//! # Why this exists
//!
//! `a2a-server-lf@0.4.1`'s `apply_event_to_task` discards
//! `StreamResponse::Message` events: only `Task` / `StatusUpdate` /
//! `ArtifactUpdate` are persisted. That means the model text the
//! agent streams never lands in `Task.history`, so a follow-up
//! `GET /api/v1/tasks/{id}` shows only the user message
//! (`prepare_task_for_execution` seeds the initial `Task` with
//! `history: Some(vec![req.message.clone()])`) and any tool
//! artifacts. The conversation looks truncated from the task
//! detail UI.
//!
//! A2A itself defines `Task.history: Option<Vec<Message>>` for
//! exactly this — a transcript of `Message`s the agent exchanged
//! with the user, with `Part::text` for plain text and
//! `Part::data` for structured payloads (tool calls / results are
//! cleanly modelled as `Part::data` rather than abusing
//! `Artifact`). The wire-level "Message events on the stream" are
//! a different thing; the executor is expected to consolidate
//! them into `Task.history` before the task goes terminal.
//!
//! # What this builder does
//!
//! 1. Records the user prompt as `Message::new(Role::User, ...)`.
//! 2. Coalesces streaming `ContentPart::Text` deltas into a single
//!    agent text message (no per-chunk fragmentation — that would
//!    explode `history` and is not what the UI wants to render).
//! 3. Emits one agent message per `ContentPart::ToolUse` and one
//!    per `ContentPart::ToolResult`, each carrying a `Part::data`
//!    with the provider-native ToolUse / ToolResult JSON shape
//!    (`{ id, name, input }` / `{ tool_use_id, content, is_error }`)
//!    — the same shape the live stream emits on `Message(agent)`, so
//!    the frontend reads history and stream with the same parser.
//! 4. On terminal, calls `TaskStore::update` to attach
//!    `history = Some(builder.into_messages())` to the task. The
//!    existing artifacts path is untouched.
//!
//! # Why we don't reuse the `Artifact` channel
//!
//! `Artifact` is the A2A protocol's "task attachment" channel —
//! its semantics are for outputs the agent produced (files,
//! images, captured side-effects). Stuffing the user prompt and
//! the agent's textual reply in there would break any A2A client
//! that walks artifacts for legitimate attachments. The right
//! home is `history`, which the protocol reserves for exactly
//! these messages.

use a2a::{Artifact, Message, Part, Role};
use serde_json::json;
use synthia_provider::{
    ContentPart,
    ResourceLink,
    ToolResult,
    ToolUse,
    types::{AudioContent, ImageContent},
};

use super::mapping::{
    audio_to_artifact,
    image_to_artifact,
    resource_link_to_artifact,
};

/// In-flight accumulator for a single task's `Task.history`.
///
/// The executor clones an instance into its `try_stream!` and
/// calls `record_*` for each relevant `AgentEvent`. The builder
/// is `!Sync` and lives entirely on the executor's task, so no
/// locking is needed.
pub(crate) struct TaskHistoryBuilder {
    messages: Vec<Message>,
    /// Accumulated artifacts for `Task.artifacts`. Each entry
    /// is the concrete `Artifact` produced by an
    /// `ArtifactUpdate` event in the live stream; we re-derive
    /// it here from the original `AgentEvent` so the persisted
    /// snapshot stays in sync with the streamed snapshot
    /// without the executor having to thread the live
    /// `ArtifactUpdate` values through.
    artifacts: Vec<Artifact>,
    /// Buffered text that hasn't yet been flushed into an agent
    /// message. Streamed `ContentPart::Text` events are chunks
    /// of the same logical text; we coalesce them and only emit
    /// a single `Message::new(Role::Agent, ...)` when we see a
    /// non-text event (ToolUse / ToolResult / ModelDone /
    /// SessionEnded).
    pending_agent_text: String,
    /// True once we've flushed at least one text segment. Used
    /// to decide whether `record_text_delta` should append to
    /// the in-progress text or start a fresh segment after a
    /// tool_use.
    have_pending_text: bool,
}

impl TaskHistoryBuilder {
    pub(crate) fn new() -> Self {
        Self {
            messages: Vec::new(),
            artifacts: Vec::new(),
            pending_agent_text: String::new(),
            have_pending_text: false,
        }
    }

    /// Record the inbound user message. Called exactly once,
    /// right before the prompt is submitted to the session
    /// controller. Empty prompts are skipped — they have no
    /// user-visible text to record and would only bloat the
    /// transcript.
    pub(crate) fn record_user_prompt(&mut self, prompt: &str) {
        if prompt.is_empty() {
            return;
        }
        self.messages.push(text_message(Role::User, prompt));
    }

    /// Append a streaming text delta. Does not push a new
    /// `Message` yet — the text is coalesced until a non-text
    /// event arrives (see [`Self::flush_pending_text`]).
    pub(crate) fn record_text_delta(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.pending_agent_text.push_str(text);
        self.have_pending_text = true;
    }

    /// Emit a `Message` for a `ToolUse` chunk, flushing any
    /// buffered text first. The payload mirrors the
    /// provider-native `ToolUse` shape `{ id, name, input }` —
    /// the same JSON the live stream emits on
    /// `Message(agent)`, so a frontend that reads `task.history`
    /// sees an identical transcript. There is no synthetic
    /// `kind` discriminator; `id` + `name` + `input` are the
    /// natural A2A signals.
    pub(crate) fn record_tool_use(&mut self, call: &ToolUse) {
        self.flush_pending_text();
        let payload = json!({
            "id": call.id,
            "name": call.name,
            "input": call.input,
        });
        self.messages.push(data_message(Role::Agent, payload));
    }

    /// Emit a `Message` for a `ToolResult`, flushing any
    /// buffered text first. Same reasoning as
    /// [`Self::record_tool_use`]: the JSON mirrors the
    /// provider-native `ToolResult` shape (`tool_use_id` +
    /// `content` + `is_error`), no synthetic wrapper.
    pub(crate) fn record_tool_result(&mut self, result: &ToolResult) {
        self.flush_pending_text();
        let preview = result
            .content
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");
        let payload = json!({
            "tool_use_id": result.tool_use_id,
            "content": preview,
            "is_error": result.is_error.unwrap_or(false),
        });
        self.messages.push(data_message(Role::Agent, payload));
    }

    /// Apply the final aggregated `ModelDone(SamplingResult)` —
    /// a per-pass terminal that carries the complete text +
    /// reasoning. We use the aggregated `text` (not the
    /// buffered deltas) so any mid-flight deduplication the
    /// provider already did wins; reasoning is dropped on
    /// purpose — it's an ephemeral provider hint, not
    /// user-facing conversation content.
    pub(crate) fn record_model_done(
        &mut self,
        result: &synthia_provider::SamplingResult,
    ) {
        if !result.text.is_empty() {
            // The deltas already accumulated the same text.
            // If they're identical, dropping them and writing
            // the aggregated value would be a no-op; if the
            // provider did any post-processing, the aggregated
            // value is more authoritative. Replace.
            self.pending_agent_text = result.text.clone();
            self.have_pending_text = true;
        }
        self.flush_pending_text();
    }

    /// Flush any buffered agent text into a single
    /// `Message::new(Role::Agent, ...)` and reset the buffer.
    /// No-op if there is nothing buffered.
    pub(crate) fn flush_pending_text(&mut self) {
        if !self.have_pending_text {
            return;
        }
        let text = std::mem::take(&mut self.pending_agent_text);
        self.have_pending_text = false;
        self.messages.push(text_message(Role::Agent, &text));
    }

    /// Drain the builder and return the consolidated
    /// `Vec<Message>`. Always call [`Self::flush_pending_text`]
    /// first if the caller cares about trailing text.
    #[cfg(test)]
    pub(crate) fn into_messages(mut self) -> Vec<Message> {
        self.flush_pending_text();
        self.messages
    }

    /// Record a tangible deliverable: a [`ResourceLink`] (MCP
    /// resource reference, file URI, CDN URL). Per A2A v1.0
    /// §3.7 the `ResourceLink` belongs on the artifact channel,
    /// NOT on the message channel — so this method only mutates
    /// the artifact accumulator and does NOT touch `messages`.
    /// Mirrors
    /// [`super::mapping::agent_event_to_stream_responses`]'s
    /// `ContentPart::Resource` arm.
    pub(crate) fn record_resource_link(&mut self, link: &ResourceLink) {
        self.artifacts.push(resource_link_to_artifact(link));
    }

    /// Record a tangible image artifact (e.g. an inline image
    /// the agent produced). Mirrors `ContentPart::Image` arm.
    pub(crate) fn record_image(&mut self, image: &ImageContent) {
        self.artifacts.push(image_to_artifact(image));
    }

    /// Record a tangible audio artifact. Mirrors
    /// `ContentPart::Audio` arm.
    pub(crate) fn record_audio(&mut self, audio: &AudioContent) {
        self.artifacts.push(audio_to_artifact(audio));
    }

    /// Drain the artifacts accumulated so far. Returns
    /// `Some(Vec<Artifact>)` only if at least one was recorded
    /// — empty artifact lists are dropped from `Task.artifacts`
    /// to keep the persisted task shape clean.
    pub(crate) fn take_artifacts(&mut self) -> Option<Vec<Artifact>> {
        if self.artifacts.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.artifacts))
        }
    }

    /// Drain both messages AND artifacts at once. The caller
    /// is expected to attach them to the terminal `Task`:
    /// `messages` → `task.history`, `artifacts` → `task.artifacts`.
    pub(crate) fn take_transcript(
        &mut self,
    ) -> (Vec<Message>, Option<Vec<Artifact>>) {
        self.flush_pending_text();
        let messages = std::mem::take(&mut self.messages);
        let artifacts = self.take_artifacts();
        (messages, artifacts)
    }
}

/// Build a `Message` carrying a single `Part::text`.
fn text_message(role: Role, text: &str) -> Message {
    Message::new(role, vec![Part::text(text.to_string())])
}

/// Build a `Message` carrying a single `Part::data` whose JSON
/// payload is the supplied [`serde_json::Value`]. The frontend
/// reads `Part::data` payloads as opaque JSON and dispatches
/// on the natural shape — `id` + `name` + `input` is a tool
/// call, `tool_use_id` + `content` + `is_error` is a tool
/// result. There is no synthetic `kind` wrapper.
fn data_message(role: Role, payload: serde_json::Value) -> Message {
    Message::new(role, vec![Part::data(payload)])
}

/// Persist the builder's transcript to the task store. The
/// executor calls this exactly once on terminal
/// (`SystemEvent::SessionEnded`).
///
/// Errors are logged but never propagate — the live stream has
/// already ended, the caller cannot retry, and the worst case
/// is the same degraded transcript the upstream A2A layer
/// would have produced without our help.
#[cfg(test)]
mod tests {
    use super::*;

    fn tool_use(id: &str, name: &str) -> ToolUse {
        ToolUse {
            id: id.to_string(),
            name: name.to_string(),
            input: json!({"x": 1}),
        }
    }

    fn tool_result(id: &str, text: &str) -> ToolResult {
        ToolResult::new(id, text)
    }

    #[test]
    fn empty_prompt_is_skipped() {
        let mut b = TaskHistoryBuilder::new();
        b.record_user_prompt("");
        assert!(b.into_messages().is_empty());
    }

    #[test]
    fn text_deltas_coalesce_into_single_agent_message() {
        let mut b = TaskHistoryBuilder::new();
        b.record_user_prompt("hello");
        b.record_text_delta("foo ");
        b.record_text_delta("bar");
        let msgs = b.into_messages();
        assert_eq!(msgs.len(), 2, "user + coalesced agent");
        // First message is the user prompt.
        assert!(matches!(msgs[0].role, Role::User));
        // Second message is the agent text — its parts are
        // the coalesced delta string.
        let json = serde_json::to_value(&msgs[1]).unwrap();
        let parts = json.get("parts").and_then(|v| v.as_array()).unwrap();
        assert_eq!(parts.len(), 1);
        // A2A v1.0 `Part::text` is serialised with field-presence:
        // the `text` field appears directly on the Part object
        // (no `content` / `type` wrapper). We only assert the
        // text shows up, not the exact wrapping, so a future
        // spec revision doesn't break this test.
        let serialised = json.to_string();
        assert!(serialised.contains("foo bar"), "got: {serialised}");
    }

    #[test]
    fn tool_use_flushes_text_then_emits_call_message() {
        let mut b = TaskHistoryBuilder::new();
        b.record_user_prompt("hi");
        b.record_text_delta("thinking...");
        b.record_tool_use(&tool_use("c1", "web_fetch"));
        b.record_text_delta("after tool");
        let msgs = b.into_messages();
        // user, agent text (thinking), agent tool_call,
        // agent text (after).
        assert_eq!(msgs.len(), 4);
    }

    #[test]
    fn model_done_replaces_buffered_text_with_aggregated() {
        let mut b = TaskHistoryBuilder::new();
        b.record_user_prompt("hi");
        b.record_text_delta("foo");
        b.record_model_done(&synthia_provider::SamplingResult {
            text: "FOO".to_string(),
            tool_calls: vec![],
            reasoning: String::new(),
            reasoning_signature: None,
            usage: synthia_provider::TokenUsage::default(),
            stop_reason: None,
        });
        let msgs = b.into_messages();
        assert_eq!(msgs.len(), 2);
        // The agent text message should carry the aggregated
        // value, not the buffered "foo".
        let json = serde_json::to_value(&msgs[1]).unwrap();
        assert!(json.to_string().contains("FOO"));
        assert!(!json.to_string().contains("\"foo\""));
    }

    #[test]
    fn tool_result_carries_preview_text_and_is_error() {
        let mut b = TaskHistoryBuilder::new();
        b.record_user_prompt("hi");
        b.record_tool_use(&tool_use("c1", "shell"));
        b.record_tool_result(&tool_result("c1", "exit: 0"));
        let msgs = b.into_messages();
        assert_eq!(msgs.len(), 3);
        let json = serde_json::to_value(&msgs[2]).unwrap();
        let s = json.to_string();
        // Natural ToolResult shape: `tool_use_id` + `content`
        // + `is_error`. No synthetic `kind` wrapper.
        assert!(s.contains("tool_use_id"), "missing tool_use_id: {s}");
        assert!(s.contains("exit: 0"), "missing preview: {s}");
        assert!(!s.contains("\"kind\""), "must not carry kind: {s}");
    }
}
