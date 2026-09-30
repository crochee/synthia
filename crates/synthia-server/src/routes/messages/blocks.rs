//! Folding a run's [`AgentEvent`] stream into an Anthropic message.
//!
//! [`Turn`] is the one place that decides what a run's event stream
//! means on the Anthropic wire. Both response paths drive the same
//! accumulator — the streaming handler forwards every [`Frame`] it
//! returns, the non-streaming handler ignores the frames and serialises
//! [`Turn::message`] — so the JSON body and the concatenated stream can
//! never disagree.
//!
//! # What becomes content
//!
//! | event | response |
//! |---|---|
//! | `Model(ContentPart::Text)` | a `text` block delta |
//! | `Model(ContentPart::Reasoning)` | a `thinking` block delta |
//! | `Model(ContentPart::ToolUse)` | buffered; see below |
//! | `Model(ContentPart::ToolResult)` | nothing; see below |
//! | `Model(Image / Audio / Resource)` | nothing |
//! | `System(Usage)` / `ModelDone` | token totals and `stop_reason` |
//! | `System(SessionEnded{Error})` | a terminal `error` frame |
//! | everything else | nothing |
//!
//! ## Tool visibility
//!
//! This server executes its own tools, so a `tool_use` the model asked
//! for is never something the client is expected to answer, and a
//! `ToolResult` is tool *output* rather than the model's answer. What
//! the client is shown therefore depends on who the client is, and
//! [`Turn::new`]'s last argument decides:
//!
//! - **`tool_blocks: false`** — an external Anthropic client (the SDKs,
//!   Claude Code, editor plugins). A call is buffered and dropped as
//!   soon as a `ToolResult` proves the agent executed it; it is only
//!   surfaced as a `tool_use` block when the run ends with a call nobody
//!   ran (the loop stopping right after the model asked for one) — the
//!   case where the call *is* the assistant's final answer. A result
//!   produces nothing at all.
//! - **`tool_blocks: true`** — a request that opted in through
//!   `metadata.synthia_session_id`, i.e. Synthia's own web client, which
//!   renders a collapsible `工具 · <name>` block with 请求 / 结果 halves.
//!   Every call and every result is surfaced as it arrives, so the two
//!   halves can be paired by id.
//!
//! ### The opted-in block shapes
//!
//! A call uses the protocol's own block — `content_block_start` seeded
//! with `{"type":"tool_use","id":…,"name":…,"input":{}}`, one
//! `input_json_delta` carrying the whole input, `content_block_stop` —
//! which is exactly the shape [`Turn::surface_tool_use`] already emits
//! for a never-executed call. The corresponding result becomes its own
//! block:
//!
//! ```text
//! {"type":"tool_result","tool_use_id":…,"name":…,"content":[…],"is_error":…}
//! ```
//!
//! Anthropic has no assistant `tool_result` block (the protocol carries
//! one inside a *user* message), so this is the endpoint's one
//! documented extension to the protocol. It lives *inside* the
//! assistant's content because the whole point is that the call and its
//! result are observable in one turn — the client cannot answer a call
//! it was never handed, and a separate `user` message would be the
//! endpoint talking to itself. Two deliberate deviations from
//! Anthropic's `tool_result` spelling: `name` is included (a result the
//! runtime labels, so a client can render 工具 · `name` even when it
//! never saw the call), and `is_error` is always present as a boolean
//! rather than an optional value, so a failure is never indistinguishable
//! from a success. The result's content is complete when the event
//! arrives, so it rides the `content_block_start` and only
//! `content_block_stop` follows — no deltas.
//!
//! Everything else is identical between the two modes: the block
//! indices, the text/thinking folding, the usage totals, and — for a run
//! with no tool activity — the bytes.
//!
//! ## Thinking signatures
//!
//! A streamed reasoning block usually carries no signature: the harness
//! publishes each reasoning part as it arrives, and stamps the
//! `signature_delta` value onto the assembled turn only afterwards
//! (`synthia::harness::agent::re_act::stream`). The signature is
//! therefore attached to the block when the part itself carries one,
//! and left empty otherwise.

use axum::response::sse::Event;
use serde::Serialize;
use synthia::{
    harness::{AgentEvent, SessionEndReason, SystemEvent},
    provider::{ContentPart, ImageContent, ToolResult, ToolUse},
};

use super::wire::{
    ASSISTANT_ROLE,
    CONTENT_BLOCK_DELTA,
    CONTENT_BLOCK_START,
    CONTENT_BLOCK_STOP,
    ContentBlockDelta,
    ContentBlockStart,
    ContentBlockStop,
    Delta,
    ERROR,
    ErrorEnvelope,
    MESSAGE_DELTA,
    MESSAGE_KIND,
    MESSAGE_START,
    MESSAGE_STOP,
    MessageDelta,
    MessageDeltaBody,
    MessageEnvelope,
    MessageStart,
    MessageStop,
    ResponseBlock,
    ResultBlock,
    ResultImageSource,
    Usage,
    error_type,
};

/// One frame of an Anthropic event stream.
///
/// The handler turns each frame into an SSE `event:`/`data:` pair with
/// [`Frame::event`].
#[derive(Debug)]
pub(super) enum Frame {
    MessageStart(MessageEnvelope),
    ContentBlockStart {
        index: usize,
        content_block: ResponseBlock,
    },
    ContentBlockDelta {
        index: usize,
        delta: Delta,
    },
    ContentBlockStop {
        index: usize,
    },
    MessageDelta {
        delta: MessageDelta,
        usage: Usage,
    },
    MessageStop,
    Error(ErrorEnvelope),
}

