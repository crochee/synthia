//! [`BlockAssembler`] — incremental chunk-to-message builder.
//!
//! dsh `packages/llm/llm/src/assembler.ts` parity (R6-1).
//!
//! ## Purpose
//!
//! Synthia's `ModelProvider::complete_with_stream` returns a
//! `Stream<Result<StreamChunk, Error>>`. The agent loop needs to
//! fold that stream into a final [`SamplingResult`] (text +
//! tool_calls + usage + stop reason) **and** a canonical list of
//! [`ContentPart`]s to feed the prompt builder. The previous
//! version of this folding logic lived inline in
//! `ReActLoop::sample_once` and `handle_chunk` (≈200 lines of
//! stream-state plumbing).
//!
//! [`BlockAssembler`] pulls that logic into a single
//! testable unit. Producers feed chunks; the loop reads
//! [`BlockAssembler::finalize_assistant`] once the stream ends.
//!
//! ## Why not just use `SamplingResult` from `IsDone`?
//!
//! Two reasons:
//!
//! 1. **Emission**: the agent loop wants to surface streamed
//!    chunks to the SSE event stream as they arrive
//!    (`AgentEvent::Model`). `BlockAssembler` keeps the
//!    accumulated parts so the loop can ship them without
//!    re-running the fold.
//! 2. **Stragglers**: `IsDone` re-lists every tool call the
//!    provider finalised, but the streamed `ToolCallEnd` chunks
//!    may have already pushed them into the parts vec. Without
//!    dedup the model-visible history ends up with duplicate
//!    tool calls. [`BlockAssembler::finalize_assistant`] pins
//!    the dedup contract.

use std::collections::{HashMap, HashSet};

use crate::{
    json_repair::parse_tool_input_logged,
    types::{
        ContentPart,
        ReasoningContent,
        SamplingResult,
        StreamChunk,
        TextContent,
        TokenUsage,
        ToolUse,
    },
};

/// One in-progress content block. Lives in [`BlockAssembler::partials`]
/// keyed by `StreamChunk::block_index()` (when the chunk carries
/// one) or the tool-call id (for the legacy `ToolCallStart` shape).
///
/// Open blocks carry their accumulated state (text + tool
/// [`BlockAssembler`] freezes the block
/// on `block-end` or `ToolCallEnd` so a straggler delta cannot
/// mutate it.
#[derive(Clone, Debug)]
enum PartialBlock {
    /// Open tool-use block — provider sends
    /// `ToolCallStart` / `ToolCallDelta` / `ToolCallEnd`.
    ToolUse {
        id: String,
        name: String,
        arguments: String,
        closed: bool,
    },
}

