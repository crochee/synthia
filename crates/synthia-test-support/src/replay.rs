//! Keyless deterministic replay of recorded model-call scripts.
//!
//! Ports the `deepseek-harness` `llm-replay` design to the
//! single-provider shape synthia's tests need: a
//! [`ReplayProvider`] is constructed from per-run scripts
//! (`Vec<Vec<StreamChunk>>`, one entry per model call) or from a
//! recorded session `events.jsonl` fixture, then hands one
//! script to each `complete` / `complete_with_stream` call.
//!
//! Two properties make it a test tool rather than a mock:
//!
//! - **No silent underruns.** A call with no script left is a
//!   hard error, and [`ReplayProvider::assert_consumed`] turns
//!   "the scenario made fewer calls than were recorded" into a
//!   crisp teardown diagnostic, including calls that were bound
//!   but left undelivered (e.g. the agent cancelled mid-stream).
//! - **Optional pacing.** [`ReplayProvider::with_pacing`] sleeps
//!   before each delivered chunk so downstream consumers observe
//!   genuinely incremental delivery. Correctness never depends on
//!   it.
//!
//! # Example
//!
//! ```rust
//! use synthia_provider::{
//!     CompletionRequest,
//!     ContentPart,
//!     ModelProvider,
//!     SamplingResult,
//!     StreamChunk,
//!     TextContent,
//! };
//! use synthia_test_support::ReplayProvider;
//!
//! # async fn example() -> Result<(), synthia_core::Error> {
//! let provider = ReplayProvider::new(vec![vec![
//!     StreamChunk::Content(ContentPart::Text(TextContent {
//!         text: "hello".to_string(),
//!         cache_control: None,
//!     })),
//!     StreamChunk::IsDone {
//!         result: Box::new(SamplingResult {
//!             text: "hello".to_string(),
//!             stop_reason: Some("end_turn".to_string()),
//!             ..Default::default()
//!         }),
//!     },
//! ]]);
//!
//! let response = provider.complete(CompletionRequest::default()).await?;
//! assert_eq!(response.stop_reason.as_deref(), Some("end_turn"));
//! provider
//!     .assert_consumed()
//!     .expect("scenario used every script");
//! # Ok(())
//! # }
//! ```

use std::{collections::VecDeque, sync::Arc, time::Duration};

use async_trait::async_trait;
use parking_lot::Mutex;
use serde_json::Value;
use synthia_core::{CancelToken, Error};
use synthia_provider::{
    CompletionRequest,
    CompletionResponse,
    Content,
    ContentPart,
    ModelConfig,
    ModelProvider,
    ProviderConfig,
    SamplingResult,
    StreamChunk,
    TextContent,
    ToolUse,
};
use thiserror::Error;

/// Errors raised while building or checking a [`ReplayProvider`].
#[derive(Debug, Error)]
pub enum ReplayError {
    /// A session-events fixture could not be turned into call
    /// scripts.
    #[error("replay fixture error: {0}")]
    Fixture(String),
    /// [`ReplayProvider::assert_consumed`] found scripts that were
    /// never bound to a call, or bound calls that did not deliver
    /// every recorded chunk.
    #[error("replay not fully consumed: {0}")]
    NotConsumed(String),
}

/// Bookkeeping for one bound call: chunks not yet delivered.
#[derive(Debug)]
struct BoundCall {
    remaining: usize,
}

/// Shared replay cursor state.
#[derive(Debug, Default)]
struct ReplayState {
    /// Scripts not yet bound to a call, in recorded order.
    pending: VecDeque<Vec<StreamChunk>>,
    /// One entry per bound call, in call order.
    bound: Vec<BoundCall>,
    /// Number of `complete*` calls issued so far.
    calls: usize,
    /// Total scripts the provider was constructed with.
    recorded: usize,
}

/// Deterministic, keyless [`ModelProvider`] that replays recorded
/// model-call scripts in order.
///
/// Each `complete` / `complete_with_stream` call binds the next
/// unbound script and advances a cursor through its chunks. The
/// provider never touches the network and never reads the clock
/// (except for the optional pacing sleeps).
pub struct ReplayProvider {
    state: Arc<Mutex<ReplayState>>,
    pace: Option<Duration>,
}

impl std::fmt::Debug for ReplayProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.state.lock();
        f.debug_struct("ReplayProvider")
            .field("recorded", &state.recorded)
            .field("calls", &state.calls)
            .field("pending", &state.pending.len())
            .field("pace", &self.pace)
            .finish()
    }
}