impl Frame {
    /// Render this frame as an SSE event.
    ///
    /// `serde_json` escapes newlines inside strings, so a frame's data
    /// is always a single SSE line — the framing cannot be broken by
    /// model output.
    pub(super) fn event(&self) -> Event {
        match self {
            Self::MessageStart(message) => sse(
                MESSAGE_START,
                &MessageStart {
                    kind: MESSAGE_START,
                    message: message.clone(),
                },
            ),
            Self::ContentBlockStart {
                index,
                content_block,
            } => sse(
                CONTENT_BLOCK_START,
                &ContentBlockStart {
                    kind: CONTENT_BLOCK_START,
                    index: *index,
                    content_block: content_block.clone(),
                },
            ),
            Self::ContentBlockDelta { index, delta } => sse(
                CONTENT_BLOCK_DELTA,
                &ContentBlockDelta {
                    kind: CONTENT_BLOCK_DELTA,
                    index: *index,
                    delta: clone_delta(delta),
                },
            ),
            Self::ContentBlockStop { index } => sse(
                CONTENT_BLOCK_STOP,
                &ContentBlockStop {
                    kind: CONTENT_BLOCK_STOP,
                    index: *index,
                },
            ),
            Self::MessageDelta { delta, usage } => sse(
                MESSAGE_DELTA,
                &MessageDeltaBody {
                    kind: MESSAGE_DELTA,
                    delta: MessageDelta {
                        stop_reason: delta.stop_reason.clone(),
                        stop_sequence: delta.stop_sequence.clone(),
                    },
                    usage: usage.clone(),
                },
            ),
            Self::MessageStop => {
                sse(MESSAGE_STOP, &MessageStop { kind: MESSAGE_STOP })
            }
            Self::Error(envelope) => {
                Event::default().event(ERROR).data(envelope.body())
            }
        }
    }
}

fn sse<T: Serialize>(name: &'static str, body: &T) -> Event {
    let data = serde_json::to_string(body).unwrap_or_else(|_| "{}".to_string());
    Event::default().event(name).data(data)
}

/// `Delta` is not `Clone` on purpose (it carries owned strings that are
/// moved into the frame); rendering clones through this helper keeps
/// the frame type `Debug`-only.
fn clone_delta(delta: &Delta) -> Delta {
    match delta {
        Delta::Text { text } => Delta::Text { text: text.clone() },
        Delta::InputJson { partial_json } => Delta::InputJson {
            partial_json: partial_json.clone(),
        },
        Delta::Thinking { thinking } => Delta::Thinking {
            thinking: thinking.clone(),
        },
        Delta::Signature { signature } => Delta::Signature {
            signature: signature.clone(),
        },
    }
}

/// The block kinds that absorb streaming deltas.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DeltaKind {
    Text,
    Thinking,
}

/// One Anthropic message being assembled from a run's events.
pub(super) struct Turn {
    id: String,
    model: String,
    /// Whether a tool call and its result become observable blocks.
    /// `false` for an external Anthropic client, `true` for a request
    /// that opted in through `metadata.synthia_session_id` — see the
    /// module docs.
    tool_blocks: bool,
    /// Blocks that have been closed (and so appear in the response).
    blocks: Vec<ResponseBlock>,
    /// The one block currently open, if any. Its response index is
    /// `blocks.len()`.
    open: Option<ResponseBlock>,
    /// Tool calls the model asked for but that no `ToolResult` has
    /// proven executed yet.
    pending_tool_calls: Vec<ToolUse>,
    /// Set once a buffered call is surfaced, so the terminal
    /// `stop_reason` can say `tool_use`.
    surfaced_tool_use: bool,
    usage: Usage,
    /// `true` once a `SystemEvent::Usage` has been folded, which makes
    /// it the usage source for the whole run.
    system_usage_seen: bool,
    /// The provider's own stop reason, mapped to Anthropic's
    /// vocabulary, from the last `ModelDone`.
    stop_reason: Option<String>,
    /// How the run ended, set by the terminal event.
    end: Option<SessionEndReason>,
}

impl Turn {
    pub(super) fn new(
        id: impl Into<String>,
        model: impl Into<String>,
        tool_blocks: bool,
    ) -> Self {
        Self {
            id: id.into(),
            model: model.into(),
            tool_blocks,
            blocks: Vec::new(),
            open: None,
            pending_tool_calls: Vec::new(),
            surfaced_tool_use: false,
            usage: Usage::default(),
            system_usage_seen: false,
            stop_reason: None,
            end: None,
        }
    }

    /// The `message_start` frame: the message before any content, with
    /// an empty usage block.
    ///
    /// Token counts cannot be known yet — the run has not been
    /// sampled — so the head reports zeroes and the real totals ride
    /// the terminal `message_delta`, whose `usage` object carries
    /// `input_tokens` alongside `output_tokens` for exactly this
    /// reason.
    pub(super) fn start(&self) -> Frame {
        Frame::MessageStart(self.envelope(Vec::new(), None))
    }