/// Fold a single provider stream into a final assistant
/// [`SamplingResult`] + canonical [`ContentPart`] list.
///
/// Construct one per LLM call. Feed every [`StreamChunk`] via
/// [`BlockAssembler::push`]; once the stream ends call
/// [`BlockAssembler::finalize_assistant`]. The
/// [`BlockAssembler`] is single-use: drop it after
/// `finalize_assistant` to free the partial buffer.
#[derive(Debug, Default)]
pub struct BlockAssembler {
    /// In-progress blocks keyed by either the chunk's
    /// `block_index()` (a numeric id the provider can stamp on
    /// each chunk) or the tool-call id for the legacy
    /// `ToolCallStart` / `ToolCallDelta` / `ToolCallEnd` shape.
    partials: HashMap<String, PartialBlock>,
    /// Stable insertion order so `finalize_assistant` reproduces
    /// the on-wire order even after dedup.
    order: Vec<String>,
    /// Accumulated text (concatenated `Content(Text)` deltas).
    text: String,
    /// Accumulated reasoning text (concatenated
    /// `Content(Reasoning)` deltas).
    reasoning: String,
    /// Whether at least one streamed text delta arrived. Used by
    /// the `IsDone` branch to decide whether to fold
    /// `result.text` in (only when no streamed text existed —
    /// non-streaming providers batch everything into `IsDone`).
    saw_streamed_text: bool,
    /// Same logic for reasoning.
    saw_streamed_reasoning: bool,
    /// The most recent `signature_delta` for the reasoning block.
    ///
    /// It arrives **after** the `thinking_delta`s that carried the text
    /// (that is the Anthropic event order), so the streamed
    /// [`ContentPart::Reasoning`] chunks cannot carry it — see
    /// `streaming::anthropic::processor`, which stamps them with
    /// whatever it has at that moment, i.e. `None`. It therefore reaches
    /// us on the terminal `IsDone` and is applied to the already-built
    /// parts in [`BlockAssembler::finalize_assistant`], where the parts
    /// are still ours to patch.
    ///
    /// Dropping it would silently break interleaved extended thinking:
    /// Anthropic rejects a re-sent thinking block without its signature,
    /// so the turn cannot be continued.
    reasoning_signature: Option<String>,
    /// `tool_use_id`s already pushed into `parts` from streamed
    /// `ToolCallEnd` chunks. The `IsDone` `result.tool_calls`
    /// re-listing is deduped against this set so we never push
    /// the same tool call twice.
    seen_tool_ids: HashSet<String>,
    /// Final usage from the `Usage` chunk; only the last one
    /// wins (Anthropic emits multiple; the terminal `message.usage`
    /// is authoritative).
    usage: Option<TokenUsage>,
    /// Provider-reported stop reason.
    stop_reason: Option<String>,
    /// Has `IsDone` arrived? After `finalize_assistant` returns,
    /// the loop expects this to be `true`.
    is_done: bool,
    /// Final assembled content parts in wire order.
    parts: Vec<ContentPart>,
}