impl ReplayProvider {
    /// New provider replaying `scripts` in order — one chunk list
    /// per model call.
    ///
    /// Each script should end with `StreamChunk::IsDone` (the
    /// canonical terminal marker); scripts without one still fold
    /// into a response from their content chunks.
    #[must_use]
    pub fn new(scripts: Vec<Vec<StreamChunk>>) -> Self {
        Self {
            state: Arc::new(Mutex::new(ReplayState {
                recorded: scripts.len(),
                pending: scripts.into(),
                ..ReplayState::default()
            })),
            pace: None,
        }
    }

    /// Build a provider from a session `events.jsonl` fixture.
    ///
    /// The fixture is the durable event-log format written by
    /// `synthia-session`: one JSON record per line, where
    /// `assistant_chunk` records carry a text delta under
    /// `data.delta` and the terminal record of a model call
    /// carries `finish_reason`. Non-chunk records (the session
    /// header, user/assistant messages, tool results, …) are
    /// skipped. Each call becomes one script terminated by
    /// `Stop(finish_reason)` + `IsDone`.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayError::Fixture`] when a line is not valid
    /// JSON, or when the log ends with chunks that never reached
    /// a `finish_reason` (a thrown/aborted stream cannot be
    /// reconstructed from the log alone — the scenario must pass
    /// an explicit script instead).
    pub fn from_events_jsonl(contents: &str) -> Result<Self, ReplayError> {
        Ok(Self::new(derive_scripts(contents)?))
    }

    /// Sleep `per_chunk` before delivering each chunk.
    ///
    /// Realism knob only: correctness must never depend on it.
    /// The default (no pacing) delivers chunks as fast as the
    /// consumer accepts them.
    #[must_use]
    pub fn with_pacing(mut self, per_chunk: Duration) -> Self {
        self.pace = Some(per_chunk);
        self
    }

    /// Whether the provider was constructed with at least one
    /// script (a fixture without model calls yields `false`).
    #[must_use]
    pub fn has_scripts(&self) -> bool {
        self.state.lock().recorded > 0
    }

    /// Assert the scenario consumed every recorded script in
    /// full.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayError::NotConsumed`] listing the number of
    /// scripts that were never bound to a call and every bound
    /// call that left chunks undelivered (e.g. cancellation
    /// mid-stream). Call this at scenario teardown.
    pub fn assert_consumed(&self) -> Result<(), ReplayError> {
        let state = self.state.lock();
        let unbound = state.pending.len();
        let undelivered: Vec<String> = state
            .bound
            .iter()
            .enumerate()
            .filter(|(_, call)| call.remaining > 0)
            .map(|(idx, call)| {
                format!(
                    "call {} left {} chunk(s) undelivered",
                    idx + 1,
                    call.remaining
                )
            })
            .collect();
        if unbound == 0 && undelivered.is_empty() {
            return Ok(());
        }
        let mut problems = Vec::new();
        if unbound > 0 {
            problems.push(format!(
                "{unbound} script(s) were never bound to a call"
            ));
        }
        if !undelivered.is_empty() {
            problems.push(undelivered.join("; "));
        }
        Err(ReplayError::NotConsumed(problems.join("; ")))
    }

    /// Bind the next unbound script to a call, returning the call
    /// ordinal (1-based) and a copy of the script.
    fn bind_next_script(&self) -> Result<(usize, Vec<StreamChunk>), Error> {
        let mut state = self.state.lock();
        state.calls += 1;
        let call_no = state.calls;
        let Some(chunks) = state.pending.pop_front() else {
            return Err(Error::provider(format!(
                "replay provider exhausted: call {call_no} requested \
                 but no script remains ({} of {} script(s) already \
                 bound)",
                state.bound.len(),
                state.recorded,
            )));
        };
        state.bound.push(BoundCall {
            remaining: chunks.len(),
        });
        Ok((call_no, chunks))
    }

    /// Note that one chunk of `call_no` reached the consumer.
    fn note_delivered(&self, call_no: usize) {
        let mut state = self.state.lock();
        if let Some(call) = state.bound.get_mut(call_no - 1) {
            call.remaining = call.remaining.saturating_sub(1);
        }
    }