    /// Fold one event, returning the frames it produces.
    pub(super) fn on_event(&mut self, event: &AgentEvent) -> Vec<Frame> {
        match event {
            AgentEvent::Model(part) => self.on_part(part),
            AgentEvent::ModelDone(result) => {
                self.on_model_done(result);
                Vec::new()
            }
            AgentEvent::System(SystemEvent::Usage {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_creation_tokens,
            }) => {
                self.on_usage(
                    *input_tokens,
                    *output_tokens,
                    *cache_read_tokens,
                    *cache_creation_tokens,
                );
                Vec::new()
            }
            AgentEvent::System(SystemEvent::SessionEnded { reason }) => {
                self.end = Some(reason.clone());
                Vec::new()
            }
            // Progress / warnings / steering notices are diagnostics,
            // and an `Agent(..)` event is a *subagent's* trace: neither
            // is part of this message.
            AgentEvent::System(_) | AgentEvent::Agent(..) => Vec::new(),
        }
    }

    /// Close the message: close the open block, surface any tool call
    /// the run never executed, and emit the terminal frames.
    ///
    /// A run that ended in an error terminates with an `error` frame
    /// instead of `message_delta` + `message_stop`, so a client never
    /// sees a successful terminal frame for a failed run.
    pub(super) fn finish(&mut self) -> Vec<Frame> {
        let mut frames = self.close_open();
        let tool_calls = std::mem::take(&mut self.pending_tool_calls);
        self.surfaced_tool_use = !tool_calls.is_empty();
        for tool_use in tool_calls {
            frames.extend(self.surface_tool_use(tool_use));
        }
        match &self.end {
            Some(SessionEndReason::Error(message)) => {
                frames.push(Frame::Error(ErrorEnvelope::new(
                    error_type::API,
                    message.clone(),
                )));
            }
            _ => {
                frames.push(Frame::MessageDelta {
                    delta: MessageDelta {
                        stop_reason: self.stop_reason(),
                        stop_sequence: None,
                    },
                    usage: self.usage.clone(),
                });
                frames.push(Frame::MessageStop);
            }
        }
        frames
    }

    /// The complete message, for the non-streaming body.
    pub(super) fn message(&self) -> MessageEnvelope {
        self.envelope(self.blocks.clone(), Some(self.stop_reason()))
    }

    /// How the run ended, once the terminal event has been folded.
    pub(super) fn end_reason(&self) -> Option<&SessionEndReason> {
        self.end.as_ref()
    }

    // -- folding ----------------------------------------------------------

    fn on_part(&mut self, part: &ContentPart) -> Vec<Frame> {
        match part {
            ContentPart::Text(text) => {
                let mut frames = self.ensure_open(DeltaKind::Text);
                if let Some(ResponseBlock::Text { text: open }) =
                    self.open.as_mut()
                {
                    open.push_str(&text.text);
                }
                frames.push(self.block_delta(Delta::Text {
                    text: text.text.clone(),
                }));
                frames
            }
            ContentPart::Reasoning(reasoning) => {
                let mut frames = self.ensure_open(DeltaKind::Thinking);
                if let Some(ResponseBlock::Thinking {
                    thinking,
                    signature,
                }) = self.open.as_mut()
                {
                    thinking.push_str(&reasoning.text);
                    if let Some(value) = &reasoning.signature {
                        *signature = value.clone();
                    }
                }
                frames.push(self.block_delta(Delta::Thinking {
                    thinking: reasoning.text.clone(),
                }));
                if let Some(signature) = &reasoning.signature {
                    frames.push(self.block_delta(Delta::Signature {
                        signature: signature.clone(),
                    }));
                }
                frames
            }
            // Opted-in clients see both halves of a tool execution (the
            // module docs explain why they are separate blocks);
            // external clients see neither, unless the run never
            // executed the call at all.
            ContentPart::ToolUse(tool_use) => {
                if self.tool_blocks {
                    return self.surface_tool_use(tool_use.clone());
                }
                self.pending_tool_calls.push(tool_use.clone());
                Vec::new()
            }
            ContentPart::ToolResult(result) => {
                if self.tool_blocks {
                    return self.surface_tool_result(result);
                }
                // Proof the agent ran the batch — drop the buffered calls.
                self.pending_tool_calls.clear();
                Vec::new()
            }
            ContentPart::Image(_)
            | ContentPart::Audio(_)
            | ContentPart::Resource(_) => Vec::new(),
        }
    }

    fn on_model_done(&mut self, result: &synthia::provider::SamplingResult) {
        if let Some(raw) = &result.stop_reason {
            self.stop_reason = Some(anthropic_stop_reason(raw).to_string());
        }
        // Strategy-driven runs (best-of-n, CoT) publish a
        // `SamplingResult` without going through the chunk sink, so no
        // `SystemEvent::Usage` is emitted for the pass.
        if !self.system_usage_seen {
            self.usage.input_tokens += result.usage.prompt_tokens as u64;
            self.usage.output_tokens += result.usage.completion_tokens as u64;
        }
    }

    fn on_usage(
        &mut self,
        input_tokens: usize,
        output_tokens: usize,
        cache_read_tokens: Option<usize>,
        cache_creation_tokens: Option<usize>,
    ) {
        self.system_usage_seen = true;
        self.usage.input_tokens += input_tokens as u64;
        self.usage.output_tokens += output_tokens as u64;
        add_optional(
            &mut self.usage.cache_read_input_tokens,
            cache_read_tokens,
        );
        add_optional(
            &mut self.usage.cache_creation_input_tokens,
            cache_creation_tokens,
        );
    }