impl BlockAssembler {
    /// Build a fresh assembler.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one chunk from the provider stream. Safe to call
    /// before, between, or after `IsDone` — `finalize_assistant`
    /// pins the ordering invariants.
    pub fn push(&mut self, chunk: StreamChunk) {
        match chunk {
            StreamChunk::Content(part) => match part {
                ContentPart::Text(tc) => {
                    self.saw_streamed_text = true;
                    self.text.push_str(&tc.text);
                    self.append_part(ContentPart::Text(tc));
                }
                ContentPart::Reasoning(rc) => {
                    self.saw_streamed_reasoning = true;
                    self.reasoning.push_str(&rc.text);
                    self.append_part(ContentPart::Reasoning(rc));
                }
                other => self.append_part(other),
            },
            StreamChunk::Usage(usage) => {
                // Anthropic emits intermediate `message_delta.usage`
                // before the terminal one. We let the last writer
                // win so the assembled result carries the
                // authoritative count.
                self.usage = Some(usage);
            }
            StreamChunk::Stop(reason) => {
                if self.stop_reason.is_none() {
                    self.stop_reason = Some(reason);
                }
            }
            StreamChunk::ToolCallStart {
                id,
                name,
                arguments,
            } => {
                let initial_args = match arguments {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                };
                if !self.partials.contains_key(&id) {
                    self.order.push(id.clone());
                }
                self.partials.insert(
                    id.clone(),
                    PartialBlock::ToolUse {
                        id,
                        name,
                        arguments: initial_args,
                        closed: false,
                    },
                );
            }
            StreamChunk::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                if let Some(PartialBlock::ToolUse { arguments, .. }) =
                    self.partials.get_mut(&id)
                {
                    arguments.push_str(&arguments_delta);
                }
            }
            StreamChunk::ToolCallEnd { id } => {
                if let Some(PartialBlock::ToolUse { closed, .. }) =
                    self.partials.get_mut(&id)
                {
                    *closed = true;
                }
                if let Some(PartialBlock::ToolUse {
                    id: tool_id,
                    name,
                    arguments,
                    ..
                }) = self.partials.get(&id).cloned()
                {
                    let parsed = parse_tool_input_logged(&arguments, &name);
                    let tool_use = ToolUse {
                        id: tool_id.clone(),
                        name,
                        input: parsed,
                    };
                    if !self.seen_tool_ids.contains(&tool_use.id) {
                        self.seen_tool_ids.insert(tool_use.id.clone());
                        self.append_part(ContentPart::ToolUse(tool_use));
                    }
                }
            }
            StreamChunk::IsDone { result } => {
                self.is_done = true;
                self.fold_is_done(*result);
            }
        }
    }

    /// True once [`StreamChunk::IsDone`] has been pushed.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.is_done
    }

    /// Read-only access to the assembled content parts. Useful
    /// for the agent loop's mid-stream event emission, which
    /// pushes the current view to the SSE stream on every chunk.
    #[must_use]
    pub fn parts(&self) -> &[ContentPart] {
        &self.parts
    }

    /// Accumulated text so far. Used by the loop to publish a
    /// "partial assistant text" event without re-walking
    /// `parts`.
    #[must_use]
    pub fn text_so_far(&self) -> &str {
        &self.text
    }

    /// Accumulated reasoning text so far.
    #[must_use]
    pub fn reasoning_so_far(&self) -> &str {
        &self.reasoning
    }

    /// Read-only view of the most recent `Usage` chunk.
    #[must_use]
    pub fn usage(&self) -> Option<&TokenUsage> {
        self.usage.as_ref()
    }

    /// Read-only view of the provider-reported stop reason.
    #[must_use]
    pub fn stop_reason(&self) -> Option<&str> {
        self.stop_reason.as_deref()
    }

    /// Finish the fold. Returns the assembled
    /// [`SamplingResult`] (consumed by the loop to publish
    /// `AgentEvent::System(Usage)` / `ModelDone`) **and** a
    /// canonical [`ContentPart`] vec (consumed by the history
    /// builder).
    ///
    /// Calling `finalize_assistant` before `IsDone` is allowed;
    /// the assembler returns the partial state it has so the
    /// loop can publish a cancellation message. The
    /// `incomplete` flag on the returned tuple signals the
    /// caller that the provider stream ended without a clean
    /// `IsDone`.
    #[must_use]
    pub fn finalize_assistant(
        &mut self,
    ) -> (SamplingResult, Vec<ContentPart>, bool) {
        // Drain any tool buffers the stream did not explicitly
        // close (e.g. a `ToolCallStart` followed by a stream
        // error). Synthia's previous code already handled this in
        // `finalize_buffers` — folded into the assembler so the
        // loop has one fewer helper to call.
        let drained_ids: Vec<String> = self
            .partials
            .iter()
            .filter_map(|(id, block)| match block {
                PartialBlock::ToolUse { closed: false, .. } => Some(id.clone()),
                _ => None,
            })
            .collect();
        for id in drained_ids {
            if let Some(PartialBlock::ToolUse {
                id: tool_id,
                name,
                arguments,
                ..
            }) = self.partials.get(&id).cloned()
            {
                let parsed = parse_tool_input_logged(&arguments, &name);
                let tool_use = ToolUse {
                    id: tool_id.clone(),
                    name,
                    input: parsed,
                };
                if !self.seen_tool_ids.contains(&tool_use.id) {
                    self.seen_tool_ids.insert(tool_use.id.clone());
                    self.append_part(ContentPart::ToolUse(tool_use));
                }
            }
        }

        // Stamp the deferred signature onto the reasoning parts that
        // were streamed without one.
        //
        // One thinking block produces *several* parts: a
        // `content_block_start` carrying the opening text plus one per
        // `thinking_delta`, and none of them can see the signature,
        // which arrives last.
        //
        // Only the **trailing contiguous** reasoning run is stamped.
        // That bound is load-bearing: on the streaming path *every*
        // reasoning part is un-signed (see `reasoning_signature`), so a
        // walk that kept going past a signed part — or past any
        // non-reasoning part — would hand the one latest signature to
        // every earlier block too, forging a signature onto text it was
        // never computed for. A signature is bound to the block it
        // signed.
        if let Some(signature) = self.reasoning_signature.clone()
            && let Some(last) = self
                .parts
                .iter()
                .rposition(|p| matches!(p, ContentPart::Reasoning(_)))
        {
            for part in self.parts[..=last].iter_mut().rev() {
                match part {
                    ContentPart::Reasoning(rc) if rc.signature.is_none() => {
                        rc.signature = Some(signature.clone());
                    }
                    // An already-signed reasoning part ends the run, as
                    // does anything that is not reasoning.
                    _ => break,
                }
            }
        }

        let tool_uses: Vec<ToolUse> = self
            .parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::ToolUse(tu) => Some(tu.clone()),
                _ => None,
            })
            .collect();

        let result = SamplingResult {
            text: self.text.clone(),
            tool_calls: tool_uses,
            reasoning: self.reasoning.clone(),
            reasoning_signature: self.reasoning_signature.clone(),
            usage: self.usage.clone().unwrap_or_default(),
            stop_reason: self.stop_reason.clone(),
        };
        let parts = std::mem::take(&mut self.parts);
        let incomplete = !self.is_done;
        (result, parts, incomplete)
    }

    // -- private helpers --------------------------------------------------

    /// Append a content part while preserving insertion order.
    /// Text/Reasoning already mutated `self.text` /
    /// `self.reasoning` in `push`; non-text parts go straight
    /// onto the parts vec.
    fn append_part(&mut self, part: ContentPart) {
        self.parts.push(part);
    }

    /// Fold an `IsDone` chunk's terminal `SamplingResult` into
    /// the accumulated state. Dedupes tool calls against the
    /// streamed `ToolCallEnd` ids; folds `result.text` only when
    /// no streamed text arrived (non-streaming providers batch
    /// everything into `IsDone`).
    fn fold_is_done(&mut self, result: SamplingResult) {
        if !self.saw_streamed_text && !result.text.is_empty() {
            self.text.push_str(&result.text);
            self.append_part(ContentPart::Text(TextContent {
                text: result.text.clone(),
                cache_control: None,
            }));
        }
        // The signature may arrive with reasoning text that was already
        // streamed (the usual shape) or with no text at all, so record
        // it regardless of whether the text branch below runs.
        if let Some(signature) = result.reasoning_signature.clone() {
            self.reasoning_signature = Some(signature);
        }
        if !self.saw_streamed_reasoning && !result.reasoning.is_empty() {
            let reasoning_text = result.reasoning.clone();
            self.reasoning.push_str(&reasoning_text);
            self.append_part(ContentPart::Reasoning(ReasoningContent {
                text: reasoning_text,
                signature: self.reasoning_signature.clone(),
            }));
        }
        for call in &result.tool_calls {
            if self.seen_tool_ids.contains(&call.id) {
                continue;
            }
            self.seen_tool_ids.insert(call.id.clone());
            self.append_part(ContentPart::ToolUse(call.clone()));
        }
        // `IsDone` carries the authoritative usage; let it
        // overwrite any intermediate `Usage` chunks (Anthropic
        // emits multiple).
        self.usage = Some(result.usage);
        if self.stop_reason.is_none() {
            self.stop_reason = result.stop_reason;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        json_repair::parse_tool_input,
        types::{Content, Message, Role},
    };

    fn text_chunk(s: &str) -> StreamChunk {
        StreamChunk::Content(ContentPart::Text(TextContent {
            text: s.to_string(),
            cache_control: None,
        }))
    }

    fn reasoning_chunk(s: &str) -> StreamChunk {
        StreamChunk::Content(ContentPart::Reasoning(ReasoningContent {
            text: s.to_string(),
            signature: None,
        }))
    }

    fn tool_start(
        id: &str,
        name: &str,
        args: serde_json::Value,
    ) -> StreamChunk {
        StreamChunk::ToolCallStart {
            id: id.to_string(),
            name: name.to_string(),
            arguments: args,
        }
    }

    fn tool_delta(id: &str, delta: &str) -> StreamChunk {
        StreamChunk::ToolCallDelta {
            id: id.to_string(),
            arguments_delta: delta.to_string(),
        }
    }

    fn tool_end(id: &str) -> StreamChunk {
        StreamChunk::ToolCallEnd { id: id.to_string() }
    }

    fn done_with(result: SamplingResult) -> StreamChunk {
        StreamChunk::IsDone {
            result: Box::new(result),
        }
    }

    /// The deferred signature lands only on the trailing reasoning run.
    ///
    /// On the streaming path every reasoning part is un-signed, so a
    /// walk that did not stop at the run's edge would stamp the single
    /// latest signature onto *every* reasoning part — including an
    /// earlier block it was never computed for. A signature is bound to
    /// the text it signed, so that would forge one.
    ///
    /// The earlier block is deliberately left unsigned: the processor
    /// tracks one signature slot per turn, so its own signature is not
    /// knowable here. The Anthropic adapter drops an unsigned block
    /// rather than sending an invalid request, so the outcome is
    /// "reasoning omitted", never "signature lying about its text".
    #[test]
    fn deferred_signature_stamps_only_the_trailing_reasoning_run() {
        let mut a = BlockAssembler::new();
        // Interleaved shape: block 1, text, block 2.
        a.push(reasoning_chunk("block one a"));
        a.push(reasoning_chunk("block one b"));
        a.push(text_chunk("answer"));
        a.push(reasoning_chunk("block two a"));
        a.push(reasoning_chunk("block two b"));
        a.push(done_with(SamplingResult {
            text: String::new(),
            tool_calls: Vec::new(),
            reasoning: String::new(),
            reasoning_signature: Some("sig_for_block_two".to_string()),
            usage: TokenUsage::default(),
            stop_reason: Some("end_turn".to_string()),
        }));

        let (_result, parts, _) = a.finalize_assistant();
        let signatures: Vec<(&str, Option<&str>)> = parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::Reasoning(rc) => {
                    Some((rc.text.as_str(), rc.signature.as_deref()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            signatures,
            vec![
                ("block one a", None),
                ("block one b", None),
                ("block two a", Some("sig_for_block_two")),
                ("block two b", Some("sig_for_block_two")),
            ],
            "only the trailing run may be stamped; a signature must never \
             be attached to an earlier block"
        );
    }

    #[test]
    fn empty_stream_returns_empty_sampling_result() {
        let mut a = BlockAssembler::new();
        let (result, parts, incomplete) = a.finalize_assistant();
        assert!(result.text.is_empty());
        assert!(result.tool_calls.is_empty());
        assert!(parts.is_empty());
        assert!(incomplete);
    }

    #[test]
    fn streamed_text_chunks_accumulate_in_order() {
        let mut a = BlockAssembler::new();
        a.push(text_chunk("hello "));
        a.push(text_chunk("world"));
        assert_eq!(a.text_so_far(), "hello world");
        let (result, _parts, _) = a.finalize_assistant();
        assert_eq!(result.text, "hello world");
    }

    #[test]
    fn reasoning_accumulates_separately_from_text() {
        let mut a = BlockAssembler::new();
        a.push(reasoning_chunk("think 1 "));
        a.push(reasoning_chunk("think 2"));
        a.push(text_chunk("answer"));
        assert_eq!(a.reasoning_so_far(), "think 1 think 2");
        assert_eq!(a.text_so_far(), "answer");
        let (result, _, _) = a.finalize_assistant();
        assert_eq!(result.reasoning, "think 1 think 2");
        assert_eq!(result.text, "answer");
    }

    #[test]
    fn tool_call_deltas_concatenate_arguments() {
        let mut a = BlockAssembler::new();
        a.push(tool_start("tc1", "shell", serde_json::json!("")));
        a.push(tool_delta("tc1", "{\"command\":\""));
        a.push(tool_delta("tc1", "ls -la\"}"));
        a.push(tool_end("tc1"));
        let (result, parts, _) = a.finalize_assistant();
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].id, "tc1");
        assert_eq!(result.tool_calls[0].name, "shell");
        assert_eq!(
            result.tool_calls[0].input,
            serde_json::json!({"command": "ls -la"})
        );
        assert_eq!(parts.len(), 1);
        assert!(matches!(parts[0], ContentPart::ToolUse(_)));
    }

    #[test]
    fn is_done_dedupes_tool_calls_already_pushed_by_stream() {
        // Provider emits ToolCallStart/Delta/End, then IsDone
        // re-lists the same call. The assembler must NOT push it
        // twice.
        let mut a = BlockAssembler::new();
        a.push(tool_start("tc1", "shell", serde_json::json!("{}")));
        a.push(tool_end("tc1"));
        let dup = ToolUse {
            id: "tc1".to_string(),
            name: "shell".to_string(),
            input: serde_json::json!({}),
        };
        a.push(done_with(SamplingResult {
            text: String::new(),
            tool_calls: vec![dup],
            reasoning: String::new(),
            reasoning_signature: None,
            usage: TokenUsage::default(),
            stop_reason: Some("tool_calls".to_string()),
        }));
        let (result, parts, _) = a.finalize_assistant();
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(parts.len(), 1);
        assert!(a.is_done);
    }

    #[test]
    fn is_done_folds_text_when_no_streamed_text() {
        // Non-streaming provider batches everything into IsDone.
        let mut a = BlockAssembler::new();
        a.push(done_with(SamplingResult {
            text: "all at once".to_string(),
            tool_calls: Vec::new(),
            reasoning: String::new(),
            reasoning_signature: None,
            usage: TokenUsage::default(),
            stop_reason: Some("stop".to_string()),
        }));
        let (result, _, incomplete) = a.finalize_assistant();
        assert_eq!(result.text, "all at once");
        assert!(!incomplete);
    }

    #[test]
    fn is_done_does_not_double_count_streamed_text() {
        // Streamed text already covered — IsDone's text is
        // ignored so the model-visible history shows one copy.
        let mut a = BlockAssembler::new();
        a.push(text_chunk("hello"));
        a.push(done_with(SamplingResult {
            text: "hello".to_string(),
            tool_calls: Vec::new(),
            reasoning: String::new(),
            reasoning_signature: None,
            usage: TokenUsage::default(),
            stop_reason: Some("stop".to_string()),
        }));
        let (result, _, _) = a.finalize_assistant();
        assert_eq!(result.text, "hello");
    }

    #[test]
    fn usage_chunk_overwritten_by_terminal_is_done_usage() {
        // Anthropic emits intermediate `message_delta.usage`
        // before the terminal one. The assembler must let the
        // terminal value win.
        let mut a = BlockAssembler::new();
        a.push(StreamChunk::Usage(TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 5,
            total_tokens: 105,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            reasoning_tokens: None,
        }));
        a.push(done_with(SamplingResult {
            text: String::new(),
            tool_calls: Vec::new(),
            reasoning: String::new(),
            reasoning_signature: None,
            usage: TokenUsage {
                prompt_tokens: 120,
                completion_tokens: 8,
                total_tokens: 128,
                cached_prompt_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
                reasoning_tokens: None,
            },
            stop_reason: Some("stop".to_string()),
        }));
        let (result, _, _) = a.finalize_assistant();
        assert_eq!(result.usage.prompt_tokens, 120);
        assert_eq!(result.usage.completion_tokens, 8);
    }

    #[test]
    fn finalize_drains_open_tool_buffers_on_early_termination() {
        // Stream ends mid-ToolCall (no ToolCallEnd) without
        // IsDone. The assembler must still surface the partial
        // tool call so the history builder does not silently
        // drop it.
        let mut a = BlockAssembler::new();
        a.push(tool_start("tc1", "shell", serde_json::json!("{}")));
        a.push(tool_delta("tc1", "{\"command\":\"ls\""));
        // No ToolCallEnd, no IsDone.
        let (result, _, incomplete) = a.finalize_assistant();
        assert!(incomplete);
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].id, "tc1");
    }

    #[test]
    fn tool_call_delta_for_unknown_id_is_ignored() {
        // Provider sometimes re-uses an id after ToolCallEnd. The
        // assembler must ignore stragglers so the closed block's
        // arguments are not corrupted.
        let mut a = BlockAssembler::new();
        a.push(tool_start("tc1", "shell", serde_json::json!("{}")));
        a.push(tool_end("tc1"));
        a.push(tool_delta("tc1", "stray"));
        let (result, _, _) = a.finalize_assistant();
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].input, serde_json::json!({}));
    }

    /// Regression (R33): the assembler used to hold a private parser
    /// that degraded invalid JSON to `Value::Null`, while the shared
    /// helper degraded to `Value::String` — the same input produced a
    /// different `ToolUse.input` depending on which path finalized
    /// it. Both go through `json_repair::parse_tool_input*` now, so
    /// the streamed path and the shared helper must agree, whether
    /// the payload is rescued by the repair or falls back to the raw
    /// text.
    #[test]
    fn streamed_tool_input_agrees_with_the_shared_parser() {
        for raw in [
            r#"{"cmd": "ls""#,          // truncated: no repair possible
            "not json at all",          // garbage
            "{\"cmd\": \"echo a\nb\"}", // repaired: raw newline
        ] {
            let mut a = BlockAssembler::new();
            a.push(tool_start("tc1", "shell", serde_json::json!("")));
            a.push(tool_delta("tc1", raw));
            a.push(tool_end("tc1"));
            let (result, _parts, _) = a.finalize_assistant();
            let input = &result.tool_calls[0].input;
            assert_eq!(
                *input,
                parse_tool_input(raw),
                "assembler and shared parser must agree on: {raw}"
            );
            assert!(
                !input.is_null(),
                "the `Value::Null` degradation is dead: {raw}"
            );
        }
    }

    #[test]
    fn stop_chunk_first_wins_for_stop_reason() {
        // Provider may emit `Stop` then `IsDone` with the same
        // reason; the assembler's contract is "first observation
        // wins" so the wire-visible reason is preserved.
        let mut a = BlockAssembler::new();
        a.push(StreamChunk::Stop("tool_use".to_string()));
        a.push(done_with(SamplingResult {
            text: String::new(),
            tool_calls: Vec::new(),
            reasoning: String::new(),
            reasoning_signature: None,
            usage: TokenUsage::default(),
            stop_reason: Some("end_turn".to_string()),
        }));
        let (result, _, _) = a.finalize_assistant();
        assert_eq!(result.stop_reason.as_deref(), Some("tool_use"));
    }

    #[test]
    fn empty_stream_can_be_used_to_construct_message() {
        // Loosely sanity-checks that the assembler's output
        // composes into a `Message` (the loop's actual
        // consumer).
        let mut a = BlockAssembler::new();
        a.push(text_chunk("done"));
        let (result, parts, _) = a.finalize_assistant();
        let _ = Content::parts(parts);
        let _ = Message {
            role: Role::Assistant,
            content: Content::text(""),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        // The Message constructor is exercised by the call
        // above; we don't assert the field assignment because
        // the assembler's job is producing parts + a
        // SamplingResult, not a fully-built Message.
        assert_eq!(result.text, "done");
    }
}