    /// Mark every chunk of `call_no` delivered (the non-streaming
    /// path aggregates the script itself).
    fn mark_all_delivered(&self, call_no: usize) {
        let mut state = self.state.lock();
        if let Some(call) = state.bound.get_mut(call_no - 1) {
            call.remaining = 0;
        }
    }

    /// Chunks of `call_no` not yet delivered.
    fn remaining_chunks(&self, call_no: usize) -> usize {
        self.state
            .lock()
            .bound
            .get(call_no - 1)
            .map(|call| call.remaining)
            .unwrap_or(0)
    }

    /// Deliver `chunks` in order through `on_delta`, honouring
    /// optional pacing and cancellation.
    ///
    /// Cancellation mid-replay leaves the remaining chunks
    /// undelivered, which [`Self::assert_consumed`] reports.
    async fn replay_script(
        &self,
        call_no: usize,
        chunks: &[StreamChunk],
        cancel: Option<&Arc<dyn CancelToken>>,
        mut on_delta: impl FnMut(StreamChunk),
    ) -> Result<(), Error> {
        for chunk in chunks {
            if let Some(token) = cancel
                && token.is_cancelled()
            {
                return Err(Error::StreamAborted {
                    message: format!(
                        "replay call {call_no} cancelled with {} \
                         chunk(s) undelivered",
                        self.remaining_chunks(call_no),
                    ),
                });
            }
            if let Some(pace) = self.pace {
                tokio::time::sleep(pace).await;
            }
            on_delta(chunk.clone());
            self.note_delivered(call_no);
        }
        Ok(())
    }
}

#[async_trait]
impl ModelProvider for ReplayProvider {
    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "replay"
    }

    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: "replay-model".to_string(),
            provider: "replay".to_string(),
            context_window: 128_000,
            max_output_tokens: 4_096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        }
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, Error> {
        let (call_no, chunks) = self.bind_next_script()?;
        let sampling = fold_chunks(&chunks);
        self.mark_all_delivered(call_no);
        Ok(response_for(call_no, &sampling))
    }

    async fn complete_with_stream(
        &self,
        _request: CompletionRequest,
        cancel_token: Option<Arc<dyn CancelToken>>,
        mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<CompletionResponse, Error> {
        let (call_no, chunks) = self.bind_next_script()?;
        self.replay_script(call_no, &chunks, cancel_token.as_ref(), |chunk| {
            on_delta(chunk);
        })
        .await?;
        let sampling = fold_chunks(&chunks);
        Ok(response_for(call_no, &sampling))
    }
}

/// Derive one script per model call from a session `events.jsonl`
/// fixture.
fn derive_scripts(
    contents: &str,
) -> Result<Vec<Vec<StreamChunk>>, ReplayError> {
    let mut scripts = Vec::new();
    let mut current: Vec<StreamChunk> = Vec::new();
    let mut current_text = String::new();

    for (line_no, raw) in contents.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let record: Value = serde_json::from_str(line).map_err(|e| {
            ReplayError::Fixture(format!(
                "line {}: invalid JSON: {e}",
                line_no + 1
            ))
        })?;
        if record.get("type").and_then(Value::as_str) != Some("assistant_chunk")
        {
            continue;
        }
        let closed =
            push_chunk_record(&record, &mut current, &mut current_text);
        if closed {
            scripts.push(std::mem::take(&mut current));
            current_text.clear();
        }
    }

    if !current.is_empty() {
        return Err(ReplayError::Fixture(
            "model call ended without a finish_reason chunk; a thrown or \
             aborted stream cannot be derived from the log — pass an \
             explicit script instead"
                .to_string(),
        ));
    }
    Ok(scripts)
}