    /// Surface one tool call as a complete `tool_use` block.
    ///
    /// Called for a never-executed call in every mode, and for *every*
    /// call when the client opted in.
    ///
    /// The protocol streams tool input as partial JSON; this server
    /// holds the whole call already, so it emits the input as a single
    /// `input_json_delta` — the same wire shape a non-streaming
    /// provider produces under the streaming protocol.
    fn surface_tool_use(&mut self, tool_use: ToolUse) -> Vec<Frame> {
        // A tool call ends the text (or thinking) block that led up to
        // it: the open block must be closed *before* this one claims an
        // index, or the two would share one — a client cannot tell two
        // blocks with the same index apart.
        let mut frames = self.close_open();
        let index = self.blocks.len();
        let partial_json = serde_json::to_string(&tool_use.input)
            .unwrap_or_else(|_| "{}".to_string());
        frames.extend([
            Frame::ContentBlockStart {
                index,
                content_block: ResponseBlock::ToolUse {
                    id: tool_use.id.clone(),
                    name: tool_use.name.clone(),
                    input: serde_json::json!({}),
                },
            },
            Frame::ContentBlockDelta {
                index,
                delta: Delta::InputJson { partial_json },
            },
            Frame::ContentBlockStop { index },
        ]);
        self.blocks.push(ResponseBlock::ToolUse {
            id: tool_use.id,
            name: tool_use.name,
            input: tool_use.input,
        });
        frames
    }

    /// Surface one executed tool's output as a `tool_result` block — the
    /// opted-in client's 结果 half, paired to its call by `tool_use_id`.
    ///
    /// The result's content is whole when the event arrives, so the block
    /// rides the `content_block_start` and only the `content_block_stop`
    /// follows: there is nothing to stream in pieces and nothing for a
    /// client to accumulate.
    fn surface_tool_result(&mut self, result: &ToolResult) -> Vec<Frame> {
        let mut frames = self.close_open();
        let index = self.blocks.len();
        let block = ResponseBlock::ToolResult {
            tool_use_id: result.tool_use_id.clone(),
            name: result.tool_name.clone(),
            content: result.content.iter().map(result_block).collect(),
            is_error: result.is_error.unwrap_or(false),
        };
        self.blocks.push(block.clone());
        frames.extend([
            Frame::ContentBlockStart {
                index,
                content_block: block,
            },
            Frame::ContentBlockStop { index },
        ]);
        frames
    }

    // -- block bookkeeping ------------------------------------------------

    /// Close the open block if it is not already `kind`, then keep
    /// `kind` open — emitting the `content_block_*` frames the
    /// transition needs.
    fn ensure_open(&mut self, kind: DeltaKind) -> Vec<Frame> {
        let already = matches!(
            (&self.open, kind),
            (Some(ResponseBlock::Text { .. }), DeltaKind::Text)
                | (Some(ResponseBlock::Thinking { .. }), DeltaKind::Thinking)
        );
        if already {
            return Vec::new();
        }
        let mut frames = self.close_open();
        let seed = match kind {
            DeltaKind::Text => ResponseBlock::empty_text(),
            DeltaKind::Thinking => ResponseBlock::empty_thinking(),
        };
        frames.push(Frame::ContentBlockStart {
            index: self.blocks.len(),
            content_block: seed.clone(),
        });
        self.open = Some(seed);
        frames
    }

    /// Close the open block, if any, returning its stop frame.
    fn close_open(&mut self) -> Vec<Frame> {
        let Some(block) = self.open.take() else {
            return Vec::new();
        };
        let index = self.blocks.len();
        self.blocks.push(block);
        vec![Frame::ContentBlockStop { index }]
    }

    /// A delta frame for the open block.
    fn block_delta(&self, delta: Delta) -> Frame {
        Frame::ContentBlockDelta {
            index: self.blocks.len(),
            delta,
        }
    }

    fn stop_reason(&self) -> String {
        if self.surfaced_tool_use {
            return "tool_use".to_string();
        }
        if matches!(self.end, Some(SessionEndReason::MaxIterations)) {
            return "max_tokens".to_string();
        }
        self.stop_reason
            .clone()
            .unwrap_or_else(|| "end_turn".to_string())
    }

    fn envelope(
        &self,
        content: Vec<ResponseBlock>,
        stop_reason: Option<String>,
    ) -> MessageEnvelope {
        MessageEnvelope {
            id: self.id.clone(),
            kind: MESSAGE_KIND,
            role: ASSISTANT_ROLE,
            model: self.model.clone(),
            content,
            stop_reason,
            stop_sequence: None,
            usage: self.usage.clone(),
        }
    }
}

/// Project one part of a tool result's content onto the protocol's
/// `tool_result` block vocabulary.
///
/// `text` and `image` are the two spellings the protocol defines for a
/// result's content. Every other part — audio, a nested call, a resource
/// link — has none, and a result the client cannot read is worse than one
/// rendered as its JSON, so those become text.
fn result_block(part: &ContentPart) -> ResultBlock {
    match part {
        ContentPart::Text(text) => ResultBlock::Text {
            text: text.text.clone(),
        },
        ContentPart::Image(image) => ResultBlock::Image {
            source: result_image_source(image),
        },
        other => ResultBlock::Text {
            text: serde_json::to_string(other).unwrap_or_default(),
        },
    }
}