/// Project one `assistant_chunk` record onto script chunks.
/// Returns `true` when the record carried a `finish_reason` and
/// therefore closes the current script.
fn push_chunk_record(
    record: &Value,
    current: &mut Vec<StreamChunk>,
    current_text: &mut String,
) -> bool {
    let delta = record
        .get("data")
        .and_then(|data| data.get("delta"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if !delta.is_empty() {
        current_text.push_str(delta);
        current.push(text_chunk(delta));
    }
    let Some(reason) = record.get("finish_reason").and_then(Value::as_str)
    else {
        return false;
    };
    current.push(StreamChunk::Stop(reason.to_string()));
    current.push(StreamChunk::IsDone {
        result: Box::new(SamplingResult {
            text: current_text.clone(),
            stop_reason: Some(reason.to_string()),
            ..SamplingResult::default()
        }),
    });
    true
}

fn text_chunk(text: &str) -> StreamChunk {
    StreamChunk::Content(ContentPart::Text(TextContent {
        text: text.to_string(),
        cache_control: None,
    }))
}

/// Fold a script into the final sampling result it represents.
///
/// A terminal `IsDone` is authoritative (last one wins — mirrors
/// how `re_act` treats streamed chunks versus the final result).
/// Without one, content/usage/stop chunks are accumulated, and
/// tool-call deltas are materialised into `ToolUse` entries the
/// way the agent's stream ingest does.
fn fold_chunks(chunks: &[StreamChunk]) -> SamplingResult {
    let mut folded = SamplingResult::default();
    let mut buffers: Vec<(String, String, String)> = Vec::new();
    let mut done: Option<SamplingResult> = None;

    for chunk in chunks {
        match chunk {
            StreamChunk::Content(ContentPart::Text(t)) => {
                folded.text.push_str(&t.text);
            }
            StreamChunk::Content(ContentPart::Reasoning(r)) => {
                folded.reasoning.push_str(&r.text);
                if r.signature.is_some() {
                    folded.reasoning_signature = r.signature.clone();
                }
            }
            StreamChunk::Content(ContentPart::ToolUse(tool)) => {
                folded.tool_calls.push(tool.clone());
            }
            StreamChunk::Content(_) => {}
            StreamChunk::Usage(usage) => folded.usage = usage.clone(),
            StreamChunk::Stop(reason) => {
                folded.stop_reason = Some(reason.clone());
            }
            StreamChunk::ToolCallStart {
                id,
                name,
                arguments,
            } => {
                buffers.push((
                    id.clone(),
                    name.clone(),
                    initial_args(arguments),
                ));
            }
            StreamChunk::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                if let Some(buf) = buffers.iter_mut().find(|buf| buf.0 == *id) {
                    buf.2.push_str(arguments_delta);
                }
            }
            StreamChunk::ToolCallEnd { id } => {
                if let Some((call_id, name, raw)) =
                    take_buffer(&mut buffers, id)
                {
                    folded.tool_calls.push(ToolUse {
                        id: call_id,
                        name,
                        input: parse_tool_input(&raw),
                    });
                }
            }
            StreamChunk::IsDone { result } => {
                done = Some((**result).clone());
            }
        }
    }

    // Streams that end mid-tool-call still surface the partial
    // buffer, matching the agent's drain-on-termination behavior.
    for (id, name, raw) in buffers {
        folded.tool_calls.push(ToolUse {
            id,
            name,
            input: parse_tool_input(&raw),
        });
    }

    done.unwrap_or(folded)
}

/// Remove and return the buffered tool call for `id`, if any.
fn take_buffer(
    buffers: &mut Vec<(String, String, String)>,
    id: &str,
) -> Option<(String, String, String)> {
    let pos = buffers.iter().position(|buf| buf.0 == id)?;
    Some(buffers.remove(pos))
}

/// Initial raw argument text for a `ToolCallStart` chunk.
fn initial_args(arguments: &Value) -> String {
    match arguments {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Parse a tool input JSON string; empty input yields `{}`,
/// unparseable input yields the raw string (mirrors
/// `re_act::parse_tool_input`).
fn parse_tool_input(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Value::Object(serde_json::Map::new());
    }
    serde_json::from_str(trimmed)
        .unwrap_or_else(|_| Value::String(raw.to_string()))
}

/// Build the `CompletionResponse` for one replayed call.
fn response_for(
    call_no: usize,
    sampling: &SamplingResult,
) -> CompletionResponse {
    let mut parts: Vec<ContentPart> = Vec::new();
    if !sampling.text.is_empty() || sampling.tool_calls.is_empty() {
        parts.push(ContentPart::Text(TextContent {
            text: sampling.text.clone(),
            cache_control: None,
        }));
    }
    for call in &sampling.tool_calls {
        parts.push(ContentPart::ToolUse(call.clone()));
    }
    let content = if parts.len() == 1 {
        Content::Single(parts.swap_remove(0))
    } else {
        Content::Multi(parts)
    };
    CompletionResponse {
        id: format!("replay-{call_no}"),
        model: "replay-model".to_string(),
        content,
        usage: sampling.usage.clone(),
        cached: false,
        stop_reason: sampling.stop_reason.clone(),
        replay_state: None,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use synthia_core::AtomicCancelToken;
    use synthia_provider::{ReasoningContent, TokenUsage};

    use super::*;

    fn done(text: &str, reason: &str) -> StreamChunk {
        StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                text: text.to_string(),
                stop_reason: Some(reason.to_string()),
                ..SamplingResult::default()
            }),
        }
    }

    type CollectedChunks = Arc<Mutex<Vec<StreamChunk>>>;

    fn collector() -> (CollectedChunks, Box<dyn FnMut(StreamChunk) + Send>) {
        let seen: CollectedChunks = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        (seen, Box::new(move |chunk| sink.lock().push(chunk)))
    }

    async fn stream_once(
        provider: &ReplayProvider,
    ) -> Result<CompletionResponse, Error> {
        let (_, on_delta) = collector();
        provider
            .complete_with_stream(CompletionRequest::default(), None, on_delta)
            .await
    }

    fn response_text(response: &CompletionResponse) -> String {
        match &response.content {
            Content::Single(ContentPart::Text(t)) => t.text.clone(),
            Content::Multi(parts) => {
                let mut text = String::new();
                for part in parts {
                    if let ContentPart::Text(t) = part {
                        text.push_str(&t.text);
                    }
                }
                text
            }
            _ => String::new(),
        }
    }

    #[tokio::test]
    async fn scripts_replay_in_order_and_advance_per_call() {
        let provider = ReplayProvider::new(vec![
            vec![text_chunk("ignored"), done("hello", "end_turn")],
            vec![text_chunk("bye"), done("bye", "stop")],
        ]);

        let (seen_a, on_a) = collector();
        let first = provider
            .complete_with_stream(CompletionRequest::default(), None, on_a)
            .await
            .unwrap();
        let delivered_a = seen_a.lock().clone();
        assert_eq!(delivered_a.len(), 2);
        assert!(
            matches!(
                &delivered_a[0],
                StreamChunk::Content(ContentPart::Text(t)) if t.text == "ignored"
            ),
            "first call must deliver its own script first"
        );
        assert!(matches!(&delivered_a[1], StreamChunk::IsDone { .. }));
        // `IsDone` is authoritative over the streamed text.
        assert_eq!(response_text(&first), "hello");
        assert_eq!(first.stop_reason.as_deref(), Some("end_turn"));
        assert_eq!(first.id, "replay-1");
        assert_eq!(first.model, "replay-model");

        let (seen_b, on_b) = collector();
        let second = provider
            .complete_with_stream(CompletionRequest::default(), None, on_b)
            .await
            .unwrap();
        assert_eq!(seen_b.lock().len(), 2);
        assert_eq!(response_text(&second), "bye");
        assert_eq!(second.id, "replay-2");

        provider.assert_consumed().unwrap();
        assert_eq!(provider.remaining_chunks(1), 0);
        assert_eq!(provider.remaining_chunks(2), 0);
    }

    #[tokio::test]
    async fn call_beyond_recorded_scripts_is_an_error() {
        let provider = ReplayProvider::new(vec![vec![done("only", "stop")]]);
        stream_once(&provider).await.unwrap();
        let err = stream_once(&provider).await.unwrap_err();
        assert!(
            err.to_string().contains("exhausted"),
            "error should name the underrun: {err}"
        );
        // The one recorded script was still fully consumed.
        provider.assert_consumed().unwrap();
    }

    #[tokio::test]
    async fn assert_consumed_reports_never_bound_scripts() {
        let provider = ReplayProvider::new(vec![
            vec![done("first", "stop")],
            vec![done("second", "stop")],
        ]);
        stream_once(&provider).await.unwrap();
        let err = provider.assert_consumed().unwrap_err();
        assert!(matches!(err, ReplayError::NotConsumed(_)), "got: {err}");
        let message = err.to_string();
        assert!(message.contains("never bound"), "message: {message}");
        assert!(message.contains('1'), "message: {message}");
    }

    #[tokio::test]
    async fn assert_consumed_reports_cancelled_undrained_call() {
        let provider = ReplayProvider::new(vec![vec![
            text_chunk("a"),
            text_chunk("b"),
            done("ab", "stop"),
        ]]);
        let token = AtomicCancelToken::shared();
        token.cancel();
        let (_, on_delta) = collector();
        let err = provider
            .complete_with_stream(
                CompletionRequest::default(),
                Some(token),
                on_delta,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, Error::StreamAborted { .. }), "got: {err}");

        let consumed = provider.assert_consumed().unwrap_err();
        let message = consumed.to_string();
        assert!(message.contains("undelivered"), "message: {message}");
        assert!(message.contains('3'), "message: {message}");
    }

    #[tokio::test]
    async fn failure_diagnostics_accumulate_across_calls() {
        let provider = ReplayProvider::new(vec![
            vec![text_chunk("x"), text_chunk("y")],
            vec![done("never used", "stop")],
        ]);
        let token = AtomicCancelToken::shared();
        token.cancel();
        let (_, on_delta) = collector();
        let _ = provider
            .complete_with_stream(
                CompletionRequest::default(),
                Some(token),
                on_delta,
            )
            .await;
        let message = provider.assert_consumed().unwrap_err().to_string();
        assert!(message.contains("never bound"), "message: {message}");
        assert!(message.contains("undelivered"), "message: {message}");
    }

    #[tokio::test]
    async fn non_streaming_complete_aggregates_and_drains_script() {
        let provider = ReplayProvider::new(vec![vec![
            StreamChunk::ToolCallStart {
                id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: Value::String(String::new()),
            },
            StreamChunk::ToolCallDelta {
                id: "call-1".to_string(),
                arguments_delta: "{\"cmd\":".to_string(),
            },
            StreamChunk::ToolCallDelta {
                id: "call-1".to_string(),
                arguments_delta: "\"ls\"}".to_string(),
            },
            StreamChunk::ToolCallEnd {
                id: "call-1".to_string(),
            },
            StreamChunk::Stop("tool_use".to_string()),
        ]]);
        let response = provider
            .complete(CompletionRequest::default())
            .await
            .unwrap();
        assert_eq!(response.stop_reason.as_deref(), Some("tool_use"));
        match &response.content {
            Content::Single(ContentPart::ToolUse(call)) => {
                assert_eq!(call.name, "bash");
                assert_eq!(call.input, serde_json::json!({"cmd": "ls"}));
            }
            other => panic!("expected a single tool-use part, got {other:?}"),
        }
        provider.assert_consumed().unwrap();
    }

    #[tokio::test]
    async fn folded_text_and_usage_surface_without_isdone() {
        let provider = ReplayProvider::new(vec![vec![
            StreamChunk::Content(ContentPart::Reasoning(ReasoningContent {
                text: "thinking".to_string(),
                signature: Some("sig".to_string()),
            })),
            text_chunk("part one "),
            text_chunk("part two"),
            StreamChunk::Usage(TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 4,
                total_tokens: 14,
                ..TokenUsage::default()
            }),
        ]]);
        let response = provider
            .complete(CompletionRequest::default())
            .await
            .unwrap();
        assert_eq!(response_text(&response), "part one part two");
        assert_eq!(response.usage.total_tokens, 14);
        assert_eq!(response.stop_reason, None);
    }

    #[tokio::test]
    async fn fixture_derives_one_script_per_model_call() {
        let fixture = concat!(
            r#"{"session_id":"s1","created_at":"2026-09-12T00:00:00Z"}"#,
            "\n",
            r#"{"type":"user_message","seq":1,"ts":"t","data":{"text":"hi"}}"#,
            "\n",
            r#"{"type":"assistant_chunk","seq":2,"ts":"t","chunk_seq":0,"data":{"delta":"Hel"}}"#,
            "\n",
            r#"{"type":"assistant_chunk","seq":3,"ts":"t","chunk_seq":1,"data":{"delta":"lo"}}"#,
            "\n",
            r#"{"type":"assistant_chunk","seq":4,"ts":"t","chunk_seq":2,"finish_reason":"end_turn","data":{"delta":""}}"#,
            "\n",
            r#"{"type":"tool_result","seq":5,"ts":"t","data":{"text":"ok"}}"#,
            "\n",
            r#"{"type":"assistant_chunk","seq":6,"ts":"t","chunk_seq":0,"data":{"delta":"again"}}"#,
            "\n",
            r#"{"type":"assistant_chunk","seq":7,"ts":"t","chunk_seq":1,"finish_reason":"end_turn","data":{"delta":""}}"#,
            "\n",
        );
        let provider = ReplayProvider::from_events_jsonl(fixture).unwrap();

        let first = stream_once(&provider).await.unwrap();
        assert_eq!(response_text(&first), "Hello");
        assert_eq!(first.stop_reason.as_deref(), Some("end_turn"));

        let second = stream_once(&provider).await.unwrap();
        assert_eq!(response_text(&second), "again");

        // A third call is an underrun: two calls were recorded.
        assert!(stream_once(&provider).await.is_err());
        provider.assert_consumed().unwrap();
    }

    #[test]
    fn fixture_rejects_unterminated_call() {
        let fixture = concat!(
            r#"{"type":"assistant_chunk","seq":1,"ts":"t","chunk_seq":0,"data":{"delta":"cut"}}"#,
            "\n",
        );
        let err = ReplayProvider::from_events_jsonl(fixture).unwrap_err();
        assert!(matches!(err, ReplayError::Fixture(_)), "got: {err}");
        assert!(err.to_string().contains("finish_reason"), "got: {err}");
    }

    #[test]
    fn fixture_rejects_malformed_line() {
        let fixture = concat!(
            r#"{"type":"assistant_chunk","seq":1,"ts":"t","chunk_seq":0,"data":{"delta":"x"}}"#,
            "\n",
            "not json\n",
        );
        let err = ReplayProvider::from_events_jsonl(fixture).unwrap_err();
        assert!(err.to_string().contains("line 2"), "got: {err}");
    }

    #[test]
    fn fixture_without_model_calls_yields_no_scripts() {
        let fixture = "\n{\"session_id\":\"s1\"}\n{\"type\":\"tool_result\",\"seq\":1,\"ts\":\"t\",\"data\":{}}\n";
        let provider = ReplayProvider::from_events_jsonl(fixture).unwrap();
        assert!(!provider.has_scripts());
        provider.assert_consumed().unwrap();
    }

    #[tokio::test]
    async fn pacing_delays_each_delivered_chunk() {
        let provider = ReplayProvider::new(vec![vec![
            text_chunk("a"),
            text_chunk("b"),
            done("ab", "stop"),
        ]])
        .with_pacing(Duration::from_millis(20));

        let start = Instant::now();
        stream_once(&provider).await.unwrap();
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(60),
            "three paced chunks must take at least 3 x 20ms, took {elapsed:?}"
        );
        provider.assert_consumed().unwrap();
    }

    /// `ReplayProvider::embed` falls through to the trait's default
    /// implementation (empty vectors), since replay exists to
    /// reproduce scripted `complete` calls — embeddings have no
    /// replay script. This pins the contract so a future override
    /// that hardcodes an error is a deliberate decision, not
    /// accidental.
    #[tokio::test]
    async fn embed_returns_empty_vectors() {
        let provider = ReplayProvider::new(vec![]);
        let vectors = provider
            .embed(vec!["x".to_string(), "y".to_string()])
            .await
            .unwrap();
        assert_eq!(vectors.len(), 2);
        assert!(vectors.iter().all(|v| v.is_empty()));
    }

    #[tokio::test]
    async fn provider_metadata_reflects_replay_identity() {
        let provider = ReplayProvider::new(vec![vec![done("x", "stop")]]);
        assert_eq!(provider.name(), "replay");
        assert!(provider.has_scripts());
        assert!(!ReplayProvider::new(vec![]).has_scripts());
        let config = provider.model_config();
        assert_eq!(config.name, "replay-model");
        assert!(config.supports_streaming);
        assert!(config.supports_tools);
    }
    #[test]
    fn delivery_counter_tracks_each_chunk() {
        let provider =
            ReplayProvider::new(vec![vec![text_chunk("a"), text_chunk("b")]]);
        let (call_no, _) = provider.bind_next_script().unwrap();
        assert_eq!(call_no, 1);
        assert_eq!(provider.remaining_chunks(1), 2);
        provider.note_delivered(1);
        assert_eq!(provider.remaining_chunks(1), 1);
        provider.mark_all_delivered(1);
        assert_eq!(provider.remaining_chunks(1), 0);
        provider.assert_consumed().unwrap();
    }

    #[test]
    fn call_ordinals_increment_across_binds() {
        let provider = ReplayProvider::new(vec![
            vec![done("a", "stop")],
            vec![done("b", "stop")],
        ]);
        let (first, _) = provider.bind_next_script().unwrap();
        let (second, _) = provider.bind_next_script().unwrap();
        assert_eq!(first, 1);
        assert_eq!(second, 2);
    }
}