/// The `image.source` spelling for a canonical image.
///
/// [`super::convert::image_part`] keeps a remote image's URL *in*
/// `data`, so classifying it back is what keeps a round trip lossless.
fn result_image_source(image: &ImageContent) -> ResultImageSource {
    if image.data.starts_with("http://") || image.data.starts_with("https://") {
        ResultImageSource::Url {
            url: image.data.clone(),
        }
    } else {
        ResultImageSource::Base64 {
            media_type: image.mime_type.clone(),
            data: image.data.clone(),
        }
    }
}

/// Map a provider stop reason onto Anthropic's vocabulary.
fn anthropic_stop_reason(raw: &str) -> &'static str {
    match raw {
        "tool_use" | "tool_calls" | "function_call" => "tool_use",
        "max_tokens" | "length" => "max_tokens",
        "stop_sequence" => "stop_sequence",
        "refusal" => "refusal",
        // `end_turn`, OpenAI's `stop`, and anything unrecognised: the
        // model finished its turn.
        _ => "end_turn",
    }
}

fn add_optional(slot: &mut Option<u64>, value: Option<usize>) {
    if let Some(value) = value {
        *slot.get_or_insert(0) += value as u64;
    }
}

#[cfg(test)]
mod tests {
    use synthia::{
        harness::SystemEvent,
        provider::{
            ContentPart,
            ReasoningContent,
            SamplingResult,
            TextContent,
            TokenUsage,
            ToolResult,
            ToolUse,
        },
    };

    use super::*;

    fn text_event(text: &str) -> AgentEvent {
        AgentEvent::Model(ContentPart::Text(TextContent {
            text: text.to_string(),
            cache_control: None,
        }))
    }

    fn reasoning_event(text: &str, signature: Option<&str>) -> AgentEvent {
        AgentEvent::Model(ContentPart::Reasoning(ReasoningContent {
            text: text.to_string(),
            signature: signature.map(str::to_string),
        }))
    }

    fn tool_use_event(id: &str) -> AgentEvent {
        AgentEvent::Model(ContentPart::ToolUse(ToolUse {
            id: id.to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({ "cmd": "ls" }),
        }))
    }

    fn tool_result_event(id: &str) -> AgentEvent {
        let mut result = ToolResult::new(id, "a.txt");
        // The runtime labels every result with the tool that produced it
        // (`commit_tool_result`), which is what lets a client render
        // 工具 · <name> even for a call it never saw.
        result.tool_name = Some("bash".to_string());
        AgentEvent::Model(ContentPart::ToolResult(result))
    }

    /// A result the runtime left unnamed — a replayed transcript row can
    /// be, and the block must then omit `name` rather than invent one.
    fn unnamed_tool_result_event(id: &str) -> AgentEvent {
        AgentEvent::Model(ContentPart::ToolResult(ToolResult::new(id, "a.txt")))
    }

    fn failed_tool_result_event(id: &str) -> AgentEvent {
        AgentEvent::Model(ContentPart::ToolResult(ToolResult::error(
            id, "boom",
        )))
    }

    fn usage_event(input: usize, output: usize) -> AgentEvent {
        AgentEvent::System(SystemEvent::Usage {
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: None,
            cache_creation_tokens: None,
        })
    }

    fn done_event(stop_reason: Option<&str>) -> AgentEvent {
        AgentEvent::ModelDone(SamplingResult {
            stop_reason: stop_reason.map(str::to_string),
            usage: TokenUsage {
                prompt_tokens: 5,
                completion_tokens: 7,
                total_tokens: 12,
                ..TokenUsage::default()
            },
            ..SamplingResult::default()
        })
    }

    fn ended_event(reason: SessionEndReason) -> AgentEvent {
        AgentEvent::System(SystemEvent::SessionEnded { reason })
    }

    /// A turn that folds `events` and returns the accumulator plus every
    /// frame `on_event` produced, in order.
    fn fold(events: &[AgentEvent]) -> (Turn, Vec<Frame>) {
        fold_in(events, false)
    }

    /// The same, for a client that opted into tool blocks.
    fn fold_with_tools(events: &[AgentEvent]) -> (Turn, Vec<Frame>) {
        fold_in(events, true)
    }

    fn fold_in(events: &[AgentEvent], tool_blocks: bool) -> (Turn, Vec<Frame>) {
        let mut turn = Turn::new("msg_1", "claude-test", tool_blocks);
        let mut frames = Vec::new();
        for event in events {
            frames.extend(turn.on_event(event));
        }
        (turn, frames)
    }

    /// The `content_block` a start frame carries, as it appears on the
    /// wire — the shape a client actually parses.
    fn block_json(frame: &Frame) -> serde_json::Value {
        match frame {
            Frame::ContentBlockStart { content_block, .. } => {
                serde_json::to_value(content_block).unwrap()
            }
            other => panic!("{other:?} is not a block start"),
        }
    }

    fn names(frames: &[Frame]) -> Vec<&'static str> {
        frames.iter().map(frame_name).collect()
    }

    fn frame_name(frame: &Frame) -> &'static str {
        match frame {
            Frame::MessageStart(_) => MESSAGE_START,
            Frame::ContentBlockStart { .. } => CONTENT_BLOCK_START,
            Frame::ContentBlockDelta { .. } => CONTENT_BLOCK_DELTA,
            Frame::ContentBlockStop { .. } => CONTENT_BLOCK_STOP,
            Frame::MessageDelta { .. } => MESSAGE_DELTA,
            Frame::MessageStop => MESSAGE_STOP,
            Frame::Error(_) => ERROR,
        }
    }

    /// The response index a block frame addresses.
    fn index(frame: &Frame) -> usize {
        match frame {
            Frame::ContentBlockStart { index, .. }
            | Frame::ContentBlockDelta { index, .. }
            | Frame::ContentBlockStop { index } => *index,
            other => panic!("{other:?} is not a block frame"),
        }
    }

    /// Two text deltas stream as one `text` block: start, one delta per
    /// part, one stop — and the assembled block holds the
    /// concatenation.
    #[test]
    fn text_deltas_share_one_block() {
        let (mut turn, frames) = fold(&[text_event("hel"), text_event("lo")]);
        assert_eq!(
            names(&frames),
            vec![
                CONTENT_BLOCK_START,
                CONTENT_BLOCK_DELTA,
                CONTENT_BLOCK_DELTA
            ]
        );
        assert_eq!(frames.iter().map(index).collect::<Vec<_>>(), vec![0, 0, 0]);

        let tail = turn.finish();
        assert_eq!(
            names(&tail),
            vec![CONTENT_BLOCK_STOP, MESSAGE_DELTA, MESSAGE_STOP]
        );
        assert_eq!(index(&tail[0]), 0);
        let message = turn.message();
        assert_eq!(message.content.len(), 1);
        match &message.content[0] {
            ResponseBlock::Text { text } => assert_eq!(text, "hello"),
            other => panic!("expected one text block: {other:?}"),
        }
    }

    /// Thinking then text produces two blocks with sequential indices,
    /// and the thinking block keeps the signature it was given.
    #[test]
    fn thinking_and_text_are_separate_blocks() {
        let (mut turn, frames) =
            fold(&[reasoning_event("why", Some("sig")), text_event("answer")]);
        assert_eq!(
            names(&frames),
            vec![
                CONTENT_BLOCK_START,
                CONTENT_BLOCK_DELTA,
                CONTENT_BLOCK_DELTA,
                CONTENT_BLOCK_STOP,
                CONTENT_BLOCK_START,
                CONTENT_BLOCK_DELTA
            ]
        );
        let indices: Vec<usize> = frames.iter().map(index).collect();
        assert_eq!(indices, vec![0, 0, 0, 0, 1, 1]);
        turn.finish();
        let message = turn.message();
        assert_eq!(message.content.len(), 2);
        match &message.content[0] {
            ResponseBlock::Thinking {
                thinking,
                signature,
            } => {
                assert_eq!(thinking, "why");
                assert_eq!(signature, "sig");
            }
            other => panic!("first block must be thinking: {other:?}"),
        }
    }

    /// An executed tool call never reaches the client: the
    /// `tool_use` part is dropped once its `tool_result` proves the
    /// agent ran it, and the response is just the final text.
    #[test]
    fn executed_tool_calls_are_not_surfaced() {
        let (mut turn, frames) = fold(&[
            tool_use_event("toolu_1"),
            tool_result_event("toolu_1"),
            text_event("done"),
        ]);
        assert_eq!(
            names(&frames),
            vec![CONTENT_BLOCK_START, CONTENT_BLOCK_DELTA],
            "a call the agent executed must produce no block"
        );
        turn.finish();
        let message = turn.message();
        assert_eq!(message.content.len(), 1);
        assert_eq!(message.stop_reason.as_deref(), Some("end_turn"));
    }

    /// Text emitted *before* a call closes its block first: every block
    /// gets its own index, in the order the model produced them.
    #[test]
    fn a_call_closes_the_text_block_that_led_up_to_it() {
        let (mut turn, frames) = fold_with_tools(&[
            text_event("working"),
            tool_use_event("toolu_1"),
            tool_result_event("toolu_1"),
            text_event("done"),
        ]);
        assert_eq!(
            names(&frames),
            vec![
                CONTENT_BLOCK_START,
                CONTENT_BLOCK_DELTA,
                CONTENT_BLOCK_STOP,
                CONTENT_BLOCK_START,
                CONTENT_BLOCK_DELTA,
                CONTENT_BLOCK_STOP,
                CONTENT_BLOCK_START,
                CONTENT_BLOCK_STOP,
                CONTENT_BLOCK_START,
                CONTENT_BLOCK_DELTA,
            ]
        );
        assert_eq!(
            frames.iter().map(index).collect::<Vec<_>>(),
            vec![0, 0, 0, 1, 1, 1, 2, 2, 3, 3],
            "four blocks, one index each — a client cannot tell two \
             blocks apart if they share an index"
        );
        // The text deltas land in the blocks that enclose them, so the
        // `("working")` delta precedes the call and `("done")` follows
        // the result.
        turn.finish();
        let message = turn.message();
        let kinds: Vec<&str> = message
            .content
            .iter()
            .map(|block| match block {
                ResponseBlock::Text { .. } => "text",
                ResponseBlock::ToolUse { .. } => "tool_use",
                ResponseBlock::ToolResult { .. } => "tool_result",
                ResponseBlock::Thinking { .. } => "thinking",
            })
            .collect();
        assert_eq!(kinds, vec!["text", "tool_use", "tool_result", "text"]);
    }

    /// An opted-in client sees the call *and* its result as two blocks,
    /// paired by the call id, with the call's input on the
    /// `input_json_delta` and the result's content inline — the two
    /// halves the web UI renders as 请求 / 结果.
    #[test]
    fn opted_in_clients_see_a_call_and_its_result_paired_by_id() {
        let (mut turn, frames) = fold_with_tools(&[
            tool_use_event("toolu_1"),
            tool_result_event("toolu_1"),
            text_event("done"),
        ]);
        assert_eq!(
            names(&frames),
            vec![
                CONTENT_BLOCK_START,
                CONTENT_BLOCK_DELTA,
                CONTENT_BLOCK_STOP,
                CONTENT_BLOCK_START,
                CONTENT_BLOCK_STOP,
                CONTENT_BLOCK_START,
                CONTENT_BLOCK_DELTA
            ]
        );
        assert_eq!(
            frames.iter().map(index).collect::<Vec<_>>(),
            vec![0, 0, 0, 1, 1, 2, 2],
            "the call, its result and the answer are three blocks"
        );

        assert_eq!(
            block_json(&frames[0]),
            serde_json::json!({
                "type": "tool_use",
                "id": "toolu_1",
                "name": "bash",
                "input": {},
            }),
            "the call opens as the protocol's own tool_use block"
        );
        assert_eq!(
            block_json(&frames[3]),
            serde_json::json!({
                "type": "tool_result",
                "tool_use_id": "toolu_1",
                "name": "bash",
                "content": [{ "type": "text", "text": "a.txt" }],
                "is_error": false,
            }),
            "the result names the call it answers, and says it succeeded"
        );

        // The non-streaming body agrees with the frames, value for value.
        turn.finish();
        let message = turn.message();
        assert_eq!(
            serde_json::to_value(&message.content).unwrap(),
            serde_json::json!([
                {
                    "type": "tool_use",
                    "id": "toolu_1",
                    "name": "bash",
                    "input": { "cmd": "ls" },
                },
                {
                    "type": "tool_result",
                    "tool_use_id": "toolu_1",
                    "name": "bash",
                    "content": [{ "type": "text", "text": "a.txt" }],
                    "is_error": false,
                },
                { "type": "text", "text": "done" },
            ])
        );
        assert_eq!(
            message.stop_reason.as_deref(),
            Some("end_turn"),
            "an opted-in client is never asked to answer a call it saw run"
        );
    }

    /// A tool that failed is distinguishable from one that succeeded
    /// without reading its output, and an unnamed result omits `name`
    /// rather than inventing one.
    #[test]
    fn opted_in_results_flag_failure_and_success() {
        let (_, frames) =
            fold_with_tools(&[failed_tool_result_event("toolu_err")]);
        let block = block_json(&frames[0]);
        assert_eq!(block["is_error"], true);
        assert_eq!(block["tool_use_id"], "toolu_err");
        assert_eq!(block["content"][0]["text"], "boom");

        let (_, frames) = fold_with_tools(&[tool_result_event("toolu_ok")]);
        assert_eq!(block_json(&frames[0])["is_error"], false);
        assert_eq!(block_json(&frames[0])["name"], "bash");

        let (_, frames) = fold_with_tools(&[unnamed_tool_result_event("t")]);
        assert!(
            block_json(&frames[0]).get("name").is_none(),
            "an unnamed result must omit the field, not send null: {:?}",
            block_json(&frames[0])
        );
    }

    /// An unexecuted call is surfaced exactly once in either mode, and
    /// an opted-in run reports the run's own stop reason rather than
    /// `tool_use`.
    #[test]
    fn opted_in_runs_keep_the_runs_own_stop_reason() {
        let (mut turn, frames) =
            fold_with_tools(&[tool_use_event("toolu_9"), text_event("hi")]);
        assert_eq!(
            names(&frames),
            vec![
                CONTENT_BLOCK_START,
                CONTENT_BLOCK_DELTA,
                CONTENT_BLOCK_STOP,
                CONTENT_BLOCK_START,
                CONTENT_BLOCK_DELTA
            ],
            "an executed-or-not call is surfaced as it arrives, never twice"
        );
        turn.finish();
        assert_eq!(turn.message().stop_reason.as_deref(), Some("end_turn"));
    }

    /// The same event stream under both modes differs only by the tool
    /// blocks: an external client's frames, content and stop reason are
    /// what they were before the opt-in existed.
    #[test]
    fn the_modes_differ_only_by_the_tool_blocks() {
        let events = [
            tool_use_event("toolu_1"),
            tool_result_event("toolu_1"),
            text_event("answer"),
            usage_event(3, 4),
            done_event(Some("end_turn")),
            ended_event(SessionEndReason::Completed),
        ];
        let (mut plain, plain_frames) = fold(&events);
        let (mut opted_in, opted_frames) = fold_with_tools(&events);

        assert_eq!(
            names(&plain_frames),
            vec![CONTENT_BLOCK_START, CONTENT_BLOCK_DELTA],
            "an external client sees neither half of an executed call"
        );
        assert_eq!(
            names(&opted_frames),
            vec![
                // The call: the protocol's own three frames...
                CONTENT_BLOCK_START,
                CONTENT_BLOCK_DELTA,
                CONTENT_BLOCK_STOP,
                // ...its result: start (whole payload) + stop...
                CONTENT_BLOCK_START,
                CONTENT_BLOCK_STOP,
                // ...and then the answer's text block.
                CONTENT_BLOCK_START,
                CONTENT_BLOCK_DELTA,
            ],
            "the opt-in adds exactly the two blocks, nothing else"
        );
        assert_eq!(
            opted_frames.len(),
            plain_frames.len() + 5,
            "three frames for the call, two for its result"
        );

        plain.finish();
        opted_in.finish();
        let plain = plain.message();
        let opted_in = opted_in.message();
        assert_eq!(
            serde_json::to_value(&plain.content).unwrap(),
            serde_json::json!([{ "type": "text", "text": "answer" }]),
            "the unopted-in content is the run's text alone"
        );
        assert_eq!(plain.stop_reason, opted_in.stop_reason);
        assert_eq!(plain.usage.input_tokens, opted_in.usage.input_tokens);
        assert_eq!(plain.usage.output_tokens, opted_in.usage.output_tokens);
        assert_eq!(plain.id, opted_in.id);
        assert_eq!(plain.model, opted_in.model);
    }

    /// A call the run never executed IS the assistant's answer, so it
    /// is surfaced as a `tool_use` block with the input as one
    /// `input_json_delta`, and the stop reason says `tool_use`.
    #[test]
    fn unexecuted_tool_calls_become_tool_use_blocks() {
        let (mut turn, _) = fold(&[tool_use_event("toolu_9")]);
        let tail = turn.finish();
        assert_eq!(
            names(&tail),
            vec![
                CONTENT_BLOCK_START,
                CONTENT_BLOCK_DELTA,
                CONTENT_BLOCK_STOP,
                MESSAGE_DELTA,
                MESSAGE_STOP
            ]
        );
        let message = turn.message();
        match &message.content[0] {
            ResponseBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "toolu_9");
                assert_eq!(name, "bash");
                assert_eq!(input["cmd"], "ls");
            }
            other => panic!("expected a tool_use block: {other:?}"),
        }
        assert_eq!(message.stop_reason.as_deref(), Some("tool_use"));
    }

    /// Usage accumulates over the run's sampling passes, and the
    /// provider's stop reason is mapped into Anthropic's vocabulary.
    #[test]
    fn usage_and_stop_reason_come_from_the_run() {
        let (mut turn, _) = fold(&[
            usage_event(10, 2),
            done_event(Some("stop")),
            usage_event(4, 3),
            done_event(Some("end_turn")),
        ]);
        turn.finish();
        let message = turn.message();
        assert_eq!(message.usage.input_tokens, 14);
        assert_eq!(message.usage.output_tokens, 5);
        assert_eq!(message.stop_reason.as_deref(), Some("end_turn"));
    }

    /// Without a `SystemEvent::Usage` (strategy-driven runs) the
    /// `SamplingResult`'s own counters are used instead.
    #[test]
    fn sampling_result_usage_is_the_fallback() {
        let (mut turn, _) = fold(&[done_event(None)]);
        turn.finish();
        let message = turn.message();
        assert_eq!(message.usage.input_tokens, 5);
        assert_eq!(message.usage.output_tokens, 7);
    }

    /// A run that ended in an error terminates with an `error` frame —
    /// no `message_delta`, no `message_stop`.
    #[test]
    fn failed_runs_terminate_with_an_error_frame() {
        let (mut turn, _) = fold(&[
            text_event("partial"),
            ended_event(SessionEndReason::Error(
                "provider exploded".to_string(),
            )),
        ]);
        let tail = turn.finish();
        assert_eq!(
            names(&tail),
            vec![CONTENT_BLOCK_STOP, ERROR],
            "an error must replace the success terminal frames"
        );
        let Some(Frame::Error(envelope)) = tail.last() else {
            panic!("the last frame must be the error")
        };
        let json: serde_json::Value =
            serde_json::from_str(&envelope.body()).unwrap();
        assert_eq!(json["type"], "error");
        assert_eq!(json["error"]["type"], "api_error");
        assert_eq!(json["error"]["message"], "provider exploded");
        assert_eq!(
            turn.end_reason(),
            Some(&SessionEndReason::Error("provider exploded".to_string()))
        );
    }

    /// Hitting the iteration cap reports `max_tokens`, the closest
    /// Anthropic stop reason for "the model was cut off".
    #[test]
    fn iteration_cap_reports_max_tokens() {
        let (mut turn, _) = fold(&[
            text_event("x"),
            ended_event(SessionEndReason::MaxIterations),
        ]);
        turn.finish();
        assert_eq!(turn.message().stop_reason.as_deref(), Some("max_tokens"));
    }

    /// `message_start` is a message with no content and no stop reason,
    /// which is what the protocol's first frame carries.
    #[test]
    fn message_start_is_an_empty_assistant_message() {
        let turn = Turn::new("msg_1", "claude-test", false);
        let Frame::MessageStart(message) = turn.start() else {
            panic!("start must be a message_start frame")
        };
        assert_eq!(message.kind, "message");
        assert_eq!(message.role, "assistant");
        assert_eq!(message.id, "msg_1");
        assert_eq!(message.model, "claude-test");
        assert!(message.content.is_empty());
        assert!(message.stop_reason.is_none());
        assert!(message.stop_sequence.is_none());
        assert_eq!(message.usage.input_tokens, 0);
    }

    /// A subagent's wrapped trace is not this message's content.
    #[test]
    fn subagent_events_are_ignored() {
        let inner = text_event("child answer");
        let wrapped = AgentEvent::Agent(
            synthia::harness::AgentMeta::new("parent", "child", 1),
            Box::new(inner),
        );
        let (mut turn, frames) = fold(&[wrapped]);
        assert!(frames.is_empty(), "a child trace must produce no frames");
        turn.finish();
        assert!(turn.message().content.is_empty());
    }
}
